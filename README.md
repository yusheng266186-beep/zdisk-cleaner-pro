<div align="center">

# ZDiskCleaner Pro

**Rust 内核的安全空间管理站 · v5.0.0**

[![Release](https://img.shields.io/badge/release-v5.0.0-blue)](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/tag/v5.0.0)
[![tests](https://img.shields.io/badge/tests-91%20passing-brightgreen)](#工程质量)
[![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)

*磁盘不会说话，但它一直在替你记账。这个工具只是帮你看一眼账单，然后把不要的划掉——笔笔可恢复，分分都算数。*

[下载 v5.0.0 安装器](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/download/v5.0.0/ZDiskCleanerPro_5.0.0_x64-setup.exe) · [便携版](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/download/v5.0.0/ZDiskCleanerPro-Portable-v5.0.0.zip) · [全部 Release](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases) · [规则手册](docs/rules.md)

</div>

---

## 为什么是它

| | |
| --- | --- |
| 🚀 **并行单遍扫描** | 70 条规则共享一次 rayon 并行遍历，目录聚合走线程局部三段式——无每文件全局锁 |
| 🛡️ **守卫双闸 + 提权白名单** | 扫描端剔除保护区命中 + 执行端 fail-closed 真实路径解析（防 subst/链接绕过）；禁删区由环境变量派生（非 C 系统盘同样受保护），property fuzz 轰炸零放行；提权后按 15 个目录级窄前缀白名单放行系统级清理，其余 Windows 树照旧整批拒绝 |
| 📼 **journal 化暂存区** | 清理先入 SQLite 台账（先记 pending 再动手，逐条 committed），7 天后悔期整批/单项可还原；台账异常时 GC 自动熔断——中途崩溃不会产生"台账外孤儿被误删"；**历史页可查看每批真实明细** |
| ♻️ **回收站一等公民** | 体检台直接查看/清空系统回收站（前后 quota 实测差值入账），不再只是"送进回收站" |
| 🔑 **按需最小提权** | 默认无管理员全覆盖；特权批走一次性 UAC worker（-EncodedCommand 免疫注入、nonce 绑定、结果原子回写、worker 猝死检测） |
| 📡 **应用内更新** | Ed25519 签名 + GitHub Releases 通道（latest.json），拒绝未签名包 |
| 🎯 **诚实口径** | 体积按磁盘簇实测（稀疏/压缩文件不虚报）、硬链接/云占位防误判防脱水、被拒目录计入 skipped、overflow 单列不计入可清理合计；1.2s 假垫时删除，"可切走"谎言修正 |

## 功能全景（10 个导航页）

- **体检台**：战报横幅+一键反悔、不定态扫描环+实时耗时速率、磁盘环直达雷达、**回收站卡**、管理员扫描开关（提权后解锁系统级规则）
- **深度清理**：系统/浏览器/开发/应用/日志/网络六大域 **70 条规则**（含 Windows Update、传递优化、pnpm store、微信系 IM 缓存面），风险四级 + `min_age` 年龄过滤，默认只勾安全档
- **空间雷达**：Squarified Treemap 下钻（可选分区/主目录为根、可取消）、选中「移入暂存区」、一键「作为迁移源」
- **大文件 / 重复文件**：Top-N 与大体积降序、重复组 **自选保留份**、行级/组级暂存删除、可取消
- **存储迁移中心**：junction 搬家工业流程——计划估算 → robocopy 校验 → `.old` 备份 → 自动回滚；后台化切页不中断；**历史页可撤销**
- **启动项管家**：Run/RunOnce 枚举（REG_SZ+EXPAND_SZ），禁用即备份、**单条恢复**、备份损坏显式报错
- **深度工具**：WinSxS 组件清理（DISM 真实百分比）/ 系统还原点 / Windows.old·休眠·页面文件盘点与引导
- **清理历史**：趋势图 + mode 筛选 + 批次明细下钻 + 还原/彻底删除闭环
- **工具箱 / 命令面板**（Ctrl+K）：安全删除任意路径（守卫+暂存+台账）、全局导航与操作

## 快速开始

**安装**：下载 [v5.0.0 安装器](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/download/v5.0.0/ZDiskCleanerPro_5.0.0_x64-setup.exe)（需系统 WebView2），双击即用；或取便携 zip 解压运行。升级走应用内更新（设置页）。

**CLI**（无 MSVC 环境亦可构建内核测试；出 exe 需 VS Build Tools）：

```bash
cargo build --release -p zc-cli
target/release/zclean.exe scan --json report.json      # 扫描（--admin 纳入系统级规则）
target/release/zclean.exe apply report.json --mode vault [--admin]
target/release/zclean.exe undo <session-id>            # 还原
target/release/zclean.exe purge <session-id>           # 彻底删除
target/release/zclean.exe sweep --days 7               # 过期清扫+孤儿 GC
target/release/zclean.exe bigfiles C:\ --top 50 --json # 大文件
target/release/zclean.exe dupes D:\ --min-mb 50        # 重复文件
target/release/zclean.exe rules --md                   # 重生成规则手册
```

退出码约定：`0` 全部成功 / `1` 错误 / `2` 部分失败 / `3` 取消。

**前端开发**（浏览器独立运行，内置真机采样 DEMO 数据）：

```bash
pnpm --dir ui install && pnpm --dir ui dev
```

## 架构

```
crates/
├─ zc-core    纯逻辑内核：并行扫描 · fail-closed 守卫(提权白名单) · 执行器(trash/vault+重启队列)
│             journal 台账(SQLite/WAL) · 暂存区 GC 三保险 · 体积树 · 去重 · 启动项 · 迁移 · 回收站 · 系统盘点
├─ zc-rules   70 条内置规则（数据驱动 + guards + min_age，结构测试强制守卫一致性）
└─ zc-cli     headless 客户端（内核第一消费者）
src-tauri/    Tauri 2 壳：29 IPC 命令 · 结构化错误码 · 进度事件桥 · 提权 worker · 单实例
ui/           React 19 + TS strict + Tailwind 4 + motion：设计令牌双主题 · 动效词汇表
```

技术选型与否决记录：[ADR-001](docs/adr/ADR-001-stack.md) · 数据层演进：[ADR-002](docs/adr/ADR-002-data-layer.md) · MFT 直读推迟：[ADR-003](docs/adr/ADR-003-defer-mft.md) · 接手必读：[docs/HANDOVER.md](docs/HANDOVER.md)

## 工程质量

- **91 个 Rust 测试全绿**（单元 + 集成 + 夹具 + property fuzz + vault_journal 安全回归网），clippy / tsc 零错
- QA 五套脚本：`qa_drive`(12) GUI 主流程 / `qa_edge`(8) 边界 / `qa_v4`(5) 新能力 / `qa_new_features`(6) v5 冒烟 / `qa_cli` 无头全链——**三态计数（PASS/SKIP/FAIL）**、报告头带 git SHA 与 exe 哈希、失败自动截图
- MSVC / GNU 双工具链可验证；`scripts/msvc-*.cmd` 包装构建；性能基准见 [docs/benchmarks.md](docs/benchmarks.md)
- CI：GitHub Actions（Rust core 测试 + UI lint/build）；发布四资产（setup.exe / .sig / latest.json / 便携 zip）命名与通道由脚本校验

## 版本史

| 版本 | 主题 |
| --- | --- |
| v1.x | Python + tkinter 小工具（原点） |
| v2.x | Python 全重构：48 规则 · Canvas 自绘动效 · 诚实释放语义 |
| v3.0.x | 换地基重造：Rust 内核 + Tauri 2 + React · GA + 六个 hotfix |
| v4.0–4.1 | 每个页面都能安全动手（vault 统一链路）· 设计系统「精装版」 |
| **v5.0.0** | **「可信」大版本**：数据安全 journal 化 · 70 规则+回收站 · 浅色救援 · 全链路闭环（[完整记录](CHANGELOG.md)） |

## 安全与隐私

数据只留在本机（`%LOCALAPPDATA%\ZDiskCleanerPro3`），没有云端、没有遥测。删除类操作永远先过守卫（真实路径解析 + 禁删区 + 白名单目录级精确匹配）；所有暂存动作台账可查可还原。详见 [docs/rules.md](docs/rules.md) 与 [docs/HANDOVER.md](docs/HANDOVER.md)。

## License

MIT
