import { useCallback, useEffect, useState } from "react";
import { Power, RotateCcw, ShieldCheck } from "lucide-react";
import { motion } from "motion/react";
import { cascade, pageVariants } from "../lib/motion";
import type { StartupEntry } from "../lib/ipc";
import * as ipc from "../lib/ipc";
import { useStore } from "../store";

/** 命令列截断宽度：超过部分以 … 收尾，title 显示全文 */
const MAX_CMD = 70;

const msgOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export function StartupManager() {
    const toast = useStore((s) => s.toast);
    /** null = 加载中（骨架），[] 且加载完成 = 空态 */
    const [entries, setEntries] = useState<StartupEntry[] | null>(null);
    const [disabled, setDisabled] = useState(0);
    const [busyKey, setBusyKey] = useState<string | null>(null);
    const [restoring, setRestoring] = useState(false);

    const refresh = useCallback(async () => {
        try {
            const [list, count] = await Promise.all([ipc.listStartups(), ipc.disabledCount()]);
            setEntries(list);
            setDisabled(count);
        } catch (e) {
            setEntries([]);
            toast("err", `读取启动项失败：${msgOf(e)}`);
        }
    }, [toast]);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    async function disableOne(entry: StartupEntry) {
        if (busyKey) return;
        setBusyKey(entry.key_id);
        try {
            const changed = await ipc.disableStartup(entry.key_id);
            if (changed) {
                toast("ok", `已禁用「${entry.name}」—— 仅写入本地备份，可随时恢复`);
                await refresh();
            } else {
                toast("warn", "该启动项已不在列表中（可能已被其他程序移除）");
                await refresh();
            }
        } catch (e) {
            toast("err", `禁用失败：${msgOf(e)}`);
        } finally {
            setBusyKey(null);
        }
    }

    async function restoreAll() {
        if (restoring) return;
        setRestoring(true);
        try {
            const n = await ipc.enableAllStartups();
            toast(n > 0 ? "ok" : "info", n > 0 ? `已恢复 ${n} 个启动项` : "备份为空，没有需要恢复的启动项");
            await refresh();
        } catch (e) {
            toast("err", `恢复失败：${msgOf(e)}`);
        } finally {
            setRestoring(false);
        }
    }

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <div className="flex items-center gap-3">
                <h1 className="text-xl font-semibold">启动项管家</h1>
                <span
                    className="num inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-[11px]"
                    title="被本工具禁用、可在备份中一键还原的数量"
                    style={{
                        background: disabled > 0 ? "color-mix(in srgb, var(--zc-warn) 12%, transparent)" : "var(--zc-surface-3)",
                        color: disabled > 0 ? "var(--zc-warn)" : "var(--zc-text-3)",
                    }}
                >
                    <ShieldCheck size={11} /> 已禁用 {disabled}
                </span>
            </div>
            <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                枚举 HKCU 的 Run / RunOnce；「禁用」只是把值搬进本地备份 JSON，不删除任何文件。
            </p>

            {/* 表头 */}
            <div className="mt-5 grid grid-cols-[92px_minmax(0,130px)_minmax(0,1fr)_64px] items-center gap-3 px-4 text-[10px] uppercase tracking-wide" style={{ color: "var(--zc-text-3)" }}>
                <span>开关状态</span>
                <span>名称</span>
                <span>命令</span>
                <span className="text-right">操作</span>
            </div>

            {entries === null ? (
                <Skeleton />
            ) : entries.length === 0 ? (
                <div
                    className="mt-2 flex flex-col items-center gap-2 rounded-xl border py-12 text-sm"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)", color: "var(--zc-text-3)" }}
                >
                    <Power size={18} style={{ color: "var(--zc-ok)" }} />
                    当前用户没有可管理的开机自启项 —— 很干净，保持住。
                </div>
            ) : (
                <ul className="mt-2 flex flex-col">
                    {entries.map((e, i) => (
                        <motion.li
                            key={e.key_id}
                            variants={cascade(i)}
                            initial="initial"
                            animate="animate"
                            className="grid grid-cols-[92px_minmax(0,130px)_minmax(0,1fr)_64px] items-center gap-3 border-b px-4 py-3 last:border-b-0"
                            style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                        >
                            <span className="flex items-center gap-1.5 text-xs" style={{ color: "var(--zc-ok)" }} title={e.run_once ? "RunOnce：下次登录执行一次后自动消失" : "每次登录时自动运行"}>
                                <span className="h-1.5 w-1.5 shrink-0 rounded-full" style={{ background: "var(--zc-ok)" }} />
                                启用中{e.run_once && <em className="not-italic" style={{ color: "var(--zc-warn)" }}>·单次</em>}
                            </span>
                            <span className="truncate text-sm font-medium">{e.name}</span>
                            <span className="num truncate text-xs" style={{ color: "var(--zc-text-2)" }} title={e.command}>
                                {truncateCmd(e.command)}
                            </span>
                            <button
                                onClick={() => void disableOne(e)}
                                disabled={busyKey !== null || restoring}
                                className="justify-self-end rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                                style={{ borderColor: "var(--zc-border-strong)", color: busyKey === e.key_id ? "var(--zc-text-3)" : "var(--zc-text-1)" }}
                            >
                                {busyKey === e.key_id ? "禁用中…" : "禁用"}
                            </button>
                        </motion.li>
                    ))}
                </ul>
            )}

            {/* 页脚：恢复全部 */}
            <div className="mt-5 flex items-center justify-between gap-4">
                <p className="text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                    禁用 ≠ 卸载：程序本体与数据不受影响。
                </p>
                <button
                    onClick={() => void restoreAll()}
                    disabled={restoring || disabled === 0}
                    className="flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                    style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-ok)" }}
                >
                    <RotateCcw size={12} /> {restoring ? "恢复中…" : `恢复全部${disabled > 0 ? `（${disabled}）` : ""}`}
                </button>
            </div>
        </motion.div>
    );
}

function truncateCmd(cmd: string): string {
    return cmd.length > MAX_CMD ? `${cmd.slice(0, MAX_CMD - 1)}…` : cmd;
}

function Skeleton() {
    return (
        <div className="mt-2 overflow-hidden rounded-xl border" style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }} aria-hidden>
            {[0, 1, 2, 3].map((i) => (
                <div
                    key={i}
                    className="grid grid-cols-[92px_minmax(0,130px)_minmax(0,1fr)_64px] items-center gap-3 border-b px-4 py-3.5 last:border-b-0"
                    style={{ borderColor: "var(--zc-border)" }}
                >
                    <div className="h-3 animate-pulse rounded" style={{ background: "var(--zc-surface-3)", width: 56 }} />
                    <div className="h-3 animate-pulse rounded" style={{ background: "var(--zc-surface-3)" }} />
                    <div className="h-3 flex-1 animate-pulse rounded" style={{ background: "var(--zc-surface-3)" }} />
                    <div className="ml-auto h-6 w-12 animate-pulse rounded-lg" style={{ background: "var(--zc-surface-3)" }} />
                </div>
            ))}
        </div>
    );
}
