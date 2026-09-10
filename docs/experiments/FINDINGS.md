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

## 四、下一步

- [ ] **持久性测试**：开关一次耳机电源，再拍快照与 `dts-current.txt` 对比
      （若关键属性不变、设置面板里仍是 DTS Headphone:X → 无需自动化）。
- [ ] 若确实会重置：确认 `SoundVolumeView.exe` 在**非管理员**会话下能否成功设置
      （能 → 直接集成；不能 → 与"服务化"一起做，服务天然具备权限）。

## 五、附：相关快照文件

| 文件 | 说明 |
|---|---|
| `dts-before.txt` | 空间音效 = 关闭 时 |
| `dts-after.txt` | 切换为 DTS Headphone:X 后 |
| `dts-current.txt` | 复核用（与 after 完全一致，仅时间戳不同） |
