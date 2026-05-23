use std::net::SocketAddr;
use std::path::Path;

use crate::transfer::protocol::{DeviceInfo, FileMeta, Message, TransferRequest};
use crate::transfer::quic::{CertManager, QuicClient};

#[derive(Debug, thiserror::Error)]
pub enum SenderError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Handshake timeout")]
    HandshakeTimeout,
    #[error("Transfer rejected: {0}")]
    TransferRejected(String),
    #[error("File IO error: {0}")]
    FileIo(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("QUIC error: {0}")]
    Quic(String),
    #[error("Transfer cancelled")]
    Cancelled,
}

pub struct FileSender;

impl FileSender {
    pub async fn send_file(
        _file_path: &Path,
        _target_addr: SocketAddr,
        _cert: &CertManager,
    ) -> Result<String, SenderError> {
        todo!("Phase 1.5: implement send_file")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sender_error_display() {
        let err = SenderError::ConnectionFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: timeout");

        let err = SenderError::HandshakeTimeout;
        assert_eq!(err.to_string(), "Handshake timeout");

        let err = SenderError::TransferRejected("no space".to_string());
        assert_eq!(err.to_string(), "Transfer rejected: no space");

        let err = SenderError::Cancelled;
        assert_eq!(err.to_string(), "Transfer cancelled");
    }

    #[test]
    fn test_sender_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: SenderError = io_err.into();
        assert!(matches!(err, SenderError::FileIo(_)));
    }

    #[test]
    fn test_sender_error_from_serde() {
        let serde_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let err: SenderError = serde_err.into();
        assert!(matches!(err, SenderError::Serialization(_)));
    }
}
