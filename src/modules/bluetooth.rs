// Bluetooth: сопряжённые устройства — подключено ли, заряд. Перечисление идёт через SetupAPI и может
// занять заметное время — в своём потоке, окно читает готовое. Подключать — в «Параметрах Windows».

use std::sync::{Arc, Mutex};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Ui};

use super::{Ctx, Module};
use crate::sys::{self, bt};
use crate::texts::Tr;

#[derive(Default)]
struct Shared {
    list: Vec<bt::Device>,
    loaded: bool,
    busy: bool,
}

#[derive(Default)]
pub struct Bluetooth {
    shared: Arc<Mutex<Shared>>,
}

impl Module for Bluetooth {
    fn key(&self) -> &'static str {
        "bluetooth"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.bluetooth
    }

    fn refresh(&mut self, _monitor: &str) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.busy {
                return;
            }
            s.busy = true;
        }
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let list = bt::devices();
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.list = list;
            s.loaded = true;
            s.busy = false;
        });
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        let (list, loaded) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.list.clone(), s.loaded)
        };
        if !loaded {
            w::spinner(ui, 16.0);
            return;
        }
        let settings_h = 34.0;
        if list.is_empty() {
            w::note(ui, t.bt_none);
        } else {
            egui::ScrollArea::vertical()
                .id_salt("bt")
                .auto_shrink([false, false])
                .max_height(ui.available_height() - settings_h)
                .show(ui, |ui| {
                    for d in &list {
                        ui.horizontal(|ui| {
                            w::dot(ui, if d.connected { Tone::Success } else { Tone::Neutral });
                            let color = if d.connected { p.text } else { p.weak };
                            ui.label(egui::RichText::new(&d.name).color(color));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if let Some(b) = d.battery {
                                    let tone = if b <= 20 { Tone::Warning } else { Tone::Neutral };
                                    w::badge(ui, &format!("{b}%"), tone);
                                }
                            });
                        });
                        ui.add_space(2.0);
                    }
                });
        }
        if w::button(ui, Kind::Ghost, Some(Icon::Gear), t.bt_settings).clicked()
            && !sys::demo()
            && let Err(e) = sys::launch("ms-settings:bluetooth", "")
        {
            eprintln!("open bluetooth settings failed: {e}");
        }
    }
}
