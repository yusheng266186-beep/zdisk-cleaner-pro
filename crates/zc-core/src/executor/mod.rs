//! 执行器编排：取消熔断 → 守卫 vet → 模式分发 → 台账。
//!
//! v5 契约（审计 A3、§A、S2）：
//! - 入口检查 `report.cancelled`：被取消的半截报告一律 Err(Cancelled)，
//!   取消契约不再靠调用方自觉；
//! - vault 模式走 journal 化 [`vault::stash_journal`]（move 前落账）；
//! - trash 批量失败降级前先 `exists()` 甄别：已被批量部分成功的文件不再
//!   谎报 failed；仍失败项接入重启删除队列兜底（A3 死代码激活）；
//! - done_bytes/条目大小回查改 HashMap 预索引（原 O(n²) find）。

pub mod reboot;
pub mod trash;
pub mod vault;

use crate::error::{Error, Result};
use crate::guard::Guard;
use crate::ledger::LedgerStore;
use crate::manifest::CleanManifest;
use crate::models::{FileHit, Finding, ScanReport};
use crate::scanner;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanMode {
    /// 移入 Windows 回收站（可从回收站还原；清空回收站后才真正释放）
    #[default]
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
/// 顺序固定：取消熔断 → 提取路径 → 父目录吞并 → **守卫 vet（fail-closed）**
/// → 执行 → 台账。守卫拒绝时整批失败，绝无部分提交；执行层逐项失败只记
/// 台账不中断其余项。
pub fn apply(report: &ScanReport, rule_ids: &[String], mode: CleanMode) -> Result<CleanOutcome> {
    if report.cancelled {
        return Err(Error::Cancelled {
            reason: "该扫描报告已被取消，发现清单不完整，禁止据此执行清理".to_string(),
        });
    }
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

    // 扫描快照大小索引（回收站搬走后不可靠实测，沿用快照口径）
    let size_of: HashMap<&Path, u64> = hits.iter().map(|h| (h.path.as_path(), h.size)).collect();

    let refs: Vec<&Path> = hits.iter().map(|h| h.path.as_path()).collect();
    // moved 统一三元组 (origin, vault 副本/空, 记账字节)
    let (moved, reboot_queued): (Vec<(PathBuf, PathBuf, u64)>, usize) = match mode {
        CleanMode::RecycleBin => {
            let empty_dst = || PathBuf::new();
            let size_of_p = |p: &Path| size_of.get(p).copied().unwrap_or(0);
            match trash::delete_to_recycle_bin(&refs) {
                Ok(()) => (
                    refs.iter()
                        .map(|p| ((*p).to_path_buf(), empty_dst(), size_of_p(p)))
                        .collect(),
                    0,
                ),
                Err(batch_err) => {
                    // 批量提交被个别锁定文件拖垮（如 GPU 进程占着着色器缓存）：
                    // 降级逐文件提交，失败项入账 failed，不拖垮其余文件
                    let mut moved: Vec<(PathBuf, PathBuf, u64)> = Vec::new();
                    let mut queued = 0usize;
                    for p in &refs {
                        if !p.exists() {
                            // exists() 甄别（审计 §A「降级说谎」）：批量调用
                            // 失败前其实已把该文件搬走——不得再报 failed/谎称
                            // 「已原样保留」
                            moved.push(((*p).to_path_buf(), empty_dst(), size_of_p(p)));
                            continue;
                        }
                        match trash::delete_to_recycle_bin(std::slice::from_ref(p)) {
                            Ok(()) => moved.push(((*p).to_path_buf(), empty_dst(), size_of_p(p))),
                            Err(e) => {
                                // A3：占用中无法入回收站的项 → MoveFileExW 重启
                                // 删除队列兜底；排队失败才如实记 failed
                                match reboot::schedule_delete_on_reboot(p) {
                                    Ok(()) => queued += 1,
                                    Err(_) => outcome.failed.push((
                                        p.display().to_string(),
                                        format!("{batch_err} → 逐项重试仍失败: {e}"),
                                    )),
                                }
                            }
                        }
                    }
                    (moved, queued)
                }
            }
        }
        CleanMode::Vault => {
            let store = LedgerStore::open()?;
            let (ok, failed) = vault::stash_journal(
                &vault::vault_session_dir(&report.id),
                &refs,
                &store,
                &report.id,
            )?;
            outcome
                .failed
                .extend(failed.into_iter().map(|(p, e)| (p.display().to_string(), e)));
            (ok, 0)
        }
    };

    // 记账口径：vault 用副本实测字节（目录=子树求和，stash_journal 已量）——
    // 扫描与清理之间活目录会增长，快照记账曾造成 vault 实重 > 台账、
    // 撤销/彻底删除对不上账；回收站条目搬走后不可靠实测，沿用扫描快照。
    outcome.done_bytes = moved.iter().map(|(_, _, s)| *s).sum();
    outcome.done_files = moved.len() as u64;

    if mode == CleanMode::RecycleBin {
        // vault 模式的台账已由 stash_journal 的 journal 落账，绝不二次写入
        let entries = moved
            .iter()
            .map(|(o, d, s)| crate::manifest::ManifestEntry {
                origin: o.display().to_string(),
                vault_rel: d.display().to_string(),
                size: *s,
            })
            .collect();
        CleanManifest {
            id: report.id.clone(),
            created_unix: scanner::now_unix(),
            mode,
            entries,
        }
        .save()?;
    }

    if reboot_queued > 0 {
        outcome.semantics_note = format!(
            "{}；另有 {} 项被占用无法立即移入回收站，已排入重启后删除队列（重启后完成删除）",
            outcome.semantics_note, reboot_queued
        );
    }
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
