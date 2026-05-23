use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    DeviceInfo = 0x03,
    Ack = 0x04,
    TransferRequest = 0x10,
    TransferAccept = 0x11,
    FileMeta = 0x12,
    FileData = 0x13,
    Complete = 0x14,
    Cancel = 0x15,
    Error = 0xFF,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceInfo {
    pub protocol: String,
    pub version: u32,
    pub supported_versions: Vec<u32>,
    pub device_name: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferRequest {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub file_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferAccept {
    pub transfer_id: String,
    pub accepted: bool,
    pub session_token: String,
    #[serde(
        serialize_with = "serialize_data_stream_token",
        deserialize_with = "deserialize_data_stream_token"
    )]
    pub data_stream_token: [u8; 16],
    pub reject_reason: Option<String>,
}

fn serialize_data_stream_token<S: serde::Serializer>(
    token: &[u8; 16],
    s: S,
) -> Result<S::Ok, S::Error> {
    let hex_str = hex_encode(token);
    s.serialize_str(&hex_str)
}

fn deserialize_data_stream_token<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<[u8; 16], D::Error> {
    let hex_str: String = serde::Deserialize::deserialize(d)?;
    hex_decode::<D>(&hex_str)
}

fn hex_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for byte in data {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

fn hex_decode<'de, D: serde::Deserializer<'de>>(hex: &str) -> Result<[u8; 16], D::Error> {
    use serde::de;
    if hex.len() != 32 {
        return Err(de::Error::custom(format!(
            "data_stream_token hex must be 32 chars, got {}",
            hex.len()
        )));
    }
    let mut buf = [0u8; 16];
    for i in 0..16 {
        buf[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|e| de::Error::custom(format!("invalid hex at position {}: {}", i * 2, e)))?;
    }
    Ok(buf)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMeta {
    pub transfer_id: String,
    pub file_size: u64,
    pub file_hash: String,
    pub offset: u64,
    pub modified_at: Option<String>,
    pub accessed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Complete {
    pub transfer_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cancel {
    pub transfer_id: String,
    pub file_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorMessage {
    pub code: String,
    pub message: String,
    pub transfer_id: Option<String>,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "DeviceInfo")]
    DeviceInfo(DeviceInfo),
    #[serde(rename = "Ack")]
    Ack,
    #[serde(rename = "TransferRequest")]
    TransferRequest(TransferRequest),
    #[serde(rename = "TransferAccept")]
    TransferAccept(TransferAccept),
    #[serde(rename = "FileMeta")]
    FileMeta(FileMeta),
    #[serde(rename = "FileData")]
    FileData,
    #[serde(rename = "Complete")]
    Complete(Complete),
    #[serde(rename = "Cancel")]
    Cancel(Cancel),
    #[serde(rename = "Error")]
    Error(ErrorMessage),
}

impl Message {
    pub fn message_type(&self) -> MessageType {
        match self {
            Message::DeviceInfo(_) => MessageType::DeviceInfo,
            Message::Ack => MessageType::Ack,
            Message::TransferRequest(_) => MessageType::TransferRequest,
            Message::TransferAccept(_) => MessageType::TransferAccept,
            Message::FileMeta(_) => MessageType::FileMeta,
            Message::FileData => MessageType::FileData,
            Message::Complete(_) => MessageType::Complete,
            Message::Cancel(_) => MessageType::Cancel,
            Message::Error(_) => MessageType::Error,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn encode_length_prefixed(&self) -> Result<Vec<u8>, serde_json::Error> {
        let json = self.to_json()?;
        let bytes = json.as_bytes();
        let len = bytes.len() as u32;
        let mut result = Vec::with_capacity(4 + bytes.len());
        result.extend_from_slice(&len.to_be_bytes());
        result.extend_from_slice(bytes);
        Ok(result)
    }

    pub fn decode_length_prefixed(data: &[u8]) -> Result<(Self, usize), ProtocolError> {
        if data.len() < 4 {
            return Err(ProtocolError::InsufficientData);
        }
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let total_len = 4 + len;
        if data.len() < total_len {
            return Err(ProtocolError::InsufficientData);
        }
        let json_str = std::str::from_utf8(&data[4..total_len])
            .map_err(|_| ProtocolError::InvalidUtf8)?;
        let msg = Message::from_json(json_str)?;
        Ok((msg, total_len))
    }
}

#[derive(Debug)]
pub enum ProtocolError {
    InsufficientData,
    InvalidUtf8,
    JsonError(serde_json::Error),
    VersionMismatch { client_versions: Vec<u32>, server_version: u32 },
}

/// The current protocol version supported by this implementation.
pub const PROTOCOL_VERSION: u32 = 1;

/// The protocol identifier string for QUIC connections.
pub const PROTOCOL_QUIC_V1: &str = "rustysend-quic-v1";

impl DeviceInfo {
    /// Create a DeviceInfo for the client side of a version negotiation.
    /// `version` is set to 0 (not yet negotiated), `supported_versions` lists all versions we support.
    pub fn new_client(device_name: String, port: u16) -> Self {
        Self {
            protocol: PROTOCOL_QUIC_V1.to_string(),
            version: 0,
            supported_versions: vec![PROTOCOL_VERSION],
            device_name,
            port,
        }
    }

    /// Create a DeviceInfo for the server side after successful version negotiation.
    /// Returns an error if no compatible version is found.
    pub fn negotiate(client_info: &DeviceInfo, device_name: String, port: u16) -> Result<Self, ProtocolError> {
        if client_info.supported_versions.contains(&PROTOCOL_VERSION) {
            Ok(Self {
                protocol: PROTOCOL_QUIC_V1.to_string(),
                version: PROTOCOL_VERSION,
                supported_versions: vec![PROTOCOL_VERSION],
                device_name,
                port,
            })
        } else {
            Err(ProtocolError::VersionMismatch {
                client_versions: client_info.supported_versions.clone(),
                server_version: PROTOCOL_VERSION,
            })
        }
    }

    /// Sanitize a file name to prevent path traversal attacks.
    /// Strips all directory components and rejects `..` patterns.
    pub fn sanitize_file_name(name: &str) -> Option<String> {
        // Reject names containing path traversal
        if name.contains("..") {
            return None;
        }
        // Use Path::file_name() to extract just the file name component
        std::path::Path::new(name)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::InsufficientData => write!(f, "Insufficient data for length-prefixed frame"),
            ProtocolError::InvalidUtf8 => write!(f, "Invalid UTF-8 in message body"),
            ProtocolError::JsonError(e) => write!(f, "JSON error: {}", e),
            ProtocolError::VersionMismatch { client_versions, server_version } => {
                write!(f, "Version mismatch: client supports {:?}, server requires {}", client_versions, server_version)
            }
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProtocolError::JsonError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(e: serde_json::Error) -> Self {
        ProtocolError::JsonError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_device_info() -> DeviceInfo {
        DeviceInfo {
            protocol: "rustysend-quic-v1".to_string(),
            version: 1,
            supported_versions: vec![1],
            device_name: "TestDevice".to_string(),
            port: 54321,
        }
    }

    fn sample_transfer_request() -> TransferRequest {
        TransferRequest {
            transfer_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            file_name: "test.txt".to_string(),
            file_size: 1024,
            file_count: 1,
        }
    }

    fn sample_transfer_accept() -> TransferAccept {
        TransferAccept {
            transfer_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            accepted: true,
            session_token: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
            data_stream_token: [0xde, 0xad, 0xbe, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78],
            reject_reason: None,
        }
    }

    fn sample_file_meta() -> FileMeta {
        FileMeta {
            transfer_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            file_size: 1024,
            file_hash: "af1349b9c6b4f1c7d8e9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2".to_string(),
            offset: 0,
            modified_at: Some("2026-05-23T12:34:56Z".to_string()),
            accessed_at: None,
        }
    }

    fn sample_complete() -> Complete {
        Complete {
            transfer_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            success: true,
        }
    }

    fn sample_cancel() -> Cancel {
        Cancel {
            transfer_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            file_id: None,
            reason: Some("User cancelled".to_string()),
        }
    }

    fn sample_error_message() -> ErrorMessage {
        ErrorMessage {
            code: "HashMismatch".to_string(),
            message: "File hash does not match".to_string(),
            transfer_id: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            details: Some(serde_json::json!({ "expected": "abc", "actual": "def" })),
        }
    }

    #[test]
    fn test_message_type_values() {
        assert_eq!(MessageType::DeviceInfo as u8, 0x03);
        assert_eq!(MessageType::Ack as u8, 0x04);
        assert_eq!(MessageType::TransferRequest as u8, 0x10);
        assert_eq!(MessageType::TransferAccept as u8, 0x11);
        assert_eq!(MessageType::FileMeta as u8, 0x12);
        assert_eq!(MessageType::FileData as u8, 0x13);
        assert_eq!(MessageType::Complete as u8, 0x14);
        assert_eq!(MessageType::Cancel as u8, 0x15);
        assert_eq!(MessageType::Error as u8, 0xFF);
    }

    #[test]
    fn test_device_info_serde() {
        let original = sample_device_info();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: DeviceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_transfer_request_serde() {
        let original = sample_transfer_request();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: TransferRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_transfer_accept_serde() {
        let original = sample_transfer_accept();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: TransferAccept = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_file_meta_serde() {
        let original = sample_file_meta();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: FileMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_complete_serde() {
        let original = sample_complete();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Complete = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_cancel_serde() {
        let original = sample_cancel();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Cancel = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_error_message_serde() {
        let original = sample_error_message();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: ErrorMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_message_enum_device_info() {
        let device = sample_device_info();
        let msg = Message::DeviceInfo(device.clone());
        assert_eq!(msg.message_type(), MessageType::DeviceInfo);

        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::DeviceInfo(d) => assert_eq!(d, device),
            _ => panic!("Expected DeviceInfo variant"),
        }
    }

    #[test]
    fn test_message_enum_ack() {
        let msg = Message::Ack;
        assert_eq!(msg.message_type(), MessageType::Ack);
        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        assert!(matches!(decoded, Message::Ack));
    }

    #[test]
    fn test_message_enum_transfer_request() {
        let req = sample_transfer_request();
        let msg = Message::TransferRequest(req.clone());
        assert_eq!(msg.message_type(), MessageType::TransferRequest);

        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::TransferRequest(r) => assert_eq!(r, req),
            _ => panic!("Expected TransferRequest variant"),
        }
    }

    #[test]
    fn test_message_enum_transfer_accept() {
        let accept = sample_transfer_accept();
        let msg = Message::TransferAccept(accept.clone());
        assert_eq!(msg.message_type(), MessageType::TransferAccept);

        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::TransferAccept(a) => assert_eq!(a, accept),
            _ => panic!("Expected TransferAccept variant"),
        }
    }

    #[test]
    fn test_message_enum_file_meta() {
        let meta = sample_file_meta();
        let msg = Message::FileMeta(meta.clone());
        assert_eq!(msg.message_type(), MessageType::FileMeta);

        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::FileMeta(m) => assert_eq!(m, meta),
            _ => panic!("Expected FileMeta variant"),
        }
    }

    #[test]
    fn test_message_enum_file_data() {
        let msg = Message::FileData;
        assert_eq!(msg.message_type(), MessageType::FileData);
        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        assert!(matches!(decoded, Message::FileData));
    }

    #[test]
    fn test_message_enum_complete() {
        let complete = sample_complete();
        let msg = Message::Complete(complete.clone());
        assert_eq!(msg.message_type(), MessageType::Complete);

        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::Complete(c) => assert_eq!(c, complete),
            _ => panic!("Expected Complete variant"),
        }
    }

    #[test]
    fn test_message_enum_cancel() {
        let cancel = sample_cancel();
        let msg = Message::Cancel(cancel.clone());
        assert_eq!(msg.message_type(), MessageType::Cancel);

        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::Cancel(c) => assert_eq!(c, cancel),
            _ => panic!("Expected Cancel variant"),
        }
    }

    #[test]
    fn test_message_enum_error() {
        let err = sample_error_message();
        let msg = Message::Error(err.clone());
        assert_eq!(msg.message_type(), MessageType::Error);

        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::Error(e) => assert_eq!(e, err),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_length_prefixed_encode_decode() {
        let device = sample_device_info();
        let msg = Message::DeviceInfo(device);

        let encoded = msg.encode_length_prefixed().unwrap();
        assert!(encoded.len() >= 4);

        let (decoded, consumed) = Message::decode_length_prefixed(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert!(matches!(decoded, Message::DeviceInfo(_)));
    }

    #[test]
    fn test_length_prefixed_insufficient_data() {
        let data = vec![0u8; 2];
        let result = Message::decode_length_prefixed(&data);
        assert!(matches!(result.unwrap_err(), ProtocolError::InsufficientData));
    }

    #[test]
    fn test_length_prefixed_partial_body() {
        let mut data = vec![];
        data.extend_from_slice(&100u32.to_be_bytes());
        data.extend_from_slice(b"{\"type\":\"Ack\"}");
        let result = Message::decode_length_prefixed(&data);
        assert!(matches!(result.unwrap_err(), ProtocolError::InsufficientData));
    }

    #[test]
    fn test_length_prefixed_invalid_utf8() {
        let mut data = vec![];
        data.extend_from_slice(&4u32.to_be_bytes());
        data.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);
        let result = Message::decode_length_prefixed(&data);
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidUtf8));
    }

    #[test]
    fn test_length_prefixed_invalid_json() {
        let mut data = vec![];
        data.extend_from_slice(&10u32.to_be_bytes());
        data.extend_from_slice(b"not json!!");
        let result = Message::decode_length_prefixed(&data);
        assert!(matches!(result.unwrap_err(), ProtocolError::JsonError(_)));
    }

    #[test]
    fn test_device_info_version_negotiation_fields() {
        let device = DeviceInfo {
            protocol: "rustysend-quic-v1".to_string(),
            version: 0,
            supported_versions: vec![1, 2],
            device_name: "Client".to_string(),
            port: 54321,
        };
        let json = serde_json::to_string(&device).unwrap();
        let decoded: DeviceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.version, 0);
        assert_eq!(decoded.supported_versions, vec![1, 2]);
    }

    #[test]
    fn test_transfer_accept_rejected() {
        let accept = TransferAccept {
            transfer_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            accepted: false,
            session_token: "".to_string(),
            data_stream_token: [0u8; 16],
            reject_reason: Some("File exists".to_string()),
        };
        let json = serde_json::to_string(&accept).unwrap();
        let decoded: TransferAccept = serde_json::from_str(&json).unwrap();
        assert!(!decoded.accepted);
        assert_eq!(decoded.reject_reason, Some("File exists".to_string()));
    }

    #[test]
    fn test_file_meta_optional_timestamps() {
        let meta = FileMeta {
            transfer_id: "t1".to_string(),
            file_size: 100,
            file_hash: "h1".to_string(),
            offset: 0,
            modified_at: None,
            accessed_at: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: FileMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.modified_at, None);
        assert_eq!(decoded.accessed_at, None);
    }

    #[test]
    fn test_cancel_with_file_id() {
        let cancel = Cancel {
            transfer_id: "t1".to_string(),
            file_id: Some("f1".to_string()),
            reason: None,
        };
        let json = serde_json::to_string(&cancel).unwrap();
        let decoded: Cancel = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.file_id, Some("f1".to_string()));
        assert_eq!(decoded.reason, None);
    }

    #[test]
    fn test_error_message_without_details() {
        let err = ErrorMessage {
            code: "DiskFull".to_string(),
            message: "No space left".to_string(),
            transfer_id: None,
            details: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        let decoded: ErrorMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.transfer_id, None);
        assert_eq!(decoded.details, None);
    }

    #[test]
    fn test_multiple_messages_in_buffer() {
        let ack = Message::Ack;
        let ack_encoded = ack.encode_length_prefixed().unwrap();

        let device = Message::DeviceInfo(sample_device_info());
        let device_encoded = device.encode_length_prefixed().unwrap();

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&ack_encoded);
        buffer.extend_from_slice(&device_encoded);

        let (msg1, consumed1) = Message::decode_length_prefixed(&buffer).unwrap();
        assert!(matches!(msg1, Message::Ack));

        let (msg2, consumed2) = Message::decode_length_prefixed(&buffer[consumed1..]).unwrap();
        assert!(matches!(msg2, Message::DeviceInfo(_)));

        assert_eq!(consumed1 + consumed2, buffer.len());
    }

    #[test]
    fn test_device_info_new_client() {
        let info = DeviceInfo::new_client("MyPC".to_string(), 54321);
        assert_eq!(info.protocol, PROTOCOL_QUIC_V1);
        assert_eq!(info.version, 0);
        assert_eq!(info.supported_versions, vec![PROTOCOL_VERSION]);
        assert_eq!(info.device_name, "MyPC");
        assert_eq!(info.port, 54321);
    }

    #[test]
    fn test_device_info_negotiate_success() {
        let client = DeviceInfo::new_client("Client".to_string(), 54321);
        let server = DeviceInfo::negotiate(&client, "Server".to_string(), 54322).unwrap();
        assert_eq!(server.version, PROTOCOL_VERSION);
        assert_eq!(server.device_name, "Server");
        assert_eq!(server.port, 54322);
    }

    #[test]
    fn test_device_info_negotiate_mismatch() {
        let client = DeviceInfo {
            protocol: PROTOCOL_QUIC_V1.to_string(),
            version: 0,
            supported_versions: vec![99], // unsupported version
            device_name: "Client".to_string(),
            port: 54321,
        };
        let result = DeviceInfo::negotiate(&client, "Server".to_string(), 54322);
        assert!(matches!(result, Err(ProtocolError::VersionMismatch { .. })));
    }

    #[test]
    fn test_sanitize_file_name_simple() {
        assert_eq!(DeviceInfo::sanitize_file_name("test.txt"), Some("test.txt".to_string()));
    }

    #[test]
    fn test_sanitize_file_name_strips_path() {
        assert_eq!(DeviceInfo::sanitize_file_name("/etc/passwd"), Some("passwd".to_string()));
        assert_eq!(DeviceInfo::sanitize_file_name("C:\\Users\\test.txt"), Some("test.txt".to_string()));
        assert_eq!(DeviceInfo::sanitize_file_name("docs/test.txt"), Some("test.txt".to_string()));
    }

    #[test]
    fn test_sanitize_file_name_rejects_traversal() {
        assert_eq!(DeviceInfo::sanitize_file_name("../../etc/cron.d/backdoor"), None);
        assert_eq!(DeviceInfo::sanitize_file_name("..\\..\\windows\\system32"), None);
    }

    #[test]
    fn test_protocol_version_constant() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn test_protocol_quic_v1_constant() {
        assert_eq!(PROTOCOL_QUIC_V1, "rustysend-quic-v1");
    }

    #[test]
    fn test_data_stream_token_hex_serialization() {
        let accept = sample_transfer_accept();
        let json = serde_json::to_string(&accept).unwrap();
        // data_stream_token should be serialized as hex string
        assert!(json.contains("deadbeef1234567890abcdef12345678"));
    }

    #[test]
    fn test_data_stream_token_hex_deserialization() {
        let json = r#"{
            "transfer_id":"550e8400-e29b-41d4-a716-446655440000",
            "accepted":true,
            "session_token":"tok",
            "data_stream_token":"0102030405060708090a0b0c0d0e0f10",
            "reject_reason":null
        }"#;
        let accept: TransferAccept = serde_json::from_str(json).unwrap();
        assert_eq!(accept.data_stream_token, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
    }

    #[test]
    fn test_data_stream_token_invalid_hex() {
        let json = r#"{
            "transfer_id":"t1",
            "accepted":true,
            "session_token":"tok",
            "data_stream_token":"zz",
            "reject_reason":null
        }"#;
        let result = serde_json::from_str::<TransferAccept>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_data_stream_token_wrong_length() {
        let json = r#"{
            "transfer_id":"t1",
            "accepted":true,
            "session_token":"tok",
            "data_stream_token":"01020304",
            "reject_reason":null
        }"#;
        let result = serde_json::from_str::<TransferAccept>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_encode_decode_roundtrip() {
        let token: [u8; 16] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let hex = hex_encode(&token);
        assert_eq!(hex, "00112233445566778899aabbccddeeff");
        let decoded = hex_decode::<serde_json::value::Value>(&hex).unwrap();
        assert_eq!(decoded, token);
    }
}
