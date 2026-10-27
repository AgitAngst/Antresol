// Съёмные тома и безопасное извлечение (2.4, плашка «Флешка»). `volumes` — какие флешки и карты сейчас вставлены:
// `GetLogicalDrives` и `GetDriveTypeW == DRIVE_REMOVABLE`, без окна сообщений; «пустой» картридер (нет носителя) в список не
// попадает — плашка появится, когда вставят карту; буква, что на деле папка (`SUBST`), — тоже не флешка. О каждом томе
// известно: серийный номер (другая флешка на той же букве — другой), диск (тома одной флешки делят его и получают одну
// плашку) и шина (SD и MMC — встроенный картридер: кнопки «Извлечь» у них нет). `eject` — то же, что «Безопасное извлечение
// устройства» в трее: том → номер диска (`IOCTL_STORAGE_GET_DEVICE_NUMBER`) → узел диска в дереве устройств
// (`GUID_DEVINTERFACE_DISK`) → его родитель (`CM_Get_Parent`) → `CM_Request_Device_EjectW`; успех — только `CR_SUCCESS` без
// отказа (`veto`), иначе «диск занят». Перед извлечением проверяется, что на букве всё ещё тот же съёмный том (иначе флешку
// успели вынуть, а букву занял другой диск).
// Внешние жёсткие диски USB система считает постоянными (`DRIVE_FIXED`) — плашки о них нет.
// В демо и в тестах (`sys::fake_reads`) мир выдуманный: список пуст, пока его не заполнят, извлечение — за заслоном
// `sys::blocked`, и настоящие флешки владельца не трогаются.

use std::ffi::c_void;
use std::sync::Mutex;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Parent, CM_Request_Device_EjectW, CONFIGRET, CR_REMOVE_VETOED, CR_SUCCESS, DIGCF_DEVICEINTERFACE,
    DIGCF_PRESENT, PNP_VETO_TYPE, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW, GetDriveTypeW,
    GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING, QueryDosDeviceW,
};
use windows::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode, THREAD_ERROR_MODE};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
    STORAGE_DEVICE_DESCRIPTOR, STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
};
use windows::core::PCWSTR;

use super::wide;

/// `DRIVE_REMOVABLE` у `GetDriveTypeW`.
const DRIVE_REMOVABLE: u32 = 2;
/// `STORAGE_BUS_TYPE`: SD и MMC — карта во встроенном картридере.
const BUS_SD: i32 = 12;
const BUS_MMC: i32 = 13;
/// `PNP_VetoTypeUnknown`: система не назвала отказа — извлечение прошло.
const VETO_NONE: i32 = 0;

/// Съёмный том, в котором есть носитель.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// Корень: `E:\`.
    pub root: String,
    /// Метка тома; без метки — пусто.
    pub label: String,
    /// Размер, байт.
    pub total: u64,
    /// Серийный номер тома: другая флешка на той же букве — другой.
    pub serial: u32,
    /// Диск (тип и номер устройства): тома одной флешки делят его. Не узнали — `None`, и том сам по себе.
    pub disk: Option<(u32, u32)>,
    /// Можно ли предложить «Извлечь»: у карты во встроенном картридере (шина SD и MMC) нельзя — родитель её узла диска сам
    /// контроллер, и «извлечение» убрало бы картридер до перезагрузки.
    pub can_eject: bool,
}

impl Volume {
    /// Как назвать том словами: метка, а без метки — буква («E:»).
    pub fn name(&self) -> String {
        if self.label.trim().is_empty() { self.root.trim_end_matches('\\').to_string() } else { self.label.clone() }
    }

    /// Тот же ли это том, что был: корень, серийный номер и размер. Метку не считаем — её переименовали, а флешка та же.
    pub fn same(&self, other: &Volume) -> bool {
        self.root.eq_ignore_ascii_case(&other.root) && self.serial == other.serial && self.total == other.total
    }

    /// Выдуманный том для образцов и тестов: серийный номер от буквы, отдельный диск, извлекается.
    pub fn invented(root: &str, label: &str, total: u64) -> Volume {
        let letter = root.bytes().next().map_or(0, |b| u32::from(b.to_ascii_uppercase()));
        Volume { root: root.into(), label: label.into(), total, serial: 0xA000 + letter, disk: None, can_eject: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EjectError {
    /// Система отказала: диск занят (файл открыт, идёт копирование).
    Busy,
    /// Такого тома уже нет: флешку вынули, а на её букве либо ничего, либо другой диск. Ничего не извлекалось.
    Gone,
    /// Не вышло по другой причине (не нашли устройство, нет прав): служебный текст — только в журнал.
    Failed(String),
}

/// Выдуманные тома (демо и тесты): пусто, пока тест не заполнит.
static FAKE: Mutex<Vec<Volume>> = Mutex::new(Vec::new());
/// Корни выдуманных томов, что откажутся извлекаться («занято»).
static FAKE_BUSY: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Съёмные тома с носителем, по буквам. Звать из потока: носитель может отвечать секундами.
pub fn volumes() -> Vec<Volume> {
    if super::fake_reads() {
        return FAKE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    }
    real_volumes()
}

/// Буква, что на деле папка (`SUBST E: C:\Флешка`): `QueryDosDeviceW` отдаёт ей цель вида `\??\C:\Флешка`, а настоящему тому —
/// `\Device\HarddiskVolumeN`.
fn is_substitute(target: &str) -> bool {
    target.starts_with(r"\??\")
}

/// Можно ли предложить «Извлечь» тому на шине `bus` (`STORAGE_BUS_TYPE`; не узнали — можно, как было).
fn bus_allows_eject(bus: Option<i32>) -> bool {
    !matches!(bus, Some(BUS_SD | BUS_MMC))
}

fn real_volumes() -> Vec<Volume> {
    // Пустой картридер или недоступный диск не должны вызывать системное окно «Вставьте диск».
    let mut old = THREAD_ERROR_MODE(0);
    unsafe {
        let _ = SetThreadErrorMode(SEM_FAILCRITICALERRORS, Some(&mut old));
    }
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    // С C: — буквы A: и B: отданы дисководам.
    for i in 2..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let path = wide(&root);
        if unsafe { GetDriveTypeW(PCWSTR(path.as_ptr())) } != DRIVE_REMOVABLE {
            continue;
        }
        // Носитель есть, когда читается метка, серийный номер и размер.
        let mut label = [0u16; 261];
        let mut serial = 0u32;
        let named = unsafe {
            GetVolumeInformationW(PCWSTR(path.as_ptr()), Some(&mut label), Some(&mut serial), None, None, None)
        };
        let mut total = 0u64;
        let sized = unsafe { GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), None, Some(&mut total), None) };
        if named.is_err() || sized.is_err() || is_substituted(letter) {
            continue;
        }
        let facts = facts_of(letter);
        let end = label.iter().position(|&c| c == 0).unwrap_or(label.len());
        out.push(Volume {
            root,
            label: String::from_utf16_lossy(&label[..end]),
            total,
            serial,
            disk: facts.disk,
            can_eject: bus_allows_eject(facts.bus),
        });
    }
    unsafe {
        let _ = SetThreadErrorMode(old, None);
    }
    out
}

/// `SUBST`-буква ли `letter`.
fn is_substituted(letter: char) -> bool {
    let name = wide(&format!("{}:", letter.to_ascii_uppercase()));
    let mut target = [0u16; 520];
    let got = unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut target)) };
    if got == 0 {
        return false;
    }
    // Ответ — несколько строк подряд; текущее отображение — первая.
    let end = target.iter().position(|&c| c == 0).unwrap_or(target.len());
    is_substitute(&String::from_utf16_lossy(&target[..end]))
}

/// Тот ли ещё на `root` съёмный том с серийным номером `serial`, что был на плашке.
fn still_there(root: &str, serial: u32) -> bool {
    let mut old = THREAD_ERROR_MODE(0);
    unsafe {
        let _ = SetThreadErrorMode(SEM_FAILCRITICALERRORS, Some(&mut old));
    }
    let path = wide(root);
    let removable = unsafe { GetDriveTypeW(PCWSTR(path.as_ptr())) } == DRIVE_REMOVABLE;
    let mut now = 0u32;
    let read = unsafe { GetVolumeInformationW(PCWSTR(path.as_ptr()), None, Some(&mut now), None, None, None) };
    unsafe {
        let _ = SetThreadErrorMode(old, None);
    }
    removable && read.is_ok() && now == serial
}

/// Что ответила система на просьбу об извлечении: успех — только `CR_SUCCESS` без отказа (`veto`). Успех с названным отказом —
/// не «можно вынимать»: ложное «можно» стоило бы данных на флешке.
fn verdict(code: CONFIGRET, veto: i32) -> Result<(), EjectError> {
    match code {
        CR_SUCCESS if veto == VETO_NONE => Ok(()),
        CR_SUCCESS | CR_REMOVE_VETOED => Err(EjectError::Busy),
        other => Err(EjectError::Failed(format!("CM_Request_Device_EjectW: {} (veto {veto})", other.0))),
    }
}

/// Безопасное извлечение тома `root` (`E:\`) с серийным номером `serial` — тем, что был на плашке. Звать из потока: система
/// может ждать, пока диск сбросит запись.
pub fn eject(root: &str, serial: u32) -> Result<(), EjectError> {
    if super::blocked(&format!("usb::eject {root}")) {
        return fake_eject(root, serial);
    }
    let letter = root.chars().next().filter(char::is_ascii_alphabetic);
    let letter = letter.ok_or_else(|| EjectError::Failed(format!("not a drive root: {root}")))?;
    // Плашка могла провисеть, пока флешку вынули, а её букву занял другой диск: извлечётся он.
    if !still_there(root, serial) {
        return Err(EjectError::Gone);
    }
    let (kind, number) =
        volume_number(letter).ok_or_else(|| EjectError::Failed(format!("no device number: {letter}:")))?;
    let disk = disk_node(kind, number).ok_or_else(|| EjectError::Failed(format!("no disk node: {letter}:")))?;
    let mut parent = 0u32;
    let got = unsafe { CM_Get_Parent(&mut parent, disk, 0) };
    if got != CR_SUCCESS {
        return Err(EjectError::Failed(format!("CM_Get_Parent: {}", got.0)));
    }
    let mut veto = PNP_VETO_TYPE(0);
    let mut name = [0u16; 260];
    super::real_write("CM_Request_Device_EjectW");
    let code = unsafe { CM_Request_Device_EjectW(parent, Some(&mut veto), Some(&mut name), 0) };
    verdict(code, veto.0)
}

/// Что известно о диске тома.
struct Facts {
    /// Тип и номер устройства.
    disk: Option<(u32, u32)>,
    /// `STORAGE_BUS_TYPE`.
    bus: Option<i32>,
}

/// Открыть устройство для вопросов: доступ 0 — ни читать, ни писать не надо.
fn open_for_questions(path: PCWSTR) -> Option<HANDLE> {
    unsafe {
        CreateFileW(path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None).ok()
    }
}

/// Тип и номер устройства по открытой ручке: `IOCTL_STORAGE_GET_DEVICE_NUMBER`.
fn number_of(handle: HANDLE) -> Option<(u32, u32)> {
    let mut number = STORAGE_DEVICE_NUMBER::default();
    let mut got = 0u32;
    let asked = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(&mut number as *mut STORAGE_DEVICE_NUMBER as *mut c_void),
            size_of::<STORAGE_DEVICE_NUMBER>() as u32,
            Some(&mut got),
            None,
        )
    };
    asked.ok().map(|_| (number.DeviceType, number.DeviceNumber))
}

/// Шина устройства по открытой ручке: `IOCTL_STORAGE_QUERY_PROPERTY` (`StorageDeviceProperty`) → `BusType`.
fn bus_of(handle: HANDLE) -> Option<i32> {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    // Буфер из `u64`: выровнен под описание устройства; за ним могут идти строки (производитель, модель).
    let mut buffer = vec![0u64; 128];
    let mut got = 0u32;
    let asked = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const STORAGE_PROPERTY_QUERY as *const c_void),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(buffer.as_mut_ptr() as *mut c_void),
            (buffer.len() * size_of::<u64>()) as u32,
            Some(&mut got),
            None,
        )
    };
    let needed = std::mem::offset_of!(STORAGE_DEVICE_DESCRIPTOR, BusType) + size_of::<i32>();
    if asked.is_err() || (got as usize) < needed {
        return None;
    }
    let descriptor = unsafe { &*(buffer.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
    Some(descriptor.BusType.0)
}

/// Тип и номер устройства по пути (`\\.\E:` — том, `\\?\…` — интерфейс диска).
fn device_number_of(path: PCWSTR) -> Option<(u32, u32)> {
    let handle = open_for_questions(path)?;
    let number = number_of(handle);
    unsafe {
        let _ = CloseHandle(handle);
    }
    number
}

/// Диск и шина тома с буквой `letter`: одна ручка на оба вопроса.
fn facts_of(letter: char) -> Facts {
    let path = wide(&format!("\\\\.\\{}:", letter.to_ascii_uppercase()));
    let Some(handle) = open_for_questions(PCWSTR(path.as_ptr())) else { return Facts { disk: None, bus: None } };
    let facts = Facts { disk: number_of(handle), bus: bus_of(handle) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    facts
}

/// Тип и номер диска, на котором лежит том с буквой `letter`.
fn volume_number(letter: char) -> Option<(u32, u32)> {
    let path = wide(&format!("\\\\.\\{}:", letter.to_ascii_uppercase()));
    device_number_of(PCWSTR(path.as_ptr()))
}

/// Узел диска в дереве устройств (`DEVINST`) с таким типом и номером: перебор интерфейсов дисков.
fn disk_node(kind: u32, number: u32) -> Option<u32> {
    unsafe {
        let set = SetupDiGetClassDevsW(
            Some(&GUID_DEVINTERFACE_DISK),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
        .ok()?;
        let mut found = None;
        let mut index = 0u32;
        loop {
            let mut data =
                SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInterfaces(set, None, &GUID_DEVINTERFACE_DISK, index, &mut data).is_err() {
                break;
            }
            index += 1;
            // Сначала узнаём, сколько нужно под путь: этот вызов «не удаётся», но называет размер.
            let mut need = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(set, &data, None, 0, Some(&mut need), None);
            if (need as usize) < size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() {
                continue;
            }
            // Буфер из `u64`: выровнен под структуру, вмещает путь после `cbSize`.
            let mut buffer = vec![0u64; (need as usize).div_ceil(8)];
            let detail = buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if SetupDiGetDeviceInterfaceDetailW(set, &data, Some(detail), need, None, Some(&mut info)).is_err() {
                continue;
            }
            let path = PCWSTR(std::ptr::addr_of!((*detail).DevicePath) as *const u16);
            if device_number_of(path) == Some((kind, number)) {
                found = Some(info.DevInst);
                break;
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
        found
    }
}

/// Извлечение в выдуманном мире: тома с таким корнем и серийным номером нет — «Флешку уже вынули»; «занято» по просьбе
/// (`fake_busy`); иначе том просто пропадает из списка.
fn fake_eject(root: &str, serial: u32) -> Result<(), EjectError> {
    let same = |a: &str| a.eq_ignore_ascii_case(root);
    let there = FAKE.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|v| same(&v.root) && v.serial == serial);
    if !there {
        return Err(EjectError::Gone);
    }
    if FAKE_BUSY.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|r| same(r)) {
        return Err(EjectError::Busy);
    }
    FAKE.lock().unwrap_or_else(|e| e.into_inner()).retain(|v| !same(&v.root));
    Ok(())
}

/// Тест: «вставить» том в выдуманный мир. Тесты берут тома с корнями, что нигде больше не встречаются (мир один на все
/// тесты, и тома друг друга не должны задевать). Тот же корень заменяется.
#[cfg(test)]
pub fn fake_add(volume: Volume) {
    let mut list = FAKE.lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|v| !v.root.eq_ignore_ascii_case(&volume.root));
    list.push(volume);
}

/// Тест: «вынуть» том из выдуманного мира.
#[cfg(test)]
pub fn fake_remove(root: &str) {
    FAKE.lock().unwrap_or_else(|e| e.into_inner()).retain(|v| !v.root.eq_ignore_ascii_case(root));
}

/// Тест: этот том откажется извлекаться («занят»).
#[cfg(test)]
pub fn fake_busy(root: &str, busy: bool) {
    let mut list = FAKE_BUSY.lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|r| !r.eq_ignore_ascii_case(root));
    if busy {
        list.push(root.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(root: &str, label: &str) -> Volume {
        Volume::invented(root, label, 32 << 30)
    }

    #[test]
    fn a_volume_is_named_by_its_label_and_without_one_by_its_letter() {
        assert_eq!(volume("E:\\", "DEMO").name(), "DEMO");
        assert_eq!(volume("E:\\", "").name(), "E:");
        assert_eq!(volume("F:\\", "  ").name(), "F:");
    }

    /// Тот же том — тот же корень, серийный номер и размер; метку переименовали — флешка та же, другой номер — другая.
    #[test]
    fn the_same_volume_is_told_by_letter_serial_and_size() {
        let a = volume("E:\\", "DEMO");
        assert!(a.same(&volume("e:\\", "RENAMED")), "метка и регистр буквы не в счёт");
        assert!(!a.same(&Volume { serial: a.serial + 1, ..a.clone() }), "другая флешка на той же букве");
        assert!(!a.same(&Volume { total: a.total / 2, ..a.clone() }));
        assert!(!a.same(&volume("F:\\", "DEMO")));
    }

    /// Мир выдуманный: тома, что в него вставили; извлечение за заслоном — «занято» по просьбе, нет такого или не тот номер —
    /// «уже вынули», иначе том пропадает.
    #[test]
    fn the_invented_world_lists_what_it_was_given_and_ejects_behind_the_guard() {
        assert!(crate::sys::fake_reads());
        let (a, b) = (volume("Q:\\", "ONE"), volume("R:\\", "TWO"));
        fake_add(a.clone());
        fake_add(b.clone());
        let mine = || volumes().into_iter().filter(|v| v.root == "Q:\\" || v.root == "R:\\").collect::<Vec<_>>();
        assert_eq!(mine(), [a.clone(), b.clone()]);
        fake_busy("r:\\", true);
        assert_eq!(eject("R:\\", b.serial), Err(EjectError::Busy), "буква без учёта регистра");
        assert_eq!(mine().len(), 2, "занятый остался");
        // Другой серийный номер на той же букве — это уже не та флешка: ничего не извлекается.
        assert_eq!(eject("Q:\\", a.serial + 1), Err(EjectError::Gone));
        assert_eq!(mine().len(), 2, "чужой номер ничего не задел");
        assert_eq!(eject("Q:\\", a.serial), Ok(()));
        assert_eq!(mine(), std::slice::from_ref(&b));
        assert_eq!(eject("Q:\\", a.serial), Err(EjectError::Gone), "второй раз — уже вынули");
        fake_busy("R:\\", false);
        assert_eq!(eject("R:\\", b.serial), Ok(()));
        assert!(mine().is_empty());
        let log = crate::sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner());
        assert!(log.iter().any(|w| w == "usb::eject Q:\\") && log.iter().any(|w| w == "usb::eject R:\\"));
    }

    #[test]
    fn success_is_only_success_without_a_veto() {
        assert_eq!(verdict(CR_SUCCESS, VETO_NONE), Ok(()));
        // Система ответила «готово», но назвала отказ — вынимать нельзя.
        assert_eq!(verdict(CR_SUCCESS, 7), Err(EjectError::Busy));
        assert_eq!(verdict(CR_REMOVE_VETOED, 7), Err(EjectError::Busy));
        assert_eq!(verdict(CR_REMOVE_VETOED, VETO_NONE), Err(EjectError::Busy));
        let other = verdict(CONFIGRET(0x16), 0);
        assert!(matches!(&other, Err(EjectError::Failed(text)) if text.contains("22")), "{other:?}");
    }

    #[test]
    fn a_card_in_a_built_in_reader_gets_no_eject_and_a_usb_drive_does() {
        assert!(!bus_allows_eject(Some(BUS_SD)) && !bus_allows_eject(Some(BUS_MMC)));
        assert!(bus_allows_eject(Some(7)), "USB");
        assert!(bus_allows_eject(Some(1)), "прочие шины — как было");
        assert!(bus_allows_eject(None), "не узнали — как было");
    }

    #[test]
    fn a_letter_that_is_really_a_folder_is_not_a_drive() {
        assert!(is_substitute(r"\??\C:\Users\Demo\Flash"));
        assert!(!is_substitute(r"\Device\HarddiskVolume5"));
        assert!(!is_substitute(""));
    }

    /// Только чтение, ничего не извлекает: узел диска системного тома находится, и у него есть родитель, а шина и номер
    /// диска читаются. Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn finds_the_disk_node_and_its_parent_of_the_system_volume() {
        let letter = std::env::var("SystemDrive").ok().and_then(|d| d.chars().next()).unwrap_or('C');
        let (kind, number) = volume_number(letter).expect("номер диска системного тома");
        let node = disk_node(kind, number).expect("узел диска");
        let mut parent = 0u32;
        assert_eq!(unsafe { CM_Get_Parent(&mut parent, node, 0) }, CR_SUCCESS);
        let facts = facts_of(letter);
        assert_eq!(facts.disk, Some((kind, number)));
        eprintln!("{letter}: тип {kind}, диск {number}, узел {node}, родитель {parent}, шина {:?}", facts.bus);
        assert!(facts.bus.is_some(), "шина системного диска читается");
        assert!(!is_substituted(letter), "системный диск — не SUBST");
        // Живое чтение съёмных томов тоже без окна «Вставьте диск».
        crate::sys::LIVE_READS.store(true, std::sync::atomic::Ordering::Relaxed);
        eprintln!("съёмных томов с носителем: {}", volumes().len());
    }
}
