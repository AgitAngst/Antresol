// Ресурсы: процессор, память, видеокарта, сеть, батарея, место на дисках. Всё — дешёвые вызовы
// Windows (GetSystemTimes, GlobalMemoryStatusEx, счётчики PDH «GPU Engine», GetIfTable2,
// GetSystemPowerStatus, GetDiskFreeSpaceExW); проценты и скорости — по разнице двух замеров.
// Температуры нет: Windows отдаёт её только через WMI с правами администратора или драйвер.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCollectQueryData,
    PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::core::{PCWSTR, w};

/// Прошлый замер старше этого — панель была спрятана: разница за всё время не нужна.
const STALE: Duration = Duration::from_secs(5);
/// Счётчики сети по адаптерам: LUID → (принято, отдано), байты.
type NetCounts = HashMap<u64, (u64, u64)>;
/// PDH: буфер мал, данных стало больше.
const PDH_MORE_DATA: u32 = 0x8000_07D2;

/// Тип диска «несъёмный» у GetDriveTypeW.
const DRIVE_FIXED: u32 = 3;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    /// 0..1; `None` — первый замер, сравнить не с чем.
    pub cpu: Option<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    /// 0..1, сумма движков 3D всех процессов; `None` — счётчиков нет.
    pub gpu: Option<f32>,
    /// Байт в секунду: принято, отдано.
    pub net: Option<(f64, f64)>,
    /// Заряд 0..100 и от сети ли; `None` — батареи нет.
    pub battery: Option<(u8, bool)>,
    /// Диски: буква, свободно, всего.
    pub disks: Vec<(char, u64, u64)>,
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// Доля занятого процессора по двум замерам (простой, ядро, пользователь); ядро включает простой.
pub fn cpu_share(before: (u64, u64, u64), after: (u64, u64, u64)) -> Option<f32> {
    let idle = after.0.checked_sub(before.0)?;
    let total = after.1.checked_sub(before.1)? + after.2.checked_sub(before.2)?;
    (total > 0).then(|| (1.0 - idle as f64 / total as f64).clamp(0.0, 1.0) as f32)
}

pub struct Sampler {
    cpu: Option<((u64, u64, u64), Instant)>,
    net: Option<(NetCounts, Instant)>,
    demo_tick: u32,
    gpu: Option<(PDH_HQUERY, PDH_HCOUNTER)>,
    gpu_primed: bool,
}

impl Default for Sampler {
    fn default() -> Self {
        let gpu = unsafe {
            let mut q = PDH_HQUERY::default();
            let mut c = PDH_HCOUNTER::default();
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut q) == 0
                && PdhAddEnglishCounterW(q, w!(r"\GPU Engine(*engtype_3D)\Utilization Percentage"), 0, &mut c) == 0
            {
                Some((q, c))
            } else {
                None
            }
        };
        Sampler { cpu: None, net: None, demo_tick: 0, gpu, gpu_primed: false }
    }
}

impl Sampler {
    pub fn sample(&mut self) -> Snapshot {
        let mut s = Snapshot::default();
        unsafe {
            let (mut i, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
            if GetSystemTimes(Some(&mut i), Some(&mut k), Some(&mut u)).is_ok() {
                let now = (ft(i), ft(k), ft(u));
                // Прошлый замер давний (панель была спрятана) — доля за всё это время не нужна.
                s.cpu = self.cpu.filter(|(_, at)| at.elapsed() < STALE).and_then(|(b, _)| cpu_share(b, now));
                self.cpu = Some((now, Instant::now()));
            }
            let mut m = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
            if GlobalMemoryStatusEx(&mut m).is_ok() {
                s.mem_total = m.ullTotalPhys;
                s.mem_used = m.ullTotalPhys.saturating_sub(m.ullAvailPhys);
            }
            s.gpu = self.gpu_share();
            s.net = self.net_rate();
            let mut p = SYSTEM_POWER_STATUS::default();
            if GetSystemPowerStatus(&mut p).is_ok() && p.BatteryLifePercent <= 100 && p.BatteryFlag != 128 {
                s.battery = Some((p.BatteryLifePercent, p.ACLineStatus == 1));
            }
            let mask = GetLogicalDrives();
            for n in 0..26u32 {
                if mask & (1 << n) == 0 {
                    continue;
                }
                let letter = (b'A' + n as u8) as char;
                let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
                if GetDriveTypeW(PCWSTR(root.as_ptr())) != DRIVE_FIXED {
                    continue;
                }
                let (mut free, mut total) = (0u64, 0u64);
                if GetDiskFreeSpaceExW(PCWSTR(root.as_ptr()), Some(&mut free), Some(&mut total), None).is_ok() {
                    s.disks.push((letter, free, total));
                }
            }
        }
        if super::demo() {
            // Для снимков в открытый репозиторий — ничего настоящего о ПК.
            let cpu_ok = s.cpu.is_some();
            self.demo_tick = self.demo_tick.wrapping_add(1);
            let wave = (self.demo_tick as f32 * 0.7).sin();
            s = Snapshot {
                cpu: cpu_ok.then_some(0.2 + 0.1 * wave),
                mem_used: 6 << 30,
                mem_total: 16 << 30,
                gpu: Some(0.12),
                net: Some((2.4 * 1024.0 * 1024.0, 180.0 * 1024.0)),
                battery: None,
                disks: vec![('C', 120 << 30, 500 << 30), ('D', 800 << 30, 2000 << 30)],
            };
        }
        s
    }

    fn gpu_share(&mut self) -> Option<f32> {
        let (q, c) = self.gpu?;
        unsafe {
            if PdhCollectQueryData(q) != 0 {
                return None;
            }
            // Счётчик-скорость: первый сбор ничего не даёт.
            if !self.gpu_primed {
                self.gpu_primed = true;
                return None;
            }
            // Первый вызов — размер буфера; пока между вызовами появились новые процессы (PDH_MORE_DATA),
            // берём больше и пробуем снова.
            let (mut size, mut count) = (0u32, 0u32);
            let first = PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if size == 0 {
                // Нет ни одного процесса на видеокарте — 0 %; другая ошибка — не знаем.
                return (first == PDH_MORE_DATA || first == 0).then_some(0.0);
            }
            for _ in 0..3 {
                // Буфер в байтах: элементы и строки имён за ними.
                let items = (size as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()) + 1;
                let mut buf = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); items];
                match PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, Some(buf.as_mut_ptr())) {
                    0 => {
                        // Годные значения — с состоянием «данные верны» (0) или «новые» (1).
                        let sum: f64 = buf[..count as usize]
                            .iter()
                            .filter(|it| it.FmtValue.CStatus <= 1)
                            .map(|it| it.FmtValue.Anonymous.doubleValue)
                            .sum();
                        return Some((sum / 100.0).clamp(0.0, 1.0) as f32);
                    }
                    PDH_MORE_DATA => continue,
                    _ => return None,
                }
            }
            None
        }
    }

    /// Скорость сети — по разнице счётчиков каждого адаптера (по LUID): адаптер, который только что
    /// появился, не даёт скачка, а пропавший — провала.
    fn net_rate(&mut self) -> Option<(f64, f64)> {
        let mut now_counts = NetCounts::new();
        unsafe {
            let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
            if GetIfTable2(&mut table) != ERROR_SUCCESS || table.is_null() {
                return None;
            }
            let t = &*table;
            let rows = std::slice::from_raw_parts(t.Table.as_ptr(), t.NumEntries as usize);
            for r in rows {
                // Только настоящие подключённые адаптеры: без фильтров и виртуальных копий.
                let flags = r.InterfaceAndOperStatusFlags._bitfield;
                let hardware = flags & 0x01 != 0;
                let filter = flags & 0x02 != 0;
                if r.OperStatus == IfOperStatusUp && hardware && !filter {
                    now_counts.insert(r.InterfaceLuid.Value, (r.InOctets, r.OutOctets));
                }
            }
            FreeMibTable(table as *const _);
        }
        let now = Instant::now();
        let rate = self.net.as_ref().filter(|(_, at)| now.duration_since(*at) < STALE).and_then(|(prev, at)| {
            let secs = now.duration_since(*at).as_secs_f64();
            (secs > 0.0).then(|| {
                let (mut rx, mut tx) = (0u64, 0u64);
                for (luid, (i, o)) in &now_counts {
                    if let Some((pi, po)) = prev.get(luid) {
                        rx += i.saturating_sub(*pi);
                        tx += o.saturating_sub(*po);
                    }
                }
                (rx as f64 / secs, tx as f64 / secs)
            })
        });
        self.net = Some((now_counts, now));
        rate
    }
}

/// «1,2 ГБ», «340 МБ», «12 КБ» — байты по-человечески; `ru` — русские единицы.
pub fn bytes(n: f64, ru: bool) -> String {
    let units: [&str; 5] = if ru { ["Б", "КБ", "МБ", "ГБ", "ТБ"] } else { ["B", "KB", "MB", "GB", "TB"] };
    let mut v = n;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let text = if v >= 100.0 || i == 0 { format!("{v:.0}") } else { format!("{v:.1}") };
    let text = if ru { text.replace('.', ",") } else { text };
    format!("{text} {}", units[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_from_times() {
        // Простой вырос на 25 из 100 тиков ядра+пользователя — занято 75 %.
        let s = cpu_share((0, 0, 0), (25, 60, 40)).unwrap();
        assert!((s - 0.75).abs() < 1e-6);
        assert!(cpu_share((10, 10, 10), (10, 10, 10)).is_none(), "без хода времени — нечего сравнивать");
        assert!(cpu_share((10, 10, 10), (5, 20, 20)).is_none(), "счётчик назад — не верим");
    }

    #[test]
    fn sampler_reads_this_pc() {
        // Настоящие вызовы Windows: память есть всегда, процессор — со второго замера.
        let mut sm = Sampler::default();
        let first = sm.sample();
        assert!(first.mem_total > 0 && first.mem_used <= first.mem_total);
        assert!(first.cpu.is_none());
        std::thread::sleep(std::time::Duration::from_millis(200));
        let second = sm.sample();
        assert!(second.cpu.is_some_and(|c| (0.0..=1.0).contains(&c)));
        assert!(second.net.is_some());
        assert!(!second.disks.is_empty());
    }

    #[test]
    fn human_bytes() {
        assert_eq!(bytes(512.0, false), "512 B");
        assert_eq!(bytes(1536.0, true), "1,5 КБ");
        assert_eq!(bytes(5.0 * 1024.0 * 1024.0 * 1024.0, false), "5.0 GB");
        assert_eq!(bytes(250.0 * 1024.0 * 1024.0, false), "250 MB");
    }
}
