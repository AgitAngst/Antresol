// Сеть: Wi‑Fi (радио Windows.Devices.Radios, имя сети — Windows.Networking.Connectivity) и VPN
// Windows (RAS: подключения «Параметры → Сеть → VPN»; вкл/выкл — `rasdial` с сохранёнными данными). Радио Bluetooth — тоже
// отсюда, но только чтение: карточка «Bluetooth» серит «Подключить», пока оно выключено.
// WinRT отвечает асинхронно — звать из потока модуля (после `media::init_thread`), не из окна.
//
// В демо и в тестах (`sys::fake_reads`) Wi‑Fi выдуманный: включён, сеть «Demo-Net», помнит, что в него записали (писатель
// за заслоном); VPN — одно подключение «Demo Office»; радио Bluetooth включено.

use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Networking::Connectivity::NetworkInformation;
use windows::Win32::NetworkManagement::Rras::{RASCONNW, RASENTRYNAMEW, RasEnumConnectionsW, RasEnumEntriesW};
use windows::core::{Error, HRESULT, PCWSTR, Result};

/// Сколько ждать ответа WinRT.
const TIMEOUT: Duration = Duration::from_secs(3);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

macro_rules! wait {
    ($op:expr) => {{
        let op = $op;
        let start = Instant::now();
        loop {
            // 0 — ещё идёт, 1 — готово, 2 — отменено, 3 — ошибка.
            match op.Status()?.0 {
                0 if start.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(15)),
                0 => {
                    let _ = op.Cancel();
                    break Err(Error::new(HRESULT(0x8000_0005u32 as i32), "radio request timed out"));
                }
                _ => break op.GetResults(),
            }
        }
    }};
}

/// Выдуманный Wi‑Fi: включено ли радио. Читается и пишется из потока модуля — отсюда `AtomicBool`.
static FAKE_WIFI: AtomicBool = AtomicBool::new(true);

/// Выдуманный Wi‑Fi не переключается: радио «не даёт доступ» (для тестов уведомления об ошибке).
#[cfg(test)]
pub static FAKE_WIFI_FAILS: AtomicBool = AtomicBool::new(false);

/// Выдуманное радио Bluetooth: включено ли. Только читается; выключают снимок «Bluetooth выключен» и тесты ([`fake_bluetooth`]).
static FAKE_BLUETOOTH: AtomicBool = AtomicBool::new(true);

/// Включить или выключить выдуманное радио Bluetooth: для снимков (`ANTRESOL_VIEW=bluetooth-off` в демо) и тестов. Настоящее
/// радио отсюда не трогается — Bluetooth в панели только читается.
pub fn fake_bluetooth(on: bool) {
    FAKE_BLUETOOTH.store(on, Ordering::Relaxed);
}

/// Выдуманный Wi‑Fi включён, радио Bluetooth тоже, как вначале (для тестов).
#[cfg(test)]
pub fn fake_reset() {
    FAKE_WIFI.store(true, Ordering::Relaxed);
    FAKE_WIFI_FAILS.store(false, Ordering::Relaxed);
    FAKE_BLUETOOTH.store(true, Ordering::Relaxed);
}

/// Wi‑Fi: включено ли радио и к какой сети подключены (пусто — ни к какой).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wifi {
    pub on: bool,
    pub network: String,
}

fn radio_of(kind: RadioKind) -> Result<Option<Radio>> {
    let radios = wait!(Radio::GetRadiosAsync()?)?;
    for r in radios {
        if r.Kind()? == kind {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

fn wifi_radio() -> Result<Option<Radio>> {
    radio_of(RadioKind::WiFi)
}

/// Включено ли радио Bluetooth: `Some(false)` — выключено или отключено (устройства не подключить); радио нет, Windows не ответила
/// или не знает состояния — `None`: тогда панель не мешает. Только чтение: Bluetooth из панели не включают. Звать из потока с COM
/// (`btaudio::Com`).
pub fn bluetooth_on() -> Option<bool> {
    if super::fake_reads() {
        return Some(FAKE_BLUETOOTH.load(Ordering::Relaxed));
    }
    let radio = radio_of(RadioKind::Bluetooth).ok()??;
    match radio.State().ok()? {
        RadioState::On => Some(true),
        RadioState::Off | RadioState::Disabled => Some(false),
        _ => None,
    }
}

/// Имя сети Wi‑Fi, через которую выход в интернет; не Wi‑Fi — пусто.
fn wifi_network() -> String {
    let name = || -> Result<String> {
        let p = NetworkInformation::GetInternetConnectionProfile()?;
        if !p.IsWlanConnectionProfile()? {
            return Ok(String::new());
        }
        Ok(p.ProfileName()?.to_string())
    };
    name().unwrap_or_default()
}

/// Wi‑Fi сейчас; радио Wi‑Fi нет или Windows не ответила — `None`.
pub fn wifi() -> Option<Wifi> {
    if super::fake_reads() {
        let on = FAKE_WIFI.load(Ordering::Relaxed);
        return Some(Wifi { on, network: if on { "Demo-Net".into() } else { String::new() } });
    }
    let r = wifi_radio().ok()??;
    let on = r.State().ok()? == RadioState::On;
    Some(Wifi { on, network: if on { wifi_network() } else { String::new() } })
}

pub fn set_wifi(on: bool) -> std::result::Result<(), String> {
    if super::blocked(&format!("set_wifi {on}")) {
        #[cfg(test)]
        if FAKE_WIFI_FAILS.load(Ordering::Relaxed) {
            return Err("radio access: 1".into());
        }
        FAKE_WIFI.store(on, Ordering::Relaxed);
        return Ok(());
    }
    super::real_write("Radio::SetStateAsync");
    let run = || -> Result<RadioAccessStatus> {
        let access = wait!(Radio::RequestAccessAsync()?)?;
        if access != RadioAccessStatus::Allowed {
            return Ok(access);
        }
        let Some(r) = wifi_radio()? else { return Ok(RadioAccessStatus::Unspecified) };
        wait!(r.SetStateAsync(if on { RadioState::On } else { RadioState::Off })?)
    };
    match run() {
        Ok(RadioAccessStatus::Allowed) => Ok(()),
        Ok(other) => Err(format!("radio access: {}", other.0)),
        Err(e) => Err(e.message()),
    }
}

/// Подключение VPN Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vpn {
    pub name: String,
    pub connected: bool,
}

fn utf16(s: &[u16]) -> String {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

/// Подключения из телефонной книги пользователя и какие из них сейчас подключены.
pub fn vpns() -> Vec<Vpn> {
    if super::fake_reads() {
        return vec![Vpn { name: "Demo Office".into(), connected: false }];
    }
    // Буфер мал (603) — Windows называет нужный размер: второй заход с ним.
    let mut entries: Vec<RASENTRYNAMEW> = Vec::new();
    let mut count = 0u32;
    let mut cap = 16usize;
    for _ in 0..2 {
        entries = vec![RASENTRYNAMEW { dwSize: size_of::<RASENTRYNAMEW>() as u32, ..Default::default() }; cap];
        let mut cb = (cap * size_of::<RASENTRYNAMEW>()) as u32;
        let r =
            unsafe { RasEnumEntriesW(PCWSTR::null(), PCWSTR::null(), Some(entries.as_mut_ptr()), &mut cb, &mut count) };
        match r {
            0 => break,
            603 => cap = (cb as usize).div_ceil(size_of::<RASENTRYNAMEW>()).max(cap + 1),
            _ => return Vec::new(),
        }
    }
    let mut connected: Vec<String> = Vec::new();
    let mut cap = 8usize;
    for _ in 0..2 {
        let mut conns = vec![RASCONNW { dwSize: size_of::<RASCONNW>() as u32, ..Default::default() }; cap];
        let mut cb = (cap * size_of::<RASCONNW>()) as u32;
        let mut live = 0u32;
        match unsafe { RasEnumConnectionsW(Some(conns.as_mut_ptr()), &mut cb, &mut live) } {
            0 => {
                connected = conns[..(live as usize).min(cap)].iter().map(|c| utf16(&c.szEntryName)).collect();
                break;
            }
            603 => cap = (cb as usize).div_ceil(size_of::<RASCONNW>()).max(cap + 1),
            _ => break,
        }
    }
    let count = (count as usize).min(entries.len()) as u32;
    entries[..count as usize]
        .iter()
        .map(|e| {
            let name = utf16(&e.szEntryName);
            Vpn { connected: connected.contains(&name), name }
        })
        .collect()
}

/// Подключить (сохранёнными данными) или отключить VPN — через `rasdial` без окна.
pub fn set_vpn(name: &str, on: bool) -> std::result::Result<(), String> {
    if super::blocked(&format!("set_vpn {name} {on}")) {
        return Ok(());
    }
    super::real_write("rasdial");
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let mut cmd = std::process::Command::new(format!(r"{windir}\System32\rasdial.exe"));
    cmd.arg(name);
    if !on {
        cmd.arg("/disconnect");
    }
    let mut child = cmd
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Сервер VPN не отвечает — не ждать вечно: поток модуля нужен и Wi‑Fi.
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(45) {
            let _ = child.kill();
            return Err("rasdial: timeout".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if status.success() {
        Ok(())
    } else {
        // У rasdial ответ на языке системы: показываем код возврата.
        Err(format!("rasdial: {}", status.code().unwrap_or(-1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живое чтение (ничего не меняет): Wi‑Fi, VPN, план питания, тема, уведомления.
    /// Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reads_live_state() {
        // Живое чтение: тема, уведомления, план питания, Wi‑Fi и VPN — настоящие, а не выдуманные.
        crate::sys::LIVE_READS.store(true, Ordering::Relaxed);
        crate::sys::media::init_thread();
        let w = wifi();
        let v = vpns();
        let (plans, active) = crate::sys::system::plans();
        eprintln!(
            "wifi radio: {:?}, network known: {}, vpns: {}, plans: {}, active plan: {}, dark: {:?}, quiet: {:?}",
            w.as_ref().map(|w| w.on),
            w.as_ref().is_some_and(|w| !w.network.is_empty()),
            v.len(),
            plans.len(),
            active.is_some(),
            crate::sys::system::dark_theme(),
            crate::sys::system::notifications_off()
        );
        assert!(!plans.is_empty(), "хотя бы один план питания есть всегда");
        assert!(plans.iter().all(|p| !p.name.is_empty()));
    }
}
