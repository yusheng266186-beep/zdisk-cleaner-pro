//! 提权任务协议：一次性 elevated worker。
//!
//! 流程（最小提权原则）：
//! 1. 主进程把任务序列化为 `JobSpec` 写入临时 JSON 文件；
//! 2. `powershell Start-Process -Verb RunAs` 拉起**同一个 exe** 带 `elevated-run`
//!    子命令（UAC 弹窗只授权这一次进程，无任何常驻服务）；
//! 3. worker 先经 fail-closed 守卫校验，执行后把 `JobResult` 落盘返回；
//! 4. 主进程轮询结果文件（含超时），UAC 拒绝 = 无结果文件 = 显式取消。
//!
//! 安全约束：
//! - worker 拒绝在未提权状态下运行特权动作（防误用低权限重放）；
//! - 动作枚举白名单，任何新动作必须显式登记。

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
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

#[derive(Debug, Serialize, Deserialize)]
pub struct JobSpec {
    pub id: String,
    #[serde(default)]
    pub created_unix: u64,
    pub action: JobAction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobResult {
    pub spec_id: String,
    pub success: bool,
    /// 人类可读摘要（成功或失败原因）
    pub message: String,
    /// 存在时为清理统计；序列化始终携带（None 亦输出 null）
    pub outcome: Option<CleanOutcome>,
}

/// 主进程入口：写入 spec → UAC 拉起 worker → 轮询结果。
pub fn run_elevated(exe: &Path, spec: &JobSpec, poll_timeout: Duration) -> Result<JobResult> {
    let dir = std::env::temp_dir().join("ZDiskCleanerPro3-jobs");
    std::fs::create_dir_all(&dir)?;
    let spec_path = dir.join(format!("{}.spec.json", spec.id));
    let result_path = dir.join(format!("{}.result.json", spec.id));
    let _ = std::fs::remove_file(&result_path);

    let mut f = std::fs::File::create(&spec_path)?;
    f.write_all(serde_json::to_vec_pretty(spec)?.as_slice())?;

    let ps = format!(
        "Start-Process -FilePath '{}' -ArgumentList 'elevated-run','--job','{}' -Verb RunAs -WindowStyle Hidden",
        exe.display(),
        spec_path.display()
    );
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .status()?;

    // 用户拒绝 UAC 时 PowerShell 一般也以非零退出；结果文件不存在同样按取消处理
    if !status.success() && !result_path.exists() {
        return Err(Error::Other("UAC 已取消或提权拉起失败".into()));
    }

    let deadline = Instant::now() + poll_timeout;
    loop {
        if result_path.exists() {
            let raw = std::fs::read_to_string(&result_path)?;
            return Ok(serde_json::from_str::<JobResult>(&raw)?);
        }
        if Instant::now() > deadline {
            return Err(Error::External(
                "worker 超时未回写结果（可能被杀毒软件拦截或用户长时间未确认）".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// worker 入口（子命令 elevated-run）：校验提权态 → 执行白名单动作 → 回写结果。
pub fn execute_as_worker(spec: &JobSpec) -> JobResult {
    let finish = |success: bool, message: String, outcome: Option<CleanOutcome>| JobResult {
        spec_id: spec.id.clone(),
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
            // 执行器内部包含 fail-closed 守卫；提权批照常全量过闸
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
        JobSpec { id: format!("t-{:x}", std::process::id()), created_unix: zc_core::now_unix(), action }
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

        let res = JobResult { spec_id: spec.id.clone(), success: false, message: "x".into(), outcome: None };
        let b: JobResult = serde_json::from_str(&serde_json::to_string(&res).unwrap()).unwrap();
        assert!(!b.success);
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
    }
}
