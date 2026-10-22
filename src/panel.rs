// Когда панель выезжает и когда прячется. Чистая логика без окна — чтобы её можно было проверить
// тестами; окно (app.rs) каждый кадр отдаёт сюда, где мышь, и получает «показана / спрятана».

use std::time::{Duration, Instant};

/// Что окно знает о мыши и о себе в этом кадре.
#[derive(Debug, Clone, Copy, Default)]
pub struct Input {
    /// Мышь упёрлась в верхний край монитора напротив панели.
    pub at_edge: bool,
    /// Мышь над панелью.
    pub over: bool,
    /// Нажата горячая клавиша.
    pub hotkey: bool,
    /// Идёт то, что нельзя прерывать: тянут край панели, ждут ответа «Оставить режим?».
    pub busy: bool,
    /// На мониторе игра или видео во весь экран — от края не выезжать.
    pub fullscreen: bool,
    /// Окно панели потеряло фокус (щёлкнули мимо).
    pub lost_focus: bool,
    /// Окно панели в фокусе.
    pub focused: bool,
}

#[derive(Debug, Default)]
pub struct Edge {
    pub shown: bool,
    /// Закреплена — не прячется сама.
    pub pinned: bool,
    /// Показана клавишей и мышь ещё не заходила на неё: прятать по уходу мыши рано.
    by_hotkey: bool,
    edge_since: Option<Instant>,
    away_since: Option<Instant>,
    /// До этого момента не прятать: показ при запуске, вызов клавишей.
    hold_until: Option<Instant>,
}

/// Сколько показывать панель при запуске программы.
pub const INTRO: Duration = Duration::from_millis(2000);
/// Вызвали клавишей, а окно так и не получило фокус и мышь не пришла — спрятать через это время.
pub const HOTKEY_HOLD: Duration = Duration::from_secs(5);

impl Edge {
    pub fn hide(&mut self) {
        self.shown = false;
        self.by_hotkey = false;
        self.edge_since = None;
        self.away_since = None;
        self.hold_until = None;
    }

    fn show(&mut self, by_hotkey: bool, now: Instant) {
        self.shown = true;
        self.by_hotkey = by_hotkey;
        self.edge_since = None;
        self.away_since = None;
        self.hold_until = by_hotkey.then(|| now + HOTKEY_HOLD);
    }

    /// Показать, как по клавише панели: клавиша профиля, чтобы вопрос «Оставить?» был виден.
    pub fn reveal(&mut self, now: Instant) {
        if !self.shown {
            self.show(true, now);
        }
    }

    /// Показать при запуске: видно, что программа работает и где панель; потом прячется сама.
    pub fn intro(&mut self, now: Instant) {
        self.show(false, now);
        self.hold_until = Some(now + INTRO);
    }

    /// Шаг логики. Возвращает, сменилось ли «показана / спрятана».
    pub fn step(&mut self, i: Input, now: Instant, show_delay: Duration, hide_delay: Duration) -> bool {
        let before = self.shown;
        if i.hotkey {
            if self.shown {
                self.hide();
                self.pinned = false;
            } else {
                self.show(true, now);
            }
            return before != self.shown;
        }
        if !self.shown {
            if i.at_edge && !i.fullscreen {
                let since = *self.edge_since.get_or_insert(now);
                if now.duration_since(since) >= show_delay {
                    self.show(false, now);
                }
            } else {
                self.edge_since = None;
            }
            return before != self.shown;
        }
        if i.over {
            self.by_hotkey = false;
            self.hold_until = None;
        }
        let held = self.hold_until.is_some_and(|until| now < until);
        if self.by_hotkey && i.lost_focus && !i.busy && !self.pinned {
            self.hide();
            return true;
        }
        if !held && self.by_hotkey && !i.focused {
            // Фокус окну так и не дали — щелчка мимо не узнаем; прячемся по времени.
            self.by_hotkey = false;
        }
        if i.over || i.busy || self.pinned || self.by_hotkey || held {
            self.away_since = None;
        } else {
            let since = *self.away_since.get_or_insert(now);
            if now.duration_since(since) >= hide_delay {
                self.hide();
            }
        }
        before != self.shown
    }

    /// Нужен ли скорый следующий кадр, даже если мышь стоит: идёт отсчёт показа или ухода.
    pub fn counting(&self) -> bool {
        self.edge_since.is_some() || self.away_since.is_some()
    }

    /// Мышь у края и идёт отсчёт выезда: какая доля задержки прошла (0…1). Полоска наполняется ею.
    pub fn edge_progress(&self, now: Instant, show_delay: Duration) -> Option<f32> {
        let since = self.edge_since?;
        let d = show_delay.as_secs_f32();
        Some(if d <= 0.0 { 1.0 } else { (now.duration_since(since).as_secs_f32() / d).min(1.0) })
    }
}

/// Мышь у края: на верхней строке пикселей монитора и между левым и правым краем панели.
/// Выше края — уже другой монитор, там панели нет.
pub fn at_edge(cursor: (i32, i32), monitor_top: i32, left: i32, right: i32) -> bool {
    cursor.1 == monitor_top && cursor.0 >= left && cursor.0 < right
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: Duration = Duration::from_millis(150);
    const HIDE: Duration = Duration::from_millis(400);

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn shows_after_dwell_at_edge() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        let edge = Input { at_edge: true, ..Default::default() };
        assert!(!e.step(edge, t0, SHOW, HIDE));
        assert!(!e.step(edge, at(t0, 100), SHOW, HIDE));
        assert!(e.step(edge, at(t0, 160), SHOW, HIDE));
        assert!(e.shown);
    }

    #[test]
    fn brushing_the_edge_does_not_show() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        e.step(Input { at_edge: true, ..Default::default() }, t0, SHOW, HIDE);
        e.step(Input::default(), at(t0, 100), SHOW, HIDE);
        e.step(Input { at_edge: true, ..Default::default() }, at(t0, 200), SHOW, HIDE);
        assert!(!e.shown, "отсчёт должен начаться заново");
    }

    #[test]
    fn fullscreen_blocks_edge_but_not_hotkey() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        let i = Input { at_edge: true, fullscreen: true, ..Default::default() };
        e.step(i, t0, SHOW, HIDE);
        e.step(i, at(t0, 500), SHOW, HIDE);
        assert!(!e.shown);
        e.step(Input { hotkey: true, fullscreen: true, ..Default::default() }, at(t0, 600), SHOW, HIDE);
        assert!(e.shown);
    }

    #[test]
    fn hides_after_mouse_leaves() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        e.step(Input { hotkey: true, ..Default::default() }, t0, SHOW, HIDE);
        e.step(Input { over: true, ..Default::default() }, at(t0, 10), SHOW, HIDE);
        e.step(Input::default(), at(t0, 20), SHOW, HIDE);
        assert!(e.shown);
        e.step(Input::default(), at(t0, 430), SHOW, HIDE);
        assert!(!e.shown);
    }

    #[test]
    fn pinned_and_busy_stay() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        e.step(Input { hotkey: true, ..Default::default() }, t0, SHOW, HIDE);
        e.step(Input { over: true, ..Default::default() }, at(t0, 10), SHOW, HIDE);
        e.step(Input { busy: true, ..Default::default() }, at(t0, 20), SHOW, HIDE);
        e.step(Input { busy: true, ..Default::default() }, at(t0, 2000), SHOW, HIDE);
        assert!(e.shown);
        e.pinned = true;
        e.step(Input::default(), at(t0, 3000), SHOW, HIDE);
        e.step(Input::default(), at(t0, 9000), SHOW, HIDE);
        assert!(e.shown);
    }

    #[test]
    fn hotkey_panel_waits_for_mouse_then_hides_on_focus_loss() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        e.step(Input { hotkey: true, ..Default::default() }, t0, SHOW, HIDE);
        let focused = Input { focused: true, ..Default::default() };
        e.step(focused, at(t0, 9000), SHOW, HIDE);
        assert!(e.shown, "мышь далеко, но панель вызвали клавишей и окно в фокусе");
        e.step(Input { lost_focus: true, ..Default::default() }, at(t0, 9100), SHOW, HIDE);
        assert!(!e.shown);
    }

    #[test]
    fn hotkey_panel_without_focus_hides_by_time() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        e.step(Input { hotkey: true, ..Default::default() }, t0, SHOW, HIDE);
        e.step(Input::default(), at(t0, 4000), SHOW, HIDE);
        assert!(e.shown);
        e.step(Input::default(), at(t0, 5100), SHOW, HIDE);
        e.step(Input::default(), at(t0, 5600), SHOW, HIDE);
        assert!(!e.shown);
    }

    #[test]
    fn intro_shows_then_hides() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        e.intro(t0);
        e.step(Input::default(), at(t0, 1500), SHOW, HIDE);
        assert!(e.shown);
        e.step(Input::default(), at(t0, 2100), SHOW, HIDE);
        e.step(Input::default(), at(t0, 2600), SHOW, HIDE);
        assert!(!e.shown);
    }

    #[test]
    fn hotkey_toggles_off_and_unpins() {
        let t0 = Instant::now();
        let mut e = Edge { pinned: true, ..Default::default() };
        e.step(Input { hotkey: true, ..Default::default() }, t0, SHOW, HIDE);
        e.step(Input { hotkey: true, ..Default::default() }, at(t0, 10), SHOW, HIDE);
        assert!(!e.shown && !e.pinned);
    }

    #[test]
    fn edge_progress_fills() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        assert_eq!(e.edge_progress(t0, SHOW), None);
        e.step(Input { at_edge: true, ..Default::default() }, t0, SHOW, HIDE);
        let half = e.edge_progress(at(t0, 75), SHOW).unwrap();
        assert!((half - 0.5).abs() < 0.01);
        assert_eq!(e.edge_progress(at(t0, 900), SHOW), Some(1.0));
    }

    #[test]
    fn edge_zone() {
        assert!(at_edge((500, 0), 0, 100, 900));
        assert!(!at_edge((500, -3), 0, 100, 900));
        assert!(!at_edge((500, 1), 0, 100, 900));
        assert!(!at_edge((950, 0), 0, 100, 900));
    }
}
