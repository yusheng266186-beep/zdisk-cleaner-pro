# ZDiskCleaner Pro · 安全空间管理站（v3 从零重构中）

> 磁盘满的时候，人总是先怪自己装了太多东西。其实大多数时候，是缓存悄悄替你做了决定。

v3 是一次**换地基的重造**：从 Python/tkinter 小工具，升级为 **Rust 内核 + Tauri 2 外壳 + React 前端**的安全空间管理站。当前状态：内核、规则库、headless CLI、提权协议与完整前端已交付；待 VS Build Tools 就绪后即可产出安装包。

- 操作清单与验收标准：[REBUILD_V3_PLAN.md](REBUILD_V3_PLAN.md)
- 决策记录：[docs/adr/](docs/adr/) · 规则手册：[docs/rules.md](docs/rules.md) · 性能基准：[docs/benchmarks.md](docs/benchmarks.md)
- 完整版本记录：[CHANGELOG.md](CHANGELOG.md)

## 它凭什么不一样

| 能力 | 说明 | 对比借鉴对象 |
| --- | --- | --- |
| **索引化单遍扫描** | 60 条规则共享一次并行磁盘遍历；实测 28k 文件 3.9s / 内存 ≈17MB | BleachBit 逐规则扫描 |
| **守卫双闸** | 扫描端剔除保护命中 + 执行端 fail-closed 校验（真实路径解析防链接绕过） | 无开源清理器做到 |
| **vault 七天后悔药** | 清理先入台账暂存区，整批/单项可还原——"永久删除"也可反悔 | Czkawka 删除不可逆 |
| **按需最小提权** | 默认无管理员全覆盖；特权操作经一次性 UAC worker（无常驻服务） | v1/v2 整程序提权 |
| **诚实例电磁铁** | 回收站 ≠ 真实释放，口径分列；[bench] 行机器可读杜绝报告造假 | 继承并制度化 |

## 当前形态怎么用

```bash
# 内核 CLI（无 MSVC 环境即可构建运行）
cargo build --release -p zc-cli
target/release/zclean.exe scan                          # 实测扫描，报告落 %LOCALAPPDATA%\ZDiskCleanerPro3\sessions
target/release/zclean.exe apply <report.json> --mode vault    # 暂存区模式清理（--admin 走 UAC 提权批）
target/release/zclean.exe undo <session-id>             # 一键还原 vault 批次
target/release/zclean.exe rules --md > docs/rules.md    # 生成规则手册

# 前端开发（浏览器独立运行，带真机采样 DEMO 数据）
pnpm --dir ui dev

# 基准
powershell scripts/bench.ps1 -Iterations 3 >> docs/benchmarks.md
```

桌面壳 `src-tauri` 的 IPC 契约（8 个命令 + 进度事件）已与前端对齐，安装 MSVC 后 `cargo tauri build` 即出 NSIS 安装包：

```
winget install Microsoft.VisualStudio.2022.BuildTools   # 需管理员，一次性
```

## 技术栈

Rust workspace（zc-core / zc-rules / zc-cli）· Tauri 2 + WebView2 · React 19 + TypeScript strict + Tailwind 4 + motion · SQLite（规划，见 ADR-002）· jwalk/globset/windows-sys/trash 语义自研。

架构决策与替代方案否决记录见 [ADR-001](docs/adr/ADR-001-stack.md)；MFT 直读引擎推迟原因见 [ADR-003](docs/adr/ADR-003-defer-mft.md)。

## 版本史

| 版本 | 主题 |
| --- | --- |
| v2.x | Python/tkinter 时代：48 条规则、Canvas 自绘动效、诚实释放语义（维护冻结） |
| v3.0.0-alpha.1 | Rust 内核 + 60 规则 + 提权 worker + 完整前端设计系统（本轮） |

## 一句话

磁盘不会说话，但它一直在替你记账。这个工具只是帮你看一眼账单，然后把不要的划掉——笔笔可恢复，分分都算数。
