# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 页面: 程序搬家 / 磁盘分析 / 启动项 / 优化报告
"""

import os
import subprocess
import threading
import tkinter as tk
from tkinter import messagebox

from gui.theme import C, FONT_FAMILY, px, round_rect, lerp_color, ease_out_cubic
from gui.widgets import Btn, ProgressBar, ScrollFrame, Card, Toggle, chip

from core import config, sysinfo, history
from core.scanner import DiskAnalyzer
from core.cleaner import Cleaner, CleanResult
from core.mover import AppMover, get_user_env
from core.analyzer import Analyzer, human_size


def open_in_explorer(path):
    """在资源管理器中定位文件。"""
    try:
        if os.path.isfile(path):
            subprocess.Popen(['explorer', '/select,', os.path.normpath(path)])
        else:
            os.startfile(path)
    except Exception:
        pass


# ===========================================================================
# 程序搬家
# ===========================================================================
class MovePage:
    def __init__(self, app, frame):
        self.app = app
        self.f = frame
        self.target_drive = 'D:'
        self.moving = False
        self._build()

    def on_show(self, **kw):
        if not self.moving and not self._cards_built():
            self.refresh()

    def _cards_built(self):
        return any(self.cards_host.winfo_children())

    # ------------------------------------------------------------------
    def _build(self):
        self.app.page_header(self.f, '程序搬家',
                             '把开发缓存 / 大型应用数据目录重定向到其他盘, 从源头减少 C 盘占用')

        bar = tk.Frame(self.f, bg=C['bg'])
        bar.pack(fill='x', padx=28, pady=(8, 0))
        # 右侧控件先布局 (pack 优先级高, 避免窄窗被挤压)
        self.btn_detect = Btn(bar, '检测可搬迁项', command=self.refresh,
                              style='primary', width=110, height=34)
        self.btn_detect.pack(side='right')
        self.total_label = tk.Label(bar, text='', font=(FONT_FAMILY, 9, 'bold'),
                                    bg=C['bg'], fg=C['text_dim'], anchor='e')
        self.total_label.pack(side='right', padx=(12, 2))
        self.progress = ProgressBar(bar, width=220, height=6)
        self.progress.pack(side='right', padx=8)
        # 左侧
        tk.Label(bar, text='目标盘', font=(FONT_FAMILY, 9), bg=C['bg'],
                 fg=C['text_dim']).pack(side='left', padx=(0, 8))
        self.drive_bar = tk.Frame(bar, bg=C['bg'])
        self.drive_bar.pack(side='left')

        # 状态独立一行, 不与按钮争空间
        self.status_label = tk.Label(self.f, text='', font=(FONT_FAMILY, 9),
                                     bg=C['bg'], fg=C['text_dim'], anchor='w')
        self.status_label.pack(fill='x', padx=28, pady=(4, 0))

        self.scroll = ScrollFrame(self.f, bg=C['bg'])
        self.scroll.pack(fill='both', expand=True, padx=28, pady=(12, 20))
        self.cards_host = self.scroll.inner

    # ------------------------------------------------------------------
    def _build_drive_buttons(self):
        for w in self.drive_bar.winfo_children():
            w.destroy()
        drives = [d for d in sysinfo.get_all_disks() if d['letter'] != 'C']
        if not drives:
            drives = [{'drive': 'D:\\', 'letter': 'D'}]
        for d in drives:
            letter = d['letter']
            active = f'{letter}:' == self.target_drive
            b = tk.Label(self.drive_bar, text=f' {letter}: ',
                         font=(FONT_FAMILY, 10, 'bold'),
                         bg=C['accent_soft'] if active else C['card'],
                         fg=C['accent'] if active else C['text_dim'],
                         padx=12, pady=4, cursor='hand2')
            b.pack(side='left', padx=(0, 6))
            b.bind('<Button-1>', lambda e, l=letter: self._set_drive(l))

    def _set_drive(self, letter):
        self.target_drive = f'{letter}:'
        self._build_drive_buttons()
        self.refresh()

    # ------------------------------------------------------------------
    def refresh(self):
        if self.moving:
            return
        self._build_drive_buttons()
        for w in self.cards_host.winfo_children():
            w.destroy()
        self.status_label.config(text='检测中…', fg=C['accent'])
        self.progress.start_shimmer()

        mover = AppMover(target_drive=self.target_drive)

        def worker():
            movable = mover.list_movable()

            def apply():
                self.progress.stop_shimmer()
                self._populate(movable)
            self.app.post(apply)

        self.app.run_worker(worker)

    def _populate(self, movable):
        total = sum(m['current_size'] for m in movable)
        self.total_label.config(
            text=f'检测到 {len(movable)} 个可搬迁项 · 共 {human_size(total)}')
        self.status_label.config(text='', fg=C['text_dim'])
        if not movable:
            tk.Label(self.cards_host, text='未检测到可搬迁的应用缓存',
                     font=(FONT_FAMILY, 10), bg=self.cards_host['bg'],
                     fg=C['text_faint']).pack(pady=30)
            return
        for app in movable:
            self._make_card(app)

    def _make_card(self, app):
        card = Card(self.cards_host, padx=16, pady=13)
        card.pack(fill='x', pady=(0, 8))
        b = card.body

        top = tk.Frame(b, bg=b['bg'])
        top.pack(fill='x')
        tk.Label(top, text=app['name'], font=(FONT_FAMILY, 11, 'bold'),
                 bg=b['bg'], fg=C['text']).pack(side='left')

        size_lbl = tk.Label(top, text=human_size(app['current_size']),
                            font=(FONT_FAMILY, 12, 'bold'), bg=b['bg'], fg=C['accent'])
        size_lbl.pack(side='right')

        mid = tk.Frame(b, bg=b['bg'])
        mid.pack(fill='x', pady=(4, 0))
        tk.Label(mid, text=app['desc'], font=(FONT_FAMILY, 9), bg=b['bg'],
                 fg=C['text_dim']).pack(side='left')

        foot = tk.Frame(b, bg=b['bg'])
        foot.pack(fill='x', pady=(8, 0))
        btn = Btn(foot, '搬迁到 ' + self.target_drive, style='soft',
                  width=120, height=30,
                  command=lambda a=dict(app): self._relocate(a))
        btn.pack(side='right')
        env = app.get('env_var')
        if env:
            cur = get_user_env(env)
            target = app.get('target_subdir', '').replace('D:', self.target_drive)
            if cur == target:
                status = f'✓ 环境变量 {env} 已指向 {target}'
                color = C['green']
            elif cur:
                status = f'环境变量 {env} = {cur} (与目标不一致)'
                color = C['yellow']
            else:
                status = f'将设置环境变量 {env} → {target}'
                color = C['text_faint']
            status = status if len(status) <= 68 else status[:67] + '…'
            tk.Label(foot, text=status, font=(FONT_FAMILY, 8), bg=b['bg'],
                     fg=color, anchor='w').pack(side='left', fill='x', expand=True)
        else:
            tk.Label(foot, text='该应用通过配置命令或手动设置重定向',
                     font=(FONT_FAMILY, 8), bg=b['bg'], anchor='w',
                     fg=C['text_faint']).pack(side='left', fill='x', expand=True)


    # ------------------------------------------------------------------
    def _relocate(self, app):
        if self.moving:
            return
        if not messagebox.askyesno(
                '确认搬迁',
                f'将把「{app["name"]}」的数据迁移到 {self.target_drive} 盘并重定向?\n\n'
                f'迁移期间请关闭相关程序(IDE/浏览器/微信等)。',
                parent=self.app.root):
            return
        self.moving = True
        self.status_label.config(text='正在搬迁…', fg=C['accent'])
        self.progress.start_shimmer()
        mover = AppMover(target_drive=self.target_drive)

        def worker():
            def cb(moved, failed):
                self.app.post(lambda: self.status_label.config(
                    text=f'迁移中… {moved} 项'))
            res = mover.relocate(app, move_data_flag=True, progress_cb=cb)

            def done():
                self.moving = False
                self.progress.stop_shimmer()
                self.status_label.config(text='', fg=C['text_dim'])
                kind = 'success' if res.success else 'error'
                self.app.toast(f'{res.name}: {res.message}', kind)
                self.refresh()
            self.app.post(done)

        self.app.run_worker(worker)


# ===========================================================================
# 磁盘分析
# ===========================================================================
class AnalyzePage:
    TABS = ['大文件', '重复文件', '旧文件', '目录占用']

    def __init__(self, app, frame):
        self.app = app
        self.f = frame
        self.analyzer = None
        self.analyzing = False
        self.data = {'large': [], 'dups': [], 'old': [], 'dirs': []}
        self.large_threshold = config.LARGE_FILE_THRESHOLD
        self.tab = 0
        self._build()

    def on_show(self, **kw):
        pass

    # ------------------------------------------------------------------
    def _build(self):
        self.app.page_header(self.f, '磁盘分析',
                             '大文件 / 重复文件 / 长期未用文件 / 目录占用排行 (扫描时自动跳过系统目录)')

        bar = tk.Frame(self.f, bg=C['bg'])
        bar.pack(fill='x', padx=28, pady=(8, 4))
        # 右侧按钮先布局
        self.btn_analyze = Btn(bar, '开始分析', command=self.start_analyze,
                               style='primary', width=104, height=32)
        self.btn_analyze.pack(side='right')
        self.btn_stop = Btn(bar, '停止', command=self.stop,
                            style='ghost', width=64, height=32)
        self.btn_stop.pack(side='right', padx=(0, 8))
        # 左侧
        tk.Label(bar, text='磁盘', font=(FONT_FAMILY, 9), bg=C['bg'],
                 fg=C['text_dim']).pack(side='left', padx=(0, 8))
        self.drive_bar = tk.Frame(bar, bg=C['bg'])
        self.drive_bar.pack(side='left')
        self._build_drive_buttons()

        tk.Label(bar, text='大文件阈值', font=(FONT_FAMILY, 9), bg=C['bg'],
                 fg=C['text_dim']).pack(side='left', padx=(16, 6))
        from gui.widgets import Segmented
        self.seg_size = Segmented(bar, ['100MB', '500MB', '1GB'],
                                  command=self._on_threshold, width=190, height=28)
        self.seg_size.pack(side='left')
        self.dup_keep_newest = tk.BooleanVar(value=False)
        tk.Checkbutton(bar, text='重复文件保留最新', variable=self.dup_keep_newest,
                       font=(FONT_FAMILY, 9), bg=C['bg'], fg=C['text_dim'],
                       selectcolor=C['card2'], activebackground=C['bg'],
                       activeforeground=C['text']).pack(side='left', padx=(14, 0))

        # 进度
        prog = tk.Frame(self.f, bg=C['bg'])
        prog.pack(fill='x', padx=28, pady=(4, 0))
        self.status = tk.Label(prog, text='选择磁盘后点击「开始分析」', font=(FONT_FAMILY, 9),
                               bg=C['bg'], fg=C['text_dim'])
        self.status.pack(anchor='w')
        self.progress = ProgressBar(prog, width=880, height=6)
        self.progress.pack(fill='x', pady=(5, 2))

        def _prog_resize(e, pb=self.progress):
            pb.cw = e.width
            pb._draw()
        self.progress.bind('<Configure>', _prog_resize)

        # 子标签
        tabbar = tk.Frame(self.f, bg=C['bg'])
        tabbar.pack(fill='x', padx=28, pady=(6, 0))
        self.tab_cv = tk.Canvas(tabbar, height=px(38), bg=C['bg'], highlightthickness=0)
        self.tab_cv.pack(fill='x')
        self.tab_cv.bind('<Configure>', lambda e: self._draw_tabs())
        self._tab_hot = [tabbar]  # keep ref

        # 结果列表
        self.scroll = ScrollFrame(self.f, bg=C['bg'])
        self.scroll.pack(fill='both', expand=True, padx=28, pady=(8, 20))
        self.list_host = self.scroll.inner
        self._draw_tabs()

    def _build_drive_buttons(self):
        for w in self.drive_bar.winfo_children():
            w.destroy()
        self.disks = sysinfo.get_all_disks()
        if not hasattr(self, 'cur_disk') or not any(
                d['drive'] == getattr(self, 'cur_disk', None) for d in self.disks):
            self.cur_disk = self.disks[0]['drive'] if self.disks else 'C:\\'
        for d in self.disks:
            active = d['drive'] == self.cur_disk
            b = tk.Label(self.drive_bar,
                         text=f" {d['letter']}: {human_size(d['free'])} 可用 ",
                         font=(FONT_FAMILY, 9, 'bold'),
                         bg=C['accent_soft'] if active else C['card'],
                         fg=C['accent'] if active else C['text_dim'],
                         padx=10, pady=4, cursor='hand2')
            b.pack(side='left', padx=(0, 6))
            b.bind('<Button-1>', lambda e, dr=d['drive']: self._set_disk(dr))

    def _set_disk(self, drive):
        self.cur_disk = drive
        self._build_drive_buttons()

    def _on_threshold(self, i):
        self.large_threshold = [100 * 1024 ** 2, 500 * 1024 ** 2, 1024 ** 3][i]

    # ------------------------------------------------------------------
    def _draw_tabs(self):
        cv = self.tab_cv
        cv.delete('all')
        w = cv.winfo_width() or px(700)
        x = 0.0
        self._tab_bounds = []
        for i, name in enumerate(self.TABS):
            active = i == self.tab
            font = (FONT_FAMILY, 10, 'bold') if active else (FONT_FAMILY, 10)
            tid = cv.create_text(x + px(16), px(19), anchor='w', text=name, font=font,
                                 fill=C['text'] if active else C['text_dim'])
            bbox = cv.bbox(tid)
            x2 = bbox[2] + px(16)
            self._tab_bounds.append((x, x2))
            if active:
                cv.create_line(x + px(4), px(34), x2 - px(4), px(34), fill=C['accent'], width=px(3))
            tag = f'tab{i}'
            cv.create_rectangle(x, 0, x2, px(38), fill='', outline='', tags=tag)
            cv.tag_bind(tag, '<Button-1>', lambda e, i=i: self._switch_tab(i))
            cv.tag_bind(tag, '<Enter>', lambda e, i=i: self._tab_hover(i, True))
            cv.tag_bind(tag, '<Leave>', lambda e, i=i: self._tab_hover(i, False))
            x = x2 + px(12)

    def _tab_hover(self, i, on):
        cv = self.tab_cv
        x1, x2 = self._tab_bounds[i]
        if on:
            cv.create_line(x1 + px(4), px(30), x2 - px(4), px(30), fill=C['border'], width=2,
                           tags='hover')
        else:
            cv.delete('hover')

    def _switch_tab(self, i):
        self.tab = i
        self._draw_tabs()
        self._render()

    # ------------------------------------------------------------------
    def start_analyze(self):
        if self.analyzing:
            return
        self.analyzing = True
        self.btn_analyze.set_style('ghost')
        self.btn_analyze.text = '分析中…'
        self.btn_analyze._draw()
        self.progress.start_shimmer()
        self.data = {'large': [], 'dups': [], 'old': [], 'dirs': []}
        self._render()
        root = self.cur_disk
        an = DiskAnalyzer(max_workers=8)
        self.analyzer = an
        thr = self.large_threshold

        def worker():
            stage = [0]
            stages = ['大文件', '重复文件', '旧文件', '目录占用']

            def note(s):
                self.app.post(lambda: self.status.config(
                    text=f'正在分析 {s} … ({stage[0] + 1}/4) · ' + root,
                    fg=C['accent']))

            def cb(n):
                pass

            note('大文件')
            large = an.find_large_files(root, thr, cb)
            self.app.post(lambda: self.status.config(text=f'正在分析 重复文件 … (2/4) · ' + root))
            dups = an.find_duplicates(root, progress_cb=cb)
            self.app.post(lambda: self.status.config(text=f'正在分析 旧文件 … (3/4) · ' + root))
            old = an.find_old_files(root, progress_cb=cb)
            self.app.post(lambda: self.status.config(text=f'正在分析 目录占用 … (4/4) · ' + root))
            dirs = an.top_dirs(root, 30, cb)

            def done():
                self.analyzing = False
                self.progress.stop_shimmer()
                self.data = {'large': large, 'dups': dups, 'old': old, 'dirs': dirs}
                dup_waste = sum(g[0]['size'] * (len(g) - 1) for g in dups)
                self.status.config(
                    text=f'分析完成 · 大文件 {len(large)} 个 · 重复组 {len(dups)} 组'
                         f' (浪费 {human_size(dup_waste)}) · 旧文件 {len(old)} 个',
                    fg=C['green'])
                self.btn_analyze.set_style('primary')
                self.btn_analyze.text = '重新分析'
                self.btn_analyze._draw()
                self._render()
                self.app.toast('磁盘分析完成', 'success')

            self.app.post(done)

        self.app.run_worker(worker)

    def stop(self):
        if self.analyzer:
            self.analyzer.stop()
        self.analyzing = False

    # ------------------------------------------------------------------
    def _render(self):
        for w in self.list_host.winfo_children():
            w.destroy()
        renderers = [self._render_large, self._render_dups,
                     self._render_old, self._render_dirs]
        renderers[self.tab]()

    def _empty_hint(self, text):
        tk.Label(self.list_host, text=text, font=(FONT_FAMILY, 10),
                 bg=self.list_host['bg'], fg=C['text_faint']).pack(pady=40)

    # ---- 大文件 ----
    def _render_large(self):
        data = self.data['large'][:200]
        if not data:
            self._empty_hint('暂无数据 · 点击「开始分析」')
            return
        for it in data:
            row = self._row_frame()
            cv = tk.Canvas(row, width=px(34), height=px(34), bg=row['bg'], highlightthickness=0)
            cv.pack(side='left', padx=(px(4), px(10)))
            ext = (it['ext'] or '?').strip('.').upper()[:4]
            round_rect(cv, 1, 1, px(33), px(33), px(8), fill=C['accent_soft'], outline='')
            cv.create_text(px(17), px(17), text=ext, font=(FONT_FAMILY, 8, 'bold'),
                           fill=C['accent'])
            tk.Label(row, text=human_size(it['size']),
                     font=(FONT_FAMILY, 11, 'bold'), bg=row['bg'],
                     fg=C['accent']).pack(side='right', padx=(10, 2))
            Btn(row, '定位', style='ghost', width=56, height=28,
                command=lambda p=it['path']: open_in_explorer(p)).pack(side='right', padx=(0, 6))
            Btn(row, '删除', style='danger', width=56, height=28,
                command=lambda p=it['path'], s=it['size']: self._delete_files([(p, s)])).pack(side='right')
            info = tk.Frame(row, bg=row['bg'])
            info.pack(side='left', fill='x', expand=True)
            path = it['path']
            disp = path if len(path) <= 70 else '…' + path[-69:]
            tk.Label(info, text=disp, font=(FONT_FAMILY, 9), bg=row['bg'],
                     fg=C['text'], anchor='w', justify='left').pack(fill='x')
            import time as _t
            mt = _t.strftime('%Y-%m-%d', _t.localtime(it['mtime']))
            tk.Label(info, text=f"修改于 {mt}", font=(FONT_FAMILY, 8),
                     bg=row['bg'], fg=C['text_faint'], anchor='w').pack(fill='x')

    # ---- 重复文件 ----
    def _render_dups(self):
        dups = self.data['dups'][:50]
        if not dups:
            self._empty_hint('暂无数据 · 点击「开始分析」')
            return
        for gi, group in enumerate(dups):
            waste = group[0]['size'] * (len(group) - 1)
            head = Card(self.list_host, padx=14, pady=10)
            head.pack(fill='x', pady=(0, 4))
            keep_idx = self._dup_keep_index(group)
            Btn(head.body, '删除其余 (保留最优)', style='danger', width=150, height=28,
                command=lambda g=list(group): self._delete_dups(g)).pack(side='right')
            tk.Label(head.body, text=f'重复组 {gi + 1} · {len(group)} 个相同文件 · 浪费 {human_size(waste)}',
                     font=(FONT_FAMILY, 10, 'bold'), bg=head.body['bg'],
                     fg=C['text']).pack(side='left', fill='x', expand=True)
            for i, it in enumerate(group):
                row = self._row_frame()
                mark_color = C['green'] if i == keep_idx else C['text_faint']
                mark = '✓ 保留' if i == keep_idx else '副本'
                tk.Label(row, text=human_size(it['size']), font=(FONT_FAMILY, 9),
                         bg=row['bg'], fg=C['text_dim']).pack(side='right', padx=(10, 2))
                Btn(row, '定位', style='ghost', width=56, height=26,
                    command=lambda p=it['path']: open_in_explorer(p)).pack(side='right')
                tk.Label(row, text=mark, font=(FONT_FAMILY, 8, 'bold'),
                         bg=row['bg'], fg=mark_color, width=7).pack(side='left')
                path = it['path']
                disp = path if len(path) <= 74 else '…' + path[-73:]
                tk.Label(row, text=disp, font=(FONT_FAMILY, 9), bg=row['bg'],
                         fg=C['text'], anchor='w').pack(side='left', fill='x', expand=True)

    # ---- 旧文件 ----
    def _render_old(self):
        data = self.data['old'][:200]
        if not data:
            self._empty_hint('暂无数据 · 点击「开始分析」')
            return
        for it in data:
            row = self._row_frame()
            tk.Label(row, text=human_size(it['size']), font=(FONT_FAMILY, 9),
                     bg=row['bg'], fg=C['text_dim']).pack(side='right', padx=(10, 2))
            Btn(row, '定位', style='ghost', width=56, height=28,
                command=lambda p=it['path']: open_in_explorer(p)).pack(side='right')
            tk.Label(row, text=f"{it['days_old']} 天", font=(FONT_FAMILY, 10, 'bold'),
                     bg=row['bg'], fg=C['yellow'], width=8).pack(side='left')
            path = it['path']
            disp = path if len(path) <= 76 else '…' + path[-75:]
            tk.Label(row, text=disp, font=(FONT_FAMILY, 9), bg=row['bg'],
                     fg=C['text'], anchor='w').pack(side='left', fill='x', expand=True)

    # ---- 目录占用 ----
    def _render_dirs(self):
        data = self.data['dirs']
        if not data:
            self._empty_hint('暂无数据 · 点击「开始分析」')
            return
        max_size = data[0]['size'] if data else 1
        host = self.list_host
        for it in data:
            row = tk.Frame(host, bg=C['card'])
            row.pack(fill='x', pady=(0, 5))
            bar = tk.Canvas(row, height=px(40), bg=C['card'], highlightthickness=0)
            bar.pack(fill='x')
            frac = it['size'] / max_size

            def draw(bar=bar, it=it, frac=frac):
                w = bar.winfo_width() or px(800)
                bar.delete('all')
                round_rect(bar, 0, 0, w, px(40), px(9), fill=C['card'], outline=C['border_soft'])
                bw = max(px(24), (w - px(200)) * frac)
                round_rect(bar, 1, 1, bw, px(39), px(8), fill=lerp_color(C['accent_soft'],
                         C['accent'], min(1, frac + 0.15)), outline='')
                bar.create_text(px(14), px(20), anchor='w', text=it['path'],
                                font=(FONT_FAMILY, 9), fill=C['text'])
                bar.create_text(w - px(12), px(20), anchor='e', text=human_size(it['size']),
                                font=(FONT_FAMILY, 10, 'bold'), fill=C['text'])
                bar.create_text(w - px(110), px(20), anchor='e',
                                text=f"{frac*100:.1f}%",
                                font=(FONT_FAMILY, 8), fill=C['text_dim'])
                bar.bind('<Button-1>', lambda e, p=it['path']: open_in_explorer(p))
                bar.bind('<Configure>', lambda e: draw())
            draw()
            # 宽度动画
            def animate(bar=bar, frac=frac):
                pass
            draw()

    def _row_frame(self):
        row = tk.Frame(self.list_host, bg=C['card'], padx=12, pady=8)
        row.pack(fill='x', pady=(0, 5))
        row.bind('<Enter>', lambda e: row.config(bg=C['card_hover']))
        row.bind('<Leave>', lambda e: row.config(bg=C['card']))
        for child in row.winfo_children():
            child.bind('<Enter>', lambda e: row.config(bg=C['card_hover']))
            child.bind('<Leave>', lambda e: row.config(bg=C['card']))
        return row

    # ------------------------------------------------------------------
    def _delete_files(self, path_size_list):
        if not messagebox.askyesno(
                '确认删除', f'将删除 {len(path_size_list)} 个文件到回收站?\n\n'
                f'共 {human_size(sum(s for _, s in path_size_list))}',
                parent=self.app.root):
            return

        deleted_paths = {p for p, _ in path_size_list}

        def worker():
            from core.scanner import FileItem
            cleaner = Cleaner(use_recycle_bin=True)
            items = [FileItem(path=p, size=s) for p, s in path_size_list]
            result = cleaner.clean_items(items)

            def done():
                history.add_record(result.deleted, result.deleted_size,
                                   '回收站', ['磁盘分析-手动删除'])
                self.app.toast(f'已删除 {result.deleted} 项 · {human_size(result.deleted_size)}', 'success')
                # 局部刷新: 从当前结果中移除已删除项并重渲染 (避免全盘重扫)
                self._prune_deleted(deleted_paths)

            self.app.post(done)

    def _prune_deleted(self, deleted_paths):
        d = self.data
        d['large'] = [it for it in d['large'] if it['path'] not in deleted_paths]
        d['old'] = [it for it in d['old'] if it['path'] not in deleted_paths]
        new_groups = []
        for g in d['dups']:
            g2 = [it for it in g if it['path'] not in deleted_paths]
            if len(g2) >= 2:
                new_groups.append(g2)
        d['dups'] = new_groups
        self._render()

        self.app.run_worker(worker)

    def _dup_keep_index(self, group):
        """保留策略: 勾选'保留最新'时保留修改时间最新的, 否则保留第一个。"""
        if getattr(self, 'dup_keep_newest', None) and self.dup_keep_newest.get():
            return max(range(len(group)),
                       key=lambda i: group[i].get('mtime', 0))
        return 0

    def _delete_dups(self, group):
        keep = self._dup_keep_index(group)
        rest = [it for i, it in enumerate(group) if i != keep]
        path_size_list = [(it['path'], it['size']) for it in rest]
        self._delete_files(path_size_list)


# ===========================================================================
# 启动项管理
# ===========================================================================
class StartupPage:
    def __init__(self, app, frame):
        self.app = app
        self.f = frame
        self._build()

    def on_show(self, **kw):
        self.refresh()

    def _build(self):
        self.app.page_header(self.f, '启动项管理',
                             '查看并管理开机自启动程序 · 禁用的项可随时恢复')

        bar = tk.Frame(self.f, bg=C['bg'])
        bar.pack(fill='x', padx=28, pady=(8, 0))
        self.info = tk.Label(bar, text='', font=(FONT_FAMILY, 9), bg=C['bg'],
                             fg=C['text_dim'])
        self.info.pack(side='left')
        Btn(bar, '刷新', command=self.refresh, style='ghost',
            width=70, height=30).pack(side='right')

        self.scroll = ScrollFrame(self.f, bg=C['bg'])
        self.scroll.pack(fill='both', expand=True, padx=28, pady=(12, 20))
        self.list_host = self.scroll.inner

    def refresh(self):
        for w in self.list_host.winfo_children():
            w.destroy()
        try:
            items = sysinfo.list_startup_items()
        except Exception as e:
            items = []
            self.info.config(text=f'读取失败: {e}', fg=C['red'])
            return
        enabled = sum(1 for i in items if i['enabled'])
        self.info.config(text=f'共 {len(items)} 个启动项 · {enabled} 个已启用', fg=C['text_dim'])
        if not items:
            tk.Label(self.list_host, text='未发现启动项', font=(FONT_FAMILY, 10),
                     bg=self.list_host['bg'], fg=C['text_faint']).pack(pady=40)
            return
        for it in items:
            self._make_row(it)

    def _make_row(self, it):
        row = tk.Frame(self.list_host, bg=C['card'], padx=14, pady=10)
        row.pack(fill='x', pady=(0, 6))
        tg = Toggle(row, value=it['enabled'], width=44, height=23,
                    command=lambda v, i=dict(it): self._toggle(i, v))
        tg.pack(side='right')
        left = tk.Frame(row, bg=C['card'])
        left.pack(side='left', fill='x', expand=True)
        name_row = tk.Frame(left, bg=C['card'])
        name_row.pack(fill='x')
        tk.Label(name_row, text=it['name'], font=(FONT_FAMILY, 10, 'bold'),
                 bg=C['card'], fg=C['text']).pack(side='left')
        loc = '已禁用' if not it['enabled'] else it['location']
        loc_color = C['text_faint'] if not it['enabled'] else C['accent']
        tk.Label(name_row, text=f'  {loc}', font=(FONT_FAMILY, 8),
                 bg=C['card'], fg=loc_color).pack(side='left')
        cmd = it['command']
        disp = cmd if len(cmd) <= 86 else '…' + cmd[-85:]
        tk.Label(left, text=disp, font=('Consolas', 8), bg=C['card'],
                 fg=C['text_faint'], anchor='w', justify='left').pack(fill='x')

    def _toggle(self, item, value):
        ok = sysinfo.set_startup_enabled(item, value)
        if ok:
            self.app.toast(f"「{item['name']}」已{'启用' if value else '禁用'}",
                           'success')
        else:
            self.app.toast(f"操作失败 (可能需要管理员权限或名称冲突)", 'error')
        self.app.root.after(300, self.refresh)


# ===========================================================================
# 优化报告
# ===========================================================================
class ReportPage:
    def __init__(self, app, frame):
        self.app = app
        self.f = frame
        self.reports_dir = os.path.join(
            os.environ.get('LOCALAPPDATA', os.path.expanduser('~')),
            'ZDiskCleanerPro', 'reports')
        self._build()

    def on_show(self, **kw):
        pass

    def _build(self):
        self.app.page_header(self.f, '优化报告',
                             '生成 Markdown 格式的磁盘体检报告, 包含可清理空间与搬家建议')

        bar = tk.Frame(self.f, bg=C['bg'])
        bar.pack(fill='x', padx=28, pady=(8, 0))
        self.btn_gen = Btn(bar, '生成报告', command=self.generate,
                           style='primary', width=104, height=32)
        self.btn_gen.pack(side='left')
        Btn(bar, '打开报告目录', command=lambda: open_in_explorer(self.reports_dir),
            style='ghost', width=110, height=32).pack(side='left', padx=8)
        self.status = tk.Label(bar, text='', font=(FONT_FAMILY, 9), bg=C['bg'],
                               fg=C['text_dim'])
        self.status.pack(side='left', padx=12)

        card = Card(self.f)
        card.pack(fill='both', expand=True, padx=28, pady=(12, 20))
        self.preview = tk.Text(card.body, bg=C['card'], fg=C['text'],
                               font=('Consolas', 9), bd=0, padx=14, pady=12,
                               wrap='none', insertbackground=C['text'],
                               selectbackground=C['accent_soft'])
        self.preview.pack(fill='both', expand=True, side='left')
        sb = tk.Scrollbar(card.body, command=self.preview.yview,
                          bg=C['card'], troughcolor=C['card'],
                          activebackground=C['scrollbar'], bd=0, width=10)
        sb.pack(side='right', fill='y')
        self.preview.config(yscrollcommand=sb.set)
        self._insert_text('# 点击「生成报告」开始\n\n报告将包含:\n- 可清理空间统计\n- 分类明细\n- 搬家建议\n')

    def _insert_text(self, text):
        self.preview.delete('1.0', 'end')
        self.preview.insert('1.0', text)

    # ------------------------------------------------------------------
    def generate(self):
        if self.app.scan_results:
            self._do_generate(self.app.scan_results)
            return
        # 没有扫描结果则先扫描
        self.status.config(text='正在扫描 (首次可能需要 1-2 分钟)…', fg=C['accent'])
        self.btn_gen.set_style('ghost')
        self.btn_gen.text = '生成中…'
        self.btn_gen._draw()
        from core.scanner import Scanner

        scanner = Scanner(max_workers=8)

        def worker():
            results = scanner.scan_all()

            def done():
                self.app.scan_results = results
                self.btn_gen.set_style('primary')
                self.btn_gen.text = '生成报告'
                self.btn_gen._draw()
                self._do_generate(results)
            self.app.post(done)

        self.app.run_worker(worker)

    def _do_generate(self, results):
        self.status.config(text='生成中…', fg=C['accent'])
        analyzer = Analyzer(results)

        def worker():
            import time
            os.makedirs(self.reports_dir, exist_ok=True)
            path = os.path.join(
                self.reports_dir, f'report_{time.strftime("%Y%m%d_%H%M%S")}.md')
            content = analyzer.generate_report(path)

            def done():
                self.status.config(text=f'已生成: {path}', fg=C['green'])
                self._insert_text(content)
                self.app.toast('报告已生成', 'success')
            self.app.post(done)

        self.app.run_worker(worker)
