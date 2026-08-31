import { useEffect } from "react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { Sparkle, HeartPulse, Wrench, History, Settings, Command, Radar, Rocket, FolderOutput, FileSearch, Copy, ShieldCheck  } from "lucide-react";
import { Home } from "./pages/Home";
import { Results } from "./pages/Results";
import { History as HistoryPage } from "./pages/History";
import { Tools } from "./pages/Tools";
import { DeepTools } from "./pages/DeepTools";
import { Radar as RadarPage } from "./pages/Radar";
import { BigFiles } from "./pages/BigFiles";
import { Duplicates } from "./pages/Duplicates";
import { Settings as SettingsPage } from "./pages/Settings";
import { StartupManager } from "./pages/StartupManager";
import { MigrateCenter } from "./pages/MigrateCenter";
import { CleaningOverlay } from "./pages/CleaningOverlay";
import { ToastStack } from "./components/ToastStack";
import { CommandPalette } from "./components/CommandPalette";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { springSnappy } from "./lib/motion";
import { useStore, type Page } from "./store";

const NAV: { id: Page; label: string; icon: typeof HeartPulse }[] = [
    { id: "home", label: "体检台", icon: HeartPulse },
    { id: "history", label: "历史", icon: History },
    { id: "tools", label: "工具箱", icon: Wrench },
    { id: "deeptools", label: "深度工具", icon: ShieldCheck },
    { id: "startup", label: "启动项", icon: Rocket },
    { id: "migrate", label: "迁移中心", icon: FolderOutput },
    { id: "radar", label: "空间雷达", icon: Radar },
    { id: "bigfiles", label: "大文件", icon: FileSearch },
    { id: "dupes", label: "重复文件", icon: Copy },
    { id: "settings", label: "设置", icon: Settings },
];

export default function App() {
    const version = useStore((s) => s.version);
    const appVer = useStore((s) => s.appVersion);
    const migrateActive = useStore((s) => s.migrateActive);
    const init = useStore((s) => s.init);
    // 页面路由提升进 store：雷达页「作为迁移源」等跨页跳转可复用 setActivePage
    const page = useStore((s) => s.activePage);
    const setPage = useStore((s) => s.setActivePage);

    useEffect(() => {
        void init();
        // 扫描结束后自动进入结果页
        const unsub = useStore.subscribe((s, prev) => {
            if (s.phase === "results" && prev?.phase !== "results") s.setActivePage("results");
        });
        return unsub;
    }, [init]);

    return (
        <MotionConfig reducedMotion="user">
            <div className="flex h-full">
                {/* ── 侧栏 ── */}
                <aside
                    className="flex w-52 shrink-0 flex-col gap-0.5 border-r p-3"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                >
                    <div className="mb-4 flex items-center gap-2.5 px-2 pt-2">
                        <div
                            className="grid h-9 w-9 place-items-center rounded-xl"
                            style={{
                                background: "var(--zc-grad-brand)",
                                boxShadow: "0 6px 18px -6px color-mix(in srgb, var(--zc-accent-b) 55%, transparent), inset 0 1px 0 rgb(255 255 255 / .35)",
                            }}
                        >
                            <Sparkle size={17} color="#fff" strokeWidth={2.4} />
                        </div>
                        <div className="min-w-0">
                            <div className="text-sm font-semibold tracking-tight">ZDiskCleaner Pro</div>
                            <div className="mt-0.5 flex items-center gap-1.5">
                                <span
                                    className="rounded-full px-1.5 py-px text-[9px] font-medium"
                                    style={{ background: "var(--zc-surface-3)", color: "var(--zc-text-2)" }}
                                >
                                    v{appVer || "…"}
                                </span>
                                <span className="text-[10px]" style={{ color: "var(--zc-text-3)" }}>
                                    core {version.replace(/^zc-core v/, "") || "…"}
                                </span>
                            </div>
                        </div>
                    </div>
                    <div
                        className="mx-2 mb-3 h-px"
                        style={{ background: "var(--zc-hairline)" }}
                    />

                    {NAV.map(({ id, label, icon: Icon }) => {
                        const active = page === id || (id === "home" && page === "results");
                        return (
                            <button
                                key={id}
                                onClick={() => setPage(id)}
                                className="relative flex items-center gap-2.5 rounded-xl px-3 py-2 text-sm transition-colors hover:bg-white/4"
                                style={{ color: active ? "var(--zc-text-1)" : "var(--zc-text-2)" }}
                            >
                                {active && (
                                    <motion.span
                                        layoutId="nav-indicator"
                                        transition={springSnappy}
                                        className="absolute inset-0 rounded-xl"
                                        style={{ background: "var(--zc-surface-3)" }}
                                    />
                                )}
                                <Icon
                                    size={15}
                                    className="relative z-10"
                                    style={active ? { color: "var(--zc-accent-b)" } : undefined}
                                />
                                <span className="relative z-10">{label}</span>
                            </button>
                        );
                    })}

                    <div className="mt-auto flex flex-col gap-2 px-2">
                        {migrateActive && (
                            <button
                                onClick={() => setPage("migrate")}
                                className="flex items-center gap-2 rounded-lg border px-2.5 py-2 text-left text-[11px] transition-colors hover:opacity-80"
                                style={{
                                    borderColor: "color-mix(in srgb, var(--zc-accent-b) 45%, transparent)",
                                    background: "color-mix(in srgb, var(--zc-accent-b) 10%, transparent)",
                                    color: "var(--zc-text-1)",
                                }}
                            >
                                <motion.span
                                    animate={{ opacity: [1, 0.35, 1] }}
                                    transition={{ repeat: Infinity, duration: 1.4 }}
                                    className="h-2 w-2 shrink-0 rounded-full"
                                    style={{ background: "var(--zc-accent-b)" }}
                                />
                                迁移后台进行中…<span className="ml-auto text-[10px]" style={{ color: "var(--zc-text-3)" }}>查看</span>
                            </button>
                        )}
                        <button
                            onClick={() => useStore.getState().togglePalette(true)}
                            className="flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-[11px] transition-colors hover:bg-white/5"
                            style={{ color: "var(--zc-text-3)" }}
                        >
                            <Command size={12} /> Ctrl+K 命令面板
                        </button>
                        <span className="text-[10px]" style={{ color: "var(--zc-text-3)" }}>
                            笔笔可恢复，分分都算数
                        </span>
                    </div>
                </aside>

                {/* ── 内容 ── */}
                <main className="min-w-0 flex-1 overflow-auto p-8">
                    <ErrorBoundary>
                    <AnimatePresence mode="wait">
                        {page === "home" && <Home key="home" />}
                        {page === "results" && <Results key="results" />}
                        {page === "history" && <HistoryPage key="history" />}
                        {page === "tools" && <Tools key="tools" />}
                        {page === "deeptools" && <DeepTools key="deeptools" />}
                        {page === "startup" && <StartupManager key="startup" />}
                        {page === "migrate" && <MigrateCenter key="migrate" />}
                        {page === "radar" && <RadarPage key="radar" />}
                        {page === "bigfiles" && <BigFiles key="bigfiles" />}
                        {page === "dupes" && <Duplicates key="dupes" />}
                        {page === "settings" && <SettingsPage key="settings" />}
                    </AnimatePresence>
                    </ErrorBoundary>
                </main>

                <CleaningOverlay />
                <ToastStack />
                <CommandPalette />
            </div>
        </MotionConfig>
    );
}
