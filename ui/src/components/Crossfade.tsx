import { AnimatePresence, motion } from "motion/react";
import { crossfade } from "../lib/motion";

/** 内容切换 crossfade（CONTRACT-v5 §4）：show 翻转时旧内容与新内容同步 0.2s 交叉淡化，
 *  不再是骨架→内容的无条件「啪」跳变。
 *  布局约定：进出两侧恒叠在同一 grid 单元（col/row-start-1）——天然重叠、
 *  容器尺寸取两者最大，切换过程零跳动，退场元素也不会因样式滞留而挤开布局。
 *  可选 fallback 提供「另一侧」内容（如骨架屏）；不给则 show=false 淡出后留空。 */
export function Crossfade({
    show,
    children,
    fallback,
}: {
    show: boolean;
    children: React.ReactNode;
    fallback?: React.ReactNode;
}) {
    const mounted = show || fallback !== undefined;
    return (
        <div className="grid">
            {mounted && (
                <AnimatePresence initial={false} mode="sync">
                    <motion.div
                        key={show ? "content" : "fallback"}
                        className="col-start-1 row-start-1"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={crossfade}
                    >
                        {show ? children : fallback}
                    </motion.div>
                </AnimatePresence>
            )}
        </div>
    );
}
