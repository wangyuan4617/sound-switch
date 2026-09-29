# Trace the DTS app's top-level windows over time:
#   - windows owned by the app process itself
#   - ApplicationFrameWindow whose title is "DTS Sound Unbound" (owner = ApplicationFrameHost)
#   - the PID of its Windows.UI.Core.CoreWindow child (that is the app's process)
param(
    [string]$Aumid = 'DTSInc.DTSSoundUnbound_t5j2fzbtdg37r!App',
    [int]$Seconds = 45
)

$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class WY {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr p, IntPtr c, string cls, string win);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
"@

function Snapshot {
    param([int]$AppPid)
    $out = New-Object System.Collections.ArrayList
    $cb = [WY+EnumProc] {
        param($h, $l)
        $wpid = 0
        [void][WY]::GetWindowThreadProcessId($h, [ref]$wpid)
        $cls = New-Object System.Text.StringBuilder 256
        [void][WY]::GetClassNameW($h, $cls, 256)
        $txt = New-Object System.Text.StringBuilder 256
        [void][WY]::GetWindowTextW($h, $txt, 256)
        $c = $cls.ToString(); $t = $txt.ToString()
        $core = [WY]::FindWindowExW($h, [IntPtr]::Zero, 'Windows.UI.Core.CoreWindow', $null)
        $cp = -1
        if ($core -ne [IntPtr]::Zero) { [void][WY]::GetWindowThreadProcessId($core, [ref]$cp) }

        if ($wpid -eq $AppPid) {
            [void]$out.Add(("own:0x{0:X} {1} vis={2}" -f [int64]$h, $c, [WY]::IsWindowVisible($h)))
        }
        if ($c -eq 'ApplicationFrameWindow' -and $t -match 'DTS') {
            [void]$out.Add(("FRAME:0x{0:X} vis={1} corepid={2} (app={3})" -f [int64]$h, [WY]::IsWindowVisible($h), $cp, $AppPid))
        }
        return $true
    }
    [void][WY]::EnumWindows($cb, [IntPtr]::Zero)
    return ($out -join ' | ')
}

$app = Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue
"=== PHASE 1: app already running? pid = " + $(if ($app) { $app.Id } else { 'none' }) + " ==="
if ($app) {
    for ($i = 0; $i -le 10; $i++) {
        "  [{0,3}s] {1}" -f $i, (Snapshot -AppPid $app.Id)
        Start-Sleep -Seconds 1
    }
    "=== killing for a clean test ==="
    Stop-Process -Id $app.Id -Force
    Start-Sleep -Seconds 2
}

"=== PHASE 2: fresh launch by AUMID, trace {0}s ===" -f $Seconds
Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$Aumid"
for ($i = 0; $i -le $Seconds; $i++) {
    $p = Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue
    if (-not $p) { "  [{0,3}s] (no process yet)" -f $i }
    else { "  [{0,3}s] pid={1} {2}" -f $i, $p.Id, (Snapshot -AppPid $p.Id) }
    Start-Sleep -Seconds 1
}
