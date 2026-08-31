import { useEffect, useState } from "react";
import { HardDrive, Sparkles, ShieldCheck, CircleStop } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Ring } from "../components/Ring";
import { RollNumber } from "../components/RollNumber";
import { cleanableText, useStore } from "../store";
import { thousand } from "../lib/format";
import { pageVariants, springSnappy } from "../lib/motion";

export function Home() {
    const phase = useStore((s) => s.phase);
    const drives = useStore((s) => s.drives);
    const scanFiles = useStore((s) => s.scanFiles);
    const scanBytes = useStore((s) => s.scanBytes);
    const startScan = useStore((s) => s.startScan);
    const cancelScan = useStore((s) => s.cancelScan);
    const cleanOutcome = useStore((s) => s.cleanOutcome);
    const undoLast = useStore((s) => s.undoLast);
    const demo = useStore((s) => s.demo);

    const scanning = phase === "scanning";

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            {/* 上次清理战报横幅 */}
            <AnimatePresence>
                {cleanOutcome && !scanning && (
                    <motion.div
                        initial={{ opacity: 0, y: -14 }}
                        animate={{ opacity: 1, y: 0, transition: springSnappy }}
                        className="relative mb-6 flex items-center justify-between overflow-hidden rounded-xl border px-4 py-3"
                        style={{
                            background: "color-mix(in srgb, var(--zc-ok) 10%, var(--zc-surface-1))",
                            borderColor: "color-mix(in srgb, var(--zc-ok) 30%, transparent)",
                            boxShadow: "var(--zc-shadow-1)",
                        }}
                    >
                        <span className="absolute inset-y-0 left-0 w-[3px]" style={{ background: "var(--zc-ok)" }} />
                        <div className="flex items-center gap-2 text-sm">
                            <ShieldCheck size={16} style={{ color: "var(--zc-ok)" }} />
                            <span>
                                本次已搬运 <b>{cleanOutcome.done_files}</b> 项（整目录计 1 项） ·{" "}
                                <RollNumber value={cleanOutcome.done_bytes} />
                            </span>
                        </div>
                        <button
                            onClick={() => void undoLast()}
                            className="rounded-lg border px-3 py-1 text-xs transition-colors hover:opacity-80"
                            style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                        >
                            反悔 · 一键还原本批
                        </button>
                    </motion.div>
                )}
            </AnimatePresence>

            <div className="flex flex-col items-center py-8">
                {/* 主环 / 扫描脉冲环 */}
                {scanning ? (
                    <motion.div animate={{ scale: [1, 1.03, 1] }} transition={{ repeat: Infinity, duration: 1.8 }}>
                        <Ring size={220} stroke={14} pct={Math.min(scanBytes / (160 * 1024 ** 2), 1)}>
                            <div className="text-center">
                                <div className="text-3xl font-semibold"><RollNumber value={scanFiles} mode="plain" /></div>
                                <div className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>已扫描文件</div>
                                <div className="num mt-0.5 text-sm" style={{ color: "var(--zc-text-2)" }}>
                                    <RollNumber value={scanBytes} />
                                </div>
                            </div>
                        </Ring>
                    </motion.div>
                ) : (
                    <Ring size={220} stroke={14} pct={pctOfLargest(drives)} color="var(--zc-accent-a)">
                        <HardDrive size={44} strokeWidth={1.2} style={{ color: "var(--zc-text-2)" }} />
                    </Ring>
                )}

                <h1 className="mt-7 text-2xl font-semibold">
                    {scanning ? "正在体检…" : "磁盘体检，一键开始"}
                    {scanning && <ScanRate />}
                </h1>
                <p className="mt-2 max-w-md text-center text-sm leading-relaxed" style={{ color: "var(--zc-text-2)" }}>
                    {scanning
                        ? "枚举即报数，绝不假进度。随时可以取消。"
                        : "60 条内置规则覆盖系统 / 浏览器 / 开发 / 应用缓存。删除默认进回收站或暂存区，笔笔可恢复。"}
                </p>

                {!scanning && (
                    <motion.button
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
                    >
                        <Sparkles size={18} />
                        开始智能体检
                    </motion.button>
                )}

                {scanning && (
                    <button
                        onClick={cancelScan}
                        className="mt-6 flex items-center gap-2 rounded-full border px-5 py-2 text-sm transition-colors hover:opacity-75"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                    >
                        <CircleStop size={15} /> 取消扫描
                    </button>
                )}

                {/* 磁盘小环列表 */}
                {!scanning && (
                    <div className="mt-12 flex gap-8">
                        {drives.map((d) => {
                            const used = 1 - d.free_bytes / d.total_bytes;
                            return (
                                <div key={d.label} className="flex flex-col items-center gap-2">
                                    <Ring size={84} stroke={7} pct={used} color={used > 0.9 ? "var(--zc-danger)" : "var(--zc-accent-b)"}>
                                        <span className="num text-sm font-medium">{d.label}</span>
                                    </Ring>
                                    <span className="text-xs" style={{ color: "var(--zc-text-3)" }}>
                                        已用 {(used * 100).toFixed(0)}%
                                    </span>
                                </div>
                            );
                        })}
                    </div>
                )}

                {demo && (
                    <div
                        className="fixed bottom-4 left-4 rounded-full border px-3 py-1 text-[11px]"
                        style={{ borderColor: "var(--zc-warn)", color: "var(--zc-warn)" }}
                    >
                        DEMO 数据（真机采样）· 壳层接入后自动转为真实 IPC
                    </div>
                )}
            </div>
        </motion.div>
    );
}

function pctOfLargest(drives: { total_bytes: number; free_bytes: number }[]): number {
    if (!drives.length) return 0;
    const d = [...drives].sort((a, b) => b.total_bytes - a.total_bytes)[0];
    return 1 - d.free_bytes / d.total_bytes;
}

// 引用避免未用告警（真实文本在 toast 中使用）
void cleanableText;


/** 扫描速率条：已用时与实时 项/s（前端自计时，内核只报累计值）。 */
function ScanRate() {
    const [start] = useState(() => Date.now());
    const [, force] = useState(0);
    useEffect(() => {
        const t = setInterval(() => force((x) => x + 1), 500);
        return () => clearInterval(t);
    }, []);
    const files = useStore((s) => s.scanFiles);
    const secs = Math.max((Date.now() - start) / 1000, 0.1);
    const rate = files / secs;
    return (
        <p className="mt-2 text-center text-[11px] num" style={{ color: "var(--zc-text-3)" }}>
            已用时 {secs.toFixed(1)}s · {thousand(Math.round(rate))} 项/s · 可随时取消
        </p>
    );
}
