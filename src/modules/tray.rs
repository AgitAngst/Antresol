// Лоток файлов: бросить файлы на панель — они лежат тут, пока не уберёшь; щелчок — открыть,
// «Показать в папке» — проводник с выделенным файлом. Список — в настройках (строки `tray=`).
// Перетащить файл из лотка обратно в другую программу пока нельзя.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anvil_ui::Icon;
use anvil_ui::widgets as w;
use eframe::egui::{self, Ui};

use super::{Action, Command, Ctx, Module, mine, row};
use crate::config::Settings;
use crate::sys;
use crate::texts::{self, Tr};

/// Сколько файлов держит лоток.
pub const KEEP: usize = 30;

/// Каких файлов нет: проверяет поток — недоступный сетевой диск отвечает секундами, окно ждать не должно.
#[derive(Default)]
pub struct Tray {
    list: Vec<String>,
    missing: Arc<Mutex<HashSet<String>>>,
    checking: Arc<Mutex<bool>>,
}

/// Добавить файлы наверх лотка: повтор поднимается, не дублируется; больше [`KEEP`] — старые уходят.
pub fn add(tray: &mut Vec<String>, files: &[String]) {
    for f in files.iter().rev() {
        tray.retain(|t| t.to_lowercase() != f.to_lowercase());
        tray.insert(0, f.clone());
    }
    tray.truncate(KEEP);
}

/// Файлы лотка в демо-режиме (выдуманные).
fn demo_files() -> Vec<String> {
    [r"C:\Demo\report.pdf", r"C:\Demo\photo.png", r"C:\Demo\notes.txt"].map(String::from).to_vec()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

impl Module for Tray {
    fn key(&self) -> &'static str {
        "tray"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.tray
    }

    fn refresh(&mut self, _monitor: &str) {
        let mut busy = self.checking.lock().unwrap_or_else(|e| e.into_inner());
        if *busy || self.list.is_empty() {
            return;
        }
        *busy = true;
        let (list, missing, checking) = (self.list.clone(), self.missing.clone(), self.checking.clone());
        std::thread::spawn(move || {
            let gone: HashSet<String> = list.into_iter().filter(|p| !Path::new(p).exists()).collect();
            *missing.lock().unwrap_or_else(|e| e.into_inner()) = gone;
            *checking.lock().unwrap_or_else(|e| e.into_inner()) = false;
        });
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        match cmd {
            Command::Dropped(files) => add(&mut cx.settings.tray, files),
            // «Открыть «…»» из поиска — щелчок по строке лотка: файл открывается, как двойным щелчком; нет файла — ничего.
            Command::RunAction(id) => {
                let rest = mine(self.key(), id).and_then(|r| r.strip_prefix("open:"));
                let Some(i) = rest.and_then(|n| n.parse::<usize>().ok()) else { return };
                let Some(path) = cx.settings.tray.get(i).cloned().filter(|_| !sys::demo()) else { return };
                if self.missing.lock().unwrap_or_else(|e| e.into_inner()).contains(&path) {
                    return;
                }
                if let Err(e) = sys::launch(&path, "") {
                    eprintln!("open from tray failed: {e}");
                    cx.toasts.push((format!("{}: {}", cx.t.failed, file_name(&path)), anvil_ui::Tone::Danger));
                }
            }
            _ => {}
        }
    }

    /// «Открыть «report.pdf»» у каждого файла лотка; нет файла на диске — серое, «нет файла».
    fn actions(&self, t: &Tr, lang: anvil_ui::Lang, settings: &Settings) -> Vec<Action> {
        let list = if sys::demo() { demo_files() } else { settings.tray.clone() };
        let gone = self.missing.lock().unwrap_or_else(|e| e.into_inner()).clone();
        list.iter()
            .enumerate()
            .map(|(i, path)| {
                let missing = !sys::demo() && gone.contains(path);
                let title = texts::search_open(lang, file_name(path));
                Action::new(format!("tray:open:{i}"), title, t.tray, Icon::File)
                    .words(&format!("файл лоток file tray {path}"))
                    .off_if(missing, t.tray_missing)
            })
            .collect()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let list = if sys::demo() { demo_files() } else { cx.settings.tray.clone() };
        self.list = list.clone();
        let gone = self.missing.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if list.is_empty() {
            w::note(ui, t.tray_empty);
            return;
        }
        let (mut open, mut reveal, mut remove) = (None, None, None);
        egui::ScrollArea::vertical().id_salt("tray").auto_shrink([false, false]).show(ui, |ui| {
            for (i, path) in list.iter().enumerate() {
                let id = ui.id().with(("tray", i));
                let missing = !sys::demo() && gone.contains(path);
                let trailing = if missing { t.tray_missing } else { "" };
                let r = row(ui, id, false, file_name(path), trailing, 60.0);
                if r.clone().on_hover_text(path.as_str()).clicked() && !missing {
                    open = Some(i);
                }
                // Справа поверх строки: «в папке» и «убрать».
                let rect = r.rect;
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(egui::pos2(rect.right() - 62.0, rect.top()), rect.max))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                if !missing && w::icon_button(&mut child, Icon::Folder, t.tray_reveal).clicked() {
                    reveal = Some(i);
                }
                if w::icon_button(&mut child, Icon::Close, t.tray_remove).clicked() {
                    remove = Some(i);
                }
            }
        });
        if sys::demo() {
            return;
        }
        if let Some(i) = open
            && let Err(e) = sys::launch(&list[i], "")
        {
            eprintln!("open from tray failed: {e}");
            cx.toasts.push((format!("{}: {}", t.failed, file_name(&list[i])), anvil_ui::Tone::Danger));
        }
        if let Some(i) = reveal
            && let Err(e) = sys::launch("explorer.exe", &format!("/select,\"{}\"", list[i]))
        {
            eprintln!("show in folder failed: {e}");
            cx.toasts.push((format!("{}: {}", t.failed, file_name(&list[i])), anvil_ui::Tone::Danger));
        }
        if let Some(i) = remove {
            cx.settings.tray.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_rules() {
        let mut tray = vec![r"C:\a.txt".to_string()];
        add(&mut tray, &[r"C:\b.txt".into(), r"c:\A.TXT".into()]);
        assert_eq!(tray, [r"C:\b.txt", r"c:\A.TXT"], "новые сверху в своём порядке, повтор без учёта регистра");
        let many: Vec<String> = (0..40).map(|i| format!(r"C:\{i}")).collect();
        add(&mut tray, &many);
        assert_eq!(tray.len(), KEEP);
        assert_eq!(tray[0], r"C:\0");
    }

    #[test]
    fn names() {
        assert_eq!(file_name(r"C:\Demo\report.pdf"), "report.pdf");
        assert_eq!(file_name("x"), "x");
    }

    fn launched(what: &str) -> bool {
        let want = format!("launch {what} ");
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).contains(&want)
    }

    fn run(tray: &mut Tray, settings: &mut Settings, id: &str) -> Vec<(String, anvil_ui::Tone)> {
        let mut toasts = Vec::new();
        let mut cx = Ctx {
            t: texts::tr(anvil_ui::Lang::En),
            lang: anvil_ui::Lang::En,
            monitor: "",
            toasts: &mut toasts,
            settings,
        };
        tray.command(&Command::RunAction(id.to_string()), &mut cx);
        toasts
    }

    #[test]
    fn tray_files_are_offered_and_opened_like_a_click_on_the_row() {
        let (a, b) = (r"C:\search-test\report-4412.pdf", r"C:\search-test\gone-4412.txt");
        let mut settings = Settings { tray: vec![a.into(), b.into()], ..Default::default() };
        let mut tray = Tray::default();
        let en = texts::tr(anvil_ui::Lang::En);
        let actions = tray.actions(en, anvil_ui::Lang::En, &settings);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Open “report-4412.pdf”", "Open “gone-4412.txt”"]);
        assert_eq!(
            tray.actions(texts::tr(anvil_ui::Lang::Ru), anvil_ui::Lang::Ru, &settings)[0].title,
            "Открыть «report-4412.pdf»"
        );
        assert!(actions.iter().all(|x| x.enabled && x.place == en.tray && x.words.contains("search-test")));
        // Файл открывается, как двойным щелчком (запуск — за заслоном: в тестах ничего не открывается).
        assert!(run(&mut tray, &mut settings, "tray:open:0").is_empty());
        assert!(launched(a), "файл не открылся");
        // Нет файла на диске (его проверяет поток карточки): серое «нет файла», и не открывается.
        tray.missing.lock().unwrap().insert(b.to_string());
        let actions = tray.actions(en, anvil_ui::Lang::En, &settings);
        assert!(actions[0].enabled && !actions[1].enabled && actions[1].reason.as_deref() == Some(en.tray_missing));
        run(&mut tray, &mut settings, "tray:open:1");
        assert!(!launched(b), "нет файла — нечего открывать");
        // Номер за списком, чужое действие и пустой лоток — ничего.
        run(&mut tray, &mut settings, "tray:open:9");
        run(&mut tray, &mut settings, "quick:wifi");
        assert!(Tray::default().actions(en, anvil_ui::Lang::En, &Settings::default()).is_empty());
    }
}
