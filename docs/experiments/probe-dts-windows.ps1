# Probe: which top-level windows belong to the DTS app, and which HWND really closes it.
# Read-only except that it launches/activates the app and finally tries WM_CLOSE.
param(
    [string]$Aumid = 'DTSInc.DTSSoundUnbound_t5j2fzbtdg37r!App',
    [switch]$NoClose
)

$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class WX {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr p, IntPtr c, string cls, string win);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
}
"@

$p = Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue
if (-not $p) {
    "app not running -> launching"
    Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$Aumid"
    Start-Sleep -Seconds 8
    $p = Get-Process DTSSoundUnbound2 -ErrorAction SilentlyContinue
}
if (-not $p) { throw "app still not running" }
$app = $p.Id
"app pid = $app"
""

$script:rows = New-Object System.Collections.ArrayList
$cb = [WX+EnumProc] {
    param($h, $l)
    $wpid = 0
    [void][WX]::GetWindowThreadProcessId($h, [ref]$wpid)
    $cls = New-Object System.Text.StringBuilder 256
    [void][WX]::GetClassNameW($h, $cls, 256)
    $txt = New-Object System.Text.StringBuilder 256
    [void][WX]::GetWindowTextW($h, $txt, 256)

    if ($wpid -eq $app) {
        [void]$script:rows.Add(("OWNS      hwnd=0x{0:X8} class={1,-32} visible={2,-5} title='{3}'" -f `
                    [int64]$h, $cls.ToString(), [WX]::IsWindowVisible($h), $txt.ToString()))
    }
    $core = [WX]::FindWindowExW($h, [IntPtr]::Zero, 'Windows.UI.Core.CoreWindow', $null)
    if ($core -ne [IntPtr]::Zero) {
        $cp = 0
        [void][WX]::GetWindowThreadProcessId($core, [ref]$cp)
        [void]$script:rows.Add(("FRAME?    hwnd=0x{0:X8} class={1,-32} visible={2,-5} corepid={3} title='{4}'" -f `
                    [int64]$h, $cls.ToString(), [WX]::IsWindowVisible($h), $cp, $txt.ToString()))
    }
    return $true
}
[void][WX]::EnumWindows($cb, [IntPtr]::Zero)
$script:rows | ForEach-Object { $_ }

# pick the frame window whose CoreWindow child belongs to the app
$frame = [IntPtr]::Zero
$cb2 = [WX+EnumProc] {
    param($h, $l)
    $core = [WX]::FindWindowExW($h, [IntPtr]::Zero, 'Windows.UI.Core.CoreWindow', $null)
    if ($core -ne [IntPtr]::Zero) {
        $cp = 0
        [void][WX]::GetWindowThreadProcessId($core, [ref]$cp)
        if ($cp -eq $app) { $script:frame = $h; return $false }
    }
    return $true
}
[void][WX]::EnumWindows($cb2, [IntPtr]::Zero)
""
"chosen frame hwnd = 0x{0:X8}" -f [int64]$script:frame

if ($script:frame -ne [IntPtr]::Zero -and -not $NoClose) {
    "sending WM_CLOSE to the frame window..."
    [void][WX]::PostMessageW($script:frame, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    for ($i = 1; $i -le 6; $i++) {
        Start-Sleep -Seconds 1
        $still = Get-Process -Id $app -ErrorAction SilentlyContinue
        $win = [WX]::IsWindowVisible($script:frame)
        "  [{0}s] process alive={1} frameVisible={2}" -f $i, [bool]$still, $win
        if (-not $still) { break }
    }
    $still = Get-Process -Id $app -ErrorAction SilentlyContinue
    if ($still) { "RESULT: process still alive after WM_CLOSE (app closes to background / ignores it)" }
    else { "RESULT: WM_CLOSE terminated the app" }
}
