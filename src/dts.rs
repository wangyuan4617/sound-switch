//! DTS Headphone:X 空间音效的「自动恢复」。
//!
//! ## 机制（2026-09-30 用客观属性 + 用户演示确认）
//!
//! 让 DTS 真正生效需要**两件事同时成立**，缺一不可：
//!
//! 1. **Windows 侧**：耳机端点的「空间音效」必须选中 **DTS Headphone:X**。
//!    它以三个端点属性持久化（`HKLM\...\MMDevices\Audio\Render\{GUID}\Properties`）：
//!    - `{908dba32-…},2`：`[12..16]` = 启用标志（`01000000` = 开），`[24..40]` = 当前引擎 GUID
//!    - `{8a845654-…},2`：`[16..32]` = 已激活引擎 GUID
//!    - `{fd8a7b27-…},2`：空间音频配置（9 字节 = 空，154 字节 = DTS 配置）
//!    这个设置会**被系统重置**（重启/待机/设备重枚举后变回"关闭"或 Windows Sonic），
//!    这就是"有时候必须手动点一下"的真正原因。
//!    **普通用户写不了这三个属性**（实测"不允许所请求的注册表访问权"），
//!    只能通过应用界面让 App（走系统服务）去写。
//! 2. **应用侧**：DTS Sound Unbound 必须与 `DTSAPO3Service` 完成授权握手
//!    （主页授权卡从「未授权」变「已授权」），否则即使选中了 DTS 也不出声。
//!
//! ## 本模块的流程（对应需求：切换到耳机时启用，切换走时什么都不做）
//!
//! 1. 按 AUMID 启动应用（`IApplicationActivationManager`），窗口立刻移到屏幕外；
//! 2. 读授权卡（`DTSXHPNewLicenseTile`）；必要时触发「更多选项 → 更新许可证」；
//! 3. **检查端点空间音效属性**：若已是 DTS Headphone:X 就结束；否则在应用里
//!    切到「耳机 X」页面，逐个点击名字含「启用」的元素：
//!    - `启用 DTS 耳机 X` → 支持 InvokePattern，直接点；
//!    - `启用 数字影院系统耳机：X` → 实测是 **Text 元素**，任何 UIA pattern 都点不动，
//!      只能把窗口临时显示出来做**真实鼠标点击**（点完立刻移回屏幕外）。
//!    每轮都用端点属性复核，直到变成 DTS 或超时。
//! 4. **仅当属性确认已切到 DTS** 时才按配置关闭应用；否则保留应用在后台并记警告。
//!
//! ## 风险与降级
//!
//! - 这是"替用户操作界面"（该应用没有任何接口）。DTS 更新后控件可能变化 → 失败只记日志，
//!   绝不影响音频切换；可用 `dts_*` 配置项改文字/关闭功能。
//! - 需要点击「启用」时会出现一次**短暂的窗口闪现**（鼠标点击兜底要求窗口在屏幕上），
//!   可用 `dts_allow_mouse_click: false` 关掉该兜底；已启用时不会有任何窗口出现。
//! - 流程可能耗时数十秒，因此**在独立线程里跑**，不阻塞 HID 事件循环。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, WPARAM};
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
    IUIAutomationLegacyIAccessiblePattern, IUIAutomationSelectionItemPattern,
    IUIAutomationTogglePattern, TreeScope_Descendants, UIA_InvokePatternId,
    UIA_LegacyIAccessiblePatternId, UIA_SelectionItemPatternId, UIA_TogglePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    SendInput, VK_SPACE, mouse_event,
};
use windows::Win32::UI::Shell::{
    AO_NOERRORUI, ApplicationActivationManager, IApplicationActivationManager,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    PostMessageW, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SetCursorPos, SetForegroundWindow,
    SetWindowPos, WM_CLOSE,
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
/// 需要真实鼠标点击时，窗口临时显示的位置/大小
const SHOW_X: i32 = 80;
const SHOW_Y: i32 = 80;
const SHOW_W: i32 = 1000;
const SHOW_H: i32 = 760;

/// DTS Headphone:X 引擎 GUID 的字节序（小端，对应 4444acb0-8dc0-4c2c-a0d8-2c76db470f86）
const DTS_ENGINE: [u8; 16] = [
    0xB0, 0xAC, 0x44, 0x44, 0xC0, 0x8D, 0x2C, 0x4C, 0xA0, 0xD8, 0x2C, 0x76, 0xDB, 0x47, 0x0F, 0x86,
];
/// 端点属性：选中的空间音效引擎 + 启用标志
const PROP_SELECTED: &str = "{908dba32-edff-4c28-8e45-c918561f6748},2";
/// 端点属性：已激活引擎
const PROP_ACTIVATED: &str = "{8a845654-d6c3-4cd7-b4eb-243d4bd99032},2";
/// 端点属性：空间音频配置（9 字节 = 空，154 字节 = DTS 配置）
const PROP_CONFIG: &str = "{fd8a7b27-0b18-4025-ab1c-bdd6b32e1604},2";
/// 「耳机 X」页面的导航项（AutomationId，与界面语言无关）
const ID_HPX_NAV: &str = "HPXRadioButton";
/// 「主页」导航项：授权卡只在主页上，所以读授权前必须先回到主页
const ID_HOME_NAV: &str = "HomeRadioButton";
/// 「启用」按钮/文字上可能出现的字样（中英文都匹配）
const ENABLE_WORDS: [&str; 4] = ["启用", "Enable", "Activate", "Turn on"];
/// 反过来：这些字样表示"禁用"，不要点
const DISABLE_WORDS: [&str; 3] = ["禁用", "Disable", "Turn off"];

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

/// 临时把窗口显示到屏幕上（仅用于"真实鼠标点击"兜底：离屏窗口取不到可点坐标）
fn show_window(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            SHOW_X,
            SHOW_Y,
            SHOW_W,
            SHOW_H,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
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
    /// 按名字关键词查找可用元素（用于找没有 AutomationId 的「启用…」按钮/文字）
    fn find_all_by_name(
        &self,
        root: &IUIAutomationElement,
        include: &[&str],
        exclude: &[&str],
    ) -> Result<Vec<IUIAutomationElement>> {
        let mut out = Vec::new();
        unsafe {
            let cond = self.uia.CreateTrueCondition().context("CreateTrueCondition 失败")?;
            let all = root
                .FindAll(TreeScope_Descendants, &cond)
                .context("FindAll 失败")?;
            let n = all.Length().unwrap_or(0);
            for i in 0..n {
                let Ok(el) = all.GetElement(i) else { continue };
                let name = element_name(&el);
                if name.is_empty() || !element_enabled(&el) {
                    continue;
                }
                if include.iter().any(|w| name.contains(w))
                    && !exclude.iter().any(|w| name.contains(w))
                {
                    out.push(el);
                }
            }
        }
        Ok(out)
    }
}

fn element_name(el: &IUIAutomationElement) -> String {
    unsafe { el.CurrentName().map(|b| b.to_string()).unwrap_or_default() }
}

/// 元素是否可用（IsEnabled）
fn element_enabled(el: &IUIAutomationElement) -> bool {
    unsafe { el.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false) }
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

/// 依次尝试 Invoke / Toggle / SelectionItem / LegacyIAccessible；都不支持则返回 false
fn click_by_pattern(el: &IUIAutomationElement) -> Option<&'static str> {
    unsafe {
        if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) {
            if p.Invoke().is_ok() {
                return Some("InvokePattern");
            }
        }
        if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) {
            if p.Toggle().is_ok() {
                return Some("TogglePattern");
            }
        }
        if let Ok(p) =
            el.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
        {
            if p.Select().is_ok() {
                return Some("SelectionItemPattern");
            }
        }
        if let Ok(p) = el
            .GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                UIA_LegacyIAccessiblePatternId,
            )
        {
            if p.DoDefaultAction().is_ok() {
                return Some("LegacyIAccessible");
            }
        }
    }
    None
}

/// 按键激活：把窗口设为前台 → **立刻压回屏幕外** → 元素聚焦 → 发送空格。
///
/// 键盘输入不依赖窗口位置，所以这条路可以做到"用户看不到窗口"；
/// 但激活的瞬间系统可能把窗口拉到屏幕上，因此在 SetForegroundWindow 之后马上再 park 一次。
/// 结束后会还原原来的前台窗口，避免抢走用户的焦点。
fn press_key_on(el: &IUIAutomationElement, frame: HWND) -> bool {
    unsafe {
        let prev = GetForegroundWindow();
        let _ = SetForegroundWindow(frame);
        park_window(frame); // 激活后马上藏回去
        std::thread::sleep(Duration::from_millis(150));
        let focused = el.SetFocus().is_ok();
        let mut sent = false;
        if focused {
            std::thread::sleep(Duration::from_millis(150));
            park_window(frame);
            let mk = |flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_SPACE,
                        wScan: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let inputs = [mk(Default::default()), mk(KEYEVENTF_KEYUP)];
            sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32;
            std::thread::sleep(Duration::from_millis(150));
        }
        park_window(frame);
        // 还原原来的前台窗口（我们只是借用一下焦点）
        if !prev.0.is_null() && prev != frame {
            let _ = SetForegroundWindow(prev);
        }
        sent
    }
}

/// 点击一个元素：先试各种 UIA pattern / 按键激活；都不行才（可选）用真实鼠标点击。
///
/// 实测：「启用 数字影院系统耳机：X」是个 **Text 元素**（有点击处理但没有任何 pattern），
/// 只有真实鼠标点击有效 —— 这是它必须把窗口临时显示出来的原因。
/// `click_element` 返回 (是否点到, 是否用了鼠标兜底)
fn click_element(el: &IUIAutomationElement, frame: HWND, allow_mouse: bool) -> (bool, bool) {
    if let Some(how) = click_by_pattern(el) {
        log::info(&format!("DTS: 点击成功（{how}，未显示窗口）"));
        return (true, false);
    }
    if press_key_on(el, frame) {
        log::info("DTS: 点击成功（借用焦点 + 空格，未显示窗口）");
        return (true, false);
    }
    if !allow_mouse {
        return (false, false);
    }
    // 鼠标兜底：窗口必须真的出现在屏幕上，点击才有效 → 显示时间尽量短
    show_window(frame);
    std::thread::sleep(Duration::from_millis(350));
    let mut pt = POINT::default();
    if let Err(e) = unsafe { el.GetClickablePoint(&mut pt) } {
        log::debug(&format!("DTS: 取可点坐标失败: {e}"));
        park_window(frame);
        return (false, true);
    }
    unsafe {
        let _ = SetCursorPos(pt.x, pt.y);
    }
    std::thread::sleep(Duration::from_millis(60));
    unsafe {
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
    }
    std::thread::sleep(Duration::from_millis(40));
    unsafe {
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
    // 立刻移回屏幕外（把"窗口可见"的时间压到最短）
    std::thread::sleep(Duration::from_millis(60));
    park_window(frame);
    log::info(&format!(
        "DTS: 该元素只能用鼠标点击，已在屏幕显示约 0.5 秒后移回屏幕外（坐标 {},{}）",
        pt.x, pt.y
    ));
    (true, true)
}

// ---------------------------------------------------------------- 端点空间音效状态

/// 从端点 id（`{0.0.0.00000000}.{GUID}`）里取出 GUID 段
fn endpoint_guid(endpoint_id: &str) -> Option<&str> {
    let start = endpoint_id.rfind('{')?;
    let end = endpoint_id.rfind('}')?;
    if end > start { Some(&endpoint_id[start..=end]) } else { None }
}

/// 端点当前的空间音效状态（读注册表，只读；写入需要管理员，见模块注释）
struct SpatialState {
    /// 三个属性是否一致指向"已启用的 DTS Headphone:X"
    enabled: bool,
    flag: String,
    selected: String,
    activated: String,
    config_len: i64,
    /// 属性是否读到了（读不到时不要据此下结论）
    read_ok: bool,
}

fn read_spatial_state(guid: &str) -> SpatialState {
    let mut st = SpatialState {
        enabled: false,
        flag: String::new(),
        selected: String::new(),
        activated: String::new(),
        config_len: -1,
        read_ok: false,
    };
    let path = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{guid}\\Properties"
    );
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let Ok(key) = hklm.open_subkey_with_flags(&path, winreg::enums::KEY_READ) else {
        return st;
    };
    let read = |name: &str| -> Option<Vec<u8>> {
        key.get_raw_value(name).ok().map(|v| v.bytes.to_vec())
    };
    let (Some(a), Some(b), Some(c)) = (
        read(PROP_SELECTED),
        read(PROP_ACTIVATED),
        read(PROP_CONFIG),
    ) else {
        return st;
    };
    st.read_ok = true;
    st.config_len = c.len() as i64;
    st.flag = hex(&a.get(12..16).unwrap_or(&[]));
    st.selected = hex(&a.get(24..40).unwrap_or(&[]));
    st.activated = hex(&b.get(16..32).unwrap_or(&[]));
    let flag_on = a.get(12..16) == Some(&[1u8, 0, 0, 0]);
    let sel_dts = a.get(24..40) == Some(&DTS_ENGINE);
    let act_dts = b.get(16..32) == Some(&DTS_ENGINE);
    st.enabled = flag_on && sel_dts && act_dts;
    st
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02X}");
    }
    s
}

/// 在应用里把端点空间音效切成 DTS Headphone:X（复刻用户手动点击「启用」）。
/// 返回是否确认成功（用端点属性客观判定）。
fn ensure_spatial_enabled(
    cfg: &Config,
    uia: &Uia,
    frame: HWND,
    endpoint_id: &str,
    deadline: Instant,
) -> Result<bool> {
    let Some(guid) = endpoint_guid(endpoint_id) else {
        return Err(anyhow!("无法从端点 id 解析 GUID: {endpoint_id}"));
    };
    let st = read_spatial_state(guid);
    if !st.read_ok {
        log::debug(&format!("DTS: 读不到端点 {guid} 的空间音效属性，跳过校验"));
        return Ok(true);
    }
    if st.enabled {
        log::info("DTS: 端点空间音效已是 DTS Headphone:X");
        return Ok(true);
    }
    log::info(&format!(
        "DTS: 端点空间音效未启用（flag={} sel={} act={} 配置={}字节），在应用里点击『启用』",
        st.flag,
        first8(&st.selected),
        first8(&st.activated),
        st.config_len
    ));

    // 启用按钮在「耳机 X」页面上 → 先切过去
    park_window(frame);
    if goto_page(uia, frame, ID_HPX_NAV) {
        log::debug("DTS: 已切到『耳机 X』页面");
        std::thread::sleep(Duration::from_secs(3));
    } else {
        log::debug("DTS: 切换『耳机 X』页面失败（继续尝试直接找按钮）");
    }

    let mut tries: Vec<(String, u32)> = Vec::new();
    while Instant::now() < deadline {
        park_window(frame);
        if read_spatial_state(guid).enabled {
            log::info("DTS: 端点空间音效已切换为 DTS Headphone:X");
            return Ok(true);
        }
        let root = uia.root_of(frame)?;
        let mut clicked = false;
        for el in uia.find_all_by_name(&root, &ENABLE_WORDS, &DISABLE_WORDS)? {
            let name = element_name(&el);
            let count = tries.iter().find(|(n, _)| *n == name).map(|(_, c)| *c).unwrap_or(0);
            if count >= 2 {
                continue; // 同一个元素最多试两次，避免死循环
            }
            log::info(&format!("DTS: 点击『{name}』（第 {} 次）", count + 1));
            let (clicked_ok, _used_mouse) = click_element(&el, frame, cfg.dts_allow_mouse_click);
            if clicked_ok {
                match tries.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, c)) => *c += 1,
                    None => tries.push((name, 1)),
                }
                clicked = true;
                std::thread::sleep(Duration::from_secs(5));
            } else {
                log::debug(&format!("DTS: 『{name}』无法点击（尝试过的所有方式都失败）"));
                tries.push((name, 2)); // 标记为已放弃
            }
        }
        if !clicked {
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    let st = read_spatial_state(guid);
    if st.enabled {
        Ok(true)
    } else {
        log::warn(&format!(
            "DTS: 超时仍未把端点空间音效切到 DTS Headphone:X（flag={} sel={} act={} 配置={}字节）",
            st.flag,
            first8(&st.selected),
            first8(&st.activated),
            st.config_len
        ));
        Ok(false)
    }
}

fn first8(s: &str) -> String {
    s.chars().take(8).collect()
}

/// 读授权卡文字；返回 None 表示卡片还没出现（页面尚未加载完 / 不在主页）
fn read_license_tile(uia: &Uia, hwnd: HWND) -> Result<Option<String>> {
    let root = uia.root_of(hwnd)?;
    match uia.find_by_id(&root, ID_LICENSE_TILE)? {
        Some(tile) => Ok(Some(element_name(&tile))),
        None => Ok(None),
    }
}

/// 切到指定导航页面（主页 / 耳机 X / DTS X）；返回是否点成功
fn goto_page(uia: &Uia, hwnd: HWND, nav_id: &str) -> bool {
    let Ok(root) = uia.root_of(hwnd) else {
        return false;
    };
    match uia.find_by_id(&root, nav_id) {
        Ok(Some(nav)) => click_by_pattern(&nav).is_some(),
        _ => false,
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

/// 同步执行一次「确保 DTS 已启用」流程。
///
/// `endpoint_id` 是耳机渲染端点的 id（`{0.0.0.00000000}.{GUID}`）：
/// 有它才能检查/修复 Windows 侧的空间音效选择；为 None 时只做应用侧的授权。
pub fn arm(cfg: &Config, endpoint_id: Option<&str>) -> Result<()> {
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

    // 2) 回主页读授权卡（授权卡只在主页上；App 会记住上次停留的页面）
    let uia = Uia::new()?;
    park_window(frame);
    if goto_page(&uia, frame, ID_HOME_NAV) {
        log::debug("DTS: 已切到『主页』读取授权状态");
        std::thread::sleep(Duration::from_secs(2));
    }

    let deadline = started + Duration::from_millis(cfg.dts_arm_timeout_ms);
    // 先给应用一点自检时间（实测它自己也会握手成功），再考虑替它点
    let mut next_refresh_at = started + Duration::from_secs(20);
    let mut refreshes = 0u32;
    let mut last_seen = String::new();
    let mut armed = false;
    // 授权卡长时间找不到（例如界面结构变了）就不要空等到超时
    let mut missing_since: Option<Instant> = None;

    loop {
        park_window(frame); // 应用可能自己把窗口再显示出来，持续压回屏幕外

        match read_license_tile(&uia, frame) {
            Ok(Some(name)) => {
                missing_since = None;
                if name != last_seen {
                    log::info(&format!("DTS: 授权状态『{name}』"));
                    last_seen = name.clone();
                }
                if name.contains(&cfg.dts_licensed_text) {
                    armed = true;
                    break;
                }
            }
            Ok(None) => {
                let since = *missing_since.get_or_insert_with(Instant::now);
                if since.elapsed() > Duration::from_secs(15) {
                    log::warn("DTS: 找不到授权卡（界面结构可能变了），跳过授权检查");
                    break;
                }
            }
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

    // 3) 关键一步：确认 Windows 侧的空间音效真的切到了 DTS Headphone:X
    //    （授权卡显示"已授权"只代表授权在手，不代表端点选中的是 DTS ——
    //     系统会把该选择重置回"关闭/Sonic"，这正是"有时候必须手动点一下"的原因）
    let mut spatial_ok = true;
    if cfg.dts_fix_spatial {
        if let Some(ep) = endpoint_id {
            let spatial_deadline = Instant::now() + Duration::from_millis(cfg.dts_spatial_timeout_ms);
            match ensure_spatial_enabled(cfg, &uia, frame, ep, spatial_deadline) {
                Ok(ok) => spatial_ok = ok,
                Err(e) => {
                    log::warn(&format!("DTS: 检查/修复端点空间音效失败: {e}"));
                    spatial_ok = false;
                }
            }
        } else {
            log::debug("DTS: 未提供耳机端点（例如手动执行），跳过空间音效校验");
        }
    }

    // 4) 收尾：只有确认"端点空间音效 = DTS"（这是唯一客观判据）才关闭应用
    let verified = if cfg.dts_fix_spatial && endpoint_id.is_some() {
        spatial_ok
    } else {
        armed // 没有端点信息时退回旧的判据
    };
    if verified {
        log::info(&format!(
            "DTS: 已就绪（授权={} 空间音效={}），DTS Headphone:X 生效",
            if armed { "OK" } else { "未确认" },
            if spatial_ok { "OK" } else { "未检查" }
        ));
        if cfg.dts_close_after {
            // 刚变成"已授权/已切换"时应用可能还在收尾（通知 APO3 服务、落盘等），
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
            "DTS: 未能确认全部就绪（授权={} 空间音效={}）；保留应用在后台运行（窗口已在屏幕外），必要时可手动查看",
            if armed { "OK" } else { "未完成" },
            if spatial_ok { "OK" } else { "未完成" }
        ));
    }
    Ok(())
}

/// 在独立线程里执行启用流程（不阻塞 HID 事件循环）。
/// 同一时刻只允许一个流程，重复调用直接跳过。
///
/// 返回线程句柄：`run` 模式下可以丢弃（进程长期存活），但**短命进程**
/// （例如 `set-default` 这种命令行一次性调用）必须 join，否则会随主进程一起退出。
pub fn spawn_arm(cfg: Config, endpoint_id: Option<String>) -> Option<std::thread::JoinHandle<()>> {
    if log::dry_run() {
        log::info("跳过 DTS Sound Unbound 的启动/启用流程");
        return None;
    }
    if ARMING.swap(true, Ordering::SeqCst) {
        log::info("DTS: 上一次启用流程仍在进行，本次跳过");
        return None;
    }
    Some(std::thread::spawn(move || {
        if let Err(e) = arm(&cfg, endpoint_id.as_deref()) {
            log::error(&format!("DTS: 启用流程失败: {e}"));
        }
        ARMING.store(false, Ordering::SeqCst);
    }))
}
