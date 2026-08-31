/** 动效系统唯一出口：弹簧预设与变体工厂。
 *  原则：一切动画可中断、可反向；时长走 token；禁止页面私定缓动。 */

import type { Transition, Variants } from "motion/react";

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
