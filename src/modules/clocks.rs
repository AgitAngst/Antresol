// Часы других поясов: своё название и пояс Windows — время там сейчас (с летним временем) и разница
// с этим ПК. Хранятся в настройках (`clock=Название | Ключ пояса`), добавляются поиском по поясам.

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette};
use eframe::egui::{self, Ui};

use super::{Action, Command, Ctx, Module, mine, row};
use crate::config::{Clock, MAX_CLOCKS, Settings};
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
    /// «Добавить пояс»: кнопка карточки и действие поиска — пояса Windows и поле поиска пояса.
    fn start_adding(&mut self) {
        self.keys = tz::keys()
            .into_iter()
            .map(|k| {
                let label = tz::now_in(&k).map(|(_, _, off)| tz::utc_label(off)).unwrap_or_default();
                (k, label)
            })
            .collect();
        self.adding = Some(String::new());
    }

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
            // Esc отменяет поиск пояса, а не прячет панель: клавишу забираем (окно проверит её уже после карточки).
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if w::button(ui, Kind::Ghost, None, t.cancel).clicked() || esc {
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

    /// «Добавить пояс»: открывает в карточке поиск пояса (панель покажет карточку). Часов уже предел — серое.
    fn actions(&self, t: &Tr, lang: anvil_ui::Lang, settings: &Settings) -> Vec<Action> {
        let count = if sys::demo() { 2 } else { settings.clocks.len() };
        let action = Action::new("clocks:add", t.clock_add, t.clocks, Icon::Plus);
        let full = texts::search_clocks_full(lang, MAX_CLOCKS);
        vec![action.words("часы пояс время timezone time zone clock").off_if(count >= MAX_CLOCKS, &full)]
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        let count = if sys::demo() { 2 } else { cx.settings.clocks.len() };
        if mine(self.key(), id) == Some("add") && count < MAX_CLOCKS {
            self.start_adding();
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let demo = vec![
            Clock { name: texts::demo_city(cx.lang, 0).into(), zone: "Tokyo Standard Time".into() },
            Clock { name: texts::demo_city(cx.lang, 1).into(), zone: "Eastern Standard Time".into() },
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
                self.start_adding();
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

    fn run(clocks: &mut Clocks, settings: &mut Settings, id: &str) {
        let mut toasts = Vec::new();
        let mut cx = Ctx {
            t: crate::texts::tr(anvil_ui::Lang::En),
            lang: anvil_ui::Lang::En,
            monitor: "",
            toasts: &mut toasts,
            settings,
        };
        clocks.command(&Command::RunAction(id.to_string()), &mut cx);
    }

    #[test]
    fn add_a_zone_is_offered_and_opens_the_zone_search_like_the_button() {
        let en = crate::texts::tr(anvil_ui::Lang::En);
        let mut settings = Settings::default();
        let mut clocks = Clocks::default();
        let actions = clocks.actions(en, anvil_ui::Lang::En, &settings);
        assert_eq!(actions.len(), 1);
        assert_eq!((actions[0].title.as_str(), actions[0].place.as_str()), ("Add time zone", en.clocks));
        assert!(actions[0].enabled && actions[0].words.contains("timezone"));
        // Поиск пояса открыт в карточке: поле пусто, пояса Windows прочитаны; карточке нужен ввод (панель покажет её).
        assert!(!clocks.typing());
        run(&mut clocks, &mut settings, "clocks:add");
        assert_eq!(clocks.adding.as_deref(), Some(""));
        assert!(clocks.typing());
        // Чужое действие — ничего.
        let mut other = Clocks::default();
        run(&mut other, &mut settings, "quick:wifi");
        assert!(other.adding.is_none());
    }

    #[test]
    fn at_the_limit_the_action_is_grey_and_does_nothing() {
        let en = crate::texts::tr(anvil_ui::Lang::En);
        let clock = |n: usize| Clock { name: format!("C{n}"), zone: format!("Zone {n}") };
        let mut settings = Settings { clocks: (0..MAX_CLOCKS).map(clock).collect(), ..Default::default() };
        let mut clocks = Clocks::default();
        let actions = clocks.actions(en, anvil_ui::Lang::En, &settings);
        // Причина называет предел, а не просто «Недоступно».
        assert!(!actions[0].enabled && actions[0].reason.as_deref() == Some("Already 8 clocks"));
        let ru = clocks.actions(crate::texts::tr(anvil_ui::Lang::Ru), anvil_ui::Lang::Ru, &settings);
        assert_eq!(ru[0].reason.as_deref(), Some("Часов уже 8"));
        run(&mut clocks, &mut settings, "clocks:add");
        assert!(clocks.adding.is_none(), "часов уже предел — как и кнопки в карточке, поиска пояса нет");
    }
}
