# 清理规则手册

> 本文档由 `zclean rules --md` 自动生成，请勿手改。
> 源文件：`crates/zc-rules/src/lib.rs`

共 **60** 条内置规则。
默认只勾选「安全」档；风险档位见下表；标注 ⚙ 的规则需要管理员权限。

| ID | 名称 | 域 | 风险 |
| --- | --- | --- | :--: |
| `sys-user-temp` | 用户临时文件 | system | 安全 |
| `sys-system-temp` | 系统临时文件 (C:/Windows/Temp) ⚙ | system | 注意 |
| `sys-update-cache` | Windows 更新下载缓存 ⚙ | system | 注意 |
| `sys-thumbnails` | 缩略图缓存 | system | 安全 |
| `sys-dx-shader` | DirectX 着色器缓存 | system | 安全 |
| `sys-crash-dumps` | 应用崩溃转储 | system | 安全 |
| `sys-wer-queue` | Windows 错误报告队列 (用户) | system | 注意 |
| `sys-font-cache` | 系统字体缓存 ⚙ | system | 注意 |
| `sys-delivery-opt` | 传递优化缓存 ⚙ | system | 注意 |
| `sys-kernel-dumps` | 内核转储 / 蓝屏 Minidump ⚙ | system | 注意 |
| `sys-prefetch` | Windows Prefetch ⚙ | system | 注意 |
| `sys-wu-logs` | Windows 更新日志 ⚙ | system | 安全 |
| `sys-perflogs` | 性能日志 (PerfLogs) ⚙ | system | 注意 |
| `chrome-cache` | Chrome 缓存 | browser | 安全 |
| `chrome-crashpad` | Chrome 崩溃报告 | browser | 安全 |
| `edge-cache` | Edge 缓存 | browser | 安全 |
| `edge-crashpad` | Edge 崩溃报告 | browser | 安全 |
| `brave-cache` | Brave 缓存 | browser | 安全 |
| `brave-crashpad` | Brave 崩溃报告 | browser | 安全 |
| `firefox-cache2` | Firefox 网络缓存 | browser | 安全 |
| `ff-startup-cache` | Firefox 启动缓存 | browser | 安全 |
| `opera-cache` | Opera 缓存 | browser | 安全 |
| `opera-gx-cache` | Opera GX 缓存 | browser | 安全 |
| `vivaldi-cache` | Vivaldi 缓存 | browser | 安全 |
| `dev-npm-cache` | npm 下载缓存 | dev | 安全 |
| `dev-pip-cache` | pip 下载缓存 | dev | 安全 |
| `dev-yarn-berry` | Yarn Berry 全局镜像缓存 | dev | 安全 |
| `dev-yarn-classic` | Yarn Classic 全局缓存 | dev | 安全 |
| `dev-pnpm-metadata` | pnpm 状态缓存 | dev | 安全 |
| `dev-uv-cache` | uv 下载缓存 | dev | 安全 |
| `dev-poetry-cache` | Poetry 下载缓存 | dev | 安全 |
| `dev-go-build` | Go 编译缓存 | dev | 安全 |
| `dev-cargo-registry-cache` | Cargo crates.io 包缓存 | dev | 安全 |
| `dev-cargo-git-checkouts` | Cargo git 依赖 checkout 副本 | dev | 注意 |
| `dev-nuget-http` | NuGet HTTP 缓存 | dev | 安全 |
| `dev-node-gyp` | node-gyp 头文件缓存 | dev | 安全 |
| `dev-electron-builder` | electron-builder 打包缓存 | dev | 安全 |
| `dev-playwright` | Playwright 浏览器二进制 | dev | 安全 |
| `dev-vscode-cacheddata` | VS Code CachedData | dev | 安全 |
| `dev-vscode-vsix` | VS Code 扩展安装包缓存 | dev | 安全 |
| `dev-gradle-mods` | Gradle 模块与构建缓存 | dev | 注意 |
| `dev-gradle-wrapper` | Gradle Wrapper 发行版 | dev | 注意 |
| `dev-maven-http` | Maven 元数据缓存 | dev | 注意 |
| `dev-jb-indexes` | JetBrains 索引与缓存 | dev | 注意 |
| `dev-hf-models` | HuggingFace 模型 blob | dev | **风险** |
| `dev-torch-checkpoints` | PyTorch Hub 预训练权重 | dev | 注意 |
| `dev-bazel-cache` | Bazel 输出缓存 | dev | 注意 |
| `dev-vcpkg-bin` | vcpkg 二进制缓存 | dev | 注意 |
| `app-discord-cache` | Discord 缓存 | apps | 安全 |
| `app-slack-cache` | Slack 缓存 | apps | 安全 |
| `app-zoom-logs` | Zoom 日志 | apps | 安全 |
| `app-obs-logs` | OBS 日志 | apps | 安全 |
| `app-spotify-datacache` | Spotify 数据缓存 | apps | 注意 |
| `app-steam-htmlcache` | Steam 客户端网页缓存 | apps | 安全 |
| `app-steam-depotcache` | Steam 内容下载临时区 | apps | 注意 |
| `app-qq-crashpad` | QQ 崩溃报告 | apps | 安全 |
| `app-amd-shader` | AMD 着色器缓存 | apps | 安全 |
| `app-nv-shader` | NVIDIA 着色器缓存 | apps | 安全 |
| `log-jetbrains-local` | JetBrains 本地日志 | logs | 安全 |
| `log-cbs` | 组件维护日志 (CBS) ⚙ | logs | **专家** |
