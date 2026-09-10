//! 业务逻辑：把 HID 事件映射为音频动作，并维护 state.json。
//! 本模块设计为无界面核心，后续做 Windows 服务时可被 service 包装复用。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio;
use crate::config::Config;
use crate::hid::{self, HidEvent};
use crate::log;
use crate::state::State;

const DEBOUNCE: Duration = Duration::from_millis(2000);

/// 事件→动作：把耳机开机/关机映射到切换默认输出设备
pub struct Actions {
    pub config: Config,
    pub state_path: PathBuf,
    pub state: State,
    last_power: Option<(HidEvent, Instant)>,
}

impl Actions {
    pub fn new(config: Config) -> Self {
        let state_path = PathBuf::from(State::DEFAULT_STATE);
        let state = State::load(&state_path);
        Self {
            config,
            state_path,
            state,
            last_power: None,
        }
    }

    /// HID 事件入口（在主循环中被调用）
    pub fn on_event(&mut self, ev: HidEvent) {
        match ev {
            HidEvent::PowerOn => self.on_power(true),
            HidEvent::PowerOff => self.on_power(false),
            // 本期不处理这些指令，且默认（info 级别）不打印，避免刷屏；
            // 需要排查时把 config.json 的 log_level 改成 debug 即可看到。
            HidEvent::VolumeUp => log::debug("HID: 音量加（暂不处理）"),
            HidEvent::VolumeDown => log::debug("HID: 音量减（暂不处理）"),
            HidEvent::MicToggle => log::debug("HID: 麦克风开关（暂不处理）"),
        }
    }

    fn on_power(&mut self, on: bool) {
        // 防抖：短时间重复的相同事件忽略
        if let Some((prev, at)) = &self.last_power {
            let same = (*prev == HidEvent::PowerOn) == on;
            if same && at.elapsed() < DEBOUNCE {
                return;
            }
        }
        self.last_power = Some((
            if on { HidEvent::PowerOn } else { HidEvent::PowerOff },
            Instant::now(),
        ));

        if on {
            if self.state.headset_active {
                log::debug("收到 power on，但已处于耳机默认状态，忽略");
                return;
            }
            self.handle_power_on();
        } else {
            self.handle_power_off();
        }
    }

    fn handle_power_on(&mut self) {
        log::info("=== 收到设备开机事件 ===");
        // 定位耳机渲染端点（端点可能在开机后延迟出现，带重试）
        let keyword = self.config.audio_keyword.clone();
        let mut found = None;
        let deadline = Instant::now() + Duration::from_millis(self.config.endpoint_wait_ms);
        loop {
            match audio::find_endpoint_by_keyword(&keyword) {
                Ok(Some(ep)) => {
                    found = Some(ep);
                    break;
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        break;
                    }
                    log::debug("尚未在活动端点中找到耳机端点，继续等待…");
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(e) => {
                    log::error(&format!("枚举音频端点失败: {e}"));
                    break;
                }
            }
        }

        let Some(target) = found else {
            log::error(&format!(
                "在等待期内未找到关键词为 '{}' 的渲染端点，跳过切换",
                keyword
            ));
            return;
        };
        log::info(&format!("目标端点: {} ({})", target.name, target.id));

        // 对每个角色：记录切换前的默认设备，再切到耳机
        let mut any_switch = false;
        for role_name in self.config.roles.clone() {
            let Some(role) = audio::parse_role(&role_name) else {
                log::warn(&format!("未知角色: {}，跳过", role_name));
                continue;
            };
            match audio::current_default_endpoint_id(role) {
                Ok(Some(cur)) if cur == target.id => {
                    // 已经是耳机，无需切换，也不覆盖 previous
                    log::info(&format!(
                        "角色 {} 默认已是耳机，跳过",
                        audio::role_name(&role)
                    ));
                }
                Ok(Some(cur)) => {
                    log::info(&format!(
                        "角色 {} 原默认: {}",
                        audio::role_name(&role),
                        cur
                    ));
                    if let Err(e) = audio::set_default_endpoint(&target.id, role) {
                        log::error(&format!("切换失败: {e}"));
                    } else {
                        self.state
                            .previous_defaults
                            .insert(role_name.clone(), cur);
                        any_switch = true;
                    }
                }
                Ok(None) => {
                    log::warn(&format!(
                        "角色 {} 没有当前默认输出设备",
                        audio::role_name(&role)
                    ));
                }
                Err(e) => {
                    log::error(&format!("查询默认设备失败: {e}"));
                }
            }
        }
        if any_switch {
            self.state.headset_endpoint_id = Some(target.id);
            self.state.headset_active = true;
            self.save();
        } else {
            log::info("没有发生实际切换（可能已全部是耳机）");
        }
    }

    fn handle_power_off(&mut self) {
        log::info("=== 收到设备关机事件 ===");
        if self.state.previous_defaults.is_empty() {
            log::info("没有可恢复的默认设备记录，跳过");
            return;
        }
        let prev = self.state.previous_defaults.clone();
        for (role_name, prev_id) in &prev {
            if prev_id.is_empty() {
                continue;
            }
            let Some(role) = audio::parse_role(role_name) else { continue };
            // 仅当当前默认不是 prev 时才恢复（避免无谓调用）
            let should_restore = match audio::current_default_endpoint_id(role) {
                Ok(Some(cur)) => cur != *prev_id,
                Ok(None) => true,
                Err(_) => true,
            };
            if !should_restore {
                log::info(&format!(
                    "角色 {} 已是恢复目标，无需操作",
                    audio::role_name(&role)
                ));
                continue;
            }
            if let Err(e) = audio::set_default_endpoint(prev_id, role) {
                log::error(&format!(
                    "恢复角色 {} 默认输出失败: {e}",
                    audio::role_name(&role)
                ));
            }
        }
        self.state.previous_defaults.clear();
        self.state.headset_active = false;
        self.save();
    }

    fn save(&mut self) {
        self.state.save(&self.state_path);
    }

    /// 主动“切到耳机”（手动测试/状态校准用，等价于 power on 动作）
    pub fn manual_set_headset_default(&mut self) {
        self.last_power = None;
        self.handle_power_on();
    }

    /// 主动“恢复之前默认”（手动测试/状态校准用，等价于 power off 动作）
    pub fn manual_restore(&mut self) {
        self.last_power = None;
        self.handle_power_off();
    }
}

/// 主监听循环：阻塞运行直到 stop 置位。
/// HID 读取在独立线程中完成，本函数只消费事件并执行音频动作。
pub fn run_forever(mut actions: Actions, stop: Arc<AtomicBool>) {
    let cfg = actions.config.clone();
    let rx = match hid::spawn(cfg, stop.clone()) {
        Ok(rx) => rx,
        Err(e) => {
            log::error(&format!("启动 HID 监听失败: {e}"));
            return;
        }
    };

    log::info("监听器启动：等待耳机开机/关机…（Ctrl+C 退出）");
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ev) => actions.on_event(ev),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => { /* 无事件，继续等 */ }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::error("HID 监听线程已退出");
                break;
            }
        }
    }
    log::info("监听器已退出");
}
