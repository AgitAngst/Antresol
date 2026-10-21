// Свои кнопки из `%APPDATA%\Antresol\buttons.txt`: программа, папка, файл или адрес в один щелчок.
// Файл перечитывается сам, как только его сохранили.

use std::time::SystemTime;

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Tone};
use eframe::egui::Ui;

use super::{Ctx, Module};
use crate::config::{self, Button};
use crate::sys;
use crate::texts::Tr;

#[derive(Default)]
pub struct Buttons {
    list: Vec<Button>,
    stamp: Option<SystemTime>,
    loaded: bool,
}

impl Module for Buttons {
    fn key(&self) -> &'static str {
        "buttons"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.buttons
    }

    fn refresh(&mut self, _monitor: &str) {
        let stamp = config::buttons_stamp();
        if !self.loaded || stamp != self.stamp {
            self.list = config::load_buttons();
            self.stamp = config::buttons_stamp();
            self.loaded = true;
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        if self.list.is_empty() {
            w::note(ui, t.buttons_empty);
        }
        let mut run = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = eframe::egui::vec2(6.0, 6.0);
            for (i, b) in self.list.iter().enumerate() {
                // Без своего «появления»: обёртка-потомок мешает переносу кнопок на новую строку.
                let r = w::button(ui, Kind::Secondary, Some(b.icon), &b.name);
                let tip = if b.args.is_empty() { b.target.clone() } else { format!("{} {}", b.target, b.args) };
                if r.on_hover_text(tip).clicked() {
                    run = Some(i);
                }
            }
        });
        if let Some(i) = run {
            let b = &self.list[i];
            if let Err(e) = sys::launch(&b.target, &b.args) {
                eprintln!("launch failed for {}: {e}", b.target);
                cx.toasts.push((format!("{}: {}", t.failed, b.name), Tone::Danger));
            }
        }
        ui.add_space(6.0);
        if w::button(ui, Kind::Ghost, Some(Icon::Pencil), t.edit_buttons).clicked()
            && let Some(path) = config::buttons_path()
        {
            if !path.exists() {
                self.loaded = false;
                self.refresh("");
            }
            // Блокнот, а не «открыть»: у .txt может стоять что угодно, а здесь нужна правка.
            let _ = sys::launch("notepad.exe", &format!("\"{}\"", path.display()));
        }
    }
}
