# 更新日志 CHANGELOG

所有版本的变更记录。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [v3.0.0-beta.1] - MSVC 贯通 · 应用二进制产出

- **壳层编译贯通**：VS Build Tools VCTools 工作负载补装后，tauri 全依赖树
  （600+ crates）在 MSVC 下编译通过；`cargo check -p zdiskcleaner-pro` exit=0
- **Release 二进制产出**：`target/release/zdiskcleaner-pro.exe` =
  **11,682,816 bytes ≈ 11.14 MB**（验收线 ≤12MB 达成；此前任一版本不可比）
- 修复 `scan_now` 嵌套 Result 的错误转换（`??` → 显式 map_err）
- 工具脚本：`scripts/msvc-check-shell.cmd`、`scripts/msvc-tauri-build.cmd`
  （vcvars 包裹链，cargo 与 tauri.cmd 全自动接线）
- ⏳ NSIS 安装包：打包器下载 nsis-3.11.zip 在本沙箱网络超时（GitHub 直连受限）。
  在可正常访问 GitHub 的终端执行
  `pnpm --dir ui exec tauri build` 或直接运行 `scripts/msvc-tauri-build.cmd` 即可产出安装包；
  已预构建 exe 无需重编译（增量）
- 前端新增「空间雷达」页（子代理交付）：Squarified Treemap 下钻/面包屑，
  算法经独立复算验证（覆盖率 100%、零重叠）

## [v3.0.0-beta.wip] - 六件套内核四连发

### 新增（zc-core 新模块）
- **analyze · 空间雷达数据源**：单遍并行目录聚合树，深度裁剪 + 每层前 N 大折叠，
  `zclean tree <dir> --depth N [--json]` 真机实测（用户目录 49GB 一级分布即出）
- **dedup · 重复文件猎手**：大小 → 64KB XXH3 预哈希 → 全量 XXH3-128 三级管道，
  rayon 并行；`zclean dupes <dir...> --min-mb N` 输出分组并标注「保留→最新」建议
- **startup · 启动项管家**：HKCU Run/RunOnce 枚举；禁用 = 注册表删除 + JSON 备份，
  enable-all 全量恢复；`zclean startup [list|disable|enable-all]`
- **migrate · 存储迁移中心**：junction 引擎——plan 试运行（体积/文件数与磁盘感知）
  → apply（robocopy /MT 迁移 → 尺寸校验 → 源改 .old → mklink /J → 冒烟读测，
  任一步失败自动逆向回滚）→ undo 兜底摘链复原；
  `zclean migrate plan|apply --yes|undo`

### 质量
- Rust 测试 29 → **35 个全绿**（新增 analyze 聚合/折叠、dedup 命中与排除、
  startup 枚举健壮性等用例）；clippy 保持零告警
- 真机冒烟：startup 列出真实 Run 键、tree 用户目录 49GB 分布、dupes 夹具命中、
  migrate plan 对 D 盘试运行（只读）全部通过

## [v3.0.0-alpha.1] - 从零重构 · 内核与规则库可用

架构彻底换代：Python/tkinter → Rust 内核（zc-core）+ Tauri 2 外壳 + React 前端。
操作清单与决策记录见 `REBUILD_V3_PLAN.md` 与 `docs/adr/`。

### 新增（内核）
- **单遍多 glob 并行扫描引擎**（jwalk）：全部规则共享一次磁盘遍历，
  取代 v2「每规则各走一遍树」；支持取消令牌与真实字节进度事件
- **fail-closed 安全守卫**：禁删区 canonicalize 后比对（防 subst/链接绕过）、
  解析失败即拒绝整批、自保护目录、父目录命中吞并子项（根治 v1 家族重复统计缺陷）
- **vault 暂存区**：清理先入受管台账，7 天内可整批/单项还原——永久删除也可反悔
- **回收站批删**（SHFileOperationW FOF_ALLOWUNDO）与**重启后删除队列**执行器
- **60 条内置规则**（五大域，Phase 3 下限达成）：守卫双闸设计——扫描端
  `filter_guards` 剔除落保护区的命中，执行端 fail-closed 再校验一遍；
  admin_required 标记与目标一致性由测试强制；权限采用进程令牌 TokenElevation 检测
- **数据目录可重定向**（ZC_DATA_DIR）：集成测试与便携模式零 AppData 污染
- headless CLI `zclean`：scan / apply / undo / show / rules（`--md` 生成规则手册）

### 工程
- Cargo workspace（default-members 排除 Tauri 壳）：无 MSVC 的机器也能开发测试内核
- CI（GitHub Actions）：Rust core (gnu) 测试 + UI lint/build 双 job
- 前端脚手架：React 19 + TS strict + Tailwind 4 + motion + zustand + 虚拟列表，
  设计令牌「深空驾驶舱」v1 入库

### 本轮追加（同 alpha.1 批次内）
- **提权 worker 协议（Phase 5）**：`elevated-run` 子命令 + `run_elevated` 启动器——
  任务 JSON 落盘 → `powershell Start-Process -Verb RunAs` 一次性拉起同 exe →
  fail-closed 守卫照常过闸 → 结果 JSON 回写。UAC 拒绝/超时均有显式语义；
  worker 拒绝非提权进程重放；CLI `apply --admin` 已接入
- **前端设计系统与五屏交付（Phase 6/7 主体）**：
  「深空驾驶舱」双主题令牌 v2、弹簧动效预设出口（page/cascade/drawer）、
  健康环 Ring（motion value 驱动 stroke 追赶）、数字滚动 RollNumber、
  风险徽章、Toast 队列、Ctrl+K 毛玻璃命令面板；
  体检台（扫描脉冲环 + 实时字节进度 + 取消）、结果页（体积降序级联卡片 +
  勾选发光 + 明细抽屉 + 底部悬浮操作条）、执行浮层（流光条 + 规则名轮换）、
  历史（趋势条形图 + 一键还原最近批次）、工具箱六件套占位卡（灰卡不装死按钮）、
  设置页策略锁定展示；侧栏指示条 layoutId 弹性滑动 + 页面共享布局转场 +
  全局尊重 prefers-reduced-motion
- **Tauri 壳 IPC 就绪**：8 个命令（scan_now 事件推送/cancel_scan/clean_selected/
  undo_session/rules_meta/history_list/drives_overview/ping）契约与前端口径一致，
  待 MSVC 即可打包验证
- **工程健康线**：29 个 Rust 测试全绿 · clippy --all-targets 零告警 ·
  tsc strict 零错 · vite 生产构建 372KB(gzip 121KB)

### 实测
- 本机实扫（60 规则全量）：16,418 文件遍历 2.1s，清理发现 151.56MB / 2,600 项，
  其中新增规则贡献约 80MB（AMD 着色器缓存、pnpm/uv/node-gyp 等）
- 单元 + 集成 + 夹具测试 26 个全绿；clippy --all-targets 零告警

## [v2.5.0] - 深色回归

### 回退
- 整体配色回归深色系（侧栏 #11141C / 内容 #0D1017 / 卡片 #161A24）
- 应用图标回归深色版（蓝紫渐变磁盘环 + 绿色扫除点）
- 标题栏回归深色沉浸模式（Win11 DWM）
- 按钮 / 开关 / 进度条 / 环形仪表 / 滚动条等组件配色同步回归

### 说明
- 保留 v2.3.0 / v2.4.0 的全部功能修复：自保护路径、回收站真实释放语义、
  应用占用检测、系统级占用检测、搬家逻辑、历史口径、分析页局部刷新等
- 保留提升对比度后的浅灰文字（次级 #B7C1D3 / 弱化 #98A2B8），深底清晰可读

## [v2.4.0] - 浅色改版

### 改版
- 整体配色从纯黑深色切换为清爽浅色主题（浅蓝灰底 + 纯白卡片 + 品牌蓝），更贴合"洁净"的工具气质
- 应用图标同步重绘为浅色底 + 描边轮廓，白底桌面清晰可见
- 标题栏同步浅色（Win11 DWM 定制底色与文字色）
- 启动项开关改为椭圆造型

### 修复
- **自保护路径**：PyInstaller 运行时解压目录（%TEMP%\_MEIxxxx）不再被"用户临时文件"规则扫描/清理，杜绝误删自身导致崩溃
- 程序搬家：Docker（WSL 虚拟盘）与微信（运行时锁定文件）不再尝试自动迁移，只做重定向设置 + 手动指引
- 清理历史：新增 `real_freed` 字段，"累计释放"只统计真实释放（永久删除或含清空回收站），不再夸大战果
- 磁盘分析：手动删除文件后局部刷新结果，不再触发全盘重扫

## [v2.3.0] - 效果与安全

### 新增
- **回收站真实释放语义**：回收站模式完成后明确提示"清空回收站后才会真正释放磁盘空间"
- **清理后自动清空回收站**选项：一步到位真正释放空间
- **应用占用检测**：清理前检测 Chrome / Edge / 微信 / QQ / Steam 等 16 类应用是否正在运行（进程快照），锁定文件提前告知
- **系统级占用检测**（仪表盘）：Windows.old、休眠文件 hiberfil.sys、页面文件 pagefile.sys，附处理引导（打开系统设置 / 复制命令）
- 清空回收站前二次确认（含本次移入的文件与历史删除项）

### 变更
- 清理规则 46 → 48：新增 NVIDIA 驱动下载缓存、JetBrains 索引缓存；Chrome/Edge 并入 Crashpad 崩溃报告
- 目录遍历改为 os.scandir 栈式实现（Windows 下枚举自带文件属性，冷缓存时每文件少一次系统调用）
- 安全校验加固：禁删清单补上 C:\Windows 本身

## [v2.2.0] - 动效升级

### 新增
- 页面滑动切换动画与侧栏指示条弹性滑动
- 按钮点击涟漪扩散效果
- 扫描完成后规则行级联淡入（按占用排序逐行错峰）
- 清理进行中确定进度条叠加流光
- 规则行悬停强调条
- 仪表盘数字滚动、磁盘环形仪表动画

## [v2.1.0] - 功能扩展

### 新增
- **仪表盘**：磁盘环形占用仪表、累计释放/清理次数/回收站占用/内存四统计卡、系统信息、快速操作、清理提醒（超 7 天未清理）
- **启动项管理**：注册表 Run 键枚举、开关式启用/禁用、禁用项备份可恢复
- **清理历史**：本地 JSON 持久化，仪表盘可追溯
- **预览模式（dry-run）**：只统计将删除什么，不实际删除
- **一键智能清理**：自动扫描 → 勾选安全项 → 清理
- **排除目录**：自定义目录不扫描不清理，设置持久化
- **重复文件保留最新**策略（按修改时间）

## [v2.0.0] - 完全重构

基于对原版 ZDiskCleaner 的逆向分析（PyInstaller 解包 + 字节码反编译）完全重写。

### 保留并增强原版四大功能
- 深度清理：46 条规则（原版 27 条），风险分级，回收站/永久删除双模式
- 程序搬家：缓存目录重定向到其他盘（环境变量 + 配置命令 + 数据迁移）
- 磁盘分析：大文件 / 重复文件（三级过滤）/ 旧文件 / 目录占用排行
- 优化报告：Markdown 体检报告
- 命令行模式：--scan / --cli / --report / --info

### 修复原版 9 个底层缺陷
1. 回收站大小统计改用 SHQueryRecycleBinW（原版遍历 $Recycle.Bin，目录项大小恒为 0）
2. 清空回收站改用 SHEmptyRecycleBinW（原版逐个删 $R 文件，残留 $I 元数据）
3. 搬家命令盘符替换失效（原版双反斜杠搜索串永不匹配）
4. SHFileOperationW 批量提交（原版逐文件调用，慢 10 倍+）
5. 规则级并行扫描（原版串行）
6. Per-Monitor-V2 DPI 感知（原版高分屏模糊）
7. 扫描实时进度回调（原版无反馈）
8. NuGet 规则不再误删全局包目录
9. GBK 控制台 Unicode 崩溃
