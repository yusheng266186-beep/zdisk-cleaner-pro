import { Component, type ErrorInfo, type ReactNode } from "react";
import { motion } from "motion/react";
import { popIn } from "../lib/motion";

/** 全局渲染兜底：任何页面抛错都落到这张恢复卡，而不是把整棵
 *  React 树卸载成黑屏（v3.0.1 雷达页黑屏的放大器）。
 *  v5：入场套 popIn（收编死码词汇）；新增可选 onReset——渲染「返回体检台」
 *  按钮兑现卡片文案（App 侧传参由页面层负责），确认红字走 --zc-danger-text 档。 */

interface Props {
    children: ReactNode;
    /** 提供时渲染「返回体检台」：跳回 home 页（并复位边界由调用方决定） */
    onReset?: () => void;
}

interface State {
    error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
    state: State = { error: null };

    static getDerivedStateFromError(error: Error): State {
        return { error };
    }

    componentDidCatch(error: Error, info: ErrorInfo) {
        console.error("[zc] 页面渲染崩溃：", error, info.componentStack);
    }

    render() {
        if (this.state.error) {
            return (
                <motion.div
                    variants={popIn}
                    initial="initial"
                    animate="animate"
                    className="mx-auto mt-10 max-w-lg rounded-xl border p-6"
                    style={{
                        background: "color-mix(in srgb, var(--zc-danger) 6%, var(--zc-surface-1))",
                        borderColor: "color-mix(in srgb, var(--zc-danger) 30%, transparent)",
                    }}
                >
                    <h2 className="text-base font-semibold" style={{ color: "var(--zc-danger-text)" }}>
                        这个页面崩溃了
                    </h2>
                    <p className="mt-2 text-xs leading-relaxed" style={{ color: "var(--zc-text-2)" }}>
                        {this.state.error.message || String(this.state.error)}
                    </p>
                    <p className="mt-1 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        其余页面不受影响；回到体检台可继续正常使用。
                    </p>
                    <div className="mt-4 flex gap-2">
                        <button
                            onClick={() => this.setState({ error: null })}
                            className="zc-press rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                            style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                        >
                            重试本页
                        </button>
                        {this.props.onReset && (
                            <button
                                onClick={this.props.onReset}
                                className="zc-press rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-accent-text)" }}
                            >
                                返回体检台
                            </button>
                        )}
                    </div>
                </motion.div>
            );
        }
        return this.props.children;
    }
}
