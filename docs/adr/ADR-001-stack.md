# ADR-001 · v3 技术栈：Tauri 2 + Rust 内核 + React 前端

状态：已接受 · 2026-08-27

## 背景

v1/v2 为 Python 3.12 + tkinter。四代迭代后暴露结构性上限：GIL 拖慢扫描、Canvas 动效帧率天花板、PyInstaller 启动解压与杀软误报阴影、整程序提权粒度过粗。

## 决策

| 层 | 选型 | 关键理由 |
| --- | --- | --- |
| 外壳 | Tauri 2 | WebView2 系统预装；体积/内存远小于 Electron；官方 NSIS+updater |
| 内核 | Rust workspace `crates/zc-core(-rules/-cli)` | 并行遍历、内存安全、headless 可独立测试；UI 只是内核的第二个消费者（CLI 是第一个） |
| MFT 直读 | `ntfs-reader`/`mft` crate，feature flag 默认关 | 秒级全盘索引的核心来源，风险隔离 |
| 前端 | React 19 + TS strict + Tailwind 4 + motion | 动效工程化能力最强生态 |

## 否决项

- Electron：体积与内存不可接受；Chromium 随包分发徒增攻击面。
- .NET/WPF：动效需要自研一堆 Linux/WASM 不友好组件；跨端路线锁死。
- 继续 Python：扫描速度无解（jwalk 类并行树遍历在 GIL 下无法达标）。

## 环境备注（本机）

本机无 MSVC Build Tools 且非管理员，故本地开发采用：
`rustup gnu 工具链 + llvm-mingw（%~/.zcache）+ RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`
编译 zc-core/zc-rules/zc-cli。Tauri 壳最终打包需 MSVC：
`winget install Microsoft.VisualStudio.2022.BuildTools` 后去掉本地覆盖即可。
CI 的 rust-core job 同样用 gnu 跑核心测试，保证两端一致。
