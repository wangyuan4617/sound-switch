//! 音频端点管理与默认输出设备切换。
//!
//! - 枚举渲染端点，读取友好名称（PKEY_Device_FriendlyName）；
//! - 按名称关键词定位耳机端点；
//! - 查询某角色当前默认输出设备 id；
//! - 通过未文档化的 `IPolicyConfig::SetDefaultEndpoint` 切换默认输出设备
//!   （Windows 没有公开的“设置默认设备”API，社区通用做法，封装见 com-policy-config crate）。

use anyhow::{anyhow, Context, Result};
use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eCommunications, eConsole, eMultimedia, eRender, IMMDevice, IMMDeviceCollection,
    IMMDeviceEnumerator, ERole, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

/// 初始化当前线程的 COM（只应在首次调用音频 API 前调用一次）
pub fn init_com() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .context("CoInitializeEx 失败")
    }
}

/// 一个渲染端点
#[derive(Debug, Clone)]
pub struct AudioEndpoint {
    /// 端点 ID，例如 {0.0.0.00000000}.{GUID}
    pub id: String,
    /// 友好名称（Windows 音频设置中显示的名称）
    pub name: String,
}

pub fn parse_role(name: &str) -> Option<ERole> {
    match name.to_ascii_lowercase().as_str() {
        "console" => Some(eConsole),
        "multimedia" => Some(eMultimedia),
        "communications" => Some(eCommunications),
        _ => None,
    }
}

pub fn role_name(role: &ERole) -> &'static str {
    match role.0 {
        0 => "console",
        1 => "multimedia",
        _ => "communications",
    }
}

fn create_enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { Ok(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?) }
}

/// 把 GetId 返回的 PWSTR 转成 String，并释放其内存
unsafe fn free_pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.0.is_null() {
        return String::new();
    }
    // edition 2024 要求在 unsafe fn 内对 unsafe 操作显式标注
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

/// 读取端点友好名；读取失败返回空字符串
fn endpoint_name(device: &IMMDevice) -> String {
    unsafe {
        let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
            return String::new();
        };
        let Ok(mut pv) = store.GetValue(&PKEY_Device_FriendlyName) else {
            return String::new();
        };
        // PROPVARIANT.vt == VT_LPWSTR(31) 时取 pwszVal
        let vt = pv.Anonymous.Anonymous.vt;
        let name = if vt == windows::Win32::System::Variant::VARENUM(31) {
            let p = pv.Anonymous.Anonymous.Anonymous.pwszVal;
            if p.0.is_null() {
                String::new()
            } else {
                p.to_string().unwrap_or_default()
            }
        } else {
            String::new()
        };
        let _ = windows::Win32::System::Com::StructuredStorage::PropVariantClear(&mut pv);
        name
    }
}

/// 枚举所有活动渲染端点
pub fn list_render_endpoints() -> Result<Vec<AudioEndpoint>> {
    unsafe {
        let enumerator = create_enumerator()?;
        let collection: IMMDeviceCollection =
            enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let count = collection.GetCount()?;
        let mut out = Vec::new();
        for i in 0..count {
            let Ok(device) = collection.Item(i) else { continue };
            let Ok(id_pwstr) = device.GetId() else { continue };
            let id = free_pwstr_to_string(id_pwstr);
            let name = endpoint_name(&device);
            out.push(AudioEndpoint { id, name });
        }
        Ok(out)
    }
}

/// 从活动端点中按名称关键词（不区分大小写）找目标端点
pub fn find_endpoint_by_keyword(keyword: &str) -> Result<Option<AudioEndpoint>> {
    let kw = keyword.to_lowercase();
    for ep in list_render_endpoints()? {
        if ep.name.to_lowercase().contains(&kw) {
            return Ok(Some(ep));
        }
    }
    Ok(None)
}

/// 查询某角色当前默认渲染端点 id
pub fn current_default_endpoint_id(role: ERole) -> Result<Option<String>> {
    unsafe {
        let enumerator = create_enumerator()?;
        match enumerator.GetDefaultAudioEndpoint(eRender, role) {
            Ok(device) => {
                let id = device.GetId()?;
                Ok(Some(free_pwstr_to_string(id)))
            }
            Err(_) => Ok(None),
        }
    }
}

/// 把某角色默认输出设备切到指定端点 id
pub fn set_default_endpoint(endpoint_id: &str, role: ERole) -> Result<()> {
    if crate::log::dry_run() {
        crate::log::info(&format!(
            "[dry-run] 角色 {} 默认输出 -> {}",
            role_name(&role),
            endpoint_id
        ));
        return Ok(());
    }
    unsafe {
        let policy: com_policy_config::IPolicyConfig =
            CoCreateInstance(&com_policy_config::PolicyConfigClient, None, CLSCTX_ALL)
                .context("CoCreateInstance(PolicyConfigClient) 失败")?;
        let wide: Vec<u16> = endpoint_id.encode_utf16().chain(Some(0)).collect();
        policy
            .SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role)
            .map_err(|e| anyhow!("SetDefaultEndpoint 失败: {e}"))?;
    }
    crate::log::info(&format!(
        "已把角色 {} 的默认输出切换为 {}",
        role_name(&role),
        endpoint_id
    ));
    Ok(())
}
