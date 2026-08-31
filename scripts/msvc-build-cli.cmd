@echo off
rem msvc-build-cli.cmd - MSVC release build wrapper for zc-cli (zclean.exe).
rem Same env recipe as msvc-test.cmd: vcvars64 + cargo on PATH.
rem Output: target\release\zclean.exe. Existing scripts untouched.
setlocal
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set VSPATH=%%i
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc
cd /d "%~dp0.."
echo [msvc-build-cli] start %DATE% %TIME%
cargo build --release -p zc-cli > "%TEMP%\zcbuildcli.log" 2>&1
echo [msvc-build-cli] cargo_exit=%ERRORLEVEL%
findstr /C:"error[" /C:"error:" /C:"Finished" "%TEMP%\zcbuildcli.log"
dir /b target\release\zclean.exe 2>nul
echo full log: %TEMP%\zcbuildcli.log
