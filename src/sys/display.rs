// Экран: режимы (разрешение и частота) и масштаб каждого монитора.
//
// Режимы — через GDI (`EnumDisplaySettingsW`, `ChangeDisplaySettingsExW`) по имени устройства
// `\\.\DISPLAYn`. Масштаб — через DisplayConfig: у Windows нет открытого API для него, поэтому
// берём недокументированные пакеты -3 (узнать) и -4 (поставить), которыми пользуется сама
// «Параметры → Дисплей». Масштаб там задаётся шагом относительно рекомендованного.

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    DisplayConfigSetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, LUID};
use windows::Win32::Graphics::Gdi::{
    CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_RESTART, DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL,
    DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
    EnumDisplaySettingsW,
};
use windows::core::PCWSTR;

use super::wide;

/// Режим экрана.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    pub bits: u32,
}

/// Масштаб монитора: текущий, рекомендованный и все, что Windows разрешает, — в процентах.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scale {
    pub current: u32,
    pub recommended: u32,
    pub options: Vec<u32>,
}

/// Шкала масштабов «Параметров Windows»; шаг относительного масштаба — индекс в ней.
const SCALES: [u32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];

fn read_mode(device: &[u16], index: ENUM_DISPLAY_SETTINGS_MODE) -> Option<Mode> {
    let mut dm = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
    let ok = unsafe { EnumDisplaySettingsW(PCWSTR(device.as_ptr()), index, &mut dm) };
    ok.as_bool().then_some(Mode {
        width: dm.dmPelsWidth,
        height: dm.dmPelsHeight,
        hz: dm.dmDisplayFrequency,
        bits: dm.dmBitsPerPel,
    })
}

/// Текущий режим устройства `\\.\DISPLAYn`.
pub fn current(device: &str) -> Option<Mode> {
    read_mode(&wide(device), ENUM_CURRENT_SETTINGS)
}

/// Все режимы устройства с той же глубиной цвета, что сейчас, без повторов.
pub fn modes(device: &str) -> Vec<Mode> {
    let name = wide(device);
    let bits = read_mode(&name, ENUM_CURRENT_SETTINGS).map(|m| m.bits);
    let mut out: Vec<Mode> = Vec::new();
    for i in 0.. {
        let Some(m) = read_mode(&name, ENUM_DISPLAY_SETTINGS_MODE(i)) else { break };
        if bits.is_some_and(|b| b != m.bits) || out.contains(&m) {
            continue;
        }
        out.push(m);
    }
    out
}

/// Поставить режим и запомнить его в реестре (как «Параметры»).
pub fn apply(device: &str, mode: Mode) -> Result<(), String> {
    let name = wide(device);
    let dm = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_BITSPERPEL,
        dmPelsWidth: mode.width,
        dmPelsHeight: mode.height,
        dmDisplayFrequency: mode.hz,
        dmBitsPerPel: mode.bits,
        ..Default::default()
    };
    let r = unsafe { ChangeDisplaySettingsExW(PCWSTR(name.as_ptr()), Some(&dm), None, CDS_UPDATEREGISTRY, None) };
    match r {
        DISP_CHANGE_SUCCESSFUL => Ok(()),
        DISP_CHANGE_RESTART => Err("restart required".into()),
        other => Err(format!("ChangeDisplaySettingsEx failed: {}", other.0)),
    }
}

// ─── Масштаб и имена мониторов (DisplayConfig) ──────────────────────────────

#[repr(C)]
struct DpiGet {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    min_rel: i32,
    cur_rel: i32,
    max_rel: i32,
}

#[repr(C)]
struct DpiSet {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    rel: i32,
}

const DPI_GET: DISPLAYCONFIG_DEVICE_INFO_TYPE = DISPLAYCONFIG_DEVICE_INFO_TYPE(-3);
const DPI_SET: DISPLAYCONFIG_DEVICE_INFO_TYPE = DISPLAYCONFIG_DEVICE_INFO_TYPE(-4);

/// Активный путь DisplayConfig для устройства GDI: источник (для масштаба) и имя монитора.
struct Source {
    adapter: LUID,
    source_id: u32,
    monitor_name: String,
}

fn active_paths() -> Vec<DISPLAYCONFIG_PATH_INFO> {
    let (mut np, mut nm) = (0u32, 0u32);
    unsafe {
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), None)
            != ERROR_SUCCESS
        {
            return Vec::new();
        }
        paths.truncate(np as usize);
        paths
    }
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn source_of(device: &str) -> Option<Source> {
    for path in active_paths() {
        let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        name.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        name.header.adapterId = path.sourceInfo.adapterId;
        name.header.id = path.sourceInfo.id;
        if unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } != 0 {
            continue;
        }
        if !from_wide(&name.viewGdiDeviceName).eq_ignore_ascii_case(device) {
            continue;
        }
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        target.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        target.header.adapterId = path.targetInfo.adapterId;
        target.header.id = path.targetInfo.id;
        let monitor_name = if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } == 0 {
            from_wide(&target.monitorFriendlyDeviceName)
        } else {
            String::new()
        };
        return Some(Source { adapter: path.sourceInfo.adapterId, source_id: path.sourceInfo.id, monitor_name });
    }
    None
}

/// Имя монитора, как его называет сам монитор («DELL U2720Q»); пусто — если не сказал.
pub fn monitor_name(device: &str) -> String {
    source_of(device).map(|s| s.monitor_name).unwrap_or_default()
}

/// Масштабы, доступные монитору, по ответу пакета -3.
fn scale_from(min_rel: i32, cur_rel: i32, max_rel: i32) -> Option<Scale> {
    // Рекомендованный — шаг |min_rel| от начала шкалы: ниже 100 % Windows не опускает.
    let rec = min_rel.unsigned_abs() as usize;
    let at = |rel: i32| SCALES.get((rec as i32 + rel).max(0) as usize).copied();
    let options: Vec<u32> = (min_rel..=max_rel).filter_map(at).collect();
    Some(Scale { current: at(cur_rel)?, recommended: *SCALES.get(rec)?, options })
}

/// Масштаб монитора `\\.\DISPLAYn`.
pub fn scale(device: &str) -> Option<Scale> {
    let src = source_of(device)?;
    let mut get = DpiGet {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DPI_GET,
            size: size_of::<DpiGet>() as u32,
            adapterId: src.adapter,
            id: src.source_id,
        },
        min_rel: 0,
        cur_rel: 0,
        max_rel: 0,
    };
    if unsafe { DisplayConfigGetDeviceInfo(&mut get.header) } != 0 {
        return None;
    }
    scale_from(get.min_rel, get.cur_rel, get.max_rel)
}

/// Поставить масштаб в процентах; он должен быть среди `Scale::options`.
pub fn set_scale(device: &str, percent: u32) -> Result<(), String> {
    let now = scale(device).ok_or("scale is not available")?;
    let rec = SCALES.iter().position(|&s| s == now.recommended).ok_or("unknown recommended scale")? as i32;
    let want = SCALES.iter().position(|&s| s == percent).ok_or("unknown scale")? as i32;
    let src = source_of(device).ok_or("display not found")?;
    let set = DpiSet {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DPI_SET,
            size: size_of::<DpiSet>() as u32,
            adapterId: src.adapter,
            id: src.source_id,
        },
        rel: want - rec,
    };
    match unsafe { DisplayConfigSetDeviceInfo(&set.header) } {
        0 => Ok(()),
        e => Err(format!("DisplayConfigSetDeviceInfo failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_steps_from_recommended() {
        // Рекомендованный 150 % (шаг 2), можно от 100 до 225.
        let s = scale_from(-2, 1, 3).unwrap();
        assert_eq!(s.recommended, 150);
        assert_eq!(s.current, 175);
        assert_eq!(s.options, vec![100, 125, 150, 175, 200, 225]);
    }

    #[test]
    fn scale_at_100_only_grows() {
        let s = scale_from(0, 0, 2).unwrap();
        assert_eq!((s.recommended, s.current), (100, 100));
        assert_eq!(s.options, vec![100, 125, 150]);
    }
}
