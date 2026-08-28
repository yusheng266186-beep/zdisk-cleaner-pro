# ZDiskCleaner Pro v3 · 从零完全重构操作清单

> 状态：**beta.4 达成**（2026-08-28）——距 GA `v3.0.0` 仅剩清单见下。
> - ✅ beta.1~4：MSVC 贯通 / 六件套 UI / 阶段进度事件 / SQLite 台账 / 应用内更新 / 口径修复
> - ⏳ GA 清单：大文件与重复文件页 UI · 深度工具卡(WinSxS/还原点) · 动效走查 pass ·
>   winget manifest · 安全校验 property fuzz · soak · 正式 tag `v3.0.0`
> - 🏆 `target/release/zdiskcleaner-pro.exe` = 11.14MB（≤12MB 验收线达成）
> - ✅ 全部内核/规则/CLI/提权/前端/Tauri IPC 就绪；35 测试全绿 · clippy/tsc 零告警错
> - ⏳ 仅剩 NSIS 安装包一步：在 GitHub 可达的终端跑 `scripts/msvc-tauri-build.cmd`
> - 六件套 UI（启动项/迁移向导）与 SQLite 台账迁移为 beta.2 内容
>
> 原则：不沿用 v1/v2 的架构思路；只借鉴开源项目的**思想**而非代码；每一项都要比被借鉴者做得更好。

---

## 0. 一句话定位

把「一个能删文件的列表工具」重造为一台 **索引化的安全空间管理站**：秒级扫描、笔笔可撤销、按需最小提权、GPU 级流畅动效。

## 1. 现状诊断：为什么必须换思路，而不是继续修 v2

v2（Python 3.12 + tkinter + PyInstaller 单文件）已到天花板：

| 维度 | v2 现状 | 根因 |
| --- | --- | --- |
| 扫描速度 | 百万文件级目录冷扫分钟级 | Python GIL + ThreadPoolExecutor 上限 |
| 进度反馈 | 个别页面曾出现假进度 | 语言层无法廉价做事件流 |
| 动效上限 | Canvas 自绘 ~20–30fps、改动成本极高 | tkinter 无合成器、无硬件加速 |
| 启动速度 | 单文件 exe 首启需解压，明显等待 | PyInstaller onefile 固有缺陷 |
| 兼容性阴影 | 曾因缺元数据被杀软误报 | PyInstaller 打包特征 |
| 撤销能力 | 只有"删进回收站"，永久删除不可逆 | 无 staging/vault 概念 |
| 提权模型 | 整个程序要求管理员 | 粒度过粗、日常使用门槛高 |

**五个根本性思路转变（这是 v3 的灵魂）：**

1. **规则清单 → 智能体检**：不止罗列"能删什么"，而是给出 0–100 的可删除评分（大小 × 可再生性 × 影响面），首页呈现磁盘健康分。
2. **删文件 → 带暂存的撤销系统**：所有清理先入 manifest 台账；回收站模式之外新增 **vault 暂存区**（保留 7 天可一件全部还原），让"永久删除"也可反悔。开源界没有清理工具做到这一点。
3. **每次全量扫 → 索引化扫描**：常规并行遍历打底，NTFS 盘提供 **MFT 直读极速模式**（管理员可选），叠加 USN 增量刷新——重扫秒级。
4. **整体提权 → 按需最小提权**：默认无管理员运行即可覆盖 90% 能力；特权操作做成"能力卡"，点击才拉起一次性提权 worker，UAC 拒绝不影响其余功能。
5. **手绘控件 → Web 动效系统**：WebView2 + GPU 合成，弹簧物理、可中断过渡、虚拟列表——动画质量与开发效率全面换代。

## 2. 技术选型

| 层 | 选型 | 理由 |
| --- | --- | --- |
| 外壳 | **Tauri 2** | 体积小（装机 ~40MB vs Electron 150MB+）、 WebView2 Win10/11 预装、官方 updater、NSIS/MSI 打包成熟 |
| 内核 | **Rust（独立 crate `zc-core`）** | 并行遍历（jwalk/rayon）、内存安全、可脱离 UI 独立做 CLI 与测试 |
| MFT | `ntfs-reader` / `omerbenamram/mft` | 纯安全 Rust 解析 $MFT；**特性开关，默认关闭** |
| 前端 | TypeScript + Vite + React 18 + Tailwind CSS | 生态最厚、招式现成 |
| 动效 | motion（原 framer-motion）+ CSS spring | 可中断弹簧动画、共享布局、手势 |
| 大数据列表 | TanStack Virtual | 百万行虚拟滚动 |
| 状态 | zustand + TanStack Query | 轻量、事件订阅天然契合 IPC 流 |
| 本地存储 | SQLite（rusqlite） | 清理台账 / 历史 / 索引缓存 / 规则命中统计 |
| 其他 | `trash`（回收站）、`xxhash-rust`（去重）、`tracing`（日志）、`windows` crate | 均为主流维护中的轮子 |

> SquirrelDisk（Rust+Tauri+React 的 WinDirStat 替代品）已验证这条技术路线在生产上可行；我们在此基础上补齐它没有的"清理 + 安全 + 撤销"闭环。

```
仓库布局
ZDiskCleanerPro/
├─ src-tauri/          # Tauri 壳 + IPC 命令 + 事件桥
│  ├─ crates/zc-core/  # 纯 Rust 内核：扫描/评分/安全/执行/台账
│  └─ crates/zc-rules/ # 规则加载与内置规则集(YAML 数据)
├─ ui/                 # React 前端（design-system/pages/lib）
├─ docs/               # benchmarks、决策记录 ADR、规则手册
└─ scripts/            # 基准、打包、真机冒烟清单
```

## 3. 借鉴来源与超越点（务必逐条兑现）

| 借鉴项目 | 借什么 | 我们怎么超越 |
| --- | --- | --- |
| BleachBit | 按应用组织的清理规则模型 | 类型化规则注册表（YAML 数据 + Rust 探针）、四维风险分级、0–100 评分、预览即所见、全链路可撤销——BleachBit 无撤销、无进度流、无评分 |
| Czkawka | 去重三级管道（大小→预哈希→全量哈希） | XXH3 + 有界并发 I/O、结果按目录树分组展示、与清理引擎一体（勾选即撤销式删除）——Czkawka 删除后无法找回 |
| WizTree / c²flux（闭源） | 直读 MFT 秒级全盘的开源化 | 用 `ntfs-reader` 实现同等速度，且叠加"可删性评分"——WizTree 只告诉你在哪，不敢告诉你哪些能删 |
| Everything / USN Journal | 变更日志增量索引 | 扫描索引 SQLite 化 + USN 增量刷新，热重扫 < 2s |
| WinDirStat / SpaceSniffer / SquirrelDisk / DaisyDisk | treemap 可视化 | GPU 平滑缩放、下钻共享元素动画、右键直接进清理流程、中文细节打磨 |
| Steam Mover / FreeMove | junction 目录搬家 | 增加：试运行估算 → 占用进程检测 → robocopy 迁移 + 抽样校验 → `.old` 备份 → 自动回滚 → 配置自动重写（环境变量/注册表/Known Folders API）一条龙，而不是只搬不管后 |
| Windows Storage Sense / DISM | 系统级清理走官方通道 | WinSxS 用 `DISM /StartComponentCleanup` 包装成进度可视化的"深度工具卡"，绝不野删 |

## 4. 核心子系统设计

### 4.1 扫描子系统（扫描路径大升级）

- **规则五大域、目标 ≥60 条**（v2 为 48 条）：System / Browsers / Dev / Apps / Logs·Crashes。新增方向：包管理器全家桶补充（pnpm store、uv、pnpm cache、Bazel、Maven wrapper）、Unity/Godot 编辑器缓存、JetBrains 全家桶旧版本残留、浏览器多 Profile 遍历、Teams/OBS/WhatsApp/Electron 应用通用 `%APPDATA%\*\Cache*` 通配探针、NVIDIA/AMD 着色器与安装缓存、Windows 错误报告队列、传递优化文件。
- **双引擎**：
  - Walk 引擎：jwalk 多线程遍历；跳过 reparse point 防循环、识别 OneDrive 按需文件属性不触碰内容、长路径 `\\?\` 前缀兼容。
  - MFT 极速引擎（feature flag，仅管理员 + 仅本地 NTFS 卷）：直读 `$MFT` 秒建全卷索引；任何异常自动降级 Walk 引擎并在 UI 说明原因。
- **索引层（P2 增强）**：扫描结果落 SQLite，下次扫描走 USN 差量，指数级提速。
- **真实进度**：枚举期上报"已发现 N 项 / 累计字节滚动估计"，计算期按阶段占比合成总进度——机制上杜绝假进度。

### 4.2 安全子系统（比所有对标项目多一层）

五道防线 + 一个后悔药：

1. 规则内置守护 glob（如 NuGet 规则不带全局 packages 目录）；
2. 全局禁删区：`C:\Windows`、`Program Files*`、`ProgramData` 核心子树、用户文档类目录——resolve 真实路径后再比对（防 subst/链接绕过）；
3. 自保护：本程序安装目录、数据目录、WebView2 用户数据文件夹一律排除；
4. 占用检测：清理前快照运行中应用，锁定文件自动转入"重启后删除"队列（`MoveFileEx MOVEFILE_DELAY_UNTIL_REBOOT`）而不是失败弹窗；
5. 四级风险（安全/注意/风险/专家），默认只勾"安全"，专家项需逐条展开二次确认；
6. **后悔药**：清理台账记录每批操作的路径/大小/散列摘要；回收站模式可借 Windows 回收站还原，vault 模式保留原件 7 天，一键全量还原或单项还原。

**诚实例电磁铁（继承 v2 口碑并制度化）**：仪表盘"累计释放"只计真实释放；回收站滞留额单独一行显示并提醒。

### 4.3 清理执行器

- `TrashBatch`（SHFileOperationW + FOF_ALLOWUNDO 批量提交）、`VaultBatch`（移入受管 vault 目录，SQLite 记账）、`PermanentBatch`（vault 过期后物理清除，SSD 只走 TRIM 语义不做假擦除；HDD 提供覆写擦除选项）。
- `DismExecutor`：后台进程跑 `DISM /Online /Cleanup-Image /StartComponentCleanup`，解析 stdout 百分比行变成真进度条。
- `RestorePointExecutor`：高风险清理前调 VSS API 建系统还原点（独访问卡，管理员）。
- 回收站清空：SHEmptyRecycleBinW + 强制二次确认 + 显示本次涉及清单。

### 4.4 权限体系（按需最小提权）

- 启动即探测权限态；每条规则声明 `required_privilege: user | admin`。
- 能力卡机制：需要管理员的操作在 UI 上是独立的"钥匙卡"；点击 → 拉起一次性提权 worker（`Start-Process -Verb RunAs`，JSON 参数经 stdin 传入、进度经 stdout 流回、结束即退出）。UAC 被拒 → 卡片转为说明文档态，主程序一切照常。
- 不提供"整个程序以管理员运行"的诱导；计划任务常驻提权方案仅作 P3 高级选项，默认关闭。

### 4.5 存储迁移中心（"项目搬家"全面升级）

三类迁移器统一流水线：

- **env 型**：pip/npm/yarn/cargo/go/hf-home 等环境变量重定向（写入用户环境变量 + WM_SETTINGCHANGE 广播 + 已验证的目标盘校验）。
- **junction 型**：VSCode 扩展、游戏库、微信/QQ 数据目录——试运行（dry-run 给出迁移体积/耗时估计）→ 占用进程检测并请求关闭 → robocopy `/MT /COPY:DAT /DCOPY:DAT` 迁移 → 尺寸+随机抽样哈希校验 → 源目录改名 `.old` → 建立 junction → 目标读写冒烟 → 通过后清理 `.old`；**任一步失败自动逆向回滚**，数据无损。
- **系统型**：Known Folders API 改置文档/下载/桌面位置；WSL vhdx 导出再导入指引卡；Steam 库管理跳转卡。

## 5. UI 设计语言 · 交互 · 动画

### 5.1 设计基调：「深空驾驶舱」

- 双主题跟随系统（深空为默认）：`--bg #0B0E14` / `--surface-1 #12161F` / `--surface-2 #181E2A` / 描边 rgba 白 8%；品牌渐变 靛蓝 `#6366F1` → 青 `#22D3EE`，成功绿 `#34D399`、警示琥珀 `#FBBF24`、危险玫瑰 `#FB7185`；文字层级 `#F2F5FA / #B7C1D3 / #7C879A`，对比度全部 ≥ AA。
- 字体：MiSans / HarmonyOS Sans SC（中文）+ Inter（西文数字）双栈；数字统一 tabular-nums 防抖动。
- 图标：lucide-react 全套线性图标，关键操作配自定义像素级点缀。
- 质感约束：玻璃拟态只用于悬浮层（命令面板、Drawer），内容卡片用纯色分层 + 1px 描边阴影，克制而有层次。

### 5.2 信息架构（导航重组）

```
体检台 Home —— 健康分主环 + 一键智能体检 CTA + 各分区可释放速览 + 回收站滞留提示
  ├─ 体检结果 —— 分类卡片瀑布（级联入场）→ 抽屉下钻规则明细/文件列表
  ├─ 清理进行 —— 真实进度流 + 已释数字滚动 + 战报收束动画
  └─ 历史 —— 释放趋势图 / 台账检索 / 一键还原最近一次
工具箱 Hub —— 空间雷达(treemap) · 大文件 · 重复文件 · 存储迁移中心 · 启动项管家 · 深度工具(WinSxS/还原点/休眠文件引导)
设置 —— 主题/排除目录/风险偏好/vault 策略/日志导出
```

### 5.3 动效系统（工程化，不是点缀）

- 统一规格：微交互 120–160ms（standard ease-out）；面板/页面 240–320ms spring(stiffness 300, damping 30)；庆祝时刻 ≤ 800ms；**一切动画可中断、可反向**；全局尊重 `prefers-reduced-motion`。
- 关键机会点清单（实施阶段逐一落实并评审）：
  1. 健康分环：SVG stroke-dashoffset 由 motion value 驱动，扫描中随进度脉冲呼吸；
  2. 数字滚动 rollup（released bytes 用千分位格式化插值）；
  3. 分类卡片按可释放体积排序级联入场（stagger 40ms，FLIP 保持布局稳定）;
  4. treemap 下钻：选中块放大成为新根的共享布局动画，面包屑可返回；
  5. 清理执行：行级流式战报（最新在上，旧的淡出），完成时统计卡聚拢收缩成一个徽章飞入历史页；
  6. 骨架屏与空态插画（空态也是状态，不许白屏）；
  7. Ctrl+K 命令面板毛玻璃入场 + 结果键盘导航；
  8. 列表拖选、悬停强调条、Toast 队列滑动接管——细节手感三件套。
- 性能红线：交互全程 60fps（120Hz 屏自适应）；虚拟列表保证 100 万行滚动不掉帧；动画只用 transform/opacity 合成属性。

## 6. 从零施工操作清单（Phase 0 → 11，每步含验收 DoD）

**Phase 0 · 定位冻结**
- [ ] 本文档评审定稿；写出与 BleachBit/Czkawka/WizTree/v2 的对标表进 `docs/comparison.md`
- [ ] 定死验收指标（见 §9 表格）

**Phase 1 · 工程脚手架**
- [ ] create-tauri-app 初始化 `src-tauri` + `ui`；`crates/zc-core`、`crates/zc-rules` workspace 拆分
- [ ] pnpm + Vite + TS strict + ESLint/Prettier + clippy/fmt/deny；rust-toolchain 固定版本
- [ ] GitHub Actions：lint/test（Rust+前端）+ Windows 构建产物（NSIS 安装包、MSI、portable zip）artifact
- [ ] tracing 日志落地 `%LOCALAPPDATA%\ZDiskCleanerPro3\logs`（轮转）
- ✅ DoD：空壳应用双击可开；CI 全绿；三个安装产物均可装

**Phase 2 · 内核 zc-core**
- [ ] 数据模型：Rule/ScanSession/Finding/CleanManifest/RiskScore
- [ ] Rule trait + YAML 数据驱动加载（zc-rules crate）+ 探针函数（注册表、env、known folder）
- [ ] jwalk 并行 Walk 引擎（reparse 防循环、占位文件属性过滤、取消令牌、事件回调）
- [ ] 安全守卫模块（禁删区 resolver、自保护、守护 glob、风险计算）
- [ ] 执行器：TrashBatch/VaultBatch/PermanentBatch/PendingReboot 队列
- [ ] SQLite：台账/历史/设置迁移
- [ ] headless CLI 小入口（`zc-core-cli scan --json`）供调试与基准
- ✅ DoD：安全模块单测覆盖率 ≥85%；fixtures 正反用例全过；CLI 可对临时树完成扫描→预览→撤销全链路

**Phase 3 · 规则库 v3**
- [ ] 编写 ≥60 条规则 YAML + 逐条真机实测（记录样本大小截图进 `docs/rules/`）
- [ ] 每条规则配套 fixture 测试（应有命中 + 不应误伤）
- ✅ DoD：规则手册生成脚本产出全量文档；误伤用例为零

**Phase 4 · 扫描性能**
- [ ] 基准脚本（冷/热缓存、用户目录与全 C 盘两场景）入 `docs/benchmarks.md`
- [ ] IPC 事件流打平：core → tauri event → 前端 zustand store，节流合并
- [ ] MFT 极速引擎（feature flag 默认关）：读 $MFT → 内存索引 → 与规则匹配输出 Findings；异常降级路径
- ✅ DoD：冷扫用户目录 ≤20s、内存峰值 ≤500MB；取消 ≤200ms 生效；前端全程不卡

**Phase 5 · 权限与提权**
- [ ] 权限探测 + 能力卡数据结构 + elevated-worker（stdin/stdout 协议 + 超时 + 杀进程兜底）
- [ ] DISM 与还原点两个代表性特权操作实机打通；UAC 拒绝/超时分支文案
- ✅ DoD：非管理员模式全功能无崩溃；两个特权操作真机验证；拒绝 UAC 后主程序不受影响

**Phase 6 · 设计系统与前端骨架**
- [ ] Design tokens（色板/字号/间距/圆角/阴影/动效时长与弹簧参数）单文件出口
- [ ] 15 个基础组件：Button/IconButton/Card/Badge(Risk)/Ring/Toggle/Slider/VirtualList/TreeView/Drawer/Modal/Toast/Tooltip/Skeleton/ContextMenu + 命令面板
- [ ] 路由与壳层（侧栏指示条弹性滑动、页面共享布局切换）；明暗主题切换；DPI 适配走查
- ✅ DoD：组件 Gallery 页全部验收；125%/150%/200% DPI 截图走查通过；主题切换无缝

**Phase 7 · 主流程五屏**
- [ ] 体检台 / 体检结果 / 清理进行 / 历史台账 / 设置，接通真实内核数据
- [ ] 全部空态/加载态/错误态；首次运行引导（安全声明 + dry-run 默认开启说明）
- ✅ DoD：真机完整走查「打开→体检→下钻→清理→查看战报→还原一批」，数字口径全程诚实一致

**Phase 8 · 工具箱六件套**
- [ ] 空间雷达：squarified treemap canvas 渲染 + 共享元素下钻 + 与清理流程互通
- [ ] 大文件 / 重复文件（XXH3 三级管道、并发限流、结果树分组、撤销式删除）
- [ ] 存储迁移中心（三类迁移器流水线 + 回滚演练脚本）
- [ ] 启动项管家（Run/RunOnce/启动文件夹，禁用即备份可还原）
- [ ] 深度工具卡（WinSxS DISM 进度可视化、还原点、hiberfil/页面文件/Windows.old 引导卡）
- ✅ DoD：每件套独立验收用例；去重结果与手工哈希对拍一致；迁移中心断电中断模拟回滚成功

**Phase 9 · 动效打磨 pass**
- [ ] 按 §5.3 机会点清单逐项实现与自审（对齐弹簧物理、可中断原则）
- [ ] 性能剖析：React profiler 消灭重渲染风暴；核显低配机实测录像
- ✅ DoD：动效审评清单全绿；60fps 达标；`prefers-reduced-motion` 生效

**Phase 10 · 打包发布**
- [ ] NSIS 安装器（可选便携 zip）；tauri-updater 接 GitHub Releases 更新通道
- [ ] 版本资源 + 图标 + 声明文档（降低杀软误报面）；README 全面重写（对标表、指标、规则手册链接）
- [ ] winget manifest 提交准备
- ✅ DoD：干净 Win10 21H2 与 Win11 虚拟机安装/更新冒烟通过；升级链路演练成功

**Phase 11 · QA 与验收**
- [ ] 安全属性测试：property-based 生成畸形路径/符号链接/subst 盘，验证禁删区永不被触碰
- [ ] 72h soak + 崩溃日志收集；v2 对照效果表发布进 README
- [ ] tag `v3.0.0`
- ✅ DoD：§9 指标全部实测达标并附证据

## 7. 里程碑切分

| 里程碑 | 包含 Phase | 标志 |
| --- | --- | --- |
| v3.0.0-alpha.1 | 1–5 | headless 内核 + CLI 可用，性能达标 |
| v3.0.0-alpha.2 | 6–7 | 主流程 UI 可日常使用 |
| v3.0.0-beta.1 | 8 | 六件套齐活，功能超 v2 全集 |
| v3.0.0-rc | 9 | 动效定稿 |
| v3.0.0 | 10–11 | 发布 |

相对工作量：Phase 2 ≈ 25%，Phase 3 ≈ 15%，Phase 6–8 合计 ≈ 35%，其余 ≈ 25%。

## 8. 风险登记册

| 风险 | 应对 |
| --- | --- |
| 直读 $MFT 被安全软件拦/系统限制 | feature flag 默认关闭；降级 Walk 引擎；文档明示前提 |
| WebView2 Runtime 缺失（老旧系统） | 安装器内置 bootstrapper；启动检测给下载指引 |
| 杀软误报复发 | 版本资源+图标+说明页；后续评估代码签名（自签/证书双路径） |
| 规则误伤用户数据 | Phase 11 property 测试 + 首启默认 dry-run + vault 七天后悔药三层兜底 |
| Rust/Tauri 学习曲线 | zc-core 先以 CLI 形式独立跑通（可与 UI 并行开发），缩小联调面 |
| 范围蔓延 | beta 前功能冻结；Anything else 进 backlog 文档 |

## 9. 验收指标（对 v2 与对标产品的硬指标）

| 指标 | v2 实况 | v3 目标 |
| --- | --- | --- |
| 用户目录冷扫（~百万文件） | 分钟级 | ≤ 20s |
| 热重扫（有索引） | 全量重扫 | ≤ 2s |
| 扫描内存峰值 | 未知（未测） | ≤ 500MB，界面进程 ≤ 200MB |
| 交互动效帧率 | ~20–30fps Canvas | 60fps+（120Hz 自适应） |
| 冷启动到可交互 | 数秒（解压） | ≤ 1.5s |
| 安装体积 | 单文件 ~12MB（另需 Python 运行时解压过程） | 安装包 ≤ 12MB、装机占用 ≤ 45MB |
| 撤销能力 | 仅回收站模式 | 台账 + vault 7 天，任意批次可还 |
| 规则数量 | 48 | ≥ 60，全部带正反测试 |
| 误伤事故 | 靠人工谨慎 | property 测试零容忍 + 后悔药兜底 |
| 提权粒度 | 整程序 | 操作级能力卡 |

## 10. 参考资料（借鉴思想的来源）

- BleachBit：<https://github.com/bleachbit/bleachbit>
- Czkawka：<https://github.com/qarmin/czkawka>
- SquirrelDisk（Tauri 路线先例）：<https://github.com/adileo/squirreldisk>
- rust-mft（$MFT 解析）：<https://github.com/omerbenamram/mft>
- ntfs-reader（MFT 快扫 + USN）：<https://lib.rs/crates/ntfs-reader>
- WizTree（MFT 速度参照，闭源）：<https://diskanalyzer.com/>
