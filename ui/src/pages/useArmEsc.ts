import { useCallback, useEffect, useRef, useState } from "react";
import { useStore } from "../store";

/** 两段式确认（armed）期间按 Esc 立即解除 —— Radar / Duplicates / BigFiles 等共用。
 *  命令面板打开时本 hook 暂停（paletteOpen 判断），Esc 优先交给面板关闭，
 *  避免面板与页面 arm 状态互相劫持。
 *  @param armed 当前是否处于待确认态
 *  @param disarm 解除函数（须为稳定引用，如 useCallback 包装后的 setState） */
export function useArmEsc(armed: boolean, disarm: () => void) {
    const paletteOpen = useStore((s) => s.paletteOpen);
    useEffect(() => {
        if (!armed || paletteOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === "Escape") disarm();
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [armed, paletteOpen, disarm]);
}

/** 列表行共用的 keyed armed 两段式：同一时刻最多一行处于待确认态；
 *  超时或被新一行顶替自动回退；armed 期间 Esc 解除（面板打开时让位）。
 *  返回的 disarmArm 是稳定引用，可安全进依赖数组。 */
export function useArmKey(timeoutMs = 4000) {
    const [armKey, setArmKey] = useState<string | null>(null);
    const timer = useRef<number | undefined>(undefined);

    const disarmArm = useCallback(() => {
        window.clearTimeout(timer.current);
        setArmKey(null);
    }, []);

    const armKeyFor = useCallback((key: string) => {
        window.clearTimeout(timer.current);
        setArmKey(key);
        timer.current = window.setTimeout(() => setArmKey(null), timeoutMs);
    }, [timeoutMs]);

    useEffect(() => () => window.clearTimeout(timer.current), []);
    useArmEsc(armKey !== null, disarmArm);

    return { armKey, armKeyFor, disarmArm };
}

/** 通用 armed 两段式状态：arm() 进入待确认，超时或 Esc 自动回退。
 *  返回的 disarm 是稳定引用，可安全进依赖数组。 */
export function useArm(timeoutMs = 4000) {
    const [armed, setArmed] = useState(false);
    const timer = useRef<number | undefined>(undefined);

    const disarm = useCallback(() => {
        window.clearTimeout(timer.current);
        setArmed(false);
    }, []);

    const arm = useCallback(() => {
        window.clearTimeout(timer.current);
        setArmed(true);
        timer.current = window.setTimeout(() => setArmed(false), timeoutMs);
    }, [timeoutMs]);

    useEffect(() => () => window.clearTimeout(timer.current), []);
    useArmEsc(armed, disarm);

    return { armed, arm, disarm };
}
