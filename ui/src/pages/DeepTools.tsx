import { useCallback, useEffect, useState } from "react";
import { Camera, CheckCircle2, Copy, Eraser, HardDrive, ShieldCheck } from "lucide-react";
import { motion } from "motion/react";
import { cascade, pageVariants, springSnappy } from "../lib/motion";
import type { OccupancyItem } from "../lib/ipc";
import * as ipc from "../lib/ipc";
import { humanSize } from "../lib/format";
import { useStore } from "../store";

const msgOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** 深度工具：三张能力卡。口径：要么走官方通道，要么只给指引——绝不野删系统文件。 */
export function DeepTools() {
    const toast = useStore((s) => s.toast);

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">深度工具</h1>
            <p
                className="mt-2 flex items-start gap-1.5 rounded-lg border px-3 py-2 text-xs leading-relaxed"
                style={{ borderColor: "var(--zc-border)", background: "var(--zc-surface-2)", color: "var(--zc-text-2)" }}
            >
                <ShieldCheck size={14} className="mt-0.5 shrink-0" style={{ color: "var(--zc-ok)" }} />
                安全声明：这些操作要么走官方通道，要么只给指引——绝不野删系统文件。
            </p>

            <div className="mt-4 flex flex-col gap-3">
                <DismCard toast={toast} />
                <RestorePointCard toast={toast} />
                <OccupancyCard toast={toast} />
            </div>
        </motion.div>
    );
}

/* ── 卡A：WinSxS 组件清理 ─────────────────────────────── */

type DismState = "idle" | "running" | "done" | "error";

function DismCard({ toast }: { toast: (kind: "ok" | "warn" | "err" | "info", msg: string) => void }) {
    const [state, setState] = useState<DismState>("idle");
    const [pct, setPct] = useState(0);
    const [errMsg, setErrMsg] = useState<string | null>(null);

    async function start() {
        if (state === "running") return;
        setState("running");
        setPct(0);
        setErrMsg(null);
        let unlisten: (() => void) | undefined;
        try {
            // 先订阅真实百分比（dism://progress），再发起清理
            unlisten = await ipc.onDismProgress((p) => setPct(Math.min(100, Math.round(p))));
            await ipc.dismCleanup();
            setPct(100);
            setState("done");
            toast("ok", "WinSxS 组件清理完成");
        } catch (e) {
            const msg = msgOf(e);
            setErrMsg(msg);
            setState("error");
            if (msg.includes("需要管理员")) {
                toast("warn", "需要管理员权限：请以管理员重启应用，或在 CLI 用 zclean apply --admin 流程");
            } else {
                toast("err", `组件清理失败：${msg}`);
            }
        } finally {
            unlisten?.();
        }
    }

    return (
        <Card index={0} icon={Eraser} title="WinSxS 组件清理">
            <p className="mt-2 text-xs leading-relaxed" style={{ color: "var(--zc-text-2)" }}>
                释放被旧组件占用的空间（Windows 更新残留的 WinSxS 旧版本），走 DISM
                官方通道，耗时数分钟，需管理员。
            </p>

            {state === "running" && (
                <div className="mt-3">
                    <div className="flex items-baseline justify-between text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        <span>DISM /StartComponentCleanup 执行中…</span>
                        <span className="num">{pct}%</span>
                    </div>
                    <div className="mt-1.5 h-2 overflow-hidden rounded-full" >
                        <motion.div
                            className="h-full rounded-full"
                            style={{ background: "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))" }}
                            animate={{ width: `${pct}%` }}
                            transition={springSnappy}
                        />
                    </div>
                    <p className="mt-1.5 text-[10px]" style={{ color: "var(--zc-text-3)" }}>
                        百分比来自 dism.exe 真实 stdout 解析，非估算进度。
                    </p>
                </div>
            )}

            {state === "done" && (
                <div className="mt-3 flex items-center gap-1.5 text-xs" style={{ color: "var(--zc-ok)" }}>
                    <CheckCircle2 size={14} />
                    组件清理完成 —— 旧组件占用的空间已交还系统（回收实际字节数以磁盘可用空间变化为准）。
                </div>
            )}

            {state === "error" && errMsg && (
                <div className="mt-3 rounded-lg border px-3 py-2 text-xs" style={{ borderColor: "var(--zc-border-strong)", background: "var(--zc-surface-2)", color: "var(--zc-warn)" }}>
                    {errMsg.includes("需要管理员")
                        ? "请以管理员重启应用，或在 CLI 用 zclean apply --admin 流程"
                        : `清理失败：${errMsg}`}
                </div>
            )}

            <button
                onClick={() => void start()}
                disabled={state === "running"}
                className="mt-4 rounded-lg border px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
                style={{
                    borderColor: state === "running" ? "var(--zc-border)" : "color-mix(in srgb, var(--zc-accent-b) 45%, transparent)",
                    color: "var(--zc-accent-b)",
                    background: state === "running" ? "transparent" : "color-mix(in srgb, var(--zc-accent-b) 10%, transparent)",
                }}
            >
                {state === "running" ? "清理中…" : state === "done" ? "再次清理" : "开始清理"}
            </button>
        </Card>
    );
}

/* ── 卡B：系统还原点 ──────────────────────────────────── */

type RpState = "idle" | "running" | "ok" | "err";

function RestorePointCard({ toast }: { toast: (kind: "ok" | "warn" | "err" | "info", msg: string) => void }) {
    const [desc, setDesc] = useState("");
    const [state, setState] = useState<RpState>("idle");
    const [result, setResult] = useState<string | null>(null);

    async function create() {
        if (state === "running") return;
        if (!desc.trim()) {
            toast("warn", "请先填写还原点描述");
            return;
        }
        setState("running");
        setResult(null);
        try {
            await ipc.createRestorePoint(desc.trim());
            setState("ok");
            setResult(`已创建还原点「${desc.trim()}」——可在 系统属性→系统保护 中查看`);
            toast("ok", "还原点创建成功");
        } catch (e) {
            const msg = msgOf(e);
            setState("err");
            setResult(
                msg.includes("需要管理员")
                    ? "请以管理员重启应用，或在 CLI 用 zclean apply --admin 流程"
                    : `创建失败：${msg}`,
            );
            toast("err", `还原点创建失败：${msg}`);
        }
    }

    return (
        <Card index={1} icon={Camera} title="系统还原点">
            <p className="mt-2 text-xs leading-relaxed" style={{ color: "var(--zc-text-2)" }}>
                建立系统还原点作为清理前的后悔药，走 Checkpoint-Computer 官方通道，需管理员。
            </p>
            <input
                value={desc}
                onChange={(e) => setDesc(e.target.value)}
                placeholder="还原点描述，例如：清理 C 盘前快照"
                maxLength={80}
                className="mt-3 w-full rounded-lg border px-3 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
            />
            {result && (
                <div
                    className="mt-3 rounded-lg border px-3 py-2 text-xs"
                    style={{
                        borderColor: "var(--zc-border-strong)",
                        background: "var(--zc-surface-2)",
                        color: state === "ok" ? "var(--zc-ok)" : "var(--zc-warn)",
                    }}
                >
                    {result}
                </div>
            )}
            <button
                onClick={() => void create()}
                disabled={state === "running"}
                className="mt-4 rounded-lg border px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
                style={{
                    borderColor: state === "running" ? "var(--zc-border)" : "color-mix(in srgb, var(--zc-accent-b) 45%, transparent)",
                    color: "var(--zc-accent-b)",
                    background: state === "running" ? "transparent" : "color-mix(in srgb, var(--zc-accent-b) 10%, transparent)",
                }}
            >
                {state === "running" ? "创建中…（Checkpoint-Computer 等待系统确认）" : "创建还原点"}
            </button>
        </Card>
    );
}

/* ── 卡C：系统级占用 ──────────────────────────────────── */

function OccupancyCard({ toast }: { toast: (kind: "ok" | "warn" | "err" | "info", msg: string) => void }) {
    const [items, setItems] = useState<OccupancyItem[] | null>(null);

    const refresh = useCallback(async () => {
        try {
            setItems(await ipc.systemOccupancy());
        } catch (e) {
            setItems([]);
            toast("err", `系统占用盘点失败：${msgOf(e)}`);
        }
    }, [toast]);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    async function copyGuide(guide: string) {
        try {
            await navigator.clipboard.writeText(guide);
            toast("ok", "指引已复制到剪贴板");
        } catch {
            toast("err", "复制失败：剪贴板不可用");
        }
    }

    return (
        <Card index={2} icon={HardDrive} title="系统级占用">
            <p className="mt-2 text-xs leading-relaxed" style={{ color: "var(--zc-text-2)" }}>
                系统级大文件只盘点不触碰：拿不到体积（ACL 拒绝）就诚实标「未知」，指引复制即走官方通道。
            </p>
            {items === null ? (
                <div className="mt-3 flex flex-col gap-2" aria-hidden>
                    {[0, 1, 2].map((i) => (
                        <div key={i} className="h-9 zc-shimmer rounded-lg"  />
                    ))}
                </div>
            ) : items.length === 0 ? (
                <p className="mt-3 text-xs" style={{ color: "var(--zc-text-3)" }}>
                    没有盘点到系统级占用项。
                </p>
            ) : (
                <ul className="mt-3 flex flex-col">
                    {items.map((it, i) => (
                        <motion.li
                            key={it.path}
                            variants={cascade(i)}
                            initial="initial"
                            animate="animate"
                            className="flex items-center gap-3 border-b py-2.5 last:border-b-0"
                            style={{ borderColor: "var(--zc-border)" }}
                        >
                            <div className="min-w-0 flex-1">
                                <div className="flex items-baseline gap-2">
                                    <span className="text-sm font-medium">{it.name}</span>
                                    <span className="num text-xs" style={{ color: "var(--zc-text-3)" }} title={it.path}>
                                        {it.size === null ? "未知" : humanSize(it.size)}
                                    </span>
                                </div>
                                <div className="mt-0.5 truncate text-[11px]" style={{ color: "var(--zc-text-3)" }} title={it.guide_zh}>
                                    {it.guide_zh}
                                </div>
                            </div>
                            <button
                                onClick={() => void copyGuide(it.guide_zh)}
                                className="flex shrink-0 items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:opacity-75"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                            >
                                <Copy size={11} /> 复制指引
                            </button>
                        </motion.li>
                    ))}
                </ul>
            )}
        </Card>
    );
}

/* ── 通用卡片壳（样式对齐工具箱）─────────────────────── */

function Card({
    index,
    icon: Icon,
    title,
    children,
}: {
    index: number;
    icon: typeof Eraser;
    title: string;
    children: React.ReactNode;
}) {
    return (
        <motion.section
            variants={cascade(index)}
            initial="initial"
            animate="animate"
            className="rounded-xl border p-5"
            style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
        >
            <div className="flex items-center gap-2">
                <Icon size={17} style={{ color: "var(--zc-accent-b)" }} />
                <span className="font-medium">{title}</span>
            </div>
            {children}
        </motion.section>
    );
}
