//! 运行环境探测。

/// 当前进程是否以提升（管理员）令牌运行。
///
/// 不用「能否写 C:\Windows\Temp」这类旁路猜测——默认 ACL 下普通用户
/// 也可能在系统 Temp 创建目录。直接查进程令牌的 TokenElevation。
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn elevated_flag_is_consistent_bool() {
        // 只验证可调用且不 panic；CI 与本机均可能为 false 或 true。
        let _ = super::is_elevated();
    }
}
