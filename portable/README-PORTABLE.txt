ZDiskCleanerPro v3.0.4 绿色便携版
====================================

【启动方式】
  双击 ZDiskCleanerPro.exe 即可运行。这就是程序的唯一入口——
  Tauri 应用本体就是这一个 exe，无需安装、无需其它组件。

【系统要求】
  Windows 10/11 x64。
  系统需自带 WebView2 运行时（Win11 全部内置；Win10 近年版本也已内置）。
  若缺失，微软官方会随 Windows Update 自动补齐，或手动安装 Evergreen 运行时。

【数据存放位置】
  台账/暂存区/历史记录默认写在：
    %LOCALAPPDATA%\ZDiskCleanerPro3\
  想改成 U 盘等自定义位置，可在启动前设置环境变量 ZC_DATA_DIR 指向目标目录。

【关于提权】
  涉及系统深处的清理（如 DISM 组件库、还原点）会按需弹出一次 UAC 窗口，
  拒绝则跳过该项，不影响其余功能。

【与应用内更新】
  便携版同样支持应用内检查更新（Ed25519 签名校验 + GitHub 渠道），
  更新会提示下载新版本；便携场景建议重新下载便携包覆盖。

【与安装版的关系】
  两者功能完全一致，可共存。安装版（NSIS）提供开始菜单/卸载入口：
  https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases/latest
