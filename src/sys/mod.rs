// Всё, что идёт в Windows напрямую. Остальная программа зовёт только эти функции.

pub mod bt;
pub mod clip;
pub mod ddc;
pub mod display;
pub mod gh;
pub mod kamerton;
pub mod media;
pub mod monitor;
pub mod net;
pub mod nightlight;
pub mod ocr;
pub mod res;
pub mod system;
pub mod tz;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
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
    FindWindowExW, GWL_EXSTYLE, GetMessageW, GetWindowLongPtrW, GetWindowThreadProcessId, MSG, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, SW_SHOWNORMAL, SetWindowLongPtrW, WM_APP, WM_HOTKEY, WS_EX_NOACTIVATE,
};
use windows::core::PCWSTR;

/// Демо-режим для снимков в открытый репозиторий (`ANTRESOL_DEMO=1`): мир выдуманный — монитор (его режимы и
/// масштаб), яркость, клавиши, автозапуск, плееры и сети, — а в Windows не пишется ничего ([`blocked`]). Ничего
/// не выдаёт про этот ПК: ни экран, ни занятые сочетания клавиш.
pub fn demo() -> bool {
    // Переменную читаем один раз: демо за время работы не включается и не выключается.
    static DEMO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DEMO.get_or_init(|| std::env::var_os("ANTRESOL_DEMO").is_some())
}

/// Работаем в выдуманном мире, а не в настоящей Windows: демо-режим и тесты. Настоящее только читается.
pub fn isolated() -> bool {
    cfg!(test) || demo()
}

/// Что заслон не пропустил в тестах: по строке на каждую остановленную запись (для проверки заслона).
#[cfg(test)]
pub static BLOCKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Заслон писателя: первой строкой каждой функции, что меняет Windows (режим экрана, яркость монитора, реестр,
/// клавиши, запуск программ, буфер обмена). `true` — мир выдуманный: писатель сразу отвечает «готово», а в Windows
/// ничего не уходит. `what` — что было бы сделано (в тестах остаётся в [`BLOCKED`]).
pub fn blocked(what: &str) -> bool {
    if !isolated() {
        return false;
    }
    #[cfg(test)]
    BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).push(what.to_string());
    let _ = what;
    true
}

/// Прямо перед самой записью в Windows. В тестах сюда попасть нельзя: значит, заслон [`blocked`] где-то пропущен, и тест
/// падает до записи, а не после неё. Тест `every_windows_write_is_behind_the_guard` следит, что она стоит у каждой записи.
pub fn real_write(what: &str) {
    #[cfg(test)]
    panic!("тест дошёл до настоящей записи в Windows: {what}");
    #[cfg(not(test))]
    let _ = what;
}

/// Строка для Win32: UTF-16 с нулём в конце.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Открыть как двойным щелчком: программу, папку, файл или адрес. `args` — только для программ.
pub fn launch(target: &str, args: &str) -> Result<(), String> {
    if blocked(&format!("launch {target} {args}")) {
        return Ok(());
    }
    real_write("ShellExecuteW");
    let (t, a) = (wide(target), wide(args));
    let args = if args.is_empty() { PCWSTR::null() } else { PCWSTR(a.as_ptr()) };
    let h = unsafe { ShellExecuteW(None, PCWSTR::null(), PCWSTR(t.as_ptr()), args, PCWSTR::null(), SW_SHOWNORMAL) };
    // По договору ShellExecute ответ больше 32 — успех.
    if h.0 as isize > 32 { Ok(()) } else { Err(format!("ShellExecute failed: {}", h.0 as isize)) }
}

/// Скруглить нижние углы окна панели регионом: всё за ним не рисуется и не ловит мышь.
/// Окно ищем по заголовку среди окон своего процесса. Ложь — окна ещё нет.
pub fn round_bottom_corners(width: i32, height: i32, radius: i32) -> bool {
    let Some(hwnd) = own_window() else { return false };
    unsafe {
        // Верхние углы прямоугольника — выше окна, поэтому скруглены только нижние.
        let rgn = CreateRoundRectRgn(0, -radius, width + 1, height + 1, radius * 2, radius * 2);
        // Регионом теперь владеет система — удалять его не надо.
        SetWindowRgn(hwnd, Some(rgn), true) != 0
    }
}

/// Окно панели этого процесса (по заголовку «Antresol»).
fn own_window() -> Option<HWND> {
    let title = wide("Antresol");
    let mut after = None;
    unsafe {
        while let Ok(hwnd) = FindWindowExW(None, after, PCWSTR::null(), PCWSTR(title.as_ptr())) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == std::process::id() {
                return Some(hwnd);
            }
            after = Some(hwnd);
        }
    }
    None
}

/// Не забирать фокус при щелчке (`WS_EX_NOACTIVATE`): так у спрятанной панели — щелчок по полоске с
/// медиа или плашке не уводит клавиатуру из программы пользователя. Ответ — удалось ли.
pub fn set_no_activate(on: bool) -> bool {
    let Some(hwnd) = own_window() else { return false };
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let flag = WS_EX_NOACTIVATE.0 as isize;
        let want = if on { style | flag } else { style & !flag };
        if want != style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want);
        }
    }
    true
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

/// Что с клавишей показа панели.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKey {
    /// Своей нет — взяли первое свободное из [`HOTKEYS`] (`None` — все заняты).
    Auto(Option<usize>),
    /// Своя из настроек: удалось ли занять.
    Custom(bool),
}

struct Keys {
    /// Клавиша показа панели снята на время (ждём новое сочетание в настройках).
    suspended: bool,
    /// Своя клавиша показа панели; `None` — первая свободная из [`HOTKEYS`].
    panel: Option<(HOT_KEY_MODIFIERS, u32)>,
    panel_status: PanelKey,
    /// Что зарегистрировать: (модификаторы, код клавиши) на каждый профиль; `None` — без клавиши.
    wanted: Vec<Option<(HOT_KEY_MODIFIERS, u32)>>,
    /// Что вышло: `Some(false)` — сочетание заняла другая программа.
    status: Vec<Option<bool>>,
    /// Номера профилей, чьи клавиши нажали.
    pressed: Vec<usize>,
}

static KEYS: Mutex<Keys> = Mutex::new(Keys {
    suspended: false,
    panel: None,
    panel_status: PanelKey::Auto(None),
    wanted: Vec::new(),
    status: Vec::new(),
    pressed: Vec::new(),
});

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

fn sync_panel() {
    let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let _ = UnregisterHotKey(None, 1);
        if keys.suspended {
            return;
        }
        real_write("RegisterHotKey");
        keys.panel_status = match keys.panel {
            Some((m, vk)) => PanelKey::Custom(RegisterHotKey(None, 1, m, vk).is_ok()),
            None => {
                PanelKey::Auto(HOTKEYS.iter().position(|(k, vk)| RegisterHotKey(None, 1, modifiers(k), *vk).is_ok()))
            }
        };
    }
}

fn sync_profiles(registered: &mut usize) {
    let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        for i in 0..*registered {
            let _ = UnregisterHotKey(None, PROFILE_ID + i as i32);
        }
        real_write("RegisterHotKey");
        keys.status = keys
            .wanted
            .iter()
            .enumerate()
            .map(|(i, w)| w.map(|(m, vk)| RegisterHotKey(None, PROFILE_ID + i as i32, m, vk).is_ok()))
            .collect();
    }
    *registered = keys.wanted.len();
}

/// Выдуманный мир: клавиши не занимаются, и что занято на этом ПК — неизвестно; у панели «работает» первое из
/// готовых сочетаний (или своё), у профилей с клавишей — «работает».
fn fake_panel_status(keys: &Keys) -> PanelKey {
    if keys.panel.is_some() { PanelKey::Custom(true) } else { PanelKey::Auto(Some(0)) }
}

/// Разобрать сочетание для RegisterHotKey: модификаторы по именам и клавиша.
fn combo(keys: &Option<(Vec<&str>, &str)>) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    keys.as_ref().and_then(|(mods, key)| Some((modifiers(mods), vk_of(key)?)))
}

fn wake_thread() {
    let thread = THREAD.load(Ordering::Acquire);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

/// Поток горячих клавиш. `panel` — своя клавиша показа панели (`None` — первая свободная из
/// [`HOTKEYS`]). `wake` будит окно. Возвращает, что вышло с клавишей показа панели.
pub fn start_hotkey(panel: Option<(Vec<&str>, &str)>, wake: impl Fn() + Send + 'static) -> PanelKey {
    {
        let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
        keys.panel = combo(&panel);
        if blocked("start_hotkey") {
            // Потока клавиш нет вовсе: ни одно сочетание не занято и не перехвачено.
            keys.panel_status = fake_panel_status(&keys);
            return keys.panel_status;
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let mut msg = MSG::default();
        // Очередь потока появляется с первым обращением к ней — до этого PostThreadMessage не дойдёт.
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        THREAD.store(GetCurrentThreadId(), Ordering::Release);
        sync_panel();
        let _ = tx.send(panel_hotkey());
        let mut registered = 0;
        sync_profiles(&mut registered);
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_SYNC {
                sync_panel();
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
    rx.recv().unwrap_or(PanelKey::Auto(None))
}

/// Снять клавишу показа панели на время или вернуть. Шлёт потоку, только если поменялось.
pub fn suspend_panel_hotkey(on: bool) {
    let changed = {
        let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
        let changed = keys.suspended != on;
        keys.suspended = on;
        changed
    };
    if changed {
        wake_thread();
    }
}

/// Что сейчас с клавишей показа панели.
pub fn panel_hotkey() -> PanelKey {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).panel_status
}

/// Сменить клавишу показа панели на ходу; `None` — снова первая свободная из [`HOTKEYS`].
pub fn set_panel_hotkey(panel: Option<(Vec<&str>, &str)>) {
    {
        let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
        keys.panel = combo(&panel);
        if blocked("set_panel_hotkey") {
            if !keys.suspended {
                keys.panel_status = fake_panel_status(&keys);
            }
            return;
        }
    }
    wake_thread();
}

pub fn take_hotkey() -> bool {
    HOTKEY.swap(false, Ordering::Relaxed)
}

/// Поставить клавиши профилей: по элементу на профиль, `None` — без клавиши. Сочетание из
/// клавиш, которых нет в [`vk_of`], считается без клавиши.
pub fn set_profile_hotkeys(list: &[Option<(Vec<&str>, &str)>]) {
    let wanted: Vec<Option<(HOT_KEY_MODIFIERS, u32)>> = list.iter().map(combo).collect();
    {
        let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
        if blocked("set_profile_hotkeys") {
            // Выдуманный мир: у каждого профиля с клавишей она «работает».
            keys.status = wanted.iter().map(|w| w.map(|_| true)).collect();
            keys.wanted = wanted;
            return;
        }
        keys.wanted = wanted;
    }
    wake_thread();
}

/// Что вышло с клавишей профиля: `Some(false)` — занята другой программой; `None` — нет клавиши
/// или ещё не регистрировали.
pub fn profile_hotkey_status(index: usize) -> Option<bool> {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).status.get(index).copied().flatten()
}

/// Тесты, что трогают общие клавиши (`KEYS`), идут по одному: иначе их состояния перемешались бы.
#[cfg(test)]
pub static KEYS_TEST: Mutex<()> = Mutex::new(());

/// Как будто нажали клавишу профиля `i` (для тестов: потока клавиш в них нет).
#[cfg(test)]
pub fn press_profile(i: usize) {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).pressed.push(i);
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

/// Автозапуск в выдуманном мире (демо и тесты): реестр не читается и не пишется.
static FAKE_AUTOSTART: AtomicBool = AtomicBool::new(false);

/// Запускается ли программа вместе с Windows (строка в HKCU\…\Run).
pub fn autostart() -> bool {
    if isolated() {
        return FAKE_AUTOSTART.load(Ordering::Relaxed);
    }
    let Some(key) = run_key(KEY_READ) else { return false };
    let name = wide(RUN_NAME);
    let found = unsafe { RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, None, None) }.is_ok();
    unsafe {
        let _ = RegCloseKey(key);
    }
    found
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    if blocked(&format!("set_autostart {on}")) {
        FAKE_AUTOSTART.store(on, Ordering::Relaxed);
        return Ok(());
    }
    let key = run_key(KEY_SET_VALUE).ok_or("cannot open Run key")?;
    let name = wide(RUN_NAME);
    real_write("RegSetValueExW / RegDeleteValueW");
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

/// Цвет акцента Windows (HKCU\…\DWM, `AccentColor` — 0xAABBGGRR) как (r, g, b).
pub fn windows_accent() -> Option<(u8, u8, u8)> {
    let mut key = HKEY::default();
    let path = wide(r"Software\Microsoft\Windows\DWM");
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, KEY_READ, &mut key) }.ok().ok()?;
    let name = wide("AccentColor");
    let mut value = 0u32;
    let mut size = 4u32;
    let r = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            None,
            Some(&mut value as *mut u32 as *mut u8),
            Some(&mut size),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    r.ok().ok()?;
    Some((value as u8, (value >> 8) as u8, (value >> 16) as u8))
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

    /// Была ли остановлена запись с такой меткой (журнал общий для всех тестов — метки у каждого свои).
    fn was_blocked(marker: &str) -> bool {
        BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|w| w.contains(marker))
    }

    /// Каждый писатель в Windows в тестах упирается в заслон: отвечает «готово» и оставляет метку в журнале. Пропусти он
    /// запись — `real_write` уронил бы тест раньше, чем что-то поменялось бы на этом ПК.
    #[test]
    fn every_writer_stops_at_the_guard() {
        let dev = r"\\.\GUARD-TEST";
        let mode = display::Mode { width: 1920, height: 1080, hz: 144, bits: 32 };
        assert!(isolated(), "в тестах мир выдуманный");
        assert_eq!(launch("guard-test.exe", "--flag"), Ok(()));
        assert!(was_blocked("launch guard-test.exe --flag"));
        assert_eq!(set_autostart(true), Ok(()));
        assert!(autostart(), "автозапуск выдуманный и помнит ответ");
        assert_eq!(set_autostart(false), Ok(()));
        assert!(!autostart());
        assert!(was_blocked("set_autostart true"));
        assert_eq!(display::apply(dev, mode), Ok(()));
        assert_eq!(display::commit(dev, mode), Ok(()));
        assert_eq!(display::set_scale(dev, 125), Ok(()));
        assert_eq!(display::set_hdr(dev, true), Ok(()));
        assert_eq!(display::set_primary(dev, &[(dev.to_string(), 0, 0)]), Ok(()));
        for what in
            ["display::apply", "display::commit", "display::set_scale", "display::set_hdr", "display::set_primary"]
        {
            assert!(was_blocked(&format!("{what} {dev}")), "{what}");
        }
        assert_eq!(ddc::set(4242, 0, ddc::BRIGHTNESS, 42), Ok(()));
        assert!(was_blocked("ddc::set 4242"));
        assert!(system::keep_awake(true));
        assert_eq!(system::set_dark_theme(true), Ok(()));
        assert_eq!(system::set_notifications_off(true), Ok(()));
        assert_eq!(system::set_plan(&windows::core::GUID::from_u128(0x6a8d)), Ok(()));
        assert_eq!(net::set_wifi(true), Ok(()));
        assert_eq!(net::set_vpn("guard-test-vpn", true), Ok(()));
        assert_eq!(nightlight::set(true), Ok(()));
        assert_eq!(clip::write_text("guard-test"), Ok(()));
        for what in [
            "keep_awake true",
            "set_dark_theme true",
            "set_notifications_off true",
            "set_plan",
            "set_wifi true",
            "set_vpn guard-test-vpn true",
            "nightlight::set true",
            "clip::write_text guard-test",
        ] {
            assert!(was_blocked(what), "{what}");
        }
    }

    /// В выдуманном мире ни одно сочетание не занимается (потока клавиш нет), а какие заняты на этом ПК —
    /// не узнать: у панели «работает», у профилей с клавишей — «работает».
    #[test]
    fn hotkeys_are_not_taken_in_the_fake_world() {
        let _one_at_a_time = KEYS_TEST.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(start_hotkey(None, || {}), PanelKey::Auto(Some(0)));
        assert_eq!(THREAD.load(Ordering::Acquire), 0, "потока клавиш нет");
        set_panel_hotkey(Some((vec!["Ctrl", "Alt"], "K")));
        assert_eq!(panel_hotkey(), PanelKey::Custom(true));
        set_panel_hotkey(None);
        assert_eq!(panel_hotkey(), PanelKey::Auto(Some(0)));
        set_profile_hotkeys(&[Some((vec!["Ctrl", "Alt"], "1")), None, Some((vec!["Ctrl"], "Enter"))]);
        assert_eq!(profile_hotkey_status(0), Some(true));
        assert_eq!(profile_hotkey_status(1), None, "без клавиши");
        assert_eq!(profile_hotkey_status(2), None, "Enter нет среди поддержанных — без клавиши");
        assert!(was_blocked("start_hotkey") && was_blocked("set_profile_hotkeys"));
    }

    /// Каждая запись в Windows стоит сразу после `real_write(…)` — без него новый писатель в тестах ушёл бы в настоящую
    /// Windows. Читает исходники: список — те вызовы, что меняют систему (не читают).
    #[test]
    fn every_windows_write_is_behind_the_guard() {
        // Имена без скобки в списке — чтобы сам тест не находил себя.
        let names = [
            "ChangeDisplaySettingsExW",
            "DisplayConfigSetDeviceInfo",
            "SetVCPFeature",
            "RegSetValueExW",
            "RegDeleteValueW",
            "ShellExecuteW",
            "RegisterHotKey",
            "EmptyClipboard",
            "PowerSetActiveScheme",
            "SetThreadExecutionState",
            "SendMessageTimeoutW",
            "SetStateAsync",
            "Command::new",
            "fs::write",
        ];
        let files = [
            ("sys/mod.rs", include_str!("mod.rs")),
            ("sys/display.rs", include_str!("display.rs")),
            ("sys/ddc.rs", include_str!("ddc.rs")),
            ("sys/system.rs", include_str!("system.rs")),
            ("sys/net.rs", include_str!("net.rs")),
            ("sys/nightlight.rs", include_str!("nightlight.rs")),
            ("sys/clip.rs", include_str!("clip.rs")),
            ("config.rs", include_str!("../config.rs")),
        ];
        let mut seen = 0;
        for (file, text) in files {
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for name in names {
                    if line.contains(&format!("{name}(")) {
                        seen += 1;
                        let window = &lines[i.saturating_sub(12)..=i];
                        assert!(
                            window.iter().any(|l| !l.trim_start().starts_with("//") && l.contains("real_write(")),
                            "{file}:{}: вызов {name} без real_write(…) выше — поставь заслон blocked(…) и real_write(…)",
                            i + 1
                        );
                    }
                }
            }
        }
        assert!(seen >= 18, "тест нашёл всего {seen} записей: список имён или файлов устарел");
    }
}
