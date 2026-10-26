// Буфер обмена: сверху закреплённые тексты (бывшие «Заготовки», `snippet=` в настройках), ниже — последние
// скопированные; щелчок по строке — положить текст обратно в буфер, булавка — закрепить или открепить.
// История только в памяти (на диск не пишется) и собирается, даже пока панель спрятана: номер копирования
// проверяется каждый кадр панели — это ничего не стоит. Приватные копии (менеджеры паролей) не запоминаются;
// запоминание выключается в настройках (закреплённое при этом остаётся).

use std::collections::VecDeque;

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Pos2, Rect, Sense, Ui, Vec2};

use super::{Ctx, Module, row};
use crate::config::MAX_PINNED;
use crate::sys::{self, clip};
use crate::texts::{self, Tr};

/// Сколько текстов помнить.
pub const KEEP: usize = 20;
/// Длиннее не храним: огромные копии (логи, файлы) — не то, что вставляют из истории.
const MAX_LEN: usize = 4000;
/// Место справа в строке под булавку.
const PIN_ROOM: f32 = 30.0;

/// То, что было в буфере до запуска, тоже возьмём — с первым кадром. В демо-режиме история выдуманная и по языку сцены
/// (`demo_recent`), своя не собирается.
#[derive(Default)]
pub struct Clipboard {
    history: VecDeque<String>,
    seen: u32,
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

/// Закреплённый текст в строке списка: первая непустая строка.
pub fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
}

/// Закреплённое в демо-режиме — выдуманные строки по языку сцены (настройки пользователя не трогаются).
fn demo_pinned(lang: anvil_ui::Lang) -> Vec<String> {
    texts::demo_clip_pinned(lang).map(String::from).to_vec()
}

/// Недавнее в демо-режиме — выдуманные строки по языку сцены.
fn demo_recent(lang: anvil_ui::Lang) -> Vec<String> {
    texts::demo_clip_recent(lang).map(String::from).to_vec()
}

/// Булавка в правом конце строки `row`: закреплённая — всегда, цветом акцента (щелчок открепляет); в истории —
/// только пока мышь над строкой (щелчок закрепляет). `salt` — для `Id`.
fn pin_button(
    ui: &mut Ui,
    row: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    pinned: bool,
    hint: &str,
) -> egui::Response {
    let p = Palette::of(ui);
    let rect = Rect::from_center_size(Pos2::new(row.right() - 16.0, row.center().y), Vec2::splat(24.0));
    let r = ui.interact(rect, ui.id().with(("pin", salt)), Sense::click());
    if pinned || ui.rect_contains_pointer(row) || r.has_focus() {
        if r.hovered() {
            ui.painter().rect_filled(rect, 6, p.hover);
        }
        let color = if pinned {
            p.accent_text
        } else if r.hovered() {
            p.text
        } else {
            p.weak
        };
        anvil_ui::icons::paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(15.0)),
            Icon::Pin,
            color,
        );
    }
    w::focus_ring(ui, rect, &r, 6);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, hint));
    r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint)
}

/// Что сделал пользователь в списке за кадр.
#[derive(Default)]
struct Picked {
    /// Положить в буфер: (закреплённое?, номер).
    copy: Option<(bool, usize)>,
    /// Закрепить строку истории.
    pin: Option<usize>,
    /// Открепить закреплённую.
    unpin: Option<usize>,
}

impl Clipboard {
    /// Положить текст в буфер обмена. Из истории — поднять его наверх; закреплённый в историю не идёт.
    fn copy(&mut self, text: String, from_history: bool, cx: &mut Ctx) {
        let t = cx.t;
        if sys::demo() {
            return;
        }
        match clip::write_text(&text) {
            Ok(()) => {
                // Своя запись — не новое копирование: поднимаем текст сами, номер запоминаем.
                self.seen = clip::sequence();
                if from_history {
                    remember(&mut self.history, text);
                }
                cx.toasts.push((t.clipboard_done.to_string(), Tone::Success));
            }
            Err(e) => {
                eprintln!("clipboard write failed: {e}");
                cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
            }
        }
    }

    /// Закрепить текст: наверх закреплённого. Полно — уведомление, ничего не меняется.
    fn pin(&mut self, text: String, cx: &mut Ctx) {
        if sys::demo() {
            return;
        }
        if cx.settings.pinned.len() >= MAX_PINNED {
            cx.toasts.push((cx.t.clip_pinned_full.to_string(), Tone::Warning));
            return;
        }
        cx.settings.pinned.retain(|p| *p != text);
        self.history.retain(|h| *h != text);
        cx.settings.pinned.insert(0, text);
    }

    /// Открепить: текст возвращается наверх недавнего (если история включена). Выключена — текст пропадает насовсем, и об
    /// этом говорит уведомление «Откреплено».
    fn unpin(&mut self, i: usize, cx: &mut Ctx) {
        if sys::demo() || i >= cx.settings.pinned.len() {
            return;
        }
        let text = cx.settings.pinned.remove(i);
        if cx.settings.clipboard {
            remember(&mut self.history, text);
        } else {
            cx.toasts.push((cx.t.clip_unpinned.to_string(), Tone::Neutral));
        }
    }

    /// «Закрепить из буфера» — когда история выключена и брать закрепляемое больше неоткуда.
    fn pin_current(&mut self, cx: &mut Ctx) {
        match clip::read_text() {
            Ok(Some(text)) if !text.trim().is_empty() => self.pin(text, cx),
            _ => cx.toasts.push((cx.t.clip_no_text.to_string(), Tone::Warning)),
        }
    }

    /// Строки: закреплённые, черта, недавние. Ответ — что выбрали.
    fn rows(&self, ui: &mut Ui, t: &Tr, pinned: &[String], recent: &[String]) -> Picked {
        let mut picked = Picked::default();
        for (i, text) in pinned.iter().enumerate() {
            let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
            let more = if lines > 1 { format!("+{}", lines - 1) } else { String::new() };
            let id = ui.id().with(("pinned", i));
            let r = row(ui, id, false, first_line(text), &more, PIN_ROOM);
            let rect = r.rect;
            if r.on_hover_text(text.as_str()).clicked() {
                picked.copy = Some((true, i));
            }
            if pin_button(ui, rect, ("pinned", i), true, t.unpin).clicked() {
                picked.unpin = Some(i);
            }
        }
        if !pinned.is_empty() && !recent.is_empty() {
            ui.add_space(3.0);
            w::divider(ui);
            ui.add_space(3.0);
        }
        for (i, text) in recent.iter().enumerate() {
            let id = ui.id().with(("clip", i));
            let r = row(ui, id, i == 0, &one_line(text), "", PIN_ROOM);
            let rect = r.rect;
            if r.on_hover_text(t.clipboard_put).clicked() {
                picked.copy = Some((false, i));
            }
            if pin_button(ui, rect, ("clip", i), false, t.clip_pin).clicked() {
                picked.pin = Some(i);
            }
        }
        picked
    }
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
            // Закреплённый текст в историю не идёт: он и так наверху карточки.
            if let Some(text) = text.filter(|t| !settings.pinned.contains(t)) {
                remember(&mut self.history, text);
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let pinned: Vec<String> = if sys::demo() { demo_pinned(cx.lang) } else { cx.settings.pinned.clone() };
        let history_on = cx.settings.clipboard;
        let recent: Vec<String> = match (history_on, sys::demo()) {
            (false, _) => Vec::new(),
            (true, true) => demo_recent(cx.lang),
            (true, false) => self.history.iter().cloned().collect(),
        };

        // История выключена: закреплённое работает, вместо недавнего — пояснение и «Закрепить из буфера».
        // Пусто совсем — подсказка, что делать.
        let mut picked = Picked::default();
        let mut pin_now = false;
        let mut clear = false;
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            // Низ карточки: подсказка и «Очистить» (чистит только недавнее) либо «Закрепить из буфера».
            if !history_on {
                let full = cx.settings.pinned.len() >= MAX_PINNED && !sys::demo();
                ui.add_enabled_ui(!full, |ui| {
                    let r = w::button(ui, Kind::Ghost, Some(Icon::Plus), t.clip_pin_from);
                    if r.clicked() {
                        pin_now = true;
                    }
                    if full {
                        r.on_disabled_hover_text(t.clip_pinned_full);
                    }
                });
                w::note(ui, t.clipboard_off);
            } else if !recent.is_empty() {
                if w::button(ui, Kind::Ghost, Some(Icon::Trash), t.clipboard_clear).clicked() {
                    clear = true;
                }
                w::note(ui, t.clip_hint);
            }
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                if pinned.is_empty() && recent.is_empty() {
                    if history_on {
                        w::note(ui, t.clipboard_empty);
                    }
                    return;
                }
                egui::ScrollArea::vertical().id_salt("clip").auto_shrink([false, false]).show(ui, |ui| {
                    picked = self.rows(ui, t, &pinned, &recent);
                });
            });
        });

        if let Some((from_pinned, i)) = picked.copy {
            let text = if from_pinned { pinned.get(i) } else { recent.get(i) };
            if let Some(text) = text.cloned() {
                self.copy(text, !from_pinned, cx);
            }
        }
        if let Some(i) = picked.pin
            && let Some(text) = recent.get(i).cloned()
        {
            self.pin(text, cx);
        }
        if let Some(i) = picked.unpin {
            self.unpin(i, cx);
        }
        if pin_now {
            self.pin_current(cx);
        }
        if clear {
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

    fn text(i: usize) -> String {
        format!("text {i}")
    }

    #[test]
    fn pin_moves_text_up_and_unpin_brings_it_back() {
        let mut settings = crate::config::Settings::default();
        let mut toasts = Vec::new();
        let mut clip = Clipboard::default();
        for i in 0..3 {
            remember(&mut clip.history, text(i));
        }
        let mut cx = Ctx {
            t: crate::texts::tr(anvil_ui::Lang::Ru),
            lang: anvil_ui::Lang::Ru,
            monitor: "",
            toasts: &mut toasts,
            settings: &mut settings,
        };
        // Закрепить «text 1»: наверх закреплённого и из истории — уходит.
        clip.pin(text(1), &mut cx);
        clip.pin(text(0), &mut cx);
        assert_eq!(cx.settings.pinned, [text(0), text(1)], "новое закреплённое — сверху");
        assert_eq!(clip.history, [text(2)]);
        // Открепить: возвращается наверх недавнего.
        clip.unpin(0, &mut cx);
        assert_eq!(cx.settings.pinned, [text(1)]);
        assert_eq!(clip.history, [text(0), text(2)]);
        // Историю выключили — открепленное просто уходит.
        cx.settings.clipboard = false;
        clip.unpin(0, &mut cx);
        assert!(cx.settings.pinned.is_empty());
        assert_eq!(clip.history, [text(0), text(2)]);
        assert_eq!(*cx.toasts, [("Откреплено".to_string(), Tone::Neutral)], "текст пропал — об этом уведомление");
    }

    #[test]
    fn pinned_limit_warns() {
        let mut settings =
            crate::config::Settings { pinned: (0..MAX_PINNED).map(text).collect(), ..Default::default() };
        let mut toasts = Vec::new();
        let mut clip = Clipboard::default();
        let mut cx = Ctx {
            t: crate::texts::tr(anvil_ui::Lang::En),
            lang: anvil_ui::Lang::En,
            monitor: "",
            toasts: &mut toasts,
            settings: &mut settings,
        };
        clip.pin("one more".into(), &mut cx);
        assert_eq!(cx.settings.pinned.len(), MAX_PINNED, "полно — ничего не добавилось");
        assert!(!cx.settings.pinned.contains(&"one more".to_string()));
        assert_eq!(*cx.toasts, [("30 pinned — unpin some first".to_string(), Tone::Warning)]);
    }

    #[test]
    fn first_lines() {
        assert_eq!(first_line("\n  Привет\nмир"), "Привет");
        assert_eq!(first_line(""), "");
    }
}
