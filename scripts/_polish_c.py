# -*- coding: utf-8 -*-
"""精修第三批:主 CTA 光泽、CleaningOverlay 发丝线与玻璃、Results 战报横幅精装。"""

# ═══ 1. Home 主按钮:光泽 + 更立体的投影 ═══
p = r'C:\Users\yusheng\.zcode\workspace\default\ZDiskCleanerPro\ui\src\pages\Home.tsx'
src = open(p, encoding='utf-8').read()
old = '''                    <motion.button
                        onClick={() => void startScan()}
                        whileHover={{ y: -2 }}
                        whileTap={{ scale: 0.97 }}
                        transition={springSnappy}
                        className="mt-7 flex items-center gap-2 rounded-full px-9 py-3.5 text-base font-medium text-white"
                        style={{
                            background:
                                "linear-gradient(135deg, var(--zc-accent-a), var(--zc-accent-b))",
                            boxShadow: "0 10px 30px -8px color-mix(in srgb, var(--zc-accent-a) 65%, transparent)",
                        }}
                    >'''
new = '''                    <motion.button
                        onClick={() => void startScan()}
                        whileHover={{ y: -2 }}
                        whileTap={{ scale: 0.97 }}
                        transition={springSnappy}
                        className="zc-sheen mt-7 flex items-center gap-2 rounded-full px-9 py-3.5 text-base font-medium text-white"
                        style={{
                            background: "var(--zc-grad-brand)",
                            boxShadow:
                                "0 12px 34px -8px color-mix(in srgb, var(--zc-accent-a) 70%, transparent), inset 0 1px 0 rgb(255 255 255 / .35)",
                        }}
                    >'''
assert old in src, "home hero"
src = src.replace(old, new)
open(p, 'w', encoding='utf-8').write(src)
print('Home hero OK')

# ═══ 2. Duplicates/BigFiles 扫描按钮:同款光泽 ═══
for pg, anchor in [
    ("Duplicates", 'className="flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"'),
    ("BigFiles", None),
]:
    p = rf'C:\Users\yusheng\.zcode\workspace\default\ZDiskCleanerPro\ui\src\pages\{pg}.tsx'
    src = open(p, encoding='utf-8').read()
    src = src.replace(
        'style={{ background: "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))", color: "#ffffff" }}',
        'style={{ background: "var(--zc-grad-brand)", color: "#ffffff", boxShadow: "0 8px 22px -8px color-mix(in srgb, var(--zc-accent-a) 60%, transparent), inset 0 1px 0 rgb(255 255 255 / .3)" }}')
    src = src.replace(
        'className="flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"',
        'className="zc-sheen flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"')
    open(p, 'w', encoding='utf-8').write(src)
    print(f'{pg} scan btn OK')

# ═══ 3. CleaningOverlay:品牌发丝线 + 玻璃卡片精装 ═══
p = r'C:\Users\yusheng\.zcode\workspace\default\ZDiskCleanerPro\ui\src\pages\CleaningOverlay.tsx'
src = open(p, encoding='utf-8').read()
old = '''                            className="w-[min(440px,88vw)] rounded-2xl border p-7 text-center"
                            style={{
                                background: "color-mix(in srgb, var(--zc-surface-1) 72%, transparent)",
                                borderColor: "var(--zc-border-strong)",
                                boxShadow: "var(--zc-shadow-pop)",
                            }}'''
new = '''                            className="relative w-[min(440px,88vw)] overflow-hidden rounded-2xl border p-7 pt-8 text-center"
                            style={{
                                background: "color-mix(in srgb, var(--zc-surface-1) 76%, transparent)",
                                borderColor: "var(--zc-border-strong)",
                                boxShadow: "var(--zc-shadow-pop)",
                                backdropFilter: "blur(14px)",
                            }}'''
assert old in src, "overlay card"
src = src.replace(old, new)
old = '''                        <div className="mt-5 text-base font-medium">正在安全搬运…</div>'''
new = '''                        <div
                            className="absolute inset-x-0 top-0 h-px"
                            style={{ background: "var(--zc-hairline)" }}
                        />
                        <div className="mt-5 text-base font-medium">正在安全搬运…</div>'''
assert old in src, "overlay hairline"
src = src.replace(old, new)
open(p, 'w', encoding='utf-8').write(src)
print('CleaningOverlay OK')

# ═══ 4. Results 战报横幅(Home 的 cleanOutcome 卡):精装 ═══
p = r'C:\Users\yusheng\.zcode\workspace\default\ZDiskCleanerPro\ui\src\pages\Home.tsx'
src = open(p, encoding='utf-8').read()
old = '''                        className="mb-6 flex items-center justify-between rounded-xl border px-4 py-3"
                        style={{
                            background: "color-mix(in srgb, var(--zc-ok) 10%, var(--zc-surface-1))",
                            borderColor: "color-mix(in srgb, var(--zc-ok) 30%, transparent)",
                        }}'''
new = '''                        className="relative mb-6 flex items-center justify-between overflow-hidden rounded-xl border px-4 py-3"
                        style={{
                            background: "color-mix(in srgb, var(--zc-ok) 10%, var(--zc-surface-1))",
                            borderColor: "color-mix(in srgb, var(--zc-ok) 30%, transparent)",
                            boxShadow: "var(--zc-shadow-1)",
                        }}'''
assert old in src, "banner"
src = src.replace(old, new)
# 左缘 ok 色条
old = '''                        <div className="flex items-center gap-2 text-sm">
                            <ShieldCheck size={16} style={{ color: "var(--zc-ok)" }} />'''
new = '''                        <span className="absolute inset-y-0 left-0 w-[3px]" style={{ background: "var(--zc-ok)" }} />
                        <div className="flex items-center gap-2 text-sm">
                            <ShieldCheck size={16} style={{ color: "var(--zc-ok)" }} />'''
assert old in src
src = src.replace(old, new)
open(p, 'w', encoding='utf-8').write(src)
print('Home banner OK')
