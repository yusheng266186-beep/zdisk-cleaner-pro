//! 启动项管家：注册表 Run/RunOnce 键 + 用户启动文件夹。
//!
//! 「禁用」= 把值搬进本地备份 JSON 并删除注册表值（HKCU 无需提权；
//! HKLM 写入需要管理员，非提权时显式返回错误而不是静默失败）。
//! 备份文件位于 data_dir()/startup-backup.json。

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegQueryInfoKeyW,
};

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
}

/// 枚举 HKCU 的 Run + RunOnce（读操作永远安全）。
pub fn list_user_startup() -> std::io::Result<Vec<StartupEntry>> {
    let mut out = Vec::new();
    for (subkey, once) in [(REG_RUN_CU.to_string(), false), (REG_RUNONCE_CU.to_string(), true)] {
        collect_hive(HKEY_CURRENT_USER, &subkey, once, &mut out);
    }
    Ok(out)
}

fn collect_hive(hive: HKEY, subkey: &str, once: bool, out: &mut Vec<StartupEntry>) {
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(hive, wide(subkey).as_ptr(), 0, KEY_READ, &mut hk) != 0 {
            return; // 键不存在属正常
        }
        let mut count: u32 = 0;
        let mut max_name: u32 = 0;
        RegQueryInfoKeyW(
            hk,
            std::ptr::null_mut(),
            &mut 0,
            std::ptr::null_mut(),
            &mut 0,
            &mut 0,
            &mut 0,
            &mut count,
            &mut max_name,
            &mut 0,
            &mut 0,
            std::ptr::null_mut(),
        );
        for i in 0..count {
            let mut name_buf = vec![0u16; max_name as usize + 1];
            let mut name_len = name_buf.len() as u32;
            let mut dtype: u32 = 0;
            let mut data_buf = [0u8; 4096];
            let mut data_len = data_buf.len() as u32;
            if RegEnumValueW(
                hk, i, name_buf.as_mut_ptr(), &mut name_len,
                std::ptr::null_mut(), &mut dtype, data_buf.as_mut_ptr(), &mut data_len,
            ) == 0 && dtype == REG_SZ
            {
                let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let pairs: Vec<u16> = data_buf[..data_len as usize]
                        .as_chunks::<2>().0
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
                });
            }
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
}

fn backup_path() -> PathBuf2 {
    crate::manifest::data_dir().join("startup-backup.json")
}
type PathBuf2 = std::path::PathBuf;

fn load_backup() -> BackupStore {
    std::fs::read_to_string(backup_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(BackupStore { entries: vec![] })
}

fn save_backup(b: &BackupStore) -> std::io::Result<()> {
    let p = backup_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_vec_pretty(b)?)
}

/// 禁用（仅支持 HKCU）：从注册表移除并写入备份。返回是否确实发生了变更。
pub fn disable(entry_key_id: &str) -> std::io::Result<bool> {
    let Some(e) = list_user_startup()?.into_iter().find(|e| e.key_id == entry_key_id) else {
        return Ok(false);
    };
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(&e.subkey).as_ptr(), 0, KEY_WRITE, &mut hk) != 0 {
            return Err(std::io::Error::other("打开 Run 键失败（权限）"));
        }
        let ok = RegDeleteValueW(hk, wide(&e.name).as_ptr()) == 0;
        RegCloseKey(hk);
        if !ok {
            return Err(std::io::Error::other("删除启动值失败"));
        }
    }
    let mut b = load_backup();
    b.entries.push(BackupItem {
        subkey: e.subkey,
        run_once: e.run_once,
        name: e.name,
        command: e.command,
    });
    save_backup(&b)?;
    Ok(true)
}

/// 恢复全部被禁用项。
pub fn enable_all() -> std::io::Result<usize> {
    let b = load_backup();
    let mut restored = 0usize;
    unsafe {
        for item in &b.entries {
            let mut hk: HKEY = std::ptr::null_mut();
            let sub = if item.run_once { REG_RUNONCE_CU } else { REG_RUN_CU };
            if RegCreateKeyExW(HKEY_CURRENT_USER, wide(sub).as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, std::ptr::null(), &mut hk, std::ptr::null_mut()) != 0 {
                continue;
            }
            let data: Vec<u8> = item
                .command
                .encode_utf16()
                .chain([0])
                .flat_map(|u| u.to_le_bytes())
                .collect();
            if RegSetValueExW(hk, wide(&item.name).as_ptr(), 0, REG_SZ, data.as_ptr(), data.len() as u32) == 0 {
                restored += 1;
            }
            RegCloseKey(hk);
        }
    }
    save_backup(&BackupStore { entries: vec![] })?;
    Ok(restored)
}

/// 列出当前被禁用的数量（供 UI 角标）。
pub fn disabled_count() -> usize {
    load_backup().entries.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_never_panics_on_any_machine() {
        let _ = list_user_startup().unwrap_or_default();
    }

    #[test]
    fn disable_unknown_key_is_noop_true_semantics_false() {
        assert!(matches!(disable("hkcu|nope|n"), Ok(false)));
    }
}
