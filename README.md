<div align="center">

# MCTier_Core

**MCTier 的非官方核心（动态库）**

<p>
  <img src="https://img.shields.io/badge/version-0.1.0--dev-blue?style=flat-square" alt="Version">
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-brightgreen?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/FFI-C%20ABI-orange?style=flat-square" alt="FFI">
  <img src="https://img.shields.io/badge/license-Custom%20Source--Available-orange?style=flat-square" alt="License">
</p>

[English](./README_EN.md) | 简体中文

</div>

> ## ⚠️ 非官方声明
>
> **MCTier_Core 是一个独立的非官方二次开发项目**，由 MCTier 的核心联机部分分离重构而成。
> 本项目与 MCTier 原作者及其团队之间**不存在**隶属、代理、赞助或背书关系，亦未获其官方授权背书。
> 如需 MCTier 官方客户端，请访问 [原项目](https://github.com/pmh1314520/MCTier)。
>
> MCTier_Core is an **independent, unofficial derivative project**. It is not affiliated with,
> endorsed by, or sponsored by the original author of MCTier.

---

## 项目简介

MCTier_Core 把 [MCTier](https://github.com/pmh1314520/MCTier)（基线版本 3.8.0）的**联机核心能力**
从 GUI 桌面应用中整体剥离出来，封装为一个**跨平台动态库**，供其他项目通过 **C ABI** 直接调用。

原 MCTier 是一个 Tauri + React 的桌面程序；MCTier_Core 只保留其中与「组网、联机、协作」相关的
Rust 后端能力，去掉窗口、托盘、热键、WebView 等全部界面层。你可以在自己的应用里实现 UI，
而把联机逻辑交给这个库：

- **游戏联机工具**：封装 Minecraft、泰拉瑞亚等局域网游戏的跨网络联机能力。
- **自定义客户端**：用任意语言（C / C++ / C# / Python / Go / Node）实现自己的界面，复用核心逻辑。
- **服务端 / 无头部署**：在没有图形环境的机器上运行组网与大厅服务。

### 核心能力覆盖

| 领域 | 能力 |
| --- | --- |
| 虚拟组网 | EasyTier 组网启停、虚拟 IP 分配、节点选择、KCP/QUIC 高级参数、出口节点配置 |
| 大厅 | 创建 / 加入 / 离开大厅、玩家列表、房主管理（公告、人数、踢人）、公开大厅广场、邀请信息 |
| 信令与打洞 | WebSocket 信令、P2P 打洞、成员状态与心跳、断线重连自愈、私有化信令地址 |
| 聊天 | P2P 文字 / 图片消息、P-256 签名鉴权、消息漫游与清理 |
| 文件协作 | 本地共享目录管理、远程文件浏览与下载、共享密码校验、传输进度事件 |
| 游戏增强 | Minecraft 局域网世界发现、MC LAN 广播、服务器扫描与延迟测量 |
| 诊断与运维 | 网络诊断（虚拟网卡 / 防火墙 / UDP 端口）、防火墙一键放行、连接质量评分、日志读写 |
| 配置 | 用户配置读写、导入导出、EasyTier 全局与单大厅高级配置、凭据安全存储 |

所有面向界面的纯 GUI 能力（窗口透明度、托盘、全局热键、弹幕窗、GameHUD、文件选择对话框等）
**不属于本项目范围**，由调用方自行实现。

> ## ⚠️ 项目定位：DEMO
>
> **本项目是 DEMO，不是正式版产品，不承诺完全可用。**
> 它不保证在生产环境长期稳定运行，不提供 ABI 版本兼容承诺，也不建立 CI。
> 工程实现本身仍按生产级标准执行（契约完整、四平台产物齐全、错误与并发语义明确），
> 但「DEMO」限定的是**交付承诺**，不是**质量标准**。
>
> 以下能力已从本项目移除：**实时语音**（语音频道 / 语音小队 / 麦克风 / ICE 服务 / 语音转写）、
> **屏幕共享**、**夸克网盘助力**、**内置 Emoji 资源包**。
> 聊天中的文字、图片、Emoji 与**语音条**消息仍保留（属消息存储，非实时流）。

## FFI 接口设计

```c
typedef void (*MctierEventCb)(const char* event, const char* payload_json, void* user_data);

int32_t mctier_core_init(const char* options_json, MctierEventCb cb, void* user_data);
int32_t mctier_core_call(const char* req_id, const char* cmd, const char* args_json);
char*   mctier_core_call_sync(const char* cmd, const char* args_json);
int32_t mctier_core_read_frame(uint8_t* buf, size_t cap);
void    mctier_core_free_string(char* s);
void    mctier_core_shutdown(void);
```

- `call` 为异步调用，结果以 `command-result` 事件携带 `req_id` 回吐，事件模型与原 MCTier 的
  `invoke` 保持同构，便于迁移既有逻辑。
- **出口模式在 `init` 时二选一**：传回调 → 库内部单线程顺序分发（`read_frame`/`poll` 恒返回 0）；
  回调传 `NULL` → 纯拉取模式，由 `read_frame`/`poll` 取帧。不提供双出口同时消费。
- 事件回调发生在库内部的异步工作线程，调用方需自行 marshal 到自己的线程模型；
  **不要在回调内调用 `call_sync`**（会死锁）。`call_sync` 走独立的同步结果通路，不经事件队列。
- `shutdown` 后允许重新 `init`；同一 `req_id` 重复投递会被拒绝。
- 以上 6 个为核心闭环，另有查询自描述（`list_commands_json`、`has_capability` 等 8 个）、
  调用控制（`call_cancel`、`poll` 等 5 个）、**88 个强类型命令直连**、日志与配置（4 个），
  合计 **111 个导出函数**。完整契约见
  [设计文档](docs/superpowers/specs/2026-10-06-c-abi-ffi-design.md)。

## 构建

### 前置条件

- Rust（stable）；Windows 需 MSVC 工具链；Android 需 `cargo ndk`
- Node.js 仅用于运行 JS 测试（`node --test tests/*.test.mjs`），编译期不再需要
- Linux 构建仍需 WebKitGTK / GTK 系统库（`Cargo.toml` 尚未移除 `tauri` 依赖，属待办工作）

### 第一步：准备第三方二进制（首次 clone 后必做）

`src/src/modules/resource_manager.rs` 通过 `include_bytes!` 在编译期内嵌 4 个第三方二进制。
这些文件体积较大且受各自许可约束，不纳入本仓库，clone 之后必须先准备：

```powershell
# 1) 下载可直接再分发的驱动类文件（wintun.dll / WinDivert64.sys）
.\scripts\fetch-binaries.ps1

# 2) 重建不依赖 Npcap 的 easytier-core.exe / easytier-cli.exe
.\scripts\build-easytier-npcap-free.ps1
```

第二个脚本会克隆 EasyTier v2.5.0、应用
[patches/pnet_datalink-0.35.0-no-npcap.patch](patches/pnet_datalink-0.35.0-no-npcap.patch)
去掉对 Npcap `packet.dll` 的静态导入。详见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 第 8 节。

### 第二步：构建

```bash
cd src
cargo build --release
```

当前产物为 `[lib] crate-type` 中的 `staticlib` / `cdylib` / `rlib`
（crate 名暂为 `mctier_lib`，后续随项目改名一并调整）。
对外交付形态为 C ABI 动态库 `mctier.dll`（Windows）、`libmctier.so`（Linux / Android）、
`libmctier.dylib`（macOS）及配套头文件 `mctier_core.h`。
其中 Windows 与 Linux 内嵌 EasyTier 二进制，macOS 与 Android 由宿主通过
`init` 的 `options_json` 注入路径。

---

## 许可协议

MCTier_Core **自有代码**沿用 MCTier 的自定义**源码可得（source-available）非商业**许可
（详见 [LICENSE](LICENSE)）：

- 仅供个人学习与非商业使用。
- 允许二次开发，但必须保留原作者信息。
- **衍生项目需要按相同协议公开源代码**（原协议第 4 条，本项目即依此条款授权）。

> 用词说明：由于本许可禁止商业用途，它**不符合** OSI 对「开源许可证」的定义，
> 因此本项目对自有代码统一表述为「源码可得 / 非商业许可」，而非「开源许可证」。
> 项目源代码仍然完整公开。项目中包含的第三方组件多为真正的开源许可证，
> 其权利不受本许可限制（见下文）。

### 归属声明

```
基于 MCTier 项目开发
原作者：MCTier Team
原项目地址：https://github.com/pmh1314520/MCTier
```

- MCTier 原始代码 Copyright (c) 2024-2026 青云制作_彭明航（MCTier Team）
- MCTier_Core 新增与修改代码 Copyright (c) 2026 X-starRelight

### 第三方组件与许可证边界

**上述限制仅适用于本项目自有代码，不适用于项目中包含的第三方组件。**

本软件使用 [EasyTier](https://github.com/EasyTier/EasyTier) 提供虚拟组网能力。
EasyTier 及对其所作的任何修改，继续按 **LGPL-3.0** 授权：

```
本软件使用 EasyTier 项目。
EasyTier Copyright (c) EasyTier contributors.
EasyTier is licensed under the GNU Lesser General Public License version 3.0 (LGPL-3.0).
Source: https://github.com/EasyTier/EasyTier
```

- 「禁止商业用途」**不适用于** EasyTier 的 LGPL-3.0 部分；
- 「二次开发必须以相同协议开源」**不适用于** EasyTier 的 LGPL-3.0 部分；
- 本项目所用自定义协议不得被解释为限制 LGPL-3.0 赋予使用者的任何权利，
  使用者可自行替换 EasyTier 动态库版本。

相关文件：

- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) — 版本、commit、修改状态、源码获取方式、二进制 SHA-256
- [LICENSE-LGPL-3.0.txt](LICENSE-LGPL-3.0.txt) — LGPL-3.0 全文
- [LICENSE-GPL-3.0.txt](LICENSE-GPL-3.0.txt) — GPL-3.0 全文（LGPL-3.0 以引用方式并入）
- [licenses/](licenses/) — 各第三方许可证全文（LGPL-3.0、GPL-3.0、GPL-2.0、Apache-2.0、MIT、BSD-3-Clause、Wintun）

### 商标与非官方声明

本项目不是官方 Minecraft 产品，未获 Mojang Studios 或 Microsoft 批准、认可、关联或背书。
Minecraft 是 Mojang Synergies AB 及其关联主体的商标。

本项目与 MCTier 项目及其原作者无隶属关系，MCTier 名称仅用于说明代码来源。
WireGuard LLC、WireGuard 项目与 Wintun 项目均未对本项目作任何背书。

## 默认服务与元数据披露

MCTier_Core 继承原 MCTier 的默认服务配置。**建立连接**以及**版本检查**会访问以下服务，
若使用默认配置，这些服务端会看到相应的连接元数据：

| 默认服务 | 地址 | 服务端可见的元数据 | 用途 |
|---|---|---|---|
| 信令服务器 | `wss://mctier.pmhs.top/signaling` | 公网 IP、连接时间、大厅名称与密码哈希用于匹配、玩家名、虚拟 IP/虚拟域名、成员数、客户端版本 | 交换 WebRTC 信令、发现同一大厅的成员 |
| EasyTier 公共节点 | 默认 `tcp://easytier.weiai.org.cn:11010`，另可选 `tcp://225284.xyz:11010`、`udp://us01.225284.xyz:11010` | 公网 IP、连接时间、EasyTier 网络标识 | P2P 打洞与必要时的流量中继 |
| 版本检查 | `https://gitee.com/api/v5/repos/peng-minghang/mctier/tags` | 公网 IP、请求时间（由 Gitee 记录） | 获取最新版本号 |

说明：

- **元数据 ≠ 通信内容**。信令服务器与 EasyTier 节点不解密、不存储聊天、语音、文件内容。
- **可以完全不使用官方服务**：通过配置填入自建信令服务器与自建 EasyTier 节点即可，
  此时不会有任何数据经过官方服务。
- 作为动态库，本项目把「使用哪个信令服务器 / 节点」完全交给调用方配置。

## 免责声明

- 本项目继承自一款**中立的虚拟局域网组网与协作工具**，仅供在符合所在地法律法规的前提下
  用于个人合法用途（如局域网游戏联机、协作、访问你本人或已获授权的服务）。
- 通信内容（聊天、语音、文件、屏幕等）在成员设备之间**点对点直接传输**，
  开发者不参与、不控制，也无法审查任何用户内容或其具体使用行为。
- **使用者须对自己的全部使用行为及传输内容独立承担法律责任。** 严禁利用本项目从事任何
  违反法律法规的活动，包括但不限于：未经许可的经营性/跨境组网、传播违法违规及侵权信息、
  未经授权控制或监控他人设备、利用语音/变声进行诈骗或冒充他人等。
- 本软件按「现状」提供，不作任何明示或默示担保；在法律允许的最大范围内，
  开发者不对使用本软件造成的任何直接或间接损失负责。
- 如你不同意上述任何内容，请勿使用本项目。

## 致谢与作者

**原项目作者（MCTier）**：青云制作_彭明航（MCTier Team）

- GitHub: <https://github.com/pmh1314520/MCTier>
- Gitee: <https://gitee.com/peng-minghang/mctier>

**MCTier_Core 维护者**：X-starRelight

---

<div align="center">

MCTier_Core 依 MCTier 相同协议提供，仅供个人非商业使用。

</div>
