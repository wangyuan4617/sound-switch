# 快照差异对比（只读）
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File docs\experiments\diff-snapshots.ps1 `
#       -Before docs\experiments\dts-before.txt -After docs\experiments\dts-after.txt

param(
    [Parameter(Mandatory = $true)][string]$Before,
    [Parameter(Mandatory = $true)][string]$After
)

$b = Get-Content $Before -Encoding UTF8
$a = Get-Content $After -Encoding UTF8

$diff = Compare-Object -ReferenceObject $b -DifferenceObject $a -CaseSensitive

if (-not $diff) {
    Write-Output "两次快照完全一致（说明该设置未写入这两个根键下的注册表）"
    exit 0
}

Write-Output "=== 差异（=> 表示新增/变化后的值，<= 表示变化前的值） ==="
$diff | ForEach-Object {
    $mark = if ($_.SideIndicator -eq '=>') { '=>' } else { '<=' }
    "$mark $($_.InputObject)"
}
Write-Output ""
Write-Output ("差异行数: " + $diff.Count)
