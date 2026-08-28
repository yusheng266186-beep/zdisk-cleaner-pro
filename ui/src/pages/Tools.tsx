import {
    FileClock,
    FileSearch,
    FolderOutput,
    ListTree,
    Rocket,
    Settings2,
    Sparkle,
} from "lucide-react";
import { motion } from "motion/react";
import { useStore, type Page } from "../store";
import { cascade, pageVariants } from "../lib/motion";

const TOOLS = [
    { icon: FolderOutput, name: "存储迁移中心", desc: "npm/pip/Gradle/微信等目录跨盘搬迁，junction 回滚保障", tag: "见左侧栏" },
    { icon: ListTree, name: "空间雷达", desc: "Treemap 可视化，下钻即定位大文件", tag: "见左侧栏" },
    { icon: FileSearch, name: "大文件", desc: "≥1MB 文件 Top-N 排行，一键定位到资源管理器", tag: "见左侧栏", go: "bigfiles" },
    { icon: FileClock, name: "重复文件猎手", desc: "XXH3 三级管道，组内标「建议保留最新」，一键定位", tag: "见左侧栏", go: "dupes" },
    { icon: Rocket, name: "启动项管家", desc: "Run 键枚举 / 禁用备份还原", tag: "见左侧栏" },
    { icon: Settings2, name: "深度工具", desc: "WinSxS 组件清理（DISM 真实进度）/ 还原点 / 系统级占用指引", tag: "见左侧栏", go: "deeptools" },
    { icon: Sparkle, name: "计划维护", desc: "每周静默体检 + 报告到桌面通知", tag: "规划中" },
];

export function Tools() {
    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">工具箱</h1>
            <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                六件套陆续点亮；灰卡即占位，不做假按钮。
            </p>

            <div className="mt-5 grid grid-cols-1 gap-3 sm:grid-cols-2">
                {TOOLS.map((t, i) => (
                    <motion.div
                        key={t.name}
                        variants={cascade(i)}
                        initial="initial"
                        animate="animate"
                        whileHover={"go" in t ? { y: -3 } : undefined}
                        transition={{ type: "spring", stiffness: 380, damping: 26 }}
                        className="rounded-xl border p-5"
                        style={{
                            background: "var(--zc-surface-1)",
                            borderColor: "var(--zc-border)",
                            cursor: "go" in t ? "pointer" : "default",
                            opacity: "go" in t ? 1 : 0.72,
                        }}
                    onClick={"go" in t ? () => useStore.getState().setActivePage(t.go as Page) : undefined}
                        >
                        <div className="flex items-center gap-2">
                            <t.icon size={17} style={{ color: "var(--zc-accent-b)" }} />
                            <span className="font-medium">{t.name}</span>
                        </div>
                        <p className="mt-2 text-xs leading-relaxed" style={{ color: "var(--zc-text-2)" }}>{t.desc}</p>
                        <div className="mt-3 inline-block rounded-full px-2 py-0.5 text-[10px]" style={{ background: "var(--zc-surface-3)", color: "var(--zc-text-3)" }}>
                            {t.tag}
                        </div>
                    </motion.div>
                ))}
            </div>
        </motion.div>
    );
}
