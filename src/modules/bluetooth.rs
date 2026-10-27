// Bluetooth: сопряжённые устройства — подключено ли, заряд. Перечисление идёт через SetupAPI и может
// занять заметное время — в своём потоке, окно читает готовое.
// Наушники одним щелчком (2.6): звуковое устройство (наушники, гарнитура, колонка) подключается и отключается щелчком по
// строке, Enter или «Подключить» (`btaudio`: просьба драйверу — без прав администратора, по замыслу). Идёт в своём потоке:
// строка — «Подключение…» с вертушкой, поток спрашивает устройства раз в секунду, до 15 с (отключение — до 10 с); подключилось
// — «Подключено» и заряд, нет — строка снова прежняя и уведомление своими словами (`problems`). Мышь, клавиатура и геймпад без
// кнопки: Windows подключает их сама, когда их включают. Радио Bluetooth выключено — «Подключить» серая с причиной. Своё
// подключение плашки «подключено» не даёт (`Shared::apply`): строка и так показывает.
// Плашки у края (2.4): аудиоустройство подключилось («Наушники: подключено», заряд) и любое устройство разряжается (15 % и
// ниже, раз за разряд: «Мышь: разряжается»). О мыши и клавиатуре «подключено» не говорят — они засыпают и просыпаются сами, и
// каждое пробуждение было бы плашкой. Устройство считается отключённым, когда оно не подключено в двух чтениях подряд: после
// сна Windows отвечает неполным списком, и одно такое чтение не должно давать ложных «подключено».
// Для этого, пока плашки Bluetooth включены, устройства читаются и при спрятанной панели — раз в несколько секунд; первое
// чтение (после запуска и после включения плашек) ничего не объявляет: плашка только о новом.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

use super::{Action, Command, Ctx, Module, anchor, mine};
use crate::config::Settings;
use crate::plaque::{self, Glyph, Plaque, Say};
use crate::sys::{self, bt, btaudio, net};
use crate::texts::{self, Tr};

/// Как часто читать устройства ради плашек, пока панель спрятана.
const WATCH_EVERY: Duration = Duration::from_secs(5);
/// Заряд, с которого устройство «разряжается» (включительно), и заряд, выше которого оно снова «в порядке»: пока не поднялось
/// выше второго, о разряде не повторяют.
pub const LOW: u8 = 15;
pub const RECOVERED: u8 = 20;
/// Сколько чтений подряд подключённое устройство должно быть не подключено, чтобы считаться отключённым.
const OFF_READS: u8 = 2;
/// Сколько ждать, пока устройство подключится после просьбы драйверу (design §2), и сколько — пока отключится; опрос — раз в
/// секунду, а в тестах быстрее ([`waits`]).
const CONNECT_WAIT: Duration = Duration::from_secs(15);
const DISCONNECT_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_secs(1);
/// Строка устройства: не ниже 38 (макет). Зазор между строками 4, а не 8 макета: выдуманные четыре устройства иначе не влезают
/// в карточку 340 высотой.
const ROW_H: f32 = 38.0;
const ROW_GAP: f32 = 4.0;
/// Слова для поиска у «Подключить» и «Отключить»: оба языка сразу.
const WORDS: &str = "наушники колонка гарнитура bluetooth headphones speaker headset";

/// Что делают с устройством сейчас.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Doing {
    Connecting,
    Disconnecting,
}

/// Что не вышло в потоке: панель скажет об этом своими словами (`Module::problems`), а английский текст ошибки остаётся в
/// журнале. Имя устройства — как в Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Failure {
    Connect(String),
    Disconnect(String),
}

/// Сколько ждать и как часто спрашивать, пока устройство станет таким, как просили. В тестах — доли секунды: «не вышло» не ждёт 15
/// секунд, а подключившееся — не ждёт вовсе (первый опрос идёт сразу после просьбы).
fn waits(doing: Doing) -> (Duration, Duration) {
    if cfg!(test) {
        return (Duration::from_millis(400), Duration::from_millis(20));
    }
    (if doing == Doing::Connecting { CONNECT_WAIT } else { DISCONNECT_WAIT }, POLL)
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Что изменилось у устройства между двумя чтениями.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Аудиоустройство стало подключённым (раньше не было в списке или не было подключено).
    Connected(bt::Device),
    /// Заряд упал до [`LOW`] и ниже — первый раз с тех пор, как был выше [`RECOVERED`].
    Low(bt::Device),
}

/// Что изменилось между чтениями. `prev` — прошлое чтение; `None` — первое: оно ничего не объявляет, только запоминает, что
/// уже севшее объявлено (`warned`). `warned` — адреса устройств, о чьём разряде уже сказали.
pub fn changes(prev: Option<&[bt::Device]>, now: &[bt::Device], warned: &mut HashSet<u64>) -> Vec<Change> {
    let mut out = Vec::new();
    for d in now {
        let low = d.battery.is_some_and(|b| b <= LOW);
        match prev {
            None => {
                if low {
                    warned.insert(d.address);
                }
            }
            Some(prev) => {
                let was = prev.iter().find(|p| p.address == d.address);
                // Только аудио: мышь и клавиатура засыпают и просыпаются сами — каждое пробуждение было бы плашкой.
                if d.connected && d.audio && was.is_none_or(|p| !p.connected) {
                    out.push(Change::Connected(d.clone()));
                }
                if low && warned.insert(d.address) {
                    out.push(Change::Low(d.clone()));
                }
            }
        }
        if d.battery.is_some_and(|b| b > RECOVERED) {
            warned.remove(&d.address);
        }
    }
    out
}

/// Какими считать устройства после нового чтения `now`: подключённое, что в одном чтении пропало или оказалось неподключённым,
/// ещё подключено (после сна Windows отвечает неполным списком); в двух подряд — отключено. `off` — сколько чтений подряд
/// устройства (по адресу) не подключены. Первое чтение (`prev` — `None`) берётся как есть.
pub fn settle(prev: Option<&[bt::Device]>, now: Vec<bt::Device>, off: &mut HashMap<u64, u8>) -> Vec<bt::Device> {
    let Some(prev) = prev else {
        off.clear();
        return now;
    };
    let was_connected = |address: u64| prev.iter().find(|p| p.address == address).filter(|p| p.connected);
    let listed: HashSet<u64> = now.iter().map(|d| d.address).collect();
    let mut out = Vec::with_capacity(now.len());
    for d in now {
        match was_connected(d.address) {
            Some(before) if !d.connected => {
                let reads = off.entry(d.address).or_insert(0);
                *reads += 1;
                if *reads >= OFF_READS {
                    off.remove(&d.address);
                    out.push(d);
                } else {
                    out.push(before.clone());
                }
            }
            _ => {
                off.remove(&d.address);
                out.push(d);
            }
        }
    }
    // Подключённое, что вовсе пропало из списка: так же — пока не в двух чтениях подряд.
    for gone in prev.iter().filter(|p| p.connected && !listed.contains(&p.address)) {
        let reads = off.entry(gone.address).or_insert(0);
        *reads += 1;
        if *reads >= OFF_READS {
            off.remove(&gone.address);
        } else {
            out.push(gone.clone());
        }
    }
    out
}

/// Плашка «подключено»: значок — наушники у аудиоустройств, иначе значок Bluetooth; под названием — заряд, если Windows его знает.
pub fn connected_plaque(d: &bt::Device) -> Plaque {
    let glyph = if d.audio { Glyph::Headphones } else { Glyph::Device };
    Plaque::new(
        plaque::Kind::Bluetooth,
        format!("bt:{}", d.address),
        glyph,
        Tone::Accent,
        Say::Connected(d.name.clone()),
    )
    .sub(Say::BtCharge(d.battery))
    .shelf("bluetooth")
}

/// Плашка «разряжается»: оговорка (`Warning`), заряд под названием.
pub fn low_plaque(d: &bt::Device) -> Plaque {
    let percent = d.battery.unwrap_or_default();
    Plaque::new(
        plaque::Kind::Bluetooth,
        format!("bt-low:{}", d.address),
        Glyph::Battery,
        Tone::Warning,
        Say::Low(d.name.clone()),
    )
    .sub(Say::Battery(percent))
    .shelf("bluetooth")
}

#[derive(Default)]
struct Shared {
    list: Vec<bt::Device>,
    loaded: bool,
    busy: bool,
    /// Какими считали устройства после прошлого чтения (`settle`), с ними сверяют новое; `None` — следующее чтение первое и
    /// ничего не объявит.
    prev: Option<Vec<bt::Device>>,
    /// Сколько чтений подряд подключённое устройство не подключено (по адресу).
    off: HashMap<u64, u8>,
    /// У кого о разряде уже сказали.
    warned: HashSet<u64>,
    /// Изменения, что ещё не стали плашками.
    pending: Vec<Change>,
    /// Что подключают или отключают сейчас, по адресу устройства: строка показывает «Подключение…», второй щелчок не нужен.
    ops: HashMap<u64, Doing>,
    /// Радио Bluetooth: `Some(false)` — выключено; `None` — не знаем (радио нет или Windows не ответила): тогда не мешаем.
    radio: Option<bool>,
    /// Что не вышло с прошлого раза: панель скажет уведомлением, даже если карточку не рисуют.
    failed: Vec<Failure>,
    /// Номер последнего начатого чтения и последнего применённого: чтение старее применённого отбрасывается — иначе запоздавший
    /// ответ вернул бы «Не подключено» сразу после подключения.
    started: u64,
    applied: u64,
}

impl Shared {
    /// Новое чтение: сверить с прошлым, а список как есть — в карточку.
    fn apply(&mut self, list: Vec<bt::Device>) {
        let believed = settle(self.prev.as_deref(), list.clone(), &mut self.off);
        let mut found = changes(self.prev.as_deref(), &believed, &mut self.warned);
        // Своё подключение (щелчок в карточке, действие поиска) плашки «подключено» не даёт: строка и так показывает.
        found.retain(|c| !matches!(c, Change::Connected(d) if self.ops.get(&d.address) == Some(&Doing::Connecting)));
        self.pending.extend(found);
        self.prev = Some(believed);
        self.list = list;
        self.loaded = true;
    }

    /// Номер нового чтения: его берут до того, как читать устройства.
    fn next_read(&mut self) -> u64 {
        self.started += 1;
        self.started
    }

    /// Применить чтение под номером `read`, если оно не старее уже применённого. Ответ — применили ли.
    fn take(&mut self, read: u64, list: Vec<bt::Device>) -> bool {
        if read < self.applied {
            return false;
        }
        self.applied = read;
        self.apply(list);
        true
    }
}

pub struct Bluetooth {
    shared: Arc<Mutex<Shared>>,
    /// Когда начали последнее чтение.
    read_at: Option<Instant>,
    /// Плашки Bluetooth включены, и мы следим: включили заново — прежнему чтению сверять уже не с чем.
    watching: bool,
}

impl Default for Bluetooth {
    fn default() -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        // Для снимков (только в демо-режиме): `ANTRESOL_VIEW=bluetooth-connecting` — выдуманная колонка «подключается» и до конца
        // не доходит (потока нет), `bluetooth-off` — радио выключено, подключённых нет.
        if sys::demo() {
            match std::env::var("ANTRESOL_VIEW").as_deref() {
                Ok("bluetooth-connecting") => {
                    lock(&shared).ops.insert(bt::FAKE_SPEAKER, Doing::Connecting);
                }
                Ok("bluetooth-off") => {
                    net::fake_bluetooth(false);
                    bt::fake_disconnect_all();
                }
                _ => {}
            }
        }
        Bluetooth { shared, read_at: None, watching: false }
    }
}

impl Bluetooth {
    /// Прочитать устройства в потоке (не двигая окно): список для карточки и сверка для плашек; `radio` — ещё и радио Bluetooth
    /// (нужно открытой карточке, а плашкам при спрятанной панели — нет).
    fn read(&mut self, radio: bool) {
        let read = {
            let mut s = lock(&self.shared);
            if s.busy {
                return;
            }
            s.busy = true;
            s.next_read()
        };
        self.read_at = Some(Instant::now());
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let _com = btaudio::Com::enter();
            let list = bt::devices();
            let radio = radio.then(net::bluetooth_on);
            let mut s = lock(&shared);
            s.take(read, list);
            if let Some(radio) = radio {
                s.radio = radio;
            }
            s.busy = false;
        });
    }

    /// Подключить или отключить звуковое устройство: щелчок по строке, Enter, «Подключить», действие поиска. `want` — что ждали
    /// от этого устройства: действие поиска «Подключить» не отключит устройство, что успело подключиться само, и наоборот.
    /// Ничего не делает, если устройство не звуковое, с ним уже что-то делают или радио выключено (подключать нечем). Работа
    /// идёт в потоке, итог — в строке и в `problems`. Ответ — начали ли.
    fn toggle(&mut self, address: u64, want: Option<Doing>) -> bool {
        let (doing, name) = {
            let mut s = lock(&self.shared);
            let Some(d) = s.list.iter().find(|d| d.address == address) else { return false };
            let (audio, name) = (d.audio, d.name.clone());
            let doing = if d.connected { Doing::Disconnecting } else { Doing::Connecting };
            let no_radio = doing == Doing::Connecting && s.radio == Some(false);
            if !audio || no_radio || s.ops.contains_key(&address) || want.is_some_and(|wanted| wanted != doing) {
                return false;
            }
            s.ops.insert(address, doing);
            (doing, name)
        };
        let shared = self.shared.clone();
        std::thread::spawn(move || work(&shared, address, &name, doing));
        true
    }
}

/// Работа потока: попросить драйвер и ждать, пока устройство станет таким, как просили, — опрос раз в секунду, не дольше
/// [`CONNECT_WAIT`] или [`DISCONNECT_WAIT`]. Итог — в общем состоянии: строка снова обычная, а не вышло — уведомление
/// (`Module::problems`), английский текст ошибки — в журнал. Список и «ничего не делаем» меняются разом, под одним замком: строка
/// не мигает прежним состоянием, а плашка «подключено» своё подключение не видит (`Shared::apply`).
fn work(shared: &Mutex<Shared>, address: u64, name: &str, doing: Doing) {
    let _com = btaudio::Com::enter();
    let on = doing == Doing::Connecting;
    let (limit, poll) = waits(doing);
    let started = Instant::now();
    let mut last: Option<(u64, Vec<bt::Device>)> = None;
    let done = match btaudio::set_connected(address, name, on) {
        Ok(_) => loop {
            let read = lock(shared).next_read();
            let list = bt::devices();
            let reached = list.iter().any(|d| d.address == address && d.connected) == on;
            last = Some((read, list));
            if reached {
                break true;
            }
            if started.elapsed() >= limit {
                break false;
            }
            std::thread::sleep(poll);
        },
        Err(e) => {
            eprintln!("bluetooth: {} failed: {e}", if on { "connect" } else { "disconnect" });
            false
        }
    };
    let mut s = lock(shared);
    if let Some((read, list)) = last {
        s.take(read, list);
    }
    s.ops.remove(&address);
    if !done {
        s.failed.push(if on { Failure::Connect(name.to_string()) } else { Failure::Disconnect(name.to_string()) });
    }
}

/// «Параметры Bluetooth» в Windows (в демо — не открываются).
fn open_settings() {
    if !sys::demo()
        && let Err(e) = sys::launch("ms-settings:bluetooth", "")
    {
        eprintln!("open bluetooth settings failed: {e}");
    }
}

/// Текст одной строкой, с многоточием, если не помещается в `room`.
fn fit(ui: &Ui, text: &str, font: FontId, color: Color32, room: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room.max(20.0));
    ui.painter().layout_job(job)
}

/// Бейдж заряда — как `w::badge` набора, но нарисованный прямо в строке: у строки одна область нажатия, и виджет поверх неё
/// мешал бы наведению. `right` — правый край, ответ — левый.
fn paint_badge(ui: &Ui, p: &Palette, right: f32, mid: f32, text: &str, tone: Tone) -> f32 {
    let color = tone.color(p);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), anvil_ui::semibold(12.0), color);
    let size = Vec2::new(galley.size().x + 14.0, 20.0);
    let rect = Rect::from_min_size(Pos2::new(right - size.x, mid - size.y / 2.0), size);
    ui.painter().rect_filled(rect, 10, p.badge_fill(color));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    rect.left()
}

/// Строка устройства по макету: слева точка (подключено), кольцо (нет) или вертушка (подключают, отключают), имя и под ним
/// состояние; справа — заряд подключённого или «Подключить» у звукового, что не подключено. Звуковое нажимается (щелчок или
/// Enter — подключить или отключить), остальные — нет: Windows подключает их сама, когда их включают. `off` — радио Bluetooth
/// выключено: «Подключить» серая, щелчок ничего не делает. Ответ — нажали ли.
fn row(ui: &mut Ui, t: &Tr, lang: anvil_ui::Lang, d: &bt::Device, doing: Option<Doing>, off: bool) -> bool {
    let p = Palette::of(ui);
    // Нажимается: звуковое, с ним ничего не делают сейчас, а подключать — только при включённом радио.
    let live = d.audio && doing.is_none() && (d.connected || !off);
    let sense = if live { Sense::click() } else { Sense::hover() };
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), sense);
    anchor(ui, rect, &format!("bt-{:x}", d.address));
    let mid = rect.center().y;
    let hovered = live && r.hovered();
    let pressed = live && r.is_pointer_button_down_on();
    if hovered {
        ui.painter().rect_filled(rect, 6, p.hover);
    }
    // Точка, кольцо или вертушка — в слоте 8 у левого края (поля строки 6).
    let slot = Pos2::new(rect.left() + 6.0 + 4.0, mid);
    match doing {
        Some(_) => {
            let mut spinner = ui.new_child(UiBuilder::new().max_rect(Rect::from_center_size(slot, Vec2::splat(12.0))));
            w::spinner(&mut spinner, 12.0);
        }
        None if d.connected => {
            ui.painter().circle_filled(slot, 4.0, Tone::Success.color(&p));
        }
        None => {
            ui.painter().circle_stroke(slot, 3.25, Stroke::new(1.5, p.weak));
        }
    }
    // Справа: «Подключить» (рисунок кнопки `Secondary` 28 высотой, текст 12,5) или заряд.
    let mut right = rect.right() - 6.0;
    if d.audio && !d.connected && doing.is_none() {
        let galley = ui.painter().layout_no_wrap(t.connect.to_owned(), anvil_ui::semibold(12.5), Color32::PLACEHOLDER);
        let size = Vec2::new(galley.size().x + 20.0, 28.0);
        let button = Rect::from_min_size(Pos2::new(right - size.x, mid - size.y / 2.0), size);
        let (fill, border, ink) = if !live {
            (p.raised, p.border, p.faint)
        } else if pressed {
            (p.border, p.border_strong, p.text)
        } else if hovered {
            (p.hover, p.border_strong, p.text)
        } else {
            (p.raised, p.border, p.text)
        };
        let radius = anvil_ui::theme::radius::CONTROL;
        ui.painter().rect(button, radius, fill, Stroke::new(1.0, border), egui::StrokeKind::Inside);
        ui.painter().galley(button.center() - galley.size() / 2.0, galley, ink);
        right = button.left() - 10.0;
    } else if let Some(percent) = d.battery.filter(|_| d.connected && doing.is_none()) {
        let tone = if percent <= 20 { Tone::Warning } else { Tone::Neutral };
        right = paint_badge(ui, &p, right, mid, &texts::scene_percent(lang, percent), tone) - 10.0;
    }
    // Имя и состояние (оба тихие, пока не подключено).
    let left = rect.left() + 24.0;
    let ink = if d.connected || doing.is_some() { p.text } else { p.weak };
    let state = match doing {
        Some(Doing::Connecting) => t.bt_connecting,
        Some(Doing::Disconnecting) => t.bt_disconnecting,
        None if d.connected => t.bt_connected,
        None => t.bt_not_connected,
    };
    let name = fit(ui, &d.name, FontId::proportional(13.5), ink, right - left);
    let sub = fit(ui, state, FontId::proportional(12.0), p.weak, right - left);
    let elided = name.elided;
    let top = mid - (name.size().y + 1.0 + sub.size().y) / 2.0;
    let below = top + name.size().y + 1.0;
    ui.painter().galley(Pos2::new(left, top), name, ink);
    ui.painter().galley(Pos2::new(left, below), sub, p.weak);
    // Подсказка: что сделает щелчок; у не звуковых — почему кнопки нет; обрезанное имя — целиком.
    let hint = match (d.audio, d.connected, doing) {
        (true, _, Some(_)) | (false, true, _) => None,
        (true, true, None) => Some(t.bt_click_disconnect),
        (true, false, None) if off => Some(t.bt_off),
        (true, false, None) => Some(t.bt_click_connect),
        (false, false, _) => Some(t.bt_by_itself),
    };
    w::focus_ring(ui, rect, &r, 6);
    let label = match hint {
        Some(hint) => format!("{}, {state}. {hint}", d.name),
        None => format!("{}, {state}", d.name),
    };
    let kind = if d.audio { egui::WidgetType::Button } else { egui::WidgetType::Label };
    r.widget_info(|| egui::WidgetInfo::labeled(kind, live, &label));
    let clicked = live && r.clicked();
    let r = if live { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r };
    match (elided, hint) {
        (true, Some(hint)) => {
            r.on_hover_text(format!("{}\n{hint}", d.name));
        }
        (true, None) => {
            r.on_hover_text(d.name.clone());
        }
        (false, Some(hint)) => {
            r.on_hover_text(hint);
        }
        (false, None) => {}
    }
    clicked
}

impl Module for Bluetooth {
    fn key(&self) -> &'static str {
        "bluetooth"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.bluetooth
    }

    fn refresh(&mut self, _monitor: &str) {
        self.read(true);
    }

    /// Слежка для плашек: пока они включены, устройства читаются и при спрятанной панели.
    fn background(&mut self, settings: &Settings) {
        let on = settings.plaques.on(plaque::Kind::Bluetooth);
        if on != self.watching {
            self.watching = on;
            let mut shared = lock(&self.shared);
            shared.prev = None;
            shared.off.clear();
        }
        if on && self.read_at.is_none_or(|at| at.elapsed() >= WATCH_EVERY) {
            self.read(false);
        }
    }

    fn events(&mut self, settings: &Settings) -> Vec<Plaque> {
        let pending = std::mem::take(&mut lock(&self.shared).pending);
        if !settings.plaques.on(plaque::Kind::Bluetooth) {
            return Vec::new();
        }
        pending
            .iter()
            .map(|change| match change {
                Change::Connected(d) => connected_plaque(d),
                Change::Low(d) => low_plaque(d),
            })
            .collect()
    }

    /// «Параметры Bluetooth» — та же кнопка, что внизу карточки; «Подключить «Наушники»» и «Отключить «Наушники»» — у каждого
    /// звукового сопряжённого устройства, то же, что щелчок по строке (мыши и клавиатуре Windows подключает сама — действий у
    /// них нет). Радио выключено — «Подключить» серая с причиной; с устройством что-то делают — серое («Подключение…»).
    fn actions(&self, t: &Tr, _lang: anvil_ui::Lang, _settings: &Settings) -> Vec<Action> {
        let (list, ops, radio) = {
            let s = lock(&self.shared);
            (s.list.clone(), s.ops.clone(), s.radio)
        };
        let settings = Action::new("bluetooth:settings", t.bt_settings, t.bluetooth, Icon::Gear);
        let mut out = vec![settings.words("блютуз устройства наушники сопряжение bluetooth devices pair headphones")];
        for d in list.iter().filter(|d| d.audio) {
            let (verb, template) =
                if d.connected { ("disconnect", t.bt_search_disconnect) } else { ("connect", t.bt_search_connect) };
            let id = format!("bluetooth:{verb}:{:x}", d.address);
            let action = Action::new(id, template.replace("{}", &d.name), t.bluetooth, Icon::Broadcast).words(WORDS);
            out.push(match ops.get(&d.address) {
                Some(Doing::Connecting) => action.off(t.bt_connecting),
                Some(Doing::Disconnecting) => action.off(t.bt_disconnecting),
                None => action.off_if(!d.connected && radio == Some(false), t.bt_off),
            });
        }
        out
    }

    fn command(&mut self, cmd: &Command, _cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        match mine(self.key(), id) {
            Some("settings") => open_settings(),
            Some(rest) => {
                let Some((verb, address)) = rest.split_once(':') else { return };
                let Ok(address) = u64::from_str_radix(address, 16) else { return };
                match verb {
                    "connect" => self.toggle(address, Some(Doing::Connecting)),
                    "disconnect" => self.toggle(address, Some(Doing::Disconnecting)),
                    _ => false,
                };
            }
            None => {}
        }
    }

    /// Не вышло подключить или отключить в потоке: «Не удалось подключить «Наушники»: включите устройство и поднесите ближе» —
    /// словами программы, английский текст ошибки остался в журнале. Панель берёт это сама каждый кадр.
    fn problems(&mut self, t: &Tr) -> Vec<(String, Tone)> {
        let failed = std::mem::take(&mut lock(&self.shared).failed);
        failed
            .into_iter()
            .map(|what| match what {
                Failure::Connect(name) => (t.bt_connect_failed.replace("{}", &name), Tone::Danger),
                Failure::Disconnect(name) => (t.bt_disconnect_failed.replace("{}", &name), Tone::Danger),
            })
            .collect()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let (t, lang) = (cx.t, cx.lang);
        let (list, loaded, ops, radio) = {
            let s = lock(&self.shared);
            (s.list.clone(), s.loaded, s.ops.clone(), s.radio)
        };
        if !loaded {
            w::spinner(ui, 16.0);
            return;
        }
        let settings_h = 34.0;
        if list.is_empty() {
            w::note(ui, t.bt_none);
        } else {
            let mut clicked = None;
            egui::ScrollArea::vertical()
                .id_salt("bt")
                .auto_shrink([false, false])
                .max_height(ui.available_height() - settings_h)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                    for d in &list {
                        if row(ui, t, lang, d, ops.get(&d.address).copied(), radio == Some(false)) {
                            clicked = Some(d.address);
                        }
                    }
                });
            if let Some(address) = clicked {
                self.toggle(address, None);
            }
        }
        if w::button(ui, Kind::Ghost, Some(Icon::Gear), t.bt_settings).clicked() {
            open_settings();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_settings_are_offered_and_opened_like_the_button() {
        let en = crate::texts::tr(anvil_ui::Lang::En);
        let mut bluetooth = Bluetooth::default();
        let actions = bluetooth.actions(en, anvil_ui::Lang::En, &Settings::default());
        assert_eq!(actions.len(), 1);
        assert_eq!((actions[0].title.as_str(), actions[0].place.as_str()), ("Bluetooth settings", en.bluetooth));
        assert!(actions[0].enabled && actions[0].words.contains("блютуз"));
        let (mut toasts, mut settings) = (Vec::new(), Settings::default());
        let mut cx = Ctx { t: en, lang: anvil_ui::Lang::En, monitor: "", toasts: &mut toasts, settings: &mut settings };
        let opened = || {
            sys::BLOCKED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter(|w| *w == "launch ms-settings:bluetooth ")
                .count()
        };
        // Чужое действие ничего не открывает, своё — «Параметры Bluetooth» (за заслоном: в тестах ничего не открывается).
        let before = opened();
        bluetooth.command(&Command::RunAction("quick:wifi".to_string()), &mut cx);
        assert_eq!(opened(), before);
        bluetooth.command(&Command::RunAction("bluetooth:settings".to_string()), &mut cx);
        assert_eq!(opened(), before + 1);
    }

    fn device(address: u64, name: &str, connected: bool, battery: Option<u8>) -> bt::Device {
        bt::Device { name: name.into(), connected, battery, address, audio: false }
    }

    /// Наушники, колонки: аудиоустройство.
    fn audio(address: u64, name: &str, connected: bool, battery: Option<u8>) -> bt::Device {
        bt::Device { audio: true, ..device(address, name, connected, battery) }
    }

    /// Первое чтение ничего не объявляет — ни подключённых, ни севших; кто уже сел, о том не скажут и потом.
    #[test]
    fn the_first_read_announces_nothing_and_marks_the_low_ones_as_told() {
        let list = [device(1, "Headphones", true, Some(80)), device(2, "Mouse", true, Some(9))];
        let mut warned = HashSet::new();
        assert!(changes(None, &list, &mut warned).is_empty());
        assert!(changes(Some(&list), &list, &mut warned).is_empty(), "и второе — то же самое — тоже тихое");
        assert_eq!(warned, HashSet::from([2]));
    }

    #[test]
    fn an_audio_device_that_became_connected_is_announced_once() {
        let mut warned = HashSet::new();
        let off = [audio(1, "Headphones", false, None), device(2, "Mouse", true, Some(60))];
        let on = [audio(1, "Headphones", true, Some(80)), device(2, "Mouse", true, Some(60))];
        assert_eq!(changes(Some(&off), &on, &mut warned), [Change::Connected(on[0].clone())]);
        assert!(changes(Some(&on), &on, &mut warned).is_empty(), "ещё раз то же — не событие");
        // Отключилось и подключилось снова — снова событие; новое сопряжённое сразу подключённое — тоже.
        assert!(changes(Some(&on), &off, &mut warned).is_empty(), "отключение плашки не даёт");
        assert_eq!(changes(Some(&off), &on, &mut warned).len(), 1);
        let fresh = [on[0].clone(), on[1].clone(), audio(3, "Speaker", true, None)];
        assert_eq!(changes(Some(&on), &fresh, &mut warned), [Change::Connected(fresh[2].clone())]);
    }

    /// Мышь и клавиатура засыпают и просыпаются сами: «подключено» о них не говорят, а «разряжается» — да.
    #[test]
    fn a_mouse_or_keyboard_waking_up_is_not_news_but_a_flat_battery_is() {
        let mut warned = HashSet::new();
        let asleep = [device(2, "Mouse", false, None), device(3, "Keyboard", false, None)];
        let awake = [device(2, "Mouse", true, Some(60)), device(3, "Keyboard", true, Some(9))];
        let told = changes(Some(&asleep), &awake, &mut warned);
        assert_eq!(told, [Change::Low(awake[1].clone())], "проснулись — молча; у клавиатуры 9 % — разряжается");
    }

    /// Устройство, пропавшее или неподключённое в одном чтении, ещё подключено; в двух подряд — отключено.
    #[test]
    fn a_device_is_off_only_after_two_reads_without_it_connected() {
        let mut off = HashMap::new();
        // Порядок в ответе не важен (пропавшее ещё подключённым идёт в конец): сравниваем по адресам.
        let sorted = |mut list: Vec<bt::Device>| {
            list.sort_by_key(|d| d.address);
            list
        };
        let on = vec![audio(1, "Headphones", true, Some(80)), device(2, "Mouse", true, Some(60))];
        // Первое чтение — как есть.
        assert_eq!(settle(None, on.clone(), &mut off), on);
        // Наушники: в списке, но «не подключены» один раз — всё ещё подключены (с прежним зарядом).
        let blip = vec![audio(1, "Headphones", false, None), device(2, "Mouse", true, Some(60))];
        assert_eq!(settle(Some(&on), blip.clone(), &mut off), on);
        // Вернулись — счёт сброшен.
        assert_eq!(settle(Some(&on), on.clone(), &mut off), on);
        assert!(off.is_empty());
        // Второй раз подряд — отключены.
        assert_eq!(settle(Some(&on), blip.clone(), &mut off), on);
        let after = settle(Some(&on), blip.clone(), &mut off);
        assert_eq!(after, blip, "в двух чтениях подряд — отключены");
        assert!(off.is_empty());
        // Пропал из списка вовсе: так же — одно чтение не в счёт, два — убран.
        let one_left = vec![device(2, "Mouse", true, Some(60))];
        assert_eq!(sorted(settle(Some(&on), one_left.clone(), &mut off)), on);
        assert_eq!(settle(Some(&on), one_left.clone(), &mut off), one_left);
        // Не подключённое и раньше — без отсрочки.
        let was_off = vec![audio(1, "Headphones", false, None)];
        assert_eq!(settle(Some(&was_off), was_off.clone(), &mut off), was_off);
        assert!(off.is_empty());
    }

    /// Сон: одно чтение без наушников и снова они — ложного «подключено» нет; настоящее отключение и возвращение — есть.
    #[test]
    fn a_sleep_blip_does_not_announce_the_headphones_again() {
        let mut warned = HashSet::new();
        let mut off = HashMap::new();
        let on = vec![audio(1, "Headphones", true, Some(80))];
        let mut believed = settle(None, on.clone(), &mut off);
        let announced = std::cell::Cell::new(0usize);
        let mut read = |list: Vec<bt::Device>, believed: &mut Vec<bt::Device>| {
            let next = settle(Some(believed), list, &mut off);
            announced.set(announced.get() + changes(Some(believed), &next, &mut warned).len());
            *believed = next;
        };
        read(vec![], &mut believed);
        read(on.clone(), &mut believed);
        assert_eq!(announced.get(), 0, "одно пустое чтение — не отключение");
        read(vec![], &mut believed);
        read(vec![], &mut believed);
        read(on.clone(), &mut believed);
        assert_eq!(announced.get(), 1, "два пустых чтения подряд — отключились; вернулись — подключено");
    }

    /// «Разряжается» — раз за разряд: заряд упал до 15 % и ниже, повторов нет, пока не поднимется выше 20 %.
    #[test]
    fn a_low_battery_is_told_once_per_discharge() {
        let mut warned = HashSet::new();
        let at = |b: Option<u8>, connected: bool| [device(2, "Mouse", connected, b)];
        let (full, low, lower) = (at(Some(80), true), at(Some(15), true), at(Some(11), true));
        assert!(changes(Some(&full), &at(Some(16), true), &mut warned).is_empty(), "16 % — ещё нет");
        assert_eq!(changes(Some(&full), &low, &mut warned), [Change::Low(low[0].clone())], "ровно 15 % — да");
        assert!(changes(Some(&low), &lower, &mut warned).is_empty(), "ниже — повтора нет");
        // Поднялся до 20 % — ещё в «разряжается»: не забываем; выше 20 % — забыли.
        assert!(changes(Some(&lower), &at(Some(20), true), &mut warned).is_empty());
        assert!(
            changes(Some(&at(Some(20), true)), &at(Some(14), true), &mut warned).is_empty(),
            "не зарядили выше 20 %"
        );
        assert!(changes(Some(&low), &at(Some(21), true), &mut warned).is_empty());
        assert_eq!(
            changes(Some(&at(Some(21), true)), &at(Some(12), true), &mut warned).len(),
            1,
            "новый разряд — новая плашка"
        );
        // Отключённый: заряда нет — ни события, ни забывания.
        assert!(changes(Some(&low), &at(None, false), &mut warned).is_empty());
    }

    /// Плашки: слова и значки; подключилось с низким зарядом — две плашки, каждая о своём.
    #[test]
    fn the_plaques_name_the_device_and_the_charge() {
        let mut headphones = device(1, "Headphones", true, Some(80));
        headphones.audio = true;
        let p = connected_plaque(&headphones);
        assert_eq!(
            (p.kind, p.key.as_str(), p.glyph, p.tone, p.shelf),
            (plaque::Kind::Bluetooth, "bt:1", Glyph::Headphones, Tone::Accent, Some("bluetooth"))
        );
        assert_eq!((p.title.clone(), p.sub.clone()), (Say::Connected("Headphones".into()), Say::BtCharge(Some(80))));
        assert!(!p.sticky && p.buttons.is_empty());
        let mouse = device(2, "Mouse", true, None);
        assert_eq!(connected_plaque(&mouse).glyph, Glyph::Device, "не аудио — значок Bluetooth");
        assert_eq!(connected_plaque(&mouse).sub, Say::BtCharge(None), "заряда нет — просто «Bluetooth»");
        let low = low_plaque(&device(2, "Mouse", true, Some(12)));
        assert_eq!((low.key.as_str(), low.glyph, low.tone), ("bt-low:2", Glyph::Battery, Tone::Warning));
        assert_eq!((low.title, low.sub), (Say::Low("Mouse".into()), Say::Battery(12)));
    }

    /// Модуль отдаёт плашки один раз, только если они включены; выключенные забывает, не копит; включённые заново — снова с
    /// первого чтения, ничего не объявляющего.
    #[test]
    fn the_module_gives_plaques_once_and_only_while_they_are_on() {
        let mut module = Bluetooth::default();
        let on = Settings::default();
        let apply = |module: &Bluetooth, list: Vec<bt::Device>| module.shared.lock().unwrap().apply(list);
        apply(&module, vec![audio(1, "Headphones", false, None)]);
        assert!(module.events(&on).is_empty(), "первое чтение тихое");
        apply(&module, vec![audio(1, "Headphones", true, Some(80))]);
        let first = module.events(&on);
        assert_eq!(first.len(), 1);
        assert!(module.events(&on).is_empty(), "второй раз то же не отдаётся");
        // Выключенные: событие случилось — плашки нет, и оно не ждёт, пока включат.
        let off = Settings {
            plaques: crate::config::Plaques { bluetooth: false, ..Default::default() },
            ..Settings::default()
        };
        // Отключились (два чтения подряд) и подключились снова.
        apply(&module, vec![audio(1, "Headphones", false, None)]);
        apply(&module, vec![audio(1, "Headphones", false, None)]);
        apply(&module, vec![audio(1, "Headphones", true, Some(80))]);
        assert!(module.events(&off).is_empty());
        assert!(module.events(&on).is_empty(), "не копится");
        // Слежка: включили — первое чтение ничего не объявит (прошлого нет), выключили — и так забыто.
        module.read_at = Some(Instant::now());
        module.read_at = Some(Instant::now());
        module.background(&off);
        assert!(!module.watching);
        module.background(&on);
        assert!(module.watching && module.shared.lock().unwrap().prev.is_none());
    }

    /// Выдуманный мир: фоновое чтение читает выдуманные устройства (настоящих в тестах нет), но ничего не объявляет —
    /// список не менялся.
    #[test]
    fn the_background_read_in_the_invented_world_announces_nothing() {
        let mut module = Bluetooth::default();
        module.background(&Settings::default());
        wait_for("поток чтения", || module.shared.lock().unwrap().loaded);
        assert!(module.events(&Settings::default()).is_empty());
        let list = module.shared.lock().unwrap().list.clone();
        assert_eq!(list.len(), 4, "выдуманные «Headphones», «Mouse», «Speaker», «Keyboard»");
    }

    // ─── Подключение из карточки (2.6.0) ────────────────────────────────────

    use anvil_ui::Lang;

    /// Выдуманный мир один на все тесты: кто его меняет, берёт `WORLD_TEST` и начинает с чистого.
    fn world() -> std::sync::MutexGuard<'static, ()> {
        let guard = sys::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        sys::reset_fake_world();
        guard
    }

    /// Ждать, пока `cond` не станет истиной: до 10 с — ПК бывает под нагрузкой, а тесты идут в debug.
    fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
        let start = Instant::now();
        while !cond() {
            assert!(start.elapsed() < Duration::from_secs(10), "не дождались: {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Карточка, что уже прочитала выдуманный мир: устройства и радио.
    fn loaded() -> Bluetooth {
        let mut card = Bluetooth::default();
        card.read(true);
        wait_for("чтение устройств", || {
            let s = card.shared.lock().unwrap();
            s.loaded && !s.busy
        });
        card
    }

    fn connected(card: &Bluetooth, address: u64) -> bool {
        card.shared.lock().unwrap().list.iter().any(|d| d.address == address && d.connected)
    }

    fn idle(card: &Bluetooth) -> bool {
        card.shared.lock().unwrap().ops.is_empty()
    }

    fn doing(card: &Bluetooth, address: u64) -> Option<Doing> {
        card.shared.lock().unwrap().ops.get(&address).copied()
    }

    /// Остановил ли заслон запись с такой меткой.
    fn blocked(what: &str) -> bool {
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|w| w == what)
    }

    const SPEAKER: u64 = bt::FAKE_SPEAKER;

    /// Щелчок по строке колонки: «Подключение…», потом «Подключено» и заряд; ни уведомлений, ни плашки о своём подключении.
    #[test]
    fn connecting_a_speaker_shows_progress_then_the_state() {
        let _world = world();
        let mut card = loaded();
        let (on, en) = (Settings::default(), texts::tr(Lang::En));
        assert!(card.events(&on).is_empty(), "первое чтение тихое");
        assert!(!connected(&card, SPEAKER));
        assert!(card.toggle(SPEAKER, None), "звуковое, не подключено, радио включено — начали");
        assert_eq!(doing(&card, SPEAKER), Some(Doing::Connecting));
        assert!(!card.toggle(SPEAKER, None), "пока идёт, второй раз не начинают");
        wait_for("конец подключения", || idle(&card));
        let list = card.shared.lock().unwrap().list.clone();
        let speaker = list.iter().find(|d| d.address == SPEAKER).unwrap();
        assert_eq!(
            (speaker.connected, speaker.battery),
            (true, Some(60)),
            "список обновлён тем же шагом, что снят «Подключение…»"
        );
        assert!(blocked("bt connect Speaker"), "просьба драйверу — за заслоном");
        assert!(card.problems(en).is_empty(), "вышло — уведомлений нет");
        assert!(card.events(&on).is_empty(), "своё подключение плашки «подключено» не даёт");
        // Щелчок по подключённой строке — отключить.
        assert!(card.toggle(SPEAKER, Some(Doing::Disconnecting)));
        wait_for("конец отключения", || idle(&card));
        assert!(!connected(&card, SPEAKER) && card.problems(en).is_empty());
    }

    /// Свою «подключено» плашка не говорит, а чужую — да: колонку включили сами, а наушники подключили из карточки.
    #[test]
    fn a_connect_from_the_card_gives_no_plaque_but_a_foreign_one_does() {
        let card = Bluetooth::default();
        let on = Settings::default();
        let apply = |list: Vec<bt::Device>| card.shared.lock().unwrap().apply(list);
        apply(vec![audio(1, "Headphones", false, None), audio(2, "Speaker", false, None)]);
        card.shared.lock().unwrap().ops.insert(1, Doing::Connecting);
        apply(vec![audio(1, "Headphones", true, Some(80)), audio(2, "Speaker", true, Some(60))]);
        let mut card = card;
        let told: Vec<Say> = card.events(&on).into_iter().map(|p| p.title).collect();
        assert_eq!(told, [Say::Connected("Speaker".into())], "о наушниках молчок, о колонке — плашка");
        // Отключили из карточки (плашки не даёт и так), потом подключили снова — опять своё.
        card.shared.lock().unwrap().ops.insert(1, Doing::Disconnecting);
        let off = || vec![audio(1, "Headphones", false, None), audio(2, "Speaker", true, Some(60))];
        card.shared.lock().unwrap().apply(off());
        card.shared.lock().unwrap().apply(off());
        card.shared.lock().unwrap().ops.insert(1, Doing::Connecting);
        card.shared
            .lock()
            .unwrap()
            .apply(vec![audio(1, "Headphones", true, Some(80)), audio(2, "Speaker", true, Some(60))]);
        assert!(card.events(&on).is_empty());
    }

    /// Устройство выключено: просьба принята, а подключения нет — через ожидание строка снова прежняя и уведомление своими
    /// словами, один раз; можно пробовать снова.
    #[test]
    fn a_device_that_stays_off_is_reported_in_words_and_can_be_tried_again() {
        let _world = world();
        bt::fake_set_unreachable(SPEAKER, true);
        let mut card = loaded();
        let (ru, en) = (texts::tr(Lang::Ru), texts::tr(Lang::En));
        assert!(card.toggle(SPEAKER, None));
        wait_for("конец ожидания", || idle(&card));
        assert_eq!(
            card.problems(en),
            [("Couldn’t connect “Speaker”: turn the device on and bring it closer".to_string(), Tone::Danger)]
        );
        assert!(card.problems(en).is_empty(), "сказано один раз");
        assert!(!connected(&card, SPEAKER), "строка снова «Не подключено»");
        assert!(card.events(&Settings::default()).is_empty());
        assert!(card.toggle(SPEAKER, None), "можно снова");
        wait_for("второе ожидание", || idle(&card));
        assert_eq!(
            card.problems(ru),
            [("Не удалось подключить «Speaker»: включите устройство и поднесите ближе".to_string(), Tone::Danger)]
        );
    }

    /// Звуковое, но точек Windows у него нет: кнопка есть, щелчок — сразу уведомление о неудаче (без ожидания всех секунд).
    #[test]
    fn a_device_without_endpoints_fails_at_once_with_the_same_words() {
        let _world = world();
        bt::fake_set_no_points(SPEAKER, true);
        let mut card = loaded();
        assert!(card.toggle(SPEAKER, None), "кнопка есть, нажать можно");
        wait_for("отказ", || idle(&card));
        let words = card.problems(texts::tr(Lang::En));
        assert_eq!(words.len(), 1);
        assert!(words[0].0.starts_with("Couldn’t connect “Speaker”"), "{words:?}");
        // И у отключения свои слова.
        bt::fake_set_no_points(1, true);
        assert!(card.toggle(1, None));
        wait_for("отказ отключения", || idle(&card));
        assert_eq!(
            card.problems(texts::tr(Lang::Ru)),
            [("Не удалось отключить «Headphones»".to_string(), Tone::Danger)]
        );
        assert!(connected(&card, 1), "наушники так и подключены");
    }

    #[test]
    fn disconnecting_headphones_shows_its_own_progress() {
        let _world = world();
        let mut card = loaded();
        assert!(card.toggle(1, None));
        assert_eq!(doing(&card, 1), Some(Doing::Disconnecting));
        wait_for("конец отключения", || idle(&card));
        assert!(!connected(&card, 1) && connected(&card, 2), "наушники отключены, мышь — как была");
        assert!(blocked("bt disconnect Headphones"));
        assert!(card.problems(texts::tr(Lang::En)).is_empty());
    }

    /// Мышь, клавиатура, чужой адрес — без щелчка: Windows подключает их сама, когда их включают.
    #[test]
    fn only_audio_devices_can_be_connected() {
        let _world = world();
        let mut card = loaded();
        for address in [2, 3, 0xdead] {
            assert!(!card.toggle(address, None), "{address:x}");
        }
        assert!(idle(&card));
    }

    /// Радио выключено: «Подключить» серая, щелчок ничего не делает; в поиске — серое с причиной.
    #[test]
    fn with_the_radio_off_nothing_connects_and_the_search_says_why() {
        let _world = world();
        net::fake_bluetooth(false);
        let mut card = loaded();
        assert_eq!(card.shared.lock().unwrap().radio, Some(false));
        assert!(!card.toggle(SPEAKER, None), "щелчок по строке ничего не делает");
        assert!(idle(&card));
        let en = texts::tr(Lang::En);
        let actions = card.actions(en, Lang::En, &Settings::default());
        let find = |id: &str| actions.iter().find(|a| a.id == id).unwrap();
        let (connect, disconnect) = (find("bluetooth:connect:4"), find("bluetooth:disconnect:1"));
        assert!(!connect.enabled && connect.reason.as_deref() == Some("Bluetooth is off"));
        assert!(disconnect.enabled, "подключённое отключается и без радио");
        // Включили радио — снова можно.
        net::fake_bluetooth(true);
        card.read(true);
        wait_for("чтение радио", || card.shared.lock().unwrap().radio == Some(true));
        assert!(card.toggle(SPEAKER, None));
        wait_for("конец подключения", || idle(&card));
    }

    /// Поиск: «Подключить «…»» и «Отключить «…»» у звуковых устройств, по языку; мышь и клавиатура — без действий.
    #[test]
    fn the_search_offers_connect_and_disconnect_for_audio_devices_only() {
        let _world = world();
        let card = loaded();
        let (ru, en) = (texts::tr(Lang::Ru), texts::tr(Lang::En));
        let actions = card.actions(en, Lang::En, &Settings::default());
        let list: Vec<(&str, &str, bool)> =
            actions.iter().map(|a| (a.id.as_str(), a.title.as_str(), a.enabled)).collect();
        assert_eq!(
            list,
            [
                ("bluetooth:settings", "Bluetooth settings", true),
                ("bluetooth:disconnect:1", "Disconnect “Headphones”", true),
                ("bluetooth:connect:4", "Connect “Speaker”", true),
            ]
        );
        assert!(actions[1..].iter().all(|a| a.place == en.bluetooth));
        // Слова обоих языков: находится и по «наушники», и по «headset».
        for a in &actions[1..] {
            for word in ["наушники", "колонка", "гарнитура", "bluetooth", "headphones", "speaker", "headset"]
            {
                assert!(a.words.contains(word), "{} · {word}", a.id);
            }
        }
        let titles: Vec<String> =
            card.actions(ru, Lang::Ru, &Settings::default()).into_iter().map(|a| a.title).collect();
        assert_eq!(titles[1..], ["Отключить «Headphones»", "Подключить «Speaker»"]);
        // С устройством что-то делают — серое, с причиной.
        card.shared.lock().unwrap().ops.insert(SPEAKER, Doing::Connecting);
        let busy = card.actions(en, Lang::En, &Settings::default()).remove(2);
        assert!(!busy.enabled && busy.reason.as_deref() == Some("Connecting…"));
        card.shared.lock().unwrap().ops.insert(SPEAKER, Doing::Disconnecting);
        let busy = card.actions(en, Lang::En, &Settings::default()).remove(2);
        assert_eq!(busy.reason.as_deref(), Some("Disconnecting…"));
    }

    /// Действие поиска не делает обратного тому, что в нём написано: устройство успело стать другим — ничего не происходит.
    #[test]
    fn a_search_action_never_does_the_opposite() {
        let _world = world();
        let mut card = loaded();
        let (mut toasts, mut settings) = (Vec::new(), Settings::default());
        let mut cx =
            Ctx { t: texts::tr(Lang::En), lang: Lang::En, monitor: "", toasts: &mut toasts, settings: &mut settings };
        for id in [
            "bluetooth:connect:1",    // наушники уже подключены
            "bluetooth:disconnect:4", // колонка не подключена
            "bluetooth:connect:2",    // мышь — не звуковая
            "bluetooth:disconnect:2",
            "bluetooth:connect:zz",
            "bluetooth:connect:",
            "bluetooth:connect",
            "bluetooth:swim:4",
            "bluetooth:connect:dead", // такого устройства нет
            "quick:wifi",
        ] {
            card.command(&Command::RunAction(id.to_string()), &mut cx);
            assert!(idle(&card), "{id}");
        }
        card.command(&Command::RunAction("bluetooth:connect:4".to_string()), &mut cx);
        assert_eq!(doing(&card, SPEAKER), Some(Doing::Connecting));
        wait_for("конец подключения", || idle(&card));
        assert!(connected(&card, SPEAKER));
        card.command(&Command::RunAction("bluetooth:disconnect:4".to_string()), &mut cx);
        wait_for("конец отключения", || idle(&card));
        assert!(!connected(&card, SPEAKER));
        assert!(card.problems(texts::tr(Lang::En)).is_empty());
    }

    /// Запоздавшее чтение не возвращает прежнее состояние поверх нового: у каждого чтения номер, старее применённого — в корзину.
    #[test]
    fn a_late_read_does_not_undo_a_newer_one() {
        let mut s = Shared::default();
        let (old, new) = (s.next_read(), s.next_read());
        let on = vec![audio(1, "Speaker", true, Some(60))];
        assert!(s.take(new, on.clone()));
        assert!(!s.take(old, vec![audio(1, "Speaker", false, None)]), "запоздавший ответ отброшен");
        assert_eq!(s.list, on);
        assert!(s.take(new, on.clone()), "тот же номер ещё применяется");
    }

    /// Ожидание — как в design: 15 с на подключение, 10 с на отключение, опрос раз в секунду (в тестах — короче).
    #[test]
    fn the_waits_follow_the_design_and_are_short_in_tests() {
        assert_eq!(
            (CONNECT_WAIT, DISCONNECT_WAIT, POLL),
            (Duration::from_secs(15), Duration::from_secs(10), Duration::from_secs(1))
        );
        let (limit, poll) = waits(Doing::Connecting);
        assert!(limit < Duration::from_secs(5) && poll < limit, "в тестах ждать недолго");
    }

    // ─── Карточка в кадрах ──────────────────────────────────────────────────

    use eframe::egui::{Event, Key, Modifiers, PointerButton};

    /// Весь текст, что нарисован в кадре, и где.
    fn painted(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Pos2)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Pos2)>) {
            match shape {
                egui::Shape::Text(t) => {
                    out.push((t.galley.text().to_string(), t.pos + t.galley.rect.center().to_vec2()))
                }
                egui::Shape::Vec(list) => list.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    }

    /// Цвет, которым в кадре нарисовано слово `word` (у надписи кнопки он идёт запасным цветом).
    fn ink_of(shapes: &[egui::epaint::ClippedShape], word: &str) -> Option<Color32> {
        fn find(shape: &egui::Shape, word: &str) -> Option<Color32> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == word => {
                    Some(t.override_text_color.unwrap_or(t.fallback_color))
                }
                egui::Shape::Vec(list) => list.iter().find_map(|s| find(s, word)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| find(&s.shape, word))
    }

    /// Карточка в кадрах без окна: рисует, ловит щелчки и клавиши.
    struct Rig {
        ctx: egui::Context,
        card: Bluetooth,
        settings: Settings,
        toasts: Vec<(String, Tone)>,
        lang: Lang,
        time: f64,
        size: Vec2,
        shapes: Vec<egui::epaint::ClippedShape>,
        cursor: egui::CursorIcon,
    }

    impl Rig {
        fn new(lang: Lang, width: f32) -> Rig {
            let settings = Settings::default();
            let ctx = egui::Context::default();
            anvil_ui::install(&ctx, crate::look::accent_of(&settings.accent), settings.common.theme);
            let mut rig = Rig {
                ctx,
                card: loaded(),
                settings,
                toasts: Vec::new(),
                lang,
                time: 0.0,
                size: Vec2::new(width, 260.0),
                shapes: Vec::new(),
                cursor: egui::CursorIcon::Default,
            };
            rig.settle();
            rig
        }

        fn frame(&mut self, events: Vec<Event>) {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            self.time += 1.0 / 30.0;
            let (card, settings, toasts, lang) = (&mut self.card, &mut self.settings, &mut self.toasts, self.lang);
            let mut out = self.ctx.run_ui(input, |ui| {
                let mut cx =
                    Ctx { t: texts::tr(lang), lang, monitor: "", toasts: &mut *toasts, settings: &mut *settings };
                card.ui(ui, &mut cx);
            });
            // Рисовать некому: изменения текстур никто не применит, и egui без `clear` на этом упадёт.
            out.textures_delta.clear();
            self.shapes = out.shapes;
            self.cursor = out.platform_output.cursor_icon;
        }

        fn settle(&mut self) {
            for _ in 0..4 {
                self.frame(Vec::new());
            }
        }

        fn text(&self) -> String {
            painted(&self.shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>().join("\n")
        }

        fn has(&self, word: &str) -> bool {
            painted(&self.shapes).iter().any(|(t, _)| t == word)
        }

        /// Сколько раз нарисовано слово.
        fn count(&self, word: &str) -> usize {
            painted(&self.shapes).iter().filter(|(t, _)| t == word).count()
        }

        /// Прямоугольник строки устройства.
        fn row(&self, address: u64) -> Rect {
            let id = egui::Id::new(("anchor", format!("bt-{address:x}")));
            self.ctx.read_response(id).unwrap_or_else(|| panic!("строки нет:\n{}", self.text())).rect
        }

        fn move_to(&mut self, pos: Pos2) {
            self.frame(vec![Event::PointerMoved(pos)]);
            self.settle();
        }

        fn click(&mut self, pos: Pos2) {
            self.move_to(pos);
            let button = |pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        fn key(&mut self, key: Key) {
            let event = |pressed| Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![event(true)]);
            self.frame(vec![event(false)]);
            self.settle();
        }

        /// Кадры, пока дело не кончится (поток ответил).
        fn until_idle(&mut self) {
            wait_for("конец работы", || idle(&self.card));
            self.settle();
        }
    }

    /// Строки по макету: у подключённого — точка, имя, «Подключено» и заряд; у звукового не подключённого — «Не подключено» и
    /// «Подключить»; у не звукового не подключённого — «Не подключено» без кнопки. Имена, заряд — по языку.
    #[test]
    fn the_rows_show_the_state_the_charge_and_one_connect_button() {
        let _world = world();
        for (lang, connect, connected, not_connected, charges) in [
            (Lang::En, "Connect", "Connected", "Not connected", ["80%", "45%"]),
            (Lang::Ru, "Подключить", "Подключено", "Не подключено", ["80 %", "45 %"]),
        ] {
            let rig = Rig::new(lang, 380.0);
            let shown = rig.text();
            for name in ["Headphones", "Mouse", "Speaker", "Keyboard"] {
                assert!(rig.has(name), "{name}:\n{shown}");
            }
            assert_eq!(rig.count(connected), 2, "наушники и мышь:\n{shown}");
            assert_eq!(rig.count(not_connected), 2, "колонка и клавиатура:\n{shown}");
            assert_eq!(rig.count(connect), 1, "кнопка одна — у колонки; у клавиатуры её нет:\n{shown}");
            assert!(charges.iter().all(|c| rig.has(c)), "заряд подключённых:\n{shown}");
            assert!(rig.has(texts::tr(lang).bt_settings), "«Параметры Bluetooth» внизу");
            // Кнопка — в строке колонки, справа; клавиатура — ниже без неё.
            let speaker = rig.row(SPEAKER);
            let button = painted(&rig.shapes).into_iter().find(|(t, _)| t == connect).unwrap().1;
            assert!(speaker.contains(button) && button.x > speaker.center().x, "кнопка справа в строке колонки");
            assert!(rig.row(3).top() > speaker.bottom() - 1.0, "четыре строки идут друг под другом");
        }
    }

    /// Щелчок по строке колонки: «Подключение…» без кнопки, потом «Подключено» с зарядом; по не звуковым — ничего.
    #[test]
    fn a_click_connects_and_the_row_tells_so() {
        let _world = world();
        let mut rig = Rig::new(Lang::En, 380.0);
        // Не звуковые — без щелчка: ни работы, ни «руки».
        for address in [2, 3] {
            let at = rig.row(address).center();
            rig.click(at);
            assert!(idle(&rig.card), "{address:x}");
            assert_ne!(rig.cursor, egui::CursorIcon::PointingHand, "над мышью и клавиатурой курсор обычный");
        }
        let at = rig.row(SPEAKER).center();
        rig.move_to(at);
        assert_eq!(rig.cursor, egui::CursorIcon::PointingHand, "над звуковой строкой — «рука»");
        rig.click(at);
        rig.until_idle();
        assert!(!rig.has("Connecting…"));
        assert_eq!(rig.count("Connected"), 3, "наушники, мышь и теперь колонка:\n{}", rig.text());
        assert!(rig.has("60%"), "и её заряд");
        assert_eq!(rig.count("Connect"), 0, "кнопки больше нет");
        assert!(rig.toasts.is_empty(), "карточка уведомлений сама не говорит");
        // Ещё щелчок по подключённой строке — отключить.
        rig.click(at);
        rig.until_idle();
        assert_eq!(rig.count("Connected"), 2);
        assert_eq!(rig.count("Connect"), 1, "«Подключить» вернулась");
    }

    /// Пока идёт — «Подключение…», вертушка вместо кнопки и точки, а имя не серое; второй щелчок ничего не запускает.
    #[test]
    fn while_connecting_the_row_shows_the_wait() {
        let _world = world();
        let mut rig = Rig::new(Lang::En, 380.0);
        // «Подключение» без потока: строка останется в нём, пока мы не снимем.
        rig.card.shared.lock().unwrap().ops.insert(SPEAKER, Doing::Connecting);
        rig.settle();
        assert!(rig.has("Connecting…"), "{}", rig.text());
        assert_eq!(rig.count("Connect"), 0, "кнопки нет");
        assert_eq!(rig.count("Not connected"), 1, "только клавиатура");
        let p = Palette::of_ctx(&rig.ctx);
        assert_eq!(ink_of(&rig.shapes, "Speaker"), Some(p.text), "имя в работе не серое");
        let at = rig.row(SPEAKER).center();
        rig.click(at);
        assert_eq!(doing(&rig.card, SPEAKER), Some(Doing::Connecting));
        assert!(!rig.card.shared.lock().unwrap().busy);
        // «Отключение…» — своё слово.
        rig.card.shared.lock().unwrap().ops.insert(1, Doing::Disconnecting);
        rig.settle();
        assert!(rig.has("Disconnecting…") && !rig.has("80%"), "заряд на время отключения не показан:\n{}", rig.text());
    }

    /// С клавиатуры: Tab до строки колонки (наушники — первая) и Enter.
    #[test]
    fn enter_on_a_focused_row_connects() {
        let _world = world();
        let mut rig = Rig::new(Lang::En, 380.0);
        rig.key(Key::Tab);
        rig.key(Key::Tab);
        let focused = rig.ctx.memory(|m| m.focused()).expect("фокус где-то есть");
        assert_eq!(rig.ctx.read_response(focused).map(|r| r.rect), Some(rig.row(SPEAKER)), "вторая по Tab — колонка");
        rig.key(Key::Enter);
        assert!(doing(&rig.card, SPEAKER).is_some() || connected(&rig.card, SPEAKER), "Enter подключил");
        rig.until_idle();
        assert!(connected(&rig.card, SPEAKER));
    }

    /// Радио выключено: «Подключить» нарисована, но серая; щелчок ничего не запускает, а Tab её не берёт.
    #[test]
    fn with_the_radio_off_the_button_is_grey_and_clicks_do_nothing() {
        let _world = world();
        net::fake_bluetooth(false);
        let mut rig = Rig::new(Lang::En, 380.0);
        assert!(rig.has("Connect"), "кнопка на месте — серая, а не спрятана:\n{}", rig.text());
        let p = Palette::of_ctx(&rig.ctx);
        assert_eq!(ink_of(&rig.shapes, "Connect"), Some(p.faint), "серая кнопка — цветом `faint`");
        let at = rig.row(SPEAKER).center();
        rig.click(at);
        assert!(idle(&rig.card), "щелчок ничего не делает");
        assert_ne!(rig.cursor, egui::CursorIcon::PointingHand, "и «руки» над серой строкой нет");
        // Серая строка не берёт Tab: первой фокус получает подключённая (наушники), а не колонка.
        rig.key(Key::Tab);
        let focused = rig.ctx.memory(|m| m.focused()).expect("фокус где-то есть");
        assert_eq!(rig.ctx.read_response(focused).map(|r| r.rect), Some(rig.row(1)));
        // Включили радио — кнопка снова живая.
        net::fake_bluetooth(true);
        rig.card.read(true);
        wait_for("чтение радио", || rig.card.shared.lock().unwrap().radio == Some(true));
        rig.settle();
        assert_eq!(ink_of(&rig.shapes, "Connect"), Some(p.text));
    }

    /// Узкая карточка: имя режется многоточием, а кнопка остаётся целой.
    #[test]
    fn a_narrow_card_cuts_the_name_not_the_button() {
        let _world = world();
        let rig = Rig::new(Lang::Ru, 220.0);
        let shown = rig.text();
        assert!(rig.has("Подключить"), "кнопка целая:\n{shown}");
        let speaker = rig.row(SPEAKER);
        let inside = painted(&rig.shapes).into_iter().all(|(_, at)| at.x < speaker.right() + 1.0);
        assert!(inside, "ничего не за краем:\n{shown}");
    }
}
