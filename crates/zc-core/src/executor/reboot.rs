//! 重启后删除队列：MoveFileExW MOVEFILE_DELAY_UNTIL_REBOOT。
//! 需要管理员权限（写 PendingFileRenameOperations）。

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT,
};

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// 登记一个「下次重启时删除」。锁定中的缓存文件由该机制兜底。
pub fn schedule_delete_on_reboot(p: &Path) -> io::Result<()> {
    let wide = to_wide(&p.as_os_str().to_string_lossy());
    let ok = unsafe { MoveFileExW(wide.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // 无法在单测里真正登记（需管理员）；只验证 wide 编码路径。
    #[test]
    fn wide_encode_roundtrip_len() {
        use std::os::windows::ffi::OsStrExt as _;
        let v = std::ffi::OsStr::new(r"C:\a b")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>();
        assert_eq!(v, [67u16, 58, 92, 97, 32, 98, 0]);
    }
}
