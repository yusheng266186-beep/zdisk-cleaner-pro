<div align="center">

# ZDiskCleaner Pro

**Rust 内核的安全空间管理站 · v3.0.0 GA**

[![GA](https://img.shields.io/badge/release-v3.0.0-blue)](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/tag/v3.0.0)
[![tests](https://img.shields.io/badge/tests-46%20passing-brightgreen)](#工程质量)
[![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)

*磁盘不会说话，但它一直在替你记账。这个工具只是帮你看一眼账单，然后把不要的划掉——笔笔可恢复，分分都算数。*

[下载 GA 安装器](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/download/v3.0.0/ZDiskCleanerPro_3.0.0_x64-setup.exe) · [全部 Release](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases) · [规则手册](docs/rules.md)

</div>

---

## 为什么是它

| | |
| --- | --- |
| 🚀 **索引化单遍扫描** | 60 条规则共享一次并行磁盘遍历——实测 28,236 文件 **3.9 秒**、内存峰值 ≈17MB |
| 🛡️ **守卫双闸** | 扫描端剔除保护区命中 + 执行端 fail-closed 真实路径解析（防 subst/符号链接绕过），property fuzz 轰炸 160 变异零放行 |
| ⏪ **vault 七天后悔药** | 清理先入 SQLite 台账暂存区，整批/单项可还原——"永久删除"也可反悔 |
| 🔑 **按需最小提权** | 默认无管理员全覆盖；特权操作走一次性 UAC worker，无常驻服务 |
| 📡 **应用内更新** | Ed25519 签名 + GitHub Releases 通道，拒绝未签名包 |
| 🎯 **诚实口径** | 回收站 ≠ 真实释放，分列计量；进度只来自内核真实阶段事件，拒绝假百分比 |

## 功能全景

- **深度清理**：系统 / 浏览器 / 开发工具 / 应用 / 日志五大域 60 条规则，风险四级，默认只勾安全档
- **空间雷达**：Squarified Treemap 下钻定位大目录，一键「作为迁移源」
- **大文件 / 重复文件**：XXH3 三级哈希管道，组内标注「建议保留最新」
- **存储迁移中心**：junction 搬家工业流程——试运行估算 → robocopy 校验 → `.old` 备份 → 自动回滚保障，五阶段真实进度直播
- **启动项管家**：Run 键禁用即备份，随时一键还原
- **深度工具**：WinSxS 组件清理（DISM 真实百分比）/ 系统还原点 / Windows.old·休眠文件·页面文件引导
- **清理历史**：趋势图 + 台账检索 + 一键还原最近批次

## 快速开始

**安装**：[下载 GA 安装器](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/download/v3.0.0/ZDiskCleanerPro_3.0.0_x64-setup.exe)（4.2MB，需系统 WebView2），双击即用。

**CLI**（无 MSVC 环境亦可构建）：

```bash
cargo build --release -p zc-cli
target/release/zclean.exe scan                          # 实测扫描
target/release/zclean.exe apply <report.json> --mode vault [--admin]
target/release/zclean.exe undo <session-id>             # 一键还原 vault 批次
target/release/zclean.exe tree | dupes | startup | migrate | rules
```

**前端开发**（浏览器独立运行，内置真机采样 DEMO 数据）：

```bash
pnpm --dir ui install && pnpm --dir ui dev
```

## 架构

```
crates/
├─ zc-core   纯逻辑内核：扫描引擎 · 守卫 · 执行器 · 台账(SQLite) · 聚合树 · 去重 · 启动项 · 迁移
├─ zc-rules  60 条内置规则（数据驱动 + 探针）
└─ zc-cli    headless 客户端（内核第一消费者）
src-tauri/   Tauri 2 壳：8+ IPC 命令 · 进度事件桥 · 提权 worker
ui/          React 19 + TS strict + Tailwind 4 + motion：八屏 + 设计系统
```

技术选型与替代方案否决记录：[ADR-001](docs/adr/ADR-001-stack.md) · 数据层演进：[ADR-002](docs/adr/ADR-002-data-layer.md) · MFT 直读推迟决策：[ADR-003](docs/adr/ADR-003-defer-mft.md)

## 工程质量

- **46 个测试全绿**（单元 + 集成 + 夹具 + property fuzz），clippy/tsc 零告警错
- MSVC / GNU 双工具链可验证；`scripts/` 提供基准与打包脚本
- 性能基准公开可复现：[docs/benchmarks.md](docs/benchmarks.md)
- CI：GitHub Actions（Rust core 测试 + UI lint/build）

## 版本史

| 版本 | 主题 |
| --- | --- |
| v1.x | Python + tkinter 小工具（原点） |
| v2.x | Python 全重构：48 规则 · Canvas 自绘动效 · 诚实释放语义 |
| **v3.0.0** | **换地基重造：Rust 内核 + Tauri 2 + React · GA**（[完整过程](CHANGELOG.md)） |

## 安全与隐私

数据只留在本机（`%LOCALAPPDATA%\ZDiskCleanerPro3`），没有云端、没有遥测。删除类操作永远先过守卫；详情见各模块文档与 [docs/rules.md](docs/rules.md)。

## License

MIT
