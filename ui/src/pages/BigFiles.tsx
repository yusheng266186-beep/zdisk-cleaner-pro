import { useState } from "react";
import type { CSSProperties } from "react";
import { motion } from "motion/react";
import { Crosshair, ScanSearch } from "lucide-react";
import * as ipc from "../lib/ipc";
import type { BigFile } from "../lib/ipc";
import { humanSize, thousand } from "../lib/format";
import { cascade, pageVariants } from "../lib/motion";
import { useStore } from "../store";

/** 大文件页：Top-N 大文件扫描（内核单遍 jwalk + 小顶堆），只报告，不动手。 */

const msgOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

const TOP_OPTIONS = [50, 100, 200];

export function BigFiles() {
    const toast = useStore((s) => s.toast);
    const desktop = ipc.isDesktop();

    const [path, setPath] = useState("");
    const [top, setTop] = useState(50);
    const [files, setFiles] = useState<BigFile[] | null>(null);
    const [scanning, setScanning] = useState(false);

    async function run() {
        if (scanning) return;
        setScanning(true);
        try {
            const list = await ipc.bigFiles(path.trim(), top);
            setFiles(list);
        } catch (e) {
            toast("err", `扫描失败：${msgOf(e)}`);
        } finally {
            setScanning(false);
        }
    }

    async function locate(p: string) {
        try {
            await ipc.revealInExplorer(p);
        } catch (e) {
            toast("err", msgOf(e));
        }
    }

    const totalBytes = (files ?? []).reduce((a, f) => a + f.size, 0);

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
                        留空 = 用户目录（%USERPROFILE%）
                    </span>
                    <input
                        value={path}
                        onChange={(e) => setPath(e.target.value)}
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
                    className="flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
                    style={{ background: "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))", color: "#ffffff" }}
                >
                    <ScanSearch size={14} /> {scanning ? "扫描中…" : "扫描"}
                </button>
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
                    <p className="mt-4 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        共 <span className="num">{thousand(files.length)}</span> 个文件 · 合计{" "}
                        <span className="num" style={{ color: "var(--zc-text-2)" }}>{humanSize(totalBytes)}</span>
                    </p>
                    <div className="mt-2 flex flex-col gap-1.5">
                        {files.map((f, i) => (
                            <motion.div
                                key={`${f.path}-${i}`}
                                variants={cascade(i)}
                                initial="initial"
                                animate="animate"
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
                                    style={{ color: "var(--zc-accent-b)" }}
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
                                    <button
                                        onClick={() => void locate(f.path)}
                                        className="flex shrink-0 items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-colors hover:opacity-75"
                                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                                    >
                                        <Crosshair size={13} /> 定位
                                    </button>
                                )}
                            </motion.div>
                        ))}
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
                    className="flex h-11 animate-pulse items-center rounded-lg border px-3"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                >
                    <div className="h-3.5" style={{ ...barStyle, width: `${w}%` }} />
                </div>
            ))}
        </div>
    );
}
