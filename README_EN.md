<div align="center">

# MCTier_Core

**The Unofficial Core of MCTier (Dynamic Library)**

<p>
  <img src="https://img.shields.io/badge/version-0.1.0--dev-blue?style=flat-square" alt="Version">
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20Android-brightgreen?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/FFI-C%20ABI-orange?style=flat-square" alt="FFI">
  <img src="https://img.shields.io/badge/license-Custom%20Source--Available-orange?style=flat-square" alt="License">
</div>

[简体中文](./README.md) | English

</div>

> ## ⚠️ Non-Affiliation Notice
>
> **MCTier_Core is an independent, unofficial derivative project**, extracted and refactored from
> the networking core of MCTier. It is **not** affiliated with, endorsed by, or sponsored by the
> original author of MCTier, nor is it officially authorized by them. For the official MCTier
> client, please visit the [original project](https://github.com/pmh1314520/MCTier).
>
> MCTier_Core 是一个**独立的非官方二次开发项目**，与 MCTier 原作者及其团队无隶属或背书关系。

---

## Introduction

MCTier_Core extracts the **core networking capabilities** of [MCTier](https://github.com/pmh1314520/MCTier)
(baseline version 3.8.0) out of its GUI desktop application and packages them into a **cross-platform
dynamic library** that other projects can call directly through the **C ABI**.

The original MCTier is a Tauri + React desktop program. MCTier_Core keeps only the Rust backend
logic related to *networking, sessions and collaboration*, dropping the entire presentation layer
(window, tray, hotkeys, WebView, …). You implement your own UI and let this library handle the
networking:

- **Game session tooling**: wrap cross-network LAN play for Minecraft, Terraria, and similar games.
- **Custom clients**: build your own interface in any language (C / C++ / C# / Python / Go / Node).
- **Headless deployment**: run lobbies and virtual networking on machines without a display.

### Capabilities

| Area | Capabilities |
| --- | --- |
| Virtual networking | EasyTier start/stop, virtual IP allocation, node selection, KCP/QUIC advanced options, exit nodes |
| Lobby | create / join / leave, player list, host management (announcements, capacity, kick), public lobby plaza, invitations |
| Signaling & NAT traversal | WebSocket signaling, P2P hole punching, member status & heartbeat, reconnect self-healing, private signaling endpoints |
| Voice | voice channels, voice squads, microphone toggle & mute, ICE discovery, voice message transcription |
| Chat | P2P text / image / emoji messages, P-256 signature authentication, message history & cleanup |
| File collaboration | local share management, remote file browsing & download, share password verification, transfer progress events |
| Game enhancements | Minecraft LAN world discovery, MC LAN broadcast, server scanning & latency measurement |
| Diagnostics | network diagnostics (virtual adapter / firewall / UDP port), one-click firewall rule, connection quality scoring, log access |
| Configuration | user config read/write, import/export, global and per-lobby EasyTier advanced config, secure credential storage |

Pure GUI capabilities (window opacity, tray, global hotkeys, danmaku overlay, GameHUD, file-open
dialogs, …) are **out of scope** and left to the caller.

## Project Status

The migration proceeds in phases. We are currently at **Phase 2 (decoupling Tauri)**:

| Phase | Content | Status |
| --- | --- | --- |
| 0 | LICENSE derivative notice, source file attribution headers, README / SECURITY rewrite | ✅ done |
| 1 | Remove GUI (React frontend, Tauri shell, pure-GUI commands & modules) | ✅ done |
| 2 | Decouple Tauri (event bus, path injection, remove `AppHandle`) | ⏳ in progress |
| 3 | Refactor command layer into a unified dispatch table | ⏳ |
| 4 | C ABI FFI layer, header file, C example | ⏳ |
| 5 | Build system & renaming (including Android cross-compilation) | ⏳ |
| 6 | Three-platform verification and test cleanup | ⏳ |

> After Phase 1 the frontend, the Tauri shell and pure-GUI commands are gone and `src-tauri/`
> has been renamed to `src/`. The `tauri` dependency is still in `Cargo.toml` (that is Phase 2
> work), so `cargo build` on Linux still requires the WebKitGTK / GTK system libraries first.

## FFI Design (to be implemented in Phase 4)

```c
typedef void (*MctierEventCb)(const char* event, const char* payload_json, void* user_data);

int32_t mctier_core_init(const char* options_json, MctierEventCb cb, void* user_data);
int32_t mctier_core_call(const char* req_id, const char* cmd, const char* args_json);
char*   mctier_core_call_sync(const char* cmd, const char* args_json);
int32_t mctier_core_read_frame(uint8_t* buf, size_t cap);
void    mctier_core_free_string(char* s);
void    mctier_core_shutdown(void);
```

- `call` is asynchronous; the result is delivered back as a `command-result` event carrying
  `req_id`, mirroring the original MCTier `invoke` model for easy migration.
- Event callbacks arrive on the library's internal worker thread; marshal them to your own thread
  model. **Never call `call_sync` from inside a callback** (deadlock).
- High-frequency binary data (microphone / screen frames) goes through the dedicated out-buffer
  `read_frame` API instead of JSON.

## Building

### Prerequisites

- Rust (stable), Node.js (only for build-time asset preparation scripts)
- Platform specifics: MSVC toolchain on Windows; WebKitGTK dependencies on Linux (no longer
  needed once Phase 2 drops the `tauri` dependency); `cargo ndk` for Android

### Step 1: prepare third-party binaries (required after the first clone)

`src/src/modules/resource_manager.rs` embeds four third-party binaries at compile time via
`include_bytes!`. They are large and covered by their own licenses, so they are not part of this
repository — prepare them first:

```powershell
# 1) Download redistributable driver files (wintun.dll / WinDivert64.sys)
.\scripts\fetch-binaries.ps1

# 2) Rebuild easytier-core.exe / easytier-cli.exe without Npcap
.\scripts\build-easytier-npcap-free.ps1
```

The second script clones EasyTier v2.5.0 and applies
[patches/pnet_datalink-0.35.0-no-npcap.patch](patches/pnet_datalink-0.35.0-no-npcap.patch) to drop
the static link against Npcap's `packet.dll`. See section 8 of
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

### Step 2: build

```bash
cd src
cargo build --release
```

The current output is the `staticlib` / `cdylib` / `rlib` declared under `[lib] crate-type`
(the crate is still named `mctier_lib`; Phase 5 renames it along with the rest of the project).
From Phase 4 on, the delivered artifact is a C ABI dynamic library —
`libmctier_core.so` / `mctier_core.dll` (same name on Android) — plus its header file.

---

## License

MCTier_Core's **own code** continues under MCTier's custom **source-available non-commercial**
license (see [LICENSE](LICENSE)):

- Personal, non-commercial use only.
- Secondary development is allowed, but attribution to the original author is mandatory.
- **Derivative works must be open-sourced under the identical license** (Section 4 of the original
  license — this project is authorized under exactly that clause).

> Wording note: because this license prohibits commercial use it does **not** meet the OSI
> definition of an "open source license". The project's own code is therefore described as
> *source-available / non-commercial*, not as "open source". The source remains fully public.
> Third-party components bundled with the project are mostly under genuine open source licenses
> and are not restricted by this license (see below).

### Attribution

```
Based on MCTier Project
Original Author: MCTier Team
Original Project: https://github.com/pmh1314520/MCTier
```

- Original MCTier code Copyright (c) 2024-2026 QingYun Studio_PengMingHang (MCTier Team)
- MCTier_Core modifications and new code Copyright (c) 2026 X-starRelight

### Third-party components and license boundary

**The restrictions above apply only to this project's own code, not to third-party components.**

This software uses [EasyTier](https://github.com/EasyTier/EasyTier) for virtual networking.
EasyTier and any modifications made to it remain licensed under **LGPL-3.0**:

```
This software uses the EasyTier project.
EasyTier Copyright (c) EasyTier contributors.
EasyTier is licensed under the GNU Lesser General Public License version 3.0 (LGPL-3.0).
Source: https://github.com/EasyTier/EasyTier
```

- "No commercial use" **does not apply** to the LGPL-3.0 parts of EasyTier;
- "Derivatives must be open-sourced under the same license" **does not apply** to them either;
- The custom license used by this project shall not be construed to limit any right granted to
  users by LGPL-3.0, including the right to replace the EasyTier shared libraries with your own
  builds.

Related files:

- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) — versions, commits, modification status, source retrieval, binary SHA-256
- [LICENSE-LGPL-3.0.txt](LICENSE-LGPL-3.0.txt) — full LGPL-3.0 text
- [LICENSE-GPL-3.0.txt](LICENSE-GPL-3.0.txt) — full GPL-3.0 text (LGPL-3.0 incorporated by reference)
- [licenses/](licenses/) — full texts of all third-party licenses

### Trademarks and non-affiliation

This project is not an official Minecraft product and is not approved by or associated with Mojang
Studios or Microsoft. Minecraft is a trademark of Mojang Synergies AB.

This project has no affiliation with the MCTier project or its original author; the MCTier name is
used solely to describe the origin of the code. Neither WireGuard LLC, the WireGuard project, nor
the Wintun project endorses this project.

## Default services and metadata disclosure

MCTier_Core inherits the default service configuration of the original MCTier. **Establishing
connections** and **version checks** access the services below; with the default configuration the
service operators can observe the corresponding connection metadata:

| Default service | Address | Metadata visible to the operator | Purpose |
|---|---|---|---|
| Signaling server | `wss://mctier.pmhs.top/signaling` | public IP, connection time, lobby name & password hash for matching, player name, virtual IP/domain, member count, client version | WebRTC signaling, lobby member discovery |
| EasyTier public nodes | default `tcp://easytier.weiai.org.cn:11010`, alternatives `tcp://225284.xyz:11010`, `udp://us01.225284.xyz:11010` | public IP, connection time, EasyTier network identifier | P2P hole punching and relay when needed |
| Version check | `https://gitee.com/api/v5/repos/peng-minghang/mctier/tags` | public IP, request time (recorded by Gitee) | fetch latest version number |

Notes:

- **Metadata ≠ content**. The signaling server and EasyTier nodes neither decrypt nor store chat,
  voice or file content.
- **You can avoid the official services entirely**: configure your own signaling server and
  EasyTier nodes, in which case no data passes through them.
- As a dynamic library, this project leaves the choice of signaling server / nodes entirely to the
  caller's configuration.

## Disclaimer

- This project descends from a **neutral virtual-LAN networking and collaboration tool**, intended
  only for lawful personal use in compliance with local regulations (LAN game sessions, collaboration,
  accessing services you own or are authorized to use).
- Communication content (chat, voice, files, screens, …) is transmitted **peer-to-peer directly**
  between member devices; developers neither participate in, control, nor can inspect any user
  content or its use.
- **Users are solely responsible for all of their actions and transmitted content.** Using this
  project for any illegal activity is strictly prohibited, including but not limited to: unauthorized
  commercial/cross-border networking, distributing illegal or infringing content, controlling or
  monitoring other devices without authorization, fraud or impersonation via voice/voice-changing.
- The software is provided "as is" without warranty of any kind; to the maximum extent permitted by
  law, the developers are not liable for any direct or indirect damages arising from its use.
- If you do not agree with any of the above, do not use this project.

## Credits and authors

**Original project author (MCTier)**: QingYun Studio_PengMingHang (MCTier Team)

- GitHub: <https://github.com/pmh1314520/MCTier>
- Gitee: <https://gitee.com/peng-minghang/mctier>

**MCTier_Core maintainer**: X-starRelight

---

<div align="center">

MCTier_Core is provided under the same license as MCTier, for personal non-commercial use only.

</div>
