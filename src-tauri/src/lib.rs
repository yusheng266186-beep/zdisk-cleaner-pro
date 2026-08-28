//! Tauri 壳层：IPC 命令桥。逻辑一律下沉 zc-core/zc-rules，这里只做搬运与事件转发。
//!
//! 事件契约：`scan://progress` payload = [files: u64, bytes_seen: u64]
//!
//! 注意：本文件依赖 tauri 2（需要 MSVC 工具链才能本地构建，见 ADR-001），
//! 与 `cargo test`（default-members 仅纯内核 crate）隔离。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{OnceLock};
use std::os::windows::ffi::OsStrExt;

use serde::Serialize;
use zc_core::{
    analyze,
    dedup,
    executor::{self, CleanMode},
    history::{self, HistoryRecord},
    manifest,
    migrate::{self, MigrationPlan},
    models::{Domain, Risk, ScanEvent, ScanReport},
    scanner,
    startup::{self, StartupEntry},
    ScanHandle,
};

static SCAN_HANDLE: OnceLock<ScanHandle> = OnceLock::new();

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            ping,
            scan_now,
            cancel_scan,
            clean_selected,
            undo_session,
            rules_meta,
            history_list,
            drives_overview,
            analyze_tree,
            startup_list,
            startup_disabled_count,
            startup_disable,
            startup_enable_all,
            migrate_plan,
            migrate_apply,
            migrate_undo,
            big_files,
            find_dupes,
            reveal_in_explorer
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn ping() -> String {
    format!("zc-core v{}", env!("CARGO_PKG_VERSION"))
}

/// 全量扫描：后台线程执行；进度经 `scan://progress` = [files, bytes] 推送；
/// 取消令牌由 cancel_scan 置位；结束报告同时落盘 sessions/ 目录。
#[tauri::command]
fn scan_now(app: tauri::AppHandle, include_admin: bool) -> Result<ScanReport, String> {
    let handle = SCAN_HANDLE.get_or_init(ScanHandle::default).clone();

    let pairs: Vec<(String, String)> = zc_rules::expand_all()
        .into_iter()
        .filter(|(id, _)| include_admin || !zc_rules::find(id).is_some_and(|r| r.admin_required))
        .collect();
    let app2 = app.clone();

    let reporter = std::thread::spawn(move || {
        scanner::scan(&pairs, &handle, move |ev| {
            if let ScanEvent::Entry { files, bytes_seen } = ev {
                use tauri::Emitter as _;
                let _ = app2.emit("scan://progress", vec![files, bytes_seen]);
            }
        })
    });

    let mut rep = reporter
        .join()
        .map_err(|_| "扫描线程崩溃".to_string())?
        .map_err(|e| e.to_string())?;
    zc_rules::filter_guards(&mut rep.findings);

    let dir = manifest::data_dir().join("sessions");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(
        dir.join(format!("{}.json", rep.id)),
        serde_json::to_vec_pretty(&rep).unwrap_or_default(),
    );
    Ok(rep)
}

#[tauri::command]
fn cancel_scan() {
    SCAN_HANDLE.get_or_init(ScanHandle::default).cancel();
}

#[derive(Serialize)]
struct RuleMetaDto {
    id: &'static str,
    name_zh: &'static str,
    domain: &'static str,
    risk: Risk,
    admin_required: bool,
}

#[tauri::command]
fn rules_meta() -> Vec<RuleMetaDto> {
    zc_rules::RULES
        .iter()
        .map(|r| RuleMetaDto {
            id: r.id,
            name_zh: r.name_zh,
            domain: match r.domain {
                Domain::System => "system",
                Domain::Browser => "browser",
                Domain::Dev => "dev",
                Domain::Apps => "apps",
                Domain::Logs => "logs",
            },
            risk: r.risk,
            admin_required: r.admin_required,
        })
        .collect()
}

#[tauri::command]
fn history_list() -> Vec<HistoryRecord> {
    history::read_all()
}

#[derive(Serialize)]
struct OutcomeDto {
    requested_files: u64,
    requested_bytes: u64,
    done_files: u64,
    done_bytes: u64,
    failed: Vec<(String, String)>,
    semantics_note: String,
}

#[tauri::command]
fn clean_selected(
    report: ScanReport,
    rule_ids: Vec<String>,
    mode: CleanMode,
) -> Result<OutcomeDto, String> {
    let outcome = executor::apply(&report, &rule_ids, mode).map_err(|e| e.to_string())?;
    let _ = history::append(&HistoryRecord {
        session_id: report.id.clone(),
        created_unix: scanner::now_unix(),
        mode,
        files: outcome.done_files,
        bytes_moved: outcome.done_bytes,
    });
    Ok(OutcomeDto {
        requested_files: outcome.requested_files,
        requested_bytes: outcome.requested_bytes,
        done_files: outcome.done_files,
        done_bytes: outcome.done_bytes,
        failed: outcome.failed,
        semantics_note: outcome.semantics_note.clone(),
    })
}

#[tauri::command]
fn undo_session(id: String) -> Result<String, String> {
    let m = manifest::CleanManifest::load(&id).map_err(|e| e.to_string())?;
    let (done, failed) = m.undo().map_err(|e| e.to_string())?;
    if failed.is_empty() {
        Ok(format!("已还原 {done}/{} 项", m.entries.len()))
    } else {
        Ok(format!(
            "已还原 {done}/{} 项，{} 项失败（首个原因：{}）",
            m.entries.len(),
            failed.len(),
            failed[0].1
        ))
    }
}

#[derive(Serialize)]
struct DriveDto {
    label: String,
    total_bytes: u64,
    free_bytes: u64,
}

#[tauri::command]
fn drives_overview() -> Vec<DriveDto> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain([0]).collect()
    }

    let mut out = Vec::new();
    for c in b'A'..=b'Z' {
        let root = format!("{}:\\", c as char);
        let root_w = wide(&root);
        let mut total: u64 = 0;
        let mut free: u64 = 0;
        unsafe {
            if GetDiskFreeSpaceExW(root_w.as_ptr(), std::ptr::null_mut(), &mut total, &mut free) != 0
            {
                out.push(DriveDto {
                    label: format!("{}:", c as char),
                    total_bytes: total,
                    free_bytes: free,
                });
            }
        }
    }
    out
}

/// 空间雷达：构建目录体积聚合树（并行单遍遍历 + 深度/宽度双上限收敛）。
/// path 为空时默认取用户主目录；depth 超过 6 时截断，防 UI 爆量。
#[tauri::command]
fn analyze_tree(path: String, depth: u32) -> Result<analyze::TreeNode, String> {
    let root = if path.is_empty() {
        std::env::var("USERPROFILE").map_err(|_| "无法定位用户主目录（USERPROFILE）".to_string())?
    } else {
        path
    };
    analyze::build_tree(
        Path::new(&root),
        analyze::TreeOptions { max_depth: depth.min(6), max_children: 40 },
    )
    .map_err(|e| e.to_string())
}

/* ── 大文件 / 重复文件猎手 ──────────────────────────────── */

#[derive(Serialize)]
struct BigFileDto {
    path: String,
    size: u64,
}

/// 大文件 Top-N：单遍 jwalk + 小顶堆截断，只报告 ≥1MB 的文件，不动手。
/// path 为空时默认取用户主目录；top 夹取 [1,200] 防 UI 爆量。
#[tauri::command]
fn big_files(path: String, top: u32) -> Result<Vec<BigFileDto>, String> {
    let root = if path.is_empty() {
        std::env::var("USERPROFILE").map_err(|_| "无法定位用户主目录（USERPROFILE）".to_string())?
    } else {
        path
    };
    let files = analyze::largest_files(Path::new(&root), top.max(1).min(200) as usize, 1024 * 1024)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(p, size)| BigFileDto { path: p.to_string_lossy().into_owned(), size })
        .collect();
    Ok(files)
}

/// 重复文件组：XXH3 三级哈希管道（大小 → 头部预哈希 → 全量哈希），只报告不动手。
#[tauri::command]
fn find_dupes(path: String, min_mb: u64) -> Result<Vec<dedup::DuplicateGroup>, String> {
    dedup::find_duplicates(
        &[PathBuf::from(path)],
        &dedup::DupOptions { min_size: min_mb * 1024 * 1024 },
    )
    .map_err(|e| e.to_string())
}

/* ── 启动项管家 ─────────────────────────────────────────── */

/// 枚举当前用户自启动项（HKCU Run / RunOnce，读操作无风险）。
#[tauri::command]
fn startup_list() -> Result<Vec<StartupEntry>, String> {
    startup::list_user_startup().map_err(|e| e.to_string())
}

/// 已禁用数量（本地备份 JSON 中的条目数），供页头徽章。
#[tauri::command]
fn startup_disabled_count() -> Result<usize, String> {
    Ok(startup::disabled_count())
}

/// 禁用单个启动项：注册表值移入本地备份 JSON，可随时恢复。
#[tauri::command]
fn startup_disable(key_id: String) -> Result<bool, String> {
    startup::disable(&key_id).map_err(|e| e.to_string())
}

/// 恢复全部被禁用项。返回成功写回的条数。
#[tauri::command]
fn startup_enable_all() -> Result<usize, String> {
    startup::enable_all().map_err(|e| e.to_string())
}

/* ── 存储迁移中心 ───────────────────────────────────────── */

/// 试运行：只测体积/文件数并生成计划，不搬任何文件。
#[tauri::command]
fn migrate_plan(src: String, dst_root: String) -> Result<MigrationPlan, String> {
    migrate::plan(Path::new(&src), Path::new(&dst_root)).map_err(|e| e.to_string())
}

fn migrate_phase_str(p: migrate::MigratePhase) -> &'static str {
    match p {
        migrate::MigratePhase::Copy => "copy",
        migrate::MigratePhase::Verify => "verify",
        migrate::MigratePhase::Link => "link",
        migrate::MigratePhase::Smoke => "smoke",
        migrate::MigratePhase::Cleanup => "cleanup",
    }
}

fn migrate_state_str(s: migrate::PhaseState) -> &'static str {
    match s {
        migrate::PhaseState::Start => "start",
        migrate::PhaseState::End => "end",
    }
}

/// 执行迁移：内部以当前参数重新 plan 后再 apply，
/// 防止用过期/被篡改的计划参数套用（fail-closed）。
///
/// 慢操作下沉后台线程，阶段推进经 `migrate://phase` 实时推送
/// （payload = [phase: snake_case 字符串, state: "start"|"end"]），
/// UI 显示的是内核真实步骤边界，不是估算进度。
#[tauri::command]
fn migrate_apply(app: tauri::AppHandle, src: String, dst_root: String) -> Result<String, String> {
    let plan = migrate::plan(Path::new(&src), Path::new(&dst_root)).map_err(|e| e.to_string())?;

    let app2 = app.clone();
    let worker = std::thread::spawn(move || {
        migrate::apply_with_phases(&plan, &mut |phase, state| {
            use tauri::Emitter as _;
            let _ = app2.emit(
                "migrate://phase",
                vec![migrate_phase_str(phase), migrate_state_str(state)],
            );
        })
    });

    worker.join().map_err(|_| "迁移线程崩溃".to_string())?
}

/// 手动兜底撤销：摘 junction 并把 `.old` 备份复位为源目录。
#[tauri::command]
fn migrate_undo(src: String) -> Result<String, String> {
    migrate::undo(Path::new(&src))
}

/* ── 空间雷达实用动作 ───────────────────────────────────── */

/// 在资源管理器中打开指定目录。仅校验存在性后拉起 explorer，
/// 进程句柄即弃（explorer 会转交已有窗口实例，退出码无意义）。
#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(format!("路径不存在：{path}"));
    }
    Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("无法启动资源管理器：{e}"))?;
    Ok(())
}
