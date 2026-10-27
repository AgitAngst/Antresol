// Встречи (2.5): ближайшая встреча из календаря Google или Outlook по секретной ссылке .ics, «Подключиться» одним щелчком,
// «Дальше сегодня» и, когда на сегодня всё, первая завтрашняя. Календари (до пяти) читаются в потоке модуля раз в 10 минут,
// пока программа работает, и по «Обновить»; ошибка — слова под календарём, прежние встречи остаются до удачи. Разбор файла —
// `ics.rs` (чистые функции), загрузка — `sys::ical`, время ПК — `sys::tz`. За две минуты до встречи со ссылкой — плашка у
// края с «Подключиться» (плашки 2.4); поиск находит «Подключиться к «…»».
// Ссылка на календарь — секрет: живёт в `config.txt` и в памяти, в окне — сокращённо, в журнал, ошибки и плашки не идёт;
// ссылка встречи (в ней бывает пароль) — тоже не в журнал. Демо и тесты — выдуманные встречи, без сети.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Lang, Palette, Tone};
use eframe::egui::{self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{Action, Command, Ctx, Module, anchor, mine, stable_key};
use crate::config::{MAX_CALENDARS, Settings};
use crate::ics::{self, Calendar, Link, Meeting};
use crate::plaque::{self, Do, Glyph, Look, Plaque, Say};
use crate::sys::{self, ical, tz};
use crate::texts::{self, Tr};

/// Как часто читать календари.
const EVERY: Duration = Duration::from_secs(600);
/// За сколько до начала встречи с ссылкой — плашка, секунд.
const WARN: i64 = 120;
/// Строк в «Дальше сегодня» не больше.
const LATER: usize = 4;
/// Сколько встреч с ссылкой находит поиск.
const SEARCH: usize = 3;
const PLATE_H: f32 = 62.0;
const ROW_H: f32 = 30.0;
/// Тесно в плашке: места под текст меньше — «Подключиться» без слова, значком (имя — в подсказке и для диктора).
const PLATE_ROOM: f32 = 190.0;
/// Службе в рамке рядом с названием нужно, чтобы названию осталось не меньше этого, иначе рамки нет (служба — в подсказке).
const TITLE_ROOM: f32 = 90.0;
/// Пояснение внизу карточки — от такой ширины: в две строки оно помещается в нижнюю строку, уже — нет совсем.
const NOTE_ROOM: f32 = 270.0;
/// Нижняя строка карточки: пояснение и значки «Обновить» и «Календари».
const BAR_H: f32 = 34.0;
const DAY: i64 = 86_400;

// ─── Время ──────────────────────────────────────────────────────────────────

/// «Сейчас» выдуманного мира (демо и тесты): 27.10.2026, 10:18 по UTC — «Планёрка через 12 мин», как на макете.
fn demo_now() -> i64 {
    ics::days_from_civil(2026, 10, 27) * DAY + 10 * 3600 + 18 * 60
}

/// Секунды с 1970 года. В демо и в тестах — выдуманное, замершее: «через 12 мин» не уходит от снимка к снимку и не
/// выдаёт настоящего времени этого ПК.
pub fn now() -> i64 {
    if sys::isolated() {
        return demo_now();
    }
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Смещение пояса ПК на момент UTC `utc`, секунды; в выдуманном мире — UTC, чтобы не выдавать пояс этого ПК.
fn local_offset(utc: i64) -> i32 {
    if sys::isolated() { 0 } else { tz::local_offset_at(utc) }
}

/// «10:30» — время по часам ПК.
fn hm(utc: i64) -> String {
    let secs = (utc + i64::from(local_offset(utc))).rem_euclid(DAY);
    format!("{:02}:{:02}", secs / 3600, secs / 60 % 60)
}

/// «10:30–10:45»; конца нет (встреча-момент) — «10:30».
fn range(m: &Meeting) -> String {
    if m.end > m.start { format!("{}–{}", hm(m.start), hm(m.end)) } else { hm(m.start) }
}

/// Сколько минут осталось, с округлением вверх, не меньше одной: «через 1 мин», а не «через 0 мин», пока не началась.
fn minutes_up(secs: i64) -> i64 {
    ((secs.max(0) + 59) / 60).max(1)
}

/// Идёт сейчас или впереди: то, что ещё стоит показывать.
fn alive(m: &Meeting, now: i64) -> bool {
    m.end > now || m.start >= now
}

// ─── Что прочитано ──────────────────────────────────────────────────────────

/// Почему календарь не прочитался — своими словами в карточке; английский код — только в журнал и без адреса.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fail {
    /// Не соединились, не дождались, сервер ответил ошибкой.
    Network,
    /// Ответили, но это не календарь (чужая страница, ссылка отозвана).
    NotCalendar,
}

impl Fail {
    fn text(self, t: &Tr) -> &'static str {
        match self {
            Fail::Network => t.meetings_failed,
            Fail::NotCalendar => t.meetings_not_calendar,
        }
    }
}

/// Один календарь: ссылка (секрет — только в памяти) и последнее удачное чтение.
struct Loaded {
    url: String,
    calendar: Option<Calendar>,
    /// Последнее чтение не вышло: прежние встречи остаются, под календарём — слова.
    error: Option<Fail>,
    /// Ответ был больше предела: прочитано начало.
    partial: bool,
}

#[derive(Default)]
struct Shared {
    loaded: Vec<Loaded>,
    busy: bool,
    /// Сколько ответов принято: по нему видно, что есть новое.
    version: u64,
}

impl Shared {
    /// Принять чтение по ссылке `url`. Не вышло — прежние встречи остаются, к ним добавляется причина.
    fn apply(&mut self, url: &str, result: Result<(Calendar, bool), Fail>) {
        let at = match self.loaded.iter().position(|l| l.url == url) {
            Some(at) => at,
            None => {
                self.loaded.push(Loaded { url: url.to_string(), calendar: None, error: None, partial: false });
                self.loaded.len() - 1
            }
        };
        let entry = &mut self.loaded[at];
        match result {
            Ok((calendar, partial)) => {
                entry.calendar = Some(calendar);
                entry.error = None;
                entry.partial = partial;
            }
            Err(fail) => entry.error = Some(fail),
        }
        self.version += 1;
    }

    fn get(&self, url: &str) -> Option<&Loaded> {
        self.loaded.iter().find(|l| l.url == url)
    }
}

/// Выдуманный календарь для демо и тестов — не настоящие встречи: «Планёрка» (серия по будням, Zoom), «Обзор макетов»
/// (Meet), «Созвон по выпуску» (Teams) и завтрашняя «Ретроспектива» без ссылки; по языку кадра, чтобы в английских не было
/// русских слов. Встречи у всех выдуманных календарей одни и те же (так видно, что одна встреча в двух календарях — одна
/// строка), имя — по номеру календаря: «Работа», «Дом», «Календарь 3».
fn demo_ics(lang: Lang, n: usize) -> String {
    let (standup, review, release, retro) = match lang {
        Lang::Ru => ("Планёрка", "Обзор макетов", "Созвон по выпуску", "Ретроспектива"),
        Lang::En => ("Daily stand-up", "Mockup review", "Release call", "Retro"),
    };
    let name = match (n, lang) {
        (1, Lang::Ru) => "Работа".to_string(),
        (1, Lang::En) => "Work".to_string(),
        (2, Lang::Ru) => "Дом".to_string(),
        (2, Lang::En) => "Home".to_string(),
        (n, Lang::Ru) => format!("Календарь {n}"),
        (n, Lang::En) => format!("Calendar {n}"),
    };
    let lines = [
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        format!("X-WR-CALNAME:{name}"),
        "BEGIN:VEVENT".to_string(),
        "UID:demo-standup".to_string(),
        format!("SUMMARY:{standup}"),
        "DTSTART:20261026T103000Z".to_string(),
        "DTEND:20261026T104500Z".to_string(),
        "RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".to_string(),
        "LOCATION:https://us02web.zoom.us/j/123456789".to_string(),
        "END:VEVENT".to_string(),
        "BEGIN:VEVENT".to_string(),
        "UID:demo-review".to_string(),
        format!("SUMMARY:{review}"),
        "DTSTART:20261027T140000Z".to_string(),
        "DTEND:20261027T150000Z".to_string(),
        "DESCRIPTION:Join: https://meet.google.com/abc-defg-hij".to_string(),
        "END:VEVENT".to_string(),
        "BEGIN:VEVENT".to_string(),
        "UID:demo-release".to_string(),
        format!("SUMMARY:{release}"),
        "DTSTART:20261027T163000Z".to_string(),
        "DTEND:20261027T170000Z".to_string(),
        "URL:https://teams.microsoft.com/l/meetup-join/demo".to_string(),
        "END:VEVENT".to_string(),
        "BEGIN:VEVENT".to_string(),
        "UID:demo-retro".to_string(),
        format!("SUMMARY:{retro}"),
        "DTSTART:20261028T093000Z".to_string(),
        "DTEND:20261028T101500Z".to_string(),
        "END:VEVENT".to_string(),
        "END:VCALENDAR".to_string(),
    ];
    lines.join("\r\n") + "\r\n"
}

/// Пояс из файла без `VTIMEZONE` — в журнал один раз за запуск: имя пояса не секрет.
fn note_zones(n: usize, calendar: &Calendar) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    for zone in calendar.unknown_zones() {
        if seen.len() < 50 && !seen.contains(&zone) {
            eprintln!("calendar {n}: time zone {zone:?} has no VTIMEZONE in the file, read as local time");
            seen.push(zone);
        }
    }
}

/// Прочитать календарь по ссылке: (разобранный, «взято только начало»). Демо и тесты — выдуманный, без сети. В журнал — только
/// номер календаря и код ошибки, без адреса.
fn load(url: &str, lang: Lang, n: usize) -> Result<(Calendar, bool), Fail> {
    let (text, partial) = if sys::isolated() {
        (demo_ics(lang, n), false)
    } else {
        let (host, port, path) = ics::split_url(url).ok_or(Fail::NotCalendar)?;
        let got = ical::get(&host, port, &path).map_err(|e| {
            eprintln!("calendar {n}: {e}");
            match e {
                ical::Fail::Status(code) if (400..500).contains(&code) => Fail::NotCalendar,
                _ => Fail::Network,
            }
        })?;
        let mut body = got.body;
        // Отрезанный на полуслове хвост не читаем: до последней целой строки.
        if got.truncated
            && let Some(at) = body.iter().rposition(|b| *b == b'\n')
        {
            body.truncate(at + 1);
        }
        (String::from_utf8_lossy(&body).into_owned(), got.truncated)
    };
    let calendar = Calendar::parse(&text, now()).ok_or_else(|| {
        eprintln!("calendar {n}: the answer is not a calendar");
        Fail::NotCalendar
    })?;
    note_zones(n, &calendar);
    Ok((calendar, partial))
}

// ─── Что показывать ─────────────────────────────────────────────────────────

/// Что в карточке: ближайшая (идущая или следующая сегодня), «Дальше сегодня» и — когда сегодня больше нет — первая
/// завтрашняя.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Agenda {
    pub next: Option<Meeting>,
    pub later: Vec<Meeting>,
    pub tomorrow: Option<Meeting>,
}

/// Из встреч по порядку (`upcoming`, сегодня и завтра) — то, что видно в `now`: законченные уже не в счёт; сегодня —
/// до `today_end` (местной полуночи).
pub fn agenda(upcoming: &[Meeting], now: i64, today_end: i64) -> Agenda {
    let live: Vec<&Meeting> = upcoming.iter().filter(|m| alive(m, now)).collect();
    let (today, rest): (Vec<&Meeting>, Vec<&Meeting>) = live.into_iter().partition(|m| m.start < today_end);
    Agenda {
        next: today.first().map(|m| (*m).clone()),
        later: today.iter().skip(1).take(LATER).map(|m| (*m).clone()).collect(),
        tomorrow: if today.is_empty() { rest.first().map(|m| (*m).clone()) } else { None },
    }
}

/// Встречи сегодня и завтра из всех календарей, посчитанные на минуту `minute`.
struct Cache {
    minute: i64,
    /// Какое чтение учтено (`Shared::version`) и для каких ссылок.
    version: u64,
    wanted: Vec<String>,
    /// Конец сегодняшнего дня по часам ПК.
    today_end: i64,
    upcoming: Vec<Meeting>,
}

/// Ключ встречи: по нему плашка и поиск узнают именно её и именно этот повтор.
fn meeting_key(m: &Meeting) -> String {
    stable_key(&[&m.uid, &m.title, &m.start.to_string()])
}

fn title_of<'a>(m: &'a Meeting, t: &'a Tr) -> &'a str {
    if m.title.is_empty() { t.media_untitled } else { &m.title }
}

/// «Место» встречи для подсказки: ссылка вместо места (в ней бывает пароль) не показывается — о ней говорит служба.
fn place_of(m: &Meeting) -> &str {
    if m.place.contains("://") { "" } else { &m.place }
}

/// Плашка за две минуты до встречи: «Планёрка через 2 мин», «10:30 · Zoom», «Подключиться» (открывает проверенную
/// ссылку), крестик; висит, пока не закроют или не начнётся встреча (`expires`).
pub fn meeting_plaque(m: &Meeting, link: &Link, time: &str, expires: Instant) -> Plaque {
    Plaque::new(
        plaque::Kind::Meeting,
        format!("meet:{}", meeting_key(m)),
        Glyph::Calendar,
        Tone::Accent,
        Say::MeetingIn { title: m.title.clone(), start: m.start },
    )
    .sub(Say::MeetingAt { time: time.to_string(), service: link.service })
    .button(Say::Join, Look::Primary, Do::Open(link.url.clone().into()))
    .external()
    .sticky()
    .expires(expires)
    .shelf("meetings")
}

/// Плашка с выдуманной встречей для снимков (`strip-plaque-meeting`) и документации: через ~100 секунд, Zoom.
pub fn sample_plaque(lang: Lang) -> Plaque {
    let start = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + 100;
    let title = if lang == Lang::Ru { "Планёрка" } else { "Daily stand-up" };
    let m =
        Meeting { uid: "demo".into(), title: title.into(), place: String::new(), start, end: start + 900, link: None };
    let link = Link { url: "https://us02web.zoom.us/j/123456789".into(), service: ics::Service::Zoom };
    meeting_plaque(&m, &link, "10:30", Instant::now() + Duration::from_secs(100))
}

/// Открыть ссылку встречи: ещё раз проверенную; не открылась — уведомление «Не получилось: Zoom». В журнал — без ссылки.
fn open(link: &Link, cx: &mut Ctx) {
    let Some(link) = ics::check_link(&link.url) else { return };
    if let Err(e) = sys::launch(&link.url, "") {
        eprintln!("open meeting link failed: {e}");
        cx.toasts.push((format!("{}: {}", cx.t.failed, texts::service_name(cx.lang, link.service)), Tone::Danger));
    }
}

// ─── Значок ─────────────────────────────────────────────────────────────────

/// Календарь в квадрате `rect`: корпус, полоса заголовка и два кольца — в сетке 24 и той же толщине линии, что у значков
/// набора (своего значка календаря в kit-v0.6.1 нет).
pub fn paint_calendar(painter: &egui::Painter, rect: Rect, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - Vec2::splat(12.0) * scale;
    let at = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;
    let stroke = Stroke::new((1.7 * scale).max(1.2), color);
    painter.add(egui::Shape::rect_stroke(
        Rect::from_min_max(at(3.5, 5.5), at(20.5, 20.5)),
        2.0 * scale,
        stroke,
        StrokeKind::Middle,
    ));
    painter.add(egui::Shape::line(vec![at(3.5, 10.5), at(20.5, 10.5)], stroke));
    painter.add(egui::Shape::line(vec![at(8.0, 3.0), at(8.0, 7.0)], stroke));
    painter.add(egui::Shape::line(vec![at(16.0, 3.0), at(16.0, 7.0)], stroke));
}

/// Подложка ближайшей встречи: `raised` со скруглением 8 и полоской 3 цвета `accent` слева по закруглению угла (как
/// внутренняя тень макета).
fn paint_plate(painter: &egui::Painter, rect: Rect, p: &Palette) {
    painter.rect_filled(rect, 8, p.raised);
    let corner = rect.intersect(Rect::from_min_max(rect.min, Pos2::new(rect.left() + 12.0, rect.bottom())));
    let clipped = painter.with_clip_rect(corner.intersect(painter.clip_rect()));
    clipped.rect_filled(rect, 8, p.accent);
    clipped.rect_filled(rect.translate(Vec2::new(3.0, 0.0)), 8, p.raised);
}

/// Текст одной строкой, с многоточием, если не помещается в `room`.
fn line(ui: &Ui, text: &str, font: FontId, color: Color32, room: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room.max(20.0));
    ui.painter().layout_job(job)
}

/// Ширина текста одной строкой.
fn width_of(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER).size().x
}

/// Строка-предупреждение под полем: значок `Warning` и слова цветом `warning`.
fn warn_line(ui: &mut Ui, text: &str) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        anvil_ui::icons::paint(ui.painter(), rect, Icon::Warning, p.warning);
        ui.label(RichText::new(text).size(12.5).color(p.warning));
    });
}

// ─── Модуль ─────────────────────────────────────────────────────────────────

pub struct Meetings {
    shared: Arc<Mutex<Shared>>,
    /// Когда начали читать в последний раз и по каким ссылкам: другие ссылки — читать сразу.
    asked: Option<Instant>,
    asked_for: Vec<String>,
    /// Ссылки и язык — из настроек (`background` и `ui` их обновляют).
    wanted: Vec<String>,
    lang: Lang,
    cache: Option<Cache>,
    /// Ключи встреч, о которых уже сказала плашка: повтор не плодит вторую, закрытая крестиком больше не показывается.
    told: HashSet<String>,
    /// Что набрано в поле «Ссылка на календарь»: пока не добавили, только здесь.
    adding: String,
    /// Вместо встреч — список календарей: убрать, добавить (правый щелчок по карточке или значок «Календари»).
    manage: bool,
    /// Когда в последний раз в поле ссылки был фокус: Esc тогда отменяет ввод, а не прячет панель.
    typing_at: Option<Instant>,
}

impl Default for Meetings {
    fn default() -> Self {
        Meetings {
            shared: Arc::default(),
            asked: None,
            asked_for: Vec::new(),
            wanted: Vec::new(),
            lang: Lang::default(),
            cache: None,
            told: HashSet::new(),
            adding: String::new(),
            // Для снимков: `ANTRESOL_VIEW=meetings-manage` — сразу список календарей (только в демо-режиме).
            manage: sys::demo() && std::env::var("ANTRESOL_VIEW").as_deref() == Ok("meetings-manage"),
            typing_at: None,
        }
    }
}

impl Meetings {
    fn sync(&mut self, settings: &Settings, lang: Lang) {
        self.lang = lang;
        if self.wanted != settings.calendars {
            self.wanted = settings.calendars.clone();
        }
    }

    /// Читать календари в потоке: все по очереди; каждый ответ принимается сразу, как пришёл. Идёт чтение — второе не
    /// начинается.
    fn fetch(&mut self) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.busy {
                return;
            }
            s.busy = true;
            // Убранный календарь не висит в памяти.
            s.loaded.retain(|l| self.wanted.contains(&l.url));
        }
        self.asked = Some(Instant::now());
        self.asked_for = self.wanted.clone();
        let (urls, lang, shared) = (self.wanted.clone(), self.lang, self.shared.clone());
        std::thread::spawn(move || {
            for (i, url) in urls.iter().enumerate() {
                // Сбой разбора не должен оставить «занято» навсегда.
                if let Ok(result) = std::panic::catch_unwind(|| load(url, lang, i + 1)) {
                    shared.lock().unwrap_or_else(|e| e.into_inner()).apply(url, result);
                }
            }
            shared.lock().unwrap_or_else(|e| e.into_inner()).busy = false;
        });
    }

    /// Пора ли читать: раз в 10 минут и сразу, когда список ссылок сменился.
    fn due(&self, now: Instant) -> bool {
        !self.wanted.is_empty()
            && (self.asked_for != self.wanted || self.asked.is_none_or(|at| now.saturating_duration_since(at) >= EVERY))
    }

    /// Встречи сегодня и завтра из всех календарей по часам ПК: как у «Часов», раз в минуту.
    fn compute(&self, now: i64) -> Cache {
        let local = |utc: i64| local_offset(utc);
        let day = (now + i64::from(local(now))).div_euclid(DAY) * DAY;
        let today_end = ics::wall_to_utc(day + DAY, &local);
        let tomorrow_end = ics::wall_to_utc(day + 2 * DAY, &local);
        let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let mut upcoming: Vec<Meeting> = Vec::new();
        for url in &self.wanted {
            if let Some(calendar) = s.get(url).and_then(|l| l.calendar.as_ref()) {
                upcoming.extend(calendar.meetings(now, tomorrow_end, &local));
            }
        }
        upcoming.sort_by(|a, b| (a.start, a.end, &a.title).cmp(&(b.start, b.end, &b.title)));
        // Одна встреча в двух календарях — одна строка.
        upcoming.dedup_by(|b, a| a.start == b.start && a.end == b.end && a.title == b.title);
        Cache { minute: now.div_euclid(60), version: s.version, wanted: self.wanted.clone(), today_end, upcoming }
    }

    /// Пересчитать, если минута сменилась, пришло чтение или сменился список ссылок.
    fn refresh_cache(&mut self, now: i64) {
        let version = self.shared.lock().unwrap_or_else(|e| e.into_inner()).version;
        let fresh = self
            .cache
            .as_ref()
            .is_some_and(|c| c.minute == now.div_euclid(60) && c.version == version && c.wanted == self.wanted);
        if !fresh {
            self.cache = Some(self.compute(now));
        }
    }

    /// Имя календаря: `X-WR-CALNAME`, нет — сайт ссылки («calendar.google.com»).
    fn label_of(&self, url: &str) -> String {
        let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.get(url)
            .and_then(|l| l.calendar.as_ref())
            .and_then(|c| c.name.clone())
            .unwrap_or_else(|| ics::short_url(url).split('/').next().unwrap_or_default().to_string())
    }

    /// Плашки за две минуты до начала. Первое чтение, что показывает уже идущее, ничего не объявляет: плашка только о
    /// предстоящем.
    fn events_at(&mut self, now: i64, settings: &Settings) -> Vec<Plaque> {
        self.refresh_cache(now);
        // Давно прошедшее из «уже сказанного» забываем.
        self.told.retain(|k| self.cache.as_ref().is_some_and(|c| c.upcoming.iter().any(|m| meeting_key(m) == *k)));
        if !settings.plaques.on(plaque::Kind::Meeting) {
            return Vec::new();
        }
        let Some(cache) = &self.cache else { return Vec::new() };
        let mut out = Vec::new();
        for m in &cache.upcoming {
            let left = m.start - now;
            let Some(link) = m.link.as_ref().filter(|_| left > 0 && left <= WARN) else { continue };
            if self.told.insert(meeting_key(m)) {
                out.push(meeting_plaque(m, link, &hm(m.start), Instant::now() + Duration::from_secs(left as u64)));
            }
        }
        out
    }

    // ─── Вид ────────────────────────────────────────────────────────────────

    /// Поле «Ссылка на календарь» и «Добавить»; под ним — сокращённая ссылка, когда она годна, и «Нужна ссылка вида…», когда
    /// нет. Набранное видно точками (ссылка — секрет, и показывать её целиком незачем): проверить можно по сокращённому.
    fn add_row(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        let parsed = ics::calendar_url(&self.adding);
        let full = cx.settings.calendars.len() >= MAX_CALENDARS;
        let ok = parsed.is_some() && !full && !sys::demo();
        let was_typing = self.typing();
        ui.horizontal(|ui| {
            // Поле уступает кнопке: в узкой карточке «Добавить» не уходит за край.
            let button_w = 12.0 * 2.0 + 16.0 + 7.0 + width_of(ui, t.meetings_add, FontId::proportional(14.0));
            let field_w = (ui.available_width() - button_w - ui.spacing().item_spacing.x).max(60.0);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.adding)
                    .password(true)
                    .hint_text(t.meetings_link)
                    .desired_width(field_w),
            );
            anchor(ui, edit.rect, "meetings-field");
            if edit.has_focus() {
                self.typing_at = Some(Instant::now());
            }
            // Esc отменяет ввод, а не прячет панель: клавишу забираем (окно проверит её уже после карточки). egui снимает
            // фокус с поля на самой Esc, до карточки, — поэтому смотрим и на то, что в поле печатали только что.
            if (edit.has_focus() || was_typing)
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.adding.clear();
                edit.surrender_focus();
                self.typing_at = None;
            }
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let add = ui.add_enabled_ui(ok, |ui| w::button(ui, Kind::Ghost, Some(Icon::Plus), t.meetings_add)).inner;
            anchor(ui, add.rect, "meetings-add");
            if (add.clicked() || (enter && ok))
                && let Some(url) = &parsed
                && ok
                && !cx.settings.calendars.contains(url)
            {
                cx.settings.calendars.push(url.clone());
                self.adding.clear();
                // Новый календарь читается сразу, а не через 10 минут.
                self.asked = None;
            }
        });
        match (&parsed, self.adding.trim().is_empty()) {
            (Some(url), _) => {
                ui.label(RichText::new(ics::short_url(url)).font(FontId::monospace(12.0)).color(p.weak));
            }
            (None, false) => warn_line(ui, t.meetings_link_bad),
            (None, true) => {}
        }
    }

    /// Календарей нет: как подключить — слова, поле ссылки и где её взять в Google и Outlook.
    fn empty_ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        egui::ScrollArea::vertical().id_salt("meetings-empty").auto_shrink([false, false]).show(ui, |ui| {
            w::note(ui, t.meetings_empty);
            ui.add_space(6.0);
            self.add_row(ui, cx);
            ui.add_space(8.0);
            w::note(ui, t.meetings_google);
            w::note(ui, t.meetings_outlook);
        });
    }

    /// Список календарей: имя и сокращённая ссылка, слова ошибки, крестик «Убрать календарь «…»»; ниже поле добавления.
    fn manage_ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let (t, lang) = (cx.t, cx.lang);
        let p = Palette::of(ui);
        let urls = cx.settings.calendars.clone();
        let (mut remove, mut done) = (None, false);
        egui::ScrollArea::vertical().id_salt("meetings-manage").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                w::section_label(ui, t.meetings_calendars);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    done = w::button(ui, Kind::Ghost, Some(Icon::Check), t.done).clicked();
                });
            });
            for (i, url) in urls.iter().enumerate() {
                let name = self.label_of(url);
                let fail = self.shared.lock().unwrap_or_else(|e| e.into_inner()).get(url).and_then(|l| l.error);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add(egui::Label::new(RichText::new(&name).size(13.5).color(p.text)).truncate());
                        ui.label(RichText::new(ics::short_url(url)).font(FontId::monospace(12.0)).color(p.weak));
                        if let Some(fail) = fail {
                            ui.label(RichText::new(fail.text(t)).size(12.0).color(p.warning));
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let hint = texts::meetings_remove(lang, &name);
                        let r = w::icon_button(ui, Icon::Close, &hint);
                        anchor(ui, r.rect, &format!("meetings-remove-{i}"));
                        if r.clicked() {
                            remove = Some(i);
                        }
                    });
                });
                ui.add_space(4.0);
            }
            if urls.len() < MAX_CALENDARS {
                ui.add_space(4.0);
                w::section_label(ui, t.meetings_add_calendar);
                self.add_row(ui, cx);
            }
        });
        if let Some(i) = remove
            && !sys::demo()
            && i < cx.settings.calendars.len()
        {
            cx.settings.calendars.remove(i);
        }
        if done || cx.settings.calendars.is_empty() {
            self.manage = false;
        }
    }

    /// Ближайшая встреча, «Дальше сегодня», слова ошибок и нижняя строка.
    fn agenda_ui(&mut self, ui: &mut Ui, cx: &mut Ctx, now: i64) {
        let t = cx.t;
        let p = Palette::of(ui);
        let (upcoming, today_end) = self.cache.as_ref().map(|c| (c.upcoming.clone(), c.today_end)).unwrap_or_default();
        let agenda = agenda(&upcoming, now, today_end);
        // Что прочитано, чем кончилось последнее чтение каждого.
        let (known, busy, notes) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let known = self.wanted.iter().any(|u| s.get(u).is_some_and(|l| l.calendar.is_some()));
            let notes: Vec<(String, Option<Fail>, bool)> = self
                .wanted
                .iter()
                .filter_map(|u| s.get(u).map(|l| (u.clone(), l.error, l.partial)))
                .filter(|(_, error, partial)| error.is_some() || *partial)
                .collect();
            (known, s.busy, notes)
        };
        let mut join: Option<Link> = None;
        egui::ScrollArea::vertical()
            .id_salt("meetings")
            .auto_shrink([false, true])
            .max_height((ui.available_height() - BAR_H - 6.0).max(60.0))
            .show(ui, |ui| {
                if !known {
                    if busy {
                        w::note(ui, t.meetings_loading);
                    }
                } else if let Some(m) = &agenda.next {
                    if self.plate(ui, cx, m, now) {
                        join = m.link.clone();
                    }
                } else {
                    w::note(ui, t.meetings_none_today);
                    if let Some(m) = &agenda.tomorrow {
                        let text = texts::meeting_tomorrow(t, &hm(m.start), title_of(m, t));
                        if Self::row(ui, cx, m, &text, None, "meetings-tomorrow") {
                            join = m.link.clone();
                        }
                    }
                }
                if !agenda.later.is_empty() {
                    ui.add_space(6.0);
                    w::section_label(ui, t.meetings_later);
                    ui.add_space(2.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (i, m) in agenda.later.iter().enumerate() {
                        if Self::row(ui, cx, m, title_of(m, t), Some(hm(m.start)), &format!("meetings-row-{i}")) {
                            join = m.link.clone();
                        }
                    }
                }
                // Слова ошибок — под календарём; при нескольких — с его именем.
                for (url, error, partial) in &notes {
                    let name = if notes.len() > 1 || self.wanted.len() > 1 { Some(self.label_of(url)) } else { None };
                    for text in error.map(|e| e.text(t)).into_iter().chain(partial.then_some(t.meetings_big)) {
                        match &name {
                            Some(name) => w::note(ui, format!("{name}: {text}")),
                            None => w::note(ui, text),
                        };
                    }
                }
            });
        if let Some(link) = join {
            open(&link, cx);
        }
        // Нижняя строка: пояснение слева (если помещается), справа «Обновить» и «Календари».
        ui.add_space((ui.available_height() - BAR_H).max(0.0));
        ui.horizontal(|ui| {
            let buttons = 30.0 * 2.0 + ui.spacing().item_spacing.x;
            let room = ui.available_width() - buttons - 8.0;
            if room >= NOTE_ROOM {
                // Пояснение переносится в своей ширине: под значками справа его не должно быть.
                ui.allocate_ui_with_layout(Vec2::new(room, BAR_H), Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(t.meetings_how).size(12.0).color(p.weak)).wrap());
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let more = w::icon_button(ui, Icon::More, t.meetings_calendars);
                anchor(ui, more.rect, "meetings-calendars");
                if more.clicked() {
                    self.manage = true;
                }
                let refresh = w::icon_button(ui, Icon::Refresh, t.meetings_refresh);
                anchor(ui, refresh.rect, "meetings-refresh");
                if refresh.clicked() {
                    self.asked = None;
                }
            });
        });
    }

    /// Ближайшая встреча: плашка `raised` с полоской акцента, название полужирным, служба в рамке, «10:30–10:45 · через
    /// 12 мин» и справа «Подключиться». Ответ — нажали ли «Подключиться».
    fn plate(&self, ui: &mut Ui, cx: &Ctx, m: &Meeting, now: i64) -> bool {
        let (t, lang) = (cx.t, cx.lang);
        let p = Palette::of(ui);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), PLATE_H), Sense::hover());
        paint_plate(ui.painter(), rect, &p);
        anchor(ui, rect, "meetings-plate");
        let mid = rect.center().y;
        let mut clicked = false;
        let mut right = rect.right() - 12.0;
        let left = rect.left() + 14.0;
        let service = m.link.as_ref().map(|l| texts::service_name(lang, l.service));
        if m.link.is_some() {
            // Тесно — «Подключиться» одним значком 34 × 34: название и время важнее слова на кнопке.
            let full = 12.0 * 2.0 + 16.0 + 7.0 + width_of(ui, t.meetings_join, anvil_ui::semibold(14.0));
            let compact = right - left - full - 12.0 < PLATE_ROOM;
            let (width, label) = if compact { (34.0, "") } else { (full, t.meetings_join) };
            let r = Rect::from_min_size(Pos2::new(right - width, mid - 17.0), Vec2::new(width, 34.0));
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(r).layout(Layout::left_to_right(Align::Center)));
            let button = w::button_sized(&mut child, Kind::Primary, Some(Icon::Play), label, r.size());
            // Имя для диктора — всегда слово; в компактном виде оно же и подсказка, со службой.
            button.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, t.meetings_join));
            let button = match (compact, service) {
                (true, Some(service)) => button.on_hover_text(format!("{} · {service}", t.meetings_join)),
                _ => button,
            };
            anchor(ui, button.rect, "meetings-join");
            clicked = button.clicked();
            right = r.left() - 12.0;
        }
        let room = (right - left).max(40.0);
        let chip_font = FontId::proportional(12.0);
        let chip_w = service.map_or(0.0, |c| width_of(ui, c, chip_font.clone()) + 14.0);
        // Служба в рамке — только если названию остаётся место.
        let chip = service.filter(|_| room - chip_w - 8.0 >= TITLE_ROOM);
        let title = line(
            ui,
            title_of(m, t),
            anvil_ui::semibold(15.0),
            p.text,
            room - if chip.is_some() { chip_w + 8.0 } else { 0.0 },
        );
        let state = if m.start > now {
            texts::meeting_in(lang, minutes_up(m.start - now))
        } else {
            texts::meeting_left(lang, minutes_up(m.end - now))
        };
        let sub = line(ui, &format!("{} · {state}", range(m)), FontId::proportional(12.5), p.weak, room);
        let block = title.size().y + 3.0 + sub.size().y;
        let top = mid - block / 2.0;
        let title_h = title.size().y;
        let title_w = title.size().x;
        ui.painter().galley(Pos2::new(left, top), title, p.text);
        ui.painter().galley(Pos2::new(left, top + title_h + 3.0), sub, p.weak);
        if let Some(chip) = chip {
            let box_ = Rect::from_min_size(
                Pos2::new(left + title_w + 8.0, top + title_h / 2.0 - 10.0),
                Vec2::new(chip_w, 20.0),
            );
            ui.painter().rect(box_, 10, Color32::TRANSPARENT, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
            ui.painter().text(box_.center(), egui::Align2::CENTER_CENTER, chip, chip_font, p.weak);
        }
        clicked
    }

    /// Строка «Дальше сегодня»: время моноширинным (`time`; у завтрашней строки его нет — оно в `text`), название, справа
    /// служба. Щелчок по строке со ссылкой — подключиться; подсказка — полное название, время и место. `key` — имя места для
    /// тестов (`anchor`). Ответ — нажали ли.
    fn row(ui: &mut Ui, cx: &Ctx, m: &Meeting, text: &str, time: Option<String>, key: &str) -> bool {
        let (t, lang) = (cx.t, cx.lang);
        let p = Palette::of(ui);
        let joinable = m.link.is_some();
        let sense = if joinable { Sense::click() } else { Sense::hover() };
        let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), sense);
        anchor(ui, rect, key);
        if joinable && r.hovered() {
            ui.painter().rect_filled(rect, 6, p.hover);
        }
        let mid = rect.center().y;
        let mut x = rect.left() + 8.0;
        let mut right = rect.right() - 8.0;
        if let Some(time) = &time {
            let g = ui.painter().layout_no_wrap(time.clone(), FontId::monospace(13.0), p.weak);
            ui.painter().galley(Pos2::new(x, mid - g.size().y / 2.0), g, p.weak);
            x += 44.0 + 12.0;
        }
        if let Some(l) = &m.link {
            let service = texts::service_name(lang, l.service);
            let g = ui.painter().layout_no_wrap(service.to_owned(), FontId::proportional(12.0), p.weak);
            right -= g.size().x;
            ui.painter().galley(Pos2::new(right, mid - g.size().y / 2.0), g, p.weak);
            right -= 12.0;
        }
        let g = line(ui, text, FontId::proportional(13.5), p.text, right - x);
        ui.painter().galley(Pos2::new(x, mid - g.size().y / 2.0), g, p.text);
        w::focus_ring(ui, rect, &r, 6);
        let place = place_of(m);
        let tip = if place.is_empty() {
            format!("{}\n{}", title_of(m, t), range(m))
        } else {
            format!("{}\n{}\n{place}", title_of(m, t), range(m))
        };
        let label = format!("{}, {}", title_of(m, t), range(m));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, joinable, &label));
        let r = if joinable { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r };
        r.on_hover_text(tip).clicked() && joinable
    }
}

impl Module for Meetings {
    fn key(&self) -> &'static str {
        "meetings"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.meetings
    }

    /// Шире соседей (на макете полторы доли): плашке нужны название, служба и кнопка.
    fn weight(&self) -> f32 {
        1.5
    }

    fn min_width(&self) -> f32 {
        260.0
    }

    /// Читают календари и без этого (`background`): раз в 10 минут, пока программа работает.
    fn refresh(&mut self, _monitor: &str) {}

    fn typing(&self) -> bool {
        self.typing_at.is_some_and(|at| at.elapsed() < Duration::from_millis(300))
    }

    /// Каждый кадр, и при спрятанной панели: читать, когда пора, и держать список встреч свежим для плашки и поиска.
    fn background(&mut self, settings: &Settings) {
        self.sync(settings, settings.common.language);
        if self.due(Instant::now()) {
            self.fetch();
        }
        self.refresh_cache(now());
    }

    /// За две минуты до встречи со ссылкой — плашка «Планёрка через 2 мин» с «Подключиться».
    fn events(&mut self, settings: &Settings) -> Vec<Plaque> {
        self.events_at(now(), settings)
    }

    /// «Подключиться к «…»» для ближайших встреч со ссылкой: то же, что кнопка в карточке.
    fn actions(&self, t: &Tr, lang: Lang, _settings: &Settings) -> Vec<Action> {
        let now = now();
        let Some(cache) = &self.cache else { return Vec::new() };
        cache
            .upcoming
            .iter()
            .filter(|m| alive(m, now) && m.link.is_some())
            .take(SEARCH)
            .map(|m| {
                let service = m.link.as_ref().map(|l| texts::service_name(lang, l.service)).unwrap_or_default();
                let words = format!("встреча созвон звонок конференция подключиться присоединиться meeting call conference join {service}");
                Action::new(format!("meetings:join:{}", meeting_key(m)), texts::search_join(lang, title_of(m, t)), t.meetings, Icon::Play)
                    .words(&words)
            })
            .collect()
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        let Some(key) = mine(self.key(), id).and_then(|r| r.strip_prefix("join:")) else { return };
        let now = now();
        let link = self.cache.as_ref().and_then(|c| {
            c.upcoming.iter().find(|m| alive(m, now) && meeting_key(m) == key).and_then(|m| m.link.clone())
        });
        if let Some(link) = link {
            open(&link, cx);
        }
    }

    /// Справа в заголовке: «Календарь «Работа»» или «Календарей: 2».
    fn meta(&self, _t: &Tr) -> String {
        match self.wanted.len() {
            0 => String::new(),
            1 => texts::meetings_calendar(self.lang, &self.label_of(&self.wanted[0])),
            n => texts::meetings_calendars_n(self.lang, n),
        }
    }

    /// Подсказка к пояснению: ссылки календарей сокращённо (сайт и конец, остальное — секрет).
    fn meta_hint(&self) -> String {
        self.wanted.iter().map(|u| ics::short_url(u)).collect::<Vec<_>>().join("\n")
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        self.sync(cx.settings, cx.lang);
        let now = now();
        self.refresh_cache(now);
        // Правый щелчок по карточке — список календарей (убрать, добавить); то же — значок «Календари» внизу.
        let card = ui.max_rect();
        if !self.wanted.is_empty()
            && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary))
            && ui.rect_contains_pointer(card)
        {
            self.manage = !self.manage;
        }
        if self.wanted.is_empty() {
            self.manage = false;
            self.empty_ui(ui, cx);
        } else if self.manage {
            self.manage_ui(ui, cx);
        } else {
            self.agenda_ui(ui, cx, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://calendar.example.com/work/demo.ics";

    fn settings(calendars: &[&str]) -> Settings {
        Settings { calendars: calendars.iter().map(|c| c.to_string()).collect(), ..Settings::default() }
    }

    fn meeting(title: &str, start: i64, minutes: i64, link: Option<&str>) -> Meeting {
        Meeting {
            uid: format!("uid-{title}"),
            title: title.into(),
            place: String::new(),
            start,
            end: start + minutes * 60,
            link: link.and_then(ics::check_link),
        }
    }

    /// Модуль с выдуманным календарём из `demo_ics`: «сейчас» — 10:18, «Планёрка» в 10:30 (в Zoom).
    fn loaded(lang: Lang) -> (Meetings, Settings) {
        let s =
            Settings { common: anvil_ui::CommonSettings { language: lang, ..Default::default() }, ..settings(&[URL]) };
        let mut m = Meetings::default();
        m.sync(&s, lang);
        let result = load(URL, lang, 1);
        m.shared.lock().unwrap().apply(URL, result);
        m.refresh_cache(now());
        (m, s)
    }

    fn launched(url: &str) -> bool {
        let want = format!("launch {url} ");
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).contains(&want)
    }

    #[test]
    fn the_invented_world_is_ten_eighteen_utc() {
        assert!(sys::isolated());
        assert_eq!(now(), demo_now());
        assert_eq!(hm(now()), "10:18");
        assert_eq!(local_offset(now()), 0, "в демо пояс ПК не выдаётся");
        assert_eq!(minutes_up(12 * 60), 12);
        assert_eq!(minutes_up(12 * 60 - 1), 12);
        assert_eq!(minutes_up(1), 1);
        assert_eq!(minutes_up(0), 1, "не «через 0 мин»");
    }

    #[test]
    fn the_invented_calendar_gives_the_board_of_the_card() {
        let (m, _) = loaded(Lang::Ru);
        let cache = m.cache.as_ref().unwrap();
        let titles: Vec<(&str, String)> = cache.upcoming.iter().map(|x| (x.title.as_str(), hm(x.start))).collect();
        // Сегодня три встречи; завтра — «Ретроспектива» (09:30) и серия «Планёрки» (10:30).
        assert_eq!(
            titles,
            [
                ("Планёрка", "10:30".to_string()),
                ("Обзор макетов", "14:00".to_string()),
                ("Созвон по выпуску", "16:30".to_string()),
                ("Ретроспектива", "09:30".to_string()),
                ("Планёрка", "10:30".to_string()),
            ]
        );
        let a = agenda(&cache.upcoming, now(), cache.today_end);
        assert_eq!(
            a.next.as_ref().map(|x| (x.title.as_str(), range(x))),
            Some(("Планёрка", "10:30–10:45".to_string()))
        );
        assert_eq!(
            a.later.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(),
            ["Обзор макетов", "Созвон по выпуску"]
        );
        assert!(a.tomorrow.is_none(), "сегодня ещё есть встречи — завтрашняя не показывается");
        // Службы по ссылкам: Zoom в месте, Meet в описании, Teams в адресе; у «Ретроспективы» ссылки нет.
        let services: Vec<Option<ics::Service>> =
            cache.upcoming.iter().map(|x| x.link.as_ref().map(|l| l.service)).collect();
        assert_eq!(
            services,
            [
                Some(ics::Service::Zoom),
                Some(ics::Service::Meet),
                Some(ics::Service::Teams),
                None,
                Some(ics::Service::Zoom)
            ]
        );
        assert_eq!(
            m.shared.lock().unwrap().get(URL).unwrap().calendar.as_ref().unwrap().name.as_deref(),
            Some("Работа")
        );
        // На английском — без русских слов.
        let (m, _) = loaded(Lang::En);
        let names: Vec<String> = m.cache.as_ref().unwrap().upcoming.iter().map(|x| x.title.clone()).collect();
        assert_eq!(names, ["Daily stand-up", "Mockup review", "Release call", "Retro", "Daily stand-up"]);
        assert!(!demo_ics(Lang::En, 1).chars().any(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap_or(' '))));
        assert!(
            demo_ics(Lang::En, 2).contains("X-WR-CALNAME:Home") && demo_ics(Lang::Ru, 2).contains("X-WR-CALNAME:Дом")
        );
    }

    #[test]
    fn what_is_next_now_and_later_follows_the_clock() {
        let day = ics::days_from_civil(2026, 10, 27) * DAY;
        let today_end = day + DAY;
        let list = vec![
            meeting("A", day + 9 * 3600, 60, None),
            meeting("B", day + 10 * 3600, 90, Some("https://meet.google.com/aaa-bbbb-ccc")),
            meeting("C", day + 12 * 3600, 30, None),
            meeting("D", day + 13 * 3600, 30, None),
            meeting("E", day + 14 * 3600, 30, None),
            meeting("F", day + 15 * 3600, 30, None),
            meeting("G", day + 16 * 3600, 30, None),
            meeting("Tomorrow", today_end + 9 * 3600, 30, None),
        ];
        let at = |h: i64, m: i64| agenda(&list, day + h * 3600 + m * 60, today_end);
        // 09:30 — идёт A; дальше — не больше четырёх.
        let a = at(9, 30);
        assert_eq!(a.next.as_ref().map(|x| x.title.as_str()), Some("A"));
        assert_eq!(
            a.later.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(),
            ["B", "C", "D", "E"],
            "четыре, а F и G — нет"
        );
        // 10:05 — A закончилась, идёт B.
        assert_eq!(at(10, 5).next.as_ref().map(|x| x.title.as_str()), Some("B"));
        // 11:45 — B закончилась в 11:30; следующая C.
        assert_eq!(at(11, 45).next.as_ref().map(|x| x.title.as_str()), Some("C"));
        // 16:15 — идёт G, дальше сегодня ничего.
        let a = at(16, 15);
        assert_eq!(
            (a.next.as_ref().map(|x| x.title.as_str()), a.later.len(), a.tomorrow.is_none()),
            (Some("G"), 0, true)
        );
        // 17:00 — сегодня больше нет; первая завтрашняя отдельной строкой.
        let a = at(17, 0);
        assert!(a.next.is_none() && a.later.is_empty());
        assert_eq!(a.tomorrow.as_ref().map(|x| x.title.as_str()), Some("Tomorrow"));
        // Ничего нет совсем.
        assert_eq!(agenda(&[], day, today_end), Agenda::default());
    }

    #[test]
    fn a_meeting_that_started_stays_until_it_ends() {
        let day = ics::days_from_civil(2026, 10, 27) * DAY;
        let list = vec![meeting("Long", day + 10 * 3600, 60, None)];
        for (secs, shown) in [(10 * 3600 - 1, true), (10 * 3600, true), (10 * 3600 + 3599, true), (11 * 3600, false)] {
            assert_eq!(agenda(&list, day + secs, day + DAY).next.is_some(), shown, "{secs}");
        }
        // Встреча-момент: показана, пока не наступила.
        let moment = vec![meeting("Call", day + 10 * 3600, 0, None)];
        assert!(agenda(&moment, day + 10 * 3600 - 1, day + DAY).next.is_some());
        assert!(agenda(&moment, day + 10 * 3600 + 1, day + DAY).next.is_none());
    }

    #[test]
    fn one_meeting_in_two_calendars_is_one_row() {
        const URL2: &str = "https://calendar.example.com/home/demo.ics";
        let s = settings(&[URL, URL2]);
        let mut m = Meetings::default();
        m.sync(&s, Lang::Ru);
        for url in [URL, URL2] {
            let result = load(url, Lang::Ru, 1);
            m.shared.lock().unwrap().apply(url, result);
        }
        m.refresh_cache(now());
        assert_eq!(m.cache.as_ref().unwrap().upcoming.len(), 5, "те же встречи из двух календарей не двоятся");
    }

    #[test]
    fn an_error_keeps_the_old_meetings_and_says_why_without_the_address() {
        let (m, _) = loaded(Lang::Ru);
        let before = m.cache.as_ref().unwrap().upcoming.len();
        m.shared.lock().unwrap().apply(URL, Err(Fail::Network));
        {
            let s = m.shared.lock().unwrap();
            let l = s.get(URL).unwrap();
            assert!(l.calendar.is_some(), "прежние встречи остаются до удачи");
            assert_eq!(l.error, Some(Fail::Network));
        }
        let mut m = m;
        m.refresh_cache(now());
        assert_eq!(m.cache.as_ref().unwrap().upcoming.len(), before);
        // Слова ошибок — свои, без адреса, на обоих языках.
        for lang in [Lang::Ru, Lang::En] {
            let t = texts::tr(lang);
            for fail in [Fail::Network, Fail::NotCalendar] {
                let text = fail.text(t);
                assert!(!text.contains("http") && !text.contains("example") && !text.is_empty());
            }
        }
        assert_eq!(Fail::Network.text(texts::tr(Lang::Ru)), "Календарь не ответил");
        assert_eq!(Fail::NotCalendar.text(texts::tr(Lang::Ru)), "Ссылка не на календарь");
        assert_eq!(Fail::Network.text(texts::tr(Lang::En)), "The calendar did not answer");
        assert_eq!(Fail::NotCalendar.text(texts::tr(Lang::En)), "The link is not a calendar");
        // Удача снимает ошибку.
        let ok = load(URL, Lang::Ru, 1);
        m.shared.lock().unwrap().apply(URL, ok);
        assert_eq!(m.shared.lock().unwrap().get(URL).unwrap().error, None);
    }

    #[test]
    fn the_calendar_link_is_never_in_the_title_of_the_card_or_its_hint() {
        let secret = "https://calendar.example.com/ical/me%40example.com/private-SECRET0123456789/basic.ics";
        let s = settings(&[secret]);
        let mut m = Meetings::default();
        m.sync(&s, Lang::Ru);
        let t = texts::tr(Lang::Ru);
        for text in [m.meta(t), m.meta_hint()] {
            assert!(!text.contains("SECRET") && !text.contains("private") && !text.contains("me%40"), "{text}");
        }
        assert_eq!(m.meta_hint(), "calendar.example.com/…/basic.ics");
        assert_eq!(m.meta(t), "Календарь «calendar.example.com»", "пока имя не прочитано — сайт");
        let result = load(secret, Lang::Ru, 1);
        m.shared.lock().unwrap().apply(secret, result);
        assert_eq!(m.meta(t), "Календарь «Работа»");
        let s2 = settings(&[secret, URL]);
        m.sync(&s2, Lang::Ru);
        assert_eq!(m.meta(t), "Календарей: 2");
        assert_eq!(texts::meetings_calendar(Lang::En, "Work"), "“Work” calendar");
        m.sync(&Settings::default(), Lang::Ru);
        assert_eq!(m.meta(t), "");
    }

    #[test]
    fn the_plaque_comes_two_minutes_before_a_meeting_with_a_link_and_only_once() {
        let (mut m, s) = loaded(Lang::Ru);
        let start = now() + 12 * 60; // «Планёрка» в 10:30
        // До окна в две минуты плашки нет.
        assert!(m.events_at(start - 121, &s).is_empty());
        let told = m.events_at(start - 120, &s);
        assert_eq!(told.len(), 1);
        let p = &told[0];
        assert_eq!(
            (p.kind, p.glyph, p.sticky, p.shelf),
            (plaque::Kind::Meeting, Glyph::Calendar, true, Some("meetings"))
        );
        assert_eq!(p.title, Say::MeetingIn { title: "Планёрка".into(), start });
        assert_eq!(p.sub, Say::MeetingAt { time: "10:30".into(), service: ics::Service::Zoom });
        assert_eq!(p.buttons.len(), 1);
        let b = &p.buttons[0];
        assert_eq!((b.label.clone(), b.look, b.external), (Say::Join, Look::Primary, true));
        assert_eq!(b.does, Do::Open("https://us02web.zoom.us/j/123456789".into()));
        assert!(p.expires.is_some(), "уходит сама, когда встреча начнётся");
        // Слова: «Планёрка через 2 мин» и «10:30 · Zoom», на обоих языках.
        assert_eq!(texts::plaque_text_at(Lang::Ru, &p.title, start - 120), "Планёрка через 2 мин");
        assert_eq!(
            texts::plaque_text_at(Lang::En, &Say::MeetingIn { title: "Daily stand-up".into(), start }, start - 119),
            "Daily stand-up in 2 min"
        );
        assert_eq!(
            texts::plaque_text_at(Lang::Ru, &p.title, start - 30),
            "Планёрка через 1 мин",
            "минуты считаются при показе"
        );
        assert_eq!(texts::plaque_text_at(Lang::Ru, &p.sub, start), "10:30 · Zoom");
        assert_eq!(texts::plaque_text_at(Lang::Ru, &Say::Join, 0), "Подключиться");
        assert_eq!(texts::plaque_text_at(Lang::En, &Say::Join, 0), "Join");
        // Повторные кадры второй плашки не плодят; после начала — тоже.
        assert!(m.events_at(start - 60, &s).is_empty());
        assert!(m.events_at(start - 1, &s).is_empty());
        assert!(m.events_at(start, &s).is_empty(), "встреча началась — плашки нет");
    }

    #[test]
    fn no_plaque_without_a_link_or_with_the_kind_switched_off_and_a_late_switch_on_still_works() {
        let (mut m, s) = loaded(Lang::Ru);
        // «Ретроспектива» без ссылки: за две минуты до неё плашки нет.
        let retro = m.cache.as_ref().unwrap().upcoming.iter().find(|x| x.title == "Ретроспектива").unwrap().start;
        assert!(m.events_at(retro - 100, &s).is_empty());
        // Выключено — молчим, а не «потратили» плашку: включили в те же две минуты — она есть.
        let off = Settings { plaques: crate::config::Plaques { meetings: false, ..Default::default() }, ..s.clone() };
        let start = now() + 12 * 60;
        assert!(m.events_at(start - 100, &off).is_empty());
        assert_eq!(m.events_at(start - 90, &s).len(), 1);
        // Календарей нет — нечего объявлять.
        let mut empty = Meetings::default();
        assert!(empty.events_at(start - 100, &Settings::default()).is_empty());
    }

    #[test]
    fn a_plaque_of_a_meeting_waits_for_its_turn_but_not_past_the_start() {
        use crate::plaque::{Gate, Queue};
        let (mut m, s) = loaded(Lang::Ru);
        let start = now() + 12 * 60;
        let p = m.events_at(start - 120, &s).remove(0);
        let t0 = Instant::now();
        let mut q = Queue::default();
        // Показ экрана: плашка встречи ждёт (таймеру можно, ей нет), а время её не идёт, но срок не стоит.
        let p = Plaque { expires: Some(t0 + Duration::from_secs(120)), ..p };
        q.push(p.clone(), t0);
        q.step(t0, false, Gate::TimerOnly, true);
        assert!(q.current().is_none() && q.waiting() == 1, "ждёт, пока уведомления нежелательны");
        q.step(t0 + Duration::from_secs(60), false, Gate::Open, true);
        assert_eq!(q.current().map(|c| c.key.clone()), Some(p.key.clone()));
        // Висит, пока не закроют: и 10, и 100 секунд спустя.
        q.step(t0 + Duration::from_secs(100), false, Gate::Open, true);
        assert!(q.current().is_some(), "важная плашка не уходит по четырём секундам");
        // Встреча началась — плашка ушла сама, даже под мышью.
        q.step(t0 + Duration::from_secs(121), true, Gate::Open, true);
        assert!(q.current().is_none());
        // А ждущая, что не дождалась очереди, пропадает без показа.
        let mut q = Queue::default();
        q.push(p, t0);
        q.step(t0, false, Gate::TimerOnly, true);
        q.step(t0 + Duration::from_secs(130), false, Gate::Open, true);
        assert!(q.is_empty(), "после начала встречи показывать нечего");
    }

    #[test]
    fn search_offers_to_join_the_nearest_meetings_with_a_link_and_the_command_opens_the_link() {
        let (mut m, s) = loaded(Lang::Ru);
        let t = texts::tr(Lang::Ru);
        let actions = m.actions(t, Lang::Ru, &s);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Подключиться к «Планёрка»", "Подключиться к «Обзор макетов»", "Подключиться к «Созвон по выпуску»"]
        );
        assert!(actions.iter().all(|a| a.enabled && a.place == t.meetings && a.id.starts_with("meetings:join:")));
        assert!(actions[0].words.contains("zoom") || actions[0].words.contains("Zoom"));
        let en = m.actions(texts::tr(Lang::En), Lang::En, &s);
        assert_eq!(en[0].title, "Join “Планёрка”", "название встречи — как в календаре");
        // Команда — то же, что кнопка: открывает проверенную ссылку.
        let mut toasts = Vec::new();
        let mut settings = s.clone();
        let mut cx = Ctx { t, lang: Lang::Ru, monitor: "", toasts: &mut toasts, settings: &mut settings };
        m.command(&Command::RunAction(actions[1].id.clone()), &mut cx);
        assert!(launched("https://meet.google.com/abc-defg-hij"));
        // Чужое действие и неизвестный ключ — ничего.
        m.command(&Command::RunAction("quick:wifi".into()), &mut cx);
        m.command(&Command::RunAction("meetings:join:nope".into()), &mut cx);
        assert!(toasts.is_empty());
        // Календарей нет — действий нет.
        assert!(Meetings::default().actions(t, Lang::Ru, &Settings::default()).is_empty());
    }

    #[test]
    fn a_link_that_does_not_pass_the_check_is_never_opened() {
        let (mut m, s) = loaded(Lang::Ru);
        // Подмена в памяти: ссылка не на службу — не открывается.
        let evil = "https://evil.example/j/1";
        m.cache.as_mut().unwrap().upcoming[0].link = Some(Link { url: evil.into(), service: ics::Service::Zoom });
        let key = meeting_key(&m.cache.as_ref().unwrap().upcoming[0]);
        let mut toasts = Vec::new();
        let mut settings = s;
        let mut cx =
            Ctx { t: texts::tr(Lang::Ru), lang: Lang::Ru, monitor: "", toasts: &mut toasts, settings: &mut settings };
        m.command(&Command::RunAction(format!("meetings:join:{key}")), &mut cx);
        assert!(!launched(evil));
    }

    #[test]
    fn a_location_that_is_a_link_is_not_shown_in_the_tooltip() {
        let mut x = meeting("A", 0, 10, None);
        x.place = "https://us02web.zoom.us/j/1?pwd=SECRET".into();
        assert_eq!(place_of(&x), "", "ссылка с паролем в подсказку не идёт — о ней говорит служба");
        x.place = "Переговорная 2".into();
        assert_eq!(place_of(&x), "Переговорная 2");
    }

    #[test]
    fn the_card_reads_the_calendars_every_ten_minutes_and_at_once_for_a_new_one() {
        let mut m = Meetings::default();
        let t0 = Instant::now();
        assert!(!m.due(t0), "календарей нет — читать нечего");
        m.sync(&settings(&[URL]), Lang::Ru);
        assert!(m.due(t0), "ещё не читали");
        m.asked = Some(t0);
        m.asked_for = m.wanted.clone();
        assert!(!m.due(t0 + Duration::from_secs(599)));
        assert!(m.due(t0 + Duration::from_secs(600)));
        // Добавили календарь — читать сразу, не дожидаясь десяти минут.
        m.sync(&settings(&[URL, "https://calendar.example.com/other.ics"]), Lang::Ru);
        assert!(m.due(t0 + Duration::from_secs(1)));
        // Фоновое чтение в выдуманном мире — без сети, выдуманным календарём.
        let mut m = Meetings::default();
        let s = settings(&[URL]);
        m.background(&s);
        assert!(m.asked.is_some());
        let start = Instant::now();
        while m.shared.lock().unwrap().version == 0 {
            assert!(start.elapsed() < Duration::from_secs(3), "поток чтения не ответил");
            std::thread::sleep(Duration::from_millis(5));
        }
        while m.shared.lock().unwrap().busy {
            std::thread::sleep(Duration::from_millis(1));
        }
        m.background(&s);
        assert_eq!(m.cache.as_ref().unwrap().upcoming.len(), 5);
        assert!(!m.due(Instant::now()), "только что прочитали");
        // Убрали календарь — его встречи из памяти уходят со следующим чтением.
        m.background(&Settings::default());
        assert!(m.cache.as_ref().unwrap().upcoming.is_empty());
    }

    #[test]
    fn a_calendar_is_added_removed_and_checked_from_the_field() {
        use crate::config::MAX_CALENDARS;
        let mut m = Meetings::default();
        // Негодная ссылка и пустая строка — календаря не будет.
        for bad in [
            "",
            "calendar.example.com/x.ics",
            "http://calendar.example.com/x.ics",
            "https://user:pw@calendar.example.com/x",
        ] {
            assert_eq!(ics::calendar_url(bad), None, "{bad:?}");
        }
        m.adding = "webcal://calendar.example.com/work/demo.ics".into();
        assert_eq!(ics::calendar_url(&m.adding).as_deref(), Some("https://calendar.example.com/work/demo.ics"));
        // Предел — пять.
        let many: Vec<String> = (0..MAX_CALENDARS).map(|i| format!("https://calendar.example.com/{i}.ics")).collect();
        let s = Settings { calendars: many, ..Settings::default() };
        assert_eq!(s.calendars.len(), MAX_CALENDARS);
        let back = crate::config::parse(&crate::config::format(&s));
        assert_eq!(back.calendars, s.calendars, "ссылки пишутся в config.txt и читаются обратно");
    }
}
