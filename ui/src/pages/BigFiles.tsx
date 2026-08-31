import { useState } from "react";
import type { CSSProperties, KeyboardEvent } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Archive, CircleStop, Crosshair, ScanSearch, Search } from "lucide-react";
import * as ipc from "../lib/ipc";
import type { BigFile } from "../lib/ipc";
import { humanSize, thousand } from "../lib/format";
import { cascade, pageVariants } from "../lib/motion";
import { useStore } from "../store";
import { useArmKey } from "./useArmEsc";

/** 大文件页：Top-N 大文件扫描（内核单遍 jwalk + 小顶堆）；每行可安全移入暂存区（台账可还原）。
 *  v5：busy-cancel 取消通道、行级两段式确认（全站一致）、Enter 提交、搜索过滤、exit 动画。 */

const msgOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

const TOP_OPTIONS = [50, 100, 200];

export function BigFiles() {
    const toast = useStore((s) => s.toast);
    const setBusyRunning = useStore((s) => s.setBusyRunning);
    const cancelBusy = useStore((s) => s.cancelBusy);
    const desktop = ipc.isDesktop();

    const [path, setPath] = useState("");
    const [top, setTop] = useState(50);
    const [files, setFiles] = useState<BigFile[] | null>(null);
    const [scanning, setScanning] = useState(false);
    const [stashing, setStashing] = useState<string | null>(null);
    const [query, setQuery] = useState("");
    // 行级暂存删除两段式：armed 行显示「再点一次确认」，4s 超时/Esc 回退
    const { armKey: armPath, armKeyFor: armRow, disarmArm: disarmRow } = useArmKey(4000);

    /** 单文件安全删除：进暂存区，台账可还原；成功后本地移除该行 */
    async function stashOne(f: BigFile) {
        if (stashing) return;
        disarmRow();
        setStashing(f.path);
        try {
            await useStore.getState().manualDelete([f.path]);
            setFiles((list) => (list ? list.filter((x) => x.path !== f.path) : list));
        } finally {
            setStashing(null);
        }
    }

    async function run() {
        if (scanning) return;
        setScanning(true);
        setBusyRunning(true);
        try {
            const list = await ipc.bigFiles(path.trim(), top);
            setFiles(list);
        } catch (e) {
            if (ipc.errCode(e) === "cancelled") {
                toast("info", "已取消扫描");
            } else {
                toast("err", `扫描失败：${msgOf(e)}`);
            }
        } finally {
            setScanning(false);
            setBusyRunning(false);
        }
    }

    function onFormKey(e: KeyboardEvent) {
        if (e.key === "Enter") void run();
    }

    async function locate(p: string) {
        try {
            await ipc.revealInExplorer(p);
        } catch (e) {
            toast("err", msgOf(e));
        }
    }

    const shown = query.trim()
        ? (files ?? []).filter((f) => f.path.toLowerCase().includes(query.trim().toLowerCase()))
        : (files ?? []);
    const totalBytes = shown.reduce((a, f) => a + f.size, 0);

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">大文件</h1>
            <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                单遍遍历找出 ≥1MB 的文件 Top-N · 体积降序 · 只报告不动手
            </p>

            {/* ── 扫描表单 ── */}
            <div
                className="mt-4 flex flex-wrap items-end gap-3 rounded-xl border p-5"
                style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
            >
                <label className="min-w-0 flex-1 basis-64">
                    <span className="text-sm">扫描路径</span>
                    <span className="mt-0.5 block text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        留空 = 用户目录（%USERPROFILE%）· Enter 直接开扫
                    </span>
                    <input
                        value={path}
                        onChange={(e) => setPath(e.target.value)}
                        onKeyDown={onFormKey}
                        placeholder={String.raw`C:\Users\you\Downloads`}
                        spellCheck={false}
                        className="num mt-2 w-full rounded-lg border px-3 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                        style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                    />
                </label>
                <label>
                    <span className="text-sm">Top</span>
                    <span className="mt-0.5 block text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        返回条数
                    </span>
                    <select
                        value={top}
                        onChange={(e) => setTop(Number(e.target.value))}
                        onKeyDown={onFormKey}
                        className="num mt-2 w-24 rounded-lg border px-2.5 py-2 text-sm outline-none"
                        style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                    >
                        {TOP_OPTIONS.map((n) => (
                            <option key={n} value={n}>{n}</option>
                        ))}
                    </select>
                </label>
                <button
                    onClick={() => void run()}
                    disabled={scanning}
                    className="zc-sheen flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
                    style={{ background: "var(--zc-grad-brand)", color: "#ffffff", boxShadow: "var(--zc-glow-brand)" }}
                >
                    <ScanSearch size={14} /> {scanning ? "扫描中…" : "扫描"}
                </button>
                {scanning && (
                    <button
                        data-testid="busy-cancel"
                        onClick={() => void cancelBusy()}
                        className="zc-press flex items-center gap-1.5 rounded-lg border px-3.5 py-2 text-sm transition-colors hover:opacity-75"
                        style={{ borderColor: "var(--zc-danger)", color: "var(--zc-danger-text)" }}
                        title="终止本次遍历（后端 cancel_busy）"
                    >
                        <CircleStop size={14} /> 取消
                    </button>
                )}
            </div>

            {/* ── 骨架 / 引导 / 空态 / 列表 ── */}
            {scanning ? (
                <Skeleton />
            ) : files === null ? (
                <p className="mt-6 text-center text-xs" style={{ color: "var(--zc-text-3)" }}>
                    选择范围后点「扫描」，从最占地方的家伙开始清。
                </p>
            ) : files.length === 0 ? (
                <div
                    className="mt-5 flex flex-col items-center rounded-xl border px-6 py-10"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                >
                    <ScanSearch size={20} style={{ color: "var(--zc-text-3)" }} />
                    <p className="mt-2 text-sm" style={{ color: "var(--zc-text-2)" }}>
                        该范围未发现 ≥1MB 的文件
                    </p>
                    <p className="mt-1 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        换个更宽的范围试试
                    </p>
                </div>
            ) : (
                <>
                    {/* 前端过滤搜索框 */}
                    <div className="mt-4 flex items-center gap-2">
                        <div className="relative min-w-0 flex-1">
                            <Search size={13} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2" style={{ color: "var(--zc-text-3)" }} />
                            <input
                                value={query}
                                onChange={(e) => setQuery(e.target.value)}
                                placeholder="按路径过滤结果（前端过滤，不重扫）"
                                spellCheck={false}
                                className="num w-full rounded-lg border py-2 pl-8 pr-3 text-xs outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border)", color: "var(--zc-text-1)" }}
                            />
                        </div>
                        <p className="num shrink-0 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                            {query ? `筛出 ${thousand(shown.length)} / ${thousand(files.length)} · ` : ""}合计{" "}
                            <span style={{ color: "var(--zc-text-2)" }}>{humanSize(totalBytes)}</span>
                        </p>
                    </div>
                    <div className="mt-2 flex flex-col gap-1.5">
                        <AnimatePresence initial={false}>
                            {shown.map((f, i) => (
                                <motion.div
                                    key={f.path}
                                    layout
                                    variants={cascade(Math.min(i, 14))}
                                    initial="initial"
                                    animate="animate"
                                    exit={{ opacity: 0, x: 28, transition: { duration: 0.18 } }}
                                    className="flex items-center gap-3 rounded-lg border px-3 py-2"
                                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                                >
                                    <span
                                        className="num w-7 shrink-0 text-right text-[11px]"
                                        style={{ color: "var(--zc-text-3)" }}
                                    >
                                        {i + 1}
                                    </span>
                                    <span
                                        className="num w-24 shrink-0 text-right text-sm font-semibold"
                                        style={{ color: "var(--zc-accent-text)" }}
                                    >
                                        {humanSize(f.size)}
                                    </span>
                                    <span
                                        className="num min-w-0 flex-1 truncate text-xs"
                                        style={{ color: "var(--zc-text-1)" }}
                                        title={f.path}
                                    >
                                        {f.path}
                                    </span>
                                    {desktop && (
                                        <>
                                            <button
                                                onClick={() => void locate(f.path)}
                                                className="zc-press flex shrink-0 items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-colors hover:opacity-75"
                                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                                            >
                                                <Crosshair size={13} /> 定位
                                            </button>
                                            <button
                                                onClick={() => {
                                                    if (armPath === f.path) void stashOne(f);
                                                    else armRow(f.path);
                                                }}
                                                disabled={stashing === f.path}
                                                className="zc-press flex shrink-0 items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium transition-colors disabled:opacity-50"
                                                style={{
                                                    background: armPath === f.path
                                                        ? "color-mix(in srgb, var(--zc-danger) 14%, transparent)"
                                                        : "color-mix(in srgb, var(--zc-accent-b) 14%, transparent)",
                                                    color: armPath === f.path ? "var(--zc-danger-text)" : "var(--zc-accent-text)",
                                                }}
                                                title="移入暂存区，7 天内可在历史页还原"
                                            >
                                                <Archive size={13} />
                                                {stashing === f.path
                                                    ? "搬运中…"
                                                    : armPath === f.path
                                                        ? "再点一次确认"
                                                        : "暂存区"}
                                            </button>
                                        </>
                                    )}
                                </motion.div>
                            ))}
                        </AnimatePresence>
                        {shown.length === 0 && (
                            <p className="mt-4 text-center text-xs" style={{ color: "var(--zc-text-3)" }}>
                                没有匹配「{query.trim()}」的路径
                            </p>
                        )}
                    </div>
                </>
            )}
        </motion.div>
    );
}

/** 行骨架加载态：纯变量配色 + pulse */
function Skeleton() {
    const barStyle: CSSProperties = { background: "var(--zc-surface-3)", borderRadius: "var(--zc-r-sm)" };
    return (
        <div className="mt-5 flex flex-col gap-1.5">
            {[92, 78, 85, 64, 88, 72].map((w, i) => (
                <div
                    key={i}
                    className="flex h-11 zc-shimmer items-center rounded-lg border px-3"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                >
                    <div className="h-3.5" style={{ ...barStyle, width: `${w}%` }} />
                </div>
            ))}
        </div>
    );
}
