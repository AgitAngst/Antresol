// Свои кнопки из `%APPDATA%\Antresol\buttons.txt`: программа, папка, файл или адрес в один щелчок.
// Файл перечитывается сам, как только его сохранили.

use std::time::SystemTime;

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Tone};
use eframe::egui::Ui;

use super::{Action, Command, Ctx, Module, mine, stable_key};
use crate::config::{self, Button, Settings};
use crate::sys;
use crate::texts::Tr;

#[derive(Default)]
pub struct Buttons {
    list: Vec<Button>,
    stamp: Option<SystemTime>,
    loaded: bool,
}

/// Устойчивый номер кнопки для поиска: имя, программа и аргументы.
fn button_key(b: &Button) -> String {
    stable_key(&[&b.name, &b.target, &b.args])
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

    /// Каждая своя кнопка: то же, что щелчок по ней в карточке (программа, папка, файл или адрес). Номер действия —
    /// отпечаток имени, программы и аргументов, а не место в файле: правка `buttons.txt` не сдвигает «Недавние».
    fn actions(&self, t: &Tr, _lang: anvil_ui::Lang, _settings: &Settings) -> Vec<Action> {
        self.list
            .iter()
            .map(|b| {
                let tip = if b.args.is_empty() { b.target.clone() } else { format!("{} {}", b.target, b.args) };
                Action::new(format!("buttons:run:{}", button_key(b)), b.name.clone(), t.buttons, b.icon)
                    .words(&format!("кнопка запустить button launch {tip}"))
            })
            .collect()
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        let Some(key) = mine(self.key(), id).and_then(|r| r.strip_prefix("run:")) else { return };
        let Some(b) = self.list.iter().find(|b| button_key(b) == key) else { return };
        if let Err(e) = sys::launch(&b.target, &b.args) {
            eprintln!("launch failed for {}: {e}", b.target);
            cx.toasts.push((format!("{}: {}", cx.t.failed, b.name), Tone::Danger));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn launched(target: &str, args: &str) -> bool {
        let want = format!("launch {target} {args}");
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).contains(&want)
    }

    fn run(b: &mut Buttons, id: &str) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        let mut settings = Settings::default();
        let mut cx = Ctx {
            t: crate::texts::tr(anvil_ui::Lang::En),
            lang: anvil_ui::Lang::En,
            monitor: "",
            toasts: &mut toasts,
            settings: &mut settings,
        };
        b.command(&Command::RunAction(id.to_string()), &mut cx);
        toasts
    }

    #[test]
    fn own_buttons_are_offered_and_launched_like_a_click() {
        let button = |name: &str, target: &str, args: &str, icon| Button {
            name: name.into(),
            target: target.into(),
            args: args.into(),
            icon,
        };
        let mut buttons = Buttons {
            list: vec![
                button("Search Notes", "search-notes-4413.exe", "", Icon::File),
                button("Search Shell", "search-shell-4413.exe", "-NoExit", Icon::Terminal),
            ],
            ..Default::default()
        };
        let en = crate::texts::tr(anvil_ui::Lang::En);
        let actions = buttons.actions(en, anvil_ui::Lang::En, &Settings::default());
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Search Notes", "Search Shell"]);
        assert_eq!((actions[0].icon, actions[1].icon), (Icon::File, Icon::Terminal), "значок — как у кнопки");
        assert!(actions.iter().all(|a| a.enabled && a.place == en.buttons));
        assert!(actions[1].words.contains("search-shell-4413.exe -NoExit"), "найти можно и по программе");
        // Запуск — как щелчок: та же программа с теми же аргументами (за заслоном: в тестах ничего не запускается).
        assert!(run(&mut buttons, &actions[1].id).is_empty());
        assert!(launched("search-shell-4413.exe", "-NoExit"));
        assert!(!launched("search-notes-4413.exe", ""), "первая кнопка не нажималась");
        // Незнакомое и чужое — ничего; кнопок нет — и действий нет.
        assert!(run(&mut buttons, "buttons:run:7").is_empty() && run(&mut buttons, "quick:wifi").is_empty());
        assert!(Buttons::default().actions(en, anvil_ui::Lang::En, &Settings::default()).is_empty());
    }

    /// Файл кнопок поправили — кнопки сдвинулись, а действие осталось при своей кнопке (в «Недавних» не сползает).
    #[test]
    fn a_button_keeps_its_action_when_the_file_changes() {
        let button = |name: &str, target: &str| Button {
            name: name.into(),
            target: target.into(),
            args: String::new(),
            icon: Icon::File,
        };
        let mut buttons = Buttons {
            list: vec![button("Search A", "search-a-7741.exe"), button("Search B", "search-b-7741.exe")],
            ..Default::default()
        };
        let en = crate::texts::tr(anvil_ui::Lang::En);
        let id_of_b = buttons.actions(en, anvil_ui::Lang::En, &Settings::default())[1].id.clone();
        // Новая кнопка встала первой; B — уже третья по счёту.
        buttons.list.insert(0, button("Search New", "search-new-7741.exe"));
        run(&mut buttons, &id_of_b);
        assert!(launched("search-b-7741.exe", ""), "запустилась названная кнопка");
        assert!(!launched("search-a-7741.exe", "") && !launched("search-new-7741.exe", ""), "соседние не нажимались");
        // Кнопку убрали — её действие ничего не делает.
        buttons.list.retain(|b| b.name != "Search B");
        let count = || {
            let want = "launch search-b-7741.exe ".to_string();
            sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|w| **w == want).count()
        };
        let before = count();
        run(&mut buttons, &id_of_b);
        assert_eq!(count(), before, "убранная кнопка второй раз не запущена");
        // Две кнопки с одним именем, но разной программой — разные действия.
        let twins = Buttons {
            list: vec![button("Same", "search-x-7741.exe"), button("Same", "search-y-7741.exe")],
            ..Default::default()
        };
        let ids: Vec<String> =
            twins.actions(en, anvil_ui::Lang::En, &Settings::default()).into_iter().map(|a| a.id).collect();
        assert_ne!(ids[0], ids[1]);
    }
}
