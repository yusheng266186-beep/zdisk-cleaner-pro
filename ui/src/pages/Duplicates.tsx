import { useState } from "react";
import type { CSSProperties } from "react";
import { motion } from "motion/react";
import { Archive, Copy, Crosshair, Fingerprint  } from "lucide-react";
import * as ipc from "../lib/ipc";
import type { DuplicateGroup } from "../lib/ipc";
import { humanSize } from "../lib/format";
import { RollNumber } from "../components/RollNumber";
import { cascade, pageVariants, springSnappy } from "../lib/motion";
import { useStore } from "../store";

/** 重复文件页：内核 XXH3 三级哈希管道（大小 → 头部预哈希 → 全量哈希），只报告不动手。 */

const msgOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export function Duplicates() {
    const toast = useStore((s) => s.toast);
    const desktop = ipc.isDesktop();

    const [path, setPath] = useState("");
    const [minMb, setMinMb] = useState("10");
    const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
    // 空结果态口径要贴本次实跑的门限，而不是输入框的即时值
    const [ranMinMb, setRanMinMb] = useState(10);
    const [hunting, setHunting] = useState(false);
    const [cleaningKey, setCleaningKey] = useState<string | null>(null);
    const [confirmKey, setConfirmKey] = useState<string | null>(null);

    /** 清理本组冗余份数：保留第 1 份（建议保留），其余进暂存区（台账可还原） */
    async function cleanGroup(gi: number) {
        const g = groups?.[gi];
        if (!g || cleaningKey) return;
        const redundant = g.files.slice(1);
        if (redundant.length === 0) return;
        setCleaningKey(g.hash + "-" + gi);
        try {
            await useStore.getState().manualDelete(redundant);
            setGroups((list) => (list ? list.filter((_, i) => i !== gi) : list));
        } finally {
            setCleaningKey(null);
            setConfirmKey(null);
        }
    }

    /** 可回收合计 = 每组保留 1 份后可释放的字节：Σ size × (份数 - 1) */
    const reclaimed = (groups ?? []).reduce(
        (a, g) => a + g.size * Math.max(g.files.length - 1, 0),
        0,
    );

    async function hunt() {
        if (hunting) return;
        const mb = Math.max(1, Math.floor(Number(minMb) || 0));
        setHunting(true);
        try {
            const g = await ipc.findDupes(path.trim(), mb);
            setRanMinMb(mb);
            setGroups(g);
        } catch (e) {
            toast("err", `猎取失败：${msgOf(e)}`);
        } finally {
            setHunting(false);
        }
    }

    async function locate(p: string) {
        try {
            await ipc.revealInExplorer(p);
        } catch (e) {
            toast("err", msgOf(e));
        }
    }

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">重复文件</h1>
            <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                XXH3-128 内容级比对 · 只报告不动手 · 删除决策由你来做
            </p>

            {/* ── 猎取表单 ── */}
            <div
                className="mt-4 flex flex-wrap items-end gap-3 rounded-xl border p-5"
                style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
            >
                <label className="min-w-0 flex-1 basis-64">
                    <span className="text-sm">扫描路径</span>
                    <span className="mt-0.5 block text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        在该目录范围内做内容级比对
                    </span>
                    <input
                        value={path}
                        onChange={(e) => setPath(e.target.value)}
                        placeholder={String.raw`D:\Photos`}
                        spellCheck={false}
                        className="num mt-2 w-full rounded-lg border px-3 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                        style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                    />
                </label>
                <label>
                    <span className="text-sm">最小体积</span>
                    <span className="mt-0.5 block text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        MB，低于门槛不进管道
                    </span>
                    <input
                        type="number"
                        min={1}
                        value={minMb}
                        onChange={(e) => setMinMb(e.target.value)}
                        className="num mt-2 w-24 rounded-lg border px-2.5 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                        style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                    />
                </label>
                <button
                    onClick={() => void hunt()}
                    disabled={hunting}
                    className="flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
                    style={{ background: "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))", color: "#ffffff" }}
                >
                    <Fingerprint size={14} /> {hunting ? "猎取中…" : "猎取重复"}
                </button>
            </div>

            {/* ── 运行态：骨架 + 管道提示 ── */}
            {hunting && (
                <div className="mt-4">
                    <div
                        className="flex items-center gap-2.5 rounded-lg border px-3 py-2.5 text-xs"
                        style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)" }}
                    >
                        <motion.span
                            aria-hidden
                            animate={{ opacity: [1, 0.2, 1] }}
                            transition={{ repeat: Infinity, duration: 1.5, ease: "easeInOut" }}
                            className="h-2 w-2 shrink-0 rounded-full"
                            style={{ background: "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))" }}
                        />
                        <motion.span
                            animate={{ opacity: [0.55, 1, 0.55] }}
                            transition={{ repeat: Infinity, duration: 1.6, ease: "easeInOut" }}
                            style={{ color: "var(--zc-text-1)" }}
                        >
                            三级哈希管道运行中（大小分组 → 头部预哈希 → 全量 XXH3）
                        </motion.span>
                    </div>
                    <Skeleton />
                </div>
            )}

            {/* ── 结果态 ── */}
            {!hunting && groups !== null && (
                groups.length === 0 ? (
                    <div
                        className="mt-5 flex flex-col items-center rounded-xl border px-6 py-10"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                    >
                        <Fingerprint size={20} style={{ color: "var(--zc-text-3)" }} />
                        <p className="mt-2 text-sm" style={{ color: "var(--zc-text-2)" }}>
                            该范围未发现 ≥{ranMinMb}MB 重复文件
                        </p>
                        <p className="mt-1 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                            降低门限或扩大范围再试一轮
                        </p>
                    </div>
                ) : (
                    <>
                        {/* 可回收横幅 */}
                        <motion.div
                            initial={{ opacity: 0, y: 10 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={springSnappy}
                            className="mt-4 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-xl border px-4 py-3"
                            style={{
                                background: "color-mix(in srgb, var(--zc-ok) 10%, var(--zc-surface-1))",
                                borderColor: "color-mix(in srgb, var(--zc-ok) 30%, transparent)",
                            }}
                        >
                            <Copy size={15} style={{ color: "var(--zc-ok)" }} />
                            <span className="text-xs" style={{ color: "var(--zc-text-2)" }}>
                                可回收合计（每组保留 1 份）
                            </span>
                            <span className="text-base font-semibold" style={{ color: "var(--zc-ok)" }}>
                                <RollNumber value={reclaimed} mode="bytes" />
                            </span>
                            <span className="num ml-auto text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                                {groups.length} 组
                            </span>
                        </motion.div>

                        {/* 组卡片级联 */}
                        <div className="mt-3 flex flex-col gap-2.5">
                            {groups.map((g, gi) => (
                                <motion.section
                                    key={`${g.hash}-${gi}`}
                                    variants={cascade(gi)}
                                    initial="initial"
                                    animate="animate"
                                    className="rounded-xl border p-4"
                                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                                >
                                    <div className="flex flex-wrap items-center gap-2 text-sm">
                                        <Copy size={14} style={{ color: "var(--zc-accent-b)" }} />
                                        <span className="num font-medium">
                                            {g.files.length} 份 × {humanSize(g.size)}
                                        </span>
                                        <span className="text-xs" style={{ color: "var(--zc-text-3)" }}>
                                            · 建议保留最新
                                        </span>
                                        <span
                                            className="num text-[11px]"
                                            style={{ color: "var(--zc-text-3)" }}
                                            title="去掉冗余份数后本组可回收"
                                        >
                                            −{humanSize(g.size * (g.files.length - 1))}
                                        </span>
                                        {desktop && g.files.length > 1 && (
                                            <button
                                                onClick={() => {
                                                    const k = g.hash + "-" + gi;
                                                    if (confirmKey === k) { void cleanGroup(gi); }
                                                    else {
                                                        setConfirmKey(k);
                                                        setTimeout(() => setConfirmKey((c) => (c === k ? null : c)), 4000);
                                                    }
                                                }}
                                                disabled={cleaningKey === g.hash + "-" + gi}
                                                className="ml-auto flex shrink-0 items-center gap-1 rounded-lg px-2.5 py-1 text-[11px] font-medium transition-transform active:scale-95 disabled:opacity-50"
                                                style={{
                                                    background: confirmKey === g.hash + "-" + gi
                                                        ? "color-mix(in srgb, var(--zc-danger) 18%, transparent)"
                                                        : "color-mix(in srgb, var(--zc-accent-b) 14%, transparent)",
                                                    color: confirmKey === g.hash + "-" + gi ? "var(--zc-danger)" : "var(--zc-accent-b)",
                                                }}
                                                title="保留第 1 份，其余移入暂存区（可在历史页还原）"
                                            >
                                                <Archive size={12} />
                                                {cleaningKey === g.hash + "-" + gi
                                                    ? "搬运中…"
                                                    : confirmKey === g.hash + "-" + gi
                                                        ? "再点一次确认"
                                                        : "清理冗余 " + (g.files.length - 1) + " 份"}
                                            </button>
                                        )}
                                    </div>
                                    <div className="mt-2 flex flex-col gap-1">
                                        {g.files.map((f, fi) => (
                                            <div
                                                key={f}
                                                className="flex items-center gap-2 rounded-lg px-2 py-1.5"
                                                style={{ background: "var(--zc-surface-2)" }}
                                            >
                                                <span
                                                    className="w-[4.5rem] shrink-0 text-[10px] font-medium"
                                                    style={{ color: fi === 0 ? "var(--zc-ok)" : "transparent" }}
                                                >
                                                    {fi === 0 ? "建议保留→" : "·"}
                                                </span>
                                                <span
                                                    className="num min-w-0 flex-1 truncate text-xs"
                                                    style={{ color: "var(--zc-text-1)" }}
                                                    title={f}
                                                >
                                                    {f}
                                                </span>
                                                {desktop && (
                                                    <button
                                                        onClick={() => void locate(f)}
                                                        className="flex shrink-0 items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-colors hover:opacity-75"
                                                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                                                    >
                                                        <Crosshair size={13} /> 定位
                                                    </button>
                                                )}
                                            </div>
                                        ))}
                                    </div>
                                </motion.section>
                            ))}
                        </div>
                    </>
                )
            )}
        </motion.div>
    );
}

/** 组骨架加载态：纯变量配色 + pulse */
function Skeleton() {
    const blockStyle: CSSProperties = { background: "var(--zc-surface-3)", borderRadius: "var(--zc-r-sm)" };
    return (
        <div className="mt-2 flex flex-col gap-2.5">
            {[3, 2, 4].map((rows, gi) => (
                <div
                    key={gi}
                    className="animate-pulse rounded-xl border p-4"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                >
                    <div className="h-4 w-40" style={blockStyle} />
                    <div className="mt-2 flex flex-col gap-1.5">
                        {Array.from({ length: rows }, (_, ri) => (
                            <div key={ri} className="h-8 rounded-lg" style={{ background: "var(--zc-surface-2)" }} />
                        ))}
                    </div>
                </div>
            ))}
        </div>
    );
}
