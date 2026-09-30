# sound-switch

根据 **HyperX 耳机（USB 无线接收器）的开机/关机数据**，自动把耳机设为/恢复为
Windows **系统默认输出音频设备** 的小工具（Rust 编写）。

> 协议来自对耳机 HID 输入报告的分析：`[0x64,0x01]` = 开机、`[0x64,0x03]` = 关机
> （VID `1008` / PID `2702`）。

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
# 产物（两个）:
#   target\release\sound-switch.exe          主程序
#   target\release\sound-switch-launch.exe   无窗口启动器（登录自启用，见下文）
```

> 构建配置 `.cargo/config.toml` 启用了 `+crt-static`（静态链接 C 运行库），
> 因此生成的 exe **不依赖 VC++ 运行库**，全新系统也能直接运行（便于迁移）。
> 如需临时关闭：`cargo build --release --config 'target.x86_64-pc-windows-msvc.rustflags=[]'`。

## 使用

在 `<项目目录>` 下运行（会在此目录生成 `config.json`、`state.json`、`logs\`）：

```powershell
# 1. 首次运行自动生成 config.json（可手动编辑）
# 2. 手动测试音频切换（不依赖耳机，验证切换/恢复可用）
.\target\release\sound-switch.exe set-default   # 模拟“关机→开机”：切到耳机
.\target\release\sound-switch.exe restore       # 模拟“开机→关机”：恢复原默认

# 3. 手动测试 DTS 音效启用流程（启动 App → 完成授权 → 关闭 App）
.\target\release\sound-switch.exe dts-arm

# 4. 正式监听
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
| `dts_enabled` | true/false | `true` | 切换到耳机时是否自动确保 **DTS Headphone:X** 音效已启用（见下文专节） |
| `dts_aumid` | 字符串 | `DTSInc.DTSSoundUnbound_t5j2fzbtdg37r!App` | DTS Sound Unbound 的 AUMID；留空则不启用本功能 |
| `dts_window_title` | 字符串 | `DTS Sound Unbound` | 兜底识别应用窗口用的标题（正常按进程识别，用不到） |
| `dts_licensed_text` | 字符串 | `已授权` | 授权卡上表示“已生效”的文字（App 换语言时改这里） |
| `dts_arm_timeout_ms` | 数字(ms) | `90000` | 等待授权完成的上限；App 首次自检可能要几十秒 |
| `dts_close_after` | true/false | `true` | 确认授权完成后关闭 DTS Sound Unbound（实测关闭后音效仍保留） |
| `dts_fix_spatial` | true/false | `true` | 是否检查/修复 Windows 侧的空间音效选择（**系统会重置它，这是"必须手动点一下"的根因**） |
| `dts_spatial_timeout_ms` | 数字(ms) | `120000` | 等待"空间音效切到 DTS"的时限（以端点属性为准） |
| `dts_allow_mouse_click` | true/false | `true` | 允许真实鼠标点击兜底（App 里那个『启用 数字影院系统耳机：X』是 Text 元素，只能真实点击；代价是需要时窗口闪现约 1 秒） |

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
| 不想自动启动 DTS Sound Unbound | `dts_enabled: false` |
| 想让 DTS App 常驻后台（不再每次拉起，也就没有窗口闪一下） | `dts_close_after: false` |
| 换了别的 DTS 应用/语言 | `dts_aumid`、`dts_window_title`、`dts_licensed_text` |

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
├─ Cargo.toml / Cargo.lock
├─ .cargo/config.toml # 构建配置（静态链接 CRT，免装 VC++ 运行库）
├─ config.json       # 配置（首次运行生成，可手工编辑）
├─ state.json        # 切换前默认设备记录（自动维护）
├─ logs/             # sound_switch.log / .log.1
├─ src/
│  ├─ main.rs        # CLI 入口
│  ├─ launch.rs      # 无窗口启动器（登录自启用）
│  ├─ config.rs      # 配置
│  ├─ log.rs         # 日志（控制台+文件+轮转）
│  ├─ hid.rs         # HID 监听/解析（每集合一线程）
│  ├─ audio.rs       # 音频端点枚举与默认设备切换
│  ├─ dts.rs         # DTS Headphone:X 的静默启用（启动 App / UI 自动化 / 关闭）
│  ├─ state.rs       # 状态持久化
│  ├─ single_instance.rs # 单实例保护（命名互斥体）
│  └─ app.rs         # 事件→动作 主逻辑
├─ scripts/
│  ├─ install-autostart.ps1    # 安装登录自启
│  ├─ uninstall-autostart.ps1  # 卸载
│  └─ make-package.ps1         # 生成便携部署包
├─ dist/             # 便携包产物（make-package.ps1 生成）
└─ docs/
   ├─ PLAN.md        # 开发计划
   ├─ PROGRESS.md    # 开发日志（每步记录）
   ├─ DEPLOY.md      # 迁移/部署指南
   └─ experiments/   # DTS 空间音效实验（快照/差异工具与结论）
      ├─ FINDINGS.md              # 实验结论（含 2026-09-29 的结论修正）
      ├─ snapshot-audio-registry.ps1 / diff-snapshots.ps1
      ├─ dump-dts-uia.ps1         # 导出 DTS App 的 UI Automation 树
      ├─ dts-enable-prototype.ps1 # 原型：静默启动 + 读授权 + 触发更新
      ├─ trace-dts-windows.ps1 / probe-dts-windows.ps1  # 窗口/关闭行为探测
      └─ dts-uia-*.txt            # UI 树快照（本机信息，不入库）
```

## 已知限制

- 运行时会占用耳机的 HID 集合；若 HyperX 官方软件需要独占访问，两者可能互相影响
  （选择其中一个运行，或在官方软件里释放该设备）。
- 登录时耳机若已处于开机状态且当前默认不是耳机，程序不会主动切换（只响应开关机事件）。
- DTS 音效的自动启用依赖 DTS Sound Unbound 的界面控件（UI 自动化）：应用升级后若控件
  id 变化，流程会失败并只记日志（可改 `dts_*` 配置或关闭该功能），不影响默认设备切换。

## 后续计划

- Windows 服务化（**暂缓**：默认输出设备按登录会话生效，服务在 Session 0 无法直接修改；
  已用"登录自启计划任务"达成同样效果）。
- 可选：「启动时状态同步」（覆盖"登录时耳机已开机"的场景），前提是能可靠判断耳机当前开关机状态。

## 关于 DTS 空间音效（自动启用，见 `src/dts.rs`）

**结论：要让 DTS 真正生效，必须同时满足两件事**，缺一不可：

1. **Windows 侧**：耳机端点的「空间音效」必须选中 **DTS Headphone:X**；
2. **应用侧**：**DTS Sound Unbound** 必须跑一次，与系统服务 `DTSAPO3Service` 完成授权握手。

而且第 1 条会被**系统自己重置**（重启/待机/设备重枚举后变回"关闭"或 Windows Sonic）——
这正是"有时候必须手动打开 App 点一下"的真正原因。本程序把这两步都自动化了。

### 实测机制与证据（2026-09-30 更正）

| 结论 | 证据 |
|---|---|
| 状态存在端点的**三个属性**里，可读不可写（普通用户） | `HKLM\...\MMDevices\Audio\Render\{GUID}\Properties` 下 `{908dba32-…},2`（`[12..16]`=启用标志、`[24..40]`=选中引擎 GUID）、`{8a845654-…},2`（`[16..32]`=已激活引擎）、`{fd8a7b27-…},2`（9 字节=空 / 154 字节=DTS 配置）。**实测写入被拒**：`不允许所请求的注册表访问权`（键有 SYSTEM 完整性标签），所以只能让应用去写 |
| **授权卡"已授权"≠ 音效生效** | 2026-09-30 实测：App 启动后授权卡正常显示「已授权」，但属性仍是 `flag=0 / 配置=9字节`，音效确实没生效 —— 之前把授权卡当作成功判据是错的 |
| 真正让端点切到 DTS 的是 App 里的**「启用」按钮** | 未启用时「耳机 X」页面出现 `启用 DTS 耳机 X`，点它之后再出现 `启用 数字影院系统耳机：X`，点后者属性立刻变成 `flag=01000000`、`sel=DTS`、`act=DTS`、配置 154 字节 |
| 第二个按钮**只能用真实鼠标点击** | 它是 **Text 元素**（不是 Button），`InvokePattern`/`TogglePattern`/`SelectionItemPattern` 全部不支持，"聚焦+回车"也无效；实测只有真实鼠标点击有效 |
| 关掉 App 后音效保留 | 属性不会因 App 退出而回退，用户听感也确认过 → 可以"用完就关" |

### 程序怎么做（对应需求：切到耳机时启用，切走时不动）

1. 检测到**耳机开机 → 默认输出切换到耳机**后，按 AUMID 启动 DTS Sound Unbound，
   窗口立刻移到屏幕外（`x=-32000`，保持"可见"以免被 UWP 生命周期挂起）；
2. 回**主页**读授权卡（授权卡只在主页；App 会记住上次停留的页面，所以要主动切回）；
   必要时触发「更多选项 → 更新许可证」（`Dotsx3Button` → `RefreshLicensesButton`）；
3. 读端点属性判断空间音效：已是 DTS 就结束；否则切到「耳机 X」页面，
   逐个点击名字含「启用」的元素 —— 优先用 UIA pattern，点不动的（那个 Text 元素）用
   **真实鼠标点击兜底**（窗口临时显示约 1 秒再移回屏幕外）；
4. **每一步都用端点属性复核**：只有属性确认 `flag=01000000 && sel=DTS && act=DTS`
   才算成功，然后才关闭 App（`dts_close_after`）。

**切换到非耳机（关机/恢复）时不做任何 DTS 操作。**

### 注意事项与降级

- 这套流程本质是**代替你操作界面**（该应用没有任何接口）。DTS 若更新 App 改了控件，
  流程会失败并**只记日志**，不影响默认输出设备切换；可改 `dts_*` 配置或把
  `dts_enabled` 设为 `false` 恢复成"手动开一次 App"。
- **需要点击「启用」时窗口可能会短暂出现**（激活应用 / 鼠标兜底所致），点完会立刻移回屏幕外，
  随后按配置关闭 App；**已经是启用状态时完全不会有窗口出现**（实测全程离屏）。
  点击方式会写进日志：`InvokePattern`、`借用焦点 + 空格`（都不显示窗口）、
  或 `鼠标`（显示约 0.5 秒）；只有前两种都失败才会用鼠标，可用
  `"dts_allow_mouse_click": false` 彻底禁止鼠标兜底（代价是某些情况下可能启用不了）。
- 若始终没能确认成功：程序会**保留** App 在后台运行（窗口仍在屏幕外）并记 `warn`，
  而不是把它关掉，方便你手动补救。
- 想让它常驻后台、避免每次重新启动：`"dts_close_after": false`。
- 排查时把 `log_level` 改成 `debug`，日志里会打印 `DTS: ...` 各个环节（含属性值）。
- 完整的实验方法、控件与原始记录见 `docs/experiments/FINDINGS.md`；
  实验脚本：`dts-fix-spatial.ps1`（复刻启用流程 + 属性验证）、`snapshot-dts-state.ps1`（前后快照）、
  `watch-dts-props.ps1`（每秒监视属性变化）。

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

---

# 迁移到新电脑 / 重装系统

**一句话：整个文件夹拷过去就行**（本程序是"绿色"的，配置、状态、日志都在自己目录里，
不写系统目录、无安装过程）。详细步骤见 `docs/DEPLOY.md`。

## 生成便携包（推荐）

```powershell
powershell -ExecutionPolicy Bypass -File scripts\make-package.ps1
# 产物：
#   dist\sound-switch-portable\        整个文件夹可直接拷走
#   dist\sound-switch-portable.zip     压缩包（便于传输）
```

包里含：两个 exe、`config.json`、**`使用说明.txt`**、三个**双击即可用**的 `.bat`
（安装 / 卸载 / 查看状态）、`scripts\*.ps1`（命令行用法）。
**不含** `state.json` 与 `logs\`（首次运行时自动生成），也不含文档与源码
（需要时加 `-WithDocs` / `-WithSource`）。

## 新电脑上：双击就行

```
1. 把文件夹放到任意位置（例如 D:\GreenTools\sound-switch）
2. 双击「安装登录自启.bat」   ← 自动注册登录自启 + 立即后台启动（无窗口），
                                结束时会提示"按回车键关闭窗口"
3. 双击「查看运行状态.bat」   ← 确认任务/进程/日志/当前默认输出
4. 取消：双击「卸载登录自启.bat」
```

命令行等价用法（可选）：

```powershell
.\sound-switch.exe list                                          # 确认能看到耳机
.\sound-switch.exe set-default ; .\sound-switch.exe restore       # 验证切换/恢复
powershell -ExecutionPolicy Bypass -File scripts\install-autostart.ps1 -StartNow   # 装自启
```

## 两个容易忽略的点

1. **不要拷贝旧机器的 `state.json`**：音频端点 GUID 在新机器上会不同，
   程序按 `config.json` 里的 `audio_keyword`（设备名关键词）在运行时查找设备，因此不需要 GUID。
2. **DTS 空间音效**：本程序会在切换到耳机时自动启动 DTS Sound Unbound 完成授权
   （详见上文"DTS 空间音效"专节）。新机器上需要先**装好 DTS Sound Unbound**（微软商店，
   或用你耳机/主板厂商提供的版本）；如果新机器上的 AUMID 不同（可用
   `Get-StartApps | Where-Object Name -match DTS` 查看），把 `config.json` 里的
   `dts_aumid` 改成新值即可；不需要本功能就设 `dts_enabled: false`。
