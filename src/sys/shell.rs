// Оболочка Windows для плашек (2.4): папка «Загрузки», перетаскивание файла с плашки в чат или папку, «Показать в папке»,
// опасные типы файлов, пробное открытие готового файла и «можно ли сейчас показывать уведомления».
// В демо и в тестах (`sys::isolated`) папки «Загрузки» этого ПК нет вовсе (`None`) — тесты берут свою временную папку, а
// перетаскивание и проводник — за заслоном `sys::blocked`; опасными считаются типы по короткому списку, а не по реестру этого
// ПК; показывать уведомления можно всегда.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IDataObject};
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, IDropSource};
use windows::Win32::UI::Shell::{
    AssocIsDangerous, BHID_DataObject, FOLDERID_Downloads, IShellItem, KF_FLAG_DEFAULT, QUNS_ACCEPTS_NOTIFICATIONS,
    QUNS_APP, QUNS_QUIET_TIME, SHCreateItemFromParsingName, SHDoDragDrop, SHGetKnownFolderPath,
    SHQueryUserNotificationState,
};
use windows::core::PCWSTR;

/// Папка «Загрузки» пользователя (её можно перенести — спрашиваем у Windows каждый раз). Не нашли — `None`.
pub fn downloads_dir() -> Option<PathBuf> {
    if super::isolated() {
        return None;
    }
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const std::ffi::c_void));
        text.filter(|t| !t.is_empty()).map(PathBuf::from)
    }
}

/// Расширения, что при открытии запускают программу, ставят пакет или подключают образ, но `AssocIsDangerous` их опасными не
/// называет (в Windows 10 их набор короче): свой список поверх системного.
const ALSO_DANGEROUS: [&str; 13] = [
    ".appref-ms",
    ".application",
    ".iso",
    ".img",
    ".vhd",
    ".vhdx",
    ".ps1",
    ".jar",
    ".py",
    ".msix",
    ".appx",
    ".appinstaller",
    ".settingcontent-ms",
];

/// Что Windows называет опасным всегда; в выдуманном мире (демо и тесты) реестр этого ПК не спрашивают, а берут этот список.
const CLASSIC_DANGEROUS: [&str; 20] = [
    ".exe", ".com", ".scr", ".pif", ".bat", ".cmd", ".msi", ".vbs", ".vbe", ".js", ".jse", ".wsf", ".wsh", ".hta",
    ".lnk", ".url", ".reg", ".cpl", ".msc", ".chm",
];

/// Расширение файла со строчными буквами и точкой (`.pdf`); у имени без расширения (и у «.gitignore») — `None`.
pub fn extension(name: &OsStr) -> Option<String> {
    let name = name.to_string_lossy();
    let at = name.rfind('.').filter(|&at| at > 0)?;
    Some(name[at..].to_lowercase())
}

/// Запускается ли файл при открытии двойным щелчком (программа, скрипт, установщик, ярлык, образ): тогда плашка не предлагает
/// «Открыть» — файл только что пришёл из сети, и случайный щелчок у края не должен его запускать. Смотрим по расширению:
/// `AssocIsDangerous` (список Windows) и свой короткий список поверх. Содержимое файла не читается.
pub fn is_dangerous(name: &OsStr) -> bool {
    let Some(ext) = extension(name) else { return false };
    if ALSO_DANGEROUS.contains(&ext.as_str()) {
        return true;
    }
    if super::fake_reads() {
        return CLASSIC_DANGEROUS.contains(&ext.as_str());
    }
    let wide = super::wide(&ext);
    unsafe { AssocIsDangerous(PCWSTR(wide.as_ptr())).as_bool() }
}

/// Показать файл в проводнике (`explorer /select,"путь"`): окно папки с выделенным файлом. Путь — как есть, без перевода в
/// текст.
pub fn reveal(path: &Path) -> Result<(), String> {
    let mut args = OsString::from("/select,\"");
    args.push(path);
    args.push("\"");
    super::launch_os(OsStr::new("explorer.exe"), &args)
}

/// Ответ пробного открытия файла.
#[derive(Debug, PartialEq, Eq)]
pub enum Probe {
    /// Файл ещё пишет другая программа (или держит его закрытым для чтения): не готов.
    Busy,
    /// Файла уже нет.
    Gone,
    /// Открыть не вышло по другой причине (нет прав): судить нечем.
    Unknown,
    /// Открылся: свежие размер и время правки — у самого файла, а не из записи каталога, которую NTFS обновляет не сразу.
    Ready { len: u64, modified: Option<SystemTime> },
}

/// Пробное открытие готового с виду файла: на чтение, но без права писать другим (`FILE_SHARE_WRITE` не даём) — файл, что
/// кто-то ещё пишет, не открывается («занят другим процессом»). Удалять и переименовывать другим разрешено
/// (`FILE_SHARE_DELETE`): браузер переименовывает `.crdownload` в настоящее имя, и наша ручка ему не помеха. Ручка закрывается
/// сразу.
pub fn probe_file(path: &Path) -> Probe {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_DELETE: u32 = 0x4;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PATH_NOT_FOUND: i32 = 3;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    let opened = std::fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE).open(path);
    match opened {
        Ok(file) => match file.metadata() {
            Ok(meta) => Probe::Ready { len: meta.len(), modified: meta.modified().ok() },
            Err(_) => Probe::Unknown,
        },
        Err(e) => match e.raw_os_error() {
            Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION) => Probe::Busy,
            Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => Probe::Gone,
            _ => Probe::Unknown,
        },
    }
}

/// Можно ли сейчас показывать уведомления: пользователь за обычной работой (`QUNS_ACCEPTS_NOTIFICATIONS`); первый час нового
/// пользователя Windows («тихое время», `QUNS_QUIET_TIME`) и приложение из магазина на экране (`QUNS_APP`) плашкам не помеха.
/// Показ экрана, игра во весь экран, «не беспокоить» и отлучка — нет; не удалось спросить — можно. В выдуманном мире всегда
/// можно.
pub fn accepts_notifications() -> bool {
    if super::fake_reads() {
        return true;
    }
    unsafe {
        SHQueryUserNotificationState()
            .map(|state| state == QUNS_ACCEPTS_NOTIFICATIONS || state == QUNS_QUIET_TIME || state == QUNS_APP)
            .unwrap_or(true)
    }
}

/// Данные файла для перетаскивания — те же, что у файла в проводнике (список файлов `CF_HDROP` и список объектов оболочки).
fn file_data_object(path: &Path) -> Result<IDataObject, String> {
    let wide = super::wide_os(path.as_os_str());
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>)
            .map_err(|e| format!("SHCreateItemFromParsingName: {e}"))?;
        item.BindToHandler(None::<&IBindCtx>, &BHID_DataObject).map_err(|e| format!("BindToHandler: {e}"))
    }
}

/// Потянуть файл из окна панели: обычное перетаскивание Windows (`SHDoDragDrop`) с файлом, как будто его тянут из проводника.
/// Копировать или ссылкой — переносить (удалять у нас) нельзя. Возвращается, когда кнопку отпустили или отменили (Esc).
pub fn drag_file(path: &Path) -> Result<(), String> {
    if super::blocked(&format!("drag_file {}", path.display())) {
        return Ok(());
    }
    let data = file_data_object(path)?;
    super::real_write("SHDoDragDrop");
    unsafe {
        SHDoDragDrop(super::own_window(), &data, None::<&IDropSource>, DROPEFFECT_COPY | DROPEFFECT_LINK)
            .map(|_| ())
            .map_err(|e| format!("SHDoDragDrop: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Своя папка теста — на диске репозитория (E:), а не в системной временной.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("plaque-test")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// В тестах и демо папка «Загрузки» этого ПК не отдаётся — её не прочитать нечаянно.
    #[test]
    fn tests_and_the_demo_never_see_the_real_downloads_folder() {
        assert!(crate::sys::isolated());
        assert_eq!(downloads_dir(), None);
    }

    #[test]
    fn the_extension_is_the_last_one_in_lower_case() {
        let ext = |name: &str| extension(OsStr::new(name));
        assert_eq!(ext("report.PDF").as_deref(), Some(".pdf"));
        assert_eq!(ext("archive.tar.gz").as_deref(), Some(".gz"));
        assert_eq!(ext("invoice.pdf .exe").as_deref(), Some(".exe"), "после пробела — это всё равно .exe");
        assert_eq!(ext("noext"), None);
        assert_eq!(ext(".gitignore"), None, "точка в начале — не расширение");
    }

    /// Файл, что при открытии запускается, — опасный: по списку Windows (в выдуманном мире — по короткому списку) и по своему
    /// списку поверх него.
    #[test]
    fn files_that_run_when_opened_are_dangerous_and_documents_are_not() {
        assert!(crate::sys::fake_reads());
        for name in [
            "setup.exe",
            "SETUP.EXE",
            "a.bat",
            "a.cmd",
            "a.msi",
            "a.vbs",
            "a.js",
            "a.lnk",
            "a.url",
            "a.reg",
            "a.hta",
            "a.scr",
            "invoice.pdf .exe",
            "a.appref-ms",
            "a.application",
            "a.iso",
            "a.img",
            "a.vhd",
            "a.vhdx",
            "a.ps1",
            "a.jar",
            "a.py",
            "a.msix",
            "a.appx",
            "a.appinstaller",
            "a.settingcontent-ms",
        ] {
            assert!(is_dangerous(OsStr::new(name)), "{name}");
        }
        for name in
            ["report.pdf", "photo.JPG", "notes.txt", "archive.zip", "song.mp3", "noext", ".gitignore", "a.exe.txt"]
        {
            assert!(!is_dangerous(OsStr::new(name)), "{name}");
        }
    }

    /// Спрашивает настоящую Windows (`AssocIsDangerous`), ничего не меняет. Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn windows_calls_programs_and_scripts_dangerous_and_documents_not() {
        crate::sys::LIVE_READS.store(true, std::sync::atomic::Ordering::Relaxed);
        for name in ["a.exe", "a.bat", "a.vbs", "a.msi"] {
            assert!(is_dangerous(OsStr::new(name)), "{name}");
        }
        for name in ["a.pdf", "a.txt", "a.png"] {
            assert!(!is_dangerous(OsStr::new(name)), "{name}");
        }
        eprintln!(
            "уведомления можно показывать: {} (ответ Windows: {:?})",
            accepts_notifications(),
            unsafe { SHQueryUserNotificationState() }
        );
    }

    /// Готовый файл открывается пробой и отдаёт свежие размер и время; файл, что пишут (открыт на запись), — занят; нет
    /// файла — нет файла. Закрыли запись — снова готов. Папка теста — своя, на E:.
    #[test]
    fn a_file_somebody_still_writes_is_busy_and_a_closed_one_is_ready() {
        use std::io::Write;
        let dir = scratch("probe");
        let path = dir.join("report.pdf");
        std::fs::File::create(&path).unwrap().write_all(&[0u8; 1234]).unwrap();
        let Probe::Ready { len, modified } = probe_file(&path) else { panic!("{:?}", probe_file(&path)) };
        assert_eq!(len, 1234);
        assert!(modified.is_some());
        // Пишущий держит файл открытым: проба не проходит, хотя в каталоге он уже виден с размером.
        let mut writer = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writer.write_all(b"more").unwrap();
        assert_eq!(probe_file(&path), Probe::Busy);
        drop(writer);
        let Probe::Ready { len, .. } = probe_file(&path) else { panic!() };
        assert_eq!(len, 1238, "свежий размер — после записи");
        // Проба не мешает ни переименовать, ни удалить файл — браузер так и делает.
        std::fs::rename(&path, dir.join("final.pdf")).unwrap();
        assert_eq!(probe_file(&path), Probe::Gone);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Ручка пробы не мешает браузеру: пока она открыта, файл переименовывается и удаляется.
    #[test]
    fn the_probe_handle_does_not_block_a_rename_or_a_delete() {
        use std::io::Write;
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("probe-share");
        let path = dir.join("a.zip");
        std::fs::File::create(&path).unwrap().write_all(b"zip").unwrap();
        // Та же ручка, что у пробы, но открытая на время проверки.
        let held = std::fs::OpenOptions::new().read(true).share_mode(0x1 | 0x4).open(&path).unwrap();
        std::fs::rename(&path, dir.join("b.zip")).expect("переименовать при открытой пробе можно");
        std::fs::remove_file(dir.join("b.zip")).expect("удалить при открытой пробе можно");
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Только чтение, без перетаскивания: оболочка отдаёт для файла данные с `CF_HDROP` — то, что принимают чат, проводник и
    /// рабочий стол. Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn the_shell_gives_a_file_list_for_a_file() {
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL,
        };
        use windows::Win32::System::Ole::CF_HDROP;
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let data =
            file_data_object(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), r"\Cargo.toml"))).expect("данные файла");
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        assert!(
            unsafe { data.QueryGetData(&format) }.is_ok(),
            "у файла нет списка файлов для перетаскивания (CF_HDROP)"
        );
        unsafe { CoUninitialize() };
    }

    /// Перетаскивание и проводник не доходят до Windows: заслон отвечает «готово» и оставляет метку.
    #[test]
    fn dragging_and_the_explorer_stop_at_the_guard() {
        assert_eq!(drag_file(Path::new(r"C:\shell-test\drag-4471.pdf")), Ok(()));
        assert_eq!(reveal(Path::new(r"C:\shell-test\reveal-4471.exe")), Ok(()));
        let log = crate::sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner());
        assert!(log.iter().any(|w| w == r"drag_file C:\shell-test\drag-4471.pdf"));
        assert!(log.iter().any(|w| w == r#"launch explorer.exe /select,"C:\shell-test\reveal-4471.exe""#));
    }
}
