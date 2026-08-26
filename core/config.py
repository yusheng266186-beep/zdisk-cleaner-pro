# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 清理规则配置
在原版规则基础上扩充, 并修正了原版的路径问题。
每条规则定义: 名称、类别、说明、路径模式、风险等级、默认勾选。
"""

import os

RISK_SAFE = 'safe'
RISK_LOW = 'low'
RISK_MEDIUM = 'medium'
RISK_HIGH = 'high'


def _env(name, default):
    v = os.environ.get(name)
    return v if v else default


USERPROFILE = os.path.expanduser('~')
APPDATA = _env('APPDATA', os.path.join(USERPROFILE, 'AppData', 'Roaming'))
LOCALAPPDATA = _env('LOCALAPPDATA', os.path.join(USERPROFILE, 'AppData', 'Local'))
TEMP = _env('TEMP', os.path.join(LOCALAPPDATA, 'Temp'))
SYSTEM_DRIVE = os.environ.get('SystemDrive', 'C:')


def _p(*parts):
    return os.path.normpath(os.path.join(*parts))


# ---------------------------------------------------------------------------
# 清理规则 (原版 27 条 + 新增)
# ---------------------------------------------------------------------------
CLEAN_RULES = [
    # ---- 系统 ----
    dict(name='Windows 临时文件', category='系统', desc='C:\\Windows\\Temp 系统临时目录',
         targets=['C:\\Windows\\Temp\\**'], risk=RISK_SAFE, default_select=True),
    dict(name='用户临时文件', category='系统', desc='%LOCALAPPDATA%\\Temp 用户级临时目录',
         targets=[_p(LOCALAPPDATA, 'Temp', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='Windows 更新下载缓存', category='系统', desc='SoftwareDistribution\\Download 已下载的更新安装包',
         targets=['C:\\Windows\\SoftwareDistribution\\Download\\**'], risk=RISK_LOW, default_select=True),
    dict(name='Windows 旧版本日志', category='系统', desc='CBS/DISM/Windows 日志文件',
         targets=['C:\\Windows\\Logs\\**', 'C:\\Windows\\Inf\\setupapi.dev.log'], risk=RISK_LOW, default_select=False),
    dict(name='Windows 错误报告', category='系统', desc='WER 报告队列与归档',
         targets=[_p(LOCALAPPDATA, 'Microsoft', 'Windows', 'WER', '**'),
                  'C:\\ProgramData\\Microsoft\\Windows\\WER\\**'], risk=RISK_SAFE, default_select=True),
    dict(name='崩溃转储 (Crash Dumps)', category='系统', desc='应用程序崩溃内存转储 .dmp',
         targets=[_p(LOCALAPPDATA, 'CrashDumps', '**'),
                  'C:\\Windows\\Minidump\\**',
                  'C:\\Windows\\LiveKernelReports\\**'], risk=RISK_LOW, default_select=False),
    dict(name='内存转储 MEMORY.DMP', category='系统', desc='蓝屏完整内存转储(占空间巨大)',
         targets=['C:\\Windows\\MEMORY.DMP'], risk=RISK_LOW, default_select=False),
    dict(name='缩略图与图标缓存', category='系统', desc='Explorer 缩略图/图标缓存, 删除后会自动重建',
         targets=[_p(LOCALAPPDATA, 'Microsoft', 'Windows', 'Explorer', 'thumbcache_*.db'),
                  _p(LOCALAPPDATA, 'Microsoft', 'Windows', 'Explorer', 'iconcache_*.db'),
                  _p(LOCALAPPDATA, 'IconCache.db')], risk=RISK_SAFE, default_select=True),
    dict(name='字体缓存', category='系统', desc='Windows 字体缓存服务缓存',
         targets=['C:\\Windows\\ServiceProfiles\\LocalService\\AppData\\Local\\FontCache\\**'],
         risk=RISK_LOW, default_select=False),
    dict(name='Windows Defender 历史扫描', category='系统', desc='Defender 历史扫描与隔离缓存',
         targets=['C:\\ProgramData\\Microsoft\\Windows Defender\\Scans\\History\\**'],
         risk=RISK_LOW, default_select=False),
    dict(name='传递优化文件 (Delivery Optimization)', category='系统', desc='P2P 更新分发缓存',
         targets=['C:\\Windows\\SoftwareDistribution\\DeliveryOptimization\\**'], risk=RISK_SAFE, default_select=True),
    # 新增: Prefetch 与 DirectX 着色器缓存
    dict(name='Windows 预读取 (Prefetch)', category='系统', desc='应用预读取数据, 删除后首次启动略慢',
         targets=['C:\\Windows\\Prefetch\\**'], risk=RISK_LOW, default_select=False),
    dict(name='DirectX 着色器缓存', category='系统', desc='D3D 着色器缓存, 删除后游戏首次加载重建',
         targets=[_p(LOCALAPPDATA, 'D3DSCache', '**'),
                  _p(LOCALAPPDATA, 'NVIDIA', 'DXCache', '**'),
                  _p(LOCALAPPDATA, 'NVIDIA', 'GLCache', '**'),
                  _p(LOCALAPPDATA, 'AMD', 'DxCache', '**'),
                  _p(LOCALAPPDATA, 'AMD', 'DxcCache', '**')], risk=RISK_LOW, default_select=False),

    # ---- 浏览器 ----
    dict(name='Chrome 缓存', category='浏览器', desc='Google Chrome 各配置缓存',
         targets=[_p(LOCALAPPDATA, 'Google', 'Chrome', 'User Data', '*', 'Cache', '**'),
                  _p(LOCALAPPDATA, 'Google', 'Chrome', 'User Data', '*', 'Code Cache', '**'),
                  _p(LOCALAPPDATA, 'Google', 'Chrome', 'User Data', '*', 'GPUCache', '**'),
                  _p(LOCALAPPDATA, 'Google', 'Chrome', 'User Data', '*', 'Service Worker', 'CacheStorage', '**'),
                  _p(LOCALAPPDATA, 'Google', 'Chrome', 'User Data', '*', 'Crashpad', 'reports', '**')],
         risk=RISK_SAFE, default_select=True),
    dict(name='Edge 缓存', category='浏览器', desc='Microsoft Edge 各配置缓存',
         targets=[_p(LOCALAPPDATA, 'Microsoft', 'Edge', 'User Data', '*', 'Cache', '**'),
                  _p(LOCALAPPDATA, 'Microsoft', 'Edge', 'User Data', '*', 'Code Cache', '**'),
                  _p(LOCALAPPDATA, 'Microsoft', 'Edge', 'User Data', '*', 'GPUCache', '**'),
                  _p(LOCALAPPDATA, 'Microsoft', 'Edge', 'User Data', '*', 'Service Worker', 'CacheStorage', '**'),
                  _p(LOCALAPPDATA, 'Microsoft', 'Edge', 'User Data', '*', 'Crashpad', 'reports', '**')],
         risk=RISK_SAFE, default_select=True),
    dict(name='IE/Edge 旧版 Internet 缓存', category='浏览器', desc='INetCache / WebCache',
         targets=[_p(LOCALAPPDATA, 'Microsoft', 'Windows', 'INetCache', '**'),
                  _p(LOCALAPPDATA, 'Microsoft', 'Windows', 'WebCache', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='Firefox 缓存', category='浏览器', desc='Firefox profile 缓存',
         targets=[_p(LOCALAPPDATA, 'Mozilla', 'Firefox', 'Profiles', '*', 'cache2', '**')], risk=RISK_SAFE, default_select=True),
    # 新增: Chromium 内核第三方浏览器
    dict(name='Brave 浏览器缓存', category='浏览器', desc='Brave 各配置缓存',
         targets=[_p(LOCALAPPDATA, 'BraveSoftware', 'Brave-Browser', 'User Data', '*', 'Cache', '**'),
                  _p(LOCALAPPDATA, 'BraveSoftware', 'Brave-Browser', 'User Data', '*', 'Code Cache', '**'),
                  _p(LOCALAPPDATA, 'BraveSoftware', 'Brave-Browser', 'User Data', '*', 'GPUCache', '**')],
         risk=RISK_SAFE, default_select=True),
    dict(name='Opera 浏览器缓存', category='浏览器', desc='Opera 稳定版缓存',
         targets=[_p(APPDATA, 'Opera Software', 'Opera Stable', 'Cache', '**'),
                  _p(APPDATA, 'Opera Software', 'Opera Stable', 'Code Cache', '**'),
                  _p(APPDATA, 'Opera Software', 'Opera Stable', 'GPUCache', '**')],
         risk=RISK_SAFE, default_select=True),
    dict(name='Vivaldi 浏览器缓存', category='浏览器', desc='Vivaldi 各配置缓存',
         targets=[_p(LOCALAPPDATA, 'Vivaldi', 'User Data', '*', 'Cache', '**'),
                  _p(LOCALAPPDATA, 'Vivaldi', 'User Data', '*', 'Code Cache', '**'),
                  _p(LOCALAPPDATA, 'Vivaldi', 'User Data', '*', 'GPUCache', '**')],
         risk=RISK_SAFE, default_select=True),

    # ---- 开发工具 ----
    dict(name='pip 缓存', category='开发工具', desc='Python pip 下载缓存',
         targets=[_p(LOCALAPPDATA, 'pip', 'cache', '**'),
                  _p(USERPROFILE, '.cache', 'pip', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='npm 缓存', category='开发工具', desc='Node.js npm 包缓存',
         targets=[_p(APPDATA, 'npm-cache', '**'),
                  _p(LOCALAPPDATA, 'npm-cache', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='yarn 缓存', category='开发工具', desc='Yarn 包缓存',
         targets=[_p(LOCALAPPDATA, 'Yarn', 'Cache', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='NuGet 缓存 (HTTP)', category='开发工具', desc='.NET NuGet HTTP 请求缓存(非全局包)',
         targets=[_p(LOCALAPPDATA, 'NuGet', 'v3-cache', '**')], risk=RISK_LOW, default_select=False),
    dict(name='VSCode 缓存', category='开发工具', desc='VSCode Cache/CachedData/CachedExtensions',
         targets=[_p(APPDATA, 'Code', 'Cache', '**'),
                  _p(APPDATA, 'Code', 'CachedData', '**'),
                  _p(APPDATA, 'Code', 'CachedExtensionVSIXs', '**'),
                  _p(APPDATA, 'Code', 'Code Cache', '**'),
                  _p(APPDATA, 'Code', 'GPUCache', '**'),
                  _p(APPDATA, 'Code', 'logs', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='Gradle 缓存', category='开发工具', desc='Gradle 构建缓存与包装器',
         targets=[_p(USERPROFILE, '.gradle', 'caches', '**'),
                  _p(USERPROFILE, '.gradle', 'wrapper', '**')], risk=RISK_LOW, default_select=False),
    dict(name='Maven 仓库', category='开发工具', desc='.m2 本地仓库(重新下载较慢, 谨慎)',
         targets=[_p(USERPROFILE, '.m2', 'repository', '**')], risk=RISK_MEDIUM, default_select=False),
    dict(name='Cargo 缓存 (Rust)', category='开发工具', desc='Rust cargo 注册表缓存',
         targets=[_p(USERPROFILE, '.cargo', 'registry', '**')], risk=RISK_LOW, default_select=False),
    dict(name='Go 模块缓存', category='开发工具', desc='Go mod 下载缓存',
         targets=[_p(USERPROFILE, 'go', 'pkg', 'mod', '**')], risk=RISK_LOW, default_select=False),
    # 新增开发缓存
    dict(name='JetBrains IDE 缓存', category='开发工具', desc='IDEA/PyCharm 等索引与日志缓存',
         targets=[_p(LOCALAPPDATA, 'JetBrains', '*', 'caches', '**'),
                  _p(LOCALAPPDATA, 'JetBrains', '*', 'log', '**')], risk=RISK_LOW, default_select=False),
    dict(name='Conda 缓存', category='开发工具', desc='Anaconda/Miniconda 包缓存',
         targets=[_p(USERPROFILE, '.conda', 'pkgs', '**'),
                  _p(USERPROFILE, 'anaconda3', 'pkgs', '**'),
                  _p(USERPROFILE, 'miniconda3', 'pkgs', '**')], risk=RISK_LOW, default_select=False),
    dict(name='HuggingFace 模型缓存', category='开发工具', desc='AI 模型下载缓存(重新下载耗时)',
         targets=[_p(USERPROFILE, '.cache', 'huggingface', '**')], risk=RISK_MEDIUM, default_select=False),

    # ---- 应用 ----
    dict(name='微信缓存', category='应用', desc='WeChat 文件/缓存(注意: 聊天文件可能在此)',
         targets=[_p(USERPROFILE, 'Documents', 'WeChat Files', '*', 'FileStorage', 'Cache', '**')],
         risk=RISK_HIGH, default_select=False),
    dict(name='微信新版本缓存', category='应用', desc='WeChat 4.x 缓存目录',
         targets=[_p(USERPROFILE, 'Documents', 'xwechat_files', '*', 'msg_cache', '**'),
                  _p(APPDATA, 'Tencent', 'xwechat', '**')], risk=RISK_LOW, default_select=False),
    dict(name='QQ 缓存', category='应用', desc='Tencent Files 缓存',
         targets=[_p(USERPROFILE, 'Documents', 'Tencent Files', '*', 'Image', '**'),
                  _p(USERPROFILE, 'Documents', 'Tencent Files', '*', 'FileRecv', '**')],
         risk=RISK_MEDIUM, default_select=False),
    dict(name='Spotify 缓存', category='应用', desc='Spotify 本地缓存',
         targets=[_p(APPDATA, 'Spotify', 'Storage', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='Discord 缓存', category='应用', desc='Discord Cache/Code Cache',
         targets=[_p(APPDATA, 'discord', 'Cache', '**'),
                  _p(APPDATA, 'discord', 'Code Cache', '**'),
                  _p(APPDATA, 'discord', 'GPUCache', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='Zoom 录制与缓存', category='应用', desc='Zoom 录制/缓存',
         targets=[_p(APPDATA, 'Zoom', '**')], risk=RISK_MEDIUM, default_select=False),
    dict(name='Teams 缓存', category='应用', desc='Microsoft Teams 缓存',
         targets=[_p(APPDATA, 'Microsoft', 'Teams', 'Cache', '**'),
                  _p(APPDATA, 'Microsoft', 'Teams', 'Service Worker', '**')], risk=RISK_SAFE, default_select=True),
    # 新增应用缓存
    dict(name='钉钉缓存', category='应用', desc='DingTalk 缓存与日志',
         targets=[_p(APPDATA, 'DingTalk', '*', 'Cache', '**'),
                  _p(APPDATA, 'DingTalk', '*', 'logs', '**')], risk=RISK_LOW, default_select=False),
    dict(name='Slack 缓存', category='应用', desc='Slack Cache/Code Cache',
         targets=[_p(APPDATA, 'Slack', 'Cache', '**'),
                  _p(APPDATA, 'Slack', 'Code Cache', '**'),
                  _p(APPDATA, 'Slack', 'GPUCache', '**')], risk=RISK_SAFE, default_select=True),
    dict(name='Steam 缓存与日志', category='应用', desc='Steam 日志/网页缓存(不含游戏)',
         targets=[_p(LOCALAPPDATA, 'Steam', 'htmlcache', '**'),
                  'C:\\Program Files (x86)\\Steam\\logs\\**'],
         risk=RISK_LOW, default_select=False),

    # ---- 其他 ----
    dict(name='回收站', category='系统', desc='清空所有盘的回收站',
         targets=['__RECYCLE_BIN__'], risk=RISK_MEDIUM, default_select=True),
    dict(name='.log 日志文件', category='日志', desc='扫描 AppData 与 ProgramData 下 .log 文件',
         targets=[_p(LOCALAPPDATA, '**', '*.log'),
                  _p(APPDATA, '**', '*.log'),
                  _p('C:\\ProgramData', '**', '*.log')], risk=RISK_LOW, default_select=False),
    dict(name='.tmp 临时文件 (用户目录)', category='系统', desc='用户目录下散落的 .tmp 文件',
         targets=[_p(USERPROFILE, '**', '*.tmp')], risk=RISK_LOW, default_select=False),
    dict(name='NVIDIA 驱动下载缓存', category='系统', desc='GeForce Experience 已下载的驱动安装包 (常达数 GB)',
         targets=['C:\\ProgramData\\NVIDIA Corporation\\Downloader\\**'],
         risk=RISK_LOW, default_select=False),
    dict(name='JetBrains 索引缓存', category='开发工具', desc='IDEA/PyCharm 项目索引 (删除后首次打开需重建索引)',
         targets=[_p(LOCALAPPDATA, 'JetBrains', '*', 'index', '**')],
         risk=RISK_MEDIUM, default_select=False),
    dict(name='旧驱动安装包', category='系统', desc='NVIDIA/AMD 显卡等旧版驱动安装包',
         targets=['C:\\NVIDIA\\**', 'C:\\AMD\\**'], risk=RISK_LOW, default_select=False),
]


# ---------------------------------------------------------------------------
# 可搬迁应用 (缓存重定向到其他盘, 从源头减少 C 盘占用)
# ---------------------------------------------------------------------------
RELOCATABLE_APPS = [
    dict(name='pip (Python)',
         detect_paths=[_p(LOCALAPPDATA, 'pip', 'cache')],
         env_var='PIP_CACHE_DIR',
         target_subdir='D:\\AppCache\\pip',
         desc='设置环境变量 PIP_CACHE_DIR 并迁移现有缓存'),
    dict(name='npm (Node.js)',
         detect_paths=[_p(APPDATA, 'npm-cache')],
         env_var=None,
         config_cmd='npm config set cache D:\\AppCache\\npm-cache',
         target_subdir='D:\\AppCache\\npm-cache',
         desc='npm config set cache 重定向缓存目录'),
    dict(name='yarn',
         detect_paths=[_p(LOCALAPPDATA, 'Yarn', 'Cache')],
         env_var='YARN_CACHE_FOLDER',
         target_subdir='D:\\AppCache\\yarn',
         desc='设置 YARN_CACHE_FOLDER 环境变量'),
    dict(name='VSCode 扩展',
         detect_paths=[_p(USERPROFILE, '.vscode', 'extensions')],
         env_var=None, config_cmd=None,
         vscode_arg='--extensions-dir=D:\\AppCache\\vscode-extensions',
         target_subdir='D:\\AppCache\\vscode-extensions',
         desc='VSCode 快捷方式添加 --extensions-dir 参数'),
    dict(name='Gradle',
         detect_paths=[_p(USERPROFILE, '.gradle')],
         env_var='GRADLE_USER_HOME',
         target_subdir='D:\\AppCache\\gradle',
         desc='设置 GRADLE_USER_HOME 环境变量'),
    dict(name='Maven',
         detect_paths=[_p(USERPROFILE, '.m2')],
         env_var=None, config_cmd=None, settings_xml=True,
         target_subdir='D:\\AppCache\\maven-repo',
         desc='修改 settings.xml 的 localRepository'),
    dict(name='Cargo (Rust)',
         detect_paths=[_p(USERPROFILE, '.cargo')],
         env_var='CARGO_HOME',
         target_subdir='D:\\AppCache\\cargo',
         desc='设置 CARGO_HOME 环境变量'),
    dict(name='Docker Desktop 数据',
         detect_paths=[_p(LOCALAPPDATA, 'Docker', 'wsl')],
         env_var=None, config_cmd=None, docker_settings=True,
         target_subdir='D:\\AppCache\\docker',
         desc='Docker Settings > Resources 修改磁盘映像位置'),
    dict(name='微信文件',
         detect_paths=[_p(USERPROFILE, 'Documents', 'WeChat Files')],
         env_var=None, config_cmd=None, app_setting='微信 设置 > 文件管理 > 修改存储路径',
         target_subdir='D:\\AppData\\WeChat',
         desc='在微信客户端内修改文件存储位置'),
    dict(name='HuggingFace 模型缓存',
         detect_paths=[_p(USERPROFILE, '.cache', 'huggingface')],
         env_var='HF_HOME',
         target_subdir='D:\\AppCache\\huggingface',
         desc='设置 HF_HOME 环境变量并迁移模型缓存'),
]


# ---------------------------------------------------------------------------
# 阈值配置
# ---------------------------------------------------------------------------
LARGE_FILE_THRESHOLD = 100 * 1024 * 1024   # 大文件阈值 100MB
OLD_FILE_DAYS = 180                        # 旧文件天数
DUPLICATE_MIN_SIZE = 10 * 1024 * 1024      # 重复文件最小体积 10MB

RISK_LABEL = {
    RISK_SAFE: '安全',
    RISK_LOW: '低风险',
    RISK_MEDIUM: '中风险',
    RISK_HIGH: '高风险',
}

RISK_COLOR = {
    RISK_SAFE: '#3dd68c',
    RISK_LOW: '#4f8cff',
    RISK_MEDIUM: '#ffb454',
    RISK_HIGH: '#ff5c69',
}

CATEGORY_ICON = {
    '系统': '⚙',
    '浏览器': '🌐',
    '开发工具': '⌨',
    '应用': '🛠',
    '日志': '📄',
}
