# 更新日志 CHANGELOG

所有版本的变更记录。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [v5.0.0] - 2026-08-31 · 大版本「可信」——数据安全重构 · 可清理面翻倍 · 全链路闭环

> 本版基于全代码库审计（docs/AUDIT-2026-08-31-全方位评估.md），修复其全部 P0 并落地大部分 P1。

### 数据安全（内核）
- **台账错误不再吞**：`live_manifest_ids/vault_copies/undo_entries` Result 化；孤儿 GC 三保险（名单获取失败整段熔断跳过 / 新建目录 24h 宽限 / 进行中批次不碰）——杜绝台账异常时误删整个暂存区
- **Vault 暂存 journal 化**：先落 `pending` 台账再动手、逐条 `committed`、全败回滚撤账；中途崩溃/掉电不再产生"台账外孤儿→被 GC 删除"的永久丢失路径（历史页对 pending 显示「未完成」警示）
- **批次 id 唯一性**：手工暂存从秒级时间戳改随机熵；台账 upsert 不再 REPLACE 覆盖非空批次
- **Guard 去硬编码**：禁删区由 `%SystemRoot%/%ProgramFiles%/%ProgramData%` 环境变量派生（非 C 系统盘获得同等保护）；USERPROFILE 缺失 → fail-closed 整批拒绝；自保护路径跟随 `ZC_DATA_DIR`
- **提权白名单**：进程已提权时 vet 放行目录级精确白名单（系统 Temp / Windows Update 下载 / 传递优化 / Prefetch / 内核转储 / PerfLogs / 系统级 WER / CBS / WinREAgent 等 15 个窄前缀）——9 条 admin 规则从"永远清不掉"变为真实可用；结构测试强制 admin 规则根 ⊆ 白名单
- **启动项管家**：备份先落盘成功再删注册表值（删失败回滚备份）；REG_EXPAND_SZ 类型保真；新增单条恢复 `enable_one`；备份损坏显式报错不再伪装为空

### 可清理面（60 → 70 条规则）
- **清空回收站**：体检台新卡片（SHQueryRecycleBinW 容量 + SHEmptyRecycleBinW 两段式确认清空，实测释放字节入账）
- 新规则 ×10：系统级 WER、Windows\Logs\DiskDiagnostic、WinREAgent、更新 ReportingEvents.log、WinINet/旧 IE 缓存、Java Deployment 缓存、**pnpm store**（真·大户）、旧版 npm-cache、VS Code 运行缓存、Firefox 崩溃报告；Chrome 扩 Beta/Dev，Brave/Opera 补齐 Code Cache+GPUCache，Vivaldi 多 profile
- 规则路径全面 `%WINDIR%/%SystemDrive%/%TEMP%` 派生（系统盘/Temp 改址用户不再失灵）；`min_age_days` 新维度——Temp/WER 默认只清 7 天前（年龄拿不准保守跳过）；Playwright→Caution、内核转储→Risky

### 性能与正确性
- 扫描引擎 jwalk rayon 并行 + 线程局部自底向上聚合（消灭每文件×深度的全局锁热点），"并行 Walk"名副其实
- 体积树/重复文件/雷达改**实际占用的磁盘簇口径**（GetCompressedFileSizeW）：稀疏文件（WSL/VHDX）不再虚报；硬链接 (volume,file_id) 归并；reparse/云占位文件跳过防触发云端水合
- 大文件/重复文件/体积树查询接入忙任务取消（`cancel_busy`）；SQLite 开 WAL；深目录操作迭代器栈化；暂存复制保 mtime；被占用文件的回收站失败改走"重启后删除"队列（reboot.rs 由死代码接线进链路）

### 壳与 IPC（24 → 29 命令）
- **错误通道结构化**：全命令 `ErrorDto{code,message}`（guard/cancelled/admin_required/not_found/busy/locked/io），前端按 code 分流，中文子串嗅探退役
- undo/purge/migrate_undo 返回结构化结果；migrate_undo 移出主线程；migrate_apply/DISM/还原点在失败路径同样失效雷达体积树缓存；`vault_delete` journal 化且合并双跑实测
- 单实例插件（二次启动聚焦已有窗口）；取消世代槽修 cancel↔reset 竞态；scan 进度监听泄漏修复；sweep/worker/关键操作全部写 zc-app.log；新增 query/empty_recycle_bin、session_entries、startup_list_disabled/enable_one、cancel_busy；scan_now 支持 include_admin（仅提权生效）；CSP 收紧、NSIS 中文、窗口主题不再硬锁

### 界面（闭环与诚信）
- **结果页可再入**（侧栏「体检结果」，report 在就能回）；危险操作言行一致：执行两段式确认 + 非安全档需展开明细后方可勾选
- **历史页**：还原/彻底删除后自动刷新（幽灵行绝迹）；mode 筛选 chips；批次明细下钻（真实台账 entries）；迁移记录可直接从历史页撤销（兑现 v4 toast 承诺）
- 体检台：删除 160MB 假分母（扫描环改诚实不定态）；管理员扫描开关；磁盘环点击直达雷达；清理遮罩不再每 1.4s 重挂（spinner 不重启）、文案诚实、显示已用时长；1.2s 假垫时删除；战报横幅持久化
- 启动项单条恢复；错误不再伪装空态（启动项/深度工具）；重复文件每组可选保留份；大文件行删二次确认；表单 Enter 提交；命令面板打分匹配+焦点陷阱；雷达分区选择器；雷达色块键盘可达与选中环；迁移中心切页不丢进度

### 设计系统 v4（浅色救援 + 动效收编）
- 浅色主题系统性补课：25 组令牌全量双主题、文字用途 `--zc-accent-text/--zc-danger-text`（AA 对比度）、阴影 light 覆盖、`--zc-hover`（导航 hover 浅色不再蒸发）、主题首帧 bootstrap 脚本（消冷启动闪深空）
- 动效词汇表收编：pulseDot/overlayIn/overlayOut/Crossfade 新增、时长令牌真实消费、`.zc-press` 按压态、`.zc-lift` 死码删除、手写渐变复制清零
- 组件：RollNumber 中断续跑（motionValue 直写 DOM）；Toast 关闭钮 + hover 暂停进度线；Ring useId；Treemap hover 容器级事件委托 + memo + 固定时长过渡（tile 级四轴弹簧 layout thrash 终结）

### CLI（自动化就绪）
- `scan --json FILE`、新增 `bigfiles`、`dupes --json`、`sweep --days N`；**退出码约定 0 全成 / 1 错误 / 2 部分失败 / 3 取消**；help 补全全部子命令并删 selftest 幽灵
- 提权链路加固：PowerShell `-EncodedCommand`（免疫引号注入/路径撇号）、结果文件原子写 + nonce 绑定、worker 猝死存活检测（不再干等 15 分钟）、catch_unwind 必写结果、UAC 拒绝→exit 3

### 工程质量
- Rust 测试 47 → **91 全绿**：新增 vault_journal 回归网（目录仅 rename/复制回滚/journal 状态流转/GC 熔断）、guard 提权白名单语义与 8.3 短名/非 C 盘派生、startup 备份往返（`ZC_STARTUP_BACKUP` 注入）、scanner 大小写混排吞并/skipped/min_age、规则-白名单双向一致性、ErrorDto 映射、EncodedCommand
- QA 升级：三套 GUI QA 改**三态计数（PASS/SKIP/FAIL，SKIP 不再记绿）** + 报告头 git SHA/exe 哈希 + 失败自动截图 + CDP 分帧重组/断线重连；启动项用例锁定夹具行（不再误碰用户真实启动项）；新增 **qa_new_features.py**（回收站/分区切换/历史下钻/筛选/取消/错误横幅）与 **qa_cli.py**（ZC_DATA_DIR 隔离的全 CLI 链回归）
- 文档纠偏：rules.md 重生成 70 条、README 重写、CHANGELOG 补齐 v3.0.1–v5.0.0 共 9 个版本、portable/ROADMAP/HANDOVER 同步

## [v4.1.0] - 2026-08-31 · 设计系统 v3「精装版」

- 氛围光深空背景、品牌渐变令牌、三档海拔体系；页面转场微缩放上浮；Toast 状态色章+左缘色条+生命周期进度线；Ring 渐变描边+辉光；侧栏 logo 光晕徽章+品牌发丝线+版本徽章+导航 hover；骨架屏流光(shimmer)替换脉冲；主 CTA 光泽扫过；清理遮罩玻璃化+发丝线；战报横幅精装；滚动条 hover 态+主题切换过渡
- docs/HANDOVER.md 项目交接文档（仓库地图/架构契约/构建测试发布流程/CDP 自动化约定/踩坑实录/上手清单）

## [v4.0.0] - 2026-08-31 · 大版本：每个页面都能安全动手

- **vault_delete 统一链路**：守卫 fail-closed → 暂存区 → 台账可还原；大文件行级「暂存区」、重复文件组级「清理冗余份数」、雷达选中「移入暂存区」、工具箱「安全删除」任意路径入口
- 迁移后台化（store 全局任务，切页不中断，侧栏指示+全局通知）；雷达体积树缓存（二次进入零等待，写操作全量失效）
- 体检台扫描实时耗时+速率；构建计划 codegen-units=1；scripts/qa_v4.py 五项新能力 QA

## [v3.0.6] - 2026-08-31 · 雷达配色重做

- 8 色相低饱和精选调色板（顶层按路径哈希取色，同目录恒定同色形成空间记忆）；下钻子块继承父色相±微漂移成家族色；明度随层级/体量节奏变化；块面纵向渐变+顶部高光+悬浮色相光环；白字加投影保证任何底色可读；缝隙 1px→2px

## [v3.0.5] - 2026-08-31 · 三处真实缺陷修复

- vault 暂存目录被占用时「复制成功+删源失败」不留无账副本——目录只做原子 rename，文件复制失败回滚副本；记账改副本实测字节（活目录扫描→清理窗口增长不再对不上账）；扫描取消令牌生命周期跟随一次扫描（取消过一次不再毒死后续所有扫描）

## [v3.0.4] - 2026-08-31 · 补全「真实释放」链路

- vault 批次可彻底删除（purge_session / zclean purge / 历史页按钮）；7 天后悔期到期自动清扫（sweep+孤儿会话 GC）；副本已不存在按目标达成计不再卡死台账；修复历史页假 session_id（第一行还原/彻底删除永远报台账不存在）；侧栏版本号改读真实应用版本；scripts/qa_drive.py CDP 全功能 QA 驱动

## [v3.0.3] - 2026-08-30 · 迁移与回收站修复

- 迁移 junction 路径修复 + undo 数据搬回 + 回收站逐文件降级（批量被个别锁定文件拖垮时不再整批失败）

## [v3.0.2] - 2026-08-30 · 雷达黑屏修复

- 空间雷达黑屏修复 + 清理链路兜底

## [v3.0.1] - 2026-08-30 · 进度与性能修复

- 修复体检台进度卡 0% 与空间雷达卡死；build_tree 性能重写 4m22s → 1m05s（三段式自底向上聚合）

## [v3.0.0] - GA 正式版

- **GA#5 安全 property fuzz**：确定性 LCG 生成畸形路径（大小写/\?\ 前缀/尾点尾空格/../混合分隔符/UNC 管理共享）
  轰炸守卫——禁删区 160 次变异零放行；良性变体（大小写/分隔符）零误伤；norm 幂等性成立
- **GA#4 winget manifest 就绪**：manifests/ 三件套（installer 校验和占位待 GA 上传后回填）
- 深度工具直达入命令面板； soak 72h 列为运行手册项（hotfix 走 patch 号）
- 全仓测试 46 个全绿（MSVC 链）
## [v3.0.0-beta.6.dev] - GA #2 深度工具卡

- **三张能力卡**：WinSxS 组件清理（DISM /StartComponentCleanup 官方通道）、
  系统还原点（Checkpoint-Computer，描述经单引号转义防注入）、系统级占用盘点
  （Windows.old 存在才实测子树字节；hiberfil/pagefile/swapfile 拿不到就诚实标「未知」
  + 每行「复制指引」直通官方设置路径）；页头安全声明：绝不野删系统文件
- **DISM 真实百分比**：stdout 逐行解析取行内最新百分比（与正则
  `(\d+(?:\.\d+)?)%` 等价的手写扫描，零新增依赖）经 `dism://progress` 推送，
  驱动确定进度条——进度来自真实输出，拒绝伪造
- **提权策略**：dism/还原点命令在未提权时直接 Err（"需要管理员：…"）不再执行，
  本次不做命令内自拉起；提权旁路由 `--dism-worker` / `--rp-worker <desc>`
  main() 前置早退分支承担（UAC 一次授权单进程、不启窗口、结果打印 stdout），
  UI 层引导「以管理员重启应用」或 zclean apply --admin 提权批
- 内核 zc-core::system 新增 `OccupancyItem` + `system_occupancy()`（含单测）；
  侧栏新增「深度工具」（ShieldCheck），工具箱深工卡改「见左侧栏」可点直达
## [v3.0.0-beta.5.dev] - GA 清单 #1：大文件/重复文件页点亮

- **内核与命令**：zc-core 新增 `largest_files`（jwalk 单遍遍历 + BinaryHeap 小顶堆 top-N
  截断，size 降序返回，含单测）；Tauri 壳新增 `big_files`（path 空 = %USERPROFILE%，
  top 夹取 [1,200]，≥1MB 起报）与 `find_dupes`（直连 dedup 三级哈希管道）两命令
- **大文件页**：路径 + Top-N(50/100/200) 表单、骨架加载、序号/体积/路径行式列表、
  桌面壳「定位」直启资源管理器、空结果态口径明示
- **重复文件页**：路径 + 最小 MB 门限，「猎取重复」运行态提示三级哈希管道；顶部横幅
  RollNumber 滚动展示可回收合计（Σ size×(份数-1)）；组卡片标「N 份 × humanSize ·
  建议保留最新」并给出保留建议行；空态贴实跑门限
- CLI（zc-cli dedup 子命令）与 UI 走同源内核管道，浏览器演示态提供同构样例数据
## [v3.0.0-beta.4] - 诚实口径修复 · 真机实测暴露并解决

- **目录命中体积口径修复**：目录级命中在扫描期即聚合整棵子树字节
  （walk 时沿祖先累加），实测预告与实际释放首次完全一致——
  真机清理暴露：预告 423MB / 实搬 1.8GB，修复后含未直接命中子孙
- 真机战报：C 盘可用 2.76GB → 4.24GB（净释放 ≈1.48GB），
  锁定文件（显卡驱动占用的 D3DSCache）优雅跳过并列清单
- 测试 41 全绿（e2e 新增“未命中子孙随目录计入”断言）；测试基础设施新增
  scripts/msvc-test.cmd（bundled sqlite 的 C 链在 MSVC 下全仓可测）
## [v3.0.0-beta.3] - 应用内更新上线

- **tauri-updater 接入**：Ed25519 签名（私钥存库外 %USERPROFILE%\.tauri\，已 gitignore），
  更新通道 = GitHub Releases latest.json；设置页新增「应用内更新」卡
  （检查→发现新版→下载安装，重启生效文案明示）
- 打包链新增签名步骤：scripts/msvc-tauri-build.cmd 由 %USERPROFILE%\.tauri\ 提供密钥环境
- 发布产物三件套：安装器 / .sig 签名 / latest.json
## [v3.0.0-beta.2.dev] - 工具箱 UI 点亮（启动项 + 迁移中心）

- **SQLite 台账迁移（ADR-002 收口）**：清理台账与历史由 JSON/JSONL 迁入单文件
  `ledger.db`（rusqlite 0.32 `bundled` 自带编译——C 编译链走 beta.1 贯通的
  MSVC 工具链，测试统一经 `scripts/msvc-test.cmd`）；首次打开自动导入旧
  `manifests/*.json` 与 `history.jsonl` 并把旧文件改名 `.imported` 留档；
  `CleanManifest::save/load`、`history::append/read_all` 等 zc-core 公共 API
  零破坏，CLI 与 Tauri 壳无需改动；还原条目按台账插入序返回
- **迁移阶段进度事件**：迁移执行改为推送五个真实阶段（复制/尺寸校验/junction/冒烟/清理）
  的 Start/End 事件——进度来自内核真实步骤边界，拒绝伪造百分比；四层贯通：
  zc-core `apply_with_phases` 回调 → zc-cli `[n/5]` 中文阶段行 → Tauri `migrate://phase`
  事件（后台线程）→ UI 阶段文案条与脉冲动效（浏览器演示态同构模拟）
- **启动项管家页**：HKCU Run/RunOnce 表格化呈现，行级禁用（注册表删除+JSON 备份）、
  已禁用计数徽章、「恢复全部」一键还原；IPC 七命令接入 zc-core::startup
- **存储迁移中心页**：form→plan→done 两步向导；计划卡展示源→目标/体积/文件数，
  执行按钮危险红调显式确认，done 态提供「撤销本次迁移」（junction 摘除+原目录复位）；
  migrate_apply 壳层内部重新 plan 防跨参数篡改
- 浏览器开发态样例内存可交互（禁用计数随操作变化）
- 三查复核通过：零硬编码色值违规（1 处品牌渐变白字豁免，与既有主按钮一致）、
  七命令注册齐全、tsc/vite/cargo check 全绿复跑
- **雷达实用化**：treemap 选中节点（点叶子或 Shift+点击任意块）后底部滑入选中条，
  提供「在资源管理器打开」（桌面壳 reveal_in_explorer 直启 explorer.exe，浏览器态隐藏）
  与「作为迁移源」两动作；跨页联动经 store —— activePage 路由提升进全局态，
  选中节点写入 pendingMigrateSrc 后自动切至迁移中心预填源目录，生成计划即清空
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
