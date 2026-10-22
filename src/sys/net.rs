// Сеть: Wi‑Fi (радио Windows.Devices.Radios, имя сети — Windows.Networking.Connectivity) и VPN
// Windows (RAS: подключения «Параметры → Сеть → VPN»; вкл/выкл — `rasdial` с сохранёнными данными).
// WinRT отвечает асинхронно — звать из потока модуля (после `media::init_thread`), не из окна.

use std::os::windows::process::CommandExt;
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

/// Wi‑Fi: включено ли радио и к какой сети подключены (пусто — ни к какой).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wifi {
    pub on: bool,
    pub network: String,
}

fn wifi_radio() -> Result<Option<Radio>> {
    let radios = wait!(Radio::GetRadiosAsync()?)?;
    for r in radios {
        if r.Kind()? == RadioKind::WiFi {
            return Ok(Some(r));
        }
    }
    Ok(None)
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
    if super::demo() {
        return Some(Wifi { on: true, network: "Demo-Net".into() });
    }
    let r = wifi_radio().ok()??;
    let on = r.State().ok()? == RadioState::On;
    Some(Wifi { on, network: if on { wifi_network() } else { String::new() } })
}

pub fn set_wifi(on: bool) -> std::result::Result<(), String> {
    if super::demo() {
        return Ok(());
    }
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
    if super::demo() {
        return vec![Vpn { name: "Demo Office".into(), connected: false }];
    }
    let mut entries = vec![RASENTRYNAMEW { dwSize: size_of::<RASENTRYNAMEW>() as u32, ..Default::default() }; 16];
    let mut cb = (entries.len() * size_of::<RASENTRYNAMEW>()) as u32;
    let mut count = 0u32;
    let r = unsafe { RasEnumEntriesW(PCWSTR::null(), PCWSTR::null(), Some(entries.as_mut_ptr()), &mut cb, &mut count) };
    if r != 0 {
        return Vec::new();
    }
    let mut conns = vec![RASCONNW { dwSize: size_of::<RASCONNW>() as u32, ..Default::default() }; 8];
    let mut cb = (conns.len() * size_of::<RASCONNW>()) as u32;
    let mut live = 0u32;
    let connected: Vec<String> = if unsafe { RasEnumConnectionsW(Some(conns.as_mut_ptr()), &mut cb, &mut live) } == 0 {
        conns[..live as usize].iter().map(|c| utf16(&c.szEntryName)).collect()
    } else {
        Vec::new()
    };
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
    if super::demo() {
        return Ok(());
    }
    let mut cmd = std::process::Command::new("rasdial");
    cmd.arg(name);
    if !on {
        cmd.arg("/disconnect");
    }
    let out = cmd.creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        // У rasdial ответ на языке системы: показываем код возврата.
        Err(format!("rasdial: {}", out.status.code().unwrap_or(-1)))
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
