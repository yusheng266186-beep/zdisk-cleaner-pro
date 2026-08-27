@echo off
setlocal
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set VSPATH=%%i
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc
cd /d "%~dp0.."
echo [msvc-check-shell] start %DATE% %TIME%
cargo check -p zdiskcleaner-pro > "%TEMP%\zcshell.log" 2>&1
echo [msvc-check-shell] cargo_exit=%ERRORLEVEL%
findstr /C:"error[" /C:"error:" /C:"Finished" /C:"Compiling zc" "%TEMP%\zcshell.log"
echo full log: %TEMP%\zcshell.log
