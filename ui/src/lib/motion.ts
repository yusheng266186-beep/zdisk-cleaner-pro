/** 动效系统唯一出口：弹簧预设与变体工厂。
 *  原则：一切动画可中断、可反向；时长走 token；禁止页面私定缓动。
 *  v5：补 pulseDot / overlayIn / overlayOut（CONTRACT-v5 §4）与收编用词汇
 *  （springCard/layoutEase/crossfade），消灭各页就地手写的 transition 字面量。 */

import type { TargetAndTransition, Transition, Variants } from "motion/react";

export const springGentle: Transition = { type: "spring", stiffness: 170, damping: 22 };
export const springSoft: Transition = { type: "spring", stiffness: 260, damping: 26 };
export const springSnappy: Transition = { type: "spring", stiffness: 420, damping: 30 };
export const fastFade: Transition = { duration: 0.15, ease: [0.2, 0, 0, 1] };

/** 页面级进出场:上浮 + 微缩放,像一张卡片从桌面上被拿起又放下 */
export const pageVariants: Variants = {
  initial: { opacity: 0, y: 14, scale: 0.996 },
  animate: {
    opacity: 1,
    y: 0,
    scale: 1,
    transition: { ...springSoft, stiffness: 300, damping: 28 },
  },
  exit: { opacity: 0, y: -6, scale: 0.998, transition: fastFade },
};

/** 弹出物(确认条/小面板):从 0.94 缩放弹入 */
export const popIn: Variants = {
  initial: { opacity: 0, scale: 0.94, y: 8 },
  animate: { opacity: 1, scale: 1, y: 0, transition: springSnappy },
  exit: { opacity: 0, scale: 0.97, y: 4, transition: fastFade },
};

/** 纯淡入(全局装饰件) */
export const fadeIn: Variants = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: { duration: 0.25, ease: [0.2, 0, 0, 1] } },
  exit: { opacity: 0, transition: { duration: 0.18 } },
};

/** 列表级联：按索引错峰入场 */
export function cascade(index: number, stepMs = 45): Variants {
  return {
    initial: { opacity: 0, y: 16 },
    animate: {
      opacity: 1,
      y: 0,
      transition: { ...springSnappy, delay: Math.min(index * (stepMs / 1000), 0.6) },
    },
    exit: { opacity: 0, scale: 0.98, transition: fastFade },
  };
}

/** 抽屉/浮层从右滑入 */
export const drawerVariants: Variants = {
  initial: { x: 48, opacity: 0 },
  animate: { x: 0, opacity: 1, transition: springSoft },
  exit: { x: 32, opacity: 0, transition: fastFade },
};

/* ── v5 新增（CONTRACT-v5 §4；页面层按名 import，勿改实现） ── */

/** 呼吸点：忙态指示的小圆点做透明度/缩放脉冲循环（1.4s 周期） */
export const pulseDot: TargetAndTransition = {
  opacity: [1, 0.4, 1],
  scale: [1, 1.35, 1],
  transition: { duration: 1.4, repeat: Infinity, ease: "easeInOut" },
};

/** 全屏遮罩入场：0.18s 淡入（配合 initial={{ opacity: 0 }}） */
export const overlayIn: TargetAndTransition = {
  opacity: [0, 1],
  transition: { duration: 0.18 },
};

/** 全屏遮罩退场：0.25s 淡出 */
export const overlayOut: TargetAndTransition = {
  opacity: [1, 0],
  transition: { duration: 0.25 },
};

/* ── 收编各处就地手写的 transition 字面量 ── */

/** 清理遮罩内卡入场弹簧（原 CleaningOverlay 私有字面量） */
export const springCard: Transition = { type: "spring", stiffness: 300, damping: 28 };

/** treemap 下钻等布局属性动画：0.28s 标准缓动曲线，禁用弹簧驱动 width/height */
export const layoutEase: Transition = { duration: 0.28, ease: [0.2, 0, 0, 1] };

/** 内容切换 crossfade：0.2s 同步淡入淡出（Crossfade.tsx 消费） */
export const crossfade: Transition = { duration: 0.2, ease: [0.2, 0, 0, 1] };
