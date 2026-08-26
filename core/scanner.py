# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 深度扫描器
将 CLEAN_RULES 解析为具体文件清单, 多线程加速, 跳过锁定/无权限文件。

相对原版的修复与增强:
  - 回收站大小统计改用 SHQueryRecycleBinW API (原版手动遍历 $Recycle.Bin,
    目录项大小恒为 0 导致统计偏小, 且无权限时大量遗漏)
  - 规则级并行扫描 (原版串行逐条规则, 慢)
  - 重复文件检测改为 三级过滤: 大小 -> 头部哈希 -> 全量哈希, 哈希阶段多线程
  - 每条规则完成即通过回调推送, UI 可实时显示
"""

import os
import sys
import glob
import time
import ctypes
import hashlib
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from typing import List, Optional

from . import config


# ---------------------------------------------------------------------------
# 回收站 API (SHQueryRecycleBinW)
# ---------------------------------------------------------------------------
class _SHQUERYRBINFO(ctypes.Structure):
    _fields_ = [
        ('cbSize', ctypes.c_uint32),
        ('i64Size', ctypes.c_int64),
        ('i64NumItems', ctypes.c_int64),
    ]


def query_recycle_bin(drive: str):
    """返回 (总大小, 项目数); 失败返回 (0, 0)。"""
    try:
        info = _SHQUERYRBINFO()
        info.cbSize = ctypes.sizeof(_SHQUERYRBINFO)
        res = ctypes.windll.shell32.SHQueryRecycleBinW(
            ctypes.c_wchar_p(drive + '\\'), ctypes.byref(info))
        if res == 0:
            return int(info.i64Size), int(info.i64NumItems)
    except Exception:
        pass
    return 0, 0


def list_drives():
    """列出所有可用盘符, 如 ['C', 'D']。"""
    drives = []
    bitmask = ctypes.windll.kernel32.GetLogicalDrives()
    for i, letter in enumerate('ABCDEFGHIJKLMNOPQRSTUVWXYZ'):
        if bitmask & (1 << i):
            drives.append(letter)
    return drives


# ---------------------------------------------------------------------------
# 数据结构
# ---------------------------------------------------------------------------
@dataclass
class FileItem:
    """单个待清理文件/目录项"""
    path: str
    size: int = 0
    is_dir: bool = False
    mtime: float = 0.0
    rule_name: str = ''
    category: str = ''
    risk: str = config.RISK_SAFE
    reason: str = ''


@dataclass
class ScanResult:
    """单条规则的扫描结果"""
    rule: dict
    items: List[FileItem] = field(default_factory=list)
    total_size: int = 0
    file_count: int = 0
    skipped: int = 0
    error: str = ''

    def add(self, item: FileItem):
        self.items.append(item)
        self.total_size += item.size
        self.file_count += 1


# ---------------------------------------------------------------------------
# 规则扫描器
# ---------------------------------------------------------------------------
def _self_protect_paths():
    """程序自身的运行时路径, 必须从清理中排除:
    - PyInstaller onefile 解压目录 (%TEMP% 下的 _MEIxxxx) —— 删了会崩
    - 程序所在目录
    - 自身数据目录 (历史/报告)
    """
    paths = []
    try:
        meipass = getattr(sys, '_MEIPASS', '')
        if meipass:
            paths.append(meipass)
    except Exception:
        pass
    try:
        if getattr(sys, 'frozen', False):
            paths.append(os.path.dirname(sys.executable))
        else:
            paths.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    except Exception:
        pass
    try:
        from . import history
        paths.append(history.DATA_DIR)
    except Exception:
        pass
    return [os.path.normcase(os.path.normpath(p)) for p in paths if p]


class Scanner:
    def __init__(self, max_workers: int = 8, exclude_paths=None):
        self.max_workers = max_workers
        self._stop = threading.Event()
        # 自保护路径 (始终生效) + 用户自定义排除
        self.exclude_paths = _self_protect_paths() + [
            os.path.normcase(os.path.normpath(p))
            for p in (exclude_paths or []) if p]

    def _excluded(self, path: str) -> bool:
        if not self.exclude_paths:
            return False
        try:
            np = os.path.normcase(os.path.normpath(path))
        except Exception:
            return False
        return any(np == e or np.startswith(e + os.sep) for e in self.exclude_paths)

    def stop(self):
        self._stop.set()

    @staticmethod
    def _safe_stat(path: str):
        try:
            return os.lstat(path)
        except (OSError, PermissionError, ValueError):
            return None

    # ---- 回收站 ----
    def _scan_recycle_bin(self):
        """用 SHQueryRecycleBinW 精确统计每个盘的回收站。"""
        items = []
        try:
            for letter in list_drives():
                size, num = query_recycle_bin(letter)
                if size > 0 or num > 0:
                    items.append(FileItem(
                        path=f'{letter}:\\$Recycle.Bin',
                        size=size, is_dir=True,
                        mtime=time.time(),
                        reason=f'{letter}: 盘回收站 ({num} 项)',
                    ))
        except Exception:
            pass
        return items

    # ---- 目录大小 ----
    def _dir_size(self, path: str):
        total = 0
        latest = 0.0
        try:
            for root, dirs, files in os.walk(path, onerror=lambda e: None):
                if self._stop.is_set():
                    break
                for f in files:
                    fp = os.path.join(root, f)
                    st = self._safe_stat(fp)
                    if st is None:
                        continue
                    total += st.st_size
                    if st.st_mtime > latest:
                        latest = st.st_mtime
            st = self._safe_stat(path)
            if st and st.st_mtime > latest:
                latest = st.st_mtime
        except (PermissionError, OSError):
            pass
        return total, latest

    # ---- 单条规则 ----
    def scan_rule(self, rule: dict, progress_cb=None) -> ScanResult:
        result = ScanResult(rule=rule)
        targets = rule.get('targets', [])

        # 特殊规则: 回收站
        if '__RECYCLE_BIN__' in targets:
            for it in self._scan_recycle_bin():
                it.rule_name = rule['name']
                it.category = rule['category']
                it.risk = rule['risk']
                result.add(it)
            if progress_cb:
                progress_cb(rule['name'], result.total_size)
            return result

        # glob 展开
        all_paths = []
        for pattern in targets:
            if self._stop.is_set():
                return result
            try:
                matches = glob.glob(pattern, recursive=True)
            except Exception:
                matches = []
            all_paths.extend(matches)

        def _norm(p):
            try:
                return os.path.normcase(os.path.realpath(p))
            except OSError:
                return os.path.normcase(p)

        seen = set()
        matched_dirs = []
        matched_files = []
        for p in all_paths:
            if self._excluded(p):
                continue
            rp = _norm(p)
            if rp in seen:
                continue
            seen.add(rp)
            if os.path.isdir(p):
                matched_dirs.append(p)
            elif os.path.isfile(p):
                matched_files.append(p)

        # 只保留最顶层目录, 去掉嵌套子目录 (避免重复统计)
        keep_dirs = []
        for d in sorted(matched_dirs, key=lambda x: len(x)):
            dn = _norm(d)
            if any(dn.startswith(k + os.sep) for k, _ in keep_dirs):
                continue
            keep_dirs.append((dn, d))

        # 文件若已位于某个保留目录内则跳过
        keep_files = [f for f in matched_files
                      if not any(_norm(f).startswith(k + os.sep) for k, _ in keep_dirs)]

        def _process_path(p, is_dir):
            if self._stop.is_set() or self._excluded(p):
                return None
            if is_dir:
                size, mtime = self._dir_size(p)
                if size <= 0:
                    return None
                return FileItem(p, size, True, mtime,
                                rule['name'], rule['category'], rule['risk'], rule['desc'])
            st = self._safe_stat(p)
            if st is None:
                return None
            return FileItem(p, st.st_size, False, st.st_mtime,
                            rule['name'], rule['category'], rule['risk'], rule['desc'])

        work = [(orig, True) for _, orig in keep_dirs] + [(f, False) for f in keep_files]
        ex = ThreadPoolExecutor(max_workers=self.max_workers)
        try:
            futures = {ex.submit(_process_path, p, d): p for p, d in work}
            for fut in as_completed(futures):
                if self._stop.is_set():
                    break
                try:
                    item = fut.result()
                except Exception:
                    result.skipped += 1
                    continue
                if item:
                    result.add(item)
        finally:
            ex.shutdown(wait=True, cancel_futures=True)

        if progress_cb:
            progress_cb(rule['name'], result.total_size)
        return result

    # ---- 全部规则 (规则级并行) ----
    def scan_all(self, rules=None, progress_cb=None, rule_done_cb=None) -> List[ScanResult]:
        if rules is None:
            rules = config.CLEAN_RULES
        # 从本地设置读取用户排除名单, 与自保护路径合并
        try:
            from . import history
            ex = history.load_settings().get('exclude_paths') or []
            user_ex = [os.path.normcase(os.path.normpath(p)) for p in ex if p]
            self.exclude_paths = list(dict.fromkeys(
                _self_protect_paths() + user_ex + self.exclude_paths))
        except Exception:
            pass
        results = []
        lock = threading.Lock()
        done_count = [0]

        def worker(rule):
            if self._stop.is_set():
                return None
            return self.scan_rule(rule)

        with ThreadPoolExecutor(max_workers=min(8, len(rules) or 1)) as ex:
            futures = {ex.submit(worker, rule): rule for rule in rules}
            for fut in as_completed(futures):
                if self._stop.is_set():
                    break
                try:
                    res = fut.result()
                except Exception:
                    res = ScanResult(rule=futures[fut], error='扫描异常')
                if res is None:
                    continue
                with lock:
                    results.append(res)
                    done_count[0] += 1
                    if rule_done_cb:
                        rule_done_cb(res, done_count[0], len(rules))
                    if progress_cb:
                        progress_cb(res.rule['name'], res.total_size,
                                    done_count[0], len(rules))

        # 按配置中的原始顺序展示
        order = {r['name']: i for i, r in enumerate(config.CLEAN_RULES)}
        results.sort(key=lambda r: order.get(r.rule['name'], 9999))
        return results


# ---------------------------------------------------------------------------
# 磁盘分析器 (非系统盘: 大文件 / 重复文件 / 旧文件 / 目录占用)
# ---------------------------------------------------------------------------
class DiskAnalyzer:
    def __init__(self, max_workers: int = 8):
        self.max_workers = max_workers
        self._stop = threading.Event()

    def stop(self):
        self._stop.set()

    _SKIP_DIRS = {'$Recycle.Bin', 'System Volume Information', '$WinREAgent',
                  'Windows', 'Program Files', 'Program Files (x86)', 'ProgramData'}

    def _walk_disk(self, root: str, file_cb):
        """遍历磁盘 (scandir 加速); 系统盘跳过系统目录, 保证安全。"""
        is_sys_drive = os.path.splitdrive(root)[0].upper() == \
            config.SYSTEM_DRIVE.upper()
        stack = [root]
        while stack:
            if self._stop.is_set():
                return
            d = stack.pop()
            try:
                with os.scandir(d) as it:
                    for entry in it:
                        if self._stop.is_set():
                            return
                        try:
                            if entry.is_dir(follow_symlinks=False):
                                if is_sys_drive and entry.name in self._SKIP_DIRS:
                                    continue
                                stack.append(entry.path)
                            elif entry.is_file(follow_symlinks=False):
                                st = entry.stat(follow_symlinks=False)
                                file_cb(entry.path, st.st_size, st.st_mtime)
                        except OSError:
                            continue
            except OSError:
                continue

    # ---- 大文件 ----
    def find_large_files(self, root: str,
                         threshold=None, progress_cb=None) -> List[dict]:
        threshold = threshold or config.LARGE_FILE_THRESHOLD
        large = []
        scanned = [0]

        def cb(path, size, mtime):
            scanned[0] += 1
            if size >= threshold:
                ext = os.path.splitext(path)[1].lower() or '(无扩展名)'
                large.append(dict(path=path, size=size, mtime=mtime, ext=ext))
            if scanned[0] % 5000 == 0 and progress_cb:
                progress_cb(scanned[0])

        self._walk_disk(root, cb)
        large.sort(key=lambda x: x['size'], reverse=True)
        return large

    # ---- 重复文件 (三级过滤: 大小 -> 头部哈希 -> 全量哈希) ----
    def _hash_file(self, path: str, chunk: int = 1024 * 1024, full: bool = True):
        try:
            h = hashlib.md5()
            with open(path, 'rb') as f:
                if not full:
                    data = f.read(64 * 1024)
                    h.update(data)
                else:
                    while True:
                        data = f.read(chunk)
                        if not data:
                            break
                        h.update(data)
            return h.hexdigest()
        except (OSError, PermissionError):
            return None

    def find_duplicates(self, root: str,
                        min_size=None, progress_cb=None) -> List[List[dict]]:
        min_size = min_size or config.DUPLICATE_MIN_SIZE
        size_map = {}
        scanned = [0]

        def cb(path, size, mtime):
            scanned[0] += 1
            if size >= min_size:
                size_map.setdefault(size, []).append(path)
            if scanned[0] % 5000 == 0 and progress_cb:
                progress_cb(scanned[0])

        self._walk_disk(root, cb)

        # 第一级: 大小相同才可能重复
        candidates = [(size, paths) for size, paths in size_map.items() if len(paths) >= 2]
        if progress_cb:
            progress_cb(scanned[0])

        # 第二级: 头部哈希分组 (多线程)
        groups = []
        if candidates:
            with ThreadPoolExecutor(max_workers=self.max_workers) as ex:
                for size, paths in candidates:
                    if self._stop.is_set():
                        return groups
                    head_map = {}
                    futures = {ex.submit(self._hash_file, p, full=False): p for p in paths}
                    for fut in as_completed(futures):
                        p = futures[fut]
                        h = fut.result()
                        if h:
                            head_map.setdefault(h, []).append(p)
                    # 第三级: 头部相同 -> 全量哈希
                    for h, plist in head_map.items():
                        if len(plist) < 2:
                            continue
                        full_map = {}
                        for p in plist:
                            fh = self._hash_file(p, full=True)
                            if fh:
                                full_map.setdefault(fh, []).append(p)
                        for fh, dups in full_map.items():
                            if len(dups) >= 2:
                                groups.append([dict(path=p, size=size, hash=fh,
                                                    mtime=os.path.getmtime(p))
                                               for p in dups])

        groups.sort(key=lambda g: g[0]['size'] * (len(g) - 1), reverse=True)
        return groups

    # ---- 旧文件 ----
    def find_old_files(self, root: str,
                       days=None, progress_cb=None) -> List[dict]:
        days = days or config.OLD_FILE_DAYS
        threshold = time.time() - days * 86400
        old = []
        scanned = [0]

        def cb(path, size, mtime):
            scanned[0] += 1
            if mtime < threshold and size > 1024 * 1024:
                old.append(dict(path=path, size=size, mtime=mtime,
                                days_old=int((time.time() - mtime) / 86400)))
            if scanned[0] % 5000 == 0 and progress_cb:
                progress_cb(scanned[0])

        self._walk_disk(root, cb)
        old.sort(key=lambda x: x['days_old'], reverse=True)
        return old

    # ---- 目录占用 Top N ----
    def top_dirs(self, root: str, top_n=20, progress_cb=None) -> List[dict]:
        dir_sizes = {}
        scanned = [0]

        def cb(path, size, mtime):
            scanned[0] += 1
            try:
                rel = os.path.relpath(path, root)
                parts = rel.split(os.sep)
                if len(parts) >= 1:
                    top = os.path.join(root, parts[0])
                    dir_sizes[top] = dir_sizes.get(top, 0) + size
            except ValueError:
                pass
            if scanned[0] % 5000 == 0 and progress_cb:
                progress_cb(scanned[0])

        self._walk_disk(root, cb)
        result = [dict(path=k, size=v) for k, v in dir_sizes.items()]
        result.sort(key=lambda x: x['size'], reverse=True)
        return result[:top_n]
