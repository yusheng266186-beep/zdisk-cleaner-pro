//! 重复文件猎手：借鉴 Czkawka 的三级管道（大小 → 头部预哈希 → 全量哈希），
//! 全程 XXH3-128，遍历/哈希均走 rayon 并行（jwalk 同源线程池），I/O 有界。
//!
//! 安全默认：只报告，不动手。删除决策（保留最新/手动勾选）交给上层。

use jwalk::WalkDir;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    // 阶段 1：按大小分组，仅保留疑似组
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for r in roots {
        for e in WalkDir::new(r)
            .follow_links(false)
            .skip_hidden(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !e.file_type().is_file() {
                continue;
            }
            let m = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if m.len() < opts.min_size || m.len() == 0 {
                continue;
            }
            by_size.entry(m.len()).or_default().push(e.path());
        }
    }


    // 只看疑似组：size 组 ≥2
    let suspects: Vec<&Vec<PathBuf>> =
        by_size.values().filter(|v| v.len() >= 2).collect();
    if suspects.is_empty() {
        return Ok(Vec::new());
    }

    let flat: Vec<(u64, PathBuf)> = suspects
        .iter()
        .flat_map(|v| v.iter().map(|p| {
            let sz = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            (sz, p.clone())
        }))
        .collect();

    // 阶段 2：64KB 头部预哈希并行过滤
    type PreMap = HashMap<u128, Vec<(u64, PathBuf)>>;
    let by_pre: PreMap = flat
        .par_iter()
        .filter_map(|(sz, p)| hash_prefix(p, 64 * 1024).map(|h| (h, *sz, p.clone())))
        .fold(PreMap::new, |mut acc: PreMap, (h, sz, p)| {
            acc.entry(h).or_default().push((sz, p));
            acc
        })
        .reduce(PreMap::new, |mut a, b| {
            for (k, mut v) in b {
                a.entry(k).or_default().append(&mut v);
            }
            a
        });

    let survivors: Vec<Vec<(u64, PathBuf)>> =
        by_pre.into_values().filter(|v| v.len() >= 2).collect();
    if survivors.is_empty() {
        return Ok(Vec::new());
    }

    // 阶段 3：全量哈希并行确认，主线程分组（候选量已很小）
    let hashed: Vec<(u128, u64, PathBuf)> = survivors
        .into_par_iter()
        .flatten()
        .filter_map(|(sz, p)| hash_whole(&p).map(|h| (h, sz, p)))
        .collect();

    let mut bucket: HashMap<u128, Vec<(u64, PathBuf)>> = HashMap::new();
    for (h, sz, p) in hashed {
        bucket.entry(h).or_default().push((sz, p));
    }

    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for (h, mut v) in bucket {
        if v.len() < 2 {
            continue;
        }
        v.sort_by(|a, b| a.1.cmp(&b.1));
        groups.push(DuplicateGroup {
            size: v[0].0,
            hash: hex128(h),
            files: v.into_iter().map(|(_, p)| p).collect(),
        });
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * g.files.len() as u64));
    Ok(groups)
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
}
