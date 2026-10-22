// Система: «не спать», тема Windows, уведомления Windows, план питания.
//
// - «Не спать» — `SetThreadExecutionState` из потока окна (живёт, пока живёт программа); выход из
//   программы снимает запрет сам.
// - Тема — `AppsUseLightTheme` и `SystemUsesLightTheme` в `HKCU\…\Themes\Personalize`, потом рассылка
//   `WM_SETTINGCHANGE` «ImmersiveColorSet» — как делают «Параметры».
// - Уведомления — `ToastEnabled` в `HKCU\…\PushNotifications` (общий выключатель уведомлений «Параметров»).
// - План питания — powrprof: `PowerEnumerate`, `PowerReadFriendlyName`, `PowerGetActiveScheme`,
//   `PowerSetActiveScheme`.

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Power::{
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, PowerEnumerate, PowerGetActiveScheme,
    PowerReadFriendlyName, PowerSetActiveScheme, SetThreadExecutionState,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use windows::core::{GUID, PCWSTR};

use super::wide;

/// Не давать уснуть компьютеру и гаснуть экрану (`true`) или снова как обычно.
pub fn keep_awake(on: bool) -> bool {
    let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED } else { ES_CONTINUOUS };
    unsafe { SetThreadExecutionState(flags) }.0 != 0
}

fn open(path: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    let p = wide(path);
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(p.as_ptr()), None, access, &mut key) }.ok().ok()?;
    Some(key)
}

fn read_dword(path: &str, name: &str) -> Option<u32> {
    let key = open(path, KEY_QUERY_VALUE)?;
    let n = wide(name);
    let mut value = 0u32;
    let mut size = 4u32;
    let r = unsafe {
        RegQueryValueExW(key, PCWSTR(n.as_ptr()), None, None, Some(&mut value as *mut u32 as *mut u8), Some(&mut size))
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    r.ok().ok().map(|_| value)
}

fn write_dword(path: &str, name: &str, value: u32) -> Result<(), String> {
    let key = open(path, KEY_SET_VALUE).ok_or_else(|| format!("cannot open {path}"))?;
    let n = wide(name);
    let r = unsafe { RegSetValueExW(key, PCWSTR(n.as_ptr()), None, REG_DWORD, Some(&value.to_le_bytes())) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    r.ok().map_err(|e| e.to_string())
}

const THEME: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const PUSH: &str = r"Software\Microsoft\Windows\CurrentVersion\PushNotifications";

/// Тёмная ли тема Windows (программы); не узнать — `None`.
pub fn dark_theme() -> Option<bool> {
    if super::demo() {
        return Some(true);
    }
    read_dword(THEME, "AppsUseLightTheme").map(|v| v == 0)
}

/// Тёмная или светлая тема Windows — и для программ, и для панели задач.
pub fn set_dark_theme(dark: bool) -> Result<(), String> {
    let light = if dark { 0 } else { 1 };
    write_dword(THEME, "AppsUseLightTheme", light)?;
    write_dword(THEME, "SystemUsesLightTheme", light)?;
    let topic = wide("ImmersiveColorSet");
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(topic.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
    Ok(())
}

/// Выключены ли уведомления Windows (`ToastEnabled` = 0); нет значения — включены.
pub fn notifications_off() -> Option<bool> {
    if super::demo() {
        return Some(false);
    }
    Some(read_dword(PUSH, "ToastEnabled").is_some_and(|v| v == 0))
}

pub fn set_notifications_off(off: bool) -> Result<(), String> {
    write_dword(PUSH, "ToastEnabled", if off { 0 } else { 1 })
}

/// План питания: GUID и имя.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub guid: GUID,
    pub name: String,
}

/// Все планы питания и какой стоит.
pub fn plans() -> (Vec<Plan>, Option<GUID>) {
    if super::demo() {
        let p = |n: u128, name: &str| Plan { guid: GUID::from_u128(n), name: name.into() };
        return (vec![p(1, "Balanced"), p(2, "High performance"), p(3, "Power saver")], Some(GUID::from_u128(1)));
    }
    let mut out = Vec::new();
    for i in 0.. {
        let mut guid = GUID::zeroed();
        let mut size = size_of::<GUID>() as u32;
        let r = unsafe {
            PowerEnumerate(
                None,
                None,
                None,
                windows::Win32::System::Power::ACCESS_SCHEME,
                i,
                Some(&mut guid as *mut GUID as *mut u8),
                &mut size,
            )
        };
        if r.0 != 0 {
            break;
        }
        let mut len = 0u32;
        unsafe {
            let _ = PowerReadFriendlyName(None, Some(&guid), None, None, None, &mut len);
        }
        let mut buf = vec![0u8; len as usize];
        let ok = unsafe { PowerReadFriendlyName(None, Some(&guid), None, None, Some(buf.as_mut_ptr()), &mut len) };
        let name = if ok.0 == 0 {
            let w: Vec<u16> =
                buf.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
            String::from_utf16_lossy(&w)
        } else {
            String::new()
        };
        out.push(Plan { guid, name });
    }
    let mut active: *mut GUID = std::ptr::null_mut();
    let current = unsafe {
        if PowerGetActiveScheme(None, &mut active).0 == 0 && !active.is_null() {
            let g = *active;
            let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(active as *mut _)));
            Some(g)
        } else {
            None
        }
    };
    (out, current)
}

pub fn set_plan(guid: &GUID) -> Result<(), String> {
    if super::demo() {
        return Ok(());
    }
    let r = unsafe { PowerSetActiveScheme(None, Some(guid)) };
    if r.0 == 0 { Ok(()) } else { Err(format!("PowerSetActiveScheme failed: {}", r.0)) }
}
