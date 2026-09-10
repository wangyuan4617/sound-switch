//! 配置：从 config.json 读取，不存在则写入默认配置。
//! 配置文件位于程序运行目录（与 exe 同级），便于手动编辑与测试。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_CONFIG: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 耳机 HID 的 Vendor ID（十进制）
    pub vendor_id: u16,
    /// 耳机 HID 的 Product ID（十进制）
    pub product_id: u16,
    /// 目标音频设备名称关键词（用于从系统渲染端点中定位耳机），不区分大小写
    pub audio_keyword: String,
    /// 检测到开机后需要设为默认输出的角色（Windows ERole）
    /// 可选: console / multimedia / communications
    pub roles: Vec<String>,
    /// HID 重枚举间隔（毫秒）。用于发现耳机重新插拔/上电；
    /// 调大更省 CPU，代价是设备变化被发现得更慢
    pub rescan_interval_ms: u64,
    /// HID 读取超时（毫秒）。每个 HID 集合独立线程读取，该值只影响退出响应速度
    pub read_timeout_ms: i32,
    /// 开机事件后，若耳机音频端点暂时未出现，最多重试多久（毫秒）
    pub endpoint_wait_ms: u64,
    /// dry-run：只打印将要执行的动作，不真正切换默认设备
    pub dry_run: bool,
    /// 日志级别: error / warn / info / debug
    /// info（默认）只打印耳机开关机等关键事件；debug 会额外打印音量/麦克风/原始 HID 数据
    pub log_level: String,
    /// 单个日志文件大小上限（KB），超过则轮转为 sound_switch.log.1
    pub log_max_kb: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vendor_id: 1008,
            product_id: 2702,
            audio_keyword: "HyperX Cloud Stinger Core Wireless".to_string(),
            roles: vec![
                "console".to_string(),
                "multimedia".to_string(),
                "communications".to_string(),
            ],
            rescan_interval_ms: 1500,
            read_timeout_ms: 200,
            endpoint_wait_ms: 15000,
            dry_run: false,
            log_level: "info".to_string(),
            log_max_kb: 512,
        }
    }
}

impl Config {
    pub const DEFAULT_CONFIG: &'static str = DEFAULT_CONFIG;

    /// 从给定路径加载；若文件不存在则创建默认配置并返回默认值
    pub fn load(path: &PathBuf) -> anyhow::Result<Config> {
        if path.exists() {
            let text = std::fs::read_to_string(path)?;
            let cfg: Config = serde_json::from_str(&text)?;
            Ok(cfg)
        } else {
            let cfg = Config::default();
            let text = serde_json::to_string_pretty(&cfg)?;
            std::fs::write(path, text)?;
            Ok(cfg)
        }
    }
}
