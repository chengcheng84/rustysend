use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::config::settings::Settings;

pub struct AppState {
    pub settings: Arc<tokio::sync::RwLock<Settings>>,
    pub receiver: Arc<tokio::sync::Mutex<Option<ReceiverHandle>>>,
    pub transfers: Arc<dashmap::DashMap<String, TransferSession>>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings: Arc::new(tokio::sync::RwLock::new(settings)),
            receiver: Arc::new(tokio::sync::Mutex::new(None)),
            transfers: Arc::new(dashmap::DashMap::new()),
        }
    }
}

pub struct ReceiverHandle {
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
    pub local_addr: SocketAddr,
}

pub struct TransferSession {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub progress: Arc<AtomicU64>,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub enum TransferStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct TransferSessionDto {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub progress: u64,
    pub status: TransferStatus,
    pub peer_ip: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_transfer_status_serde() {
        let statuses = vec![
            TransferStatus::Pending,
            TransferStatus::InProgress,
            TransferStatus::Completed,
            TransferStatus::Failed,
            TransferStatus::Cancelled,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let decoded: TransferStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, decoded);
        }
    }

    #[test]
    fn test_transfer_status_variants() {
        assert_eq!(
            serde_json::to_string(&TransferStatus::Pending).unwrap(),
            "\"Pending\""
        );
        assert_eq!(
            serde_json::to_string(&TransferStatus::InProgress).unwrap(),
            "\"InProgress\""
        );
        assert_eq!(
            serde_json::to_string(&TransferStatus::Completed).unwrap(),
            "\"Completed\""
        );
        assert_eq!(
            serde_json::to_string(&TransferStatus::Failed).unwrap(),
            "\"Failed\""
        );
        assert_eq!(
            serde_json::to_string(&TransferStatus::Cancelled).unwrap(),
            "\"Cancelled\""
        );
    }

    #[test]
    fn test_transfer_session_progress() {
        let session = TransferSession {
            transfer_id: "t1".to_string(),
            file_name: "test.txt".to_string(),
            file_size: 1000,
            progress: Arc::new(AtomicU64::new(0)),
            status: TransferStatus::Pending,
        };
        assert_eq!(session.progress.load(Ordering::Relaxed), 0);
        session.progress.store(500, Ordering::Relaxed);
        assert_eq!(session.progress.load(Ordering::Relaxed), 500);
    }

    #[test]
    fn test_transfer_session_dto_serde() {
        let dto = TransferSessionDto {
            transfer_id: "t1".to_string(),
            file_name: "test.txt".to_string(),
            file_size: 1024,
            progress: 512,
            status: TransferStatus::InProgress,
            peer_ip: "192.168.1.1".to_string(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        let decoded: TransferSessionDto = serde_json::from_str(&json).unwrap();
        assert_eq!(dto.transfer_id, decoded.transfer_id);
        assert_eq!(dto.file_name, decoded.file_name);
        assert_eq!(dto.file_size, decoded.file_size);
        assert_eq!(dto.progress, decoded.progress);
        assert_eq!(dto.status, decoded.status);
        assert_eq!(dto.peer_ip, decoded.peer_ip);
    }

    #[test]
    fn test_app_state_new() {
        let settings = Settings::default();
        let state = AppState::new(settings.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let read_settings = state.settings.read().await;
            assert_eq!(read_settings.device_name, settings.device_name);
            assert_eq!(read_settings.port, settings.port);
        });
    }

    #[test]
    fn test_receiver_handle_fields() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();
        let handle = ReceiverHandle {
            shutdown_tx: tx,
            local_addr: addr,
        };
        assert_eq!(handle.local_addr.port(), 54321);
    }
}
