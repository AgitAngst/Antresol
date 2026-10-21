// Звук через Core Audio: устройства вывода, громкость и «без звука» устройства по умолчанию,
// смена устройства по умолчанию.
//
// Сменить устройство по умолчанию открытым API нельзя: берём IPolicyConfig — недокументированный
// COM-интерфейс, которым это делает сама панель звука Windows (Windows 7–11, тот же IID).

use std::ffi::c_void;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCommunications, eConsole, eMultimedia,
    eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};
use windows::core::{GUID, PCWSTR, PWSTR};

mod policy {
    // Имена методов — как в Windows: так их проще сверять с описаниями интерфейса.
    #![allow(non_snake_case)]

    use std::ffi::c_void;

    use windows::Win32::Media::Audio::ERole;
    use windows::core::{HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, interface};

    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: IUnknown {
        // Первые десять методов не нужны, но должны стоять на своих местах в таблице.
        fn GetMixFormat(&self, id: PCWSTR, format: *mut *mut c_void) -> HRESULT;
        fn GetDeviceFormat(&self, id: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
        fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
        fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
        fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, period: *mut i64, min: *mut i64) -> HRESULT;
        fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
        fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
        fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
        fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        pub fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
        fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
    }
}

use policy::IPolicyConfig;

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Устройство вывода звука.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

/// Что сейчас со звуком.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    pub devices: Vec<Device>,
    pub default_id: Option<String>,
    /// 0..1
    pub volume: f32,
    pub muted: bool,
}

/// Подготовить COM в этом потоке. Окно уже могло сделать это само (OLE для перетаскивания) —
/// тогда ответ «уже готово» тоже годится.
pub fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn take_pwstr(p: PWSTR) -> String {
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
    s
}

fn device_id(d: &IMMDevice) -> windows::core::Result<String> {
    Ok(take_pwstr(unsafe { d.GetId()? }))
}

fn device_name(d: &IMMDevice) -> windows::core::Result<String> {
    unsafe {
        let store = d.OpenPropertyStore(STGM_READ)?;
        let value = store.GetValue(&PKEY_Device_FriendlyName)?;
        Ok(take_pwstr(PropVariantToStringAlloc(&value)?))
    }
}

fn endpoint_volume() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None) }
}

/// Прочитать всё разом.
pub fn read() -> windows::core::Result<State> {
    let e = enumerator()?;
    let mut state = State::default();
    unsafe {
        let list = e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for i in 0..list.GetCount()? {
            let d = list.Item(i)?;
            state.devices.push(Device { id: device_id(&d)?, name: device_name(&d).unwrap_or_default() });
        }
        if let Ok(d) = e.GetDefaultAudioEndpoint(eRender, eConsole) {
            state.default_id = device_id(&d).ok();
            let v: IAudioEndpointVolume = d.Activate(CLSCTX_ALL, None)?;
            state.volume = v.GetMasterVolumeLevelScalar()?;
            state.muted = v.GetMute()?.as_bool();
        }
    }
    Ok(state)
}

/// Громкость устройства по умолчанию, 0..1.
pub fn set_volume(level: f32) -> windows::core::Result<()> {
    unsafe { endpoint_volume()?.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null()) }
}

pub fn set_muted(muted: bool) -> windows::core::Result<()> {
    unsafe { endpoint_volume()?.SetMute(muted, std::ptr::null()) }
}

/// Сделать устройство устройством по умолчанию для всех трёх ролей, как панель звука Windows.
pub fn set_default(id: &str) -> windows::core::Result<()> {
    let wide = super::wide(id);
    unsafe {
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
        for role in [eConsole, eMultimedia, eCommunications] {
            policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok()?;
        }
    }
    Ok(())
}
