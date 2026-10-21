// Звук: громкость и «без звука» устройства по умолчанию, выбор устройства вывода.
// Колесо мыши над строкой громкости — шаг 2 %.

use std::time::Instant;

use anvil_ui::widgets as w;
use anvil_ui::{Palette, Tone};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Ctx, Module, row};
use crate::sys::audio::{self, State};
use crate::texts::Tr;

#[derive(Default)]
pub struct Audio {
    state: State,
    error: bool,
    /// Когда двигали громкость сами — чтобы чтение из системы не дёргало ползунок под рукой.
    touched: Option<Instant>,
}

/// Динамик с волнами по громкости; без звука — крестик вместо волн. Своё: в наборе такого значка нет.
fn paint_speaker(painter: &egui::Painter, rect: Rect, volume: f32, muted: bool, color: Color32) {
    let s = rect.height() / 24.0;
    let at = |x: f32, y: f32| Pos2::new(rect.left() + x * s, rect.top() + y * s);
    let stroke = Stroke::new(1.8 * s.max(0.8), color);
    let body = vec![at(3.0, 9.0), at(7.0, 9.0), at(12.0, 4.5), at(12.0, 19.5), at(7.0, 15.0), at(3.0, 15.0)];
    painter.add(egui::Shape::closed_line(body, stroke));
    if muted {
        painter.line_segment([at(15.5, 9.0), at(21.0, 15.0)], stroke);
        painter.line_segment([at(21.0, 9.0), at(15.5, 15.0)], stroke);
        return;
    }
    let waves = if volume <= 0.001 {
        0
    } else if volume < 0.34 {
        1
    } else if volume < 0.67 {
        2
    } else {
        3
    };
    for i in 0..waves {
        let r = (4.0 + i as f32 * 3.5) * s;
        let c = at(12.5, 12.0);
        let pts: Vec<Pos2> = (0..=12)
            .map(|k| {
                let a = -0.9 + 1.8 * k as f32 / 12.0;
                Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
            })
            .collect();
        painter.add(egui::Shape::line(pts, stroke));
    }
}

impl Audio {
    fn mute_button(&mut self, ui: &mut Ui, t: &Tr, cx: &mut Ctx) {
        let p = Palette::of(ui);
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
        let hovered = resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, 6, p.hover);
        }
        let color = if self.state.muted {
            p.danger
        } else if hovered {
            p.text
        } else {
            p.weak
        };
        paint_speaker(ui.painter(), rect.shrink(6.0), self.state.volume, self.state.muted, color);
        let hint = if self.state.muted { t.unmute } else { t.mute };
        let resp = resp.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            let muted = !self.state.muted;
            match audio::set_muted(muted) {
                Ok(()) => self.state.muted = muted,
                Err(e) => cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger)),
            }
        }
    }

    fn set_volume(&mut self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        if audio::set_volume(v).is_ok() {
            self.state.volume = v;
            self.touched = Some(Instant::now());
        }
    }
}

impl Module for Audio {
    fn key(&self) -> &'static str {
        "audio"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.sound
    }

    fn refresh(&mut self, _monitor: &str) {
        if self.touched.is_some_and(|at| at.elapsed().as_millis() < 800) {
            return;
        }
        match audio::read() {
            Ok(s) => {
                self.state = s;
                self.error = false;
            }
            Err(e) => {
                if !self.error {
                    eprintln!("audio read failed: {e}");
                }
                self.error = true;
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        if self.error {
            w::note(ui, t.failed);
            return;
        }

        let volume_row = ui.horizontal(|ui| {
            self.mute_button(ui, t, cx);
            let mut pct = (self.state.volume * 100.0).round();
            ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(60.0);
            let r = ui.add_enabled(!self.state.muted, egui::Slider::new(&mut pct, 0.0..=100.0).show_value(false));
            if r.changed() {
                self.set_volume(pct / 100.0);
            }
            let shown = anvil_ui::motion::Motion::of(ui.ctx()).value(
                ui.ctx(),
                ui.id().with("vol"),
                self.state.volume * 100.0,
                anvil_ui::motion::HOVER,
            );
            w::mono(ui, &format!("{:>3.0}%", shown), Some(if self.state.muted { p.faint } else { p.text }));
        });
        // Колесо мыши над строкой громкости — шаг 2 %. Над списком колесо листает список.
        let wheel: f32 = ui.input(|i| {
            i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum()
        });
        if wheel != 0.0 && ui.rect_contains_pointer(volume_row.response.rect) && !self.state.muted {
            self.set_volume(self.state.volume + wheel.signum() * 0.02);
        }
        ui.add_space(8.0);

        w::section_label(ui, t.output);
        if self.state.devices.is_empty() {
            w::note(ui, t.no_devices);
            return;
        }
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("out").auto_shrink([false, false]).show(ui, |ui| {
            for (i, d) in self.state.devices.iter().enumerate() {
                let on = self.state.default_id.as_deref() == Some(d.id.as_str());
                let id = ui.id().with(("dev", &d.id));
                let resp = anvil_ui::motion::widgets::enter(ui, ("dev", &d.id), i, |ui| row(ui, id, on, &d.name, ""));
                if resp.clicked() && !on {
                    pick = Some(d.id.clone());
                }
            }
        });
        if let Some(id) = pick {
            match audio::set_default(&id) {
                Ok(()) => self.state.default_id = Some(id),
                Err(e) => {
                    eprintln!("default device change failed: {e}");
                    cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                }
            }
            self.touched = None;
            self.refresh("");
        }
    }
}
