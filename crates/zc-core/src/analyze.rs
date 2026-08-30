//! 空间雷达数据源：并行目录体积聚合树。
//!
//! 三段式，杜绝 O(files × depth) 与 O(nodes × dirs) 的隐藏平方级：
//! ① 并行单遍遍历（jwalk + rayon），每个文件只把体积记到「直接父目录」一格；
//! ② 按父->子目录索引自底向上求每目录子树合计（memo，每目录只算一次）；
//! ③ 组装时直接查索引取子目录，内存按「深度裁剪 + 每层仅保留前 N 大」收敛。

use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

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

/// 一次性构建以 `root` 为根的聚合树。
/// 返回的 root 节点 size 已含全部后代（含被 max_children/max_depth 裁掉的部分）。
pub fn build_tree(root: &Path, opts: TreeOptions) -> std::io::Result<TreeNode> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    // ── pass 1：并行单遍遍历，每文件只记「直接父目录」一格 ──
    // 旧实现把每个文件沿祖先链逐级累加（45 万文件 × 十几层 = 数百万次
    // 加锁 + 路径克隆 + 哈希探测），是雷达分析分钟级耗时的元凶。
    let direct: std::sync::Mutex<HashMap<PathBuf, (u64, u64)>> =
        std::sync::Mutex::new(HashMap::with_capacity(4096));
    let subdirs: std::sync::Mutex<HashSet<PathBuf>> =
        std::sync::Mutex::new(HashSet::with_capacity(4096));
    let files_seen = AtomicU64::new(0);

    for entry in jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: Duration::from_secs(600),
        })
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            subdirs.lock().expect("tree mutex").insert(entry.path());
        } else if let Ok(m) = entry.metadata() {
            let sz = m.len();
            files_seen.fetch_add(1, Ordering::Relaxed);
            let parent = entry.parent_path().to_path_buf();
            let mut g = direct.lock().expect("tree mutex");
            let slot = g.entry(parent).or_insert((0, 0));
            slot.0 += sz;
            slot.1 += 1;
        }
    }

    let subdirs = subdirs.into_inner().expect("tree mutex");
    let direct = direct.into_inner().expect("tree mutex");

    // ── 父目录 -> 直接子目录索引（每种关系只建一次，杜绝组装期全表扫描） ──
    let mut kids_of: HashMap<&Path, Vec<&Path>> = HashMap::with_capacity(subdirs.len());
    for d in &subdirs {
        if let Some(pp) = d.parent() {
            kids_of.entry(pp).or_default().push(d);
        }
    }

    // ── pass 2：自底向上求每目录子树合计（memo，每目录只算一次） ──
    fn totals<'a>(
        dir: &'a Path,
        direct: &HashMap<PathBuf, (u64, u64)>,
        kids_of: &HashMap<&'a Path, Vec<&'a Path>>,
        memo: &mut HashMap<&'a Path, (u64, u64)>,
    ) -> (u64, u64) {
        if let Some(v) = memo.get(dir) {
            return *v;
        }
        let mut acc = direct.get(dir).copied().unwrap_or((0, 0));
        if let Some(kids) = kids_of.get(dir) {
            for &k in kids {
                let t = totals(k, direct, kids_of, memo);
                acc.0 += t.0;
                acc.1 += t.1;
            }
        }
        memo.insert(dir, acc);
        acc
    }
    let mut memo: HashMap<&Path, (u64, u64)> = HashMap::with_capacity(subdirs.len());
    let root_total = totals(root, &direct, &kids_of, &mut memo);

    let mut node = TreeNode {
        name,
        path: crate::patterns::norm(root),
        size: root_total.0,
        files: root_total.1,
        dirs: 0,
        children: Vec::new(),
    };
    assemble(&mut node, root, &memo, &kids_of, 1, opts);
    Ok(node)
}

fn assemble(
    parent: &mut TreeNode,
    dir: &Path,
    totals: &HashMap<&Path, (u64, u64)>,
    kids_of: &HashMap<&Path, Vec<&Path>>,
    depth: u32,
    opts: TreeOptions,
) {
    if depth > opts.max_depth {
        return;
    }
    // 直接子目录（索引直查，不再全表过滤）
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
            path: crate::patterns::norm(path),
            size: v.0,
            files: v.1,
            dirs: 0,
            children: Vec::new(),
        };
        if depth < opts.max_depth {
            assemble(&mut child, path, totals, kids_of, depth + 1, opts);
        }
        child.dirs = child.children.len() as u64;
        parent.dirs += child.dirs + 1;
        parent.children.push(child);
    }
    if taken > 0 {
        // 折叠掉的兄弟体积并入父节点的独立余量：父 size 不变，
        // 通过在 children 末尾放一个聚合节点保持总数一致
        parent.children.push(TreeNode {
            name: "…其他".into(),
            path: format!("{}/__folded__", crate::patterns::norm(dir)),
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
pub fn largest_files(root: &Path, top: usize, min_size: u64) -> std::io::Result<Vec<(PathBuf, u64)>> {
    // 小顶堆元素 = (size, path)；Reverse 使最小者居堆顶，超限弹最小
    let mut heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::with_capacity(top.saturating_add(1));

    for entry in jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(m) = entry.metadata() else { continue };
        let sz = m.len();
        if sz < min_size {
            continue;
        }
        if heap.len() < top {
            heap.push(Reverse((sz, entry.path())));
        } else if let Some(Reverse((smallest, _))) = heap.peek() {
            if sz > *smallest {
                heap.pop();
                heap.push(Reverse((sz, entry.path())));
            }
        }
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
        assert_eq!(t.size, 157);
        assert_eq!(t.files, 3);

        let big = t.children.iter().find(|c| c.name == "big").unwrap();
        assert_eq!(big.size, 150);
        assert_eq!(big.files, 2);
        assert!(big.children.iter().any(|c| c.name == "sub" && c.size == 50));

        let small = t.children.iter().find(|c| c.name == "small").unwrap();
        assert_eq!(small.size, 0);
    }

    #[test]
    fn missing_root_yields_empty_node_ok_semantics() {
        let ghost = tempfile::tempdir().unwrap();
        let p = ghost.path().join("nope");
        let t = build_tree(&p, TreeOptions::default()).unwrap();
        assert_eq!(t.size, 0);
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
        assert_eq!(top3[0].1, 900);
        assert!(top3[0].0.ends_with("a.bin"));
        assert_eq!(top3[1].1, 700);
        assert_eq!(top3[2].1, 500);

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
        assert_eq!(big_only[0].1, 900);

        // top=0：空集
        assert!(largest_files(root, 0, 0).unwrap().is_empty());
    }
}
