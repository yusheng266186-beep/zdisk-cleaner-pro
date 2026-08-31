//! 提权任务协议：一次性 elevated worker（v5 加固版，审计 T4）。
//!
//! 流程（最小提权原则）：
//! 1. 主进程把任务序列化为 `JobSpec`（携带随机 nonce）原子写入临时 JSON；
//! 2. PowerShell 经 **`-EncodedCommand`（UTF-16LE base64）** 执行
//!    `Start-Process -Verb RunAs` 拉起同一个 exe 的 `elevated-run` 子命令
//!    （UAC 只授权这一次进程，无常驻服务；编码通道彻底免疫引号注入——
//!    旧的单引号裸插值在路径含 `'` 时即断裂且是注入面）；
//! 3. worker fail-closed 校验（必须提权 + 动作白名单），执行后把携带
//!    **同一 nonce** 的 `JobResult` 原子回写（tmp+rename）；
//! 4. 主进程轮询结果文件：nonce 不匹配 = 伪造/陈旧 → 显式拒绝；
//!    带 worker 进程存活检测（Start-Process -PassThru 回传 PID，
//!    OpenProcess+GetExitCodeProcess）——worker 已死而无结果 = 异常终止，
//!    立刻报错而不是干等到超时；UAC 拒绝（无 PID 无结果）= 显式取消；
//! 5. spec/result 临时文件在拿到结果后清理。
//!
//! 提权动作本身（CleanRules）在 worker 内走 `executor::apply`，管理员
//! 目录豁免由内核 elevated guard allowlist 兑现（白名单在核内，不在壳）。

use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zc_core::{
    executor::{self, CleanMode, CleanOutcome},
    models::ScanReport,
    Error, Result,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum JobAction {
    /// 按已有报告清理选中规则（提权批）
    CleanRules {
        report_path: String,
        rule_ids: Vec<String>,
        mode: CleanMode,
    },
}

impl JobAction {
    /// 动作类型标签（调试与日志用）
    #[allow(dead_code)]
    pub fn kind(&self) -> &'static str {
        match self {
            JobAction::CleanRules { .. } => "clean_rules",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub id: String,
    #[serde(default)]
    pub created_unix: u64,
    /// 一次性随机数：spec 生成、result 回带，launcher 校验绑定
    /// （同目录里预写的伪造 result.json 因 nonce 不符被拒）。
    #[serde(default)]
    pub nonce: String,
    pub action: JobAction,
}

impl JobSpec {
    pub fn new(id: String, action: JobAction) -> Self {
        Self { id, created_unix: zc_core::now_unix(), nonce: gen_nonce(), action }
    }
}

/// 生成 nonce：纳秒时钟 × PID × 单调计数的混合熵，hex 输出。
/// 用途是防同用户进程盲猜文件名伪造结果（TOCTOU 锚定），不承载秘密密钥强度。
fn gen_nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TICK: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let a = nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (std::process::id() as u64).wrapping_shl(17)
        ^ TICK.fetch_add(1, Ordering::SeqCst).wrapping_mul(0xBF03_6391);
    let b = a.rotate_left(29) ^ nanos << 3 ^ 0xA5A5_5A5A;
    format!("{a:016x}{b:016x}")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobResult {
    pub spec_id: String,
    /// 回带 spec 的 nonce；launcher 以此锚定结果归属
    #[serde(default)]
    pub nonce: String,
    pub success: bool,
    /// 人类可读摘要（成功或失败原因）
    pub message: String,
    /// 存在时为清理统计；序列化始终携带（None 亦输出 null）
    pub outcome: Option<CleanOutcome>,
}

impl JobResult {
    pub fn fail(spec_id: &str, nonce: &str, message: &str) -> Self {
        Self {
            spec_id: spec_id.to_string(),
            nonce: nonce.to_string(),
            success: false,
            message: message.to_string(),
            outcome: None,
        }
    }
}

/// spec 路径 → result 路径（协议常量，launcher 与 worker 共用）。
pub fn result_path_for(spec_path: &Path) -> PathBuf {
    let s = spec_path.to_string_lossy();
    PathBuf::from(s.replace(".spec.json", ".result.json"))
}

/// 原子写结果：tmp 落盘 + rename 顶替。
/// 轮询方永远只会看到「不存在」或「完整 JSON」两态（审计 T4-④）。
pub fn write_result_atomic(path: &Path, res: &JobResult) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(serde_json::to_vec_pretty(res)?.as_slice())?;
        f.flush()?;
    }
    // rename 目标已存在时 Windows 会失败：先清掉再顶上
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/* ── PowerShell -EncodedCommand 通道 ─────────────────────── */

/// 标准 base64（无换行）。零依赖手写：删掉一整类注入面比引一个 crate 划算。
fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// 把 PowerShell 脚本编译成 `-EncodedCommand` 参数对：
/// UTF-16LE → base64。脚本与路径里出现的任何引号/反引号都只活在
/// base64 里，命令行层不存在解析面。
fn encoded_command(script: &str) -> Vec<String> {
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-EncodedCommand".into(),
        base64_encode(&utf16),
    ]
}

/// 组装 launcher 脚本：RunAs 拉起 worker，回传 worker PID（-PassThru）。
/// exe/spec 路径以 PowerShell 单引号字面量书写（'' 转义），脚本本体
/// 经 EncodedCommand 传输后这层引号不再暴露给任何中间 shell。
pub(crate) fn launcher_script(exe: &Path, spec_path: &Path) -> String {
    let q = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "''"));
    format!(
        "$ErrorActionPreference='Stop'; \
         $p = Start-Process -FilePath {} -ArgumentList 'elevated-run','--job',{} -Verb RunAs -WindowStyle Hidden -PassThru; \
         if ($p) {{ [Console]::Out.Write($p.Id) }}",
        q(exe),
        q(spec_path)
    )
}

/* ── worker 进程存活检测（windows-sys）───────────────────── */

/// PID 是否仍在运行且未退出。拿不到句柄（进程已回收）或退出码可读
/// （≠ STILL_ACTIVE）都判死——宁误判死为快报错，不干等 15 分钟。
fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == STILL_ACTIVE as u32
    }
}

/* ── launcher / worker 两端 ─────────────────────────────── */

/// 主进程入口：原子写 spec → EncodedCommand 拉起 worker → 轮询结果
/// （nonce 校验 + worker 死亡检测）→ 清理临时文件。
pub fn run_elevated(exe: &Path, spec: &JobSpec, poll_timeout: Duration) -> Result<JobResult> {
    let dir = std::env::temp_dir().join("ZDiskCleanerPro3-jobs");
    std::fs::create_dir_all(&dir)?;
    let spec_path = dir.join(format!("{}.spec.json", spec.id));
    let result_path = result_path_for(&spec_path);
    let _ = std::fs::remove_file(&result_path);

    // spec 同样原子写：worker 读到的必须是完整 JSON
    {
        let tmp = spec_path.with_extension("tmp");
        std::fs::File::create(&tmp)?.write_all(serde_json::to_vec_pretty(spec)?.as_slice())?;
        let _ = std::fs::remove_file(&spec_path);
        std::fs::rename(&tmp, &spec_path)?;
    }

    let ps = encoded_command(&launcher_script(exe, &spec_path));
    let out = std::process::Command::new("powershell.exe").args(&ps).output()?;
    let worker_pid: Option<u32> = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .ok();

    // UAC 拒绝（PS 报错退出且无 PID）且结果不存在 → 显式取消
    if !out.status.success() && worker_pid.is_none() && !result_path.exists() {
        let _ = std::fs::remove_file(&spec_path);
        return Err(Error::Cancelled {
            reason: "UAC 已取消或提权拉起失败".into(),
        });
    }

    let deadline = Instant::now() + poll_timeout;
    loop {
        if result_path.exists() {
            let read = std::fs::read_to_string(&result_path)
                .map_err(|e| Error::Other(format!("读取提权结果失败: {e}")));
            match read.and_then(|raw| {
                serde_json::from_str::<JobResult>(&raw)
                    .map_err(|e| Error::Other(format!("提权结果解析失败: {e}")))
            }) {
                Ok(res) => {
                    if res.nonce != spec.nonce {
                        // 伪造/陈旧结果：删除重等；worker 真结果只会带对的 nonce
                        let _ = std::fs::remove_file(&result_path);
                        return Err(Error::Other(
                            "提权结果与任务 nonce 不符（疑似伪造或陈旧残留），已拒绝".into(),
                        ));
                    }
                    let _ = std::fs::remove_file(&result_path);
                    let _ = std::fs::remove_file(&spec_path);
                    return Ok(res);
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&result_path);
                    return Err(e);
                }
            }
        }
        // worker 死亡检测：进程已退出却没有结果 → 立刻止损
        if let Some(pid) = worker_pid {
            if !pid_alive(pid) {
                let _ = std::fs::remove_file(&spec_path);
                return Err(Error::External(
                    "提权 worker 未写结果即退出（可能被拦截或异常终止）".into(),
                ));
            }
        }
        if Instant::now() > deadline {
            let _ = std::fs::remove_file(&spec_path);
            return Err(Error::External(
                "worker 超时未回写结果（可能被杀毒软件拦截或用户长时间未确认）".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// worker 入口（子命令 elevated-run，外层另有 catch_unwind 兜底）：
/// 校验提权态 → 执行白名单动作 → 返回携带 nonce 的结果。
pub fn execute_as_worker(spec: &JobSpec) -> JobResult {
    let finish = |success: bool, message: String, outcome: Option<CleanOutcome>| JobResult {
        spec_id: spec.id.clone(),
        nonce: spec.nonce.clone(),
        success,
        message,
        outcome,
    };

    if !zc_core::is_elevated() {
        return finish(false, "特权动作必须在提升的进程中执行".into(), None);
    }

    match &spec.action {
        JobAction::CleanRules { report_path, rule_ids, mode } => {
            let raw = match std::fs::read_to_string(report_path) {
                Ok(r) => r,
                Err(e) => return finish(false, format!("读取报告失败: {e}"), None),
            };
            let rep: ScanReport = match serde_json::from_str(&raw) {
                Ok(r) => r,
                Err(e) => return finish(false, format!("报告反序列化失败: {e}"), None),
            };
            // 执行器内部包含 fail-closed 守卫；提权进程中守卫自动挂上
            // elevated allowlist（目录级白名单在核内，见 zc-core::guard）。
            match executor::apply(&rep, rule_ids, *mode) {
                Ok(outcome) => finish(
                    true,
                    format!(
                        "完成 {}/{} 项",
                        outcome.done_files, outcome.requested_files
                    ),
                    Some(outcome),
                ),
                Err(e) => finish(false, e.to_string(), None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec(action: JobAction) -> JobSpec {
        JobSpec::new(format!("t-{:x}", std::process::id()), action)
    }

    #[test]
    fn spec_result_roundtrip_serde() {
        let spec = sample_spec(JobAction::CleanRules {
            report_path: "C:/tmp/r.json".into(),
            rule_ids: vec!["sys-user-temp".into()],
            mode: CleanMode::Vault,
        });
        let s = serde_json::to_string(&spec).unwrap();
        let back: JobSpec = serde_json::from_str(&s).unwrap();
        assert_eq!(back.action.kind(), "clean_rules");
        assert_eq!(back.nonce, spec.nonce);

        let res = JobResult {
            spec_id: spec.id.clone(),
            nonce: spec.nonce.clone(),
            success: false,
            message: "x".into(),
            outcome: None,
        };
        let b: JobResult = serde_json::from_str(&serde_json::to_string(&res).unwrap()).unwrap();
        assert!(!b.success);
        assert_eq!(b.nonce, spec.nonce);
    }

    #[test]
    fn nonce_is_unique_and_stable_width() {
        let a = gen_nonce();
        let b = gen_nonce();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b, "连续生成的 nonce 不得重复（计数熵）");
    }

    /// base64 参照 RFC4648 已知向量 + UTF-16LE 编码通道的注入免疫性：
    /// 路径含单引号时脚本字面量必须 '' 转义，且整个脚本以 base64 上送。
    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn encoded_command_roundtrips_utf16le() {
        let args = encoded_command("Write-Output '✓éO'");
        assert_eq!(args[2], "-EncodedCommand");
        let b64 = &args[3];
        // 解码（仅测试侧反推验证）：base64 → bytes → u16LE → String
        let bytes = base64_decode(b64);
        let units: Vec<u16> =
            bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "Write-Output '✓éO'");
    }

    #[test]
    fn launcher_script_escapes_single_quotes() {
        let s = launcher_script(
            Path::new(r"C:\Program Files\o'briens\zclean.exe"),
            Path::new(r"C:\Temp\it's-a.spec.json"),
        );
        assert!(s.contains(r"'C:\Program Files\o''briens\zclean.exe'"), "{s}");
        assert!(s.contains(r"'C:\Temp\it''s-a.spec.json'"), "{s}");
        // 一次性任务通道必须显式 RunAs + PassThru（死亡检测的前置）
        assert!(s.contains("-Verb RunAs") && s.contains("-PassThru"));
    }

    #[test]
    fn worker_refuses_non_elevated_process_in_tests() {
        // 单测进程几乎必然非提权；若恰为提权则跳过断言以保证确定性。
        if zc_core::is_elevated() {
            return;
        }
        let spec = sample_spec(JobAction::CleanRules {
            report_path: "definitely-missing.json".into(),
            rule_ids: vec![],
            mode: CleanMode::Vault,
        });
        let r = execute_as_worker(&spec);
        assert!(!r.success);
        assert!(r.message.contains("提升"), "{}", r.message);
        assert_eq!(r.spec_id, spec.id);
        assert_eq!(r.nonce, spec.nonce, "失败结果也必须回带 nonce（launcher 校验）");
    }

    #[test]
    fn result_path_mapping() {
        assert_eq!(
            result_path_for(Path::new(r"C:\t\job-1.spec.json")),
            PathBuf::from(r"C:\t\job-1.result.json")
        );
    }

    #[test]
    fn write_result_atomic_leaves_no_tmp_and_is_readable() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.result.json");
        let res = JobResult::fail("sid", "nonce-1", "boom");
        write_result_atomic(&p, &res).unwrap();
        assert!(p.is_file());
        assert!(!p.with_extension("tmp").exists(), "tmp 残留必须被 rename 消化");
        let back: JobResult =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(back.nonce, "nonce-1");
        // 覆写路径：已存在结果文件时仍能原子顶替
        write_result_atomic(&p, &JobResult::fail("sid", "nonce-2", "again")).unwrap();
        let back2: JobResult =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(back2.nonce, "nonce-2");
    }

    fn base64_decode(s: &str) -> Vec<u8> {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut acc = 0u32;
        let mut bits = 0;
        let mut out = Vec::new();
        for c in s.bytes() {
            if c == b'=' {
                break;
            }
            let v = T.iter().position(|&x| x == c).expect("非法 base64 字符") as u32;
            acc = acc << 6 | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
        out
    }
}
