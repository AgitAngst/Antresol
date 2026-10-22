// Снимок своего окна для документации: `ANTRESOL_SHOT=файл.png` — панель снимает сама себя через
// egui (ViewportCommand::Screenshot), сохраняет PNG и закрывается. В кадр попадает только окно
// панели — чужой экран за ним снять нельзя, что и нужно для открытого репозитория.
// PNG пишем сами, без сжатия (блоки «stored»): ради снимков не тянем библиотеку картинок.

use std::time::{Duration, Instant};

use eframe::egui;

/// Через сколько после запуска снимать: выезд при запуске закончен, модули прочитали систему.
const AFTER: Duration = Duration::from_secs(4);
/// Снимка нет и через столько (окно не рисуется: монитор уснул) — закрыться без него.
const GIVE_UP: Duration = Duration::from_secs(20);

pub struct Shot {
    path: String,
    started: Instant,
    asked: bool,
}

impl Shot {
    pub fn from_env() -> Option<Shot> {
        let path = std::env::var("ANTRESOL_SHOT").ok().filter(|p| !p.is_empty())?;
        Some(Shot { path, started: Instant::now(), asked: false })
    }

    /// Звать каждый кадр: вовремя просит снимок, получив — сохраняет и закрывает окно.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.asked && self.started.elapsed() >= AFTER {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.asked = true;
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let [w, h] = image.size;
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            match std::fs::write(&self.path, png(w as u32, h as u32, &rgba)) {
                Ok(()) => eprintln!("screenshot saved: {}", self.path),
                Err(e) => eprintln!("screenshot failed: {e}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.started.elapsed() >= GIVE_UP {
            eprintln!("screenshot failed: the window did not repaint (is the monitor asleep?)");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// PNG RGBA 8 бит без сжатия.
pub fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    // Строки с байтом фильтра 0 впереди.
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    // zlib: заголовок, блоки «stored» до 65535 байт, adler32.
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    if blocks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(b) = blocks.next() {
        z.push(if blocks.peek().is_none() { 1 } else { 0 });
        let len = b.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_layout() {
        let p = png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&p[12..16], b"IHDR");
        assert_eq!(&p[p.len() - 8..p.len() - 4], b"IEND");
    }
}
