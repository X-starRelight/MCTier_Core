# MCTier_Core C ABI FFI 层设计（阶段 4）

日期：2026-10-06
状态：设计已确认（含外部评审 10 条修订），待实现

## 0. 项目定位声明

**本项目为 DEMO 项目，不是正式版产品，不承诺完全可用。**

但工程实现仍按生产级标准执行：契约必须补齐、88 个命令全量暴露、四平台产物齐全、错误与并发语义明确。"DEMO" 限定的是**交付承诺**（不保证生产环境长期稳定运行、不做 ABI 版本兼容承诺、不建 CI），不是**质量标准**。

## 1. 背景与目标

MCTier_Core 是从 MCTier（Tauri + React 桌面程序）提取的纯 Rust 后端，只保留组网、联机、协作能力。阶段 1 已删除 GUI（React 前端、Tauri 壳、GUI 相关模块），阶段前清理已删除夸克三件套、实时语音五模块与 builtin-emoji。

阶段 4 的目标：把核心封装为**跨平台 C ABI 动态库**，让任意宿主程序（C/C++/C#/Python/Go/Rust）通过 FFI 直接调用，宿主自建界面。

### 非目标

- 不做语言绑定层（C#/Python/Go 绑定由宿主或后续阶段负责）。
- 不做实时语音、屏幕共享、字幕、网盘助力（已在阶段前清理删除）。
- 不保留 tauri 运行时依赖（见 §3）。

## 2. 已确认的决策

| # | 决策点 | 结论 |
|---|---|---|
| D1 | 目标平台 | Windows `.dll`、Linux `.so`、macOS `.so`、Android `.so` 四者全要 |
| D2 | 命令暴露范围 | 现有 88 个 tauri 命令**一次性全部暴露** |
| D3 | tauri 耦合 | **彻底移除 tauri 依赖**（不保留编译依赖，不拆 crate） |
| D4 | 事件通道 | **回调为主**，`read_frame` 作为 cb 为 NULL 时的备用路径 |
| D5 | 调用结果回传 | 走事件通道，带 `req_id` 关联；`call_sync` 是阻塞等结果的便捷封装 |
| D6 | EasyTier 二进制 | **支持宿主注入 + 内嵌兜底**（Windows/Linux 内嵌，macOS/Android 必须注入） |
| D7 | 桥接方案 | **方案 A：手工分发表，按现有源文件拆 13 个 dispatch 模块** |
| D8 | C ABI 面 | 五组共 111 个函数（6 核心 / 8 查询自描述 / 5 调用控制 / 88 强类型直连 / 4 日志配置） |
| D9 | 强类型直连形态 | **参数用强类型，返回统一 JSON 字符串**（不做 C 结构体返回值） |
| D10 | 外部评审 | 十条契约缺陷全部采纳（见 §11），含生命周期、异步状态机、符号导出 |

### 方案取舍记录

桥接层评估过三个方案：

- **A. 手工分发表（选定）**：零新依赖、编译期类型安全、错误一眼可见、文件规模可控。代价是 88 处机械样板。
- B. 宏驱动分发：样板少，但宏展开难调试，且命令签名不规则（有的带 `AppHandle`/`Channel`、返回类型各异）会让宏膨胀。
- C. 类型擦除注册表：数据驱动，但失去编译期检查，注册遗漏只能运行时发现，与现有 async 函数贴合度差。

不给返回值做 C 结构体的理由：`Lobby`/`UserConfig` 等结构一改字段 ABI 即断，还要维护 88 套深度释放函数；JSON 返回让释放只有一条规则，版本演进靠 JSON 字段而非 ABI 布局。

## 3. 架构与文件布局

### 3.1 命令分布

88 个命令（`#[tauri::command]` 精确计数，无重名）分布在 13 个源文件：

| 文件 | 数量 | | 文件 | 数量 |
|---|---|---|---|---|
| `tauri_commands/file_share.rs` | 15 | | `easytier_advanced_commands.rs` | 5 |
| `tauri_commands/network.rs` | 13 | | `tauri_commands/diagnostics.rs` | 4 |
| `tauri_commands/chat.rs` | 13 | | `minecraft_discovery.rs` | 3 |
| `tauri_commands/session.rs` | 11 | | `tauri_commands/archive.rs` | 3 |
| `tauri_commands/settings.rs` | 8 | | `secret_store.rs` | 2 |
| `tauri_commands/remote_files.rs` | 7 | | `tauri_commands/logging.rs` | 2 |
| | | | `mc_lan_bridge.rs` | 2 |

### 3.2 去 tauri 重构（改现有代码）

耦合面量化（重构前）：

| 项 | 数量 |
|---|---|
| `#[tauri::command]` | 88 |
| `State<'_, AppState>` | 59 |
| `AppHandle` | 49 |
| `tauri::` 总引用 | 138（跨 15 文件） |

重构规则：

1. `#[tauri::command]` → 普通 `pub async fn`。
2. `State<'_, AppState>` → 参数 `AppState`（`#[derive(Clone)]`）。`AppState` 目前只有一个字段 `core: Arc<Mutex<AppCore>>`，克隆是廉价的 Arc 克隆。
3. 新增 `modules/event_sink.rs`：
   ```rust
   pub trait EventEmitter: Send + Sync {
       fn emit(&self, name: &str, payload: serde_json::Value) -> Result<(), String>;
   }
   ```
4. `tauri_events.rs` 的 6 个 `emit_*(app_handle, ...)` 改为接收 `&dyn EventEmitter`，内部逻辑不变。
5. `resource_manager.rs` 去掉 `use tauri::Manager`。
6. 顺手删除 `error.rs` 中零外部引用的 `AppError::AudioError` 与 `AppError::VoiceError` 及其 `match` 分支。

**AppHandle 的非 emit 用途（去 tauri 时必须一并处理）**：`AppHandle` 不只用于 `emit`。`ResourceManager::get_runtime_path(app_handle)` 内部调用 `app_handle.path().resource_dir()`，被 `get_easytier_path`、`get_easytier_cli_path`、`get_wintun_dll_path`、`get_windivert_sys_path`、`get_config_dir`、`get_log_dir` 使用，命令层有 4+ 处调用。这些路径解析改为从运行时上下文读取注入值（见 §8.4 的 `runtime_dir`），不再依赖 tauri 的应用路径 API。

`restart_as_admin` 等命令中 `let _ = app_handle;` 的占位参数在去 tauri 时直接删除。

`AppError` 各变体外部引用实测：ValidationError 18、NetworkError 26、ConfigError 47、ProcessError 11、FileError 8、AudioError 0、VoiceError 0。`IoError`/`SerializationError` 通过 `From` impl 隐式使用，保留。

### 3.3 新增 FFI 层（不碰业务代码）

```
src/src/ffi/
├── mod.rs           导出 #[no_mangle] extern "C" 函数
├── runtime.rs       tokio runtime 全局持有 + init/shutdown 生命周期
├── events.rs        事件队列 + 回调分发 task + 背压
├── error.rs         int32 返回码常量与 AppError 映射
├── capabilities.rs  命令 → 平台要求 + 动态探测的静态表
└── dispatch/
    ├── mod.rs                    按命令前缀路由到下列模块
    ├── chat.rs  network.rs  session.rs  settings.rs
    ├── file_share.rs  remote_files.rs  archive.rs
    ├── diagnostics.rs  logging.rs
    ├── easytier_advanced.rs  minecraft_discovery.rs
    └── secret_store.rs  mc_lan_bridge.rs
include/mctier_core.h
examples/c/demo.c
```

dispatch 模块数与 §3.1 的源文件一一对应（13 个），`dispatch/mod.rs` 持有 `COMMANDS` 常量表并做前缀路由。

`[lib] crate-type` 当前已含 `["staticlib", "cdylib", "rlib"]`，无需改动。

### 3.4 数据流

事件 → 序列化为 `Frame { name, payload }` → 入 mpsc 队列 → 两种出口：

- 注册了 cb（`SinkMode::Callback`）：由**单一分发 task** 顺序调用 cb（宿主不会并发收到回调）。
- cb 为 NULL（`SinkMode::Pull`）：留在队列，等 `read_frame` 批量取走。

调用流：`call` 拷贝 `args_json` → 查 dispatch 表 → 投递到 tokio task → 完成后入队 `command-result`。`call_sync` 与它同一条执行路径但用独立 `oneshot` 等结果（不经队列，见 §5.1），是阻塞封装。

## 4. C ABI 接口面

```c
typedef void (*MctierEventCb)(const char* event_name,
                              const char* payload_json,
                              void* user_data);
typedef void (*MctierLogCb)(int32_t level, const char* message, void* user_data);
```

### 4.1 核心闭环（6 个）

```c
int32_t mctier_core_init(const char* options_json, MctierEventCb cb, void* user_data);
int32_t mctier_core_call(const char* req_id, const char* cmd, const char* args_json);
char*   mctier_core_call_sync(const char* cmd, const char* args_json);
int32_t mctier_core_read_frame(uint8_t* buf, size_t cap);
void    mctier_core_free_string(char* s);
void    mctier_core_shutdown(void);
```

### 4.2 查询与自描述（8 个）

```c
const char* mctier_core_version(void);              /* 静态串，无需 free */
int32_t     mctier_core_is_initialized(void);
char*       mctier_core_list_commands_json(void);   /* 恒列全 88 条，附 supported 标记 */
int32_t     mctier_core_has_capability(const char* cmd);
int32_t     mctier_core_event_count(void);
int32_t     mctier_core_next_frame_size(void);      /* 队首帧所需字节数，0 = 队列空 */
const char* mctier_core_last_error(void);           /* TLS 内部缓冲，宿主不得 free */
int32_t     mctier_core_pending_calls(void);
```

`last_error` 返回 `const char*` 而非 `char*`：它指向线程局部内部缓冲，宿主**不得释放**，且指针只在同一线程的下一次 FFI 调用前有效。这与 `char*`（malloc、须 `free_string` 释放）形成明确区分。`next_frame_size` 配合 `read_frame` 的 `-6` 使用：宿主先取所需大小再分配缓冲。

### 4.3 调用与等待控制（5 个）

```c
int32_t mctier_core_call_cancel(const char* req_id);
int32_t mctier_core_poll(uint32_t timeout_ms);      /* 超时等待，返回待取帧数 */
int32_t mctier_core_call_with_timeout(const char* req_id, const char* cmd,
                                      const char* args_json, uint32_t timeout_ms);
int32_t mctier_core_set_event_queue_capacity(uint32_t n);
void    mctier_core_reset_last_error(void);
```

### 4.4 强类型命令直连（88 个）

参数使用 C 原生类型与 `const char*`，**返回值统一为 malloc 的 JSON 字符串**（返回 `()` 或 `bool` 的命令直接返回 `int32_t`）：

```c
char* mctier_join_lobby(const char* name, const char* password,
                        const char* player_name, const char* player_id,
                        const char* server_node, const char* signaling_server,
                        int32_t use_domain, int32_t address_attempt);
char* mctier_get_players(void);
int32_t mctier_leave_lobby(void);
```

返回 `char*` 的用 `mctier_free_string` 释放。

**命名规则**：`mctier_` + 命令名原样（已实测 88 个命令名无重复，无需后缀消歧）。参数顺序与 Rust 函数一致，`AppHandle`/`State` 这类注入参数在去 tauri 后不出现在 C 签名里。

**参数映射必须逐个在头文件标注**，不能只靠通用规则。实测有 14 个命令含无法用标量表达的参数：

| 参数形态 | 映射方式 |
|---|---|
| `Option<String>` | `const char*`，`NULL` = None |
| `Option<bool>` / `Option<u16>` | `int32_t`，`-1` = None |
| `Option<Vec<serde_json::Value>>` | `const char*` JSON 数组，`NULL` = None |
| `Vec<String>`、`Vec<McServer>` | `const char*` JSON 数组 |
| `Vec<ChatPeerIdentity>` | `const char*` JSON 数组 |
| `Vec<u8>`（如 `image_data`、`data`） | `const uint8_t* + size_t len` |
| `serde_json::Value`（如 `message`、`config_json`） | `const char*` JSON 文本 |
| `mut config: UserConfig`（结构体） | `const char*` JSON 文本 |

因此**带复杂参数的命令，其多余参数统一退化为 `const char* args_json`**（签名尾部追加一个 JSON 载荷参数），简单参数仍用原生类型。头文件为每个命令逐一注释哪些参数可为 `NULL`、哪些用 `-1` 表示未设置。

每导出一个直连函数，就同步在 `COMMANDS` 表里登记其 C 名，使 `list_commands_json()` 能同时给出 `cmd`（走 `call` 用）与 `c_symbol`（直连用）两种调用方式。

参数复杂度实测：0 参数 7 个、1 参数 47 个、2 参数 18 个、3 参数 3 个，`join_lobby` 最多（去 tauri 后 8 个业务参数）。

### 4.5 日志与配置（4 个）

```c
void    mctier_core_set_logger(MctierLogCb cb, void* user_data, int32_t level);
int32_t mctier_core_set_option(const char* key, const char* value);
char*   mctier_core_get_option(const char* key);
void    mctier_core_flush_logs(void);
```

**`MctierLogCb` 可能在多个线程被并发调用**（tokio worker、EasyTier 子进程读取线程、回调分发 task 都会产日志）。与 `MctierEventCb` 的"单一分发 task 顺序触发"不同，日志回调**没有顺序与并发保证**，宿主实现必须线程安全。这条约束写入头文件注释。

## 5. 函数契约细节

### 5.1 生命周期与线程

**运行时存储与重入**：`init` 创建 multi-thread tokio runtime，存入

```rust
static RUNTIME: Mutex<Option<(Arc<Runtime>, SinkHandle)>> = ...;
```

不能用裸 `OnceLock`——它一经设置无法重置，`shutdown` 释放 runtime 后就再也无法 `init`。规则：

- 首次 `init` 成功返回 `0`；已初始化时再次 `init` 返回 `-2 ERR_ALREADY_INITIALIZED`。
- `shutdown` 清空该 `Option`（drop runtime、abort 分发 task、drain 队列），幂等。
- **`shutdown` 之后允许重新 `init`**（宿主重启库会话时不必重新加载动态库）。

**出口模式（互斥，`init` 时定死）**：

```rust
enum SinkMode { Callback, Pull }
```

- `cb != NULL` → `SinkMode::Callback`：由单一分发 task 消费队列。此模式下 `read_frame` 恒返回 `0`，`poll` 恒返回 `0`（二者明确报"当前非拉取模式"，不做静默无操作）。
- `cb == NULL` → `SinkMode::Pull`：无分发 task，帧留在队列由 `read_frame`/`poll` 取。
- **不提供 tee（双出口同时消费）**：两个消费者会互相偷帧，语义不可控。需要两者时，宿主在回调里自行转发。

**各函数语义**：

- `call` **进入时立即 `CStr` 拷贝** `args_json`（函数随即返回，宿主可能立即释放）。查表命中才返回 `0`，否则 `-5 ERR_UNKNOWN_CMD`（不占用事件通道）。
- `call_sync` **不经事件队列**：内部持有该次调用的 `tokio::sync::oneshot` 接收端，`block_on` 等它完成。若走 `command-result` 队列，宿主注册了回调时会被分发 task 抢先消费，`call_sync` 永远等不到（挂死）。因此同步与异步是**两条独立结果通路**，只有异步路径产出事件帧。
- `call_sync` **禁止在回调内调用**（死锁：分发 task 正持锁通知宿主）；回调内调 `call` 安全。此约束写入头文件显著位置。
- `shutdown` 取消所有在途 task、drain 队列、drop runtime、abort 分发 task。
- 回调由单一 task 顺序触发，宿主**不会并发收到回调**（除非宿主自身并行开线程）。
- 所有 `const char*` 入参库只在调用期间借用并拷贝，绝不保留指针；所有返回 `char*` 由库 malloc，宿主必须用 `mctier_core_free_string` 释放；`const char*` 返回值（`version`/`last_error`）指向库内部缓冲，宿主**不得释放**。

### 5.2 read_frame 帧格式

```
[u32 name_len][name bytes][u32 payload_len][payload bytes]   /* 小端 */
```

- 返回 `>0`：实际写入字节数。
- 返回 `0`：队列为空（或当前为 `SinkMode::Callback`，见 §5.1）。
- `cap` 放不下整帧：返回 `-6 ERR_BUFFER_TOO_SMALL` 且**不消费该帧**。宿主随后调 `mctier_core_next_frame_size()` 取所需字节数，扩容后重试。
- `poll(timeout_ms)` 用 `tokio::sync::Notify` 带超时等待，醒来返回待取帧数；宿主再循环 `read_frame` 取完，避免忙轮询。仅在 `SinkMode::Pull` 下有效。

### 5.3 异步调用状态机与 command-result 事件

```json
{"req_id":"a1","ok":true,"result":{...}}
{"req_id":"a1","ok":false,"error":{"code":-104,"kind":"process","message":"进程错误: …"}}
```

事件名沿用 README 既定的 **`command-result`**（与原 MCTier 的 `invoke` 同构，便于宿主迁移既有逻辑）。

**`PendingCall` 状态机**（解决超时/取消后任务仍完成导致的重复与幽灵结果）：

```
            ┌── cancel ──▶ Cancelled
Waiting ────┤
            ├── timeout ─▶ TimedOut
            └── 完成 ────▶ Done
```

- `req_id` 是表的键，值为 `PendingCall { state, tx }`。
- 任务真正完成时，**对 state 做 CAS**：只有从 `Waiting` 成功切到 `Done` 才入队结果；已经是 `Cancelled`/`TimedOut` 的直接丢弃，不入队。
- `call_cancel`/超时同理，CAS 成功才入队 `-8`/`-9` 结果。**同一只 `req_id` 至多产出一条结果**。
- `pending_calls()` 返回表长。

**重复 `req_id` 拒绝**：投递时若表中已存在该 `req_id`，返回 `-13 ERR_DUPLICATE_REQ_ID`，不覆盖既有条目。`req_id` 由宿主生成，库不保证其唯一性，必须显式拒绝以避免两条执行流共用一个状态位。

**`call_cancel` 的语义边界**：它只让该 `req_id` 的**结果**被标记为已取消并入队一条结果，**不中断底层操作**——EasyTier 子进程、网络连接、正在进行的传输不会因此停止。头文件必须注明，避免宿主误以为 cancel 等价于"回滚"。

## 6. 事件队列与背压

- 单一队列，入队前已序列化为 `Frame { name, payload }`，回调出口与 `read_frame` 出口共用同一批帧（只有一条数据路径；出口二选一见 §5.1 的 `SinkMode`）。
- 两类事件进同一队列：**业务事件**（现有 6 个 `emit_*`：`player-joined`、`player-left`、`network-status-change`、`error`、`lobby-update`、`app-state-change`）与**调用结果**（`command-result`）。
- 业务事件默认容量 1024，可由 `set_event_queue_capacity` 或 `init` 的 `options_json` 调整。
- **`command-result` 走独立的高优先队列**，不与业务事件争抢容量。理由：宿主可能正在阻塞等待某个 `req_id` 的结果，结果丢失 = 永久挂起。业务队列挤无可挤时丢最旧帧并累计 `dropped_frames` 计数；`command-result` **绝不丢弃**，其生产者在队列满时于 tokio task 内等待腾位（异步路径本就在 task 中，不会阻塞宿主线程）。
- 两个队列在出口处合并为单一帧序列（`command-result` 优先出队），因此 `read_frame`/回调看到的仍是统一流。

## 7. 错误模型

三条出口，两种载体（`int32` 返回码 + JSON）：

| 场景 | 载体 |
|---|---|
| 同步 C 函数直接失败 | `int32_t` 负数返回码 |
| `call_sync` 执行失败 | 返回的 `char*` JSON |
| `call` 执行失败 | `command-result` 事件 JSON |

**码位分配**（`0` = 成功）：

```
FFI 层   -1  ERR_NOT_INITIALIZED      -2 ERR_ALREADY_INITIALIZED
         -3  ERR_NULL_POINTER         -4 ERR_INVALID_JSON
         -5  ERR_UNKNOWN_CMD          -6 ERR_BUFFER_TOO_SMALL
         -7  ERR_QUEUE_FULL           -8 ERR_CANCELLED
         -9  ERR_TIMEOUT              -10 ERR_REQ_NOT_FOUND
         -11 ERR_INTERNAL             -12 ERR_UNSUPPORTED
         -13 ERR_DUPLICATE_REQ_ID

业务层   -101 ValidationError          -102 NetworkError
         -103 ConfigError              -104 ProcessError
         -105 FileError                -106 IoError
         -107 SerializationError       -108 Unknown
```

- `-7 ERR_QUEUE_FULL` 只在 `set_event_queue_capacity` 请求把容量缩到低于当前待取帧数时返回。正常投递不返回它：业务事件满时丢最旧，`command-result` 走独立队列（见 §6）。
- `-13 ERR_DUPLICATE_REQ_ID` 在 `call`/`call_with_timeout` 发现 `req_id` 已在表中时返回（见 §5.3）。

`error` 对象三个字段：

```json
{"code":-104,"kind":"process","message":"进程错误: EasyTier 启动失败"}
```

- `code` 供程序分支判断。
- `message` 保留现有中文原文，宿主自行本地化。
- `kind` 是稳定的字符串别名，避免宿主硬编码数字。

`mctier_core_last_error()`：每个返回负数的 FFI 函数把详细消息（含上下文）写入**线程局部**缓冲，返回 `const char*`（宿主不得释放，指针仅在同一线程下次 FFI 调用前有效）；`reset_last_error()` 清空。

## 8. 平台差异与能力降级

### 8.1 产物矩阵

| 平台 | 产物 | EasyTier 二进制 | 提权 helper | 防火墙/驱动 |
|---|---|---|---|---|
| Windows x64 | `mctier.dll` | 内嵌 exe + wintun/WinDivert | 有 | 有 |
| Linux x64/arm64 | `libmctier.so` | 内嵌 ELF（现有） | unix_hosts_helper | TUN |
| macOS | `libmctier.dylib` | **宿主注入** | 无 | 无 |
| Android | `libmctier.so` | **宿主注入** | 无 | 需 VpnService/root |

macOS 的 Rust `cdylib` 默认产出 `.dylib`（不是 `.so`）；头文件与构建说明按 `.dylib` 书写，不做强制重命名。

`resource_manager.rs` 当前只有 `#[cfg(windows)]` 与 `#[cfg(not(windows))]` 两套分支，后者会把 Linux ELF 嵌进 macOS/Android。内嵌条件改为 `#[cfg(windows)]` / `#[cfg(target_os = "linux")]`，macOS 与 Android 不内嵌、必须由宿主注入路径，否则相关命令返回 `-12`。

### 8.2 两层能力判定与符号导出

**符号必须在四平台全部导出**：88 个 `mctier_*` 直连函数在任何平台都存在（`#[no_mangle] pub extern "C"` 不加平台 `#[cfg]`）。若某函数在 macOS/Android 上根本不编译，宿主会在**链接期**（静态链接）或 `dlsym` 时失败——这比运行期返回错误严重得多。平台差异在**函数内部**处理：不可用时返回 `-12 ERR_UNSUPPORTED`。

同理，`list_commands_json()` 恒列全 88 条，用 `supported` 字段标记，宿主一次拿到全部可用性，无需逐个试错、也不会因符号缺失崩溃。

`has_capability(cmd)` 综合两层：

1. **静态门**（编译期 `#[cfg]` 汇总到位掩码）：命令在该平台不可用 → 返回 0，调用返回 `-12`。
2. **动态探测**（运行时）：命令可用但环境不满足 → 调用时返回 `-12`，`last_error()` 说明原因（如 TUN 权限不足、未注入 EasyTier 路径）。

实现为 `ffi/capabilities.rs` 的静态表 `cmd → 平台要求位掩码 + 动态检查函数`。

### 8.3 Android 边界

建虚拟网卡超出本库职责（需宿主 `VpnService` 或 root），库只负责 EasyTier 进程编排与组网信令；`has_capability` 如实反映当前 TUN 可用性。此边界写入 README/头文件注释。

### 8.4 init 的 options_json

```json
{"easytier_core":"/path/…","easytier_cli":"/path/…",
 "runtime_dir":"/path/…","data_dir":"/path/…",
 "log_level":"info","event_queue_capacity":1024,"worker_threads":4}
```

- `easytier_core` / `easytier_cli`：宿主注入的二进制路径（D6）。缺省时用内嵌二进制（仅 Windows/Linux 内嵌；macOS/Android 缺省即为不可用，相关命令返回 `-12`）。
- `runtime_dir`：**替代 `app_handle.path().resource_dir()`**（见 §3.2）。库把 EasyTier 二进制、wintun.dll、WinDivert64.sys 提取到此目录并从这里启动子进程。未指定时退化为 `data_dir` 下的 `runtime/` 子目录。
- `data_dir`：配置、日志、共享索引等可写数据的根目录。

缺省值全部来自库内推导（临时目录/当前平台约定路径），不要求 tauri。

## 9. 测试策略

现有基线必须保持：Rust 191/191、JS 14/14、`cargo check` 零警告。去 tauri 是机械重构，**业务测试原样全绿**是重构未改坏行为的主证据。

新增四层：

1. **dispatch 完整性**：`const COMMANDS` 表驱动 `list_commands_json()`，测试断言表内 88 条与各模块 match 分支一一对应，漏写分支即测试失败。
2. **FFI 契约测试**（Rust 直接调 `extern "C"` 函数）：
   - `init`/`shutdown` 幂等；**`shutdown` 后再次 `init` 成功**（`Mutex<Option>` 可重入）。
   - 未 `init` 调 `call` → `-1`；重复 `init` → `-2`；重复 `req_id` → `-13`。
   - 未知命令 → `-5`；`args_json` 非法 → `-4`。
   - `call` → 收到 `command-result` 且 `req_id` 匹配。
   - **超时/取消后任务完成，不产生第二条结果**（PendingCall CAS，断言同 `req_id` 至多一条帧）。
   - `call_sync` 结果不经队列：注册回调后调 `call_sync`，断言回调未消费其结果、`call_sync` 正常返回。
   - `read_frame` 帧格式逐字节比对；`cap` 不足返回 `-6` 且队列不消耗，`next_frame_size()` 返回正确大小。
   - **`SinkMode::Callback` 下 `read_frame`/`poll` 返回 0**；`SinkMode::Pull` 下正常取帧。
   - 背压：业务队列塞满后投递 `command-result`，断言未被丢弃（独立队列）；`dropped_frames` 计数递增。
   - `last_error()` 在返回负数后可取到消息，`reset_last_error()` 后为空。
   - **并发 shutdown**：多个线程同时调 `shutdown` 不 panic、不重复释放。
3. **C 端冒烟**（`examples/c/demo.c`）：`init → call → poll → read_frame → shutdown` 全流程。有 C 编译器就跑，没有则跳过不阻塞。
4. **四平台编译与符号验证**：
   - `cargo check --target` 分别验 `x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`、`aarch64-apple-darwin`、`aarch64-linux-android`；工具链不全时对 `cfg` 采用保守写法，不引用未经验证的 API。
   - **符号存在性断言**：对产物跑 `nm -D`/`dumpbin /EXPORTS`，确认 88 个 `mctier_*` 加 6 个核心加 8 个查询加 5 个控制加 4 个日志配置（合计 111 个）在该平台全部导出。

## 10. 交付物

- 四平台动态库产物（`mctier.dll`、`libmctier.so`、`libmctier.dylib`）。
- `include/mctier_core.h`：**111 个函数**（6 核心 + 8 查询 + 5 控制 + 88 直连 + 4 日志配置）的声明、`MctierEventCb`/`MctierLogCb` 类型、返回码常量宏、逐参数契约注释。
- `examples/c/demo.c`：可编译运行的最小示例。
- README 同步：阶段状态、核心能力表（删除已移除的实时语音行）、FFI 段（`command-result`、`SinkMode`、无 `OnceLock` 陷阱）、平台徽章含 macOS、DEMO 定位声明。

## 11. 外部评审采纳记录

十条契约缺陷全部采纳，逐条落点：

| # | 问题 | 采纳位置 |
|---|---|---|
| 1 | `OnceLock` 与 `shutdown` 后无法重入矛盾 | §5.1 改 `Mutex<Option<...>>`，明确允许 re-init |
| 2 | 超时/取消后任务完成会重复投递结果 | §5.3 `PendingCall` 状态机 + CAS |
| 3 | `call_sync` 走事件队列会被回调抢走 → 挂死 | §5.1 改独立 `oneshot`，与异步双通路 |
| 4 | 回调与拉取的互斥语义未定义 | §5.1 `SinkMode::Callback`/`Pull`，`init` 定死，不做 tee |
| 5 | 平台裁剪导致符号缺失 → 链接失败 | §8.2 符号四平台全导出，内部返回 `-12` |
| 6 | `last_error` 的 `char*` 与 TLS 释放规则冲突 | §4.2 改 `const char*`，宿主不得释放 |
| 7 | `read_frame` 缓冲不足时不知所需大小 | §4.2 新增 `next_frame_size()` |
| 8 | macOS 产物应为 `.dylib` | §8.1 修正 |
| 9 | 复杂参数无法用标量穷尽 | §4.4 映射表 + 复杂参数退化 `args_json`，头文件逐个标注 |
| 10 | 重复 `req_id` 会覆盖状态 | §5.3 显式拒绝，新增 `-13` |

次要建议采纳：函数总数修正为 111（§10）、`command-result` 独立队列不丢弃（§6）、`call_cancel` 不中断底层操作（§5.3）、日志回调多线程并发（§4.5）、`AppHandle` 非 emit 用途盘点与 `runtime_dir` 注入（§3.2/§8.4）、`resource_manager` 改 `target_os = "linux"`（§8.1）、测试补充状态机与符号断言（§9）。

事件名采用 README 既定的 `command-result`（原评审稿与初稿写的 `call-result` 已统一）。
