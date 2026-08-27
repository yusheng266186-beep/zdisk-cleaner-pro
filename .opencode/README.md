# 子代理工作流（主代理策划/检阅 · worker 实现）

## 组成

| 角色 | 承载 | 职责 |
| --- | --- | --- |
| 主代理（策划/检阅） | ZCode 会话 | 拆解规格 → 下发任务 → 审查 diff/自测证据 → 合并或打回 |
| 实现工位 | `.opencode/agent/zclean-worker.md` → DeepSeek v4 Flash（opencode Zen 网关） | 按规格独立实现 + 自测，输出四段式交付 |

## 使用

```bash
opencode run --agent zclean-worker "<任务规格原文>"
```

> 模型路由已固定为配置里的 `open-code/deepseek-v4-flash(new)`；
> 若首次使用报鉴权错误，先执行一次 `opencode auth login` 完成 Zen 登录。

## 规格模板（主代理下发时遵循）

```
目标：<一句话>
涉及文件：<明确路径清单>
接口契约：<函数签名/事件名/返回结构>
验收标准：<可执行判定，如 "cargo test -p zc-cli 全绿 + tsc -b 零错">
```

## 分工纪律

worker 的工程红线写在 agent 定义里（守卫不可绕过、诚实口径、令牌化样式、依赖克制、不越界重构）；
主代理对每份交付执行「三查」：①红线扫描 ②接口契约一致性 ③真机自测复跑。
