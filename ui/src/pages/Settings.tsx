import { Moon, Sun, Database } from "lucide-react";
import { pageVariants, springSnappy } from "../lib/motion";
import { motion } from "motion/react";
import { useStore } from "../store";

export function Settings() {
    const theme = useStore((s) => s.theme);
    const toggleTheme = useStore((s) => s.toggleTheme);
    const version = useStore((s) => s.version);

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">设置</h1>

            <Section title="外观">
                <Row
                    title="主题"
                    sub="跟随你的审美，本地持久化"
                >
                    <button
                        onClick={toggleTheme}
                        className="flex items-center gap-2 rounded-full border px-4 py-1.5 text-sm transition-transform active:scale-95"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                    >
                        <motion.span key={theme} initial={{ rotate: -90, opacity: 0 }} animate={{ rotate: 0, opacity: 1 }} transition={springSnappy} className="flex items-center gap-1.5">
                            {theme === "dark" ? <Moon size={14} /> : <Sun size={14} />}
                            {theme === "dark" ? "深空" : "浅色"}
                        </motion.span>
                    </button>
                </Row>
            </Section>

            <Section title="策略（默认值即安全值）">
                <Row title="默认仅勾选安全档" sub="注意及以上档位需逐条展开二次确认（UI 强制）" locked />
                <Row title="vault 暂存期 7 天" sub="到期物理清除前会再次提示；期间随时一键全量还原" locked />
                <Row title="执行端 fail-closed 守卫" sub="任何路径解析失败都拒绝整批操作——宁可少删，不可误删" locked />
            </Section>

            <Section title="数据位置">
                <Row title="数据目录" sub="%LOCALAPPDATA%\\ZDiskCleanerPro3（台账 / 历史 / 会话报告）；测试可用 ZC_DATA_DIR 重定向" locked />
                <Row title="内核版本" sub={`zc-core ${version || "…"}`} locked />
            </Section>

            <p className="mt-8 flex items-center gap-1.5 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                <Database size={12} /> 数据永远只留在本机。没有云端，没有遥测。
            </p>
        </motion.div>
    );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
    return (
        <section className="mt-6">
            <h2 className="mb-2 text-sm font-medium" style={{ color: "var(--zc-text-2)" }}>{title}</h2>
            <div className="overflow-hidden rounded-xl border" style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}>
                {children}
            </div>
        </section>
    );
}

function Row({ title, sub, children, locked }: { title: string; sub?: string; children?: React.ReactNode; locked?: boolean }) {
    return (
        <div className="flex items-center justify-between gap-4 border-b px-4 py-3 last:border-b-0" style={{ borderColor: "var(--zc-border)" }}>
            <div className="min-w-0">
                <div className="text-sm">{title}</div>
                {sub && <div className="mt-0.5 text-xs leading-relaxed" style={{ color: "var(--zc-text-3)" }}>{sub}</div>}
            </div>
            {locked ? (
                <span className="shrink-0 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] text-emerald-400">默认安全</span>
            ) : (
                children
            )}
        </div>
    );
}
