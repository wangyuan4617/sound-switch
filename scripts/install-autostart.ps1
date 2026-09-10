<#
.SYNOPSIS
    安装 sound-switch 的“登录时自动启动”（计划任务，运行于当前用户会话）。

.DESCRIPTION
    为什么用计划任务而不是 Windows 服务：
      默认输出音频设备是“按登录会话”生效的，而 Windows 服务运行在 Session 0，
      无法修改你桌面会话的默认设备；因此“以当前用户身份、在登录时启动”才是正确做法。
      （这与 SoundSwitch 等同类工具的做法一致。）

    任务特性：
      - 触发   ：当前用户登录时（Run only when user is logged on）
      - 工作目录：项目根目录（config.json / state.json / logs 都相对它）
      - 无窗口 ：经 sound-switch-launch.exe 启动（GUI 子系统启动器 + CREATE_NO_WINDOW），
                 不会出现控制台窗口，也不会闪一下
      - 时间上限：无（解除默认的“运行超过 3 天就停止”）
      - 多实例 ：IgnoreNew（配合程序自身的单实例保护）
      - 权限   ：普通用户权限（改默认输出设备不需要管理员）

.PARAMETER StartNow
    安装后立即启动任务（无需重新登录即可生效）。

.PARAMETER NoLauncher
    不走启动器，直接运行 sound-switch.exe run（会带出可见控制台窗口，仅用于排查问题）。

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-autostart.ps1
    powershell -ExecutionPolicy Bypass -File scripts\install-autostart.ps1 -StartNow
#>
[CmdletBinding()]
param(
    [string]$TaskName = 'sound-switch',
    [string]$ExePath,
    [string]$WorkDir,
    [switch]$StartNow,
    [switch]$NoLauncher
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
if (-not $WorkDir) { $WorkDir = $root }

# 自动查找可执行文件：兼容两种目录结构
#   1) 便携包结构：exe 与 config.json 同在包根目录（拷贝到新电脑后的形态）
#   2) 开发结构  ：exe 在 target\release\
$exeName = if ($NoLauncher) { 'sound-switch.exe' } else { 'sound-switch-launch.exe' }
if (-not $ExePath) {
    $candidates = @(
        (Join-Path $root $exeName),
        (Join-Path $root "bin\$exeName"),
        (Join-Path $root "target\release\$exeName")
    )
    $ExePath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if (-not $ExePath -or -not (Test-Path $ExePath)) {
    throw "找不到 $exeName —— 请先执行 cargo build --release，或把便携包里的 exe 与本脚本放在同一目录结构下"
}

$argument = if ($NoLauncher) { 'run' } else { '' }
if (-not (Test-Path (Join-Path $WorkDir 'config.json'))) {
    Write-Warning "工作目录下没有 config.json（首次运行会自动生成）: $WorkDir"
}

$user = "$env:USERDOMAIN\$env:USERNAME"

if ($argument) {
    $action = New-ScheduledTaskAction -Execute $ExePath -Argument $argument -WorkingDirectory $WorkDir
} else {
    # 无参数时不能传空字符串（New-ScheduledTaskAction 会拒绝空 Argument）
    $action = New-ScheduledTaskAction -Execute $ExePath -WorkingDirectory $WorkDir
}
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
$settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -StartWhenAvailable `
    -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Seconds 0)
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger `
    -Settings $settings -Principal $principal -Force `
    -Description 'sound-switch: 耳机开机时把默认输出切到耳机、关机时恢复（登录时自动运行，无窗口）' | Out-Null

$t = Get-ScheduledTask -TaskName $TaskName
Write-Output "已安装计划任务: $TaskName"
Write-Output ("  执行程序  : " + $t.Actions[0].Execute)
Write-Output ("  参数      : " + $t.Actions[0].Arguments)
Write-Output ("  工作目录  : " + $t.Actions[0].WorkingDirectory)
Write-Output ("  时间上限  : " + $t.Settings.ExecutionTimeLimit + "  (PT0S = 不限制)")
Write-Output ("  多实例    : " + $t.Settings.MultipleInstances)
Write-Output ("  运行身份  : " + $t.Principal.UserId)
Write-Output ("  无窗口    : " + $(if ($NoLauncher) { '否（可见控制台窗口）' } else { '是（经 sound-switch-launch.exe）' }))
Write-Output ""
Write-Output "查看状态: Get-ScheduledTask -TaskName $TaskName | Format-List"
Write-Output "手动启动: Start-ScheduledTask -TaskName $TaskName"
Write-Output "卸载    : powershell -ExecutionPolicy Bypass -File scripts\uninstall-autostart.ps1"

if ($StartNow) {
    Start-ScheduledTask -TaskName $TaskName
    Start-Sleep -Seconds 2
    $p = Get-Process sound-switch -ErrorAction SilentlyContinue
    if ($p) {
        Write-Output ("`n已启动，正在运行: PID " + ($p.Id -join ', '))
    } else {
        Write-Warning "任务已触发但未发现进程，请查看 logs\sound_switch.log"
    }
}
