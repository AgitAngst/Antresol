// Лоток файлов: бросить файлы на панель — они лежат тут, пока не уберёшь; щелчок — открыть,
// «Показать в папке» — проводник с выделенным файлом. Список — в настройках (строки `tray=`).
// Перетащить файл из лотка обратно в другую программу пока нельзя.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anvil_ui::Icon;
use anvil_ui::widgets as w;
use eframe::egui::{self, Ui};

use super::{Command, Ctx, Module, row};
use crate::sys;
use crate::texts::Tr;

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
        if let Command::Dropped(files) = cmd {
            add(&mut cx.settings.tray, files);
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let demo: Vec<String> =
            [r"C:\Demo\report.pdf", r"C:\Demo\photo.png", r"C:\Demo\notes.txt"].map(String::from).to_vec();
        let list = if sys::demo() { demo } else { cx.settings.tray.clone() };
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
}
