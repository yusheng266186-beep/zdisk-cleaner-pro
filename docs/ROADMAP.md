# GA 后运行手册 · Roadmap

| 事项 | 说明 | 触发条件 |
| --- | --- | --- |
| 72h soak | GA 版真机连续观察，崩溃/误伤零事故记录于本文件附录 | GA 发布后一周内 |
| winget 提交 | `manifests/` 三件套回填 InstallerSha256 后，向 microsoft/winget-pkgs 提 PR（人工） | 随时可做 |
| 代码签名证书 | 当前 Ed25519 仅覆盖应用内更新；SmartScreen 信誉需 OV/EV 证书 | 用户决策 |
| hotfix 流程 | GA 后缺陷修复一律 patch 号（3.0.x），latest.json 随 Release 重生成 | 视事故 |
| backlog | Treemap 内直接清理动作 · 迁移长任务后台化与系统通知 · 多语言 en 包 | 择期 |
