# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - Windows 深度磁盘清理工具 (重构增强版)
入口文件

用法:
    python main.py            # 启动图形界面
    python main.py --scan     # 仅扫描, 输出结果到控制台
    python main.py --cli      # 交互式命令行清理
    python main.py --report   # 扫描并生成 Markdown 报告
    python main.py --info     # 查看系统/磁盘信息
"""

import os
import sys
import time

# 控制台输出兼容: GBK 终端无法编码部分 Unicode 字符 (如 ░), 统一改为 UTF-8 宽容输出
for _stream in (sys.stdout, sys.stderr):
    if _stream is not None and hasattr(_stream, 'reconfigure'):
        try:
            _stream.reconfigure(encoding='utf-8', errors='replace')
        except Exception:
            pass

if getattr(sys, 'frozen', False):
    _APP_DIR = os.path.dirname(sys.executable)
else:
    _APP_DIR = os.path.dirname(os.path.abspath(__file__))
if _APP_DIR not in sys.path:
    sys.path.insert(0, _APP_DIR)

from core import config
from core.scanner import Scanner, DiskAnalyzer
from core.cleaner import Cleaner
from core.analyzer import Analyzer, human_size
from core import sysinfo

LINE = '=' * 62


# ---------------------------------------------------------------------------
def launch_gui():
    from gui.theme import enable_high_dpi, set_scale
    enable_high_dpi()
    import tkinter as tk
    from gui.app import App
    root = tk.Tk()
    set_scale(root)
    App(root)
    root.mainloop()


def launch_cli_scan():
    print(LINE)
    print('ZDiskCleaner Pro - 扫描中...')
    print(LINE)
    scanner = Scanner()

    def cb(name, size, done=None, total=None, **kw):
        if done is not None:
            print(f'  [{done}/{total}] {name}: {human_size(size)}')

    results = scanner.scan_all(progress_cb=cb)
    analyzer = Analyzer(results)
    s = analyzer.summary()
    print('\n' + LINE)
    print(f'扫描完成! 总可清理空间: {human_size(s["total_size"])}')
    print(f'项目数: {s["total_files"]}')
    print(LINE)
    print('\n按类别:')
    for cat, info in sorted(s['by_category'].items(),
                            key=lambda x: x[1]['size'], reverse=True):
        print(f'  {cat:<10} {human_size(info["size"]):>12}  ({info["count"]} 项)')
    print('\n清理项 (按大小降序):')
    for r in s['rules']:
        if r['size'] > 0:
            sel = '✓' if r['default_select'] else ' '
            print(f'  [{sel}] {r["name"]:<28} {human_size(r["size"]):>12}'
                  f'  {r["count"]:>5}项  {config.RISK_LABEL.get(r["risk"], "")}')


def launch_cli_clean():
    print('扫描中...')
    scanner = Scanner()
    results = scanner.scan_all()
    analyzer = Analyzer(results)
    s = analyzer.summary()
    print(f'\n总可清理: {human_size(s["total_size"])}\n')
    rules = sorted(s['rules'], key=lambda x: x['size'], reverse=True)
    shown = [r for r in rules if r['size'] > 0]
    for i, r in enumerate(shown):
        sel = '✓' if r['default_select'] else ' '
        print(f'  [{i + 1:>2}] {sel} {r["name"]:<28} {human_size(r["size"]):>12}'
              f' ({config.RISK_LABEL.get(r["risk"], "")})')
    print("\n输入要清理的编号(逗号分隔), 或 'a' 清理所有安全项, 或 'q' 退出:")
    try:
        choice = input('> ').strip().lower()
    except EOFError:
        return
    if choice == 'q':
        return
    selected = set()
    if choice == 'a':
        for r in shown:
            if r['risk'] == config.RISK_SAFE and r['size'] > 0:
                selected.add(r['name'])
    else:
        try:
            for n in choice.split(','):
                idx = int(n.strip()) - 1
                if 0 <= idx < len(shown):
                    selected.add(shown[idx]['name'])
        except ValueError:
            print('输入无效')
            return
    if not selected:
        print('未选择任何项')
        return
    total, count = analyzer.selected_total(selected)
    print(f'\n将清理 {len(selected)} 项, 释放约 {human_size(total)} (删除到回收站)')
    try:
        if input('确认? (y/n) > ').strip().lower() != 'y':
            return
    except EOFError:
        return
    cleaner = Cleaner(use_recycle_bin=True)
    result = cleaner.clean_results(results, selected)
    print(f'\n✓ 清理完成: 删除 {result.deleted} 项, 释放 {human_size(result.deleted_size)}')


def launch_report():
    print('扫描并生成报告...')
    scanner = Scanner()
    results = scanner.scan_all()
    analyzer = Analyzer(results)
    reports_dir = os.path.join(_APP_DIR, 'logs')
    os.makedirs(reports_dir, exist_ok=True)
    path = os.path.join(reports_dir,
                        f'report_{time.strftime("%Y%m%d_%H%M%S")}.md')
    analyzer.generate_report(path)
    print(f'报告已生成: {path}')


def launch_info():
    print(LINE)
    print('系统信息')
    print(LINE)
    info = sysinfo.get_windows_info()
    mem = sysinfo.get_memory()
    print(f'  操作系统   : {info["os"]}')
    print(f'  计算机名   : {sysinfo.get_computer_name()}')
    print(f'  CPU        : {sysinfo.get_cpu_count()} 核心逻辑处理器')
    print(f'  内存       : {mem["total_gb"]:.1f} GB (已用 {mem["usage"]}%)')
    print(f'  开机时长   : {sysinfo.format_uptime(sysinfo.get_uptime())}')
    print('\n磁盘:')
    for d in sysinfo.get_all_disks():
        bar_len = 24
        filled = int(d['usage'] * bar_len)
        bar = '█' * filled + '·' * (bar_len - filled)
        print(f'  {d["drive"]} {d["label"]:<16} {bar} {d["usage"]*100:5.1f}%'
              f'  剩余 {human_size(d["free"])}/{human_size(d["total"])}')


def main():
    if len(sys.argv) > 1:
        arg = sys.argv[1]
        if arg == '--scan':
            launch_cli_scan()
            return
        if arg == '--cli':
            launch_cli_clean()
            return
        if arg == '--report':
            launch_report()
            return
        if arg == '--info':
            launch_info()
            return
        if arg in ('--help', '-h'):
            print(__doc__)
            return
        if arg in ('--gui', '-g'):
            launch_gui()
            return
        print(f'未知参数: {arg}\n{__doc__}')
        return
    launch_gui()


if __name__ == '__main__':
    main()
