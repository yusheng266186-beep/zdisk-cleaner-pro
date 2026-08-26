# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 程序搬家模块
将 C 盘缓存/数据目录重定向到其他盘, 从源头减少占用。
支持: 设置用户环境变量(注册表 HKCU\\Environment + 广播) / 执行配置命令 / 迁移现有数据 / 手动操作指引。

相对原版的修复:
  - config_cmd 的盘符替换: 原版用 'D:\\\\AppCache'(双反斜杠) 作为搜索串,
    与命令串中的单反斜杠 'D:\\AppCache' 永远不匹配, 换目标盘后命令仍写 D 盘;
    现改为对 'D:' 盘符整体替换, 任意目标盘都正确。
"""

import os
import sys
import ctypes
import shutil
import subprocess
import threading
from dataclasses import dataclass
from typing import Optional

from . import config


# ---------------------------------------------------------------------------
# 环境变量 (用户级, 永久)
# ---------------------------------------------------------------------------
def set_user_env(name: str, value: str) -> bool:
    try:
        import winreg
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment', 0,
                             winreg.KEY_SET_VALUE)
        winreg.SetValueEx(key, name, 0, winreg.REG_EXPAND_SZ, value)
        winreg.CloseKey(key)
        _broadcast_env_change()
        return True
    except Exception as e:
        print(f'设置环境变量失败: {e}')
        return False


def get_user_env(name: str) -> Optional[str]:
    try:
        import winreg
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment', 0,
                             winreg.KEY_READ)
        val, _ = winreg.QueryValueEx(key, name)
        winreg.CloseKey(key)
        return val
    except (FileNotFoundError, OSError):
        return None


def _broadcast_env_change():
    """通知所有窗口环境变量已更新 (无需注销)。"""
    HWND_BROADCAST = 0xFFFF
    WM_SETTINGCHANGE = 26
    try:
        ctypes.windll.user32.SendMessageTimeoutW(
            HWND_BROADCAST, WM_SETTINGCHANGE, 0, 'Environment', 2, 1000, None)
    except Exception:
        pass


# ---------------------------------------------------------------------------
# 数据迁移
# ---------------------------------------------------------------------------
def _move_file_safe(src: str, dst: str) -> tuple:
    """移动单个文件; 目标已存在且内容一致则删除源, 否则重命名副本。"""
    try:
        if os.path.exists(dst):
            if (os.path.getsize(src) == os.path.getsize(dst)
                    and os.stat(src).st_mtime == os.stat(dst).st_mtime):
                os.remove(src)
                return (True, '')
            base, ext = os.path.splitext(dst)
            i = 1
            while os.path.exists(f'{base}.dup{i}{ext}'):
                i += 1
            shutil.move(src, f'{base}.dup{i}{ext}')
            return (True, '')
        shutil.move(src, dst)
        return (True, '')
    except (OSError, shutil.Error) as e:
        return (False, str(e))


def move_data(src: str, dst: str, progress_cb=None) -> tuple:
    """把 src 目录/文件移动到 dst (合并已有内容), 返回 (ok, message)。"""
    if not os.path.exists(src):
        return (False, f'源路径不存在: {src}')

    if os.path.isfile(src):
        ok, err = _move_file_safe(src, dst)
        if ok:
            return (True, '已迁移 1 项')
        return (False, f'迁移失败: {err}')

    if os.path.dirname(dst):
        os.makedirs(os.path.dirname(dst), exist_ok=True)
    os.makedirs(dst, exist_ok=True)

    moved = 0
    failed = 0
    try:
        for entry in os.listdir(src):
            s = os.path.join(src, entry)
            d = os.path.join(dst, entry)
            try:
                if os.path.isdir(s):
                    if os.path.exists(d):
                        fail = _merge_tree(s, d)
                        if fail == 0:
                            shutil.rmtree(s, ignore_errors=True)
                        else:
                            failed += fail
                    else:
                        shutil.move(s, d)
                else:
                    ok, _ = _move_file_safe(s, d)
                    if not ok:
                        failed += 1
                moved += 1
            except (OSError, shutil.Error):
                failed += 1
            if progress_cb:
                progress_cb(moved, failed)
        try:
            os.rmdir(src)
        except OSError:
            pass
    except Exception as e:
        return (False, f'迁移异常: {e}')

    msg = f'已迁移 {moved} 项'
    if failed:
        msg += f', {failed} 项失败(可能被占用, 请关闭相关程序后重试)'
    return (True, msg)


def _merge_tree(src: str, dst: str) -> int:
    failed = 0
    for entry in os.listdir(src):
        s = os.path.join(src, entry)
        d = os.path.join(dst, entry)
        if os.path.isdir(s):
            os.makedirs(d, exist_ok=True)
            failed += _merge_tree(s, d)
        else:
            ok, _ = _move_file_safe(s, d)
            if not ok:
                failed += 1
    return failed


def _path_size(path: str) -> int:
    try:
        if os.path.isfile(path):
            return os.path.getsize(path)
        total = 0
        for root, dirs, files in os.walk(path, onerror=lambda e: None):
            for f in files:
                try:
                    total += os.path.getsize(os.path.join(root, f))
                except OSError:
                    pass
        return total
    except OSError:
        return 0


# ---------------------------------------------------------------------------
# 搬家执行器
# ---------------------------------------------------------------------------
@dataclass
class MoveResult:
    name: str
    success: bool
    message: str
    target: str


class AppMover:
    """程序搬家执行器"""

    def __init__(self, target_drive: str = 'D:'):
        self.target_drive = target_drive

    def list_movable(self):
        """检测本机存在的可搬迁应用及其当前占用。"""
        movable = []
        for app in config.RELOCATABLE_APPS:
            exists = any(os.path.exists(p) for p in app['detect_paths'])
            if not exists:
                continue
            size = 0
            for p in app['detect_paths']:
                if os.path.exists(p):
                    size += _path_size(p)
            movable.append(dict(app, current_size=size, exists=True))
        return movable

    def relocate(self, app: dict, move_data_flag: bool = True,
                 progress_cb=None) -> MoveResult:
        name = app['name']
        target = app.get('target_subdir', '').replace('D:', self.target_drive)

        # 1. 环境变量重定向
        if app.get('env_var'):
            set_user_env(app['env_var'], target)

        # 2. 配置命令重定向 (修复原版替换失效 bug)
        if app.get('config_cmd'):
            cmd = app['config_cmd'].replace('D:', self.target_drive)
            try:
                subprocess.run(cmd, shell=True, check=False, capture_output=True, timeout=30)
            except Exception:
                pass

        # 3. 迁移现有数据
        #    Docker (WSL 虚拟盘) 与微信 (运行时锁定文件) 自动迁移必然失败,
        #    这类应用只做重定向设置 + 手动指引, 不硬搬数据
        manual_only = bool(app.get('docker_settings') or app.get('app_setting'))
        msg_parts = []
        if move_data_flag and not manual_only:
            for src in app['detect_paths']:
                if not os.path.exists(src):
                    continue
                dst_name = os.path.basename(src)
                dst = (os.path.join(os.path.dirname(target), dst_name)
                       if os.path.isfile(src) else target)
                ok, m = move_data(src, dst, progress_cb)
                msg_parts.append(m)
                if not ok:
                    return MoveResult(name, False, '; '.join(msg_parts), target)

        # 4. 手动操作指引
        if manual_only:
            msg_parts.append('⚠ 数据需按指引手动迁移 (程序占用中, 自动迁移会失败)')
        if app.get('app_setting'):
            msg_parts.append(f'⚠ 需手动: {app["app_setting"]}')
        if app.get('docker_settings'):
            msg_parts.append('⚠ 需手动: Docker Settings > Resources 修改位置')
        if app.get('vscode_arg'):
            arg = app['vscode_arg'].replace('D:', self.target_drive)
            msg_parts.append(f'⚠ 修改 VSCode 快捷方式: {arg}')
        if app.get('settings_xml'):
            msg_parts.append(f'⚠ 修改 Maven settings.xml localRepository={target}')

        return MoveResult(name, True, '; '.join(msg_parts) or '完成', target)
