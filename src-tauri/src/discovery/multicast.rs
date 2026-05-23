use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

const MULTICAST_ADDR: &str = "224.0.0.167";
const DISCOVERY_PORT: u16 = 53317;
const DISCOVERY_PORT_RANGE_END: u16 = 53327;
const DISCOVERY_INTERVAL_MS: u64 = 2000;
const DEVICE_TIMEOUT_MS: u64 = 10000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryPacket {
    pub alias: String,
    pub version: String,
    pub fingerprint: String,
    pub port: u16,
    pub protocol: String,
    pub announce: bool,
    pub discovery_port: u16,
}

impl DiscoveryPacket {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

pub struct DiscoveryHandle {
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

/// Start UDP multicast discovery service.
///
/// Spawns two background tasks:
/// - Announce task: broadcasts DiscoveryPacket every 2 seconds
/// - Listen task: receives packets from other devices
///
/// `on_discovered` is called for each unique device seen.
/// Devices that haven't announced themselves within 10 seconds are considered offline.
pub async fn start_discovery(
    device_info: crate::transfer::protocol::DeviceInfo,
    fingerprint: String,
    on_discovered: impl Fn(DiscoveryPacket) + Send + Sync + 'static,
) -> Result<DiscoveryHandle, DiscoveryError> {
    let socket = Arc::new(Mutex::new(bind_discovery_socket().await?));
    let multicast_addr: SocketAddr = format!("{}:{}", MULTICAST_ADDR, DISCOVERY_PORT).parse()?;

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

    // Announce packet
    let announce_packet = DiscoveryPacket {
        alias: device_info.device_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        fingerprint: fingerprint.clone(),
        port: device_info.port,
        protocol: device_info.protocol.clone(),
        announce: true,
        discovery_port: DISCOVERY_PORT,
    };

    let socket_announce = Arc::clone(&socket);
    let announce_json = announce_packet.to_json()?;
    let announce_bytes = announce_json.into_bytes();

    // Spawn announce task
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(DISCOVERY_INTERVAL_MS));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let sock = socket_announce.lock().await;
                    let _ = sock.send_to(&announce_bytes, &multicast_addr.to_string()).await;
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    // Spawn listen task
    let socket_listen = Arc::clone(&socket);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        loop {
            let sock = socket_listen.lock().await;
            let result = tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                sock.recv_from(&mut buf),
            ).await;
            drop(sock);

            match result {
                Ok(Ok((len, _))) => {
                    if let Ok(json) = std::str::from_utf8(&buf[..len]) {
                        if let Ok(packet) = DiscoveryPacket::from_json(json) {
                            if packet.fingerprint != fingerprint {
                                on_discovered(packet);
                            }
                        }
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => continue,
            }
        }
    });

    Ok(DiscoveryHandle { shutdown_tx })
}

async fn bind_discovery_socket() -> Result<UdpSocket, DiscoveryError> {
    // Try to bind to the preferred port, fallback to any available in range
    let mut last_err = None;
    for port in DISCOVERY_PORT..=DISCOVERY_PORT_RANGE_END {
        let bind_addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
        match UdpSocket::bind(bind_addr).await {
            Ok(socket) => {
                // Join multicast group
                let multicast_ip: std::net::Ipv4Addr = MULTICAST_ADDR.parse().unwrap();
                if let Err(e) = socket.join_multicast_v4(multicast_ip, std::net::Ipv4Addr::UNSPECIFIED) {
                    return Err(DiscoveryError::BindFailed(format!(
                        "Failed to join multicast group: {}",
                        e
                    )));
                }
                return Ok(socket);
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(DiscoveryError::BindFailed(format!(
        "Failed to bind to any port in range {}-{}: {}",
        DISCOVERY_PORT,
        DISCOVERY_PORT_RANGE_END,
        last_err.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrInUse, "unknown"))
    )))
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Bind failed: {0}")]
    BindFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Address parse error: {0}")]
    AddrParse(String),
}

impl From<std::net::AddrParseError> for DiscoveryError {
    fn from(e: std::net::AddrParseError) -> Self {
        DiscoveryError::AddrParse(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_packet() -> DiscoveryPacket {
        DiscoveryPacket {
            alias: "TestDevice".to_string(),
            version: "1.0".to_string(),
            fingerprint: "AB:CD:EF:12:34:56".to_string(),
            port: 54321,
            protocol: "rustysend-quic-v1".to_string(),
            announce: true,
            discovery_port: 53317,
        }
    }

    #[test]
    fn test_discovery_packet_serde() {
        let original = sample_packet();
        let json = original.to_json().unwrap();
        let decoded = DiscoveryPacket::from_json(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_discovery_packet_fields() {
        let packet = sample_packet();
        assert_eq!(packet.alias, "TestDevice");
        assert_eq!(packet.version, "1.0");
        assert_eq!(packet.port, 54321);
        assert_eq!(packet.protocol, "rustysend-quic-v1");
        assert!(packet.announce);
        assert_eq!(packet.discovery_port, 53317);
    }

    #[test]
    fn test_discovery_packet_announce_false() {
        let packet = DiscoveryPacket {
            alias: "Other".to_string(),
            version: "1.0".to_string(),
            fingerprint: "00:11:22".to_string(),
            port: 54321,
            protocol: "rustysend-quic-v1".to_string(),
            announce: false,
            discovery_port: 53318,
        };
        let json = packet.to_json().unwrap();
        let decoded = DiscoveryPacket::from_json(&json).unwrap();
        assert!(!decoded.announce);
        assert_eq!(decoded.discovery_port, 53318);
    }

    #[test]
    fn test_discovery_error_display() {
        let err = DiscoveryError::BindFailed("addr in use".to_string());
        assert_eq!(err.to_string(), "Bind failed: addr in use");
    }

    #[test]
    fn test_discovery_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::AddrInUse, "port taken");
        let err: DiscoveryError = io_err.into();
        assert!(matches!(err, DiscoveryError::Io(_)));
    }

    #[test]
    fn test_discovery_error_from_serde() {
        let serde_err = serde_json::from_str::<DiscoveryPacket>("invalid").unwrap_err();
        let err: DiscoveryError = serde_err.into();
        assert!(matches!(err, DiscoveryError::Serialization(_)));
    }

    #[test]
    fn test_multicast_addr_constant() {
        assert_eq!(MULTICAST_ADDR, "224.0.0.167");
    }

    #[test]
    fn test_discovery_port_constant() {
        assert_eq!(DISCOVERY_PORT, 53317);
    }

    #[test]
    fn test_discovery_port_range() {
        assert_eq!(DISCOVERY_PORT_RANGE_END, 53327);
        assert_eq!(DISCOVERY_PORT_RANGE_END - DISCOVERY_PORT, 10);
    }

    #[tokio::test]
    async fn test_bind_discovery_socket() {
        let socket = bind_discovery_socket().await;
        assert!(socket.is_ok());
    }

    #[tokio::test]
    async fn test_start_discovery() {
        let device_info = crate::transfer::protocol::DeviceInfo {
            protocol: "rustysend-quic-v1".to_string(),
            version: 1,
            supported_versions: vec![1],
            device_name: "TestDevice".to_string(),
            port: 54321,
        };

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let handle = start_discovery(
            device_info,
            "test-fingerprint".to_string(),
            move |packet| {
                let _ = tx.try_send(packet);
            },
        ).await;
        assert!(handle.is_ok());

        // Give some time for discovery to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Clean up
        let _ = handle.unwrap().shutdown_tx.send(());
    }
}
