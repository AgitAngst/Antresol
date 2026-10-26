// Система: «не спать», тема Windows, уведомления Windows, план питания.
//
// - «Не спать» — `SetThreadExecutionState` из потока окна (живёт, пока живёт программа); выход из
//   программы снимает запрет сам.
// - Тема — `AppsUseLightTheme` и `SystemUsesLightTheme` в `HKCU\…\Themes\Personalize`, потом рассылка
//   `WM_SETTINGCHANGE` «ImmersiveColorSet» — как делают «Параметры».
// - Уведомления — `ToastEnabled` в `HKCU\…\PushNotifications` (общий выключатель уведомлений «Параметров»).
// - План питания — powrprof: `PowerEnumerate`, `PowerReadFriendlyName`, `PowerGetActiveScheme`,
//   `PowerSetActiveScheme`.
//
// В демо и в тестах (`sys::fake_reads`) тема, уведомления и план питания — выдуманные: мир помнит, что в него записали
// (писатель за заслоном), чтобы сцены проверялись без Windows. Вначале: тема тёмная, уведомления включены, план «Balanced».

use std::sync::Mutex;

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

/// Тема Windows: два значения реестра по отдельности. У Windows 10 по умолчанию программы светлые, а панель задач тёмная
/// («смешанный» режим), поэтому одного признака «тёмная» мало: вернуть тему «как была» можно, только помня оба значения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// `AppsUseLightTheme` = 0: программы тёмные.
    pub apps_dark: bool,
    /// `SystemUsesLightTheme` = 0: панель задач, «Пуск» и системные окна тёмные.
    pub system_dark: bool,
}

impl Theme {
    /// Обе темы тёмные (`true`) или обе светлые.
    pub fn both(dark: bool) -> Theme {
        Theme { apps_dark: dark, system_dark: dark }
    }

    /// Обе темы такие, как `dark`.
    pub fn is(self, dark: bool) -> bool {
        self == Theme::both(dark)
    }
}

/// Выдуманная система: что «стоит» сейчас.
struct World {
    theme: Theme,
    quiet: bool,
    /// План питания — GUID числом.
    plan: u128,
}

const WORLD_START: World = World { theme: Theme { apps_dark: true, system_dark: true }, quiet: false, plan: 1 };

static WORLD: Mutex<World> = Mutex::new(WORLD_START);

fn world<R>(f: impl FnOnce(&mut World) -> R) -> R {
    f(&mut WORLD.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Выдуманная система как вначале (для тестов).
#[cfg(test)]
pub fn fake_reset() {
    world(|w| *w = WORLD_START);
}

/// GUID плана питания строкой, как он лежит в `config.txt`: `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
pub fn guid_string(guid: &GUID) -> String {
    let h = format!("{:032X}", guid.to_u128());
    format!("{{{}-{}-{}-{}-{}}}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// GUID из строки `config.txt` (скобки и регистр не важны); не GUID — `None`.
pub fn guid_from(s: &str) -> Option<GUID> {
    let canon = crate::config::normalize_guid(s)?;
    let hex: String = canon.chars().filter(char::is_ascii_hexdigit).collect();
    u128::from_str_radix(&hex, 16).ok().map(GUID::from_u128)
}

/// Не давать уснуть компьютеру и гаснуть экрану (`true`) или снова как обычно. Ответ — получилось ли
/// (ноль от Windows — ошибка; прежнее состояние с `ES_CONTINUOUS` нулём не бывает).
pub fn keep_awake(on: bool) -> bool {
    if super::blocked(&format!("keep_awake {on}")) {
        return true;
    }
    let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED } else { ES_CONTINUOUS };
    super::real_write("SetThreadExecutionState");
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
    super::real_write("RegSetValueExW");
    let r = unsafe { RegSetValueExW(key, PCWSTR(n.as_ptr()), None, REG_DWORD, Some(&value.to_le_bytes())) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    r.ok().map_err(|e| e.to_string())
}

const THEME: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const PUSH: &str = r"Software\Microsoft\Windows\CurrentVersion\PushNotifications";

/// Тема Windows сейчас: и программ, и панели задач. Значение программ не прочитать — `None`; панели задач нет (старая
/// Windows) — она считается такой же, как программы.
pub fn theme() -> Option<Theme> {
    if super::fake_reads() {
        return Some(world(|w| w.theme));
    }
    let apps_dark = read_dword(THEME, "AppsUseLightTheme")? == 0;
    let system_dark = read_dword(THEME, "SystemUsesLightTheme").map_or(apps_dark, |v| v == 0);
    Some(Theme { apps_dark, system_dark })
}

/// Тёмная ли тема программ Windows (плитка «Тёмная тема»); не узнать — `None`.
pub fn dark_theme() -> Option<bool> {
    theme().map(|t| t.apps_dark)
}

/// Поставить тему Windows: каждое из двух значений — своим, как передано (так сцена возвращает «смешанный» режим).
pub fn set_theme(theme: Theme) -> Result<(), String> {
    if super::blocked(&format!("set_theme {theme:?}")) {
        world(|w| w.theme = theme);
        return Ok(());
    }
    write_dword(THEME, "AppsUseLightTheme", u32::from(!theme.apps_dark))?;
    write_dword(THEME, "SystemUsesLightTheme", u32::from(!theme.system_dark))?;
    // Рассылка всем окнам может ждать по 200 мс на каждое зависшее — не в потоке панели.
    std::thread::spawn(|| {
        let topic = wide("ImmersiveColorSet");
        super::real_write("SendMessageTimeoutW");
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
    });
    Ok(())
}

/// Тёмная или светлая тема Windows — и для программ, и для панели задач.
pub fn set_dark_theme(dark: bool) -> Result<(), String> {
    set_theme(Theme::both(dark))
}

/// Выключены ли уведомления Windows (`ToastEnabled` = 0); нет значения — включены.
pub fn notifications_off() -> Option<bool> {
    if super::fake_reads() {
        return Some(world(|w| w.quiet));
    }
    Some(read_dword(PUSH, "ToastEnabled").is_some_and(|v| v == 0))
}

pub fn set_notifications_off(off: bool) -> Result<(), String> {
    if super::blocked(&format!("set_notifications_off {off}")) {
        world(|w| w.quiet = off);
        return Ok(());
    }
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
    if super::fake_reads() {
        let p = |n: u128, name: &str| Plan { guid: GUID::from_u128(n), name: name.into() };
        let list = vec![p(1, "Balanced"), p(2, "High performance"), p(3, "Power saver")];
        return (list, Some(GUID::from_u128(world(|w| w.plan))));
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
    if super::blocked(&format!("set_plan {guid:?}")) {
        world(|w| w.plan = guid.to_u128());
        return Ok(());
    }
    super::real_write("PowerSetActiveScheme");
    let r = unsafe { PowerSetActiveScheme(None, Some(guid)) };
    if r.0 == 0 { Ok(()) } else { Err(format!("PowerSetActiveScheme failed: {}", r.0)) }
}
