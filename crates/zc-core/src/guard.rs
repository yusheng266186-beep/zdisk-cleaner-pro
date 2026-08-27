//! 安全守卫：删除前的最后一道闸。
//!
//! 设计原则 **fail-closed**：任何路径无法解析、无法归一化、无法确认归属，
//! 一律按违规处理，拒绝整批操作。

use crate::error::{Error, Result};
use crate::patterns::norm;
use std::path::{Path, PathBuf};

/// 系统级禁删根（始终生效）。
const SYSTEM_FORBIDDEN: &[&str] = &[
    r"C:\Windows",
    r"C:\Program Files",
    r"C:\Program Files (x86)",
    r"C:\ProgramData", // 整个 ProgramData 只进只出（WER 等由规则只读统计、清理走 admin worker 白名单）
];

/// 用户资料内的文档性目录（不属于任何缓存语义）。
fn user_forbidden() -> Vec<PathBuf> {
    let up = match std::env::var("USERPROFILE") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => return Vec::new(),
    };
    ["Documents", "Desktop", "Downloads", "Pictures", "Music", "Videos"]
        .iter()
        .map(|d| up.join(d))
        .collect()
}

/// 本程序自保护路径集合。
pub fn self_protected() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        // exe 所在目录整体保护（覆盖便携模式与安装目录）
        if let Some(dir) = exe.parent() {
            v.push(dir.to_path_buf());
        }
    }
    if let Ok(lad) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(&lad).join("ZDiskCleanerPro3"));
    }
    v
}

pub struct Guard {
    /// 归一化后的禁删前缀（规范化真实的盘符大小写/符号链接形态后再存）
    prefixes: Vec<String>,
}

impl Guard {
    /// 构建守卫。仅收集存在的路径；全部缺失时依然可用（vet 将 fail-closed）。
    pub fn new() -> Self {
        let mut prefixes = Vec::new();
        for p in SYSTEM_FORBIDDEN {
            Self::push_canonical(p.as_ref(), &mut prefixes);
        }
        for p in user_forbidden().iter().chain(self_protected().iter()) {
            Self::push_canonical(p, &mut prefixes);
        }
        Self { prefixes }
    }

    fn push_canonical(p: &Path, out: &mut Vec<String>) {
        match std::fs::canonicalize(p) {
            Ok(real) => out.push(norm(&real)),
            Err(_) => {
                // 目标不存在时退化为字面归一化（扫描态可容忍），但 vet 时真实解析
                out.push(norm(p));
            }
        }
    }

    /// 校验一批即将被改动（移动/删除）的路径。任一违规即返回 Err。
    pub fn vet<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) -> Result<()> {
        for p in paths {
            let real = std::fs::canonicalize(p).map_err(|e| Error::GuardRejected {
                path: p.display().to_string(),
                reason: format!("无法解析真实路径（fail-closed）: {e}"),
            })?;
            let n = norm(&real);
            for pref in &self.prefixes {
                if n == *pref || n.starts_with(&format!("{pref}/")) {
                    return Err(Error::GuardRejected {
                        path: p.display().to_string(),
                        reason: format!("命中禁删区 [{pref}]"),
                    });
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn prefix_count(&self) -> usize {
        self.prefixes.len()
    }
}

impl Default for Guard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_inside_windows() {
        let g = Guard::new();
        assert!(g.vet([Path::new(r"C:\Windows\System32\config.sys")]).is_err());
    }

    #[test]
    fn rejects_unresolvable_fail_closed() {
        let g = Guard::new();
        let ghost = PathBuf::from(format!("C:\\nonexistent-zc-{:x}\\x.bin", std::process::id()));
        assert!(g.vet([ghost.as_path()]).is_err());
    }

    #[test]
    fn allows_temp_junk_in_custom_root() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("junk.log");
        std::fs::write(&f, b"x").unwrap();
        let g = Guard::new();
        assert!(g.vet([f.as_path()]).is_ok());
    }

    #[test]
    fn guard_has_system_prefixes() {
        assert!(Guard::new().prefix_count() >= 3);
    }
}
