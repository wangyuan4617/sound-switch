<#
.SYNOPSIS
    生成“便携部署包”，便于拷到新电脑 / 重装系统后直接使用。

.DESCRIPTION
    产物：
      <dist>\sound-switch-portable\       可以整个文件夹拷走
      <dist>\sound-switch-portable.zip    压缩包（便于传输）

    包内含：两个 exe、config.json、安装/卸载脚本、说明文档、源码（便于在新机器重新编译）。
    不含：state.json、logs\、target\（运行时会自动生成 / 体积大且可重建）。

.PARAMETER OutDir
    输出目录，默认 <项目根>\dist

.PARAMETER SkipBuild
    即使缺少 exe 也不自动执行 cargo build。

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\make-package.ps1
    powershell -ExecutionPolicy Bypass -File scripts\make-package.ps1 -OutDir D:\dist
#>
[CmdletBinding()]
param(
    [string]$PackageName = 'sound-switch-portable',
    [string]$OutDir,
    [switch]$SkipBuild,
    [switch]$NoZip
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $root 'dist' }
$pkg = Join-Path $OutDir $PackageName

# ---------- 1) 准备 exe ----------
$exeMain = Join-Path $root 'target\release\sound-switch.exe'
$exeLauncher = Join-Path $root 'target\release\sound-switch-launch.exe'

if (-not (Test-Path $exeMain) -or -not (Test-Path $exeLauncher)) {
    if ($SkipBuild) { throw "缺少 exe（请先 cargo build --release）：$exeMain" }
    $running = Get-Process sound-switch, sound-switch-launch -ErrorAction SilentlyContinue
    if ($running) {
        Write-Warning ("检测到程序正在运行（会锁定 exe，导致编译失败）：PID " + ($running.Id -join ', '))
        Write-Warning "请先停止：Get-Process sound-switch | Stop-Process"
        throw "编译前请先停止正在运行的实例"
    }
    Write-Output "未找到 exe，正在执行 cargo build --release ..."
    Push-Location $root
    try { cargo build --release } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw "cargo build 失败" }
}

# ---------- 2) 复制文件 ----------
if (Test-Path $pkg) { Remove-Item $pkg -Recurse -Force }
New-Item -ItemType Directory -Path $pkg -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $pkg 'scripts') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $pkg 'docs') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $pkg '.cargo') -Force | Out-Null

# 2.1 可执行文件（放在包根目录，与 config.json 同级 —— 便携结构）
Copy-Item $exeMain     (Join-Path $pkg 'sound-switch.exe') -Force
Copy-Item $exeLauncher (Join-Path $pkg 'sound-switch-launch.exe') -Force

# 2.2 配置与参考实现
foreach ($f in 'config.json', 'README.md', 'index.js', 'Cargo.toml', 'Cargo.lock') {
    $src = Join-Path $root $f
    if (Test-Path $src) { Copy-Item $src (Join-Path $pkg $f) -Force }
}

# 2.3 脚本
Copy-Item (Join-Path $root 'scripts\*.ps1') (Join-Path $pkg 'scripts') -Force

# 2.4 构建配置（静态链接 CRT，保证新机器无需 VC++ 运行库）
$cargoCfg = Join-Path $root '.cargo\config.toml'
if (Test-Path $cargoCfg) { Copy-Item $cargoCfg (Join-Path $pkg '.cargo\config.toml') -Force }

# 2.5 源码
if (Test-Path (Join-Path $root 'src')) {
    Copy-Item (Join-Path $root 'src') (Join-Path $pkg 'src') -Recurse -Force
}

# 2.6 文档：md 全带，实验快照（体积大且是旧机器的记录）不带
Copy-Item (Join-Path $root 'docs\*.md') (Join-Path $pkg 'docs') -Force
$expSrc = Join-Path $root 'docs\experiments'
if (Test-Path $expSrc) {
    $expDst = Join-Path $pkg 'docs\experiments'
    New-Item -ItemType Directory -Path $expDst -Force | Out-Null
    Copy-Item (Join-Path $expSrc '*.ps1') $expDst -Force -ErrorAction SilentlyContinue
    Copy-Item (Join-Path $expSrc 'FINDINGS.md') $expDst -Force -ErrorAction SilentlyContinue
}

# 2.7 部署说明：docs\DEPLOY.md 复制一份到包根，命名为“部署说明.md”
$deploy = Join-Path $root 'docs\DEPLOY.md'
if (Test-Path $deploy) { Copy-Item $deploy (Join-Path $pkg '部署说明.md') -Force }

# ---------- 3) 打包 zip ----------
$zip = Join-Path $OutDir "$PackageName.zip"
if (-not $NoZip) {
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $pkg '*') -DestinationPath $zip -Force
}

# ---------- 4) 汇总 ----------
$files = Get-ChildItem $pkg -Recurse -File
$sizeMB = [math]::Round((($files | Measure-Object -Property Length -Sum).Sum) / 1MB, 2)
Write-Output ""
Write-Output "便携包已生成："
Write-Output ("  文件夹 : " + $pkg)
if (-not $NoZip) {
    $zipMB = [math]::Round((Get-Item $zip).Length / 1MB, 2)
    Write-Output ("  压缩包 : " + $zip + "  ($zipMB MB)")
}
Write-Output ("  文件数 : " + $files.Count + "  合计 $sizeMB MB")
Write-Output ""
Write-Output "新电脑上的使用步骤（详见 部署说明.md）："
Write-Output "  1. 拷贝该文件夹到新电脑（如 D:\tools\sound-switch）"
Write-Output "  2. .\sound-switch.exe list          # 确认能看到耳机 HID 与音频端点"
Write-Output "  3. .\sound-switch.exe set-default   # 手动验证切换（再用 restore 恢复）"
Write-Output "  4. powershell -ExecutionPolicy Bypass -File scripts\install-autostart.ps1 -StartNow"
Write-Output ""
Write-Output "别忘了在新电脑上重设一次：设置 → 系统 → 声音 → 耳机 → 空间音效 → DTS Headphone:X"
