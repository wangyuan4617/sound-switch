//! 无窗口启动器（供“登录时自动启动”的计划任务使用）。
//!
//! 为什么需要它：
//!   计划任务直接启动控制台程序（sound-switch.exe）时会带出一个可见的控制台窗口；
//!   本启动器自身是 GUI 子系统（不分配控制台），再用 `CREATE_NO_WINDOW` 拉起真正的程序，
//!   因此开机自启全程无窗口、也不会闪一下。
//!
//! 行为：
//!   - 找到与自己同目录的 sound-switch.exe，以 `run` 参数启动后立即退出；
//!   - 工作目录：优先沿用当前目录（计划任务里设置的是项目根目录）；
//!     若当前目录没有 config.json，则回退到项目根目录（exe 上两级：target\release\..\..）。

#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

/// Win32: 以无窗口方式创建控制台进程
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn main() {
    // 退出码会记录在计划任务的“上次运行结果”里，便于排查：
    //   0 = 已成功拉起；2 = 找不到主程序；3 = 取自身路径失败；4 = 启动失败
    std::process::exit(spawn_agent());
}

fn spawn_agent() -> i32 {
    let Ok(self_path) = std::env::current_exe() else {
        return 3;
    };
    let Some(exe_dir) = self_path.parent().map(Path::to_path_buf) else {
        return 3;
    };

    let target = exe_dir.join("sound-switch.exe");
    if !target.exists() {
        return 2;
    }

    let mut cmd = Command::new(&target);
    cmd.arg("run");

    // 若当前目录不是项目根目录（没有 config.json），回退到 exe 上两级
    if !Path::new("config.json").exists() {
        if let Some(root) = exe_dir.parent().and_then(|p| p.parent()) {
            if root.join("config.json").exists() {
                cmd.current_dir(root);
            }
        }
    }

    // 启动后立即退出；真正的程序在后台无窗口运行
    match cmd.creation_flags(CREATE_NO_WINDOW).spawn() {
        Ok(_) => 0,
        Err(_) => 4,
    }
}
