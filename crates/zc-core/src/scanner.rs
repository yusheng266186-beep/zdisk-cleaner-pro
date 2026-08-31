//! Walk 引擎：全规则共享一次磁盘遍历（多 glob 单遍匹配），
//! 这是相对 v2「每规则独立走一遍树」的结构性提速来源。

use crate::error::{Error, Result};
use crate::models::{FileHit, Finding, ScanEvent, ScanReport};
use crate::patterns::{literal_root, norm};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use jwalk::WalkDir;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 每条规则最多记录多少个明细文件，超出部分进入 overflow 计数。
const MAX_HITS_PER_RULE: usize = 50_000;

/// 把一组「规则 → 通配模式」编译成可单遍匹配的集合。
pub struct RuleMatcher {
    set: GlobSet,
    /// globset 命中索引 → 所属规则 id
    glob_rule_ids: Vec<String>,
}

impl RuleMatcher {
    pub fn build(entries: &[(String /*rule_id*/, String /*pattern*/)]) -> Result<Self> {
        let mut b = GlobSetBuilder::new();
        let mut ids = Vec::with_capacity(entries.len());
        for (rule_id, pat) in entries {
            let glob = GlobBuilder::new(pat)
                .literal_separator(true)
                .case_insensitive(true)
                .build()
                .map_err(|source| Error::Glob {
                    pattern: pat.clone(),
                    source,
                })?;
            b.add(glob);
            ids.push(rule_id.clone());
        }
        let set = b.build().map_err(|e| Error::Other(format!("{e}")))?;
        Ok(Self { set, glob_rule_ids: ids })
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    fn matched_rule_ids(&self, path_norm: &str) -> Vec<&str> {
        self.set
            .matches(path_norm)
            .into_iter()
            .filter_map(|i| self.glob_rule_ids.get(i).map(|s| s.as_str()))
            .collect()
    }
}

/// 扫描取消句柄：UI 侧持 Arc 引用，随时置位。
#[derive(Clone, Default)]
pub struct ScanHandle(Arc<AtomicBool>);

impl ScanHandle {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    /// 扫描开始时清除残留的取消标记。令牌生命周期跟随一次扫描——
    /// 否则取消过一次后，同一进程内的所有后续扫描都会立即自取消
    /// （前端表现为「扫描失败」，本会话再也无法体检）。
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 生成会话 id：<unix 秒>-<纳秒熵十六进制>。
pub fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.subsec_nanos() ^ (d.as_secs() as u32)) as u64)
        .unwrap_or(0);
    format!("{}-{:08x}", now_unix(), nanos & 0xFFFF_FFFF)
}

/// 对若干已展开的「模式 → 根」集合做一次并行遍历。
///
/// * `pairs`: (rule_id, 完整模式的归一化串)
/// * 报告只包含真实观察到的事实；进度回调可选。
pub fn scan(
    pairs: &[(String, String)],
    handle: &ScanHandle,
    mut on_event: impl FnMut(ScanEvent),
) -> Result<ScanReport> {
    handle.reset(); // 上一次扫描的取消标记不得泄漏到这一次
    let matcher = RuleMatcher::build(pairs)?;
    // 根去重：不同规则的多个模式可能共享同一字面根
    let roots: BTreeMap<String, ()> = pairs
        .iter()
        .map(|(_, p)| literal_root(p))
        .filter(|r| r.len() >= 3 && Path::new(r).is_dir())
        .map(|r| (r, ()))
        .collect();

    let started = std::time::Instant::now();
    let files_seen = AtomicU64::new(0);
    let bytes_seen = AtomicU64::new(0);
    let findings: std::sync::Mutex<BTreeMap<String, Finding>> = Default::default();
    // 目录 -> 子树累计字节（含未直接命中的后代），供目录级命中做诚实口径
    let dir_sizes: std::sync::Mutex<HashMap<PathBuf, u64>> =
        std::sync::Mutex::new(HashMap::with_capacity(2048));

    for root in roots.keys() {
        if handle.cancelled() {
            break;
        }
        let root_path = PathBuf::from(root);
        for entry in WalkDir::new(&root_path)
            .follow_links(false) // junction/symlink 不下钻，防循环 + 防越界
            .skip_hidden(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if handle.cancelled() {
                break;
            }
            let is_dir = entry.file_type().is_dir();
            let p: PathBuf = entry.path();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size = if is_dir { 0 } else { meta.len() };
            files_seen.fetch_add(1, Ordering::Relaxed);
            bytes_seen.fetch_add(size, Ordering::Relaxed);

            if !is_dir {
                let mut anc: Option<&Path> = Some(p.as_path());
                while let Some(a) = anc {
                    *dir_sizes.lock().expect("dir_sizes").entry(a.to_path_buf()).or_insert(0) += size;
                    anc = a.parent();
                }
            }

            let n = norm(&p);
            for rule_id in matcher.matched_rule_ids(&n) {
                add_hit(
                    &findings,
                    rule_id,
                    FileHit { path: p.clone(), size, is_dir },
                );
            }

            if files_seen.load(Ordering::Relaxed).is_multiple_of(4096) {
                on_event(ScanEvent::Entry {
                    files: files_seen.load(Ordering::Relaxed),
                    bytes_seen: bytes_seen.load(Ordering::Relaxed),
                });
            }
        }
    }

    on_event(ScanEvent::Done {
        files: files_seen.load(Ordering::Relaxed),
        bytes_seen: bytes_seen.load(Ordering::Relaxed),
    });

    let dir_map = dir_sizes.into_inner().expect("dir_sizes");
    let mut findings: Vec<Finding> = findings
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .into_values()
        .collect();
    for f in &mut findings {
        for h in &mut f.hits {
            if h.is_dir {
                if let Some(v) = dir_map.get(&h.path) {
                    h.size = *v;
                }
            }
        }
        dedup_nested(&mut f.hits);
    }
    findings.retain(|f| f.total_count() > 0);
    findings.sort_by_key(|f| std::cmp::Reverse(f.total_bytes()));

    Ok(ScanReport {
        id: new_session_id(),
        started_unix: now_unix(),
        duration_ms: started.elapsed().as_millis() as u64,
        files_seen: files_seen.load(Ordering::Relaxed),
        bytes_seen: bytes_seen.load(Ordering::Relaxed),
        cancelled: handle.cancelled(),
        findings,
    })
}

fn add_hit(
    map: &std::sync::Mutex<BTreeMap<String, Finding>>,
    rule_id: &str,
    hit: FileHit,
) {
    let mut m = map.lock().expect("scan mutex poisoned");
    let f = m.entry(rule_id.to_string()).or_insert_with(|| Finding::new(rule_id));
    if f.hits.len() < MAX_HITS_PER_RULE {
        f.hits.push(hit);
    } else {
        f.overflow_hits += 1;
        f.overflow_bytes += hit.size;
    }
}

/// 同一条规则内，「父目录已命中」时其子孙条目不再重复计数
/// （修复 v1/v2 家族的重复统计缺陷）。hits 排序后前缀贪心即可。
pub fn dedup_nested(hits: &mut Vec<FileHit>) {
    hits.sort_by(|a, b| a.path.as_os_str().cmp(b.path.as_os_str()));
    let mut subsumed_prefix: Option<String> = None;
    let mut kept: Vec<FileHit> = Vec::with_capacity(hits.len());
    for h in hits.drain(..) {
        let n = norm(&h.path);
        if let Some(pref) = &subsumed_prefix {
            if n.starts_with(pref.as_str()) {
                continue; // 已被命中的父目录覆盖
            }
        }
        // 目录命中会吞掉后续同前缀条目（'/' 收尾保证前缀边界正确）
        if h.is_dir {
            subsumed_prefix = Some(format!("{}/", n));
        }
        kept.push(h);
    }
    *hits = kept;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_nested_removes_children_of_matched_dirs() {
        let mk = |p: &str, s: u64, d: bool| FileHit { path: PathBuf::from(p), size: s, is_dir: d };
        let mut hits = vec![
            mk(r"C:\t\cache\a.tmp", 10, false),
            mk(r"C:\t\Cache\b.tmp", 20, false),
            mk(r"C:\t\Cache", 0, true), // 目录本身被命中
            mk(r"C:\t\other.bin", 30, false),
        ];
        dedup_nested(&mut hits);
        assert_eq!(hits.len(), 2);
        let lower: Vec<String> = hits
            .iter()
            .map(|h| h.path.to_string_lossy().to_lowercase())
            .collect();
        assert!(lower.contains(&"c:\\t\\cache".to_string()));
        assert!(lower.contains(&"c:\\t\\other.bin".to_string()));
    }
}
