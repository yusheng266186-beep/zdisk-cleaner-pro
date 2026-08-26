# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 系统信息模块 (新增)
磁盘空间 / 内存 / CPU / Windows 版本 / 开机启动项管理。
"""

import os
import ctypes
import ctypes.wintypes as wt
import datetime


# ---------------------------------------------------------------------------
# 磁盘
# ---------------------------------------------------------------------------
class _ULARGE_INTEGER(ctypes.Union):
    _fields_ = [('QuadPart', ctypes.c_ulonglong),
                ('LowPart', ctypes.c_ulong), ('HighPart', ctypes.c_ulong)]


def get_disk_free(path: str) -> int:
    free = _ULARGE_INTEGER()
    total = _ULARGE_INTEGER()
    avail = _ULARGE_INTEGER()
    ok = ctypes.windll.kernel32.GetDiskFreeSpaceExW(
        ctypes.c_wchar_p(path), ctypes.byref(free),
        ctypes.byref(total), ctypes.byref(avail))
    return free.QuadPart if ok else 0


def get_disk_total(path: str) -> int:
    free = _ULARGE_INTEGER()
    total = _ULARGE_INTEGER()
    avail = _ULARGE_INTEGER()
    ok = ctypes.windll.kernel32.GetDiskFreeSpaceExW(
        ctypes.c_wchar_p(path), ctypes.byref(free),
        ctypes.byref(total), ctypes.byref(avail))
    return total.QuadPart if ok else 0


DRIVE_TYPE_NAMES = {
    3: '本地磁盘', 4: '网络驱动器', 2: '可移动磁盘', 5: '光驱', 6: '内存盘',
}


def get_all_disks():
    """返回所有盘的信息列表: [{drive, letter, label, type, total, free, used}]"""
    disks = []
    bitmask = ctypes.windll.kernel32.GetLogicalDrives()
    for i, letter in enumerate('ABCDEFGHIJKLMNOPQRSTUVWXYZ'):
        if not (bitmask & (1 << i)):
            continue
        root = f'{letter}:\\'
        dtype = ctypes.windll.kernel32.GetDriveTypeW(ctypes.c_wchar_p(root))
        if dtype == 5:  # 光驱跳过
            continue
        label = ctypes.create_unicode_buffer(261)
        fs = ctypes.create_unicode_buffer(261)
        serial = wt.DWORD()
        maxcomp = wt.DWORD()
        flags = wt.DWORD()
        ctypes.windll.kernel32.GetVolumeInformationW(
            ctypes.c_wchar_p(root), label, 261, ctypes.byref(serial),
            ctypes.byref(maxcomp), ctypes.byref(flags), fs, 261)
        total = get_disk_total(root)
        free = get_disk_free(root)
        if total <= 0:
            continue
        disks.append({
            'drive': root, 'letter': letter,
            'label': label.value or DRIVE_TYPE_NAMES.get(dtype, '磁盘'),
            'type': DRIVE_TYPE_NAMES.get(dtype, '磁盘'),
            'fs': fs.value,
            'total': total, 'free': free, 'used': total - free,
            'usage': (total - free) / total if total else 0,
        })
    return disks


# ---------------------------------------------------------------------------
# 内存
# ---------------------------------------------------------------------------
class _MEMORYSTATUSEX(ctypes.Structure):
    _fields_ = [('dwLength', ctypes.c_ulong),
                ('dwMemoryLoad', ctypes.c_ulong),
                ('ullTotalPhys', ctypes.c_ulonglong),
                ('ullAvailPhys', ctypes.c_ulonglong),
                ('ullTotalPageFile', ctypes.c_ulonglong),
                ('ullAvailPageFile', ctypes.c_ulonglong),
                ('ullTotalVirtual', ctypes.c_ulonglong),
                ('ullAvailVirtual', ctypes.c_ulonglong),
                ('ullAvailExtendedVirtual', ctypes.c_ulonglong)]


def get_memory():
    stat = _MEMORYSTATUSEX()
    stat.dwLength = ctypes.sizeof(_MEMORYSTATUSEX)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(stat))
    total_gb = stat.ullTotalPhys / 1024 ** 3
    avail_gb = stat.ullAvailPhys / 1024 ** 3
    return {
        'total': stat.ullTotalPhys,
        'avail': stat.ullAvailPhys,
        'used': stat.ullTotalPhys - stat.ullAvailPhys,
        'usage': stat.dwMemoryLoad,
        'total_gb': total_gb,
        'avail_gb': avail_gb,
    }


# ---------------------------------------------------------------------------
# CPU / 系统
# ---------------------------------------------------------------------------
def get_cpu_count():
    return os.cpu_count() or 1


def get_windows_info():
    class _OSVERSIONINFOEXW(ctypes.Structure):
        _fields_ = [('dwOSVersionInfoSize', ctypes.c_ulong),
                    ('dwMajorVersion', ctypes.c_ulong),
                    ('dwMinorVersion', ctypes.c_ulong),
                    ('dwBuildNumber', ctypes.c_ulong),
                    ('dwPlatformId', ctypes.c_ulong),
                    ('szCSDVersion', ctypes.c_wchar * 128),
                    ('wServicePackMajor', ctypes.c_ushort),
                    ('wServicePackMinor', ctypes.c_ushort),
                    ('wSuiteMask', ctypes.c_ushort),
                    ('wProductType', ctypes.c_byte),
                    ('wReserved', ctypes.c_byte)]

    info = _OSVERSIONINFOEXW()
    info.dwOSVersionInfoSize = ctypes.sizeof(_OSVERSIONINFOEXW)
    # GetVersionExW 在 Win10+ 会被兼容性垫片欺骗, 用 RtlGetVersion
    try:
        ctypes.windll.ntdll.RtlGetVersion(ctypes.byref(info))
        build = info.dwBuildNumber
        if build >= 22000:
            name = f'Windows 11 (Build {build})'
        elif build >= 10240:
            name = f'Windows 10 (Build {build})'
        else:
            name = f'Windows {info.dwMajorVersion}.{info.dwMinorVersion} (Build {build})'
        return {'os': name, 'build': build}
    except Exception:
        return {'os': 'Windows', 'build': 0}


def get_computer_name():
    buf = ctypes.create_unicode_buffer(256)
    size = wt.DWORD(256)
    ctypes.windll.kernel32.GetComputerNameW(buf, ctypes.byref(size))
    return buf.value


def get_uptime():
    """系统开机时长 (秒)。"""
    return int(ctypes.windll.kernel32.GetTickCount64() / 1000)


def format_uptime(seconds):
    delta = datetime.timedelta(seconds=seconds)
    days = delta.days
    hours, rem = divmod(delta.seconds, 3600)
    minutes = rem // 60
    if days:
        return f'{days} 天 {hours} 小时'
    if hours:
        return f'{hours} 小时 {minutes} 分钟'
    return f'{minutes} 分钟'


# ---------------------------------------------------------------------------
# 开机启动项管理 (新增功能)
# ---------------------------------------------------------------------------
RUN_KEYS = [
    ('HKCU\\...\\Run', r'Software\Microsoft\Windows\CurrentVersion\Run', 'HKEY_CURRENT_USER'),
    ('HKLM\\...\\Run', r'Software\Microsoft\Windows\CurrentVersion\Run', 'HKEY_LOCAL_MACHINE'),
    ('HKCU\\...\\Run (32)', r'Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Run', 'HKEY_CURRENT_USER'),
    ('HKLM\\...\\Run (32)', r'Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Run', 'HKEY_LOCAL_MACHINE'),
]

# 已禁用启动项存放位置
DISABLED_KEY = r'Software\ZDiskCleanerPro\DisabledRun'


def list_startup_items():
    """枚举注册表 Run 键的启动项。返回 [{name, command, location, enabled}]"""
    import winreg
    items = []
    for loc_label, subkey, hive_name in RUN_KEYS:
        hive = getattr(winreg, hive_name)
        try:
            key = winreg.OpenKey(hive, subkey, 0, winreg.KEY_READ)
        except OSError:
            continue
        with key:
            i = 0
            while True:
                try:
                    name, value, _ = winreg.EnumValue(key, i)
                except OSError:
                    break
                i += 1
                if not isinstance(value, str):
                    continue
                items.append({
                    'name': name,
                    'command': value,
                    'location': loc_label,
                    'hive': hive_name,
                    'subkey': subkey,
                    'value_name': name,
                    'enabled': True,
                })
    # 已禁用项
    try:
        dkey = winreg.OpenKey(winreg.HKEY_CURRENT_USER, DISABLED_KEY, 0, winreg.KEY_READ)
        with dkey:
            i = 0
            while True:
                try:
                    name, value, _ = winreg.EnumValue(dkey, i)
                except OSError:
                    break
                i += 1
                if not isinstance(value, str) or '\x00' not in value:
                    continue
                hive_name, subkey, vname, cmd = value.split('\x00', 3)
                items.append({
                    'name': vname,
                    'command': cmd,
                    'location': '已禁用',
                    'hive': hive_name,
                    'subkey': subkey,
                    'value_name': vname,
                    'enabled': False,
                })
    except OSError:
        pass
    items.sort(key=lambda x: (not x['enabled'], x['name'].lower()))
    return items


def set_startup_enabled(item, enabled: bool) -> bool:
    """启用/禁用一个启动项 (禁用 = 移到备份键, 启用 = 移回原位)。"""
    import winreg
    try:
        if enabled:
            # 从禁用备份移回原位
            dkey = winreg.OpenKey(winreg.HKEY_CURRENT_USER, DISABLED_KEY, 0, winreg.KEY_READ)
            with dkey:
                val, _ = winreg.QueryValueEx(dkey, item['value_name'])
            hive_name, subkey, vname, cmd = val.split('\x00', 3)
            hive = getattr(winreg, hive_name)
            # 若原位置已被同名项占用则失败
            try:
                okey = winreg.OpenKey(hive, subkey, 0, winreg.KEY_SET_VALUE)
                with okey:
                    winreg.SetValueEx(okey, vname, 0, winreg.REG_SZ, cmd)
            except OSError:
                return False
            ddkey = winreg.OpenKey(winreg.HKEY_CURRENT_USER, DISABLED_KEY, 0, winreg.KEY_SET_VALUE)
            with ddkey:
                winreg.DeleteValue(ddkey, item['value_name'])
        else:
            # 移到禁用备份
            hive = getattr(winreg, item['hive'])
            okey = winreg.OpenKey(hive, item['subkey'], 0, winreg.KEY_SET_VALUE)
            with okey:
                winreg.DeleteValue(okey, item['value_name'])
            try:
                winreg.CreateKey(winreg.HKEY_CURRENT_USER, DISABLED_KEY)
            except OSError:
                pass
            dkey = winreg.OpenKey(winreg.HKEY_CURRENT_USER, DISABLED_KEY, 0, winreg.KEY_SET_VALUE)
            with dkey:
                blob = '\x00'.join([item['hive'], item['subkey'],
                                    item['value_name'], item['command']])
                winreg.SetValueEx(dkey, item['value_name'], 0, winreg.REG_SZ, blob)
        return True
    except Exception:
        return False


# ---------------------------------------------------------------------------
# 进程枚举 (检测浏览器/应用是否正在运行, 缓存被锁时提前告知)
# ---------------------------------------------------------------------------
def list_process_names():
    """返回当前所有进程名的集合, 如 {'chrome.exe', 'explorer.exe'}。"""
    import ctypes
    from ctypes import wintypes
    TH32CS_SNAPPROCESS = 0x2

    class _PE32(ctypes.Structure):
        _fields_ = [('dwSize', wintypes.DWORD),
                    ('cntUsage', wintypes.DWORD),
                    ('th32ProcessID', wintypes.DWORD),
                    ('th32DefaultHeapID', ctypes.c_size_t),
                    ('th32ModuleID', wintypes.DWORD),
                    ('cntThreads', wintypes.DWORD),
                    ('th32ParentProcessID', wintypes.DWORD),
                    ('pcPriClassBase', ctypes.c_long),
                    ('dwFlags', wintypes.DWORD),
                    ('szExeFile', ctypes.c_wchar * 260)]

    k32 = ctypes.windll.kernel32
    names = set()
    snap = k32.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    if snap == -1:
        return names
    entry = _PE32()
    entry.dwSize = ctypes.sizeof(_PE32)
    try:
        if k32.Process32FirstW(snap, ctypes.byref(entry)):
            while True:
                names.add(entry.szExeFile.lower())
                if not k32.Process32NextW(snap, ctypes.byref(entry)):
                    break
    finally:
        k32.CloseHandle(snap)
    return names


# 清理规则涉及的进程: 规则名关键词 -> 进程名
RULE_PROCESS_MAP = {
    'chrome': ['chrome.exe'],
    'edge': ['msedge.exe'],
    'firefox': ['firefox.exe'],
    'brave': ['brave.exe'],
    'opera': ['opera.exe'],
    'vivaldi': ['vivaldi.exe'],
    '微信': ['wechat.exe', 'weixin.exe', 'wechatappex.exe'],
    'qq': ['qq.exe'],
    'spotify': ['spotify.exe'],
    'discord': ['discord.exe'],
    'zoom': ['zoom.exe'],
    'teams': ['ms-teams.exe', 'teams.exe'],
    'slack': ['slack.exe'],
    'steam': ['steam.exe'],
    '钉钉': ['dingtalk.exe'],
    'vscode': ['code.exe'],
}


def detect_busy_apps(selected_rule_names):
    """根据勾选的规则, 返回正在运行的相关应用列表 [(规则名, 进程名)]。"""
    try:
        running = list_process_names()
    except Exception:
        return []
    busy = []
    for rule_name in selected_rule_names:
        low = rule_name.lower()
        for key, procs in RULE_PROCESS_MAP.items():
            if key in low or key in rule_name:
                for p in procs:
                    if p in running:
                        busy.append((rule_name, p))
                        break
    return busy


# ---------------------------------------------------------------------------
# 系统级大块头检测 (不能自动删, 给出引导)
# ---------------------------------------------------------------------------
def detect_system_hogs():
    """检测 Windows.old / 休眠文件 / 页面文件 等系统级占用。"""
    import os
    sys_drive = os.environ.get('SystemDrive', 'C:')
    items = []
    # Windows.old
    wo = os.path.join(sys_drive + os.sep, 'Windows.old')
    if os.path.isdir(wo):
        from .mover import _path_size
        items.append({
            'name': 'Windows.old (旧系统备份)',
            'size': _path_size(wo),
            'how': '设置 > 系统 > 存储 > 临时文件 > 勾选"以前的 Windows 安装"删除',
            'setting': 'ms-settings:cleanuprecommendations',
        })
    # 休眠文件
    hiber = os.path.join(sys_drive + os.sep, 'hiberfil.sys')
    try:
        if os.path.exists(hiber):
            items.append({
                'name': '休眠文件 hiberfil.sys',
                'size': os.path.getsize(hiber),
                'how': '不需要休眠功能时: 管理员运行 powercfg /h off 可立即释放',
                'cmd': 'powercfg /h off',
            })
    except OSError:
        pass
    # 页面文件
    pf = os.path.join(sys_drive + os.sep, 'pagefile.sys')
    try:
        if os.path.exists(pf):
            items.append({
                'name': '虚拟内存页面文件 pagefile.sys',
                'size': os.path.getsize(pf),
                'how': '可在 设置 > 系统 > 关于 > 高级系统设置 中移到其他盘 (建议保留)',
                'setting': 'ms-settings:storagesense',
            })
    except OSError:
        pass
    return items
