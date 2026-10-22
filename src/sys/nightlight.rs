// Ночной свет Windows. Открытого API нет: «Параметры» хранят состояние в CloudStore —
// двоичное значение `Data`. Устройство (проверено на Windows 10/11): заголовок `43 42 01 00`,
// `0a 02 01 00 2a 06`, время изменения (секунды Unix, LEB128, 5 байт), `2a 2b 0e`, длина вложенного
// блока, вложенный блок `43 42 01 00` [`02 01` — включён] `c6 14` …. Включить — вставить `02 01` и
// длину +2; выключить — наоборот; время — сейчас, иначе Windows не заметит. Незнакомое устройство
// значения — не трогаем (`None`).

use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_BINARY, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use windows::core::PCWSTR;

use super::wide;

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\DefaultAccount\Current\default$windows.data.bluelightreduction.bluelightreductionstate\windows.data.bluelightreduction.bluelightreductionstate";

/// Где что лежит в значении.
const TIME: std::ops::Range<usize> = 10..15;
const LEN: usize = 18;
const FLAG: usize = 23;

/// Знакомо ли устройство значения.
fn known(d: &[u8]) -> bool {
    d.len() > FLAG + 2
        && d[0..4] == [0x43, 0x42, 0x01, 0x00]
        && d[8..10] == [0x2a, 0x06]
        && d[15..18] == [0x2a, 0x2b, 0x0e]
        && d[19..23] == [0x43, 0x42, 0x01, 0x00]
}

/// Включён ли ночной свет; незнакомое значение — `None`.
pub fn parse(d: &[u8]) -> Option<bool> {
    known(d).then(|| d[FLAG..FLAG + 2] == [0x02, 0x01])
}

/// Секунды в LEB128 ровно 5 байт (хватит до 2038+).
fn leb5(mut v: u64) -> [u8; 5] {
    let mut out = [0u8; 5];
    for (i, b) in out.iter_mut().enumerate() {
        *b = (v & 0x7f) as u8;
        v >>= 7;
        if i < 4 {
            *b |= 0x80;
        }
    }
    out
}

/// Новое значение с нужным состоянием и временем `now` (секунды Unix); незнакомое — `None`.
pub fn with_state(d: &[u8], on: bool, now: u64) -> Option<Vec<u8>> {
    let was = parse(d)?;
    let mut out = d.to_vec();
    // Длина — один байт LEB128: от 0x80 и выше она стала бы двухбайтной — такое не трогаем.
    if out[LEN] >= 0x7e {
        return None;
    }
    if on && !was {
        out.splice(FLAG..FLAG, [0x02, 0x01]);
        out[LEN] = out[LEN].checked_add(2)?;
    } else if !on && was {
        out.drain(FLAG..FLAG + 2);
        out[LEN] = out[LEN].checked_sub(2)?;
    }
    out[TIME].copy_from_slice(&leb5(now));
    Some(out)
}

fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    let path = wide(KEY);
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, access, &mut key) }.ok().ok()?;
    Some(key)
}

fn read() -> Option<Vec<u8>> {
    let key = open(KEY_QUERY_VALUE)?;
    let name = wide("Data");
    let mut size = 0u32;
    let mut buf = Vec::new();
    let r = unsafe {
        let _ = RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size));
        buf.resize(size as usize, 0);
        RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, Some(buf.as_mut_ptr()), Some(&mut size))
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    r.ok().ok()?;
    buf.truncate(size as usize);
    Some(buf)
}

/// Включён ли ночной свет сейчас; не узнать — `None` (плитка «недоступно»).
pub fn state() -> Option<bool> {
    if super::demo() {
        return Some(false);
    }
    read().and_then(|d| parse(&d))
}

pub fn set(on: bool) -> Result<(), String> {
    let data = read().ok_or("night light state not found")?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let new = with_state(&data, on, now).ok_or("unknown night light format")?;
    let key = open(KEY_SET_VALUE).ok_or("cannot open night light key")?;
    let name = wide("Data");
    let r = unsafe { RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_BINARY, Some(&new)) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    r.ok().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Выключенный ночной свет (снято с Windows 11, 22.10.2026).
    const OFF: [u8; 38] = [
        0x43, 0x42, 0x01, 0x00, 0x0a, 0x02, 0x01, 0x00, 0x2a, 0x06, 0xc0, 0x94, 0xe6, 0xd6, 0x06, 0x2a, 0x2b, 0x0e,
        0x10, 0x43, 0x42, 0x01, 0x00, 0xc6, 0x14, 0xc4, 0x84, 0xa8, 0xf8, 0x9c, 0xbb, 0xd8, 0xee, 0x01, 0x00, 0x00,
        0x00, 0x00,
    ];

    #[test]
    fn toggles() {
        assert_eq!(parse(&OFF), Some(false));
        let on = with_state(&OFF, true, 1_792_641_600).unwrap();
        assert_eq!(on.len(), OFF.len() + 2);
        assert_eq!(parse(&on), Some(true));
        assert_eq!(on[LEN], 0x12);
        assert_eq!(on[TIME], OFF[TIME], "то же время — те же байты");
        let off = with_state(&on, false, 1_792_641_600).unwrap();
        assert_eq!(off, OFF.to_vec());
        let later = with_state(&OFF, false, 1_792_641_601).unwrap();
        assert_ne!(later[TIME], OFF[TIME], "время обновляется");
    }

    #[test]
    fn unknown_is_left_alone() {
        assert_eq!(parse(&[1, 2, 3]), None);
        let mut bad = OFF;
        bad[19] = 0;
        assert_eq!(with_state(&bad, true, 0), None);
    }
}
