// Всё, что идёт в Windows напрямую. Остальная программа зовёт только эти функции.

pub mod display;
pub mod monitor;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey, VK_A,
    VK_F1, VK_OEM_3, VK_SPACE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetMessageW, GetWindowThreadProcessId, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SW_SHOWNORMAL, WM_APP, WM_HOTKEY,
};
use windows::core::PCWSTR;

/// Демо-режим для снимков в открытый репозиторий (`ANTRESOL_DEMO=1`): вместо настоящих имён
/// (монитор) — выдуманные. Остальное работает как обычно.
pub fn demo() -> bool {
    std::env::var_os("ANTRESOL_DEMO").is_some()
}

/// Строка для Win32: UTF-16 с нулём в конце.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Открыть как двойным щелчком: программу, папку, файл или адрес. `args` — только для программ.
pub fn launch(target: &str, args: &str) -> Result<(), String> {
    let (t, a) = (wide(target), wide(args));
    let args = if args.is_empty() { PCWSTR::null() } else { PCWSTR(a.as_ptr()) };
    let h = unsafe { ShellExecuteW(None, PCWSTR::null(), PCWSTR(t.as_ptr()), args, PCWSTR::null(), SW_SHOWNORMAL) };
    // По договору ShellExecute ответ больше 32 — успех.
    if h.0 as isize > 32 { Ok(()) } else { Err(format!("ShellExecute failed: {}", h.0 as isize)) }
}

/// Скруглить нижние углы окна панели регионом: всё за ним не рисуется и не ловит мышь.
/// Окно ищем по заголовку среди окон своего процесса. Ложь — окна ещё нет.
pub fn round_bottom_corners(width: i32, height: i32, radius: i32) -> bool {
    let title = wide("Antresol");
    let mut after = None;
    unsafe {
        while let Ok(hwnd) = FindWindowExW(None, after, PCWSTR::null(), PCWSTR(title.as_ptr())) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == std::process::id() {
                // Верхние углы прямоугольника — выше окна, поэтому скруглены только нижние.
                let rgn = CreateRoundRectRgn(0, -radius, width + 1, height + 1, radius * 2, radius * 2);
                // Регионом теперь владеет система — удалять его не надо.
                return SetWindowRgn(hwnd, Some(rgn), true) != 0;
            }
            after = Some(hwnd);
        }
    }
    false
}

// ─── Горячие клавиши ────────────────────────────────────────────────────────
//
// Все клавиши живут в одном потоке со своей очередью сообщений: RegisterHotKey без окна
// привязывает клавишу к потоку, который её зарегистрировал, — и снимать её можно только там же.
// Набор клавиш профилей окно меняет на ходу: кладёт новый в `KEYS` и будит поток сообщением.

/// Нажата ли клавиша показа панели с прошлого вопроса.
static HOTKEY: AtomicBool = AtomicBool::new(false);
static THREAD: AtomicU32 = AtomicU32::new(0);
/// Сообщение потоку клавиш: «перечитай набор клавиш профилей».
const WM_SYNC: u32 = WM_APP + 1;
/// Номера клавиш профилей у Windows: 100 + номер профиля.
const PROFILE_ID: i32 = 100;

#[derive(Default)]
struct Keys {
    /// Что зарегистрировать: (модификаторы, код клавиши) на каждый профиль; `None` — без клавиши.
    wanted: Vec<Option<(HOT_KEY_MODIFIERS, u32)>>,
    /// Что вышло: `Some(false)` — сочетание заняла другая программа.
    status: Vec<Option<bool>>,
    /// Номера профилей, чьи клавиши нажали.
    pressed: Vec<usize>,
}

static KEYS: Mutex<Keys> = Mutex::new(Keys { wanted: Vec::new(), status: Vec::new(), pressed: Vec::new() });

/// Сочетания для показа панели по порядку: первое свободное и берём.
pub const HOTKEYS: [(&[&str], u32); 4] = [
    (&["Ctrl", "Alt", "Space"], VK_SPACE.0 as u32),
    (&["Ctrl", "Alt", "A"], VK_A.0 as u32),
    (&["Ctrl", "Shift", "Space"], VK_SPACE.0 as u32),
    (&["Ctrl", "Alt", "`"], VK_OEM_3.0 as u32),
];

fn modifiers(keys: &[&str]) -> HOT_KEY_MODIFIERS {
    let mut m = MOD_NOREPEAT;
    for k in keys {
        m |= match *k {
            "Ctrl" => MOD_CONTROL,
            "Alt" => MOD_ALT,
            "Shift" => MOD_SHIFT,
            "Win" => MOD_WIN,
            _ => HOT_KEY_MODIFIERS(0),
        };
    }
    m
}

/// Код клавиши Windows по имени: буквы, цифры, F1–F24, Space; остальное не поддержано.
pub fn vk_of(key: &str) -> Option<u32> {
    let up = key.to_ascii_uppercase();
    let mut chars = up.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_uppercase() || c.is_ascii_digit() => return Some(c as u32),
        _ => {}
    }
    if up == "SPACE" {
        return Some(VK_SPACE.0 as u32);
    }
    let n: u32 = up.strip_prefix('F')?.parse().ok()?;
    (1..=24).contains(&n).then(|| VK_F1.0 as u32 + n - 1)
}

fn sync_profiles(registered: &mut usize) {
    let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        for i in 0..*registered {
            let _ = UnregisterHotKey(None, PROFILE_ID + i as i32);
        }
        keys.status = keys
            .wanted
            .iter()
            .enumerate()
            .map(|(i, w)| w.map(|(m, vk)| RegisterHotKey(None, PROFILE_ID + i as i32, m, vk).is_ok()))
            .collect();
    }
    *registered = keys.wanted.len();
}

/// Поток горячих клавиш. `wake` будит окно. Возвращает номер занятого сочетания показа панели
/// в [`HOTKEYS`]; `None` — все заняты другими программами (клавиши профилей работают и тогда).
pub fn start_hotkey(wake: impl Fn() + Send + 'static) -> Option<usize> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let mut msg = MSG::default();
        // Очередь потока появляется с первым обращением к ней — до этого PostThreadMessage не дойдёт.
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        THREAD.store(GetCurrentThreadId(), Ordering::Release);
        let got = HOTKEYS.iter().position(|(keys, vk)| RegisterHotKey(None, 1, modifiers(keys), *vk).is_ok());
        let _ = tx.send(got);
        let mut registered = 0;
        sync_profiles(&mut registered);
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_SYNC {
                sync_profiles(&mut registered);
                wake();
            } else if msg.message == WM_HOTKEY {
                let id = msg.wParam.0 as i32;
                if id == 1 {
                    HOTKEY.store(true, Ordering::Relaxed);
                } else if id >= PROFILE_ID {
                    KEYS.lock().unwrap_or_else(|e| e.into_inner()).pressed.push((id - PROFILE_ID) as usize);
                }
                wake();
            }
        }
    });
    rx.recv().unwrap_or(None)
}

pub fn take_hotkey() -> bool {
    HOTKEY.swap(false, Ordering::Relaxed)
}

/// Поставить клавиши профилей: по элементу на профиль, `None` — без клавиши. Сочетание из
/// клавиш, которых нет в [`vk_of`], считается без клавиши.
pub fn set_profile_hotkeys(list: &[Option<(Vec<&str>, &str)>]) {
    let wanted = list.iter().map(|k| k.as_ref().and_then(|(mods, key)| Some((modifiers(mods), vk_of(key)?)))).collect();
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).wanted = wanted;
    let thread = THREAD.load(Ordering::Acquire);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

/// Что вышло с клавишей профиля: `Some(false)` — занята другой программой; `None` — нет клавиши
/// или ещё не регистрировали.
pub fn profile_hotkey_status(index: usize) -> Option<bool> {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).status.get(index).copied().flatten()
}

/// Номера профилей, чьи клавиши нажали с прошлого вопроса.
pub fn take_profile_presses() -> Vec<usize> {
    std::mem::take(&mut KEYS.lock().unwrap_or_else(|e| e.into_inner()).pressed)
}

// ─── Автозапуск ─────────────────────────────────────────────────────────────

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "Antresol";

fn run_key(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    let path = wide(RUN_KEY);
    let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, access, &mut key) };
    r.is_ok().then_some(key)
}

/// Запускается ли программа вместе с Windows (строка в HKCU\…\Run).
pub fn autostart() -> bool {
    let Some(key) = run_key(KEY_READ) else { return false };
    let name = wide(RUN_NAME);
    let found = unsafe { RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, None, None) }.is_ok();
    unsafe {
        let _ = RegCloseKey(key);
    }
    found
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    let key = run_key(KEY_SET_VALUE).ok_or("cannot open Run key")?;
    let name = wide(RUN_NAME);
    let r = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = wide(&format!("\"{}\"", exe.display()));
        let bytes = unsafe { std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2) };
        unsafe { RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)) }
    } else {
        unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    r.ok().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_codes() {
        assert_eq!(vk_of("A"), Some(0x41));
        assert_eq!(vk_of("z"), Some(0x5A));
        assert_eq!(vk_of("1"), Some(0x31));
        assert_eq!(vk_of("F1"), Some(0x70));
        assert_eq!(vk_of("F12"), Some(0x7B));
        assert_eq!(vk_of("Space"), Some(0x20));
        assert_eq!(vk_of("F25"), None);
        assert_eq!(vk_of("Enter"), None);
        assert_eq!(vk_of(""), None);
    }
}
