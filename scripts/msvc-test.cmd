@echo off
setlocal
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set VSPATH=%%i
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc
cd /d "%~dp0.."
echo [msvc-test] start %DATE% %TIME%
cargo test > "%TEMP%\zctest.log" 2>&1
echo [msvc-test] cargo_exit=%ERRORLEVEL%
findstr /C:"test result" "%TEMP%\zctest.log"
findstr /C:"error[" /C:"error:" "%TEMP%\zcptest.log" 2>nul
findstr /C:"error[" /C:"error:" "%TEMP%\zctest.log"
echo full log: %TEMP%\zctest.log
