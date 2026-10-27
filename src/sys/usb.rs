// Съёмные тома и безопасное извлечение (2.4, плашка «Флешка»). `volumes` — какие флешки и карты сейчас вставлены:
// `GetLogicalDrives` и `GetDriveTypeW == DRIVE_REMOVABLE`, без окна сообщений; «пустой» картридер (нет носителя) в список не
// попадает — плашка появится, когда вставят карту. `eject` — то же, что «Безопасное извлечение устройства» в трее: том →
// номер диска (`IOCTL_STORAGE_GET_DEVICE_NUMBER`) → узел диска в дереве устройств (`GUID_DEVINTERFACE_DISK`) → его родитель
// (`CM_Get_Parent`) → `CM_Request_Device_EjectW`; отказ системы (файл открыт, идёт копирование) — «диск занят».
// Внешние жёсткие диски USB система считает постоянными (`DRIVE_FIXED`) — плашки о них нет.
// В демо и в тестах (`sys::fake_reads`) мир выдуманный: список пуст, пока его не заполнят, извлечение — за заслоном
// `sys::blocked`, и настоящие флешки владельца не трогаются.

use std::ffi::c_void;
use std::sync::Mutex;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Parent, CM_Request_Device_EjectW, CR_REMOVE_VETOED, CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    PNP_VETO_TYPE, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW, GetDriveTypeW,
    GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING,
};
use windows::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode, THREAD_ERROR_MODE};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER};
use windows::core::PCWSTR;

use super::wide;

/// `DRIVE_REMOVABLE` у `GetDriveTypeW`.
const DRIVE_REMOVABLE: u32 = 2;

/// Съёмный том, в котором есть носитель.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// Корень: `E:\`.
    pub root: String,
    /// Метка тома; без метки — пусто.
    pub label: String,
    /// Размер, байт.
    pub total: u64,
}

impl Volume {
    /// Как назвать том словами: метка, а без метки — буква («E:»).
    pub fn name(&self) -> String {
        if self.label.trim().is_empty() { self.root.trim_end_matches('\\').to_string() } else { self.label.clone() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EjectError {
    /// Система отказала: диск занят (файл открыт, идёт копирование).
    Busy,
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
        let root = format!("{}:\\", (b'A' + i as u8) as char);
        let path = wide(&root);
        if unsafe { GetDriveTypeW(PCWSTR(path.as_ptr())) } != DRIVE_REMOVABLE {
            continue;
        }
        // Носитель есть, когда читается метка и размер.
        let mut label = [0u16; 261];
        let named = unsafe { GetVolumeInformationW(PCWSTR(path.as_ptr()), Some(&mut label), None, None, None, None) };
        let mut total = 0u64;
        let sized = unsafe { GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), None, Some(&mut total), None) };
        if named.is_err() || sized.is_err() {
            continue;
        }
        let end = label.iter().position(|&c| c == 0).unwrap_or(label.len());
        out.push(Volume { root, label: String::from_utf16_lossy(&label[..end]), total });
    }
    unsafe {
        let _ = SetThreadErrorMode(old, None);
    }
    out
}

/// Безопасное извлечение тома `root` (`E:\`). Звать из потока: система может ждать, пока диск сбросит запись.
pub fn eject(root: &str) -> Result<(), EjectError> {
    if super::blocked(&format!("usb::eject {root}")) {
        return fake_eject(root);
    }
    let letter = root.chars().next().filter(char::is_ascii_alphabetic);
    let letter = letter.ok_or_else(|| EjectError::Failed(format!("not a drive root: {root}")))?;
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
    match code {
        CR_SUCCESS => Ok(()),
        CR_REMOVE_VETOED => Err(EjectError::Busy),
        other => Err(EjectError::Failed(format!("CM_Request_Device_EjectW: {} (veto {})", other.0, veto.0))),
    }
}

/// Тип и номер устройства по пути (`\\.\E:` — том, `\\?\…` — интерфейс диска): `IOCTL_STORAGE_GET_DEVICE_NUMBER`.
fn device_number_of(path: PCWSTR) -> Option<(u32, u32)> {
    unsafe {
        // Доступ 0: устройству только задают вопрос, ни читать, ни писать не надо.
        let handle =
            CreateFileW(path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None)
                .ok()?;
        let mut number = STORAGE_DEVICE_NUMBER::default();
        let mut got = 0u32;
        let asked = DeviceIoControl(
            handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(&mut number as *mut STORAGE_DEVICE_NUMBER as *mut c_void),
            size_of::<STORAGE_DEVICE_NUMBER>() as u32,
            Some(&mut got),
            None,
        );
        let _ = CloseHandle(handle);
        asked.ok().map(|_| (number.DeviceType, number.DeviceNumber))
    }
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

/// Извлечение в выдуманном мире: «занято» по просьбе (`fake_busy`), иначе том просто пропадает из списка.
fn fake_eject(root: &str) -> Result<(), EjectError> {
    let same = |a: &str| a.eq_ignore_ascii_case(root);
    if FAKE_BUSY.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|r| same(r)) {
        return Err(EjectError::Busy);
    }
    FAKE.lock().unwrap_or_else(|e| e.into_inner()).retain(|v| !same(&v.root));
    Ok(())
}

/// Тест: что «вставлено» в выдуманном мире. Тесты с общим миром берут тома с корнями, что нигде больше не встречаются.
#[cfg(test)]
pub fn fake_set(list: Vec<Volume>) {
    *FAKE.lock().unwrap_or_else(|e| e.into_inner()) = list;
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
        Volume { root: root.into(), label: label.into(), total: 32 << 30 }
    }

    #[test]
    fn a_volume_is_named_by_its_label_and_without_one_by_its_letter() {
        assert_eq!(volume("E:\\", "DEMO").name(), "DEMO");
        assert_eq!(volume("E:\\", "").name(), "E:");
        assert_eq!(volume("F:\\", "  ").name(), "F:");
    }

    /// В тестах настоящие тома не читаются, а извлечение не доходит до Windows: «занято» — по просьбе, иначе том пропадает.
    #[test]
    fn the_invented_world_lists_what_it_was_given_and_ejects_behind_the_guard() {
        assert!(crate::sys::fake_reads());
        let (a, b) = (volume("Q:\\", "ONE"), volume("R:\\", "TWO"));
        fake_set(vec![a.clone(), b.clone()]);
        assert_eq!(volumes(), [a.clone(), b.clone()]);
        fake_busy("r:\\", true);
        assert_eq!(eject("R:\\"), Err(EjectError::Busy), "буква без учёта регистра");
        assert_eq!(volumes().len(), 2, "занятый остался");
        assert_eq!(eject("Q:\\"), Ok(()));
        assert_eq!(volumes(), [b]);
        fake_busy("R:\\", false);
        assert_eq!(eject("R:\\"), Ok(()));
        assert!(volumes().is_empty());
        let log = crate::sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner());
        assert!(log.iter().any(|w| w == "usb::eject Q:\\") && log.iter().any(|w| w == "usb::eject R:\\"));
        drop(log);
        fake_set(Vec::new());
    }

    /// Только чтение, ничего не извлекает: узел диска системного тома находится, и у него есть родитель. Не в общем
    /// прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn finds_the_disk_node_and_its_parent_of_the_system_volume() {
        let letter = std::env::var("SystemDrive").ok().and_then(|d| d.chars().next()).unwrap_or('C');
        let (kind, number) = volume_number(letter).expect("номер диска системного тома");
        let node = disk_node(kind, number).expect("узел диска");
        let mut parent = 0u32;
        assert_eq!(unsafe { CM_Get_Parent(&mut parent, node, 0) }, CR_SUCCESS);
        eprintln!("{letter}: тип {kind}, диск {number}, узел {node}, родитель {parent}");
        // Живое чтение съёмных томов тоже без окна «Вставьте диск».
        crate::sys::LIVE_READS.store(true, std::sync::atomic::Ordering::Relaxed);
        eprintln!("съёмных томов с носителем: {}", volumes().len());
    }
}
