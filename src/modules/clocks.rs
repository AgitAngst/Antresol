// Часы других поясов: своё название и пояс Windows — время там сейчас (с летним временем) и разница
// с этим ПК. Хранятся в настройках (`clock=Название | Ключ пояса`), добавляются поиском по поясам.

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette};
use eframe::egui::{self, Ui};

use super::{Ctx, Module, row};
use crate::config::{Clock, MAX_CLOCKS};
use crate::sys::{self, tz};
use crate::texts::{self, Tr};

/// Когда считали, время каждых часов (часы, минуты, смещение) и смещение этого ПК.
type Times = (std::time::Instant, Vec<Option<(u16, u16, i32)>>, i32);

#[derive(Default)]
pub struct Clocks {
    /// Ищут пояс, чтобы добавить: что набрали.
    adding: Option<String>,
    /// Пояса Windows и их смещение («UTC+9») — когда начали добавлять.
    keys: Vec<(String, String)>,
    /// Время часов и когда считали: пересчёт раз в секунду, а не на каждый кадр.
    times: Option<Times>,
}

/// Название по умолчанию из ключа: «Tokyo Standard Time» → «Tokyo».
pub fn short_name(key: &str) -> String {
    key.trim_end_matches(" Standard Time").trim_end_matches(" Daylight Time").to_string()
}

impl Clocks {
    fn list_ui(&mut self, ui: &mut Ui, cx: &mut Ctx, list: &[Clock]) {
        let t = cx.t;
        let p = Palette::of(ui);
        // Демо (снимки в открытый репозиторий): свой пояс — UTC, время — выдуманное 10:00 UTC, чтобы
        // на снимке не было ни настоящего времени, ни пояса этого ПК.
        let demo = sys::demo();
        let stale = self.times.as_ref().is_none_or(|(at, v, _)| at.elapsed().as_secs() >= 1 || v.len() != list.len());
        if stale {
            let here = if demo { 0 } else { tz::local_offset() };
            let v = list
                .iter()
                .map(|c| {
                    tz::now_in(&c.zone).map(|(h, m, off)| {
                        if demo {
                            let mins = (600 + off).rem_euclid(1440);
                            ((mins / 60) as u16, (mins % 60) as u16, off)
                        } else {
                            (h, m, off)
                        }
                    })
                })
                .collect();
            self.times = Some((std::time::Instant::now(), v, here));
        }
        let (times, here) = self.times.as_ref().map(|(_, v, h)| (v.clone(), *h)).unwrap_or_default();
        let mut remove = None;
        for (i, c) in list.iter().enumerate() {
            let now = times.get(i).copied().flatten();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(egui::RichText::new(&c.name).size(13.0).color(p.weak));
                    match now {
                        Some((h, m, off)) => {
                            ui.label(
                                egui::RichText::new(format!("{h:02}:{m:02}"))
                                    .font(egui::FontId::monospace(20.0))
                                    .strong()
                                    .color(p.text),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {}",
                                    tz::utc_label(off),
                                    texts::time_diff(cx.lang, off - here)
                                ))
                                .size(12.0)
                                .color(p.weak),
                            );
                        }
                        None => {
                            w::note(ui, t.clock_unknown);
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::icon_button(ui, Icon::Close, t.clock_remove).clicked() {
                        remove = Some(i);
                    }
                });
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove
            && !sys::demo()
        {
            cx.settings.clocks.remove(i);
        }
    }

    /// Поиск пояса: поле и подходящие пояса строками; щелчок — добавить.
    fn add_ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let Some(mut query) = self.adding.take() else { return };
        let mut done = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text(t.clock_search).desired_width(150.0));
            if query.is_empty() && !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            if w::button(ui, Kind::Ghost, None, t.cancel).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = true;
            }
        });
        let q = query.to_lowercase();
        let found: Vec<&(String, String)> =
            self.keys.iter().filter(|(k, _)| q.is_empty() || k.to_lowercase().contains(&q)).take(30).collect();
        let mut picked = None;
        for (i, (k, label)) in found.iter().enumerate() {
            if row(ui, ui.id().with(("zone", i)), false, k, label, 0.0).clicked() {
                picked = Some(k.clone());
            }
        }
        if let Some(zone) = picked {
            let dup = cx.settings.clocks.iter().any(|c| c.zone == zone);
            if cx.settings.clocks.len() < MAX_CLOCKS && !dup && !sys::demo() {
                cx.settings.clocks.push(Clock { name: short_name(&zone), zone });
            }
            done = true;
        }
        if !done {
            self.adding = Some(query);
        }
    }
}

impl Module for Clocks {
    fn key(&self) -> &'static str {
        "clocks"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.clocks
    }

    fn refresh(&mut self, _monitor: &str) {}

    fn typing(&self) -> bool {
        self.adding.is_some()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let demo = vec![
            Clock { name: "Токио".into(), zone: "Tokyo Standard Time".into() },
            Clock { name: "New York".into(), zone: "Eastern Standard Time".into() },
        ];
        let list = if sys::demo() { demo } else { cx.settings.clocks.clone() };
        egui::ScrollArea::vertical().id_salt("clocks").auto_shrink([false, false]).show(ui, |ui| {
            if self.adding.is_some() {
                self.add_ui(ui, cx);
                return;
            }
            if list.is_empty() {
                w::note(ui, t.clocks_empty);
            }
            self.list_ui(ui, cx, &list);
            if list.len() < MAX_CLOCKS && w::button(ui, Kind::Ghost, Some(Icon::Plus), t.clock_add).clicked() {
                self.keys = tz::keys()
                    .into_iter()
                    .map(|k| {
                        let label = tz::now_in(&k).map(|(_, _, off)| tz::utc_label(off)).unwrap_or_default();
                        (k, label)
                    })
                    .collect();
                self.adding = Some(String::new());
            }
        });
        // Минуты идут — перерисовать к следующей.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(short_name("Tokyo Standard Time"), "Tokyo");
        assert_eq!(short_name("UTC"), "UTC");
    }
}
