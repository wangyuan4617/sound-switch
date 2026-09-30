# 复刻"用户手动启用 DTS Headphone:X"的界面操作，并用端点属性做客观验证。
#
# 实测流程（未启用状态下，App 的「耳机 X」页上）：
#   1) 出现 [Button] '启用 DTS 耳机 X'（无 AutomationId，只能按名字找）
#   2) 点它之后出现 '启用 数字影院系统耳机：X'（Text 元素，UIA pattern 都点不动）
#   3) 点第二个之后，Windows 侧空间音效被切成 DTS，端点属性变为：
#        {908dba32},2 flag=01000000 sel=DTS / {8a845654},2 act=DTS / {fd8a7b27},2 len=154
#   启用后界面变成"准备好了！"、按钮消失 —— 所以本脚本用**属性**判断成败。
#
# 用法：
#   -Action status       看当前状态          -Action enable       执行启用流程并验证
#   -DeviceKeyword HyperX   自动识别端点用的名称关键词
#   -AllowMouseClick     允许最后兜底用真实鼠标点击（会把窗口显示约 0.5 秒）
param(
    [ValidateSet('status', 'enable')]
    [string]$Action = 'status',
    [string]$Aumid = 'DTSInc.DTSSoundUnbound_t5j2fzbtdg37r!App',
    [string]$EndpointGuid = '',
    [string]$DeviceKeyword = 'HyperX',
    [int]$TimeoutSec = 90,
    [switch]$AllowMouseClick
)

# 自动识别耳机渲染端点（避免把某台机器的端点 GUID 写进脚本）
if (-not $EndpointGuid) {
    $EndpointGuid = (Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render' -ErrorAction SilentlyContinue |
        Where-Object { (Get-ItemProperty (Join-Path $_.PSPath 'Properties') -ErrorAction SilentlyContinue).'{b3f8fa53-0004-438e-9003-51a46e139bfc},6' -match $DeviceKeyword } |
        Select-Object -First 1).PSChildName
    if (-not $EndpointGuid) { throw "找不到名称含 '$DeviceKeyword' 的渲染端点（耳机是否已开机？）" }
}

$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class DF {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  public const uint SWP_NOZORDER=0x0004, SWP_NOACTIVATE=0x0010, SWP_SHOWWINDOW=0x0040;
  public const uint LEFTDOWN=0x0002, LEFTUP=0x0004;
  public static void Click(int x, int y) {
    SetCursorPos(x, y); System.Threading.Thread.Sleep(80);
    mouse_event(LEFTDOWN, 0,0,0, IntPtr.Zero); System.Threading.Thread.Sleep(50);
    mouse_event(LEFTUP, 0,0,0, IntPtr.Zero);
  }
}
"@
$AE = [System.Windows.Automation.AutomationElement]
$PROPS = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$EndpointGuid\Properties"
$DTS = 'B0AC4444C08D2C4CA0D82C76DB470F86'

function Get-State {
    $pv = Get-ItemProperty $PROPS -ErrorAction SilentlyContinue
    $a = $pv.'{908dba32-edff-4c28-8e45-c918561f6748},2'
    $b = $pv.'{8a845654-d6c3-4cd7-b4eb-243d4bd99032},2'
    $c = $pv.'{fd8a7b27-0b18-4025-ab1c-bdd6b32e1604},2'
    $sel = if ($a -and $a.Length -ge 40) { (($a[24..39]) | ForEach-Object { $_.ToString('X2') }) -join '' } else { '?' }
    $flag = if ($a -and $a.Length -ge 16) { (($a[12..15]) | ForEach-Object { $_.ToString('X2') }) -join '' } else { '?' }
    $act = if ($b -and $b.Length -ge 32) { (($b[16..31]) | ForEach-Object { $_.ToString('X2') }) -join '' } else { '?' }
    [pscustomobject]@{
        Flag = $flag; Sel = $sel; Act = $act
        CfgLen = if ($c) { $c.Length } else { -1 }
        Enabled = ($flag -eq '01000000' -and $sel -eq $DTS -and $act -eq $DTS)
    }
}
function Show-State($tag) {
    $s = Get-State
    Write-Host ("[{0,-18}] flag={1} sel={2} act={3} cfgLen={4} => {5}" -f `
            $tag, $s.Flag, $s.Sel.Substring(0, 8), $s.Act.Substring(0, 8), $s.CfgLen, $(if ($s.Enabled) { '已启用' } else { '未启用' }))
    return $s
}
function Frame {
    $c = New-Object System.Windows.Automation.PropertyCondition($AE::ClassNameProperty, 'ApplicationFrameWindow')
    foreach ($f in $AE::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $c)) {
        if ($f.Current.Name -match 'DTS') { return $f }
    }
    return $null
}
function Park($f) { if ($f) { [void][DF]::SetWindowPos([IntPtr]$f.Current.NativeWindowHandle, [IntPtr]::Zero, -32000, -32000, 1200, 900, ([DF]::SWP_NOZORDER -bor [DF]::SWP_NOACTIVATE)) } }
function Show-Win($f) { if ($f) { [void][DF]::SetWindowPos([IntPtr]$f.Current.NativeWindowHandle, [IntPtr]::Zero, 80, 80, 1000, 760, ([DF]::SWP_NOZORDER -bor [DF]::SWP_NOACTIVATE -bor [DF]::SWP_SHOWWINDOW)) } }
function All($f) {
    if (-not $f) { return @() }
    $a = $f.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $o = @(); for ($i = 0; $i -lt $a.Count; $i++) { $o += $a.Item($i) }; return $o
}
function By-Id($f, $id) {
    if (-not $f) { return $null }
    $c = New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, $id)
    return $f.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
}
function Try-Click($el) {
    $name = $el.Current.Name
    foreach ($p in @([System.Windows.Automation.InvokePattern]::Pattern,
            [System.Windows.Automation.TogglePattern]::Pattern,
            [System.Windows.Automation.SelectionItemPattern]::Pattern)) {
        try {
            $obj = $el.GetCurrentPattern($p)
            switch -Wildcard ($p.ProgrammaticName) {
                '*InvokePattern' { $obj.Invoke() }
                '*TogglePattern' { $obj.Toggle() }
                '*SelectionItemPattern' { $obj.Select() }
            }
            Write-Host ("    '{0}' <- {1} OK" -f $name, ($p.ProgrammaticName -replace 'PatternIdentifiers\.', ''))
            return $true
        } catch { }
    }
    try {
        $el.SetFocus(); Start-Sleep -Milliseconds 250
        [System.Windows.Forms.SendKeys]::SendWait(' ')
        Write-Host ("    '{0}' <- SetFocus+SPACE" -f $name); return $true
    } catch { }
    if ($AllowMouseClick) {
        try {
            $f = Frame; Show-Win $f; Start-Sleep -Milliseconds 350
            $pt = $el.GetClickablePoint()
            [DF]::Click([int]$pt.X, [int]$pt.Y)
            Start-Sleep -Milliseconds 60; Park $f
            Write-Host ("    '{0}' <- 鼠标点击 ({1},{2})" -f $name, [int]$pt.X, [int]$pt.Y)
            return $true
        } catch { Write-Host ("    鼠标点击失败: " + $_.Exception.Message) }
    }
    return $false
}

switch ($Action) {
    'status' {
        Write-Host ("端点: " + $EndpointGuid)
        Show-State 'status' | Out-Null
        $p = Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue
        Write-Host ("app: " + $(if ($p) { "running pid=$($p.Id)" } else { 'not running' }))
    }
    'enable' {
        $s0 = Show-State 'before'
        if ($s0.Enabled) { Write-Host "已经是启用状态；要验证启用流程，请先在 Windows 设置里把该耳机的空间音效改成『关闭』"; break }
        if (-not (Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue)) {
            Write-Host "启动 DTS Sound Unbound…"
            Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$Aumid"
            $d = (Get-Date).AddSeconds(25)
            while ((Get-Date) -lt $d) { Start-Sleep -Milliseconds 150; $f = Frame; if ($f) { Park $f; break } }
        }
        Start-Sleep -Seconds 15
        $f = Frame; Park $f
        $hpx = By-Id $f 'HPXRadioButton'
        if ($hpx) {
            try { $hpx.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Write-Host "已切到『耳机 X』页" } catch { }
            Start-Sleep -Seconds 4
        }
        $deadline = (Get-Date).AddSeconds($TimeoutSec)
        $tried = @{}
        while ((Get-Date) -lt $deadline) {
            Park (Frame)
            if ((Get-State).Enabled) { break }
            $cands = All (Frame) | Where-Object { $_.Current.IsEnabled -and $_.Current.Name -match '启用' -and $_.Current.Name -notmatch '禁用' }
            if ($cands.Count -eq 0) { Start-Sleep -Seconds 3; continue }
            foreach ($el in $cands) {
                $key = $el.Current.Name
                if ($tried.ContainsKey($key)) { continue }
                Write-Host ("  发现可点元素 '" + $key + "'")
                if (Try-Click $el) { $tried[$key] = $true; Start-Sleep -Seconds 6 }
            }
            Start-Sleep -Seconds 2
        }
        Write-Host ""
        $final = Show-State 'after'
        if ($final.Enabled) { Write-Host "启用成功（属性已变为 DTS）" } else { Write-Host "未能启用" }
        Park (Frame)
    }
}