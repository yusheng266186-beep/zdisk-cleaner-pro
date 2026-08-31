@echo off
setlocal
rem 发版构建（带 updater 签名）。密钥内容不进仓库/日志：
rem   私钥 %USERPROFILE%\.tauri\zdiskcleaner.key，口令 %USERPROFILE%\.tauri\password.txt
if not exist "%USERPROFILE%\.tauri\zdiskcleaner.key" (echo [signed-build] missing key & exit /b 1)
for /f "usebackq tokens=*" %%i in ("%USERPROFILE%\.tauri\password.txt") do set "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=%%i"
set "TAURI_SIGNING_PRIVATE_KEY=%USERPROFILE%\.tauri\zdiskcleaner.key"
call "%~dp0msvc-tauri-build.cmd"
endlocal
