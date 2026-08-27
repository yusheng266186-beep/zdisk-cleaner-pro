# ADR-002 · 数据层：Phase 2 用 JSON，SQLite 推迟到 MSVC 就绪后

状态：已接受 · 2026-08-27

## 背景

台账/历史/索引缓存的规划载体是 SQLite（rusqlite bundled 特性）。但 bundled 编译需要 C 工具链（cc → gcc），而当前本地工具链为「rustup gnu + 自带残缺 binutils」，`dlltool` 环节即失败，正在通过 MSYS2 补齐。即便如此，第一批落地优先保证纯 Rust 依赖可编译可测试。

## 决策

1. Phase 2 台账与历史采用 **JSON/JSONL**：每会话一份 `manifests/<id>.json`，历史为追加式 `history.jsonl`。规模预估单用户全年 <10MB，读写频度极低，性能无风险。
2. `data_dir()` 通过 `ZC_DATA_DIR` 环境变量可重定向——集成测试与便携模式共用此机制，零 AppData 污染。
3. Phase 4 引入扫描索引缓存时迁移到 SQLite（届时本地已有完整 MinGW/gcc，bundled 可编译），并提供一次性自动升级（读取旧 JSONL 导入）。
4. 对外 schema 字段已按 SQLite 迁移友好设计（entries 平铺、无嵌套多态）。

## 后果

- 优点：内核依赖树里没有 C 编译环节，GNU 工具链开箱可测；schema 提前冻结。
- 风险：并发写历史文件靠 append-only 原子性保证，跨进程同时清理理论上可能交错（UI 单实例锁在壳层解决，Phase 7 落地 mutex 命名对象）。
