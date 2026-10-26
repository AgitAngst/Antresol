// Быстрые настройки: четыре плитки «включить / выключить» системного (как быстрые настройки Windows) —
// «Не спать», «Тёмная тема», «Тишина», «Wi‑Fi», — план питания и VPN Windows. Плитка — состояние, прочитанное
// при показе панели, и щелчок — поменять. Не узнали состояние — плитка «Недоступно» и не нажимается. Wi‑Fi и VPN
// отвечают медленно — их читает и меняет поток модуля. Ночной свет — выключателем в карточке «Яркость».

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Ctx, Module, chips};
use crate::sys::net::{self, Vpn, Wifi};
use crate::sys::system;
use crate::texts::Tr;

enum Job {
    Read,
    Wifi(bool),
    Vpn(String, bool),
}

#[derive(Default)]
struct Shared {
    /// `None` — ещё не прочитано; `Some(None)` — радио Wi‑Fi нет.
    wifi: Option<Option<Wifi>>,
    vpns: Vec<Vpn>,
    /// Ошибка последнего действия — показать уведомлением.
    error: Option<String>,
}

pub struct Quick {
    /// «Не спать» — только пока работает программа.
    awake: bool,
    dark: Option<bool>,
    quiet: Option<bool>,
    plans: Vec<system::Plan>,
    plan: Option<windows::core::GUID>,
    jobs: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
}

impl Default for Quick {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let s = shared.clone();
        std::thread::spawn(move || worker(rx, s));
        Quick { awake: false, dark: None, quiet: None, plans: Vec::new(), plan: None, jobs: tx, shared }
    }
}

fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
    crate::sys::media::init_thread();
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
        }
        let mut error = None;
        for j in jobs {
            let r = match j {
                Job::Read => Ok(()),
                Job::Wifi(on) => net::set_wifi(on),
                Job::Vpn(name, on) => net::set_vpn(&name, on),
            };
            if let Err(e) = r {
                eprintln!("quick settings failed: {e}");
                error = Some(e);
            }
        }
        let (wifi, vpns) = (net::wifi(), net::vpns());
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.wifi = Some(wifi);
        s.vpns = vpns;
        if error.is_some() {
            s.error = error;
        }
    }
}

/// Плитка 2 строки: значок и состояние сверху, ниже название. Включена — подложка акцента.
/// `state` — `None`: недоступно (не нажимается). `on_text` — что написать вместо «Вкл», пока плитка включена
/// (имя сети, «без всплывающих»); длинное обрезается многоточием, целиком — в подсказке плитки. Ответ — щелчок.
pub fn tile(ui: &mut Ui, icon: Icon, label: &str, state: Option<bool>, on_text: Option<&str>, t: &Tr) -> bool {
    let p = Palette::of(ui);
    let size = Vec2::new(ui.available_width(), 48.0);
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
        Some(true) => (p.accent_text, on_text.unwrap_or(t.on)),
        Some(false) => (p.text, t.off),
        None => (p.faint, t.unavailable),
    };
    // Сверху значок и состояние, снизу название во всю ширину.
    let ic = Rect::from_min_size(rect.min + Vec2::new(10.0, 7.0), Vec2::splat(16.0));
    anvil_ui::icons::paint(ui.painter(), ic, icon, fg);
    let sub_color = if on { p.accent_text } else { p.weak };
    let mut sub_job =
        egui::text::LayoutJob::simple_singleline(sub.to_owned(), egui::FontId::proportional(12.0), sub_color);
    sub_job.wrap = egui::text::TextWrapping::truncate_at_width((rect.width() - 10.0 - 16.0 - 8.0 - 10.0).max(20.0));
    let sub_galley = ui.painter().layout_job(sub_job);
    let sub_at = Pos2::new(rect.right() - 10.0 - sub_galley.size().x, ic.center().y - sub_galley.size().y / 2.0);
    ui.painter().galley(sub_at, sub_galley, sub_color);
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), anvil_ui::semibold(13.0), fg);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 20.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(Pos2::new(rect.left() + 10.0, rect.bottom() - 7.0 - galley.size().y), galley, fg);
    w::focus_ring(ui, rect, &r, 8);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, state.is_some(), on, label));
    let mut r = if state.is_some() { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r };
    if on && on_text.is_some() {
        r = r.on_hover_text(sub);
    }
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
        240.0
    }

    fn refresh(&mut self, _monitor: &str) {
        self.dark = system::dark_theme();
        self.quiet = system::notifications_off();
        (self.plans, self.plan) = system::plans();
        let _ = self.jobs.send(Job::Read);
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let (wifi, vpns, error) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.wifi.clone(), s.vpns.clone(), s.error.take())
        };
        if let Some(e) = error {
            cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
        }
        let wifi_state = wifi.clone().flatten();
        // Название плитки одно — «Wi‑Fi»; сеть, через которую выход в интернет, — в строке состояния.
        let network = wifi_state.as_ref().filter(|wf| wf.on && !wf.network.is_empty()).map(|wf| wf.network.as_str());
        egui::ScrollArea::vertical().id_salt("quick").auto_shrink([false, false]).show(ui, |ui| {
            // Четыре плитки по две в ряд.
            let tiles: [(Icon, &str, Option<bool>, Option<&str>); 4] = [
                (Icon::Sun, t.keep_awake, Some(self.awake), None),
                (Icon::Image, t.dark_theme_win, self.dark, None),
                (Icon::Pause, t.quiet, self.quiet, Some(t.quiet_on)),
                (Icon::Broadcast, "Wi‑Fi", wifi_state.as_ref().map(|wf| wf.on), network),
            ];
            let gap = 6.0;
            let half = ((ui.available_width() - gap) / 2.0).floor();
            let mut clicked = None;
            for (r, row) in tiles.chunks(2).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (c, (icon, label, state, on_text)) in row.iter().enumerate() {
                        ui.allocate_ui(Vec2::new(half, 48.0), |ui| {
                            if tile(ui, *icon, label, *state, *on_text, t) {
                                clicked = Some(r * 2 + c);
                            }
                        });
                    }
                });
            }
            if let Some(i) = clicked {
                self.toggle(i, wifi_state.as_ref(), cx);
            }

            if !self.plans.is_empty() {
                ui.add_space(8.0);
                w::section_label(ui, t.power_plan);
                let opts: Vec<(Option<windows::core::GUID>, &str)> =
                    self.plans.iter().map(|p| (Some(p.guid), p.name.as_str())).collect();
                let mut want = self.plan;
                if chips(ui, &mut want, &opts)
                    && let Some(g) = want
                {
                    match system::set_plan(&g) {
                        Ok(()) => self.plan = Some(g),
                        Err(e) => cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger)),
                    }
                }
            }

            if !vpns.is_empty() {
                ui.add_space(8.0);
                w::section_label(ui, "VPN");
                for v in &vpns {
                    ui.horizontal(|ui| {
                        w::dot(ui, if v.connected { Tone::Success } else { Tone::Neutral });
                        let p = Palette::of(ui);
                        ui.label(egui::RichText::new(&v.name).color(if v.connected { p.text } else { p.weak }));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = if v.connected { t.disconnect } else { t.connect };
                            if w::button(ui, Kind::Ghost, None, label).clicked() {
                                let _ = self.jobs.send(Job::Vpn(v.name.clone(), !v.connected));
                            }
                        });
                    });
                }
            }
        });
    }
}

impl Quick {
    /// Щелчок по плитке с номером `i` (порядок — как в `ui`).
    fn toggle(&mut self, i: usize, wifi: Option<&Wifi>, cx: &mut Ctx) {
        let t = cx.t;
        let result = match i {
            0 => {
                let on = !self.awake;
                Some(if system::keep_awake(on) {
                    self.awake = on;
                    Ok(())
                } else {
                    Err("SetThreadExecutionState failed".to_string())
                })
            }
            1 => self.dark.map(|on| system::set_dark_theme(!on).map(|()| self.dark = Some(!on))),
            2 => self.quiet.map(|on| system::set_notifications_off(!on).map(|()| self.quiet = Some(!on))),
            3 => wifi.map(|wf| {
                let _ = self.jobs.send(Job::Wifi(!wf.on));
                Ok(())
            }),
            _ => None,
        };
        if let Some(Err(e)) = result {
            eprintln!("quick settings failed: {e}");
            cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
        }
    }
}
