# 监视耳机端点的空间音效属性变化（每秒采样，只在变化时记录 + 每 30 秒心跳）
param(
    [string]$Out = (Join-Path $PSScriptRoot 'dts-watch.log'),
    [int]$Minutes = 20,
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
$props = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$EndpointGuid\Properties"
function Snapshot {
    $pv = Get-ItemProperty $props -ErrorAction SilentlyContinue
    $a = $pv.'{908dba32-edff-4c28-8e45-c918561f6748},2'
    $b = $pv.'{8a845654-d6c3-4cd7-b4eb-243d4bd99032},2'
    $c = $pv.'{fd8a7b27-0b18-4025-ab1c-bdd6b32e1604},2'
    $sel = if ($a -and $a.Length -ge 40) { (($a[24..39]) | ForEach-Object { $_.ToString('X2') }) -join '' } else { '?' }
    $flag = if ($a -and $a.Length -ge 16) { (($a[12..15]) | ForEach-Object { $_.ToString('X2') }) -join '' } else { '?' }
    $act = if ($b -and $b.Length -ge 32) { (($b[16..31]) | ForEach-Object { $_.ToString('X2') }) -join '' } else { '?' }
    $cfg = if ($c) { $c.Length } else { -1 }
    $app = Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue
    $appTxt = if ($app) { "app=run(pid=" + $app.Id + ")" } else { 'app=off' }
    "{0}| flag={1} sel={2} act={3} cfgLen={4} | {5}" -f (Get-Date -Format 'HH:mm:ss'), $flag, $sel.Substring(0, 8), $act.Substring(0, 8), $cfg, $appTxt
}
Add-Content -Path $Out -Value ("=== watch start " + (Get-Date -Format 'yyyy-MM-dd HH:mm:ss') + " (1s, $Minutes min) ===") -Encoding UTF8
$last = ''; $lastBeat = Get-Date; $deadline = (Get-Date).AddMinutes($Minutes)
while ((Get-Date) -lt $deadline) {
    $s = Snapshot; $body = $s.Substring(9)
    if ($body -ne $last) { Add-Content -Path $Out -Value $s -Encoding UTF8; $last = $body }
    elseif (((Get-Date) - $lastBeat).TotalSeconds -ge 30) { Add-Content -Path $Out -Value ($s + '   [heartbeat]') -Encoding UTF8; $lastBeat = Get-Date }
    Start-Sleep -Milliseconds 1000
}
Add-Content -Path $Out -Value "=== watch end ===" -Encoding UTF8