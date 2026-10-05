// MCTier_Core —— 基于 MCTier 项目开发
// 原作者：MCTier Team　原项目地址：https://github.com/pmh1314520/MCTier
// 原始代码 Copyright (c) 2024-2026 青云制作_彭明航（MCTier Team）
// 本项目修改与新增代码 Copyright (c) 2026 X-starRelight
// 许可证：见仓库根目录 LICENSE（MCTier 自定义源码可得非商业许可）
//! Windows system paths used by privileged and diagnostic operations.

#![cfg(windows)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

/// Resolve the actual Windows system directory without consulting PATH or
/// the inherited SystemRoot environment variable.
pub fn system_directory() -> PathBuf {
    let mut buffer = [0u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
    assert!(
        length > 0 && length < buffer.len(),
        "GetSystemDirectoryW failed"
    );
    PathBuf::from(OsString::from_wide(&buffer[..length]))
}

/// Resolve a fixed system executable path. Callers pass only compile-time
/// names or paths; reject traversal so this cannot become a path join helper.
pub fn system_command(name: &str) -> PathBuf {
    assert!(
        !name.is_empty()
            && !name.contains('\0')
            && !name.contains("..")
            && !name.starts_with(['\\', '/'])
            && !name.contains(':')
            && !name.contains('*')
            && !name.contains('?'),
        "invalid Windows system path"
    );
    let command = system_directory().join(name);
    log::info!(
        "system_command resolved: name={} command={}",
        name,
        command.display()
    );
    command
}

pub fn hosts_path() -> PathBuf {
    system_directory().join("drivers").join("etc").join("hosts")
}
