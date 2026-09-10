//! 单实例保护：确保同一个登录会话里只有一个 `run` 实例在监听。
//!
//! 用命名互斥体实现（`Local\` 前缀 = 按登录会话隔离，多用户/多会话各自独立）。
//! 进程退出或崩溃时由系统自动释放，不会留下需要手工清理的残留文件。

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// 持有互斥体句柄的守卫，Drop 时释放
pub struct Guard(Option<HANDLE>);

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(h) = self.0 {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

/// 尝试获取单实例锁：
/// - `Some(guard)`：当前是唯一实例（若创建互斥体失败则降级为"不阻塞"，guard 为空）
/// - `None`：同一会话里已有实例在运行
pub fn acquire() -> Option<Guard> {
    unsafe {
        match CreateMutexW(None, true, w!("Local\\sound-switch-single-instance")) {
            Ok(handle) => {
                // 名字已存在说明已有实例持有该互斥体
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = CloseHandle(handle);
                    None
                } else {
                    Some(Guard(Some(handle)))
                }
            }
            // 创建失败（极端情况）时不阻塞程序运行
            Err(_) => Some(Guard(None)),
        }
    }
}
