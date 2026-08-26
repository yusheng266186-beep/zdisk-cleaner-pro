# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 页面: 仪表盘 / 深度清理
"""

import os
import time
import threading
import tkinter as tk
from tkinter import messagebox

from gui.theme import C, FONT_FAMILY, px, round_rect, lerp_color, ease_out_cubic
from gui.widgets import (Btn, RingGauge, ProgressBar, CountUpLabel, ScrollFrame,
                         Segmented, Card, chip)

from core import config, sysinfo, history
from core.scanner import Scanner, query_recycle_bin, list_drives
from core.cleaner import Cleaner
from core.analyzer import human_size

RISK_UI = {
    config.RISK_SAFE: ('安全', C['green'], C['green_soft']),
    config.RISK_LOW: ('低风险', C['accent'], C['accent_soft']),
    config.RISK_MEDIUM: ('中风险', C['yellow'], C['yellow_soft']),
    config.RISK_HIGH: ('高风险', C['red'], C['red_soft']),
}


def usage_color(u):
    if u < 0.6:
        return C['green']
    if u < 0.85:
        return C['yellow']
    return C['red']


# ===========================================================================
# 仪表盘
# ===========================================================================
class DashboardPage:
    def __init__(self, app, frame):
        self.app = app
        self.f = frame
        self.rings = []
        self._built = False

    def on_show(self, **kw):
        if not self._built:
            self._build()
            self._built = True
        self.refresh()
        self._maybe_remind()

    def _maybe_remind(self):
        """超过 7 天未清理时, 在仪表盘顶部显示提醒条。"""
        import time as _t
        try:
            hist = history.load_history()
        except Exception:
            hist = []
        remind = False
        days = 0
        if not hist:
            remind, days = True, -1
        else:
            try:
                last = _t.mktime(_t.strptime(hist[0]['time'], '%Y-%m-%d %H:%M:%S'))
                days = int((_t.time() - last) / 86400)
                remind = days >= 7
            except Exception:
                pass
        # 移除旧提醒条
        for w in self.f.winfo_children():
            info = getattr(w, '_is_reminder', False)
            if info:
                w.destroy()
        if not remind:
            return
        banner = tk.Frame(self.f, bg=C['yellow_soft'], padx=14, pady=8)
        banner._is_reminder = True
        # 插到标题之后 (index 1)
        banner.pack(fill='x', padx=28, before=self.disk_row, pady=(6, 0))
        msg = ('还没有清理过 — 点击右侧按钮开始第一次智能清理' if days < 0
               else f'已经 {days} 天没有清理了, C 盘可能又堆了不少缓存')
        tk.Label(banner, text='⏰ ' + msg, font=(FONT_FAMILY, 9),
                 bg=C['yellow_soft'], fg=C['yellow']).pack(side='left')
        Btn(banner, '一键智能清理', command=self._smart_clean,
            style='ghost', width=110, height=28).pack(side='right')

    # ------------------------------------------------------------------
    def _build(self):
        App = self.app
        self.app.page_header(self.f, '仪表盘', '系统与磁盘空间总览')

        # ---- 磁盘卡片区 ----
        self.disk_row = tk.Frame(self.f, bg=C['bg'])
        self.disk_row.pack(fill='x', padx=28, pady=(10, 4))

        # ---- 统计行 ----
        stats = tk.Frame(self.f, bg=C['bg'])
        stats.pack(fill='x', padx=28, pady=8)
        self.stat_cards = []
        for i, (label, key) in enumerate([('累计释放空间', 'freed'), ('累计清理次数', 'count'),
                                          ('回收站占用', 'recycle'), ('内存使用', 'mem')]):
            card = Card(stats, padx=16, pady=12)
            card.grid(row=0, column=i, sticky='nsew', padx=(0, 10))
            stats.columnconfigure(i, weight=1)
            tk.Label(card.body, text=label, font=(FONT_FAMILY, 9),
                     bg=card.body['bg'], fg=C['text_dim']).pack(anchor='w')
            val = CountUpLabel(card.body, text='—', font=(FONT_FAMILY, 17, 'bold'),
                               bg=card.body['bg'], fg=C['text'])
            val.pack(anchor='w', pady=(2, 0))
            sub = tk.Label(card.body, text='', font=(FONT_FAMILY, 8),
                           bg=card.body['bg'], fg=C['text_faint'])
            sub.pack(anchor='w')
            self.stat_cards.append((key, val, sub))

        # ---- 系统信息 + 快速操作 ----
        mid = tk.Frame(self.f, bg=C['bg'])
        mid.pack(fill='x', padx=28, pady=(2, 8))
        mid.columnconfigure(0, weight=3)
        mid.columnconfigure(1, weight=2)

        syscard = Card(mid)
        syscard.grid(row=0, column=0, sticky='nsew', padx=(0, 10))
        tk.Label(syscard.body, text='系统信息', font=(FONT_FAMILY, 11, 'bold'),
                 bg=syscard.body['bg'], fg=C['text']).pack(anchor='w')
        self.sys_lines = tk.Frame(syscard.body, bg=syscard.body['bg'])
        self.sys_lines.pack(fill='x', pady=(6, 0))
        self.mem_bar = ProgressBar(syscard.body, width=360, height=7, determinate=True)
        self.mem_bar.pack(anchor='w', pady=(6, 2))
        self.mem_label = tk.Label(syscard.body, text='', font=(FONT_FAMILY, 8),
                                  bg=syscard.body['bg'], fg=C['text_dim'])
        self.mem_label.pack(anchor='w')

        actcard = Card(mid)
        actcard.grid(row=0, column=1, sticky='nsew')
        tk.Label(actcard.body, text='快速操作', font=(FONT_FAMILY, 11, 'bold'),
                 bg=actcard.body['bg'], fg=C['text']).pack(anchor='w')
        acts = tk.Frame(actcard.body, bg=actcard.body['bg'])
        acts.pack(fill='x', pady=(8, 2))
        Btn(acts, '⚡  一键智能清理', command=self._smart_clean,
            style='primary', width=200, height=40).pack(pady=(0, 8), fill='x')
        Btn(acts, '🔍  深度扫描', command=lambda: self.app.switch_page('clean'),
            style='soft', width=200, height=36).pack(pady=(0, 8), fill='x')
        Btn(acts, '📄  生成优化报告', command=lambda: self.app.switch_page('report'),
            style='ghost', width=200, height=36).pack(fill='x')

        # ---- 系统级占用 (Windows.old / 休眠文件等, 需手动处理) ----
        hogscard = Card(self.f)
        hogscard.pack(fill='x', padx=28, pady=(0, 8))
        tk.Label(hogscard.body, text='系统级占用 (体积大, 需按引导手动处理)',
                 font=(FONT_FAMILY, 11, 'bold'),
                 bg=hogscard.body['bg'], fg=C['text']).pack(anchor='w')
        self.hogs_host = tk.Frame(hogscard.body, bg=hogscard.body['bg'])
        self.hogs_host.pack(fill='x', pady=(6, 0))

        # ---- 清理历史 ----
        histcard = Card(self.f)
        histcard.pack(fill='both', expand=True, padx=28, pady=(0, 22))
        tk.Label(histcard.body, text='最近清理记录', font=(FONT_FAMILY, 11, 'bold'),
                 bg=histcard.body['bg'], fg=C['text']).pack(anchor='w')
        self.hist_list = tk.Frame(histcard.body, bg=histcard.body['bg'])
        self.hist_list.pack(fill='x', pady=(6, 0))

    # ------------------------------------------------------------------
    def refresh(self):
        # 磁盘卡片 (重建)
        for w in self.disk_row.winfo_children():
            w.destroy()
        self.rings = []
        disks = sysinfo.get_all_disks()
        for i, d in enumerate(disks[:6]):
            card = Card(self.disk_row, padx=14, pady=12)
            card.grid(row=0, column=i, sticky='nsew', padx=(0, 10))
            self.disk_row.columnconfigure(i, weight=1 if len(disks) <= 4 else 0)
            # 环内只放百分比; 盘符/容量移到环外, 避免文字压到圆环
            ring = RingGauge(card.body, size=88, thickness=10,
                             label=f"{d['usage']*100:.0f}%",
                             color=usage_color(d['usage']))
            ring.pack()
            tk.Label(card.body, text=f"{d['letter']}: {(d['label'] or '磁盘')[:12]}",
                     font=(FONT_FAMILY, 9, 'bold'), bg=card.body['bg'],
                     fg=C['text']).pack(pady=(4, 1))
            tk.Label(card.body, text=f"剩 {human_size(d['free'])}",
                     font=(FONT_FAMILY, 9, 'bold'), bg=card.body['bg'],
                     fg=usage_color(d['usage'])).pack()
            tk.Label(card.body, text=f"共 {human_size(d['total'])}",
                     font=(FONT_FAMILY, 8), bg=card.body['bg'],
                     fg=C['text_dim']).pack()
            self.rings.append((ring, d['usage']))

        # 统计
        hist = history.load_history()
        total_freed = sum(r.get('freed', 0) for r in hist)
        rb_size = 0
        for letter in list_drives():
            s, _ = query_recycle_bin(letter)
            rb_size += s
        mem = sysinfo.get_memory()
        for key, val, sub in self.stat_cards:
            if key == 'freed':
                val.set_value(total_freed, format=lambda v: human_size(v))
                sub.config(text=f'共 {len(hist)} 次清理')
            elif key == 'count':
                val.set_value(len(hist), format=lambda v: f'{v:.0f} 次')
                sub.config(text='历史记录保存于本地')
            elif key == 'recycle':
                val.set_value(rb_size, format=lambda v: human_size(v))
                sub.config(text='所有盘合计')
            elif key == 'mem':
                val.set_value(mem['usage'], format=lambda v: f'{v:.0f}%')
                sub.config(text=f"{mem['avail_gb']:.1f} GB 可用 / {mem['total_gb']:.1f} GB")

        # 系统信息
        for w in self.sys_lines.winfo_children():
            w.destroy()
        info = sysinfo.get_windows_info()
        rows = [
            ('操作系统', info['os']),
            ('计算机名', sysinfo.get_computer_name()),
            ('处理器', f"{sysinfo.get_cpu_count()} 核心逻辑处理器"),
            ('开机时长', sysinfo.format_uptime(sysinfo.get_uptime())),
        ]
        for k, v in rows:
            line = tk.Frame(self.sys_lines, bg=self.sys_lines['bg'])
            line.pack(fill='x', pady=2)
            tk.Label(line, text=k, width=9, anchor='w', font=(FONT_FAMILY, 9),
                     bg=line['bg'], fg=C['text_dim']).pack(side='left')
            tk.Label(line, text=v, font=(FONT_FAMILY, 9), bg=line['bg'],
                     fg=C['text']).pack(side='left')
        self.mem_bar.set_progress(mem['usage'] / 100)
        self.mem_label.config(text=f"物理内存: {human_size(mem['used'])} / {human_size(mem['total'])}")

        # 历史
        for w in self.hist_list.winfo_children():
            w.destroy()
        if not hist:
            tk.Label(self.hist_list, text='暂无清理记录 — 从右侧「一键智能清理」开始',
                     font=(FONT_FAMILY, 9), bg=self.hist_list['bg'],
                     fg=C['text_faint']).pack(pady=10, anchor='w')
        else:
            for r in hist[:6]:
                row = tk.Frame(self.hist_list, bg=self.hist_list['bg'])
                row.pack(fill='x', pady=2)
                dot = tk.Canvas(row, width=px(10), height=px(10), bg=row['bg'], highlightthickness=0)
                dot.pack(side='left', padx=(0, px(8)))
                dot.create_oval(px(1), px(1), px(9), px(9), fill=C['green'], outline='')
                tk.Label(row, text=r['time'], font=(FONT_FAMILY, 9), bg=row['bg'],
                         fg=C['text_dim'], width=20, anchor='w').pack(side='left')
                tk.Label(row, text=f"清理 {r.get('deleted', 0)} 项",
                         font=(FONT_FAMILY, 9), bg=row['bg'], fg=C['text_dim']).pack(side='left')
                tk.Label(row, text=f"释放 {r.get('freed_h', '0 B')}",
                         font=(FONT_FAMILY, 9, 'bold'), bg=row['bg'],
                         fg=C['green']).pack(side='left', padx=12)
                rules_txt = ' · '.join(r.get('rules', [])[:3]) or '—'
                if len(rules_txt) > 40:
                    rules_txt = rules_txt[:39] + '…'
                tk.Label(row, text=rules_txt,
                         font=(FONT_FAMILY, 8), bg=row['bg'],
                         fg=C['text_faint']).pack(side='left')

        # 环形图动画
        for ring, u in self.rings:
            ring.set_value(u)

        # 系统级占用检测 (后台线程, Windows.old 遍历可能耗时)
        def hog_worker():
            try:
                hogs = sysinfo.detect_system_hogs()
            except Exception:
                hogs = []
            self.app.post(lambda: self._populate_hogs(hogs))

        self.app.run_worker(hog_worker)

    def _populate_hogs(self, hogs):
        for w in self.hogs_host.winfo_children():
            w.destroy()
        if not hogs:
            tk.Label(self.hogs_host,
                     text='未检测到 Windows.old / 休眠文件等系统级占用, 状态良好',
                     font=(FONT_FAMILY, 9), bg=self.hogs_host['bg'],
                     fg=C['text_faint']).pack(anchor='w')
            return
        for h in hogs:
            row = tk.Frame(self.hogs_host, bg=self.hogs_host['bg'])
            row.pack(fill='x', pady=3)
            tk.Label(row, text='●', font=(FONT_FAMILY, 8),
                     bg=row['bg'], fg=C['yellow']).pack(side='left')
            tk.Label(row, text=h['name'], font=(FONT_FAMILY, 10, 'bold'),
                     bg=row['bg'], fg=C['text']).pack(side='left', padx=(8, 0))
            tk.Label(row, text=human_size(h['size']), font=(FONT_FAMILY, 10, 'bold'),
                     bg=row['bg'], fg=C['yellow']).pack(side='left', padx=10)
            tk.Label(row, text=h['how'], font=(FONT_FAMILY, 8),
                     bg=row['bg'], fg=C['text_dim']).pack(side='left')
            if h.get('setting'):
                Btn(row, '打开设置', style='soft', width=78, height=26,
                    command=lambda s=h['setting']: self._open_setting(s)).pack(side='right')
            if h.get('cmd'):
                Btn(row, '复制命令', style='ghost', width=78, height=26,
                    command=lambda c=h['cmd']: self._copy_cmd(c)).pack(side='right', padx=(0, 6))

    def _open_setting(self, uri):
        try:
            os.startfile(uri)
        except Exception as e:
            self.app.toast(f'无法打开: {e}', 'error')

    def _copy_cmd(self, cmd):
        self.app.root.clipboard_clear()
        self.app.root.clipboard_append(cmd)
        self.app.toast('命令已复制, 请以管理员身份运行', 'info')

    # ------------------------------------------------------------------
    def _smart_clean(self):
        """一键智能清理: 跳转清理页, 扫描完成后自动勾选安全项并清理。"""
        self.app.switch_page('clean', auto='safe')


# ===========================================================================
# 深度清理
# ===========================================================================
class CleanPage:
    ROW_H = 70  # 逻辑像素, 运行时经 px() 换算

    def __init__(self, app, frame):
        self.app = app
        self.f = frame
        self.rows = {}          # name -> row state
        self.delete_mode = 0    # 0 回收站 1 永久
        self.dry_run = tk.BooleanVar(value=False)
        self.auto_empty = tk.BooleanVar(value=False)
        self._auto_clean = False
        self._build()

    def on_show(self, auto=None, **kw):
        if auto == 'safe':
            self._auto_clean = True
            self.root_after = self.app.root.after(400, self.start_scan)

    # ------------------------------------------------------------------
    def _build(self):
        self.app.page_header(self.f, '深度清理', '扫描 C 盘 46+ 类缓存垃圾 · 默认删除进回收站, 随时可恢复')

        # ---- 工具栏 ----
        bar = tk.Frame(self.f, bg=C['bg'])
        bar.pack(fill='x', padx=28, pady=(8, 0))
        # 右侧控件先布局 (pack 顺序决定空间分配优先级, 避免被左侧挤压)
        right = tk.Frame(bar, bg=C['bg'])
        right.pack(side='right')
        self.dry_chk = tk.Checkbutton(right, text='预览模式',
                                      variable=self.dry_run, font=(FONT_FAMILY, 9),
                                      bg=C['bg'], fg=C['text_dim'],
                                      selectcolor=C['card2'], activebackground=C['bg'],
                                      activeforeground=C['text'])
        self.dry_chk.pack(side='left', padx=(14, 0))
        tk.Label(right, text='删除方式', font=(FONT_FAMILY, 9), bg=C['bg'],
                 fg=C['text_dim']).pack(side='left', padx=(16, 8))
        self.seg = Segmented(right, ['回 收 站', '永久删除'],
                             command=self._on_mode, width=160, height=30)
        self.seg.pack(side='left')

        self.btn_scan = Btn(bar, '开始扫描', command=self.start_scan,
                            style='primary', width=108, height=34)
        self.btn_scan.pack(side='left')
        self.btn_stop = Btn(bar, '停止', command=self.stop_scan,
                            style='ghost', width=70, height=34)
        self.btn_stop.pack(side='left', padx=(8, 0))
        self.btn_safe = Btn(bar, '仅选安全项', command=self.select_safe,
                            style='soft', width=96, height=34)
        self.btn_safe.pack(side='left', padx=(8, 0))
        self.btn_none = Btn(bar, '全部取消', command=self.select_none,
                            style='ghost', width=80, height=34)
        self.btn_none.pack(side='left', padx=(8, 0))
        self.btn_exclude = Btn(bar, '⚙ 排除目录', command=self.edit_excludes,
                               style='ghost', width=96, height=34)
        self.btn_exclude.pack(side='left', padx=(8, 0))

        # ---- 进度区 ----
        prog = tk.Frame(self.f, bg=C['bg'])
        prog.pack(fill='x', padx=28, pady=(10, 0))
        self.status_label = tk.Label(prog, text='尚未扫描 · 点击「开始扫描」检测可清理空间',
                                     font=(FONT_FAMILY, 9), bg=C['bg'], fg=C['text_dim'])
        self.status_label.pack(anchor='w')
        self.progress = ProgressBar(prog, width=880, height=7)
        self.progress.pack(fill='x', pady=(6, 0))
        self._bind_progress_stretch()

        # ---- 底部操作栏 ----
        bottom = tk.Frame(self.f, bg=C['card'])
        bottom.pack(fill='x', side='bottom', padx=28, pady=(0, 18))
        inner = tk.Frame(bottom, bg=C['card'])
        inner.pack(fill='x', padx=18, pady=12)
        self.btn_clean = Btn(inner, '开始清理', command=self.start_clean,
                             style='green', width=130, height=38)
        self.btn_clean.pack(side='right')
        self.auto_empty_chk = tk.Checkbutton(
            inner, text='清理后清空回收站', variable=self.auto_empty,
            font=(FONT_FAMILY, 9), bg=C['card'], fg=C['text_dim'],
            selectcolor=C['card2'], activebackground=C['card'],
            activeforeground=C['text'])
        self.auto_empty_chk.pack(side='right', padx=(0, 14))
        self.sel_label = tk.Label(inner, text='已选 0 项 · 预计释放 0 B',
                                  font=(FONT_FAMILY, 11, 'bold'), bg=C['card'],
                                  fg=C['text'], anchor='w')
        self.sel_label.pack(side='left', fill='x', expand=True)
        self.sel_sub = tk.Label(inner, text='', font=(FONT_FAMILY, 9), bg=C['card'],
                                fg=C['text_dim'], anchor='e')
        self.sel_sub.pack(side='right', padx=10)

        # ---- 规则列表 ----
        self.scroll = ScrollFrame(self.f, bg=C['bg'])
        self.scroll.pack(fill='both', expand=True, padx=28, pady=(10, 12))
        self.list_host = self.scroll.inner
        # 行宽以滚动视口为准 (内层容器宽度会被行自身撑大, 不能作为依据)
        self.scroll.canvas.bind('<Configure>', self._on_list_resize, add='+')
        self._build_rows()

    def _bind_progress_stretch(self):
        def on_resize(e):
            self.progress.cw = e.width
            self.progress._draw()
        self.progress.bind('<Configure>', on_resize)

    def _on_list_resize(self, e=None):
        w = self.scroll.canvas.winfo_width()
        if w > 50 and getattr(self, '_list_w', 0) != w:
            self._list_w = w
            for name in self.rows:
                self._render_row(name)

    # ------------------------------------------------------------------
    # 规则行
    # ------------------------------------------------------------------
    def _build_rows(self):
        for rule in config.CLEAN_RULES:
            self._make_row(rule)

    def _make_row(self, rule):
        name = rule['name']
        cv = tk.Canvas(self.list_host, height=px(self.ROW_H), bg=C['card'],
                       highlightthickness=0, cursor='hand2')
        cv.pack(fill='x', pady=(0, 6))
        state = dict(rule=rule, cv=cv, selected=bool(rule.get('default_select')),
                     check_t=1.0 if rule.get('default_select') else 0.0,
                     hover_t=0.0, enter_t=1.0, size=0, count=0, scanned=False,
                     expanded=False)
        self.rows[name] = state
        if state['selected']:
            self.app.selected_rules.add(name)

        cv.bind('<Button-1>', lambda e, n=name: self._toggle(n))
        cv.bind('<Double-Button-1>', lambda e, n=name: self._toggle(n))
        cv.bind('<Enter>', lambda e, n=name: self._row_hover(n, 1.0))
        cv.bind('<Leave>', lambda e, n=name: self._row_hover(n, 0.0))
        # 动画进度闪烁点
        state['flash'] = False
        self._render_row(name)

    def _row_hover(self, name, target):
        st = self.rows[name]
        start = st['hover_t']
        if abs(start - target) < 1e-6:
            return
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 120)
            st['hover_t'] = start + (target - start) * ease_out_cubic(t)
            self._render_row(name)
            if t < 1.0:
                self.app.root.after(16, tick)

        tick()

    def _toggle(self, name):
        st = self.rows[name]
        if not st['scanned']:
            # 未扫描时也可勾选, 记住偏好
            st['selected'] = not st['selected']
        else:
            st['selected'] = not st['selected']
        if st['selected']:
            self.app.selected_rules.add(name)
        else:
            self.app.selected_rules.discard(name)
        self._animate_check(name)
        self._update_selected()

    def _animate_check(self, name):
        st = self.rows[name]
        start = st['check_t']
        target = 1.0 if st['selected'] else 0.0
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 160)
            st['check_t'] = start + (target - start) * ease_out_cubic(t)
            self._render_row(name)
            if t < 1.0:
                self.app.root.after(16, tick)

        tick()

    def _render_row(self, name):
        st = self.rows[name]
        cv = st['cv']
        w = getattr(self, '_list_w', 0) or px(880)
        cv.config(width=w)
        cv.delete('all')
        H = px(self.ROW_H)

        hov = st['hover_t']
        sel = st['selected']
        ent = st.get('enter_t', 1.0)
        bg = lerp_color(C['card'], C['card_hover'], hov)
        if sel:
            bg = lerp_color(bg, C['accent_soft'], 0.55)
        if ent < 1.0:
            bg = lerp_color(C['bg'], bg, ent)
        # 圆角卡背景
        round_rect(cv, 1, 1, w - 2, H - 1, px(10), fill=bg, outline=C['border_soft'])
        if sel:
            cv.create_line(px(4), px(12), px(4), H - px(12),
                           fill=C['accent'], width=px(3), capstyle='butt')
        elif hov > 0.05:
            cv.create_line(px(4), px(14), px(4), H - px(14),
                           fill=lerp_color(bg, C['accent'], hov * 0.8),
                           width=px(3), capstyle='butt')

        # ---- 布局: 复选框与标题行对齐; 徽章行在下方, 纵向完全分离 ----
        name_cy = H // 2 - px(12)     # 标题行中心
        chip_cy = H // 2 + px(15)     # 徽章行中心
        ct = st['check_t']
        cx, cy = px(26), name_cy      # 复选框对齐标题行
        r = px(9)
        box_bg = lerp_color(C['card2'], C['accent'], ct)
        cv.create_oval(cx - r, cy - r, cx + r, cy + r, fill=box_bg,
                       outline=lerp_color(C['border'], C['accent'], ct), width=1.6)
        if ct > 0.05:
            s = ease_out_cubic(ct)
            cv.create_line(cx - 4.5 * s, cy, cx - 1.5 * s, cy + 3.5 * s,
                           fill='#ffffff', width=px(2.2), capstyle='round')
            cv.create_line(cx - 1.5 * s, cy + 3.5 * s, cx + 4.5 * s, cy - 3.5 * s,
                           fill='#ffffff', width=px(2.2), capstyle='round')

        # ---- 标题与徽章 (x 与复选框保持充足间距) ----
        x = px(54)
        name_col = C['text'] if ent >= 0.999 else lerp_color(C['bg'], C['text'], ent)
        cv.create_text(x, name_cy, anchor='w', text=st['rule']['name'],
                       font=(FONT_FAMILY, 10, 'bold'), fill=name_col)
        label, fg, bgc = RISK_UI[st['rule']['risk']]
        x2 = chip(cv, x, chip_cy, st['rule']['category'], C['text_dim'], C['card2'])
        chip(cv, x2 + px(6), chip_cy, label, fg, bgc)

        # ---- 右侧大小 ----
        if st['scanned']:
            size_txt = human_size(st['size']) if st['size'] else '0 B'
            count_txt = f"{st['count']} 项"
        else:
            size_txt = '· · ·' if st.get('flash') else '—'
            count_txt = ''
        cv.create_text(w - px(122), name_cy, anchor='e', text=size_txt,
                       font=(FONT_FAMILY, 12, 'bold'),
                       fill=C['text'] if st['size'] else C['text_faint'])
        if count_txt:
            cv.create_text(w - px(114), name_cy, anchor='w', text=count_txt,
                           font=(FONT_FAMILY, 9), fill=C['text_dim'])

    # ------------------------------------------------------------------
    def _on_mode(self, i):
        self.delete_mode = i

    def select_safe(self):
        for name, st in self.rows.items():
            if st['rule']['risk'] == config.RISK_SAFE:
                st['selected'] = True
                self.app.selected_rules.add(name)
            else:
                st['selected'] = False
                self.app.selected_rules.discard(name)
            self._animate_check(name)
        self._update_selected()
        self.app.toast('已自动勾选全部「安全」级别清理项', 'info')

    def select_none(self):
        for name, st in self.rows.items():
            st['selected'] = False
            self.app.selected_rules.discard(name)
            self._animate_check(name)
        self._update_selected()

    def _update_selected(self):
        n = 0
        size = 0
        for res in self.app.scan_results:
            if res.rule['name'] in self.app.selected_rules:
                n += res.file_count
                size += res.total_size
        self.sel_label.config(text=f'已选 {len(self.app.selected_rules)} 类 · {n} 项 · 预计释放 {human_size(size)}')
        if self.delete_mode == 1:
            self.sel_sub.config(text='永久删除模式 · 不可恢复!')
            self.btn_clean.set_style('danger')
            self.btn_clean.text = '永久删除'
        else:
            self.sel_sub.config(text='删除进回收站 · 可恢复' + (' (预览)' if self.dry_run.get() else ''))
            self.btn_clean.set_style('green')
            self.btn_clean.text = '预览清理结果' if self.dry_run.get() else '开始清理'
        self.btn_clean._draw()

    # ------------------------------------------------------------------
    # 扫描
    # ------------------------------------------------------------------
    def start_scan(self):
        if self.app.scan_running:
            return
        self.app.scan_running = True
        self.app.scan_results = []
        for name, st in self.rows.items():
            st['size'] = 0
            st['count'] = 0
            st['scanned'] = False
            self._render_row(name)
        self.btn_scan.set_style('ghost')
        self.btn_scan.text = '扫描中…'
        self.btn_scan._draw()
        self.status_label.config(text='正在扫描…', fg=C['accent'])
        self.progress.start_shimmer()
        self._flash_thread()

        scanner = Scanner(max_workers=8)
        self.app.scanner = scanner
        done_rules = [0]
        total_rules = len(config.CLEAN_RULES)

        def worker():
            def rule_done(res, done, total):
                self.app.post(lambda: self._on_rule_done(res, done, total))
            results = scanner.scan_all(config.CLEAN_RULES, rule_done_cb=rule_done)
            self.app.post(lambda: self._on_scan_done(results))

        self.app.run_worker(worker)

    def _flash_thread(self):
        """扫描时未完成规则的 '···' 闪烁。"""
        def tick():
            if not self.app.scan_running:
                return
            for name, st in self.rows.items():
                if not st['scanned']:
                    st['flash'] = not st.get('flash', False)
                    self._render_row(name)
                    break  # 每帧只更新一个, 分散闪烁
            self.app.root.after(350, tick)
        tick()

    def stop_scan(self):
        if self.app.scanner:
            self.app.scanner.stop()
        self.app.scan_running = False

    def _on_rule_done(self, res, done, total):
        st = self.rows.get(res.rule['name'])
        if st:
            st['size'] = res.total_size
            st['count'] = res.file_count
            st['scanned'] = True
            self._render_row(res.rule['name'])
        self.status_label.config(text=f'正在扫描… ({done}/{total})')

    def _on_scan_done(self, results):
        self.app.scan_running = False
        self.app.scan_results = results
        self.progress.stop_shimmer()
        self.progress.set_progress(1.0)
        total = sum(r.total_size for r in results)
        self.status_label.config(
            text=f'扫描完成 · 发现可清理空间 {human_size(total)}, 共 '
                 f'{sum(r.file_count for r in results)} 项', fg=C['green'])
        # 按大小重排行
        order = sorted(results, key=lambda r: r.total_size, reverse=True)
        order_names = [r.rule['name'] for r in order]
        no_result = [n for n in self.rows if n not in order_names]
        for name in order_names + no_result:
            self.rows[name]['cv'].pack_forget()
            self.rows[name]['cv'].pack(fill='x', pady=(0, 6))
        self._play_entrance(order_names + no_result)
        self._update_selected()
        self.app.toast(f'扫描完成 · 可释放 {human_size(total)}', 'success')

        self.btn_scan.set_style('primary')
        self.btn_scan.text = '重新扫描'
        self.btn_scan._draw()

        if self._auto_clean:
            self._auto_clean = False
            self.app.root.after(600, self._auto_clean_safe)

    def _play_entrance(self, names):
        """扫描完成后行级联淡入 (每行错峰 24ms)。"""
        for st in self.rows.values():
            st['enter_t'] = 0.0
        import time as _t
        for i, name in enumerate(names):
            st = self.rows[name]
            delay = i * 24

            def anim(st=st, delay=delay):
                t0 = _t.perf_counter()

                def tick():
                    t = min(1.0, (_t.perf_counter() - t0) * 1000 / 260)
                    st['enter_t'] = ease_out_cubic(t)
                    self._render_row(st['rule']['name'])
                    if t < 1.0:
                        self.app.root.after(16, tick)
                self.app.root.after(delay, tick)

            anim()

    def _auto_clean_safe(self):
        self.select_safe()
        self.app.root.after(300, self.start_clean)

    # ------------------------------------------------------------------
    # 排除目录编辑
    # ------------------------------------------------------------------
    def edit_excludes(self):
        from core import history
        win = tk.Toplevel(self.app.root)
        win.title('排除目录')
        win.configure(bg=C['card'])
        win.transient(self.app.root)
        win.resizable(False, False)
        win.geometry(f'+{self.app.root.winfo_rootx() + 240}+{self.app.root.winfo_rooty() + 140}')
        tk.Label(win, text='排除目录 (每行一个路径)', font=(FONT_FAMILY, 11, 'bold'),
                 bg=C['card'], fg=C['text']).pack(anchor='w', padx=18, pady=(16, 4))
        tk.Label(win, text='命中这些目录的文件将不会被扫描和清理\n例如保留某个文件夹里的日志: D:\\MyLogs',
                 font=(FONT_FAMILY, 9), bg=C['card'], fg=C['text_dim'],
                 justify='left').pack(anchor='w', padx=18, pady=(0, 8))
        txt = tk.Text(win, width=52, height=9, bg=C['card2'], fg=C['text'],
                      font=('Consolas', 9), bd=0, padx=10, pady=8,
                      insertbackground=C['text'])
        txt.pack(padx=18, pady=(0, 12))
        current = history.load_settings().get('exclude_paths') or []
        txt.insert('1.0', '\n'.join(current))

        def save():
            paths = [l.strip() for l in txt.get('1.0', 'end').splitlines() if l.strip()]
            history.save_settings({'exclude_paths': paths})
            self.app.toast(f'已保存 {len(paths)} 条排除目录', 'success')
            win.destroy()

        row = tk.Frame(win, bg=C['card'])
        row.pack(fill='x', padx=18, pady=(0, 16))
        Btn(row, '保存', command=save, style='primary',
            width=90, height=32).pack(side='right')
        Btn(row, '取消', command=win.destroy, style='ghost',
            width=70, height=32).pack(side='right', padx=(0, 8))

    # ------------------------------------------------------------------
    # 清理
    # ------------------------------------------------------------------
    def start_clean(self):
        if self.app.clean_running:
            return
        if not self.app.scan_results:
            self.app.toast('请先扫描', 'warn')
            return
        if not self.app.selected_rules:
            self.app.toast('请至少勾选一类清理项', 'warn')
            return
        permanent = self.delete_mode == 1
        selected = set(self.app.selected_rules)
        dry = self.dry_run.get()

        # 回收站规则 = 清空回收站, 不可恢复, 单独二次确认
        if '回收站' in selected and not dry:
            if not messagebox.askyesno(
                    '确认清空回收站',
                    '勾选的「回收站」规则将清空所有盘的回收站\n'
                    '(包括本次清理移入的文件与之前手动删除的文件), 不可恢复。\n\n继续?',
                    icon='warning', parent=self.app.root):
                return
        if permanent and not dry:
            if not messagebox.askyesno(
                    '确认永久删除',
                    '永久删除不会放入回收站, 将无法恢复!\n\n确定要继续吗?',
                    icon='warning', parent=self.app.root):
                return
        # 占用检测: 相关应用正在运行时缓存可能被锁定
        if not dry:
            from core import sysinfo as _si
            busy = _si.detect_busy_apps(selected)
            if busy:
                procs = sorted({p for _, p in busy})
                if not messagebox.askyesno(
                        '检测到相关应用正在运行',
                        '以下应用正在运行, 其缓存文件可能被锁定而无法完全清理:\n\n'
                        + '、'.join(procs)
                        + '\n\n建议关闭后重新清理以获得最佳效果。\n仍要现在继续吗?',
                        parent=self.app.root):
                    return

        self.app.clean_running = True
        self.btn_clean.set_style('ghost')
        self.btn_clean.text = '清理中…'
        self.btn_clean._draw()
        self.status_label.config(text='正在清理…', fg=C['accent'])
        self.progress.set_progress(0)
        self.progress.start_shimmer()
        use_rb = not permanent
        cleaner = Cleaner(use_recycle_bin=use_rb, dry_run=dry)
        auto_empty = (not permanent and not dry
                      and self.delete_mode == 0 and self.auto_empty.get())

        def worker():
            def cb(done, tot, size):
                self.app.post(lambda: self._clean_progress(done, tot, size))
            result = cleaner.clean_results(self.app.scan_results, selected, cb)
            cleaned_dirs = 0
            real_freed = 0
            if not dry:
                cleaned_dirs = cleaner.clean_empty_dirs(self.app.scan_results)
                if auto_empty and result.deleted > 0:
                    cleaner.empty_recycle_bin(confirm=False)
                    real_freed = result.deleted_size
            self.app.post(lambda: self._on_clean_done(result, cleaned_dirs, dry,
                                                      real_freed, auto_empty))

        self.app.run_worker(worker)

    def _clean_progress(self, done, tot, size):
        self.progress.set_progress(done / max(1, tot))
        self.status_label.config(text=f'正在清理… {done}/{tot} · 已释放 {human_size(size)}')

    def _on_clean_done(self, result, cleaned_dirs, dry, real_freed=0, auto_empty=False):
        self.app.clean_running = False
        self.progress.stop_shimmer()
        self.progress.set_progress(1.0)
        self.btn_clean.set_style('green' if self.delete_mode == 0 else 'danger')
        self.btn_clean.text = '预览清理结果' if dry else '开始清理'
        self.btn_clean._draw()

        if dry:
            self.status_label.config(
                text=f'预览: 将删除 {result.deleted} 项, 释放 {human_size(result.deleted_size)}',
                fg=C['yellow'])
            self.app.toast(f'预览: 将释放 {human_size(result.deleted_size)}', 'info')
            return

        permanent = self.delete_mode == 1
        if not dry:
            real = (result.deleted_size if (permanent or real_freed) else 0)
            history.add_record(
                result.deleted, result.deleted_size,
                '永久删除' if permanent else ('回收站+已清空' if auto_empty else '回收站'),
                list(self.app.selected_rules), real_freed=real)
        # 清空已清理项的显示
        for res in self.app.scan_results:
            if res.rule['name'] in self.app.selected_rules:
                res.items = []
                res.total_size = 0
                res.file_count = 0
                st = self.rows.get(res.rule['name'])
                if st:
                    st['size'] = 0
                    st['count'] = 0
                    st['scanned'] = False
                    st['selected'] = False
                    self.app.selected_rules.discard(res.rule['name'])
                    self._render_row(res.rule['name'])
        self._update_selected()

        # 真实释放语义: 回收站模式只有清空回收站后才真正释放空间
        if permanent:
            msg = f'清理完成 · 已永久删除 {human_size(result.deleted_size)}'
            toast_kind, toast_msg = 'success', f'✓ 已彻底释放 {human_size(result.deleted_size)}'
            status_fg = C['green']
        elif real_freed:
            msg = (f'清理完成 · 已彻底释放 {human_size(real_freed)} '
                   f'(含清空回收站) · 空目录 +{cleaned_dirs}')
            toast_kind = 'success'
            toast_msg = f'✓ 已彻底释放 {human_size(real_freed)}'
            status_fg = C['green']
        else:
            msg = (f'已移入回收站 {human_size(result.deleted_size)} · '
                   f'清空回收站后才会真正释放磁盘空间')
            toast_kind = 'info'
            toast_msg = (f'已移入回收站 {human_size(result.deleted_size)} · '
                         f'清空回收站后才真正释放')
            status_fg = C['yellow']
        if result.failed:
            msg += f' · {len(result.failed)} 项失败'
        self.status_label.config(text=msg, fg=status_fg)
        self.app.toast(toast_msg, toast_kind)
        # 刷新仪表盘数据
        if 'dashboard' in self.app.pages and self.app.pages['dashboard']._built:
            self.app.pages['dashboard'].refresh()
