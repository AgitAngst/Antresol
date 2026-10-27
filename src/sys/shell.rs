// Оболочка Windows для плашек (2.4): папка «Загрузки» и перетаскивание файла с плашки в чат или папку.
// В демо и в тестах (`sys::isolated`) папки «Загрузки» этого ПК нет вовсе (`None`) — тесты берут свою временную папку, а
// перетаскивание — за заслоном `sys::blocked`.

use std::path::PathBuf;

use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IDataObject};
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, IDropSource};
use windows::Win32::UI::Shell::{
    BHID_DataObject, FOLDERID_Downloads, IShellItem, KF_FLAG_DEFAULT, SHCreateItemFromParsingName, SHDoDragDrop,
    SHGetKnownFolderPath,
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

/// Данные файла для перетаскивания — те же, что у файла в проводнике (список файлов `CF_HDROP` и список объектов оболочки).
fn file_data_object(path: &str) -> Result<IDataObject, String> {
    let wide = super::wide(path);
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>)
            .map_err(|e| format!("SHCreateItemFromParsingName: {e}"))?;
        item.BindToHandler(None::<&IBindCtx>, &BHID_DataObject).map_err(|e| format!("BindToHandler: {e}"))
    }
}

/// Потянуть файл из окна панели: обычное перетаскивание Windows (`SHDoDragDrop`) с файлом, как будто его тянут из проводника.
/// Копировать или ссылкой — переносить (удалять у нас) нельзя. Возвращается, когда кнопку отпустили или отменили (Esc).
pub fn drag_file(path: &str) -> Result<(), String> {
    if super::blocked(&format!("drag_file {path}")) {
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

    /// В тестах и демо папка «Загрузки» этого ПК не отдаётся — её не прочитать нечаянно.
    #[test]
    fn tests_and_the_demo_never_see_the_real_downloads_folder() {
        assert!(crate::sys::isolated());
        assert_eq!(downloads_dir(), None);
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
        let data = file_data_object(concat!(env!("CARGO_MANIFEST_DIR"), r"\Cargo.toml")).expect("данные файла");
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

    /// Перетаскивание не доходит до Windows: заслон отвечает «готово» и оставляет метку.
    #[test]
    fn dragging_stops_at_the_guard() {
        assert_eq!(drag_file(r"C:\shell-test\drag-4471.pdf"), Ok(()));
        let log = crate::sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner());
        assert!(log.iter().any(|w| w == r"drag_file C:\shell-test\drag-4471.pdf"));
    }
}
