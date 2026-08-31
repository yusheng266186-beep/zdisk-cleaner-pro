//! 启动项管家：注册表 Run/RunOnce 键 + 用户启动文件夹。
//!
//! 「禁用」= 把值搬进本地备份 JSON 并删除注册表值（HKCU 无需提权；
//! HKLM 写入需要管理员，非提权时显式返回错误而不是静默失败）。
//! 备份文件位于 data_dir()/startup-backup.json，可用 `ZC_STARTUP_BACKUP`
//! 环境变量整体重定向（测试夹具用，v5）。
//!
//! v5 契约（审计 S5）：
//! - **先落备份、写后校验、再删注册表值**（旧序两步之间进程被杀即永久
//!   丢失还原凭据）；删除失败自动回滚备份；
//! - `enable_all` 只移除成功回写项 + 写后回读校验 + 返回逐项明细；
//! - 新增 `enable_one(key_id)` 单条恢复与 `list_disabled()` 对账视图；
//! - 备份 JSON 损坏不再静默当空——上抛 Err；
//! - REG_EXPAND_SZ 完整往返（记录并回写原值类型）；
//! - RegQueryInfoKeyW 预分配缓冲（消灭 4096 定长截断面）。

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ,
    REG_OPTION_NON_VOLATILE, REG_SZ, RegQueryInfoKeyW,
};
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};

pub const REG_RUN_CU: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const REG_RUNONCE_CU: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
pub const REG_RUN_LM: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const REG_RUNONCE_LM: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain([0]).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupEntry {
    /// 唯一键：hive|subkey|value_name
    pub key_id: String,
    pub hive: String,
    pub subkey: String,
    pub run_once: bool,
    pub name: String,
    pub command: String,
    /// 注册表值类型（REG_SZ=1 / REG_EXPAND_SZ=2），恢复时原样回写
    #[serde(default = "default_reg_sz")]
    pub value_kind: u32,
}

fn default_reg_sz() -> u32 {
    REG_SZ
}

/// 枚举 HKCU 的 Run + RunOnce（读操作永远安全）。
pub fn list_user_startup() -> io::Result<Vec<StartupEntry>> {
    let mut out = Vec::new();
    for (subkey, once) in [(REG_RUN_CU.to_string(), false), (REG_RUNONCE_CU.to_string(), true)] {
        collect_hive(HKEY_CURRENT_USER, &subkey, once, &mut out);
    }
    Ok(out)
}

fn collect_hive(hive: HKEY, subkey: &str, once: bool, out: &mut Vec<StartupEntry>) {
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(hive, wide(subkey).as_ptr(), 0, KEY_READ, &mut hk) != ERROR_SUCCESS {
            return; // 键不存在属正常
        }
        // v5：RegQueryInfoKeyW 预分配名称与数据缓冲，杜绝定长 [u8;4096] 截断
        let mut value_count: u32 = 0;
        let mut max_name: u32 = 0;
        let mut max_data: u32 = 0;
        RegQueryInfoKeyW(
            hk,
            std::ptr::null_mut(),
            &mut 0,
            std::ptr::null_mut(),
            &mut 0,
            &mut 0,
            &mut 0,
            &mut value_count,
            &mut max_name,
            &mut max_data,
            &mut 0,
            std::ptr::null_mut(),
        );
        for i in 0..value_count {
            let mut name_buf = vec![0u16; max_name as usize + 2];
            let mut data_buf = vec![0u8; (max_data as usize).max(64) + 2];
            let mut attempt = 0;
            let (name, dtype, data) = loop {
                let mut name_len = name_buf.len() as u32;
                let mut data_len = data_buf.len() as u32;
                let mut dtype = 0u32;
                let code = RegEnumValueW(
                    hk,
                    i,
                    name_buf.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    &mut dtype,
                    data_buf.as_mut_ptr(),
                    &mut data_len,
                );
                if code == ERROR_MORE_DATA && attempt < 3 && data_len as usize > data_buf.len() {
                    data_buf.resize(data_len as usize + 8, 0);
                    attempt += 1;
                    continue;
                }
                if code != ERROR_SUCCESS {
                    break (None, 0, Vec::new());
                }
                let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                break (Some(name), dtype, data_buf[..data_len as usize].to_vec());
            };
            let Some(name) = name else { continue };
            if dtype != REG_SZ && dtype != REG_EXPAND_SZ {
                continue; // 只接管字符串型自启值
            }
            let pairs: Vec<u16> = data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let cmd = String::from_utf16_lossy(&pairs)
                .trim_end_matches('\0')
                .to_string();
            out.push(StartupEntry {
                key_id: format!("hkcu|{subkey}|{name}"),
                hive: "hkcu".to_string(),
                subkey: subkey.to_string(),
                run_once: once,
                name,
                command: cmd,
                value_kind: dtype,
            });
        }
        RegCloseKey(hk);
    }
}

/* ── 禁用 / 启用 ─────────────────────────────────────────── */

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupStore {
    entries: Vec<BackupItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupItem {
    subkey: String,
    run_once: bool,
    name: String,
    command: String,
    /// 原注册表值类型（REG_SZ / REG_EXPAND_SZ），回写时保真
    #[serde(default = "default_reg_sz")]
    kind: u32,
}

impl BackupItem {
    fn key_id(&self) -> String {
        format!("hkcu|{}|{}", self.subkey, self.name)
    }
}

/// 备份文件路径。`ZC_STARTUP_BACKUP` 环境变量可整体覆盖（测试夹具）。
fn backup_path() -> PathBuf {
    if let Ok(p) = std::env::var("ZC_STARTUP_BACKUP") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    crate::manifest::data_dir().join("startup-backup.json")
}

/// 读取备份。文件不存在 = 空；**存在但解析失败 → Err 上抛**
/// （静默当空会让禁用项无对账、恢复无凭据——审计 S5）。
fn load_backup() -> io::Result<BackupStore> {
    match std::fs::read_to_string(backup_path()) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("启动项备份 JSON 损坏（{}）: {e}", backup_path().display()),
            )
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BackupStore { entries: vec![] }),
        Err(e) => Err(e),
    }
}

fn save_backup_atomic(b: &BackupStore) -> io::Result<()> {
    let p = backup_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    // 原子写：临时文件 + rename，杜绝半截 JSON
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(b)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// 写后校验：重读备份确认条目在案。
fn backup_contains(item: &BackupItem) -> io::Result<bool> {
    Ok(load_backup()?.entries.iter().any(|e| e.key_id() == item.key_id()))
}

/// 单条被禁用项（对账视图，CONTRACT §1 DisabledEntry）。
#[derive(Debug, Clone, Serialize)]
pub struct DisabledEntry {
    pub key_id: String,
    pub value: String,
}

/// 从备份 JSON 列出全部被禁用项；备份损坏上抛 Err（不再静默空表）。
pub fn list_disabled() -> io::Result<Vec<DisabledEntry>> {
    Ok(load_backup()?
        .entries
        .into_iter()
        .map(|e| DisabledEntry { key_id: e.key_id(), value: e.command })
        .collect())
}

/// 禁用（仅支持 HKCU）：**备份先落盘并校验，再删注册表值**；
/// 删除失败回滚备份条目，还原凭据永不出窗口期。返回是否确实变更。
pub fn disable(entry_key_id: &str) -> io::Result<bool> {
    let Some(e) = list_user_startup()?.into_iter().find(|e| e.key_id == entry_key_id) else {
        return Ok(false);
    };
    let item = BackupItem {
        subkey: e.subkey.clone(),
        run_once: e.run_once,
        name: e.name.clone(),
        command: e.command.clone(),
        kind: e.value_kind,
    };

    let mut b = load_backup()?;
    if !b.entries.iter().any(|x| x.key_id() == item.key_id()) {
        b.entries.push(item.clone());
    }
    save_backup_atomic(&b)?;
    if !backup_contains(&item)? {
        return Err(io::Error::other("备份写后校验失败，未触碰注册表"));
    }

    let del = unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(&e.subkey).as_ptr(), 0, KEY_WRITE, &mut hk)
            != ERROR_SUCCESS
        {
            Err(io::Error::other("打开 Run 键失败（权限）"))
        } else {
            let ok = RegDeleteValueW(hk, wide(&e.name).as_ptr()) == ERROR_SUCCESS;
            RegCloseKey(hk);
            if ok {
                Ok(())
            } else {
                Err(io::Error::other("删除启动值失败"))
            }
        }
    };
    if let Err(err) = del {
        // 注册表原值还在：把刚写入的备份撤掉，维持「备份=已禁用集合」语义
        if let Ok(mut b2) = load_backup() {
            b2.entries.retain(|x| x.key_id() != item.key_id());
            let _ = save_backup_atomic(&b2);
        }
        return Err(err);
    }
    Ok(true)
}

/// enable_all 逐项明细（v5：失败项留在备份里可重试，绝不结尾一把清空）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct EnableSummary {
    pub restored: usize,
    /// (key_id, 失败原因)
    pub failed: Vec<(String, String)>,
}

/// 恢复全部被禁用项：只移除成功回写并通过读回校验的条目；
/// 失败项保留在备份中待下次重试，逐项明细随 [`EnableSummary`] 返回。
pub fn enable_all() -> io::Result<EnableSummary> {
    let b = load_backup()?;
    let mut summary = EnableSummary::default();
    let mut survivors: Vec<BackupItem> = Vec::new();
    for item in b.entries {
        match write_back(&item) {
            Ok(()) => summary.restored += 1,
            Err(e) => {
                summary.failed.push((item.key_id(), e.to_string()));
                survivors.push(item);
            }
        }
    }
    save_backup_atomic(&BackupStore { entries: survivors })?;
    Ok(summary)
}

/// 单条恢复：成功才从备份移除；失败上抛且备份条目保留。
pub fn enable_one(key_id: &str) -> io::Result<bool> {
    let b = load_backup()?;
    let Some(item) = b.entries.iter().find(|e| e.key_id() == key_id).cloned() else {
        return Ok(false);
    };
    write_back(&item)?;
    let mut b2 = load_backup()?;
    b2.entries.retain(|x| x.key_id() != key_id);
    save_backup_atomic(&b2)?;
    Ok(true)
}

/// 回写注册表值（保持原类型）+ 写后读回校验。
fn write_back(item: &BackupItem) -> io::Result<()> {
    let data: Vec<u8> = item
        .command
        .encode_utf16()
        .chain([0])
        .flat_map(|u| u.to_le_bytes())
        .collect();
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        // KEY_READ | KEY_WRITE：写入后还要读回校验（v5 写后校验）
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(&item.subkey).as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            std::ptr::null(),
            &mut hk,
            std::ptr::null_mut(),
        ) != ERROR_SUCCESS
        {
            return Err(io::Error::other("打开/创建 Run 子键失败（权限）"));
        }
        let set = RegSetValueExW(
            hk,
            wide(&item.name).as_ptr(),
            0,
            item.kind,
            data.as_ptr(),
            data.len() as u32,
        );
        if set != ERROR_SUCCESS {
            RegCloseKey(hk);
            return Err(io::Error::other("回写启动值失败"));
        }
        // 写后校验（v5）：读回比对，防「报成功实未落」
        let mut dtype = 0u32;
        let mut cb: u32 = data.len() as u32 + 8;
        let mut buf = vec![0u8; cb as usize];
        let q = RegQueryValueExW(
            hk,
            wide(&item.name).as_ptr(),
            std::ptr::null(),
            &mut dtype,
            buf.as_mut_ptr(),
            &mut cb,
        );
        RegCloseKey(hk);
        if q != ERROR_SUCCESS {
            return Err(io::Error::other("回写后读回校验失败"));
        }
        let read_back = String::from_utf16_lossy(
            &buf[..cb as usize]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<u16>>(),
        )
        .trim_end_matches('\0')
        .to_string();
        if dtype != item.kind || read_back != item.command {
            return Err(io::Error::other(format!(
                "回写校验不符（type {dtype} vs {}, 值不一致: {read_back:?}）",
                item.kind
            )));
        }
    }
    Ok(())
}

/// 列出当前被禁用的数量（供 UI 角标）。备份损坏时上抛（S5 不再静默 0）。
pub fn disabled_count() -> io::Result<usize> {
    Ok(load_backup()?.entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn list_never_panics_on_any_machine() {
        let _ = list_user_startup().unwrap_or_default();
    }

    #[test]
    fn disable_unknown_key_is_noop_true_semantics_false() {
        assert!(matches!(disable("hkcu|nope|n"), Ok(false)));
    }

    #[test]
    fn corrupt_backup_surfaces_instead_of_silence() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("startup-backup.json");
        std::fs::write(&p, b"{ not json").unwrap();
        std::env::set_var("ZC_STARTUP_BACKUP", &p);
        let err = list_disabled().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(disabled_count().is_err());
        std::env::remove_var("ZC_STARTUP_BACKUP");
    }

    /// 完整备份→恢复往返：借 ZC_STARTUP_BACKUP 与一个专属测试子键
    /// （不碰真实 Run 键），覆盖 REG_EXPAND_SZ 保真与写后校验。
    #[test]
    fn enable_one_roundtrip_preserves_expand_sz_type() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("startup-backup.json");
        std::env::set_var("ZC_STARTUP_BACKUP", &p);
        let subkey = format!("Software\\ZDiskCleanerPro-selftest-{}", std::process::id());

        // 手工构造备份（等价 disable 落账后的形态）
        let b = BackupStore {
            entries: vec![BackupItem {
                subkey: subkey.clone(),
                run_once: false,
                name: "probe".into(),
                command: "%WINDIR%\\explorer.exe".into(),
                kind: REG_EXPAND_SZ,
            }],
        };
        save_backup_atomic(&b).unwrap();
        assert_eq!(list_disabled().unwrap().len(), 1);
        assert_eq!(disabled_count().unwrap(), 1);

        // 单条恢复：注册表实际回写 + 类型保真 + 备份条目移除
        assert!(enable_one(&b.entries[0].key_id()).unwrap());
        unsafe {
            let mut hk: HKEY = std::ptr::null_mut();
            assert_eq!(
                RegOpenKeyExW(HKEY_CURRENT_USER, wide(&subkey).as_ptr(), 0, KEY_READ, &mut hk),
                ERROR_SUCCESS
            );
            let mut dtype = 0u32;
            let mut cb = 0u32;
            assert_eq!(
                RegQueryValueExW(hk, wide("probe").as_ptr(), std::ptr::null(), &mut dtype, std::ptr::null_mut(), &mut cb),
                ERROR_SUCCESS,
                "恢复后注册表值必须实际存在"
            );
            assert_eq!(dtype, REG_EXPAND_SZ, "REG_EXPAND_SZ 类型必须保真回写");
            RegCloseKey(hk);
            // 清理测试键
            windows_sys::Win32::System::Registry::RegDeleteKeyW(
                HKEY_CURRENT_USER,
                wide(&subkey).as_ptr(),
            );
        }
        assert!(list_disabled().unwrap().is_empty(), "成功恢复项必须从备份移除");
        assert!(!enable_one("hkcu|ghost|g").unwrap(), "未知 id → Ok(false)");

        std::env::remove_var("ZC_STARTUP_BACKUP");
    }
}
