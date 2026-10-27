// Состояние GitHub: последний прогон CI и открытые пулл-реквесты своих репозиториев — только
// чтение. Запросы — в своём потоке; без токена GitHub даёт 60 в час, поэтому на репозиторий не больше 24: прогоны — не чаще
// раза в 5 минут (для плашек или карточки) и пулл-реквесты — не чаще раза в 5 минут и только для карточки (`Pacing`).
// Щелчок по репозиторию — его страница Actions в браузере. CI отсюда не запускается.
// Плашки у края (2.4): последний прогон сменил состояние — «сборка упала» (висит, пока не закроют) или «сборка прошла».
// Пока плашки GitHub включены, прогоны читаются и при спрятанной панели (один запрос на репозиторий, без пулл-реквестов,
// раз в 5 минут); первое чтение ничего не объявляет: плашка только о новом.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Ui};

use super::{Action, Command, Ctx, Module, mine};
use crate::config::Settings;
use crate::plaque::{self, Do, Glyph, Look, Plaque, Say};
use crate::sys::{self, gh};
use crate::texts::{self, Tr};

/// Как часто читать прогоны и, отдельно, пулл-реквесты: каждое не чаще этого — 12 запросов в час на репозиторий, оба вместе 24.
const EVERY: Duration = Duration::from_secs(300);

/// Когда что читать: прогоны и пулл-реквесты ведут свои часы, а сменился список репозиториев — читать сразу.
#[derive(Default)]
struct Pacing {
    asked_runs: Option<Instant>,
    /// Для каких репозиториев прогоны читали в последний раз.
    runs_for: Vec<String>,
    asked_prs: Option<Instant>,
    prs_for: Vec<String>,
}

impl Pacing {
    /// Пора ли читать (прогоны, пулл-реквесты). Прогоны нужны плашкам и карточке, пулл-реквесты — только карточке (`card`).
    fn due(&self, now: Instant, wanted: &[String], card: bool) -> (bool, bool) {
        let late = |asked: Option<Instant>, was_for: &[String]| {
            was_for != wanted || asked.is_none_or(|at| now.saturating_duration_since(at) >= EVERY)
        };
        (late(self.asked_runs, &self.runs_for), card && late(self.asked_prs, &self.prs_for))
    }

    /// Прочитали (или начали читать) это.
    fn asked(&mut self, now: Instant, wanted: &[String], runs: bool, prs: bool) {
        if runs {
            self.asked_runs = Some(now);
            self.runs_for = wanted.to_vec();
        }
        if prs {
            self.asked_prs = Some(now);
            self.prs_for = wanted.to_vec();
        }
    }
}

#[derive(Default)]
struct Shared {
    repos: Vec<gh::Repo>,
    busy: bool,
    /// Сколько чтений закончилось: по нему видно, что есть новое для сверки (плашки).
    version: u64,
}

impl Shared {
    /// Новое чтение. Что не читали (`runs` — прогоны, `prs` — пулл-реквесты), остаётся прежним: короткое чтение не знает ни
    /// числа PR, ни прогона и не стирает их.
    fn apply(&mut self, mut list: Vec<gh::Repo>, runs: bool, prs: bool) {
        for repo in &mut list {
            let Some(old) = self.repos.iter().find(|r| r.name == repo.name) else { continue };
            if !runs {
                repo.run = old.run.clone();
                repo.error = old.error.clone();
            }
            if !prs {
                repo.open_prs = old.open_prs;
                repo.pr_error = old.pr_error.clone();
            }
        }
        self.repos = list;
        self.version += 1;
    }
}

#[derive(Default)]
pub struct Github {
    shared: Arc<Mutex<Shared>>,
    /// Какое чтение уже сверено с известным (`Shared::version`).
    seen: u64,
    /// Что было известно о последнем прогоне каждого репозитория: запись есть — репозиторий уже читали (`None` — прогонов
    /// не было). Нет записи — первое чтение, оно ничего не объявляет.
    known: HashMap<String, Option<gh::Run>>,
    /// Плашки GitHub включены и репозитории есть, мы следим: иначе прежнее чтение сверять не с чем.
    watching: bool,
    /// Когда что читали: прогоны и пулл-реквесты — каждое не чаще раза в 5 минут.
    pacing: Pacing,
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

/// Что случилось с последним прогоном.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunEvent {
    Failed,
    Passed,
}

/// Сменил ли последний прогон состояние так, что об этом говорят: `old` — что знали, `new` — что прочитали. Событие — только
/// законченный прогон: прошёл (`success`) или упал (`failure`, `timed_out`, `startup_failure`); отменён, пропущен и идущий
/// молчат. Тот же прогон в том же состоянии — не событие; другой (по номеру, а нет номера — по состоянию и времени) — событие.
pub fn run_event(old: Option<&gh::Run>, new: Option<&gh::Run>) -> Option<RunEvent> {
    let new = new.filter(|r| r.status == "completed")?;
    let changed = match old {
        None => true,
        Some(o) if o.id != 0 && o.id == new.id => (&o.status, &o.conclusion) != (&new.status, &new.conclusion),
        Some(o) => (o.id, &o.status, &o.conclusion, &o.updated) != (new.id, &new.status, &new.conclusion, &new.updated),
    };
    if !changed {
        return None;
    }
    match new.conclusion.as_str() {
        "success" => Some(RunEvent::Passed),
        "failure" | "timed_out" | "startup_failure" => Some(RunEvent::Failed),
        _ => None,
    }
}

/// Плашка о прогоне: упал — висит, пока не закроют; прошёл — 4 с; «Открыть» ведёт к самому прогону (номер — число, адрес
/// собирается из имени репозитория, записанного в настройках, а не из ответа GitHub); щелчок по плашке — карточка «GitHub».
pub fn run_plaque(repo: &str, run: &gh::Run, event: RunEvent) -> Plaque {
    let url = match run.id {
        0 => format!("https://github.com/{repo}/actions"),
        id => format!("https://github.com/{repo}/actions/runs/{id}"),
    };
    let age = Say::RunAge { branch: run.branch.clone(), at: gh::iso_to_unix(&run.updated) };
    let key = format!("gh:{repo}");
    let plaque = match event {
        RunEvent::Failed => {
            Plaque::new(plaque::Kind::Github, key, Glyph::Branch, Tone::Danger, Say::BuildFailed(repo.to_string()))
                .sticky()
        }
        RunEvent::Passed => {
            Plaque::new(plaque::Kind::Github, key, Glyph::Branch, Tone::Accent, Say::BuildPassed(repo.to_string()))
        }
    };
    plaque.sub(age).button(Say::Open, Look::Secondary, Do::Open(url.into())).external().shelf("github")
}

impl Github {
    /// Читать в потоке: `runs` — прогоны (один запрос на репозиторий), `prs` — открытые пулл-реквесты (ещё один; нужны
    /// только карточке).
    fn fetch(&mut self, runs: bool, prs: bool) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.busy {
                return;
            }
            s.busy = true;
        }
        self.pacing.asked(Instant::now(), &self.wanted, runs, prs);
        let (repos, token, shared) = (self.wanted.clone(), self.token.clone(), self.shared.clone());
        std::thread::spawn(move || {
            // Сбой чтения не должен оставить «занято» навсегда — иначе карточка замрёт.
            let list = std::panic::catch_unwind(|| {
                // Демо и тесты — выдуманный мир, без сети.
                if sys::isolated() {
                    demo()
                } else {
                    let token = token.as_deref();
                    repos
                        .iter()
                        .map(|r| match (runs, prs) {
                            (true, true) => gh::read(r, token),
                            (true, false) => gh::read_runs(r, token),
                            _ => gh::read_prs(r, token),
                        })
                        .collect()
                }
            });
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            if let Ok(list) = list {
                s.apply(list, runs, prs);
            }
            s.busy = false;
        });
    }
}

/// Страница Actions репозитория в браузере; не открылась — уведомление «Не получилось: имя». Одно и то же для щелчка по
/// имени в карточке и для действия из поиска.
fn open_repo(name: &str, cx: &mut Ctx) {
    if let Err(e) = sys::launch(&format!("https://github.com/{name}/actions"), "") {
        eprintln!("open repository failed: {e}");
        cx.toasts.push((format!("{}: {name}", cx.t.failed), Tone::Danger));
    }
}

fn demo() -> Vec<gh::Repo> {
    let run = |status: &str, conclusion: &str, mins: i64| {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) - mins * 60;
        Some(gh::Run {
            id: 7001,
            name: "CI".into(),
            status: status.into(),
            conclusion: conclusion.into(),
            updated: unix_iso(at),
            branch: "main".into(),
        })
    };
    vec![
        gh::Repo {
            name: "demo/app".into(),
            run: run("completed", "success", 12),
            open_prs: Some(2),
            ..Default::default()
        },
        gh::Repo {
            name: "demo/site".into(),
            run: run("completed", "failure", 95),
            open_prs: Some(0),
            ..Default::default()
        },
        gh::Repo { name: "demo/tool".into(), run: run("in_progress", "", 1), open_prs: Some(1), ..Default::default() },
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
        240.0
    }

    /// Карточка на открытой полке: прогоны и пулл-реквесты — каждое, когда подошло его время (не чаще раза в 5 минут).
    fn refresh(&mut self, _monitor: &str) {
        if self.wanted.is_empty() && !sys::demo() {
            return;
        }
        let (runs, prs) = self.pacing.due(Instant::now(), &self.wanted, true);
        if runs || prs {
            self.fetch(runs, prs);
        }
    }

    /// Слежка для плашек: пока они включены и репозитории есть, прогоны читаются и при спрятанной панели.
    fn background(&mut self, settings: &Settings) {
        self.wanted = settings.github.clone();
        self.token = settings.github_token.clone();
        let on = settings.plaques.on(plaque::Kind::Github) && !self.wanted.is_empty();
        if on != self.watching {
            self.watching = on;
            self.known.clear();
        }
        if on && self.pacing.due(Instant::now(), &self.wanted, false).0 {
            self.fetch(true, false);
        }
    }

    /// Прогон сменил состояние — плашка. Первое чтение репозитория (после запуска, после включения плашек, новый репозиторий)
    /// ничего не объявляет; чтение с ошибкой не в счёт — известное остаётся прежним.
    fn events(&mut self, settings: &Settings) -> Vec<Plaque> {
        let (version, repos) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.version, s.repos.clone())
        };
        if version == self.seen {
            return Vec::new();
        }
        self.seen = version;
        let mut out = Vec::new();
        for repo in repos.iter().filter(|r| r.error.is_none() && (sys::demo() || settings.github.contains(&r.name))) {
            let before = self.known.insert(repo.name.clone(), repo.run.clone());
            if let Some(before) = before
                && settings.plaques.on(plaque::Kind::Github)
                && let Some(run) = &repo.run
                && let Some(event) = run_event(before.as_ref(), repo.run.as_ref())
            {
                out.push(run_plaque(&repo.name, run, event));
            }
        }
        // Убранный из настроек репозиторий забываем: добавят снова — первое чтение будет тихим.
        self.known.retain(|name, _| repos.iter().any(|r| &r.name == name));
        out
    }

    /// «Открыть «владелец/имя»» у каждого репозитория: щелчок по его имени — страница Actions в браузере.
    fn actions(&self, t: &Tr, lang: anvil_ui::Lang, settings: &Settings) -> Vec<Action> {
        let names: Vec<String> =
            if sys::demo() { demo().into_iter().map(|r| r.name).collect() } else { settings.github.clone() };
        names
            .iter()
            .map(|name| {
                let title = texts::search_open(lang, name);
                Action::new(format!("github:open:{name}"), title, t.github, Icon::Branch)
                    .words("репозиторий прогон ci actions pull request repo github")
            })
            .collect()
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        let Some(name) = mine(self.key(), id).and_then(|r| r.strip_prefix("open:")) else { return };
        // Только то, что записано в настройках: адрес собирается из имени репозитория, а не из чего попало.
        if sys::demo() || !cx.settings.github.iter().any(|g| g == name) {
            return;
        }
        open_repo(name, cx);
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
        let (mut remove, mut open) = (None, None);
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
                            open = Some(r.name.clone());
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
                    let failure = r.error.as_ref().or(r.pr_error.as_ref());
                    let detail = match (&r.run, failure) {
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
        if let Some(name) = open {
            open_repo(&name, cx);
        }
        if let Some(i) = remove {
            let name = repos[i].name.clone();
            cx.settings.github.retain(|g| *g != name);
        }
        // Добавить: «владелец/имя».
        ui.horizontal(|ui| {
            // Поле уступает кнопке: в узкой карточке «Добавить» не должно уходить за край.
            let font = egui::FontId::proportional(14.0);
            let label = ui.painter().layout_no_wrap(t.github_add.to_string(), font, egui::Color32::PLACEHOLDER);
            let button_w = 12.0 * 2.0 + 16.0 + 7.0 + label.size().x;
            let field_w = (ui.available_width() - button_w - ui.spacing().item_spacing.x).clamp(60.0, 140.0);
            let edit =
                ui.add(egui::TextEdit::singleline(&mut self.adding).hint_text("owner/repo").desired_width(field_w));
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

    fn launched(url: &str) -> bool {
        let want = format!("launch {url} ");
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).contains(&want)
    }

    fn run(github: &mut Github, settings: &mut Settings, id: &str) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        let mut cx = Ctx {
            t: crate::texts::tr(anvil_ui::Lang::En),
            lang: anvil_ui::Lang::En,
            monitor: "",
            toasts: &mut toasts,
            settings,
        };
        github.command(&Command::RunAction(id.to_string()), &mut cx);
        toasts
    }

    #[test]
    fn repositories_are_offered_and_opened_like_a_click_on_the_name() {
        let mut settings =
            Settings { github: vec!["search-test/alpha".into(), "search-test/beta".into()], ..Default::default() };
        let mut github = Github::default();
        let en = crate::texts::tr(anvil_ui::Lang::En);
        let actions = github.actions(en, anvil_ui::Lang::En, &settings);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Open “search-test/alpha”", "Open “search-test/beta”"]);
        let ru = github.actions(crate::texts::tr(anvil_ui::Lang::Ru), anvil_ui::Lang::Ru, &settings);
        assert_eq!(ru[0].title, "Открыть «search-test/alpha»");
        assert!(actions.iter().all(|a| a.enabled && a.place == en.github));
        // Страница Actions репозитория в браузере.
        assert!(run(&mut github, &mut settings, "github:open:search-test/beta").is_empty());
        assert!(launched("https://github.com/search-test/beta/actions"));
        // Чего нет в настройках, то не открывается: адрес не собирается из чего попало.
        run(&mut github, &mut settings, "github:open:evil/repo");
        assert!(!launched("https://github.com/evil/repo/actions"));
        run(&mut github, &mut settings, "quick:wifi");
        // Репозиториев нет — и действий нет.
        assert!(Github::default().actions(en, anvil_ui::Lang::En, &Settings::default()).is_empty());
    }

    fn gh_run(id: u64, status: &str, conclusion: &str, updated: &str) -> gh::Run {
        gh::Run {
            id,
            name: "CI".into(),
            status: status.into(),
            conclusion: conclusion.into(),
            updated: updated.into(),
            branch: "main".into(),
        }
    }

    /// Что говорят о прогоне: законченный, упал или прошёл, и не тот же самый, что знали.
    #[test]
    fn only_a_finished_run_that_changed_is_an_event() {
        let (running, passed, failed) = (
            gh_run(1, "in_progress", "", "t1"),
            gh_run(1, "completed", "success", "t2"),
            gh_run(1, "completed", "failure", "t2"),
        );
        // Шёл — и упал или прошёл.
        assert_eq!(run_event(Some(&running), Some(&failed)), Some(RunEvent::Failed));
        assert_eq!(run_event(Some(&running), Some(&passed)), Some(RunEvent::Passed));
        // Идёт, в очереди, отменён, пропущен — молчим.
        assert_eq!(run_event(Some(&passed), Some(&running)), None);
        assert_eq!(run_event(Some(&running), Some(&gh_run(1, "completed", "cancelled", "t2"))), None);
        assert_eq!(run_event(Some(&running), Some(&gh_run(1, "completed", "skipped", "t2"))), None);
        assert_eq!(run_event(Some(&running), Some(&gh_run(1, "queued", "", "t2"))), None);
        // Тот же прогон в том же состоянии — не событие, даже если «обновлён» другим временем.
        assert_eq!(run_event(Some(&failed), Some(&gh_run(1, "completed", "failure", "t9"))), None);
        // Другой прогон, закончившийся между чтениями: прошёл, потом упал — событие, и наоборот.
        assert_eq!(run_event(Some(&passed), Some(&gh_run(2, "completed", "failure", "t5"))), Some(RunEvent::Failed));
        assert_eq!(run_event(Some(&failed), Some(&gh_run(2, "completed", "success", "t5"))), Some(RunEvent::Passed));
        for kind in ["timed_out", "startup_failure"] {
            assert_eq!(
                run_event(Some(&running), Some(&gh_run(1, "completed", kind, "t"))),
                Some(RunEvent::Failed),
                "{kind}"
            );
        }
        // Репозиторий без прогонов: первый законченный — событие; без прогона — нет.
        assert_eq!(run_event(None, Some(&passed)), Some(RunEvent::Passed));
        assert_eq!(run_event(Some(&passed), None), None);
        // Номера нет (не прочитался): сравниваем по состоянию и времени.
        let (a, b) = (gh_run(0, "completed", "failure", "t1"), gh_run(0, "completed", "failure", "t2"));
        assert_eq!(run_event(Some(&a), Some(&a)), None);
        assert_eq!(run_event(Some(&a), Some(&b)), Some(RunEvent::Failed));
    }

    #[test]
    fn the_plaque_of_a_failed_run_stays_and_opens_the_run_and_a_passed_one_goes() {
        let mut failed = gh_run(7412, "completed", "failure", "2026-10-22T05:12:00Z");
        failed.branch = "release".into();
        let p = run_plaque("demo/site", &failed, RunEvent::Failed);
        assert_eq!(
            (p.key.as_str(), p.glyph, p.tone, p.sticky, p.shelf),
            ("gh:demo/site", Glyph::Branch, Tone::Danger, true, Some("github"))
        );
        assert_eq!(p.title, Say::BuildFailed("demo/site".into()));
        assert_eq!(p.sub, Say::RunAge { branch: "release".into(), at: gh::iso_to_unix("2026-10-22T05:12:00Z") });
        assert_eq!(p.buttons.len(), 1);
        let b = &p.buttons[0];
        assert_eq!((b.label.clone(), b.look, b.external), (Say::Open, Look::Secondary, true));
        assert_eq!(b.does, Do::Open("https://github.com/demo/site/actions/runs/7412".into()), "сам прогон по номеру");
        // Прошёл — тот же ключ (заменит упавшую), без «висит».
        let ok =
            run_plaque("demo/site", &gh_run(7413, "completed", "success", "2026-10-22T06:00:00Z"), RunEvent::Passed);
        assert_eq!((ok.key.as_str(), ok.tone, ok.sticky), ("gh:demo/site", Tone::Accent, false));
        assert_eq!(ok.title, Say::BuildPassed("demo/site".into()));
        // Номера нет — страница Actions репозитория.
        let none = run_plaque("demo/site", &gh_run(0, "completed", "failure", ""), RunEvent::Failed);
        assert_eq!(none.buttons[0].does, Do::Open("https://github.com/demo/site/actions".into()));
    }

    fn repo(name: &str, run: Option<gh::Run>, error: Option<&str>) -> gh::Repo {
        gh::Repo { name: name.into(), run, open_prs: Some(1), error: error.map(String::from), pr_error: None }
    }

    /// Чтение приходит из потока: модуль сверяет его с известным и отдаёт плашки один раз.
    #[test]
    fn a_changed_run_gives_a_plaque_once_and_the_first_read_gives_none() {
        let settings = Settings { github: vec!["demo/site".into(), "demo/tool".into()], ..Settings::default() };
        let mut github = Github::default();
        let read = |github: &Github, list: Vec<gh::Repo>| github.shared.lock().unwrap().apply(list, true, true);
        let running = gh_run(1, "in_progress", "", "t1");
        let failed = gh_run(1, "completed", "failure", "t2");
        // Первое чтение: упавший прогон уже был — молчим.
        read(
            &github,
            vec![repo("demo/site", Some(failed.clone()), None), repo("demo/tool", Some(running.clone()), None)],
        );
        assert!(github.events(&settings).is_empty());
        assert!(github.events(&settings).is_empty(), "нового чтения нет — нечего сверять");
        // Второй репозиторий закончил — плашка о нём одном.
        read(
            &github,
            vec![
                repo("demo/site", Some(failed.clone()), None),
                repo("demo/tool", Some(gh_run(1, "completed", "success", "t3")), None),
            ],
        );
        let told = github.events(&settings);
        assert_eq!(told.len(), 1);
        assert_eq!((told[0].key.as_str(), &told[0].title), ("gh:demo/tool", &Say::BuildPassed("demo/tool".into())));
        // Чтение с ошибкой (лимит, сеть) ничего не объявляет и ничего не стирает: когда вернётся, сверяем с прежним.
        read(&github, vec![repo("demo/site", None, Some("403")), repo("demo/tool", None, Some("403"))]);
        assert!(github.events(&settings).is_empty());
        read(
            &github,
            vec![
                repo("demo/site", Some(gh_run(2, "completed", "failure", "t4")), None),
                repo("demo/tool", Some(gh_run(1, "completed", "success", "t3")), None),
            ],
        );
        let told = github.events(&settings);
        assert_eq!(told.len(), 1, "упал новый прогон demo/site, а demo/tool не менялся");
        assert!(told[0].sticky && told[0].key == "gh:demo/site");
    }

    #[test]
    fn plaques_off_or_a_repository_gone_make_the_next_read_a_quiet_one() {
        let on = Settings { github: vec!["demo/site".into()], ..Settings::default() };
        let off = Settings { plaques: crate::config::Plaques { github: false, ..Default::default() }, ..on.clone() };
        let mut github = Github::default();
        let read = |github: &Github, run: gh::Run| {
            github.shared.lock().unwrap().apply(vec![repo("demo/site", Some(run), None)], true, false)
        };
        read(&github, gh_run(1, "in_progress", "", "t1"));
        assert!(github.events(&on).is_empty());
        // Плашки выключены: прогон закончился — плашки нет, а известное обновилось (включат — не всплывёт старое).
        read(&github, gh_run(1, "completed", "failure", "t2"));
        assert!(github.events(&off).is_empty());
        read(&github, gh_run(1, "completed", "failure", "t2"));
        assert!(github.events(&on).is_empty(), "то же состояние уже известно");
        // Слежка выключалась и включалась — известное забыто, следующее чтение первое.
        read(&github, gh_run(2, "in_progress", "", "t3"));
        github.events(&on);
        github.pacing.asked(Instant::now(), &on.github, true, false);
        github.background(&on);
        assert!(github.watching);
        github.background(&off);
        assert!(!github.watching && github.known.is_empty());
        github.background(&on);
        read(&github, gh_run(2, "completed", "failure", "t4"));
        assert!(github.events(&on).is_empty(), "первое чтение после включения тихое");
        // Репозиторий убрали из ответа — его не помним.
        github.shared.lock().unwrap().apply(Vec::new(), true, true);
        github.events(&on);
        assert!(github.known.is_empty());
    }

    /// Что не читали, то остаётся прежним: чтение прогонов не стирает число PR карточки, а чтение PR — прогон и его ошибку.
    #[test]
    fn a_read_of_only_one_kind_keeps_what_it_did_not_read() {
        let github = Github::default();
        let mut full = repo("demo/site", Some(gh_run(1, "completed", "success", "t")), None);
        full.open_prs = Some(3);
        github.shared.lock().unwrap().apply(vec![full], true, true);
        // Прогоны одни: PR не знает — остаются прежние, а прогон новый.
        let mut runs_only = repo("demo/site", Some(gh_run(2, "completed", "success", "t")), None);
        runs_only.open_prs = None;
        github.shared.lock().unwrap().apply(vec![runs_only], true, false);
        {
            let s = github.shared.lock().unwrap();
            assert_eq!(s.repos[0].open_prs, Some(3));
            assert_eq!(s.repos[0].run.as_ref().map(|r| r.id), Some(2), "а прогон — новый");
            assert_eq!(s.version, 2);
        }
        // PR одни: прогона не знает — остаётся прежний, а число новое (и своя ошибка, если не прочиталось).
        let prs_only = gh::Repo { name: "demo/site".into(), open_prs: Some(5), ..Default::default() };
        github.shared.lock().unwrap().apply(vec![prs_only], false, true);
        {
            let s = github.shared.lock().unwrap();
            assert_eq!(s.repos[0].open_prs, Some(5));
            assert_eq!(s.repos[0].run.as_ref().map(|r| r.id), Some(2), "прогон не тронут");
        }
        let failed = gh::Repo { name: "demo/site".into(), pr_error: Some("403".into()), ..Default::default() };
        github.shared.lock().unwrap().apply(vec![failed], false, true);
        let s = github.shared.lock().unwrap();
        assert_eq!((s.repos[0].open_prs, s.repos[0].pr_error.as_deref()), (None, Some("403")));
        assert!(s.repos[0].error.is_none(), "сбой PR прогонам не мешает: плашки о сборке работают");
    }

    /// Сбой пулл-реквестов (лимит) не глушит плашки о сборке: прогоны читаются отдельно.
    #[test]
    fn a_pull_request_failure_does_not_silence_the_build_plaques() {
        let settings = Settings { github: vec!["demo/site".into()], ..Settings::default() };
        let mut github = Github::default();
        let running = gh_run(1, "in_progress", "", "t1");
        let failed = gh_run(1, "completed", "failure", "t2");
        github.shared.lock().unwrap().apply(vec![repo("demo/site", Some(running), None)], true, true);
        assert!(github.events(&settings).is_empty());
        let pr_limit = gh::Repo { name: "demo/site".into(), pr_error: Some("403".into()), ..Default::default() };
        github.shared.lock().unwrap().apply(vec![pr_limit], false, true);
        assert!(github.events(&settings).is_empty());
        github.shared.lock().unwrap().apply(vec![repo("demo/site", Some(failed), None)], true, false);
        let told = github.events(&settings);
        assert_eq!(told.len(), 1, "упала сборка — плашка есть, хоть PR и не читаются");
        assert!(told[0].sticky);
    }

    /// Не больше 24 запросов в час на репозиторий: прогоны — не чаще раза в 5 минут, пулл-реквесты — тоже, и только пока
    /// карточка на виду. Считаем по часам, которыми владеет тест.
    #[test]
    fn reads_stay_within_twenty_four_requests_an_hour_per_repository() {
        let wanted = vec!["demo/site".to_string()];
        let t0 = Instant::now();
        // За час: окно открыто всё время (читает и прогоны, и PR) или спрятано (плашки читают прогоны).
        for card in [true, false] {
            let mut pacing = Pacing::default();
            let (mut runs_read, mut prs_read) = (0, 0);
            for second in 0..3600u64 {
                let now = t0 + Duration::from_secs(second);
                let (runs, prs) = pacing.due(now, &wanted, card);
                if runs || prs {
                    pacing.asked(now, &wanted, runs, prs);
                }
                runs_read += usize::from(runs);
                prs_read += usize::from(prs);
            }
            assert_eq!(runs_read, 12, "прогоны: раз в 5 минут");
            assert_eq!(prs_read, if card { 12 } else { 0 }, "PR — только для карточки, раз в 5 минут");
            assert!(runs_read + prs_read <= 24);
        }
        // Сменился список репозиториев — читать сразу, обоих видов; а в обычное время — по часам.
        let mut pacing = Pacing::default();
        pacing.asked(t0, &wanted, true, true);
        let soon = t0 + Duration::from_secs(10);
        assert_eq!(pacing.due(soon, &wanted, true), (false, false));
        let more = vec!["demo/site".to_string(), "demo/tool".to_string()];
        assert_eq!(pacing.due(soon, &more, true), (true, true));
        assert_eq!(pacing.due(soon, &more, false), (true, false), "слежка для плашек PR не читает");
        // Прогоны читали слежкой минуту назад, PR не читали вовсе: карточка читает только PR.
        let mut pacing = Pacing::default();
        pacing.asked(t0, &wanted, true, false);
        assert_eq!(pacing.due(t0 + Duration::from_secs(60), &wanted, true), (false, true));
    }

    /// Слежка читает только прогоны, а карточка при открытии добирает пулл-реквесты, не перечитывая прогоны, что слежка прочла
    /// минуту назад: иначе до пяти минут не было бы числа PR.
    #[test]
    fn opening_the_card_after_a_watch_read_reads_only_the_pull_requests() {
        let mut github = Github::default();
        let on = Settings { github: vec!["demo/site".into()], ..Settings::default() };
        github.background(&on);
        assert!(github.pacing.asked_runs.is_some() && github.pacing.asked_prs.is_none(), "слежка — только прогоны");
        let runs_asked = github.pacing.asked_runs;
        let wait = |github: &Github, version: u64| {
            let start = Instant::now();
            while github.shared.lock().unwrap().version < version {
                assert!(start.elapsed() < Duration::from_secs(3), "поток чтения не ответил");
                std::thread::sleep(Duration::from_millis(5));
            }
            // Поток отпускает «занято» сразу после ответа.
            while github.shared.lock().unwrap().busy {
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        wait(&github, 1);
        github.refresh("");
        assert!(github.pacing.asked_prs.is_some(), "карточка прочла PR, не дожидаясь пяти минут");
        assert_eq!(github.pacing.asked_runs, runs_asked, "а прогоны не перечитала");
        wait(&github, 2);
        // И то, и другое уже читали — повторно в ближайшие пять минут не читает.
        github.refresh("");
        assert_eq!(github.shared.lock().unwrap().version, 2);
    }

    /// Слежка идёт, только пока плашки включены и репозитории есть; в выдуманном мире — без сети, выдуманными прогонами.
    #[test]
    fn watching_needs_the_plaques_and_repositories_and_reads_the_invented_world() {
        let mut github = Github::default();
        github.background(&Settings::default());
        assert!(!github.watching && github.pacing.asked_runs.is_none(), "репозиториев нет — читать нечего");
        let off = Settings {
            github: vec!["demo/site".into()],
            plaques: crate::config::Plaques { github: false, ..Default::default() },
            ..Settings::default()
        };
        github.background(&off);
        assert!(!github.watching && github.pacing.asked_runs.is_none(), "плашки выключены — не читаем");
        let on = Settings { github: vec!["demo/site".into()], ..Settings::default() };
        github.background(&on);
        assert!(github.watching && github.pacing.asked_runs.is_some());
        let start = Instant::now();
        while github.shared.lock().unwrap().version == 0 {
            assert!(start.elapsed() < Duration::from_secs(3), "поток чтения не ответил");
            std::thread::sleep(Duration::from_millis(5));
        }
        // Выдуманные demo/app, demo/site, demo/tool: сети нет, а этого репозитория в настройках — «demo/site».
        assert!(github.events(&on).is_empty());
        assert!(github.known.contains_key("demo/site"));
    }
}
