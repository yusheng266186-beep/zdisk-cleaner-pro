import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useStore } from "../store";
import { fastFade, overlayIn, overlayOut, springCard } from "../lib/motion";

/** 执行中浮层：流光不确定进度 + 规则名轮换 + 秒级已用时计时。
 *  v5 U4 修复：
 *  - 旧版把 key={String(idx)} 挂在整张 fixed 遮罩上，规则名每 1.4s 轮换 → 遮罩整体
 *    remount（淡出淡入重播 + spinner 从 0° 重启）。现在根遮罩全程只挂载一次，
 *    轮换 key 只挂在内层标签文字行上（局部 AnimatePresence）；
 *  - 文案诚实化：fixed inset-0 z-[55] 期间应用确实锁死，删掉「可切走」谎言；
 *  - 补秒级计时器缓解「无进度」等待焦虑（纯文本 1Hz 更新，无布局动画）；
 *  - 出入场收编 overlayIn/overlayOut，内卡弹簧收编 springCard。 */
export function CleaningOverlay() {
    const phase = useStore((s) => s.phase);
    const selection = useStore((s) => s.selection);
    const selectionSize = selection.size;
    const report = useStore((s) => s.report);
    const rules = useStore((s) => s.rules);

    // 选中规则名轮换展示
    const names = useMemo(
        () =>
            (report?.findings ?? [])
                .filter((f) => selection.has(f.rule_id))
                .map((f) => rules.find((r) => r.id === f.rule_id)?.name_zh ?? f.rule_id),
        [report, rules, selection],
    );
    const nameCount = names.length;

    // 标签轮换：只重挂文字行；计时器：每秒一次的纯文本 tick
    const [idx, setIdx] = useState(0);
    const [elapsed, setElapsed] = useState(0);
    useEffect(() => {
        if (phase !== "cleaning") {
            setIdx(0);
            setElapsed(0);
            return;
        }
        const rot =
            nameCount > 1 ? setInterval(() => setIdx((i) => i + 1), 1400) : null;
        const sec = setInterval(() => setElapsed((e) => e + 1), 1000);
        return () => {
            if (rot) clearInterval(rot);
            clearInterval(sec);
        };
    }, [phase, nameCount]);

    return (
        <AnimatePresence>
            {phase === "cleaning" && (
                <motion.div
                    key="cleaning-overlay"
                    data-testid="cleaning-overlay"
                    initial={{ opacity: 0 }}
                    animate={overlayIn}
                    exit={overlayOut}
                    className="fixed inset-0 z-[55] grid place-items-center"
                    style={{ background: "color-mix(in srgb, var(--zc-bg) 72%, transparent)", backdropFilter: "blur(10px)" }}
                >
                    <motion.div
                        initial={{ scale: 0.94, y: 16 }}
                        animate={{ scale: 1, y: 0, transition: springCard }}
                        className="relative w-[min(440px,88vw)] overflow-hidden rounded-2xl border p-7 pt-8 text-center"
                        style={{
                            background: "color-mix(in srgb, var(--zc-surface-1) 76%, transparent)",
                            borderColor: "var(--zc-border-strong)",
                            boxShadow: "var(--zc-shadow-pop)",
                            backdropFilter: "blur(14px)",
                        }}
                    >
                        <div className="absolute inset-x-0 top-0 h-px" style={{ background: "var(--zc-hairline)" }} />
                        <motion.div
                            animate={{ rotate: 360 }}
                            transition={{ repeat: Infinity, duration: 2.4, ease: "linear" }}
                            className="mx-auto h-10 w-10 rounded-full border-[3px] border-transparent"
                            style={{
                                borderTopColor: "var(--zc-accent-a)",
                                borderRightColor: "var(--zc-accent-b)",
                            }}
                        />
                        <div className="mt-5 text-base font-medium">正在安全搬运…（请勿关闭窗口）</div>
                        {/* 轮换只重挂这一行文字，根遮罩与 spinner 不动 */}
                        <div className="num mt-1 flex h-5 items-center justify-center overflow-hidden text-xs" style={{ color: "var(--zc-text-3)" }}>
                            {nameCount > 0 && (
                                <AnimatePresence mode="wait">
                                    <motion.span
                                        key={idx}
                                        initial={{ opacity: 0, y: 5 }}
                                        animate={{ opacity: 1, y: 0 }}
                                        exit={{ opacity: 0, y: -5 }}
                                        transition={fastFade}
                                        className="inline-block max-w-full truncate"
                                    >
                                        {names[idx % nameCount]}
                                    </motion.span>
                                </AnimatePresence>
                            )}
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
                            守卫 fail-closed 校验通过后才动手 · 目标 {selectionSize || "—"} 条规则 ·{" "}
                            <span className="num">已用 {elapsed}s</span>
                        </p>
                    </motion.div>
                </motion.div>
            )}
        </AnimatePresence>
    );
}
