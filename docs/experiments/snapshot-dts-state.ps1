# 抓取「DTS 音效当前状态」的结构化快照，用于前后对比
param(
    [string]$Out = (Join-Path $PSScriptRoot 'dts-state.txt'),
    [string]$EndpointGuid = '',
    [string]$DeviceKeyword = 'HyperX'
)
if (-not $EndpointGuid) {
    $EndpointGuid = (Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render' -ErrorAction SilentlyContinue |
        Where-Object { (Get-ItemProperty (Join-Path $_.PSPath 'Properties') -ErrorAction SilentlyContinue).'{b3f8fa53-0004-438e-9003-51a46e139bfc},6' -match $DeviceKeyword } |
        Select-Object -First 1).PSChildName
    if (-not $EndpointGuid) { throw "找不到名称含 '$DeviceKeyword' 的渲染端点" }
}
$ErrorActionPreference = 'Continue'
$lines = New-Object System.Collections.ArrayList
function Add-Line($s) { [void]$lines.Add($s) }
Add-Line ("### snapshot at " + (Get-Date -Format 'yyyy-MM-dd HH:mm:ss.fff'))
$app = Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue
Add-Line ("app      : " + $(if ($app) { "running pid=" + $app.Id } else { 'not running' }))
Add-Line ("apo3svc  : " + (Get-Service DTSAPO3Service -ErrorAction SilentlyContinue).Status)
$props = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$EndpointGuid\Properties"
$pv = Get-ItemProperty $props -ErrorAction SilentlyContinue
foreach ($pn in '{908dba32-edff-4c28-8e45-c918561f6748},2','{8a845654-d6c3-4cd7-b4eb-243d4bd99032},2','{fd8a7b27-0b18-4025-ab1c-bdd6b32e1604},2') {
    $v = $pv.$pn
    if ($null -eq $v) { Add-Line ("prop {0} : <absent>" -f $pn) }
    elseif ($v -is [byte[]]) { Add-Line ("prop {0} : len={1} hex={2}" -f $pn, $v.Length, (($v | ForEach-Object { $_.ToString('X2') }) -join '')) }
    else { Add-Line ("prop {0} : {1}" -f $pn, $v) }
}
[System.IO.File]::WriteAllLines($Out, $lines, (New-Object System.Text.UTF8Encoding($false)))
$lines | ForEach-Object { $_ }
"--- written: $Out"