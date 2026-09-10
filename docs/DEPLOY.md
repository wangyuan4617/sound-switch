# 迁移 / 部署到新电脑（重装系统或换机）

> 本程序是"绿色"的：所有状态（`config.json`、`state.json`、`logs\`）都在程序自己的文件夹里，
> 不写系统目录、不依赖安装过程。**整个文件夹拷过去就是全部**，唯一需要在新机器上重做的是
> 「注册登录自启任务」这一步（以及 DTS 空间音效的手动设置一次）。

## 一、推荐流程：用便携包（新电脑不需要装 Rust）

### 1. 在旧电脑上生成便携包

```powershell
cd <项目目录>
powershell -ExecutionPolicy Bypass -File scripts\make-package.ps1
```

产物：

- `dist\sound-switch-portable\`（文件夹，可直接拷）
- `dist\sound-switch-portable.zip`（压缩包，便于传输）

便携包默认是**精简版**，只含运行与安装所必需的文件：

```
sound-switch.exe              # 主程序
sound-switch-launch.exe       # 无窗口启动器（登录自启用）
config.json                   # 配置（含调好的设置）
scripts\install-autostart.ps1
scripts\uninstall-autostart.ps1
```

**不包含**旧机器的 `state.json` 和 `logs\`（不需要带过去），也不含文档与源码；
需要时可用 `-WithDocs` / `-WithSource` 追加：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\make-package.ps1 -WithSource -WithDocs
```

### 2. 拷到新电脑

把文件夹（或 zip 解压后）放到合适位置，例如 `D:\tools\sound-switch`。

> 建议放**英文路径**；中文/空格路径一般也能用，但未充分测试。
> 程序对目录位置没有要求 —— 脚本会自动按"脚本所在位置"推导工作目录。

### 3. 在新电脑上验证（3 条命令）

```powershell
cd D:\tools\sound-switch

# ① 能否看到耳机：应列出匹配的 HID 集合，且渲染端点里有"关键词匹配"标记
.\sound-switch.exe list

# ② 手动验证切换/恢复是否可用
.\sound-switch.exe set-default     # 切到耳机
.\sound-switch.exe restore         # 恢复切换前的设备

# ③ 安装登录自启（并立即启动一次）
powershell -ExecutionPolicy Bypass -File scripts\install-autostart.ps1 -StartNow
```

确认自启已在后台无窗口运行：

```powershell
Get-Process sound-switch | Select Id, MainWindowHandle   # MainWindowHandle 应为 0
Get-Content .\logs\sound_switch.log -Tail 3 -Encoding UTF8
```

### 4. 收尾

重新登录或重启一次，确认自启生效（`Get-ScheduledTaskInfo -TaskName sound-switch` 的
`LastTaskResult` 应为 0）。

## 二、不用打包：手工拷这几个文件也够

最小集合（放在同一个文件夹里）：

```
sound-switch.exe
sound-switch-launch.exe
config.json
scripts\install-autostart.ps1
scripts\uninstall-autostart.ps1
```

然后在那个文件夹里运行 install 脚本即可（脚本会自动找到同目录的 exe）。

## 三、在新电脑上从源码编译（可选）

适合想改代码、或新机器是 ARM64 等需要自己编译的情况：

1. 安装 Rust：<https://rustup.rs>（`rustup` 会自动提示安装 MSVC 生成工具）
2. 安装 "Visual Studio Build Tools"（勾选 **C++ 生成工具**）
3. 拿到源码（拷贝 `src\`、`Cargo.toml`、`Cargo.lock`、`.cargo\config.toml`，或 git clone）
4. 编译：

```powershell
cargo build --release
# 产物：target\release\sound-switch.exe 与 sound-switch-launch.exe
```

## 四、新电脑上的注意事项（重要）

| # | 事项 | 说明 |
|---|---|---|
| 1 | **音频端点 GUID 会变** | 不用管：程序按 `config.json` 里的 `audio_keyword`（名称关键词）在运行时查找设备，不依赖 GUID。**不要把旧机器的 `state.json` 拷过来**（那是旧机器的记录，没有意义）。 |
| 2 | **DTS 空间音效要重设一次** | 该设置按音频端点保存在注册表里，不会跟着程序走。设置一次即长期有效：**设置 → 系统 → 声音 → 耳机 → 空间音效 → DTS Headphone:X**（需先在新机器装好 DTS Sound Unbound）。详见 `docs/experiments/FINDINGS.md`。 |
| 3 | **设备名不同怎么办** | 先 `.\sound-switch.exe list` 看实际名字，把 `config.json` 的 `audio_keyword` 改成实际名称里的独特片段（例如 `HyperX Cloud Stinger Core Wireless`）；换的是别的耳机型号，还要改 `vendor_id` / `product_id`（用 `list` 里的 `VID_xxxx&PID_xxxx` 换算成十进制）。 |
| 4 | **无需 VC++ 运行库** | exe 已静态链接 C 运行库（`.cargo/config.toml` 中的 `+crt-static`），依赖只剩 Windows 系统 DLL，全新系统也能直接跑。 |
| 5 | **SmartScreen 提示** | exe 未签名，首次运行可能提示"Windows 已保护你的电脑" → 点「更多信息」→「仍要运行」。 |
| 6 | **系统要求** | Windows 10 / 11 **x64**（本项目按 x64 编译）。ARM64 Windows 未测试，建议自行 `cargo build`。 |
| 7 | **计划任务不属于文件夹** | 重装系统会清掉计划任务，重新跑一次 `install-autostart.ps1` 即可；程序文件本身不受影响。 |
| 8 | **两台机器别同时监听同一副耳机** | 一般不会发生（耳机只连一台机器）。若真并行，两边都会各自响应，属于预期外用法。 |
| 9 | **旧机器要清理的话** | 跑 `scripts\uninstall-autostart.ps1`（删任务 + 结束实例），然后删掉文件夹即可，系统里不留东西。 |

## 五、便携包内容清单

默认（精简版）：

```
sound-switch-portable\
├─ sound-switch.exe              # 主程序（手动调试用，有控制台输出）
├─ sound-switch-launch.exe       # 无窗口启动器（登录自启用它）
├─ config.json                   # 配置（VID/PID、设备名关键词、角色、日志等）
└─ scripts\
   ├─ install-autostart.ps1      # 安装登录自启
   └─ uninstall-autostart.ps1    # 卸载
```

加上 `-WithDocs` / `-WithSource` 后会额外包含：

```
├─ 部署说明.md                    # 本文件的副本
├─ README.md                     # 使用与配置说明
├─ src\                          # 源码（想在新机器重新编译时用）
├─ Cargo.toml / Cargo.lock
├─ .cargo\config.toml            # 静态链接 CRT 的构建配置
└─ docs\                         # 开发记录（PLAN / PROGRESS / DEPLOY）
```

> `state.json` 与 `logs\` 会在首次运行时自动生成，不属于便携包内容。
