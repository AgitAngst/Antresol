//! Значок окна панели — PNG 256 px из `assets/icon` (`scripts/render-icon.ps1`); система сама уменьшает его до нужного
//! размера. Значок exe — `build.rs`; знак в шапке настроек и «О программе» — `mark.rs`.

use eframe::egui;

const PNG: &[u8] = include_bytes!("../assets/icon/antresol-256.png");

/// Значок для `ViewportBuilder::with_icon`. PNG зашит в программу и проверен тестом: ошибки чтения быть не может.
pub fn window() -> egui::IconData {
    eframe::icon_data::from_png_bytes(PNG).expect("assets/icon/antresol-256.png is a valid PNG")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_icon_is_a_256_square() {
        let icon = window();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // Плитка залита в центре, углы скруглены и прозрачны.
        assert_eq!(icon.rgba[(128 * 256 + 128) * 4 + 3], 255);
        assert_eq!(icon.rgba[3], 0);
    }
}
