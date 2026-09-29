# Inspect the UI of the "DTS Sound Unbound" UWP app via UI Automation.
#
# READ-ONLY with respect to DTS settings: it may launch/activate the app, resize
# its window and navigate between pages (radio buttons) to enumerate controls,
# but it never toggles a DTS feature on/off.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File docs\experiments\dump-dts-uia.ps1
#   powershell -ExecutionPolicy Bypass -File docs\experiments\dump-dts-uia.ps1 -Activate -WaitSec 15
#
# EndAction (default 'offscreen'): what to do with the app window at the end
#   offscreen = move it off-screen (app stays alive, invisible)  [default]
#   minimize  = minimize it
#   close     = send WM_CLOSE (NOTE: observed to terminate the app)
#   none      = leave it as-is

param(
    [switch]$Activate,
    [int]$WaitSec = 15,
    [int]$Width = 1200,
    [int]$Height = 900,
    [string]$OutPrefix = (Join-Path $PSScriptRoot 'dts-uia'),
    [ValidateSet('offscreen', 'minimize', 'close', 'none')]
    [string]$EndAction = 'offscreen',
    [string]$Aumid = 'DTSInc.DTSSoundUnbound_t5j2fzbtdg37r!App'
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W2 {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
  public const uint SWP_NOZORDER = 0x0004, SWP_NOACTIVATE = 0x0010;
  public const int SW_MINIMIZE = 6;
  public const uint WM_CLOSE = 0x0010;
}
"@

$AE = [System.Windows.Automation.AutomationElement]
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$IsOffscreenProp = [System.Windows.Automation.AutomationElement]::IsOffscreenProperty

function Get-AppProcs { @(Get-Process -Name 'DTSSoundUnbound*' -ErrorAction SilentlyContinue) }

function Find-Window {
    param([int[]]$Pids)
    $cond = New-Object System.Windows.Automation.PropertyCondition($AE::ClassNameProperty, 'ApplicationFrameWindow')
    $frames = $AE::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
    foreach ($f in $frames) { if ($f.Current.Name -match 'DTS') { return $f } }
    return $null
}

function Wait-Window {
    param([int]$Seconds = 25)
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        $w = Find-Window
        if ($w) { return $w }
        Start-Sleep -Milliseconds 400
    }
    return $null
}

function Find-ById {
    param($Root, [string]$AutomationId)
    $c = New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, $AutomationId)
    return $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
}

function SendKeys-Escape {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
}

$script:dumpNo = 0
function Dump-Tree {
    param($Root, [string]$Tag)
    $script:dumpNo++
    $file = '{0}-{1}-{2}.txt' -f $OutPrefix, $script:dumpNo, $Tag
    $sb = New-Object System.Text.StringBuilder
    $interesting = New-Object System.Collections.ArrayList

    function Walk {
        param($el, [int]$depth)
        if ($depth -gt 40) { return }
        $c = $el.Current
        $ct = ($c.ControlType.ProgrammaticName -replace 'ControlType\.', '')
        $line = ('{0}[{1}] name="{2}" id="{3}" class="{4}" enabled={5} offscreen={6}' -f `
            ('  ' * $depth), $ct, $c.Name, $c.AutomationId, $c.ClassName, $c.IsEnabled, $c.IsOffscreen)
        $pats = @()
        foreach ($pat in @(
                [System.Windows.Automation.InvokePattern]::Pattern,
                [System.Windows.Automation.TogglePattern]::Pattern,
                [System.Windows.Automation.SelectionItemPattern]::Pattern,
                [System.Windows.Automation.ExpandCollapsePattern]::Pattern)) {
            try { $null = $el.GetCurrentPattern($pat); $pats += ($pat.ProgrammaticName -replace 'PatternIdentifiers\.Pattern$', '') } catch { }
        }
        if ($pats.Count -gt 0) { $line += '  patterns=' + ($pats -join '|') }
        try {
            $tp = $el.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
            $line += '  toggle=' + $tp.Current.ToggleState
        } catch { }
        [void]$sb.AppendLine($line)
        if ($pats.Count -gt 0 -or $ct -match 'Text|Button|CheckBox|RadioButton|MenuItem|TabItem') {
            [void]$interesting.Add(('{0} | {1} | name="{2}" | id="{3}" | offscreen={4}' -f `
                $ct, ($pats -join '|'), $c.Name, $c.AutomationId, $c.IsOffscreen))
        }
        $child = $walker.GetFirstChild($el)
        while ($child) { Walk $child ($depth + 1); $child = $walker.GetNextSibling($child) }
    }
    Walk $Root 0
    [System.IO.File]::WriteAllText($file, $sb.ToString(), (New-Object System.Text.UTF8Encoding($false)))
    "---- [$Tag] -> $file ----"
    $interesting | ForEach-Object { '   ' + $_ }
    ""
}

# ---------------------------------------------------------------- main
"== before: app processes =="
Get-AppProcs | ForEach-Object { "  pid={0} name={1} start={2}" -f $_.Id, $_.ProcessName, $_.StartTime }

if ($Activate) {
    "== activating app (AUMID) =="
    Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$Aumid"
}

$win = Wait-Window -Seconds 30
if (-not $win) { throw "No 'DTS' window found within 30s" }
$hwnd = [IntPtr]$win.Current.NativeWindowHandle
"== window: name='{0}' hwnd=0x{1:X} pid={2}" -f $win.Current.Name, [int64]$hwnd, $win.Current.ProcessId

"== waiting {0}s for the license check to settle ==" -f $WaitSec
Start-Sleep -Seconds $WaitSec

# enlarge so XAML does not virtualize away list content.
# NOTE: the window is kept off-screen (-32000) the whole time so the user never
# sees it; UIA works fine for off-screen windows.
if ($Width -gt 0 -and $Height -gt 0 -and $hwnd -ne [IntPtr]::Zero) {
    [void][W2]::SetWindowPos($hwnd, [IntPtr]::Zero, -32000, -32000, $Width, $Height, ([W2]::SWP_NOZORDER -bor [W2]::SWP_NOACTIVATE))
    Start-Sleep -Seconds 2
    "== resized to ${Width}x${Height} (off-screen) =="
}

$win = Find-Window
Dump-Tree -Root $win -Tag 'home'

function Select-ById {
    param($Root, [string]$AutomationId, [int]$SettleSec = 3)
    $el = Find-ById -Root $Root -AutomationId $AutomationId
    if (-not $el) { "!! element not found: $AutomationId"; return $false }
    try {
        $el.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    } catch {
        "!! select failed for ${AutomationId}: $($_.Exception.Message)"; return $false
    }
    Start-Sleep -Seconds $SettleSec
    return $true
}

$hpx = Find-ById -Root $win -AutomationId 'HPXRadioButton'
if ($hpx) {
    "== navigating to the 'Headphone X' page =="
    [void](Select-ById -Root $win -AutomationId 'HPXRadioButton' -SettleSec 5)
    $win = Find-Window
    Dump-Tree -Root $win -Tag 'hpx'
} else { "!! HPXRadioButton not found" }

# The HPX page has tabs (video / configure / explore). Select each by index so
# this script does not depend on localized tab names.
$win = Find-Window
$tabCond = New-Object System.Windows.Automation.PropertyCondition(
    $AE::ControlTypeProperty, [System.Windows.Automation.ControlType]::TabItem)
$tabs = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $tabCond)
"== HPX tabs found: {0} ==" -f $tabs.Count
for ($i = 0; $i -lt $tabs.Count; $i++) {
    $tab = $tabs.Item($i)
    "== selecting tab #{0} name='{1}' ==" -f $i, $tab.Current.Name
    try {
        $tab.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    } catch {
        "!! tab select failed: $($_.Exception.Message)"; continue
    }
    Start-Sleep -Seconds 4
    $win = Find-Window
    Dump-Tree -Root $win -Tag ("hpx-tab{0}" -f $i)
}

$homeBtn = Find-ById -Root $win -AutomationId 'HomeRadioButton'
if ($homeBtn) { [void](Select-ById -Root $win -AutomationId 'HomeRadioButton' -SettleSec 3) }

$win = Find-Window
$dots = Find-ById -Root $win -AutomationId 'Dotsx3Button'
if ($dots) {
    "== opening the 'more options' menu =="
    $dots.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Seconds 2
    Dump-Tree -Root (Find-Window) -Tag 'menu'
    SendKeys-Escape
} else { "!! Dotsx3Button not found" }

"== end action: $EndAction =="
switch ($EndAction) {
    'offscreen' {
        if ($hwnd -ne [IntPtr]::Zero) {
            [void][W2]::SetWindowPos($hwnd, [IntPtr]::Zero, -32000, -32000, $Width, $Height, ([W2]::SWP_NOZORDER -bor [W2]::SWP_NOACTIVATE))
        }
    }
    'minimize' { if ($hwnd -ne [IntPtr]::Zero) { [void][W2]::ShowWindowAsync($hwnd, [W2]::SW_MINIMIZE) } }
    'close' { if ($hwnd -ne [IntPtr]::Zero) { [void][W2]::PostMessageW($hwnd, [W2]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) } }
    'none' { }
}
Start-Sleep -Seconds 2
"== after: app processes =="
Get-AppProcs | ForEach-Object { "  pid={0} name={1} win={2}" -f $_.Id, $_.ProcessName, $_.MainWindowHandle }
