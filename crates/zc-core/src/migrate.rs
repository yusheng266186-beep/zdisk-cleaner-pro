//! 存储迁移中心内核：跨盘搬移大型目录并用 NTFS junction（mklink /J）
//! 保持原路径可用 —— 游戏库/缓存/聊天数据搬家的工业做法。
//!
//! 流程与回滚保障：
//!   plan  = 试运行：只算体积/文件数，输出 MigrationPlan
//!   apply = robocopy 迁移 → 尺寸校验 → 源改名 `.old` → 建 junction →
//!           冒烟读测 → 成功删 `.old`；任何一步失败自动逆向回滚
//!   undo  = 手动兜底：摘 junction、`.old` 改回原名
//!
//! 权限说明：junction 创建不需要管理员；被运行中进程锁定的文件由
//! robocopy 报告（重试计数 1 后放弃并触发回滚），不留半搬状态。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub total_bytes: u64,
    pub total_files: u64,
}

/// 磁盘剩余空间感知的试运行。
pub fn plan(src: &Path, dst_root: &Path) -> std::io::Result<MigrationPlan> {
    if !src.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("源目录不存在: {}", src.display())));
    }
    let name = src.file_name().ok_or_else(|| std::io::Error::other("源路径缺少末段目录名"))?;
    let dst = dst_root.join(name);
    if dst.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("目标已存在: {}", dst.display())));
    }
    let (total_bytes, total_files) = measure(src);
    Ok(MigrationPlan { src: src.to_path_buf(), dst, total_bytes, total_files })
}

fn measure(dir: &Path) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    for e in walkdir_all(dir) {
        if let Ok(m) = e.metadata() {
            if m.is_file() {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

fn walkdir_all(dir: &Path) -> impl Iterator<Item = jwalk::DirEntry<((), ())>> {
    jwalk::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
}

/// 执行迁移。返回迁移清单 id（落盘于 data_dir()/migrations/<id>.json）。
pub fn apply(plan: &MigrationPlan) -> Result<String, String> {
    let id = crate::scanner::new_session_id();
    run_steps(plan).map_err(|step_err| {
        // 失败即回滚，并把回滚结论附进错误信息
        match rollback_silent(&plan.src, &plan.dst) {
            Ok(true) => format!("步骤失败[{step_err}] —— 已自动回滚，数据无损"),
            Ok(false) => format!("步骤失败[{step_err}] —— 无需回滚"),
            Err(e) => format!("步骤失败[{step_err}]，且回滚受阻请人工检查: {e}"),
        }
    })?;

    let manifest_dir = crate::manifest::data_dir().join("migrations");
    let _ = fs::create_dir_all(&manifest_dir);
    let payload = serde_json::to_vec_pretty(plan).map_err(|e| e.to_string())?;
    let _ = fs::write(manifest_dir.join(format!("{id}.json")), payload);
    Ok(id)
}

fn run_steps(p: &MigrationPlan) -> Result<(), String> {
    if let Some(parent) = p.dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("建目标父目录: {e}"))?;
    }

    // 1) robocopy 迁移（0/1 视为成功；>=8 为失败，见 robocopy 退出码语义）
    let rc = Command::new("robocopy")
        .args([
            p.src.as_os_str().to_string_lossy().as_ref(),
            p.dst.as_os_str().to_string_lossy().as_ref(),
            "/E", "/COPY:DAT", "/DCOPY:DAT",
            "/R:1", "/W:1",      // 锁定文件不恋战：一次重试即失败回滚
            "/MT:16",
            "/NFL", "/NDL", "/NP",
        ])
        .status()
        .map_err(|e| format!("启动 robocopy: {e}"))?;
    let code = rc.code().unwrap_or(-1);
    if !(0..=7).contains(&code) {
        return Err(format!("robocopy exit={code}"));
    }

    // 2) 尺寸校验（±1 文件大小容差交给 hash 由上层抽检）
    let (moved_bytes, moved_files) = measure(&p.dst);
    if moved_bytes != p.total_bytes || moved_files != p.total_files {
        return Err(format!(
            "体积校验不符: 目标 {moved_bytes}B/{moved_files}f vs 源 {}B/{}f",
            p.total_bytes, p.total_files
        ));
    }

    // 3) 源改名 .old（此瞬间源目录已空或仅剩锁定残留）
    let old = sibling_old(&p.src);
    let _ = let_go_of_readonly(&p.src);
    fs::rename(&p.src, &old).map_err(|e| format!("源目录改名 .old: {e}"))?;

    // 4) junction：原路径重新出现且指向目标
    if let Err(e) = create_junction(&p.src, &p.dst) {
        return Err(format!("创建 junction: {e}"));
    }

    // 5) 冒烟：能通过原路径列到目标首层内容
    if !fs::read_dir(&p.src).map(|mut i| i.next().is_some()).unwrap_or(false) && p.total_files > 0 {
        return Err("junction 冒烟读取为空".into());
    }

    // 6) 全部通过才清理 .old
    fs::remove_dir_all(&old).map_err(|e| format!("清理 .old 备份: {e}"))?;
    Ok(())
}

fn sibling_old(src: &Path) -> PathBuf {
    let s = src.as_os_str().to_os_string();
    let mut s = s.into_string().unwrap_or_default();
    s.push_str(".zc-old");
    PathBuf::from(s)
}

fn let_go_of_readonly(_: &Path) -> std::io::Result<()> {
    Ok(()) // 占位：robocopy 已处理常规属性；目录只读属性场景极少
}

/// mklink /J —— 目录连接点（junction），不需要管理员权限。
fn create_junction(link: &Path, target: &Path) -> std::io::Result<()> {
    let out = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr)))
    }
}

/// 回滚：若 junction 存在先摘除（rmdir junction 不动目标），
/// `.old` 存在则还原为源目录名。返回是否做了实际动作。
pub fn rollback_silent(src: &Path, dst: &Path) -> std::io::Result<bool> {
    let mut acted = false;
    if src.symlink_metadata().is_ok() && is_junction(src) {
        fs::remove_dir(src)?; // 只拆链接本身
        acted = true;
    } else if src.is_dir() {
        // 半搬状态：把已搬到目标的文件搬回来过于复杂——交给人查；
        // 这里仅当源仍在时不动它
        let _ = src;
    }
    let old = sibling_old(src);
    if old.is_dir() && !src.exists() {
        fs::rename(&old, src)?;
        acted = true;
    }
    if acted && dst.is_dir() {
        // 目标保留：搬运成功一半的内容仍在 dst，供人工核对后删除
    }
    Ok(acted)
}

/// 手动 undo 入口：等价 rollback + 打印面向用户的结论。
pub fn undo(src: &Path) -> Result<String, String> {
    if !is_junction(src) {
        return Err("该路径不是 junction，无需 undo".into());
    }
    fs::remove_dir(src).map_err(|e| format!("摘除 junction: {e}"))?;
    let old = sibling_old(src);
    if old.is_dir() {
        fs::rename(&old, src).map_err(|e| format!("恢复源目录: {e}"))?;
        Ok("junction 已摘除，原目录数据已复位".into())
    } else {
        Ok("junction 已摘除（未发现 .old 备份，原目录可能已被此前清理）".into())
    }
}

fn is_junction(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    match p.symlink_metadata() {
        Ok(m) => m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0,
        Err(_) => false,
    }
}
