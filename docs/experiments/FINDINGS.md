# DTS 空间音效切换 —— 实验结论

> 实验时间：2026-09-10 22:45 ~ 22:53
> 目的：验证「耳机开机时自动切换到 DTS 音效」能否通过程序实现，以及正确的实现路径。

## 一、实验方法

1. 用只读脚本 `snapshot-audio-registry.ps1` 导出注册表快照（5 个根键、5226 行）→ `dts-before.txt`；
2. 用户在 **Windows 设置 → 系统 → 声音 → 耳机 → 空间音效** 里，
   把耳机端点的空间音效从「关闭」改成「**DTS Headphone:X**」；
3. 再拍一次快照 → `dts-after.txt`，用 `diff-snapshots.ps1` 逐行/逐字节对比。

## 二、实验结果

全部变化只出现在耳机端点的这个键下：

```
HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{耳机端点GUID}\Properties
```

另外 `HKCU\SOFTWARE\Microsoft\Multimedia`、DTS 应用的 AppModel 键、`HKLM\...\CurrentVersion\Audio`
**都没有任何变化** —— 也就是说，这个设置**只写在 HKLM 的端点属性里**。

### 1) 可用格式清单（内容没变，只是被重写）

属性 `{a45429a4-aa63-4480-b7f8-3f2552daee93}` 的 pid 2~6 分别是一种可用格式，
每条 842 字节；切换前后**内容完全相同**，只有头部 2 字节序列号变化（音频栈重写产生的噪声，diff 时需忽略）。

解码出「名称 → 引擎 GUID」映射如下：

| pid | 名称（从 UTF-16 字符串解码） | 引擎 GUID |
|---|---|---|
| 2 | Windows Sonic | `b53d940c-b846-4831-9f76-d102b9b725a0` |
| 3 | Dolby Atmos for Headphones | `1459ac38-3875-49bf-bb59-0fe80f4d395d` |
| 4 | Dolby Atmos for built-in speakers | `4c81e564-c8ef-4ad9-9f2c-9ef995533790` |
| 5 | **DTS Headphone:X** | `4444acb0-8dc0-4c2c-a0d8-2c76db470f86` |
| 6 | DTS:X Ultra | `adafd3c6-ac4c-404a-836a-9615e9060564` |

### 2) 真正记录「当前生效引擎」的三处属性（关键）

| 属性 | 长度变化 | 变化内容 | 含义推断 |
|---|---|---|---|
| `{908dba32-edff-4c28-8e45-c918561f6748},2` | 84 → 84 字节 | 第 1 个 GUID：Windows Sonic → **DTS Headphone:X**；2 个标志字节 `0→1` | 当前/上次引擎 + 启用标志 |
| `{8a845654-d6c3-4cd7-b4eb-243d4bd99032},2` | 32 → 32 字节 | 第 2 个 GUID：全零 → **DTS Headphone:X** | 已激活引擎记录 |
| `{fd8a7b27-0b18-4025-ab1c-bdd6b32e1604},2` | 9 → 154 字节 | 由「空」变为 DTS GUID + 一组 float32 参数 | 空间音频配置文件（含虚拟扬声器参数） |

原始字节（供后续实现参考）：

```
{8a845654-...},2
  before( 32B): 41001EB3010000000000...0000        (两个 GUID 槽全零)
  after ( 32B): 41001800010000000000000000000000 B0AC4444C08D2C4CA0D82C76DB470F86

{908dba32-...},2
  before( 84B): 410070B301000000 0100005A 00000000 01000000 01000000
                0C943DB546B831489F76D102B9B725A0 0C943DB546B831489F76D102B9B725A0 ...
  after ( 84B): 4100108001000000 0100005A 01000000 01000000 01000000
                B0AC4444C08D2C4CA0D82C76DB470F86 0C943DB546B831489F76D102B9B725A0 ...

{fd8a7b27-...},2
  before(  9B): 410072000100000000                  (空)
  after (154B): 41005AA301000000 0100005A B0AC4444C08D2C4CA0D82C76DB470F86
                FEFF0300 01000000 20000000 A0400000 ...(float32 参数数组)
```

（`B0AC4444C08D2C4CA0D82C76DB470F86` 即小端序的 DTS Headphone:X 引擎 GUID `4444acb0-8dc0-4c2c-a0d8-2c76db470f86`）

## 三、结论

1. **机制确认**：把系统的「空间音效格式」设为 **DTS Headphone:X**，就是启用这副耳机 DTS 音效的正确做法
   —— 与 SoundVolumeView 的 `/SetSpatial` 是同一件事。
2. **没有简单开关值**：不是某个 DWORD 从 0 变 1，而是几个**未公开的二进制结构**
   （GUID 表 + 标志位 + float 参数数组）；且全部位于 **HKLM**，
   本机实测普通用户对该键只有读权限（Owner = SYSTEM），因此自己实现**需要管理员权限 + 逆向这些结构**，
   风险和维护成本都高。
3. **推荐路线**：
   - **方案 A（推荐）**：调用 `SoundVolumeView.exe /SetSpatial "<设备>" "DTS Headphone:X"`，
     由现成工具负责格式细节；我们的程序只在"开机事件"时调用它（关机可选恢复）。
   - **方案 B**：自己用 COM 属性存储写上面三个属性（需逆向 + 可能需管理员），
     好处是不依赖第三方 exe。
4. **先验证是否真的需要自动化**：该设置是按音频端点持久化的（端点 GUID 不变），
   因此可能**只需设置一次就长期有效**。若开关耳机后仍是 DTS Headphone:X，则本需求可以不写任何代码。

## 四、持久性测试（已完成）

**方法**：保持上述 DTS Headphone:X 设置不变，物理开关耳机电源两次（22:59、23:00），
然后再拍快照 `dts-persist.txt` 与设置后的基线 `dts-current.txt` 对比。

**结果：设置完整保留，不需要任何程序干预。**

| 检查项 | 结果 |
|---|---|
| `{908dba32-…},2`（当前引擎 GUID + 标志位） | **未变化**，仍指向 DTS Headphone:X |
| `{8a845654-…},2`（激活引擎 GUID） | **未变化** |
| `{fd8a7b27-…},2`（空间音频配置，154B） | **未变化** |
| 耳机音频端点 GUID | **未变化**（仍为 `{耳机端点GUID}`） |
| 耳机端点其它属性 | 仅 `Level:0/1`（音量值）有 +8 的微小变化，与空间音效无关 |
| 其它端点的变化 | Realtek/AMD 端点的音量值、DTS 应用界面状态（PCT/PTT/ITT、Positions）——均为无关噪声 |

**结论**：
- 「空间音效 = DTS Headphone:X」是按音频端点持久保存的，**开关耳机不会重置**；
- 因此**不需要编写任何自动化代码**（既不需要 SoundVolumeView 集成，也不需要自己写注册表）；
- 只要手动设置一次，之后每次耳机开机都会沿用 DTS Headphone:X。

## 五、最终结论

1. 用户的判断正确：把系统的「空间音效格式」设为 DTS Headphone:X 就是启用该耳机 DTS 音效的方式。
2. 该设置持久有效（按端点保存），**一次性设置即可，无需程序干预** → 本项需求关闭，不写代码。
3. 若将来需要程序化控制（例如换设备、或某次更新后被重置），推荐方案仍是调用
   `SoundVolumeView.exe /SetSpatial "<设备>" "DTS Headphone:X"`（理由见第三节），
   并注意该写入涉及 HKLM 端点属性，可能需要管理员权限。

> ⚠️ **以上第 2、3 条已于 2026-09-29 被推翻，请看第七节。**
> 正确结论是：**必须让 DTS Sound Unbound 应用运行一次完成授权**，只设"空间音效"不够；
> 该步已由 `src/dts.rs` 自动化实现。

---

## 七、结论修正（2026-09-29）：必须启动 DTS Sound Unbound，且已被程序自动化

> 触发原因：用户反馈"实际上仅修改注册表并不能真的生效，还是需要我手动启动 DTS Sound Unbound"。
> 本次用**对照实验 + UI Automation**重新验证，推翻了第四、五节"无需代码"的结论。

### 7.1 对照实验：注册表属性 ≠ 音效生效

| 步骤 | 结果 |
|---|---|
| App 运行中，读耳机端点三个空间音效属性 | 选中引擎 = DTS、`{8a845654}` 记录 = DTS |
| **杀掉 App**，再读同样三个属性 | **一个字节都没变** |
| 用户听感确认（App 窗口被移到屏幕外、未点任何按钮） | **音效正常** |

→ 结论：端点属性只记录"你选了哪个引擎"；**真正的授权是运行期的**（App 与系统服务
`DTSAPO3Service` 握手），所以不存在"写注册表就能生效"的路径，也没有可写的开关值。

用户听感还确认了一个关键点：**授权完成后关掉 App，音效依然保留**（本次会话内）。
这正是"启动 → 等它授权 → 关掉"三步骤可行的依据。

### 7.2 UI Automation 实测：应用界面完全可程序化驱动

DTS Sound Unbound 没有命令行接口，但作为 UWP XAML 应用，它暴露完整的 UI Automation 树。
实测（工具：`dump-dts-uia.ps1` / `dts-enable-prototype.ps1`，快照：`dts-uia-*.txt`）：

| 元素 | AutomationId | 用途 |
|---|---|---|
| 主页授权卡 `DTS 耳机 X 已授权 / 未授权` | `DTSXHPNewLicenseTile` | **读授权状态**（程序据此判断是否已生效） |
| 「更多选项」按钮 | `Dotsx3Button` | 打开右上角菜单 |
| 「更新许可证」菜单项 | `RefreshLicensesButton` | **等价于用户手动点的那一下** |
| 主页 / 耳机 X / DTS X 导航 | `HomeRadioButton` / `HPXRadioButton` / `DTSXRadioButton` | 页面导航 |
| 耳机 X → 配置页 | `SADSelectionCombobox`、`BypassBtn` 等 | 耳机调音/空间模式（与"启用"无关） |

行为观察：
- 应用启动后授权卡先显示「未授权/载入中」，**约 3~20 秒后自动变为「已授权」**（热启动约 3 秒）；
- 点授权卡只是**导航到"耳机 X"页面**（不会启用），真正刷新授权的是菜单里的「更新许可证」；
- 因此"有时候必须点一下"很可能是用户在它自检完成前手动干预。

### 7.3 窗口与关闭行为（实测坑）

| 观察 | 说明 |
|---|---|
| 可见窗口类是 `ApplicationFrameWindow`，标题 `DTS Sound Unbound` | 它属于 **ApplicationFrameHost.exe**，不是应用进程 |
| 该版本的框架窗口**没有** `Windows.UI.Core.CoreWindow` 子窗口 | "靠子窗口 PID 认框架窗口"的方法在本机失效，**改用标题匹配**（`trace-dts-windows.ps1`） |
| 窗口从启动第 1 秒起就可见 | 要做到"无感"，必须尽快把它移到屏幕外（`SetPointPos(-32000,-32000)`，保持 visible 以免被 UWP 生命周期挂起） |
| 关闭窗口 = **转入后台**（窗口消失、进程常驻） | 这与用户"关掉窗口它还留在后台"的观察一致；要真正退出需结束进程 |

### 7.4 落地实现

见 `src/dts.rs`：切换到耳机 → 按 AUMID 启动应用 → 窗口移到屏幕外 → UI Automation 读授权卡 →
未授权则触发「更新许可证」（最多 3 次）→ 确认「已授权」后关闭应用。
切换到非耳机时不做任何操作。配置项与注意事项见 README 的"DTS 空间音效"专节。

### 7.5 仍未被证实/存疑的点（如实记录）

- **"完全没有规律"的根因未定位**：授权偶发失败可能与授权租约/网络/设备重连有关，
  本次没有抓到可复现的失败样本；实现的策略是"没确认成功就把 App 留在后台"以便补救。
- **强制结束进程对音效是否有影响**：实测"窗口关闭转入后台 → 结束进程"后音效保留，
  但样本主要来自用户听感确认，没有客观测量手段。
- 应用升级后控件 id 可能变化，届时需更新 `src/dts.rs` 顶部的常量或配置项。

---

## 六、附：快照文件说明

实验当时拍过 4 份注册表快照（各约 827KB）：

| 文件 | 说明 |
|---|---|
| `dts-before.txt` | 空间音效 = 关闭 时 |
| `dts-after.txt` | 切换为 DTS Headphone:X 后 |
| `dts-current.txt` | 复核用（与 after 完全一致，仅时间戳不同） |
| `dts-persist.txt` | 两次开关耳机电源之后（关键属性仍未变，见第四节） |

**这些快照不纳入仓库**：它们是本机的临时证据，含本机音频端点 GUID、USB 实例路径等设备信息，
体积也较大（各约 827KB）。仓库里只保留**结论与关键字节**（见第二、三节），
因此不影响复核。需要自己复核时：

1. 用目录下的 `snapshot-audio-registry.ps1` 拍快照：
   ```powershell
   powershell -ExecutionPolicy Bypass -File docs\experiments\snapshot-audio-registry.ps1 -Out docs\experiments\dts-now.txt
   ```
2. 用 `diff-snapshots.ps1` 与另一份快照对比：
   ```powershell
   powershell -ExecutionPolicy Bypass -File docs\experiments\diff-snapshots.ps1 -Before a.txt -After b.txt
   ```
3. 建议把生成的 `dts-*.txt` 加入 `.gitignore` 或放在仓库外，避免把本机设备信息提交上去。

> 注意：快照里的音频端点 GUID、USB 实例路径等都是**每台机器/每次系统安装各不相同**的标识，
> 与实验结论无关；本节已把与结论相关的属性名和字节值直接写在正文里。
