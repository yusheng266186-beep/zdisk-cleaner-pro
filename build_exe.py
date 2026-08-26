# -*- coding: utf-8 -*-
"""PyInstaller 打包脚本"""
import PyInstaller.__main__
import os

HERE = os.path.dirname(os.path.abspath(__file__))

PyInstaller.__main__.run([
    os.path.join(HERE, 'main.py'),
    '--name=ZDiskCleanerPro',
    '--onefile',
    '--windowed',
    '--clean',
    '--noconfirm',
    '--icon', os.path.join(HERE, 'app_icon.ico'),
    '--add-data', os.path.join(HERE, 'app_icon.ico') + ';.',
    '--distpath', os.path.join(HERE, 'dist'),
    '--workpath', os.path.join(HERE, 'build'),
    '--specpath', os.path.join(HERE),
    # 排除测试脚本
    '--exclude-module', 'smoke_test',
    '--exclude-module', 'functional_test',
    '--exclude-module', 'e2e_test',
    '--exclude-module', 'tkinter.test',
    '--exclude-module', 'unittest',
    '--exclude-module', 'pydoc',
])
