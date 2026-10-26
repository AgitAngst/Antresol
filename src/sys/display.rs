// Экран: режимы (разрешение и частота) и масштаб каждого монитора.
//
// Режимы — через GDI (`EnumDisplaySettingsW`, `ChangeDisplaySettingsExW`) по имени устройства
// `\\.\DISPLAYn`. Масштаб — через DisplayConfig: у Windows нет открытого API для него, поэтому
// берём недокументированные пакеты -3 (узнать) и -4 (поставить), которыми пользуется сама
// «Параметры → Дисплей». Масштаб там задаётся шагом относительно рекомендованного.
//
// В демо-режиме и в тестах (`sys::isolated`) всё это — выдуманный экран из `static`: Windows не читается и не
// пишется. У него два режима, как у настоящего: «сейчас» и «в реестре» — `apply` меняет только первый, `commit`
// переносит его во второй (после «Оставить»), так что откат без окна проверяется тестами.

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    DisplayConfigSetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, LUID};
use windows::Win32::Graphics::Gdi::{
    CDS_NORESET, CDS_SET_PRIMARY, CDS_TYPE, CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW,
    DISP_CHANGE_RESTART, DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
    DM_POSITION, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplaySettingsW,
};
use windows::core::PCWSTR;

use std::sync::Mutex;

use super::monitor::{self, Monitor, PxRect};
use super::{blocked, isolated, real_write, wide};

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

// ─── Выдуманный экран (демо-режим и тесты) ──────────────────────────────────

/// Устройство выдуманного монитора в демо-режиме. Такого `DISPLAYn` у настоящей Windows нет — даже сорвавшийся
/// заслон ничего бы не поменял.
pub const FAKE_DEVICE: &str = r"\\.\DISPLAY9";

/// Что «стоит» у выдуманного монитора.
struct Fake {
    /// Режим на экране сейчас.
    now: Mode,
    /// Режим «в реестре» — тот, что вернётся после перезагрузки: меняется только подтверждением ([`commit`]).
    saved: Mode,
    /// Масштаб, %.
    scale: u32,
}

/// Выдуманные мониторы по именам устройств: нужный заводится при первом обращении.
static FAKE: Mutex<Vec<(String, Fake)>> = Mutex::new(Vec::new());

const fn fake_mode(width: u32, height: u32, hz: u32) -> Mode {
    Mode { width, height, hz, bits: 32 }
}

/// Режим, с которого начинает выдуманный монитор: 4K, 120 Гц.
const FAKE_START: Mode = fake_mode(3840, 2160, 120);
/// Масштаб выдуманного монитора вначале и рекомендованный ему.
const FAKE_SCALE: u32 = 150;
const FAKE_RECOMMENDED: u32 = 175;

/// Режимы выдуманного монитора.
fn fake_modes() -> Vec<Mode> {
    vec![
        fake_mode(3840, 2160, 60),
        fake_mode(3840, 2160, 120),
        fake_mode(2560, 1440, 60),
        fake_mode(2560, 1440, 120),
        fake_mode(2560, 1440, 144),
        fake_mode(1920, 1080, 60),
        fake_mode(1920, 1080, 120),
        fake_mode(1920, 1080, 144),
        fake_mode(1600, 900, 60),
        fake_mode(1280, 720, 60),
    ]
}

/// Масштабы выдуманного монитора.
fn fake_options() -> Vec<u32> {
    vec![100, 125, 150, 175, 200, 225]
}

fn with_fake<R>(device: &str, f: impl FnOnce(&mut Fake) -> R) -> R {
    let mut list = FAKE.lock().unwrap_or_else(|e| e.into_inner());
    let i = match list.iter().position(|(d, _)| d.eq_ignore_ascii_case(device)) {
        Some(i) => i,
        None => {
            list.push((device.to_string(), Fake { now: FAKE_START, saved: FAKE_START, scale: FAKE_SCALE }));
            list.len() - 1
        }
    };
    f(&mut list[i].1)
}

/// Имя устройства, под которым карточки видят монитор панели: настоящее; в демо-режиме — выдуманное
/// ([`FAKE_DEVICE`]); в тестах — то, что дали (у каждого теста свой монитор, тесты друг другу не мешают).
pub fn device_for(panel: &str) -> &str {
    if super::demo() { FAKE_DEVICE } else { panel }
}

/// Мониторы для карточек: настоящие; в демо-режиме и в тестах — один выдуманный (4K, основной).
pub fn monitors(panel: &str) -> Vec<Monitor> {
    if isolated() {
        let rect = PxRect { left: 0, top: 0, right: 3840, bottom: 2160 };
        return vec![Monitor { device: device_for(panel).to_string(), rect, primary: true, handle: 0 }];
    }
    monitor::monitors()
}

/// Текущий режим устройства `\\.\DISPLAYn`.
pub fn current(device: &str) -> Option<Mode> {
    if isolated() {
        return Some(with_fake(device, |f| f.now));
    }
    read_mode(&wide(device), ENUM_CURRENT_SETTINGS)
}

/// Режим, который выдуманная Windows помнит в реестре (вернётся после перезагрузки) — для тестов отката.
#[cfg(test)]
pub fn saved(device: &str) -> Mode {
    with_fake(device, |f| f.saved)
}

/// Все режимы устройства с той же глубиной цвета, что сейчас, без повторов.
pub fn modes(device: &str) -> Vec<Mode> {
    if isolated() {
        return fake_modes();
    }
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

fn devmode(mode: Mode) -> DEVMODEW {
    DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_BITSPERPEL,
        dmPelsWidth: mode.width,
        dmPelsHeight: mode.height,
        dmDisplayFrequency: mode.hz,
        dmBitsPerPel: mode.bits,
        ..Default::default()
    }
}

fn change_result(r: windows::Win32::Graphics::Gdi::DISP_CHANGE) -> Result<(), String> {
    match r {
        DISP_CHANGE_SUCCESSFUL => Ok(()),
        DISP_CHANGE_RESTART => Err("restart required".into()),
        other => Err(format!("ChangeDisplaySettingsEx failed: {}", other.0)),
    }
}

/// Поставить режим на время — «динамически», без записи в реестр: пока «Оставить» не нажали (закрыли программу,
/// она упала, перезагрузили Windows), настоящий режим остаётся прежним и после перезагрузки вернётся. Подтверждённый
/// режим запоминает [`commit`].
pub fn apply(device: &str, mode: Mode) -> Result<(), String> {
    if blocked(&format!("display::apply {device} {}x{}@{}", mode.width, mode.height, mode.hz)) {
        if !fake_modes().contains(&mode) {
            return Err("mode is not supported".into());
        }
        with_fake(device, |f| f.now = mode);
        return Ok(());
    }
    let name = wide(device);
    let dm = devmode(mode);
    real_write("ChangeDisplaySettingsExW");
    change_result(unsafe { ChangeDisplaySettingsExW(PCWSTR(name.as_ptr()), Some(&dm), None, CDS_TYPE(0), None) })
}

/// Запомнить режим в реестре (после «Оставить»), как «Параметры»: он переживёт перезагрузку. На экране ничего не
/// меняется (`CDS_NORESET`) — режим `mode` уже стоит.
pub fn commit(device: &str, mode: Mode) -> Result<(), String> {
    if blocked(&format!("display::commit {device} {}x{}@{}", mode.width, mode.height, mode.hz)) {
        with_fake(device, |f| f.saved = mode);
        return Ok(());
    }
    let name = wide(device);
    let dm = devmode(mode);
    real_write("ChangeDisplaySettingsExW");
    change_result(unsafe {
        ChangeDisplaySettingsExW(PCWSTR(name.as_ptr()), Some(&dm), None, CDS_UPDATEREGISTRY | CDS_NORESET, None)
    })
}

// ─── Основной монитор ──────────────────────────────────────────────────────

/// Новые места мониторов, чтобы `primary` встал в (0, 0): все сдвигаются на его угол, взаимное
/// расположение не меняется (основной у Windows всегда в начале координат).
pub fn shifted(places: &[(String, i32, i32)], primary: &str) -> Option<Vec<(String, i32, i32)>> {
    let (_, px, py) = places.iter().find(|(d, _, _)| d == primary)?;
    Some(places.iter().map(|(d, x, y)| (d.clone(), x - px, y - py)).collect())
}

/// Записать в реестр место монитора (без применения — `CDS_NORESET`), основной — с `CDS_SET_PRIMARY`.
fn write_position(device: &str, x: i32, y: i32, primary: bool) -> Result<(), String> {
    let name = wide(device);
    let mut dm = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
    if !unsafe { EnumDisplaySettingsW(PCWSTR(name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm) }.as_bool() {
        return Err(format!("cannot read {device}"));
    }
    dm.dmFields = DM_POSITION;
    dm.Anonymous1.Anonymous2.dmPosition.x = x;
    dm.Anonymous1.Anonymous2.dmPosition.y = y;
    let mut flags: CDS_TYPE = CDS_UPDATEREGISTRY | CDS_NORESET;
    if primary {
        flags |= CDS_SET_PRIMARY;
    }
    real_write("ChangeDisplaySettingsExW");
    let r = unsafe { ChangeDisplaySettingsExW(PCWSTR(name.as_ptr()), Some(&dm), None, flags, None) };
    if r == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        Err(format!("ChangeDisplaySettingsEx failed on {device}: {}", r.0))
    }
}

/// Сделать монитор основным: места всех мониторов (левый верхний угол в пикселях экрана) — `places`.
/// Пишется в реестр без применения (`CDS_NORESET`), потом применяется разом. Сбой посреди записи —
/// уже записанное возвращается к прежнему (иначе его применил бы любой следующий вызов), без применения.
pub fn set_primary(device: &str, places: &[(String, i32, i32)]) -> Result<(), String> {
    let moved = shifted(places, device).ok_or("monitor not found")?;
    if blocked(&format!("display::set_primary {device}")) {
        return Ok(());
    }
    for (i, (d, x, y)) in moved.iter().enumerate() {
        if let Err(e) = write_position(d, *x, *y, d == device) {
            for (od, ox, oy) in &places[..i] {
                let _ = write_position(od, *ox, *oy, *ox == 0 && *oy == 0);
            }
            return Err(e);
        }
    }
    // Применить записанное разом.
    real_write("ChangeDisplaySettingsExW");
    let r = unsafe { ChangeDisplaySettingsExW(PCWSTR::null(), None, None, CDS_TYPE(0), None) };
    match r {
        DISP_CHANGE_SUCCESSFUL => Ok(()),
        other => Err(format!("ChangeDisplaySettingsEx apply failed: {}", other.0)),
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

/// Имя монитора, как его называет сам монитор («Monitor 27»); пусто — если не сказал.
pub fn monitor_name(device: &str) -> String {
    if isolated() {
        return "Monitor 27".to_string();
    }
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
    if isolated() {
        let current = with_fake(device, |f| f.scale);
        return Some(Scale { current, recommended: FAKE_RECOMMENDED, options: fake_options() });
    }
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
    if blocked(&format!("display::set_scale {device} {percent}")) {
        if !fake_options().contains(&percent) {
            return Err("unknown scale".into());
        }
        with_fake(device, |f| f.scale = percent);
        return Ok(());
    }
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
    real_write("DisplayConfigSetDeviceInfo");
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

#[cfg(test)]
mod primary_tests {
    use super::*;

    #[test]
    fn shifts_to_origin() {
        let places = vec![("A".to_string(), 0, 0), ("B".to_string(), 2560, -200), ("C".to_string(), -1920, 0)];
        let s = shifted(&places, "B").unwrap();
        assert_eq!(s, vec![("A".to_string(), -2560, 200), ("B".to_string(), 0, 0), ("C".to_string(), -4480, 200)]);
        assert!(shifted(&places, "D").is_none());
    }
}

#[cfg(test)]
mod fake_tests {
    use super::*;

    #[test]
    fn fake_apply_is_temporary_until_commit() {
        let d = r"\\.\TEST-TEMP";
        assert_eq!(current(d), Some(FAKE_START));
        let want = fake_mode(1920, 1080, 144);
        apply(d, want).unwrap();
        assert_eq!(current(d), Some(want));
        assert_eq!(saved(d), FAKE_START, "в реестр режим попадает только после подтверждения");
        commit(d, want).unwrap();
        assert_eq!(saved(d), want);
        assert!(apply(d, fake_mode(1, 1, 1)).is_err(), "такого режима у монитора нет");
        assert_eq!(current(d), Some(want), "неудачная смена ничего не меняет");
        assert!(modes(d).contains(&want) && modes(d).contains(&FAKE_START));
    }

    #[test]
    fn fake_scale_follows_its_options() {
        let d = r"\\.\TEST-SCALE";
        let s = scale(d).unwrap();
        assert_eq!((s.current, s.recommended), (FAKE_SCALE, FAKE_RECOMMENDED));
        assert!(s.options.contains(&s.current) && s.options.contains(&s.recommended));
        set_scale(d, 200).unwrap();
        assert_eq!(scale(d).unwrap().current, 200);
        assert!(set_scale(d, 133).is_err(), "такого масштаба нет");
        assert_eq!(scale(d).unwrap().current, 200);
    }

    #[test]
    fn fake_world_is_one_invented_monitor() {
        // Один выдуманный монитор «Monitor 27»; настоящие экраны в демо и тестах не показываются.
        assert_eq!(monitors(r"\\.\TEST-FAKE").len(), 1);
        assert_eq!(monitor_name(r"\\.\TEST-FAKE"), "Monitor 27");
        assert!(fake_modes().contains(&FAKE_START), "стартовый режим есть среди режимов");
        assert!(fake_options().contains(&FAKE_SCALE) && fake_options().contains(&FAKE_RECOMMENDED));
    }
}
