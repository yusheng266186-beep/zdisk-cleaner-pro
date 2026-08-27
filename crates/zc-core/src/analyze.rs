//! 空间雷达数据源：并行目录体积聚合树。
//!
//! 单遍遍历（与扫描引擎同款 jwalk 线程池），边走边把每个条目的
//! 大小累加到其所有祖先节点；内存按「深度裁剪 + 每层仅保留前 N 大」
//! 双上限收敛，保证 UI 可渲染。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub name: String,
    /// 归一化绝对路径
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub dirs: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
/// 返回的 root 节点 size 已含全部后代。
pub fn build_tree(root: &Path, opts: TreeOptions) -> std::io::Result<TreeNode> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    // path -> (累计字节, 文件数, 目录数)；单独 files 原子计数兜底防重复进 map 的开销
    let sizes: std::sync::Mutex<HashMap<PathBuf, (u64, u64)>> =
        std::sync::Mutex::new(HashMap::with_capacity(4096));
    let files_seen = AtomicU64::new(0);

    for entry in jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            let p: PathBuf = entry.path();
            sizes.lock().expect("tree mutex").entry(p).or_insert((0, 0)).1 += 0;
        } else if let Ok(m) = entry.metadata() {
            let p: PathBuf = entry.path();
            let sz = m.len();
            files_seen.fetch_add(1, Ordering::Relaxed);
            // 把大小累加到每个祖先（含自身所在目录）
            let mut anc = Some(p.as_path());
            while let Some(cur) = anc {
                let mut g = sizes.lock().expect("tree mutex");
                let slot = g.entry(cur.to_path_buf()).or_insert((0, 0));
                slot.0 += sz;
                slot.1 += 1;
                drop(g);
                anc = cur.parent();
            }
        }
    }

    let mut map = sizes.into_inner().expect("tree mutex");
    let root_entry = map.remove(root).unwrap_or((0, 0));
    let mut node = TreeNode {
        name,
        path: crate::patterns::norm(root),
        size: root_entry.0,
        files: root_entry.1,
        dirs: 0,
        children: Vec::new(),
    };
    assemble(&mut node, root, &mut map, 1, opts);
    Ok(node)
}

fn assemble(
    parent: &mut TreeNode,
    dir: &Path,
    map: &mut HashMap<PathBuf, (u64, u64)>,
    depth: u32,
    opts: TreeOptions,
) {
    if depth > opts.max_depth {
        return;
    }
    // 收集直接子目录
    let kids: Vec<(PathBuf, (u64, u64))> = map
        .iter()
        .filter(|(p, _)| p.parent().map(|pp| pp == dir).unwrap_or(false))
        .map(|(p, v)| (p.clone(), *v))
        .collect();

    let mut taken: u64 = 0;
    let mut kept: Vec<(PathBuf, (u64, u64))> = kids;
    kept.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
    if kept.len() > opts.max_children {
        let hidden = &kept[opts.max_children..];
        taken = hidden.iter().map(|(_, v)| v.0).sum();
        kept.truncate(opts.max_children);
    }

    parent.children.reserve(kept.len());
    for (path, v) in kept {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut child = TreeNode {
            name,
            path: crate::patterns::norm(&path),
            size: v.0,
            files: v.1,
            dirs: 0,
            children: Vec::new(),
        };
        if depth < opts.max_depth {
            assemble(&mut child, &path, map, depth + 1, opts);
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
}
