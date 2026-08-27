//! 回收站批删：SHFileOperationW + FOF_ALLOWUNDO 批量提交。
//!
//! 注意：该 API 不支持 `\\?\` 超长路径前缀，超长输入显式报错
//! （UI 应引导改用 vault 模式，vault 走 fs::rename 无此限制）。

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

const FO_DELETE: u32 = 0x0003;

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// 把多个路径一次提交进回收站（FOF_ALLOWUNDO 语义）。
pub fn delete_to_recycle_bin(paths: &[&Path]) -> io::Result<()> {
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, SHFILEOPSTRUCTW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
        FOF_SILENT,
    };

    if paths.is_empty() {
        return Ok(());
    }

    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        let s = p.as_os_str().to_string_lossy();
        if s.chars().count() > 240 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("路径过长，回收站 API 不支持，请使用 vault 模式: {s}"),
            ));
        }
        list.extend(to_wide(&s));
    }
    list.push(0); // 双 NUL 终结

    let flags: u16 = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as _;

    let mut op = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: list.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: flags,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 {
        return Err(io::Error::other(format!("回收站提交失败 (code={code})")));
    }
    Ok(())
}
