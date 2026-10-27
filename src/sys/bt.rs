// Bluetooth: сопряжённые устройства, подключено ли, и заряд — если Windows его знает.
// Список — BluetoothFindFirstDevice (без поиска новых: только запомненные); заряд — свойство
// устройства {104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2 у узлов BTHENUM и BTHLEDEVICE, которые
// сопоставляются с устройством по адресу в их ID (12 шестнадцатеричных знаков). У тех же узлов — ContainerId устройства
// (`containers_of`): по нему `btaudio` находит звуковые точки устройства. Подключать и отключать звуковые устройства — `btaudio`.
// В демо и в тестах (`sys::fake_reads`) мир выдуманный: «Headphones», «Mouse», «Speaker», «Keyboard»; он помнит, что подключили
// (`fake_connect`, писатель за заслоном).

use std::sync::Mutex;

use windows::Win32::Devices::Bluetooth::{
    BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS, BluetoothFindDeviceClose, BluetoothFindFirstDevice,
    BluetoothFindNextDevice,
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW,
};
use windows::Win32::Devices::Properties::{DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID, DEVPROPTYPE};
use windows::Win32::Foundation::DEVPROPKEY;
use windows::core::{BOOL, GUID, w};

const BATTERY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub connected: bool,
    /// Заряд 0..100; `None` — Windows не знает.
    pub battery: Option<u8>,
    pub address: u64,
    /// Аудио или видео по классу устройства (наушники, колонки): плашка «подключены» рисует наушники, а карточка даёт
    /// «Подключить» — только таким (мышь, клавиатура, геймпад Windows подключает сама, когда их включают).
    pub audio: bool,
}

/// Старшие классы устройства (биты 8–12 `ulClassofDevice`): 4 — аудио и видео.
const MAJOR_AUDIO: u32 = 4;

/// Адрес из ID узла устройства: последний кусок из ровно 12 шестнадцатеричных знаков.
/// `BTHENUM\{0000111e-…}_LOCALMFG&0002\7&27e1d9e9&0&A0B1C2D3E4F5_C00000000` → 0xA0B1C2D3E4F5.
pub fn address_in(id: &str) -> Option<u64> {
    // Хвост базового UUID Bluetooth (…-00805f9b34fb) — не адрес.
    id.rsplit(|c: char| !c.is_ascii_hexdigit())
        .find(|part| part.len() == 12 && !part.eq_ignore_ascii_case("00805f9b34fb"))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
}

/// Обойти узлы `BTHENUM` и `BTHLEDEVICE`: для каждого — его ID и (набор, запись) SetupAPI, чтобы спросить у него свойство.
/// `present` — только присутствующие в системе (у сопряжённого, но не подключённого устройства узел обычно есть); нет — ещё и
/// «призраки» (узлы, которых сейчас нет).
fn each_node(present: bool, mut visit: impl FnMut(&str, HDEVINFO, &SP_DEVINFO_DATA)) {
    let flags = if present { DIGCF_PRESENT | DIGCF_ALLCLASSES } else { DIGCF_ALLCLASSES };
    for enumerator in [w!("BTHENUM"), w!("BTHLEDEVICE")] {
        unsafe {
            let Ok(set) = SetupDiGetClassDevsW(None, enumerator, None, flags) else {
                continue;
            };
            let mut i = 0;
            loop {
                let mut data = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
                if SetupDiEnumDeviceInfo(set, i, &mut data).is_err() {
                    break;
                }
                i += 1;
                let mut id = [0u16; 512];
                if SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id), None).is_err() {
                    continue;
                }
                let end = id.iter().position(|&c| c == 0).unwrap_or(id.len());
                visit(&String::from_utf16_lossy(&id[..end]), set, &data);
            }
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
    }
}

/// Заряд всех узлов, где он есть: (адрес, заряд).
fn batteries() -> Vec<(u64, u8)> {
    let mut out = Vec::new();
    each_node(true, |id, set, data| {
        let mut kind = DEVPROPTYPE::default();
        let mut value = [0u8; 1];
        if unsafe { SetupDiGetDevicePropertyW(set, data, &BATTERY, &mut kind, Some(&mut value), None, 0) }.is_err() {
            return;
        }
        if let Some(addr) = address_in(id)
            && value[0] <= 100
        {
            out.push((addr, value[0]));
        }
    });
    out
}

/// ContainerId устройства: он один у всех частей физического устройства — у узлов `BTHENUM` и `BTHLEDEVICE` с его адресом в ID
/// (те же, что дают заряд) и у его звуковых точек, по которым `btaudio` находит фильтры. Несколько — если Windows ведёт
/// устройство двумя узлами (классика и LE) с разными контейнерами. Сначала ищет среди присутствующих узлов, не нашёл — среди
/// всех. Пусто — узлов нет или Windows не ответила. Только чтение.
pub fn containers_of(address: u64) -> Vec<u128> {
    if super::fake_reads() {
        // Выдуманный мир: контейнер выдуманного устройства — его адрес.
        return vec![address as u128];
    }
    let mut out = Vec::new();
    for present in [true, false] {
        each_node(present, |id, set, data| {
            if address_in(id) != Some(address) {
                return;
            }
            let mut kind = DEVPROPTYPE::default();
            let mut guid = GUID::zeroed();
            let bytes = unsafe { std::slice::from_raw_parts_mut((&raw mut guid).cast::<u8>(), size_of::<GUID>()) };
            let read = unsafe {
                SetupDiGetDevicePropertyW(set, data, &DEVPKEY_Device_ContainerId, &mut kind, Some(bytes), None, 0)
            };
            let container = guid.to_u128();
            if read.is_ok() && kind == DEVPROP_TYPE_GUID && !out.contains(&container) {
                out.push(container);
            }
        });
        if !out.is_empty() {
            break;
        }
    }
    out
}

// ─── Выдуманный мир (демо и тесты) ──────────────────────────────────────────

/// Выдуманная колонка (звуковая, не подключена): её «Подключить» в демо.
pub const FAKE_SPEAKER: u64 = 4;

/// Выдуманные устройства по порядку: имя, адрес, звуковое ли, заряд, пока подключено.
const FAKE_DEVICES: [(&str, u64, bool, u8); 4] = [
    ("Headphones", 1, true, 80),
    ("Mouse", 2, false, 45),
    ("Speaker", FAKE_SPEAKER, true, 60),
    ("Keyboard", 3, false, 30),
];

/// Выдуманный мир помнит, что в него записали: кто подключён (вначале наушники и мышь); у кого выключено питание — подключение
/// не выходит (для проверки «не удалось подключить»), у «зависшего» — не выходит отключение; у кого нет звуковых точек —
/// звуковое устройство, но подключать нечем.
struct World {
    connected: [bool; FAKE_DEVICES.len()],
    unreachable: [bool; FAKE_DEVICES.len()],
    stuck: [bool; FAKE_DEVICES.len()],
    no_points: [bool; FAKE_DEVICES.len()],
}

const WORLD_START: World = World {
    connected: [true, true, false, false],
    unreachable: [false; FAKE_DEVICES.len()],
    stuck: [false; FAKE_DEVICES.len()],
    no_points: [false; FAKE_DEVICES.len()],
};

static WORLD: Mutex<World> = Mutex::new(WORLD_START);

fn fake_index(address: u64) -> Option<usize> {
    FAKE_DEVICES.iter().position(|d| d.1 == address)
}

/// Есть ли у выдуманного устройства звуковые точки (у адреса, которого в мире нет, их нет).
pub fn fake_has_points(address: u64) -> bool {
    fake_index(address).is_some_and(|i| !WORLD.lock().unwrap_or_else(|e| e.into_inner()).no_points[i])
}

/// Писатель выдуманного мира: подключить или отключить выдуманное устройство. Выключенное (`unreachable`) не подключается,
/// «зависшее» (`stuck`) не отключается.
pub fn fake_connect(address: u64, on: bool) {
    let Some(i) = fake_index(address) else { return };
    let mut world = WORLD.lock().unwrap_or_else(|e| e.into_inner());
    let ignores = if on { world.unreachable[i] } else { world.stuck[i] };
    if !ignores {
        world.connected[i] = on;
    }
}

/// Радио выключено — подключённых устройств нет: для снимка «Bluetooth выключен» (`ANTRESOL_VIEW=bluetooth-off` в демо).
pub fn fake_disconnect_all() {
    WORLD.lock().unwrap_or_else(|e| e.into_inner()).connected = [false; FAKE_DEVICES.len()];
}

/// Выдуманный мир как вначале, ворота открыты (для тестов).
#[cfg(test)]
pub fn fake_reset() {
    *WORLD.lock().unwrap_or_else(|e| e.into_inner()) = WORLD_START;
    fake_hold(false);
}

/// Ворота для тестов: пока закрыты, выдуманная просьба драйверу стоит на месте ([`fake_wait_hold`]) — тест видит «Подключение…»
/// и сам решает, когда драйвер «ответит», не гадая о времени.
#[cfg(test)]
static HOLD: (Mutex<bool>, std::sync::Condvar) = (Mutex::new(false), std::sync::Condvar::new());

/// Закрыть (`true`) или открыть ворота.
#[cfg(test)]
pub fn fake_hold(closed: bool) {
    *HOLD.0.lock().unwrap_or_else(|e| e.into_inner()) = closed;
    HOLD.1.notify_all();
}

/// Ждёт, пока ворота открыты, но не дольше 20 с: тест, что упал с закрытыми воротами, не должен вешать чужие потоки.
#[cfg(test)]
pub fn fake_wait_hold() {
    let start = std::time::Instant::now();
    let mut closed = HOLD.0.lock().unwrap_or_else(|e| e.into_inner());
    while *closed && start.elapsed() < std::time::Duration::from_secs(20) {
        closed = HOLD.1.wait_timeout(closed, std::time::Duration::from_millis(20)).unwrap_or_else(|e| e.into_inner()).0;
    }
}

/// Выдуманное устройство «выключено»: подключение принимается, но не выходит (для тестов).
#[cfg(test)]
pub fn fake_set_unreachable(address: u64, on: bool) {
    if let Some(i) = fake_index(address) {
        WORLD.lock().unwrap_or_else(|e| e.into_inner()).unreachable[i] = on;
    }
}

/// Выдуманное устройство «зависло»: отключение принимается, но не выходит (для тестов).
#[cfg(test)]
pub fn fake_set_stuck(address: u64, on: bool) {
    if let Some(i) = fake_index(address) {
        WORLD.lock().unwrap_or_else(|e| e.into_inner()).stuck[i] = on;
    }
}

/// У выдуманного устройства нет звуковых точек (для тестов).
#[cfg(test)]
pub fn fake_set_no_points(address: u64, on: bool) {
    if let Some(i) = fake_index(address) {
        WORLD.lock().unwrap_or_else(|e| e.into_inner()).no_points[i] = on;
    }
}

/// Сопряжённые устройства: подключённые — первыми. В демо и в тестах (`sys::fake_reads`) — выдуманные: настоящие
/// устройства этого ПК не читаются.
pub fn devices() -> Vec<Device> {
    if super::fake_reads() {
        let world = WORLD.lock().unwrap_or_else(|e| e.into_inner());
        return FAKE_DEVICES
            .iter()
            .enumerate()
            .map(|(i, &(name, address, audio, battery))| {
                let connected = world.connected[i];
                Device { name: name.into(), connected, battery: connected.then_some(battery), address, audio }
            })
            .collect();
    }
    let mut out = Vec::new();
    let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
        fReturnAuthenticated: BOOL(1),
        fReturnRemembered: BOOL(1),
        fReturnUnknown: BOOL(0),
        fReturnConnected: BOOL(1),
        fIssueInquiry: BOOL(0),
        cTimeoutMultiplier: 0,
        ..Default::default()
    };
    let mut info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
    unsafe {
        let Ok(find) = BluetoothFindFirstDevice(&params, &mut info) else { return out };
        loop {
            let end = info.szName.iter().position(|&c| c == 0).unwrap_or(info.szName.len());
            out.push(Device {
                name: String::from_utf16_lossy(&info.szName[..end]),
                connected: info.fConnected.as_bool(),
                battery: None,
                address: info.Address.Anonymous.ullLong,
                audio: (info.ulClassofDevice >> 8) & 0x1F == MAJOR_AUDIO,
            });
            info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
            if BluetoothFindNextDevice(find, &mut info).is_err() {
                break;
            }
        }
        let _ = BluetoothFindDeviceClose(find);
    }
    let charge = batteries();
    for d in &mut out {
        // У отключённого Windows хранит старое значение — его не показываем.
        d.battery = charge.iter().find(|(a, _)| *a == d.address && d.connected).map(|(_, b)| *b);
        if d.name.trim().is_empty() {
            d.name = format!("{:012X}", d.address);
        }
    }
    out.sort_by_key(|d| !d.connected);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Настоящий вызов Windows (только чтение): адаптера может не быть — тогда список пуст, но без паники. Не в общем
    /// прогоне — `cargo test -- --ignored`: в обычных тестах устройства этого ПК не читаются.
    #[test]
    #[ignore]
    fn reads_this_pc_without_crashing() {
        crate::sys::LIVE_READS.store(true, std::sync::atomic::Ordering::Relaxed);
        for d in devices() {
            assert!(d.battery.is_none_or(|b| b <= 100));
            // ContainerId читается у узлов устройства или его нет, но не падает и не бывает десятком.
            assert!(containers_of(d.address).len() <= 4);
        }
    }

    /// В тестах и демо читается выдуманный мир, а не устройства этого ПК.
    #[test]
    fn tests_and_the_demo_read_the_invented_devices() {
        let _world = super::super::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        super::super::reset_fake_world();
        assert!(crate::sys::fake_reads());
        let list = devices();
        assert_eq!(
            list.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["Headphones", "Mouse", "Speaker", "Keyboard"]
        );
        let states: Vec<_> = list.iter().map(|d| (d.audio, d.connected, d.battery)).collect();
        assert_eq!(
            states,
            [(true, true, Some(80)), (false, true, Some(45)), (true, false, None), (false, false, None)],
            "наушники подключены, мышь тоже, колонка (звуковая) нет, клавиатура (не звуковая) нет; у отключённого заряда нет"
        );
        assert_eq!(list[2].address, FAKE_SPEAKER);
    }

    /// Выдуманный мир помнит подключение; у выключенного устройства оно не выходит; сброс возвращает как вначале.
    #[test]
    fn the_invented_world_remembers_a_connection() {
        let _world = super::super::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        super::super::reset_fake_world();
        let speaker = || devices().into_iter().find(|d| d.address == FAKE_SPEAKER).unwrap();
        assert!(!speaker().connected && speaker().battery.is_none());
        fake_connect(FAKE_SPEAKER, true);
        assert_eq!((speaker().connected, speaker().battery), (true, Some(60)));
        fake_connect(FAKE_SPEAKER, false);
        assert!(!speaker().connected);
        fake_set_unreachable(FAKE_SPEAKER, true);
        fake_connect(FAKE_SPEAKER, true);
        assert!(!speaker().connected, "выключенное не подключается");
        fake_connect(2, false);
        assert!(!devices().iter().any(|d| d.address == 2 && d.connected), "а отключается без помех");
        // «Зависшее» не отключается, хотя подключается.
        fake_set_stuck(1, true);
        fake_connect(1, false);
        assert!(devices().iter().any(|d| d.address == 1 && d.connected), "наушники не отключились");
        assert!(fake_has_points(FAKE_SPEAKER) && !fake_has_points(0xdead));
        fake_set_no_points(FAKE_SPEAKER, true);
        assert!(!fake_has_points(FAKE_SPEAKER));
        super::super::reset_fake_world();
        assert!(devices().iter().all(|d| d.connected == (d.address <= 2)), "как вначале: подключены наушники и мышь");
        assert!(fake_has_points(FAKE_SPEAKER));
        assert_eq!(
            containers_of(FAKE_SPEAKER),
            [FAKE_SPEAKER as u128],
            "у выдуманного устройства контейнер — его адрес"
        );
    }

    #[test]
    fn addresses_from_ids() {
        let classic =
            r"BTHENUM\{0000111e-0000-1000-8000-00805f9b34fb}_LOCALMFG&0002\7&27e1d9e9&0&A0B1C2D3E4F5_C00000000";
        assert_eq!(address_in(classic), Some(0xA0B1C2D3E4F5));
        let le = r"BTHLEDEVICE\{0000180f-0000-1000-8000-00805f9b34fb}_DEV_VID&02045E_PID&0B13_REV&0509_112233445566\8&1a2b3c4d&0&0020";
        assert_eq!(address_in(le), Some(0x112233445566));
        assert_eq!(address_in(r"USB\VID_1234&PID_5678\0001"), None);
        assert_eq!(address_in(r"BTHENUM\{0000110b-0000-1000-8000-00805f9b34fb}_X"), None, "хвост UUID — не адрес");
    }
}
