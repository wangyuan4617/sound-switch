# sound-switch 开发计划

> 创建时间：2026-09-10 00:20（用户批准后生效）

## 目标

在 `<项目目录>` 下用 **Rust** 编写一个 Windows 控制台程序 `sound-switch.exe`：

1. 监听 HyperX Cloud Stinger Core Wireless DTS 耳机（USB HID，VID `0x03F0`=1008 / PID `0x0A8E`=2702）上报的数据；
2. 监听逻辑与 `index.js` 一致：
   - 开机 `[0x64, 0x01]` → 把耳机设为系统默认输出音频设备；
   - 关机 `[0x64, 0x03]` → 恢复切换前的默认输出音频设备；
   - 音量/麦克风/其它指令仅记录日志（本期不做动作，为后续扩展留接口）；
3. 只做上述两个功能；**服务化（Windows 服务）留到功能手动测试通过之后再做**（代码结构上预先按「可被服务托管」的方式组织）。

## 技术方案

### 依赖
- `hidapi`：跨平台 HID 枚举/读取（与 node-hid 同源）；
- `windows`：COM / Core Audio（IMMDeviceEnumerator 等官方 API）；
- `winreg`：读取注册表里音频端点友好名（映射端点 → 设备名）；
- `serde`/`serde_json`：配置与状态持久化；
- `ctrlc`：优雅退出。

### HID 监听（src/hid.rs）
- 周期性枚举 VID/PID 匹配的所有 HID 路径（可能多个 collection），逐个打开；
- 每个设备一个读取线程，`read_timeout` 短超时轮询；打开失败/掉线自动重试，无需重启；
- 解析规则：去掉可选的前导 `0x00` 报告号后，按 `index.js` 中的字节模式精确匹配；
- 事件通过 channel 上报主循环：`PowerOn / PowerOff / VolumeUp / VolumeDown / MicOn / MicOff / Unknown(bytes)`。

### 音频端点（src/audio.rs）
- 目标设备定位：遍历 `HKLM\...\MMDevices\Audio\Render` 注册表活动端点，读取友好名
  （属性 `{a45c254e-df1c-4efd-8020-67d146a850e0},2`），按配置关键词（默认 `HyperX Cloud Stinger Core Wireless`）匹配；
- 读取当前默认输出设备：`IMMDeviceEnumerator::GetDefaultAudioEndpoint`（eRender + 三个角色）；
- 切换默认设备：COM `IPolicyConfig::SetDefaultEndpoint(deviceId, role)`（windows crate 若未导出则手写 vtable）；
- 三个角色 eConsole / eMultimedia / eCommunications 全部切换/恢复（可配置），
  每个角色分别记录切换前的默认设备 id → 存 `state.json`。

### 主流程（src/main.rs）
- `run`：启动 hid 监听 + 音频动作；
- 事件 → 动作：
  - PowerOn：查目标端点 id（活动端点按关键词）；若仍非默认，则逐角色记录旧默认并切到目标；
  - PowerOff：逐角色把默认恢复到 `state.json` 中记录的旧设备（旧设备已不存在则跳过并记日志）；
- 附加子命令方便自测：
  - `list`：列出匹配的 HID 设备 + 全部渲染端点（id/名称/当前默认标记）；
  - `status`：当前默认输出 + 目标端点是否存在 + state 概览；
  - `set-default` / `restore`：不依赖耳机事件，手动触发“切到耳机/恢复旧默认”，供单独验证音频切换；
  - 支持 `--dry-run`（只打印将执行的动作，不真正切换）。

### 配置与状态文件（都在当前目录，不动系统文件）
- `config.json`：VID/PID、音频关键词、角色列表、轮询间隔等（首次运行自动生成默认值）；
- `state.json`：切换前逐角色记录的旧默认设备 id、当前是否处于“已切到耳机”状态；
- `logs/sound_switch.log`：运行日志。

### 文档（要求 4：过程留痕）
- `docs/PLAN.md`：本计划；
- `docs/PROGRESS.md`：开发日志，每完成一步追加记录（时间+内容+结果）；
- `README.md`：构建/使用说明。

## 目录结构
```
<项目目录>\
├─ index.js            # 参考实现（不动）
├─ Cargo.toml
├─ config.json         # 运行时自动生成
├─ state.json          # 运行时自动生成
├─ logs\               # 运行时日志
├─ src\
│  ├─ main.rs          # CLI 入口
│  ├─ config.rs        # 配置
│  ├─ log.rs           # 日志（控制台 + 文件）
│  ├─ hid.rs           # HID 监听/解析
│  ├─ audio.rs         # 音频端点查询与默认设备切换
│  ├─ state.rs         # 状态持久化
│  └─ app.rs           # 事件→动作 主逻辑（未来可被服务包装复用）
└─ docs\
   ├─ PLAN.md
   └─ PROGRESS.md
```

## 验收
1. `cargo build --release` 通过；
2. 运行 `list` 能看到耳机 HID 设备与 “HyperX Cloud Stinger Core Wireless” 渲染端点；
3. 手动测试（用户执行）：运行 `run`，关闭耳机电源 → 日志出现 PowerOff → Windows 默认输出恢复为旧设备；
   打开耳机电源 → 日志出现 PowerOn → 默认输出切到耳机。
