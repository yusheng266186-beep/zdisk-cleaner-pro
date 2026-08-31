//! 安全守卫：删除前的最后一道闸。
//!
//! 设计原则 **fail-closed**：任何路径无法解析、无法归一化、无法确认归属，
//! 一律按违规处理，拒绝整批操作。
//!
//! v5 契约（CONTRACT-v5 §1 / 审计 S4、A1）：
//! - 禁删根不再硬编码 C 盘，由 `%SystemRoot%` / `%ProgramFiles%` /
//!   `%ProgramFiles(x86)%` / `%ProgramData%` 派生；env 缺失回落 C: 字面。
//! - `USERPROFILE` 缺失 → vet 直接 Err（守卫内唯一的 fail-open 分支已封死）。
//! - 自保护路径使用 [`crate::manifest::data_dir`]（覆盖 ZC_DATA_DIR 重定向的
//!   便携模式，vault 本体永远在保护名单内）。
//! - 提权进程自动追加 elevated allowlist：仅目录级精确豁免少数已知安全的
//!   系统清理目录（Windows\Temp、SoftwareDistribution\Download 等），
//!   Windows 树其余部分依旧 fail-closed——打破「admin 规则扫得出清不掉」死锁。
//! - 测试可用 [`Guard::with_env`] 注入 env 快照，不依赖 `set_var`。

use crate::error::{Error, Result};
use crate::patterns::norm;
use std::path::{Path, PathBuf};

/// env 快照：Guard 的全部环境依赖。测试注入用，生产走 [`GuardEnv::from_process_env`]。
#[derive(Debug, Clone)]
pub struct GuardEnv {
    pub user_profile: Option<PathBuf>,
    pub system_root: Option<PathBuf>,
    pub program_files: Option<PathBuf>,
    pub program_files_x86: Option<PathBuf>,
    pub program_data: Option<PathBuf>,
    /// SystemDrive 环境变量（形如 "C:"），禁删根/白名单盘符推导的兜底。
    pub system_drive: Option<PathBuf>,
    /// 本程序数据目录（vault/台账所在），自保护。
    pub data_dir: PathBuf,
    /// 进程是否已提权（system::is_elevated）。提权时 vet 应用 elevated allowlist。
    pub elevated: bool,
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

impl GuardEnv {
    /// 从当前进程环境构建快照。
    pub fn from_process_env() -> Self {
        Self {
            user_profile: env_path("USERPROFILE"),
            system_root: env_path("SystemRoot").or_else(|| env_path("WINDIR")),
            program_files: env_path("ProgramFiles"),
            program_files_x86: env_path("ProgramFiles(x86)"),
            program_data: env_path("ProgramData"),
            system_drive: env_path("SystemDrive"),
            data_dir: crate::manifest::data_dir(),
            elevated: crate::system::is_elevated(),
        }
    }

    /// 系统盘根符 norm 串（"c:"）：SystemRoot 父目录 → SystemDrive → C: 兜底。
    pub fn drive_norm(&self) -> String {
        if let Some(sr) = &self.system_root {
            if let Some(parent) = sr.parent() {
                let n = norm(parent);
                let n = n.trim_end_matches('/').to_string();
                if !n.is_empty() {
                    return n;
                }
            }
        }
        if let Some(d) = &self.system_drive {
            let n = norm(d);
            let n = n.trim_end_matches('/').to_string();
            if !n.is_empty() {
                return n;
            }
        }
        "c:".to_string()
    }

    /// 系统根 norm 串（"c:/windows"），缺失回落 C 盘字面。
    fn system_root_norm(&self) -> String {
        match &self.system_root {
            Some(p) => norm(p),
            None => norm(Path::new(r"C:\Windows")),
        }
    }

    /// ProgramData norm 串，缺失回落 {系统盘}/ProgramData 字面。
    fn program_data_norm(&self) -> String {
        match &self.program_data {
            Some(p) => norm(p),
            None => format!("{}/programdata", self.drive_norm()),
        }
    }
}

/// elevated allowlist（进程环境派生）：提权 worker 唯一可豁免的禁删区前缀。
/// 全部为**目录级（或单文件级）精确前缀**，norm 串，见 CONTRACT-v5 §1。
pub fn elevated_allowlist() -> Vec<String> {
    elevated_allowlist_from(&GuardEnv::from_process_env())
}

/// 同 [`elevated_allowlist`]，但基于注入的 env 快照（测试用，零 set_var）。
pub fn elevated_allowlist_from(env: &GuardEnv) -> Vec<String> {
    let sr = env.system_root_norm();
    let pd = env.program_data_norm();
    let drive = env.drive_norm();
    [
        format!("{sr}/temp"),
        format!("{sr}/softwaredistribution/download"),
        format!("{sr}/logs/windowsupdate"),
        format!("{sr}/logs/cbs"),
        // v5 新增规则所需的两个窄前缀（见回报「契约偏差」）：
        format!("{sr}/logs/diagnostics"),
        format!("{sr}/softwaredistribution/reportingevents.log"),
        format!("{sr}/serviceprofiles/localservice/appdata/local/fontcache"),
        format!("{sr}/serviceprofiles/localservice/appdata/local/temp"),
        format!("{pd}/microsoft/deliveryoptimization"),
        format!("{sr}/prefetch"),
        format!("{sr}/memory.dmp"),
        format!("{sr}/minidump"),
        format!("{drive}/perflogs"),
        format!("{pd}/microsoft/windows/wer"),
        format!("{drive}/$winreagent"),
    ]
    .into_iter()
    .collect()
}

/// 用户资料内的文档性目录（不属于任何缓存语义）。
fn user_forbidden(user_profile: Option<&Path>) -> Vec<PathBuf> {
    let Some(up) = user_profile.filter(|p| !p.as_os_str().is_empty()) else {
        return Vec::new();
    };
    ["Documents", "Desktop", "Downloads", "Pictures", "Music", "Videos"]
        .iter()
        .map(|d| up.join(d))
        .collect()
}

/// 本程序自保护路径集合（exe 目录 + 数据目录/vault 本体）。
pub fn self_protected() -> Vec<PathBuf> {
    Guard::self_protected_from(&GuardEnv::from_process_env())
}

impl Guard {
    fn self_protected_from(env: &GuardEnv) -> Vec<PathBuf> {
        let mut v = vec![env.data_dir.clone()];
        if let Ok(exe) = std::env::current_exe() {
            // exe 所在目录整体保护（覆盖便携模式与安装目录）
            if let Some(dir) = exe.parent() {
                v.push(dir.to_path_buf());
            }
        }
        v
    }

    /// 从 env 快照构建（测试注入用）。elevated=true 时自动应用 allowlist。
    pub fn with_env(env: GuardEnv) -> Self {
        // 禁删根：env 派生，缺失回落 C: 字面
        let fallback_root = env.drive_norm();
        let system_roots: [PathBuf; 4] = [
            env.system_root
                .clone()
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows")),
            env.program_files
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("{fallback_root}\\Program Files"))),
            env.program_files_x86
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("{fallback_root}\\Program Files (x86)"))),
            env.program_data
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("{fallback_root}\\ProgramData"))),
        ];
        let mut prefixes = Vec::new();
        for p in &system_roots {
            Self::push_canonical(p, &mut prefixes);
        }
        let up = env.user_profile.as_deref();
        for p in user_forbidden(up).iter().chain(Self::self_protected_from(&env).iter()) {
            Self::push_canonical(p, &mut prefixes);
        }
        let allowlist = if env.elevated {
            elevated_allowlist_from(&env)
        } else {
            Vec::new()
        };
        Self {
            prefixes,
            allowlist,
            user_profile_missing: up.is_none(),
        }
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
    ///
    /// 提权守卫下，命中禁删前缀但落入 elevated allowlist 某**精确前缀**内
    /// （相等或其子孙）的路径被豁免；其余 Windows 树路径依旧拒绝。
    pub fn vet<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) -> Result<()> {
        if self.user_profile_missing {
            return Err(Error::GuardRejected {
                path: "-".to_string(),
                reason: "env:USERPROFILE 缺失，文档/桌面/下载保护无法建立（fail-closed）".to_string(),
            });
        }
        for p in paths {
            let real = std::fs::canonicalize(p).map_err(|e| Error::GuardRejected {
                path: p.display().to_string(),
                reason: format!("无法解析真实路径（fail-closed）: {e}"),
            })?;
            let n = norm(&real);
            for pref in &self.prefixes {
                if n == *pref || n.starts_with(&format!("{pref}/")) {
                    if self.allowlisted(&n) {
                        break; // elevated 豁免，检查下一条路径
                    }
                    return Err(Error::GuardRejected {
                        path: p.display().to_string(),
                        reason: format!("命中禁删区 [{pref}]"),
                    });
                }
            }
        }
        Ok(())
    }

    fn allowlisted(&self, n: &str) -> bool {
        // 防御性纵深：vet 传入的 n 来自 canonicalize（理论上已折叠 `..`），
        // 任何含 `..` 段的串仍直接拒绝，绝不给白名单开穿越门。
        if n.split('/').any(|seg| seg == "..") {
            return false;
        }
        self.allowlist.iter().any(|a| {
            n == a.as_str() || n.starts_with(&format!("{a}/"))
        })
    }

    /// 禁删前缀清单（norm 串）。结构测试与诊断用。
    pub fn forbidden_prefixes(&self) -> &[String] {
        &self.prefixes
    }

    /// 当前生效的 elevated allowlist（非提权守卫为空）。
    pub fn allowlist(&self) -> &[String] {
        &self.allowlist
    }

    #[cfg(test)]
    pub fn prefix_count(&self) -> usize {
        self.prefixes.len()
    }
}

pub struct Guard {
    /// 归一化后的禁删前缀（规范化真实的盘符大小写/符号链接形态后再存）
    prefixes: Vec<String>,
    /// 提权守卫的目录级精确豁免前缀（norm 串）；非提权守卫为空。
    allowlist: Vec<String>,
    /// USERPROFILE 缺失标记：vet fail-closed。
    user_profile_missing: bool,
}

impl Guard {
    /// 构建守卫（进程环境快照）。缺 USERPROFILE 时 vet 直接 Err。
    pub fn new() -> Self {
        Self::with_env(GuardEnv::from_process_env())
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

    fn snapshot_env(elevated: bool) -> GuardEnv {
        GuardEnv { elevated, ..GuardEnv::from_process_env() }
    }

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

    #[test]
    fn forbidden_roots_derive_from_env_snapshot() {
        // SystemRoot 在 D: 盘的机器：真实 Windows 树必须得到保护（S4 修复核心）
        let env = GuardEnv {
            system_root: Some(PathBuf::from(r"D:\Windows")),
            program_files: Some(PathBuf::from(r"D:\Program Files")),
            program_files_x86: Some(PathBuf::from(r"D:\Program Files (x86)")),
            program_data: Some(PathBuf::from(r"D:\ProgramData")),
            user_profile: Some(PathBuf::from(r"D:\Users\tester")),
            system_drive: Some(PathBuf::from("D:")),
            data_dir: PathBuf::from(r"D:\app-data"),
            elevated: false,
        };
        let g = Guard::with_env(env);
        assert!(g.forbidden_prefixes().iter().any(|p| p.starts_with("d:/windows")));
        assert!(g.forbidden_prefixes().iter().any(|p| p.starts_with("d:/program files")));
        assert!(g.forbidden_prefixes().iter().any(|p| p.starts_with("d:/programdata")));
    }

    #[test]
    fn userprofile_missing_is_fail_closed() {
        let env = GuardEnv {
            user_profile: None,
            ..snapshot_env(false)
        };
        let g = Guard::with_env(env);
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x.bin");
        std::fs::write(&f, b"x").unwrap();
        let err = g.vet([f.as_path()]).unwrap_err().to_string();
        assert!(err.contains("USERPROFILE"), "{err}");
    }

    #[test]
    fn elevated_allowlist_admits_known_safe_dirs_but_not_the_rest() {
        let sr = GuardEnv::from_process_env().system_root_norm();
        let windir = PathBuf::from(
            std::env::var_os("SystemRoot").expect("测试机必有 SystemRoot"),
        );
        let temp_dir = windir.join("Temp");
        let sys32 = windir.join("System32");
        assert!(temp_dir.is_dir(), "夹具前提：{temp_dir:?}");

        let g_plain = Guard::with_env(snapshot_env(false));
        assert!(g_plain.vet([temp_dir.as_path()]).is_err(), "未提权必须拒绝 {sr}/temp");

        let g_adm = Guard::with_env(snapshot_env(true));
        assert!(g_adm.allowlist().iter().any(|a| a.ends_with("/temp")), "allowlist 未派生");
        assert!(g_adm.vet([temp_dir.as_path()]).is_ok(), "提权守卫应豁免 {sr}/temp 本体");
        // Windows 树其余部分（System32）依旧 fail-closed
        assert!(g_adm.vet([sys32.as_path()]).is_err(), "提权也绝不放行 {sr}/system32");
    }

    #[test]
    fn allowlist_prefixes_are_boundary_exact() {
        // 纯前缀语义单元验证（无需真实路径）：{a} 放行 a 与 a/...，不放行 aX
        let env = GuardEnv {
            elevated: true,
            ..GuardEnv::from_process_env()
        };
        let g = Guard::with_env(env);
        let a = g.allowlist()[0].clone(); // {sr}/temp
        assert!(g.allowlisted(&a));
        assert!(g.allowlisted(&format!("{a}/sub/x.tmp")));
        assert!(!g.allowlisted(&format!("{a}x/y")));
        assert!(!g.allowlisted(&format!("{a}/../system32")));
    }

    #[test]
    fn self_protected_covers_redirected_data_dir() {
        // S4：ZC_DATA_DIR 重定向的便携模式，vault 本体必须在自保护清单
        let env = GuardEnv {
            data_dir: PathBuf::from(r"X:\portable-zc\data"),
            ..GuardEnv::from_process_env()
        };
        let g = Guard::with_env(env);
        assert!(g.forbidden_prefixes().iter().any(|p| p.starts_with("x:/portable-zc/data")));
    }
}
