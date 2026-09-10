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
