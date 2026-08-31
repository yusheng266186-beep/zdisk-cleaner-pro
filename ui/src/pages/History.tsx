import { useCallback, useState } from "react";
import { Archive, ChevronDown, FolderOutput, History as HistIcon, RotateCcw, Trash2, Undo2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { cascade, pageVariants, springSnappy } from "../lib/motion";
import { useStore } from "../store";
import { humanSize, timeAgo } from "../lib/format";
import { errCode, errMsg, migrateUndo, sessionEntries } from "../lib/ipc";
import type { HistoryRecord, SessionEntryDto } from "../lib/types";
import { useArmKey } from "./useArmEsc";

/** 清理历史（v5）：mode 筛选 chips + 批次详情下钻 + 迁移批次行（可事后撤销）。
 *  undo/purge 成功后由 store 重拉台账，幽灵行不再驻留。 */

type FilterKey = "all" | "vault" | "recycle_bin" | "system" | "migrate";

const CHIPS: { k: FilterKey; label: string; tid: string }[] = [
    { k: "all", label: "全部", tid: "hf-all" },
    { k: "vault", label: "vault", tid: "hf-vault" },
    { k: "recycle_bin", label: "回收站", tid: "hf-recycle" },
    { k: "system", label: "系统清理", tid: "hf-system" },
    { k: "migrate", label: "迁移", tid: "hf-migrate" },
];

/** 批次类别：v5 起后端带 kind；缺省回落 mode（旧台账行） */
const recKind = (r: HistoryRecord): string => r.kind ?? r.mode;

/** 结清判定（v5）：台账已抹（live=false）或流水已落终态标签——
 *  历史页隐藏全部动作，只留结清徽标；缺省 live 视为存活（浏览器演示态）。 */
const settledOf = (r: HistoryRecord): null | "undo" | "purge" => {
    const kind = recKind(r);
    if (kind === "undo") return "undo";
    if (kind === "purge") return "purge";
    if (r.live === false) return "purge"; // 到期清扫/外键抹账：统一按不可还原呈现
    return null;
};

export function History() {
    const history = useStore((s) => s.history);
    const toast = useStore((s) => s.toast);
    const reloadHistory = useStore((s) => s.reloadHistory);
    // 彻底删除二次确认：点一下变「再点一次确认」，4 秒未确认自动复位
    const { armKey: confirmId, armKeyFor: armConfirm, disarmArm: disarmConfirm } = useArmKey(4000);
    const [filter, setFilter] = useState<FilterKey>("all");
    // 批次详情下钻：一次只展开一批；条目缓存供收起后再展开免重拉
    const [detailId, setDetailId] = useState<string | null>(null);
    const [entries, setEntries] = useState<Record<string, SessionEntryDto[]>>({});
    const [entryLoading, setEntryLoading] = useState(false);
    const [undoingMigrate, setUndoingMigrate] = useState<string | null>(null);
    const max = Math.max(1, ...history.map((h) => h.bytes_moved));

    const rows = history.filter((h) => {
        if (filter === "all") return true;
        const k = recKind(h);
        // vault/回收站 chips 按 mode 归组——结清后的 kind 终态标签（undo/purge）不应让行失踪
        if (filter === "vault") return h.mode === "vault" && k !== "migrate" && k !== "migrate_undo";
        if (filter === "recycle_bin") return h.mode === "recycle_bin";
        if (filter === "migrate") return k === "migrate" || k === "migrate_undo";
        return k === filter;
    });

    const toggleDetail = useCallback(async (id: string) => {
        if (detailId === id) { setDetailId(null); return; }
        setDetailId(id);
        if (entries[id]) return;
        setEntryLoading(true);
        try {
            const es = await sessionEntries(id);
            setEntries((m) => ({ ...m, [id]: es }));
        } catch (e) {
            toast("err", `读取批次明细失败：${errMsg(e)}`);
            if (errCode(e) === "not_found") setEntries((m) => ({ ...m, [id]: [] }));
        } finally {
            setEntryLoading(false);
        }
    }, [detailId, entries, toast]);

    /** 撤销迁移批次：结构化 MigrateUndoDto，成功后重拉台账 */
    async function undoMigrate(h: HistoryRecord) {
        if (!h.src || undoingMigrate) return;
        setUndoingMigrate(h.session_id);
        try {
            const r = await migrateUndo(h.src, h.dst ?? undefined);
            const failNote = r.failed.length ? `，${r.failed.length} 项未能复位` : "";
            toast(r.failed.length ? "warn" : "ok", `撤销完成：已复位 ${r.restored} 项${failNote}`);
            await reloadHistory();
        } catch (e) {
            toast("err", `撤销失败：${errMsg(e)}`);
        } finally {
            setUndoingMigrate(null);
        }
    }

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">清理历史</h1>
            <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                「移入」不等于「真实释放」：回收站批次以清空系统回收站为准；暂存区（vault）批次 7 天内可还原，
                随时可「彻底删除」立即释放空间，超过 7 天的批次应用启动时自动清扫。
            </p>

            {/* 趋势条形图（纯 CSS 高度动画；口径为全部批次，不随筛选变化） */}
            <div className="mt-6 flex h-28 items-end gap-2 rounded-xl border p-4" style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}>
                {history.slice(0, 16).map((h, i) => (
                    <motion.div
                        key={h.session_id + i}
                        initial={{ height: 0 }}
                        animate={{ height: `${Math.max(8, (h.bytes_moved / max) * 100)}%` }}
                        transition={{ type: "spring", stiffness: 120, damping: 20, delay: i * 0.03 }}
                        className="flex-1 rounded-t-md"
                        style={{ background: "linear-gradient(180deg,var(--zc-accent-b),var(--zc-accent-a))", opacity: 0.35 + (h.bytes_moved / max) * 0.65 }}
                        title={`${humanSize(h.bytes_moved)} · ${timeAgo(h.created_unix)}`}
                    />
                ))}
                {history.length === 0 && (
                    <div className="w-full py-10 text-center text-sm" style={{ color: "var(--zc-text-3)" }}>
                        还没有清理记录 —— 完成第一次体检后这里会开始记账
                    </div>
                )}
            </div>

            {/* mode 筛选 chips */}
            <div className="mt-5 flex flex-wrap items-center gap-1.5">
                {CHIPS.map(({ k, label, tid }) => (
                    <button
                        key={k}
                        data-testid={tid}
                        onClick={() => setFilter(filter === k ? "all" : k)}
                        className="zc-press rounded-full border px-3 py-1 text-[11px] font-medium transition-colors"
                        style={{
                            borderColor: filter === k ? "color-mix(in srgb, var(--zc-accent-b) 55%, transparent)" : "var(--zc-border)",
                            background: filter === k ? "color-mix(in srgb, var(--zc-accent-b) 14%, transparent)" : "var(--zc-surface-1)",
                            color: filter === k ? "var(--zc-accent-text)" : "var(--zc-text-2)",
                        }}
                        aria-pressed={filter === k}
                    >
                        {label}
                    </button>
                ))}
            </div>

            <motion.ul layout className="mt-3 flex flex-col gap-2">
                <AnimatePresence initial={false}>
                    {rows.map((h, i) => {
                        const kind = recKind(h);
                        const isMigrate = kind === "migrate";
                        const detailOpen = detailId === h.session_id;
                        const es = entries[h.session_id];
                        const pending = es?.filter((e) => e.status === "pending").length ?? 0;
                        return (
                            <motion.li
                                key={h.session_id}
                                data-session={h.session_id}
                                layout
                                variants={cascade(i)}
                                initial="initial"
                                animate="animate"
                                exit="exit"
                                style={{ borderRadius: 12 }}
                            >
                                <div
                                    className="flex items-center gap-3 rounded-xl border p-3.5"
                                    style={{
                                        background: "var(--zc-surface-1)",
                                        borderColor: detailOpen ? "color-mix(in srgb, var(--zc-accent-b) 40%, transparent)" : "var(--zc-border)",
                                    }}
                                >
                                    {isMigrate ? (
                                        <FolderOutput size={15} style={{ color: "var(--zc-accent-text)" }} />
                                    ) : h.mode === "vault" ? (
                                        <Archive size={15} style={{ color: "var(--zc-accent-text)" }} />
                                    ) : (
                                        <HistIcon size={15} style={{ color: "var(--zc-text-3)" }} />
                                    )}
                                    <span className="flex-1 min-w-0 text-sm">
                                        {isMigrate ? (
                                            <span className="block truncate" title={`${h.src ?? ""} → ${h.dst ?? ""}`}>
                                                迁移 <span className="num" style={{ color: "var(--zc-text-3)" }}>{humanSize(h.bytes_moved)}</span>
                                                <span style={{ color: "var(--zc-text-3)" }}> · {h.src ?? "—"}</span>
                                            </span>
                                        ) : (
                                            <>
                                                {humanSize(h.bytes_moved)}
                                                <span style={{ color: "var(--zc-text-3)" }}> · 搬运 {h.files.toLocaleString("en-US")} 项</span>
                                            </>
                                        )}
                                    </span>
                                    <span className="num text-xs shrink-0" style={{ color: "var(--zc-text-3)" }}>{timeAgo(h.created_unix)}</span>
                                    {h.session_id.startsWith("elev-") && (
                                        <span className="shrink-0 rounded-full border px-2 py-0.5 text-[10px]" style={{ borderColor: "var(--zc-border)", color: "var(--zc-text-3)" }}>
                                            提权批
                                        </span>
                                    )}
                                    {isMigrate ? (
                                        <>
                                            {h.src && (
                                                <button
                                                    data-testid={`migrate-undo-${h.src}`}
                                                    onClick={() => void undoMigrate(h)}
                                                    disabled={undoingMigrate !== null}
                                                    className="zc-press flex shrink-0 items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                                                    style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-ok)" }}
                                                >
                                                    <Undo2 size={11} /> {undoingMigrate === h.session_id ? "撤销中…" : "撤销"}
                                                </button>
                                            )}
                                            <span className="text-[10px] shrink-0" style={{ color: "var(--zc-text-3)" }}>
                                                junction 回滚保障
                                            </span>
                                        </>
                                    ) : (
                                        <>
                                            {(() => {
                                                const settled = settledOf(h);
                                                if (settled) {
                                                    // 结清行：台账已抹/流水已落终态——只呈现徽标，不留死按钮
                                                    return (
                                                        <span
                                                            data-testid={`settled-${h.session_id}`}
                                                            className="shrink-0 rounded-full border px-2 py-0.5 text-[10px]"
                                                            style={{
                                                                borderColor: settled === "undo" ? "color-mix(in srgb, var(--zc-ok) 40%, transparent)" : "var(--zc-border-strong)",
                                                                color: settled === "undo" ? "var(--zc-ok)" : "var(--zc-text-3)",
                                                            }}
                                                        >
                                                            {settled === "undo" ? "已还原" : "已彻底删除"}
                                                        </span>
                                                    );
                                                }
                                                return (
                                                    <>
                                                        <button
                                                            data-testid={`detail-${h.session_id}`}
                                                            onClick={() => void toggleDetail(h.session_id)}
                                                            className="flex shrink-0 items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:bg-[var(--zc-hover)]"
                                                            style={{ borderColor: "var(--zc-border)", color: "var(--zc-text-2)" }}
                                                            title="查看本批条目明细"
                                                        >
                                                            详情
                                                            <motion.span animate={{ rotate: detailOpen ? 180 : 0 }} transition={springSnappy}>
                                                                <ChevronDown size={11} />
                                                            </motion.span>
                                                        </button>
                                                        {h.mode === "vault" ? (
                                                            <>
                                                                <button
                                                                    onClick={() => void useStore.getState().undoSession(h.session_id)}
                                                                    className="zc-press flex shrink-0 items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:opacity-75"
                                                                    style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-ok)" }}
                                                                >
                                                                    <RotateCcw size={11} /> 还原
                                                                </button>
                                                                <button
                                                                    onClick={() => {
                                                                        if (confirmId === h.session_id) {
                                                                            disarmConfirm();
                                                                            void useStore.getState().purgeSession(h.session_id);
                                                                        } else {
                                                                            armConfirm(h.session_id);
                                                                        }
                                                                    }}
                                                                    className="zc-press flex shrink-0 items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:opacity-75"
                                                                    style={{
                                                                        borderColor: confirmId === h.session_id ? "var(--zc-danger)" : "var(--zc-border-strong)",
                                                                        color: confirmId === h.session_id ? "var(--zc-danger-text)" : "var(--zc-text-3)",
                                                                    }}
                                                                >
                                                                    <Trash2 size={11} />
                                                                    {confirmId === h.session_id ? "再点一次确认" : "彻底删除"}
                                                                </button>
                                                            </>
                                                        ) : (
                                                            <span className="shrink-0 text-[10px]" style={{ color: "var(--zc-text-3)" }}>
                                                                在系统回收站
                                                            </span>
                                                        )}
                                                    </>
                                                );
                                            })()}
                                        </>
                                    )}
                                </div>

                                {/* 批次详情抽屉（entries 表下钻：origin/大小/pending 警示） */}
                                <AnimatePresence>
                                    {detailOpen && (
                                        <motion.div
                                            initial={{ height: 0, opacity: 0 }}
                                            animate={{ height: "auto", opacity: 1, transition: springSnappy }}
                                            exit={{ height: 0, opacity: 0, transition: { duration: 0.18 } }}
                                            className="overflow-hidden"
                                        >
                                            <div
                                                className="rounded-b-xl border border-t-0 px-4 py-2"
                                                style={{ background: "var(--zc-surface-2)", borderColor: "color-mix(in srgb, var(--zc-accent-b) 40%, transparent)" }}
                                            >
                                                {entryLoading && !es ? (
                                                    <p className="py-2 text-[11px]" style={{ color: "var(--zc-text-3)" }}>正在读取台账明细…</p>
                                                ) : !es || es.length === 0 ? (
                                                    <p className="py-2 text-[11px]" style={{ color: "var(--zc-text-3)" }}>该批次没有可展示的条目明细</p>
                                                ) : (
                                                    <>
                                                        {pending > 0 && (
                                                            <p className="py-1.5 text-[11px]" style={{ color: "var(--zc-warn)" }}>
                                                                ⚠ {pending} 条台账处于「未完成」（journal 中间态），下次启动会自动核对
                                                            </p>
                                                        )}
                                                        <ul className="max-h-60 overflow-auto">
                                                            {es.map((en, j) => (
                                                                <li
                                                                    key={j}
                                                                    data-testid={`entry-${h.session_id}-${j}`}
                                                                    className="flex items-center gap-3 border-b py-1.5 last:border-b-0"
                                                                    style={{ borderColor: "var(--zc-border)" }}
                                                                >
                                                                    <span className="num min-w-0 flex-1 truncate font-mono text-[11px]" style={{ color: "var(--zc-text-2)" }} title={en.origin}>
                                                                        {en.origin}
                                                                    </span>
                                                                    <span className="num shrink-0 text-[11px]" style={{ color: "var(--zc-text-3)" }}>{humanSize(en.size)}</span>
                                                                    {en.status === "pending" ? (
                                                                        <span
                                                                            className="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium"
                                                                            style={{ background: "color-mix(in srgb, var(--zc-warn) 16%, transparent)", color: "var(--zc-warn)" }}
                                                                        >
                                                                            未完成
                                                                        </span>
                                                                    ) : (
                                                                        <span className="shrink-0 text-[10px]" style={{ color: "var(--zc-text-3)" }}>已入账</span>
                                                                    )}
                                                                </li>
                                                            ))}
                                                        </ul>
                                                    </>
                                                )}
                                            </div>
                                        </motion.div>
                                    )}
                                </AnimatePresence>
                            </motion.li>
                        );
                    })}
                </AnimatePresence>
            </motion.ul>

            {rows.length === 0 && history.length > 0 && (
                <p className="mt-6 text-center text-xs" style={{ color: "var(--zc-text-3)" }}>
                    该类别下还没有记录
                </p>
            )}
        </motion.div>
    );
}
