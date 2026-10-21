// Всё, что идёт в Windows напрямую. Остальная программа зовёт только эти функции.

pub mod audio;
pub mod display;
pub mod monitor;

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, VK_A, VK_OEM_3, VK_SPACE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetMessageW, GetWindowThreadProcessId, MSG, SW_SHOWNORMAL, WM_HOTKEY,
};
use windows::core::PCWSTR;

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

// ─── Горячая клавиша ────────────────────────────────────────────────────────

/// Нажата ли горячая клавиша с прошлого вопроса.
static HOTKEY: AtomicBool = AtomicBool::new(false);

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
            _ => HOT_KEY_MODIFIERS(0),
        };
    }
    m
}

/// Горячая клавиша в отдельном потоке со своей очередью сообщений. `wake` будит окно.
/// Возвращает номер занятого сочетания в [`HOTKEYS`]; `None` — все заняты другими программами.
pub fn start_hotkey(wake: impl Fn() + Send + 'static) -> Option<usize> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let got = HOTKEYS.iter().position(|(keys, vk)| RegisterHotKey(None, 1, modifiers(keys), *vk).is_ok());
        let _ = tx.send(got);
        if got.is_none() {
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_HOTKEY {
                HOTKEY.store(true, Ordering::Relaxed);
                wake();
            }
        }
    });
    rx.recv().unwrap_or(None)
}

pub fn take_hotkey() -> bool {
    HOTKEY.swap(false, Ordering::Relaxed)
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
