# v5.0.0 跨层实施契约（CONTRACT）

> 供各实现代理遵守。任何一层偏离本契约，集成期以本文件为准。Rust→前端 JSON 一律 snake_case 字段名（serde 默认）；前端 invoke 参数 camelCase（Tauri 自动转换）。

## 1. zc-core 新增/变更公开 API

```rust
// 新模块 src/recycle_bin.rs（feature: windows Win32 UI_Shell；不可用则加到 Cargo.toml）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RecycleBinInfo { pub items: u64, pub bytes: u64 }          // SHQueryRecycleBinW 逐盘聚合
#[derive(Serialize, Clone, Debug)]
pub struct RecycleBinSummary { pub items_before: u64, pub bytes_before: u64, pub bytes_freed: u64 }
pub fn query_all() -> RecycleBinInfo;                                  // 永不失败，出错记 0
pub fn empty_all() -> Result<RecycleBinSummary>;                       // SHERB_NOCONFIRMATION|NOPROGRESSUI|NOSOUND；前后 quota 差=freed

// scanner.rs: pub fn new_session_id() -> String（从私有升公开，含随机熵）

// dedup.rs
pub fn find_duplicates_cancellable(roots: &[PathBuf], min_size: u64, cancel: &crate::scanner::ScanHandle) -> Result<Vec<DuplicateGroup>>;
// 同文件(volume,file_id)硬链接去重（suspect 阶段 GetFileInformationByHandle 开句柄）；reparse point 文件跳过（云占位防脱水）

// analyze.rs
pub fn build_tree_cancellable(root:&Path, depth:usize, max_children:usize, cancel:&ScanHandle) -> TreeNode; // cancel 命中提前返回部分树；缓存层负责报错

// ledger.rs（模式迁移：open() 内 PRAGMA table_info + 幂等 ALTER）
//   entries 增加 status TEXT NOT NULL DEFAULT 'committed'（journal 用）
//   history 增加 src TEXT NULL, dst TEXT NULL（迁移历史用）
pub fn live_manifest_ids(&self) -> Result<Vec<String>>;   // 不再吞错
pub fn vault_copies(&self, id:&str) -> Result<Vec<(String,String,u64)>>;
pub fn undo_entries(&self, id:&str) -> Result<Vec<(String,String)>>;
pub fn session_entries(&self, id:&str) -> Result<Vec<EntryDto>>;  // {origin, vault_rel, size, status}

// startup.rs
pub fn enable_one(key_id:&str) -> Result<bool>;   // 单条恢复；成功才从备份 JSON 移除；备份先于删值写入（disable 顺序修正）
#[derive(Serialize)] pub struct DisabledEntry { pub key_id:String, pub value:String }
pub fn list_disabled() -> Vec<DisabledEntry>;     // 从备份 JSON；解析失败 → 上抛 Err（不再静默空）

// executor（内部行为，签名不变）
//   Guard::new()：禁删根由 %SystemRoot%/%ProgramFiles%/%ProgramFiles(x86)%/%ProgramData% 派生，缺失回落 C: 字面；
//     USERPROFILE 缺失 → Guard::new 记录标记，vet() 直接 Err("env:USERPROFILE 缺失，fail-closed")；
//     进程已提权(system::is_elevated()) 时自动追加白名单（下列 norm 前缀）：
//     windows/temp、windows/softwaredistribution/download、windows/logs/windowsupdate、windows/logs/cbs、
//     windows/serviceprofiles/localservice/appdata/local/fontcache、windows/serviceprofiles/localservice/appdata/local/temp、
//     programdata/microsoft/deliveryoptimization、windows/prefetch、windows/memory.dmp、windows/minidump、perflogs、
//     programdata/microsoft/windows/wer、winreagent（$WinREAgent）
//   vault stash 改 journal 化：move 前落 manifest+entries(status=pending)，逐条成功 UPDATE committed，全败回滚+drop manifest；
//   sweep 的孤儿 GC：live 名单 Err → 整个 GC 跳过；目录 mtime < 24h → 不删；pending manifest 的目录 → 不删
//   回收站批量降级前 p.exists() 甄别；done_bytes O(n²) 回查改 HashMap
//   reboot.rs：接入 executor trash 失败分支（占用文件 MoveFileExW 入队并在 note 提示重启）
// 手写路径 session id：manual-<new_session_id()>；upsert 对已存在且 entries 非空的 id 返回 Err
```

## 2. src-tauri 命令与 DTO（S1 实现；U1 按此写 ipc.ts）

JSON DTO（serde rename_all camelCase 或字段本名 snake——统一 **snake_case** 字段名输出）：

```
ErrorDto { code: "io"|"guard"|"admin_required"|"not_found"|"busy"|"locked"|"cancelled"|"internal", message: String }
  —— 所有命令 Err(ErrorDto)，由 thiserror 分类映射；admin_required 用于 DISM/还原点/非提权时的 admin 规则场景
UndoResultDto { id: String, done: u64, bytes: u64, failed: Vec<FailDto> }   FailDto { path: String, error: String }
SessionOpDto 同 UndoResultDto（purge/empty_recycle_bin 复用）
MigrateUndoDto { restored: u64, failed: Vec<FailDto> }
SessionEntryDto { origin: String, vault_rel: String, size: u64, status: String }
StartupDisabledEntry = core DisabledEntry
```

命令清单（新增/变更项；未列出的保持现状）：

```
scan_now(include_admin: Option<bool>)        # true 且已提权 → 含 admin 规则
cancel_scan()                                # 修竞态：世代号，运行中才接受
cancel_busy()                                # 新：取消 big_files/find_dupes/analyze_tree 的忙任务（全局 BUSY_HANDLE）
big_files(path, top)            # 走 BUSY_HANDLE；取消 → Err(code=cancelled)
find_dupes(path, min_mb)        # 同上
analyze_tree(path, depth, fresh)             # 同上 + path 参数兑现（空=主目录）
undo_session(id) -> UndoResultDto            # 不再返回中文句子
purge_session(id) -> UndoResultDto
migrate_undo(src, dst) -> MigrateUndoDto     # 改 async spawn_blocking；失败路径也 invalidate
migrate_apply -> String                     # 成功/失败都 invalidate（失败=Link 后中断也失效）
empty_recycle_bin() -> RecycleBinSummary     # 新
query_recycle_bin() -> RecycleBinInfo        # 新
session_entries(id) -> Vec<SessionEntryDto>  # 新（历史下钻）
startup_list_disabled() -> Vec<StartupDisabledEntry>   # 新
startup_enable_one(key_id) -> bool                     # 新
startup_disable(key_id) -> bool               # 保持
clean_selected / vault_delete                  # vault_delete 双跑 actual_size 合并；写 history；结构化错误
```

其余：history_list 返回含 kind/src/dst 的记录；dism/rp 成功也 invalidate；drives_overview/system_occupancy 失败→Err。

## 3. ui/src/lib/ipc.ts 前端函数签名（U1 实现）

```ts
export class ZcError extends Error { code: string }        // 非对象 reject → {code:'internal'}
export const errCode = (e:unknown):string => e instanceof ZcError ? e.code : 'internal'
emptyRecycleBin(): Promise<{items_before:number; bytes_before:number; bytes_freed:number}>
queryRecycleBin(): Promise<{items:number; bytes:number}>
sessionEntries(id:string): Promise<SessionEntryDto[]>
listDisabledStartups(): Promise<{key_id:string;value:string}[]>
enableOneStartup(keyId:string): Promise<boolean>
cancelBusy(): Promise<void>
startScan(includeAdmin?:boolean)   // store 传值
// undoSession/purgeSession → Promise<UndoResultDto>；migrateUndo → Promise<MigrateUndoDto>
// scan://progress 监听必须持有并复用 unlisten
```

## 4. 设计令牌与 motion 词汇（U2 实现；U1 只许 import 下列既有+新名）

```css
/* global.css 新增（dark+light 都定义） */
--zc-hover           /* 导航/卡片 hover 底色，light 下可见 */
--zc-accent-text     /* 品牌色文字：dark #22d3ee / light #0e7490 */
--zc-danger-text     /* 确认红字：dark #fb7185 / light #b91c1c */
--zc-glow-brand      /* CTA 辉光阴影参数串，收编三处手写 */
--zc-shadow-1/2/3 的 light 覆盖补齐
.zc-press  {  active 态 scale(.96) 工具类 }
/* light 下 --zc-accent-b 等按可读性重定义 */
```
```ts
/* motion.ts 新增导出（U1 直接 import，勿改实现） */
export const pulseDot   // 呼吸点 {duration:1.4..} 系列
export const overlayIn = { opacity:[0,1], transition:{duration:0.18} }
export const overlayOut = { opacity:[1,0], transition:{duration:0.25} }
// 保留现有全部导出名（pageVariants/cascade/springSnappy/...）；死码 popIn/drawerVariants 由 U2 用于崩溃卡/抽屉
```

## 5. store.ts 字段纪律（U1）

- **既有字段名一个不许改**（phase/scanFiles/scanBytes/report/selection/history/drives/rules/theme/…，QA 读 __zcStore 依赖）。只许新增：initError、busyRunning、migratePhase、homeAdmin(bool)。
- 新增动作：`cancelBusy()`, `clearInitError()` 等自定。

## 6. QA 锚点保护（全体）

- 不得修改以下文案：H1「磁盘体检，一键开始」、按钮「开始智能体检」、treemap 色块保持 div、toast「移入暂存区」「清理冗余份数」「暂存区」类文案（改前先在 scripts/qa_*.py grep click_text/wait_toast 的字符串清单核对）。
- 三套 QA 继续跑通是发布硬门槛；Q1 的脚本改动不得放松断言（SKIP 单列，不算绿）。

## 7. 发版纪律

- 版本 5.0.0：Cargo.toml [workspace.package].version + src-tauri/tauri.conf.json；ui/package.json 不动。
- rules.md 用 `zclean rules --md` 重生成；CHANGELOG 从 git tags 补齐 v3.0.1→v5.0.0；portable README 同步 5.0.0；winget manifests 回填 5.0.0 + InstallerSha256。
- 4 资产精确命名：ZDiskCleanerPro_5.0.0_x64-setup.exe / .sig / latest.json / ZDiskCleanerPro-Portable-v5.0.0.zip
