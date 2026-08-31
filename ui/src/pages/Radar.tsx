import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Archive, CircleStop, FolderOpen, FolderOutput, RefreshCcw, Trash2 } from "lucide-react";
import { TreemapCanvas } from "../components/TreemapCanvas";
import { analyzeTree, driveRootPath, errCode, errMsg, isDesktop, revealInExplorer } from "../lib/ipc";
import type { TreeNode } from "../lib/tree";
import { humanSize, thousand } from "../lib/format";
import { pageVariants, springSnappy } from "../lib/motion";
import { useStore } from "../store";
import { useArm } from "./useArmEsc";

/** 空间雷达：目录体积聚合树的 treemap 可视化。
 *  v5：根目录选择器（分区 + 主目录）兑现 analyze_tree 的 path 参数；
 *  忙任务走 cancel_busy 通道；armed 态 Esc 可解除。 */

const CANVAS_H = "h-[clamp(300px,48vh,560px)]"; // 固定可视高度，避免 ResizeObserver 反馈循环

type LoadStatus = "loading" | "ready" | "error" | "cancelled";

export function Radar() {
    const [tree, setTree] = useState<TreeNode | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [status, setStatus] = useState<LoadStatus>("loading");
    const [tick, setTick] = useState(0);
    // 移入暂存区两段式确认（armed 期间 Esc 解除，命令面板打开时让位）
    const { armed: purgeArm, arm: armPurge, disarm: disarmPurge } = useArm(4000);
    // treemap 选中节点：点叶子或 shift+点击任意块后出现底部选中条
    const [selected, setSelected] = useState<TreeNode | null>(null);

    const toast = useStore((s) => s.toast);
    const setPendingMigrateSrc = useStore((s) => s.setPendingMigrateSrc);
    const setActivePage = useStore((s) => s.setActivePage);
    const drives = useStore((s) => s.drives);
    const root = useStore((s) => s.radarRootPath) ?? "";
    const setRadarRoot = useStore((s) => s.setRadarRoot);
    const setBusyRunning = useStore((s) => s.setBusyRunning);
    const cancelBusy = useStore((s) => s.cancelBusy);
    const desktop = isDesktop();

    useEffect(() => {
        let alive = true;
        setStatus("loading");
        setError(null);
        setBusyRunning(true);
        analyzeTree(root, 4, tick > 0)
            .then((t) => {
                if (!alive) return;
                setTree(t);
                setStatus("ready");
            })
            .catch((e) => {
                if (!alive) return;
                if (errCode(e) === "cancelled") {
                    setStatus("cancelled");
                    toast("info", "已取消体积分析");
                } else {
                    setError(errMsg(e));
                    setStatus("error");
                }
            })
            .finally(() => {
                if (alive) setBusyRunning(false);
            });
        return () => {
            alive = false;
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [root, tick]);

    const refresh = useCallback(() => {
        setTree(null); // 回到骨架加载态
        setError(null);
        setSelected(null); // 旧树的选中节点随之失效
        disarmPurge();
        setTick((t) => t + 1);
    }, [disarmPurge]);

    /** 选中目录安全删除：守卫 + 暂存区 + 台账，可还原；成功后重建体积树 */
    async function stashSelected() {
        if (!selected) return;
        disarmPurge();
        await useStore.getState().manualDelete([selected.path]);
        refresh();
    }

    /** 在资源管理器打开选中目录（仅桌面壳有该按钮） */
    async function openInExplorer() {
        if (!selected) return;
        try {
            await revealInExplorer(selected.path);
        } catch (e) {
            toast("err", errMsg(e));
        }
    }

    /** 选中节点暂存为迁移源，跨页跳转迁移中心预填表单 */
    function useAsMigrateSource() {
        if (!selected) return;
        // 节点 path 为归一化字符串，Windows 路径 API 接受原样直传
        setPendingMigrateSrc(selected.path);
        setActivePage("migrate");
    }

    // U2 在 TreemapCanvas 内实现选中视觉环；集成前经 spread 传入选中键不破编译
    const treemapExtra = { selectedKey: selected?.path ?? null } as { selectedKey?: string | null };

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto flex max-w-5xl flex-col">
            {/* ── 页头 ── */}
            <div className="flex items-start justify-between gap-4">
                <div>
                    <h1 className="text-xl font-semibold">空间雷达</h1>
                    <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                        {status === "ready" && tree
                            ? `${humanSize(tree.size)} · ${thousand(tree.files)} 个文件 · ${thousand(tree.dirs)} 个目录`
                            : status === "loading"
                              ? "正在枚举目录体积…"
                              : status === "cancelled"
                                ? "已取消上一次的体积分析"
                                : "体积树构建失败"}
                    </p>
                </div>
                <div className="flex items-center gap-2">
                    {/* 分析根：各分区 + 主目录（v5 兑现 analyze_tree path 参数） */}
                    <select
                        data-testid="radar-root"
                        value={root}
                        onChange={(e) => {
                            setRadarRoot(e.target.value || null);
                            setSelected(null);
                            disarmPurge();
                        }}
                        disabled={status === "loading"}
                        className="num w-56 rounded-lg border px-2.5 py-1.5 text-xs outline-none"
                        style={{
                            background: "var(--zc-surface-2)",
                            borderColor: "var(--zc-border)",
                            color: "var(--zc-text-2)",
                        }}
                        title="选择要分析的根目录"
                    >
                        <option value="">主目录（%USERPROFILE%）</option>
                        {drives.map((d) => (
                            <option key={d.label} value={driveRootPath(d.label)}>{d.label} 盘根目录</option>
                        ))}
                    </select>
                    <button
                        onClick={refresh}
                        disabled={status === "loading"}
                        className="zc-press flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                    >
                        <RefreshCcw size={13} /> 刷新
                    </button>
                    {status === "loading" && (
                        <button
                            data-testid="busy-cancel"
                            onClick={() => void cancelBusy()}
                            className="zc-press flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                            style={{ borderColor: "var(--zc-danger)", color: "var(--zc-danger-text)" }}
                            title="取消体积树构建（提前返回部分结果或终止）"
                        >
                            <CircleStop size={13} /> 取消
                        </button>
                    )}
                </div>
            </div>

            {/* ── 错误态 / 取消态 / 骨架 / treemap ── */}
            {status === "error" && error ? (
                <div
                    className={`mt-5 ${CANVAS_H} flex flex-col items-center justify-center rounded-xl border`}
                    style={{
                        background: "color-mix(in srgb, var(--zc-danger) 8%, var(--zc-surface-1))",
                        borderColor: "color-mix(in srgb, var(--zc-danger) 30%, transparent)",
                    }}
                >
                    <p className="text-sm" style={{ color: "var(--zc-danger-text)" }}>
                        构建体积树失败：{error}
                    </p>
                    <button
                        onClick={refresh}
                        className="zc-press mt-3 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                    >
                        重试
                    </button>
                </div>
            ) : status === "cancelled" ? (
                <div
                    className={`mt-5 ${CANVAS_H} flex flex-col items-center justify-center rounded-xl border`}
                    style={{
                        background: "var(--zc-surface-1)",
                        borderColor: "var(--zc-border)",
                    }}
                >
                    <CircleStop size={22} style={{ color: "var(--zc-text-3)" }} />
                    <p className="mt-2 text-sm" style={{ color: "var(--zc-text-2)" }}>
                        分析已取消，未改动任何文件
                    </p>
                    <button
                        onClick={refresh}
                        className="zc-press mt-3 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-accent-text)" }}
                    >
                        重新分析
                    </button>
                </div>
            ) : status === "ready" && tree ? (
                <div className={`mt-5 ${CANVAS_H}`}>
                    <TreemapCanvas node={tree} onSelectNode={setSelected} {...treemapExtra} />
                </div>
            ) : (
                <Skeleton />
            )}

            {/* ── 页脚提示 ── */}
            <p className="mt-3 text-center text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                点击色块下钻 · Shift+点击或点叶子块选中 · 面积=体积 · 取消/确认态按 Esc 解除
            </p>

            {/* ── 选中条：选中节点后滑入，提供两个实用动作 ── */}
            <AnimatePresence>
                {selected && (
                    <motion.div
                        initial={{ opacity: 0, y: 10 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: 8 }}
                        transition={springSnappy}
                        role="status"
                        className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1.5 rounded-xl border px-3 py-2"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                    >
                        <span className="shrink-0 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                            已选中
                        </span>
                        <span
                            className="num min-w-0 max-w-[24rem] truncate text-xs"
                            style={{ color: "var(--zc-text-1)" }}
                            title={selected.path}
                        >
                            {selected.path}
                        </span>
                        {desktop && (
                            <button
                                onClick={() => void openInExplorer()}
                                className="zc-press flex shrink-0 items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-colors hover:opacity-75"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                            >
                                <FolderOpen size={13} /> 在资源管理器打开
                            </button>
                        )}
                        <button
                            onClick={useAsMigrateSource}
                            className="zc-press flex shrink-0 items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium transition-transform active:scale-95"
                            style={{
                                background: "color-mix(in srgb, var(--zc-accent-b) 16%, transparent)",
                                color: "var(--zc-accent-text)",
                            }}
                        >
                            <FolderOutput size={13} /> 作为迁移源
                        </button>
                        <button
                            onClick={() => {
                                if (purgeArm) { void stashSelected(); }
                                else { armPurge(); }
                            }}
                            className="zc-press flex shrink-0 items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs font-medium transition-colors hover:opacity-75"
                            style={{
                                borderColor: purgeArm ? "var(--zc-danger)" : "var(--zc-border-strong)",
                                color: purgeArm ? "var(--zc-danger-text)" : "var(--zc-text-2)",
                            }}
                            title="移入暂存区（守卫校验，可在历史页 7 天内还原）"
                        >
                            {purgeArm ? <Trash2 size={13} /> : <Archive size={13} />}
                            {purgeArm ? "再点一次确认删除" : "移入暂存区"}
                        </button>
                    </motion.div>
                )}
            </AnimatePresence>
        </motion.div>
    );
}

/** treemap 形状的骨架加载态：纯变量配色 + pulse */
function Skeleton() {
    const blockStyle = (bg: string): CSSProperties => ({ background: bg, borderRadius: "var(--zc-r-sm)" });
    return (
        <div
            className={`mt-5 ${CANVAS_H} zc-shimmer overflow-hidden rounded-xl border p-1`}
            style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
        >
            <div className="flex h-full gap-1">
                <div className="flex h-full w-[46%] flex-col gap-1">
                    <div className="h-[62%]" style={blockStyle("var(--zc-surface-3)")} />
                    <div className="flex min-h-0 flex-1 gap-1">
                        <div className="h-full w-1/2" style={blockStyle("var(--zc-surface-2)")} />
                        <div className="h-full w-1/2" style={blockStyle("var(--zc-surface-2)")} />
                    </div>
                </div>
                <div className="flex h-full flex-1 flex-col gap-1">
                    <div className="flex h-[38%] gap-1">
                        <div className="h-full w-[58%]" style={blockStyle("var(--zc-surface-3)")} />
                        <div className="h-full flex-1" style={blockStyle("var(--zc-surface-2)")} />
                    </div>
                    <div className="flex min-h-0 flex-1 gap-1">
                        <div className="h-full w-1/3" style={blockStyle("var(--zc-surface-2)")} />
                        <div className="h-full w-1/3" style={blockStyle("var(--zc-surface-3)")} />
                        <div className="h-full flex-1" style={blockStyle("var(--zc-surface-2)")} />
                    </div>
                </div>
            </div>
        </div>
    );
}
