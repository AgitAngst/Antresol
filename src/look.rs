// Цвет программы: пять готовых (лайм — по умолчанию) и свой. Свой задаётся одним цветом и
// выводится под каждую тему: заливку осветляем (тёмная) или затемняем (светлая), пока она не даст
// 3:1 на карточке; акцентный текст — 4,5:1 на панели, карточке, кнопке и мягкой подложке акцента;
// знак на заливке — белый, почти чёрный или чёрный, что контрастнее.

use anvil_ui::Accent;
use anvil_ui::theme::{Swatch, over};
use eframe::egui::Color32;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const fn sw(fill: u32, on_fill: u32, text: u32) -> Swatch {
    Swatch { fill: rgb(fill), on_fill: rgb(on_fill), text: rgb(text) }
}

pub const LIME: Accent =
    Accent { name: "lime", dark: sw(0xA6D64A, 0x141F04, 0xB5E063), light: sw(0x4F7A12, 0xFFFFFF, 0x466D0F) };
pub const VIOLET: Accent =
    Accent { name: "violet", dark: sw(0xA48BF5, 0x160B33, 0xB7A3F8), light: sw(0x6A46D1, 0xFFFFFF, 0x5F3CC4) };
pub const INDIGO: Accent =
    Accent { name: "indigo", dark: sw(0x8B93F8, 0x0B0E33, 0xA3A9FA), light: sw(0x4B4FD6, 0xFFFFFF, 0x4044C4) };
pub const FUCHSIA: Accent =
    Accent { name: "fuchsia", dark: sw(0xE27AD8, 0x2A0627, 0xEA94E2), light: sw(0xB0309F, 0xFFFFFF, 0xA02A91) };
pub const SKY: Accent =
    Accent { name: "sky", dark: sw(0x4FB8E8, 0x04192A, 0x6CC6EF), light: sw(0x0F6FA3, 0xFFFFFF, 0x0D6595) };

/// Готовые цвета в порядке образцов настроек; первый — по умолчанию.
pub const PRESETS: [Accent; 5] = [LIME, VIOLET, INDIGO, FUCHSIA, SKY];

/// Фоны, на которых стоит акцентный текст: панель, карточка, кнопка.
fn grounds(dark: bool) -> [Color32; 3] {
    if dark { [rgb(0x161618), rgb(0x1C1C1F), rgb(0x252529)] } else { [rgb(0xFAFAFA), rgb(0xFFFFFF), rgb(0xF0F0F2)] }
}

fn card(dark: bool) -> Color32 {
    grounds(dark)[1]
}

fn linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

fn luminance(c: Color32) -> f32 {
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
}

/// Контраст двух непрозрачных цветов по WCAG: 1…21.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Сдвигать цвет к `toward`, пока контраст с каждым из `grounds` не станет не ниже `target`.
fn reach(c: Color32, grounds: &[Color32], target: f32, toward: Color32) -> Color32 {
    let ok = |x: Color32| grounds.iter().all(|g| contrast(x, *g) >= target);
    let mut t = 0.0;
    let mut r = c;
    while !ok(r) && t < 1.0 {
        t = (t + 0.02_f32).min(1.0);
        r = mix(c, toward, t);
    }
    r
}

/// Мягкая подложка акцента — как `Palette::badge_fill` набора.
pub fn soft(fill: Color32, dark: bool) -> Color32 {
    over(fill.gamma_multiply(if dark { 0.16 } else { 0.12 }), card(dark))
}

/// Акцент одной темы из выбранного цвета.
pub fn derive(base: Color32, dark: bool) -> Swatch {
    let toward = if dark { Color32::WHITE } else { Color32::BLACK };
    let fill = reach(base, &[card(dark)], 3.0, toward);
    let mut on_grounds = grounds(dark).to_vec();
    on_grounds.push(soft(fill, dark));
    let text = reach(base, &on_grounds, 4.5, toward);
    let candidates = [Color32::WHITE, mix(fill, Color32::BLACK, 0.85), Color32::BLACK];
    let on_fill = candidates.into_iter().find(|c| contrast(*c, fill) >= 4.5).unwrap_or_else(|| {
        if contrast(Color32::WHITE, fill) > contrast(Color32::BLACK, fill) { Color32::WHITE } else { Color32::BLACK }
    });
    Swatch { fill, on_fill, text }
}

/// Свой цвет — акцент для обеих тем.
pub fn custom(base: Color32) -> Accent {
    Accent { name: "custom", dark: derive(base, true), light: derive(base, false) }
}

/// «#a6d64a» или «a6d64a» → цвет.
pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(rgb)
}

pub fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Значение `accent=` из настроек: имя готового или «#rrggbb». Незнакомое — лайм.
pub fn accent_of(value: &str) -> Accent {
    if let Some(a) = PRESETS.iter().find(|a| a.name == value) {
        return *a;
    }
    parse_hex(value).map(custom).unwrap_or(LIME)
}

/// Годится ли значение `accent=`: имя готового или код цвета.
pub fn valid(value: &str) -> bool {
    PRESETS.iter().any(|a| a.name.eq_ignore_ascii_case(value)) || parse_hex(value).is_some()
}

/// Цвет по оттенку (0…360), насыщенности и яркости (0…1).
pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let f = |n: f32| {
        let k = (n + h / 60.0) % 6.0;
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    Color32::from_rgb((f(5.0) * 255.0).round() as u8, (f(3.0) * 255.0).round() as u8, (f(1.0) * 255.0).round() as u8)
}

/// Цвет → оттенок, насыщенность, яркость.
pub fn to_hsv(c: Color32) -> (f32, f32, f32) {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let d = mx - mn;
    let mut h = if d == 0.0 {
        0.0
    } else if mx == r {
        ((g - b) / d) % 6.0
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    if h < 0.0 {
        h += 360.0;
    }
    (h, if mx > 0.0 { d / mx } else { 0.0 }, mx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readable(a: &Accent) {
        for dark in [true, false] {
            let s = a.swatch(dark);
            for g in grounds(dark) {
                assert!(contrast(s.text, g) >= 4.5, "{} {dark}: текст на фоне", a.name);
            }
            assert!(contrast(s.text, soft(s.fill, dark)) >= 4.5, "{} {dark}: текст на подложке", a.name);
            assert!(contrast(s.on_fill, s.fill) >= 4.5, "{} {dark}: знак на заливке", a.name);
        }
    }

    #[test]
    fn presets_are_readable() {
        for a in PRESETS {
            readable(&a);
        }
    }

    #[test]
    fn custom_is_adjusted() {
        // Тёмно-синий в тёмной теме и жёлтый в светлой сами по себе не читаются.
        for base in [rgb(0x101060), rgb(0xFFF200), rgb(0x808080), Color32::WHITE, Color32::BLACK] {
            readable(&custom(base));
        }
        assert_eq!(derive(rgb(0x2E4F0A), false).text, rgb(0x2E4F0A), "читаемый цвет не трогаем");
    }

    #[test]
    fn values() {
        assert_eq!(accent_of("violet").name, "violet");
        assert_eq!(accent_of("nope").name, "lime");
        assert_eq!(accent_of("#a6d64a").name, "custom");
        assert!(valid("sky") && valid("#00ff00") && valid("00ff00") && !valid("#00ff0") && !valid("red"));
        assert_eq!(hex(parse_hex("A6D64A").unwrap()), "#a6d64a");
    }

    #[test]
    fn hsv_round_trip() {
        for c in [rgb(0xA6D64A), rgb(0x0F6FA3), rgb(0xE27AD8), rgb(0x808080)] {
            let (h, s, v) = to_hsv(c);
            let back = hsv(h, s, v);
            assert!((back.r() as i32 - c.r() as i32).abs() <= 1 && (back.g() as i32 - c.g() as i32).abs() <= 1);
        }
    }
}
