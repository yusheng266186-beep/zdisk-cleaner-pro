//! Tauri 壳层：IPC 命令桥。逻辑一律下沉 zc-core/zc-rules，这里只做搬运与事件转发。
//!
//! v5 契约（CONTRACT-v5 §2）：
//! - 所有命令失败一律 `Err(ErrorDto{code,message})`，前端 ZcError.code 据此分流，
//!   禁止再靠消息子串匹配；
//! - 事件 `scan://progress` payload = [files: u64, bytes_seen: u64]；
//!   `migrate://phase` payload = [phase, state]；`dism://progress` payload = f32。
//! - 取消通道两条：scan（世代号防竞态）与 busy（big_files/find_dupes/analyze_tree
//!   共用全局忙句柄）。
//!
//! 注意：本文件依赖 tauri 2（需要 MSVC 工具链才能本地构建，见 ADR-001）。

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::os::windows::ffi::OsStrExt;

use serde::Serialize;
use tauri::Manager as _;
use zc_core::{
    analyze, dedup,
    executor::{self, vault, CleanMode},
    guard::Guard,
    history::{self, HistoryRecord},
    ledger::{self, LedgerStore},
    manifest,
    migrate::{self, MigrationPlan},
    models::{Domain, Risk, ScanEvent, ScanReport},
    scanner,
    startup::{self, DisabledEntry, StartupEntry},
    ScanHandle,
};

/* ── 结构化错误通道（CONTRACT-v5 §2 ErrorDto）───────────────
 * thiserror 分类 → code 表：guard / cancelled / admin_required /
 * not_found / busy / locked / io（其余兜底）/ internal（后台任务 panic）。
 * io::ErrorKind::NotFound 同样归 not_found，壳层 pre-check 缺失台账用。 */

#[derive(Serialize)]
struct ErrorDto {
    code: &'static str,
    message: String,
}

impl ErrorDto {
    fn of(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

/// 内核 Error 变体 → code。纯函数，单测表驱动覆盖。
fn classify(e: &zc_core::Error) -> &'static str {
    use zc_core::Error as E;
    match e {
        E::GuardRejected { .. } => "guard",
        E::Cancelled { .. } => "cancelled",
        E::AdminRequired { .. } => "admin_required",
        E::NotFound { .. } => "not_found",
        E::Busy { .. } => "busy",
        E::Locked { .. } => "locked",
        E::Io(io) if io.kind() == std::io::ErrorKind::NotFound => "not_found",
        _ => "io",
    }
}

fn eint(e: zc_core::Error) -> ErrorDto {
    ErrorDto::of(classify(&e), e.to_string())
}

fn eio(e: std::io::Error) -> ErrorDto {
    let code = if e.kind() == std::io::ErrorKind::NotFound { "not_found" } else { "io" };
    ErrorDto::of(code, e.to_string())
}

/// spawn_blocking 的 JoinError（闭包 panic）→ internal。
fn ejoin(task: &str) -> ErrorDto {
    ErrorDto::of("internal", format!("{task}后台任务异常终止（已 panic），请重试"))
}

/* ── 统一日志（审计 C：生产 windows_subsystem 下 stdout 全部蒸发）──
 * zc-app.log 落在 zc_core::manifest::data_dir()（%LOCALAPPDATA%\ZDiskCleanerPro3）。 */

pub fn zlog(msg: &str) {
    let dir = manifest::data_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return; // 数据目录不可写时日志静默；主流程绝不因日志失败中断
    }
    let ts = ts_utc(scanner::now_unix());
    let line = format!("[{ts}Z] {msg}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("zc-app.log")) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Unix 秒 → "YYYY-MM-DD HH:MM:SS"（UTC，Howard Hinnant civil_from_days，零依赖）。
fn ts_utc(unix: u64) -> String {
    let secs = unix as i64;
    let (days, sod) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    // 历元年内 1/2 月排在 3 月之后（Hinnant：era 从 3 月起算），需要进年
    let y = y + if m <= 2 { 1 } else { 0 };
    let (h, mi, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/* ── 取消通道 1：扫描（世代号修竞态，审计 §A「取消契约靠自觉」）────
 * 每次 scan_now 领取一个全新的 ScanHandle 并登记 (gen, handle)；
 * cancel_scan 只取消「当前在册」的句柄——扫描结束后槽位已清空，
 * 迟到取消不会命中下一世代；句柄逐扫描新建，内核入口的 reset()
 * 也抹不掉「起跑前置位」的竞态取消意图（内核如实返回 cancelled 报告）。 */

static SCAN_RUNNING: AtomicBool = AtomicBool::new(false);
static SCAN_GEN: AtomicU64 = AtomicU64::new(0);
static SCAN_SLOT: Mutex<Option<(u64, ScanHandle)>> = Mutex::new(None);

/* ── 取消通道 2：忙任务（big_files / find_dupes / analyze_tree）─────
 * 全局单槽忙句柄：任一重查询开始时登记，cancel_busy 置位当前在册者；
 * 被取消的调用以 code=cancelled 上抛（绝不把半截结果当完整结果）。 */

static BUSY_GEN: AtomicU64 = AtomicU64::new(0);
static BUSY_SLOT: Mutex<Option<(u64, ScanHandle)>> = Mutex::new(None);

/// 任务收尾守卫：仅当槽内仍是自己这一代时清空（防误摘后来者），
/// 可选复位 running 标志。Drop 实现：闭包正常返回 / `?` 早退 / panic
///  unwind 三条路径都会释放，绝不留「永远 busy」。
struct SlotRelease {
    gen: u64,
    slot: &'static Mutex<Option<(u64, ScanHandle)>>,
    running: Option<&'static AtomicBool>,
}

impl Drop for SlotRelease {
    fn drop(&mut self) {
        if let Ok(mut g) = self.slot.lock() {
            if g.as_ref().is_some_and(|(cur, _)| *cur == self.gen) {
                *g = None;
            }
        }
        if let Some(flag) = self.running {
            flag.store(false, Ordering::SeqCst);
        }
    }
}

fn claim(
    slot: &'static Mutex<Option<(u64, ScanHandle)>>,
    gen: &'static AtomicU64,
) -> (u64, ScanHandle) {
    let g = gen.fetch_add(1, Ordering::SeqCst);
    let h = ScanHandle::default();
    if let Ok(mut s) = slot.lock() {
        *s = Some((g, h.clone()));
    }
    (g, h)
}

/* ── 深度工具提权 worker 协议 ─────────────────────────────
 * 特权动作（DISM 组件清理 / 系统还原点）要求管理员令牌；未提权一律
 * Err(code=admin_required)，前端按 code 分流展示提权引导。
 * 自提权旁路：powershell Start-Process -Verb RunAs 拉当前 exe +
 * worker 参数（UAC 一次性授权，无常驻服务），worker 在 run() 前置
 * 早退分支执行并把输出转写 zc-app.log。 */

const DISM_WORKER_ARG: &str = "--dism-worker";
const RP_WORKER_ARG: &str = "--rp-worker";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // main() 前置早退分支：worker 模式不创建窗口，干完活即退出。
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == DISM_WORKER_ARG) {
        zlog("[dism-worker] 提权 worker 启动");
        std::process::exit(dism_run(None));
    }
    if let Some(pos) = args.iter().position(|a| a == RP_WORKER_ARG) {
        let desc = args.get(pos + 1).cloned().unwrap_or_default();
        zlog(&format!("[rp-worker] 提权 worker 启动 desc={desc}"));
        let code = match rp_run(&desc) {
            Ok(()) => 0,
            Err(e) => {
                zlog(&format!("[rp-worker] 失败：{e}"));
                eprintln!("[rp-worker] {e}");
                1
            }
        };
        std::process::exit(code);
    }

    zlog(&format!(
        "[app] ZDiskCleaner Pro v{} 启动（tauri 壳）",
        env!("CARGO_PKG_VERSION")
    ));

    tauri::Builder::default()
        // 单实例（审计 T3）：双进程 = 双启动 sweep + SQLite 同库竞争 +
        // 双扫描抢 CPU。第二次启动聚焦已有主窗口。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            ping,
            scan_now,
            cancel_scan,
            cancel_busy,
            clean_selected,
            undo_session,
            purge_session,
            session_entries,
            vault_delete,
            rules_meta,
            history_list,
            drives_overview,
            analyze_tree,
            startup_list,
            startup_disable,
            startup_enable_all,
            startup_list_disabled,
            startup_enable_one,
            migrate_plan,
            migrate_apply,
            migrate_undo,
            big_files,
            find_dupes,
            reveal_in_explorer,
            system_occupancy,
            query_recycle_bin,
            empty_recycle_bin,
            dism_component_cleanup,
            create_restore_point
        ])
        .setup(|_app| {
            // vault 7 天后悔期到期自动清扫：启动即后台执行，绝不阻塞窗口；
            // 结果进 zc-app.log，不打扰用户（被占用批次保留，下次启动再扫）。
            tauri::async_runtime::spawn_blocking(|| {
                match zc_core::executor::vault::sweep_expired(7) {
                    Ok(s) => {
                        if s.sessions > 0 {
                            let msg = format!(
                                "[vault-sweep] 清扫 {} 个过期批次，{} 项 / {}（孤儿GC熔断={}）",
                                s.sessions,
                                s.items,
                                human_bytes(s.bytes),
                                s.gc_skipped
                            );
                            eprintln!("{msg}");
                            zlog(&msg);
                        }
                    }
                    Err(e) => {
                        let msg = format!("[vault-sweep] 失败：{e}");
                        eprintln!("{msg}");
                        zlog(&msg);
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn ping() -> String {
    format!("zc-core v{}", env!("CARGO_PKG_VERSION"))
}

/* ── 扫描 ─────────────────────────────────────────────── */

/// 全量扫描：spawn_blocking 执行；进度经 `scan://progress` 推送；
/// include_admin=true 且进程已提权 → 纳入 admin 规则（未提权静默剔除，
/// admin 规则目标全在系统禁删区，非提权 apply 必整批 guard 连坐）。
/// 结束报告落盘 sessions/ 目录；运行中被取消 → Err(code=cancelled)。
#[tauri::command]
async fn scan_now(
    app: tauri::AppHandle,
    include_admin: Option<bool>,
) -> Result<ScanReport, ErrorDto> {
    if SCAN_RUNNING.swap(true, Ordering::SeqCst) {
        return Err(ErrorDto::of("busy", "已有扫描在运行，请先取消或等待其结束"));
    }
    let (gen, handle) = claim(&SCAN_SLOT, &SCAN_GEN);
    // 提权探测放进闭包外的轻量判断（GetCurrentThreadToken 级开销）
    let include_admin = include_admin.unwrap_or(false) && zc_core::is_elevated();

    let out = match tauri::async_runtime::spawn_blocking(move || {
        let _release = SlotRelease { gen, slot: &SCAN_SLOT, running: Some(&SCAN_RUNNING) };
        let pairs: Vec<(String, String)>;
        let ages: std::collections::BTreeMap<String, u64>;
        {
            let keep = |id: &str| {
                include_admin || !zc_rules::find(id).is_some_and(|r| r.admin_required)
            };
            let (all_pairs, all_ages) = zc_rules::expand_all_with_opts();
            pairs = all_pairs.into_iter().filter(|(id, _)| keep(id)).collect();
            ages = all_ages.into_iter().filter(|(id, _)| keep(id)).collect();
        }
        let mut rep = scanner::scan_with_opts(&pairs, &ages, &handle, move |ev| {
            if let ScanEvent::Entry { files, bytes_seen } = ev {
                use tauri::Emitter as _;
                let _ = app.emit("scan://progress", vec![files, bytes_seen]);
            }
        })
        .map_err(eint)?;
        zc_rules::filter_guards(&mut rep.findings);

        // 报告落盘供 show/apply 复查；写失败不影响本次结果
        let dir = manifest::data_dir().join("sessions");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join(format!("{}.json", rep.id)),
            serde_json::to_vec_pretty(&rep).unwrap_or_default(),
        );
        if rep.cancelled {
            return Err(ErrorDto::of("cancelled", "扫描已被取消"));
        }
        Ok(rep)
    })
    .await
    {
        Ok(r) => r,
        Err(_) => {
            // panic 场景闭包内守卫已释放；spawn 失败则守卫根本没跑，
            // 这里按代际补偿释放（幂等，flag 重复置 false 无害）
            if let Ok(mut g) = SCAN_SLOT.lock() {
                if g.as_ref().is_some_and(|(cur, _)| *cur == gen) {
                    *g = None;
                }
            }
            SCAN_RUNNING.store(false, Ordering::SeqCst);
            Err(ejoin("扫描"))
        }
    };

    match &out {
        Ok(rep) => zlog(&format!(
            "[scan] {} · {} 文件 · {} 规则命中 · 耗时 {}ms",
            rep.id,
            rep.files_seen,
            rep.findings.len(),
            rep.duration_ms
        )),
        Err(e) => zlog(&format!("[scan] 失败 code={} msg={}", e.code, e.message)),
    }
    out
}

#[tauri::command]
fn cancel_scan() {
    // 仅扫描在跑（槽内有主）时置位；不在跑 = 空操作，不会毒化下次扫描
    if let Ok(g) = SCAN_SLOT.lock() {
        if let Some((_, h)) = g.as_ref() {
            h.cancel();
        }
    }
}

/// 取消当前忙任务（big_files/find_dupes/analyze_tree）。无忙任务时空操作。
#[tauri::command]
fn cancel_busy() {
    if let Ok(g) = BUSY_SLOT.lock() {
        if let Some((_, h)) = g.as_ref() {
            h.cancel();
        }
    }
}

/* ── 元信息 ───────────────────────────────────────────── */

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

#[derive(Serialize)]
struct HistoryDto {
    session_id: String,
    created_unix: u64,
    mode: CleanMode,
    files: u64,
    bytes_moved: u64,
    kind: Option<String>,
    src: Option<String>,
    dst: Option<String>,
    /// v5：台账行是否仍存活——false = 批次已结清（还原/彻底删除/到期清扫），
    /// 历史页据此隐藏还原/彻底删除动作（流水保留作审计，终态不装作可动）。
    live: bool,
}

#[tauri::command]
fn history_list() -> Vec<HistoryDto> {
    let recs = history::read_all();
    // fail-open：台账不可读时一律标 live——历史列表不许因此清空，
    // 动作按钮的报错在点击路径上如实呈现。
    let live: Option<std::collections::HashSet<String>> = LedgerStore::open()
        .ok()
        .and_then(|s| s.live_manifest_ids().ok())
        .map(|v| v.into_iter().collect());
    recs.into_iter()
        .map(|r| HistoryDto {
            live: live
                .as_ref()
                .map(|ls| ls.contains(&r.session_id))
                .unwrap_or(true),
            session_id: r.session_id,
            created_unix: r.created_unix,
            mode: r.mode,
            files: r.files,
            bytes_moved: r.bytes_moved,
            kind: r.kind,
            src: r.src,
            dst: r.dst,
        })
        .collect()
}

/// 结清标记：还原/彻底删除完成后给历史流水行的 kind 落终态标签（同 session 覆盖写）。
fn mark_history_kind(store: &LedgerStore, id: &str, kind: &str) {
    let mut recs = store.read_history();
    if let Some(r) = recs.iter_mut().find(|r| r.session_id == id) {
        r.kind = Some(kind.to_string());
        let _ = store.append_history(r);
    }
}

/* ── 清理 / 还原 / 彻底删除 ───────────────────────────── */

#[derive(Serialize)]
struct OutcomeDto {
    requested_files: u64,
    requested_bytes: u64,
    done_files: u64,
    done_bytes: u64,
    failed: Vec<(String, String)>,
    semantics_note: String,
}

impl OutcomeDto {
    fn from_core(o: executor::CleanOutcome) -> Self {
        Self {
            requested_files: o.requested_files,
            requested_bytes: o.requested_bytes,
            done_files: o.done_files,
            done_bytes: o.done_bytes,
            failed: o.failed,
            semantics_note: o.semantics_note,
        }
    }
}

#[derive(Serialize)]
struct FailDto {
    path: String,
    error: String,
}

/// undo_session / purge_session 共用（CONTRACT §2 UndoResultDto）。
#[derive(Serialize)]
struct UndoResultDto {
    id: String,
    done: u64,
    bytes: u64,
    failed: Vec<FailDto>,
}

/// 按报告勾选执行清理。取消熔断 / 守卫拒绝都经内核如实上抛；
/// semantics_note 透传（含「重启后完成删除」兜底说明）。
#[tauri::command]
async fn clean_selected(
    report: ScanReport,
    rule_ids: Vec<String>,
    mode: CleanMode,
) -> Result<OutcomeDto, ErrorDto> {
    let out: Result<OutcomeDto, ErrorDto> = tauri::async_runtime::spawn_blocking(move || {
        let outcome = executor::apply(&report, &rule_ids, mode).map_err(eint)?;
        let _ = history::append(&HistoryRecord {
            session_id: report.id.clone(),
            created_unix: scanner::now_unix(),
            mode,
            files: outcome.done_files,
            bytes_moved: outcome.done_bytes,
            ..Default::default()
        });
        analyze_cache_invalidate();
        zlog(&format!(
            "[clean] {} mode={} done {}/{} 项 {} 失败 {} 项",
            report.id,
            if mode == CleanMode::Vault { "vault" } else { "recycle_bin" },
            outcome.done_files,
            outcome.requested_files,
            outcome.done_bytes,
            outcome.failed.len()
        ));
        Ok(OutcomeDto::from_core(outcome))
    })
    .await
    .map_err(|_| ejoin("清理"))?;
    if let Err(e) = &out {
        zlog(&format!("[clean] 失败 code={} msg={}", e.code, e.message));
    }
    out
}

/// 还原本批（结构化 DTO；bytes = 成功复位条目的台账字节和）。
#[tauri::command]
async fn undo_session(id: String) -> Result<UndoResultDto, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = LedgerStore::open().map_err(eio)?;
        let m = store
            .load_manifest(&id)
            .ok_or_else(|| ErrorDto::of("not_found", format!("台账 {id} 不存在或已被彻底删除")))?;
        if m.mode != CleanMode::Vault {
            return Err(ErrorDto::of(
                "internal",
                "回收站模式批次无法程序化还原：请到系统回收站中还原",
            ));
        }
        let committed: Vec<(String, u64)> = store
            .session_entries(&id)
            .map_err(eio)?
            .into_iter()
            .filter(|e| e.status == "committed")
            .map(|e| (e.origin, e.size))
            .collect();
        let (done, failed) = m.undo().map_err(eint)?;
        analyze_cache_invalidate();
        let failed_paths: HashSet<String> =
            failed.iter().map(|(p, _)| p.display().to_string()).collect();
        let bytes: u64 = committed
            .iter()
            .filter(|(o, _)| !failed_paths.contains(o))
            .map(|(_, s)| *s)
            .sum();
        let dto = UndoResultDto {
            id: id.clone(),
            done: done as u64,
            bytes,
            failed: failed
                .into_iter()
                .map(|(p, e)| FailDto { path: p.display().to_string(), error: e })
                .collect(),
        };
        zlog(&format!(
            "[undo] {id} done={} bytes={} failed={}",
            dto.done,
            dto.bytes,
            dto.failed.len()
        ));
        if dto.failed.is_empty() {
            // 全部复位后 vault 已空，台账留着只会让历史页出现点一次错一次的
            // 死按钮（v5 结清语义）：抹账行 + 清空壳目录 + 流水标「已还原」。
            // 部分失败不动——entries 仍在，允许再次还原重试。
            let _ = store.drop_manifest(&id);
            let _ = std::fs::remove_dir_all(executor::vault::vault_session_dir(&id));
            mark_history_kind(&store, &id, "undo");
        }
        Ok(dto)
    })
    .await
    .map_err(|_| ejoin("还原"))?
}

/// 彻底删除一批 vault 副本（bytes = 实际释放字节）。
#[tauri::command]
async fn purge_session(id: String) -> Result<UndoResultDto, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let m = manifest::CleanManifest::load(&id)
            .map_err(|e| ErrorDto::of(classify(&e), e.to_string()))?;
        let (deleted, freed, failed) = m.purge_forever().map_err(eint)?;
        analyze_cache_invalidate();
        zlog(&format!("[purge] {id} deleted={deleted} freed={freed} failed={}", failed.len()));
        if failed.is_empty() {
            // purge_forever 全成才抹账；流水标「已彻底删除」供历史页结清展示
            if let Ok(store) = LedgerStore::open() {
                mark_history_kind(&store, &id, "purge");
            }
        }
        Ok(UndoResultDto {
            id: id.clone(),
            done: deleted as u64,
            bytes: freed,
            failed: failed
                .into_iter()
                .map(|(p, e)| FailDto { path: p, error: e })
                .collect(),
        })
    })
    .await
    .map_err(|_| ejoin("彻底删除"))?
}

/// 批次明细下钻（journal 中间态 status='pending' = 未完成警示）。
#[tauri::command]
async fn session_entries(id: String) -> Result<Vec<ledger::SessionEntryDto>, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = LedgerStore::open().map_err(eio)?;
        if store.load_manifest(&id).is_none() {
            return Err(ErrorDto::of("not_found", format!("台账 {id} 不存在或已被彻底删除")));
        }
        store.session_entries(&id).map_err(eio)
    })
    .await
    .map_err(|_| ejoin("台账明细"))?
}

fn human_bytes(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", U[i])
    }
}

/* ── 盘符 / 雷达 / 大文件 / 查重（忙任务可取消）─────────── */

#[derive(Serialize)]
struct DriveDto {
    label: String,
    total_bytes: u64,
    free_bytes: u64,
}

/// 已挂载盘符容量总览（GetLogicalDrives 位图 × 逐盘查询，绝不裸探 A-Z）。
#[tauri::command]
async fn drives_overview() -> Result<Vec<DriveDto>, ErrorDto> {
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
    .map_err(|_| ejoin("盘符总览"))
}

/// 空间雷达体积树缓存：同一 (根, 深度) 10 分钟内直接复用。
/// 任何改动磁盘 Tree 形态的操作（清理/撤销/彻底删除/手动删除/迁移）都会失效它。
static ANALYZE_CACHE: Mutex<Option<(String, u32, analyze::TreeNode, std::time::Instant)>> =
    Mutex::new(None);

fn analyze_cache_invalidate() {
    if let Ok(mut m) = ANALYZE_CACHE.lock() {
        *m = None;
    }
}

/// 空间雷达：构建目录体积聚合树。path 空 = 主目录；depth 上限 6。
/// 走 build_tree_cancellable + 全局忙句柄；取消命中（内核返回部分树）
/// 由壳层如实转 Err(cancelled)，绝不把半截树当完整树喂给缓存。
#[tauri::command]
async fn analyze_tree(path: String, depth: u32, fresh: Option<bool>) -> Result<analyze::TreeNode, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let (gen, handle) = claim(&BUSY_SLOT, &BUSY_GEN);
        let _release = SlotRelease { gen, slot: &BUSY_SLOT, running: None };
        let root = if path.is_empty() {
            std::env::var("USERPROFILE")
                .map_err(|_| ErrorDto::of("not_found", "无法定位用户主目录（USERPROFILE）"))?
        } else {
            path
        };
        let depth = depth.min(6);
        let fresh = fresh.unwrap_or(false);
        if !fresh {
            if let Ok(guard) = ANALYZE_CACHE.lock() {
                if let Some((r, d, tree, at)) = guard.as_ref() {
                    if *r == root && *d == depth && at.elapsed() < std::time::Duration::from_secs(600) {
                        return Ok(tree.clone());
                    }
                }
            }
        }
        let tree = analyze::build_tree_cancellable(
            Path::new(&root),
            depth as usize,
            40,
            &handle,
        );
        if handle.cancelled() {
            return Err(ErrorDto::of("cancelled", "体积分析已取消"));
        }
        if let Ok(mut m) = ANALYZE_CACHE.lock() {
            *m = Some((root, depth, tree.clone(), std::time::Instant::now()));
        }
        Ok(tree)
    })
    .await
    .map_err(|_| ejoin("雷达"))?
}

#[derive(Serialize)]
struct BigFileDto {
    path: String,
    size: u64,
}

/// 大文件 Top-N：可取消（取消 → Err(code=cancelled)，绝不返回半截榜单）。
#[tauri::command]
async fn big_files(path: String, top: u32) -> Result<Vec<BigFileDto>, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let (gen, handle) = claim(&BUSY_SLOT, &BUSY_GEN);
        let _release = SlotRelease { gen, slot: &BUSY_SLOT, running: None };
        let root = if path.is_empty() {
            std::env::var("USERPROFILE")
                .map_err(|_| ErrorDto::of("not_found", "无法定位用户主目录（USERPROFILE）"))?
        } else {
            path
        };
        let files = analyze::largest_files_cancellable(
            Path::new(&root),
            top.max(1).min(200) as usize,
            1024 * 1024,
            &handle,
        )
        .map_err(eint)?
        .into_iter()
        .map(|(p, size)| BigFileDto { path: p.to_string_lossy().into_owned(), size })
        .collect();
        Ok(files)
    })
    .await
    .map_err(|_| ejoin("大文件"))?
}

/// 重复文件组（XXH3 三级管道 + 硬链接去重 + 云占位跳过）：可取消。
#[tauri::command]
async fn find_dupes(path: String, min_mb: u64) -> Result<Vec<dedup::DuplicateGroup>, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let (gen, handle) = claim(&BUSY_SLOT, &BUSY_GEN);
        let _release = SlotRelease { gen, slot: &BUSY_SLOT, running: None };
        dedup::find_duplicates_cancellable(
            &[PathBuf::from(path)],
            min_mb * 1024 * 1024,
            &handle,
        )
        .map_err(eint)
    })
    .await
    .map_err(|_| ejoin("查重"))?
}

/// 手动安全删除：守卫 vet（fail-closed）→ journal 化 stash（move 前落账）
/// → 台账/历史。actual_size 只在 stash_journal 内跑一遍（修 v4 双遍历）。
#[tauri::command]
async fn vault_delete(paths: Vec<String>) -> Result<OutcomeDto, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let mut outcome = executor::CleanOutcome {
            requested_files: refs.len() as u64,
            ..Default::default()
        };
        outcome.semantics_note = "已移入暂存区 vault：7 天内可在历史页一键还原".to_string();
        let existing: Vec<&Path> = refs.iter().map(|p| p.as_path()).filter(|p| p.exists()).collect();
        if existing.is_empty() {
            return Ok(OutcomeDto::from_core(outcome));
        }
        Guard::new().vet(existing.iter().copied()).map_err(eint)?;

        let session = format!("manual-{}", scanner::new_session_id());
        let store = LedgerStore::open().map_err(eio)?;
        let (ok, failed) = vault::stash_journal(
            &vault::vault_session_dir(&session),
            &existing,
            &store,
            &session,
        )
        .map_err(eint)?;
        // stash_journal 已逐条实测副本字节（journal 同步落库），不再二次 actual_size
        outcome.done_bytes = ok.iter().map(|(_, _, s)| *s).sum();
        outcome.done_files = ok.len() as u64;
        outcome
            .failed
            .extend(failed.into_iter().map(|(p, e)| (p.display().to_string(), e)));

        let _ = history::append(&HistoryRecord {
            session_id: session.clone(),
            created_unix: scanner::now_unix(),
            mode: CleanMode::Vault,
            files: outcome.done_files,
            bytes_moved: outcome.done_bytes,
            ..Default::default()
        });
        analyze_cache_invalidate();
        if !outcome.failed.is_empty() {
            outcome.semantics_note = format!(
                "{}；另有 {} 项未能处理（多为文件被占用），已原样保留",
                outcome.semantics_note,
                outcome.failed.len()
            );
        }
        zlog(&format!(
            "[vault-delete] {session} done={} bytes={} failed={}",
            outcome.done_files,
            outcome.done_bytes,
            outcome.failed.len()
        ));
        Ok(OutcomeDto::from_core(outcome))
    })
    .await
    .map_err(|_| ejoin("手动删除"))?
}

/* ── 启动项管家 ───────────────────────────────────────── */

#[tauri::command]
fn startup_list() -> Result<Vec<StartupEntry>, ErrorDto> {
    startup::list_user_startup().map_err(eio)
}

#[tauri::command]
fn startup_disable(key_id: String) -> Result<bool, ErrorDto> {
    startup::disable(&key_id).map_err(eio)
}

/// 恢复全部被禁用项：返回成功写回条数（ipc.ts 契约 number）；
/// 内核逐项明细中的失败项转写 zc-app.log（保留在备份里可重试）。
#[tauri::command]
fn startup_enable_all() -> Result<usize, ErrorDto> {
    let s = startup::enable_all().map_err(eio)?;
    if !s.failed.is_empty() {
        zlog(&format!(
            "[startup] enable_all 部分失败 {}/{}：{}",
            s.failed.len(),
            s.restored + s.failed.len(),
            s.failed
                .iter()
                .map(|(k, e)| format!("{k}={e}"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    Ok(s.restored)
}

/// 已禁用区清单（备份 JSON 对账；损坏上抛 Err，不再静默空）。
#[tauri::command]
fn startup_list_disabled() -> Result<Vec<DisabledEntry>, ErrorDto> {
    startup::list_disabled().map_err(eio)
}

/// 单条恢复：成功才从备份移除；返回是否回写成功。
#[tauri::command]
fn startup_enable_one(key_id: String) -> Result<bool, ErrorDto> {
    startup::enable_one(&key_id).map_err(eio)
}

/* ── 存储迁移中心 ─────────────────────────────────────── */

#[tauri::command]
async fn migrate_plan(src: String, dst_root: String) -> Result<MigrationPlan, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        migrate::plan(Path::new(&src), Path::new(&dst_root)).map_err(eio)
    })
    .await
    .map_err(|_| ejoin("迁移计划"))?
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

/// 执行迁移：内部重新 plan 后 apply（fail-closed）。阶段推进经
/// `migrate://phase` 实时推送。成功与失败两路径都失效雷达缓存
/// （审计 T2：Link 之后失败时 junction 已建成，树形态已变）；
/// 成功时写 kind='migrate' 历史行（src/dst 列供历史页撤销）。
#[tauri::command]
async fn migrate_apply(
    app: tauri::AppHandle,
    src: String,
    dst_root: String,
) -> Result<String, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let plan = migrate::plan(Path::new(&src), Path::new(&dst_root)).map_err(eio)?;
        let result = migrate::apply_with_phases(&plan, &mut |phase, state| {
            use tauri::Emitter as _;
            let _ = app.emit(
                "migrate://phase",
                vec![migrate_phase_str(phase), migrate_state_str(state)],
            );
        })
        .map_err(|e| ErrorDto::of("io", e));
        // 两分支统一先失效缓存再分发结果
        analyze_cache_invalidate();
        match result {
            Ok(id) => {
                let rec = HistoryRecord {
                    session_id: id.clone(),
                    created_unix: scanner::now_unix(),
                    mode: CleanMode::Vault,
                    files: plan.total_files,
                    bytes_moved: plan.total_bytes,
                    kind: Some("migrate".to_string()),
                    src: Some(plan.src.display().to_string()),
                    dst: Some(plan.dst.display().to_string()),
                };
                if let Err(e) = history::append(&rec) {
                    zlog(&format!("[migrate] 历史行写入失败 id={id}: {e}"));
                }
                zlog(&format!(
                    "[migrate] apply ✓ id={id} {} → {}（{} 文件 / {} B）",
                    plan.src.display(),
                    plan.dst.display(),
                    plan.total_files,
                    plan.total_bytes
                ));
                Ok(id)
            }
            Err(e) => {
                zlog(&format!("[migrate] apply ✗ {src} → {dst_root}: {}", e.message));
                Err(e)
            }
        }
    })
    .await
    .map_err(|_| ejoin("迁移"))?
}

#[derive(Serialize)]
struct MigrateUndoDto {
    restored: u64,
    failed: Vec<FailDto>,
}

/// 撤销迁移：摘 junction 并复位数据。重 IO（跨盘回退复制）改 async
/// spawn_blocking（审计 T1）；成功写 kind='migrate_undo' 历史行；
/// 成功/失败两路径都失效雷达缓存。
#[tauri::command]
async fn migrate_undo(src: String, dst: Option<String>) -> Result<MigrateUndoDto, ErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = migrate::undo(Path::new(&src), dst.as_deref().map(Path::new));
        analyze_cache_invalidate();
        match result {
            Ok(msg) => {
                // restored：优先取迁移清单里的文件数（同内核 undo 的清单定位逻辑）
                let restored =
                    migration_manifest_files(Path::new(&src), dst.as_deref().map(Path::new));
                let rec = HistoryRecord {
                    session_id: format!("migrate-undo-{}", scanner::new_session_id()),
                    created_unix: scanner::now_unix(),
                    mode: CleanMode::Vault,
                    files: restored,
                    bytes_moved: 0,
                    kind: Some("migrate_undo".to_string()),
                    src: Some(src.clone()),
                    dst: dst.clone(),
                };
                let _ = history::append(&rec);
                zlog(&format!("[migrate] undo ✓ {src}: {msg}"));
                Ok(MigrateUndoDto { restored, failed: Vec::new() })
            }
            Err(e) => {
                zlog(&format!("[migrate] undo ✗ {src}: {e}"));
                let code = if e.contains("不是 junction") { "not_found" } else { "io" };
                Err(ErrorDto::of(code, e))
            }
        }
    })
    .await
    .map_err(|_| ejoin("撤销迁移"))?
}

/// 在 data_dir()/migrations/ 清单中按 src（可选 dst 双匹配）取最近一次
/// 迁移的 total_files，作为 undo「已复位项数」的记账口径；找不到按 1。
fn migration_manifest_files(src: &Path, dst: Option<&Path>) -> u64 {
    use std::cmp::Reverse;
    let dir = manifest::data_dir().join("migrations");
    let Ok(rd) = std::fs::read_dir(&dir) else { return 1 };
    let mut hits: Vec<(std::time::SystemTime, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok().and_then(|m| m.modified().ok().map(|t| (t, e.path()))))
        .collect();
    hits.sort_by_key(|(t, _)| Reverse(*t));
    let want_src = zc_core::norm(src);
    let want_dst = dst.map(zc_core::norm);
    for (_, path) in hits {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(plan) = serde_json::from_slice::<MigrationPlan>(&bytes) else { continue };
        if zc_core::norm(&plan.src) == want_src
            && want_dst.as_deref().is_none_or(|d| zc_core::norm(&plan.dst) == d)
        {
            return plan.total_files.max(1);
        }
    }
    1
}

/* ── 回收站（v5 新增，审计 A2）────────────────────────── */

/// 查询全部分盘回收站占用（SHQueryRecycleBinW 聚合；内核永不失败）。
#[tauri::command]
async fn query_recycle_bin() -> Result<zc_core::RecycleBinInfo, ErrorDto> {
    tauri::async_runtime::spawn_blocking(zc_core::recycle_bin::query_all)
        .await
        .map_err(|_| ejoin("回收站查询"))
}

/// 一键清空全部回收站（SHEmptyRecycleBinW 静默模式；释放口径 = 前后配额差）。
#[tauri::command]
async fn empty_recycle_bin() -> Result<zc_core::RecycleBinSummary, ErrorDto> {
    let out = tauri::async_runtime::spawn_blocking(zc_core::recycle_bin::empty_all)
        .await
        .map_err(|_| ejoin("清空回收站"))?
        .map_err(eint);
    match &out {
        Ok(s) => zlog(&format!(
            "[recycle-bin] 清空 {} 项，释放 {} B",
            s.items_before, s.bytes_freed
        )),
        Err(e) => zlog(&format!("[recycle-bin] 清空失败 code={} msg={}", e.code, e.message)),
    }
    out
}

/* ── 空间雷达实用动作 ─────────────────────────────────── */

/// 在资源管理器中打开指定目录。
#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), ErrorDto> {
    if !Path::new(&path).exists() {
        return Err(ErrorDto::of("not_found", format!("路径不存在：{path}")));
    }
    Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| ErrorDto::of("io", format!("无法启动资源管理器：{e}")))?;
    Ok(())
}

/* ── 深度工具：系统占用 / WinSxS 组件清理 / 系统还原点 ──── */

/// 系统级占用盘点（只读；失败 → Err，不再吞错返回空表）。
#[tauri::command]
async fn system_occupancy() -> Result<Vec<zc_core::system::OccupancyItem>, ErrorDto> {
    tauri::async_runtime::spawn_blocking(zc_core::system::system_occupancy)
        .await
        .map_err(|_| ejoin("系统占用"))
}

/// WinSxS 组件清理：未提权 → Err(code=admin_required)；已提权内联执行，
/// 真实百分比经 `dism://progress` 推送；成功后失效雷达缓存。
#[tauri::command]
async fn dism_component_cleanup(app: tauri::AppHandle) -> Result<(), ErrorDto> {
    if !zc_core::is_elevated() {
        return Err(ErrorDto::of("admin_required", "DISM 组件清理需要管理员权限"));
    }
    let code = tauri::async_runtime::spawn_blocking(move || dism_run(Some(&app)))
        .await
        .map_err(|_| ejoin("DISM"))?;
    if code == 0 {
        analyze_cache_invalidate();
        Ok(())
    } else {
        Err(ErrorDto::of("io", format!("dism.exe 退出码 {code}，详见 zc-app.log")))
    }
}

/// 系统还原点：要求管理员令牌（官方 Checkpoint-Computer 通道）。
#[tauri::command]
async fn create_restore_point(desc: String) -> Result<(), ErrorDto> {
    if !zc_core::is_elevated() {
        return Err(ErrorDto::of("admin_required", "创建还原点需要管理员权限"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        rp_run(&desc).map_err(|e| ErrorDto::of("io", e))?;
        analyze_cache_invalidate();
        Ok(())
    })
    .await
    .map_err(|_| ejoin("还原点"))?
}

/// DISM 共用执行体：命令内联（app=Some，emit 进度）与 --dism-worker
/// 子进程（app=None）复用同一实现；输出逐行转写 zc-app.log。
/// 返回 dism.exe 退出码；spawn 失败返回 -1。
fn dism_run(app: Option<&tauri::AppHandle>) -> i32 {
    let start_msg = "[dism-worker] dism.exe /Online /Cleanup-Image /StartComponentCleanup 启动…";
    println!("{start_msg}");
    zlog(start_msg);
    let mut child = match Command::new("dism.exe")
        .args(["/Online", "/Cleanup-Image", "/StartComponentCleanup"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("[dism-worker] dism.exe 拉起失败：{e}");
            eprintln!("{msg}");
            zlog(&msg);
            return -1;
        }
    };

    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            println!("[dism] {line}");
            zlog(&format!("[dism] {}", truncate(&line, 200)));
            if let Some(pct) = parse_percent(&line) {
                if let Some(a) = app {
                    use tauri::Emitter as _;
                    let _ = a.emit("dism://progress", pct);
                }
            }
        }
    }
    if let Some(err) = child.stderr.take() {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                zlog(&format!("[dism:err] {}", truncate(&line, 200)));
            }
        }
    }

    match child.wait() {
        Ok(s) => {
            let code = s.code().unwrap_or(-1);
            let msg = format!("[dism-worker] 结束，退出码 {code}");
            println!("{msg}");
            zlog(&msg);
            code
        }
        Err(e) => {
            let msg = format!("[dism-worker] 等待 dism.exe 失败：{e}");
            eprintln!("{msg}");
            zlog(&msg);
            -1
        }
    }
}

/// 还原点创建共用执行体：spawn powershell Checkpoint-Computer。
/// 输出（含 stderr）捕获后转写 zc-app.log——生产 GUI 下 stdout 不可见。
fn rp_run(desc: &str) -> Result<(), String> {
    // PowerShell 单引号转义（'' = 字面单引号）防注入
    let esc = desc.replace('\'', "''");
    println!("[rp-worker] Checkpoint-Computer 启动（描述：{esc}）…");
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Checkpoint-Computer -Description '{esc}' -RestorePointType MODIFY_SETTINGS"),
        ])
        .output()
        .map_err(|e| format!("无法启动 powershell：{e}"))?;
    let so = String::from_utf8_lossy(&out.stdout);
    let se = String::from_utf8_lossy(&out.stderr);
    if !so.trim().is_empty() {
        zlog(&format!("[rp] {}", truncate(so.trim(), 400)));
    }
    if !se.trim().is_empty() {
        zlog(&format!("[rp:err] {}", truncate(se.trim(), 400)));
    }
    if out.status.success() {
        zlog("[rp] 还原点创建成功");
        Ok(())
    } else {
        Err(format!("Checkpoint-Computer 退出码 {:?}", out.status.code()))
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// 与正则 r"(\d+(?:\.\d+)?)%" 等价的首尾扫描：返回行内最后一个合法百分比。
/// DISM 用退格符原地刷新进度，管道里一行可能含多段 "NN.N%"，取最后一段即最新值。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// ErrorDto code 映射表驱动（CONTRACT-v5 §2）：每个变体 → 契约码，
    /// 序列化形状恒为 {code,message}（前端 wrapReject 依赖双键存在）。
    #[test]
    fn error_code_mapping_is_table_complete() {
        let cases: Vec<(zc_core::Error, &str)> = vec![
            (
                zc_core::Error::GuardRejected { path: "p".into(), reason: "r".into() },
                "guard",
            ),
            (zc_core::Error::Cancelled { reason: "x".into() }, "cancelled"),
            (zc_core::Error::AdminRequired { reason: "x".into() }, "admin_required"),
            (zc_core::Error::NotFound { what: "x".into() }, "not_found"),
            (zc_core::Error::Busy { reason: "x".into() }, "busy"),
            (zc_core::Error::Locked { path: "x".into() }, "locked"),
            (
                zc_core::Error::Io(std::io::Error::from(std::io::ErrorKind::NotFound)),
                "not_found",
            ),
            (zc_core::Error::Io(std::io::Error::other("disk full")), "io"),
            (zc_core::Error::Other("whatever".into()), "io"),
            (zc_core::Error::External("cmd failed".into()), "io"),
            (
                zc_core::Error::Json(serde_json::from_str::<String>("not-json").unwrap_err()),
                "io",
            ),
        ];
        for (e, want) in cases {
            let dbg = format!("{e:?}");
            let dto = eint(e);
            assert_eq!(dto.code, want, "变体 {dbg} 应映射为 {want}");
            let json = serde_json::to_string(&dto).unwrap();
            assert!(
                json.starts_with("{\"code\":\"") && json.contains("\"message\":\""),
                "ErrorDto 序列化必须恒为 {{code,message}} 形状：{json}"
            );
        }
        // JoinError → internal 通道存在且形状一致
        let j = serde_json::to_string(&ejoin("测试")).unwrap();
        assert!(j.contains("\"internal\""), "{j}");
        // io::Error 直传分类：NotFound 族 → not_found，其余 io
        assert_eq!(eio(std::io::Error::from(std::io::ErrorKind::NotFound)).code, "not_found");
        assert_eq!(eio(std::io::Error::other("x")).code, "io");
    }

    #[test]
    fn ts_utc_formats_known_instants() {
        assert_eq!(ts_utc(0), "1970-01-01 00:00:00");
        // 2026-08-18 00:00:00 UTC = 1787011200（今天附近锚点，防历法漂移）
        assert_eq!(ts_utc(1_787_011_200), "2026-08-18 00:00:00");
        // 含闰年/世纪边界的抽查
        assert_eq!(ts_utc(1_700_000_000), "2023-11-14 22:13:20");
    }

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
