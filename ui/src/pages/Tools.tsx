import { useState } from "react";
import {
    Archive,
    FileClock,
    FileSearch,
    FolderOutput,
    ListTree,
    Rocket,
    Settings2,
    ShieldCheck,
    Sparkle,
} from "lucide-react";
import { motion } from "motion/react";
import { useStore, type Page } from "../store";
import { cascade, pageVariants } from "../lib/motion";

const TOOLS: { icon: typeof FolderOutput; name: string; desc: string; go: Page }[] = [
    { icon: Sparkle, name: "体检台", desc: "60 条规则体检，安全档一键清理，笔笔可还原", go: "home" },
    { icon: FolderOutput, name: "存储迁移中心", desc: "npm/pip/Gradle/微信等目录跨盘搬迁，junction 回滚保障", go: "migrate" },
    { icon: ListTree, name: "空间雷达", desc: "Treemap 可视化，下钻定位，选中即安全删除", go: "radar" },
    { icon: FileSearch, name: "大文件", desc: "≥1MB Top-N 排行，逐个定位或移入暂存区", go: "bigfiles" },
    { icon: FileClock, name: "重复文件猎手", desc: "XXH3 三级管道，一键清理组内冗余份数", go: "dupes" },
    { icon: Rocket, name: "启动项管家", desc: "Run 键枚举 / 禁用备份还原", go: "startup" },
    { icon: Settings2, name: "深度工具", desc: "WinSxS 组件清理（DISM 真实进度）/ 还原点 / 系统级占用", go: "deeptools" },
    { icon: Sparkle, name: "命令面板", desc: "Ctrl+K 全局跳转，手不离键盘", go: "home" },
];

export function Tools() {
    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">工具箱</h1>
            <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                所有工具即点即达；下方「安全删除」对任意路径生效，走守卫 + 暂存区 + 台账。
            </p>

            <div className="mt-5 grid grid-cols-1 gap-3 sm:grid-cols-2">
                {TOOLS.map((t, i) => (
                    <motion.div
                        key={t.name}
                        variants={cascade(i)}
                        initial="initial"
                        animate="animate"
                        whileHover={{ y: -3 }}
                        transition={{ type: "spring", stiffness: 380, damping: 26 }}
                        className="cursor-pointer rounded-xl border p-5"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                        onClick={() => useStore.getState().setActivePage(t.go)}
                    >
                        <div className="flex items-center gap-2">
                            <t.icon size={17} style={{ color: "var(--zc-accent-b)" }} />
                            <span className="font-medium">{t.name}</span>
                        </div>
                        <p className="mt-2 text-xs leading-relaxed" style={{ color: "var(--zc-text-2)" }}>{t.desc}</p>
                    </motion.div>
                ))}
            </div>

            <SafeDeleteCard />
        </motion.div>
    );
}

/** 安全删除：输入任意路径 → 守卫校验 → 移入暂存区（台账可还原）。
 *  大文件/重复文件/雷达页的手动删除都走同一条 manualDelete 链路。 */
function SafeDeleteCard() {
    const [path, setPath] = useState("");
    const [armed, setArmed] = useState(false);
    const [busy, setBusy] = useState(false);

    async function run() {
        const p = path.trim().replaceAll("/", "\\");
        if (!p || busy) return;
        setBusy(true);
        try {
            await useStore.getState().manualDelete([p]);
            setPath("");
            setArmed(false);
        } finally {
            setBusy(false);
        }
    }

    return (
        <motion.div
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            className="mt-6 rounded-xl border p-5"
            style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
        >
            <div className="flex items-center gap-2">
                <ShieldCheck size={17} style={{ color: "var(--zc-ok)" }} />
                <span className="font-medium">安全删除</span>
                <span className="rounded-full px-2 py-0.5 text-[10px]" style={{ background: "var(--zc-surface-3)", color: "var(--zc-text-3)" }}>
                    守卫 fail-closed · 可还原
                </span>
            </div>
            <p className="mt-2 text-xs leading-relaxed" style={{ color: "var(--zc-text-2)" }}>
                粘贴任意文件/目录路径，移入暂存区而非直接删除；7 天内可在「历史」页一键还原。
                系统关键路径会被守卫直接拒绝。
            </p>
            <div className="mt-3 flex items-center gap-2">
                <input
                    value={path}
                    onChange={(e) => { setPath(e.target.value); setArmed(false); }}
                    placeholder={String.raw`C:\Users\you\某目录 或 D:\大文件.iso`}
                    spellCheck={false}
                    className="num min-w-0 flex-1 rounded-lg border px-3 py-2 text-xs outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                    style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                />
                <button
                    onClick={() => { if (armed) { void run(); } else { setArmed(true); setTimeout(() => setArmed(false), 4000); } }}
                    disabled={!path.trim() || busy}
                    className="flex shrink-0 items-center gap-1.5 rounded-lg px-4 py-2 text-xs font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
                    style={{
                        background: armed ? "color-mix(in srgb, var(--zc-danger) 20%, transparent)" : "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))",
                        color: armed ? "var(--zc-danger)" : "#ffffff",
                    }}
                >
                    <Archive size={13} />
                    {busy ? "搬运中…" : armed ? "再点一次确认" : "移入暂存区"}
                </button>
            </div>
        </motion.div>
    );
}
