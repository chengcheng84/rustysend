# RustySend Phase 1 设计文档

**版本**: v1.0
**日期**: 2026-05-23  
**目标**: QUIC 连接建立 + 单文件传输

---

## 1. 架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                          Frontend (React)                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ DevicesPage │  │ TransferPage│  │      SettingsPage       │  │
│  └──────┬──────┘  └──────┬──────┘  └─────────────────────────┘  │
│         │                │                                       │
│         └────────────────┼───────────────────────────────────────┘
│                          │ invoke / listen
├──────────────────────────┼───────────────────────────────────────┤
│                          ▼                                       │
│                     Tauri Bridge                                 │
│                          │                                       │
├──────────────────────────┼───────────────────────────────────────┤
│                          ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Rust Backend                              │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │ │
│  │  │  Commands   │  │   Events    │  │   State Manager     │  │ │
│  │  │  ─────────  │  │  ─────────  │  │  ─────────────────  │  │ │
│  │  │ start_recv  │  │ transfer-   │  │  ReceiverService    │  │ │
│  │  │ stop_recv   │  │   progress  │  │  TransferSessions   │  │ │
│  │  │ send_file   │  │ transfer-   │  │  ConfigStore        │  │ │
│  │  │             │  │   complete  │  │                     │  │ │
│  │  │             │  │ file-       │  │                     │  │ │
│  │  │             │  │   received  │  │                     │  │ │
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘  │ │
│  │                          │                                   │ │
│  │                          ▼                                   │ │
│  │  ┌─────────────────────────────────────────────────────────┐ │ │
│  │  │              Transfer Module (src/transfer/)             │ │ │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐ │ │ │
│  │  │  │ protocol │  │   quic   │  │  sender  │  │receiver │ │ │ │
│  │  │  │   .rs    │  │   .rs    │  │   .rs    │  │  .rs    │ │ │ │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └─────────┘ │ │ │
│  │  └─────────────────────────────────────────────────────────┘ │ │
│  └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 模块设计

### 2.1 目录结构

```
src-tauri/src/
├── lib.rs                    # Tauri 入口，命令注册
├── main.rs                   # 程序入口
├── commands/                 # Tauri 命令模块
│   ├── mod.rs               # 命令导出
│   ├── receiver.rs          # 接收相关命令
│   └── sender.rs            # 发送相关命令
├── state/                   # 状态管理
│   ├── mod.rs              # 状态导出
│   └── app_state.rs        # 应用状态管理
├── transfer/                # 传输核心模块
│   ├── mod.rs              # 模块导出
│   ├── protocol.rs         # 消息协议定义
│   ├── quic.rs             # QUIC 连接管理
│   ├── https.rs            # HTTPS 回退（Phase 1.3）
│   ├── fallback.rs         # 协议降级探测（Phase 1.3）
│   ├── sender.rs           # 发送逻辑
│   └── receiver.rs         # 接收逻辑
├── discovery/               # 设备发现模块
│   ├── mod.rs              # 模块导出
│   └── multicast.rs        # UDP multicast 发现逻辑
└── config/                  # 配置管理
    ├── mod.rs              # 配置导出
    └── settings.rs         # 设置存储
```

### 2.2 模块职责

| 模块 | 职责 | 关键类型/函数 |
|------|------|---------------|
| `protocol` | 定义消息格式和序列化 | `Message`, `MessageType`, `TransferRequest`, `FileMeta` |
| `quic` | QUIC 连接管理、证书生成 | `QuicServer`, `QuicClient`, `generate_cert` |
| `https` | HTTPS 回退服务端/客户端（Phase 1.3） | `HttpsServer`, `HttpsClient` |
| `fallback` | 协议降级探测（Phase 1.3） | `connect_with_fallback`, `Protocol` |
| `sender` | 文件发送流程 | `FileSender`, `send_file` |
| `receiver` | 文件接收流程 | `FileReceiver`, `start_receiver` |
| `discovery` | UDP multicast 设备发现 | `start_discovery`, `DiscoveryPacket` |
| `commands` | Tauri 命令暴露 | `start_receiver_cmd`, `stop_receiver_cmd`, `send_file_cmd` |
| `state` | 全局状态管理 | `AppState`, `ReceiverHandle` |
| `config` | 配置持久化 | `Settings`, `get_settings`, `save_settings` |

---

## 3. 消息协议详细设计

### 3.1 QUIC Stream 分离设计

**架构决策**：采用 **控制流 + 数据流分离** 方案，充分利用 QUIC 的多路复用特性。

#### 控制流（双向，首个 bidirectional stream）
- 连接建立后，客户端主动打开第一个 bidirectional stream 作为控制通道
- 所有 JSON 控制消息（DeviceInfo/TransferRequest/TransferAccept 等）在此通道传输
- 使用 Length-Prefixed 帧格式：
```
┌─────────────┬─────────────────┐
│   Length    │   JSON Body     │
│  4 bytes BE │   (UTF-8)       │
└─────────────┴─────────────────┘
```

#### 数据流（单向，每个传输独立创建）
- 发送方创建新的 unidirectional stream 传输文件数据
- **数据流结构**：开头 16 字节为 `data_stream_token` 的**原始二进制（raw bytes，即 hex 字符串解码后的 `[u8; 16]`）**，其后为裸文件字节
- 接收方 `accept_uni()` 后先 `read_exact(16 bytes)` 校验 token，校验通过后剩余字节即为裸文件数据
- 利用 QUIC stream 的可靠有序性，天然保证数据完整性
- 取消传输时采用**双管齐下**策略：控制流发送 `Cancel` 消息同步业务状态（触发清理 .tmp 文件和 UI 更新），同时对数据流调用 `stream.stop(0)` 立即切断底层字节流（触发 QUIC `RESET_STREAM`），避免网络带宽浪费

**优势**：
- 省掉自定义分帧协议，减少 bug 面
- 天然支持并行传输（多个数据流互不阻塞）
- 流控由 QUIC 自动处理，应用层无需关心
- 取消传输简单高效
- 16 字节 token 前缀在保持应用层极简的同时，防止未授权流攻击

**注意**：`FileData = 0x13` 消息类型保留但不使用，为后续可能的流复用方案预留。

### 3.2 消息类型定义

```rust
#[repr(u8)]
pub enum MessageType {
    // 设备发现与心跳
    DeviceInfo = 0x03,      // 设备信息交换（含版本协商）
    Ack = 0x04,

    // 传输控制
    TransferRequest = 0x10,
    TransferAccept = 0x11,

    // 文件传输
    FileMeta = 0x12,
    FileData = 0x13,        // 保留：流复用场景使用，Phase 1 裸字节流不启用
    Complete = 0x14,
    Cancel = 0x15,          // 取消传输

    // 错误
    Error = 0xFF,
}

// - 版本协商已收敛到 DeviceInfo
// - 心跳功能由 QUIC 连接层 keep-alive 或应用层 DeviceInfo 轮询替代
// - 简化协议，减少状态机复杂度
```

### 3.3 消息体结构

```rust
// 设备信息交换（包含版本协商 + 协议标识）
#[derive(Serialize, Deserialize)]
pub struct DeviceInfo {
    pub protocol: String,          // "rustysend-quic-v1" | "rustysend-https-v1"（Phase 1.2 预留）
    pub version: u32,              // 协商后的版本号
    pub supported_versions: Vec<u32>, // 支持的版本列表 [1, 2]
    pub device_name: String,
    pub port: u16,
}

// 版本协商流程：
// 1. 连接建立后，Client 打开首个 bidirectional stream
// 2. Client → Server: DeviceInfo { protocol: "rustysend-quic-v1", supported_versions: [1], version: 0, ... }
// 3. Server → Client: DeviceInfo { protocol: "rustysend-quic-v1", version: 1, ... } 或 Error { code: VersionMismatch }
// 4. 版本不兼容时关闭连接
// Phase 1 只实现 v1，但协议框架预留扩展能力
//
// 注：Ping/Pong 已移除，版本协商完全收敛到 DeviceInfo

// 传输请求
#[derive(Serialize, Deserialize)]
pub struct TransferRequest {
    pub transfer_id: String,  // UUID 文本格式 (36 bytes)
    pub file_name: String,
    pub file_size: u64,
    pub file_count: u32,      // 文件数量，默认 1，为文件夹传输预留
}

// ⚠️ 安全说明：file_name 必须 sanitize 后才能使用
// - 拒绝包含路径分隔符（/、\）或 .. 的文件名
// - 使用 Path::file_name() 提取纯文件名，丢弃所有路径前缀
// - 防止路径遍历攻击：../../../etc/cron.d/backdoor

// 传输响应（新增 session_token + data_stream_token 安全机制）
#[derive(Serialize, Deserialize)]
pub struct TransferAccept {
    pub transfer_id: String,
    pub accepted: bool,
    pub session_token: String,          // 控制流认证令牌（UUID 格式）
    pub data_stream_token: String,      // 数据流鉴权令牌（16 字节随机，hex 编码为 32 字符）
    pub reject_reason: Option<String>,
}

// data_stream_token 序列化说明：
// - 内部使用 [u8; 16] 存储，JSON 序列化时转为 hex 字符串（32 字符）
// - 使用 serde 自定义序列化：
//   #[serde(serialize_with = "hex_serialize", deserialize_with = "hex_deserialize")]
//   pub data_stream_token: [u8; 16],
// - 或直接使用 String 存储 hex 编码，前端/调试更友好
// - 生成方式：hex::encode(rand::random::<[u8; 16]>())

// 安全机制说明：
// - session_token：用于控制流消息认证，防止 transfer_id 被猜测后伪造控制消息
// - data_stream_token：独立的 16 字节随机 token，仅用于数据流鉴权
// - 数据流开头携带 16 字节 data_stream_token（由 TransferAccept 分配，hex 解码后使用）
// - 接收方 accept_uni() 后先 read_exact(16 bytes) 校验 token
// - 校验失败立即 stream.stop(0) + 控制流发送 Error::InvalidToken
// - 校验通过后，剩余字节即为裸文件数据
// - 此设计在保持应用层极简的同时，防止未授权流攻击
//
// ⚠️ session_token 校验范围（Phase 1）：
// - 由于 QUIC bidirectional stream 由 TLS 1.3 加密且单连接内控制流唯一，
//   攻击者无法跨连接注入控制消息。Phase 1 中 session_token 作为安全兜底字段保留，
//   但不强制在 FileMeta/Complete/Cancel 中校验。
// - Phase 2 可启用全量校验（每次控制消息携带 session_token 并验证），增强防御纵深。

// 文件元数据（新增时间戳传递）
#[derive(Serialize, Deserialize)]
pub struct FileMeta {
    pub transfer_id: String,
    pub file_size: u64,
    pub file_hash: String,      // blake3 hash，用于端到端校验
    pub offset: u64,            // 断点续传偏移量，Phase 1 固定为 0，预留字段
    pub modified_at: Option<String>,  // RFC 3339 格式，如 "2021-01-01T12:34:56Z"
    pub accessed_at: Option<String>,  // 同上，保持文件原始属性
}

// 传输完成
#[derive(Serialize, Deserialize)]
pub struct Complete {
    pub transfer_id: String,
    pub success: bool,
}

// 取消传输（支持会话级和文件级取消）
#[derive(Serialize, Deserialize)]
pub struct Cancel {
    pub transfer_id: String,
    pub file_id: Option<String>,    // None = 取消整个会话，Some = 取消单个文件（文件夹传输预留）
    pub reason: Option<String>,
}

// 错误消息（通过控制流发送）
#[derive(Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: String,                 // 错误码，如 "HashMismatch", "InvalidToken", "DiskFull"
    pub message: String,              // 人类可读的错误描述
    pub transfer_id: Option<String>,  // 关联的传输 ID（可选）
    pub details: Option<serde_json::Value>,  // 额外调试信息（可选）
}
```

---

## 4. QUIC 连接设计

### 4.1 证书策略

```rust
pub struct CertManager {
    cert: Certificate,
    key: PrivateKey,
}

impl CertManager {
    /// 生成自签名证书（有效期 3 年）
    ///
    /// 有效期考量：
    /// - 原设计 7 天过短，不常开机的用户每次启动都要重新生成证书
    /// - 365 天仍然偏短，用户如果一年没打开应用，TOFU 信任会失效
    /// - 3 年是自签名证书的常见做法，平衡了安全性和用户体验
    pub fn generate() -> Result<Self, CertError> {
        // 使用 rcgen 生成 RSA 密钥对
        // 生成自签名证书（有效期 3 年 = 1095 天）
        // 返回 CertManager
    }
    
    /// 从文件加载或生成新证书
    /// 
    /// ⚠️ 安全备注：Phase 1 私钥明文存储于应用数据目录
    /// - 这是为了简化实现，降低用户门槛
    /// - Phase 2 将迁移至 OS Keychain / Tauri Stronghold 加密存储
    /// - 当前方案在设备被物理访问时存在私钥泄露风险
    pub fn load_or_generate(path: &Path) -> Result<Self, CertError> {
        // 检查缓存文件
        // 如果存在且未过期，加载
        // 否则生成新证书并保存（明文 PEM 格式）
    }
    
    /// 获取证书指纹（用于 TOFU 信任验证）
    pub fn fingerprint(&self) -> String {
        // 返回证书 SHA-256 指纹，格式: "AB:CD:EF:..."
    }

    /// 强制重新生成证书（用户手动触发或安全事件响应）
    ///
    /// 使用场景：
    /// - 用户怀疑证书泄露（设备丢失后找回）
    /// - 安全审计要求强制轮换
    /// - 手动重置 TOFU 信任链
    ///
    /// 注意：轮换后所有已信任设备需要重新确认指纹
    ///
    /// ⚠️ 证书身份持久化建议：
    /// - 证书丢失（重装系统/应用）会导致 TOFU 信任中断，所有已信任设备需重新确认
    /// - 建议用户备份证书文件（{app_data}/cert.pem + key.pem）
    /// - Phase 2 可考虑引入持久化身份密钥对（绑定硬件 ID），证书仅作为短期会话凭证
    pub fn rotate(&mut self) -> Result<(), CertError> {
        // 生成新证书
        // 更新 self.cert 和 self.key
        // 保存到原路径（覆盖旧证书）
    }
}
```

### 4.2 服务端设计

```rust
pub struct QuicServer {
    endpoint: Endpoint,
    incoming: Incoming,
    connection_semaphore: Arc<Semaphore>,  // 连接数限制
}

// 最大并发连接数（防 DoS）
const MAX_CONCURRENT_CONNECTIONS: usize = 16;

impl QuicServer {
    pub async fn bind(addr: SocketAddr, cert: CertManager) -> Result<Self, QuicError> {
        // 配置 QUIC 服务端
        // 绑定地址
        // 初始化 Semaphore
        // 返回 QuicServer
    }
    
    pub async fn accept(&mut self) -> Option<Connection> {
        // 等待 Semaphore permit
        // 接受新连接
        // 连接关闭时释放 permit
    }
}

// 连接处理
pub async fn handle_connection(
    conn: Connection,
    state: Arc<AppState>,
    event_sender: EventSender,
) -> Result<(), TransferError> {
    // 1. 等待 DeviceInfo（包含 supported_versions），进行版本协商
    // 2. 发送 DeviceInfo（包含协商后的 version）
    // 3. 等待 TransferRequest
    // 4. 磁盘空间预检，发送 TransferAccept
    // 5. 接收 FileMeta（包含 blake3 hash）
    // 6. 接受数据流（accept_uni），在独立 unidirectional stream 中接收裸文件字节
    // 7. 流式写入文件并**边写入边计算 blake3**
    // 8. 传输完成后比对 hash，不匹配则返回 HashMismatch 错误
    // 9. 接收 Complete，发送 Ack
    //
    // 注：流式 hash 计算避免传输后全量读盘，满足 50MB/s 性能目标
}
```

### 4.3 客户端设计

```rust
pub struct QuicClient;

impl QuicClient {
    pub async fn connect(
        addr: SocketAddr,
        cert: CertManager,
        trusted_fingerprints: &HashSet<String>, // TOFU 信任列表
    ) -> Result<Connection, QuicError> {
        // 配置 QUIC 客户端
        // 连接到服务端
        // ⚠️ 证书验证策略（安全考虑）：
        // - Phase 1: 采用 TOFU (Trust On First Use)
        // - 首次连接：显示证书指纹，用户确认后存入信任列表
        // - 后续连接：校验指纹是否匹配
        // - 不匹配时：警告用户可能的 MITM 攻击
        //
        // TOFU 信任列表持久化：
        // - 存储路径：{app_data}/trust_store.json
        // - 每次新增信任指纹时同步写入磁盘
        // - 应用启动时从磁盘加载到内存 HashSet
        // - Phase 1 采用本地 JSON 文件，Phase 2 可迁移至 tauri-plugin-store
        //
        // 已知风险：公共 WiFi、企业内网、被入侵的同网设备可能进行 MITM
        // 未来改进：设备配对码、短期 PIN + 证书绑定（类似 AirDrop）
    }
}
```

### 4.4 协议降级设计（QUIC → HTTPS，Phase 1.2-1.3）

**背景**：企业防火墙、访客 WiFi、运营商 CGNAT 对 UDP 的阻断是高频场景。如果仅依赖 QUIC（UDP），产品在 30% 以上的网络环境将完全不可用。

**架构决策**：QUIC 优先，自动降级到 HTTPS。

```rust
pub enum Protocol {
    Quic,    // QUIC over UDP（首选）
    Https,   // HTTPS over TCP（回退，Phase 1.2 实现）
}

pub async fn connect_with_fallback(
    addr: SocketAddr,
    cert: &CertManager,
    trusted_fingerprints: &HashSet<String>,
) -> Result<(Connection, Protocol), TransferError> {
    // 1. 优先尝试 QUIC（UDP），超时 3s
    // 2. 如果 QUIC 连接失败（超时或明确拒绝），自动降级到 HTTPS
    // 3. HTTPS 使用相同端口号，TCP 与 UDP 在传输层天然隔离，OS 自动路由
    // 4. 返回实际使用的协议，供上层记录和日志
}
```

**HTTPS 回退设计要点**：
- **TCP 与 UDP 监听同一端口号**（如 54321）。客户端连接时先尝试 UDP (QUIC)，若超时则回退尝试 TCP (HTTPS)。两者在传输层天然隔离，无需应用层协议区分
- HTTPS 服务端使用 `axum`，提供最小 REST API（与 LocalSend v2 兼容）
- `DeviceInfo` 的 `protocol` 字段标识实际使用的协议：`"rustysend-quic-v1"` | `"rustysend-https-v1"`
- 降级成功后，前端显示"使用兼容模式连接"提示

**目录结构新增**：
```
src/transfer/
├── https.rs         # HTTPS 服务端/客户端（Phase 1.2）
└── fallback.rs      # 协议降级探测逻辑（Phase 1.2）
```

**风险缓解更新**（16 节）：
| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| QUIC 库不稳定 | 高 | 使用成熟的 quinn 库，准备回退到 HTTPS |
| 防火墙阻断 UDP | 高 | QUIC 失败后自动降级到 HTTPS（TCP 443/自定义端口） |

### 4.5 设备发现设计（UDP Multicast）

**背景**：LocalSend 的成功证明零配置发现是核心功能而非锦上添花。手动输入 IP 在手机→电脑、跨网段、DHCP 动态分配等场景几乎不可用。

**架构决策**：与 LocalSend 默认 multicast 组兼容，降低网络配置成本。

```rust
// 与 LocalSend 默认 multicast 组兼容
const MULTICAST_ADDR: &str = "224.0.0.167";
const DISCOVERY_PORT: u16 = 53317;  // 与 LocalSend 一致

#[derive(Serialize, Deserialize)]
pub struct DiscoveryPacket {
    pub alias: String,           // 设备别名
    pub version: String,         // 协议版本 "1.0"
    pub fingerprint: String,     // 证书 SHA-256 指纹，用于设备识别和 TOFU 验证
    pub port: u16,               // 实际传输端口（可能与发现端口不同）
    pub protocol: String,        // "rustysend-quic-v1" 等
    pub announce: bool,          // true = 主动宣告，false = 响应
    pub discovery_port: u16,     // 发现服务使用的 UDP 端口（用于应答路由）
}

pub struct DiscoveryHandle {
    pub shutdown_tx: Sender<()>,
}

pub async fn start_discovery(
    device_info: DeviceInfo,
    on_discovered: impl Fn(DiscoveryPacket),
) -> Result<DiscoveryHandle, DiscoveryError> {
    // 1. 尝试绑定 53317 端口，如果被占用则尝试 53318-53327
    // 2. 加入 multicast 组 224.0.0.167，向绑定的端口发送/接收
    // 3. 定期广播 DiscoveryPacket（announce=true，discovery_port=实际绑定端口）
    // 4. 监听其他设备的广播包
    // 5. 收到 announce=false 的请求包时，回复自身信息（向对方的 discovery_port 发送）
}
```

**端口复用策略**：
> 优先尝试 53317 端口。如果端口被占用（如 LocalSend 正在运行），自动尝试 53318-53327 范围内的端口作为**发现端口**。`DiscoveryPacket` 中新增 `discovery_port` 字段标识实际使用的发现端口，其他设备收到广播包后，向 `discovery_port` 发送应答包，确保应答能正确路由到 RustySend 而非 LocalSend。`port` 字段仍标识实际传输端口（QUIC/HTTPS 端口）。

**发现流程**：
1. 启动接收服务时，同时启动 discovery 广播
2. 每 3 秒发送一次 announce=true 的 DiscoveryPacket
3. 收到其他设备的广播包时，更新设备列表
4. 前端显示在线设备列表，用户点击即可发起传输

**目录结构新增**：
```
src-tauri/src/
├── discovery/
│   ├── mod.rs          # 模块导出
│   └── multicast.rs    # UDP multicast 发现逻辑
```

**Cargo.toml 新增依赖**：
```toml
tokio = { version = "1", features = ["full"] }  # "full" 已包含 "net"，用于 UDP multicast
```

**验收标准更新**（15.3 节）：
- [ ] 两台设备能互相发现（UDP multicast，与 LocalSend 兼容组地址）
- [ ] 手动输入 IP 作为 fallback
- [ ] 设备离线后自动从列表移除（超时 10 秒无广播）

---

## 5. 传输流程设计

### 5.1 发送流程

```
┌─────────────┐
│   开始发送   │
└──────┬──────┘
       │
       ▼
┌─────────────┐     失败     ┌─────────────┐
│ 建立 QUIC   │ ───────────► │  返回错误    │
│   连接      │  重试3次     │             │
└──────┬──────┘              └─────────────┘
       │
       ▼
┌─────────────────────────┐     失败     ┌─────────────┐
│ 发送 DeviceInfo          │ ───────────► │  返回错误    │
│ (含 supported_versions)  │  超时5s      │             │
│ 等待 DeviceInfo          │              │             │
└──────────┬──────────────┘              └─────────────┘
           │
           ▼
┌─────────────┐     拒绝     ┌─────────────┐
│ 发送 Transfer│ ───────────► │  返回拒绝    │
│   Request   │              │   原因      │
│ 等待 Accept │              │             │
└──────┬──────┘              └─────────────┘
       │
       ▼
┌─────────────┐
│ 发送 FileMeta│
└──────┬──────┘
       │
       ▼
┌─────────────────────┐     完成     ┌─────────────┐
│ 创建 unidirectional │ ───────────► │ 发送 Complete│
│ stream              │              │  关闭连接    │
│ 写入 16 字节 token   │              │             │
│ 写入裸文件字节(循环) │              │             │
└─────────────────────┘              └─────────────┘
```

### 5.2 接收流程

```
┌─────────────┐
│  等待连接    │
└──────┬──────┘
       │
       ▼
┌─────────────────────────┐
│ 收到 DeviceInfo          │
│ (含 supported_versions)  │
│ 发送 DeviceInfo          │
│ (含协商后 version)       │
└──────────┬──────────────┘
           │
           ▼
┌─────────────┐     拒绝     ┌─────────────┐
│ 收到 Transfer│ ───────────► │ 发送拒绝     │
│   Request   │              │  等待新连接  │
│  用户确认   │              │             │
└──────┬──────┘              └─────────────┘
       │
       ▼
┌─────────────┐
│ 发送 Accept │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ 收到 FileMeta│
│ 创建文件    │
└──────┬──────┘
       │
       ▼
┌─────────────────────┐     完成     ┌─────────────┐
│ accept_uni()        │ ───────────► │ 收到 Complete│
│ read_exact(16) 校验  │              │ 关闭文件     │
│ token，失败则 stop  │              │ 恢复文件时间戳│
│ 成功则流式写入(循环) │              │ 通知前端     │
└─────────────────────┘              └─────────────┘
```

---

## 6. 状态管理设计

### 6.1 应用状态

```rust
pub struct AppState {
    // 配置：读极多写极少，使用 tokio::sync::RwLock
    pub settings: Arc<tokio::sync::RwLock<Settings>>,
    
    // 接收服务状态：独立锁
    pub receiver: Arc<tokio::sync::Mutex<Option<ReceiverHandle>>>,
    
    // 活跃的传输会话：使用 DashMap 实现无锁并发读写
    pub transfers: Arc<DashMap<String, TransferSession>>,
}

// 锁策略说明：
// - DashMap 替代 RwLock<HashMap>，避免读写锁竞争，支持真正的并发访问
// - 各字段使用独立 Arc，从结构上消除嵌套锁依赖
// - 严禁在持有 settings 锁的情况下去获取 receiver 锁
// - 优先使用 tokio::sync::* 而非 parking_lot（后者在 .await 点会阻塞 OS 线程）

// DashMap 使用约束：
// - 避免在 iter() 中调用 remove()，如需清理，先 collect keys 再批量删除
// - TransferSession 中的 progress 用 Arc<AtomicU64>，DashMap 只持有 Arc 指针
// - 定期清理（如 Completed 超过 1 小时的会话）应在独立 task 中进行

pub struct ReceiverHandle {
    pub shutdown_tx: Sender<()>,
    pub local_addr: SocketAddr,
}

pub struct TransferSession {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub progress: Arc<AtomicU64>,
    pub status: TransferStatus,
}

#[derive(Serialize)]
pub enum TransferStatus {
    Pending,      // 等待确认
    InProgress,   // 传输中
    Completed,    // 完成
    Failed,       // 失败
    Cancelled,    // 取消
}
```

**序列化说明**：`TransferStatus` 使用 serde 默认 PascalCase 序列化。前端 TypeScript 类型应使用 `'Pending' | 'InProgress' | 'Completed' | 'Failed' | 'Cancelled'`。

### 6.2 配置结构

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub save_path: PathBuf,
    pub device_name: String,
    pub port: u16,
    pub auto_accept: bool,
    pub file_exists_policy: FileExistsPolicy,  // 文件已存在时的处理策略
    pub max_concurrent_transfers: u32,         // 最大并发传输数
    pub buffer_pool_size: u32,                 // 预留：Phase 2 动态 buffer 调整时使用，Phase 1 固定 1MB
    pub connection_timeout_secs: u32,          // 连接超时时间 (秒)
}

#[derive(Serialize, Deserialize, Clone)]
pub enum FileExistsPolicy {
    Overwrite,  // 覆盖
    Rename,     // 自动重命名：file.txt → file (1).txt
    Reject,     // 拒绝传输
}

// 注：`auto_accept=false` 已覆盖"询问用户"场景（传输确认弹窗）。
// `FileExistsPolicy` 仅控制文件已存在时的处理策略，不涉及传输确认。
//
// ⚠️ 文件存在性检查时机：
// 收到 TransferRequest 后，**立即检查目标路径文件是否存在**。
// - 若存在且策略为 `Reject`：直接发送 `TransferAccept { accepted: false, reject_reason: "File exists" }`，不触发前端确认弹窗
// - 若存在且策略为 `Rename`：计算新文件名（如 `file (1).txt`），在 TransferAccept 中返回最终文件名，继续流程
// - 若不存在：正常继续流程
// 这样避免"用户同意传输 → 后端发现文件已存在且策略为 Reject → 报错"的糟糕体验

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_path: default_download_dir(),
            device_name: default_device_name(),
            port: 54321,
            auto_accept: false,           // 默认关闭，防止恶意文件推送
            file_exists_policy: FileExistsPolicy::Rename,  // 默认重命名，避免数据丢失
            max_concurrent_transfers: 4,
            buffer_pool_size: 1,          // 预留：Phase 2 动态 buffer 调整时使用，Phase 1 固定 1MB
            connection_timeout_secs: 10,
        }
    }
}

// 安全说明：
// - auto_accept 默认关闭，防止恶意文件推送
// - 未来可扩展为信任设备白名单，仅对信任设备自动接收
```

---

## 7. Tauri 接口设计

### 7.1 命令（Frontend → Backend）

```rust
// 启动接收服务
#[tauri::command]
async fn start_receiver(
    state: State<'_, Arc<AppState>>,
    window: Window,
) -> Result<ReceiverInfo, String>;

// 停止接收服务
#[tauri::command]
async fn stop_receiver(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String>;

// 发送文件（支持设备发现后的直接调用）
// Protocol 枚举定义见 4.4 节，Tauri v2 支持枚举反序列化，前端传 "Quic" 或 "Https" 即可自动映射
#[tauri::command]
async fn send_file(
    state: State<'_, Arc<AppState>>,
    window: Window,
    file_path: String,
    target_ip: String,
    target_port: u16,           // 从发现或手动输入获取
    protocol: Protocol,         // 使用枚举而非 String，类型安全
) -> Result<TransferResult, String>;

// 获取设置
#[tauri::command]
async fn get_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<Settings, String>;

// 保存设置
#[tauri::command]
async fn save_settings(
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<(), String>;

// 获取活跃传输列表（前端 reload 后恢复状态）
// ⚠️ 注意：TransferSession 包含 Arc<AtomicU64>，不支持 serde 序列化
// 必须使用 DTO 转换为可序列化的结构，status 与 TransferStatus 枚举保持一致
#[derive(Serialize)]
pub struct TransferSessionDto {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub progress: u64,      // 从 AtomicU64 load() 转换
    pub status: TransferStatus,  // 与核心枚举一致
    pub peer_ip: String,
}

#[tauri::command]
async fn get_active_transfers(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<TransferSessionDto>, String>;

// 取消传输
#[tauri::command]
async fn cancel_transfer(
    state: State<'_, Arc<AppState>>,
    transfer_id: String,
) -> Result<(), String>;
```

### 7.2 事件（Backend → Frontend）

```typescript
// 传输进度（增量推送，减少后端计算开销）
interface TransferProgressEvent {
  transferId: string;
  bytesDelta: number;    // 自上次事件新增的字节数（前端累加计算 total）
  timestamp: number;     // 后端时间戳（ms），前端计算 speed = bytesDelta / delta_time
}

// 前端速度计算（滑动窗口平均）：
// - 窗口大小：2 秒
// - 采样频率：每 100ms 收到 bytesDelta
// - speed = (最近 20 个 bytesDelta 之和) / 2.0
// - 避免瞬时抖动，显示更平滑的速度
// - 前端维护 Map<transferId, { totalBytes: number, speedHistory: number[] }>
// - 边界情况：若历史采样不足 20 个（传输初期），按实际样本数计算：speed = (sum) / (样本数 × 0.1s)，避免显示 0

// 事件保序说明：
// - 进度事件通过 QUIC bidirectional stream（可靠有序字节流）传输
// - QUIC stream 天然保证消息按发送顺序到达，前端直接累加 bytesDelta 即可
// - 无需处理乱序事件，无需 timestamp 比较逻辑

// 传输完成
interface TransferCompleteEvent {
  transferId: string;
  success: boolean;
  filePath?: string;
  error?: string;
}

// 收到文件
interface FileReceivedEvent {
  filePath: string;
  fileName: string;
  fileSize: number;
  senderName: string;
}

// 传输请求（需要用户确认）
interface TransferRequestEvent {
  transferId: string;
  fileName: string;
  fileSize: number;
  senderName: string;
  senderIp: string;
}

// 事件监听与取消订阅（Tauri v2）
// 注意：listen() 返回的 UnlistenFn 必须在组件卸载时调用，防止内存泄漏
// 
// 示例：
// const unlisten = await listen<TransferProgressEvent>('transfer-progress', (event) => {
//   console.log(event.payload);
// });
// 
// // 组件卸载时
// unlisten();
```

---

## 8. 错误处理策略

### 8.1 错误类型

```rust
#[derive(Error, Debug)]
pub enum TransferError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Handshake timeout")]
    HandshakeTimeout,

    #[error("Transfer rejected: {0}")]
    TransferRejected(String),

    #[error("Insufficient disk space")]
    InsufficientSpace,

    #[error("File hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Version mismatch: supported {supported}, got {received}")]
    VersionMismatch { supported: Vec<u32>, received: u32 },

    #[error("Invalid data stream token")]
    InvalidToken,

    #[error("File IO error: {0}")]
    FileIo(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("QUIC error: {0}")]
    Quic(String),

    #[error("Transfer cancelled")]
    Cancelled,

    #[error("Certificate fingerprint mismatch: expected {expected}")]
    CertFingerprintMismatch { expected: String },

    // 新增：LocalSend 验证过的业务错误码
    #[error("Session conflict: another transfer is in progress")]
    SessionConflict,

    #[error("Too many requests")]
    RateLimited,

    #[error("PIN required or invalid")]
    InvalidPin,

    #[error("Path traversal blocked: {0}")]
    PathTraversalBlocked(String),
}

// 新增：结构化错误响应（预留国际化支持）
#[derive(Serialize, Deserialize)]
pub struct TransferErrorResponse {
    pub code: String,                    // 机器可读码，如 "HashMismatch"
    pub message: String,                 // 英文默认描述
    pub details: Option<serde_json::Value>, // 动态参数，如 { expected: "abc", actual: "def" }
}

// 使用示例：
// TransferError::HashMismatch { expected, actual } => TransferErrorResponse {
//     code: "HashMismatch".to_string(),
//     message: "File hash mismatch".to_string(),
//     details: Some(json!({ "expected": expected, "actual": actual })),
// }
```

### 8.2 重试策略

| 场景 | 重试次数 | 间隔策略 | 行为 |
|------|----------|----------|------|
| 连接失败 | 3 | 1s → 2s → 4s | 指数退避 |
| 握手超时 | 2 | 2s → 4s | 指数退避（HANDSHAKE_TIMEOUT=5s，超时后等待 2s 再重试） |

**注**：QUIC 传输层已有内建重传机制，应用层不应对"发送超时"进行重试，避免双重重传。

---

## 9. 依赖项

### 9.1 Cargo.toml 添加

```toml
[dependencies]
# 现有依赖...
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
tauri-plugin-store = "2"  # 确认兼容 Tauri v2

# QUIC
quinn = "0.11"
rustls = { version = "0.23", default-features = false, features = ["ring"] }

# 证书生成
rcgen = "0.13"
rustls-pemfile = "2"  # rcgen 证书类型与 rustls 期望类型之间的 PEM 转换

# 异步运行时
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"  # tokio 工具库（codec、io 等）

# UUID
uuid = { version = "1", features = ["v4", "serde"] }

# 数据完整性校验
blake3 = "1"

# 错误处理
thiserror = "1"

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 文件系统
dirs-next = "5"
fs2 = "0.4"           # 磁盘空间检查（精确查询路径所在分区的可用空间）

# hex 编码（data_stream_token 序列化）
hex = "0.4"

# 随机数生成（data_stream_token、session_token）
rand = "0.8"

# HTTPS 回退（Phase 1.3）
axum = "0.7"
tower = "0.4"
hyper = { version = "1", features = ["full"] }
hyper-util = "0.1"
tokio-rustls = "0.26"

# 并发原语（替代方案）
dashmap = "6"         # 并发 HashMap，替代 RwLock<HashMap>

# 日志
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"  # 日志文件轮转

# 性能测试
criterion = { version = "0.5", features = ["html_reports"] }

# 系统信息（内存采样等）
sysinfo = "0.30"
```

---

## 10. 日志规范

### 10.1 日志级别使用

| 级别 | 使用场景 | 示例 |
|------|----------|------|
| `ERROR` | 传输失败、IO 错误、连接断开 | `传输失败: {transfer_id}, 错误: {err}` |
| `WARN` | 重试、超时、异常但可恢复 | `连接超时，第 {retry} 次重试` |
| `INFO` | 关键流程节点 | `启动接收服务: {addr}`, `开始传输: {transfer_id}` |
| `DEBUG` | 详细流程信息 | `发送块 {index}/{total}, 大小: {size}` |
| `TRACE` | 最详细的数据流 | `原始消息内容: {bytes:?}` |

### 10.2 结构化日志字段

```rust
// 使用 tracing 的 span 和字段
info!(
    transfer_id = %transfer_id,
    file_name = %file_name,
    file_size = file_size,
    target_ip = %target_ip,
    "开始文件传输"
);

// 错误日志
error!(
    transfer_id = %transfer_id,
    error = %err,
    stage = "handshake",  // 错误发生的阶段
    "传输失败"
);
```

### 10.3 日志输出

- 开发环境：输出到控制台 + 文件
- 生产环境：输出到文件，按天轮转
- 日志路径：`{app_data}/logs/rustysend_{date}.log`

---

## 11. Phase 1 性能目标

### 11.1 性能指标

| 指标 | 目标值 | 测量条件 | 优先级 |
|------|--------|----------|--------|
| 单文件速度 | ≥ 50 MB/s | 千兆有线，1GB 文件 | P0 |
| 握手延迟 | ≤ 500ms | DeviceInfo 往返 | P0 |
| 内存占用 | ≤ 50 MB | 传输 100MB 文件 | P1 |
| CPU 占用 | ≤ 20% | 单核，传输中 | P1 |
| 并发传输 | 4 个 | 同时传输不卡顿 | P1 |

### 11.2 资源限制

```rust
// 并发控制
const MAX_CONCURRENT_TRANSFERS: usize = 4;

// Phase 1：固定 1MB buffer，平衡内存占用与吞吐
// 4 并发 × 1MB = 4MB，加上 QUIC 接收窗口（2MB/流 × 4 = 8MB），总计约 12-15MB
// 远低于 50MB 内存目标，留有充足余量
const BUFFER_SIZE: usize = 1024 * 1024; // 1MB

// 背压与流控说明：
// - 发送端仅根据 QUIC 的流控窗口发送，不额外做应用层限速
// - 依靠系统 UDP 缓冲区正常调度，QUIC 自动处理拥塞控制
// - 4 流全速发送时，总接收窗口 8MB + 应用层 4MB buffer = ~12MB，仍在内存目标内

// Phase 2：根据 benchmark 结果决定是否实现动态 buffer 调整
// 动态调整方案（预留）：
// - 如果连续 3 个周期 speed > 20MB/s，buffer *= 2
// - 如果 speed < 5MB/s，buffer /= 2
// - 上限 2MB，下限 64KB

// QUIC 接收窗口限制（控制内存）
// let mut config = quinn::TransportConfig::default();
// config.receive_window(VarInt::from_u64(2 * 1024 * 1024)); // 2MB
// config.send_window(2 * 1024 * 1024);

// 超时配置
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

// 动态传输超时：基础 5 分钟 + 每 100MB 增加 2 分钟
const BASE_TRANSFER_TIMEOUT: Duration = Duration::from_secs(300);
const TIMEOUT_PER_100MB: Duration = Duration::from_secs(120);

fn calculate_transfer_timeout(file_size: u64) -> Duration {
    BASE_TRANSFER_TIMEOUT + (file_size / (100 * 1024 * 1024)) * TIMEOUT_PER_100MB
}
```

### 11.3 测试方法学

**基准测试工具**（`src-tauri/benches/`）：

```rust
// 使用 criterion 做标准化测试
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_transfer_speed(c: &mut Criterion) {
    c.bench_function("transfer_1gb", |b| {
        b.iter(|| {
            // 测量条件：
            // - 本地回环 (127.0.0.1)
            // - 1GB 随机数据文件
            // - 不计入握手时间（只测数据传输）
            // - 预热：忽略前 100MB
            black_box(run_transfer_test(1024 * 1024 * 1024))
        })
    });
}
```

**测量标准**：
- **吞吐**: `file_size / (complete_time - first_byte_time)`
- **延迟**: DeviceInfo 往返时间（发送 DeviceInfo 到收到对端 DeviceInfo）
- **内存**: `max_rss - baseline_rss`（使用 `sysinfo` 采样）
- **CPU**: `cpu_time / wall_time`（单核百分比）

---

## 12. 清理策略

### 12.1 传输失败处理

| 失败阶段 | 处理方式 | 说明 |
|----------|----------|------|
| 握手前 | 无操作 | 未创建文件 |
| 传输中 | 删除不完整文件 | 收到 Error/Cancel 时删除 |
| 写入后 | 保留 | 校验失败时保留供调试 |

### 12.2 临时文件管理

```rust
// 接收文件时先写入 .tmp 文件
let temp_path = format!("{}.tmp", final_path);

// 传输完成后重命名
if success {
    fs::rename(&temp_path, &final_path)?;
} else {
    fs::remove_file(&temp_path)?;
}
```

### 12.3 定期清理

- 启动时清理超过 24 小时的 .tmp 文件
- 记录未完成的传输到日志

---

## 13. 大文件优化

### 13.1 磁盘预分配

对于 > 1GB 的文件，预先分配磁盘空间：

```rust
if file_size > 1024 * 1024 * 1024 {
    file.set_len(file_size)?;  // 预分配
}
```

### 13.2 顺序写入优化

**⚠️ 注意**：`std::io::BufWriter` 是同步 API，在 async 函数中调用会阻塞 tokio runtime worker 线程。

**Phase 1 采用方案 A：手动 1MB buffer + `write_all`（推荐）**：
```rust
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

const BUFFER_SIZE: usize = 1024 * 1024; // 1MB

let mut buf = vec![0u8; BUFFER_SIZE];
let mut file = File::create(&temp_path).await?;
loop {
    let n = stream.read(&mut buf).await?;
    if n == 0 { break; }
    file.write_all(&buf[..n]).await?; // 异步写入，不阻塞 runtime
}
file.flush().await?;
```

**优势**：
- 避免 `spawn_blocking` 的上下文切换开销
- 可控 buffer 大小，与 11.2 节资源限制一致
- 纯异步 IO，不阻塞 tokio worker 线程

**⚠️ 性能预警**：`tokio::fs::File` 底层依赖 `spawn_blocking` 线程池执行同步 IO。1MB 大块写入可显著减少调度次数（50MB/s 约 50 次/秒）。若 benchmark 发现 CPU 占用过高（>20%），应切换为**方案 B**（在单个 `spawn_blocking` 任务中使用 `std::fs::File` + `std::io::BufWriter`）。

**替代方案 B**（不推荐，仅在有特殊需求时考虑）：
```rust
// 使用 spawn_blocking + std::io::BufWriter
let file = std::fs::File::create(&temp_path)?;
let mut writer = std::io::BufWriter::with_capacity(BUFFER_SIZE, file);
tokio::task::spawn_blocking(move || {
    // 在阻塞线程池中执行同步 IO
}).await??;
```

### 13.3 关于内存映射（mmap）

**Phase 1 不使用 mmap**，原因：
- Windows 上文件被 mmap 后无法删除/重命名
- 网络传输速度 < 磁盘 IO 时，mmap 没有优势
- 内存不足时会触发大量 page fault

**替代方案**：`BufWriter<File>` + 1MB buffer 已经足够高效。mmap 留给 Phase 2 做 benchmark 后再决定。

---

## 14. 实现计划

### Phase 1.1: 基础架构
- [ ] 创建 transfer 模块结构
- [ ] 实现 protocol.rs 消息定义（含 DeviceInfo/TransferRequest/TransferAccept/FileMeta/Complete/Cancel）
- [ ] 添加依赖到 Cargo.toml
- [ ] 创建 discovery 模块结构

### Phase 1.2: 设备发现 + QUIC 连接
- [ ] 实现证书生成（3 年有效期 + rotate 接口）
- [ ] 实现 UDP multicast 设备发现（224.0.0.167:53317，与 LocalSend 兼容）
- [ ] 实现 QuicServer / QuicClient
- [ ] 实现协议降级探测框架（QUIC 优先 → HTTPS 回退，HTTPS 部分 Phase 1.3 实现）
- [ ] 测试本地回环连接（QUIC 路径）

### Phase 1.3: HTTPS 回退（网络兼容性）
- [ ] 实现 HttpsServer（axum，最小 REST API，与 LocalSend v2 兼容）
- [ ] 实现协议降级探测完整逻辑
- [ ] 测试本地回环连接（QUIC + HTTPS 双路径）

### Phase 1.4: 接收功能
- [ ] 实现 receiver.rs 核心逻辑（含 session_token/data_stream_token 校验）
- [ ] 实现接收命令
- [ ] 测试接收流程（含 hash 校验）

### Phase 1.5: 发送功能
- [ ] 实现 sender.rs 核心逻辑
- [ ] 实现发送命令
- [ ] 测试发送流程

### Phase 1.6: 前端集成
- [ ] 添加 Tauri 事件监听（增量进度推送）
- [ ] 更新 TransferPage
- [ ] 实现设备发现 UI
- [ ] 端到端测试

---

## 15. 测试策略

### 15.1 单元测试
- 消息序列化/反序列化
- 证书生成
- 分块逻辑

### 15.2 集成测试
- 本地回环传输 (127.0.0.1)
- 局域网双机测试
- 大文件传输 (>1GB)
- 网络中断恢复

### 15.3 验收标准
- [ ] 两台设备能互相发现（UDP multicast，与 LocalSend 兼容组地址）
- [ ] 手动输入 IP 作为 fallback
- [ ] 小文件 (<1MB) 传输成功
- [ ] 大文件 (>100MB) 传输成功
- [ ] 传输进度实时显示
- [ ] 传输完成前端收到通知
- [ ] 路径遍历攻击被阻断（如 file_name="../../../etc/passwd"）
- [ ] 设备离线后自动从列表移除（超时 10 秒无广播）

**关于设备发现**：
> Phase 1 实现 UDP multicast 发现（与 LocalSend 兼容组地址 224.0.0.167:53317），手动输入 IP 作为 fallback。

---

## 16. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| QUIC 库不稳定 | 高 | 使用成熟的 quinn 库，准备回退到 HTTPS |
| 防火墙阻断 UDP | 高 | QUIC 失败后自动降级到 HTTPS（TCP 443/自定义端口） |
| 大文件内存占用 | 中 | 流式处理，固定 1MB buffer，QUIC 接收窗口限制 2MB |
| 跨平台兼容性 | 中 | CI 测试 Windows/macOS/Linux |

---

## 17. 前端错误边界说明

### 17.1 重置策略

使用 `window.location.reload()` 而非状态重置的原因：

> Tauri 桌面应用的错误边界通常是全局性的，简单重载比尝试修复不可预测的状态更可靠。Rust 后端状态独立于前端，重载不会丢失已建立的传输连接。

### 17.2 错误分类

| 错误类型 | 处理方式 | 用户提示 |
|----------|----------|----------|
| 渲染错误 | 显示错误边界 | "应用出错，点击刷新" |
| 网络错误 | 自动重试 | "连接中..." / "连接失败" |
| 传输错误 | 通知 + 记录 | "传输失败: {原因}" |

---

## 附录: 前端接口定义

```typescript
// src/types/transfer.ts

export interface TransferSessionDto {
  transferId: string;
  fileName: string;
  fileSize: number;
  progress: number;
  status: 'Pending' | 'InProgress' | 'Completed' | 'Failed' | 'Cancelled';
  peerIp: string;
}

export interface TransferProgressEvent {
  transferId: string;
  bytesDelta: number;
  timestamp: number;
}

export interface TransferCompleteEvent {
  transferId: string;
  success: boolean;
  filePath?: string;
  error?: string;
}

export interface FileReceivedEvent {
  filePath: string;
  fileName: string;
  fileSize: number;
  senderName: string;
}

export interface TransferRequestEvent {
  transferId: string;
  fileName: string;
  fileSize: number;
  senderName: string;
  senderIp: string;
}

// src/types/discovery.ts

export interface DiscoveryDevice {
  id: string;          // fingerprint
  name: string;
  ip: string;
  port: number;
  protocol: string;    // "rustysend-quic-v1" | "rustysend-https-v1"（与 DiscoveryPacket.protocol 一致）
  lastSeen: number;    // timestamp
}

// src/api/transfer.ts

export async function startReceiver(): Promise<{ port: number }>;
export async function stopReceiver(): Promise<void>;
export async function sendFile(
  filePath: string, 
  targetIp: string,
  targetPort: number,
  protocol: 'Quic' | 'Https'
): Promise<{ transferId: string }>;
export async function getActiveTransfers(): Promise<TransferSessionDto[]>;
export async function cancelTransfer(transferId: string): Promise<void>;

// 事件监听
export function onTransferProgress(
  callback: (event: TransferProgressEvent) => void
): UnlistenFn;

export function onTransferComplete(
  callback: (event: TransferCompleteEvent) => void
): UnlistenFn;

export function onFileReceived(
  callback: (event: FileReceivedEvent) => void
): UnlistenFn;

export function onTransferRequest(
  callback: (event: TransferRequestEvent) => void
): UnlistenFn;

// src/api/discovery.ts

export function onDeviceDiscovered(
  callback: (device: DiscoveryDevice) => void
): UnlistenFn;

export function onDeviceLost(
  callback: (deviceId: string) => void
): UnlistenFn;
```
│
│  │ DevicesPage │  │ TransferPage│  │      SettingsPage       │  │
│  └──────┬──────┘  └──────┬──────┘  └─────────────────────────┘  │
│         │                │                                       │
│         └────────────────┼───────────────────────────────────────┘
│                          │ invoke / listen
├──────────────────────────┼───────────────────────────────────────┤
│                          ▼                                       │
│                     Tauri Bridge                                 │
│                          │                                       │
├──────────────────────────┼───────────────────────────────────────┤
│                          ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    Rust Backend                              │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │ │
│  │  │  Commands   │  │   Events    │  │   State Manager     │  │ │
│  │  │  ─────────  │  │  ─────────  │  │  ─────────────────  │  │ │
│  │  │ start_recv  │  │ transfer-   │  │  ReceiverService    │  │ │
│  │  │ stop_recv   │  │   progress  │  │  TransferSessions   │  │ │
│  │  │ send_file   │  │ transfer-   │  │  ConfigStore        │  │ │
│  │  │             │  │   complete  │  │                     │  │ │
│  │  │             │  │ file-       │  │                     │  │ │
│  │  │             │  │   received  │  │                     │  │ │
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘  │ │
│  │                          │                                   │ │
│  │                          ▼                                   │ │
│  │  ┌─────────────────────────────────────────────────────────┐ │ │
│  │  │              Transfer Module (src/transfer/)             │ │ │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐ │ │ │
│  │  │  │ protocol │  │   quic   │  │  sender  │  │receiver │ │ │ │
│  │  │  │   .rs    │  │   .rs    │  │   .rs    │  │  .rs    │ │ │ │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └─────────┘ │ │ │
│  │  └─────────────────────────────────────────────────────────┘ │ │
│  └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 模块设计

### 2.1 目录结构

```
src-tauri/src/
├── lib.rs                    # Tauri 入口，命令注册
├── main.rs                   # 程序入口
├── commands/                 # Tauri 命令模块
│   ├── mod.rs               # 命令导出
│   ├── receiver.rs          # 接收相关命令
│   └── sender.rs            # 发送相关命令
├── state/                   # 状态管理
│   ├── mod.rs              # 状态导出
│   └── app_state.rs        # 应用状态管理
├── transfer/                # 传输核心模块
│   ├── mod.rs              # 模块导出
│   ├── protocol.rs         # 消息协议定义
│   ├── quic.rs             # QUIC 连接管理
│   ├── https.rs            # HTTPS 回退（Phase 1.3）
│   ├── fallback.rs         # 协议降级探测（Phase 1.3）
│   ├── sender.rs           # 发送逻辑
│   └── receiver.rs         # 接收逻辑
├── discovery/               # 设备发现模块
│   ├── mod.rs              # 模块导出
│   └── multicast.rs        # UDP multicast 发现逻辑
└── config/                  # 配置管理
    ├── mod.rs              # 配置导出
    └── settings.rs         # 设置存储
```

### 2.2 模块职责

| 模块 | 职责 | 关键类型/函数 |
|------|------|---------------|
| `protocol` | 定义消息格式和序列化 | `Message`, `MessageType`, `TransferRequest`, `FileMeta` |
| `quic` | QUIC 连接管理、证书生成 | `QuicServer`, `QuicClient`, `generate_cert` |
| `https` | HTTPS 回退服务端/客户端（Phase 1.3） | `HttpsServer`, `HttpsClient` |
| `fallback` | 协议降级探测（Phase 1.3） | `connect_with_fallback`, `Protocol` |
| `sender` | 文件发送流程 | `FileSender`, `send_file` |
| `receiver` | 文件接收流程 | `FileReceiver`, `start_receiver` |
| `discovery` | UDP multicast 设备发现 | `start_discovery`, `DiscoveryPacket` |
| `commands` | Tauri 命令暴露 | `start_receiver_cmd`, `stop_receiver_cmd`, `send_file_cmd` |
| `state` | 全局状态管理 | `AppState`, `ReceiverHandle` |
| `config` | 配置持久化 | `Settings`, `get_settings`, `save_settings` |

---

## 3. 消息协议详细设计

### 3.1 QUIC Stream 分离设计

**架构决策**：采用 **控制流 + 数据流分离** 方案，充分利用 QUIC 的多路复用特性。

#### 控制流（双向，首个 bidirectional stream）
- 连接建立后，客户端主动打开第一个 bidirectional stream 作为控制通道
- 所有 JSON 控制消息（DeviceInfo/TransferRequest/TransferAccept 等）在此通道传输
- 使用 Length-Prefixed 帧格式：
```
┌─────────────┬─────────────────┐
│   Length    │   JSON Body     │
│  4 bytes BE │   (UTF-8)       │
└─────────────┴─────────────────┘
```

#### 数据流（单向，每个传输独立创建）
- 发送方创建新的 unidirectional stream 传输文件数据
- **数据流结构**：开头 16 字节为 `data_stream_token` 的**原始二进制（raw bytes，即 hex 字符串解码后的 `[u8; 16]`）**，其后为裸文件字节
- 接收方 `accept_uni()` 后先 `read_exact(16 bytes)` 校验 token，校验通过后剩余字节即为裸文件数据
- 利用 QUIC stream 的可靠有序性，天然保证数据完整性
- 取消传输时采用**双管齐下**策略：控制流发送 `Cancel` 消息同步业务状态（触发清理 .tmp 文件和 UI 更新），同时对数据流调用 `stream.stop(0)` 立即切断底层字节流（触发 QUIC `RESET_STREAM`），避免网络带宽浪费

**优势**：
- 省掉自定义分帧协议，减少 bug 面
- 天然支持并行传输（多个数据流互不阻塞）
- 流控由 QUIC 自动处理，应用层无需关心
- 取消传输简单高效
- 16 字节 token 前缀在保持应用层极简的同时，防止未授权流攻击

**注意**：`FileData = 0x13` 消息类型保留但不使用，为后续可能的流复用方案预留。

### 3.2 消息类型定义

```rust
#[repr(u8)]
pub enum MessageType {
    // 设备发现与心跳
    DeviceInfo = 0x03,      // 设备信息交换（含版本协商）
    Ack = 0x04,

    // 传输控制
    TransferRequest = 0x10,
    TransferAccept = 0x11,

    // 文件传输
    FileMeta = 0x12,
    FileData = 0x13,        // 保留：流复用场景使用，Phase 1 裸字节流不启用
    Complete = 0x14,
    Cancel = 0x15,          // 取消传输

    // 错误
    Error = 0xFF,
}

// - 版本协商已收敛到 DeviceInfo
// - 心跳功能由 QUIC 连接层 keep-alive 或应用层 DeviceInfo 轮询替代
// - 简化协议，减少状态机复杂度
```

### 3.3 消息体结构

```rust
// 设备信息交换（包含版本协商 + 协议标识）
#[derive(Serialize, Deserialize)]
pub struct DeviceInfo {
    pub protocol: String,          // "rustysend-quic-v1" | "rustysend-https-v1"（Phase 1.2 预留）
    pub version: u32,              // 协商后的版本号
    pub supported_versions: Vec<u32>, // 支持的版本列表 [1, 2]
    pub device_name: String,
    pub port: u16,
}

// 版本协商流程：
// 1. 连接建立后，Client 打开首个 bidirectional stream
// 2. Client → Server: DeviceInfo { protocol: "rustysend-quic-v1", supported_versions: [1], version: 0, ... }
// 3. Server → Client: DeviceInfo { protocol: "rustysend-quic-v1", version: 1, ... } 或 Error { code: VersionMismatch }
// 4. 版本不兼容时关闭连接
// Phase 1 只实现 v1，但协议框架预留扩展能力
//
// 注：Ping/Pong 已移除，版本协商完全收敛到 DeviceInfo

// 传输请求
#[derive(Serialize, Deserialize)]
pub struct TransferRequest {
    pub transfer_id: String,  // UUID 文本格式 (36 bytes)
    pub file_name: String,
    pub file_size: u64,
    pub file_count: u32,      // 文件数量，默认 1，为文件夹传输预留
}

// ⚠️ 安全说明：file_name 必须 sanitize 后才能使用
// - 拒绝包含路径分隔符（/、\）或 .. 的文件名
// - 使用 Path::file_name() 提取纯文件名，丢弃所有路径前缀
// - 防止路径遍历攻击：../../../etc/cron.d/backdoor

// 传输响应（新增 session_token + data_stream_token 安全机制）
#[derive(Serialize, Deserialize)]
pub struct TransferAccept {
    pub transfer_id: String,
    pub accepted: bool,
    pub session_token: String,          // 控制流认证令牌（UUID 格式）
    pub data_stream_token: String,      // 数据流鉴权令牌（16 字节随机，hex 编码为 32 字符）
    pub reject_reason: Option<String>,
}

// data_stream_token 序列化说明：
// - 内部使用 [u8; 16] 存储，JSON 序列化时转为 hex 字符串（32 字符）
// - 使用 serde 自定义序列化：
//   #[serde(serialize_with = "hex_serialize", deserialize_with = "hex_deserialize")]
//   pub data_stream_token: [u8; 16],
// - 或直接使用 String 存储 hex 编码，前端/调试更友好
// - 生成方式：hex::encode(rand::random::<[u8; 16]>())

// 安全机制说明：
// - session_token：用于控制流消息认证，防止 transfer_id 被猜测后伪造控制消息
// - data_stream_token：独立的 16 字节随机 token，仅用于数据流鉴权
// - 数据流开头携带 16 字节 data_stream_token（由 TransferAccept 分配，hex 解码后使用）
// - 接收方 accept_uni() 后先 read_exact(16 bytes) 校验 token
// - 校验失败立即 stream.stop(0) + 控制流发送 Error::InvalidToken
// - 校验通过后，剩余字节即为裸文件数据
// - 此设计在保持应用层极简的同时，防止未授权流攻击
//
// ⚠️ session_token 校验范围（Phase 1）：
// - 由于 QUIC bidirectional stream 由 TLS 1.3 加密且单连接内控制流唯一，
//   攻击者无法跨连接注入控制消息。Phase 1 中 session_token 作为安全兜底字段保留，
//   但不强制在 FileMeta/Complete/Cancel 中校验。
// - Phase 2 可启用全量校验（每次控制消息携带 session_token 并验证），增强防御纵深。

// 文件元数据（新增时间戳传递）
#[derive(Serialize, Deserialize)]
pub struct FileMeta {
    pub transfer_id: String,
    pub file_size: u64,
    pub file_hash: String,      // blake3 hash，用于端到端校验
    pub offset: u64,            // 断点续传偏移量，Phase 1 固定为 0，预留字段
    pub modified_at: Option<String>,  // RFC 3339 格式，如 "2021-01-01T12:34:56Z"
    pub accessed_at: Option<String>,  // 同上，保持文件原始属性
}

// 传输完成
#[derive(Serialize, Deserialize)]
pub struct Complete {
    pub transfer_id: String,
    pub success: bool,
}

// 取消传输（支持会话级和文件级取消）
#[derive(Serialize, Deserialize)]
pub struct Cancel {
    pub transfer_id: String,
    pub file_id: Option<String>,    // None = 取消整个会话，Some = 取消单个文件（文件夹传输预留）
    pub reason: Option<String>,
}

// 错误消息（通过控制流发送）
#[derive(Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: String,                 // 错误码，如 "HashMismatch", "InvalidToken", "DiskFull"
    pub message: String,              // 人类可读的错误描述
    pub transfer_id: Option<String>,  // 关联的传输 ID（可选）
    pub details: Option<serde_json::Value>,  // 额外调试信息（可选）
}
```

---

## 4. QUIC 连接设计

### 4.1 证书策略

```rust
pub struct CertManager {
    cert: Certificate,
    key: PrivateKey,
}

impl CertManager {
    /// 生成自签名证书（有效期 3 年）
    ///
    /// 有效期考量：
    /// - 原设计 7 天过短，不常开机的用户每次启动都要重新生成证书
    /// - 365 天仍然偏短，用户如果一年没打开应用，TOFU 信任会失效
    /// - 3 年是自签名证书的常见做法，平衡了安全性和用户体验
    pub fn generate() -> Result<Self, CertError> {
        // 使用 rcgen 生成 RSA 密钥对
        // 生成自签名证书（有效期 3 年 = 1095 天）
        // 返回 CertManager
    }
    
    /// 从文件加载或生成新证书
    /// 
    /// ⚠️ 安全备注：Phase 1 私钥明文存储于应用数据目录
    /// - 这是为了简化实现，降低用户门槛
    /// - Phase 2 将迁移至 OS Keychain / Tauri Stronghold 加密存储
    /// - 当前方案在设备被物理访问时存在私钥泄露风险
    pub fn load_or_generate(path: &Path) -> Result<Self, CertError> {
        // 检查缓存文件
        // 如果存在且未过期，加载
        // 否则生成新证书并保存（明文 PEM 格式）
    }
    
    /// 获取证书指纹（用于 TOFU 信任验证）
    pub fn fingerprint(&self) -> String {
        // 返回证书 SHA-256 指纹，格式: "AB:CD:EF:..."
    }

    /// 强制重新生成证书（用户手动触发或安全事件响应）
    ///
    /// 使用场景：
    /// - 用户怀疑证书泄露（设备丢失后找回）
    /// - 安全审计要求强制轮换
    /// - 手动重置 TOFU 信任链
    ///
    /// 注意：轮换后所有已信任设备需要重新确认指纹
    ///
    /// ⚠️ 证书身份持久化建议：
    /// - 证书丢失（重装系统/应用）会导致 TOFU 信任中断，所有已信任设备需重新确认
    /// - 建议用户备份证书文件（{app_data}/cert.pem + key.pem）
    /// - Phase 2 可考虑引入持久化身份密钥对（绑定硬件 ID），证书仅作为短期会话凭证
    pub fn rotate(&mut self) -> Result<(), CertError> {
        // 生成新证书
        // 更新 self.cert 和 self.key
        // 保存到原路径（覆盖旧证书）
    }
}
```

### 4.2 服务端设计

```rust
pub struct QuicServer {
    endpoint: Endpoint,
    incoming: Incoming,
    connection_semaphore: Arc<Semaphore>,  // 连接数限制
}

// 最大并发连接数（防 DoS）
const MAX_CONCURRENT_CONNECTIONS: usize = 16;

impl QuicServer {
    pub async fn bind(addr: SocketAddr, cert: CertManager) -> Result<Self, QuicError> {
        // 配置 QUIC 服务端
        // 绑定地址
        // 初始化 Semaphore
        // 返回 QuicServer
    }
    
    pub async fn accept(&mut self) -> Option<Connection> {
        // 等待 Semaphore permit
        // 接受新连接
        // 连接关闭时释放 permit
    }
}

// 连接处理
pub async fn handle_connection(
    conn: Connection,
    state: Arc<AppState>,
    event_sender: EventSender,
) -> Result<(), TransferError> {
    // 1. 等待 DeviceInfo（包含 supported_versions），进行版本协商
    // 2. 发送 DeviceInfo（包含协商后的 version）
    // 3. 等待 TransferRequest
    // 4. 磁盘空间预检，发送 TransferAccept
    // 5. 接收 FileMeta（包含 blake3 hash）
    // 6. 接受数据流（accept_uni），在独立 unidirectional stream 中接收裸文件字节
    // 7. 流式写入文件并**边写入边计算 blake3**
    // 8. 传输完成后比对 hash，不匹配则返回 HashMismatch 错误
    // 9. 接收 Complete，发送 Ack
    //
    // 注：流式 hash 计算避免传输后全量读盘，满足 50MB/s 性能目标
}
```

### 4.3 客户端设计

```rust
pub struct QuicClient;

impl QuicClient {
    pub async fn connect(
        addr: SocketAddr,
        cert: CertManager,
        trusted_fingerprints: &HashSet<String>, // TOFU 信任列表
    ) -> Result<Connection, QuicError> {
        // 配置 QUIC 客户端
        // 连接到服务端
        // ⚠️ 证书验证策略（安全考虑）：
        // - Phase 1: 采用 TOFU (Trust On First Use)
        // - 首次连接：显示证书指纹，用户确认后存入信任列表
        // - 后续连接：校验指纹是否匹配
        // - 不匹配时：警告用户可能的 MITM 攻击
        //
        // TOFU 信任列表持久化：
        // - 存储路径：{app_data}/trust_store.json
        // - 每次新增信任指纹时同步写入磁盘
        // - 应用启动时从磁盘加载到内存 HashSet
        // - Phase 1 采用本地 JSON 文件，Phase 2 可迁移至 tauri-plugin-store
        //
        // 已知风险：公共 WiFi、企业内网、被入侵的同网设备可能进行 MITM
        // 未来改进：设备配对码、短期 PIN + 证书绑定（类似 AirDrop）
    }
}
```

### 4.4 协议降级设计（QUIC → HTTPS，Phase 1.2-1.3）

**背景**：企业防火墙、访客 WiFi、运营商 CGNAT 对 UDP 的阻断是高频场景。如果仅依赖 QUIC（UDP），产品在 30% 以上的网络环境将完全不可用。

**架构决策**：QUIC 优先，自动降级到 HTTPS。

```rust
pub enum Protocol {
    Quic,    // QUIC over UDP（首选）
    Https,   // HTTPS over TCP（回退，Phase 1.2 实现）
}

pub async fn connect_with_fallback(
    addr: SocketAddr,
    cert: &CertManager,
    trusted_fingerprints: &HashSet<String>,
) -> Result<(Connection, Protocol), TransferError> {
    // 1. 优先尝试 QUIC（UDP），超时 3s
    // 2. 如果 QUIC 连接失败（超时或明确拒绝），自动降级到 HTTPS
    // 3. HTTPS 使用相同端口号，TCP 与 UDP 在传输层天然隔离，OS 自动路由
    // 4. 返回实际使用的协议，供上层记录和日志
}
```

**HTTPS 回退设计要点**：
- **TCP 与 UDP 监听同一端口号**（如 54321）。客户端连接时先尝试 UDP (QUIC)，若超时则回退尝试 TCP (HTTPS)。两者在传输层天然隔离，无需应用层协议区分
- HTTPS 服务端使用 `axum`，提供最小 REST API（与 LocalSend v2 兼容）
- `DeviceInfo` 的 `protocol` 字段标识实际使用的协议：`"rustysend-quic-v1"` | `"rustysend-https-v1"`
- 降级成功后，前端显示"使用兼容模式连接"提示

**目录结构新增**：
```
src/transfer/
├── https.rs         # HTTPS 服务端/客户端（Phase 1.2）
└── fallback.rs      # 协议降级探测逻辑（Phase 1.2）
```

**风险缓解更新**（16 节）：
| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| QUIC 库不稳定 | 高 | 使用成熟的 quinn 库，准备回退到 HTTPS |
| 防火墙阻断 UDP | 高 | QUIC 失败后自动降级到 HTTPS（TCP 443/自定义端口） |

### 4.5 设备发现设计（UDP Multicast）

**背景**：LocalSend 的成功证明零配置发现是核心功能而非锦上添花。手动输入 IP 在手机→电脑、跨网段、DHCP 动态分配等场景几乎不可用。

**架构决策**：与 LocalSend 默认 multicast 组兼容，降低网络配置成本。

```rust
// 与 LocalSend 默认 multicast 组兼容
const MULTICAST_ADDR: &str = "224.0.0.167";
const DISCOVERY_PORT: u16 = 53317;  // 与 LocalSend 一致

#[derive(Serialize, Deserialize)]
pub struct DiscoveryPacket {
    pub alias: String,           // 设备别名
    pub version: String,         // 协议版本 "1.0"
    pub fingerprint: String,     // 证书 SHA-256 指纹，用于设备识别和 TOFU 验证
    pub port: u16,               // 实际传输端口（可能与发现端口不同）
    pub protocol: String,        // "rustysend-quic-v1" 等
    pub announce: bool,          // true = 主动宣告，false = 响应
    pub discovery_port: u16,     // 发现服务使用的 UDP 端口（用于应答路由）
}

pub struct DiscoveryHandle {
    pub shutdown_tx: Sender<()>,
}

pub async fn start_discovery(
    device_info: DeviceInfo,
    on_discovered: impl Fn(DiscoveryPacket),
) -> Result<DiscoveryHandle, DiscoveryError> {
    // 1. 尝试绑定 53317 端口，如果被占用则尝试 53318-53327
    // 2. 加入 multicast 组 224.0.0.167，向绑定的端口发送/接收
    // 3. 定期广播 DiscoveryPacket（announce=true，discovery_port=实际绑定端口）
    // 4. 监听其他设备的广播包
    // 5. 收到 announce=false 的请求包时，回复自身信息（向对方的 discovery_port 发送）
}
```

**端口复用策略**：
> 优先尝试 53317 端口。如果端口被占用（如 LocalSend 正在运行），自动尝试 53318-53327 范围内的端口作为**发现端口**。`DiscoveryPacket` 中新增 `discovery_port` 字段标识实际使用的发现端口，其他设备收到广播包后，向 `discovery_port` 发送应答包，确保应答能正确路由到 RustySend 而非 LocalSend。`port` 字段仍标识实际传输端口（QUIC/HTTPS 端口）。

**发现流程**：
1. 启动接收服务时，同时启动 discovery 广播
2. 每 3 秒发送一次 announce=true 的 DiscoveryPacket
3. 收到其他设备的广播包时，更新设备列表
4. 前端显示在线设备列表，用户点击即可发起传输

**目录结构新增**：
```
src-tauri/src/
├── discovery/
│   ├── mod.rs          # 模块导出
│   └── multicast.rs    # UDP multicast 发现逻辑
```

**Cargo.toml 新增依赖**：
```toml
tokio = { version = "1", features = ["full"] }  # "full" 已包含 "net"，用于 UDP multicast
```

**验收标准更新**（15.3 节）：
- [ ] 两台设备能互相发现（UDP multicast，与 LocalSend 兼容组地址）
- [ ] 手动输入 IP 作为 fallback
- [ ] 设备离线后自动从列表移除（超时 10 秒无广播）

---

## 5. 传输流程设计

### 5.1 发送流程

```
┌─────────────┐
│   开始发送   │
└──────┬──────┘
       │
       ▼
┌─────────────┐     失败     ┌─────────────┐
│ 建立 QUIC   │ ───────────► │  返回错误    │
│   连接      │  重试3次     │             │
└──────┬──────┘              └─────────────┘
       │
       ▼
┌─────────────────────────┐     失败     ┌─────────────┐
│ 发送 DeviceInfo          │ ───────────► │  返回错误    │
│ (含 supported_versions)  │  超时5s      │             │
│ 等待 DeviceInfo          │              │             │
└──────────┬──────────────┘              └─────────────┘
           │
           ▼
┌─────────────┐     拒绝     ┌─────────────┐
│ 发送 Transfer│ ───────────► │  返回拒绝    │
│   Request   │              │   原因      │
│ 等待 Accept │              │             │
└──────┬──────┘              └─────────────┘
       │
       ▼
┌─────────────┐
│ 发送 FileMeta│
└──────┬──────┘
       │
       ▼
┌─────────────────────┐     完成     ┌─────────────┐
│ 创建 unidirectional │ ───────────► │ 发送 Complete│
│ stream              │              │  关闭连接    │
│ 写入 16 字节 token   │              │             │
│ 写入裸文件字节(循环) │              │             │
└─────────────────────┘              └─────────────┘
```

### 5.2 接收流程

```
┌─────────────┐
│  等待连接    │
└──────┬──────┘
       │
       ▼
┌─────────────────────────┐
│ 收到 DeviceInfo          │
│ (含 supported_versions)  │
│ 发送 DeviceInfo          │
│ (含协商后 version)       │
└──────────┬──────────────┘
           │
           ▼
┌─────────────┐     拒绝     ┌─────────────┐
│ 收到 Transfer│ ───────────► │ 发送拒绝     │
│   Request   │              │  等待新连接  │
│  用户确认   │              │             │
└──────┬──────┘              └─────────────┘
       │
       ▼
┌─────────────┐
│ 发送 Accept │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ 收到 FileMeta│
│ 创建文件    │
└──────┬──────┘
       │
       ▼
┌─────────────────────┐     完成     ┌─────────────┐
│ accept_uni()        │ ───────────► │ 收到 Complete│
│ read_exact(16) 校验  │              │ 关闭文件     │
│ token，失败则 stop  │              │ 恢复文件时间戳│
│ 成功则流式写入(循环) │              │ 通知前端     │
└─────────────────────┘              └─────────────┘
```

---

## 6. 状态管理设计

### 6.1 应用状态

```rust
pub struct AppState {
    // 配置：读极多写极少，使用 tokio::sync::RwLock
    pub settings: Arc<tokio::sync::RwLock<Settings>>,
    
    // 接收服务状态：独立锁
    pub receiver: Arc<tokio::sync::Mutex<Option<ReceiverHandle>>>,
    
    // 活跃的传输会话：使用 DashMap 实现无锁并发读写
    pub transfers: Arc<DashMap<String, TransferSession>>,
}

// 锁策略说明：
// - DashMap 替代 RwLock<HashMap>，避免读写锁竞争，支持真正的并发访问
// - 各字段使用独立 Arc，从结构上消除嵌套锁依赖
// - 严禁在持有 settings 锁的情况下去获取 receiver 锁
// - 优先使用 tokio::sync::* 而非 parking_lot（后者在 .await 点会阻塞 OS 线程）

// DashMap 使用约束：
// - 避免在 iter() 中调用 remove()，如需清理，先 collect keys 再批量删除
// - TransferSession 中的 progress 用 Arc<AtomicU64>，DashMap 只持有 Arc 指针
// - 定期清理（如 Completed 超过 1 小时的会话）应在独立 task 中进行

pub struct ReceiverHandle {
    pub shutdown_tx: Sender<()>,
    pub local_addr: SocketAddr,
}

pub struct TransferSession {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub progress: Arc<AtomicU64>,
    pub status: TransferStatus,
}

#[derive(Serialize)]
pub enum TransferStatus {
    Pending,      // 等待确认
    InProgress,   // 传输中
    Completed,    // 完成
    Failed,       // 失败
    Cancelled,    // 取消
}
```

**序列化说明**：`TransferStatus` 使用 serde 默认 PascalCase 序列化。前端 TypeScript 类型应使用 `'Pending' | 'InProgress' | 'Completed' | 'Failed' | 'Cancelled'`。

### 6.2 配置结构

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub save_path: PathBuf,
    pub device_name: String,
    pub port: u16,
    pub auto_accept: bool,
    pub file_exists_policy: FileExistsPolicy,  // 文件已存在时的处理策略
    pub max_concurrent_transfers: u32,         // 最大并发传输数
    pub buffer_pool_size: u32,                 // 预留：Phase 2 动态 buffer 调整时使用，Phase 1 固定 1MB
    pub connection_timeout_secs: u32,          // 连接超时时间 (秒)
}

#[derive(Serialize, Deserialize, Clone)]
pub enum FileExistsPolicy {
    Overwrite,  // 覆盖
    Rename,     // 自动重命名：file.txt → file (1).txt
    Reject,     // 拒绝传输
}

// 注：`auto_accept=false` 已覆盖"询问用户"场景（传输确认弹窗）。
// `FileExistsPolicy` 仅控制文件已存在时的处理策略，不涉及传输确认。
//
// ⚠️ 文件存在性检查时机：
// 收到 TransferRequest 后，**立即检查目标路径文件是否存在**。
// - 若存在且策略为 `Reject`：直接发送 `TransferAccept { accepted: false, reject_reason: "File exists" }`，不触发前端确认弹窗
// - 若存在且策略为 `Rename`：计算新文件名（如 `file (1).txt`），在 TransferAccept 中返回最终文件名，继续流程
// - 若不存在：正常继续流程
// 这样避免"用户同意传输 → 后端发现文件已存在且策略为 Reject → 报错"的糟糕体验

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_path: default_download_dir(),
            device_name: default_device_name(),
            port: 54321,
            auto_accept: false,           // 默认关闭，防止恶意文件推送
            file_exists_policy: FileExistsPolicy::Rename,  // 默认重命名，避免数据丢失
            max_concurrent_transfers: 4,
            buffer_pool_size: 4,          // 4MB (1MB/流 × 4 并发)，满足 50MB/s 目标
            connection_timeout_secs: 10,
        }
    }
}

// 安全说明：
// - auto_accept 默认关闭，防止恶意文件推送
// - 未来可扩展为信任设备白名单，仅对信任设备自动接收
```

---

## 7. Tauri 接口设计

### 7.1 命令（Frontend → Backend）

```rust
// 启动接收服务
#[tauri::command]
async fn start_receiver(
    state: State<'_, Arc<AppState>>,
    window: Window,
) -> Result<ReceiverInfo, String>;

// 停止接收服务
#[tauri::command]
async fn stop_receiver(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String>;

// 发送文件（支持设备发现后的直接调用）
// Protocol 枚举定义见 4.4 节，Tauri v2 支持枚举反序列化，前端传 "Quic" 或 "Https" 即可自动映射
#[tauri::command]
async fn send_file(
    state: State<'_, Arc<AppState>>,
    window: Window,
    file_path: String,
    target_ip: String,
    target_port: u16,           // 从发现或手动输入获取
    protocol: Protocol,         // 使用枚举而非 String，类型安全
) -> Result<TransferResult, String>;

// 获取设置
#[tauri::command]
async fn get_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<Settings, String>;

// 保存设置
#[tauri::command]
async fn save_settings(
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<(), String>;

// 获取活跃传输列表（前端 reload 后恢复状态）
// ⚠️ 注意：TransferSession 包含 Arc<AtomicU64>，不支持 serde 序列化
// 必须使用 DTO 转换为可序列化的结构，status 与 TransferStatus 枚举保持一致
#[derive(Serialize)]
pub struct TransferSessionDto {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub progress: u64,      // 从 AtomicU64 load() 转换
    pub status: TransferStatus,  // 与核心枚举一致
    pub peer_ip: String,
}

#[tauri::command]
async fn get_active_transfers(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<TransferSessionDto>, String>;

// 取消传输
#[tauri::command]
async fn cancel_transfer(
    state: State<'_, Arc<AppState>>,
    transfer_id: String,
) -> Result<(), String>;
```

### 7.2 事件（Backend → Frontend）

```typescript
// 传输进度（增量推送，减少后端计算开销）
interface TransferProgressEvent {
  transferId: string;
  bytesDelta: number;    // 自上次事件新增的字节数（前端累加计算 total）
  timestamp: number;     // 后端时间戳（ms），前端计算 speed = bytesDelta / delta_time
}

// 前端速度计算（滑动窗口平均）：
// - 窗口大小：2 秒
// - 采样频率：每 100ms 收到 bytesDelta
// - speed = (最近 20 个 bytesDelta 之和) / 2.0
// - 避免瞬时抖动，显示更平滑的速度
// - 前端维护 Map<transferId, { totalBytes: number, speedHistory: number[] }>
// - 边界情况：若历史采样不足 20 个（传输初期），按实际样本数计算：speed = (sum) / (样本数 × 0.1s)，避免显示 0

// 事件保序说明：
// - 进度事件通过 QUIC bidirectional stream（可靠有序字节流）传输
// - QUIC stream 天然保证消息按发送顺序到达，前端直接累加 bytesDelta 即可
// - 无需处理乱序事件，无需 timestamp 比较逻辑

// 传输完成
interface TransferCompleteEvent {
  transferId: string;
  success: boolean;
  filePath?: string;
  error?: string;
}

// 收到文件
interface FileReceivedEvent {
  filePath: string;
  fileName: string;
  fileSize: number;
  senderName: string;
}

// 传输请求（需要用户确认）
interface TransferRequestEvent {
  transferId: string;
  fileName: string;
  fileSize: number;
  senderName: string;
  senderIp: string;
}

// 事件监听与取消订阅（Tauri v2）
// 注意：listen() 返回的 UnlistenFn 必须在组件卸载时调用，防止内存泄漏
// 
// 示例：
// const unlisten = await listen<TransferProgressEvent>('transfer-progress', (event) => {
//   console.log(event.payload);
// });
// 
// // 组件卸载时
// unlisten();
```

---

## 8. 错误处理策略

### 8.1 错误类型

```rust
#[derive(Error, Debug)]
pub enum TransferError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Handshake timeout")]
    HandshakeTimeout,

    #[error("Transfer rejected: {0}")]
    TransferRejected(String),

    #[error("Insufficient disk space")]
    InsufficientSpace,

    #[error("File hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Version mismatch: supported {supported}, got {received}")]
    VersionMismatch { supported: Vec<u32>, received: u32 },

    #[error("Invalid data stream token")]
    InvalidToken,

    #[error("File IO error: {0}")]
    FileIo(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("QUIC error: {0}")]
    Quic(String),

    #[error("Transfer cancelled")]
    Cancelled,

    #[error("Certificate fingerprint mismatch: expected {expected}")]
    CertFingerprintMismatch { expected: String },

    // 新增：LocalSend 验证过的业务错误码
    #[error("Session conflict: another transfer is in progress")]
    SessionConflict,

    #[error("Too many requests")]
    RateLimited,

    #[error("PIN required or invalid")]
    InvalidPin,

    #[error("Path traversal blocked: {0}")]
    PathTraversalBlocked(String),
}

// 新增：结构化错误响应（预留国际化支持）
#[derive(Serialize, Deserialize)]
pub struct TransferErrorResponse {
    pub code: String,                    // 机器可读码，如 "HashMismatch"
    pub message: String,                 // 英文默认描述
    pub details: Option<serde_json::Value>, // 动态参数，如 { expected: "abc", actual: "def" }
}

// 使用示例：
// TransferError::HashMismatch { expected, actual } => TransferErrorResponse {
//     code: "HashMismatch".to_string(),
//     message: "File hash mismatch".to_string(),
//     details: Some(json!({ "expected": expected, "actual": actual })),
// }
```

### 8.2 重试策略

| 场景 | 重试次数 | 间隔策略 | 行为 |
|------|----------|----------|------|
| 连接失败 | 3 | 1s → 2s → 4s | 指数退避 |
| 握手超时 | 2 | 2s → 4s | 指数退避（HANDSHAKE_TIMEOUT=5s，超时后等待 2s 再重试） |

**注**：QUIC 传输层已有内建重传机制，应用层不应对"发送超时"进行重试，避免双重重传。

---

## 9. 依赖项

### 9.1 Cargo.toml 添加

```toml
[dependencies]
# 现有依赖...
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
tauri-plugin-store = "2"  # 确认兼容 Tauri v2

# QUIC
quinn = "0.11"
rustls = { version = "0.23", default-features = false, features = ["ring"] }

# 证书生成
rcgen = "0.13"
rustls-pemfile = "2"  # rcgen 证书类型与 rustls 期望类型之间的 PEM 转换

# 异步运行时
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"  # tokio 工具库（codec、io 等）

# UUID
uuid = { version = "1", features = ["v4", "serde"] }

# 数据完整性校验
blake3 = "1"

# 错误处理
thiserror = "1"

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 文件系统
dirs-next = "5"
fs2 = "0.4"           # 磁盘空间检查（精确查询路径所在分区的可用空间）

# hex 编码（data_stream_token 序列化）
hex = "0.4"

# 随机数生成（data_stream_token、session_token）
rand = "0.8"

# HTTPS 回退（Phase 1.3）
axum = "0.7"
tower = "0.4"
hyper = { version = "1", features = ["full"] }
hyper-util = "0.1"
tokio-rustls = "0.26"

# 并发原语（替代方案）
dashmap = "6"         # 并发 HashMap，替代 RwLock<HashMap>

# 日志
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"  # 日志文件轮转

# 性能测试
criterion = { version = "0.5", features = ["html_reports"] }

# 系统信息（内存采样等）
sysinfo = "0.30"
```

---

## 10. 日志规范

### 10.1 日志级别使用

| 级别 | 使用场景 | 示例 |
|------|----------|------|
| `ERROR` | 传输失败、IO 错误、连接断开 | `传输失败: {transfer_id}, 错误: {err}` |
| `WARN` | 重试、超时、异常但可恢复 | `连接超时，第 {retry} 次重试` |
| `INFO` | 关键流程节点 | `启动接收服务: {addr}`, `开始传输: {transfer_id}` |
| `DEBUG` | 详细流程信息 | `发送块 {index}/{total}, 大小: {size}` |
| `TRACE` | 最详细的数据流 | `原始消息内容: {bytes:?}` |

### 10.2 结构化日志字段

```rust
// 使用 tracing 的 span 和字段
info!(
    transfer_id = %transfer_id,
    file_name = %file_name,
    file_size = file_size,
    target_ip = %target_ip,
    "开始文件传输"
);

// 错误日志
error!(
    transfer_id = %transfer_id,
    error = %err,
    stage = "handshake",  // 错误发生的阶段
    "传输失败"
);
```

### 10.3 日志输出

- 开发环境：输出到控制台 + 文件
- 生产环境：输出到文件，按天轮转
- 日志路径：`{app_data}/logs/rustysend_{date}.log`

---

## 11. Phase 1 性能目标

### 11.1 性能指标

| 指标 | 目标值 | 测量条件 | 优先级 |
|------|--------|----------|--------|
| 单文件速度 | ≥ 50 MB/s | 千兆有线，1GB 文件 | P0 |
| 握手延迟 | ≤ 500ms | DeviceInfo 往返 | P0 |
| 内存占用 | ≤ 50 MB | 传输 100MB 文件 | P1 |
| CPU 占用 | ≤ 20% | 单核，传输中 | P1 |
| 并发传输 | 4 个 | 同时传输不卡顿 | P1 |

### 11.2 资源限制

```rust
// 并发控制
const MAX_CONCURRENT_TRANSFERS: usize = 4;

// Phase 1：固定 1MB buffer，平衡内存占用与吞吐
// 4 并发 × 1MB = 4MB，加上 QUIC 接收窗口（2MB/流 × 4 = 8MB），总计约 12-15MB
// 远低于 50MB 内存目标，留有充足余量
const BUFFER_SIZE: usize = 1024 * 1024; // 1MB

// 背压与流控说明：
// - 发送端仅根据 QUIC 的流控窗口发送，不额外做应用层限速
// - 依靠系统 UDP 缓冲区正常调度，QUIC 自动处理拥塞控制
// - 4 流全速发送时，总接收窗口 8MB + 应用层 4MB buffer = ~12MB，仍在内存目标内

// Phase 2：根据 benchmark 结果决定是否实现动态 buffer 调整
// 动态调整方案（预留）：
// - 如果连续 3 个周期 speed > 20MB/s，buffer *= 2
// - 如果 speed < 5MB/s，buffer /= 2
// - 上限 2MB，下限 64KB

// QUIC 接收窗口限制（控制内存）
// let mut config = quinn::TransportConfig::default();
// config.receive_window(VarInt::from_u64(2 * 1024 * 1024)); // 2MB
// config.send_window(2 * 1024 * 1024);

// 超时配置
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

// 动态传输超时：基础 5 分钟 + 每 100MB 增加 2 分钟
const BASE_TRANSFER_TIMEOUT: Duration = Duration::from_secs(300);
const TIMEOUT_PER_100MB: Duration = Duration::from_secs(120);

fn calculate_transfer_timeout(file_size: u64) -> Duration {
    BASE_TRANSFER_TIMEOUT + (file_size / (100 * 1024 * 1024)) * TIMEOUT_PER_100MB
}
```

### 11.3 测试方法学

**基准测试工具**（`src-tauri/benches/`）：

```rust
// 使用 criterion 做标准化测试
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_transfer_speed(c: &mut Criterion) {
    c.bench_function("transfer_1gb", |b| {
        b.iter(|| {
            // 测量条件：
            // - 本地回环 (127.0.0.1)
            // - 1GB 随机数据文件
            // - 不计入握手时间（只测数据传输）
            // - 预热：忽略前 100MB
            black_box(run_transfer_test(1024 * 1024 * 1024))
        })
    });
}
```

**测量标准**：
- **吞吐**: `file_size / (complete_time - first_byte_time)`
- **延迟**: DeviceInfo 往返时间（发送 DeviceInfo 到收到对端 DeviceInfo）
- **内存**: `max_rss - baseline_rss`（使用 `sysinfo` 采样）
- **CPU**: `cpu_time / wall_time`（单核百分比）

---

## 12. 清理策略

### 12.1 传输失败处理

| 失败阶段 | 处理方式 | 说明 |
|----------|----------|------|
| 握手前 | 无操作 | 未创建文件 |
| 传输中 | 删除不完整文件 | 收到 Error/Cancel 时删除 |
| 写入后 | 保留 | 校验失败时保留供调试 |

### 12.2 临时文件管理

```rust
// 接收文件时先写入 .tmp 文件
let temp_path = format!("{}.tmp", final_path);

// 传输完成后重命名
if success {
    fs::rename(&temp_path, &final_path)?;
} else {
    fs::remove_file(&temp_path)?;
}
```

### 12.3 定期清理

- 启动时清理超过 24 小时的 .tmp 文件
- 记录未完成的传输到日志

---

## 13. 大文件优化

### 13.1 磁盘预分配

对于 > 1GB 的文件，预先分配磁盘空间：

```rust
if file_size > 1024 * 1024 * 1024 {
    file.set_len(file_size)?;  // 预分配
}
```

### 13.2 顺序写入优化

**⚠️ 注意**：`std::io::BufWriter` 是同步 API，在 async 函数中调用会阻塞 tokio runtime worker 线程。

**Phase 1 采用方案 A：手动 1MB buffer + `write_all`（推荐）**：
```rust
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

const BUFFER_SIZE: usize = 1024 * 1024; // 1MB

let mut buf = vec![0u8; BUFFER_SIZE];
let mut file = File::create(&temp_path).await?;
loop {
    let n = stream.read(&mut buf).await?;
    if n == 0 { break; }
    file.write_all(&buf[..n]).await?; // 异步写入，不阻塞 runtime
}
file.flush().await?;
```

**优势**：
- 避免 `spawn_blocking` 的上下文切换开销
- 可控 buffer 大小，与 11.2 节资源限制一致
- 纯异步 IO，不阻塞 tokio worker 线程

**⚠️ 性能预警**：`tokio::fs::File` 底层依赖 `spawn_blocking` 线程池执行同步 IO。1MB 大块写入可显著减少调度次数（50MB/s 约 50 次/秒）。若 benchmark 发现 CPU 占用过高（>20%），应切换为**方案 B**（在单个 `spawn_blocking` 任务中使用 `std::fs::File` + `std::io::BufWriter`）。

**替代方案 B**（不推荐，仅在有特殊需求时考虑）：
```rust
// 使用 spawn_blocking + std::io::BufWriter
let file = std::fs::File::create(&temp_path)?;
let mut writer = std::io::BufWriter::with_capacity(BUFFER_SIZE, file);
tokio::task::spawn_blocking(move || {
    // 在阻塞线程池中执行同步 IO
}).await??;
```

### 13.3 关于内存映射（mmap）

**Phase 1 不使用 mmap**，原因：
- Windows 上文件被 mmap 后无法删除/重命名
- 网络传输速度 < 磁盘 IO 时，mmap 没有优势
- 内存不足时会触发大量 page fault

**替代方案**：`BufWriter<File>` + 1MB buffer 已经足够高效。mmap 留给 Phase 2 做 benchmark 后再决定。

---

## 14. 实现计划

### Phase 1.1: 基础架构
- [ ] 创建 transfer 模块结构
- [ ] 实现 protocol.rs 消息定义（含 DeviceInfo/TransferRequest/TransferAccept/FileMeta/Complete/Cancel）
- [ ] 添加依赖到 Cargo.toml
- [ ] 创建 discovery 模块结构

### Phase 1.2: 设备发现 + QUIC 连接
- [ ] 实现证书生成（3 年有效期 + rotate 接口）
- [ ] 实现 UDP multicast 设备发现（224.0.0.167:53317，与 LocalSend 兼容）
- [ ] 实现 QuicServer / QuicClient
- [ ] 实现协议降级探测框架（QUIC 优先 → HTTPS 回退，HTTPS 部分 Phase 1.3 实现）
- [ ] 测试本地回环连接（QUIC 路径）

### Phase 1.3: HTTPS 回退（网络兼容性）
- [ ] 实现 HttpsServer（axum，最小 REST API，与 LocalSend v2 兼容）
- [ ] 实现协议降级探测完整逻辑
- [ ] 测试本地回环连接（QUIC + HTTPS 双路径）

### Phase 1.4: 接收功能
- [ ] 实现 receiver.rs 核心逻辑（含 session_token/data_stream_token 校验）
- [ ] 实现接收命令
- [ ] 测试接收流程（含 hash 校验）

### Phase 1.5: 发送功能
- [ ] 实现 sender.rs 核心逻辑
- [ ] 实现发送命令
- [ ] 测试发送流程

### Phase 1.6: 前端集成
- [ ] 添加 Tauri 事件监听（增量进度推送）
- [ ] 更新 TransferPage
- [ ] 实现设备发现 UI
- [ ] 端到端测试

---

## 15. 测试策略

### 15.1 单元测试
- 消息序列化/反序列化
- 证书生成
- 分块逻辑

### 15.2 集成测试
- 本地回环传输 (127.0.0.1)
- 局域网双机测试
- 大文件传输 (>1GB)
- 网络中断恢复

### 15.3 验收标准
- [ ] 两台设备能互相发现（UDP multicast，与 LocalSend 兼容组地址）
- [ ] 手动输入 IP 作为 fallback
- [ ] 小文件 (<1MB) 传输成功
- [ ] 大文件 (>100MB) 传输成功
- [ ] 传输进度实时显示
- [ ] 传输完成前端收到通知
- [ ] 路径遍历攻击被阻断（如 file_name="../../../etc/passwd"）
- [ ] 设备离线后自动从列表移除（超时 10 秒无广播）

**关于设备发现**：
> Phase 1 实现 UDP multicast 发现（与 LocalSend 兼容组地址 224.0.0.167:53317），手动输入 IP 作为 fallback。

---

## 16. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| QUIC 库不稳定 | 高 | 使用成熟的 quinn 库，准备回退到 HTTPS |
| 防火墙阻断 UDP | 高 | QUIC 失败后自动降级到 HTTPS（TCP 443/自定义端口） |
| 大文件内存占用 | 中 | 流式处理，固定 1MB buffer，QUIC 接收窗口限制 2MB |
| 跨平台兼容性 | 中 | CI 测试 Windows/macOS/Linux |

---

## 17. 前端错误边界说明

### 17.1 重置策略

使用 `window.location.reload()` 而非状态重置的原因：

> Tauri 桌面应用的错误边界通常是全局性的，简单重载比尝试修复不可预测的状态更可靠。Rust 后端状态独立于前端，重载不会丢失已建立的传输连接。

### 17.2 错误分类

| 错误类型 | 处理方式 | 用户提示 |
|----------|----------|----------|
| 渲染错误 | 显示错误边界 | "应用出错，点击刷新" |
| 网络错误 | 自动重试 | "连接中..." / "连接失败" |
| 传输错误 | 通知 + 记录 | "传输失败: {原因}" |

---

## 附录: 前端接口定义

```typescript
// src/types/transfer.ts

export interface TransferSessionDto {
  transferId: string;
  fileName: string;
  fileSize: number;
  progress: number;
  status: 'Pending' | 'InProgress' | 'Completed' | 'Failed' | 'Cancelled';
  peerIp: string;
}

export interface TransferProgressEvent {
  transferId: string;
  bytesDelta: number;
  timestamp: number;
}

export interface TransferCompleteEvent {
  transferId: string;
  success: boolean;
  filePath?: string;
  error?: string;
}

export interface FileReceivedEvent {
  filePath: string;
  fileName: string;
  fileSize: number;
  senderName: string;
}

export interface TransferRequestEvent {
  transferId: string;
  fileName: string;
  fileSize: number;
  senderName: string;
  senderIp: string;
}

// src/types/discovery.ts

export interface DiscoveryDevice {
  id: string;          // fingerprint
  name: string;
  ip: string;
  port: number;
  protocol: string;    // "rustysend-quic-v1" | "rustysend-https-v1"（与 DiscoveryPacket.protocol 一致）
  lastSeen: number;    // timestamp
}

// src/api/transfer.ts

export async function startReceiver(): Promise<{ port: number }>;
export async function stopReceiver(): Promise<void>;
export async function sendFile(
  filePath: string, 
  targetIp: string,
  targetPort: number,
  protocol: 'Quic' | 'Https'
): Promise<{ transferId: string }>;
export async function getActiveTransfers(): Promise<TransferSessionDto[]>;
export async function cancelTransfer(transferId: string): Promise<void>;

// 事件监听
export function onTransferProgress(
  callback: (event: TransferProgressEvent) => void
): UnlistenFn;

export function onTransferComplete(
  callback: (event: TransferCompleteEvent) => void
): UnlistenFn;

export function onFileReceived(
  callback: (event: FileReceivedEvent) => void
): UnlistenFn;

export function onTransferRequest(
  callback: (event: TransferRequestEvent) => void
): UnlistenFn;

// src/api/discovery.ts

export function onDeviceDiscovered(
  callback: (device: DiscoveryDevice) => void
): UnlistenFn;

export function onDeviceLost(
  callback: (deviceId: string) => void
): UnlistenFn;
```
