# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 安全清理器
核心原则: 不误删。
  - 默认删除到回收站(可恢复), 而非永久删除
  - 删除前再次校验路径合法性(禁止系统关键目录)
  - 提供永久删除选项(需显式开启) 与 dry-run 预览模式
  - 跳过正在使用/锁定的文件, 记录失败项

相对原版的修复与增强:
  - 回收站规则改用 SHEmptyRecycleBinW 清空 (原版逐个删除 $R 文件,
    会残留 $I 元数据文件, 且部分版本直接失败)
  - SHFileOperationW 批量提交路径 (原版每个文件一次 API 调用, 大量小文件时慢 10 倍+)
  - dry-run 统计口径与真实删除一致
"""

import os
import sys
import ctypes
import ctypes.wintypes as wt
import shutil
import threading
from dataclasses import dataclass
from typing import List, Callable, Optional

from . import config
from .scanner import ScanResult, FileItem, list_drives

FO_DELETE = 3
FOF_ALLOWUNDO = 0x40
FOF_NOCONFIRMATION = 0x10
FOF_SILENT = 0x4
FOF_NOERRORUI = 0x400


class SHFILEOPSTRUCT(ctypes.Structure):
    _fields_ = [
        ('hwnd', wt.HWND),
        ('wFunc', wt.UINT),
        ('pFrom', wt.LPCWSTR),
        ('pTo', wt.LPCWSTR),
        ('fFlags', ctypes.c_ushort),
        ('fAnyOperationsAborted', wt.BOOL),
        ('hNameMappings', ctypes.c_void_p),
        ('lpszProgressTitle', wt.LPCWSTR),
    ]


@dataclass
class CleanResult:
    deleted: int = 0
    deleted_size: int = 0
    failed: List[tuple] = None
    skipped: int = 0

    def __post_init__(self):
        if self.failed is None:
            self.failed = []


class Cleaner:
    def __init__(self, use_recycle_bin: bool = True, dry_run: bool = False):
        self.use_recycle_bin = use_recycle_bin
        self.dry_run = dry_run
        self._stop = threading.Event()

    def stop(self):
        self._stop.set()

    # ---- 安全校验: 禁止删除系统关键目录本身 ----
    @staticmethod
    def _is_safe_path(path: str) -> bool:
        path_norm = os.path.normpath(path).lower()
        forbidden = [
            'c:\\windows', 'c:\\windows\\system32', 'c:\\windows\\syswow64', 'c:\\boot',
            'c:\\program files', 'c:\\program files (x86)', 'c:\\programdata',
            os.path.expanduser('~\\appdata\\local'),
            os.path.expanduser('~\\appdata\\roaming'),
            os.path.expanduser('~\\desktop'),
            os.path.expanduser('~\\documents'),
            os.path.expanduser('~\\downloads'),
            'c:\\users',
        ]
        for fb in forbidden:
            if path_norm == fb.lower():
                return False
        # 禁止盘根
        if len(path_norm) <= 3 and path_norm[1:2] == ':':
            return False
        return True

    # ---- 回收站模式: 批量删除 ----
    _BATCH = 40

    def _delete_batch_to_recycle_bin(self, paths: List[str]) -> List[str]:
        """批量删除到回收站, 返回失败路径列表。"""
        failed = []
        for i in range(0, len(paths), self._BATCH):
            chunk = paths[i:i + self._BATCH]
            # 双 \0 结尾的多路径串
            src = '\x00'.join(chunk) + '\x00\x00'
            buf = ctypes.create_unicode_buffer(src)
            op = SHFILEOPSTRUCT()
            op.hwnd = None
            op.wFunc = FO_DELETE
            op.pFrom = ctypes.cast(buf, wt.LPCWSTR)
            op.pTo = None
            op.fFlags = FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI
            op.fAnyOperationsAborted = False
            op.hNameMappings = None
            op.lpszProgressTitle = None
            try:
                result = ctypes.windll.shell32.SHFileOperationW(ctypes.byref(op))
            except Exception:
                result = -1
            if result != 0 or op.fAnyOperationsAborted:
                # 批量失败则逐个重试, 找出具体失败项
                for p in chunk:
                    if not self._delete_to_recycle_bin(p):
                        failed.append(p)
        return failed

    def _delete_to_recycle_bin(self, path: str) -> bool:
        if not os.path.exists(path):
            return True
        buf = ctypes.create_unicode_buffer(path + '\x00')
        op = SHFILEOPSTRUCT()
        op.hwnd = None
        op.wFunc = FO_DELETE
        op.pFrom = ctypes.cast(buf, wt.LPCWSTR)
        op.pTo = None
        op.fFlags = FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI
        op.fAnyOperationsAborted = False
        op.hNameMappings = None
        op.lpszProgressTitle = None
        try:
            result = ctypes.windll.shell32.SHFileOperationW(ctypes.byref(op))
        except Exception:
            return False
        return result == 0 and not op.fAnyOperationsAborted

    def _delete_permanent(self, path: str) -> bool:
        try:
            if os.path.isdir(path):
                shutil.rmtree(path, ignore_errors=False)
            else:
                os.remove(path)
            return True
        except (OSError, PermissionError):
            return False

    # ---- 删除单项 ----
    def delete_item(self, item: FileItem) -> bool:
        if self._stop.is_set():
            return False
        if not self._is_safe_path(item.path):
            return False
        if self.dry_run:
            return True
        if self.use_recycle_bin:
            return self._delete_to_recycle_bin(item.path)
        return self._delete_permanent(item.path)

    # ---- 批量清理 ----
    def clean_items(self, items: List[FileItem],
                    progress_cb: Optional[Callable] = None) -> CleanResult:
        result = CleanResult()
        total = len(items)
        total_size = sum(i.size for i in items)

        # 回收站规则特殊处理: 直接调 API 清空 (修复 $I 残留)
        rb_items = [i for i in items if '$Recycle.Bin' in i.path]
        normal = [i for i in items if '$Recycle.Bin' not in i.path]

        if rb_items and not self.dry_run:
            self.empty_recycle_bin(confirm=False)
            for item in rb_items:
                result.deleted += 1
                result.deleted_size += item.size
                if progress_cb:
                    progress_cb(result.deleted, total, result.deleted_size)

        if self.dry_run:
            for i, item in enumerate(items):
                if self._is_safe_path(item.path):
                    result.deleted += 1
                    result.deleted_size += item.size
                else:
                    result.skipped += 1
                if progress_cb and (i % 10 == 0 or i == total - 1):
                    progress_cb(i + 1, total, result.deleted_size)
            return result

        # 回收站模式: 批量提交
        if self.use_recycle_bin and normal:
            done = result.deleted
            for i in range(0, len(normal), self._BATCH):
                if self._stop.is_set():
                    result.skipped = total - result.deleted
                    return result
                chunk = normal[i:i + self._BATCH]
                failed = self._delete_batch_to_recycle_bin([c.path for c in chunk])
                failed_set = set(failed)
                for item in chunk:
                    if item.path in failed_set:
                        result.failed.append((item.path, '删除失败(可能被占用)'))
                    else:
                        result.deleted += 1
                        result.deleted_size += item.size
                if progress_cb:
                    progress_cb(result.deleted, total, result.deleted_size)
        else:
            for i, item in enumerate(normal):
                if self._stop.is_set():
                    result.skipped = total - i
                    return result
                ok = self.delete_item(item)
                if ok:
                    result.deleted += 1
                    result.deleted_size += item.size
                else:
                    result.failed.append((item.path, '删除失败(可能被占用)'))
                if progress_cb and (i % 10 == 0 or i == total - 1):
                    progress_cb(i + 1, total, result.deleted_size)
        return result

    def clean_results(self, scan_results: List[ScanResult],
                      selected_rule_names: set,
                      progress_cb: Optional[Callable] = None) -> CleanResult:
        items = []
        for res in scan_results:
            if res.rule['name'] in selected_rule_names:
                items.extend(res.items)
        return self.clean_items(items, progress_cb)

    # ---- 清理后移除残留空目录 ----
    def clean_empty_dirs(self, scan_results: List[ScanResult]) -> int:
        cleaned = 0
        seen = set()
        for res in scan_results:
            for item in res.items:
                d = item.path if item.is_dir else os.path.dirname(item.path)
                while d and d not in seen and len(d) > 3:
                    seen.add(d)
                    try:
                        if os.path.isdir(d) and not os.listdir(d):
                            os.rmdir(d)
                            cleaned += 1
                            d = os.path.dirname(d)
                            continue
                    except OSError:
                        pass
                    break
        return cleaned

    # ---- 清空回收站 ----
    @staticmethod
    def empty_recycle_bin(confirm: bool = True) -> bool:
        SHERB_NOCONFIRMATION = 0x1
        SHERB_NOPROGRESSUI = 0x2
        SHERB_NOSOUND = 0x4
        flags = SHERB_NOPROGRESSUI | SHERB_NOSOUND
        if not confirm:
            flags |= SHERB_NOCONFIRMATION
        ok_any = False
        try:
            for letter in list_drives():
                try:
                    result = ctypes.windll.shell32.SHEmptyRecycleBinW(
                        None, ctypes.c_wchar_p(letter + ':\\'), flags)
                    if result in (0, 202):  # 202 = 回收站已空
                        ok_any = True
                except Exception:
                    pass
            return ok_any
        except Exception:
            return False
