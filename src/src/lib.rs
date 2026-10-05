// MCTier_Core —— 基于 MCTier 项目开发
// 原作者：MCTier Team　原项目地址：https://github.com/pmh1314520/MCTier
// 原始代码 Copyright (c) 2024-2026 青云制作_彭明航（MCTier Team）
// 本项目修改与新增代码 Copyright (c) 2026 X-starRelight
// 许可证：见仓库根目录 LICENSE（MCTier 自定义源码可得非商业许可）
//
// MCTier_Core 动态库入口。
//
// 阶段1已删除原 Tauri 应用壳：tauri::Builder、系统托盘、全局热键、窗口管理、
// single-instance、deep-link、WebView 页面钩子以及 main.rs 二进制入口，命令层
// （modules::tauri_commands）暂时以普通函数形式存在；阶段3 将重构为统一分发表，
// 阶段4 通过 ffi.rs 以 C ABI 对外暴露。
pub mod modules;
