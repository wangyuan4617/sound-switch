<#
.SYNOPSIS
    生成“绿色便携包”，便于拷到新电脑 / 重装系统后直接使用。

.DESCRIPTION
    默认产物（精简，只含运行与安装所必需的文件）：
      <dist>\sound-switch-portable\
        sound-switch.exe            主程序
        sound-switch-launch.exe     无窗口启动器（登录自启用）
        config.json                 配置（含当前调好的设置）
        scripts\install-autostart.ps1
        scripts\uninstall-autostart.ps1
      <dist>\sound-switch-portable.zip   压缩包（便于传输）

    不含：state.json、logs\、target\、文档、源码（后两者可用 -WithDocs / -WithSource 追加）。

.PARAMETER WithDocs
    额外打包文档（README.md、docs\*.md，并附一份“部署说明.md”）。

.PARAMETER WithSource
    额外打包源码与构建配置（src\、Cargo.toml、Cargo.lock、.cargo\config.toml），
    便于在新电脑上重新编译。

.PARAMETER OutDir
    输出目录，默认 <项目根>\dist

.PARAMETER SkipBuild
    即使缺少 exe 也不自动执行 cargo build。

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\make-package.ps1
    powershell -ExecutionPolicy Bypass -File scripts\make-package.ps1 -WithSource
#>
[CmdletBinding()]
param(
    [string]$PackageName = 'sound-switch-portable',
    [string]$OutDir,
    [switch]$WithDocs,
    [switch]$WithSource,
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

# ---------- 2) 组装包（精简） ----------
if (Test-Path $pkg) { Remove-Item $pkg -Recurse -Force }
New-Item -ItemType Directory -Path $pkg -Force | Out-Null

# 2.1 可执行文件（放在包根目录，与 config.json 同级 —— 便携结构）
Copy-Item $exeMain (Join-Path $pkg 'sound-switch.exe') -Force
Copy-Item $exeLauncher (Join-Path $pkg 'sound-switch-launch.exe') -Force

# 2.2 配置：带上当前调好的设置（不含任何机器相关信息，换机可直接用）
Copy-Item (Join-Path $root 'config.json') (Join-Path $pkg 'config.json') -Force

# 2.3 安装 / 卸载脚本
New-Item -ItemType Directory -Path (Join-Path $pkg 'scripts') -Force | Out-Null
foreach ($s in 'install-autostart.ps1', 'uninstall-autostart.ps1') {
    Copy-Item (Join-Path $root "scripts\$s") (Join-Path $pkg "scripts\$s") -Force
}

# 2.4 可选：文档
if ($WithDocs) {
    New-Item -ItemType Directory -Path (Join-Path $pkg 'docs') -Force | Out-Null
    Get-ChildItem (Join-Path $root 'docs') -Filter *.md -File | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $pkg 'docs') -Force
    }
    Copy-Item (Join-Path $root 'README.md') (Join-Path $pkg 'README.md') -Force
    Copy-Item (Join-Path $root 'docs\DEPLOY.md') (Join-Path $pkg '部署说明.md') -Force
}

# 2.5 可选：源码与构建配置
if ($WithSource) {
    Copy-Item (Join-Path $root 'src') (Join-Path $pkg 'src') -Recurse -Force
    foreach ($f in 'Cargo.toml', 'Cargo.lock') {
        if (Test-Path (Join-Path $root $f)) { Copy-Item (Join-Path $root $f) (Join-Path $pkg $f) -Force }
    }
    New-Item -ItemType Directory -Path (Join-Path $pkg '.cargo') -Force | Out-Null
    $cargoCfg = Join-Path $root '.cargo\config.toml'
    if (Test-Path $cargoCfg) { Copy-Item $cargoCfg (Join-Path $pkg '.cargo\config.toml') -Force }
}

# ---------- 3) 打包 zip ----------
$zip = Join-Path $OutDir "$PackageName.zip"
if (-not $NoZip) {
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $pkg '*') -DestinationPath $zip -Force
}

# ---------- 4) 汇总 ----------
Write-Output ""
Write-Output "便携包已生成："
Write-Output ("  文件夹 : " + $pkg)
if (-not $NoZip) {
    $zipMB = [math]::Round((Get-Item $zip).Length / 1MB, 2)
    Write-Output ("  压缩包 : " + $zip + "  ($zipMB MB)")
}
Write-Output "  内容   :"
$rootLen = (Resolve-Path $pkg).Path.Length + 1
Get-ChildItem $pkg -Recurse -File | ForEach-Object {
    Write-Output ("    " + $_.FullName.Substring($rootLen) + "   (" + [math]::Round($_.Length / 1KB, 0) + " KB)")
}
if (-not ($WithDocs -and $WithSource)) {
    Write-Output "  （如需附带文档/源码：加 -WithDocs / -WithSource 参数重新生成）"
}
Write-Output ""
Write-Output "新电脑上的使用步骤："
Write-Output "  1. 把文件夹（或解压后的 zip）放到任意位置，例如 D:\GreenTools\sound-switch"
Write-Output "  2. cd 到该目录；.\sound-switch.exe list        # 确认能看到耳机 HID 与音频端点"
Write-Output "  3. .\sound-switch.exe set-default              # 手动验证切换（再用 restore 恢复）"
Write-Output "  4. powershell -ExecutionPolicy Bypass -File scripts\install-autostart.ps1 -StartNow"
Write-Output ""
Write-Output "提醒：新电脑上重设一次 设置 → 系统 → 声音 → 耳机 → 空间音效 → DTS Headphone:X"
