<#
.SYNOPSIS
    查看 sound-switch 的运行状态（登录自启任务 / 后台进程 / 最近日志 / 当前默认输出）。
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\status.ps1
    powershell -ExecutionPolicy Bypass -File scripts\status.ps1 -Pause
#>
[CmdletBinding()]
param(
    [string]$TaskName = 'sound-switch',
    [int]$LogLines = 15,
    [switch]$Pause
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot

Write-Output "================ sound-switch 运行状态 ================"
Write-Output ("程序目录 : " + $root)
Write-Output ""

# ---- 登录自启任务 ----
$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($task) {
    $info = Get-ScheduledTaskInfo -TaskName $TaskName -ErrorAction SilentlyContinue
    Write-Output "【登录自启】已安装"
    Write-Output ("  状态      : " + $task.State)
    Write-Output ("  上次运行  : " + $info.LastRunTime + "    结果: " + $info.LastTaskResult + "  (0 = 成功)")
    Write-Output ("  执行程序  : " + $task.Actions[0].Execute)
    Write-Output ("  工作目录  : " + $task.Actions[0].WorkingDirectory)
} else {
    Write-Output "【登录自启】未安装 —— 双击「安装登录自启.bat」即可安装"
}

# ---- 后台进程 ----
$procs = Get-Process sound-switch -ErrorAction SilentlyContinue
Write-Output ""
if ($procs) {
    Write-Output "【后台进程】正在运行"
    foreach ($p in $procs) {
        $win = if ($p.MainWindowHandle -eq 0) { '无窗口（正常）' } else { '有窗口（异常）' }
        Write-Output ("  PID " + $p.Id + "   窗口: " + $win + "   内存: " + [math]::Round($p.WorkingSet64 / 1MB, 1) + " MB")
    }
} else {
    Write-Output "【后台进程】未运行"
}

# ---- 最近日志 ----
$log = Join-Path $root 'logs\sound_switch.log'
Write-Output ""
if (Test-Path $log) {
    Write-Output ("【最近日志】" + $log)
    Get-Content $log -Tail $LogLines -Encoding UTF8 | ForEach-Object { "  " + $_ }
} else {
    Write-Output "【最近日志】暂无（程序尚未在本目录运行过）"
}

# ---- 当前默认输出设备 ----
$exe = Join-Path $root 'sound-switch.exe'
if (Test-Path $exe) {
    Write-Output ""
    Write-Output "【当前默认输出设备】"
    & $exe status 2>&1 | Select-String '当前默认|目标端点' | ForEach-Object { "  " + $_.Line }
} else {
    Write-Output ""
    Write-Output ("【提示】未找到 sound-switch.exe，请确认目录结构完整：" + $root)
}

if ($Pause) {
    Write-Output ""
    Write-Output "-------------------------------------------------------"
    try { Read-Host "按回车键关闭窗口" | Out-Null } catch { }
}
