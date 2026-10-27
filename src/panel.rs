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
    /// Клавиатура у панели: её вызвали клавишей (показа или сцены) или в ней нажали мышью. Выдвинутая мышью у края (и
    /// показ при запуске) клавиатуру у программы пользователя не забирает: окно остаётся неактивным, пока в нём не нажмут
    /// мышью, — набранное в чужой программе в панель не попадает. Значима, пока панель показана.
    keyboard: bool,
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
        self.keyboard = false;
        self.edge_since = None;
        self.away_since = None;
        self.hold_until = None;
    }

    fn show(&mut self, by_hotkey: bool, now: Instant) {
        self.shown = true;
        self.by_hotkey = by_hotkey;
        // Клавишу нажал сам пользователь — клавиатура у панели сразу; мышь у края этого не значит.
        self.keyboard = by_hotkey;
        self.edge_since = None;
        self.away_since = None;
        self.hold_until = by_hotkey.then(|| now + HOTKEY_HOLD);
    }

    /// Есть ли у панели клавиатура (см. поле `keyboard`); пока она не показана, ответ не важен.
    pub fn keyboard(&self) -> bool {
        self.keyboard
    }

    /// Панели отдают клавиатуру: в ней нажали мышью (само нажатие работает как обычный щелчок) или её открыли щелчком по
    /// плашке. Ответ — взяла только что, а не уже была: окну велят взять фокус один раз.
    pub fn take_keyboard(&mut self) -> bool {
        !std::mem::replace(&mut self.keyboard, true)
    }

    /// Показать, как по клавише панели: клавиша сцены, чтобы вопрос «Оставить?» был виден.
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

/// Мышь у края: в верхних `depth` строках пикселей монитора (хотя бы одна) и между `left` и `right` (`edge_span`).
/// Выше края — уже другой монитор, там панели нет.
pub fn at_edge(cursor: (i32, i32), monitor_top: i32, depth: u32, left: i32, right: i32) -> bool {
    let depth = depth.max(1) as i32;
    cursor.1 >= monitor_top && cursor.1 < monitor_top + depth && cursor.0 >= left && cursor.0 < right
}

/// Где вдоль края мышь выдвигает панель, пиксели `[left, right)`: `zone_pct` % ширины панели по её центру, 0 — весь край
/// монитора; не шире монитора. 100 % — ровно под панелью.
pub fn edge_span(monitor: (i32, i32), panel: (i32, i32), zone_pct: u32) -> (i32, i32) {
    if zone_pct == 0 {
        return monitor;
    }
    let center = (panel.0 + panel.1) / 2;
    let half = ((panel.1 - panel.0) as i64 * zone_pct as i64 / 200) as i32;
    ((center - half).max(monitor.0), (center + half).min(monitor.1))
}

/// Мышь над окном в пикселях `(left, top, right, bottom)`; правый и нижний край не входят.
pub fn inside(cursor: (i32, i32), rect: (i32, i32, i32, i32)) -> bool {
    cursor.0 >= rect.0 && cursor.0 < rect.2 && cursor.1 >= rect.1 && cursor.1 < rect.3
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

    /// Кто дал клавиатуру: мышь у края — нет, клавиша показа и клавиша сцены — да, нажатие мышью внутри — да.
    #[test]
    fn a_panel_pulled_out_by_the_mouse_leaves_the_keyboard_to_the_user_until_a_press() {
        let t0 = Instant::now();
        let mut e = Edge::default();
        let edge = Input { at_edge: true, ..Default::default() };
        e.step(edge, t0, SHOW, HIDE);
        assert!(e.step(edge, at(t0, 160), SHOW, HIDE) && e.shown, "выехала от края");
        assert!(!e.keyboard(), "мышь у края клавиатуру не забирает");
        // Мышь ходит по панели, уходит, потеря фокуса — клавиатура всё равно не появляется сама.
        e.step(Input { over: true, ..Default::default() }, at(t0, 200), SHOW, HIDE);
        assert!(!e.keyboard());
        // Нажали мышью внутри: клавиатура у панели; взяла только что, второе нажатие ничего не меняет.
        assert!(e.take_keyboard());
        assert!(e.keyboard() && !e.take_keyboard());
        // Спряталась — клавиатуры нет; следующий выезд мышью — снова без неё.
        e.step(Input::default(), at(t0, 300), SHOW, HIDE);
        e.step(Input::default(), at(t0, 800), SHOW, HIDE);
        assert!(!e.shown && !e.keyboard());
        e.step(edge, at(t0, 900), SHOW, HIDE);
        e.step(edge, at(t0, 1100), SHOW, HIDE);
        assert!(e.shown && !e.keyboard(), "каждый выезд мышью начинается без клавиатуры");
    }

    #[test]
    fn a_panel_called_by_a_key_takes_the_keyboard_at_once() {
        let t0 = Instant::now();
        // Клавиша показа.
        let mut e = Edge::default();
        e.step(Input { hotkey: true, ..Default::default() }, t0, SHOW, HIDE);
        assert!(e.shown && e.keyboard());
        assert!(!e.take_keyboard(), "уже есть");
        // Вторая клавиша прячет — и клавиатуры нет.
        e.step(Input { hotkey: true, ..Default::default() }, at(t0, 10), SHOW, HIDE);
        assert!(!e.shown && !e.keyboard());
        // Клавиша сцены (`reveal`) при спрятанной панели — тоже сразу; при показанной ничего не меняется.
        e.reveal(at(t0, 20));
        assert!(e.shown && e.keyboard());
        let mut by_mouse = Edge::default();
        let edge = Input { at_edge: true, ..Default::default() };
        by_mouse.step(edge, t0, SHOW, HIDE);
        by_mouse.step(edge, at(t0, 200), SHOW, HIDE);
        by_mouse.reveal(at(t0, 300));
        assert!(
            by_mouse.shown && !by_mouse.keyboard(),
            "клавиша сцены у уже выдвинутой мышью панели клавиатуру не даёт"
        );
        // Показ при запуске — без клавиатуры: окно не должно забрать её у того, кто запустил.
        let mut intro = Edge::default();
        intro.intro(t0);
        assert!(intro.shown && !intro.keyboard());
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
        assert!(at_edge((500, 0), 0, 1, 100, 900));
        assert!(!at_edge((500, -3), 0, 1, 100, 900));
        assert!(!at_edge((500, 1), 0, 1, 100, 900));
        assert!(!at_edge((950, 0), 0, 1, 100, 900));
        // Толщина: верхние строки входят, ниже — нет; 0 считается одной строкой.
        assert!(at_edge((500, 3), 0, 4, 100, 900));
        assert!(!at_edge((500, 4), 0, 4, 100, 900));
        assert!(!at_edge((500, -1), 0, 4, 100, 900));
        assert!(at_edge((500, 0), 0, 0, 100, 900) && !at_edge((500, 1), 0, 0, 100, 900));
    }

    #[test]
    fn edge_span_along_edge() {
        let (mon, panel) = ((0, 2560), (630, 1930));
        assert_eq!(edge_span(mon, panel, 100), panel, "100 % — под панелью, как было");
        assert_eq!(edge_span(mon, panel, 200), (0, 2560), "шире монитора не бывает");
        assert_eq!(edge_span(mon, panel, 150), (305, 2255));
        assert_eq!(edge_span(mon, panel, 50), (955, 1605));
        assert_eq!(edge_span(mon, panel, 0), mon, "0 — весь край");
        // Второй монитор справа: отсчёт не с нуля.
        assert_eq!(edge_span((2560, 4480), (2870, 4170), 0), (2560, 4480));
    }

    #[test]
    fn inside_rect() {
        assert!(inside((10, 0), (0, 0, 120, 5)));
        assert!(!inside((120, 0), (0, 0, 120, 5)) && !inside((10, 5), (0, 0, 120, 5)));
    }
}
