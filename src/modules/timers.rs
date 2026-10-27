// Таймеры: быстрые 5, 10, 15 минут и «помидор» (25 работы, 5 отдыха — по кругу). Идут, даже пока
// панель спрятана; дошёл — у края плашка «Таймер дошёл» с «Готово» (панель не выезжает и клавиатуру не берёт; панель
// выдвинута — уведомление; звука нет: звук панели исключён). Хранятся только в памяти: после выхода таймеров нет.

use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Ui};

use super::ShowNoted;
use super::{Action, Command, Ctx, Module, mine};
use crate::config::Settings;
use crate::plaque::{self, Do, Glyph, Look, Plaque, Say};
use crate::texts::{self, Tr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Plain,
    /// «Помидор»: работа или отдых; после конца — следующий отрезок сам.
    Work,
    Rest,
}

#[derive(Debug, Clone)]
pub struct Timer {
    pub kind: Phase,
    pub total: Duration,
    /// Своё имя («чай» из запроса поиска); без него — «Таймер».
    pub name: Option<String>,
    /// Сколько осталось на момент `since` (или всё время, пока на паузе).
    left: Duration,
    /// Идёт с этого момента; `None` — на паузе.
    since: Option<Instant>,
}

impl Timer {
    pub fn new(kind: Phase, total: Duration, now: Instant) -> Timer {
        Timer { kind, total, name: None, left: total, since: Some(now) }
    }

    /// Простой таймер со своим именем.
    pub fn named(name: &str, total: Duration, now: Instant) -> Timer {
        Timer { name: Some(name.to_string()).filter(|n| !n.is_empty()), ..Timer::new(Phase::Plain, total, now) }
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        match self.since {
            Some(at) => self.left.saturating_sub(now.saturating_duration_since(at)),
            None => self.left,
        }
    }

    pub fn paused(&self) -> bool {
        self.since.is_none()
    }

    pub fn toggle(&mut self, now: Instant) {
        match self.since {
            Some(_) => {
                self.left = self.remaining(now);
                self.since = None;
            }
            None => self.since = Some(now),
        }
    }
}

pub const WORK: Duration = Duration::from_secs(25 * 60);
pub const REST: Duration = Duration::from_secs(5 * 60);

/// Шаг: снять дошедшие таймеры; «помидор» сменяется следующим отрезком. Ответ — что дошло и как оно называлось.
pub fn step(timers: &mut Vec<Timer>, now: Instant) -> Vec<(Phase, Option<String>)> {
    let mut done = Vec::new();
    let mut next = Vec::new();
    timers.retain(|t| {
        if t.paused() || !t.remaining(now).is_zero() {
            return true;
        }
        done.push((t.kind, t.name.clone()));
        // Следующий отрезок «помидора» — ровно с конца прошлого, а не с этого кадра: без сдвига за цикл.
        let end = t.since.map(|at| at + t.left).unwrap_or(now);
        match t.kind {
            Phase::Work => next.push(Timer::new(Phase::Rest, REST, end)),
            Phase::Rest => next.push(Timer::new(Phase::Work, WORK, end)),
            Phase::Plain => {}
        }
        false
    });
    timers.extend(next);
    done
}

/// «12:05» или «1:02:03».
pub fn clock(d: Duration) -> String {
    let s = d.as_secs() + u64::from(d.subsec_nanos() > 0);
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// То же для метки на вкладке: минуты с нулём впереди («03:42»), чтобы метка не дёргалась по ширине.
fn badge_clock(d: Duration) -> String {
    let s = d.as_secs() + u64::from(d.subsec_nanos() > 0);
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

/// Плашка «таймер дошёл»: висит, пока не нажмут «Готово» (как висела прежняя плашка таймера); щелчок по ней открывает
/// «Таймеры». Своё имя — своя плашка: два таймера, что дошли вместе, видны оба.
pub fn done_plaque(kind: Phase, name: Option<String>) -> Plaque {
    let (key, say) = match (kind, name) {
        (Phase::Plain, Some(name)) => (format!("timer:{name}"), Say::TimerDone(Some(name))),
        (Phase::Plain, None) => ("timer".to_string(), Say::TimerDone(None)),
        (Phase::Work, _) => ("timer:work".to_string(), Say::WorkDone),
        (Phase::Rest, _) => ("timer:rest".to_string(), Say::RestDone),
    };
    Plaque::new(plaque::Kind::Timer, key, Glyph::Clock, Tone::Accent, say)
        .button(Say::Done, Look::Primary, Do::Done)
        .sticky()
        .shelf("timers")
}

pub struct Timers {
    list: Vec<Timer>,
    /// Что дошло с прошлого вопроса окна (и как оно называлось): окно покажет плашку.
    done: Vec<(Phase, Option<String>)>,
}

impl Default for Timers {
    fn default() -> Self {
        let mut list = Vec::new();
        if crate::sys::demo() {
            // Для снимков — выдуманные таймеры: «помидор» на середине и таймер на паузе.
            let now = Instant::now();
            let mut work = Timer::new(Phase::Work, WORK, now);
            work.left = WORK - Duration::from_secs(9 * 60 + 20);
            let mut plain = Timer::new(Phase::Plain, Duration::from_secs(10 * 60), now);
            plain.left = Duration::from_secs(4 * 60 + 5);
            plain.since = None;
            list = vec![work, plain];
        }
        Timers { list, done: Vec::new() }
    }
}

impl Timers {
    /// Как таймер назван в списке: «Работа», «Отдых», своё имя или «Таймер».
    fn label<'a>(timer: &'a Timer, t: &'a Tr) -> &'a str {
        match (timer.kind, &timer.name) {
            (Phase::Plain, Some(name)) => name,
            (Phase::Plain, None) => t.timer,
            (Phase::Work, _) => t.timer_work,
            (Phase::Rest, _) => t.timer_rest,
        }
    }

    /// Завести таймер: кнопки карточки и действия поиска. `name` — своё имя (из запроса «чай 5»).
    fn start(&mut self, minutes: u64, name: Option<&str>) {
        let total = Duration::from_secs(minutes * 60);
        let now = Instant::now();
        self.list.push(match name {
            Some(name) => Timer::named(name, total, now),
            None => Timer::new(Phase::Plain, total, now),
        });
    }
}

impl Module for Timers {
    fn key(&self) -> &'static str {
        "timers"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.timers
    }

    fn refresh(&mut self, _monitor: &str) {}

    fn background(&mut self, _settings: &crate::config::Settings) {
        let done = step(&mut self.list, Instant::now());
        self.done.extend(done);
    }

    fn events(&mut self, _settings: &Settings) -> Vec<Plaque> {
        std::mem::take(&mut self.done).into_iter().map(|(kind, name)| done_plaque(kind, name)).collect()
    }

    /// Те же кнопки, что в карточке (5, 10, 15 минут, «Помидор»), и таймер из запроса «чай 5» (`timers:new:<минуты>:<имя>`).
    fn command(&mut self, cmd: &Command, _cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        match mine(self.key(), id) {
            Some("pomodoro") => self.list.push(Timer::new(Phase::Work, WORK, Instant::now())),
            Some(what) => {
                if let Some(min) = what.strip_prefix("min:").and_then(|m| m.parse::<u64>().ok()) {
                    self.start(min, None);
                } else if let Some((min, name)) = what.strip_prefix("new:").and_then(|r| r.split_once(':'))
                    && let Ok(min) = min.parse::<u64>()
                    && (1..=999).contains(&min)
                {
                    self.start(min, Some(name.trim()));
                }
            }
            None => {}
        }
    }

    fn actions(&self, t: &Tr, lang: anvil_ui::Lang, _settings: &Settings) -> Vec<Action> {
        let mut out: Vec<Action> = [5u64, 10, 15]
            .into_iter()
            .map(|min| {
                let title = format!("{}: {}", t.timer, texts::minutes(lang, min));
                Action::new(format!("timers:min:{min}"), title, t.timers, Icon::Clock)
                    .words("таймер минуты timer minutes")
            })
            .collect();
        let title = format!("{} — {}", t.pomodoro, texts::minutes(lang, WORK.as_secs() / 60));
        let words = "помидор помодоро работа отдых pomodoro focus work break";
        out.push(Action::new("timers:pomodoro", title, t.timers, Icon::Target).words(words));
        out
    }

    fn ticking(&self) -> bool {
        self.list.iter().any(|t| !t.paused())
    }

    /// Ближайший идущий таймер (не на паузе): сколько осталось и как он называется.
    fn badge(&self, t: &Tr, lang: anvil_ui::Lang) -> Option<(String, String)> {
        let now = Instant::now();
        let timer = self.list.iter().filter(|x| !x.paused()).min_by_key(|x| x.remaining(now))?;
        let left = badge_clock(timer.remaining(now));
        let name = match (timer.kind, &timer.name) {
            (Phase::Plain, Some(name)) => name.clone(),
            (Phase::Plain, None) => texts::minutes(lang, timer.total.as_secs() / 60),
            (Phase::Work, _) => t.timer_work.to_string(),
            (Phase::Rest, _) => t.timer_rest.to_string(),
        };
        let hint = texts::timer_running(lang, &name, &left);
        Some((left, hint))
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        let now = Instant::now();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for min in [5u64, 10, 15] {
                if w::button(ui, Kind::Secondary, Some(Icon::Clock), &texts::minutes(cx.lang, min)).clicked() {
                    self.start(min, None);
                }
            }
            if w::button(ui, Kind::Secondary, Some(Icon::Target), t.pomodoro).on_hover_text(t.pomodoro_hint).clicked() {
                self.list.push(Timer::new(Phase::Work, WORK, now));
            }
        });
        ui.add_space(6.0);
        if self.list.is_empty() {
            w::note(ui, t.timers_none);
            return;
        }
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt("timers").auto_shrink([false, false]).show_noted(ui, |ui| {
            for (i, timer) in self.list.iter_mut().enumerate() {
                let left = timer.remaining(now);
                ui.horizontal(|ui| {
                    // Название уступает времени и кнопкам справа: места не хватает — «на паузе» вместо «Таймер · на паузе»,
                    // а длинное — с многоточием, не под ними.
                    let time_w =
                        ui.painter().layout_no_wrap(clock(left), egui::FontId::monospace(20.0), p.text).size().x;
                    let room =
                        (ui.available_width() - time_w - 30.0 * 2.0 - ui.spacing().item_spacing.x * 3.0).max(30.0);
                    let width_of = |text: &str| {
                        ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(14.0), p.weak).size().x
                    };
                    // На паузе — не только цветом: слово «на паузе» после названия.
                    let name = if timer.paused() {
                        let full = format!("{} · {}", Self::label(timer, t), t.paused);
                        if width_of(&full) <= room { full } else { t.paused.to_string() }
                    } else {
                        Self::label(timer, t).to_string()
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(room, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| ui.add(egui::Label::new(egui::RichText::new(name).color(p.weak)).truncate()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::icon_button(ui, Icon::Close, t.timer_cancel).clicked() {
                            remove = Some(i);
                        }
                        let (icon, hint) =
                            if timer.paused() { (Icon::Play, t.timer_resume) } else { (Icon::Pause, t.timer_pause) };
                        if w::icon_button(ui, icon, hint).clicked() {
                            timer.toggle(now);
                        }
                        let color = if timer.paused() { p.weak } else { p.text };
                        ui.label(
                            egui::RichText::new(clock(left)).font(egui::FontId::monospace(20.0)).strong().color(color),
                        );
                    });
                });
                let done = 1.0 - left.as_secs_f32() / timer.total.as_secs_f32().max(1.0);
                w::progress(ui, Some(done.clamp(0.0, 1.0)), ui.available_width());
                ui.add_space(6.0);
            }
        });
        if let Some(i) = remove {
            self.list.remove(i);
        }
        if self.ticking() {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_and_pause() {
        let t0 = Instant::now();
        let mut t = Timer::new(Phase::Plain, Duration::from_secs(60), t0);
        assert_eq!(t.remaining(t0 + Duration::from_secs(20)), Duration::from_secs(40));
        t.toggle(t0 + Duration::from_secs(20));
        assert!(t.paused());
        assert_eq!(t.remaining(t0 + Duration::from_secs(50)), Duration::from_secs(40), "на паузе не идёт");
        t.toggle(t0 + Duration::from_secs(50));
        assert_eq!(t.remaining(t0 + Duration::from_secs(60)), Duration::from_secs(30));
    }

    #[test]
    fn pomodoro_cycles() {
        let t0 = Instant::now();
        let mut list = vec![Timer::new(Phase::Work, WORK, t0), Timer::new(Phase::Plain, Duration::from_secs(5), t0)];
        let kinds = |done: Vec<(Phase, Option<String>)>| done.into_iter().map(|(kind, _)| kind).collect::<Vec<_>>();
        let done = kinds(step(&mut list, t0 + WORK));
        assert_eq!(done, vec![Phase::Work, Phase::Plain]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, Phase::Rest, "после работы — отдых");
        let done = kinds(step(&mut list, t0 + WORK + REST));
        assert_eq!(done, vec![Phase::Rest]);
        assert_eq!(list[0].kind, Phase::Work, "после отдыха — снова работа");
    }

    #[test]
    fn badge_shows_nearest_running() {
        let t0 = Instant::now();
        let mut timers = Timers { list: Vec::new(), done: Vec::new() };
        let tr = texts::tr(anvil_ui::Lang::Ru);
        assert_eq!(timers.badge(tr, anvil_ui::Lang::Ru), None, "таймеров нет — метки нет");
        let mut paused = Timer::new(Phase::Plain, Duration::from_secs(60), t0);
        paused.toggle(t0);
        timers.list = vec![
            Timer::new(Phase::Plain, Duration::from_secs(15 * 60), t0),
            Timer::new(Phase::Plain, Duration::from_secs(5 * 60), t0),
            paused,
        ];
        let (text, hint) = timers.badge(tr, anvil_ui::Lang::Ru).unwrap();
        // Идёт ближайший (5 минут), а не тот, что на паузе, хоть у него осталось меньше.
        assert!(text.starts_with("04:") || text == "05:00", "{text}");
        assert_eq!(hint, format!("Идёт таймер «5 мин», осталось {text}"));
        assert_eq!(badge_clock(Duration::from_secs(3723)), "1:02:03");
        assert_eq!(badge_clock(Duration::from_secs(5)), "00:05");
    }

    #[test]
    fn clocks() {
        assert_eq!(clock(Duration::from_secs(65)), "1:05");
        assert_eq!(clock(Duration::from_millis(500)), "0:01", "неполная секунда — вверх");
        assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    }

    fn run(timers: &mut Timers, id: &str) {
        run_command(timers, &Command::RunAction(id.to_string()));
    }

    fn run_command(timers: &mut Timers, cmd: &Command) {
        let mut toasts = Vec::new();
        let mut settings = Settings::default();
        let mut cx = Ctx {
            t: texts::tr(anvil_ui::Lang::En),
            lang: anvil_ui::Lang::En,
            monitor: "",
            toasts: &mut toasts,
            settings: &mut settings,
        };
        timers.command(cmd, &mut cx);
    }

    #[test]
    fn search_offers_the_quick_buttons_of_the_card() {
        let timers = Timers { list: Vec::new(), done: Vec::new() };
        let (ru, en) = (texts::tr(anvil_ui::Lang::Ru), texts::tr(anvil_ui::Lang::En));
        let titles = |t: &Tr, lang| -> Vec<String> {
            timers.actions(t, lang, &Settings::default()).into_iter().map(|a| a.title).collect()
        };
        assert_eq!(
            titles(ru, anvil_ui::Lang::Ru),
            ["Таймер: 5 мин", "Таймер: 10 мин", "Таймер: 15 мин", "Помидор — 25 мин"]
        );
        assert_eq!(
            titles(en, anvil_ui::Lang::En),
            ["Timer: 5 min", "Timer: 10 min", "Timer: 15 min", "Pomodoro — 25 min"]
        );
        let actions = timers.actions(en, anvil_ui::Lang::En, &Settings::default());
        assert!(actions.iter().all(|a| a.enabled && a.place == en.timers));
        assert!(actions[3].words.contains("помидор") && actions[3].words.contains("pomodoro"));
    }

    #[test]
    fn search_buttons_start_the_same_timers_as_the_card() {
        let mut timers = Timers { list: Vec::new(), done: Vec::new() };
        run(&mut timers, "timers:min:10");
        run(&mut timers, "timers:pomodoro");
        assert_eq!(timers.list.len(), 2);
        assert_eq!((timers.list[0].kind, timers.list[0].total), (Phase::Plain, Duration::from_secs(600)));
        assert_eq!((timers.list[1].kind, timers.list[1].total), (Phase::Work, WORK));
        assert!(timers.list.iter().all(|t| t.name.is_none() && !t.paused()), "идут сразу, без имени");
        assert!(timers.ticking(), "карточке нужны кадры");
        // Чужое действие и чужая команда — ничего.
        run(&mut timers, "quick:wifi");
        run_command(&mut timers, &Command::Media(crate::sys::media::Action::Next));
        assert_eq!(timers.list.len(), 2);
    }

    #[test]
    fn a_timer_from_the_query_has_its_name_in_the_list_the_tab_and_the_notice() {
        let mut timers = Timers { list: Vec::new(), done: Vec::new() };
        run(&mut timers, "timers:new:5:чай");
        assert_eq!(timers.list.len(), 1);
        let timer = &timers.list[0];
        assert_eq!(timer.kind, Phase::Plain);
        assert_eq!(timer.total, Duration::from_secs(300));
        assert_eq!(timer.name.as_deref(), Some("чай"));
        let (ru, en) = (texts::tr(anvil_ui::Lang::Ru), texts::tr(anvil_ui::Lang::En));
        assert_eq!(Timers::label(timer, ru), "чай", "в списке — своё имя, а не «Таймер»");
        // Метка на вкладке и подсказка к ней называют его.
        let (left, hint) = timers.badge(ru, anvil_ui::Lang::Ru).expect("идёт — метка есть");
        assert_eq!(hint, format!("Идёт таймер «чай», осталось {left}"));
        // Дошёл: плашка со своим именем.
        let told = |timers: &mut Timers, lang: anvil_ui::Lang| -> Vec<String> {
            timers.background(&Settings::default());
            timers.events(&Settings::default()).iter().map(|p| texts::plaque_text(lang, &p.title)).collect()
        };
        timers.list[0].left = Duration::ZERO;
        assert_eq!(told(&mut timers, anvil_ui::Lang::Ru), ["Таймер «чай» дошёл"]);
        assert!(timers.list.is_empty());
        run(&mut timers, "timers:new:5:tea");
        timers.list[0].left = Duration::ZERO;
        assert_eq!(told(&mut timers, anvil_ui::Lang::En), ["Timer “tea” is up"]);
        // Без имени — как у быстрых таймеров.
        run(&mut timers, "timers:new:7:");
        assert_eq!(timers.list[0].name, None);
        assert_eq!(Timers::label(&timers.list[0], en), "Timer");
        timers.list[0].left = Duration::ZERO;
        assert_eq!(told(&mut timers, anvil_ui::Lang::Ru), ["Таймер дошёл"]);
    }

    /// Плашка таймера: висит, пока не нажмут «Готово», открывает «Таймеры»; два таймера, что дошли вместе, — две плашки;
    /// второй раз то же не отдаётся.
    #[test]
    fn a_finished_timer_gives_a_plaque_that_stays_with_a_done_button_and_is_given_once() {
        let t0 = Instant::now();
        let mut timers = Timers { list: Vec::new(), done: Vec::new() };
        timers.list = vec![
            Timer::named("чай", Duration::from_secs(60), t0),
            Timer::new(Phase::Plain, Duration::from_secs(60), t0),
            Timer::new(Phase::Work, WORK, t0),
        ];
        let at_end = t0 + WORK;
        timers.done.extend(step(&mut timers.list, at_end));
        let plaques = timers.events(&Settings::default());
        assert_eq!(plaques.len(), 3, "по плашке на каждый таймер");
        for p in &plaques {
            assert!(p.sticky && p.kind == plaque::Kind::Timer && p.shelf == Some("timers"));
            assert_eq!(p.buttons.len(), 1);
            assert_eq!(
                (p.buttons[0].look, &p.buttons[0].does, &p.buttons[0].label),
                (Look::Primary, &Do::Done, &Say::Done)
            );
        }
        let keys: Vec<&str> = plaques.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["timer:чай", "timer", "timer:work"]);
        assert_eq!(plaques[2].title, Say::WorkDone);
        assert!(timers.events(&Settings::default()).is_empty(), "второй раз то же не отдаётся");
        // Таймер выключить нельзя: событий нет только потому, что их нет, а не из-за настроек.
        let off = Settings {
            plaques: crate::config::Plaques {
                bluetooth: false,
                usb: false,
                github: false,
                downloads: false,
                meetings: false,
            },
            ..Settings::default()
        };
        timers.done.push((Phase::Plain, None));
        assert_eq!(timers.events(&off).len(), 1);
    }

    #[test]
    fn a_bad_timer_request_starts_nothing() {
        let mut timers = Timers { list: Vec::new(), done: Vec::new() };
        for id in [
            "timers:new:0:чай",
            "timers:new:1000:чай",
            "timers:new:x:чай",
            "timers:new:5",
            "timers:min:x",
            "timers:what",
        ] {
            run(&mut timers, id);
        }
        assert!(timers.list.is_empty());
    }
}
