# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 数据统计分析模块
对扫描结果进行多维度统计: 按类别/风险汇总、可释放空间计算、
生成搬家建议与 Markdown 优化报告。
"""

import os
import time
from collections import defaultdict
from typing import List

from . import config


def human_size(n) -> str:
    if n is None:
        return '0 B'
    n = float(n)
    for unit in ('B', 'KB', 'MB', 'GB', 'TB'):
        if abs(n) < 1024.0:
            if unit != 'B':
                return f'{n:.2f} {unit}'
            return f'{int(n)} {unit}'
        n /= 1024.0
    return f'{n:.2f} PB'


class Analyzer:
    def __init__(self, scan_results: List):
        self.results = scan_results

    def summary(self) -> dict:
        total_size = 0
        total_files = 0
        by_category = defaultdict(lambda: {'size': 0, 'count': 0})
        by_risk = defaultdict(lambda: {'size': 0, 'count': 0})
        rules_summary = []

        for res in self.results:
            total_size += res.total_size
            total_files += res.file_count
            by_category[res.rule['category']]['size'] += res.total_size
            by_category[res.rule['category']]['count'] += res.file_count
            by_risk[res.rule['risk']]['size'] += res.total_size
            by_risk[res.rule['risk']]['count'] += res.file_count
            rules_summary.append({
                'name': res.rule['name'],
                'category': res.rule['category'],
                'risk': res.rule['risk'],
                'size': res.total_size,
                'count': res.file_count,
                'default_select': res.rule.get('default_select', False),
            })

        rules_summary.sort(key=lambda x: x['size'], reverse=True)
        return {
            'total_size': total_size,
            'total_files': total_files,
            'by_category': dict(by_category),
            'by_risk': dict(by_risk),
            'rules': rules_summary,
        }

    def selected_total(self, selected_rule_names: set) -> tuple:
        total = 0
        count = 0
        for res in self.results:
            if res.rule['name'] in selected_rule_names:
                total += res.total_size
                count += res.file_count
        return total, count

    @staticmethod
    def relocation_suggestions() -> List[dict]:
        """搬家建议: 检测本机存在的可搬迁应用。"""
        from .mover import AppMover
        mover = AppMover()
        return mover.list_movable()

    def generate_report(self, report_path: str = None) -> str:
        s = self.summary()
        now = time.strftime('%Y-%m-%d %H:%M:%S')

        lines = [
            '# ZDiskCleaner Pro 磁盘优化报告',
            '',
            f'> 生成时间: {now}',
            '',
            '## 总览',
            '',
            f'- 可清理空间: **{human_size(s["total_size"])}**',
            f'- 可清理项目: **{s["total_files"]}** 项',
            '',
            '## 按类别',
            '',
            '| 类别 | 大小 | 项目数 |',
            '| --- | --- | --- |',
        ]
        for cat, info in sorted(s['by_category'].items(),
                                key=lambda x: x[1]['size'], reverse=True):
            lines.append(f'| {cat} | {human_size(info["size"])} | {info["count"]} |')

        lines += ['', '## 清理项明细', '',
                  '| 清理项 | 类别 | 大小 | 项目数 | 风险 |',
                  '| --- | --- | --- | --- | --- |']
        for r in s['rules']:
            if r['size'] > 0:
                lines.append(
                    f'| {r["name"]} | {r["category"]} | {human_size(r["size"])} '
                    f'| {r["count"]} | {config.RISK_LABEL.get(r["risk"], r["risk"])} |')

        lines += ['', '## 搬家建议 (从源头减少 C 盘占用)', '']
        try:
            for app in self.relocation_suggestions():
                size = app.get('current_size', 0)
                if size > 1024 * 1024:
                    lines.append(f'- **{app["name"]}**: 当前占用 {human_size(size)}, '
                                 f'可搬迁至其他盘 ({app["desc"]})')
        except Exception:
            pass

        lines += ['', '---', '', '*由 ZDiskCleaner Pro 自动生成*', '']
        content = '\n'.join(lines)

        if report_path:
            os.makedirs(os.path.dirname(report_path) or '.', exist_ok=True)
            with open(report_path, 'w', encoding='utf-8') as f:
                f.write(content)
        return content
