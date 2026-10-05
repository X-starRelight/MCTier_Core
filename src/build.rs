fn main() {
    // Model preparation happens on the build machine, never on the user's device.
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    for file in ["shared/speech-model.json", "scripts/prepare-speech-model.mjs", "shared/generated/speech-model/model.int8.onnx.gzip", "shared/generated/speech-model/tokens.txt", "shared/builtin-emoji/builtin-v3.pack.gz"] {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }
    let status = std::process::Command::new("node")
        .arg(root.join("scripts/prepare-speech-model.mjs"))
        .status().expect("Node.js is required to prepare the bundled speech model");
    assert!(status.success(), "Bundled speech model preparation failed");
    #[cfg(windows)]
    {
        // RCDATA keeps compressed model data out of LLVM IR during optimization.
        let model_path = root.join("shared/generated/speech-model/model.int8.onnx.gzip")
            .display().to_string().replace('\\', "/");
        let emoji_path = root.join("shared/builtin-emoji/builtin-v3.pack.gz")
            .display().to_string().replace('\\', "/");
        let rc = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("speech-model.rc");
        std::fs::write(&rc, format!("#pragma code_page(65001)\nMCTIER_SPEECH_MODEL 10 \"{model_path}\"\nMCTIER_BUILTIN_EMOJI_PACK 11 \"{emoji_path}\"\n"))
            .expect("write bundled speech and emoji resources");
        embed_resource::compile_for_everything(&rc, embed_resource::NONE)
            .manifest_required().expect("compile bundled speech and emoji resources");

        // 阶段 1 已移除 GUI 与 tauri.conf.json，不再通过 tauri_build 嵌入应用清单：
        // 本 crate 现在只有 lib target，UAC/assembly 清单属于调用方的可执行文件。
        // 但依赖链中静态导入了 comctl32 的 `TaskDialogIndirect`，该导出只存在于并行
        // 程序集里的 comctl32 v6；缺少清单时测试进程在入口点就以
        // STATUS_ENTRYPOINT_NOT_FOUND(0xC0000139) 崩溃。给测试目标补上同一份依赖。
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
