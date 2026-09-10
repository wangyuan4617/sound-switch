<#
.SYNOPSIS
    卸载 sound-switch 的“登录时自动启动”计划任务。

.PARAMETER KeepRunning
    只删除计划任务，不结束当前正在运行的实例。

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-autostart.ps1
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-autostart.ps1 -KeepRunning
#>
[CmdletBinding()]
param(
    [string]$TaskName = 'sound-switch',
    [switch]$KeepRunning
)

$ErrorActionPreference = 'Stop'

$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if (-not $task) {
    Write-Output "未找到计划任务: $TaskName（无需卸载）"
} else {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    Write-Output "已删除计划任务: $TaskName"
}

if ($KeepRunning) {
    Write-Output "保留正在运行的实例（-KeepRunning）"
    return
}

$p = Get-Process sound-switch -ErrorAction SilentlyContinue
if ($p) {
    $p | Stop-Process -Force
    Write-Output ("已结束正在运行的实例: PID " + ($p.Id -join ', '))
} else {
    Write-Output "当前没有正在运行的实例"
}
