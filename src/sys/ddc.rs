// Яркость, контраст и вход внешних мониторов по DDC/CI (MCCS, коды VCP 0x10, 0x12, 0x60).
//
// Каждый запрос к монитору идёт по шине монитора и занимает десятки миллисекунд, строка
// возможностей — до секунды, поэтому всё это зовётся только из потока модуля, не из окна.
// Встроенный экран ноутбука DDC/CI не отвечает — для него здесь ничего нет.

use windows::Win32::Devices::Display::{
    CapabilitiesRequestAndCapabilitiesReply, DestroyPhysicalMonitors, GetCapabilitiesStringLength,
    GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply,
    PHYSICAL_MONITOR, SetVCPFeature,
};
use windows::Win32::Graphics::Gdi::HMONITOR;

pub const BRIGHTNESS: u8 = 0x10;
pub const CONTRAST: u8 = 0x12;
pub const INPUT: u8 = 0x60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    pub current: u32,
    pub max: u32,
}

/// Один физический монитор за логическим (`\\.\DISPLAYn`) — обычно один.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Physical {
    pub device: String,
    pub index: usize,
    pub name: String,
    pub brightness: Option<Feature>,
    pub contrast: Option<Feature>,
    pub input: Option<u32>,
    /// Входы из строки возможностей монитора; пусто — не сказал.
    pub inputs: Vec<u32>,
}

/// Открыть физические мониторы логического, сделать дело и закрыть.
fn with_physical<R>(handle: usize, f: impl FnOnce(&[PHYSICAL_MONITOR]) -> R) -> Option<R> {
    let h = HMONITOR(handle as *mut core::ffi::c_void);
    let mut n = 0u32;
    unsafe {
        GetNumberOfPhysicalMonitorsFromHMONITOR(h, &mut n).ok()?;
        if n == 0 {
            return None;
        }
        let mut list = vec![PHYSICAL_MONITOR::default(); n as usize];
        GetPhysicalMonitorsFromHMONITOR(h, &mut list).ok()?;
        let r = f(&list);
        let _ = DestroyPhysicalMonitors(&list);
        Some(r)
    }
}

fn read_feature(m: &PHYSICAL_MONITOR, code: u8) -> Option<Feature> {
    let (mut cur, mut max) = (0u32, 0u32);
    let ok = unsafe { GetVCPFeatureAndVCPFeatureReply(m.hPhysicalMonitor, code, None, &mut cur, Some(&mut max)) };
    (ok != 0).then_some(Feature { current: cur, max })
}

fn capabilities(m: &PHYSICAL_MONITOR) -> Option<String> {
    let mut len = 0u32;
    unsafe {
        if GetCapabilitiesStringLength(m.hPhysicalMonitor, &mut len) == 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        if CapabilitiesRequestAndCapabilitiesReply(m.hPhysicalMonitor, &mut buf) == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
    }
}

/// Прочитать монитор. `with_caps` — ещё и строку возможностей (медленно; входы меняются редко).
pub fn read(device: &str, handle: usize, with_caps: bool) -> Vec<Physical> {
    with_physical(handle, |list| {
        list.iter()
            .enumerate()
            .map(|(index, m)| {
                // Структура упакована: поле копируем, ссылку на него брать нельзя.
                let desc = m.szPhysicalMonitorDescription;
                let end = desc.iter().position(|&c| c == 0).unwrap_or(desc.len());
                let name =
                    if super::demo() { "Monitor 27".to_string() } else { String::from_utf16_lossy(&desc[..end]) };
                if super::demo() {
                    // Для снимков — выдуманный монитор: значения и входы не настоящие.
                    let f = |current| Some(Feature { current, max: 100 });
                    return Physical {
                        device: device.to_string(),
                        index,
                        name,
                        brightness: f(70),
                        contrast: f(50),
                        input: Some(0x11),
                        inputs: if with_caps { vec![0x11, 0x0F] } else { Vec::new() },
                    };
                }
                Physical {
                    device: device.to_string(),
                    index,
                    name,
                    brightness: read_feature(m, BRIGHTNESS),
                    contrast: read_feature(m, CONTRAST),
                    // Младший байт — код входа; старший у части мониторов занят чем-то своим.
                    input: read_feature(m, INPUT).map(|f| f.current & 0xFF),
                    inputs: if with_caps {
                        capabilities(m).map(|c| parse_inputs(&c)).unwrap_or_default()
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Поставить значение VCP физическому монитору `index` логического `handle`.
pub fn set(handle: usize, index: usize, code: u8, value: u32) -> Result<(), String> {
    with_physical(handle, |list| {
        let m = list.get(index).ok_or("no such physical monitor")?;
        match unsafe { SetVCPFeature(m.hPhysicalMonitor, code, value) } {
            0 => Err(format!("SetVCPFeature 0x{code:02X} failed")),
            _ => Ok(()),
        }
    })
    .unwrap_or_else(|| Err("monitor is gone".into()))
}

/// Коды входов из строки возможностей: `...vcp(02 04 10 12 60(0F 11 12) ...)...`.
pub fn parse_inputs(caps: &str) -> Vec<u32> {
    let lower = caps.to_ascii_lowercase();
    let Some(vcp) = lower.find("vcp(") else { return Vec::new() };
    let body = &lower[vcp + 4..];
    // Ищем «60(» на верхнем уровне списка vcp: вложенные скобки других кодов пропускаем.
    let bytes = body.as_bytes();
    let mut depth = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' if depth == 0 => break,
            b')' => depth -= 1,
            b'6' if depth == 0 && body[i..].starts_with("60(") && (i == 0 || !bytes[i - 1].is_ascii_hexdigit()) => {
                let rest = &body[i + 3..];
                let end = rest.find(')').unwrap_or(rest.len());
                let mut out: Vec<u32> =
                    rest[..end].split_whitespace().filter_map(|h| u32::from_str_radix(h, 16).ok()).collect();
                out.dedup();
                return out;
            }
            _ => {}
        }
        i += 1;
    }
    Vec::new()
}

/// Имя входа по коду MCCS.
pub fn input_name(code: u32) -> String {
    match code {
        0x01 => "VGA 1".into(),
        0x02 => "VGA 2".into(),
        0x03 => "DVI 1".into(),
        0x04 => "DVI 2".into(),
        0x0F => "DisplayPort 1".into(),
        0x10 => "DisplayPort 2".into(),
        0x11 => "HDMI 1".into(),
        0x12 => "HDMI 2".into(),
        0x1B => "USB-C".into(),
        other => format!("0x{other:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_from_caps() {
        let caps = "(prot(monitor)type(lcd)model(X)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(05 08 0B) 16 18 1A 60(0F 11 12) 62 AC AE B6 C6 C8 C9 D6(01 04) DF)mswhql(1)asset_eep(40)mccs_ver(2.2))";
        assert_eq!(parse_inputs(caps), vec![0x0F, 0x11, 0x12]);
    }

    #[test]
    fn inputs_absent_or_nested_only() {
        assert!(parse_inputs("vcp(10 12 14(05 06))").is_empty());
        assert!(parse_inputs("no vcp here").is_empty());
        // «60(» внутри чужих скобок — не вход.
        assert!(parse_inputs("vcp(14(60(01)) 10)").is_empty());
        // Код, оканчивающийся на 6 перед «0(», не путаем: «160(» — не 60.
        assert!(parse_inputs("vcp(160(01 02))").is_empty());
    }

    #[test]
    fn input_names() {
        assert_eq!(input_name(0x11), "HDMI 1");
        assert_eq!(input_name(0x0F), "DisplayPort 1");
        assert_eq!(input_name(0x99), "0x99");
    }
}
