import { Component, type ErrorInfo, type ReactNode } from "react";

/** 全局渲染兜底：任何页面抛错都落到这张恢复卡，而不是把整棵
 *  React 树卸载成黑屏（v3.0.1 雷达页黑屏的放大器）。 */

interface Props {
    children: ReactNode;
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
                <div
                    className="mx-auto mt-10 max-w-lg rounded-xl border p-6"
                    style={{
                        background: "color-mix(in srgb, var(--zc-danger) 6%, var(--zc-surface-1))",
                        borderColor: "color-mix(in srgb, var(--zc-danger) 30%, transparent)",
                    }}
                >
                    <h2 className="text-base font-semibold" style={{ color: "var(--zc-danger)" }}>
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
                            className="rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                            style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                        >
                            重试本页
                        </button>
                    </div>
                </div>
            );
        }
        return this.props.children;
    }
}
