//! Walk 引擎：全规则共享一次磁盘遍历（多 glob 单遍匹配），
//! 这是相对 v2「每规则独立走一遍树」的结构性提速来源。
//!
//! v5 契约（审计 §A）：
//! - jwalk 显式走 rayon 新线程池（兑现「并行 Walk」宣称，且规避共享池
//!   busy 时的静默空遍历）；
//! - 目录体积统计不再「每文件沿祖先链抢全局锁」：每个文件只向直接父目录
//!   记一格（遍历产物在消费线程侧即 per-walk 局部 map，零锁），结束后
//!   自底向上 O(dirs) 归并出子树合计——消灭 O(文件数×深度) 锁热点；
//! - 不可读条目计入 `ScanReport::skipped`（诚实口径，取消/受阻不再静默少报）；
//! - 溢出明细字节单列为 `honest_overflow_bytes`；
//! - `dedup_nested` 改按 norm 排序（修大小写混排时父+子双计数）；
//! - 规则级 `min_age_days` 在此消费：mtime 不早于阈值的命中剔除；
//!   目录命中对年龄规则不采信（防误删目录内新文件）；mtime 拿不准一律跳过（保守不删）。

use crate::error::{Error, Result};
use crate::models::{FileHit, Finding, ScanEvent, ScanReport};
use crate::patterns::{literal_root, norm};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use jwalk::WalkDir;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

    /// 命中的规则 id（去重：同一规则多条 pattern 同时命中一条路径时，
    /// 只算一次——防止 `%TEMP%/**` 与 `%LOCALAPPDATA%/Temp/**` 变体双计数）。
    pub fn matched_rule_ids(&self, path_norm: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for i in self.set.matches(path_norm) {
            if let Some(s) = self.glob_rule_ids.get(i).map(|s| s.as_str()) {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
        out
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
    ///
    /// v5 语义补充：进入 scan 前已被置位的句柄视为「起跑即作废」的
    /// 竞态取消意图，会被如实尊重（返回空 cancelled 报告），不复位。
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

/// 生成会话 id：<unix 秒>-<纳秒熵十六进制>。v5 起为公开 API：
/// 手写暂存批次 id 必须用它（秒级时间戳碰撞会整批覆盖台账）。
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
    on_event: impl FnMut(ScanEvent),
) -> Result<ScanReport> {
    scan_with_opts(pairs, &BTreeMap::new(), handle, on_event)
}

/// 同 [`scan`]，附带规则级最小年龄阈值（rule_id → 天）。
///
/// 年龄规则只收「mtime 严格早于阈值」的文件命中；目录命中一律不采信
/// （目录 mtime 不代表子孙年龄，整目录搬走会误删新文件）；
/// mtime 读取失败保守跳过。
pub fn scan_with_opts(
    pairs: &[(String, String)],
    min_age_days: &BTreeMap<String, u64>,
    handle: &ScanHandle,
    mut on_event: impl FnMut(ScanEvent),
) -> Result<ScanReport> {
    if handle.cancelled() {
        // 起跑前置位 = 竞态取消意图，尊重之：空发现、cancelled=true
        return Ok(ScanReport {
            id: new_session_id(),
            started_unix: now_unix(),
            duration_ms: 0,
            files_seen: 0,
            bytes_seen: 0,
            cancelled: true,
            findings: Vec::new(),
            skipped: 0,
            honest_overflow_bytes: 0,
        });
    }
    handle.reset(); // 上一次扫描的取消标记不得泄漏到这一次
    let matcher = RuleMatcher::build(pairs)?;
    // 根去重：不同规则的多个模式可能共享同一字面根
    let roots: BTreeMap<String, ()> = pairs
        .iter()
        .map(|(_, p)| literal_root(p))
        .filter(|r| r.len() >= 3 && Path::new(r).is_dir())
        .map(|r| (r, ()))
        .collect();

    // 年龄规则的统一阈值时刻（同一秒内构建，够精度）
    let age_cutoffs: BTreeMap<&str, SystemTime> = min_age_days
        .iter()
        .filter_map(|(k, days)| {
            let secs = now_unix().checked_sub(days.checked_mul(86_400)?)?;
            Some((k.as_str(), UNIX_EPOCH + Duration::from_secs(secs)))
        })
        .collect();

    let started = std::time::Instant::now();
    let mut files_seen: u64 = 0;
    let mut bytes_seen: u64 = 0;
    let mut skipped: usize = 0;
    // 单消费线程侧局部收集，无跨线程锁
    let mut findings: BTreeMap<String, Finding> = BTreeMap::new();
    // 目录 -> 直接文件字节和（仅直接子文件；子树合计最后自底向上归并）
    let mut direct: HashMap<PathBuf, u64> = HashMap::with_capacity(2048);
    // 遍历中观察到的全部目录（含扫描根），归并需要完整的父链
    let mut dirs: HashSet<PathBuf> = HashSet::with_capacity(2048);

    for root in roots.keys() {
        if handle.cancelled() {
            break;
        }
        let root_path = PathBuf::from(root);
        dirs.insert(root_path.clone());
        let cancel_probe = handle.clone();
        let walker = WalkDir::new(&root_path)
            .follow_links(false) // junction/symlink 不下钻，防循环 + 防越界
            .skip_hidden(false)
            // 显式 rayon 新池(4 线程封顶：兑现并行 Walk 同时把核留给 UI，QA 探针回归钉此)；新池无共享 busy 竞争，
            // 取消时 process_read_dir 清空待展队列实现快速止损
            .parallelism(jwalk::Parallelism::RayonNewPool(4))
            .process_read_dir(move |_depth, _path, _state, children| {
                if cancel_probe.cancelled() {
                    children.clear();
                }
            });
        for entry in walker.into_iter() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    // AccessDenied / IO 错误：如实计数（含目录整棵不可读）
                    skipped += 1;
                    continue;
                }
            };
            if handle.cancelled() {
                break;
            }
            let is_dir = entry.file_type().is_dir();
            let p: PathBuf = entry.path();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let size = if is_dir { 0 } else { meta.len() };
            files_seen += 1;
            bytes_seen += size;

            if is_dir {
                dirs.insert(p.clone());
            } else if let Some(par) = p.parent() {
                *direct.entry(par.to_path_buf()).or_insert(0) += size;
            }

            let n = norm(&p);
            let matched = matcher.matched_rule_ids(&n);
            if !matched.is_empty() {
                let mut mtime: Option<SystemTime> = None;
                for rule_id in matched {
                    if let Some(cutoff) = age_cutoffs.get(rule_id) {
                        if is_dir {
                            continue; // 年龄规则不整目录吞并
                        }
                        let t = match mtime {
                            Some(t) => t,
                            None => match meta.modified() {
                                Ok(t) => t,
                                Err(_) => {
                                    skipped += 1;
                                    continue; // 拿不准就不删
                                }
                            },
                        };
                        mtime = Some(t);
                        if t >= *cutoff {
                            continue; // 未满最小年龄
                        }
                    }
                    add_hit(&mut findings, rule_id, FileHit { path: p.clone(), size, is_dir });
                }
            }

            if files_seen.is_multiple_of(4096) {
                on_event(ScanEvent::Entry { files: files_seen, bytes_seen });
            }
        }
    }

    on_event(ScanEvent::Done { files: files_seen, bytes_seen });

    // ── 自底向上归并目录子树合计：每目录只被处理一次，O(dirs) ──
    let mut totals: HashMap<PathBuf, u64> = HashMap::with_capacity(dirs.len());
    for d in &dirs {
        totals.entry(d.clone()).or_insert(0);
    }
    for (d, v) in direct {
        *totals.entry(d).or_insert(0) += v;
    }
    let mut by_depth: Vec<PathBuf> = totals.keys().cloned().collect();
    by_depth.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for d in &by_depth {
        let v = totals[d];
        if v == 0 {
            continue;
        }
        if let Some(par) = d.parent() {
            if let Some(t) = totals.get_mut(par) {
                *t += v;
            }
        }
    }

    let mut findings: Vec<Finding> = findings.into_values().collect();
    let honest_overflow_bytes: u64 = findings.iter().map(|f| f.overflow_bytes).sum();
    for f in &mut findings {
        for h in &mut f.hits {
            if h.is_dir {
                if let Some(v) = totals.get(&h.path) {
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
        files_seen,
        bytes_seen,
        cancelled: handle.cancelled(),
        findings,
        skipped,
        honest_overflow_bytes,
    })
}

fn add_hit(map: &mut BTreeMap<String, Finding>, rule_id: &str, hit: FileHit) {
    let f = map.entry(rule_id.to_string()).or_insert_with(|| Finding::new(rule_id));
    if f.hits.len() < MAX_HITS_PER_RULE {
        f.hits.push(hit);
    } else {
        f.overflow_hits += 1;
        f.overflow_bytes += hit.size;
    }
}

/// 同一条规则内，「父目录已命中」时其子孙条目不再重复计数
/// （修复 v1/v2 家族的重复统计缺陷）。
///
/// v5 修正：排序键改用与吞并判定一致的 norm 串（此前按原始字节序排序、
/// 却用小写 norm 做前缀吞并——NTFS 同父下大小写混排名时子可排在父之前
/// 不被吞并，造成父+子双计数）。norm 全等去重同时兜住多 glob 双命中。
pub fn dedup_nested(hits: &mut Vec<FileHit>) {
    hits.sort_by_cached_key(|h| norm(&h.path));
    let mut kept: Vec<FileHit> = Vec::with_capacity(hits.len());
    let mut prev_norm: Option<String> = None;
    let mut subsumed_prefix: Option<String> = None;
    for h in hits.drain(..) {
        let n = norm(&h.path);
        if prev_norm.as_deref() == Some(n.as_str()) {
            continue; // 同一路径多模式重复命中
        }
        prev_norm = Some(n.clone());
        if let Some(pref) = &subsumed_prefix {
            if n.starts_with(pref.as_str()) {
                continue; // 已被命中的父目录覆盖
            }
        }
        // 目录命中会吞掉后续同前缀条目（'/' 收尾保证前缀边界正确）
        if h.is_dir {
            subsumed_prefix = Some(format!("{n}/"));
        }
        kept.push(h);
    }
    *hits = kept;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(p: &str, s: u64, d: bool) -> FileHit {
        FileHit { path: PathBuf::from(p), size: s, is_dir: d }
    }

    #[test]
    fn dedup_nested_removes_children_of_matched_dirs() {
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

    #[test]
    fn dedup_nested_mixed_case_parent_no_double_count() {
        // 回归（v5 审计）：文件排在大写目录前时，字节序排序会让其逃过吞并
        let mut hits = vec![
            mk(r"C:\T\cache\a.bin", 10, false), // 小写 cache：字节序先于 "C:\t\Cache"
            mk(r"C:\t\Cache", 100, true),
        ];
        dedup_nested(&mut hits);
        assert_eq!(hits.len(), 1, "父目录命中必须吞并大小写变体路径下的子文件");
        assert!(hits[0].is_dir);
    }

    #[test]
    fn dedup_nested_collapses_identical_norm_duplicates() {
        let mut hits = vec![
            mk(r"C:\t\Cache", 0, true),
            mk(r"C:\t\cache", 0, true),
            mk(r"C:\t\file", 5, false),
        ];
        dedup_nested(&mut hits);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn pre_cancelled_handle_yields_empty_cancelled_report() {
        let h = ScanHandle::default();
        h.cancel();
        let rep = scan(&[("r".to_string(), "**/x".to_string())], &h, |_| {}).unwrap();
        assert!(rep.cancelled);
        assert_eq!(rep.files_seen, 0);
        assert_eq!(rep.cleanable_count(), 0);
        // 竞态取消意图不被复位吃掉，但句柄仍可显式 reset 后复用
        h.reset();
        let rep2 = scan(&[], &h, |_| {}).unwrap();
        assert!(!rep2.cancelled);
    }

    #[test]
    fn min_age_filters_fresh_files_and_keeps_old() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let fresh = root.join("fresh.tmp");
        let old = root.join("old.tmp");
        let young_sub = root.join("sub").join("ageless.tmp");
        fs_touch(&fresh, 10);
        fs_touch(&old, 10);
        fs_touch(&young_sub, 10);
        backdate(&old, 10 * 86_400);

        let pairs = vec![("r".to_string(), format!("{}/**", norm(root)))];
        let mut ages = BTreeMap::new();
        ages.insert("r".to_string(), 7u64);
        let rep = scan_with_opts(&pairs, &ages, &ScanHandle::default(), |_| {}).unwrap();
        let paths: Vec<String> = rep
            .findings
            .iter()
            .flat_map(|f| f.hits.iter())
            .map(|h| norm(&h.path))
            .collect();
        assert!(paths.iter().any(|p| p.ends_with("/old.tmp")), "{paths:?}");
        assert!(!paths.iter().any(|p| p.ends_with("/fresh.tmp")), "{paths:?}");
        assert!(!paths.iter().any(|p| p.ends_with("/ageless.tmp")), "{paths:?}");
        // 年龄规则不得产生目录命中
        assert!(rep.findings.iter().all(|f| f.hits.iter().all(|h| !h.is_dir)));
    }

    #[test]
    fn scan_report_new_fields_serde_backward_compatible() {
        // 旧（v3/v4）报告 JSON 缺 skipped/honest_overflow_bytes → 必须可反序列化
        let legacy = r#"{"id":"x","started_unix":1,"duration_ms":2,"files_seen":3,
            "bytes_seen":4,"cancelled":false,"findings":[]}"#;
        let rep: ScanReport = serde_json::from_str(legacy).unwrap();
        assert_eq!(rep.skipped, 0);
        assert_eq!(rep.honest_overflow_bytes, 0);
    }

    fn fs_touch(p: &Path, n: u64) {
        if let Some(par) = p.parent() {
            std::fs::create_dir_all(par).unwrap();
        }
        std::fs::write(p, vec![0u8; n as usize]).unwrap();
    }

    fn backdate(p: &Path, secs: u64) {
        use std::fs::FileTimes;
        let t = UNIX_EPOCH + Duration::from_secs(now_unix().saturating_sub(secs));
        let f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
        f.set_times(FileTimes::new().set_modified(t)).unwrap();
    }
}
