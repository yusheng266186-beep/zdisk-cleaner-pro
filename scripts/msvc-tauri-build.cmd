@echo off
setlocal
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set VSPATH=%%i
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
rem node 目录从 %USERPROFILE% 派生(原硬编码 C:\Users\yusheng,行为不变、换机可用)
set "PATH=%USERPROFILE%\.cargo\bin;%~dp0..\ui\node_modules\.bin;%USERPROFILE%\nodejs\node-v20.18.3-win-x64;%PATH%"
set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc
cd /d "%~dp0.."
echo [tauri-build] start
call ui\node_modules\.bin\tauri.cmd build
echo [tauri-build] exit=%ERRORLEVEL%
dir /b src-tauri\target\release\bundle\nsis 2>nul
