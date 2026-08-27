import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useStore } from "../store";
import { Search, CornerDownLeft } from "lucide-react";

interface Cmd {
    id: string;
    label: string;
    hint?: string;
    run: () => void;
}

/** Ctrl+K 命令面板：毛玻璃浮层 + 键盘导航。
 *  动画只用 transform/opacity，保证合成器路径。 */
export function CommandPalette() {
    const open = useStore((s) => s.paletteOpen);
    const togglePalette = useStore((s) => s.togglePalette);
    const startScan = useStore((s) => s.startScan);
    const selectSafeOnly = useStore((s) => s.selectSafeOnly);
    const clearSelection = useStore((s) => s.clearSelection);
    const toggleTheme = useStore((s) => s.toggleTheme);

    const [q, setQ] = useState("");
    const [idx, setIdx] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
                e.preventDefault();
                togglePalette();
            }
            if (e.key === "Escape") togglePalette(false);
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [togglePalette]);

    useEffect(() => {
        if (open) {
            setQ("");
            setIdx(0);
            setTimeout(() => inputRef.current?.focus(), 30);
        }
    }, [open]);

    const cmds: Cmd[] = useMemo(
        () => [
            { id: "scan", label: "开始磁盘体检", hint: "Home", run: () => void startScan() },
            { id: "safe", label: "只勾选安全规则", run: selectSafeOnly },
            { id: "clear", label: "清空勾选", run: clearSelection },
            { id: "theme", label: "切换 明/暗 主题", run: toggleTheme },
            {
                id: "close",
                label: "关闭面板",
                run: () => togglePalette(false),
            },
        ],
        [startScan, selectSafeOnly, clearSelection, toggleTheme, togglePalette],
    );

    const filtered = cmds.filter((c) => c.label.toLowerCase().includes(q.toLowerCase()));
    const active = Math.min(idx, Math.max(filtered.length - 1, 0));

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
                        onClick={(e) => e.stopPropagation()}
                        onKeyDown={(e) => {
                            if (e.key === "ArrowDown") { e.preventDefault(); setIdx((i) => i + 1); }
                            if (e.key === "ArrowUp") { e.preventDefault(); setIdx((i) => Math.max(i - 1, 0)); }
                            if (e.key === "Enter") { filtered[active]?.run(); togglePalette(false); }
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
                                        onMouseEnter={() => setIdx(i)}
                                        onClick={() => { c.run(); togglePalette(false); }}
                                        className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm"
                                        style={{
                                            background: i === active ? "var(--zc-surface-2)" : "transparent",
                                            color: "var(--zc-text-1)",
                                        }}
                                    >
                                        {c.label}
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
