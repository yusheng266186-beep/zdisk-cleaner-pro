ZDiskCleanerPro v5.0.0 绿色便携版
====================================

【启动方式】
  双击 ZDiskCleanerPro.exe 即可运行。无需安装、无需其它组件。
  包内附赠 zclean.exe（v5 起）：无图形界面的命令行客户端，
  scan/apply/undo/purge/vault/sweep/bigfiles/dupes 全链路可用，
  与图形版共享同一数据目录与台账。

【系统要求】
  Windows 10/11 x64。
  系统需自带 WebView2 运行时（Win11 全部内置；Win10 近年版本也已内置）。
  若缺失，微软官方会随 Windows Update 自动补齐，或手动安装 Evergreen 运行时。

【数据存放位置】
  台账/暂存区/历史记录默认写在：
    %LOCALAPPDATA%\ZDiskCleanerPro3\
  想改成 U 盘等自定义位置，可在启动前设置环境变量 ZC_DATA_DIR 指向目标目录。

【关于提权】
  涉及系统深处的清理（DISM 组件库、还原点、以及 Windows Update 缓存/
  系统 Temp/传递优化等系统级规则）需要以管理员身份运行本程序：
  提权后守卫自动按目录级白名单放行这些已知安全目录，其余系统
  文件照旧 fail-closed 拒绝；拒绝 UAC 则跳过该项，不影响其余功能。

【与应用内更新】
  便携版同样支持应用内检查更新（Ed25519 签名校验 + GitHub 渠道），
  更新会提示下载新版本；便携场景建议重新下载便携包覆盖。

【与安装版的关系】
  两者功能完全一致，可共存。安装版（NSIS）提供开始菜单/卸载入口：
  https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/latest
