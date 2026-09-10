# sound-switch

根据 **HyperX 耳机（USB 无线接收器）的开机/关机数据**，自动把耳机设为/恢复为
Windows **系统默认输出音频设备** 的小工具（Rust 编写）。

> 参考协议实现：本项目根目录的 `index.js`（监听 VID `1008` / PID `2702` 的 HID 输入报告）。

## 功能

- 监听 HID 数据（每个 HID 集合一个独立读取线程，事件毫秒级响应）：
  - `[0x64, 0x01]` → 设备**开机** → 把耳机扬声器设为默认输出设备（角色可配置）；
  - `[0x64, 0x03]` → 设备**关机** → 把切换前记录的默认输出设备**恢复**；
  - 音量加减 / 麦克风开关等指令：本期不做动作，且默认**不打印**（需要时用 `debug` 级别查看）。
- 状态持久化：切换前的默认设备记录在 `state.json`，程序重启后依然能正确恢复。
- 日志自动限体积：默认单文件不超过 512KB，超出自动轮转为 `sound_switch.log.1`。
- 手动子命令（不依赖耳机事件）便于测试。

## 构建

```powershell
cargo build --release
# 产物: target\release\sound-switch.exe
```

## 使用

在 `<项目目录>` 下运行（会在此目录生成 `config.json`、`state.json`、`logs\`）：

```powershell
# 1. 首次运行自动生成 config.json（可手动编辑）
# 2. 手动测试音频切换（不依赖耳机，验证切换/恢复可用）
.\target\release\sound-switch.exe set-default   # 模拟“关机→开机”：切到耳机
.\target\release\sound-switch.exe restore       # 模拟“开机→关机”：恢复原默认

# 3. 正式监听
.\target\release\sound-switch.exe run

# 辅助查看
.\target\release\sound-switch.exe list          # 列出匹配 HID 设备与渲染端点
.\target\release\sound-switch.exe status        # 当前默认输出与 state
```

通用参数：`--config <path>` 指定配置文件（默认 `./config.json`）；`--dry-run` 只打印将执行的动作，不真正切换。
退出：`Ctrl+C`。

---

# config.json 配置项说明

首次运行会自动生成；**改完后需重启程序生效**（`run` 模式下按 `Ctrl+C` 再启动）。
所有键都可省略：省略时使用“默认值”一列的值。

| 键 | 类型 | 默认值 | 作用 |
|---|---|---|---|
| `vendor_id` | 数字 | `1008` | 耳机 HID 的 Vendor ID（十进制） |
| `product_id` | 数字 | `2702` | 耳机 HID 的 Product ID（十进制） |
| `audio_keyword` | 字符串 | `HyperX Cloud Stinger Core Wireless` | 用于在系统输出设备里认出“耳机”的名称关键词（不区分大小写） |
| `roles` | 字符串数组 | `["console","multimedia","communications"]` | 检测到开机时要切换、关机时要恢复的默认设备“角色” |
| `rescan_interval_ms` | 数字(ms) | `1500` | 重新枚举 HID 设备的间隔；用于感知耳机接收器插拔 |
| `read_timeout_ms` | 数字(ms) | `200` | 每个 HID 集合单次读取的等待上限；影响退出响应速度 |
| `endpoint_wait_ms` | 数字(ms) | `15000` | 收到开机事件后，等待耳机音频端点出现的最长时间 |
| `dry_run` | true/false | `false` | 演练模式：只打印将要执行的动作，不真正切换默认设备 |
| `log_level` | 字符串 | `info` | 日志级别：`error` / `warn` / `info` / `debug` |
| `log_max_kb` | 数字(KB) | `512` | 单个日志文件大小上限，超出后轮转 |

## 逐项详解

### `vendor_id` / `product_id`
耳机的 USB HID 标识，程序只监听这两个值都匹配的设备。
- 本耳机（HyperX Cloud Stinger Core Wireless DTS 接收器）为 `1008` / `2702`（即十六进制 `0x03F0` / `0x0A8E`）。
- **换成别的耳机时**：先运行 `sound-switch.exe list`，输出里 `path=\\?\HID#VID_xxxx&PID_xxxx&...` 就是它的 VID/PID；
  把十六进制换算成十进制填进来（例如 `VID_046D` → `1133`）。换算：`0x046D = 4×4096 + 6×256 + 13 = 1133`。
- 注意 `list` 只显示当前 `config.json` 中 VID/PID 的设备；想找新设备可以先临时把两个值改大范围或用系统“设备管理器”查看硬件 ID。

### `audio_keyword`
音频端点在系统里显示的名字通常形如 `扬声器 (HyperX Cloud Stinger Core Wireless DTS)`，
程序用这个关键词做**子串匹配**（忽略大小写）来定位“耳机那一个输出设备”。
- 默认值只写到品牌型号部分，已能匹配；不要写得太短（例如只写 `扬声器`），否则可能匹配到主板声卡。
- **改名/换耳机时**：先用 `list` 看“当前活动渲染端点”列表里的完整名字，挑一段唯一的文字填进来。
- 中文名称也可以（配置按 UTF-8 保存）。

### `roles`（默认设备角色）
Windows 的“默认输出设备”其实按用途分三个角色（ERole），程序对列表里的每个角色分别切换/恢复：

| 角色 | 含义 | 建议 |
|---|---|---|
| `console` | 常规程序/系统声音用的默认设备（“声音设置”里显示的那个） | 一般必留 |
| `multimedia` | 多媒体播放类程序使用的默认设备 | 一般必留 |
| `communications` | 通信类程序（语音通话等）使用的默认设备 | 不想影响通话设备时可删掉 |

- 只想改“普通播放”的设备：`"roles": ["console"]`。
- 每个角色在切换前都会单独记录原设备（存 `state.json`），关机时逐个恢复，互不干扰。

### `rescan_interval_ms`（性能/发现速度的取舍）
HID 数据是事件驱动的，这个间隔只决定“多久检查一次有没有新设备/设备是否被拔掉”。
- 默认 `1500`（1.5 秒）。实测空闲占用约 **0.16% 单核**（12 核机器上约 0.013%）。
- 调大（如 `5000`）→ 更省 CPU，但接收器拔了/插上后要更久才被发现；
- 调小（如 `500`）→ 发现更快，CPU 略升。一般 `1500～5000` 都合适。

### `read_timeout_ms`
每个 HID 集合由独立线程阻塞读取，这个值只影响“按 `Ctrl+C` 后多久真正退出”和线程回收速度。
`200` 足够；除非有特殊需求，不建议改。

### `endpoint_wait_ms`
耳机刚开机时，Windows 可能还没把它的输出设备枚举出来。
程序收到“开机”事件后会在这个时间内**每秒重试定位**耳机端点，找到就立即切换。
- 若你的耳机开机后设备出现得慢（例如 10 秒以上），把它调大。
- 调小则“找不到就放弃切换”来得更快。

### `dry_run`
`true` 时，所有“切换/恢复”只写日志不执行，适合先确认逻辑、或想知道程序会做什么。
也可以在命令行临时用 `--dry-run` 覆盖。

### `log_level` 与日志内容
| 级别 | 会打印什么 |
|---|---|
| `error` | 仅错误 |
| `warn` | 错误 + 警告（如 HID 被其它软件占用） |
| `info`（默认） | 再加“耳机开机/关机”等关键事件与切换结果；**音量和麦克风指令不打印** |
| `debug` | 再加音量/麦克风指令、原始 HID 数据、HID 集合打开/关闭等排查信息 |

- 排查问题时把 `log_level` 改成 `debug`；只想安静运行时保持 `info`。
- 日志同时输出到控制台和 `logs\sound_switch.log`（UTF-8）。

### `log_max_kb`
日志体积上限（默认 512KB）。达到上限时：
```
logs\sound_switch.log   ->   logs\sound_switch.log.1   （旧备份被覆盖）
logs\sound_switch.log   （新建，从头开始写）
```
因此 `logs\` 目录占用上限约为 **2 × log_max_kb**（默认约 1MB）。
设为 `0` 表示不限制（不推荐长期运行）。

## 常见场景改法

| 需求 | 改哪些键 |
|---|---|
| 换一副耳机 | `vendor_id`、`product_id`、`audio_keyword` |
| 只想切“普通播放设备”，不影响通话设备 | `roles: ["console"]` |
| 先演练不真正切换 | `dry_run: true` |
| 降低 CPU 占用 | `rescan_interval_ms: 5000` |
| 排查“为什么没切换” | `log_level: "debug"`，再看 `logs\sound_switch.log` |
| 耳机端点出现慢 | `endpoint_wait_ms: 30000` |
| 日志想留更久 | `log_max_kb: 2048` |

## 工作原理（简述）

1. **HID 监听**：`hidapi` 枚举匹配 VID/PID 的全部 HID 集合，**每个集合一个独立线程**持续读取输入报告，
   字节匹配识别开关机等指令（兼容可选前导 `0x00`）；主线程周期性重枚举以感知插拔。
2. **音频定位**：Core Audio `IMMDeviceEnumerator` 枚举活动渲染端点，读取 `PKEY_Device_FriendlyName`，
   按 `audio_keyword` 找到耳机端点。
3. **切换/恢复**：未文档化 COM 接口 `IPolicyConfig::SetDefaultEndpoint`
   （Windows 没有公开的“设置默认输出设备”API，这是社区通用方案），
   逐角色记录原设备后切到耳机；收到关机事件时逐角色恢复并清空记录。

## 目录

```
sound_switch/
├─ index.js          # 参考协议实现
├─ Cargo.toml
├─ config.json       # 配置（首次运行生成，可手工编辑）
├─ state.json        # 切换前默认设备记录（自动维护）
├─ logs/             # sound_switch.log / .log.1
├─ src/
│  ├─ main.rs        # CLI
│  ├─ config.rs      # 配置
│  ├─ log.rs         # 日志（控制台+文件+轮转）
│  ├─ hid.rs         # HID 监听/解析（多线程）
│  ├─ audio.rs       # 音频端点枚举与默认设备切换
│  ├─ state.rs       # 状态持久化
│  └─ app.rs         # 事件→动作 主逻辑（后续服务化可复用）
└─ docs/
   ├─ PLAN.md        # 开发计划
   └─ PROGRESS.md    # 开发日志（每步记录）
```

## 已知限制

- 运行时会占用耳机的 HID 集合；若 HyperX 官方软件需要独占访问，两者可能互相影响
  （选择其中一个运行，或在官方软件里释放该设备）。
- 手动运行阶段需要保持一个控制台窗口；服务化（后台运行、开机自启）在下一阶段实现。

## 后续计划

- 安装为 Windows 服务 / 开机自启。

## 关于 DTS 空间音效（已结案，无需代码）

实测结论：Windows 的「空间音效」设置是**按音频端点持久保存**的，开关耳机电源**不会重置**，
因此只需**手动设置一次**即可长期有效：

> 设置 → 系统 → 声音 → 选择耳机 → 空间音效 → **DTS Headphone:X**

本程序无需介入（既不需要调用第三方工具，也不需要写注册表）。
完整的实验方法、注册表证据与结论见 `docs/experiments/FINDINGS.md`。

---

# 开机自启（登录时自动运行，无窗口）

## 为什么用计划任务而不是 Windows 服务

「默认输出音频设备」是**按登录会话**生效的设置，而 Windows 服务运行在 Session 0，
**无法修改你桌面会话的默认设备**。因此正确做法是：以**当前用户身份、在登录时**启动
（SoundSwitch 等同类工具也是这个方案）。真正的服务形式需要"服务 + 每会话代理进程"，
复杂度高，留待后续评估。

## 安装

```powershell
# 先构建（会生成两个 exe）
cargo build --release

# 安装“登录时自动启动”，并立即启动一次（无需重新登录）
powershell -ExecutionPolicy Bypass -File scripts\install-autostart.ps1 -StartNow
```

脚本会注册一个名为 `sound-switch` 的计划任务：

| 项目 | 值 |
|---|---|
| 触发 | 当前用户**登录时**（Run only when user is logged on） |
| 执行 | `target\release\sound-switch-launch.exe`（无窗口启动器） |
| 工作目录 | 项目根目录（`config.json` / `state.json` / `logs\` 都相对它） |
| 时间上限 | 无（已解除默认的"运行超过 3 天就停止"） |
| 多实例 | IgnoreNew（另有程序自身的单实例保护兜底） |
| 权限 | 普通用户权限（Level=Limited，改默认输出设备不需要管理员） |

> **关于"无窗口"**：计划任务直接启动控制台程序时**会显示一个控制台窗口**（实测确认）。
> 因此这里用 `sound-switch-launch.exe`：它本身是 GUI 子系统程序（不分配控制台），
> 再用 `CREATE_NO_WINDOW` 拉起真正的 `sound-switch.exe`，所以既无窗口、也不会闪一下。

## 查看与操作

```powershell
# 查看任务状态
Get-ScheduledTask -TaskName sound-switch | Format-List
Get-ScheduledTaskInfo -TaskName sound-switch | Select LastRunTime, LastTaskResult   # 0 = 成功

# 查看后台实例（MainWindowHandle 应为 0 = 无窗口）
Get-Process sound-switch | Select Id, MainWindowHandle, WorkingSet64

# 查看运行日志
Get-Content logs\sound_switch.log -Tail 20 -Encoding UTF8

# 手动启动 / 停止
Start-ScheduledTask -TaskName sound-switch
Get-Process sound-switch | Stop-Process

# 卸载（删除任务并结束实例）
powershell -ExecutionPolicy Bypass -File scripts\uninstall-autostart.ps1
```

> 任务里的启动器进程会立刻退出（任务显示 Ready 是正常的），真正常驻的是 `sound-switch.exe`。
> 因此"停止"要停进程，而不是 `Stop-ScheduledTask`。

## 验证自启是否生效

重启或注销后重新登录，然后确认：

```powershell
Get-Process sound-switch | Select Id, MainWindowHandle     # 应有进程且 MainWindowHandle=0
Get-Content logs\sound_switch.log -Tail 3 -Encoding UTF8   # 末尾应有当次登录时间的「监听器启动」
```

## 已知限制

- **登录时耳机已经是开机状态**：程序只对"开机/关机事件"做反应，因此不会主动切换设备
  （如果此时默认设备不是耳机，需要你手动切一次，或关机后再开机一次触发事件）。
  如需覆盖该场景，可后续增加"启动时状态同步"（要先能可靠判断耳机当前是否开机）。
- 修改 `config.json` 后需要重启后台实例（`Get-Process sound-switch | Stop-Process` 再
  `Start-ScheduledTask -TaskName sound-switch`）。
- 重新编译前必须先停掉后台实例（Windows 会锁定正在运行的 exe）。
