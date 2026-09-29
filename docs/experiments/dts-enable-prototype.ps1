# Prototype: drive "DTS Sound Unbound" silently (no visible window, no manual click).
#
# Actions:
#   status : report whether the app runs + its license-tile state (UIA)
#   enable : launch the app if needed, keep its window OFF-SCREEN, wait for the
#            license check, and click the license tile / "refresh licenses" if it
#            still reports "not licensed"
#   kill   : close the app (to test what happens to the audio effect without it)
#   props  : print the headset endpoint's spatial-sound registry properties
#
# All user-visible strings are matched by AutomationId or by [char] codes, so this
# file is pure ASCII (PowerShell 5.1 reads non-BOM files as ANSI).

param(
    [ValidateSet('status', 'enable', 'kill', 'props')]
    [string]$Action = 'status',
    [int]$TimeoutSec = 45,
    [string]$Aumid = 'DTSInc.DTSSoundUnbound_t5j2fzbtdg37r!App',
    [switch]$WaitForInput
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W3 {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
  public const uint SWP_NOZORDER = 0x0004, SWP_NOACTIVATE = 0x0010, WM_CLOSE = 0x0010;
}
"@

$AE = [System.Windows.Automation.AutomationElement]
$UNLICENSED = [string]([char]0x672A + [char]0x6388 + [char]0x6743)   # "not licensed"
$LICENSED = [string]([char]0x5DF2 + [char]0x6388 + [char]0x6743)     # "licensed"
$PARK_X = -32000
$PARK_Y = -32000
$PARK_W = 1200
$PARK_H = 900

function Get-AppProcs { @(Get-Process -Name 'DTSSoundUnbound*' -ErrorAction SilentlyContinue) }

function Find-Frame {
    $cond = New-Object System.Windows.Automation.PropertyCondition($AE::ClassNameProperty, 'ApplicationFrameWindow')
    $frames = $AE::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
    foreach ($f in $frames) { if ($f.Current.Name -match 'DTS') { return $f } }
    return $null
}

function Find-ById {
    param($Root, [string]$AutomationId)
    if (-not $Root) { return $null }
    $c = New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, $AutomationId)
    return $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
}

function Park-Window {
    param($Frame)
    if (-not $Frame) { return }
    $h = [IntPtr]$Frame.Current.NativeWindowHandle
    if ($h -eq [IntPtr]::Zero) { return }
    [void][W3]::SetWindowPos($h, [IntPtr]::Zero, $PARK_X, $PARK_Y, $PARK_W, $PARK_H,
        ([W3]::SWP_NOZORDER -bor [W3]::SWP_NOACTIVATE))
}

function Get-TileState {
    $frame = Find-Frame
    if (-not $frame) { return $null }
    $tile = Find-ById -Root $frame -AutomationId 'DTSXHPNewLicenseTile'
    if (-not $tile) { return $null }
    $name = $tile.Current.Name
    $state = 'unknown'
    if ($name.Contains($LICENSED)) { $state = 'licensed' }
    elseif ($name.Contains($UNLICENSED)) { $state = 'not-licensed' }
    return [pscustomobject]@{ State = $state; Name = $name }
}

function Show-SpatialProps {
    $root = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render'
    foreach ($dev in Get-ChildItem $root) {
        $props = Join-Path $dev.PSPath 'Properties'
        $pv = Get-ItemProperty $props -ErrorAction SilentlyContinue
        if (-not $pv) { continue }
        $desc = $pv.'{b3f8fa53-0004-438e-9003-51a46e139bfc},6'
        if ($desc -notmatch 'HyperX') { continue }
        "endpoint {0}  ({1})" -f $dev.PSChildName, $desc
        foreach ($pn in '{908dba32-edff-4c28-8e45-c918561f6748},2',
            '{8a845654-d6c3-4cd7-b4eb-243d4bd99032},2',
            '{fd8a7b27-0b18-4025-ab1c-bdd6b32e1604},2') {
            $v = $pv.$pn
            if ($null -eq $v) { "  {0} = <absent>" -f $pn }
            elseif ($v -is [byte[]]) { "  {0} = {1}" -f $pn, (($v | ForEach-Object { $_.ToString('X2') }) -join '') }
            else { "  {0} = {1}" -f $pn, $v }
        }
    }
}

function Invoke-ById {
    param($Frame, [string]$AutomationId)
    $el = Find-ById -Root $Frame -AutomationId $AutomationId
    if (-not $el) { "   !! not found: $AutomationId"; return $false }
    try {
        $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        "   invoked: $AutomationId  (name='$($el.Current.Name)')"
        return $true
    } catch {
        "   !! invoke failed for ${AutomationId}: $($_.Exception.Message)"
        return $false
    }
}

# ------------------------------------------------------------------ actions
switch ($Action) {

    'props' { Show-SpatialProps }

    'status' {
        $p = Get-AppProcs
        if (-not $p) { "app: NOT running" }
        else { $p | ForEach-Object { "app: running pid={0} start={1} win={2}" -f $_.Id, $_.StartTime, $_.MainWindowHandle } }
        $t = Get-TileState
        if ($t) { "license tile: {0}  ('{1}')" -f $t.State, $t.Name } else { "license tile: <no window / not readable>" }
    }

    'kill' {
        $p = Get-AppProcs
        if (-not $p) { "app: already not running" }
        foreach ($proc in $p) {
            $frame = Find-Frame
            if ($frame) {
                $h = [IntPtr]$frame.Current.NativeWindowHandle
                [void][W3]::PostMessageW($h, [W3]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero)
                "sent WM_CLOSE to the app window (pid {0})" -f $proc.Id
            }
            Start-Sleep -Seconds 3
            $still = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
            if ($still) {
                Stop-Process -Id $proc.Id -Force
                "process {0} did not exit; force-killed" -f $proc.Id
            } else { "process {0} exited" -f $proc.Id }
        }
        Start-Sleep -Seconds 2
        if (Get-AppProcs) { "app: STILL running" } else { "app: stopped" }
    }

    'enable' {
        $proc = Get-AppProcs
        if (-not $proc) {
            "app not running -> launching by AUMID (silently)"
            Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$Aumid"
            # park the window as soon as it exists so it is never seen
            $deadline = (Get-Date).AddSeconds(20)
            while ((Get-Date) -lt $deadline) {
                Start-Sleep -Milliseconds 150
                $f = Find-Frame
                if ($f) { Park-Window -Frame $f; break }
            }
            Start-Sleep -Seconds 1
            Park-Window -Frame (Find-Frame)
        } else {
            "app already running (pid {0}) -> parking window" -f ($proc | Select-Object -First 1).Id
            Park-Window -Frame (Find-Frame)
        }

        # wait for the license check to settle
        $deadline = (Get-Date).AddSeconds($TimeoutSec)
        $state = $null
        while ((Get-Date) -lt $deadline) {
            $t = Get-TileState
            if ($t -and $t.State -ne 'unknown') { $state = $t; break }
            Start-Sleep -Milliseconds 700
        }
        if ($state) { "license tile after load: {0} ('{1}')" -f $state.State, $state.Name }
        else { "license tile: could not be read within {0}s" -f $TimeoutSec }

        if (-not $state -or $state.State -eq 'not-licensed') {
            "-> clicking the license tile and 'refresh licenses'"
            $frame = Find-Frame
            Park-Window -Frame $frame
            [void](Invoke-ById -Frame $frame -AutomationId 'DTSXHPNewLicenseTile')
            Start-Sleep -Seconds 3
            # also try the menu item "refresh licenses"
            $frame = Find-Frame
            if (Invoke-ById -Frame $frame -AutomationId 'Dotsx3Button') {
                Start-Sleep -Seconds 2
                [void](Invoke-ById -Frame (Find-Frame) -AutomationId 'RefreshLicensesButton')
                Start-Sleep -Seconds 1
            }
            # close the flyout if it is still open
            Add-Type -AssemblyName System.Windows.Forms
            [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
            $deadline = (Get-Date).AddSeconds($TimeoutSec)
            while ((Get-Date) -lt $deadline) {
                Start-Sleep -Milliseconds 700
                $t = Get-TileState
                if ($t -and $t.State -eq 'licensed') { $state = $t; break }
            }
            if ($state) { "license tile after click: {0} ('{1}')" -f $state.State, $state.Name }
        }

        Park-Window -Frame (Find-Frame)
        "done. app " + $(if (Get-AppProcs) { 'running, window parked off-screen' } else { 'is NOT running' })
    }
}

if ($WaitForInput) { [void](Read-Host 'press Enter to close') }
