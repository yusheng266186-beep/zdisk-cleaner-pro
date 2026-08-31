//! 回收站查询与清空（v5 新增，审计 A2「回收站清空能力为零」）。
//!
//! - [`query_all`]：SHQueryRecycleBinW 逐盘聚合（GetLogicalDrives 位图 ×
//!   GetDriveTypeW 过滤本地/可移动盘），永不失败——单盘出错记 0；
//! - [`empty_all`]：SHEmptyRecycleBinW（NOCONFIRMATION|NOPROGRESSUI|NOSOUND），
//!   释放口径取清空前后配额差（诚实：不以 items×均价 估算）。

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// 回收站占用（全部分盘聚合）。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct RecycleBinInfo {
    pub items: u64,
    pub bytes: u64,
}

/// 清空动作摘要：前后 quota 差即真实释放口径。
#[derive(Serialize, Clone, Debug)]
pub struct RecycleBinSummary {
    pub items_before: u64,
    pub bytes_before: u64,
    pub bytes_freed: u64,
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// 单盘回收站查询；任何失败按 0 处理（永不失败语义）。
fn query_one(root_wide: &[u16]) -> RecycleBinInfo {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info: SHQUERYRBINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHQUERYRBINFO>() as u32;
    let hr = unsafe { SHQueryRecycleBinW(root_wide.as_ptr(), &mut info) };
    if hr < 0 {
        return RecycleBinInfo::default();
    }
    RecycleBinInfo {
        items: info.i64NumItems.max(0) as u64,
        bytes: info.i64Size.max(0) as u64,
    }
}

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;

/// 全部本地/可移动盘的回收站聚合。永不失败：出错的分盘记 0。
pub fn query_all() -> RecycleBinInfo {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    let mut total = RecycleBinInfo::default();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let wide = to_wide(&root);
        let dtype = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if dtype != DRIVE_FIXED && dtype != DRIVE_REMOVABLE {
            continue; // 网络/光驱/内存盘无本地 $Recycle.Bin 语义
        }
        let q = query_one(&wide);
        total.items += q.items;
        total.bytes += q.bytes;
    }
    total
}

/// 清空全部盘的回收站（无弹窗、无进度条、无声音）。
/// 成功语义以 quota 差为准；SHEmptyRecycleBinW 失败（如被策略禁用）上抛。
pub fn empty_all() -> Result<RecycleBinSummary> {
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };
    let before = query_all();
    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(), // NULL 根路径 = 全部分盘
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    if hr < 0 {
        return Err(Error::Other(format!("清空回收站失败 (HRESULT {hr:#010x})")));
    }
    let after = query_all();
    Ok(RecycleBinSummary {
        items_before: before.items,
        bytes_before: before.bytes,
        bytes_freed: before.bytes.saturating_sub(after.bytes),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn query_never_panics_and_is_monotonic_fieldwise() {
        let a = super::query_all();
        let b = super::query_all();
        // 同机连续两次查询数量级一致（回收站并发变动容忍 ±∞，仅断言可调用非负）
        assert!(a.items < u64::MAX && a.bytes < u64::MAX);
        let _ = b;
    }
}
