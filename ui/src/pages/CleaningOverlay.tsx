import { AnimatePresence, motion } from "motion/react";
import { useStore } from "../store";

/** 执行中浮层：流光不确定进度 + 规则名轮换。
 *  结果由 store 落定后自动退场，完成战报转场到 Home 横幅。 */
export function CleaningOverlay() {
    const phase = useStore((s) => s.phase);
    const selectionSize = useStore((s) => s.selection.size);
    const report = useStore((s) => s.report);
    const rules = useStore((s) => s.rules);

    // 选中规则名轮换展示
    const names = (report?.findings ?? [])
        .filter((f) => useStore.getState().selection.has(f.rule_id))
        .map((f) => rules.find((r) => r.id === f.rule_id)?.name_zh ?? f.rule_id);
    let idx = Math.floor((Date.now() / 260) % Math.max(names.length, 1));

    return (
        <AnimatePresence>
            {phase === "cleaning" && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1, transition: { duration: 0.18 } }}
                    exit={{ opacity: 0, transition: { duration: 0.25 } }}
                    className="fixed inset-0 z-[55] grid place-items-center"
                    style={{ background: "color-mix(in srgb, var(--zc-bg) 72%, transparent)", backdropFilter: "blur(10px)" }}
                    key={String(idx)}
                >
                    <motion.div
                        initial={{ scale: 0.94, y: 16 }}
                        animate={{ scale: 1, y: 0, transition: { type: "spring", stiffness: 300, damping: 28 } }}
                        className="w-[min(440px,88vw)] rounded-2xl border p-7 text-center"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border-strong)", boxShadow: "var(--zc-shadow-pop)" }}
                    >
                        <motion.div
                            animate={{ rotate: 360 }}
                            transition={{ repeat: Infinity, duration: 2.4, ease: "linear" }}
                            className="mx-auto h-10 w-10 rounded-full border-[3px] border-transparent"
                            style={{
                                borderTopColor: "var(--zc-accent-a)",
                                borderRightColor: "var(--zc-accent-b)",
                            }}
                        />
                        <div className="mt-5 text-base font-medium">正在安全搬运…</div>
                        <div className="num mt-1 h-5 text-xs" style={{ color: "var(--zc-text-3)" }}>
                            {names.length ? names[idx % names.length] : ""}
                        </div>

                        {/* 流光条 */}
                        <div className="relative mt-6 h-1.5 overflow-hidden rounded-full" style={{ background: "var(--zc-surface-3)" }}>
                            <motion.div
                                className="absolute inset-y-0 w-2/5 rounded-full"
                                style={{
                                    background: "linear-gradient(90deg, transparent, var(--zc-accent-b), transparent)",
                                }}
                                animate={{ x: ["-120%", "320%"] }}
                                transition={{ repeat: Infinity, duration: 1.15, ease: "easeInOut" }}
                            />
                        </div>

                        <p className="mt-4 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                            守卫 fail-closed 校验通过后才动手 · 目标 {selectionSize || "—"} 条规则
                        </p>
                    </motion.div>
                </motion.div>
            )}
        </AnimatePresence>
    );
}
