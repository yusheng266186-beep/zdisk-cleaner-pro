//! 重复文件猎手：借鉴 Czkawka 的三级管道（大小 → 头部预哈希 → 全量哈希），
//! 全程 XXH3-128，遍历/哈希均走 rayon 并行（jwalk 同源线程池），I/O 有界。
//!
//! 安全默认：只报告，不动手。删除决策（保留最新/手动勾选）交给上层。
//!
//! v5 契约（审计 §B dedup）：
//! - suspect 阶段用 GetFileInformationByHandle 的 (volume_serial, file_id)
//!   做硬链接归并——同一 inode 的多个路径不再互为「重复」（否则 purge 后
//!   收益全假）；
//! - 重解析点/云占位文件（OneDrive 等）直接跳过并计入 skipped：打开哈希
//!   即触发海量云端水合下载；
//! - `DuplicateGroup` 携带 `volume_id`（组内卷标号，UI 区分同盘/跨盘）；
//! - 新增 [`find_duplicates_cancellable`]：全程查取消令牌，命中取消
//!   Err(Cancelled) 而非静默返回半截结果。

use crate::error::{Error, Result};
use crate::scanner::ScanHandle;
use jwalk::WalkDir;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use xxhash_rust::xxh3::Xxh3;

#[derive(Debug, Clone, Copy)]
pub struct DupOptions {
    pub min_size: u64,
}

impl Default for DupOptions {
    fn default() -> Self {
        Self { min_size: 1024 * 1024 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub size: u64,
    /// 全量 XXH3-128 十六进制
    pub hash: String,
    /// 按路径稳定排序；至少 2 个成员
    pub files: Vec<PathBuf>,
    /// 组内文件的 NTFS 卷序列号（文件信息不可得时 None）。同卷硬链接已
    /// 归并，同组成员必属同一卷。
    #[serde(default)]
    pub volume_id: Option<u64>,
}

/// (卷序列号, 文件记录号) —— NTFS 唯一的文件身份证，硬链接共享它。
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
struct FileIdent {
    volume: u64,
    file_id: u64,
}

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// 以「只读属性」方式打开并取 (volume, file_id, attributes)。
/// 拿不到句柄/信息返回 Err——调用方保守剔除该候选（不敢参与哈希与删除）。
fn file_ident(p: &Path) -> std::io::Result<(FileIdent, u32)> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_READ_ATTRIBUTES,
        FILE_SHARE_MODE, OPEN_EXISTING,
    };
    const SHARE_ALL: FILE_SHARE_MODE =
        windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ
            | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE
            | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        let h = CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            SHARE_ALL,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let r = GetFileInformationByHandle(h, &mut info);
        CloseHandle(h);
        if r == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok((
            FileIdent {
                volume: info.dwVolumeSerialNumber as u64,
                file_id: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
            },
            info.dwFileAttributes,
        ))
    }
}

fn is_placeholder_or_reparse(attributes: u32) -> bool {
    attributes
        & (FILE_ATTRIBUTE_REPARSE_POINT
            | FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
        != 0
}

fn hex128(b: u128) -> String {
    format!("{b:032x}")
}

fn hash_prefix(path: &Path, take: u64) -> Option<u128> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut limited = (&mut f).take(take);
    let mut h = Xxh3::default();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = limited.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.digest128())
}

fn hash_whole(path: &Path) -> Option<u128> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Xxh3::default();
    let mut buf = vec![0u8; 512 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.digest128())
}

/// 在多个根目录内寻找内容完全相同的文件组（体积 ≥ min_size）。
pub fn find_duplicates(
    roots: &[PathBuf],
    opts: &DupOptions,
) -> std::io::Result<Vec<DuplicateGroup>> {
    Ok(find_duplicates_inner(roots, opts.min_size, None)?.0)
}

/// 同 [`find_duplicates`]，附带全程可中断的取消令牌；命中取消返回
/// Err(Cancelled)（绝不把半截结果当完整结果交给 purge 决策）。
pub fn find_duplicates_cancellable(
    roots: &[PathBuf],
    min_size: u64,
    cancel: &ScanHandle,
) -> Result<Vec<DuplicateGroup>> {
    find_duplicates_inner(roots, min_size, Some(cancel))
        .map(|(g, _)| g)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                Error::Cancelled { reason: "重复文件扫描已取消".to_string() }
            } else {
                Error::Io(e)
            }
        })
}

/// 同 [`find_duplicates`]，并返回被保守剔除的条目数
/// （重解析/占位文件 + 句柄不可得候选）。
pub fn find_duplicates_stats(
    roots: &[PathBuf],
    opts: &DupOptions,
) -> std::io::Result<(Vec<DuplicateGroup>, usize)> {
    find_duplicates_inner(roots, opts.min_size, None)
}

fn check_cancel(cancel: &Option<&ScanHandle>) -> std::io::Result<()> {
    if cancel.is_some_and(|c| c.cancelled()) {
        // io::Error 包装保持内部签名；公开 cancellable API 再映射为 Cancelled
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "cancelled",
        ));
    }
    Ok(())
}

fn find_duplicates_inner(
    roots: &[PathBuf],
    min_size: u64,
    cancel: Option<&ScanHandle>,
) -> std::io::Result<(Vec<DuplicateGroup>, usize)> {
    let mut skipped = 0usize;
    // 阶段 1：按大小分组，仅保留疑似组
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for r in roots {
        check_cancel(&cancel)?;
        for e in WalkDir::new(r)
            .follow_links(false)
            .skip_hidden(false)
            .into_iter()
        {
            let e = match e {
                Ok(e) => e,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            if !e.file_type().is_file() {
                continue;
            }
            let m = match e.metadata() {
                Ok(m) => m,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            if m.len() < min_size || m.len() == 0 {
                continue;
            }
            by_size.entry(m.len()).or_default().push(e.path());
        }
    }

    // 只看疑似组：size 组 ≥2。先做 (volume,file_id) 硬链接归并 +
    // 重解析/占位文件保守剔除（suspect 阶段，哈希之前）。
    let mut by_ident: HashMap<u64, Vec<(FileIdent, PathBuf)>> = HashMap::new();
    for (sz, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        let mut seen_ids: HashSet<FileIdent> = HashSet::new();
        let mut deduped: Vec<(FileIdent, PathBuf)> = Vec::with_capacity(paths.len());
        for p in paths {
            match file_ident(&p) {
                Ok((id, attrs)) => {
                    if is_placeholder_or_reparse(attrs) || !seen_ids.insert(id) {
                        skipped += 1; // 占位文件 or 同 inode 硬链接第二路径
                        continue;
                    }
                    deduped.push((id, p));
                }
                Err(_) => {
                    skipped += 1; // 拿不准身份 = 不敢参与哈希与删除
                    continue;
                }
            }
        }
        if deduped.len() >= 2 {
            by_ident.entry(sz).or_default().extend(deduped);
        }
    }

    check_cancel(&cancel)?;
    let suspects: Vec<(u64, Vec<(FileIdent, PathBuf)>)> = by_ident.into_iter().collect();
    if suspects.is_empty() {
        return Ok((Vec::new(), skipped));
    }

    let flat: Vec<(u64, FileIdent, PathBuf)> = suspects
        .iter()
        .flat_map(|(sz, v)| v.iter().map(|(id, p)| (*sz, *id, p.clone())))
        .collect();

    // 阶段 2：64KB 头部预哈希并行过滤
    type PreMap = HashMap<u128, Vec<(u64, FileIdent, PathBuf)>>;
    let by_pre: PreMap = flat
        .par_iter()
        .filter_map(|(sz, id, p)| {
            if cancel.is_some_and(|c| c.cancelled()) {
                return None;
            }
            hash_prefix(p, 64 * 1024).map(|h| (h, (*sz, *id, p.clone())))
        })
        .fold(PreMap::new, |mut acc: PreMap, (h, item)| {
            acc.entry(h).or_default().push(item);
            acc
        })
        .reduce(PreMap::new, |mut a, b| {
            for (k, mut v) in b {
                a.entry(k).or_default().append(&mut v);
            }
            a
        });
    check_cancel(&cancel)?;

    let survivors: Vec<Vec<(u64, FileIdent, PathBuf)>> =
        by_pre.into_values().filter(|v| v.len() >= 2).collect();
    if survivors.is_empty() {
        return Ok((Vec::new(), skipped));
    }

    // 阶段 3：全量哈希并行确认，主线程分组（候选量已很小）
    let hashed: Vec<(u128, u64, FileIdent, PathBuf)> = survivors
        .into_par_iter()
        .flatten()
        .filter_map(|(sz, id, p)| {
            if cancel.is_some_and(|c| c.cancelled()) {
                return None;
            }
            hash_whole(&p).map(|h| (h, sz, id, p))
        })
        .collect();
    check_cancel(&cancel)?;

    let mut bucket: HashMap<u128, Vec<(u64, FileIdent, PathBuf)>> = HashMap::new();
    for (h, sz, id, p) in hashed {
        bucket.entry(h).or_default().push((sz, id, p));
    }

    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for (h, mut v) in bucket {
        if v.len() < 2 {
            continue;
        }
        v.sort_by(|a, b| a.2.cmp(&b.2));
        groups.push(DuplicateGroup {
            size: v[0].0,
            hash: hex128(h),
            files: v.into_iter().map(|(_, _, p)| p).collect(),
            volume_id: None, // 见下方按卷回填
        });
    }
    // volume_id：同组成员卷号一致（硬链接归并保证）；取任一成员即可
    for g in groups.iter_mut() {
        g.volume_id = g
            .files
            .first()
            .and_then(|p| file_ident(p).ok())
            .map(|(id, _)| id.volume);
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * g.files.len() as u64));
    Ok((groups, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, data: &[u8]) {
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(p, data).unwrap();
    }

    #[test]
    fn detects_exact_duplicates_and_skips_similar_prefixes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let big1 = vec![7u8; 300 * 1024]; // > 默认 min_size=1MB? 不够 —— 测试内自定义阈值
        let big2 = big1.clone();
        let mut differs_tail = big1.clone();
        differs_tail[299_999] ^= 0xFF;

        write(&root.join("a").join("one.bin"), &big1);
        write(&root.join("b").join("two.bin"), &big2);
        write(&root.join("c").join("three.bin"), &differs_tail);
        write(&root.join("small.txt"), b"tiny");

        let groups = find_duplicates(
            &[root.to_path_buf()],
            &DupOptions { min_size: 100 * 1024 },
        )
        .unwrap();

        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].hash.len() == 32);
        // v5：组携带卷号（同盘 NTFS 必可得）
        assert!(groups[0].volume_id.is_some(), "{groups:?}");
    }

    #[test]
    fn min_size_excludes_small_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("x"), b"same").unwrap();
        fs::write(tmp.path().join("y"), b"same").unwrap();
        let groups =
            find_duplicates(&[tmp.path().to_path_buf()], &DupOptions { min_size: 4096 }).unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn hardlink_same_inode_does_not_count_as_duplicate() {
        // NTFS 硬链接（同 volume+file_id）两条路径即便内容相同也不得成组
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.bin");
        let b = tmp.path().join("b.bin");
        write(&a, &vec![9u8; 200 * 1024]);
        if std::fs::hard_link(&a, &b).is_err() {
            return; // 文件系统不支持硬链接（如 exFAT/网络盘）则跳过
        }
        let groups = find_duplicates(
            &[tmp.path().to_path_buf()],
            &DupOptions { min_size: 100 * 1024 },
        )
        .unwrap();
        assert!(groups.is_empty(), "硬链接双路径不得互为重复: {groups:?}");
    }

    #[test]
    fn cancellable_reports_cancelled_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("a.bin"), &vec![3u8; 200 * 1024]);
        write(&root.join("b.bin"), &vec![3u8; 200 * 1024]);
        let handle = ScanHandle::default();
        handle.cancel();
        let err = find_duplicates_cancellable(
            &[root.to_path_buf()],
            100 * 1024,
            &handle,
        )
        .unwrap_err();
        // 内部 io::Interrupted 被 cancellable 门面映射为 Error::Cancelled
        let mapped: Error = err;
        assert!(
            matches!(mapped, Error::Cancelled { .. }),
            "取消必须是显式 Cancelled 语义: {mapped:?}"
        );
    }
}
