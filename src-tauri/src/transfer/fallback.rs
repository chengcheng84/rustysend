use std::net::SocketAddr;

use crate::transfer::quic::{CertManager, TrustStore, TrustVerification};

/// Protocol type used for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// QUIC over UDP (preferred)
    Quic,
    /// HTTPS over TCP (fallback, Phase 1.3)
    Https,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::Quic => "quic",
            Protocol::Https => "https",
        }
    }
}

/// Result of a protocol fallback connection attempt.
pub struct FallbackConnection {
    pub protocol: Protocol,
    pub quic_conn: Option<quinn::Connection>,
}

/// QUIC connection timeout for fallback probing.
const QUIC_PROBE_TIMEOUT_MS: u64 = 3000;

/// Connect to a peer with automatic protocol fallback.
///
/// 1. First tries QUIC (UDP) with a 3-second timeout
/// 2. If QUIC fails, falls back to HTTPS (TCP) - stub for Phase 1.3
///
/// Returns the connection and the protocol actually used.
pub async fn connect_with_fallback(
    addr: SocketAddr,
    cert: &CertManager,
    trust_store: &mut TrustStore,
) -> Result<FallbackConnection, FallbackError> {
    // Try QUIC first
    match try_quic(addr, cert, trust_store).await {
        Ok(conn) => {
            return Ok(FallbackConnection {
                protocol: Protocol::Quic,
                quic_conn: Some(conn),
            });
        }
        Err(_e) => {
            eprintln!("QUIC connection failed to {}, trying fallback", addr);
        }
    }

    // Fallback to HTTPS (Phase 1.3)
    Err(FallbackError::NoProtocolAvailable)
}

async fn try_quic(
    addr: SocketAddr,
    cert: &CertManager,
    trust_store: &mut TrustStore,
) -> Result<quinn::Connection, FallbackError> {
    let endpoint = crate::transfer::quic::QuicClient::connect(addr, cert)
        .map_err(|e| FallbackError::Quic(e.to_string()))?;

    let conn = endpoint
        .connect(addr, "rustysend")
        .map_err(|e| FallbackError::Quic(e.to_string()))?;

    let conn = tokio::time::timeout(
        tokio::time::Duration::from_millis(QUIC_PROBE_TIMEOUT_MS),
        conn,
    )
    .await
    .map_err(|_| FallbackError::QuicTimeout)?
    .map_err(|e| FallbackError::Quic(e.to_string()))?;

    // TOFU: verify fingerprint
    // Quinn's peer_identity returns opaque data; we skip fingerprint extraction in Phase 1.2
    // and use a placeholder. Phase 2 will implement proper certificate pinning.
    let peer_fp = "unknown".to_string();

    let peer_addr = addr.to_string();
    match trust_store.verify(&peer_addr, &peer_fp) {
        TrustVerification::Trusted => {}
        TrustVerification::FirstSeen => {
            // In Phase 1, auto-trust for simplicity
            // TODO: Phase 2 - prompt user for confirmation
            trust_store.trust(&peer_addr, &peer_fp)?;
        }
        TrustVerification::Mismatch { expected, actual } => {
            return Err(FallbackError::FingerprintMismatch { expected, actual });
        }
    }

    Ok(conn)
}

#[derive(Debug, thiserror::Error)]
pub enum FallbackError {
    #[error("QUIC error: {0}")]
    Quic(String),
    #[error("QUIC connection timeout")]
    QuicTimeout,
    #[error("HTTPS fallback not yet implemented (Phase 1.3)")]
    HttpsNotImplemented,
    #[error("No protocol available — both QUIC and HTTPS failed")]
    NoProtocolAvailable,
    #[error("Certificate fingerprint mismatch: expected {expected}, got {actual}")]
    FingerprintMismatch { expected: String, actual: String },
    #[error("Trust store error: {0}")]
    TrustStore(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<crate::transfer::quic::QuicError> for FallbackError {
    fn from(e: crate::transfer::quic::QuicError) -> Self {
        FallbackError::Quic(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_as_str() {
        assert_eq!(Protocol::Quic.as_str(), "quic");
        assert_eq!(Protocol::Https.as_str(), "https");
    }

    #[test]
    fn test_fallback_error_display() {
        let err = FallbackError::QuicTimeout;
        assert_eq!(err.to_string(), "QUIC connection timeout");

        let err = FallbackError::NoProtocolAvailable;
        assert_eq!(err.to_string(), "No protocol available — both QUIC and HTTPS failed");

        let err = FallbackError::FingerprintMismatch {
            expected: "AA:BB".to_string(),
            actual: "CC:DD".to_string(),
        };
        assert!(err.to_string().contains("AA:BB"));
        assert!(err.to_string().contains("CC:DD"));
    }
}
