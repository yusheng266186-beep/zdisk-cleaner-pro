import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useStore } from "../store";
import { Search, CornerDownLeft } from "lucide-react";

interface Cmd {
    id: string;
    label: string;
    /** 打分语料：中英文别名/同义词，命中子串即可搜到 */
    keywords?: string[];
    hint?: string;
    run: () => void;
}

/* ── 打分排序（v5）：label + keywords 子串匹配；前缀命中加权，不做拼音 ── */
function scoreCmd(ql: string, c: Cmd): number {
    if (!ql) return 0;
    let best = -1;
    const texts = [c.label, ...(c.keywords ?? [])];
    for (const t of texts) {
        const i = t.toLowerCase().indexOf(ql);
        if (i === -1) continue;
        const s = (i === 0 ? 100 : 60) - Math.min(i, 20) + (t === c.label ? 8 : 0);
        if (s > best) best = s;
    }
    return best;
}

/** Ctrl+K 命令面板：毛玻璃浮层 + 键盘导航。
 *  动画只用 transform/opacity，保证合成器路径。
 *  v5：焦点陷阱（Tab 循环）、关闭还焦触发元素、active 项 scrollIntoView；
 *  Esc 只归本面板处理——页面 armed 态的 Esc hook 在面板打开时让位（paletteOpen）。 */
export function CommandPalette() {
    const open = useStore((s) => s.paletteOpen);
    const togglePalette = useStore((s) => s.togglePalette);
    const startScan = useStore((s) => s.startScan);
    const selectSafeOnly = useStore((s) => s.selectSafeOnly);
    const selectAll = useStore((s) => s.selectAll);
    const clearSelection = useStore((s) => s.clearSelection);
    const toggleTheme = useStore((s) => s.toggleTheme);
    const setActivePage = useStore((s) => s.setActivePage);

    const [q, setQ] = useState("");
    const [idx, setIdx] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);
    const panelRef = useRef<HTMLDivElement>(null);
    const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
    // 关闭时把焦点还给打开面板的触发元素
    const triggerRef = useRef<HTMLElement | null>(null);

    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
                e.preventDefault();
                if (!useStore.getState().paletteOpen) triggerRef.current = document.activeElement as HTMLElement | null;
                togglePalette();
            }
            if (e.key === "Escape" && useStore.getState().paletteOpen) togglePalette(false);
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [togglePalette]);

    useEffect(() => {
        if (open) {
            setQ("");
            setIdx(0);
            if (!triggerRef.current) triggerRef.current = document.activeElement as HTMLElement | null;
            setTimeout(() => inputRef.current?.focus(), 30);
        } else {
            // 关闭还焦：触发元素可能已被卸载（如切页），静默失败即可
            requestAnimationFrame(() => triggerRef.current?.focus?.());
            triggerRef.current = null;
        }
    }, [open]);

    const cmds: Cmd[] = useMemo(
        () => [
            { id: "nav-home", label: "前往 体检台", keywords: ["home", "jiankangtai", "首页"], run: () => setActivePage("home") },
            { id: "nav-results", label: "前往 体检结果", keywords: ["results", "jieguo", "结果页"], run: () => setActivePage("results") },
            { id: "nav-radar", label: "前往 空间雷达", keywords: ["radar", "leida", "treemap", "体积树"], run: () => setActivePage("radar") },
            { id: "nav-startup", label: "前往 启动项管家", keywords: ["startup", "qidongxiang", "自启"], run: () => setActivePage("startup") },
            { id: "nav-migrate", label: "前往 迁移中心", keywords: ["migrate", "qianyi", "junction"], run: () => setActivePage("migrate") },
            { id: "nav-history", label: "前往 历史", keywords: ["history", "lishi", "台账"], run: () => setActivePage("history") },
            { id: "nav-tools", label: "前往 工具箱", keywords: ["tools", "gongju"], run: () => setActivePage("tools") },
            { id: "nav-settings", label: "前往 设置", keywords: ["settings", "shezhi"], run: () => setActivePage("settings") },
            { id: "nav-deeptools", label: "前往 深度工具", keywords: ["deep", "dism", "shendu", "winsxs"], run: () => setActivePage("deeptools") },
            { id: "nav-bigfiles", label: "前往 大文件", keywords: ["big", "da", "wenjian"], run: () => setActivePage("bigfiles") },
            { id: "nav-dupes", label: "前往 重复文件", keywords: ["dupes", "duplicate", "chongfu"], run: () => setActivePage("dupes") },
            { id: "scan", label: "开始磁盘体检", keywords: ["scan", "saomiao", "体检", "扫描"], run: () => void startScan() },
            { id: "safe", label: "只勾选安全规则", keywords: ["safe", "anquan", "勾选"], run: selectSafeOnly },
            { id: "all", label: "全选规则", keywords: ["all", "quanxuan", "勾选"], run: selectAll },
            { id: "clear", label: "清空勾选", keywords: ["clear", "qingkong", "取消勾选"], run: clearSelection },
            { id: "theme", label: "切换 明/暗 主题", keywords: ["theme", "zhuti", "dark", "light", "浅色", "深色"], run: toggleTheme },
            { id: "close", label: "关闭面板", keywords: ["close", "guanbi", "esc"], run: () => togglePalette(false) },
        ],
        [startScan, selectSafeOnly, selectAll, clearSelection, toggleTheme, togglePalette, setActivePage],
    );

    const filtered = useMemo(() => {
        const ql = q.toLowerCase().trim();
        if (!ql) return cmds;
        return cmds
            .map((c) => ({ c, s: scoreCmd(ql, c) }))
            .filter((x) => x.s >= 0)
            .sort((a, b) => b.s - a.s)
            .map((x) => x.c);
    }, [q, cmds]);
    const active = Math.min(idx, Math.max(filtered.length - 1, 0));

    // active 变化时把当前项滚进可视区（↑↓ 连按不脱靶）
    useEffect(() => {
        if (open) itemRefs.current[active]?.scrollIntoView({ block: "nearest" });
    }, [active, open, filtered.length]);

    /** 焦点陷阱：Tab / Shift+Tab 只在面板内循环 */
    function trapTab(e: React.KeyboardEvent) {
        if (e.key !== "Tab") return;
        const nodes = panelRef.current?.querySelectorAll<HTMLElement>("input, button");
        if (!nodes?.length) return;
        const list = [...nodes];
        const first = list[0];
        const last = list[list.length - 1];
        const cur = document.activeElement as HTMLElement | null;
        e.preventDefault();
        if (e.shiftKey) (cur === first || cur === null ? last : list[list.indexOf(cur as HTMLElement) - 1] ?? last).focus();
        else (cur === last || cur === null ? first : list[list.indexOf(cur as HTMLElement) + 1] ?? first).focus();
    }

    function runActive() {
        const cmd = filtered[active];
        if (!cmd) return;
        cmd.run();
        togglePalette(false);
    }

    return (
        <AnimatePresence>
            {open && (
                <motion.div
                    className="fixed inset-0 z-[60] flex items-start justify-center pt-[16vh]"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1, transition: { duration: 0.14 } }}
                    exit={{ opacity: 0, transition: { duration: 0.12 } }}
                    onClick={() => togglePalette(false)}
                    style={{ background: "rgba(4,6,12,0.45)", backdropFilter: "blur(6px)" }}
                >
                    <motion.div
                        ref={panelRef}
                        role="dialog"
                        aria-modal="true"
                        aria-label="命令面板"
                        onClick={(e) => e.stopPropagation()}
                        onKeyDown={(e) => {
                            if (e.key === "ArrowDown") { e.preventDefault(); setIdx((i) => Math.min(i + 1, filtered.length - 1)); }
                            if (e.key === "ArrowUp") { e.preventDefault(); setIdx((i) => Math.max(i - 1, 0)); }
                            if (e.key === "Enter") { e.preventDefault(); runActive(); }
                            trapTab(e);
                        }}
                        initial={{ y: -14, scale: 0.98, opacity: 0 }}
                        animate={{ y: 0, scale: 1, opacity: 1, transition: { type: "spring", stiffness: 380, damping: 30 } }}
                        exit={{ y: -10, scale: 0.985, opacity: 0, transition: { duration: 0.12 } }}
                        className="w-[min(520px,90vw)] overflow-hidden rounded-xl border"
                        style={{
                            background: "var(--zc-surface-1)",
                            borderColor: "var(--zc-border-strong)",
                            boxShadow: "var(--zc-shadow-pop)",
                        }}
                    >
                        <div className="flex items-center gap-2 border-b px-4 py-3" style={{ borderColor: "var(--zc-border)" }}>
                            <Search size={15} style={{ color: "var(--zc-text-3)" }} />
                            <input
                                ref={inputRef}
                                value={q}
                                onChange={(e) => { setQ(e.target.value); setIdx(0); }}
                                placeholder="输入命令… (↑↓ 选择 · Enter 执行)"
                                className="w-full bg-transparent text-sm outline-none"
                                style={{ color: "var(--zc-text-1)" }}
                            />
                        </div>
                        <ul className="max-h-[46vh] overflow-auto p-1.5">
                            {filtered.map((c, i) => (
                                <li key={c.id}>
                                    <button
                                        ref={(el) => { itemRefs.current[i] = el; }}
                                        onMouseEnter={() => setIdx(i)}
                                        onClick={() => { c.run(); togglePalette(false); }}
                                        className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm"
                                        style={{
                                            background: i === active ? "var(--zc-surface-2)" : "transparent",
                                            color: "var(--zc-text-1)",
                                        }}
                                    >
                                        {c.label}
                                        {c.hint && <span className="text-[10px]" style={{ color: "var(--zc-text-3)" }}>{c.hint}</span>}
                                        {i === active && (
                                            <CornerDownLeft size={13} style={{ color: "var(--zc-text-3)" }} />
                                        )}
                                    </button>
                                </li>
                            ))}
                            {filtered.length === 0 && (
                                <li className="px-3 py-6 text-center text-sm" style={{ color: "var(--zc-text-3)" }}>
                                    没有匹配的命令
                                </li>
                            )}
                        </ul>
                    </motion.div>
                </motion.div>
            )}
        </AnimatePresence>
    );
}
