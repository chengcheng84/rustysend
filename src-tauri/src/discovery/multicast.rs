use serde::{Deserialize, Serialize};

const MULTICAST_ADDR: &str = "224.0.0.167";
const DISCOVERY_PORT: u16 = 53317;
const DISCOVERY_PORT_RANGE_END: u16 = 53327;

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

pub async fn start_discovery(
    _device_info: crate::transfer::protocol::DeviceInfo,
    _on_discovered: impl Fn(DiscoveryPacket),
) -> Result<DiscoveryHandle, DiscoveryError> {
    todo!("Phase 1.2: implement UDP multicast discovery")
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Bind failed: {0}")]
    BindFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
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
}
