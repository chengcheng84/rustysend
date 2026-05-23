use tauri::State;
use std::sync::Arc;

use crate::state::app_state::AppState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Protocol {
    Quic,
    Https,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransferResult {
    pub transfer_id: String,
}

#[tauri::command]
pub async fn send_file(
    _state: State<'_, Arc<AppState>>,
    _file_path: String,
    _target_ip: String,
    _target_port: u16,
    _protocol: Protocol,
) -> Result<TransferResult, String> {
    todo!("Phase 1.5: implement send_file command")
}

#[tauri::command]
pub async fn get_active_transfers(
    _state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::state::app_state::TransferSessionDto>, String> {
    todo!("Phase 1.5: implement get_active_transfers command")
}

#[tauri::command]
pub async fn cancel_transfer(
    _state: State<'_, Arc<AppState>>,
    _transfer_id: String,
) -> Result<(), String> {
    todo!("Phase 1.5: implement cancel_transfer command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_quic_serde() {
        let p = Protocol::Quic;
        let json = serde_json::to_string(&p).unwrap();
        let decoded: Protocol = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, Protocol::Quic));
    }

    #[test]
    fn test_protocol_https_serde() {
        let p = Protocol::Https;
        let json = serde_json::to_string(&p).unwrap();
        let decoded: Protocol = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, Protocol::Https));
    }

    #[test]
    fn test_transfer_result_serde() {
        let result = TransferResult {
            transfer_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: TransferResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.transfer_id, "550e8400-e29b-41d4-a716-446655440000");
    }
}
