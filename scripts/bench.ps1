# ZDiskCleaner Pro v3 · 扫描基准脚本
# 用法: powershell ./scripts/bench.ps1 [-Iterations 3]
# 依赖: 先 `cargo build --release -p zc-cli`
# 输出: 向 stdout 打印一段 Markdown，重定向保存进 docs/benchmarks.md
# 说明: CLI 在 ZC_BENCH=1 时输出 ASCII 的 [bench] 行，规避管道编码差异

param([int]$Iterations = 3)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $repo "target\release\zclean.exe"
if (-not (Test-Path $exe)) { Write-Error "先执行: cargo build --release -p zc-cli" }

$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$ram = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
$os = (Get-CimInstance Win32_OperatingSystem).Caption

$env:ZC_BENCH = "1"
$samples = @()
for ($i = 1; $i -le $Iterations; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $out = & $exe scan 2>&1 | Out-String
    $sw.Stop()
    if ($out -match "\[bench\] files=(\d+) duration_ms=(\d+) cleanable_bytes=(\d+) cleanable_count=(\d+)") {
        $files = "{0:N0}" -f [long]$Matches[1]
        $dur   = [math]::Round([long]$Matches[2] / 1000.0, 2)
        $found = "{0:N2} MB" -f ([long]$Matches[3] / 1MB)
    } else {
        $files = "?"; $dur = "?"; $found = "?"
    }
    $samples += [pscustomobject]@{
        Run     = $i
        WallSec = [math]::Round($sw.Elapsed.TotalSeconds, 2)
        EngineS = $dur
        Files   = $files
        Found   = $found
    }
}

# 独立子进程采样峰值内存（WorkingSet64）
$memPeak = 0
$p = Start-Process -FilePath $exe -ArgumentList "scan" -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput "$env:TEMP\zc-bench-out.txt"
while (-not $p.HasExited) {
    $p.Refresh()
    if ($p.WorkingSet64 -gt $memPeak) { $memPeak = $p.WorkingSet64 }
    Start-Sleep -Milliseconds 50
}
$memMB = [math]::Round($memPeak / 1MB)

@"
## 扫描基准 ($(Get-Date -Format "yyyy-MM-dd HH:mm"))

| 项目 | 值 |
| --- | --- |
| 机器 | $os · $(($cpu -replace '\(R\)','').Trim()) · ${ram}GB RAM |
| 场景 | 全部内置规则 · 用户态（无提权）· 连续 ${Iterations} 次 |

| 次 | 墙钟(s) | 引擎内(s) | 遍历文件 | 发现可清理 |
| :-: | :-: | :-: | :-: | :-: |
$( ($samples | ForEach-Object { "| $($_.Run) | $($_.WallSec) | $($_.EngineS) | $($_.Files) | $($_.Found) |" }) -join "`n" )

进程峰值内存（WorkingSet）≈ **${memMB} MB**

口径：含规则展开、单遍并行遍历、守卫过滤、报告落盘；不含删除。
第 1 次偏冷缓存，其后为热缓存；数字为本机实测样本，非承诺值。
"@
