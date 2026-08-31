//! 执行器编排：守卫 vet → 模式分发 → 生成台账。

pub mod reboot;
pub mod trash;
pub mod vault;

use crate::error::Result;
use crate::guard::Guard;
use crate::manifest::CleanManifest;
use crate::models::{FileHit, Finding, ScanReport};
use crate::scanner;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanMode {
    /// 移入 Windows 回收站（可从回收站还原；清空回收站后才真正释放）
    RecycleBin,
    /// 移入受管 vault 暂存区（7 天后悔期）
    Vault,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CleanOutcome {
    pub requested_files: u64,
    pub requested_bytes: u64,
    pub done_files: u64,
    pub done_bytes: u64,
    pub failed: Vec<(String, String)>,
    /// 模式语义提示：回收站≠真实释放，由 UI/CLI 原样转述
    pub semantics_note: String,
}

/// 对报告中选中的规则执行清理。
///
/// 顺序固定：提取路径 → 父目录吞并 → **守卫 vet（fail-closed）** → 执行 → 台账。
/// 守卫拒绝时整批失败，绝无部分提交；执行层逐项失败只记台账不中断其余项。
pub fn apply(report: &ScanReport, rule_ids: &[String], mode: CleanMode) -> Result<CleanOutcome> {
    let mut hits: Vec<FileHit> = Vec::new();
    for f in &report.findings {
        if rule_ids.iter().any(|id| id == &f.rule_id) {
            hits.extend(f.hits.iter().cloned());
        }
    }
    // 目录命中吞并子文件后统一提交大颗粒
    scanner::dedup_nested(&mut hits);

    let mut outcome = CleanOutcome {
        requested_files: hits.len() as u64,
        requested_bytes: hits.iter().map(|h| h.size).sum(),
        ..Default::default()
    };
    outcome.semantics_note = match mode {
        CleanMode::RecycleBin => "已移入回收站：清空回收站前不会真正释放磁盘空间".to_string(),
        CleanMode::Vault => "已移入暂存区 vault：7 天内可一键还原".to_string(),
    };
    if hits.is_empty() {
        return Ok(outcome);
    }

    Guard::new().vet(hits.iter().map(|h| h.path.as_path()))?;

    let refs: Vec<&Path> = hits.iter().map(|h| h.path.as_path()).collect();
    let moved: Vec<(PathBuf, PathBuf)> = match mode {
        CleanMode::RecycleBin => {
            match trash::delete_to_recycle_bin(&refs) {
                Ok(()) => refs.iter().map(|p| ((*p).to_path_buf(), PathBuf::new())).collect(),
                Err(batch_err) => {
                    // 批量提交被个别锁定文件拖垮（如 GPU 进程占着着色器缓存）：
                    // 降级逐文件提交，失败项入账 failed，不拖垮其余文件
                    for p in &refs {
                        match trash::delete_to_recycle_bin(std::slice::from_ref(p)) {
                            Ok(()) => {}
                            Err(e) => outcome.failed.push((
                                p.display().to_string(),
                                format!("{batch_err} → 逐项重试仍失败: {e}"),
                            )),
                        }
                    }
                    refs.iter()
                        .filter(|p| !outcome.failed.iter().any(|(f, _)| f == &p.display().to_string()))
                        .map(|p| ((*p).to_path_buf(), PathBuf::new()))
                        .collect()
                }
            }
        }
        CleanMode::Vault => {
            let (ok, failed) = vault::stash(&vault::vault_session_dir(&report.id), &refs);
            outcome
                .failed
                .extend(failed.into_iter().map(|(p, e)| (p.display().to_string(), e)));
            ok
        }
    };

    // 记账口径：vault 用副本实测字节（目录=子树求和）——扫描与清理之间活目录
    // 会增长，快照记账曾造成 vault 实重 > 台账、撤销/彻底删除对不上账；
    // 回收站条目搬走后不可靠实测，沿用扫描快照（清空回收站才真正释放）。
    outcome.done_bytes = moved
        .iter()
        .map(|(origin, dst)| match mode {
            CleanMode::Vault => super::executor::vault::actual_size(dst),
            CleanMode::RecycleBin => {
                hits.iter().find(|h| &h.path == origin).map(|h| h.size).unwrap_or(0)
            }
        })
        .sum();
    outcome.done_files = moved.len() as u64;

    let entries = moved
        .iter()
        .map(|(o, d)| crate::manifest::ManifestEntry {
            origin: o.display().to_string(),
            vault_rel: d.display().to_string(),
            size: match mode {
                CleanMode::Vault => super::executor::vault::actual_size(d),
                CleanMode::RecycleBin => {
                    hits.iter().find(|h| &h.path == o).map(|h| h.size).unwrap_or(0)
                }
            },
        })
        .collect();
    CleanManifest {
        id: report.id.clone(),
        created_unix: scanner::now_unix(),
        mode,
        entries,
    }
    .save()?;

    if !outcome.failed.is_empty() {
        outcome.semantics_note = format!(
            "{}；另有 {} 项未能处理（多为文件被占用），已原样保留",
            outcome.semantics_note,
            outcome.failed.len()
        );
    }

    Ok(outcome)
}

/// 供 CLI/UI 汇总：(总条目数, 总字节)
pub fn findings_total(findings: &[Finding]) -> (u64, u64) {
    (
        findings.iter().map(|f| f.total_count()).sum(),
        findings.iter().map(|f| f.total_bytes()).sum(),
    )
}
