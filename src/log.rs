//! 全局日志：同时输出到控制台与 logs/sound_switch.log
//!
//! - 级别: error(0) < warn(1) < info(2) < debug(3)
//!   默认 info：只打印耳机开关机等关键事件；音量、麦克风与原始 HID 数据需要 debug。
//! - 大小限制：日志文件超过 `log_max_kb`（默认 512KB）时轮转：
//!   `sound_switch.log` → `sound_switch.log.1`（只保留一个备份，旧备份被覆盖），
//!   因此 logs 目录占用上限约为 2 × log_max_kb。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

struct Sink {
    file: Option<std::fs::File>,
    written: u64,
}

struct LoggerInner {
    level: u8,
    dry_run: bool,
    log_path: PathBuf,
    max_bytes: u64,
    sink: Mutex<Sink>,
}

static LOGGER: OnceLock<LoggerInner> = OnceLock::new();

fn level_num(s: &str) -> u8 {
    match s.to_ascii_lowercase().as_str() {
        "error" => 0,
        "warn" => 1,
        "info" => 2,
        "debug" => 3,
        _ => 2,
    }
}

fn now_str() -> String {
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, st.wMilliseconds
    )
}

fn open_log(path: &Path) -> Sink {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(f) => {
            let written = f.metadata().map(|m| m.len()).unwrap_or(0);
            Sink {
                file: Some(f),
                written,
            }
        }
        Err(_) => Sink {
            file: None,
            written: 0,
        },
    }
}

/// 轮转：当前日志改名成 .1（覆盖旧备份），重新开始写
fn rotate(path: &Path) -> Sink {
    let backup = path.with_extension("log.1");
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::rename(path, &backup);
    open_log(path)
}

pub fn init(log_dir: &PathBuf, level: &str, dry_run: bool, max_kb: u64) {
    let log_path = log_dir.join("sound_switch.log");
    let sink = if log_dir.is_dir() {
        open_log(&log_path)
    } else {
        Sink {
            file: None,
            written: 0,
        }
    };
    let _ = LOGGER.set(LoggerInner {
        level: level_num(level),
        dry_run,
        log_path,
        max_bytes: max_kb.saturating_mul(1024),
        sink: Mutex::new(sink),
    });
}

fn write(lvl: &str, msg: &str) {
    let Some(l) = LOGGER.get() else { return };
    let lnum = level_num(lvl);
    if lnum > l.level {
        return;
    }
    let prefix = if l.dry_run && lnum >= 2 { "[dry-run] " } else { "" };
    let line = format!("{} [{}] {}{}\n", now_str(), lvl.to_uppercase(), prefix, msg);

    // 控制台（写失败也不影响运行：例如无窗口启动、标准输出被关闭时）
    {
        let mut out = std::io::stdout();
        let _ = out.write_all(line.as_bytes());
        let _ = out.flush();
    }

    // 文件：写之前先判断是否会超限，超限则先轮转
    if let Ok(mut sink) = l.sink.lock() {
        if sink.file.is_some()
            && l.max_bytes > 0
            && sink.written + line.len() as u64 > l.max_bytes
        {
            *sink = rotate(&l.log_path);
        }
        if let Some(f) = sink.file.as_mut() {
            if f.write_all(line.as_bytes()).is_ok() {
                let _ = f.flush();
                sink.written += line.len() as u64;
            } else {
                sink.file = None; // 写失败时静默降级为仅控制台
            }
        }
    }
}

pub fn error(msg: &str) {
    write("error", msg);
}
pub fn warn(msg: &str) {
    write("warn", msg);
}
pub fn info(msg: &str) {
    write("info", msg);
}
pub fn debug(msg: &str) {
    write("debug", msg);
}

pub fn dry_run() -> bool {
    LOGGER.get().map(|l| l.dry_run).unwrap_or(false)
}
