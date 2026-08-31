# ZDiskCleaner Pro — 项目交接文档（HANDOVER）

> 读者：接手本项目的 AI / 开发者。目标：30 分钟内建立全局认知，1 小时内能在本机构建、运行、调试、跑完整 QA。
> 口径说明：本文只描述"项目有什么、东西在哪、怎么用、环境里有哪些坑"，不包含改进建议。

---

## 1. 项目是什么

ZDiskCleaner Pro 是一个 **Windows 磁盘清理工具**，当前版本 **4.1.0**：

- **技术栈**：Rust workspace（核心逻辑）+ Tauri 2（桌面壳）+ React 19（前端，zustand 状态 + motion/react 动画 + Tailwind 4 + lucide 图标）。
- **产品形态**：桌面 GUI 应用（NSIS 安装包 + 免安装便携 zip）+ 无头 CLI（`zclean`）+ 自动更新通道（GitHub Releases `latest.json` + Ed25519 签名）。
- **安全模型**：所有删除动作先经**守卫（guard）fail-closed 审核**，再进入两条执行路径之一——系统回收站（trash）或应用自管暂存区（vault，台账可还原，7 天后悔期）。台账（ledger）是还原/彻底删除的唯一事实来源。
- **远程仓库**：`github.com/yusheng266186-beep/zdisk-cleaner-pro`（main 分支即发布分支）。

---

## 2. 仓库地图

```
ZDiskCleanerPro/
├── Cargo.toml               # workspace 定义；[workspace.package].version 是全仓版本号
├── crates/
│   ├── zc-core/             # 纯逻辑内核（无 UI 依赖）：
│   │                        #   scanner(扫描+取消令牌) / rules(规则引擎) / guard(fail-closed 守卫)
│   │                        #   executor(trash|vault 两种执行模式) / vault(暂存区 stash/undo/过期清扫)
│   │                        #   ledger(台账:manifests/entries/history) / manifest(执行清单+purge)
│   │                        #   migrate(目录迁移:junction 计划/应用/撤销)
│   ├── zc-rules/            # 内置清理规则定义（sys-user-temp、edge-cache、sys-dx-shader、sys-thumbnails 等）
│   └── zc-cli/              # zclean.exe 无头客户端（scan/apply/undo/purge/vault/sweep/show/rules）
├── src-tauri/               # Tauri 2 壳：全部 IPC 命令在 src/lib.rs；tauri.conf.json（版本/更新通道/打包）
├── ui/                      # React 前端（pnpm；vite 构建）
│   └── src/
│       ├── pages/           # Home(体检台)/Radar(空间雷达)/BigFiles/Duplicates/MigrateCenter/Tools/History/Settings
│       ├── components/      # TreemapCanvas(空间雷 treemap)/Ring/ToastStack/CleaningOverlay 等
│       ├── store.ts         # zustand 全局状态；含 window.__zcStore 调试句柄（自动化测试依赖它）
│       ├── lib/ipc.ts       # invoke 封装；lib/motion.ts 动效词汇表；lib/format.ts humanSize 等
│       └── styles/global.css# 设计令牌（--zc-* CSS 变量体系），主题 dark/light
├── scripts/
│   ├── msvc-*.cmd           # MSVC 构建包装（见 §5，必须走包装脚本）
│   ├── qa_drive.py          # GUI 主流程 QA（11 步）
│   ├── qa_edge.py           # 边界 QA（8 项）
│   └── qa_v4.py             # 新能力 QA（5 项）
├── docs/                    # ROADMAP / rules.md / benchmarks.md / soak-log.md / adr/(ADR-001~003)
├── portable/                # 便携包说明 README-PORTABLE.txt
└── target/release/          # 构建产物（注意：target 在仓库根，不在 src-tauri/ 下）
```

三篇 ADR 值得先读：`docs/adr/ADR-001-stack.md`（选型）、`ADR-002-data-layer.md`（台账/数据层）、`ADR-003-defer-mft.md`（扫描范围决策）。规则语义在 `docs/rules.md`。

---

## 3. 架构与核心概念

### 3.1 一次"清理"的完整链路

```
扫描(scanner, 可取消) → 规则匹配(zc-rules) → 用户勾选(UI) →
守卫审核(guard, fail-closed: 任何拿不准的路径一律拒绝) →
执行器(executor, mode=trash|vault) →
  vault 模式: 暂存(vault::stash) + 写台账(CleanManifest) + 追加 history → 通知前端
  trash 模式: 送系统回收站
```

**vault 暂存的两条硬性实现约束**（改代码必须保持）：
- **目录**只允许**原子 rename** 进暂存区——绝不做"复制+删除"（跨盘/中断会把数据劈成两半，且产生台账外的孤儿副本）。
- **文件**是"复制→删源"，复制成功但删源失败时必须**回滚删除暂存副本**（禁止台账外的无主副本）。

**计量口径**：台账/暂存区/UI 三方字节一致，靠的是执行后用 `actual_size(dst)`（目录=子树求和）重新实测入账，而不是用扫描时刻的快照。任何新写入暂存区的路径都要沿用这个口径。

### 3.2 台账（ledger）是唯一的还原事实来源

- SQLite 文件：`%LOCALAPPDATA%\ZDiskCleanerPro3\ledger.db`，三张表：
  - `manifests`：一次清理批次（id 形如 `1788161892-74f23cf0` 或 `manual-<ts>`）；
  - `entries`：批内每条记录（列：`manifest_id, origin, vault_rel, size`）；
  - `history`：给"历史记录"页展示的操作流水。
- **还原（undo）**：把暂存副本搬回 origin；**彻底删除（purge）**：删暂存副本 + 抹台账行（不可逆）；**sweep**：清扫超过 7 天后悔期的批次，并 GC 没有台账引用的孤儿会话目录。
- 暂存区物理位置：`%LOCALAPPDATA%\ZDiskCleanerPro3\vault\<session-id>\`，目录名即批次 id。

### 3.3 前后端集成约定（改动时必须遵守的既有契约）

- **雷达体积树缓存**：`analyze_tree` 命令有 10 分钟 TTL 的进程内缓存；**所有会产生写操作的 IPC 命令结尾都要调用 `analyze_cache_invalidate()`**（现有 `clean_selected / undo_session / purge_session / vault_delete / migrate_apply / migrate_undo` 均已接入）。新增写命令时漏掉这一步，雷达页会显示过期数据。
- **扫描取消令牌**：`ScanHandle` 每次扫描开始时会 `reset()`。生命周期语义：一个句柄贯穿"请求→进行→取消"，取消只在扫描进行中有效。
- **迁移（migrate）**：`plan → apply_with_phases(带阶段回调, emit "migrate://phase") → junction 切换`；`migrate_undo` 摘 junction 并把 `.old` 备份复位回源目录。前端把迁移做成了全局后台任务（切页不中断，侧栏有指示）。
- **IPC 命令清单**全在 `src-tauri/src/lib.rs`（`#[tauri::command]`），DTO 定义同文件，新增命令记得在 `invoke_handler` 注册。
- **版本号三处口径**：`Cargo.toml [workspace.package].version` 与 `src-tauri/tauri.conf.json version` 是发版版本（改版必须同步这两处）；`ui/package.json` 的 version 独立存在，不参与发版。

---

## 4. 环境要求

| 组件 | 说明 |
|---|---|
| Rust (stable-x86_64-pc-windows-msvc) | `C:\Users\yusheng\.cargo\bin`，包装脚本会自动注入 PATH |
| Node + pnpm | ui 依赖安装用 `pnpm install`（在 ui/ 下）；node 路径已在包装脚本里 |
| Visual Studio Build Tools (MSVC) | 只有打包 Tauri 壳需要；纯逻辑 crate 的 `cargo test` 不装 VS 也能跑（workspace 的 `default-members` 不含 src-tauri，就是为此） |
| Python 3 | 跑 QA 脚本（仅标准库，无第三方依赖） |
| gh CLI | 已登录，用于 release 管理 |

**Shell 纪律（每条命令开头都要做）**：
```bash
unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy ALL_PROXY all_proxy
```
本机环境带代理变量，不清掉会连累 cargo/git/npm/gh 的网络访问，症状是各种"连接重置/EOF"。

---

## 5. 构建与运行

### 5.1 构建

```bash
# 完整发布构建（UI + Tauri + NSIS + updater 工件）——必须走包装脚本注入 MSVC 环境：
cmd //c scripts\\msvc-tauri-build.cmd
# 产物：
#   target/release/zdiskcleaner-pro.exe          主程序
#   target/release/zclean.exe                    CLI（注意：见下方"陈旧 zclean"坑）
#   target/release/bundle/nsis/*-setup.exe       安装包
#   updater 签名工件（.sig / latest.json 相关）随 createUpdaterArtifacts 生成

# 只构建 CLI（不涉及 Tauri）：
C:/Users/yusheng/.cargo/bin/cargo.exe build --release -p zc-cli

# 前端单独构建/开发：
cd ui && pnpm install && pnpm build      # 或 pnpm dev 起 vite
```

### 5.2 运行

```bash
# 普通启动：直接双击或
./target/release/zdiskcleaner-pro.exe

# 自动化/调试启动（开 CDP 调试端口 9223）：
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223" ./target/release/zdiskcleaner-pro.exe
```

### 5.3 测试

```bash
# ① 纯逻辑单元测试（不需要 MSVC 环境）：
cargo test
# 完整（含 Tauri 壳链接）：
cmd //c scripts\\msvc-test.cmd

# ② GUI 全量 QA（先按 5.2 的 CDP 方式启动应用，再跑）：
python scripts/qa_drive.py    # 主流程 11 步（含真实扫描与一次真实清理）
python scripts/qa_edge.py     # 边界 8 项（取消扫描/空选守卫/清理→撤销闭环/刷新持久化…）
python scripts/qa_v4.py       # 能力 5 项（安全删除/行级暂存/组级清冗余/雷达暂存/后台迁移）
# 报告落在 C:\Temp\zc-qa-*.json（带唯一时间戳，不会互相覆盖）

# ③ CLI 冒烟（无 GUI 验证内核链路）：
./target/release/zclean.exe scan
./target/release/zclean.exe show <REPORT>
./target/release/zclean.exe vault <PATH>...      # 手动安全删除（守卫+暂存+台账）
./target/release/zclean.exe undo <SESSION-ID>    # 还原批次
./target/release/zclean.exe purge <SESSION-ID>   # 彻底删除批次
./target/release/zclean.exe sweep                # 清扫过期批次+孤儿 GC
```

三套 GUI QA **不要并发跑**（共享一个 CDP 端口和应用实例，夹具与台账会互相干扰）。

---

## 6. CDP 自动化方法（QA 脚本的原理与约定）

`scripts/qa_drive.py` 顶部有一个可直接复用的最小 CDP 客户端（原始 WebSocket 实现，无第三方依赖），要点：

1. **连接**：`http://127.0.0.1:9223/json/list` 找 `type=="page"` 的目标 → 取 `webSocketDebuggerUrl`。应用必须以 §5.2 的环境变量方式启动，否则端口不存在。
2. **状态断言**：生产构建没有 `window.__TAURI__`，读内部状态一律走 `window.__zcStore`（zustand store 调试句柄，store.ts 里挂的）。
3. **UI 操作**：优先 `document.querySelector(...).click()` / 合成事件，配合 `Runtime.evaluate` 轮询断言。脚本里有现成的 `goto / wait_expr / click_text / native_set_input / probe`（探针=重负载期间求值往返延迟，卡顿断言用它）。
4. **导航等待**：页面切换是 `AnimatePresence mode="wait"`，过渡约 0.4s，导航后必须等新页挂载再查询（脚本已处理，自己写新用例时注意）。
5. **窗口可见性**：跑自动化期间保持应用窗口不被遮挡——WebView2 对被遮挡窗口会节流 rAF，一切依赖动画时序的 DOM 断言都会失真。
6. **DPR 坑**：应用运行在 devicePixelRatio=2 下，CDP 的 `Input.dispatchMouseEvent` 屏幕坐标会被按 CSS 像素解释（点击整体偏移）。需要精确点击元素时用**元素级合成事件**（如 tile 上派发 `new MouseEvent('click', {shiftKey:true, bubbles:true})`），不要用原始鼠标坐标。
7. **受控输入**：给 React 受控输入赋值要用"原生 setter + 派发 input 事件"，且**查询条件必须写成谓词本体**（`(i) => ...` 的函数体直接内联进模板），不要外面再包一层箭头函数——包一层会让所有输入命中同一个元素（脚本注释里有记录）。

已知文案/结构锚点（写用例直接用）：
- 体检台主按钮文案是「**开始智能体检**」（页面 H1「磁盘体检，一键开始」是标题）；
- 空间雷达的 treemap 色块是 **div** 不是 canvas；
- purge 部分失败（文件被占用）的 toast 文案：「已删除 N 项，M 项未能删除…」。

---

## 7. 发布流程（每次发版照抄）

1. **升版本**：`Cargo.toml [workspace.package].version` + `src-tauri/tauri.conf.json version`，两处同步（如 4.1.0 → 4.1.1）。
2. **签名环境**（updater 工件需要）：私钥 `~/.tauri/zdiskcleaner.key`，口令在 `~/.tauri/password.txt`，构建前设
   `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（内容别进仓库/日志）。
3. **构建**：`cmd //c scripts\\msvc-tauri-build.cmd`；如 CLI 也有改动，**显式** `cargo build --release -p zc-cli`（见 §9 坑 T3）。
4. **提交 + tag**：`git add -A && git commit && git tag v<版本>`。
5. **推送与发布**：`git push origin main --tags`（用已存储凭据或 gh；**任何令牌不得写入仓库与文档**）。然后
   `gh release create v<版本>` 上传 **4 个资产，名字必须精确**：
   - `ZDiskCleanerPro_<版本>_x64-setup.exe`
   - `ZDiskCleanerPro_<版本>_x64-setup.exe.sig`
   - `latest.json` ← **就这个名字**，更新通道按文件名拉取，多一个后缀都会让全量用户升级失败
   - `ZDiskCleanerPro-Portable-v<版本>.zip`（便携包，`portable/README-PORTABLE.txt` 里的版本与数据目录说明要同步更新）
6. **通道校验**：`curl https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/latest/download/latest.json`，确认 `version` 字段 = 新版本、`url` 可下载。
7. **桌面便携目录**：如需更新桌面上的免安装副本，先确认应用没有在运行（exe 被运行实例锁定时文件替换会失败），再从便携 zip 解开覆盖。

`docs/soak-log.md` 是 GA 后 72h 稳定性巡检记录（崩溃数/台账健康/C 盘余水），由定时自动化逐日追加。

---

## 8. 运行时数据与调试入口速查

| 东西 | 位置 / 方法 |
|---|---|
| 应用数据目录 | `%LOCALAPPDATA%\ZDiskCleanerPro3\`（ledger.db、vault\、会话报告） |
| 台账直查 | `python -c "import sqlite3; ..."` 查 `ledger.db`（表与列见 §3.2） |
| CLI 看历史报告 | `./target/release/zclean.exe show <REPORT>` |
| 前端状态 | CDP 里求值 `window.__zcStore.getState()` |
| 应用日志 | `%LOCALAPPDATA%\ZDiskCleanerPro3\zc-app.log`（C:\Temp 下另有 zc-qa-*.json 测试报告） |
| 崩溃查询（PowerShell） | `Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000,1002;StartTime=(Get-Date).AddDays(-1)}` 后按消息含 `zdiskcleaner` 过滤 |
| 规则手册 | `./target/release/zclean.exe rules --md` / `docs/rules.md` |

---

## 9. 踩坑实录（环境/流程类，按类别）

### Shell 与网络
- **P1 代理变量**：每条 bash 先 unset 全部代理变量（§4），否则网络类工具随机失败，症状是 `unexpected EOF` / 连接重置，极具迷惑性。
- **P2 CRLF**：Windows 文本模式产生的文件带 `\r\n`，`while read` 循环前要 `tr -d '\r'`，否则 id 末尾带 `\r` 导致各种"查无此物"。
- **P3 `grep -c` 的退出码**：匹配数为 0 时退出码 1，放在 `&&` 链里会静默断链。验证型 grep 单独跑，或用 `|| true`。

### 构建
- **T1 必须走 MSVC 包装脚本**：直接开 bash 跑 cargo 链接 Tauri 壳会因缺 MSVC 环境失败。`scripts/msvc-*.cmd` 负责注入 vcvars64 + PATH。注意包装脚本本身不带 cargo 时要用绝对路径 `C:/Users/yusheng/.cargo/bin/cargo.exe`。
- **T2 target 目录位置**：workspace 的产物在**仓库根** `target/release/`，不在 `src-tauri/target/`（找 exe 别找错地方）。
- **T3 陈旧 zclean**：`msvc-tauri-build.cmd`（tauri build）**不会**重编 zc-cli。改过内核/CLI 后直接跑 `zclean` 会用到旧二进制，症状是"新子命令不存在"。CLI 变更后显式 `cargo build --release -p zc-cli`。

### CDP / GUI 自动化
- **C1 DPR=2 坐标偏移**：见 §6.6，一律元素级合成事件。
- **C2 窗口遮挡 → rAF 节流**：见 §6.5。
- **C3 页面过渡**：见 §6.4，0.4s 内查询拿到的还是旧页。
- **C4 受控输入谓词**：见 §6.7。
- **C5 并发跑 QA**：三套脚本串行跑；历史上有过并发导致共享日志/报告互相覆盖的混乱（现报告名带唯一时间戳，但端口和实例仍共享）。

### 本机环境（这台机器特有）
- **E1 外部安全软件干扰**：机器上有第三方安全软件（疑似电脑管家类），行为包括：**~3 秒内删除 HKCU Run 新增值**；**秒删测试夹具文件**；偶发挂起进程。因此：启动项相关的 GUI 测试夹具经常立失，QA 脚本对启动项用例做了"夹具被外部删除即 SKIP"的探测；排查"文件/注册表值莫名消失"时先想到它。
- **E2 桌面便携副本被锁**：用户可能正开着桌面的免安装副本，替换其 exe 前先 tasklist 确认。

### 发布
- **R1 latest.json 命名**：见 §7.5，血泪教训——历史上按 `latest-<版本>.json` 上传过一次，立刻删掉重传为 `latest.json` 才修复。
- **R2 版本三处口径**：见 §3.3 末条，发版漏改任意一处会出现"关于页与主界面版本不一致"。

---

## 10. 快速上手清单（建议的前三天）

**第一天：建立认知**
1. 读本文 §1–§3，再读 `docs/adr/` 三篇 ADR 与 `docs/rules.md`。
2. `cargo test`（纯逻辑，无 MSVC 依赖）确认工具链通。
3. `cd ui && pnpm install && pnpm build`，再按 §5.1 跑一次完整构建。
4. 按 §5.2 启动应用，把 8 个页面各点一遍（体检台 / 空间雷达 / 大文件 / 重复文件 / 迁移中心 / 工具箱 / 历史记录 / 设置——含主题切换）。

**第二天：链路打通**
1. 按 §5.2 的 CDP 方式启动，串行跑三套 QA（§5.3 ②），读 `C:\Temp` 下三份 JSON 报告。
2. 用 sqlite 直查 `ledger.db`（§3.2），对照"历史记录"页理解批次/条目/history 三层关系。
3. 用 `zclean` CLI 走一遍 headless 链路：`scan → apply --mode vault → undo → purge`（§5.3 ③），对照 GUI 的清理→还原闭环。
4. 做一次故障演练：清理后立刻 `purge`，确认台账行被抹除且"历史记录"里该批次不再可还原。

**第三天：能改能调**
1. 挑一条 `zc-rules` 里的规则通读，理解 rule → guard → executor 的字段流转。
2. 在 `src-tauri/src/lib.rs` 里通读一遍全部 IPC 命令与 DTO，对照 `ui/src/lib/ipc.ts` 的封装。
3. 按 §6 的约定给某个页面写一条新的 QA 用例（用 qa_drive.py 里现成的 CDP 客户端与工具函数），跑通即算出师。
4. 通读 §7 发布流程（即使暂不发版），把 latest.json 命名约束和版本号两处同步记牢。

---

## 11. 文档索引

| 文档 | 内容 |
|---|---|
| `docs/adr/ADR-001-stack.md` | 技术选型与理由 |
| `docs/adr/ADR-002-data-layer.md` | 台账/数据层设计 |
| `docs/adr/ADR-003-defer-mft.md` | 扫描范围（MFT 相关）决策 |
| `docs/rules.md` | 清理规则手册 |
| `docs/benchmarks.md` | 性能基准记录 |
| `docs/soak-log.md` | 72h 稳定性巡检日志 |
| `docs/ROADMAP.md` | 路线图 |
| `portable/README-PORTABLE.txt` | 便携包说明（版本/数据目录） |

---
*交接完成基准：v4.1.0，main 分支。GUI 回归基线：qa_drive 11/11 + qa_edge 8/8 + qa_v4 5/5 全绿。*
