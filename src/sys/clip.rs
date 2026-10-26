// Буфер обмена: узнать, что скопировали (текст или картинка), и положить обратно. Новое копирование видно по номеру
// GetClipboardSequenceNumber — его опрос ничего не стоит. Менеджеры паролей помечают свои копии форматами «не запоминать» —
// такие пропускаем и для картинок. Картинка — битовая карта DIB (`CF_DIB`, а если её нет — `CF_DIBV5`) как пришла, с пределами
// на размер: большая в историю не идёт.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_UNICODETEXT};
use windows::core::w;

/// Картинка тяжелее — не запоминаем (16 МБ).
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// Любая сторона картинки длиннее — не запоминаем.
pub const MAX_IMAGE_SIDE: u32 = 8192;

/// Картинка из буфера обмена: битовая карта DIB (заголовок и пиксели) в том виде, как пришла.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub bytes: Vec<u8>,
    /// Пришла как `CF_DIBV5`, а не `CF_DIB`: так её и кладут обратно.
    pub v5: bool,
    pub width: u32,
    pub height: u32,
}

/// Что скопировали.
#[derive(Debug, PartialEq, Eq)]
pub enum Copied {
    Text(String),
    Image(Image),
}

pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Картинка из байтов DIB: размеры — по заголовку (`BITMAPINFOHEADER` и его продолжения), пределы — `MAX_IMAGE_BYTES` и
/// `MAX_IMAGE_SIDE`. Не битовая карта и не по пределам — `None`. Байты чужие: ничего, кроме заголовка, не читаем.
pub fn image_of(bytes: Vec<u8>, v5: bool) -> Option<Image> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let header = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
    if !(40..=bytes.len()).contains(&header) {
        return None;
    }
    let width = i32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
    let height = i32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?);
    // Высота со знаком минус — строки сверху вниз.
    if width <= 0 || height == 0 {
        return None;
    }
    let (width, height) = (width as u32, height.unsigned_abs());
    if width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
        return None;
    }
    Some(Image { bytes, v5, width, height })
}

/// Копия помечена «не запоминать»: менеджеры паролей, «Ножницы» с приватным содержимым. Звать при открытом буфере.
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

/// Текст из открытого буфера; нет текста — `None`.
fn text_inside() -> Option<String> {
    unsafe {
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
    }
}

/// Картинка из открытого буфера: `CF_DIB` (Windows сама собирает её из других видов картинки), нет — `CF_DIBV5`. Тяжелее
/// предела — не копируем вовсе: размер известен до копирования.
fn image_inside() -> Option<Image> {
    unsafe {
        for (format, v5) in [(CF_DIB, false), (CF_DIBV5, true)] {
            let id = format.0 as u32;
            if IsClipboardFormatAvailable(id).is_err() {
                continue;
            }
            let Ok(h) = GetClipboardData(id) else { continue };
            let g = HGLOBAL(h.0);
            let size = GlobalSize(g);
            if size == 0 || size > MAX_IMAGE_BYTES {
                return None;
            }
            let p = GlobalLock(g) as *const u8;
            if p.is_null() {
                continue;
            }
            let bytes = std::slice::from_raw_parts(p, size).to_vec();
            let _ = GlobalUnlock(g);
            return image_of(bytes, v5);
        }
        None
    }
}

/// Что скопировали: текст, а если текста нет — картинка. `Ok(None)` — там ни то ни другое, картинка не по пределам или копия
/// приватная; `Err` — буфер занят другой программой (прочитать ещё раз позже).
pub fn read() -> Result<Option<Copied>, ()> {
    unsafe {
        OpenClipboard(None).map_err(|_| ())?;
        let found = (|| {
            if private_copy() {
                return None;
            }
            // Таблицу из Excel или абзац из Word программа кладёт и текстом, и картинкой: в истории — текст.
            match text_inside() {
                Some(text) if !text.trim().is_empty() => Some(Copied::Text(text)),
                _ => image_inside().map(Copied::Image),
            }
        })();
        let _ = CloseClipboard();
        Ok(found)
    }
}

/// Текст из буфера: `Ok(None)` — там не текст или копия приватная; `Err` — буфер занят другой программой (прочитать ещё
/// раз позже).
pub fn read_text() -> Result<Option<String>, ()> {
    unsafe {
        OpenClipboard(None).map_err(|_| ())?;
        let text = if private_copy() { None } else { text_inside() };
        let _ = CloseClipboard();
        Ok(text)
    }
}

/// Положить текст в буфер (как «Копировать»).
pub fn write_text(text: &str) -> Result<(), String> {
    if super::blocked(&format!("clip::write_text {text}")) {
        return Ok(());
    }
    let wide: Vec<u8> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    put(CF_UNICODETEXT.0 as u32, &wide)
}

/// Положить картинку в буфер: тем видом, каким пришла.
pub fn write_image(image: &Image) -> Result<(), String> {
    if super::blocked(&format!("clip::write_image {}x{} {} bytes", image.width, image.height, image.bytes.len())) {
        return Ok(());
    }
    let format = if image.v5 { CF_DIBV5 } else { CF_DIB };
    put(format.0 as u32, &image.bytes)
}

/// Очистить буфер и положить в него `data` этого вида.
fn put(format: u32, data: &[u8]) -> Result<(), String> {
    super::real_write("SetClipboardData");
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let r = (|| {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let g = GlobalAlloc(GMEM_MOVEABLE, data.len()).map_err(|e| e.to_string())?;
            let p = GlobalLock(g) as *mut u8;
            if p.is_null() {
                let _ = GlobalFree(Some(g));
                return Err("GlobalLock failed".to_string());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
            let _ = GlobalUnlock(g);
            // Удалось — память теперь принадлежит буферу обмена; не удалось — она ещё наша.
            if let Err(e) = SetClipboardData(format, Some(HANDLE(g.0))) {
                let _ = GlobalFree(Some(g));
                return Err(e.to_string());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DIB без пикселей: заголовок нужного размера, ширина и высота (высота со знаком).
    fn dib(header: u32, width: i32, height: i32) -> Vec<u8> {
        let mut b = vec![0u8; header.max(12) as usize];
        b[0..4].copy_from_slice(&header.to_le_bytes());
        b[4..8].copy_from_slice(&width.to_le_bytes());
        b[8..12].copy_from_slice(&height.to_le_bytes());
        b
    }

    #[test]
    fn a_dib_gives_its_size_from_the_header() {
        let img = image_of(dib(40, 1280, 720), false).expect("обычная DIB");
        assert_eq!((img.width, img.height, img.v5), (1280, 720, false));
        // Строки сверху вниз (высота со знаком минус) и заголовок V5.
        let img = image_of(dib(124, 640, -480), true).expect("V5, сверху вниз");
        assert_eq!((img.width, img.height, img.v5), (640, 480, true));
        assert_eq!(img.bytes.len(), 124, "байты — как пришли");
    }

    #[test]
    fn a_dib_beyond_the_limits_or_not_a_dib_is_not_kept() {
        assert_eq!(image_of(dib(40, MAX_IMAGE_SIDE as i32, 1), false).map(|i| i.width), Some(MAX_IMAGE_SIDE));
        assert!(image_of(dib(40, MAX_IMAGE_SIDE as i32 + 1, 1), false).is_none(), "шире 8192");
        assert!(image_of(dib(40, 1, -(MAX_IMAGE_SIDE as i32) - 1), false).is_none(), "выше 8192");
        assert!(image_of(dib(40, i32::MAX, i32::MIN), false).is_none(), "поддельные размеры");
        assert!(image_of(dib(40, 0, 10), false).is_none(), "нулевая ширина");
        assert!(image_of(dib(40, -5, 10), false).is_none(), "ширина со знаком минус");
        assert!(image_of(dib(40, 10, 0), false).is_none(), "нулевая высота");
        assert!(image_of(dib(12, 10, 10), false).is_none(), "заголовок короче BITMAPINFOHEADER");
        assert!(image_of(dib(200, 10, 10)[..100].to_vec(), false).is_none(), "заголовок длиннее самих байтов");
        assert!(image_of(Vec::new(), false).is_none() && image_of(vec![40, 0, 0, 0, 1, 0], false).is_none());
        // Тяжелее 16 МБ — нет, ровно 16 МБ — да.
        let mut big = dib(40, 2048, 2048);
        big.resize(MAX_IMAGE_BYTES, 0);
        assert!(image_of(big.clone(), false).is_some());
        big.push(0);
        assert!(image_of(big, false).is_none(), "на байт больше предела");
    }

    #[test]
    fn writers_stop_at_the_guard_and_leave_a_mark() {
        let img = image_of(dib(40, 3, 2), false).unwrap();
        assert_eq!(write_image(&img), Ok(()));
        assert_eq!(write_text("clip-test-text-4417"), Ok(()));
        let log = crate::sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner());
        assert!(log.iter().any(|w| w == "clip::write_image 3x2 40 bytes"));
        assert!(log.iter().any(|w| w == "clip::write_text clip-test-text-4417"));
    }
}
