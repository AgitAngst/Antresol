// Заготовки текста: подписи, адреса, ответы — щелчок кладёт текст в буфер обмена. Хранятся в
// настройках (`snippet=`), добавляются из того, что сейчас в буфере.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Tone};
use eframe::egui::{self, Ui};

use super::{Ctx, Module, row};
use crate::config::MAX_SNIPPETS;
use crate::sys::{self, clip};
use crate::texts::Tr;

#[derive(Default)]
pub struct Snippets;

/// Заготовка в строке списка: первая строка текста.
pub fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
}

impl Module for Snippets {
    fn key(&self) -> &'static str {
        "snippets"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.snippets
    }

    fn refresh(&mut self, _monitor: &str) {}

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let demo: Vec<String> =
            ["Спасибо, получил!", "demo@example.com", "Встреча завтра в 10:00\nКомната 3"].map(String::from).to_vec();
        let list = if sys::demo() { demo } else { cx.settings.snippets.clone() };
        let (mut copy, mut remove) = (None, None);
        egui::ScrollArea::vertical()
            .id_salt("snippets")
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 34.0)
            .show(ui, |ui| {
                if list.is_empty() {
                    w::note(ui, t.snippets_empty);
                }
                for (i, s) in list.iter().enumerate() {
                    let id = ui.id().with(("snippet", i));
                    let lines = s.lines().count();
                    let more = if lines > 1 { format!("+{}", lines - 1) } else { String::new() };
                    let r = row(ui, id, false, first_line(s), &more, 30.0);
                    if r.clone().on_hover_text(s.as_str()).clicked() {
                        copy = Some(i);
                    }
                    let rect = r.rect;
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_min_max(egui::pos2(rect.right() - 32.0, rect.top()), rect.max))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    if w::icon_button(&mut child, Icon::Close, t.snippet_remove).clicked() {
                        remove = Some(i);
                    }
                }
            });
        let full = cx.settings.snippets.len() >= MAX_SNIPPETS;
        ui.add_enabled_ui(!full, |ui| {
            if w::button(ui, w::Kind::Ghost, Some(Icon::Plus), t.snippet_add).clicked() {
                match clip::read_text() {
                    Ok(Some(text)) if !text.trim().is_empty() => {
                        if !sys::demo() && !cx.settings.snippets.contains(&text) {
                            cx.settings.snippets.insert(0, text);
                        }
                    }
                    _ => cx.toasts.push((t.snippet_no_text.to_string(), Tone::Warning)),
                }
            }
        });
        if let Some(i) = copy {
            match clip::write_text(&list[i]) {
                Ok(()) => cx.toasts.push((t.clipboard_done.to_string(), Tone::Neutral)),
                Err(e) => cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger)),
            }
        }
        if let Some(i) = remove
            && !sys::demo()
        {
            cx.settings.snippets.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_lines() {
        assert_eq!(first_line("\n  Привет\nмир"), "Привет");
        assert_eq!(first_line(""), "");
    }
}
