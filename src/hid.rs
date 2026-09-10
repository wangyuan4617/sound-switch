//! HID 监听模块。
//!
//! 参考 `index.js`：匹配 vendor_id/product_id 的所有 HID 集合。
//! 与旧实现（单线程顺序轮询）不同，这里**每个 HID 集合一个独立读取线程**：
//! - 各集合互不阻塞，事件产生即刻上报，不存在“排队等十几秒”的问题；
//! - 主线程只负责周期性重枚举，发现插拔/集合变化时增删读取线程。

use anyhow::{Context, Result};
use hidapi::{HidApi, HidDevice};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::config::Config;
use crate::log;

/// HID 事件（由解析后的指令产生）
#[derive(Debug, Clone, PartialEq)]
pub enum HidEvent {
    PowerOn,
    PowerOff,
    VolumeUp,
    VolumeDown,
    MicToggle,
}

fn matches(data: &[u8], pattern: &[u8]) -> bool {
    // 容忍可能存在的前导 0x00（部分后端会附带 report id 0）
    if data == pattern {
        return true;
    }
    if data.len() == pattern.len() + 1 && data[0] == 0x00 && data[1..] == *pattern {
        return true;
    }
    false
}

/// 解析一段输入报告，识别指令
pub fn parse_report(data: &[u8]) -> Option<HidEvent> {
    if matches(data, &[0x64, 0x01]) {
        return Some(HidEvent::PowerOn);
    }
    if matches(data, &[0x64, 0x03]) {
        return Some(HidEvent::PowerOff);
    }
    if matches(data, &[0x01, 0x01, 0x00, 0x00, 0x00]) {
        return Some(HidEvent::VolumeUp);
    }
    if matches(data, &[0x01, 0x02, 0x00, 0x00, 0x00]) {
        return Some(HidEvent::VolumeDown);
    }
    if matches(data, &[0x65, 0x00]) || matches(data, &[0x65, 0x04]) {
        return Some(HidEvent::MicToggle);
    }
    None
}

/// 单个 HID 集合的读取循环（独立线程）
fn reader_loop(
    dev: HidDevice,
    label: String,
    timeout_ms: i32,
    tx: Sender<HidEvent>,
    stop: Arc<AtomicBool>,
) {
    let mut buf = [0u8; 128];
    while !stop.load(Ordering::Relaxed) {
        match dev.read_timeout(&mut buf, timeout_ms) {
            Ok(0) => { /* 超时：无数据，继续等待 */ }
            Ok(n) => {
                let data = &buf[..n];
                match parse_report(data) {
                    Some(ev) => {
                        // 事件即刻上报；通道若已关闭说明主循环退出，直接结束
                        if tx.send(ev).is_err() {
                            break;
                        }
                    }
                    None => log::debug(&format!("HID[{}] 数据 {:02X?}", label, data)),
                }
            }
            Err(e) => {
                // 设备移除/被独占：结束本线程，交由主线程重枚举后重新打开
                log::debug(&format!("HID[{}] 读取结束: {e}", label));
                break;
            }
        }
    }
}

/// 启动监听：为每个匹配集合维护一个读取线程，直到 stop 置位。
/// 事件通过 `tx` 上报给调用方（主循环）。
pub fn listen(cfg: &Config, stop: Arc<AtomicBool>, tx: Sender<HidEvent>) {
    let mut api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            log::error(&format!("HID 初始化失败: {e}"));
            return;
        }
    };

    let mut readers: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut warned: HashSet<String> = HashSet::new();

    while !stop.load(Ordering::Relaxed) {
        // 回收已结束的读取线程（设备掉线 / 被其它软件独占）
        readers.retain(|_, h| !h.is_finished());

        if let Err(e) = api.refresh_devices() {
            log::debug(&format!("刷新 HID 设备列表失败: {e}"));
        }
        let paths: Vec<CString> = api
            .device_list()
            .filter(|d| d.vendor_id() == cfg.vendor_id && d.product_id() == cfg.product_id)
            .map(|d| d.path().to_owned())
            .collect();

        for path in paths {
            let key = path.to_string_lossy().into_owned();
            if readers.contains_key(&key) {
                continue; // 已在监听
            }
            match api.open_path(&path) {
                Ok(dev) => {
                    warned.remove(&key);
                    let label = short_label(&key);
                    log::debug(&format!("开始监听 HID 集合: {}", label));
                    let tx2 = tx.clone();
                    let stop2 = stop.clone();
                    let timeout = cfg.read_timeout_ms;
                    let handle =
                        std::thread::spawn(move || reader_loop(dev, label, timeout, tx2, stop2));
                    readers.insert(key, handle);
                }
                Err(e) => {
                    // 同一路径只告警一次，避免重枚举时刷屏
                    if warned.insert(key.clone()) {
                        log::warn(&format!(
                            "打开 HID 集合失败（可能被其它软件占用）: {} -> {e}",
                            short_label(&key)
                        ));
                    }
                }
            }
        }

        // 分片休眠，保证 stop 后能快速退出
        let mut slept = 0u64;
        while slept < cfg.rescan_interval_ms && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
            slept += 50;
        }
    }

    // 退出：read_timeout 很短，读取线程会在几十毫秒内返回
    for (_, handle) in readers {
        let _ = handle.join();
    }
    log::debug("HID 监听线程已全部退出");
}

/// 生成便于阅读的设备标签，例如 VID_03F0&PID_0A8E&MI_03&Col02
fn short_label(path: &str) -> String {
    if let Some(pos) = path.find("VID_") {
        let rest = &path[pos..];
        let end = rest.find('#').unwrap_or(rest.len());
        return rest[..end].to_string();
    }
    let end = path.rfind('#').unwrap_or(path.len());
    path[..end].to_string()
}

/// 便捷封装：创建通道并启动监听线程，返回事件接收端
pub fn spawn(cfg: Config, stop: Arc<AtomicBool>) -> Result<std::sync::mpsc::Receiver<HidEvent>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("hid-listen".to_string())
        .spawn(move || listen(&cfg, stop, tx))
        .context("启动 HID 监听线程失败")?;
    Ok(rx)
}
