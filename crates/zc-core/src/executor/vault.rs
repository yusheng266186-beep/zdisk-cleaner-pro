//! vault 暂存区：把待删项移动到受管目录，保留 7 天可整批/单项还原。
//! 相比回收站模式：vault 在同盘是瞬时 rename，跨盘才真正拷贝，
//! 且不受 SHFileOperationW 的路径长度限制。
//!
//! v5 契约（审计 S1/S2、§A）：
//! - **journal 化 stash**：move 之前先把 manifest + 全量 entries
//!   （status='pending'）落账，逐条搬运成功即 UPDATE 为 committed，
//!   失败项撤行；全败时批次整行抹除。崩溃窗口内 vault 有实体而台账无
//!   记录的「无主副本」从此不存在，也满足「禁止台账外无主副本」不变量。
//! - 孤儿 GC 三保险：台账读名单 Err → 本轮 GC 整体熔断；会话目录
//!   mtime 不足 24h → 不删；仍持有 pending 条目的 id → 不删。
//! - 目录搬运依旧只允许原子 rename、文件 copy 回退删源失败必回滚副本
//!   （两条硬约束不变）；copy 保留 mtime；实际占用量（actual_size）与
//!   copy_all 改迭代器栈实现（深目录零爆栈风险）。
//! - 重解析点/云占位文件（OneDrive 等）跨盘 copy 直接记 failed，
//!   不触发云端水合拉流。
//! - 数据目录不可写等 IO 失败一律 Err 传播，库层不再 expect panic。

use crate::error::{Error, Result};
use crate::ledger::LedgerStore;
use crate::patterns::norm;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// vault 会话目录：`{data_dir}/vault/<session>`
pub fn vault_session_dir(session_id: &str) -> PathBuf {
    crate::manifest::data_dir().join("vault").join(session_id)
}

fn ensure_unique(parent: &Path, file_name: &Path) -> PathBuf {
    let target = parent.join(file_name);
    if !target.exists() {
        return target;
    }
    let stem = file_name.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = file_name.extension().map(|s| format!(".{}", s.to_string_lossy()));
    for n in 1..u32::MAX {
        let cand = match &ext {
            Some(e) => parent.join(format!("{stem}.dup{n}{e}")),
            None => parent.join(format!("{stem}.dup{n}")),
        };
        if !cand.exists() {
            return cand;
        }
    }
    unreachable!("unique name space exhausted")
}

/// 单批搬运结果：成功 (原路径, vault 副本路径, 实测字节)；失败 (原路径, 错误说明)。
pub type StashOutcome = (Vec<(PathBuf, PathBuf, u64)>, Vec<(PathBuf, String)>);

/// 重解析点/云占位/离线文件检测（OneDrive、DeDupe 占位等）。
/// 打开或复制这类文件会触发海量云端水合下载，搬运一律拒绝。
fn is_reparse_or_placeholder(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    match fs::symlink_metadata(p) {
        Ok(m) => m.file_attributes()
            & (FILE_ATTRIBUTE_REPARSE_POINT
                | FILE_ATTRIBUTE_OFFLINE
                | FILE_ATTRIBUTE_RECALL_ON_OPEN
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
            != 0,
        Err(_) => true, // 拿不准就不动（保守）
    }
}

/// 副本实际字节数（目录=整棵子树求和）。记账用实测值而非扫描时快照，
/// 否则「活目录」（D3DSCache/temp 在扫描→清理窗口内仍会增长）造成
/// vault 实重 > 台账，撤销/彻底删除的对账永远差一截。
/// v5：显式栈迭代，深目录零递归爆栈风险。
pub fn actual_size(p: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack: Vec<PathBuf> = vec![p.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let meta = match fs::symlink_metadata(&cur) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            if let Ok(rd) = fs::read_dir(&cur) {
                for e in rd.filter_map(|e| e.ok()) {
                    stack.push(e.path());
                }
            }
        } else {
            total += meta.len();
        }
    }
    total
}

/// 文件 copy 并保留 mtime（跨盘回退路径；副本时间线不应骗过后缀过滤）。
fn copy_preserving_mtime(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::copy(src, dst)?;
    if let Ok(m) = fs::metadata(src) {
        if let Ok(t) = m.modified() {
            if let Ok(f) = fs::File::options().write(true).open(dst) {
                let _ = f.set_times(fs::FileTimes::new().set_modified(t));
            }
        }
    }
    Ok(())
}

/// 把一批文件/目录移入 vault（不带台账 journal；仅供测试与特殊路径使用，
/// 交互清理一律走 [`stash_journal`]，保证「禁止台账外无主副本」不变量）。
pub fn stash(session_dir: &Path, sources: &[&Path]) -> Result<StashOutcome> {
    stash_core(session_dir, sources, None)
}

/// journal 化搬运：move 前落 manifest+pending entries，逐条成功 UPDATE
/// committed，失败撤行；全败（零 committed）时抹掉 manifest 行。
pub fn stash_journal(
    session_dir: &Path,
    sources: &[&Path],
    ledger: &LedgerStore,
    session_id: &str,
) -> Result<StashOutcome> {
    stash_core(session_dir, sources, Some((ledger, session_id)))
}

fn stash_core(
    session_dir: &Path,
    sources: &[&Path],
    journal: Option<(&LedgerStore, &str)>,
) -> Result<StashOutcome> {
    fs::create_dir_all(session_dir)
        .map_err(|e| Error::Other(format!("无法创建 vault 会话目录 {}: {e}", session_dir.display())))?;

    // 同一父下按字典序保证 deterministic
    let mut sorted: Vec<&Path> = sources.to_vec();
    sorted.sort_by_key(|p| norm(p));

    // 预排 dst（journal 要求 move 前全量落账；同一父桶内保证唯一）
    let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut plan: Vec<(PathBuf /*src*/, PathBuf /*dst*/)> = Vec::with_capacity(sorted.len());
    for (idx, src) in sorted.iter().enumerate() {
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".into());
        let dst_parent = session_dir.join(format!("{:04}", idx / 512));
        let mut dst = ensure_unique(&dst_parent, Path::new(&name));
        while !taken.insert(norm(&dst)) {
            let stem = Path::new(&name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = Path::new(&name)
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy()));
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            dst = match &ext {
                Some(e) => dst_parent.join(format!("{stem}.dup{n}{e}")),
                None => dst_parent.join(format!("{stem}.dup{n}")),
            };
        }
        plan.push((src.to_path_buf(), dst));
    }

    if let Some((ledger, id)) = journal {
        let ents: Vec<(String, String)> = plan
            .iter()
            .map(|(s, d)| (s.display().to_string(), d.display().to_string()))
            .collect();
        ledger.begin_session(id, crate::executor::CleanMode::Vault, crate::scanner::now_unix(), &ents)?;
    }

    let mut ok: Vec<(PathBuf, PathBuf, u64)> = Vec::new();
    let mut failed: Vec<(PathBuf, String)> = Vec::new();
    let mut infra_err: Option<std::io::Error> = None;
    let mut it = plan.into_iter().peekable();
    while let Some((src, dst)) = it.next() {
        // 桶目录惰性建；建不动 = 基础设施故障，剩余 pending 全部撤账
        let mk = match dst.parent() {
            Some(p) => fs::create_dir_all(p),
            None => Ok(()),
        };
        if let Err(e) = mk {
            journal_abandon(journal, &src);
            for (s2, _) in it {
                journal_abandon(journal, s2.as_path());
            }
            infra_err = Some(e);
            break;
        }
        let moved = move_one(src.as_path(), dst.as_path());
        match moved {
            Ok(()) => {
                let size = actual_size(dst.as_path());
                if let Some((ledger, id)) = journal {
                    if let Err(e) =
                        ledger.mark_entry_committed(id, &src.display().to_string(), size)
                    {
                        // 台账写不回：副本留在 vault、journal 仍 pending——
                        // GC 三保险会保护它；如实上抛，绝不假装成功
                        return Err(Error::Other(format!(
                            "journal 提交失败（会话 {id}）: {e}"
                        )));
                    }
                }
                ok.push((src, dst, size));
            }
            Err(e) => {
                failed.push((src.clone(), e));
                journal_abandon(journal, src.as_path());
            }
        }
    }
    if let Some(e) = infra_err {
        return Err(Error::Other(format!("vault 桶目录创建失败: {e}")));
    }

    // 全败收尾：一条都没搬动 → 空批次不留账（S3/孤儿名单洁净），
    // 本轮自建的空桶目录一并收走，绝不让「台账外空壳」进 GC 视野。
    if ok.is_empty() {
        if let Some((ledger, id)) = journal {
            ledger.drop_session_if_no_entries(id)?;
            let _ = fs::remove_dir_all(session_dir);
        }
    }
    Ok((ok, failed))
}

fn journal_abandon(journal: Option<(&LedgerStore, &str)>, src: &Path) {
    if let Some((ledger, id)) = journal {
        let _ = ledger.abandon_entry(id, &src.display().to_string());
    }
}

/// 单项搬运。两条硬约束（v5 不变）：
///
/// - 目录只允许原子 rename（占用即败，绝不「递归复制+删源」劈半）；
/// - 文件允许 copy 回退（跨盘），但删源失败必须回滚副本。
///
/// 另：重解析/占位/离线文件不做 copy 回退（云水合风暴防护），直接记 failed。
fn move_one(src: &Path, dst: &Path) -> std::result::Result<(), String> {
    let moved = match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) if src.is_dir() => Err(std::io::Error::other(
            "目录被占用或需跨盘，暂存区只做原子搬移，已原样保留",
        )),
        Err(first) => {
            if is_reparse_or_placeholder(src) {
                Err(std::io::Error::other(format!(
                    "重解析/云占位/离线文件不支持跨盘暂存（原名 {first}）"
                )))
            } else {
                copy_preserving_mtime(src, dst)
                    .and_then(|()| fs::remove_file(src))
                    .inspect_err(|_| {
                        // 删源失败：回滚副本，数据完整留在原位
                        let _ = fs::remove_file(dst);
                    })
            }
        }
    };
    moved.map_err(|e| e.to_string())
}

fn copy_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    // 显式栈迭代（v5：消灭递归爆栈面）；copy 保留 mtime
    let mut stack: Vec<(PathBuf, PathBuf)> = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((s, d)) = stack.pop() {
        if s.is_dir() {
            fs::create_dir_all(&d)?;
            for entry in fs::read_dir(&s)? {
                let e = entry?;
                stack.push((e.path(), d.join(e.file_name())));
            }
        } else {
            copy_preserving_mtime(&s, &d)?;
        }
    }
    Ok(())
}

/// 从 vault 还原到原位；目标被占用时改名 .dupN。
/// 入参为 (origin, in_vault) 对。
pub fn restore(moved: &[(PathBuf, PathBuf)]) -> (usize, Vec<(PathBuf, String)>) {
    let mut done = 0;
    let mut failed = Vec::new();
    for (origin, in_vault) in moved {
        if !in_vault.exists() {
            failed.push((origin.clone(), "vault 副本缺失".into()));
            continue;
        }
        if origin.exists() {
            // 目标已出现同名：不覆盖，改存 .restored 副本
            let dup = ensure_unique(origin.parent().unwrap_or(Path::new(".")), &PathBuf::from(
                origin.file_name().unwrap_or_default(),
            ));
            if let Err(e) = safe_move(in_vault, &dup) {
                failed.push((origin.clone(), e.to_string()));
                continue;
            }
        } else if let Err(e) = safe_move(in_vault, origin) {
            failed.push((origin.clone(), e.to_string()));
            continue;
        }
        done += 1;
    }
    (done, failed)
}

fn safe_move(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        return Err(std::io::Error::other(format!("目标已存在: {}", to.display())));
    }
    if from.is_file() && is_reparse_or_placeholder(from) {
        return Err(std::io::Error::other("重解析/占位副本不支持跨盘回拷"));
    }
    fs::rename(from, to).or_else(|_| {
        copy_all(from, to)?;
        if from.is_dir() {
            fs::remove_dir_all(from)
        } else {
            fs::remove_file(from)
        }
    })
}

/// 过期清扫摘要。
#[derive(Debug, Clone, Copy)]
pub struct SweepSummary {
    pub sessions: usize,
    pub items: usize,
    pub bytes: u64,
    /// v5：本轮孤儿 GC 是否因台账读取失败被熔断（审计 S1 防线可观测化）。
    pub gc_skipped: bool,
}

fn unix_of(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(i64::MAX)
}

/// 7 天后悔期到期清扫：删除 vault 内副本并抹台账行（history 统计保留）。
/// 单批失败（文件被占用等）不阻塞其余批次，该批台账保留待下次再扫。
/// 由应用启动后台线程 / `zclean sweep` 调用，绝不在交互路径上等它。
pub fn sweep_expired(max_age_days: u64) -> std::result::Result<SweepSummary, String> {
    let store = LedgerStore::open().map_err(|e| e.to_string())?;
    sweep_with_store(&store, max_age_days)
}

fn sweep_with_store(
    store: &LedgerStore,
    max_age_days: u64,
) -> std::result::Result<SweepSummary, String> {
    let cutoff = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64)
        - (max_age_days as i64) * 86_400
        // days=0 语义为"立即收走全部已落账批次"：同秒新建的 created_unix 恰等于
        // now，严格小于会把它漏在同一秒外，抬一格包含；days>0 维持原边界。
        + if max_age_days == 0 { 1 } else { 0 };
    let batches = store
        .expired_vault_batches(cutoff)
        .map_err(|e| format!("读取过期批次: {e}"))?;

    let mut summary = SweepSummary { sessions: 0, items: 0, bytes: 0, gc_skipped: false };
    for (id, total, copies) in batches {
        let mut all_ok = true;
        let mut deleted = 0usize;
        for (_, rel, _) in &copies {
            if rel.is_empty() {
                continue;
            }
            let p = PathBuf::from(rel);
            if !p.exists() {
                // 副本已不在（如此前整批还原过）：目标状态已达成
                deleted += 1;
                continue;
            }
            let r = if p.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
            match r {
                Ok(()) => deleted += 1,
                Err(_) => {
                    all_ok = false;
                    break; // 本批保留，下次启动再试
                }
            }
        }
        if all_ok {
            let _ = fs::remove_dir_all(vault_session_dir(&id));
            // drop 失败则保留账目待下次（副本已删，下次 exists()=false 视为已达成）
            if store.drop_manifest(&id).is_ok() {
                summary.sessions += 1;
                summary.items += deleted;
                summary.bytes += total;
            }
        }
    }

    // 无主会话目录 GC：vault 下存在、但台账里已无对应批次的目录
    // （多为半删除残留/异常中断产物）。台账仍存在的目录绝不动。
    summary.gc_skipped = gc_orphan_session_dirs(store, &mut summary);
    Ok(summary)
}

/// 孤儿会话目录 GC（三保险，返回是否被熔断）。台账条目仍存在的目录绝不动。
fn gc_orphan_session_dirs(store: &LedgerStore, summary: &mut SweepSummary) -> bool {
    // 保险 1：名单读取失败 → 本轮 GC 整体熔断。吞错变空名单 = 把全部
    // 合法暂存当孤儿删光（审计 S1 的灾难链）。
    let live: std::collections::HashSet<String> = match store.live_manifest_ids() {
        Ok(v) => v.into_iter().collect(),
        Err(_) => return true,
    };
    // 保险 3：journal 未完成（仍带 pending 条目）的会话目录绝不删除。
    let pending: std::collections::HashSet<String> = match store.pending_session_ids() {
        Ok(v) => v.into_iter().collect(),
        Err(_) => return true,
    };
    let vault_root = crate::manifest::data_dir().join("vault");
    let now = unix_of(SystemTime::now());
    if let Ok(rd) = fs::read_dir(&vault_root) {
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().to_string();
            if live.contains(&name) || pending.contains(&name) {
                continue;
            }
            // 保险 2：新建/崩溃窗口内的目录给 24h 宽限期；mtime 拿不准也不删
            match e.metadata().and_then(|m| m.modified()) {
                Ok(t) if now.saturating_sub(unix_of(t)) >= 86_400 => {}
                _ => continue,
            }
            if fs::remove_dir_all(e.path()).is_ok() {
                summary.sessions += 1;
            }
        }
    }
    false
}
