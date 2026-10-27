// Живые плашки у края (2.4): событие показывается узкой плашкой у верхнего края монитора, не выдвигая панель. Здесь —
// что такое плашка и её очередь: чистая логика без окна и без Windows, проверяется тестами. События отдают модули
// (`Module::events`) и `watch.rs` (флешки, загрузки); рисует плашку `app/plaque_ui.rs`; слова собирает `texts.rs`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anvil_ui::Tone;

/// Размер окна плашки, в точках: как плашка таймера была, но шире (значок, два текста, до двух кнопок).
pub const WIDTH: f32 = 372.0;
pub const HEIGHT: f32 = 48.0;
/// Сколько висит обычная плашка; пока мышь над ней, время стоит.
pub const LIFE: Duration = Duration::from_secs(4);
/// Сколько плашек ждёт своей очереди: больше не копится, старые обычные уходят первыми.
pub const WAIT_MAX: usize = 5;

/// Вид события — по нему выключатель в настройках («Живые плашки»); таймер выключить нельзя.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bluetooth,
    Usb,
    Github,
    Downloads,
    Timer,
}

/// Значок плашки. Своих знаков (наушники, флешка, батарея) в наборе нет — рисует `app/plaque_ui.rs`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    Headphones,
    /// Устройство Bluetooth, не наушники: значок «Bluetooth» набора.
    Device,
    Battery,
    Usb,
    Branch,
    Download,
    Clock,
}

/// Слова плашки — ключ и числа, а не готовая строка: смена языка не требует повторной работы, а висящая долго плашка
/// («сборка упала») меняет язык вместе с панелью. Строки — `texts::plaque_text`.
#[derive(Clone, Debug, PartialEq)]
pub enum Say {
    /// Ничего: второй строки нет.
    None,
    /// «Таймер «чай» дошёл» / «Таймер дошёл».
    TimerDone(Option<String>),
    WorkDone,
    RestDone,
    /// «Наушники подключены» — имя устройства.
    Connected(String),
    /// «Bluetooth · заряд 80 %»; заряда не знаем — просто «Bluetooth».
    BtCharge(Option<u8>),
    /// «Мышь разряжается».
    Low(String),
    /// «Заряд 12 %».
    Battery(u8),
    /// «Флешка «DEMO»».
    Usb(String),
    /// «32 ГБ · вставлена» — размер в байтах.
    UsbIn(u64),
    /// «Можно вынимать».
    Safe,
    /// «Не извлечь: диск занят».
    Busy,
    /// «demo/site: сборка упала».
    BuildFailed(String),
    BuildPassed(String),
    /// «main · 5 мин назад»: ветка и когда прогон обновился (секунды с 1970); сколько назад, считается при показе.
    RunAge {
        branch: String,
        at: Option<i64>,
    },
    /// «отчёт.pdf скачан».
    Downloaded(String),
    /// «1,2 МБ · Загрузки» — размер в байтах.
    InDownloads(u64),
    /// «Не получилось: report.pdf».
    Failed(String),
    // Кнопки.
    Open,
    Eject,
    ToTray,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    /// Главное: залита акцентом, одна на плашку («Готово»).
    Primary,
    Secondary,
    /// Тихое: без рамки («Извлечь», «В лоток»).
    Ghost,
}

/// Что делает кнопка плашки; исполняет окно (`App::plaque_do`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    /// Открыть, как двойным щелчком: диск, файл или адрес.
    Open(String),
    /// Положить файл в «Лоток».
    ToTray(String),
    /// Безопасно извлечь съёмный том: корень (`E:\`) и имя для слов.
    Eject { root: String, name: String },
    /// Убрать плашку.
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: Say,
    pub look: Look,
    /// Открывает другое окно или сайт: рядом значок «наружу».
    pub external: bool,
    pub does: Do,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plaque {
    pub kind: Kind,
    /// Повтор с тем же ключом заменяет плашку, а не встаёт следом.
    pub key: String,
    pub glyph: Glyph,
    /// Акцент — обычное, `Danger` — беда, `Warning` — оговорка.
    pub tone: Tone,
    pub title: Say,
    pub sub: Say,
    pub buttons: Vec<Button>,
    /// Висит, пока не закроют (крестиком или своей кнопкой): сборка упала, таймер дошёл.
    pub sticky: bool,
    /// Карточка события: щелчок по плашке открывает панель на её полке (если она стоит на полке).
    pub shelf: Option<&'static str>,
    /// Файл, который можно потянуть с плашки в чат или папку.
    pub file: Option<String>,
}

impl Plaque {
    pub fn new(kind: Kind, key: impl Into<String>, glyph: Glyph, tone: Tone, title: Say) -> Plaque {
        Plaque {
            kind,
            key: key.into(),
            glyph,
            tone,
            title,
            sub: Say::None,
            buttons: Vec::new(),
            sticky: false,
            shelf: None,
            file: None,
        }
    }

    pub fn sub(mut self, sub: Say) -> Plaque {
        self.sub = sub;
        self
    }

    pub fn button(mut self, label: Say, look: Look, does: Do) -> Plaque {
        self.buttons.push(Button { label, look, external: false, does });
        self
    }

    /// Последняя кнопка открывает сайт или другую программу.
    pub fn external(mut self) -> Plaque {
        if let Some(b) = self.buttons.last_mut() {
            b.external = true;
        }
        self
    }

    pub fn sticky(mut self) -> Plaque {
        self.sticky = true;
        self
    }

    pub fn shelf(mut self, card: &'static str) -> Plaque {
        self.shelf = Some(card);
        self
    }

    pub fn file(mut self, path: impl Into<String>) -> Plaque {
        self.file = Some(path.into());
        self
    }

    /// Есть своя кнопка, что убирает плашку («Готово»): крестик тогда не нужен.
    pub fn has_done(&self) -> bool {
        self.buttons.iter().any(|b| b.does == Do::Done)
    }
}

/// Плашка, что висит сейчас.
#[derive(Debug)]
struct Shown {
    plaque: Plaque,
    /// Сколько осталось обычной плашке.
    left: Duration,
    /// Когда считали время в последний раз.
    last: Instant,
    /// Кнопка нажата, ответа ждём («Извлечь»): кнопки не нажимаются, пока не заменит итог.
    busy: bool,
    /// Для снимков: доля, на которой плашка замерла.
    held: Option<f32>,
}

/// Очередь плашек: одна на экране, остальные ждут. Время тикает, пока плашка может быть на экране и мышь не над ней.
#[derive(Debug, Default)]
pub struct Queue {
    shown: Option<Shown>,
    wait: VecDeque<Plaque>,
}

impl Queue {
    /// Новое событие. Тот же ключ, что у показанной или ждущей, — замена на месте (показанная живёт снова целых 4 с);
    /// иначе — в конец очереди. Очередь полна — уходит самая старая обычная из ждущих (важные берегутся), а нет обычных —
    /// самая старая.
    pub fn push(&mut self, plaque: Plaque, now: Instant) {
        if let Some(s) = self.shown.as_mut().filter(|s| s.plaque.key == plaque.key) {
            *s = Shown { plaque, left: LIFE, last: now, busy: false, held: None };
            return;
        }
        if let Some(w) = self.wait.iter_mut().find(|w| w.key == plaque.key) {
            *w = plaque;
            return;
        }
        if self.wait.len() >= WAIT_MAX {
            let at = self.wait.iter().position(|w| !w.sticky).unwrap_or(0);
            self.wait.remove(at);
        }
        self.wait.push_back(plaque);
    }

    /// Показать сразу и держать на доле `fraction` (0…1): для снимков, время не идёт.
    pub fn hold(&mut self, plaque: Plaque, fraction: f32, now: Instant) {
        self.shown = Some(Shown { plaque, left: LIFE, last: now, busy: false, held: Some(fraction.clamp(0.0, 1.0)) });
    }

    /// Шаг времени. `hovered` — мышь над плашкой (время стоит); `can_show` — плашке сейчас можно быть на экране (панель
    /// спрятана, не игра во весь экран и не программа из «Не выезжать»: иначе плашки копятся, а время стоит); `idle` — окно
    /// уже убрало прежнюю плашку, можно ставить следующую.
    pub fn step(&mut self, now: Instant, hovered: bool, can_show: bool, idle: bool) {
        if let Some(s) = self.shown.as_mut() {
            let dt = now.saturating_duration_since(s.last);
            s.last = now;
            if can_show && !hovered && !s.plaque.sticky && s.held.is_none() {
                s.left = s.left.saturating_sub(dt);
            }
            if s.left.is_zero() && !s.plaque.sticky && s.held.is_none() {
                self.shown = None;
            }
        }
        if self.shown.is_none()
            && can_show
            && idle
            && let Some(plaque) = self.wait.pop_front()
        {
            self.shown = Some(Shown { plaque, left: LIFE, last: now, busy: false, held: None });
        }
    }

    pub fn current(&self) -> Option<&Plaque> {
        self.shown.as_ref().map(|s| &s.plaque)
    }

    /// Сколько ещё ждёт своей очереди.
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        self.wait.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.shown.is_none() && self.wait.is_empty()
    }

    /// Сколько осталось обычной плашке: 1 — только показали, 0 — уходит. У важной полоски нет.
    pub fn fraction(&self) -> Option<f32> {
        let s = self.shown.as_ref().filter(|s| !s.plaque.sticky)?;
        Some(s.held.unwrap_or_else(|| s.left.as_secs_f32() / LIFE.as_secs_f32()))
    }

    /// Показанную плашку убрали (кнопка, крестик, щелчок): следующая встанет, когда окно её уберёт.
    pub fn dismiss(&mut self) {
        self.shown = None;
    }

    /// Нажата кнопка, что ждёт ответа: пока итог не пришёл, кнопки не нажимаются.
    pub fn set_busy(&mut self, on: bool) {
        if let Some(s) = self.shown.as_mut() {
            s.busy = on;
        }
    }

    pub fn busy(&self) -> bool {
        self.shown.as_ref().is_some_and(|s| s.busy)
    }

    /// Выдвинулась панель: она и есть то место, где смотрят, — обычные плашки (показанная и ждущие) уходят, их слова скажут
    /// уведомления; важные остаются и показываются после, пока их не закроют.
    pub fn take_ordinary(&mut self) -> Vec<Plaque> {
        let mut out = Vec::new();
        if self.shown.as_ref().is_some_and(|s| !s.plaque.sticky)
            && let Some(s) = self.shown.take()
        {
            out.push(s.plaque);
        }
        let (keep, gone): (VecDeque<Plaque>, VecDeque<Plaque>) = self.wait.drain(..).partition(|p| p.sticky);
        self.wait = keep;
        out.extend(gone);
        out
    }
}

/// Мышь над плашкой у края: окно стоит по центру верхнего края монитора (`monitor` — его прямоугольник в пикселях).
pub fn over(cursor: (i32, i32), monitor: &crate::sys::monitor::PxRect, ppp: f32) -> bool {
    let half = (WIDTH * ppp / 2.0) as i32;
    let center = monitor.left + monitor.width() / 2;
    let rect = crate::sys::monitor::PxRect {
        left: center - half,
        top: monitor.top,
        right: center + half,
        bottom: monitor.top + (HEIGHT * ppp) as i32,
    };
    rect.contains(cursor)
}

/// Плашка с выдуманными данными для снимков (`ANTRESOL_VIEW=strip-plaque-<вид>`) и для документации: «bluetooth», «battery»,
/// «usb», «eject», «busy», «github», «downloads», «timer». Имена — по языку кадра; настоящих устройств, дисков и файлов нет.
pub fn sample(kind: &str, lang: anvil_ui::Lang) -> Option<Plaque> {
    use std::path::Path;

    use crate::modules::bluetooth;
    use crate::modules::github::{self, RunEvent};
    use crate::modules::timers::{self, Phase};
    use crate::sys::{bt, gh, usb};
    use crate::watch;

    let ru = lang == anvil_ui::Lang::Ru;
    let device = |name: &str, battery: u8, audio: bool| bt::Device {
        name: name.into(),
        connected: true,
        battery: Some(battery),
        address: 1,
        audio,
    };
    let drive = usb::Volume { root: "E:\\".into(), label: "DEMO".into(), total: 32 << 30 };
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    Some(match kind {
        "bluetooth" => bluetooth::connected_plaque(&device(if ru { "Наушники" } else { "Headphones" }, 80, true)),
        "battery" => bluetooth::low_plaque(&device(if ru { "Мышь" } else { "Mouse" }, 12, false)),
        "usb" => watch::usb_plaque(&drive),
        "eject" => watch::ejected_plaque(&drive.root, &drive.name(), &Ok(())),
        "busy" => watch::ejected_plaque(&drive.root, &drive.name(), &Err(usb::EjectError::Busy)),
        "github" => {
            let run = gh::Run {
                id: 7001,
                name: "CI".into(),
                status: "completed".into(),
                conclusion: "failure".into(),
                updated: String::new(),
                branch: "main".into(),
            };
            let mut p = github::run_plaque("demo/site", &run, RunEvent::Failed);
            // Прогон закончился минуту назад: «main · 1 мин назад».
            p.sub = Say::RunAge { branch: "main".into(), at: Some(now - 70) };
            p
        }
        "downloads" => {
            let entry = watch::Entry {
                name: if ru { "отчёт.pdf" } else { "report.pdf" }.into(),
                len: 1_258_291,
                modified: None,
            };
            watch::download_plaque(Path::new(r"C:\Demo\Downloads"), &entry)
        }
        "timer" => timers::done_plaque(Phase::Plain, None),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(key: &str) -> Plaque {
        Plaque::new(Kind::Usb, key, Glyph::Usb, Tone::Accent, Say::Safe)
    }

    fn sticky(key: &str) -> Plaque {
        p(key).sticky()
    }

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// Ключи по порядку: показанная, потом ждущие.
    fn keys(q: &Queue) -> Vec<String> {
        q.current().map(|c| c.key.clone()).into_iter().chain(q.wait.iter().map(|w| w.key.clone())).collect()
    }

    /// Шаг, как у окна: плашку можно показать, мышь не над ней, окно свободно.
    fn tick(q: &mut Queue, now: Instant) {
        q.step(now, false, true, true);
    }

    #[test]
    fn an_ordinary_plaque_lives_four_seconds_and_the_next_comes_after_the_window_hides_it() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        q.push(p("b"), t0);
        assert!(q.current().is_none(), "пока шага не было, ничего не показано");
        tick(&mut q, t0);
        assert_eq!(keys(&q), ["a", "b"], "по одной: показана первая, вторая ждёт");
        assert_eq!(q.waiting(), 1);
        tick(&mut q, at(t0, 3900));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("a"), "ещё не 4 с");
        // Ровно 4 с — ушла; окно ещё уезжает (`idle` ложь), следующая ждёт.
        q.step(at(t0, 4000), false, true, false);
        assert!(q.current().is_none());
        assert_eq!(q.waiting(), 1);
        // Окно убрало плашку — встаёт следующая и живёт свои 4 с с этого момента.
        tick(&mut q, at(t0, 4100));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("b"));
        tick(&mut q, at(t0, 7900));
        assert!(q.current().is_some(), "с момента показа прошло 3,8 с");
        tick(&mut q, at(t0, 8100));
        assert!(q.is_empty());
    }

    #[test]
    fn the_bar_runs_down_in_a_straight_line_and_an_important_plaque_has_none() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        assert_eq!(q.fraction(), None, "пока ничего не показано");
        tick(&mut q, t0);
        assert_eq!(q.fraction(), Some(1.0));
        tick(&mut q, at(t0, 1000));
        assert!((q.fraction().unwrap() - 0.75).abs() < 1e-3);
        tick(&mut q, at(t0, 3000));
        assert!((q.fraction().unwrap() - 0.25).abs() < 1e-3);
        let mut q = Queue::default();
        q.push(sticky("s"), t0);
        tick(&mut q, t0);
        assert!(q.current().is_some() && q.fraction().is_none(), "у важной полоски нет");
    }

    #[test]
    fn the_pointer_over_the_plaque_stops_the_clock() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        tick(&mut q, t0);
        q.step(at(t0, 2000), false, true, true);
        // Мышь стоит над плашкой десять секунд: время не идёт.
        q.step(at(t0, 2100), true, true, true);
        q.step(at(t0, 12_000), true, true, true);
        assert!((q.fraction().unwrap() - 0.5).abs() < 1e-3, "{:?}", q.fraction());
        // Ушла — остаётся то, что оставалось: 2 с.
        q.step(at(t0, 12_100), false, true, true);
        q.step(at(t0, 13_900), false, true, true);
        assert!(q.current().is_some());
        q.step(at(t0, 14_100), false, true, true);
        assert!(q.current().is_none());
    }

    #[test]
    fn an_important_plaque_stays_until_it_is_closed_and_the_rest_wait_behind_it() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(sticky("fail"), t0);
        q.push(p("later"), t0);
        tick(&mut q, t0);
        tick(&mut q, at(t0, 3_600_000));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("fail"), "час спустя всё ещё висит");
        assert_eq!(q.waiting(), 1);
        q.dismiss();
        tick(&mut q, at(t0, 3_600_010));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("later"));
    }

    #[test]
    fn the_same_key_replaces_in_place() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        for key in ["a", "b", "c"] {
            q.push(p(key), t0);
        }
        tick(&mut q, t0);
        // Ждущая заменяется на своём месте.
        q.push(Plaque::new(Kind::Usb, "c", Glyph::Usb, Tone::Warning, Say::Busy), t0);
        assert_eq!(keys(&q), ["a", "b", "c"]);
        assert_eq!(q.wait[1].title, Say::Busy);
        // Показанная заменяется и живёт снова целых 4 с.
        tick(&mut q, at(t0, 3000));
        assert!((q.fraction().unwrap() - 0.25).abs() < 1e-3);
        q.set_busy(true);
        q.push(Plaque::new(Kind::Usb, "a", Glyph::Usb, Tone::Accent, Say::Busy), at(t0, 3000));
        assert_eq!(q.current().map(|c| c.title.clone()), Some(Say::Busy));
        assert_eq!(q.fraction(), Some(1.0));
        assert!(!q.busy(), "итог заменил плашку — кнопки снова живые");
        assert_eq!(keys(&q), ["a", "b", "c"], "очередь не выросла");
    }

    #[test]
    fn the_queue_holds_five_and_drops_the_oldest_ordinary_one_first() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("shown"), t0);
        tick(&mut q, t0);
        q.push(sticky("important"), t0);
        for i in 0..7 {
            q.push(p(&format!("n{i}")), t0);
        }
        // Ждёт пять: важная осталась, обычные — четыре самых новых.
        assert_eq!(keys(&q), ["shown", "important", "n3", "n4", "n5", "n6"]);
        // Одни важные: уходит самая старая.
        let mut q = Queue::default();
        for i in 0..7 {
            q.push(sticky(&format!("s{i}")), t0);
        }
        assert_eq!(keys(&q), ["s2", "s3", "s4", "s5", "s6"]);
    }

    #[test]
    fn nothing_comes_up_while_it_may_not_and_the_clock_stands() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        q.push(p("b"), t0);
        // Игра во весь экран: копится, на экран не встаёт.
        q.step(t0, false, false, true);
        q.step(at(t0, 60_000), false, false, true);
        assert!(q.current().is_none() && q.waiting() == 2);
        // Можно — встаёт первая, и время идёт с этого момента.
        q.step(at(t0, 60_100), false, true, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("a"));
        q.step(at(t0, 62_100), false, true, true);
        // Игра началась посреди показа: плашка замерла и ждёт, а не сгорает.
        q.step(at(t0, 62_200), false, false, true);
        q.step(at(t0, 200_000), false, false, true);
        assert!((q.fraction().unwrap() - 0.5).abs() < 1e-3, "{:?}", q.fraction());
        q.step(at(t0, 200_100), false, true, true);
        q.step(at(t0, 202_000), false, true, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("b"), "a досчитала свои 4 с — встала b");
    }

    #[test]
    fn the_next_plaque_waits_while_the_window_has_not_hidden_the_last_one() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        q.push(p("b"), t0);
        q.step(t0, false, true, true);
        q.dismiss();
        q.step(at(t0, 10), false, true, false);
        assert!(q.current().is_none(), "окно ещё уезжает");
        q.step(at(t0, 300), false, true, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("b"));
    }

    #[test]
    fn the_panel_takes_the_ordinary_plaques_and_leaves_the_important_ones() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("shown"), t0);
        q.push(p("a"), t0);
        q.push(sticky("fail"), t0);
        q.push(p("b"), t0);
        tick(&mut q, t0);
        let gone: Vec<String> = q.take_ordinary().into_iter().map(|g| g.key).collect();
        assert_eq!(gone, ["shown", "a", "b"], "по порядку: показанная и ждущие");
        assert!(q.current().is_none());
        assert_eq!(keys(&q), ["fail"]);
        // Важная уже показана — она остаётся показанной (окно спрячет её само, пока панель открыта).
        tick(&mut q, at(t0, 100));
        assert!(q.take_ordinary().is_empty());
        assert_eq!(keys(&q), ["fail"]);
    }

    #[test]
    fn a_plaque_held_for_a_snapshot_does_not_run_down() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.hold(p("a"), 0.62, t0);
        q.step(at(t0, 100_000), false, true, true);
        assert_eq!(q.fraction(), Some(0.62));
        assert!(q.current().is_some());
    }

    #[test]
    fn a_plaque_with_a_button_of_its_own_that_closes_it_needs_no_cross() {
        let done = p("t").button(Say::Done, Look::Primary, Do::Done);
        assert!(done.has_done());
        let open = p("u").button(Say::Open, Look::Secondary, Do::Open("E:\\".into())).external();
        assert!(!open.has_done() && open.buttons[0].external);
    }

    /// Образцы для снимков есть на каждый вид, и в них только выдуманное: корень диска «E:\\», файл в «C:\\Demo».
    #[test]
    fn every_sample_for_the_snapshots_exists_and_is_invented() {
        for lang in [anvil_ui::Lang::Ru, anvil_ui::Lang::En] {
            for kind in ["bluetooth", "battery", "usb", "eject", "busy", "github", "downloads", "timer"] {
                let p = sample(kind, lang).unwrap_or_else(|| panic!("нет образца {kind}"));
                let title = crate::texts::plaque_text(lang, &p.title);
                assert!(!title.is_empty(), "{kind}");
            }
        }
        assert!(sample("nope", anvil_ui::Lang::Ru).is_none());
        // Русские слова — только в русском кадре, английские — в английском.
        let words = |lang, kind| crate::texts::plaque_text(lang, &sample(kind, lang).unwrap().title);
        assert_eq!(words(anvil_ui::Lang::Ru, "bluetooth"), "Наушники подключены");
        assert_eq!(words(anvil_ui::Lang::En, "bluetooth"), "Headphones connected");
        assert_eq!(words(anvil_ui::Lang::Ru, "downloads"), "отчёт.pdf скачан");
        assert_eq!(words(anvil_ui::Lang::En, "downloads"), "report.pdf downloaded");
        assert_eq!(words(anvil_ui::Lang::En, "github"), "demo/site: build failed");
        let github = sample("github", anvil_ui::Lang::En).unwrap();
        assert!(github.sticky);
    }

    #[test]
    fn the_pointer_is_over_the_plaque_only_inside_its_window_at_the_top_edge() {
        use crate::sys::monitor::PxRect;
        let m = PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        // 372 × 48 по центру: от 774 до 1146 по x.
        assert!(over((960, 0), &m, 1.0) && over((774, 47), &m, 1.0));
        assert!(!over((773, 10), &m, 1.0) && !over((1146, 10), &m, 1.0) && !over((960, 48), &m, 1.0));
        // Масштаб 150 %: окно 558 × 72 точки экрана.
        assert!(over((960, 71), &m, 1.5) && !over((960, 72), &m, 1.5));
        assert!(over((681, 10), &m, 1.5) && !over((680, 10), &m, 1.5));
        // Второй монитор правее: окно у его края, а не у первого.
        let second = PxRect { left: 1920, top: 0, right: 3840, bottom: 1080 };
        assert!(over((2880, 10), &second, 1.0) && !over((960, 10), &second, 1.0));
    }
}
