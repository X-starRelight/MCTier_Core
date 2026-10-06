fn main() {
    // 阶段 1 已移除 GUI 与 tauri.conf.json，不再通过 tauri_build 嵌入应用清单：
    // 本 crate 现在只有 lib target，UAC/assembly 清单属于调用方的可执行文件。
    // 但依赖链中静态导入了 comctl32 的 `TaskDialogIndirect`，该导出只存在于并行
    // 程序集里的 comctl32 v6；缺少清单时测试进程在入口点就以
    // STATUS_ENTRYPOINT_NOT_FOUND(0xC0000139) 崩溃。给测试目标补上同一份依赖。
    #[cfg(windows)]
    println!(
        "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}
