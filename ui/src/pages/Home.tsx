import { useCallback, useEffect, useState } from "react";
import { HardDrive, Sparkles, ShieldCheck, CircleStop, Trash2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Ring } from "../components/Ring";
import { RollNumber } from "../components/RollNumber";
import { useStore } from "../store";
import { thousand, humanSize } from "../lib/format";
import { pageVariants, springSnappy } from "../lib/motion";
import { driveRootPath, emptyRecycleBin, errCode, errMsg, queryRecycleBin } from "../lib/ipc";
import type { RecycleBinInfo } from "../lib/types";
import { useArm } from "./useArmEsc";

export function Home() {
    const phase = useStore((s) => s.phase);
    const drives = useStore((s) => s.drives);
    const rules = useStore((s) => s.rules);
    const scanFiles = useStore((s) => s.scanFiles);
    const startScan = useStore((s) => s.startScan);
    const cancelScan = useStore((s) => s.cancelScan);
    const cleanOutcome = useStore((s) => s.cleanOutcome);
    const undoLast = useStore((s) => s.undoLast);
    const demo = useStore((s) => s.demo);
    const homeAdmin = useStore((s) => s.homeAdmin);
    const setHomeAdmin = useStore((s) => s.setHomeAdmin);
    const setActivePage = useStore((s) => s.setActivePage);
    const setRadarRoot = useStore((s) => s.setRadarRoot);

    const scanning = phase === "scanning";

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            {/* 上次清理战报横幅（v5：已持久化，刷新不丢「反悔」入口） */}
            <AnimatePresence>
                {cleanOutcome && !scanning && (
                    <motion.div
                        initial={{ opacity: 0, y: -14 }}
                        animate={{ opacity: 1, y: 0, transition: springSnappy }}
                        exit={{ opacity: 0, y: -14, transition: { duration: 0.18 } }}
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
                            className="zc-press rounded-lg border px-3 py-1 text-xs transition-colors hover:opacity-80"
                            style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                        >
                            反悔 · 一键还原本批
                        </button>
                    </motion.div>
                )}
            </AnimatePresence>

            <div className="flex flex-col items-center py-8">
                {/* 主环 / 扫描不定态环：v5 删除 160MB 假分母，扫描中不声称任何百分比 */}
                {scanning ? (
                    <ScanRing files={scanFiles} />
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
                        ? "枚举即报数，不预估、不装进度。随时可以取消。"
                        : `${rules.length > 0 ? rules.length : "…"} 条内置规则覆盖系统 / 浏览器 / 开发 / 应用缓存。删除默认进回收站或暂存区（vault），笔笔可恢复。`}
                </p>

                {!scanning && (
                    <>
                        <motion.button
                            onClick={() => void startScan()}
                            whileHover={{ y: -2 }}
                            whileTap={{ scale: 0.97 }}
                            transition={springSnappy}
                            className="zc-sheen mt-7 flex items-center gap-2 rounded-full px-9 py-3.5 text-base font-medium text-white"
                            style={{
                                background: "var(--zc-grad-brand)",
                                boxShadow: "var(--zc-glow-brand)",
                            }}
                        >
                            <Sparkles size={18} />
                            开始智能体检
                        </motion.button>

                        {/* v5：系统级项目开关 → startScan(includeAdmin) */}
                        <label className="mt-4 flex cursor-pointer items-center gap-2 text-xs" style={{ color: "var(--zc-text-2)" }}>
                            <input
                                type="checkbox"
                                data-testid="admin-toggle"
                                checked={homeAdmin}
                                onChange={(e) => setHomeAdmin(e.target.checked)}
                                className="h-3.5 w-3.5 accent-[var(--zc-accent-b)]"
                            />
                            包含系统级项目（需管理员）
                        </label>
                        <p className="mt-1 text-center text-[10px]" style={{ color: "var(--zc-text-3)" }}>
                            勾选后扫描纳入系统 Temp / Windows Update 残留等 admin 规则；仅在以管理员身份运行时才会纳入清理
                        </p>
                    </>
                )}

                {scanning && (
                    <button
                        onClick={cancelScan}
                        className="zc-press mt-6 flex items-center gap-2 rounded-full border px-5 py-2 text-sm transition-colors hover:opacity-75"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                    >
                        <CircleStop size={15} /> 取消扫描
                    </button>
                )}

                {/* 回收站卡（v5 主线二：一键清空系统回收站） */}
                <RecycleBinCard />

                {/* 磁盘小环列表：点击 = 以该盘为根进入空间雷达 */}
                {!scanning && (
                    <div className="mt-10 flex gap-6">
                        {drives.map((d) => {
                            const used = 1 - d.free_bytes / d.total_bytes;
                            return (
                                <button
                                    key={d.label}
                                    onClick={() => {
                                        setRadarRoot(driveRootPath(d.label));
                                        setActivePage("radar");
                                    }}
                                    title={`在空间雷达分析 ${d.label} 盘占用`}
                                    className="zc-press flex flex-col items-center gap-2 rounded-xl px-3 py-2 transition-colors hover:bg-[var(--zc-hover)]"
                                >
                                    <Ring size={84} stroke={7} pct={used} color={used > 0.9 ? "var(--zc-danger)" : "var(--zc-accent-b)"}>
                                        <span className="num text-sm font-medium">{d.label}</span>
                                    </Ring>
                                    <span className="text-xs" style={{ color: "var(--zc-text-3)" }}>
                                        已用 {(used * 100).toFixed(0)}%
                                    </span>
                                </button>
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

/** v5 U4 修复：扫描环不定态 —— 旋转描边 + 真实计数跳动，不声称具体百分比。 */
function ScanRing({ files }: { files: number }) {
    const size = 220;
    const stroke = 14;
    const r = (size - stroke) / 2;
    const c = 2 * Math.PI * r;
    const scanBytes = useStore((s) => s.scanBytes);
    return (
        <div className="relative grid place-items-center" style={{ width: size, height: size }}>
            <svg width={size} height={size} className="absolute inset-0">
                <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--zc-surface-3)" strokeWidth={stroke} />
            </svg>
            <motion.div
                className="absolute inset-0"
                animate={{ rotate: 360 }}
                transition={{ repeat: Infinity, duration: 2.6, ease: "linear" }}
            >
                <svg width={size} height={size}>
                    <defs>
                        <linearGradient id="zc-scan-grad" x1="0%" y1="0%" x2="100%" y2="100%">
                            <stop offset="0%" stopColor="var(--zc-accent-a)" />
                            <stop offset="100%" stopColor="var(--zc-accent-b)" />
                        </linearGradient>
                    </defs>
                    <circle
                        cx={size / 2} cy={size / 2} r={r} fill="none"
                        stroke="url(#zc-scan-grad)" strokeWidth={stroke} strokeLinecap="round"
                        strokeDasharray={`${c * 0.24} ${c * 0.76}`}
                        style={{ filter: "drop-shadow(0 0 6px color-mix(in srgb, var(--zc-accent-b) 35%, transparent))" }}
                    />
                </svg>
            </motion.div>
            <div className="relative text-center">
                <div className="text-3xl font-semibold"><RollNumber value={files} mode="plain" /></div>
                <div className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>已扫描文件</div>
                <div className="num mt-0.5 text-sm" style={{ color: "var(--zc-text-2)" }}>
                    <RollNumber value={scanBytes} />
                </div>
            </div>
        </div>
    );
}

/** 回收站卡：query 显示 items/bytes；>0 时两段式清空；admin 错误走提权引导。 */
function RecycleBinCard() {
    const toast = useStore((s) => s.toast);
    const refreshDrives = useStore((s) => s.refreshDrives);
    const { armed, arm, disarm } = useArm(4000);
    const [info, setInfo] = useState<RecycleBinInfo | null>(null);
    const [busy, setBusy] = useState(false);

    const query = useCallback(async () => {
        try {
            setInfo(await queryRecycleBin());
        } catch (e) {
            toast("err", `回收站读数失败：${errMsg(e)}`);
        }
    }, [toast]);

    useEffect(() => {
        void query();
    }, [query]);

    async function emptyNow() {
        if (busy) return;
        disarm();
        setBusy(true);
        try {
            const s = await emptyRecycleBin();
            toast("ok", `已清空回收站 · ${s.items_before} 项 · 释放 ${humanSize(s.bytes_freed)}`);
            await query();
            void refreshDrives();
        } catch (e) {
            if (errCode(e) === "admin_required") {
                toast("warn", "清空回收站需要管理员权限：请以管理员身份重启应用后再试");
            } else {
                toast("err", `清空回收站失败：${errMsg(e)}`);
            }
        } finally {
            setBusy(false);
        }
    }

    const hasItems = (info?.items ?? 0) > 0;
    return (
        <div
            data-card="recyclebin"
            className="mt-10 flex w-full max-w-md items-center gap-3 rounded-xl border px-4 py-3.5"
            style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
        >
            <Trash2 size={16} style={{ color: hasItems ? "var(--zc-warn)" : "var(--zc-text-3)" }} />
            <div className="min-w-0 flex-1">
                <div className="text-sm font-medium">系统回收站</div>
                <div className="num mt-0.5 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                    {busy
                        ? "正在清空…"
                        : info === null
                          ? "正在查询…"
                          : hasItems
                            ? `${thousand(info.items)} 项 · ${humanSize(info.bytes)}`
                            : "空的，无需打理"}
                </div>
            </div>
            {hasItems && !busy && (
                <button
                    onClick={() => (armed ? void emptyNow() : arm())}
                    className="zc-press shrink-0 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors"
                    style={{
                        borderColor: armed ? "var(--zc-danger)" : "var(--zc-border-strong)",
                        color: armed ? "var(--zc-danger-text)" : "var(--zc-text-1)",
                        background: armed ? "color-mix(in srgb, var(--zc-danger) 12%, transparent)" : "transparent",
                    }}
                    title="清空后不经过任何后悔期，立即物理删除"
                >
                    {armed ? "确认清空，不可还原" : "清空回收站"}
                </button>
            )}
        </div>
    );
}

function pctOfLargest(drives: { total_bytes: number; free_bytes: number }[]): number {
    if (!drives.length) return 0;
    const d = [...drives].sort((a, b) => b.total_bytes - a.total_bytes)[0];
    return 1 - d.free_bytes / d.total_bytes;
}

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
