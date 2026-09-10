# 音频注册表快照工具（只读）
#
# 用途：为"耳机开机自动切换到 DTS 空间音效"做可行性实验 ——
#       在手动切换「空间音效」下拉框前后各拍一次快照，对比出对应的注册表属性。
#
# 用法（本机只有 Windows PowerShell 5.1，脚本已存为 UTF-8 with BOM，可直接运行）：
#   powershell -ExecutionPolicy Bypass -File docs\experiments\snapshot-audio-registry.ps1 -Out docs\experiments\dts-before.txt
#
# 说明：本脚本只执行 reg query（读取），不写入、不修改任何注册表内容。

param(
    [Parameter(Mandatory = $true)][string]$Out
)

# 让 reg.exe 的输出按 UTF-8 解码，避免中文设备名乱码
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$roots = @(
    'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices',
    'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Audio',
    'HKCU\SOFTWARE\Microsoft\Multimedia',
    'HKCU\SOFTWARE\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\SystemAppData\DTSInc.DTSSoundUnbound_t5j2fzbtdg37r',
    'HKCU\SOFTWARE\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\SystemAppData\DTSInc.DTSCustomforAsus_t5j2fzbtdg37r'
)

$dir = Split-Path -Parent $Out
if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }

$header = @(
    "# 音频注册表快照",
    "# 文件: $Out",
    "# 时间: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')",
    "# 说明: 只读快照，用于对比「空间音效」设置变化前后的注册表差异"
)
$header | Set-Content -Path $Out -Encoding UTF8

foreach ($root in $roots) {
    Add-Content -Path $Out -Value "" -Encoding UTF8
    Add-Content -Path $Out -Value "===== $root =====" -Encoding UTF8
    reg query $root /s 2>&1 | Add-Content -Path $Out -Encoding UTF8
}

Write-Output ("Snapshot written: $Out")
Write-Output ("Lines: " + (Get-Content $Out | Measure-Object).Count)
