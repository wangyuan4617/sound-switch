//! 状态持久化：state.json 记录"切换前各角色的默认输出设备"。
//! 目的：即使程序异常退出/重启，只要 state.json 还在，就能知道应恢复到哪个设备。
//! 只读写当前目录下文件，不触碰系统文件。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const DEFAULT_STATE: &str = "state.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// 当前是否处于"耳机已设为默认"状态（即上一次收到 power on）
    pub headset_active: bool,
    /// 每个角色切换前记录的默认输出设备端点 id -> 角色名(console/multimedia/communications)
    pub previous_defaults: BTreeMap<String, String>,
    /// 耳机渲染端点 id（上次成功定位后缓存）
    pub headset_endpoint_id: Option<String>,
}

impl State {
    pub const DEFAULT_STATE: &'static str = DEFAULT_STATE;

    pub fn load(path: &PathBuf) -> Self {
        if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, path: &PathBuf) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }
}
