// MCTier_Core —— 基于 MCTier 项目开发
// 原作者：MCTier Team　原项目地址：https://github.com/pmh1314520/MCTier
// 原始代码 Copyright (c) 2024-2026 青云制作_彭明航（MCTier Team）
// 本项目修改与新增代码 Copyright (c) 2026 X-starRelight
// 许可证：见仓库根目录 LICENSE（MCTier 自定义源码可得非商业许可）

/// 获取日志文件路径
///
/// # 返回
/// * `Ok(String)` - 日志文件路径
/// * `Err(String)` - 错误信息
#[tauri::command]
pub async fn get_log_file_path() -> Result<String, String> {
    let log_path = crate::modules::app_paths::log_path().map_err(|e| e.to_string())?;

    Ok(log_path.to_string_lossy().to_string())
}

/// 读取最近的运行日志，供设置页内查看。仅返回末尾内容，避免日志过大阻塞界面。
#[tauri::command]
pub async fn read_log_file() -> Result<String, String> {
    let log_path = crate::modules::app_paths::log_path().map_err(|e| e.to_string())?;

    let bytes = tokio::fs::read(&log_path)
        .await
        .map_err(|e| format!("读取日志失败: {}", e))?;
    const MAX_BYTES: usize = 512 * 1024;
    let start = bytes.len().saturating_sub(MAX_BYTES);
    let mut content = String::from_utf8_lossy(&bytes[start..]).into_owned();
    if start > 0 {
        content = format!("[仅显示最近 512 KB 日志]\n{}", content);
    }
    Ok(content)
}
