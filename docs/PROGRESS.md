# sound-switch 开发日志

> 规则：每完成一步，在此追加一条记录（时间 / 步骤 / 说明与结果）。

## 2026-09-10 00:20 — 第 1 步：确认需求与环境，初始化工程
- 读取参考实现 `index.js`（VID 1008 / PID 2702；开机 `[0x64,0x01]`、关机 `[0x64,0x03]`、音量/麦克风等指令）。
- 确认本机当前连接：HID `USB\VID_03F0&PID_0A8E`（1008/2702）与渲染端点「扬声器 (HyperX Cloud Stinger Core Wireless DTS)」。
- 确认工具链：cargo/rustc 1.94.1 (x86_64-pc-windows-msvc)，MSVC 链接器可用，crates.io 可访问。
- 用户批准计划；`cargo init --name sound-switch` 完成。
- 产物：`docs/PLAN.md`、`Cargo.toml`、`src/main.rs`。

## 2026-09-10 00:30 — 第 2 步：工程脚手架与依赖
- 依赖：`hidapi` 2.6.7（HID 读取）、`windows` 0.61（Core Audio/COM，
  features: Win32_Foundation/Media_Audio/System_Com/System_Com_StructuredStorage/
  System_Variant/System_SystemInformation/UI_Shell_PropertiesSystem/
  Devices_FunctionDiscovery/Devices_Custom）、`com-policy-config` 0.6
  （未文档化 `IPolicyConfig` COM 绑定，用于设置默认输出设备）、
  `serde`/`serde_json`（配置与状态）、`winreg`（注册表读取辅助）、`anyhow`、`ctrlc`。
- 说明：`windows` crate 未导出 `IPolicyConfig`（Windows 没有“设置默认设备”公开 API），
  经调研采用社区通用方案 `com-policy-config`（与 windows 0.61 类型统一）。
- 编写模块：`config.rs`（config.json 自动生成）、`log.rs`（控制台+文件日志，本地时间）。

## 2026-09-10 00:45 — 第 3 步：HID 监听与解析
- `hid.rs`：枚举 VID 1008/PID 2702 的全部 HID 顶层集合并打开；轮询 `read_timeout`；
  字节匹配 power on/off、音量、麦克风指令；设备插拔自动重枚举；兼容前导 `0x00` 报告号。
- 自测：`list` 输出 3 个匹配 HID 设备（usage_page 0xFFC0 / 0xFF00 / 0x000C），符合预期。

## 2026-09-10 00:50 — 第 4 步：音频端点枚举与默认设备切换/恢复
- `audio.rs`：COM 初始化、`IMMDeviceEnumerator` 枚举活动渲染端点并读取
  `PKEY_Device_FriendlyName`；按 `audio_keyword` 定位耳机端点；
  `IPolicyConfig::SetDefaultEndpoint` 按角色切换/恢复。
- `state.rs`：state.json 持久化“切换前默认设备”。
- `app.rs`：HID 事件→动作映射（power on → 切耳机并记录原默认；power off → 恢复），
  防抖 + 端点等待重试；`main.rs` CLI（run/list/status/set-default/restore）。
- **自测（真实切换+恢复往返）**：
  - `set-default` 把 console 角色默认输出从 Realtek 切到耳机端点，`status` 验证生效；
  - `restore` 恢复 Realtek，`status` 验证生效；state.json 正确记录并清空。
- `run` 模式启动验证：日志正常输出「监听器启动…」，Ctrl+C 可退出。

## 2026-09-10 00:52 — 第 5 步：文档收尾
- 编写 `README.md`（构建/使用/config 说明/工作原理）。
- 更新本日志。
- 控制台输出代码页设为 UTF-8（SetConsoleOutputCP），避免中文乱码。

## 2026-09-10 00:54 — 第 6 步：回归自测
- 重编译 release；`run` 模式 8 秒冒烟测试正常，Ctrl+C/Ctrl+C 任务终止正常。
- 全部功能就绪，等待用户物理开关耳机电源做端到端手动测试。

> **待用户手动测试**：运行 `sound-switch.exe run`，物理开关耳机电源，
> 观察日志出现 PowerOn/PowerOff，并确认 Windows 默认输出设备自动切换/恢复。

## 2026-09-10 22:30 — 第 7 步：性能实测（用户提问）
- 实测旧实现（单线程顺序轮询）空闲占用：**0.573% 单核**（0.172s CPU / 30s，12 核机器占 0.048%），
  内存 6.5MB，句柄 97 稳定，无泄漏。
- 结论：对性能影响可忽略；轮询是“阻塞等待”而非忙等。
- 同时定位到用户反馈的“音量日志延迟十几秒”根因：单线程顺序读取 3 个 HID 集合，
  空集合各自等满 500ms 超时，有数据的集合每轮只取 1 条 → 消费速度约 1 条/秒，
  滚动音量产生的十几条报告因此排队十几秒。

## 2026-09-10 22:35 — 第 8 步：HID 读取改为“每集合一个独立线程”
- `hid.rs` 重写：
  - 每个匹配的 HID 集合由独立线程 `read_timeout` 阻塞读取，事件产生即刻经 channel 上报；
  - 主线程只做周期重枚举（按 `rescan_interval_ms`），负责增删读取线程、回收掉线线程；
  - 打开失败的集合只告警一次，避免重枚举刷屏；退出时 join 全部读取线程。
- `app.rs`：主循环改为 `recv_timeout` 消费事件（不再自己做轮询）。
- `config.rs`：`read_timeout_ms` 默认 500 → 200（只影响退出响应速度）。
- 实测新实现空闲占用：**0.156% 单核**（0.0313s CPU / 20s），比旧实现降低约 3.7 倍；
  线程：主 + 监听 + 3 个集合读取线程，均处于事件等待状态；内存 6.6MB、句柄 101 稳定。

## 2026-09-10 22:40 — 第 9 步：日志精简与体积限制
- 日志级别语义调整：`info`（默认）只打印耳机开关机与切换结果；
  音量/麦克风/原始 HID 数据降级为 `debug`（默认不打印，排查时改 `log_level` 即可）。
- `log.rs` 增加轮转：新增配置 `log_max_kb`（默认 512KB），
  超限时 `sound_switch.log` → `sound_switch.log.1`（单备份，旧备份覆盖），
  目录占用上限约 2×`log_max_kb`。
- 自测：用临时配置（`log_max_kb=1`、`log_level=debug`）连续运行 3 次，
  确认产生 `.1`（987 字节）且新日志重新开始写；测试文件已清理。

## 2026-09-10 22:45 — 第 10 步：README 补齐 config.json 逐项说明
- `README.md` 新增完整的配置项参考：类型/默认值/作用表 + 逐项详解 +
  “常见场景改法”速查表 + 日志级别与轮转说明 + 已知限制。
- 回归自测：`list` / `status` / `set-default` / `restore` 全部通过；
  期间从 `state.json` 确认用户此前的真机测试**已成功触发开机切换**
  （`headset_active=true`，记录原默认 console=Realtek），随后往返测试已恢复原状态。

## 2026-09-10 22:50 — 第 11 步：DTS 空间音效切换可行性调研（用户提问）
- 结论：Windows **没有公开 API** 控制“空间音效”下拉框（Windows Sonic / DTS Headphone:X / DTS:X Ultra），
  该选择存放在音频端点的属性存储中；本机实测：
  - 端点键 `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{GUID}`（Owner=SYSTEM），
    其 `Properties`/`FxProperties` 子键对 `BUILTIN\Users` **仅授予 ReadKey**，普通用户**无法写入**；
  - 当前会话非管理员（`Elevated: False`）→ 需要以管理员/SYSTEM 身份运行才能改。
- 已定位相关属性集：`FxProperties` 下 `{d3993a3f-99c2-4402-b5ec-a92a0367664b}`（空间音效相关），
  其值为 APO/Mode GUID（如 `{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}`，系驱动 INF 中的信号处理 Mode）。
  但“用户在下拉框里选中 DTS”对应的确切属性值尚未确定。
- 待定方案（需用户确认）：在真机上做一次“切换空间音效前后注册表只读快照对比”，
  定位出对应值后实现为**可选功能**（配置开关，默认关闭），并明确要求以管理员身份运行；
  或推迟到服务化阶段（服务天然具备管理员权限）一并实现。
- 另：DTS Sound Unbound 为 Store 应用，其应用内部设置不受此路径控制，只能控制 Windows 侧的空间音效选择。

## 2026-09-10 22:46 — 第 12 步：git 基线提交 + DTS 快照实验准备
- **git 基线**：`cargo init` 时未建仓库，本次创建仓库并提交当前阶段性代码，
  作为后续改动的回滚点：
  - `ee2c6b6` chore: 阶段性提交 — HID 多线程监听 + 默认输出设备切换/恢复（15 文件 / 1937 行）；
  - `0e6cee1` test(experiments): 音频注册表快照/差异对比工具与 DTS 实验基线。
  - `.gitignore` 排除 `target/`（445MB）、`logs/`、`state.json`、`.idea/`；
    `.gitattributes` 统一换行为 LF，避免后续 diff 抖动。
- **实验工具**（`docs/experiments/`）：
  - `snapshot-audio-registry.ps1`：只读快照（`reg query`），覆盖
    `HKLM\...\MMDevices`（全树）、`HKLM\...\CurrentVersion\Audio`、
    `HKCU\SOFTWARE\Microsoft\Multimedia`、DTS 两个 Store 应用的 AppModel 键；
  - `diff-snapshots.ps1`：两次快照差异对比；
  - `dts-before.txt`：切换「空间音效」前的基线快照（5226 行）。
  - 注意：本机只有 Windows PowerShell 5.1，脚本必须以 **UTF-8 with BOM** 保存，
    否则中文注释会被按 ANSI 解码导致语法错误（已踩坑并修正）。
- **本轮排除的假设**（避免后续重复排查）：
  - `FxProperties` 的 `{d3993a3f-99c2-4402-b5ec-a92a0367664b},5/6` = `{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}`
    在**所有**渲染端点（含未插拔的）取值完全相同 → 不是用户的“空间音效”选择；
  - `{C18E2F7E-...}` 未在 `HKLM\SOFTWARE\Classes\CLSID` 注册 → 不是 APO/引擎 CLSID，
    更像驱动 INF 中的信号处理 Mode GUID；
  - `HKCU\...\Multimedia\Audio` 下只有 `DefaultEndpoint`（微信的按应用默认设备）与 `DeviceCpl`，
    没有空间音效选项；DTS 应用的 `HAM\AUI\App\V1\LU` 只有界面时间戳。
- **待用户操作**：在 Windows 设置里切换一次「空间音效」下拉框，随后拍 `dts-after.txt` 并 diff。
- 代码状态：回归自测通过（`list`/`status`/`set-default`/`restore`），无残留进程。

## 2026-09-10 22:53 — 第 13 步：DTS 快照实验完成（结论已定）
- 用户把耳机端点的「空间音效」从**关闭**改为 **DTS Headphone:X**；对比快照得到结论，
  详见 `docs/experiments/FINDINGS.md`。
- **变化位置**：仅耳机端点
  `HKLM\...\MMDevices\Audio\Render\{耳机端点GUID}\Properties`；
  `HKCU\SOFTWARE\Microsoft\Multimedia`、DTS 应用 AppModel 键、`HKLM\...\CurrentVersion\Audio` 全无变化
  → 该设置**只写在 HKLM 端点属性**里（普通用户对该键只读）。
- **解码出的格式映射**（`{a45429a4-...}` pid2~6，内容本身未变，仅头部 2 字节噪声）：
  Windows Sonic `b53d940c-…`、Dolby Atmos for Headphones `1459ac38-…`、
  Dolby Atmos for built-in speakers `4c81e564-…`、
  **DTS Headphone:X `4444acb0-8dc0-4c2c-a0d8-2c76db470f86`**、DTS:X Ultra `adafd3c6-…`。
- **真正记录"当前生效引擎"的三处属性**：
  `{908dba32-…},2`（当前引擎 GUID + 标志位，Windows Sonic → DTS）、
  `{8a845654-…},2`（激活引擎 GUID，全零 → DTS）、
  `{fd8a7b27-…},2`（空间音频配置，9B 空 → 154B：DTS GUID + float32 参数数组）。
- **结论**：机制上"设置系统空间音效格式"就是正确做法（与 SoundVolumeView `/SetSpatial` 等价），
  但没有简单开关值，写的是未公开二进制结构且位于 HKLM → 自行实现需管理员权限 + 逆向，风险高；
  推荐方案 A（调用 `SoundVolumeView.exe /SetSpatial`），或先验证"只需设置一次即长期有效"。
- **下一步**：开关一次耳机电源做持久性测试（对比 `dts-current.txt`）；
  若会重置再决定集成方式，并确认该工具在非管理员会话下是否可用。
- 新增文件：`docs/experiments/FINDINGS.md`、`dts-after.txt`、`dts-current.txt`。

## 2026-09-10 22:37（事后取证）— 第 14 步：用户真机测试，需求 2 验证通过
从 `logs/sound_switch.log` 找到用户自己的端到端测试记录（程序 `run` 模式）：
```
22:37:10 监听器启动：等待耳机开机/关机…
22:37:19 === 收到设备关机事件 ===
22:37:19 没有可恢复的默认设备记录，跳过
22:37:25 === 收到设备开机事件 ===
22:37:25 目标端点: 扬声器 (HyperX Cloud Stinger Core Wireless DTS) ({0.0.0.00000000}.{耳机端点GUID})
22:37:25 角色 console 原默认: {0.0.0.00000000}.{原默认端点GUID}   (Realtek 扬声器)
22:37:25 已把角色 console 的默认输出切换为 {0.0.0.00000000}.{耳机端点GUID}
22:37:25 角色 multimedia/communications 默认已是耳机，跳过
```
- **结论**：
  - 关机事件、开机事件均能正确识别（毫秒级，无延迟）；
  - **需求 2（开机自动设为默认输出）真机验证通过**：console 由 Realtek 切到耳机，并正确记录了原默认；
  - 关机时因当时耳机并非默认设备，`previous_defaults` 为空 → 跳过恢复，逻辑正确；
  - **需求 3（关机自动恢复）尚未在真机上验证**（需要"耳机是默认设备"时关机）。
- 当时状态：`headset_active=true`，`previous_defaults={"console": Realtek}`，三个角色默认均为耳机
  → 下次关机事件即可验证需求 3。

## 2026-09-10 23:00 — 第 15 步：持久性测试 + 需求 3 真机验证通过（阶段性里程碑）
- 用户保持 DTS Headphone:X 设置，物理开关耳机电源两次；对比 `dts-persist.txt` 与基线：
  - **DTS 空间音效设置完整保留**：`{908dba32-…},2`、`{8a845654-…},2`、`{fd8a7b27-…},2` 三处均未变化；
  - 耳机音频端点 GUID 未变（仍为 `{耳机端点GUID}`）；
  - 耳机端点唯一变化是 `Level:0/1`（音量值 +8），与空间音效无关；
  - → **本需求不需要写任何自动化代码**（不集成 SoundVolumeView、不写注册表），手动设置一次即长期有效。
- **需求 3（关机自动恢复默认输出）真机验证通过**（此前未验证）：
```
22:59:20 === 收到设备关机事件 ===
22:59:20 已把角色 console 的默认输出切换为 {原默认端点GUID}   (Realtek)
23:00:02 === 收到设备关机事件 ===
23:00:02 已把角色 console 的默认输出切换为 {原默认端点GUID}   (Realtek)
```
- **需求 2 再次验证**（两次开机均自动切到耳机）：
```
22:59:31 / 23:00:11 === 收到设备开机事件 === → 角色 console 原默认 Realtek → 切到耳机
```
- 附带观察：本机 Windows 中 **console 与 multimedia 联动**（改 console 会带着 multimedia；恢复时亦然），
  communications 独立（用户原先手动指向耳机）→ 恢复 console 即可让 media 应用也回到音箱。
- 事件识别为即时的（开关机事件在操作后立刻出现），此前的"音量日志延迟"问题已随多线程改造消失。
- 至此用户需求 1/2/3（Rust 实现、开机切默认、关机恢复）**全部真机验证通过**；
  剩余计划：服务化（后台运行 / 开机自启）。

## 2026-09-10 23:15 — 第 16 步：开机自启（登录时自动运行，无窗口）
- **技术选型**：默认输出音频设备是"按登录会话"生效的，Windows 服务在 Session 0 无法修改
  桌面会话的默认设备 → 采用"当前用户登录时启动的计划任务"（与 SoundSwitch 等同类工具一致）；
  真正的服务形式（服务 + 每会话代理）留待后续评估。
- **代码改动**：
  - 新增 `src/single_instance.rs`：命名互斥体（`Local\sound-switch-single-instance`）单实例保护，
    仅用于 `run` 模式；进程崩溃也会由系统自动释放，无残留文件。
    实测：已有实例运行时，第二个实例立即输出
    「已有 sound-switch 实例正在运行（单实例保护），本次退出」并以 0 退出。
  - 新增第二个可执行文件 `src/launch.rs` → `sound-switch-launch.exe`（`#![windows_subsystem = "windows"]`）：
    自身不分配控制台，用 `CREATE_NO_WINDOW` 拉起 `sound-switch.exe run`；
    退出码 0=成功 / 2=找不到主程序 / 3=取路径失败 / 4=启动失败（便于看任务的"上次运行结果"）。
  - `log.rs`：控制台输出改为忽略写失败（原 `print!` 在标准输出异常时会 panic）。
- **实测发现（重要）**：计划任务直接启动控制台程序**会带出可见窗口**
  （实测 `MainWindowHandle=197922`），与"任务会隐藏窗口"的常见说法不符 → 因此必须用启动器方案；
  用启动器后实测 `MainWindowHandle=0`（无可见窗口），内存 6.56MB。
- **安装结果**（`scripts\install-autostart.ps1 -StartNow`）：
  - 任务名 `sound-switch`；执行 `target\release\sound-switch-launch.exe`；
    工作目录 = 项目根目录；触发 = 当前用户登录时（Interactive, RunLevel=Limited）；
    `ExecutionTimeLimit=PT0S`（无上限，默认 3 天限制已解除）；`MultipleInstances=IgnoreNew`。
  - 实测通过任务启动后：进程无窗口、日志写入项目 `logs\`（证明工作目录正确）、
    `Get-ScheduledTaskInfo.LastTaskResult=0`。
- 新增文件：`src/launch.rs`、`src/single_instance.rs`、`scripts\install-autostart.ps1`、
  `scripts\uninstall-autostart.ps1`；README 增加"开机自启"整节。
- **已知限制（记入 README）**：登录时耳机若已处于开机状态且当前默认不是耳机，
  程序不会主动切换（只对开关机事件做反应）；如需覆盖需增加"启动时状态同步"，
  前提是能可靠判断耳机当前开关机状态（待研究，例如向 HID 写入状态查询包）。
- 期间为重建 exe 两次停掉了用户手动启动的实例（exe 被运行中的进程锁定）。

## 2026-09-10 23:21 — 第 17 步：重启后自启验证通过
- 用户重启电脑（开机时间 23:20:46），**未做任何手动操作**；检查结果：
  - 计划任务：`LastRunTime=23:21:21`、`LastTaskResult=0`、`NumberOfMissedRuns=0`、State=Ready；
  - 进程：PID 9860，**启动时间 23:21:17**（开机后约 31 秒 / 登录后立即），
    `MainWindowHandle=0`（**无窗口**），内存 3.52MB，CPU 累计 0.3 秒，
    线程 = 主线程 + HID 监听线程 + 3 个集合读取线程；
  - 启动器 `sound-switch-launch.exe` 已按预期自行退出（常驻的是主程序）；
  - 日志：`logs\sound_switch.log` 末尾出现 `23:21:17 监听器启动…`，
    且文件写在项目根目录下 → **工作目录设置正确**；
  - `state.json` 完好；当前默认输出为基线（console/multimedia=Realtek，communications=耳机）。
- 顺带解释了一处历史日志疑点：23:10:18 的关机事件与切换各打印了两遍，
  原因是当时同时运行着两个实例（用户手动启动的 + 我测试用的）；现在有单实例保护，不会再发生。
- 结论：**"开机自启 + 无窗口后台运行"目标达成**。
- 待办：让用户在不手动启动程序的前提下开关一次耳机，做后台实例的端到端确认。
