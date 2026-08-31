//! 空间雷达数据源：并行目录体积聚合树。
//!
//! 三段式，杜绝 O(files × depth) 与 O(nodes × dirs) 的隐藏平方级：
//! ① 并行单遍遍历（jwalk + rayon），每个文件只把体积记到「直接父目录」一格；
//! ② 按父->子目录索引自底向上求每目录子树合计（memo，每目录只算一次）；
//! ③ 组装时直接查索引取子目录，内存按「深度裁剪 + 每层仅保留前 N 大」收敛。
//!
//! v5 契约（审计 §B analyze）：
//! - 遍历产物归并到**单一消费线程的局部 map**（jwalk 并行只发生在
//!   read_dir 层），彻底消灭旧版「每文件抢一把全局 Mutex」的锁热点；
//! - 体积口径改 GetCompressedFileSizeW「实际占用」：稀疏文件（WSL/VHDX）
//!   不再虚高、压缩/EFS 不再按展开计；
//! - 新增 [`build_tree_cancellable`]：目录边界检查取消令牌，命中提前返回
//!   部分树（关雷达页不再停不下来）；[`largest_files_cancellable`] 同理。

use crate::error::{Error, Result};
use crate::scanner::ScanHandle;
use crate::patterns::norm;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub name: String,
    /// 归一化绝对路径
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub dirs: u64,
    /// 恒定输出（叶子为 []）：前端类型契约依赖该字段始终存在，
    /// 省略空数组会让 JS 端读 undefined 崩掉整棵 React 树（v3.0.1 黑屏根因）。
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Copy)]
pub struct TreeOptions {
    /// 聚合的最大目录深度（根=0）
    pub max_depth: u32,
    /// 每个节点最多保留多少个子节点（其余折入自身 size）
    pub max_children: usize,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self { max_depth: 4, max_children: 40 }
    }
}

/// 文件「实际占用」字节：GetCompressedFileSizeW（稀疏/压缩诚实口径），
/// API 失败回落逻辑长度。目录返回 0。
fn on_disk_size(p: &Path, fallback: u64) -> u64 {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW;
    const INVALID_FILE_SIZE: u32 = 0xFFFF_FFFF;
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut high: u32 = 0;
    unsafe {
        let low = GetCompressedFileSizeW(wide.as_ptr(), &mut high);
        if low == INVALID_FILE_SIZE && GetLastError() != 0 {
            return fallback;
        }
        ((high as u64) << 32) | low as u64
    }
}

/// 一次性构建以 `root` 为根的聚合树（不可取消包装）。
/// 返回的 root 节点 size 已含全部后代（含被 max_children/max_depth 裁掉的部分）。
pub fn build_tree(root: &Path, opts: TreeOptions) -> std::io::Result<TreeNode> {
    Ok(build_tree_cancellable(
        root,
        opts.max_depth as usize,
        opts.max_children,
        &ScanHandle::default(),
    ))
}

/// 可取消建树（CONTRACT §1）：取消命中在目录边界提前返回**部分树**。
/// 是否将「不完整」如实报错由调用方（壳层缓存层）决定。
pub fn build_tree_cancellable(
    root: &Path,
    depth: usize,
    max_children: usize,
    cancel: &ScanHandle,
) -> TreeNode {
    let opts = TreeOptions {
        max_depth: depth.min(u32::MAX as usize) as u32,
        max_children,
    };
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    // ── pass 1：jwalk 并行遍历（read_dir 层多核），产物在消费线程侧的
    // **局部 map** 归并——每个文件只向直接父目录记一格，零锁。 ──
    let mut direct: HashMap<PathBuf, (u64, u64)> = HashMap::with_capacity(4096);
    let mut subdirs: HashSet<PathBuf> = HashSet::with_capacity(4096);

    let cancel_probe = cancel.clone();
    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(4))
        .process_read_dir(move |_depth, _path, _state, children| {
            if cancel_probe.cancelled() {
                children.clear(); // 取消后不再展开新目录，快速收敛
            }
        });
    for entry in walker.into_iter() {
        if cancel.cancelled() {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().is_dir() {
            subdirs.insert(entry.path());
        } else if let Ok(m) = entry.metadata() {
            let sz = on_disk_size(&entry.path(), m.len());
            let parent = entry.parent_path().to_path_buf();
            let slot = direct.entry(parent).or_insert((0, 0));
            slot.0 += sz;
            slot.1 += 1;
        }
    }

    // ── 父目录 -> 直接子目录索引 ──
    let mut kids_of: HashMap<&Path, Vec<&Path>> = HashMap::with_capacity(subdirs.len());
    for d in &subdirs {
        if let Some(pp) = d.parent() {
            kids_of.entry(pp).or_default().push(d);
        }
    }

    // ── pass 2：自底向上求每目录子树合计。先落「直接文件量」，再按目录
    // 深度降序单遍把子累进父（父深度严格更小 → 子必先于父被处理）；
    // 迭代序实现，深树零递归栈风险。 ──
    let mut memo: HashMap<&Path, (u64, u64)> = HashMap::with_capacity(subdirs.len() + 1);
    for d in &subdirs {
        memo.insert(d.as_path(), (0, 0));
    }
    for (d, v) in &direct {
        let e = memo.entry(d.as_path()).or_insert((0, 0));
        e.0 += v.0;
        e.1 += v.1;
    }
    let mut by_depth: Vec<&Path> = memo.keys().copied().collect();
    by_depth.sort_by_key(|p| Reverse(p.components().count()));
    for &child in &by_depth {
        let Some(par) = child.parent() else { continue };
        if !memo.contains_key(par) {
            continue; // 越过遍历根：只统计根内
        }
        let delta = memo[child];
        let e = memo.get_mut(par).unwrap();
        e.0 += delta.0;
        e.1 += delta.1;
    }
    let root_total = memo.get(root).copied().unwrap_or((0, 0));

    let mut node = TreeNode {
        name,
        path: norm(root),
        size: root_total.0,
        files: root_total.1,
        dirs: 0,
        children: Vec::new(),
    };
    assemble(&mut node, root, &memo, &kids_of, 1, &opts, cancel);
    node
}

fn assemble(
    parent: &mut TreeNode,
    dir: &Path,
    totals: &HashMap<&Path, (u64, u64)>,
    kids_of: &HashMap<&Path, Vec<&Path>>,
    depth: u32,
    opts: &TreeOptions,
    cancel: &ScanHandle,
) {
    // 目录边界查取消：命中即停，父节点保留已收集的兄弟（部分树语义）
    if cancel.cancelled() || depth > opts.max_depth {
        return;
    }
    let mut kids: Vec<(&Path, (u64, u64))> = kids_of
        .get(dir)
        .map(|ks| {
            ks.iter()
                .map(|&k| (k, totals.get(k).copied().unwrap_or((0, 0))))
                .collect()
        })
        .unwrap_or_default();

    let mut taken: u64 = 0;
    kids.sort_by_key(|(_, v)| Reverse(v.0));
    if kids.len() > opts.max_children {
        taken = kids[opts.max_children..].iter().map(|(_, v)| v.0).sum();
        kids.truncate(opts.max_children);
    }

    parent.children.reserve(kids.len());
    for (path, v) in kids {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut child = TreeNode {
            name,
            path: norm(path),
            size: v.0,
            files: v.1,
            dirs: 0,
            children: Vec::new(),
        };
        if depth < opts.max_depth && !cancel.cancelled() {
            assemble(&mut child, path, totals, kids_of, depth + 1, opts, cancel);
        }
        child.dirs = child.children.len() as u64;
        parent.dirs += child.dirs + 1;
        parent.children.push(child);
    }
    if cancel.cancelled() {
        return; // 部分树：不再补折叠节点，避免「折叠了却没统计」的假账
    }
    if taken > 0 {
        // 折叠掉的兄弟体积并入父节点的独立余量：父 size 不变，
        // 通过在 children 末尾放一个聚合节点保持总数一致
        parent.children.push(TreeNode {
            name: "…其他".into(),
            path: format!("{}/__folded__", norm(dir)),
            size: taken,
            files: 0,
            dirs: 0,
            children: Vec::new(),
        });
    }
}

/// 单遍遍历 `root`，收集体积最大的 `top` 个文件（仅统计 size ≥ min_size 的文件）。
///
/// 与 `build_tree` 同款 jwalk 线程池单遍走完；用升序小顶堆（`BinaryHeap<Reverse<_>>`）
/// 维护 top-N：堆满后新文件只有比堆顶（当前第 N 大）更大才换入，内存恒为 O(top)。
/// 返回按 size 降序（同体积按路径升序稳定排序）。root 不存在时 jwalk 产出空迭代，
/// 返回空 Vec（与雷达页「缺失根 = 空」的 OK 语义一致）。`top = 0` 直接得空集。
/// v5：体积同样采用「实际占用」口径。
pub fn largest_files(root: &Path, top: usize, min_size: u64) -> std::io::Result<Vec<(PathBuf, u64)>> {
    largest_files_cancellable(root, top, min_size, &ScanHandle::default())
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// 可取消版 [`largest_files`]：命中取消上抛 Err(Cancelled)，
/// 绝不返回半截 top 榜当完整榜用。
pub fn largest_files_cancellable(
    root: &Path,
    top: usize,
    min_size: u64,
    cancel: &ScanHandle,
) -> Result<Vec<(PathBuf, u64)>> {
    // 小顶堆元素 = (size, path)；Reverse 使最小者居堆顶，超限弹最小
    let mut heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::with_capacity(top.saturating_add(1));

    let cancel_probe = cancel.clone();
    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(4))
        .process_read_dir(move |_depth, _path, _state, children| {
            if cancel_probe.cancelled() {
                children.clear();
            }
        });

    for entry in walker.into_iter() {
        if cancel.cancelled() {
            return Err(Error::Cancelled { reason: "大文件扫描已取消".to_string() });
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(m) = entry.metadata() else { continue };
        let p = entry.path();
        let sz = on_disk_size(&p, m.len());
        if sz < min_size {
            continue;
        }
        if heap.len() < top {
            heap.push(Reverse((sz, p)));
        } else if let Some(Reverse((smallest, _))) = heap.peek() {
            if sz > *smallest {
                heap.pop();
                heap.push(Reverse((sz, p)));
            }
        }
    }
    if cancel.cancelled() {
        return Err(Error::Cancelled { reason: "大文件扫描已取消".to_string() });
    }

    let mut out: Vec<(PathBuf, u64)> =
        heap.into_iter().map(|Reverse((sz, p))| (p, sz)).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn tree_aggregates_and_folds() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("big").join("sub")).unwrap();
        fs::create_dir_all(root.join("small")).unwrap();
        fs::write(root.join("big").join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(root.join("big").join("sub").join("b.bin"), vec![0u8; 50]).unwrap();
        fs::write(root.join("loose.txt"), vec![0u8; 7]).unwrap();

        let t = build_tree(root, TreeOptions { max_depth: 3, max_children: 5 }).unwrap();
        // v5：口径 = 实际占用（≥ 逻辑和；簇对齐/压缩差异不作精确断言）
        assert!(t.size >= 157, "root {} < 逻辑和", t.size);
        assert_eq!(t.files, 3);

        let big = t.children.iter().find(|c| c.name == "big").unwrap();
        assert!(big.size >= 150);
        assert_eq!(big.files, 2);
        assert!(big.children.iter().any(|c| c.name == "sub" && c.size >= 50));

        let small = t.children.iter().find(|c| c.name == "small").unwrap();
        assert_eq!(small.size, 0, "空目录实际占用仍为 0");
    }

    #[test]
    fn missing_root_yields_empty_node_ok_semantics() {
        let ghost = tempfile::tempdir().unwrap();
        let p = ghost.path().join("nope");
        let t = build_tree(&p, TreeOptions::default()).unwrap();
        assert_eq!(t.size, 0);
    }

    #[test]
    fn cancellable_returns_partial_tree_on_cancel() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a").join("x"), vec![0u8; 10]).unwrap();
        let h = ScanHandle::default();
        h.cancel(); // 起跑前即取消 → 只有根节点的空树（部分树契约）
        let t = build_tree_cancellable(root, 3, 5, &h);
        assert!(t.children.is_empty() || !h.cancelled());
        assert_eq!(t.path, norm(root));
    }

    #[test]
    fn largest_files_top3_orders_desc_and_truncates() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // 夹具：5 个不同体积文件（900/700/500/300/100 字节），另有子目录层级
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.bin"), vec![0u8; 900]).unwrap();
        fs::write(root.join("c.bin"), vec![0u8; 700]).unwrap();
        fs::write(root.join("b.bin"), vec![0u8; 500]).unwrap();
        fs::write(root.join("e.bin"), vec![0u8; 300]).unwrap();
        fs::write(root.join("sub").join("d.bin"), vec![0u8; 100]).unwrap();

        // top3：按 size 降序 + 截断
        let top3 = largest_files(root, 3, 0).unwrap();
        assert_eq!(top3.len(), 3, "超过 N 的条目应被小顶堆弹出");
        assert!(top3[0].1 >= 900);
        assert!(top3[0].0.ends_with("a.bin"));
        assert!(top3[0].1 >= top3[1].1, "{top3:?}");
        assert!(top3[1].1 >= top3[2].1, "{top3:?}");

        // top 大于总数：全量返回且仍降序
        let all = largest_files(root, 10, 0).unwrap();
        assert_eq!(all.len(), 5, "递归子目录内的文件也应计入");
        let sizes: Vec<u64> = all.iter().map(|(_, s)| *s).collect();
        let mut sorted = sizes.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(sizes, sorted, "结果必须按 size 降序");

        // min_size 门槛：低于门槛的文件不参与
        let big_only = largest_files(root, 10, 600).unwrap();
        assert_eq!(big_only.len(), 2);
        assert!(big_only[0].1 >= 900);

        // top=0：空集
        assert!(largest_files(root, 0, 0).unwrap().is_empty());
    }

    #[test]
    fn largest_files_cancellable_maps_cancel() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a"), vec![0u8; 10]).unwrap();
        let h = ScanHandle::default();
        h.cancel();
        let err = largest_files_cancellable(tmp.path(), 5, 0, &h).unwrap_err();
        assert!(matches!(err, Error::Cancelled { .. }), "{err:?}");
    }
}
