use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub save_path: PathBuf,
    pub device_name: String,
    pub port: u16,
    pub auto_accept: bool,
    pub file_exists_policy: FileExistsPolicy,
    pub max_concurrent_transfers: u32,
    pub buffer_pool_size: u32,
    pub connection_timeout_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileExistsPolicy {
    Overwrite,
    Rename,
    Reject,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_path: default_download_dir(),
            device_name: default_device_name(),
            port: 54321,
            auto_accept: false,
            file_exists_policy: FileExistsPolicy::Rename,
            max_concurrent_transfers: 4,
            buffer_pool_size: 1,
            connection_timeout_secs: 10,
        }
    }
}

fn default_download_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn default_device_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "RustySend Device".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert!(!settings.device_name.is_empty());
        assert_eq!(settings.port, 54321);
        assert!(!settings.auto_accept);
        assert!(matches!(settings.file_exists_policy, FileExistsPolicy::Rename));
        assert_eq!(settings.max_concurrent_transfers, 4);
        assert_eq!(settings.buffer_pool_size, 1);
        assert_eq!(settings.connection_timeout_secs, 10);
    }

    #[test]
    fn test_settings_serde() {
        let original = Settings::default();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_file_exists_policy_variants() {
        let policies = vec![
            FileExistsPolicy::Overwrite,
            FileExistsPolicy::Rename,
            FileExistsPolicy::Reject,
        ];
        for policy in policies {
            let json = serde_json::to_string(&policy).unwrap();
            let decoded: FileExistsPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(policy, decoded);
        }
    }

    #[test]
    fn test_file_exists_policy_serde_names() {
        assert_eq!(
            serde_json::to_string(&FileExistsPolicy::Overwrite).unwrap(),
            "\"Overwrite\""
        );
        assert_eq!(
            serde_json::to_string(&FileExistsPolicy::Rename).unwrap(),
            "\"Rename\""
        );
        assert_eq!(
            serde_json::to_string(&FileExistsPolicy::Reject).unwrap(),
            "\"Reject\""
        );
    }

    #[test]
    fn test_settings_custom_values() {
        let settings = Settings {
            save_path: PathBuf::from("/tmp/downloads"),
            device_name: "MyDevice".to_string(),
            port: 12345,
            auto_accept: true,
            file_exists_policy: FileExistsPolicy::Overwrite,
            max_concurrent_transfers: 8,
            buffer_pool_size: 2,
            connection_timeout_secs: 30,
        };
        let json = serde_json::to_string(&settings).unwrap();
        let decoded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.device_name, "MyDevice");
        assert_eq!(decoded.port, 12345);
        assert!(decoded.auto_accept);
        assert!(matches!(decoded.file_exists_policy, FileExistsPolicy::Overwrite));
        assert_eq!(decoded.max_concurrent_transfers, 8);
        assert_eq!(decoded.buffer_pool_size, 2);
        assert_eq!(decoded.connection_timeout_secs, 30);
    }

    #[test]
    fn test_default_download_dir() {
        let dir = default_download_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_default_device_name() {
        let name = default_device_name();
        assert!(!name.is_empty());
    }
}
