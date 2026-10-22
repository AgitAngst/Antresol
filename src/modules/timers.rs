// Таймеры: быстрые 5, 10, 15 минут и «помидор» (25 работы, 5 отдыха — по кругу). Идут, даже пока
// панель спрятана; дошёл — панель выезжает с уведомлением (звука нет: звук панели исключён).
// Хранятся только в памяти: после выхода из программы таймеров нет.

use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette};
use eframe::egui::{self, Ui};

use super::{Ctx, Module};
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
    /// Сколько осталось на момент `since` (или всё время, пока на паузе).
    left: Duration,
    /// Идёт с этого момента; `None` — на паузе.
    since: Option<Instant>,
}

impl Timer {
    pub fn new(kind: Phase, total: Duration, now: Instant) -> Timer {
        Timer { kind, total, left: total, since: Some(now) }
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

/// Шаг: снять дошедшие таймеры; «помидор» сменяется следующим отрезком. Ответ — что дошло.
pub fn step(timers: &mut Vec<Timer>, now: Instant) -> Vec<Phase> {
    let mut done = Vec::new();
    let mut next = Vec::new();
    timers.retain(|t| {
        if t.paused() || !t.remaining(now).is_zero() {
            return true;
        }
        done.push(t.kind);
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

pub struct Timers {
    list: Vec<Timer>,
    /// Что дошло с прошлого вопроса окна: оно покажет уведомление и выдвинет панель.
    done: Vec<Phase>,
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
    fn label(kind: Phase, t: &Tr) -> &'static str {
        match kind {
            Phase::Plain => t.timer,
            Phase::Work => t.timer_work,
            Phase::Rest => t.timer_rest,
        }
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

    fn attention(&mut self, t: &Tr) -> Option<String> {
        let kind = self.done.pop()?;
        self.done.clear();
        Some(match kind {
            Phase::Plain => t.timer_done.to_string(),
            Phase::Work => t.timer_work_done.to_string(),
            Phase::Rest => t.timer_rest_done.to_string(),
        })
    }

    fn ticking(&self) -> bool {
        self.list.iter().any(|t| !t.paused())
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        let now = Instant::now();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for min in [5u64, 10, 15] {
                if w::button(ui, Kind::Secondary, Some(Icon::Clock), &texts::minutes(cx.lang, min)).clicked() {
                    self.list.push(Timer::new(Phase::Plain, Duration::from_secs(min * 60), now));
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
        egui::ScrollArea::vertical().id_salt("timers").auto_shrink([false, false]).show(ui, |ui| {
            for (i, timer) in self.list.iter_mut().enumerate() {
                let left = timer.remaining(now);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(Self::label(timer.kind, t)).color(p.weak));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::icon_button(ui, Icon::Close, t.timer_cancel).clicked() {
                            remove = Some(i);
                        }
                        let (icon, hint) =
                            if timer.paused() { (Icon::Play, t.timer_resume) } else { (Icon::Pause, t.timer_pause) };
                        if w::icon_button(ui, icon, hint).clicked() {
                            timer.toggle(now);
                        }
                        let color = if timer.paused() { p.faint } else { p.text };
                        ui.label(egui::RichText::new(clock(left)).font(anvil_ui::semibold(16.0)).color(color));
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
        let done = step(&mut list, t0 + WORK);
        assert_eq!(done, vec![Phase::Work, Phase::Plain]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, Phase::Rest, "после работы — отдых");
        let done = step(&mut list, t0 + WORK + REST);
        assert_eq!(done, vec![Phase::Rest]);
        assert_eq!(list[0].kind, Phase::Work, "после отдыха — снова работа");
    }

    #[test]
    fn clocks() {
        assert_eq!(clock(Duration::from_secs(65)), "1:05");
        assert_eq!(clock(Duration::from_millis(500)), "0:01", "неполная секунда — вверх");
        assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    }
}
