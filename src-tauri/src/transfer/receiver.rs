use std::net::SocketAddr;

use crate::transfer::protocol::Message;
use crate::transfer::quic::{CertManager, QuicServer};

#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    #[error("Bind failed: {0}")]
    BindFailed(String),
    #[error("Accept failed: {0}")]
    AcceptFailed(String),
    #[error("Handshake error: {0}")]
    HandshakeError(String),
    #[error("File IO error: {0}")]
    FileIo(#[from] std::io::Error),
    #[error("Invalid token")]
    InvalidToken,
    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
}

pub struct FileReceiver;

impl FileReceiver {
    pub async fn start(
        _addr: SocketAddr,
        _cert: &CertManager,
    ) -> Result<(), ReceiverError> {
        todo!("Phase 1.4: implement receiver start")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receiver_error_display() {
        let err = ReceiverError::BindFailed("addr in use".to_string());
        assert_eq!(err.to_string(), "Bind failed: addr in use");

        let err = ReceiverError::InvalidToken;
        assert_eq!(err.to_string(), "Invalid token");

        let err = ReceiverError::HashMismatch {
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert_eq!(err.to_string(), "Hash mismatch: expected abc, got def");
    }

    #[test]
    fn test_receiver_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no access");
        let err: ReceiverError = io_err.into();
        assert!(matches!(err, ReceiverError::FileIo(_)));
    }
}
