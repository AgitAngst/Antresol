// Быстрые настройки: плитки «включить / выключить» системного (как быстрые настройки Windows).
// Плитка — состояние, прочитанное при показе панели, и щелчок — поменять. Не узнали состояние —
// плитка «Недоступно» и не нажимается.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Ctx, Module};
use crate::sys::nightlight;
use crate::texts::Tr;

#[derive(Default)]
pub struct Quick {
    /// Ночной свет: включён ли; `None` — не узнать.
    night: Option<bool>,
}

/// Плитка 2 строки: значок и название, ниже состояние. Включена — подложка акцента.
/// `state` — `None`: недоступно (не нажимается). Ответ — щелчок.
pub fn tile(ui: &mut Ui, icon: Icon, label: &str, state: Option<bool>, t: &Tr) -> bool {
    let p = Palette::of(ui);
    let size = Vec2::new(ui.available_width(), 54.0);
    let sense = if state.is_some() { Sense::click() } else { Sense::hover() };
    let (rect, r) = ui.allocate_exact_size(size, sense);
    let on = state == Some(true);
    let hovered = r.hovered() && state.is_some();
    let fill = if on {
        p.badge_fill(p.accent)
    } else if hovered {
        p.hover
    } else {
        p.raised
    };
    let stroke = if on { p.accent } else { p.border };
    ui.painter().rect(rect, 8, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let (fg, sub) = match state {
        Some(true) => (p.accent_text, t.on),
        Some(false) => (p.text, t.off),
        None => (p.faint, t.unavailable),
    };
    // Сверху значок и состояние, снизу название во всю ширину.
    let ic = Rect::from_min_size(rect.min + Vec2::new(10.0, 9.0), Vec2::splat(16.0));
    anvil_ui::icons::paint(ui.painter(), ic, icon, fg);
    let sub_color = if on { p.accent_text } else { p.weak };
    ui.painter().text(
        Pos2::new(rect.right() - 10.0, ic.center().y),
        egui::Align2::RIGHT_CENTER,
        sub,
        egui::FontId::proportional(12.0),
        sub_color,
    );
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), anvil_ui::semibold(13.0), fg);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 20.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(Pos2::new(rect.left() + 10.0, rect.bottom() - 9.0 - galley.size().y), galley, fg);
    w::focus_ring(ui, rect, &r, 8);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, state.is_some(), on, label));
    let r = if state.is_some() { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r };
    r.clicked()
}

impl Module for Quick {
    fn key(&self) -> &'static str {
        "quick"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.quick
    }

    fn min_width(&self) -> f32 {
        260.0
    }

    fn refresh(&mut self, _monitor: &str) {
        self.night = nightlight::state();
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let gap = 6.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        let mut night_click = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui(Vec2::new(half, 54.0), |ui| {
                night_click = tile(ui, Icon::Moon, t.night_light, self.night, t);
            });
        });
        if night_click && let Some(on) = self.night {
            match nightlight::set(!on) {
                Ok(()) => self.night = Some(!on),
                Err(e) => {
                    eprintln!("night light failed: {e}");
                    cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                }
            }
        }
    }
}
