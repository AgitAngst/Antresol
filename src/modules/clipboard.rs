// Буфер обмена: последние скопированные тексты, щелчок — положить текст обратно в буфер.
// История только в памяти (на диск не пишется) и собирается, даже пока панель спрятана: номер
// копирования проверяется каждый кадр панели — это ничего не стоит. Приватные копии (менеджеры
// паролей) не запоминаются; запоминание выключается в настройках.

use std::collections::VecDeque;

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Tone};
use eframe::egui::{self, Ui};

use super::{Ctx, Module, row};
use crate::sys::{self, clip};
use crate::texts::Tr;

/// Сколько текстов помнить.
pub const KEEP: usize = 20;
/// Длиннее не храним: огромные копии (логи, файлы) — не то, что вставляют из истории.
const MAX_LEN: usize = 4000;

pub struct Clipboard {
    history: VecDeque<String>,
    seen: u32,
}

impl Default for Clipboard {
    fn default() -> Self {
        let history = if sys::demo() {
            ["Пример текста для вставки", "https://example.com/demo", "#2A5FD0"].into_iter().map(String::from).collect()
        } else {
            VecDeque::new()
        };
        // То, что было в буфере до запуска, тоже возьмём — с первым кадром.
        Clipboard { history, seen: 0 }
    }
}

/// Положить текст наверх истории: повтор поднимается, а не дублируется.
pub fn remember(history: &mut VecDeque<String>, text: String) {
    if text.trim().is_empty() || text.chars().count() > MAX_LEN {
        return;
    }
    history.retain(|t| *t != text);
    history.push_front(text);
    history.truncate(KEEP);
}

/// Одна строка для списка: переводы строк и табуляции — пробелами.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Module for Clipboard {
    fn key(&self) -> &'static str {
        "clipboard"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.clipboard
    }

    fn refresh(&mut self, _monitor: &str) {}

    fn background(&mut self, settings: &crate::config::Settings) {
        if sys::demo() {
            return;
        }
        let seq = clip::sequence();
        if seq == self.seen {
            return;
        }
        if !settings.clipboard {
            self.seen = seq;
            return;
        }
        // Буфер занят (`Err`) — номер не запоминаем: прочитаем в следующем кадре.
        if let Ok(text) = clip::read_text() {
            self.seen = seq;
            if let Some(text) = text {
                remember(&mut self.history, text);
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        if !cx.settings.clipboard {
            w::note(ui, t.clipboard_off);
            return;
        }
        if self.history.is_empty() {
            w::note(ui, t.clipboard_empty);
            return;
        }
        let mut put = None;
        egui::ScrollArea::vertical()
            .id_salt("clip")
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 36.0)
            .show(ui, |ui| {
                for (i, text) in self.history.iter().enumerate() {
                    let id = ui.id().with(("clip", i));
                    if row(ui, id, i == 0, &one_line(text), "", 0.0).on_hover_text(t.clipboard_put).clicked() {
                        put = Some(i);
                    }
                }
            });
        if let Some(i) = put {
            let text = self.history[i].clone();
            if sys::demo() {
                return;
            }
            match clip::write_text(&text) {
                Ok(()) => {
                    // Своя запись — не новое копирование: поднимаем текст сами, номер запоминаем.
                    self.seen = clip::sequence();
                    remember(&mut self.history, text);
                    cx.toasts.push((t.clipboard_done.to_string(), Tone::Success));
                }
                Err(e) => {
                    eprintln!("clipboard write failed: {e}");
                    cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                }
            }
        }
        ui.add_space(4.0);
        if w::button(ui, Kind::Ghost, Some(Icon::Trash), t.clipboard_clear).clicked() {
            self.history.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_rules() {
        let mut h = VecDeque::new();
        remember(&mut h, "a".into());
        remember(&mut h, "b".into());
        remember(&mut h, "a".into());
        assert_eq!(h, ["a", "b"], "повтор поднимается наверх, не дублируется");
        remember(&mut h, "   ".into());
        remember(&mut h, "x".repeat(MAX_LEN + 1));
        assert_eq!(h.len(), 2, "пустое и огромное не храним");
        for i in 0..30 {
            remember(&mut h, i.to_string());
        }
        assert_eq!(h.len(), KEEP);
        assert_eq!(h[0], "29");
    }

    #[test]
    fn single_line() {
        assert_eq!(one_line("a\n\tb   c"), "a b c");
    }
}
