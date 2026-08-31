//! `zclean` — headless CLI，是内核的第一消费者（UI 只是第二个）。
//!
//! 用法：
//!   zclean scan [--admin] [--json [FILE]]     扫描内置规则（--json 无 FILE 则报告走 stdout）
//!   zclean apply REPORT.json --mode vault     按报告执行清理（trash|vault）
//!   zclean undo SESSION-ID                    还原一次 vault 批次
//!   zclean purge SESSION-ID                   彻底删除 vault 批次副本
//!   zclean vault P1 [P2...]                   手动安全删除（守卫 + 暂存区 + 台账）
//!   zclean sweep [--days N]                   清扫超过 N 天后悔期的 vault 批次
//!   zclean bigfiles PATH [--top N] [--json]   大文件 Top-N
//!   zclean dupes PATH [--min-mb N] [--json]   重复文件组
//!   zclean show REPORT.json                   重放展示某次扫描结果
//!   zclean rules [--md]                       规则列表 / Markdown 手册
//!   zclean tree / startup / migrate / elevated-run   [内部/辅助] 见 `zclean` 帮助
//!
//! 退出码约定（v5，自动化可对账）：
//!   0 全部成功 · 1 错误 · 2 部分失败（failed 清单非空）· 3 取消

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use zc_core::{executor, history::HistoryRecord, manifest::CleanManifest, models::*, *};

mod elevate;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("错误: {e}");
            // 取消与错误分道：3=取消，1=其余错误
            if matches!(e, Error::Cancelled { .. }) {
                ExitCode::from(3)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

/// 退出码决策（纯函数，单测覆盖）：取消优先于部分失败。
fn decide_exit_code(failed_len: usize, cancelled: bool) -> ExitCode {
    if cancelled {
        ExitCode::from(3)
    } else if failed_len > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn run(args: &[String]) -> Result<ExitCode> {
    match args.first().map(|s| s.as_str()) {
        Some("scan") => cmd_scan(args),
        Some("apply") => cmd_apply(args),
        Some("undo") => cmd_undo(args),
        Some("purge") => cmd_purge(args),
        Some("vault") => cmd_vault(args),
        Some("sweep") => cmd_sweep(args),
        Some("show") => cmd_show(args),
        Some("rules") => cmd_rules(args),
        Some("elevated-run") => Ok(cmd_elevated_run(args)),
        Some("tree") => cmd_tree(args),
        Some("bigfiles") => cmd_bigfiles(args),
        Some("dupes") => cmd_dupes(args),
        Some("startup") => cmd_startup(args),
        Some("migrate") => cmd_migrate(args),
        _ => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn print_help() {
    println!(
        "zclean v{} — ZDiskCleaner Pro headless 内核客户端
退出码: 0 全成 · 1 错误 · 2 部分失败(failed 非空) · 3 取消

命令:
  scan [--admin] [--json [FILE]]
                    扫描内置规则并保存会话报告；--admin 在提权终端纳入
                    系统级规则；--json 输出机器可读报告（缺省写 stdout）
  apply REPORT --mode MODE    执行清理 (MODE=trash|vault)
          [--rules id1,id2]   显式规则（缺省=仅安全档）
          [--admin]           含管理员规则时必须携带：走一次性 UAC 提权批
                              （未提权且未带 --admin 一律拒绝，不再静默剔除）
  undo SESSION-ID             还原 vault 批次（部分失败 → exit 2）
  purge SESSION-ID            彻底删除 vault 批次副本（部分失败 → exit 2）
  vault P1 [P2...]            手动安全删除：任意路径走守卫+暂存区+台账（可还原）
  sweep [--days N]            清扫超过 N 天后悔期的 vault 批次（缺省 7）
  bigfiles PATH [--top N] [--json]   大文件 Top-N（缺省 50 条，仅 ≥1MB）
  dupes PATH [--min-mb N] [--json]   重复文件组（XXH3 内容级，缺省 ≥1MB）
  show REPORT                 展示历史报告
  rules [--md]                规则列表 / Markdown 手册

内部/辅助命令:
  tree PATH [--depth N] [--json]     [内部] 目录体积树（GUI 雷达调试）
  startup list|disable|enable|enable-all
                                     [内部] 启动项管家（GUI 同内核通道）
  migrate plan|apply|undo            [内部] 存储迁移（GUI 迁移中心同通道）
  elevated-run --job SPEC            [内部] 提权 worker，由 apply --admin 拉起",
        env!("CARGO_PKG_VERSION")
    );
}

/// 非 admin 时过滤 admin_required 规则：直接查进程令牌提权态。
fn is_admin_probe() -> bool {
    zc_core::is_elevated()
}

fn save_report(rep: &ScanReport) -> Result<PathBuf> {
    let p = manifest::data_dir().join("sessions").join(format!("{}.json", rep.id));
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, serde_json::to_vec_pretty(rep)?)?;
    Ok(p)
}

fn load_report(path: &str) -> Result<ScanReport> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Other(format!("读取 {path} 失败: {e}")))?;
    Ok(serde_json::from_str(&raw)?)
}

/// 解析 `--json [FILE]`：返回 Some(Option(file))——无该 flag 为 None。
/// 下一个 token 存在、不以 -- 开头、且此前未出现同类参数时视为文件路径。
fn parse_json_flag(args: &[String], i: usize) -> (Option<Option<String>>, usize) {
    match args.get(i + 1) {
        Some(next) if !next.starts_with("--") => (Some(Some(next.clone())), i + 2),
        _ => (Some(None), i + 1),
    }
}

fn cmd_scan(args: &[String]) -> Result<ExitCode> {
    let mut want_admin = false;
    let mut json: Option<Option<String>> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--admin" => {
                want_admin = true;
                i += 1;
            }
            "--json" => {
                let (v, n) = parse_json_flag(args, i);
                json = v;
                i = n;
            }
            other => return Err(Error::Other(format!("scan 未知参数 {other}"))),
        }
    }
    let admin = is_admin_probe();
    if want_admin && !admin {
        return Err(Error::AdminRequired {
            reason: "scan --admin 需要在提权的终端中运行（右键「以管理员身份运行」后重试）；\
                     不带 --admin 的扫描将自动跳过系统级管理员规则"
                .into(),
        });
    }
    let say = |s: String| {
        if json.is_none() {
            println!("{s}");
        }
    };
    if !admin {
        say("· 未检测到管理员权限：管理员规则已跳过".into());
    }

    // v5：expand_all_with_opts 携带规则级 min_age_days，扫描端按 mtime 过滤
    let keep = |id: &str| admin || !zc_rules::find(id).is_some_and(|r| r.admin_required);
    let (all_pairs, all_ages) = zc_rules::expand_all_with_opts();
    let pairs: Vec<(String, String)> = all_pairs.into_iter().filter(|(id, _)| keep(id)).collect();
    let ages: std::collections::BTreeMap<String, u64> =
        all_ages.into_iter().filter(|(id, _)| keep(id)).collect();
    let rule_count = pairs
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    say(format!("启用规则 {rule_count} 条"));

    let handle = ScanHandle::default();
    let t0 = std::time::Instant::now();
    let mut last_flush = std::time::Instant::now();
    let mut rep = scanner::scan_with_opts(&pairs, &ages, &handle, |ev| {
        if let ScanEvent::Entry { files, bytes_seen } = ev {
            if json.is_none() && last_flush.elapsed().as_millis() > 250 {
                print!("\r扫描中… {:>9} 项 / {:<10}", files, human_size(bytes_seen));
                let _ = std::io::stdout().flush();
                last_flush = std::time::Instant::now();
            }
        }
    })?;
    zc_rules::filter_guards(&mut rep.findings);
    if json.is_none() {
        println!();
    }
    if rep.cancelled {
        return Err(Error::Cancelled { reason: "扫描被取消".into() });
    }

    let saved = save_report(&rep)?;
    match json {
        // 机器可读通道：报告全文进 FILE 或 stdout，人类输出全部闭嘴
        Some(target) => {
            let body = serde_json::to_vec_pretty(&rep)?;
            match target {
                Some(p) => std::fs::write(&p, &body)?,
                None => {
                    let mut so = std::io::stdout().lock();
                    so.write_all(&body)?;
                    so.write_all(b"\n")?;
                }
            }
        }
        None => {
            println!(
                "\n扫描完成 · 遍历 {} 文件 · 历时 {:.1}s",
                format_number(rep.files_seen),
                t0.elapsed().as_secs_f64()
            );
            print_findings(&rep);
            println!(
                "\n可清理合计: {} ({})",
                human_size(rep.cleanable_bytes()),
                format_number(rep.cleanable_count())
            );
            println!("报告已保存: {}", saved.display());
            println!("执行清理:   zclean apply \"{}\" --mode vault", saved.display());
        }
    }

    // 基准脚本消费的机器可读行（与本地化文本解耦，规避管道编码差异）
    if std::env::var_os("ZC_BENCH").is_some() {
        println!(
            "[bench] files={} duration_ms={} cleanable_bytes={} cleanable_count={}",
            rep.files_seen,
            rep.duration_ms,
            rep.cleanable_bytes(),
            rep.cleanable_count()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn print_findings(rep: &ScanReport) {
    println!("{:<28} {:>12} {:>10}", "规则", "体积", "条目数");
    println!("{}", "-".repeat(54));
    for f in &rep.findings {
        let name = zc_rules::find(&f.rule_id)
            .map(|r| r.name_zh)
            .unwrap_or(f.rule_id.as_str());
        println!(
            "{name:<24} {:>12} {:>8}",
            human_size(f.total_bytes()),
            format_number(f.total_count())
        );
    }
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in b.iter().enumerate() {
        if i > 0 && (b.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

fn parse_mode(v: Option<&String>) -> Result<CleanMode> {
    match v.map(|s| s.as_str()) {
        Some("trash") => Ok(CleanMode::RecycleBin),
        Some("vault") => Ok(CleanMode::Vault),
        other => Err(Error::Other(format!("--mode 仅支持 trash|vault，收到 {other:?}"))),
    }
}

fn cmd_apply(args: &[String]) -> Result<ExitCode> {
    // 解析: apply REPORT --mode MODE [--rules id1,id2] [--admin]
    let mut report_path: Option<String> = None;
    let mut mode = CleanMode::Vault;
    let mut rules: Option<Vec<String>> = None;
    let mut include_admin = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                mode = parse_mode(args.get(i + 1))?;
                i += 2;
            }
            "--rules" => {
                rules = Some(
                    args.get(i + 1)
                        .ok_or_else(|| Error::Other("--rules 需要参数".into()))?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                );
                i += 2;
            }
            "--admin" => {
                include_admin = true;
                i += 1;
            }
            p if !p.starts_with('-') => {
                report_path = Some(p.to_string());
                i += 1;
            }
            other => return Err(Error::Other(format!("未知参数 {other}"))),
        }
    }
    let path = report_path.ok_or_else(|| Error::Other("缺少报告路径".into()))?;
    let rep = load_report(&path)?;

    // 未显式指定规则时默认只清 Safe 档（UI 接管后由用户勾选）
    let mut only: Vec<String> = rules.unwrap_or_else(|| {
        rep.findings
            .iter()
            .filter_map(|f| {
                zc_rules::find(&f.rule_id)
                    .filter(|r| r.risk == Risk::Safe)
                    .map(|_| f.rule_id.clone())
            })
            .collect()
    });

    let mode_cn = match mode {
        CleanMode::RecycleBin => "回收站",
        CleanMode::Vault => "vault 暂存区",
    };

    let is_admin_rule = |id: &str| zc_rules::find(id).is_some_and(|r| r.admin_required);
    let admin_sel: Vec<&String> = only.iter().filter(|id| is_admin_rule(id)).collect();

    // v5：勾选含管理员规则且未提权 → 必须显式 --admin 走 UAC 批，
    // 不再「自行剔除 admin 规则 + 提示」的静默降级分支（言行一致）。
    if !admin_sel.is_empty() && !is_admin_probe() && !include_admin {
        return Err(Error::AdminRequired {
            reason: format!(
                "勾选规则含 {} 条管理员规则（目标在系统禁删区），当前进程未提权。\
                 请在提权终端重跑，或添加 --admin 走一次性 UAC 提权批",
                admin_sel.len()
            ),
        });
    }

    let mut worker_err: Option<String> = None;
    // --admin 且未提权：管理员规则拆分给一次性 UAC worker，其余本进程执行。
    // （已提权时不拆分：内核 elevated guard 白名单自动生效，整批直接 apply。）
    if include_admin && !is_admin_probe() && !admin_sel.is_empty() {
        let (user_rules, admin_rules): (Vec<String>, Vec<String>) =
            only.clone().into_iter().partition(|id| !is_admin_rule(id));
        println!(
            "◆ {} 条管理员规则 → 一次性 UAC 提权批",
            admin_rules.len()
        );
        let spec = elevate::JobSpec::new(
            format!("elev-{}", rep.id),
            elevate::JobAction::CleanRules {
                report_path: path.clone(),
                rule_ids: admin_rules,
                mode,
            },
        );
        match elevate::run_elevated(
            &std::env::current_exe()?,
            &spec,
            std::time::Duration::from_secs(15 * 60),
        ) {
            Ok(res) if res.success => {
                if let Some(o) = res.outcome {
                    println!(
                        "  ↳ 提权批完成: {} 项 / {}",
                        o.done_files,
                        human_size(o.done_bytes)
                    );
                    history::append(&HistoryRecord {
                        session_id: spec.id.clone(),
                        created_unix: now_unix(),
                        mode,
                        files: o.done_files,
                        bytes_moved: o.done_bytes,
                        kind: Some("elevated_batch".to_string()),
                        ..Default::default()
                    })?;
                }
            }
            Ok(res) => worker_err = Some(format!("提权批失败: {}", res.message)),
            Err(e) => worker_err = Some(format!("提权批未完成: {e}")),
        }
        only = user_rules;
    }

    println!(
        "模式: {mode_cn} · 计划批次 {} 条规则\n守卫校验中…",
        only.len()
    );
    let mut code = ExitCode::SUCCESS;
    if !only.is_empty() {
        let outcome = executor::apply(&rep, &only, mode)?;
        history::append(&HistoryRecord {
            session_id: rep.id.clone(),
            created_unix: now_unix(),
            mode,
            files: outcome.done_files,
            bytes_moved: outcome.done_bytes,
            kind: Some("clean".to_string()),
            ..Default::default()
        })?;

        println!(
            "\n完成 {}/{} 项 · 计 {} (移入)",
            outcome.done_files,
            outcome.requested_files,
            human_size(outcome.done_bytes)
        );
        if !outcome.failed.is_empty() {
            println!("失败 {} 项：", outcome.failed.len());
            for (p, e) in outcome.failed.iter().take(20) {
                println!("  ✗ {p} — {e}");
            }
        }
        println!("\n▶ {}", outcome.semantics_note);
        if mode == CleanMode::Vault {
            println!("▶ 反悔通道: zclean undo {}", rep.id);
        }
        code = decide_exit_code(outcome.failed.len(), false);
    } else {
        println!("（本进程无用户态规则可执行——全部为提权批条目）");
    }
    // 提权批出错：用户批的部分失败(2)不得掩盖真实错误 → 收敛到 1
    if let Some(msg) = worker_err {
        eprintln!("错误: {msg}");
        code = ExitCode::FAILURE;
    }
    Ok(code)
}

/// [内部] 提权 worker：外层 catch_unwind 兜底（审计 T4-③：worker panic
/// 则 launcher 干等 15 分钟）——任何阶段 panic 都回写 failed 结果文件；
/// 结果经原子写（tmp+rename）落盘，200ms 轮询不再可能读到半截 JSON。
fn cmd_elevated_run(args: &[String]) -> ExitCode {
    let spec_path = match args
        .iter()
        .position(|a| a == "--job")
        .and_then(|i| args.get(i + 1))
    {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("elevated-run 需要 --job <spec.json>");
            return ExitCode::FAILURE;
        }
    };
    let out_path = elevate::result_path_for(&spec_path);

    let res: elevate::JobResult = std::panic::catch_unwind(|| {
        if !zc_core::is_elevated() {
            return elevate::JobResult::fail("", "", "elevated-run 必须在提升进程中运行");
        }
        let raw = match std::fs::read_to_string(&spec_path) {
            Ok(r) => r,
            Err(e) => {
                return elevate::JobResult::fail("", "", &format!("读取任务失败: {e}"));
            }
        };
        let spec: elevate::JobSpec = match serde_json::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                return elevate::JobResult::fail("", "", &format!("任务反序列化失败: {e}"));
            }
        };
        elevate::execute_as_worker(&spec)
    })
    .unwrap_or_else(|panic| {
        let msg = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "未知 panic".to_string());
        elevate::JobResult::fail("", "", &format!("worker panic: {msg}"))
    });

    if let Err(e) = elevate::write_result_atomic(&out_path, &res) {
        eprintln!("结果回写失败: {e}");
        return ExitCode::FAILURE;
    }
    println!("{}", res.message);
    if res.success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn cmd_undo(args: &[String]) -> Result<ExitCode> {
    let id = args.get(1).ok_or_else(|| Error::Other("用法: zclean undo SESSION-ID".into()))?;
    let m = CleanManifest::load(id)?;
    let (done, failed) = m.undo()?;
    println!("已还原 {done}/{} 项", m.entries.len());
    for (p, e) in failed.iter().take(20) {
        println!("  ✗ {} — {e}", p.display());
    }
    Ok(decide_exit_code(failed.len(), false))
}

fn cmd_purge(args: &[String]) -> Result<ExitCode> {
    let id = args.get(1).ok_or_else(|| Error::Other("用法: zclean purge SESSION-ID".into()))?;
    let m = CleanManifest::load(id)?;
    let (deleted, freed, failed) = m.purge_forever()?;
    println!("已彻底删除 {deleted} 项，实际释放 {} 字节", format_number(freed));
    for (p, e) in failed.iter().take(20) {
        println!("  ✗ {p} — {e}");
    }
    if !failed.is_empty() {
        println!("  （{} 项保留台账，可重试或照常还原）", failed.len());
    }
    Ok(decide_exit_code(failed.len(), false))
}

fn cmd_vault(args: &[String]) -> Result<ExitCode> {
    let paths: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        return Err(Error::Other("用法: zclean vault P1 [P2...]".into()));
    }
    let existing: Vec<&Path> = paths.iter().map(|p| p.as_path()).filter(|p| p.exists()).collect();
    if existing.is_empty() {
        return Err(Error::Other("所有路径都不存在".into()));
    }
    crate::guard_check(&existing)?;
    // v5 S3：批次 id 必须含随机熵（秒级时间戳碰撞会整批抹账）；
    // 搬运走 stash_journal（move 前落台账），不再「先搬后记账」。
    let session = format!("manual-{}", zc_core::scanner::new_session_id());
    let session_dir = zc_core::executor::vault::vault_session_dir(&session);
    let ledger = zc_core::ledger::LedgerStore::open()?;
    let (ok, failed) =
        zc_core::executor::vault::stash_journal(&session_dir, &existing, &ledger, &session)?;
    let bytes: u64 = ok.iter().map(|(_, _, s)| *s).sum();
    // 与 GUI vault_delete 同一历史口径：搬运事实进 history（7 天后悔期）
    let _ = history::append(&HistoryRecord {
        session_id: session.clone(),
        created_unix: now_unix(),
        mode: executor::CleanMode::Vault,
        files: ok.len() as u64,
        bytes_moved: bytes,
        kind: Some("manual_vault".to_string()),
        ..Default::default()
    });
    println!(
        "已移入暂存区 {} 项 / {} 字节；反悔通道: zclean undo {}",
        ok.len(),
        format_number(bytes),
        session
    );
    for (p, e) in failed.iter().take(20) {
        println!("  ✗ {} — {e}", p.display());
    }
    Ok(decide_exit_code(failed.len(), false))
}

fn guard_check(paths: &[&Path]) -> Result<()> {
    zc_core::guard::Guard::new().vet(paths.iter().copied())
}

fn cmd_sweep(args: &[String]) -> Result<ExitCode> {
    // sweep [--days N]：后悔期透传内核（sweep_expired 本身以天为参数）
    let mut days = 7u64;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--days" => {
                days = args
                    .get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::Other("--days 需要非负整数（0=立即收走全部已落账批次）".into()))?;
                i += 2;
            }
            other => return Err(Error::Other(format!("sweep 未知参数 {other}"))),
        }
    }
    let s = zc_core::executor::vault::sweep_expired(days).map_err(Error::Other)?;
    if s.sessions == 0 {
        println!("没有超过 {days} 天后悔期的 vault 批次");
    } else {
        println!(
            "已清扫 {} 个过期批次：{} 项 / {} 字节{}",
            s.sessions,
            format_number(s.items as u64),
            format_number(s.bytes),
            if s.gc_skipped {
                "（孤儿目录 GC 因台账读取失败被熔断，未动任何目录）"
            } else {
                ""
            }
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_show(args: &[String]) -> Result<ExitCode> {
    let p = args.get(1).ok_or_else(|| Error::Other("用法: zclean show REPORT.json".into()))?;
    let rep = load_report(p)?;
    println!("会话 {} · 扫描用时 {}ms", rep.id, rep.duration_ms);
    print_findings(&rep);
    Ok(ExitCode::SUCCESS)
}

fn cmd_rules(args: &[String]) -> Result<ExitCode> {
    if args.iter().any(|a| a == "--md") {
        println!("# 清理规则手册");
        println!();
        println!(
            "> 本文档由 `zclean rules --md` 自动生成，请勿手改。\n\
             > 源文件：`crates/zc-rules/src/lib.rs`\n"
        );
        println!("共 **{}** 条内置规则。", zc_rules::RULES.len());
        println!("默认只勾选「安全」档；风险档位见下表；标注 ⚙ 的规则需要管理员权限。");
        println!();
        println!("| ID | 名称 | 域 | 风险 |");
        println!("| --- | --- | --- | :--: |");
        for r in zc_rules::RULES {
            let risk = match r.risk {
                Risk::Safe => "安全",
                Risk::Caution => "注意",
                Risk::Risky => "**风险**",
                Risk::Expert => "**专家**",
            };
            let dom = match r.domain {
                Domain::System => "system",
                Domain::Browser => "browser",
                Domain::Dev => "dev",
                Domain::Apps => "apps",
                Domain::Logs => "logs",
            };
            let gear = if r.admin_required { " ⚙" } else { "" };
            println!("| `{}` | {}{gear} | {dom} | {risk} |", r.id, r.name_zh);
        }
        return Ok(ExitCode::SUCCESS);
    }

    println!("{:<30} {:<8} {:<6}", "ID", "域", "风险");
    for r in zc_rules::RULES {
        let risk = match r.risk {
            Risk::Safe => "安全",
            Risk::Caution => "注意",
            Risk::Risky => "风险",
            Risk::Expert => "专家",
        };
        let dom = match r.domain {
            Domain::System => "system",
            Domain::Browser => "browser",
            Domain::Dev => "dev",
            Domain::Apps => "apps",
            Domain::Logs => "logs",
        };
        let flag = if r.admin_required { " [admin]" } else { "" };
        println!("{:<30} {:<8} {:<6} {}{flag}", r.id, dom, risk, r.name_zh);
    }
    Ok(ExitCode::SUCCESS)
}

// ── 工具箱：tree / bigfiles / dupes / startup / migrate ─────────────────────

fn cmd_tree(args: &[String]) -> Result<ExitCode> {
    let mut path: Option<String> = None;
    let mut depth = 4u32;
    let mut as_json = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--depth" => { depth = args.get(i+1).and_then(|s| s.parse().ok()).unwrap_or(4); i += 2; }
            "--json" => { as_json = true; i += 1; }
            p if !p.starts_with('-') && path.is_none() => { path = Some(p.to_string()); i += 1; }
            other => return Err(Error::Other(format!("未知参数 {other}"))),
        }
    }
    let root = Path::new(path.as_deref().ok_or_else(|| Error::Other("用法: zclean tree <目录> [--depth N] [--json]".into()))?);
    use zc_core::analyze::{build_tree, TreeOptions};
    let t = build_tree(root, TreeOptions { max_depth: depth, max_children: 40 })
        .map_err(|e| Error::Other(e.to_string()))?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&t)?);
        return Ok(ExitCode::SUCCESS);
    }
    fn walk(n: &zc_core::analyze::TreeNode, indent: usize) {
        if n.size == 0 && n.children.is_empty() { return; }
        println!(
            "{:>10} {:indent$}{name}",
            human_size(n.size),
            "",
            indent = indent * 2,
            name = n.name
        );
        for c in &n.children {
            walk(c, indent + 1);
        }
    }
    walk(&t, 0);
    Ok(ExitCode::SUCCESS)
}

/// bigfiles PATH [--top N] [--json]：大文件 Top-N（接内核 largest_files）。
fn cmd_bigfiles(args: &[String]) -> Result<ExitCode> {
    let mut path: Option<String> = None;
    let mut top = 50usize;
    let mut as_json = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--top" => {
                top = args
                    .get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .filter(|t: &usize| *t > 0)
                    .ok_or_else(|| Error::Other("--top 需要正整数".into()))?;
                i += 2;
            }
            "--json" => { as_json = true; i += 1; }
            p if !p.starts_with('-') && path.is_none() => { path = Some(p.to_string()); i += 1; }
            other => return Err(Error::Other(format!("bigfiles 未知参数 {other}"))),
        }
    }
    let root = Path::new(
        path.as_deref()
            .ok_or_else(|| Error::Other("用法: zclean bigfiles <目录> [--top N] [--json]".into()))?,
    );
    if !root.is_dir() {
        return Err(Error::Other(format!("目录不存在: {}", root.display())));
    }
    let files = zc_core::analyze::largest_files(root, top, 1024 * 1024)
        .map_err(|e| Error::Other(e.to_string()))?;
    if as_json {
        let dto: Vec<serde_json::Value> = files
            .iter()
            .map(|(p, s)| serde_json::json!({ "path": p.to_string_lossy(), "size": s }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&dto)?);
        return Ok(ExitCode::SUCCESS);
    }
    println!("大文件 Top-{}（≥1MB）", top);
    for (p, s) in &files {
        println!("{:>12}  {}", human_size(*s), p.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_dupes(args: &[String]) -> Result<ExitCode> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut min_mb = 1u64;
    let mut as_json = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--min-mb" => { min_mb = args.get(i+1).and_then(|s| s.parse().ok()).unwrap_or(1); i += 2; }
            "--json" => { as_json = true; i += 1; }
            p if !p.starts_with('-') => { paths.push(PathBuf::from(p)); i += 1; }
            other => return Err(Error::Other(format!("未知参数 {other}"))),
        }
    }
    if paths.is_empty() {
        return Err(Error::Other("用法: zclean dupes <目录...> [--min-mb N] [--json]".into()));
    }

    let t0 = std::time::Instant::now();
    let groups = zc_core::dedup::find_duplicates(
        &paths,
        &zc_core::dedup::DupOptions { min_size: min_mb * 1024 * 1024 },
    )?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&groups)?);
        return Ok(ExitCode::SUCCESS);
    }
    let waste: u64 = groups.iter().map(|g| g.size * (g.files.len() as u64 - 1)).sum();
    println!(
        "发现 {} 组重复（≥{}MB）· 可回收 ≈ {} · 耗时 {:.1}s",
        groups.len(), min_mb, human_size(waste), t0.elapsed().as_secs_f64()
    );
    for (gi, g) in groups.iter().enumerate().take(50) {
        println!("\n#{:<3} {} ×{}", gi + 1, human_size(g.size), g.files.len());
        // 建议保留最新的一个（修改时间最大；无法读取时间的一律不标保留）
        let times: Vec<Option<std::time::SystemTime>> = g.files.iter()
            .map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
            .collect();
        let newest = times.iter().flatten().max().copied();
        for (f, mt) in g.files.iter().zip(&times) {
            let keep = newest.is_some() && mt.is_some() && *mt == newest;
            println!("  {} {}", if keep { "保留→" } else { "      " }, f.display());
        }
    }
    if groups.len() > 50 {
        println!("\n… 另有 {} 组未列出", groups.len() - 50);
    }
    Ok(ExitCode::SUCCESS)
}
use std::fs;

fn cmd_startup(args: &[String]) -> Result<ExitCode> {
    match args.get(1).map(|s| s.as_str()) {
        Some("list") | None => {
            let entries = zc_core::startup::list_user_startup().map_err(|e| Error::Other(e.to_string()))?;
            println!("{:<6} {:<24}", "#", "键名");
            for (i, e) in entries.iter().enumerate() {
                println!(
                    "{:<6} {:<24} {}",
                    i,
                    e.name.chars().take(22).collect::<String>(),
                    e.command.chars().take(70).collect::<String>()
                );
            }
            let d = zc_core::startup::disabled_count()?;
            if d > 0 {
                println!("\n已禁用 {d} 项（enable / enable-all 可恢复）：");
                for e in zc_core::startup::list_disabled()? {
                    println!("  ✂ {} — {}", e.key_id, e.value.chars().take(60).collect::<String>());
                }
            }
        }
        Some("disable") => {
            let key = args.get(2).ok_or_else(|| Error::Other("用法: startup disable <index|名称>".into()))?;
            let entries = zc_core::startup::list_user_startup().map_err(|e| Error::Other(e.to_string()))?;
            let target = entries.iter()
                .find(|e| &e.name == key)
                .or_else(|| key.parse::<usize>().ok().and_then(|ix| entries.get(ix)));
            let t = target.ok_or_else(|| Error::Other("找不到该启动项".into()))?;
            let changed = zc_core::startup::disable(&t.key_id)?;
            println!("{}", if changed { format!("已禁用「{}」并备份", t.name) } else { "无需变更".into() });
        }
        Some("enable-all") => {
            // v5：逐项明细，失败项保留备份可重试
            let s = zc_core::startup::enable_all()?;
            println!("已恢复 {} 个被禁用的启动项", s.restored);
            for (k, e) in s.failed.iter().take(20) {
                println!("  ✗ {k} — {e}");
            }
            if !s.failed.is_empty() {
                println!("  （{} 项写回失败，已保留在备份中可重试）", s.failed.len());
            }
            return Ok(decide_exit_code(s.failed.len(), false));
        }
        Some("enable") => {
            // 单条恢复（v5）：成功才从备份移除；未知 id → false
            let key = args.get(2).ok_or_else(|| Error::Other("用法: startup enable <KEY-ID>".into()))?;
            let done = zc_core::startup::enable_one(key)?;
            println!("{}", if done { format!("已恢复「{key}」") } else { "备份中无此条目".into() });
            if !done {
                return Ok(ExitCode::from(2));
            }
        }
        Some(other) => return Err(Error::Other(format!("未知子命令 startup {other}（list|disable|enable|enable-all）"))),
    }
    Ok(ExitCode::SUCCESS)
}

/// 迁移相位 → 行号与中文文案（与 UI 阶段条一一对应）
fn migrate_phase_row(p: zc_core::migrate::MigratePhase) -> (usize, &'static str) {
    use zc_core::migrate::MigratePhase as P;
    match p {
        P::Copy => (1, "正在复制内容"),
        P::Verify => (2, "尺寸校验"),
        P::Link => (3, "建立 junction"),
        P::Smoke => (4, "冒烟验证"),
        P::Cleanup => (5, "清理备份"),
    }
}

fn cmd_migrate(args: &[String]) -> Result<ExitCode> {
    let action = args.get(1).map(|s| s.as_str()).unwrap_or("plan");
    let src = PathBuf::from(args.get(2).cloned().unwrap_or_default());
    if action != "undo" {
        let dst_root = PathBuf::from(args.get(3).cloned().unwrap_or_default());
        if src.as_os_str().is_empty() || dst_root.as_os_str().is_empty() {
            return Err(Error::Other("用法: migrate plan|apply <源目录> <目标盘根/父目录>\n       migrate undo <原路径> [目标目录]".into()));
        }
        let plan = zc_core::migrate::plan(&src, &dst_root)?;
        println!(
            "迁移计划：{} → {}\n体积 {} · 文件 {}",
            plan.src.display(), plan.dst.display(),
            human_size(plan.total_bytes), plan.total_files
        );
        if action == "apply" {
            if !args.contains(&"--yes".to_string()) {
                return Err(Error::Other("apply 需要显式 --yes 确认（含自动回滚保障）".into()));
            }
            let res = zc_core::migrate::apply_with_phases(&plan, &mut |phase, state| {
                let (n, label) = migrate_phase_row(phase);
                match state {
                    zc_core::migrate::PhaseState::Start => print!("\r[{n}/5] {label}…"),
                    zc_core::migrate::PhaseState::End => println!("\r[{n}/5] {label} ✓ 完成"),
                }
                let _ = std::io::stdout().flush();
            })
            .map_err(|e| Error::Other(e.to_string()));
            // 失败路径不补发 End：把悬着的进行中行收掉再上抛
            let id = match res {
                Ok(id) => id,
                Err(e) => {
                    println!();
                    return Err(e);
                }
            };
            // 与 GUI 同一历史口径：kind=migrate + src/dst 行，历史页可事后撤销
            let _ = history::append(&HistoryRecord {
                session_id: id.clone(),
                created_unix: now_unix(),
                mode: executor::CleanMode::Vault,
                files: plan.total_files,
                bytes_moved: plan.total_bytes,
                kind: Some("migrate".to_string()),
                src: Some(plan.src.display().to_string()),
                dst: Some(plan.dst.display().to_string()),
            });
            println!("✓ 迁移完成，junction 已建立。清单 id={id}");
            println!("  undo 方式: zclean migrate undo \"{}\"", plan.src.display());
        }
        return Ok(ExitCode::SUCCESS);
    }
    // undo：`migrate undo <原路径> [目标目录]`（带 dst 时不依赖清单定位）
    let src_ref: &String = if src.as_os_str().is_empty() {
        args.get(2).or_else(|| args.get(3)).ok_or_else(|| Error::Other("migrate undo 需要原路径".into()))?
    } else {
        args.get(2).ok_or_else(|| Error::Other("migrate undo 需要原路径".into()))?
    };
    let dst_arg = args.get(3).cloned();
    let msg = zc_core::migrate::undo(
        Path::new(src_ref),
        if src.as_os_str().is_empty() { None } else { dst_arg.as_deref().map(Path::new) },
    )
    .map_err(|e| Error::Other(e))?;
    let _ = history::append(&HistoryRecord {
        session_id: format!("migrate-undo-{}", zc_core::scanner::new_session_id()),
        created_unix: now_unix(),
        mode: executor::CleanMode::Vault,
        files: 1,
        bytes_moved: 0,
        kind: Some("migrate_undo".to_string()),
        src: Some(src_ref.clone()),
        dst: dst_arg,
    });
    println!("{msg}");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_number_groups() {
        assert_eq!(super::format_number(1234567), "1,234,567");
        assert_eq!(super::format_number(999), "999");
    }

    #[test]
    fn parse_mode_rejects_unknown() {
        let v = String::from("nuke");
        assert!(super::parse_mode(Some(&v)).is_err());
        let ok = String::from("vault");
        assert!(super::parse_mode(Some(&ok)).is_ok());
    }

    /// 退出码约定（CONTRACT §B12）：0 全成 / 2 部分失败 / 3 取消，取消优先。
    #[test]
    fn exit_code_decision() {
        assert_eq!(decide_exit_code(0, false), ExitCode::SUCCESS);
        assert_eq!(decide_exit_code(3, false), ExitCode::from(2));
        assert_eq!(decide_exit_code(0, true), ExitCode::from(3));
        assert_eq!(decide_exit_code(5, true), ExitCode::from(3), "取消标志必须压过部分失败");
    }

    #[test]
    fn json_flag_parses_optional_file() {
        let a: Vec<String> = ["scan", "--json", "C:\\t\\x.json"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_json_flag(&a, 1).0, Some(Some("C:\\t\\x.json".into())));
        assert_eq!(parse_json_flag(&a, 1).1, 3);
        let b: Vec<String> = ["scan", "--json"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_json_flag(&b, 1).0, Some(None));
        let c: Vec<String> = ["scan", "--json", "--admin"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_json_flag(&c, 1).0, Some(None), "-- 开头的 token 不得被吞成文件名");
    }
}
