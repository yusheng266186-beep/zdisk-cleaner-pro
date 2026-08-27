//! vault 暂存区：把待删项移动到受管目录，保留 7 天可整批/单项还原。
//! 相比回收站模式：vault 在同盘是瞬时 rename，跨盘才真正拷贝，
//! 且不受 SHFileOperationW 的路径长度限制。

use crate::patterns::norm;
use std::fs;
use std::path::{Path, PathBuf};

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
    unreachable!("unique name space exhausted");
}

/// 单批搬运结果：成功 (原路径, vault 副本路径)；失败 (原路径, 错误说明)。
pub type StashOutcome = (Vec<(PathBuf, PathBuf)>, Vec<(PathBuf, String)>);

/// 把一批文件/目录移入 vault。
/// 返回 (成功, 失败列表)。目录内部自包含，直接整体搬移。
pub fn stash(session_dir: &Path, sources: &[&Path]) -> StashOutcome {
    fs::create_dir_all(session_dir).expect("vault session dir");
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    // 同一父下按字典序保证 deterministic
    let mut sorted: Vec<&Path> = sources.to_vec();
    sorted.sort_by_key(|p| norm(p));
    for src in sorted {
        // vault 内保持相对父目录的扁平编号，避免同名互踩
        let idx = ok.len();
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "unnamed".into());
        let dst_parent = session_dir.join(format!("{:04}", idx / 512));
        fs::create_dir_all(&dst_parent).expect("vault bucket dir");
        let dst = ensure_unique(&dst_parent, Path::new(&name));
        match fs::rename(src, &dst).or_else(|_| {
            // 跨卷 fallback：copy + remove（文件）；目录递归拷贝
            copy_all(src, &dst)?;
            fs::remove_dir_all(src)
                .or_else(|e| {
                    if src.is_file() { fs::remove_file(src).map_err(|_| e)?; Ok(()) } else { Err(e) }
                })
        }) {
            Ok(()) => ok.push((src.to_path_buf(), dst)),
            Err(e) => failed.push((src.to_path_buf(), e.to_string())),
        }
    }
    (ok, failed)
}

fn copy_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let e = entry?;
            copy_all(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
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
    fs::rename(from, to).or_else(|_| {
        copy_all(from, to)?;
        if from.is_dir() {
            fs::remove_dir_all(from)
        } else {
            fs::remove_file(from)
        }
    })
}
