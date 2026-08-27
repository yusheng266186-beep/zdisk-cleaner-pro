/** 空间雷达数据契约：与内核 zc_core::analyze::TreeNode 同构（serde 蛇形直传，无映射层）。 */

export interface TreeNode {
    name: string;
    /** 归一化绝对路径（小写正斜杠），与内核 patterns::norm 同口径 */
    path: string;
    size: number;
    files: number;
    dirs: number;
    children: TreeNode[];
}

/** 子节点体积之和 —— 占比计算 / 校验「父 size ≥ 子和」的小工具 */
export function totalOf(n: TreeNode): number {
    return n.children.reduce((a, c) => a + c.size, 0);
}

/* ── 浏览器开发模式的内置样本（真机布局采样的确定性副本）─────────────── */

const GB = 1024 ** 3;

type Seed = {
    name: string;
    path: string;
    size?: number;
    files?: number;
    dirs?: number;
    children?: Seed[];
};

function grow(s: Seed): TreeNode {
    const children = (s.children ?? []).map(grow);
    const childSum = children.reduce((a, c) => a + c.size, 0);
    return {
        name: s.name,
        path: s.path,
        size: Math.max(s.size ?? 0, childSum),
        files: s.files ?? 0,
        dirs: s.dirs ?? children.length,
        children,
    };
}

/** 形状仿真实机器：根 C: 下 7 个一级目录，每个再挂 2~4 个二级；
 *  Users/yusheng 一支的 AppData/Local 是大头，与真机扫描样本同源。 */
export const SAMPLE_TREE: TreeNode = grow({
    name: "C:",
    path: "c:/",
    size: 381.6 * GB,
    files: 1_842_306,
    dirs: 184_113,
    children: [
        {
            name: "Users",
            path: "c:/users",
            size: 187.4 * GB,
            files: 963_114,
            dirs: 41_207,
            children: [
                {
                    name: "yusheng",
                    path: "c:/users/yusheng",
                    size: 185.7 * GB,
                    files: 961_002,
                    dirs: 40_865,
                    children: [
                        {
                            name: "AppData",
                            path: "c:/users/yusheng/appdata",
                            size: 108.9 * GB,
                            files: 512_387,
                            dirs: 28_902,
                            children: [
                                {
                                    name: "Local",
                                    path: "c:/users/yusheng/appdata/local",
                                    size: 82.3 * GB,
                                    files: 396_241,
                                    dirs: 21_134,
                                    children: [
                                        {
                                            name: "Microsoft",
                                            path: "c:/users/yusheng/appdata/local/microsoft",
                                            size: 38.6 * GB,
                                            files: 148_902,
                                            dirs: 8_236,
                                        },
                                        {
                                            name: "Google",
                                            path: "c:/users/yusheng/appdata/local/google",
                                            size: 16.2 * GB,
                                            files: 62_441,
                                            dirs: 3_117,
                                        },
                                        {
                                            name: "Temp",
                                            path: "c:/users/yusheng/appdata/local/temp",
                                            size: 13.8 * GB,
                                            files: 84_119,
                                            dirs: 1_244,
                                        },
                                        {
                                            name: "AMD",
                                            path: "c:/users/yusheng/appdata/local/amd",
                                            size: 9.4 * GB,
                                            files: 20_375,
                                            dirs: 402,
                                        },
                                    ],
                                },
                                {
                                    name: "Roaming",
                                    path: "c:/users/yusheng/appdata/roaming",
                                    size: 21.3 * GB,
                                    files: 89_003,
                                    dirs: 5_881,
                                    children: [
                                        {
                                            name: "discord",
                                            path: "c:/users/yusheng/appdata/roaming/discord",
                                            size: 8.4 * GB,
                                            files: 31_206,
                                            dirs: 986,
                                        },
                                        {
                                            name: "Microsoft",
                                            path: "c:/users/yusheng/appdata/roaming/microsoft",
                                            size: 11.2 * GB,
                                            files: 44_887,
                                            dirs: 3_302,
                                        },
                                    ],
                                },
                                {
                                    name: "LocalLow",
                                    path: "c:/users/yusheng/appdata/locallow",
                                    size: 2.6 * GB,
                                    files: 9_118,
                                    dirs: 417,
                                },
                            ],
                        },
                        {
                            name: "Downloads",
                            path: "c:/users/yusheng/downloads",
                            size: 28.9 * GB,
                            files: 1_942,
                            dirs: 66,
                        },
                        {
                            name: "repos",
                            path: "c:/users/yusheng/repos",
                            size: 41.0 * GB,
                            files: 302_441,
                            dirs: 8_772,
                        },
                        {
                            name: "Desktop",
                            path: "c:/users/yusheng/desktop",
                            size: 4.2 * GB,
                            files: 1_106,
                            dirs: 128,
                        },
                    ],
                },
                {
                    name: "Public",
                    path: "c:/users/public",
                    size: 1.3 * GB,
                    files: 214,
                    dirs: 26,
                },
            ],
        },
        {
            name: "Windows",
            path: "c:/windows",
            size: 89.6 * GB,
            files: 486_772,
            dirs: 78_450,
            children: [
                { name: "WinSxS", path: "c:/windows/winsxs", size: 41.2 * GB, files: 121_038, dirs: 31_204 },
                { name: "Installer", path: "c:/windows/installer", size: 17.6 * GB, files: 8_902, dirs: 12 },
                { name: "Fonts", path: "c:/windows/fonts", size: 2.4 * GB, files: 1_204, dirs: 4 },
                { name: "Temp", path: "c:/windows/temp", size: 3.1 * GB, files: 14_118, dirs: 306 },
            ],
        },
        {
            name: "Program Files",
            path: "c:/program files",
            size: 52.9 * GB,
            files: 214_883,
            dirs: 22_104,
            children: [
                { name: "Windows Apps", path: "c:/program files/windowsapps", size: 24.5 * GB, files: 88_442, dirs: 9_118 },
                { name: "Common Files", path: "c:/program files/common files", size: 6.8 * GB, files: 32_107, dirs: 2_043 },
                { name: "Google", path: "c:/program files/google", size: 12.4 * GB, files: 24_991, dirs: 1_186 },
            ],
        },
        {
            name: "Program Files (x86)",
            path: "c:/program files (x86)",
            size: 6.9 * GB,
            files: 42_106,
            dirs: 5_003,
            children: [
                { name: "Common Files", path: "c:/program files (x86)/common files", size: 3.2 * GB, files: 18_554, dirs: 1_764 },
                { name: "Internet Explorer", path: "c:/program files (x86)/internet explorer", size: 1.1 * GB, files: 2_088, dirs: 92 },
            ],
        },
        {
            name: "ProgramData",
            path: "c:/programdata",
            size: 18.5 * GB,
            files: 96_208,
            dirs: 12_887,
            children: [
                { name: "Microsoft", path: "c:/programdata/microsoft", size: 8.9 * GB, files: 51_337, dirs: 7_209 },
                { name: "Package Cache", path: "c:/programdata/package cache", size: 5.6 * GB, files: 12_441, dirs: 1_033 },
                { name: "NVIDIA", path: "c:/programdata/nvidia", size: 2.8 * GB, files: 6_802, dirs: 418 },
            ],
        },
        {
            name: "$Recycle.Bin",
            path: "c:/$recycle.bin",
            size: 6.5 * GB,
            files: 1_077,
            dirs: 18,
            children: [
                { name: "S-1-5-18", path: "c:/$recycle.bin/s-1-5-18", size: 0.3 * GB, files: 12, dirs: 2 },
                {
                    name: "S-1-5-21-1004336348",
                    path: "c:/$recycle.bin/s-1-5-21-1004336348",
                    size: 5.9 * GB,
                    files: 1_064,
                    dirs: 15,
                },
            ],
        },
        {
            name: "Temp",
            path: "c:/temp",
            size: 11.2 * GB,
            files: 8_442,
            dirs: 401,
            children: [
                { name: "logs", path: "c:/temp/logs", size: 1.4 * GB, files: 6_213, dirs: 92 },
                { name: "builds", path: "c:/temp/builds", size: 9.8 * GB, files: 2_228, dirs: 304 },
            ],
        },
    ],
});
