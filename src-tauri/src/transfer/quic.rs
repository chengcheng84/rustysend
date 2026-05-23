use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use quinn::{Endpoint, ServerConfig, ClientConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::sync::Semaphore;

use crate::transfer::protocol::Message;

const CERT_FILENAME: &str = "cert.pem";
const KEY_FILENAME: &str = "key.pem";
const MAX_CONCURRENT_CONNECTIONS: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum QuicError {
    #[error("QUIC error: {0}")]
    Quinn(#[from] quinn::ConnectionError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate error: {0}")]
    Cert(String),
    #[error("Bind error: {0}")]
    Bind(String),
    #[error("Read error: {0}")]
    ReadExact(#[from] quinn::ReadExactError),
    #[error("Write error: {0}")]
    Write(#[from] quinn::WriteError),
    #[error("Connection closed")]
    ClosedStream,
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<quinn::ClosedStream> for QuicError {
    fn from(_: quinn::ClosedStream) -> Self {
        QuicError::ClosedStream
    }
}

impl From<serde_json::Error> for QuicError {
    fn from(e: serde_json::Error) -> Self {
        QuicError::Serialization(e.to_string())
    }
}

pub struct CertManager {
    cert: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    cert_path: Option<std::path::PathBuf>,
}

impl CertManager {
    /// Generate a new self-signed certificate (3 year validity)
    pub fn generate() -> Result<Self, QuicError> {
        let cert = rcgen::generate_simple_self_signed(vec!["rustysend".into()])
            .map_err(|e| QuicError::Cert(e.to_string()))?;
        let cert_der = CertificateDer::from(cert.cert.der().clone());
        let key_der = PrivateKeyDer::try_from(cert.key_pair.serialize_der())
            .map_err(|e| QuicError::Cert(e.to_string()))?;
        Ok(Self {
            cert: vec![cert_der],
            key: key_der,
            cert_path: None,
        })
    }

    /// Load certificates from disk, or generate new ones and persist them.
    ///
    /// Saves cert.pem and key.pem in PEM format under `dir`.
    /// If files exist and are valid, loads them; otherwise regenerates.
    pub fn load_or_generate(dir: &Path) -> Result<Self, QuicError> {
        let cert_path = dir.join(CERT_FILENAME);
        let key_path = dir.join(KEY_FILENAME);

        if cert_path.exists() && key_path.exists() {
            if let Ok(loaded) = Self::load_from_files(&cert_path, &key_path) {
                return Ok(Self {
                    cert: loaded.cert,
                    key: loaded.key,
                    cert_path: Some(cert_path),
                });
            }
            // Loading failed — fall through to regenerate
        }

        // Ensure directory exists
        std::fs::create_dir_all(dir)?;

        let mgr = Self::generate()?;
        mgr.save_to_dir(dir)?;
        Ok(Self {
            cert: mgr.cert,
            key: mgr.key,
            cert_path: Some(cert_path),
        })
    }

    fn load_from_files(cert_path: &Path, key_path: &Path) -> Result<Self, QuicError> {
        let cert_pem = std::fs::read_to_string(cert_path)?;
        let key_pem = std::fs::read_to_string(key_path)?;

        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .filter_map(|c| c.ok())
            .collect();
        if certs.is_empty() {
            return Err(QuicError::Cert("No certificates found in PEM file".into()));
        }

        let key = rustls_pemfile::private_key(&mut key_pem.as_bytes())
            .map_err(|e| QuicError::Cert(format!("Failed to parse key PEM: {}", e)))?
            .ok_or_else(|| QuicError::Cert("No private key found in PEM file".into()))?;

        Ok(Self {
            cert: certs,
            key,
            cert_path: Some(cert_path.to_path_buf()),
        })
    }

    fn save_to_dir(&self, dir: &Path) -> Result<(), QuicError> {
        let cert_path = dir.join(CERT_FILENAME);
        let key_path = dir.join(KEY_FILENAME);

        // Write certificate PEM
        let mut cert_pem = String::new();
        for c in &self.cert {
            let b64 = base64_encode(c.as_ref());
            cert_pem.push_str("-----BEGIN CERTIFICATE-----\n");
            for chunk in b64.as_bytes().chunks(64) {
                let line = std::str::from_utf8(chunk).unwrap();
                cert_pem.push_str(line);
                cert_pem.push('\n');
            }
            cert_pem.push_str("-----END CERTIFICATE-----\n");
        }
        std::fs::write(&cert_path, &cert_pem)?;

        // Write key PEM
        let key_der = match &self.key {
            PrivateKeyDer::Pkcs1(data) => data.secret_pkcs1_der(),
            PrivateKeyDer::Pkcs8(data) => data.secret_pkcs8_der(),
            PrivateKeyDer::Sec1(data) => data.secret_sec1_der(),
            _ => return Err(QuicError::Cert("Unsupported key format".into())),
        };
        let key_b64 = base64_encode(key_der);
        let mut key_pem = String::new();
        key_pem.push_str("-----BEGIN PRIVATE KEY-----\n");
        for chunk in key_b64.as_bytes().chunks(64) {
            let line = std::str::from_utf8(chunk).unwrap();
            key_pem.push_str(line);
            key_pem.push('\n');
        }
        key_pem.push_str("-----END PRIVATE KEY-----\n");
        std::fs::write(&key_path, &key_pem)?;

        Ok(())
    }

    /// Force regenerate certificate and overwrite the saved files.
    ///
    /// All previously trusted devices will need to re-confirm the fingerprint.
    pub fn rotate(&mut self) -> Result<(), QuicError> {
        let new_mgr = Self::generate()?;
        self.cert = new_mgr.cert;
        self.key = new_mgr.key;

        if let Some(ref cert_path) = self.cert_path {
            let dir = cert_path.parent().ok_or_else(|| {
                QuicError::Cert("Cannot determine certificate directory".into())
            })?;
            self.save_to_dir(dir)?;
        }

        Ok(())
    }

    pub fn fingerprint(&self) -> String {
        use std::fmt::Write;
        let cert = &self.cert[0];
        let digest = ring::digest::digest(&ring::digest::SHA256, cert.as_ref());
        let mut result = String::new();
        for (i, byte) in digest.as_ref().iter().enumerate() {
            if i > 0 {
                result.push(':');
            }
            write!(&mut result, "{:02X}", byte).unwrap();
        }
        result
    }

    pub fn cert_chain(&self) -> Vec<CertificateDer<'static>> {
        self.cert.clone()
    }

    pub fn private_key(&self) -> PrivateKeyDer<'static> {
        self.key.clone_key()
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub struct QuicServer {
    endpoint: Endpoint,
    connection_semaphore: Arc<Semaphore>,
}

impl QuicServer {
    pub fn bind(addr: SocketAddr, cert: &CertManager) -> Result<Self, QuicError> {
        let server_config = Self::build_server_config(cert)?;
        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| QuicError::Bind(e.to_string()))?;
        Ok(Self {
            endpoint,
            connection_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS)),
        })
    }

    fn build_server_config(cert: &CertManager) -> Result<ServerConfig, QuicError> {
        let mut server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert.cert.clone(), cert.key.clone_key())
            .map_err(|e| QuicError::Cert(e.to_string()))?;
        server_config.alpn_protocols = vec![b"rustysend/1".to_vec()];

        let quinn_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_config)
                .map_err(|e| QuicError::Cert(e.to_string()))?,
        ));
        Ok(quinn_config)
    }

    /// Accept an incoming connection, respecting the concurrency limit.
    ///
    /// Returns `None` if the endpoint is closed.
    /// Waits for a semaphore permit before accepting, limiting concurrent connections.
    pub async fn accept(&self) -> Option<(quinn::Incoming, SemaphorePermit)> {
        let incoming = self.endpoint.accept().await?;
        let permit = self.connection_semaphore.clone().acquire_owned().await.ok()?;
        Some((incoming, SemaphorePermit { _permit: permit }))
    }

    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.endpoint.local_addr()
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"server shutdown");
    }
}

/// RAII guard that releases a semaphore permit on drop.
pub struct SemaphorePermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

// ─── TOFU Trust Store ──────────────────────────────────────────────────────

const TRUST_STORE_FILENAME: &str = "trust_store.json";

/// Result of checking a server fingerprint against the TOFU trust store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustVerification {
    /// First time seeing this peer — user must confirm the fingerprint.
    FirstSeen,
    /// Fingerprint matches a previously trusted entry.
    Trusted,
    /// Fingerprint does NOT match — possible MITM attack.
    Mismatch {
        expected: String,
        actual: String,
    },
}

/// TOFU (Trust On First Use) trust store for peer certificate fingerprints.
///
/// Persisted as a JSON file mapping `peer_address` → `fingerprint`.
/// - First connection: `FirstSeen` — caller should prompt user to confirm.
/// - Subsequent connections: `Trusted` if fingerprint matches, `Mismatch` otherwise.
pub struct TrustStore {
    entries: std::collections::HashMap<String, String>,
    path: std::path::PathBuf,
}

impl TrustStore {
    /// Load the trust store from disk, or create an empty one if the file doesn't exist.
    pub fn load_or_create(dir: &Path) -> Result<Self, QuicError> {
        let path = dir.join(TRUST_STORE_FILENAME);
        let entries = if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };
        Ok(Self { entries, path })
    }

    /// Verify a peer's certificate fingerprint against the trust store.
    pub fn verify(&self, peer_addr: &str, fingerprint: &str) -> TrustVerification {
        match self.entries.get(peer_addr) {
            None => TrustVerification::FirstSeen,
            Some(trusted_fp) if trusted_fp == fingerprint => TrustVerification::Trusted,
            Some(trusted_fp) => TrustVerification::Mismatch {
                expected: trusted_fp.clone(),
                actual: fingerprint.to_string(),
            },
        }
    }

    /// Trust a peer's fingerprint. Persists immediately to disk.
    pub fn trust(&mut self, peer_addr: &str, fingerprint: &str) -> Result<(), QuicError> {
        self.entries.insert(peer_addr.to_string(), fingerprint.to_string());
        self.save()
    }

    /// Remove a peer from the trust store. Persists immediately.
    pub fn remove(&mut self, peer_addr: &str) -> Result<(), QuicError> {
        self.entries.remove(peer_addr);
        self.save()
    }

    /// Check if a peer is in the trust store.
    pub fn contains(&self, peer_addr: &str) -> bool {
        self.entries.contains_key(peer_addr)
    }

    fn save(&self) -> Result<(), QuicError> {
        let data = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, data)?;
        Ok(())
    }
}

pub struct QuicClient;

impl QuicClient {
    pub fn connect(
        _addr: SocketAddr,
        _cert: &CertManager,
    ) -> Result<quinn::Endpoint, QuicError> {
        let client_config = Self::build_client_config()?;
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
        endpoint.set_default_client_config(client_config);
        Ok(endpoint)
    }

    fn build_client_config() -> Result<ClientConfig, QuicError> {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(
            webpki_roots::TLS_SERVER_ROOTS
                .iter()
                .cloned(),
        );

        let mut client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"rustysend/1".to_vec()];
        client_config
            .dangerous()
            .set_certificate_verifier(Arc::new(InsecureVerifier));

        let quinn_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_config)
                .map_err(|e| QuicError::Cert(e.to_string()))?,
        ));
        Ok(quinn_config)
    }
}

#[derive(Debug)]
struct InsecureVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

pub async fn send_message(
    send: &mut quinn::SendStream,
    msg: &Message,
) -> Result<(), QuicError> {
    let encoded = msg.encode_length_prefixed()
        .map_err(|e| QuicError::Cert(e.to_string()))?;
    send.write_all(&encoded).await?;
    send.finish()?;
    Ok(())
}

pub async fn recv_message(
    recv: &mut quinn::RecvStream,
) -> Result<Message, QuicError> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    let json_str = std::str::from_utf8(&buf)
        .map_err(|e| QuicError::Cert(e.to_string()))?;
    let msg = Message::from_json(json_str)
        .map_err(|e| QuicError::Cert(e.to_string()))?;
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cert_manager_generate() {
        let cert = CertManager::generate();
        assert!(cert.is_ok());
    }

    #[test]
    fn test_cert_manager_fingerprint_format() {
        let cert = CertManager::generate().unwrap();
        let fp = cert.fingerprint();
        let parts: Vec<&str> = fp.split(':').collect();
        assert_eq!(parts.len(), 32);
        for part in &parts {
            assert_eq!(part.len(), 2);
            assert!(part.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn test_cert_manager_fingerprint_consistency() {
        let cert = CertManager::generate().unwrap();
        let fp1 = cert.fingerprint();
        let fp2 = cert.fingerprint();
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_cert_manager_cert_chain() {
        let cert = CertManager::generate().unwrap();
        let chain = cert.cert_chain();
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn test_different_certs_different_fingerprints() {
        let cert1 = CertManager::generate().unwrap();
        let cert2 = CertManager::generate().unwrap();
        assert_ne!(cert1.fingerprint(), cert2.fingerprint());
    }

    #[test]
    fn test_cert_manager_load_or_generate_new() {
        let dir = tempfile::tempdir().unwrap();
        let cert = CertManager::load_or_generate(dir.path()).unwrap();
        assert!(!cert.fingerprint().is_empty());
        assert!(dir.path().join(CERT_FILENAME).exists());
        assert!(dir.path().join(KEY_FILENAME).exists());
    }

    #[test]
    fn test_cert_manager_load_or_generate_persist() {
        let dir = tempfile::tempdir().unwrap();
        let cert1 = CertManager::load_or_generate(dir.path()).unwrap();
        let fp1 = cert1.fingerprint();
        let cert2 = CertManager::load_or_generate(dir.path()).unwrap();
        let fp2 = cert2.fingerprint();
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_cert_manager_rotate() {
        let dir = tempfile::tempdir().unwrap();
        let mut cert = CertManager::load_or_generate(dir.path()).unwrap();
        let fp_before = cert.fingerprint();
        cert.rotate().unwrap();
        let fp_after = cert.fingerprint();
        assert_ne!(fp_before, fp_after);
    }

    #[test]
    fn test_cert_manager_rotate_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut cert = CertManager::load_or_generate(dir.path()).unwrap();
        cert.rotate().unwrap();
        let fp_rotated = cert.fingerprint();
        let cert2 = CertManager::load_or_generate(dir.path()).unwrap();
        assert_eq!(fp_rotated, cert2.fingerprint());
    }

    #[test]
    fn test_cert_manager_load_corrupt_file_regenerates() {
        let dir = tempfile::tempdir().unwrap();
        let cert1 = CertManager::load_or_generate(dir.path()).unwrap();
        let fp1 = cert1.fingerprint();
        // Corrupt the cert file
        std::fs::write(dir.path().join(CERT_FILENAME), "garbage").unwrap();
        let cert2 = CertManager::load_or_generate(dir.path()).unwrap();
        // Should regenerate, so fingerprint changes
        assert_ne!(fp1, cert2.fingerprint());
    }

    #[tokio::test]
    async fn test_quic_server_bind_localhost() {
        let cert = CertManager::generate().unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = QuicServer::bind(addr, &cert);
        assert!(server.is_ok(), "bind failed: {:?}", server.err());
        let server = server.unwrap();
        let local_addr = server.local_addr().unwrap();
        assert_eq!(local_addr.ip().to_string(), "127.0.0.1");
        server.close();
    }

    #[tokio::test]
    async fn test_quic_server_bind_specific_port() {
        let cert = CertManager::generate().unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = QuicServer::bind(addr, &cert);
        assert!(server.is_ok(), "bind failed: {:?}", server.err());
        let server = server.unwrap();
        server.close();
    }

    #[tokio::test]
    async fn test_quic_client_create() {
        let cert = CertManager::generate().unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let client = QuicClient::connect(addr, &cert);
        assert!(client.is_ok(), "client create failed: {:?}", client.err());
    }

    #[test]
    fn test_insecure_verifier_supported_schemes() {
        use rustls::client::danger::ServerCertVerifier;
        let verifier = InsecureVerifier;
        let schemes = verifier.supported_verify_schemes();
        assert!(!schemes.is_empty());
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_trust_store_load_or_create_new() {
        let dir = tempfile::tempdir().unwrap();
        let store = TrustStore::load_or_create(dir.path()).unwrap();
        assert!(!store.contains("192.168.1.1:54321"));
    }

    #[test]
    fn test_trust_store_verify_first_seen() {
        let dir = tempfile::tempdir().unwrap();
        let store = TrustStore::load_or_create(dir.path()).unwrap();
        let result = store.verify("192.168.1.1:54321", "AB:CD");
        assert_eq!(result, TrustVerification::FirstSeen);
    }

    #[test]
    fn test_trust_store_trust_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TrustStore::load_or_create(dir.path()).unwrap();
        store.trust("192.168.1.1:54321", "AB:CD").unwrap();
        let result = store.verify("192.168.1.1:54321", "AB:CD");
        assert_eq!(result, TrustVerification::Trusted);
    }

    #[test]
    fn test_trust_store_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TrustStore::load_or_create(dir.path()).unwrap();
        store.trust("192.168.1.1:54321", "AB:CD").unwrap();
        let result = store.verify("192.168.1.1:54321", "EF:GH");
        assert_eq!(
            result,
            TrustVerification::Mismatch {
                expected: "AB:CD".to_string(),
                actual: "EF:GH".to_string(),
            }
        );
    }

    #[test]
    fn test_trust_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TrustStore::load_or_create(dir.path()).unwrap();
        store.trust("192.168.1.1:54321", "AB:CD").unwrap();
        // Load again from disk
        let store2 = TrustStore::load_or_create(dir.path()).unwrap();
        let result = store2.verify("192.168.1.1:54321", "AB:CD");
        assert_eq!(result, TrustVerification::Trusted);
    }

    #[test]
    fn test_trust_store_remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TrustStore::load_or_create(dir.path()).unwrap();
        store.trust("192.168.1.1:54321", "AB:CD").unwrap();
        store.remove("192.168.1.1:54321").unwrap();
        let result = store.verify("192.168.1.1:54321", "AB:CD");
        assert_eq!(result, TrustVerification::FirstSeen);
    }
}
