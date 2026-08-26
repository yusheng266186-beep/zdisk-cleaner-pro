# -*- coding: utf-8 -*-
"""功能测试: 模拟完整用户流程 (扫描注入/勾选/清理/报告/搬家检测/启动项)。"""
import os
import sys
import time
import tempfile
import traceback

sys.path.insert(0, '.')
from gui.theme import enable_high_dpi
enable_high_dpi()

import tkinter as tk
from gui.app import App

PASS, FAIL = [], []


def check(name, cond, detail=''):
    (PASS if cond else FAIL).append(name)
    print(f'  [{"PASS" if cond else "FAIL"}] {name}' + (f' - {detail}' if detail and not cond else ''))


root = tk.Tk()
root.geometry('1200x780+40+30')
app = App(root)

try:
    # ============ 1. 仪表盘 ============
    print('== 仪表盘 ==')
    root.update()
    dash = app.pages['dashboard']
    rings = len(dash.rings)
    check('磁盘环形图已渲染 (>=1)', rings >= 1, f'{rings}')
    check('统计卡片为 4 个', len(dash.stat_cards) == 4)

    # ============ 2. 深度清理页 ============
    print('== 深度清理 ==')
    app.switch_page('clean')
    root.update()
    clean = app.pages['clean']
    n_rows = len(clean.rows)
    from core import config as _cfg
    check(f'规则行已构建 ({len(_cfg.CLEAN_RULES)})', n_rows == len(_cfg.CLEAN_RULES), f'{n_rows}')
    # 默认勾选安全项
    default_sel = sum(1 for st in clean.rows.values() if st['selected'])
    check('默认勾选了安全项 (>10)', default_sel > 10, f'{default_sel}')

    # 注入模拟扫描结果
    from core.scanner import ScanResult, FileItem
    mock = []
    for rule in __import__('core.config', fromlist=['CLEAN_RULES']).CLEAN_RULES[:5]:
        r = ScanResult(rule=rule)
        r.add(FileItem(path=os.path.join(tempfile.gettempdir(), 'zdc_test', 'a.bin'),
                       size=10 * 1024 * 1024, is_dir=True))
        mock.append(r)
    app.scan_results = mock
    for res in mock:
        st = clean.rows[res.rule['name']]
        st.update(size=res.total_size, count=res.file_count, scanned=True)
        clean._render_row(res.rule['name'])
    clean._update_selected()
    check('注入结果后更新选中统计', '预计释放' in clean.sel_label.cget('text'))

    # 勾选交互
    clean.rows[mock[0].rule['name']]['selected'] = False
    app.selected_rules.discard(mock[0].rule['name'])
    clean._update_selected()
    check('取消勾选后统计变化', mock[0].rule['name'] not in app.selected_rules)

    # 仅选安全项
    clean.select_safe()
    root.update()
    safe_names = {n for n, st in clean.rows.items()
                  if st['rule']['risk'] == 'safe'}
    check('仅选安全项正确', app.selected_rules == safe_names)

    # ============ 3. 真实 dry-run 清理 ============
    print('== 清理引擎 (dry-run + 回收站) ==')
    from core.cleaner import Cleaner
    # 造测试文件
    test_dir = os.path.join(tempfile.gettempdir(), 'zdc_func_test')
    os.makedirs(test_dir, exist_ok=True)
    f1 = os.path.join(test_dir, 'file1.dat')
    f2 = os.path.join(test_dir, 'file2.dat')
    with open(f1, 'wb') as f:
        f.write(b'A' * 10000)
    with open(f2, 'wb') as f:
        f.write(b'B' * 20000)
    items = [FileItem(path=f1, size=10000), FileItem(path=f2, size=20000)]

    # dry-run
    c1 = Cleaner(use_recycle_bin=True, dry_run=True)
    r1 = c1.clean_items(items)
    check('dry-run 报告删除但保留文件',
          r1.deleted == 2 and os.path.exists(f1) and os.path.exists(f2))

    # 回收站模式真实删除
    c2 = Cleaner(use_recycle_bin=True)
    r2 = c2.clean_items(items)
    check('回收站删除成功且文件消失',
          r2.deleted == 2 and not os.path.exists(f1) and not os.path.exists(f2),
          f'deleted={r2.deleted}')

    # 安全校验: 拒绝系统目录
    check('安全校验拒绝 C:\\Windows\\System32',
          not Cleaner._is_safe_path('C:\\Windows\\System32'))
    check('安全校验拒绝盘根', not Cleaner._is_safe_path('C:\\'))
    check('安全校验允许临时文件', Cleaner._is_safe_path(f1))

    # ============ 4. 报告 ============
    print('== 优化报告 ==')
    app.switch_page('report')
    root.update()
    from core.analyzer import Analyzer
    ana = Analyzer(mock)
    content = ana.generate_report()
    check('报告包含标题', 'ZDiskCleaner Pro 磁盘优化报告' in content)
    check('报告包含统计', '可清理空间' in content)

    # ============ 5. 搬家检测 ============
    print('== 程序搬家 ==')
    from core.mover import AppMover
    mv = AppMover('D:')
    movable = mv.list_movable()
    check('搬家检测正常返回列表', isinstance(movable, list))

    # ============ 6. 启动项 ============
    print('== 启动项 ==')
    app.switch_page('startup')
    root.update()
    from core import sysinfo
    items_st = sysinfo.list_startup_items()
    check('启动项枚举正常 (>=0)', isinstance(items_st, list) and len(items_st) >= 0,
          f'{len(items_st)} items')

    # ============ 7. 分析页 ============
    print('== 磁盘分析 ==')
    app.switch_page('analyze')
    root.update()
    an = app.pages['analyze']
    check('分析页磁盘按钮 >=1', len(an.disks) >= 1)
    an._switch_tab(1)
    root.update()
    an._switch_tab(3)
    root.update()
    check('子标签切换正常', an.tab == 3)

    # ============ 8. 历史记录 ============
    print('== 清理历史 ==')
    from core import history
    history.add_record(2, 30000, '回收站', ['测试'])
    h = history.load_history()
    check('历史记录写入读取', len(h) >= 1 and h[0]['freed'] == 30000)

except Exception:
    print('\nEXCEPTION:')
    print(traceback.format_exc())
    FAIL.append('exception')

print('\n' + '=' * 50)
print(f'FUNCTIONAL TEST: {len(PASS)} passed, {len(FAIL)} failed')
if FAIL:
    print('FAILED:', FAIL)
root.destroy()
sys.exit(1 if FAIL else 0)
