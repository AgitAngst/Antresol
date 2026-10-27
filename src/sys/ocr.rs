// Текст с экрана: выделить область «Ножницами» (`ms-screenclip:` кладёт снимок в буфер обмена),
// взять картинку из буфера (CF_DIB) и распознать текст Windows.Media.Ocr на языках профиля.
// WinRT отвечает асинхронно — звать из потока модуля (после `media::init_thread`).

use std::time::{Duration, Instant};

use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_DIB;
use windows::core::{Error, HRESULT, Result};

use super::dib::Layout;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Картинка: ширина, высота и пиксели BGRA сверху вниз.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// CF_DIB (BITMAPINFOHEADER и пиксели) → BGRA сверху вниз. Только 24 и 32 бита без палитры (`sys::dib`): таблица цветов перед
/// пикселями и порядок каналов по маскам учитываются.
pub fn dib_to_bgra(b: &[u8]) -> Option<Image> {
    let layout = Layout::parse(b)?;
    let (w, h) = (layout.width, layout.height);
    let mut out = vec![0u8; w.checked_mul(h)?.checked_mul(4)?];
    for y in 0..h {
        let row = layout.row(b, y)?;
        for x in 0..w {
            let [r, g, bl] = layout.rgb(row, x);
            out[(y * w + x) * 4..][..4].copy_from_slice(&[bl, g, r, 255]);
        }
    }
    Some(Image { width: w as u32, height: h as u32, bgra: out })
}

/// Картинка из буфера обмена; не картинка или буфер занят — `None`.
pub fn clipboard_image() -> Option<Image> {
    unsafe {
        OpenClipboard(None).ok()?;
        let img = (|| {
            IsClipboardFormatAvailable(CF_DIB.0 as u32).ok()?;
            let h = GetClipboardData(CF_DIB.0 as u32).ok()?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u8;
            if p.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(p, GlobalSize(g)).to_vec();
            let _ = GlobalUnlock(g);
            Some(bytes)
        })();
        let _ = CloseClipboard();
        // Разбор — уже после закрытия буфера: что бы ни случилось, буфер не остаётся занятым.
        img.and_then(|b| dib_to_bgra(&b))
    }
}

/// Нет языка распознавания в Windows (`recognize` отвечает этой ошибкой).
pub const NO_LANGUAGE: &str = "no-ocr-language";

/// Уменьшить картинку целым шагом, чтобы стороны были не больше `max` (ближайший пиксель).
pub fn fit(img: &Image, max: u32) -> Image {
    let step = img.width.max(img.height).div_ceil(max.max(1)).max(1);
    if step == 1 {
        return Image { width: img.width, height: img.height, bgra: img.bgra.clone() };
    }
    let (w, h) = (img.width / step, img.height / step);
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let i = (((y * step) * img.width + x * step) * 4) as usize;
            bgra.extend_from_slice(&img.bgra[i..i + 4]);
        }
    }
    Image { width: w, height: h, bgra }
}

/// Распознать текст картинки; строки — через перевод строки.
pub fn recognize(img: &Image) -> Result<String> {
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|_| Error::new(HRESULT(0x8000_4005u32 as i32), NO_LANGUAGE))?;
    let img = fit(img, OcrEngine::MaxImageDimension().unwrap_or(4096));
    let buf = CryptographicBuffer::CreateFromByteArray(&img.bgra)?;
    let bmp = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buf,
        BitmapPixelFormat::Bgra8,
        img.width as i32,
        img.height as i32,
        BitmapAlphaMode::Premultiplied,
    )?;
    let op = engine.RecognizeAsync(&bmp)?;
    let start = Instant::now();
    let result = loop {
        match op.Status()?.0 {
            0 if start.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(20)),
            0 => {
                let _ = op.Cancel();
                break Err(Error::new(HRESULT(0x8000_0005u32 as i32), "ocr timed out"));
            }
            _ => break op.GetResults(),
        }
    }?;
    let mut lines = Vec::new();
    for l in result.Lines()? {
        lines.push(l.Text()?.to_string());
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dib(w: i32, h: i32, bits: u16, pixels: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 40];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[4..8].copy_from_slice(&w.to_le_bytes());
        b[8..12].copy_from_slice(&h.to_le_bytes());
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        b[14..16].copy_from_slice(&bits.to_le_bytes());
        b.extend_from_slice(pixels);
        b
    }

    #[test]
    fn bottom_up_24bit() {
        // 2 × 2, строки по 6 байт + 2 выравнивания; снизу вверх: нижняя строка первая.
        let px = [1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12, 0, 0];
        let img = dib_to_bgra(&dib(2, 2, 24, &px)).unwrap();
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(&img.bgra[0..4], &[7, 8, 9, 255], "верхняя строка — вторая в файле");
        assert_eq!(&img.bgra[12..16], &[4, 5, 6, 255]);
    }

    /// Живое распознавание: строка, нарисованная GDI в памяти (ни экран, ни буфер не трогаются).
    /// Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn recognizes_drawn_text() {
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
            DeleteObject, PatBlt, SelectObject, TextOutW, WHITENESS,
        };
        crate::sys::media::init_thread();
        let (w, h) = (480i32, 80i32);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let bgra = unsafe {
            let dc = CreateCompatibleDC(None);
            let mut bits = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            let old = SelectObject(dc, bmp.into());
            let _ = PatBlt(dc, 0, 0, w, h, WHITENESS);
            let text: Vec<u16> = "ANTRESOL 2026".encode_utf16().collect();
            let _ = TextOutW(dc, 20, 30, &text);
            let px = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            px
        };
        let bgra: Vec<u8> = bgra.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        let text = recognize(&Image { width: w as u32, height: h as u32, bgra }).unwrap();
        assert!(text.to_uppercase().contains("ANTRESOL"), "распознано: {text:?}");
    }

    #[test]
    fn top_down_32bit_and_rejects() {
        let px = [1, 2, 3, 0, 4, 5, 6, 0];
        let img = dib_to_bgra(&dib(2, -1, 32, &px)).unwrap();
        assert_eq!(&img.bgra, &[1, 2, 3, 255, 4, 5, 6, 255]);
        assert!(dib_to_bgra(&dib(2, 1, 8, &px)).is_none(), "палитра — нет");
        assert!(dib_to_bgra(&dib(4, 4, 32, &px)).is_none(), "пикселей не хватает");
        assert!(dib_to_bgra(&dib(0x7fff_ffff, i32::MIN, 32, &px)).is_none(), "поддельные размеры");
        let big = Image { width: 10, height: 4, bgra: vec![7; 160] };
        let small = fit(&big, 5);
        assert_eq!((small.width, small.height, small.bgra.len()), (5, 2, 40));
    }
}
