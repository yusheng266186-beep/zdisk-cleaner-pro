//! Tauri 壳层：IPC 命令桥。逻辑一律下沉 zc-core/zc-rules，这里只做搬运与事件转发。
//!
//! 事件契约：`scan://progress` payload = [files: u64, bytes_seen: u64]
//!
//! 注意：本文件依赖 tauri 2（需要 MSVC 工具链才能本地构建，见 ADR-001），
//! 与 `cargo test`（default-members 仅纯内核 crate）隔离。

use std::path::Path;
use std::sync::{OnceLock};
use std::os::windows::ffi::OsStrExt;

use serde::Serialize;
use zc_core::{
    analyze,
    executor::{self, CleanMode},
    history::{self, HistoryRecord},
    manifest,
    models::{Domain, Risk, ScanEvent, ScanReport},
    scanner, ScanHandle,
};

static SCAN_HANDLE: OnceLock<ScanHandle> = OnceLock::new();

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ping,
            scan_now,
            cancel_scan,
            clean_selected,
            undo_session,
            rules_meta,
            history_list,
            drives_overview,
            analyze_tree
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
