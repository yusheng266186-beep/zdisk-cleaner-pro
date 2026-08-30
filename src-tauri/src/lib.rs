//! Tauri 壳层：IPC 命令桥。逻辑一律下沉 zc-core/zc-rules，这里只做搬运与事件转发。
//!
//! 事件契约：`scan://progress` payload = [files: u64, bytes_seen: u64]
//!
//! 注意：本文件依赖 tauri 2（需要 MSVC 工具链才能本地构建，见 ADR-001），
//! 与 `cargo test`（default-members 仅纯内核 crate）隔离。

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

/* ── 深度工具提权 worker 协议 ─────────────────────────────
 * 特权动作（DISM 组件清理 / 系统还原点）要求管理员令牌。策略：
 * 1. 命令在未提权时直接返回 Err（"需要管理员：…"），本次不做命令内自拉起，
 *    UI 层引导「以管理员重启应用」或走 CLI 的 `zclean apply --admin` 提权批；
 * 2. 自提权旁路由 worker 模式承担：用 powershell `Start-Process -Verb RunAs`
 *    以当前 exe（std::env::current_exe()）+ worker 参数拉起子进程（UAC 只授权
 *    这一次进程，无常驻服务），见下方 spawn_elevated_worker；
 * 3. worker 被拉起后走 run() 开头的 main() 前置早退分支：不启动窗口，
 *    直接执行特权动作、把结果打印到 stdout 供排查，然后退出。
 * 与 zc-cli::elevate 的 JobSpec 临时文件协议同思想、更简单（参数即任务）。 */

const DISM_WORKER_ARG: &str = "--dism-worker";
const RP_WORKER_ARG: &str = "--rp-worker";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // main() 前置早退分支：worker 模式不创建窗口，干完活即退出。
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == DISM_WORKER_ARG) {
        std::process::exit(dism_run(None));
    }
    if let Some(pos) = args.iter().position(|a| a == RP_WORKER_ARG) {
        let desc = args.get(pos + 1).cloned().unwrap_or_default();
        let code = match rp_run(&desc) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("[rp-worker] {e}");
                1
            }
        };
        std::process::exit(code);
    }

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
            reveal_in_explorer,
            system_occupancy,
            dism_component_cleanup,
            create_restore_point
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn ping() -> String {
    format!("zc-core v{}", env!("CARGO_PKG_VERSION"))
}

/// 全量扫描：spawn_blocking 执行（绝不上主线程，窗口/事件循环保持响应）；
/// 进度经 `scan://progress` = [files, bytes] 推送；
/// 取消令牌由 cancel_scan 置位；结束报告同时落盘 sessions/ 目录。
#[tauri::command]
async fn scan_now(app: tauri::AppHandle, include_admin: Option<bool>) -> Result<ScanReport, String> {
    let include_admin = include_admin.unwrap_or(false);
    let handle = SCAN_HANDLE.get_or_init(ScanHandle::default).clone();

    let pairs: Vec<(String, String)> = zc_rules::expand_all()
        .into_iter()
        .filter(|(id, _)| include_admin || !zc_rules::find(id).is_some_and(|r| r.admin_required))
        .collect();

    tauri::async_runtime::spawn_blocking(move || {
        let mut rep = scanner::scan(&pairs, &handle, move |ev| {
            if let ScanEvent::Entry { files, bytes_seen } = ev {
                use tauri::Emitter as _;
                let _ = app.emit("scan://progress", vec![files, bytes_seen]);
            }
        })
        .map_err(|e| e.to_string())?;
        zc_rules::filter_guards(&mut rep.findings);

        let dir = manifest::data_dir().join("sessions");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join(format!("{}.json", rep.id)),
            serde_json::to_vec_pretty(&rep).unwrap_or_default(),
        );
        Ok(rep)
    })
    .await
    .map_err(|_| "扫描后台任务失败".to_string())?
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
async fn clean_selected(
    report: ScanReport,
    rule_ids: Vec<String>,
    mode: CleanMode,
) -> Result<OutcomeDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|_| "清理后台任务失败".to_string())?
}

#[tauri::command]
async fn undo_session(id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|_| "撤销后台任务失败".to_string())?
}

#[derive(Serialize)]
struct DriveDto {
    label: String,
    total_bytes: u64,
    free_bytes: u64,
}

/// 已挂载盘符容量总览。用 GetLogicalDrivesW 先拿真实存在的盘符位图，
/// 只查询存在的盘（绝不裸探 A-Z，避免空光驱/残网络映射卡住）；
/// spawn_blocking 执行，不占主线程。
#[tauri::command]
async fn drives_overview() -> Vec<DriveDto> {
    tauri::async_runtime::spawn_blocking(|| {
        use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetLogicalDrives};

        fn wide(s: &str) -> Vec<u16> {
            std::ffi::OsStr::new(s).encode_wide().chain([0]).collect()
        }

        let bitmask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for c in b'A'..=b'Z' {
            if bitmask & (1u32 << (c - b'A')) == 0 {
                continue;
            }
            let root = format!("{}:\\", c as char);
            let root_w = wide(&root);
            let mut total: u64 = 0;
            let mut free: u64 = 0;
            unsafe {
                if GetDiskFreeSpaceExW(root_w.as_ptr(), std::ptr::null_mut(), &mut total, &mut free)
                    != 0
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
    })
    .await
    .unwrap_or_default()
}

/// 空间雷达：构建目录体积聚合树（并行单遍遍历 + 深度/宽度双上限收敛）。
/// path 为空时默认取用户主目录；depth 超过 6 时截断，防 UI 爆量。
/// spawn_blocking 执行——主目录遍历可达数十万文件，绝不能占主线程。
#[tauri::command]
async fn analyze_tree(path: String, depth: u32) -> Result<analyze::TreeNode, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = if path.is_empty() {
            std::env::var("USERPROFILE")
                .map_err(|_| "无法定位用户主目录（USERPROFILE）".to_string())?
        } else {
            path
        };
        analyze::build_tree(
            Path::new(&root),
            analyze::TreeOptions { max_depth: depth.min(6), max_children: 40 },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "雷达后台任务失败".to_string())?
}

/* ── 大文件 / 重复文件猎手 ──────────────────────────────── */

#[derive(Serialize)]
struct BigFileDto {
    path: String,
    size: u64,
}

/// 大文件 Top-N：单遍 jwalk + 小顶堆截断，只报告 ≥1MB 的文件，不动手。
/// path 为空时默认取用户主目录；top 夹取 [1,200] 防 UI 爆量。spawn_blocking 执行。
#[tauri::command]
async fn big_files(path: String, top: u32) -> Result<Vec<BigFileDto>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = if path.is_empty() {
            std::env::var("USERPROFILE")
                .map_err(|_| "无法定位用户主目录（USERPROFILE）".to_string())?
        } else {
            path
        };
        let files =
            analyze::largest_files(Path::new(&root), top.max(1).min(200) as usize, 1024 * 1024)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|(p, size)| BigFileDto { path: p.to_string_lossy().into_owned(), size })
                .collect();
        Ok(files)
    })
    .await
    .map_err(|_| "大文件后台任务失败".to_string())?
}

/// 重复文件组：XXH3 三级哈希管道（大小 → 头部预哈希 → 全量哈希），只报告不动手。
/// 哈希可能耗时数分钟，spawn_blocking 执行。
#[tauri::command]
async fn find_dupes(path: String, min_mb: u64) -> Result<Vec<dedup::DuplicateGroup>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        dedup::find_duplicates(
            &[PathBuf::from(path)],
            &dedup::DupOptions { min_size: min_mb * 1024 * 1024 },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "查重后台任务失败".to_string())?
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

/// 迁移计划：measure 需全树测量源目录体积，spawn_blocking 执行。
#[tauri::command]
async fn migrate_plan(src: String, dst_root: String) -> Result<MigrationPlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        migrate::plan(Path::new(&src), Path::new(&dst_root)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "迁移计划后台任务失败".to_string())?
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
/// spawn_blocking 执行（robocopy 可达数 GB，绝不上主线程），
/// 阶段推进经 `migrate://phase` 实时推送
/// （payload = [phase: snake_case 字符串, state: "start"|"end"]），
/// UI 显示的是内核真实步骤边界，不是估算进度。
#[tauri::command]
async fn migrate_apply(app: tauri::AppHandle, src: String, dst_root: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let plan =
            migrate::plan(Path::new(&src), Path::new(&dst_root)).map_err(|e| e.to_string())?;
        migrate::apply_with_phases(&plan, &mut |phase, state| {
            use tauri::Emitter as _;
            let _ = app.emit(
                "migrate://phase",
                vec![migrate_phase_str(phase), migrate_state_str(state)],
            );
        })
    })
    .await
    .map_err(|_| "迁移后台任务失败".to_string())?
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

/* ── 深度工具：系统占用 / WinSxS 组件清理 / 系统还原点 ──── */

/// 系统级占用盘点：直接透传 zc-core 的只读盘点（Windows.old 实测 +
/// hiberfil/pagefile/swapfile，ACL 拒绝即 size=None 诚实标「未知」）。
#[tauri::command]
async fn system_occupancy() -> Vec<zc_core::system::OccupancyItem> {
    tauri::async_runtime::spawn_blocking(zc_core::system::system_occupancy)
        .await
        .unwrap_or_default()
}

/// WinSxS 组件清理：要求管理员令牌，未提权直接拒绝，不再往下执行。
/// 本次不做命令内自提权拉起——UI 层引导「以管理员重启应用」，或走
/// zclean CLI 的提权批（`zclean apply --admin` 流程）；自提权旁路见
/// spawn_elevated_worker + run() 的 --dism-worker 早退分支。
/// 已提权则内联执行 worker 同一实现，DISM 真实百分比经
/// `dism://progress`（payload = f32）推给前端驱动确定进度条。
/// DISM 一跑数分钟，spawn_blocking 执行，窗口绝不冻结。
#[tauri::command]
async fn dism_component_cleanup(app: tauri::AppHandle) -> Result<(), String> {
    if !zc_core::is_elevated() {
        return Err("需要管理员：请在弹出的 UAC 中允许".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let code = dism_run(Some(&app));
        if code == 0 {
            Ok(())
        } else {
            Err(format!("dism.exe 退出码 {code}，详见控制台输出"))
        }
    })
    .await
    .map_err(|_| "DISM 后台任务失败".to_string())?
}

/// 系统还原点：同样要求管理员令牌（官方 Checkpoint-Computer 通道，不碰注册表野路子）。
/// --rp-worker <desc> 旁路由 run() 早退分支承接；两 worker 均打印结果到 stdout 供排查。
/// 还原点创建需数十秒，spawn_blocking 执行。
#[tauri::command]
async fn create_restore_point(desc: String) -> Result<(), String> {
    if !zc_core::is_elevated() {
        return Err("需要管理员：请在弹出的 UAC 中允许".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || rp_run(&desc))
        .await
        .map_err(|_| "还原点后台任务失败".to_string())?
}

/// DISM 组件清理共用执行体：命令内联路径（app=Some，emit 进度事件）与
/// --dism-worker 子进程（app=None，只打印 stdout 供排查）复用同一实现。
/// 返回 dism.exe 退出码；spawn 失败返回 -1。
fn dism_run(app: Option<&tauri::AppHandle>) -> i32 {
    println!("[dism-worker] dism.exe /Online /Cleanup-Image /StartComponentCleanup 启动…");
    let mut child = match Command::new("dism.exe")
        .args(["/Online", "/Cleanup-Image", "/StartComponentCleanup"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[dism-worker] dism.exe 拉起失败：{e}");
            return -1;
        }
    };

    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            // 全量回显 stdout，便于事后排查（worker 模式的唯一观测通道）
            println!("[dism] {line}");
            if let Some(pct) = parse_percent(&line) {
                if let Some(a) = app {
                    use tauri::Emitter as _;
                    let _ = a.emit("dism://progress", pct);
                }
            }
        }
    }

    match child.wait() {
        Ok(s) => {
            let code = s.code().unwrap_or(-1);
            println!("[dism-worker] 结束，退出码 {code}");
            code
        }
        Err(e) => {
            eprintln!("[dism-worker] 等待 dism.exe 失败：{e}");
            -1
        }
    }
}

/// 还原点创建共用执行体：spawn powershell Checkpoint-Computer 并等待结束。
/// 成功返回 Ok(())，失败返回 Err（命令退出码）。desc 经 PowerShell 单引号
/// 转义（'' = 字面单引号）防止注入。
fn rp_run(desc: &str) -> Result<(), String> {
    let desc = desc.replace('\'', "''");
    println!("[rp-worker] Checkpoint-Computer 启动（描述：{desc}）…");
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Checkpoint-Computer -Description '{desc}' -RestorePointType MODIFY_SETTINGS"),
        ])
        .status()
        .map_err(|e| format!("无法启动 powershell：{e}"))?;
    if status.success() {
        println!("[rp-worker] 还原点创建成功");
        Ok(())
    } else {
        let msg = format!("Checkpoint-Computer 退出码 {:?}", status.code());
        eprintln!("[rp-worker] {msg}");
        Err(msg)
    }
}

/// 与正则 r"(\d+(?:\.\d+)?)%" 等价的首尾扫描：返回行内最后一个合法百分比。
/// DISM 用退格符原地刷新进度，管道里一行可能含多段 "NN.N%"，取最后一段即最新值。
/// 零新增依赖——workspace 没有 regex，手写扫描保持同等语义。
fn parse_percent(line: &str) -> Option<f32> {
    let bytes = line.as_bytes();
    let mut found: Option<f32> = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'%' || i == 0 || !bytes[i - 1].is_ascii_digit() {
            continue;
        }
        // 从 % 往左收集 [0-9]，允许一个内嵌小数点（且点后必须有数字）
        let mut start = i;
        let mut seen_dot = false;
        while start > 0 {
            let c = bytes[start - 1];
            if c.is_ascii_digit() {
                start -= 1;
            } else if c == b'.' && !seen_dot && start >= 2 && bytes[start - 2].is_ascii_digit() {
                seen_dot = true;
                start -= 1;
            } else {
                break;
            }
        }
        if let Ok(v) = line[start..i].parse::<f32>() {
            if (0.0..=100.0).contains(&v) {
                found = Some(v);
            }
        }
    }
    found
}

/// 自提权统一拉起入口：powershell `Start-Process -Verb RunAs` 当前 exe +
/// worker 参数（UAC 授权一次的子进程，无常驻服务）。按规格不做 -Wait
/// 结果读取（worker 自行 emit/打印），保持简单。本次 dism/还原点命令在
/// 未提权时直接拒绝、不自动调用本函数；留给后续 UI 流程或 zclean 提权批复用。
#[allow(dead_code)]
fn spawn_elevated_worker(worker_args: &[String]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法定位当前 exe：{e}"))?;
    let joined = worker_args
        .iter()
        .map(|a| format!("'{a}'"))
        .collect::<Vec<_>>()
        .join(",");
    let ps = format!(
        "Start-Process -FilePath '{}' -ArgumentList {} -Verb RunAs -WindowStyle Hidden",
        exe.display(),
        joined
    );
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("提权拉起失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::parse_percent;

    #[test]
    fn parses_disstyle_progress_lines() {
        assert_eq!(parse_percent("  62.3%"), Some(62.3));
        assert_eq!(parse_percent("100%"), Some(100.0));
        assert_eq!(parse_percent("  40.0%\t 50.0% complete"), Some(50.0));
        // DISM 退格刷新在同一段管道行里出现多段百分比，取最后一段（最新值）
        assert_eq!(parse_percent("\u{8}\u{8}\u{8} 10.0%\u{8}\u{8}\u{8} 20.5%"), Some(20.5));
    }

    #[test]
    fn rejects_non_progress_lines() {
        assert_eq!(parse_percent("The operation completed successfully."), None);
        assert_eq!(parse_percent(""), None);
        assert_eq!(parse_percent("%"), None);
        assert_eq!(parse_percent("5 . 3 %"), None);
        // 越界值不采纳
        assert_eq!(parse_percent("120.0%"), None);
    }
}
