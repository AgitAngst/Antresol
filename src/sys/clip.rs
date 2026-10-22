// Буфер обмена: узнать, что скопировали (только текст), и положить текст обратно.
// Новое копирование видно по номеру GetClipboardSequenceNumber — его опрос ничего не стоит.
// Менеджеры паролей помечают свои копии форматами «не запоминать» — такие пропускаем.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::core::w;

pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Копия помечена «не запоминать»: менеджеры паролей, «Ножницы» с приватным содержимым.
fn private_copy() -> bool {
    unsafe {
        let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        let history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
        if IsClipboardFormatAvailable(exclude).is_ok() {
            return true;
        }
        // «CanIncludeInClipboardHistory» = 0 — тоже не запоминать.
        if IsClipboardFormatAvailable(history).is_ok()
            && let Ok(h) = GetClipboardData(history)
        {
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u32;
            let deny = !p.is_null() && *p == 0;
            let _ = GlobalUnlock(g);
            return deny;
        }
        false
    }
}

/// Текст из буфера: `Ok(None)` — там не текст или копия приватная; `Err` — буфер занят другой
/// программой (прочитать ещё раз позже).
pub fn read_text() -> Result<Option<String>, ()> {
    unsafe {
        OpenClipboard(None).map_err(|_| ())?;
        let text = (|| {
            if private_copy() {
                return None;
            }
            IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).ok()?;
            let h = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if p.is_null() {
                return None;
            }
            let max = GlobalSize(g) / 2;
            let s = std::slice::from_raw_parts(p, max);
            let end = s.iter().position(|&c| c == 0).unwrap_or(max);
            let text = String::from_utf16_lossy(&s[..end]);
            let _ = GlobalUnlock(g);
            Some(text)
        })();
        let _ = CloseClipboard();
        Ok(text)
    }
}

/// Положить текст в буфер (как «Копировать»).
pub fn write_text(text: &str) -> Result<(), String> {
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let r = (|| {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let g = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).map_err(|e| e.to_string())?;
            let p = GlobalLock(g) as *mut u16;
            if p.is_null() {
                let _ = GlobalFree(Some(g));
                return Err("GlobalLock failed".to_string());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
            let _ = GlobalUnlock(g);
            // Удалось — память теперь принадлежит буферу обмена; не удалось — она ещё наша.
            if let Err(e) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(g.0))) {
                let _ = GlobalFree(Some(g));
                return Err(e.to_string());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        r
    }
}
