//! sound-switch CLI 入口
//!
//! 用法:
//!   sound-switch run                # 监听 HID 并自动切换默认输出设备（默认命令）
//!   sound-switch list               # 列出匹配 HID 设备与全部渲染端点
//!   sound-switch status             # 显示当前默认输出与 state
//!   sound-switch set-default        # 手动执行“切到耳机”（等价 power on，用于测试）
//!   sound-switch restore            # 手动执行“恢复原默认”（等价 power off，用于测试）
//! 通用参数:
//!   --config <path>   配置文件路径（默认 ./config.json）
//!   --dry-run         只打印动作不真正切换

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::Result;

mod app;
mod audio;
mod config;
mod hid;
mod log;
mod single_instance;
mod state;

use config::Config;

fn usage() {
    println!(
        "sound-switch — 根据 HyperX 耳机开机/关机自动切换系统默认输出设备

用法:
  sound-switch run                # 监听 HID 并自动切换（默认）
  sound-switch list               # 列出匹配 HID 设备与全部渲染端点
  sound-switch status             # 显示当前默认输出、目标端点、state 概览
  sound-switch set-default        # 手动执行“切到耳机”（测试用）
  sound-switch restore            # 手动执行“恢复原默认”（测试用）

通用参数:
  --config <path>                 配置文件路径，默认 ./config.json（首次运行自动生成）
  --dry-run                       只打印动作，不真正切换默认设备
"
    );
}

struct Args {
    cmd: String,
    config: PathBuf,
    dry_run: bool,
}

fn parse_args() -> Result<Args> {
    let mut cmd = "run".to_string();
    let mut config = PathBuf::from(Config::DEFAULT_CONFIG);
    let mut dry_run = false;

    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            "--config" => {
                i += 1;
                config = PathBuf::from(
                    raw.get(i)
                        .ok_or_else(|| anyhow::anyhow!("--config 缺少参数"))?,
                );
            }
            "--dry-run" => dry_run = true,
            s if !s.starts_with("--") => cmd = s.to_string(),
            other => return Err(anyhow::anyhow!("未知参数: {other}")),
        }
        i += 1;
    }
    Ok(Args {
        cmd,
        config,
        dry_run,
    })
}

/// 确保 logs 目录存在
fn ensure_log_dir() -> PathBuf {
    let dir = PathBuf::from("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn cmd_list(cfg: &Config) -> Result<()> {
    // HID
    println!(
        "--- 匹配 HID 设备 (VID {}/PID {}) ---",
        cfg.vendor_id, cfg.product_id
    );
    if let Ok(api) = hidapi::HidApi::new() {
        let found: Vec<_> = api
            .device_list()
            .filter(|d| d.vendor_id() == cfg.vendor_id && d.product_id() == cfg.product_id)
            .collect();
        if found.is_empty() {
            println!("(无匹配设备)");
        }
        for d in found {
            println!(
                "path={} usage_page=0x{:04X} usage=0x{:04X}",
                d.path().to_string_lossy(),
                d.usage_page(),
                d.usage()
            );
        }
    }
    // 渲染端点
    println!("--- 当前活动渲染端点 ---");
    for ep in audio::list_render_endpoints()? {
        let mark = if ep
            .name
            .to_lowercase()
            .contains(&cfg.audio_keyword.to_lowercase())
        {
            "  <== 目标关键词匹配"
        } else {
            ""
        };
        println!("  {} | {}{}", ep.name, ep.id, mark);
    }
    Ok(())
}

fn cmd_status(cfg: &Config) -> Result<()> {
    let st = state::State::load(&PathBuf::from(state::DEFAULT_STATE));
    println!("headset_active        : {}", st.headset_active);
    println!("headset_endpoint_id   : {:?}", st.headset_endpoint_id);
    println!("previous_defaults     : {:?}", st.previous_defaults);
    println!("audio_keyword         : {}", cfg.audio_keyword);
    for role_name in &cfg.roles {
        if let Some(role) = audio::parse_role(role_name) {
            match audio::current_default_endpoint_id(role) {
                Ok(Some(id)) => {
                    println!("当前默认输出[{}]: {}", role_name, id);
                }
                _ => println!("当前默认输出[{}]: (无)", role_name),
            }
        }
    }
    match audio::find_endpoint_by_keyword(&cfg.audio_keyword) {
        Ok(Some(ep)) => println!("目标端点存在: {} ({})", ep.name, ep.id),
        Ok(None) => println!("目标端点不存在（耳机当前可能未开机）"),
        Err(e) => println!("查询目标端点失败: {e}"),
    }
    Ok(())
}

fn main() -> Result<()> {
    // 让控制台用 UTF-8 输出，避免中文乱码（只影响本进程控制台）
    let _ = unsafe { windows::Win32::System::Console::SetConsoleOutputCP(65001) };

    let args = parse_args()?;
    let cfg = Config::load(&args.config)?;
    let log_dir = ensure_log_dir();
    log::init(&log_dir, &cfg.log_level, args.dry_run, cfg.log_max_kb);

    // 音频切换使用 Core Audio COM，需要先初始化（失败仅告警，HID 功能仍可用）
    if let Err(e) = audio::init_com() {
        log::warn(&format!("COM 初始化失败，音频切换功能可能不可用: {e}"));
    }

    match args.cmd.as_str() {
        "list" => {
            cmd_list(&cfg)?;
        }
        "status" => {
            cmd_status(&cfg)?;
        }
        "set-default" => {
            let mut actions = app::Actions::new(cfg);
            actions.manual_set_headset_default();
        }
        "restore" => {
            let mut actions = app::Actions::new(cfg);
            actions.manual_restore();
        }
        "run" | _ => {
            // 单实例保护：开机自启的实例与手动启动的实例不会同时监听
            let Some(_guard) = single_instance::acquire() else {
                log::warn("已有 sound-switch 实例正在运行（单实例保护），本次退出");
                return Ok(());
            };
            let actions = app::Actions::new(cfg);
            let stop = Arc::new(AtomicBool::new(false));
            let stop2 = stop.clone();
            let _ = ctrlc::set_handler(move || {
                log::info("收到 Ctrl+C，正在退出…");
                stop2.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            app::run_forever(actions, stop);
        }
    }
    Ok(())
}
