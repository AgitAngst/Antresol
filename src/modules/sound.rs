// Звук — через Kamerton: громкость и «без звука» устройства вывода по умолчанию и программ на нём,
// кнопка окна микшера Kamerton. Своего звука у панели нет: всё делает Kamerton по каналу
// (`sys::kamerton`). Канал — в потоке модуля, окно читает готовое.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Ctx, Module};
use crate::sys::kamerton::{self, Command, Error, State};
use crate::texts::Tr;

/// Где взять Kamerton, если его нет.
pub const KAMERTON_URL: &str = "https://github.com/AgitAngst/Kamerton";

enum Job {
    Read,
    Send(Command),
}

#[derive(Default)]
struct Shared {
    state: Option<Result<State, Error>>,
    failed: bool,
}

pub struct Sound {
    jobs: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
    /// Сдвинутый ползунок: своё значение, пока Kamerton не подтвердит (ключ — «» для устройства).
    local: HashMap<String, (u32, Instant)>,
}

impl Default for Sound {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let s = shared.clone();
        std::thread::spawn(move || worker(rx, s));
        Sound { jobs: tx, shared, local: HashMap::new() }
    }
}

fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
        }
        // Из скопившихся громкостей одной цели — только последняя.
        let mut sends: Vec<Command> = Vec::new();
        for j in jobs {
            if let Job::Send(c) = j {
                let same = |a: &Command| match (a, &c) {
                    (Command::Volume(_), Command::Volume(_)) => true,
                    (Command::AppVolume(x, _), Command::AppVolume(y, _)) => x == y,
                    _ => false,
                };
                sends.retain(|a| !same(a));
                sends.push(c);
            }
        }
        let mut failed = false;
        for c in &sends {
            if let Err(e) = kamerton::send(c) {
                eprintln!("kamerton command failed: {e:?}");
                failed = true;
            }
        }
        let state = kamerton::state();
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.state = Some(state);
        s.failed |= failed;
    }
}

/// Значок динамика: корпус и волны; без звука — крест вместо волн. Свой: в наборе такого нет.
pub fn paint_speaker(painter: &egui::Painter, rect: Rect, muted: bool, color: Color32) {
    let s = rect.height() / 16.0;
    let c = rect.center() - Vec2::new(3.0 * s, 0.0);
    let body = vec![
        Pos2::new(c.x - 4.0 * s, c.y - 2.5 * s),
        Pos2::new(c.x - 1.5 * s, c.y - 2.5 * s),
        Pos2::new(c.x + 2.5 * s, c.y - 6.0 * s),
        Pos2::new(c.x + 2.5 * s, c.y + 6.0 * s),
        Pos2::new(c.x - 1.5 * s, c.y + 2.5 * s),
        Pos2::new(c.x - 4.0 * s, c.y + 2.5 * s),
    ];
    painter.add(egui::Shape::closed_line(body, Stroke::new(1.6, color)));
    let x = c.x + 6.5 * s;
    if muted {
        let d = 2.5 * s;
        painter.line_segment([Pos2::new(x - d, c.y - d), Pos2::new(x + d, c.y + d)], Stroke::new(1.6, color));
        painter.line_segment([Pos2::new(x - d, c.y + d), Pos2::new(x + d, c.y - d)], Stroke::new(1.6, color));
    } else {
        for r in [3.5, 6.5] {
            let pts: Vec<Pos2> = (-3..=3)
                .map(|i| {
                    let a = i as f32 * 0.22;
                    Pos2::new(c.x + 3.0 * s + r * s * a.cos(), c.y + r * s * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.6, color)));
        }
    }
}

/// Кнопка «без звука» 28 × 28 со значком динамика.
fn mute_button(ui: &mut Ui, muted: bool, hint: &str) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, 6, p.hover);
    }
    let color = if muted {
        p.danger
    } else if r.hovered() {
        p.text
    } else {
        p.weak
    };
    paint_speaker(ui.painter(), rect.shrink(6.0), muted, color);
    w::focus_ring(ui, rect, &r, 6);
    r.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl Sound {
    fn shown(&self, key: &str, from_kamerton: u32) -> u32 {
        match self.local.get(key) {
            Some((v, at)) if at.elapsed() < Duration::from_secs(3) => *v,
            _ => from_kamerton,
        }
    }

    fn send(&self, c: Command) {
        let _ = self.jobs.send(Job::Send(c));
    }

    /// Строка громкости: подпись сверху, ниже «без звука», ползунок и проценты; колесо — шаг 5 %.
    /// Ответ — новое значение, если сдвинули.
    fn level(&mut self, ui: &mut Ui, t: &Tr, key: &str, label: &str, volume: u32, muted: bool) -> (Option<u32>, bool) {
        let p = Palette::of(ui);
        let mut job =
            egui::text::LayoutJob::simple_singleline(label.to_owned(), egui::FontId::proportional(12.5), p.weak);
        job.wrap = egui::text::TextWrapping::truncate_at_width(ui.available_width());
        ui.label(job);
        let mut pct = self.shown(key, volume) as f32;
        let mut toggle = false;
        let row = ui.horizontal(|ui| {
            if mute_button(ui, muted, if muted { t.unmute } else { t.mute }).clicked() {
                toggle = true;
            }
            ui.spacing_mut().slider_width = (ui.available_width() - 48.0).max(60.0);
            let r = ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).show_value(false));
            w::mono(ui, &format!("{pct:>3.0}%"), if muted { Some(p.faint) } else { None });
            r
        });
        let mut changed = row.inner.changed();
        let wheel: f32 = ui.input(|i| {
            i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum()
        });
        if wheel != 0.0 && ui.rect_contains_pointer(row.response.rect) {
            pct = (pct + wheel.signum() * 5.0).clamp(0.0, 100.0);
            changed = true;
        }
        let v = pct.round() as u32;
        if changed {
            self.local.insert(key.to_string(), (v, Instant::now()));
        }
        (changed.then_some(v), toggle)
    }
}

impl Module for Sound {
    fn key(&self) -> &'static str {
        "sound"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.sound
    }

    fn min_width(&self) -> f32 {
        240.0
    }

    fn refresh(&mut self, _monitor: &str) {
        let _ = self.jobs.send(Job::Read);
    }

    fn meta(&self) -> String {
        let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        match &s.state {
            Some(Ok(st)) => format!("Kamerton {}", st.version),
            _ => String::new(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let (state, failed) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.state.clone(), std::mem::take(&mut s.failed))
        };
        if failed {
            cx.toasts.push((t.kamerton_failed.to_string(), Tone::Danger));
        }
        let state = match state {
            None => {
                w::spinner(ui, 16.0);
                return;
            }
            Some(Err(Error::NotRunning)) => {
                w::note(ui, t.kamerton_off);
                ui.add_space(4.0);
                let path = cx.settings.kamerton.clone();
                if !path.is_empty() {
                    if w::button(ui, Kind::Secondary, Some(Icon::Play), t.kamerton_start).clicked() {
                        match kamerton::launch(&path) {
                            Ok(()) => self.refresh(""),
                            Err(e) => cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger)),
                        }
                    }
                } else if w::button(ui, Kind::Ghost, Some(Icon::Download), t.kamerton_get).clicked()
                    && let Err(e) = crate::sys::launch(KAMERTON_URL, "")
                {
                    cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                }
                return;
            }
            Some(Err(Error::Failed(e))) => {
                w::note(ui, format!("{}: {e}", t.kamerton_failed));
                return;
            }
            Some(Ok(s)) => s,
        };
        egui::ScrollArea::vertical()
            .id_salt("sound")
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 34.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                match &state.device {
                    Some(d) => {
                        let (v, toggle) = self.level(ui, t, "", &d.name, d.volume, d.muted);
                        if let Some(v) = v {
                            self.send(Command::Volume(v));
                        }
                        if toggle {
                            self.send(Command::Mute(!d.muted));
                        }
                    }
                    None => {
                        w::note(ui, t.no_output);
                    }
                }
                if !state.apps.is_empty() {
                    ui.add_space(6.0);
                    w::section_label(ui, t.sound_apps);
                }
                for a in &state.apps {
                    let (v, toggle) = self.level(ui, t, &a.key, &a.name, a.volume, a.muted);
                    if let Some(v) = v {
                        self.send(Command::AppVolume(a.key.clone(), v));
                    }
                    if toggle {
                        self.send(Command::AppMute(a.key.clone(), !a.muted));
                    }
                }
            });
        if w::button(ui, Kind::Ghost, Some(Icon::Window), t.kamerton_mixer).clicked() {
            self.send(Command::Open);
        }
    }
}
