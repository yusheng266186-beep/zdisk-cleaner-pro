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
            trash::delete_to_recycle_bin(&refs)?;
            refs.iter().map(|p| ((*p).to_path_buf(), PathBuf::new())).collect()
        }
        CleanMode::Vault => {
            let (ok, failed) = vault::stash(&vault::vault_session_dir(&report.id), &refs);
            outcome
                .failed
                .extend(failed.into_iter().map(|(p, e)| (p.display().to_string(), e)));
            ok
        }
    };

    for (origin, _) in &moved {
        outcome.done_bytes +=
            hits.iter().find(|h| &h.path == origin).map(|h| h.size).unwrap_or(0);
    }
    outcome.done_files = moved.len() as u64;

    let entries = moved
        .iter()
        .map(|(o, d)| crate::manifest::ManifestEntry {
            origin: o.display().to_string(),
            vault_rel: d.display().to_string(),
            size: hits.iter().find(|h| &h.path == o).map(|h| h.size).unwrap_or(0),
        })
        .collect();
    CleanManifest {
        id: report.id.clone(),
        created_unix: scanner::now_unix(),
        mode,
        entries,
    }
    .save()?;

    Ok(outcome)
}

/// 供 CLI/UI 汇总：(总条目数, 总字节)
pub fn findings_total(findings: &[Finding]) -> (u64, u64) {
    (
        findings.iter().map(|f| f.total_count()).sum(),
        findings.iter().map(|f| f.total_bytes()).sum(),
    )
}
