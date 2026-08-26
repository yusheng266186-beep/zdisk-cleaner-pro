# ZDiskCleaner Pro · 深度磁盘清理

> 磁盘满的时候，人总是先怪自己装了太多东西。其实大多数时候，是缓存悄悄替你做了决定。

ZDiskCleaner Pro 是一款 Windows 深度磁盘清理工具，由 ZDiskCleaner 完全重构而来。它扫描 C 盘上 48 类缓存与垃圾（浏览器、开发工具、聊天应用、系统残留），默认删除进回收站、随时可恢复，并把"到底释放了多少空间"诚实地告诉你。

> 下载：[Releases 发布页](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases) · 完整版本记录：[CHANGELOG](CHANGELOG.md)

## 为什么重写它

原版 ZDiskCleaner 是一个 PyInstaller 打包的 tkinter 小工具，功能方向是对的，但细节经不起推敲：回收站大小统计永远偏小、清空回收站会残留元数据、高分屏下整片模糊、搬家功能会去硬搬必然失败的锁定文件。

于是把它拆开、反编译、逐行分析，然后重写了一遍。重构版保留了原版全部能力，修复了 9 个底层缺陷，并把界面、动画、安全策略全部推倒重来。这个仓库记录的就是这次重写的全部过程。

## 它能做什么

### 深度清理（48 条规则）

覆盖系统 / 浏览器 / 开发工具 / 应用 / 日志五大类：Windows 临时文件、更新缓存、崩溃转储、缩略图缓存、DirectX 着色器，Chrome / Edge / Firefox / Brave / Opera / Vivaldi 缓存，pip / npm / yarn / Gradle / Cargo / Go / JetBrains / Conda / HuggingFace，微信 / QQ / 钉钉 / Slack / Steam / Spotify / Discord，NVIDIA 驱动下载缓存等。

每条规则都带风险徽章（安全 / 低 / 中 / 高），默认只勾选安全项。三种保护默认开启：

- 删除进回收站，随时可恢复；
- 高风险项（如微信聊天文件）默认不勾选；
- 内置自保护路径——程序永远不会清理自己正在运行的文件。

### 程序搬家

把 pip / npm / yarn / Gradle / Maven / Cargo / VSCode 扩展 / HuggingFace 等缓存目录重定向到其他盘：设置环境变量、执行配置命令、迁移现有数据一步完成。Docker 与微信这类运行时锁文件的场景，只给出手动指引而不硬搬。

### 磁盘分析

大文件（阈值可调）、重复文件（大小 → 头部哈希 → 全量哈希三级过滤，支持"保留最新"）、长期未访问文件、目录占用排行。所有结果支持在资源管理器中定位与回收站删除，删除后局部刷新而不是重新全盘扫描。

### 启动项管理

枚举注册表 Run 键的自启动程序，开关式启用 / 禁用，禁用的项被备份保存、随时恢复。

### 系统级占用检测

Windows.old 旧系统、休眠文件 hiberfil.sys、页面文件——这些不能安全自动删除的大块头，会被检测出来并给出处理引导（打开系统设置或复制命令），而不是假装帮你删掉。

### 优化报告

一键生成 Markdown 体检报告：可清理空间统计、分类明细、搬家建议。

## 关于"诚实"的设计

清理工具最容易撒谎的地方是效果数字。ZDiskCleaner Pro 的原则：

- 回收站模式完成后会明确提示"**清空回收站后才会真正释放磁盘空间**"，而不是把数字直接算进战果；
- 历史记录区分"移入回收站"与"真实释放"，仪表盘的累计数字只统计后者；
- 提供"清理后自动清空回收站"选项，一步到位真正释放。

## 使用

从 [Releases](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases) 下载 `ZDiskCleanerPro.exe`，双击即用（单文件，无需安装）。

命令行模式：

```
ZDiskCleanerPro.exe            # 图形界面
ZDiskCleanerPro.exe --scan     # 仅扫描，输出到控制台
ZDiskCleanerPro.exe --cli      # 交互式命令行清理
ZDiskCleanerPro.exe --report   # 生成 Markdown 报告
ZDiskCleanerPro.exe --info     # 查看系统与磁盘信息
```

数据位置：清理历史与设置保存在 `%LOCALAPPDATA%\ZDiskCleanerPro\`。

## 技术栈

Python 3.12 · tkinter（零第三方 UI 依赖）· ctypes Win32 API · PyInstaller 单文件打包（约 12MB）。

界面为纯 Canvas 自绘组件：环形仪表、流光进度条、涟漪按钮、级联入场、椭圆开关、Toast 通知——全部带动画，且在 100%–200% DPI 下比例一致。

## 版本史

| 版本 | 主题 |
| --- | --- |
| v2.0.0 | 完全重构：逆向原版，复刻四大功能，修复 9 个底层缺陷 |
| v2.1.0 | 功能扩展：仪表盘、启动项管理、清理历史、预览模式、排除目录 |
| v2.2.0 | 动效升级：涟漪、级联入场、流光进度、悬停强调 |
| v2.3.0 | 效果与安全：自保护路径、真实释放语义、占用检测、系统级占用 |
| v2.4.0 | 浅色改版：整体配色换新、浅色图标、椭圆开关、高 DPI 适配 |

完整记录见 [CHANGELOG.md](CHANGELOG.md)。

## 一句话

磁盘不会说话，但它一直在替你记账。这个工具只是帮你看一眼账单，然后把不要的划掉——笔笔可恢复，分分都算数。
