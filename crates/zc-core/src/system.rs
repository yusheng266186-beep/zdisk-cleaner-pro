//! 运行环境探测与系统级占用盘点。

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

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

/* ── 系统级占用盘点（深度工具卡 C）────────────────────────── */

/// 系统级占用条目：只报事实 + 官方通道指引，绝不野删系统文件。
#[derive(Serialize)]
pub struct OccupancyItem {
    pub name: &'static str,
    pub path: String,
    /// 字节数；stat 被 ACL 拒绝（hiberfil/pagefile 常态）时为 None（UI 显示「未知」）
    pub size: Option<u64>,
    pub guide_zh: &'static str,
}

/// 系统级大占用盘点：Windows.old / hiberfil.sys / pagefile.sys / swapfile.sys。
///
/// 口径：
/// - Windows.old 仅在存在时列出并实测整棵子树字节；
/// - 盘点文件条目恒在（size 拿不到就诚实给 None），保证返回非空、结构稳定；
/// - 本函数只读不删，清理动作一律走条目自带的官方通道指引。
pub fn system_occupancy() -> Vec<OccupancyItem> {
    // SystemDrive 形如 "C:"；补反斜杠成根目录再 join，避免 "C:" 盘相对路径歧义
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let root = if drive.ends_with('\\') { drive } else { format!("{drive}\\") };
    let mut out = Vec::new();

    // 1) Windows.old：旧系统安装残留，存在才测量（整棵子树字节）
    let win_old = Path::new(&root).join("Windows.old");
    if win_old.is_dir() {
        out.push(OccupancyItem {
            name: "Windows.old",
            path: win_old.to_string_lossy().into_owned(),
            size: dir_bytes(&win_old),
            guide_zh: "设置→系统→存储→临时文件→以前的 Windows 安装",
        });
    }

    // 2) hiberfil.sys：休眠文件，默认 ACL 下 stat 常被拒绝 → size=None
    out.push(root_file_item(
        &root,
        "hiberfil.sys",
        "管理员运行 powercfg /h off 可关闭休眠并释放",
    ));

    // 3) pagefile.sys / swapfile.sys：虚拟内存，同上不给野路子
    for file in ["pagefile.sys", "swapfile.sys"] {
        out.push(root_file_item(
            &root,
            file,
            "此为虚拟内存，建议通过 系统属性→高级→性能→虚拟内存 调整",
        ));
    }
    out
}

fn root_file_item(root: &str, file: &'static str, guide_zh: &'static str) -> OccupancyItem {
    let p = Path::new(root).join(file);
    let size = std::fs::metadata(&p).map(|m| m.len()).ok();
    OccupancyItem {
        name: file,
        path: p.to_string_lossy().into_owned(),
        size,
        guide_zh,
    }
}

/// 目录字节总量：与扫描引擎同款 jwalk 单遍走完，只累加普通文件长度。
/// 子项读取失败（ACL/占用）按跳过处理——盘点只报事实，不阻塞不报错。
fn dir_bytes(root: &Path) -> Option<u64> {
    let total = AtomicU64::new(0);
    for entry in jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Ok(m) = entry.metadata() {
            if m.is_file() {
                total.fetch_add(m.len(), Ordering::Relaxed);
            }
        }
    }
    Some(total.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    #[test]
    fn elevated_flag_is_consistent_bool() {
        // 只验证可调用且不 panic；CI 与本机均可能为 false 或 true。
        let _ = super::is_elevated();
    }

    #[test]
    fn occupancy_vec_nonempty_and_structured() {
        let items = super::system_occupancy();
        // 结构成立：三大盘点文件条目恒在（Windows.old 仅存在时才列入）
        assert!(items.iter().any(|i| i.name == "pagefile.sys"));
        assert!(items.iter().any(|i| i.name == "swapfile.sys"));
        assert!(items.iter().any(|i| i.name == "hiberfil.sys"));
        assert!(!items.is_empty());
        // 指引文案全非空，路径全非空
        assert!(items.iter().all(|i| !i.guide_zh.is_empty()));
        assert!(items.iter().all(|i| !i.path.is_empty()));
    }
}
