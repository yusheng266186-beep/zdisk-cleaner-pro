# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 清理历史记录 (新增)
以 JSON 存储在 %LOCALAPPDATA%\\ZDiskCleanerPro\\history.json
"""

import os
import json
import time

from .analyzer import human_size

DATA_DIR = os.path.join(os.environ.get('LOCALAPPDATA', os.path.expanduser('~')),
                        'ZDiskCleanerPro')
HISTORY_FILE = os.path.join(DATA_DIR, 'history.json')
SETTINGS_FILE = os.path.join(DATA_DIR, 'settings.json')
MAX_RECORDS = 100


def _ensure_dir():
    os.makedirs(DATA_DIR, exist_ok=True)


def load_history():
    try:
        with open(HISTORY_FILE, 'r', encoding='utf-8') as f:
            return json.load(f)
    except (OSError, ValueError):
        return []


def add_record(deleted: int, freed: int, mode: str, rules: list,
               real_freed: int = None):
    """real_freed: 真实释放 (永久删除或含清空回收站); None 表示与 freed 相同。"""
    _ensure_dir()
    history = load_history()
    real = freed if real_freed is None else real_freed
    history.insert(0, {
        'time': time.strftime('%Y-%m-%d %H:%M:%S'),
        'deleted': deleted,
        'freed': freed,
        'real_freed': real,
        'mode': mode,
        'rules': rules,
        'freed_h': human_size(freed),
    })
    with open(HISTORY_FILE, 'w', encoding='utf-8') as f:
        json.dump(history[:MAX_RECORDS], f, ensure_ascii=False, indent=1)


def total_freed():
    """累计真实释放 (不含仍在回收站里的量)。"""
    return sum(r.get('real_freed', r.get('freed', 0)) for r in load_history())


def load_settings() -> dict:
    try:
        with open(SETTINGS_FILE, 'r', encoding='utf-8') as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_settings(settings: dict):
    _ensure_dir()
    current = load_settings()
    current.update(settings)
    with open(SETTINGS_FILE, 'w', encoding='utf-8') as f:
        json.dump(current, f, ensure_ascii=False, indent=1)
