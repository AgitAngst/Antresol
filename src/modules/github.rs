// Состояние GitHub: последний прогон CI и открытые пулл-реквесты своих репозиториев — только
// чтение. Запросы — в своём потоке и не чаще раза в 5 минут (без токена GitHub даёт 60 в час).
// Щелчок по репозиторию — его страница Actions в браузере. CI отсюда не запускается.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Ui};

use super::{Ctx, Module};
use crate::sys::{self, gh};
use crate::texts::{self, Tr};

/// Как часто перечитывать, пока панель открыта.
const EVERY: Duration = Duration::from_secs(300);

#[derive(Default)]
struct Shared {
    repos: Vec<gh::Repo>,
    busy: bool,
}

#[derive(Default)]
pub struct Github {
    shared: Arc<Mutex<Shared>>,
    asked: Option<Instant>,
    /// Что спрашивали в последний раз: сменился список — спросить сразу.
    asked_for: Vec<String>,
    /// Пишут имя нового репозитория.
    adding: String,
    /// Последние известные репозитории и токен — из настроек (refresh их не видит).
    wanted: Vec<String>,
    token: Option<String>,
}

/// Тон прогона: зелёный — прошёл, красный — упал, акцент — идёт.
pub fn tone(run: &gh::Run) -> Tone {
    match (run.status.as_str(), run.conclusion.as_str()) {
        ("completed", "success") => Tone::Success,
        ("completed", "failure" | "timed_out" | "startup_failure") => Tone::Danger,
        ("completed", _) => Tone::Neutral,
        _ => Tone::Accent,
    }
}

/// Правильное имя репозитория: `владелец/имя` из букв, цифр, `-`, `_`, `.`.
pub fn valid_repo(s: &str) -> bool {
    let ok = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    matches!(s.split_once('/'), Some((o, n)) if ok(o) && ok(n) && !n.contains('/'))
}

impl Github {
    fn fetch(&mut self) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.busy {
                return;
            }
            s.busy = true;
        }
        self.asked = Some(Instant::now());
        self.asked_for = self.wanted.clone();
        let (repos, token, shared) = (self.wanted.clone(), self.token.clone(), self.shared.clone());
        std::thread::spawn(move || {
            // Сбой чтения не должен оставить «занято» навсегда — иначе карточка замрёт.
            let list = std::panic::catch_unwind(|| {
                if sys::demo() { demo() } else { repos.iter().map(|r| gh::read(r, token.as_deref())).collect() }
            });
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            if let Ok(list) = list {
                s.repos = list;
            }
            s.busy = false;
        });
    }
}

fn demo() -> Vec<gh::Repo> {
    let run = |status: &str, conclusion: &str, mins: i64| {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) - mins * 60;
        Some(gh::Run { name: "CI".into(), status: status.into(), conclusion: conclusion.into(), updated: unix_iso(at) })
    };
    vec![
        gh::Repo { name: "demo/app".into(), run: run("completed", "success", 12), open_prs: Some(2), error: None },
        gh::Repo { name: "demo/site".into(), run: run("completed", "failure", 95), open_prs: Some(0), error: None },
        gh::Repo { name: "demo/tool".into(), run: run("in_progress", "", 1), open_prs: Some(1), error: None },
    ]
}

/// Обратное к `iso_to_unix` — только для демо-данных.
fn unix_iso(t: i64) -> String {
    let (days, rem) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem / 60 % 60, rem % 60)
}

impl Module for Github {
    fn key(&self) -> &'static str {
        "github"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.github
    }

    fn min_width(&self) -> f32 {
        260.0
    }

    fn refresh(&mut self, _monitor: &str) {
        if self.wanted.is_empty() && !sys::demo() {
            return;
        }
        if self.asked.is_none_or(|at| at.elapsed() >= EVERY) || self.asked_for != self.wanted {
            self.fetch();
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        self.wanted = cx.settings.github.clone();
        self.token = cx.settings.github_token.clone();
        // Только те, что ещё в настройках: убранный не висит до следующего чтения.
        let repos: Vec<gh::Repo> = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .repos
            .iter()
            .filter(|r| sys::demo() || cx.settings.github.contains(&r.name))
            .cloned()
            .collect();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("gh")
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 40.0)
            .show(ui, |ui| {
                if repos.is_empty() && cx.settings.github.is_empty() && !sys::demo() {
                    w::note(ui, t.github_none);
                }
                for (i, r) in repos.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let tone = r.run.as_ref().map(tone).unwrap_or(if r.error.is_some() {
                            Tone::Warning
                        } else {
                            Tone::Neutral
                        });
                        w::dot(ui, tone);
                        let name = ui.add(
                            egui::Label::new(egui::RichText::new(&r.name).color(p.text)).sense(egui::Sense::click()),
                        );
                        if name.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !sys::demo() {
                            let _ = sys::launch(&format!("https://github.com/{}/actions", r.name), "");
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !sys::demo() && w::icon_button(ui, Icon::Close, t.github_remove).clicked() {
                                remove = Some(i);
                            }
                            if let Some(n) = r.open_prs {
                                w::badge(ui, &texts::prs(cx.lang, n), if n > 0 { Tone::Accent } else { Tone::Neutral });
                            }
                        });
                    });
                    let detail = match (&r.run, &r.error) {
                        (_, Some(e)) => texts::github_error(cx.lang, e),
                        (Some(run), _) => {
                            let ago = gh::iso_to_unix(&run.updated)
                                .map(|at| texts::ago(cx.lang, now - at))
                                .unwrap_or_default();
                            format!(
                                "{} · {} · {ago}",
                                run.name,
                                texts::run_state(cx.lang, &run.status, &run.conclusion)
                            )
                        }
                        (None, None) => t.github_no_runs.to_string(),
                    };
                    w::note(ui, detail);
                    ui.add_space(4.0);
                }
            });
        if let Some(i) = remove {
            let name = repos[i].name.clone();
            cx.settings.github.retain(|g| *g != name);
        }
        // Добавить: «владелец/имя».
        ui.horizontal(|ui| {
            let edit =
                ui.add(egui::TextEdit::singleline(&mut self.adding).hint_text("owner/repo").desired_width(140.0));
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let name = self.adding.trim().to_string();
            let ok = valid_repo(&name);
            if (ui.add_enabled_ui(ok, |ui| w::button(ui, Kind::Ghost, Some(Icon::Plus), t.github_add)).inner.clicked()
                || (enter && ok))
                && !cx.settings.github.contains(&name)
            {
                cx.settings.github.push(name);
                self.adding.clear();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names() {
        assert!(valid_repo("AgitAngst/Antresol"));
        assert!(valid_repo("a-b/c.d_e"));
        assert!(!valid_repo("noslash"));
        assert!(!valid_repo("a/b/c"));
        assert!(!valid_repo("a/ b"));
        assert!(!valid_repo("/b"));
    }

    #[test]
    fn tones() {
        let r = |s: &str, c: &str| gh::Run { status: s.into(), conclusion: c.into(), ..Default::default() };
        assert_eq!(tone(&r("completed", "success")), Tone::Success);
        assert_eq!(tone(&r("completed", "failure")), Tone::Danger);
        assert_eq!(tone(&r("in_progress", "")), Tone::Accent);
        assert_eq!(tone(&r("completed", "cancelled")), Tone::Neutral);
    }

    #[test]
    fn iso_round_trip() {
        for t in [0i64, 951_868_800, 1_792_645_920, 1_700_000_123] {
            assert_eq!(gh::iso_to_unix(&unix_iso(t)), Some(t));
        }
    }
}
