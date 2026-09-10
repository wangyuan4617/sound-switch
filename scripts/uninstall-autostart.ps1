<#
.SYNOPSIS
    卸载 sound-switch 的“登录时自动启动”计划任务（并结束后台实例）。

.PARAMETER KeepRunning
    只删除计划任务，不结束当前正在运行的实例。

.PARAMETER Pause
    结束时等待回车再退出（供双击 .bat 运行时使用，避免窗口一闪而过）。

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-autostart.ps1
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-autostart.ps1 -KeepRunning
#>
[CmdletBinding()]
param(
    [string]$TaskName = 'sound-switch',
    [switch]$KeepRunning,
    [switch]$Pause
)

$ErrorActionPreference = 'Stop'

Write-Output "============ 卸载 sound-switch 登录自启 ============"

$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if (-not $task) {
    Write-Output "未找到计划任务: $TaskName（可能本来就没安装）"
} else {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    Write-Output "已删除计划任务: $TaskName"
}

if ($KeepRunning) {
    Write-Output "保留正在运行的实例（-KeepRunning）"
} else {
    $p = Get-Process sound-switch -ErrorAction SilentlyContinue
    if ($p) {
        $p | Stop-Process -Force
        Write-Output ("已结束正在运行的实例: PID " + ($p.Id -join ', '))
    } else {
        Write-Output "当前没有正在运行的实例"
    }
}

Write-Output ""
Write-Output "================== 卸载完成 =================="
Write-Output "程序文件仍保留在本目录（可直接删除整个文件夹）。"
Write-Output "以后想重新安装：双击「安装登录自启.bat」。"

if ($Pause) {
    Write-Output ""
    Write-Output "-------------------------------------------------------"
    try { Read-Host "按回车键关闭窗口" | Out-Null } catch { }
}
