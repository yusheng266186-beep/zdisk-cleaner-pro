import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { AnimatePresence, motion } from "motion/react";
import { FolderOpen, FolderOutput, RefreshCcw } from "lucide-react";
import { TreemapCanvas } from "../components/TreemapCanvas";
import { analyzeTree, isDesktop, revealInExplorer } from "../lib/ipc";
import type { TreeNode } from "../lib/tree";
import { humanSize, thousand } from "../lib/format";
import { pageVariants, springSnappy } from "../lib/motion";
import { useStore } from "../store";

/** 空间雷达：目录体积聚合树的 treemap 可视化。 */

const CANVAS_H = "h-[clamp(300px,48vh,560px)]"; // 固定可视高度，避免 ResizeObserver 反馈循环

export function Radar() {
    const [tree, setTree] = useState<TreeNode | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [tick, setTick] = useState(0);
    // treemap 选中节点：点叶子或 shift+点击任意块后出现底部选中条
    const [selected, setSelected] = useState<TreeNode | null>(null);

    const toast = useStore((s) => s.toast);
    const setPendingMigrateSrc = useStore((s) => s.setPendingMigrateSrc);
    const setActivePage = useStore((s) => s.setActivePage);
    const desktop = isDesktop();

    useEffect(() => {
        let alive = true;
        setError(null);
        analyzeTree("", 4)
            .then((t) => {
                if (alive) setTree(t);
            })
            .catch((e) => {
                if (alive) setError(e instanceof Error ? e.message : String(e));
            });
        return () => {
            alive = false;
        };
    }, [tick]);

    const refresh = useCallback(() => {
        setTree(null); // 回到骨架加载态
        setError(null);
        setSelected(null); // 旧树的选中节点随之失效
        setTick((t) => t + 1);
    }, []);

    /** 在资源管理器打开选中目录（仅桌面壳有该按钮） */
    async function openInExplorer() {
        if (!selected) return;
        try {
            await revealInExplorer(selected.path);
        } catch (e) {
            toast("err", e instanceof Error ? e.message : String(e));
        }
    }

    /** 选中节点暂存为迁移源，跨页跳转迁移中心预填表单 */
    function useAsMigrateSource() {
        if (!selected) return;
        // 节点 path 为归一化字符串，Windows 路径 API 接受原样直传
        setPendingMigrateSrc(selected.path);
        setActivePage("migrate");
    }

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto flex max-w-5xl flex-col">
            {/* ── 页头 ── */}
            <div className="flex items-start justify-between gap-4">
                <div>
                    <h1 className="text-xl font-semibold">空间雷达</h1>
                    <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                        {tree
                            ? `${humanSize(tree.size)} · ${thousand(tree.files)} 个文件 · ${thousand(tree.dirs)} 个目录`
                            : "正在枚举目录体积…"}
                    </p>
                </div>
                <div className="flex items-center gap-2">
                    <input
                        readOnly
                        value={tree?.path ?? ""}
                        placeholder={error ? "" : "分析中…"}
                        spellCheck={false}
                        title="当前分析的根路径"
                        className="w-72 rounded-lg border px-3 py-1.5 text-xs outline-none"
                        style={{
                            background: "var(--zc-surface-2)",
                            borderColor: "var(--zc-border)",
                            color: "var(--zc-text-2)",
                        }}
                    />
                    <button
                        onClick={refresh}
                        disabled={!tree && !error}
                        className="flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                    >
                        <RefreshCcw size={13} /> 刷新
                    </button>
                </div>
            </div>

            {/* ── 错误态 / 骨架 / treemap ── */}
            {error ? (
                <div
                    className={`mt-5 ${CANVAS_H} flex flex-col items-center justify-center rounded-xl border`}
                    style={{
                        background: "color-mix(in srgb, var(--zc-danger) 8%, var(--zc-surface-1))",
                        borderColor: "color-mix(in srgb, var(--zc-danger) 30%, transparent)",
                    }}
                >
                    <p className="text-sm" style={{ color: "var(--zc-danger)" }}>
                        构建体积树失败：{error}
                    </p>
                    <button
                        onClick={refresh}
                        className="mt-3 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                    >
                        重试
                    </button>
                </div>
            ) : tree ? (
                <div className={`mt-5 ${CANVAS_H}`}>
                    <TreemapCanvas node={tree} onSelectNode={setSelected} />
                </div>
            ) : (
                <Skeleton />
            )}

            {/* ── 页脚提示 ── */}
            <p className="mt-3 text-center text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                点击色块下钻 · Shift+点击或点叶子块选中 · 面积=体积
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
                                className="flex shrink-0 items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-colors hover:opacity-75"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                            >
                                <FolderOpen size={13} /> 在资源管理器打开
                            </button>
                        )}
                        <button
                            onClick={useAsMigrateSource}
                            className="flex shrink-0 items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium transition-transform active:scale-95"
                            style={{
                                background: "color-mix(in srgb, var(--zc-accent-b) 16%, transparent)",
                                color: "var(--zc-accent-b)",
                            }}
                        >
                            <FolderOutput size={13} /> 作为迁移源
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
            className={`mt-5 ${CANVAS_H} animate-pulse overflow-hidden rounded-xl border p-1`}
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
