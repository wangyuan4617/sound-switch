//! DTS Headphone:X 空间音效的「静默启用」。
//!
//! ## 为什么需要这个模块
//!
//! 实测结论（工具与原始数据见 `docs/experiments/`）：
//!
//! - Windows 里把「空间音效」设成 **DTS Headphone:X**，只是写了音频端点的属性
//!   （`HKLM\...\MMDevices\Audio\Render\{GUID}\Properties`），**并不会让效果真的生效**；
//! - 真正让 DTS 生效的是 **DTS Sound Unbound** 这个 UWP 应用：它启动后会和
//!   `DTSAPO3Service`（系统服务）做一次授权握手，界面上主页那张卡片会从
//!   「未授权」变成「已授权」；关机/重启后需要重新来一次，所以用户不得不
//!   「每次都手动打开 App 点一下」；
//! - 实测：**一旦握手完成，关闭 App 效果依然保留**（本次会话内），
//!   因此可以「启动 → 等它自检 → 必要时点一下 → 关掉」。
//! - 该应用没有命令行、没有公开接口，唯一可行的手段就是 **UI Automation**
//!   （UWP XAML 应用天然暴露完整的 UI Automation 树，实测可读可点）。
//!
//! ## 本模块做的事（对应需求：切换到耳机时启用，切换走时什么都不做）
//!
//! 1. 用 `IApplicationActivationManager::ActivateApplication` 启动应用（按 AUMID）；
//! 2. 立刻把它的窗口移到屏幕外（`x = -32000`），用户看不见；保持"可见但离屏"
//!    是为了不让 UWP 生命周期管理把它挂起；
//! 3. 用 UI Automation 读主页授权卡（`DTSXHPNewLicenseTile`）的文字：
//!    - 含「已授权」→ 认为已生效，进入第 4 步；
//!    - 仍是「未授权」或状态不明 → 触发「更多选项 → 更新许可证」
//!      （`Dotsx3Button` → `RefreshLicensesButton`），这正是用户平时手点的那一下；
//! 4. 成功后按配置关闭应用（`dts_close_after`）；若始终没确认成功，则**保留**应用
//!    在后台运行（窗口仍在屏幕外）并记警告，避免把用户唯一的补救入口也关掉。
//!
//! ## 风险与降级
//!
//! - 这是"替用户操作界面"，DTS 更新应用后控件 id 可能变化 → 失败只记日志，
//!   绝不影响音频切换本身；必要时可用 `dts_*` 配置项改控件/文字或直接关掉本功能。
//! - 启用流程可能耗时数十秒（应用要联网/本地校验授权），因此**在独立线程里跑**，
//!   不阻塞 HID 事件循环。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CoCreateInstance, CoInitializeEx,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    TreeScope_Descendants, UIA_InvokePatternId,
};
use windows::Win32::UI::Shell::{
    AO_NOERRORUI, ApplicationActivationManager, IApplicationActivationManager,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId,
    PostMessageW, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WM_CLOSE,
};
use windows::core::{BOOL, PCWSTR};

use crate::config::Config;
use crate::log;

/// 主页授权卡（读它的文字判断「已授权 / 未授权」）
const ID_LICENSE_TILE: &str = "DTSXHPNewLicenseTile";
/// 「更多选项」按钮（打开右上角菜单）
const ID_MORE_OPTIONS: &str = "Dotsx3Button";
/// 菜单里的「更新许可证」（= 用户平时手点的那一下）
const ID_REFRESH_LICENSES: &str = "RefreshLicensesButton";

/// 窗口被移到屏幕外的坐标（保持"可见"以免被 UWP 生命周期挂起）
const PARK_X: i32 = -32000;
const PARK_Y: i32 = -32000;
const PARK_W: i32 = 1200;
const PARK_H: i32 = 900;

/// 同一次只允许一个启用流程在跑
static ARMING: AtomicBool = AtomicBool::new(false);

/// 把 &str 转成以 NUL 结尾的宽字符串
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// 在调用线程初始化 COM（MTA；失败通常是已初始化过，忽略）
fn init_com_thread() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// 通过 AUMID 启动应用（已在运行则激活已有实例），返回其进程 id
fn activate(aumid: &str) -> Result<u32> {
    unsafe {
        // 说明：MS 文档建议"只为拉起目标应用"的进程用 CLSCTX_LOCAL_SERVER 创建该 COM 对象
        let mgr: IApplicationActivationManager =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                .context("CoCreateInstance(ApplicationActivationManager) 失败")?;
        let id = wide(aumid);
        let pid = mgr
            .ActivateApplication(PCWSTR(id.as_ptr()), PCWSTR::null(), AO_NOERRORUI)
            .map_err(|e| anyhow!("ActivateApplication 失败: {e}"))?;
        Ok(pid)
    }
}

/// EnumWindows 回调的上下文。
///
/// 实测（见 docs/experiments/trace-dts-windows.ps1）：
/// - 可见窗口是 `ApplicationFrameWindow`，标题固定为应用显示名，且属于
///   **ApplicationFrameHost.exe**（不是应用进程）；
/// - 这个版本的 DTS Sound Unbound **没有** `Windows.UI.Core.CoreWindow` 子窗口，
///   所以"靠子窗口 PID 认框架窗口"不可靠 → 以**标题匹配框架窗口**为首选。
struct WinFind {
    pid: u32,
    title: Vec<u16>,
    /// 首选：标题匹配的框架窗口
    frame_by_title: Option<HWND>,
    /// 次选：内部有属于目标进程的 CoreWindow 子窗口的框架窗口
    frame_by_core: Option<HWND>,
    /// 兜底：属于目标进程的任意顶层窗口
    owned: Option<HWND>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = unsafe { &mut *(lparam.0 as *mut WinFind) };

    let mut class = [0u16; 128];
    let cn = unsafe { GetClassNameW(hwnd, &mut class) };
    let class = if cn > 0 {
        String::from_utf16_lossy(&class[..cn as usize])
    } else {
        String::new()
    };

    let mut wpid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut wpid)) };

    if class == "ApplicationFrameWindow" {
        // 首选：标题一致
        if !ctx.title.is_empty() {
            let mut t = [0u16; 512];
            let tn = unsafe { GetWindowTextW(hwnd, &mut t) };
            if tn > 0 && t[..tn as usize] == ctx.title[..] {
                ctx.frame_by_title = Some(hwnd);
                return BOOL(0); // 命中最佳匹配，停止枚举
            }
        }
        // 次选：框架里含属于目标进程的 CoreWindow 子窗口
        if ctx.pid != 0 && ctx.frame_by_core.is_none() {
            let cls = wide("Windows.UI.Core.CoreWindow");
            if let Ok(child) =
                unsafe { FindWindowExW(Some(hwnd), None, PCWSTR(cls.as_ptr()), PCWSTR::null()) }
            {
                if !child.0.is_null() {
                    let mut cpid = 0u32;
                    unsafe { GetWindowThreadProcessId(child, Some(&mut cpid)) };
                    if cpid == ctx.pid {
                        ctx.frame_by_core = Some(hwnd);
                    }
                }
            }
        }
    } else if ctx.pid != 0 && wpid == ctx.pid && ctx.owned.is_none() {
        // 兜底：应用自己的顶层窗口（可能是 IME 之类的辅助窗口，不理想但聊胜于无）
        ctx.owned = Some(hwnd);
    }

    BOOL(1)
}

fn find_app_window(pid: u32, title: &str) -> Option<HWND> {
    let mut ctx = WinFind {
        pid,
        title: wide(title),
        frame_by_title: None,
        frame_by_core: None,
        owned: None,
    };
    unsafe {
        let _ = EnumWindows(Some(enum_windows_proc), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.frame_by_title.or(ctx.frame_by_core).or(ctx.owned)
}

/// 把窗口移到屏幕外（保持"可见"，避免 UWP 被挂起）
fn park_window(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            PARK_X,
            PARK_Y,
            PARK_W,
            PARK_H,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// UI Automation 的薄封装
struct Uia {
    uia: IUIAutomation,
}

impl Uia {
    fn new() -> Result<Self> {
        unsafe {
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .context("CoCreateInstance(CUIAutomation) 失败")?;
            Ok(Self { uia })
        }
    }

    fn root_of(&self, hwnd: HWND) -> Result<IUIAutomationElement> {
        unsafe {
            self.uia
                .ElementFromHandle(hwnd)
                .context("ElementFromHandle 失败")
        }
    }

    /// 按 AutomationId 在窗口内查找元素（一次取全部后代再本地比对，避免构造 VARIANT 条件）
    fn find_by_id(&self, root: &IUIAutomationElement, id: &str) -> Result<Option<IUIAutomationElement>> {
        unsafe {
            let cond = self.uia.CreateTrueCondition().context("CreateTrueCondition 失败")?;
            let all = root
                .FindAll(TreeScope_Descendants, &cond)
                .context("FindAll 失败")?;
            let n = all.Length().unwrap_or(0);
            for i in 0..n {
                let Ok(el) = all.GetElement(i) else { continue };
                let Ok(aid) = el.CurrentAutomationId() else {
                    continue;
                };
                if aid.to_string() == id {
                    return Ok(Some(el));
                }
            }
        }
        Ok(None)
    }
}

fn element_name(el: &IUIAutomationElement) -> String {
    unsafe { el.CurrentName().map(|b| b.to_string()).unwrap_or_default() }
}

fn invoke(el: &IUIAutomationElement) -> Result<()> {
    unsafe {
        let pat: IUIAutomationInvokePattern = el
            .GetCurrentPatternAs(UIA_InvokePatternId)
            .context("元素不支持 Invoke（可能控件类型变了）")?;
        pat.Invoke().context("Invoke 调用失败")?;
    }
    Ok(())
}

/// 读授权卡文字；返回 None 表示卡片还没出现（页面尚未加载完）
fn read_license_tile(uia: &Uia, hwnd: HWND) -> Result<Option<String>> {
    let root = uia.root_of(hwnd)?;
    match uia.find_by_id(&root, ID_LICENSE_TILE)? {
        Some(tile) => Ok(Some(element_name(&tile))),
        None => Ok(None),
    }
}

/// 触发「更多选项 → 更新许可证」（= 用户手动点的那一下）
fn trigger_refresh(uia: &Uia, hwnd: HWND) -> Result<()> {
    let root = uia.root_of(hwnd)?;
    let more = uia
        .find_by_id(&root, ID_MORE_OPTIONS)?
        .ok_or_else(|| anyhow!("找不到「更多选项」按钮（{ID_MORE_OPTIONS}）"))?;
    invoke(&more)?;
    std::thread::sleep(Duration::from_millis(1200));

    let root = uia.root_of(hwnd)?;
    let refresh = uia
        .find_by_id(&root, ID_REFRESH_LICENSES)?
        .ok_or_else(|| anyhow!("找不到「更新许可证」菜单项（{ID_REFRESH_LICENSES}）"))?;
    invoke(&refresh)?;
    Ok(())
}

/// 等进程退出；返回 true 表示已退出
fn wait_process_exit(pid: u32, ms: u32) -> bool {
    if pid == 0 {
        return true;
    }
    unsafe {
        let Ok(handle) = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            false,
            pid,
        ) else {
            return true; // 打不开通常意味着进程已经没了
        };
        let _ = WaitForSingleObject(handle, ms);
        let mut code = 0u32;
        let alive = GetExitCodeProcess(handle, &mut code).is_ok() && code == 259; // STILL_ACTIVE
        let _ = CloseHandle(handle);
        !alive
    }
}

fn force_kill(pid: u32) {
    if pid == 0 {
        return;
    }
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(handle, 0);
            let _ = CloseHandle(handle);
        }
    }
}

/// 关闭应用。
///
/// 实测行为：DTS Sound Unbound 的窗口被关闭时**默认转入后台**（窗口消失、进程常驻），
/// 这正是用户平时"关掉窗口它还在后台"的现象。既然需求是"设置完成后关闭 App"，
/// 这里在窗口关闭后再结束它的进程（实测关闭后音效仍然保留）。
fn close_app(hwnd: HWND, pid: u32, window_title: &str) {
    for attempt in 1..=2u32 {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if wait_process_exit(pid, 5000) {
            log::debug("DTS: 应用进程已退出");
            return;
        }
        if attempt == 1 {
            if find_app_window(pid, window_title).is_none() {
                log::debug("DTS: 窗口已关闭、进程转入后台（应用自身行为），按要求结束其进程");
            } else {
                log::debug("DTS: 首次关闭未生效，重试一次");
            }
        }
    }
    log::debug("DTS: 结束 DTS Sound Unbound 进程");
    force_kill(pid);
    let _ = wait_process_exit(pid, 3000);
}

/// 同步执行一次「确保 DTS 已启用」流程
pub fn arm(cfg: &Config) -> Result<()> {
    // dry-run 下绝不真的去启动/关闭第三方应用（日志前缀由 log 模块自动添加）
    if log::dry_run() {
        log::info("跳过 DTS Sound Unbound 的启动/启用流程");
        return Ok(());
    }

    let started = Instant::now();
    init_com_thread();

    log::info(&format!("DTS: 启动 DTS Sound Unbound（{}）", cfg.dts_aumid));
    let pid = activate(&cfg.dts_aumid)?;
    log::debug(&format!("DTS: ActivateApplication -> pid={pid}"));

    // 1) 等窗口出现，并把窗口挪到屏幕外（100ms 轮询，尽量不闪）
    let win_deadline = started + Duration::from_secs(25);
    let mut frame = None;
    while Instant::now() < win_deadline {
        if let Some(h) = find_app_window(pid, &cfg.dts_window_title) {
            park_window(h);
            frame = Some(h);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let Some(frame) = frame else {
        return Err(anyhow!(
            "未找到 DTS 应用窗口（pid={pid}）—— 应用可能未安装或 AUMID 有误"
        ));
    };
    log::debug("DTS: 窗口已移到屏幕外");

    // 2) 轮询授权状态；必要时触发「更新许可证」
    let uia = Uia::new()?;
    let deadline = started + Duration::from_millis(cfg.dts_arm_timeout_ms);
    // 先给应用一点自检时间（实测它自己也会握手成功），再考虑替它点
    let mut next_refresh_at = started + Duration::from_secs(20);
    let mut refreshes = 0u32;
    let mut last_seen = String::new();
    let mut armed = false;

    loop {
        park_window(frame); // 应用可能自己把窗口再显示出来，持续压回屏幕外

        match read_license_tile(&uia, frame) {
            Ok(Some(name)) => {
                if name != last_seen {
                    log::info(&format!("DTS: 授权状态『{name}』"));
                    last_seen = name.clone();
                }
                if name.contains(&cfg.dts_licensed_text) {
                    armed = true;
                    break;
                }
            }
            Ok(None) => { /* 页面还没渲染出来 */ }
            Err(e) => log::debug(&format!("DTS: 读取授权卡失败: {e}")),
        }

        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if now >= next_refresh_at && refreshes < 3 {
            refreshes += 1;
            log::info(&format!("DTS: 触发「更新许可证」（第 {refreshes} 次）"));
            if let Err(e) = trigger_refresh(&uia, frame) {
                log::warn(&format!("DTS: 触发失败: {e}"));
            }
            next_refresh_at = now + Duration::from_secs(15);
        }
        std::thread::sleep(Duration::from_millis(600));
    }

    // 3) 收尾
    if armed {
        log::info("DTS: 已授权（DTS Headphone:X 生效）");
        if cfg.dts_close_after {
            // 刚变成"已授权"时应用可能还在收尾（通知 APO3 服务、落盘等），
            // 立刻关掉有打断的风险，也容易被闪屏阶段忽略 WM_CLOSE → 先等几秒。
            std::thread::sleep(Duration::from_secs(4));
            park_window(frame);
            log::info("DTS: 关闭 DTS Sound Unbound");
            close_app(frame, pid, &cfg.dts_window_title);
        } else {
            log::debug("DTS: 按配置保留应用在后台运行（窗口在屏幕外）");
        }
    } else {
        log::warn(&format!(
            "DTS: {} 秒内未确认授权状态；保留应用在后台运行（窗口已在屏幕外），必要时可手动查看",
            cfg.dts_arm_timeout_ms / 1000
        ));
    }
    Ok(())
}

/// 在独立线程里执行启用流程（不阻塞 HID 事件循环）。
/// 同一时刻只允许一个流程，重复调用直接跳过。
///
/// 返回线程句柄：`run` 模式下可以丢弃（进程长期存活），但**短命进程**
/// （例如 `set-default` 这种命令行一次性调用）必须 join，否则会随主进程一起退出。
pub fn spawn_arm(cfg: Config) -> Option<std::thread::JoinHandle<()>> {
    if log::dry_run() {
        log::info("跳过 DTS Sound Unbound 的启动/启用流程");
        return None;
    }
    if ARMING.swap(true, Ordering::SeqCst) {
        log::info("DTS: 上一次启用流程仍在进行，本次跳过");
        return None;
    }
    Some(std::thread::spawn(move || {
        if let Err(e) = arm(&cfg) {
            log::error(&format!("DTS: 启用流程失败: {e}"));
        }
        ARMING.store(false, Ordering::SeqCst);
    }))
}
