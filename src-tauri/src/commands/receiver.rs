use tauri::State;
use std::sync::Arc;

use crate::state::app_state::AppState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReceiverInfo {
    pub port: u16,
}

#[tauri::command]
pub async fn start_receiver(
    _state: State<'_, Arc<AppState>>,
) -> Result<ReceiverInfo, String> {
    todo!("Phase 1.4: implement start_receiver command")
}

#[tauri::command]
pub async fn stop_receiver(
    _state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    todo!("Phase 1.4: implement stop_receiver command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receiver_info_serde() {
        let info = ReceiverInfo { port: 54321 };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: ReceiverInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.port, 54321);
    }
}
