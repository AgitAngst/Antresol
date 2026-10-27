// Часовые пояса Windows: список (`EnumDynamicTimeZoneInformation`) и время в поясе сейчас
// (`SystemTimeToTzSpecificLocalTimeEx` — с переходом на летнее время, как в часах Windows).
// Пояс называется ключом Windows: «Russian Standard Time», «Tokyo Standard Time».

use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetSystemTime;
use windows::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation, SystemTimeToTzSpecificLocalTimeEx,
};

fn utf16(s: &[u16]) -> String {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

/// Все пояса Windows (ключ и описание), прочитанные из реестра один раз за запуск.
fn all() -> &'static [(String, DYNAMIC_TIME_ZONE_INFORMATION)] {
    static ZONES: std::sync::OnceLock<Vec<(String, DYNAMIC_TIME_ZONE_INFORMATION)>> = std::sync::OnceLock::new();
    ZONES.get_or_init(|| {
        let mut out = Vec::new();
        for i in 0.. {
            let mut z = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if unsafe { EnumDynamicTimeZoneInformation(i, &mut z) } != 0 {
                break;
            }
            out.push((utf16(&z.TimeZoneKeyName), z));
        }
        out
    })
}

/// Ключи всех поясов, по алфавиту.
pub fn keys() -> Vec<String> {
    let mut k: Vec<String> = all().iter().map(|(key, _)| key.clone()).collect();
    k.sort();
    k.dedup();
    k
}

/// Время в поясе сейчас: часы, минуты и смещение от UTC в минутах; пояса нет — `None`.
pub fn now_in(key: &str) -> Option<(u16, u16, i32)> {
    let (_, z) = all().iter().find(|(k, _)| k == key)?;
    let utc = unsafe { GetSystemTime() };
    let mut local = SYSTEMTIME::default();
    unsafe {
        SystemTimeToTzSpecificLocalTimeEx(Some(z), &utc, &mut local).ok()?;
    }
    Some((local.wHour, local.wMinute, offset(&utc, &local)))
}

/// Смещение пояса этого ПК от UTC в минутах; не узнать — 0.
pub fn local_offset() -> i32 {
    let utc = unsafe { GetSystemTime() };
    let mut local = SYSTEMTIME::default();
    let ok = unsafe { windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime(None, &utc, &mut local) }.is_ok();
    if ok { offset(&utc, &local) } else { 0 }
}

/// Смещение пояса этого ПК от UTC в секундах на момент UTC `utc` (секунды с 1970 года), с летним временем того момента:
/// встреча завтра считается по завтрашнему смещению, а не по сегодняшнему. Не узнать — 0.
pub fn local_offset_at(utc: i64) -> i32 {
    let Some(at) = system_time(utc) else { return 0 };
    let mut local = SYSTEMTIME::default();
    let ok = unsafe { windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime(None, &at, &mut local) }.is_ok();
    if ok { offset(&at, &local) * 60 } else { 0 }
}

/// Момент по секундам с 1970 года в виде `SYSTEMTIME` (UTC); вне годов 1601–30827 — `None`.
fn system_time(unix: i64) -> Option<SYSTEMTIME> {
    let (days, rest) = (unix.div_euclid(86_400), unix.rem_euclid(86_400));
    let (y, m, d) = crate::ics::civil_from_days(days);
    if !(1601..=30_827).contains(&y) {
        return None;
    }
    Some(SYSTEMTIME {
        wYear: y as u16,
        wMonth: m as u16,
        wDayOfWeek: (days + 4).rem_euclid(7) as u16,
        wDay: d as u16,
        wHour: (rest / 3600) as u16,
        wMinute: (rest / 60 % 60) as u16,
        wSecond: (rest % 60) as u16,
        wMilliseconds: 0,
    })
}

/// Разница двух моментов (местное − UTC) в минутах, с учётом смены суток.
fn offset(utc: &SYSTEMTIME, local: &SYSTEMTIME) -> i32 {
    let minutes = |t: &SYSTEMTIME| days(t) * 1440 + t.wHour as i64 * 60 + t.wMinute as i64;
    (minutes(local) - minutes(utc)) as i32
}

/// Дни от начала эры — только для разницы двух дат.
fn days(t: &SYSTEMTIME) -> i64 {
    let (y, m, d) = (t.wYear as i64, t.wMonth as i64, t.wDay as i64);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy
}

/// «UTC+5:30», «UTC−3», «UTC».
pub fn utc_label(minutes: i32) -> String {
    if minutes == 0 {
        return "UTC".into();
    }
    let sign = if minutes > 0 { "+" } else { "−" };
    let m = minutes.abs();
    if m % 60 == 0 { format!("UTC{sign}{}", m / 60) } else { format!("UTC{sign}{}:{:02}", m / 60, m % 60) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(y: u16, mo: u16, d: u16, h: u16, mi: u16) -> SYSTEMTIME {
        SYSTEMTIME { wYear: y, wMonth: mo, wDay: d, wHour: h, wMinute: mi, ..Default::default() }
    }

    #[test]
    fn offsets() {
        assert_eq!(offset(&st(2026, 10, 22, 23, 0), &st(2026, 10, 23, 2, 0)), 180, "через полночь");
        assert_eq!(offset(&st(2027, 1, 1, 1, 0), &st(2026, 12, 31, 20, 0)), -300, "через год назад");
        assert_eq!(offset(&st(2026, 10, 22, 10, 0), &st(2026, 10, 22, 15, 30)), 330);
        assert_eq!(utc_label(330), "UTC+5:30");
        assert_eq!(utc_label(-180), "UTC−3");
        assert_eq!(utc_label(0), "UTC");
    }

    #[test]
    fn a_unix_moment_becomes_a_system_time() {
        let t = system_time(0).unwrap();
        assert_eq!((t.wYear, t.wMonth, t.wDay, t.wHour, t.wDayOfWeek), (1970, 1, 1, 0, 4), "четверг");
        let t = system_time(1_792_645_920).unwrap();
        assert_eq!((t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond), (2026, 10, 22, 5, 12, 0));
        assert!(system_time(-20_000_000_000).is_none(), "раньше 1601 года Windows времени не знает");
    }

    /// Смещение этого ПК на любой момент — целое число четвертей часа в пределах суток; какое именно — зависит от ПК. На
    /// «сейчас» оно то же, что даёт `local_offset` (если между двумя вопросами не перевели часы).
    #[test]
    fn the_local_offset_is_a_sane_number() {
        for t in [0, 1_700_000_000, 1_792_645_920, 1_800_000_000] {
            let off = local_offset_at(t);
            assert!(off.abs() <= 14 * 3600 && off % 900 == 0, "{off}");
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        assert_eq!(local_offset_at(now), local_offset() * 60);
    }

    /// Живое: пояса Windows есть, Токио — UTC+9 (летнего времени нет).
    #[test]
    #[ignore]
    fn reads_zones() {
        assert!(keys().len() > 50);
        let (_, _, off) = now_in("Tokyo Standard Time").unwrap();
        assert_eq!(off, 540);
    }
}
