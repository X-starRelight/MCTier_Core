# MCTier_Core C ABI FFI 层设计（阶段 4）

日期：2026-10-06
状态：设计已确认，待实现

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
| D8 | C ABI 面 | 五组约 100 个函数（核心 / 查询自描述 / 调用控制 / 强类型直连 / 日志配置） |
| D9 | 强类型直连形态 | **参数用强类型，返回统一 JSON 字符串**（不做 C 结构体返回值） |

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

- 注册了 cb：由**单一分发 task** 顺序调用 cb（宿主不会并发收到回调）。
- cb 为 NULL：留在队列，等 `read_frame` 批量取走。

调用流：`call` 拷贝 `args_json` → 查 dispatch 表 → 投递到 tokio task → 完成后入队 `call-result`。`call_sync` 是在宿主线程 `block_on` 同一条路径的阻塞封装。

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

### 4.2 查询与自描述（7 个）

```c
const char* mctier_core_version(void);              /* 静态串，无需 free */
int32_t     mctier_core_is_initialized(void);
char*       mctier_core_list_commands_json(void);   /* 88 个命令名、参数签名、supported */
int32_t     mctier_core_has_capability(const char* cmd);
int32_t     mctier_core_event_count(void);
char*       mctier_core_last_error(void);           /* 线程局部最近一次错误详情 */
int32_t     mctier_core_pending_calls(void);
```

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

**命名规则**：`mctier_` + 命令名原样（已实测 88 个命令名无重复，无需后缀消歧）。参数顺序与 Rust 函数一致，`Option<T>` 映射为「指针可为 `NULL`」（`Option<String>`）或 `-1` 表示未设置（`Option<u16>`/`Option<bool>` 用 `int32_t`，`-1` = None）。`AppHandle`/`State` 这类注入参数在去 tauri 后不出现在 C 签名里。

每导出一个直连函数，就同步在 `COMMANDS` 表里登记其 C 名，使 `list_commands_json()` 能同时给出 `cmd`（走 `call` 用）与 `c_symbol`（直连用）两种调用方式。

参数复杂度实测：0 参数 7 个、1 参数 47 个、2 参数 18 个、3 参数 3 个，`join_lobby` 最多（去 tauri 后 8 个业务参数）。

### 4.5 日志与配置（4 个）

```c
void    mctier_core_set_logger(MctierLogCb cb, void* user_data, int32_t level);
int32_t mctier_core_set_option(const char* key, const char* value);
char*   mctier_core_get_option(const char* key);
void    mctier_core_flush_logs(void);
```

## 5. 函数契约细节

### 5.1 生命周期与线程

- `init` 创建 multi-thread tokio runtime 存入全局 `OnceLock`；`cb` 可为 `NULL`。重复调用返回 `-2 ERR_ALREADY_INITIALIZED`。
- `call` **进入时立即 `CStr` 拷贝** `args_json`（函数随即返回，宿主可能立即释放）。查表命中才返回 `0`，否则 `-5 ERR_UNKNOWN_CMD`（不占用事件通道）。
- `call_sync` 阻塞宿主线程到完成，返回 malloc 的 JSON。**禁止在回调内调用**（死锁：库正持有分发锁通知宿主）；回调内调 `call` 安全。
- `shutdown` 取消所有 task、drain 队列、drop runtime、abort 分发 task，幂等。
- 回调由单一 task 顺序触发，宿主**不会并发收到回调**（除非宿主自身并行）。
- 所有 `const char*` 入参库只在调用期间借用并拷贝，绝不保留指针；所有返回 `char*` 由库 malloc，宿主必须用 `mctier_core_free_string` 释放。

### 5.2 read_frame 帧格式

```
[u32 name_len][name bytes][u32 payload_len][payload bytes]   /* 小端 */
```

- 返回 `>0`：实际写入字节数。
- 返回 `0`：队列为空。
- `cap` 放不下整帧：返回 `-6 ERR_BUFFER_TOO_SMALL` 且**不消费该帧**（宿主扩容后重试）。
- `poll(timeout_ms)` 用 `tokio::sync::Notify` 带超时等待，醒来返回待取帧数；宿主再循环 `read_frame` 取完，避免忙轮询。

### 5.3 call-result 事件

```json
{"req_id":"a1","ok":true,"result":{...}}
{"req_id":"a1","ok":false,"error":{"code":-104,"kind":"process","message":"进程错误: …"}}
```

`call` 投递时把 `req_id` 记入 `HashMap<req_id, ()>`；执行完成后入队结果并移除。

- `call_cancel`：从表中取走，入队 `{"ok":false,"error":{"code":-8,"kind":"cancelled"}}`。
- `call_with_timeout` 超时：入队 `{"code":-9,"kind":"timeout"}`。
- `pending_calls()` 返回表长。

## 6. 事件队列与背压

- 单一队列，入队前已序列化为 `Frame { name, payload }`，回调出口与 `read_frame` 出口共用同一批帧（只有一条数据路径）。
- 两类事件进同一队列：**业务事件**（现有 6 个 `emit_*`：`player-joined`、`player-left`、`network-status-change`、`error`、`lobby-update`、`app-state-change`）与**调用结果**（`call-result`）。
- 默认容量 1024，可由 `set_event_queue_capacity` 或 `init` 的 `options_json` 调整。
- **`call-result` 有优先权**：队列满时挤掉最旧的业务事件腾位，避免宿主等待的结果被丢弃导致永久挂起。
- 业务事件挤无可挤时丢最旧，累计 `dropped_frames` 计数并对外暴露，宿主可自查是否丢过帧。

## 7. 错误模型

三条出口，两种载体（`int32` 返回码 + JSON）：

| 场景 | 载体 |
|---|---|
| 同步 C 函数直接失败 | `int32_t` 负数返回码 |
| `call_sync` 执行失败 | 返回的 `char*` JSON |
| `call` 执行失败 | `call-result` 事件 JSON |

**码位分配**（`0` = 成功）：

```
FFI 层   -1  ERR_NOT_INITIALIZED      -2 ERR_ALREADY_INITIALIZED
         -3  ERR_NULL_POINTER         -4 ERR_INVALID_JSON
         -5  ERR_UNKNOWN_CMD          -6 ERR_BUFFER_TOO_SMALL
         -7  ERR_QUEUE_FULL           -8 ERR_CANCELLED
         -9  ERR_TIMEOUT              -10 ERR_REQ_NOT_FOUND
         -11 ERR_INTERNAL             -12 ERR_UNSUPPORTED

业务层   -101 ValidationError          -102 NetworkError
         -103 ConfigError              -104 ProcessError
         -105 FileError                -106 IoError
         -107 SerializationError       -108 Unknown
```

`-7 ERR_QUEUE_FULL` 只在 `set_event_queue_capacity` 请求把容量缩到低于当前待取帧数时返回。正常投递路径不返回它：业务事件满时丢最旧，`call-result` 满时挤业务事件（见 §6）。

`error` 对象三个字段：

```json
{"code":-104,"kind":"process","message":"进程错误: EasyTier 启动失败"}
```

- `code` 供程序分支判断。
- `message` 保留现有中文原文，宿主自行本地化。
- `kind` 是稳定的字符串别名，避免宿主硬编码数字。

`mctier_core_last_error()`：每个返回负数的 FFI 函数把详细消息（含上下文）写入**线程局部**存储，宿主可紧接着取；`reset_last_error()` 清空。

## 8. 平台差异与能力降级

### 8.1 产物矩阵

| 平台 | 产物 | EasyTier 二进制 | 提权 helper | 防火墙/驱动 |
|---|---|---|---|---|
| Windows x64 | `mctier.dll` | 内嵌 exe + wintun/WinDivert | 有 | 有 |
| Linux x64/arm64 | `libmctier.so` | 内嵌 ELF（现有） | unix_hosts_helper | TUN |
| macOS | `libmctier.so` | **宿主注入** | 无 | 无 |
| Android | `libmctier.so` | **宿主注入** | 无 | 需 VpnService/root |

`resource_manager.rs` 当前只有 `#[cfg(windows)]` 与 `#[cfg(not(windows))]` 两套分支，后者会把 Linux ELF 嵌进 macOS/Android。重构后 `#[cfg(not(windows))]` 的内嵌仅在 Linux 生效，macOS/Android 必须由宿主注入路径，否则相关命令返回 `-12`。

### 8.2 两层能力判定

`has_capability(cmd)` 综合：

1. **静态门**（编译期 `#[cfg]`）：命令在该平台未编译进去 → 返回 0，调用返回 `-12`。
2. **动态探测**（运行时）：命令已编译但环境不满足 → 调用时返回 `-12`，`last_error()` 说明原因（如 TUN 权限不足、未注入 EasyTier 路径）。

实现为 `ffi/capabilities.rs` 的静态表 `cmd → 平台要求位掩码 + 动态检查函数`。`list_commands_json()` 为每个命令附 `supported` 字段，宿主一次拿到全部可用性，无需逐个试错。

### 8.3 Android 边界

建虚拟网卡超出本库职责（需宿主 `VpnService` 或 root），库只负责 EasyTier 进程编排与组网信令；`has_capability` 如实反映当前 TUN 可用性。此边界写入 README/头文件注释。

### 8.4 init 的 options_json

```json
{"easytier_core":"/path/…","easytier_cli":"/path/…","data_dir":"…",
 "log_level":"info","event_queue_capacity":1024,"worker_threads":4}
```

缺省时使用内嵌二进制（Windows/Linux）与默认路径。

## 9. 测试策略

现有基线必须保持：Rust 191/191、JS 14/14、`cargo check` 零警告。去 tauri 是机械重构，**业务测试原样全绿**是重构未改坏行为的主证据。

新增四层：

1. **dispatch 完整性**：`const COMMANDS` 表驱动 `list_commands_json()`，测试断言表内 88 条与各模块 match 分支一一对应，漏写分支即测试失败。
2. **FFI 契约测试**（Rust 直接调 `extern "C"` 函数）：
   - `init`/`shutdown` 幂等；未 `init` 调 `call` → `-1`。
   - 未知命令 → `-5`；`args_json` 非法 → `-4`。
   - `call` → 收到 `call-result` 且 `req_id` 匹配。
   - `read_frame` 帧格式逐字节比对；`cap` 不足返回 `-6` 且队列不消耗。
   - 背压：塞满队列后投递 `call-result`，断言未被挤掉。
   - `last_error()` 在返回负数后可取到消息，`reset_last_error()` 后为空。
3. **C 端冒烟**（`examples/c/demo.c`）：`init → call → poll → read_frame → shutdown` 全流程。有 C 编译器就跑，没有则跳过不阻塞。
4. **四平台编译验证**：`cargo check --target` 分别验 `x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`、`aarch64-apple-darwin`、`aarch64-linux-android`；工具链不全时对 `cfg` 采用保守写法，不引用未经验证的 API。

## 10. 交付物

- 四平台动态库产物（`mctier.dll` / `libmctier.so`）。
- `include/mctier_core.h`：全部约 100 个函数的声明、`MctierEventCb`/`MctierLogCb` 类型、返回码常量宏、契约注释。
- `examples/c/demo.c`：可编译运行的最小示例。
- README 阶段 4 状态更新为完成，附四平台构建与调用说明。
