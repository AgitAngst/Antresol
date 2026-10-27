// Битовая карта DIB, как она лежит в буфере обмена: заголовок (`BITMAPINFOHEADER` и его продолжения), маски каналов, таблица
// цветов, пиксели. Разбор — для миниатюры в истории буфера и для «Текста с экрана». Байты чужие: всё читается через `get`
// и `checked_*`, картинка не копируется — строки берутся прямо из байтов.

/// Сторона длиннее не разбирается, пикселей.
const MAX_SIDE: usize = 16_384;

/// Где что лежит в DIB. Разбираются 24 и 32 бита: `BI_RGB`, `BI_BITFIELDS` и `BI_ALPHABITFIELDS` с масками в целый байт
/// (так отдают снимки экрана и браузеры). Палитры, 16 бит, RLE, JPEG и PNG внутри DIB — нет: вместо картинки значок.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: usize,
    pub height: usize,
    /// Строки идут сверху вниз (высота в заголовке со знаком минус); иначе — снизу вверх.
    pub top_down: bool,
    /// Байт на пиксель: 3 или 4.
    pub bytes_per_pixel: usize,
    /// Байт на строку (кратно 4).
    pub stride: usize,
    /// Откуда в DIB идут пиксели: за заголовком, масками и таблицей цветов.
    pub start: usize,
    /// Номер байта в пикселе, где красный, зелёный и синий.
    pub channels: [usize; 3],
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i.checked_add(4)?)?.try_into().ok()?))
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i.checked_add(2)?)?.try_into().ok()?))
}

/// Номер байта пикселя, который целиком закрывает маска (`0x00ff0000` → 2); другая маска (10 бит на канал) — `None`.
fn byte_of(mask: u32) -> Option<usize> {
    match mask {
        0x0000_00ff => Some(0),
        0x0000_ff00 => Some(1),
        0x00ff_0000 => Some(2),
        0xff00_0000 => Some(3),
        _ => None,
    }
}

impl Layout {
    /// Разобрать заголовок. Не поддерживается, размеры поддельные или пикселей не хватает — `None`.
    pub fn parse(dib: &[u8]) -> Option<Layout> {
        let header = u32_at(dib, 0)? as usize;
        let width = i32::from_le_bytes(dib.get(4..8)?.try_into().ok()?);
        let height = i32::from_le_bytes(dib.get(8..12)?.try_into().ok()?);
        let bits = u16_at(dib, 14)?;
        let compression = u32_at(dib, 16)?;
        // Таблица цветов у 24 и 32 бит тоже бывает — для системной палитры; пиксели идут за ней.
        let colors = u32_at(dib, 32)? as usize;
        if header < 40 || width <= 0 || height == 0 {
            return None;
        }
        let (w, h) = (width as usize, height.unsigned_abs() as usize);
        if w > MAX_SIDE || h > MAX_SIDE {
            return None;
        }
        // `BI_RGB` — байты синий, зелёный, красный (и пустой); `BI_BITFIELDS` (3) и `BI_ALPHABITFIELDS` (6) — три маски (у
        // заголовка 40 они идут за ним: 12 и 16 байт, у длиннее — внутри), по ним и порядок каналов: у RGBA красный первый.
        let (bytes_per_pixel, channels, masks) = match (bits, compression) {
            (24, 0) => (3, [2, 1, 0], 0),
            (32, 0) => (4, [2, 1, 0], 0),
            (32, 3 | 6) => {
                let channels = [byte_of(u32_at(dib, 40)?)?, byte_of(u32_at(dib, 44)?)?, byte_of(u32_at(dib, 48)?)?];
                if channels[0] == channels[1] || channels[0] == channels[2] || channels[1] == channels[2] {
                    return None;
                }
                let behind = match (header, compression) {
                    (40, 3) => 12,
                    (40, _) => 16,
                    _ => 0,
                };
                (4, channels, behind)
            }
            _ => return None,
        };
        let start = header.checked_add(masks)?.checked_add(colors.checked_mul(4)?)?;
        let stride = w.checked_mul(bytes_per_pixel)?.div_ceil(4).checked_mul(4)?;
        // Строк на месте не меньше, чем сказано в заголовке.
        dib.get(start..start.checked_add(stride.checked_mul(h)?)?)?;
        Some(Layout { width: w, height: h, top_down: height < 0, bytes_per_pixel, stride, start, channels })
    }

    /// Строка `y` сверху: `width * bytes_per_pixel` байт. За пределами или в других байтах — `None`.
    pub fn row<'a>(&self, dib: &'a [u8], y: usize) -> Option<&'a [u8]> {
        if y >= self.height {
            return None;
        }
        let line = if self.top_down { y } else { self.height - 1 - y };
        let from = self.start.checked_add(line.checked_mul(self.stride)?)?;
        dib.get(from..from.checked_add(self.width.checked_mul(self.bytes_per_pixel)?)?)
    }

    /// Красный, зелёный и синий пикселя `x` в строке `row` (из `row`). За пределами — чёрный.
    pub fn rgb(&self, row: &[u8], x: usize) -> [u8; 3] {
        let at = x.saturating_mul(self.bytes_per_pixel);
        match row.get(at..at.saturating_add(self.bytes_per_pixel)) {
            Some(p) => [p[self.channels[0]], p[self.channels[1]], p[self.channels[2]]],
            None => [0; 3],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DIB без пикселей: заголовок `header` байт, поля, маски за 40-байтовым заголовком (если просят) и таблица цветов.
    struct Head {
        header: u32,
        w: i32,
        h: i32,
        bits: u16,
        compression: u32,
        colors: u32,
        masks: Option<[u32; 3]>,
    }

    impl Head {
        fn new(w: i32, h: i32, bits: u16) -> Head {
            Head { header: 40, w, h, bits, compression: 0, colors: 0, masks: None }
        }

        fn bytes(&self, pixels: &[u8]) -> Vec<u8> {
            let mut b = vec![0u8; self.header as usize];
            b[0..4].copy_from_slice(&self.header.to_le_bytes());
            b[4..8].copy_from_slice(&self.w.to_le_bytes());
            b[8..12].copy_from_slice(&self.h.to_le_bytes());
            b[12..14].copy_from_slice(&1u16.to_le_bytes());
            b[14..16].copy_from_slice(&self.bits.to_le_bytes());
            b[16..20].copy_from_slice(&self.compression.to_le_bytes());
            b[32..36].copy_from_slice(&self.colors.to_le_bytes());
            if let Some(masks) = self.masks {
                for (i, m) in masks.iter().enumerate() {
                    if self.header == 40 {
                        b.extend_from_slice(&m.to_le_bytes());
                    } else {
                        b[40 + i * 4..44 + i * 4].copy_from_slice(&m.to_le_bytes());
                    }
                }
            }
            // Таблица цветов: настоящая, пока её размер разумный (поддельный размер в заголовке памяти не требует).
            b.extend(std::iter::repeat_n(0xee, self.colors.min(1000) as usize * 4));
            b.extend_from_slice(pixels);
            b
        }
    }

    #[test]
    fn plain_24_and_32_bit_are_blue_green_red() {
        let l = Layout::parse(&Head::new(2, 2, 24).bytes(&[0; 16])).expect("24 бита");
        assert_eq!((l.width, l.height, l.bytes_per_pixel, l.stride, l.start), (2, 2, 3, 8, 40));
        assert_eq!((l.channels, l.top_down), ([2, 1, 0], false));
        let l = Layout::parse(&Head::new(3, -2, 32).bytes(&[0; 24])).expect("32 бита сверху вниз");
        assert_eq!((l.bytes_per_pixel, l.stride, l.top_down), (4, 12, true));
        // Пиксель (B, G, R) = (1, 2, 3) → красный 3.
        let dib = Head::new(1, 1, 24).bytes(&[1, 2, 3, 0]);
        let l = Layout::parse(&dib).unwrap();
        assert_eq!(l.rgb(l.row(&dib, 0).unwrap(), 0), [3, 2, 1]);
    }

    #[test]
    fn rows_come_from_the_bottom_unless_the_height_is_negative() {
        // Две строки по одному 24-битному пикселю: в файле сначала нижняя.
        let bottom_up = Head::new(1, 2, 24).bytes(&[10, 10, 10, 0, 20, 20, 20, 0]);
        let l = Layout::parse(&bottom_up).unwrap();
        let top = l.rgb(l.row(&bottom_up, 0).unwrap(), 0);
        let low = l.rgb(l.row(&bottom_up, 1).unwrap(), 0);
        assert_eq!((top, low), ([20; 3], [10; 3]));
        let top_down = Head::new(1, -2, 24).bytes(&[10, 10, 10, 0, 20, 20, 20, 0]);
        let l = Layout::parse(&top_down).unwrap();
        assert_eq!(
            (l.rgb(l.row(&top_down, 0).unwrap(), 0), l.rgb(l.row(&top_down, 1).unwrap(), 0)),
            ([10; 3], [20; 3])
        );
        assert!(l.row(&top_down, 2).is_none(), "строки за краем нет");
    }

    #[test]
    fn masks_set_the_order_of_channels() {
        // BGRA: красный — третий байт; RGBA: красный — первый (так отдают некоторые программы).
        for (masks, red, blue) in
            [([0x00ff_0000u32, 0xff00, 0xff], 2usize, 0usize), ([0xff, 0xff00, 0x00ff_0000], 0, 2)]
        {
            for header in [40u32, 124] {
                let head = Head { header, compression: 3, masks: Some(masks), ..Head::new(1, 1, 32) };
                let mut px = [0u8; 4];
                px[red] = 200;
                px[1] = 100;
                px[blue] = 50;
                let dib = head.bytes(&px);
                let l = Layout::parse(&dib).unwrap_or_else(|| panic!("маски {masks:x?}, заголовок {header}"));
                assert_eq!(l.rgb(l.row(&dib, 0).unwrap(), 0), [200, 100, 50], "маски {masks:x?}, заголовок {header}");
                assert_eq!(l.start, if header == 40 { 52 } else { 124 });
            }
        }
        // `BI_ALPHABITFIELDS` с заголовком 40: за ним четыре маски (16 байт).
        let mut head = Head { compression: 6, masks: Some([0x00ff_0000, 0xff00, 0xff]), ..Head::new(1, 1, 32) };
        head.header = 40;
        let mut dib = head.bytes(&[]);
        dib.extend_from_slice(&0xff00_0000u32.to_le_bytes());
        dib.extend_from_slice(&[7, 8, 9, 255]);
        assert_eq!(Layout::parse(&dib).map(|l| l.start), Some(56));
        // Не байтовые и повторяющиеся маски — не разбираем.
        for masks in [[0x3ff0_0000u32, 0xffc00, 0x3ff], [0xff, 0xff, 0xff00], [0, 0, 0]] {
            let head = Head { compression: 3, masks: Some(masks), ..Head::new(1, 1, 32) };
            assert_eq!(Layout::parse(&head.bytes(&[0; 4])), None, "маски {masks:x?}");
        }
    }

    #[test]
    fn a_color_table_in_front_of_the_pixels_is_skipped() {
        let head = Head { colors: 3, ..Head::new(1, 1, 24) };
        let dib = head.bytes(&[4, 5, 6, 0]);
        let l = Layout::parse(&dib).expect("24 бита с таблицей цветов");
        assert_eq!(l.start, 40 + 12);
        assert_eq!(l.rgb(l.row(&dib, 0).unwrap(), 0), [6, 5, 4], "пиксель взят после таблицы, а не из неё");
        // Таблица за пределами байтов — не DIB.
        let head = Head { colors: u32::MAX, ..Head::new(1, 1, 24) };
        assert_eq!(Layout::parse(&head.bytes(&[0; 4])), None);
        // С масками таблица — за ними.
        let head = Head { colors: 2, compression: 3, masks: Some([0x00ff_0000, 0xff00, 0xff]), ..Head::new(1, 1, 32) };
        assert_eq!(Layout::parse(&head.bytes(&[0; 4])).map(|l| l.start), Some(40 + 12 + 8));
    }

    #[test]
    fn what_is_not_a_plain_true_color_dib_is_not_parsed() {
        let px = [0u8; 4096];
        for (w, h, bits, compression) in [
            (2, 2, 1u16, 0u32),
            (2, 2, 4, 0),
            (2, 2, 8, 0),
            (2, 2, 16, 0),
            (2, 2, 16, 3),
            (2, 2, 8, 1),
            (2, 2, 4, 2),
            (2, 2, 24, 3),
            (2, 2, 24, 5),
            (2, 2, 0, 4),
            (2, 2, 32, 4),
            (2, 2, 32, 5),
            (0, 2, 24, 0),
            (-2, 2, 24, 0),
            (2, 0, 24, 0),
            (16_385, 1, 24, 0),
            (1, 16_385, 24, 0),
            (i32::MAX, i32::MIN, 32, 0),
        ] {
            let head = Head { compression, ..Head::new(w, h, bits) };
            assert_eq!(Layout::parse(&head.bytes(&px)), None, "{w}×{h}, {bits} бит, сжатие {compression}");
        }
        // Пикселей меньше, чем в заголовке.
        assert_eq!(Layout::parse(&Head::new(4, 4, 32).bytes(&[0; 63])), None);
        assert!(Layout::parse(&Head::new(4, 4, 32).bytes(&[0; 64])).is_some());
        // Заголовок короче 40 байт и обрывки.
        let mut short = Head::new(2, 2, 24).bytes(&px);
        short[0..4].copy_from_slice(&12u32.to_le_bytes());
        assert_eq!(Layout::parse(&short), None);
        assert_eq!(Layout::parse(&[]), None);
        assert_eq!(Layout::parse(&[40, 0, 0, 0, 1, 0]), None);
        assert_eq!(Layout::parse(&Head::new(2, 2, 24).bytes(&px)[..35]), None);
    }

    /// Простая псевдослучайность без зависимостей.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        /// Первое в списке — «как у настоящих»; половину раз берём его, чтобы настоящие встречались.
        fn pick(&mut self, list: &[u32]) -> u32 {
            if self.next().is_multiple_of(2) { list[0] } else { list[(self.next() % list.len() as u64) as usize] }
        }
    }

    /// Случайные заголовки и хвосты: ни паники, ни строк за пределами байтов.
    #[test]
    fn random_headers_never_panic_or_reach_outside_the_bytes() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut parsed = 0;
        for _ in 0..40_000 {
            let hs = rng.pick(&[40, 0, 12, 39, 41, 52, 56, 64, 108, 124, 200, 1 << 20, u32::MAX]);
            let size = if hs > 4096 { 64 } else { (hs as usize).max(60) };
            let mut b = vec![0u8; size];
            b[0..4].copy_from_slice(&hs.to_le_bytes());
            let dims = [3u32, 0, 1, 2, 7, 100, 1280, 4096, 16_384, 16_385, i32::MAX as u32, i32::MIN as u32, u32::MAX];
            b[4..8].copy_from_slice(&rng.pick(&dims).to_le_bytes());
            b[8..12].copy_from_slice(&rng.pick(&dims).to_le_bytes());
            b[14..16].copy_from_slice(&(rng.pick(&[24, 0, 1, 4, 8, 16, 32, 48, 65_535]) as u16).to_le_bytes());
            b[16..20].copy_from_slice(&rng.pick(&[0, 1, 2, 3, 4, 5, 6, 13, u32::MAX]).to_le_bytes());
            b[32..36].copy_from_slice(&rng.pick(&[0, 1, 3, 256, 65_536, u32::MAX]).to_le_bytes());
            for at in [40usize, 44, 48] {
                b[at..at + 4].copy_from_slice(
                    &rng.pick(&[0xff, 0, 0xff00, 0x00ff_0000, 0xff00_0000, 0x3ff, u32::MAX]).to_le_bytes(),
                );
            }
            let tail = (rng.next() % 300) as usize;
            b.resize(b.len() + tail, 0x5a);
            if rng.next().is_multiple_of(4) {
                let cut = (rng.next() as usize) % b.len();
                b.truncate(cut);
            }
            if let Some(l) = Layout::parse(&b) {
                parsed += 1;
                for y in [0, l.height / 2, l.height - 1, l.height, usize::MAX] {
                    if let Some(row) = l.row(&b, y) {
                        assert_eq!(row.len(), l.width * l.bytes_per_pixel);
                        let _ = l.rgb(row, 0);
                        let _ = l.rgb(row, l.width - 1);
                        let _ = l.rgb(row, l.width);
                    } else {
                        assert!(y >= l.height, "строка внутри картинки есть");
                    }
                }
            }
        }
        assert!(parsed > 10, "и настоящие встречаются: {parsed}");
    }
}
