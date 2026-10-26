// Яркость, контраст и вход внешних мониторов по DDC/CI. Мониторы отвечают медленно, поэтому
// все запросы — в своём потоке: окно шлёт ему «перечитай» и «поставь», а читает готовое из общего
// списка. Ползунок под рукой показывает своё значение, пока монитор не подтвердит новое.
// Внизу карточки — «Ночной свет» выключателем: он от монитора не зависит и работает, даже если DDC/CI молчит.
// Сцены ставят отсюда яркость всех мониторов, что отвечают по DDC/CI, и «Ночной свет»: перед сменой модуль помнит, что
// было (`before`), и второй щелчок по стоящей сцене возвращает это; прежнюю яркость помнит и возвращает поток модуля.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

use super::{Command, Ctx, Module, chips};
use crate::config::{Profile, SceneRow};
use crate::sys::ddc::{self, BRIGHTNESS, CONTRAST, Feature, INPUT, Physical};
use crate::sys::{self, display, monitor, nightlight};
use crate::texts::{self, Tr};

/// Высота строки «Ночной свет» внизу карточки (вместе с чертой над ней).
const NIGHT_H: f32 = 36.0;

/// Что поток делает по просьбе окна.
enum Job {
    /// Перечитать все мониторы; `caps` — ещё и входы (строка возможностей, медленно).
    Read {
        monitors: Vec<(String, usize)>,
        caps: bool,
    },
    Set {
        handle: usize,
        index: usize,
        code: u8,
        value: u32,
    },
    /// Сцена: поставить яркость в процентах всем мониторам, что отвечают, запомнив прежнюю; `keep` — прежнюю уже помним с
    /// первого раза этой же сцены (у кого помним, остаётся).
    SceneBright {
        monitors: Vec<(String, usize)>,
        percent: u8,
        keep: bool,
    },
    /// Вернуть яркость, как была до сцены.
    SceneBrightUndo,
}

/// Ключ значения: монитор, физический номер, код VCP.
type Key = (String, usize, u8);

/// Что не вышло в потоке модуля: слова программы скажет `Module::problems`, английский текст ошибки остаётся в журнале.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Problem {
    /// Монитор не принял значение ползунка.
    NotAccepted,
    /// Яркость сцены не поставилась: монитор не ответил по DDC/CI даже после повторов.
    SceneBright,
}

#[derive(Default)]
struct Shared {
    list: Vec<Physical>,
    /// Прочитали хотя бы раз — до этого карточка показывает «опрашиваю».
    loaded: bool,
    /// Что не вышло с прошлого раза — панель скажет уведомлением, даже если карточку не рисуют.
    problems: Vec<Problem>,
    /// Мониторы, что хоть раз отвечали яркостью по DDC/CI: их молчание после смены режима — повод подождать и сказать.
    /// Пока не отвечал никто, ждут все мониторы, у которых есть что опрашивать.
    answered: Vec<String>,
    /// Яркость до сцены: монитор, физический номер, «ручка» и значение, как его отдал монитор.
    bright_before: Vec<(String, usize, usize, u32)>,
}

impl Shared {
    fn report(&mut self, problem: Problem) {
        if !self.problems.contains(&problem) {
            self.problems.push(problem);
        }
    }
}

/// Как было до сцены: что стояло в тех настройках, которые она поменяла (`None` / `false` — не меняла).
struct SceneBefore {
    /// Номер сцены ([`Profile::id`]): переименование возврат не теряет, новая сцена с тем же именем чужой не наследует.
    scene: u64,
    night: Option<bool>,
    /// Яркость сцена трогала: прежняя лежит у потока (`Shared::bright_before`).
    bright: bool,
}

/// Яркость монитора в процентах.
fn percent(f: Feature) -> u8 {
    (f.current * 100 + f.max / 2).checked_div(f.max).map_or(0, |p| p.min(100) as u8)
}

pub struct Brightness {
    jobs: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
    /// Что двигали сами и когда — показывать своё, пока монитор не догонит.
    local: HashMap<Key, (u32, Instant)>,
    /// Входы, прочитанные из возможностей, — помним между перечитываниями.
    inputs: HashMap<(String, usize), Vec<u32>>,
    /// Сколько раз уже спрашивали входы: не ответил — ещё раз, но не больше трёх (это медленно).
    caps_tries: u8,
    /// Ждём подтверждения смены входа: монитор, физический номер, код входа.
    confirm_input: Option<(String, usize, usize, u32)>,
    /// Ночной свет Windows: включён ли; `None` — не узнать.
    night: Option<bool>,
    /// Как было до сцены, что стоит сейчас, — на второй щелчок по ней.
    before: Option<SceneBefore>,
}

impl Default for Brightness {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let s = shared.clone();
        std::thread::spawn(move || worker(rx, s));
        Brightness {
            jobs: tx,
            shared,
            local: HashMap::new(),
            inputs: HashMap::new(),
            caps_tries: 0,
            confirm_input: None,
            night: None,
            before: None,
        }
    }
}

fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
    while let Ok(first) = rx.recv() {
        // Скопилось — ставим только последнее значение каждого ползунка, читаем один раз.
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
        }
        let mut sets: Vec<(usize, usize, u8, u32)> = Vec::new();
        let mut read: Option<(Vec<(String, usize)>, bool)> = None;
        for j in jobs {
            match j {
                Job::Set { handle, index, code, value } => {
                    sets.retain(|s| !(s.0 == handle && s.1 == index && s.2 == code));
                    sets.push((handle, index, code, value));
                }
                Job::Read { monitors, caps } => {
                    let caps = caps || read.as_ref().is_some_and(|r| r.1);
                    read = Some((monitors, caps));
                }
                // Сцена идёт по порядку: что накопилось раньше (ползунки) — ставим сперва; после неё — перечитать.
                Job::SceneBright { monitors, percent, keep } => {
                    flush(&mut sets, &shared);
                    scene_bright(&shared, &monitors, percent, keep);
                    let caps = read.as_ref().is_some_and(|r| r.1);
                    read = Some((monitors, caps));
                }
                Job::SceneBrightUndo => {
                    flush(&mut sets, &shared);
                    let monitors = scene_bright_undo(&shared);
                    let caps = read.as_ref().is_some_and(|r| r.1);
                    read = Some((monitors, caps));
                }
            }
        }
        flush(&mut sets, &shared);
        if let Some((monitors, caps)) = read {
            let list: Vec<Physical> = monitors.iter().flat_map(|(d, h)| ddc::read(d, *h, caps)).collect();
            if std::env::var_os("ANTRESOL_TRACE").is_some() {
                eprintln!("ddc: {} monitors, {} physical: {:?}", monitors.len(), list.len(), list);
            }
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            for p in list.iter().filter(|p| p.brightness.is_some()) {
                if !s.answered.contains(&p.device) {
                    s.answered.push(p.device.clone());
                }
            }
            s.list = list;
            s.loaded = true;
        }
    }
}

/// Поставить накопленные значения ползунков.
fn flush(sets: &mut Vec<(usize, usize, u8, u32)>, shared: &Arc<Mutex<Shared>>) {
    for (handle, index, code, value) in std::mem::take(sets) {
        if let Err(e) = ddc::set(handle, index, code, value) {
            eprintln!("ddc set failed: {e}");
            shared.lock().unwrap_or_else(|e| e.into_inner()).report(Problem::NotAccepted);
        }
    }
}

/// Сколько ждать ответа монитора по DDC/CI после смены режима или HDR: он пересинхронизируется 1–3 с и пока молчит.
const SILENT_FOR: Duration = if cfg!(test) { Duration::from_millis(600) } else { Duration::from_secs(3) };
/// Как часто спрашивать молчащий монитор снова.
const RETRY_EVERY: Duration = if cfg!(test) { Duration::from_millis(40) } else { Duration::from_millis(300) };

/// Чем кончилась попытка поставить яркость монитору.
enum Attempt {
    /// Поставлена (или уже такая).
    Done,
    /// Монитор есть, но молчит или не принял значение: подождать и спросить снова.
    Silent,
    /// Нечего ставить: DDC/CI у такого монитора нет (встроенный экран) или он и раньше не отвечал.
    Skipped,
}

/// Одна попытка: прочитать яркость монитора, запомнить прежнюю (один раз на монитор) и поставить `percent` %.
fn bright_attempt(
    shared: &Arc<Mutex<Shared>>,
    device: &str,
    handle: usize,
    percent: u8,
    was: &mut Vec<(String, usize, usize, u32)>,
) -> Attempt {
    let physical = ddc::read(device, handle, false);
    if physical.is_empty() {
        return Attempt::Skipped;
    }
    let (mut answered, mut failed) = (false, false);
    for p in &physical {
        let Some(b) = p.brightness else { continue };
        answered = true;
        if !was.iter().any(|w| w.0 == device && w.1 == p.index) {
            was.push((device.to_string(), p.index, handle, b.current));
        }
        let want = (u32::from(percent) * b.max + 50) / 100;
        if want != b.current
            && let Err(e) = ddc::set(handle, p.index, BRIGHTNESS, want)
        {
            eprintln!("ddc scene set failed: {e}");
            failed = true;
        }
    }
    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
    if answered && !s.answered.iter().any(|d| d == device) {
        s.answered.push(device.to_string());
    }
    // Молчащий монитор ждём, только если он уже отвечал (или не отвечал ещё никто): не по DDC/CI управляемому
    // второму монитору предупреждение при каждой сцене ни к чему.
    let expected = s.answered.is_empty() || s.answered.iter().any(|d| d == device);
    match (answered, failed) {
        (true, false) => Attempt::Done,
        (true, true) => Attempt::Silent,
        (false, _) if expected => Attempt::Silent,
        (false, _) => Attempt::Skipped,
    }
}

/// Сцена: запомнить яркость каждого отвечающего монитора и поставить `percent` %. Прежнее запоминается один раз за «сеанс»
/// сцены: при `keep` у монитора, что уже помним, остаётся то, что было до первой постановки. Монитор, что только что
/// сменил режим или HDR, молчит по DDC/CI секунду-другую — его спрашивают снова до `SILENT_FOR`; не ответил — модуль
/// скажет, что яркость не поставилась (`Problem::SceneBright`).
fn scene_bright(shared: &Arc<Mutex<Shared>>, monitors: &[(String, usize)], percent: u8, keep: bool) {
    let mut was: Vec<(String, usize, usize, u32)> = Vec::new();
    let mut waiting: Vec<&(String, usize)> = monitors.iter().collect();
    let deadline = Instant::now() + SILENT_FOR;
    loop {
        waiting.retain(|(device, handle)| {
            matches!(bright_attempt(shared, device, *handle, percent, &mut was), Attempt::Silent)
        });
        if waiting.is_empty() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(RETRY_EVERY);
    }
    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
    let mut before = if keep { std::mem::take(&mut s.bright_before) } else { Vec::new() };
    for w in was {
        if !before.iter().any(|b| b.0 == w.0 && b.1 == w.1) {
            before.push(w);
        }
    }
    s.bright_before = before;
    if !waiting.is_empty() {
        s.report(Problem::SceneBright);
    }
}

/// Сцена: вернуть яркость, как была — с теми же повторами: возврат режима только что сбил монитор с шины. Ответ —
/// мониторы, что трогали (их перечитать).
fn scene_bright_undo(shared: &Arc<Mutex<Shared>>) -> Vec<(String, usize)> {
    let before = std::mem::take(&mut shared.lock().unwrap_or_else(|e| e.into_inner()).bright_before);
    let mut monitors: Vec<(String, usize)> = Vec::new();
    for (device, _, handle, _) in &before {
        if !monitors.iter().any(|m| &m.0 == device) {
            monitors.push((device.clone(), *handle));
        }
    }
    let mut left = before;
    let deadline = Instant::now() + SILENT_FOR;
    loop {
        left.retain(|(_, index, handle, old)| match ddc::set(*handle, *index, BRIGHTNESS, *old) {
            Ok(()) => false,
            Err(e) => {
                eprintln!("ddc scene undo failed: {e}");
                true
            }
        });
        if left.is_empty() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(RETRY_EVERY);
    }
    if !left.is_empty() {
        shared.lock().unwrap_or_else(|e| e.into_inner()).report(Problem::SceneBright);
    }
    monitors
}

impl Brightness {
    fn handle_of(device: &str) -> Option<usize> {
        // Демо и тесты: у выдуманного монитора своя «ручка» (по ней `ddc::set` знает, чью яркость запомнить); писатель всё
        // равно остановит заслон.
        if sys::isolated() {
            return Some(ddc::fake_handle(device));
        }
        monitor::monitors().into_iter().find(|m| m.device == device).map(|m| m.handle)
    }

    /// Мониторы панели с «ручками» для потока: настоящие или (демо, тесты) выдуманные.
    fn ddc_monitors(panel: &str) -> Vec<(String, usize)> {
        display::monitors(panel)
            .into_iter()
            .map(|m| {
                let handle = if sys::isolated() { ddc::fake_handle(&m.device) } else { m.handle };
                (m.device, handle)
            })
            .collect()
    }

    fn send_set(&mut self, p: &Physical, code: u8, value: u32) {
        let Some(handle) = Self::handle_of(&p.device) else { return };
        self.local.insert((p.device.clone(), p.index, code), (value, Instant::now()));
        let _ = self.jobs.send(Job::Set { handle, index: p.index, code, value });
    }

    /// Значение ползунка: своё, пока свежее, иначе — что ответил монитор.
    fn shown(&self, p: &Physical, code: u8, from_monitor: u32) -> u32 {
        match self.local.get(&(p.device.clone(), p.index, code)) {
            Some((v, at)) if at.elapsed() < Duration::from_secs(3) => *v,
            _ => from_monitor,
        }
    }

    /// Ползунок 0..max в процентах; колесо мыши над ним — шаг 5 %.
    fn slider(&mut self, ui: &mut Ui, label: &str, p: &Physical, code: u8, f: ddc::Feature) {
        if f.max == 0 {
            return;
        }
        let value = self.shown(p, code, f.current);
        let mut pct = (value as f32 * 100.0 / f.max as f32).round();
        let row = ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).color(Palette::of(ui).weak));
            ui.spacing_mut().slider_width = (ui.available_width() - 48.0).max(60.0);
            let r = ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).show_value(false));
            w::mono(ui, &format!("{pct:>3.0}%"), None);
            r
        });
        let mut changed = row.inner.changed();
        let wheel: f32 = ui.input(|i| {
            i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum()
        });
        if wheel != 0.0 && ui.is_enabled() && ui.rect_contains_pointer(row.response.rect) {
            pct = (pct + wheel.signum() * 5.0).clamp(0.0, 100.0);
            changed = true;
        }
        if changed {
            let v = (pct / 100.0 * f.max as f32).round() as u32;
            self.send_set(p, code, v);
        }
    }
}

impl Brightness {
    /// Поставить то, что сцена отметила здесь: «Ночной свет» и яркость. Перед сменой — запомнить, что было. Возврат прежней
    /// сцены пропадает; та же сцена ещё раз продолжает прежний «сеанс»: что уже помним, то и остаётся.
    fn apply_scene(&mut self, scene: &Profile, cx: &mut Ctx) {
        let s = &scene.system;
        let mut before = match self.before.take() {
            Some(b) if b.scene == scene.id => b,
            _ => SceneBefore { scene: scene.id, night: None, bright: false },
        };
        if let Some(want) = s.night
            && let Some(now) = nightlight::state()
        {
            before.night = before.night.or(Some(now));
            if now != want {
                match nightlight::set(want) {
                    Ok(()) => self.night = Some(want),
                    Err(e) => {
                        eprintln!("scene night light failed: {e}");
                        cx.toasts.push((texts::scene_failed(cx.t, SceneRow::Night), Tone::Danger));
                    }
                }
            }
        }
        if let Some(percent) = s.bright {
            let monitors = Self::ddc_monitors(cx.monitor);
            let _ = self.jobs.send(Job::SceneBright { monitors, percent, keep: before.bright });
            before.bright = true;
        }
        self.before = (before.night.is_some() || before.bright).then_some(before);
    }

    /// Вернуть то, что сцена поменяла, как было до неё. Одноразово.
    fn undo_scene(&mut self, scene: &Profile, cx: &mut Ctx) {
        if !self.scene_can_undo(scene) {
            return;
        }
        let Some(before) = self.before.take() else { return };
        if let Some(night) = before.night
            && nightlight::state().is_some_and(|now| now != night)
        {
            match nightlight::set(night) {
                Ok(()) => self.night = Some(night),
                Err(e) => {
                    eprintln!("scene night light undo failed: {e}");
                    cx.toasts.push((texts::scene_failed(cx.t, SceneRow::Night), Tone::Danger));
                }
            }
        }
        if before.bright {
            let _ = self.jobs.send(Job::SceneBrightUndo);
        }
    }

    /// Взять текущие значения этих строк в сцену: «Ночной свет» — как стоит, яркость — первого монитора, что отвечает; что не
    /// прочиталось — как было в сцене.
    fn save_scene(&mut self, scene: &mut Profile, rows: &[SceneRow]) {
        for row in rows {
            match row {
                SceneRow::Night => scene.system.night = nightlight::state().or(scene.system.night),
                SceneRow::Bright => {
                    let list = self.shared.lock().unwrap_or_else(|e| e.into_inner()).list.clone();
                    let first = list.iter().find_map(|p| p.brightness).map(percent);
                    scene.system.bright = first.or(scene.system.bright);
                }
                _ => {}
            }
        }
    }

    /// Подтверждение смены входа: экран может погаснуть — вернуть можно только кнопками монитора. `false` — смены входа
    /// никто не просил, рисовать нечего.
    fn confirm_input_ui(&mut self, ui: &mut Ui, cx: &mut Ctx, list: &[Physical]) -> bool {
        let t = cx.t;
        let Some((device, index, list_i, code)) = self.confirm_input.clone() else { return false };
        let name = list.get(list_i).map(|p| p.name.clone()).unwrap_or_default();
        let body = texts::input_warning(cx.lang, &name, &ddc::input_name(code));
        w::banner(ui, Tone::Warning, t.input_title, &body, |ui| {
            if w::button(ui, Kind::Danger, None, t.input_switch).clicked() {
                if let Some(handle) = Self::handle_of(&device) {
                    let _ = self.jobs.send(Job::Set { handle, index, code: INPUT, value: code });
                }
                // Чип показывает новый вход сразу, не дожидаясь следующего опроса.
                self.local.insert((device.clone(), index, INPUT), (code, Instant::now()));
                self.confirm_input = None;
            }
            if w::button(ui, Kind::Secondary, None, t.cancel).clicked() {
                self.confirm_input = None;
            }
        });
        true
    }

    /// Ползунки и входы каждого монитора, что отвечает по DDC/CI; пока опрашиваются — «Опрашиваю…», никто не
    /// отвечает — пояснение.
    fn monitors_ui(&mut self, ui: &mut Ui, cx: &mut Ctx, list: &[Physical], loaded: bool) {
        let t = cx.t;
        if !loaded {
            ui.horizontal(|ui| {
                w::spinner(ui, 16.0);
                w::note(ui, t.ddc_reading);
            });
            return;
        }
        let answering: Vec<&Physical> = list.iter().filter(|p| p.brightness.is_some()).collect();
        if answering.is_empty() {
            w::note(ui, t.ddc_none);
            return;
        }
        egui::ScrollArea::vertical().id_salt("ddc").auto_shrink([false, false]).show(ui, |ui| {
            let mut first = true;
            for (i, p) in list.iter().enumerate() {
                let Some(b) = p.brightness else { continue };
                if !first {
                    ui.add_space(6.0);
                }
                first = false;
                // Имя монитора обычным текстом; устройство (`DISPLAYn`) — только в подсказке.
                let short = p.device.trim_start_matches(r"\\.\");
                if !p.name.is_empty() {
                    let name = egui::RichText::new(&p.name).size(12.5).color(Palette::of(ui).weak);
                    ui.label(name).on_hover_text(short);
                }
                self.slider(ui, t.brightness_short, p, BRIGHTNESS, b);
                if let Some(c) = p.contrast {
                    self.slider(ui, t.contrast, p, CONTRAST, c);
                }
                let inputs = self.inputs.get(&(p.device.clone(), p.index)).cloned().unwrap_or_default();
                if inputs.len() > 1 {
                    let names: Vec<String> = inputs.iter().map(|c| ddc::input_name(*c)).collect();
                    let opts: Vec<(u32, &str)> = inputs.iter().zip(&names).map(|(c, n)| (*c, n.as_str())).collect();
                    let mut want = self.shown(p, INPUT, p.input.unwrap_or(0));
                    if chips(ui, &mut want, &opts) {
                        self.confirm_input = Some((p.device.clone(), p.index, i, want));
                    }
                }
            }
        });
    }

    /// «Ночной свет» внизу карточки: черта, значок, подпись и выключатель. Не узнать состояние — строка недоступна.
    fn night_row(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
        let inner = Rect::from_min_max(rect.min + Vec2::new(0.0, 7.0), rect.max);
        let layout = egui::Layout::left_to_right(egui::Align::Center);
        let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(layout));
        row.spacing_mut().item_spacing.x = 10.0;
        let (icon, _) = row.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        anvil_ui::icons::paint(row.painter(), icon, Icon::Moon, p.weak);
        let known = self.night.is_some();
        let mut on = self.night.unwrap_or(false);
        let r = row.add_enabled_ui(known, |ui| w::switch(ui, &mut on, t.night_light)).inner;
        if !known {
            r.on_disabled_hover_text(t.unavailable);
        } else if r.changed() {
            match nightlight::set(on) {
                Ok(()) => self.night = Some(on),
                Err(e) => {
                    eprintln!("night light failed: {e}");
                    cx.toasts.push((texts::scene_failed(t, SceneRow::Night), Tone::Danger));
                }
            }
        }
    }
}

impl Module for Brightness {
    fn key(&self) -> &'static str {
        "brightness"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.brightness
    }

    fn refresh(&mut self, panel: &str) {
        let monitors = Self::ddc_monitors(panel);
        // Входы — пока не ответили, до трёх раз: строка возможностей отвечает до секунды на монитор.
        let caps = self.inputs.is_empty() && self.caps_tries < 3;
        if caps {
            self.caps_tries += 1;
        }
        let _ = self.jobs.send(Job::Read { monitors, caps });
        self.night = nightlight::state();
    }

    fn busy(&self) -> bool {
        self.confirm_input.is_some()
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        match cmd {
            Command::ApplyScene(i) => {
                if let Some(scene) = cx.settings.profiles.get(*i).cloned() {
                    self.apply_scene(&scene, cx);
                }
            }
            Command::UndoScene(i) => {
                if let Some(scene) = cx.settings.profiles.get(*i).cloned() {
                    self.undo_scene(&scene, cx);
                }
            }
            Command::SaveScene(i, rows) => {
                if let Some(scene) = cx.settings.profiles.get_mut(*i) {
                    self.save_scene(scene, rows);
                }
            }
            Command::Dropped(_) | Command::Media(_) => {}
        }
    }

    /// Совпадает ли то, что сцена отметила здесь, с тем, что стоит: «Ночной свет» и яркость всех отвечающих мониторов (±2 %
    /// от заданной). Не прочиталось — не мешает; ничего не отмечено или не прочитать — `None`.
    fn scene_part(&self, scene: &Profile) -> Option<bool> {
        let s = &scene.system;
        let mut same: Vec<bool> = Vec::new();
        if let (Some(want), Some(now)) = (s.night, self.night) {
            same.push(want == now);
        }
        if let Some(want) = s.bright {
            let list = self.shared.lock().unwrap_or_else(|e| e.into_inner()).list.clone();
            let now: Vec<u8> = list.iter().filter_map(|p| p.brightness).map(percent).collect();
            if !now.is_empty() {
                same.push(now.iter().all(|p| p.abs_diff(want) <= 2));
            }
        }
        (!same.is_empty()).then(|| same.into_iter().all(|ok| ok))
    }

    fn scene_can_undo(&self, scene: &Profile) -> bool {
        self.before.as_ref().is_some_and(|b| b.scene == scene.id)
    }

    /// Монитор не принял значение или не ответил по DDC/CI: слова программы; английский текст — в журнале.
    fn problems(&mut self, t: &Tr) -> Vec<(String, Tone)> {
        let problems = std::mem::take(&mut self.shared.lock().unwrap_or_else(|e| e.into_inner()).problems);
        problems
            .into_iter()
            .map(|p| match p {
                Problem::NotAccepted => (t.ddc_failed.to_string(), Tone::Danger),
                Problem::SceneBright => (t.scene_bright_failed.to_string(), Tone::Warning),
            })
            .collect()
    }

    /// «Яркость» — если хоть один монитор отвечает по DDC/CI; «Ночной свет» — если прочитался.
    fn scene_rows(&self) -> Vec<(SceneRow, bool)> {
        let answering =
            self.shared.lock().unwrap_or_else(|e| e.into_inner()).list.iter().any(|p| p.brightness.is_some());
        vec![(SceneRow::Bright, answering), (SceneRow::Night, self.night.is_some())]
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let (list, loaded) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.list.clone(), s.loaded)
        };
        for p in &list {
            if !p.inputs.is_empty() {
                self.inputs.insert((p.device.clone(), p.index), p.inputs.clone());
            }
        }

        // Внизу — «Ночной свет» (всегда: и пока мониторы опрашиваются, и если по DDC/CI никто не отвечает, и пока висит
        // подтверждение смены входа), выше — мониторы или это подтверждение.
        let full = ui.available_rect_before_wrap();
        let night = Rect::from_min_max(Pos2::new(full.left(), (full.bottom() - NIGHT_H).max(full.top())), full.max);
        let body = Rect::from_min_max(full.min, Pos2::new(full.right(), (night.top() - 6.0).max(full.top() + 24.0)));
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        if !self.confirm_input_ui(&mut body_ui, cx, &list) {
            self.monitors_ui(&mut body_ui, cx, &list, loaded);
        }
        self.night_row(ui, night, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;

    /// Весь текст, что нарисовали за кадр.
    fn painted_text(shapes: &[egui::epaint::ClippedShape]) -> String {
        fn walk(shape: &egui::Shape, out: &mut String) {
            match shape {
                egui::Shape::Text(t) => {
                    out.push_str(t.galley.text());
                    out.push('\n');
                }
                egui::Shape::Vec(list) => list.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = String::new();
        shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    }

    fn frame(card: &mut Brightness) -> String {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        anvil_ui::install(&ctx, crate::look::accent_of(&settings.accent), settings.common.theme);
        let mut toasts = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 300.0))),
            ..Default::default()
        };
        let mut shown = String::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(input.clone(), |ui| {
                let mut cx = Ctx {
                    t: texts::tr(anvil_ui::Lang::En),
                    lang: anvil_ui::Lang::En,
                    monitor: r"\\.\TEST-BRIGHTNESS",
                    toasts: &mut toasts,
                    settings: &mut settings,
                };
                card.ui(ui, &mut cx);
            });
            out.textures_delta.clear();
            shown = painted_text(&out.shapes);
        }
        shown
    }

    #[test]
    fn night_light_row_stays_while_input_is_confirmed() {
        let mut card = Brightness::default();
        assert!(frame(&mut card).contains("Night light"), "без подтверждения строка есть (контроль)");
        card.confirm_input = Some((r"\\.\TEST-BRIGHTNESS".into(), 0, 0, 0x0F));
        let shown = frame(&mut card);
        assert!(shown.contains("Night light"), "пока висит подтверждение входа, «Ночной свет» виден: {shown}");
        assert!(shown.contains(texts::tr(anvil_ui::Lang::En).input_title), "и само подтверждение тоже: {shown}");
    }
}

// ─── Сцены без окна ─────────────────────────────────────────────────────────
//
// Выдуманный монитор DDC/CI помнит яркость (у каждого теста свой — по имени устройства), выдуманный «Ночной свет» один на
// все тесты (под `WORLD_TEST`). Яркость идёт через поток модуля — тесты ждут его.

#[cfg(test)]
mod scene_tests {
    use std::sync::MutexGuard;
    use std::time::{Duration, Instant};

    use anvil_ui::Lang;

    use super::*;
    use crate::config::{SceneSystem, Settings};

    /// Мир под замком, как вначале.
    fn world() -> MutexGuard<'static, ()> {
        let guard = sys::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        sys::reset_fake_world();
        guard
    }

    fn scene(name: &str, system: SceneSystem) -> Profile {
        Profile { system, ..Profile::new(name) }
    }

    /// Команда модулю на мониторе `dev`; ответ — уведомления.
    fn run(card: &mut Brightness, settings: &mut Settings, dev: &str, cmd: Command) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        let mut cx = Ctx { t: texts::tr(Lang::En), lang: Lang::En, monitor: dev, toasts: &mut toasts, settings };
        card.command(&cmd, &mut cx);
        toasts
    }

    fn wait(what: &str, mut ok: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ok() {
            assert!(start.elapsed() < Duration::from_secs(3), "не дождались: {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Яркость выдуманного монитора `dev`, %.
    fn world_bright(dev: &str) -> u32 {
        ddc::read(dev, 0, false)[0].brightness.unwrap().current
    }

    /// Карточка на выдуманном мониторе: опрошена, как при показе панели, и ответила.
    fn card(dev: &str) -> Brightness {
        let mut card = Brightness::default();
        card.refresh(dev);
        wait("монитор ответил", || card.scene_rows().contains(&(SceneRow::Bright, true)));
        card
    }

    #[test]
    fn scene_sets_brightness_and_night_light_and_the_second_click_returns_them() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-SCENE";
        let mut card = card(dev);
        let mut settings = Settings::default();
        settings
            .profiles
            .push(scene("Movie", SceneSystem { bright: Some(30), night: Some(true), ..Default::default() }));
        let movie = settings.profiles[0].clone();
        assert_eq!(world_bright(dev), 70, "выдуманный монитор вначале — 70 %");
        assert_eq!(card.scene_part(&movie), Some(false));
        let toasts = run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        assert!(toasts.is_empty(), "{toasts:?}");
        wait("яркость поставлена", || world_bright(dev) == 30);
        assert_eq!(nightlight::state(), Some(true));
        wait("карточка видит сцену стоящей", || card.scene_part(&movie) == Some(true));
        assert!(card.scene_can_undo(&movie));
        // Второй щелчок.
        run(&mut card, &mut settings, dev, Command::UndoScene(0));
        wait("яркость вернулась", || world_bright(dev) == 70);
        assert_eq!(nightlight::state(), Some(false));
        wait("карточка видит, что сцены нет", || card.scene_part(&movie) == Some(false));
        // Возврат одноразовый.
        assert!(!card.scene_can_undo(&movie));
        ddc::set(ddc::fake_handle(dev), 0, BRIGHTNESS, 55).unwrap();
        run(&mut card, &mut settings, dev, Command::UndoScene(0));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(world_bright(dev), 55);
    }

    #[test]
    fn brightness_matches_within_two_percent_and_not_beyond() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-TOLERANCE";
        let card = card(dev);
        let want = |b| scene("S", SceneSystem { bright: Some(b), ..Default::default() });
        // Монитор на 70 %.
        for (asked, stands) in [(70, true), (68, true), (72, true), (67, false), (73, false), (0, false), (100, false)]
        {
            assert_eq!(card.scene_part(&want(asked)), Some(stands), "{asked} %");
        }
        // Не отмечено — нечего проверять.
        assert_eq!(card.scene_part(&Profile::new("N")), None);
    }

    #[test]
    fn the_same_scene_again_returns_to_what_was_before_the_first_time() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-AGAIN";
        let mut card = card(dev);
        let mut settings = Settings::default();
        settings.profiles.push(scene("Dim", SceneSystem { bright: Some(30), ..Default::default() }));
        run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        wait("яркость поставлена", || world_bright(dev) == 30);
        // Яркость ушла от сцены сама, и сцену ставят снова.
        ddc::set(ddc::fake_handle(dev), 0, BRIGHTNESS, 50).unwrap();
        run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        wait("яркость поставлена снова", || world_bright(dev) == 30);
        run(&mut card, &mut settings, dev, Command::UndoScene(0));
        wait("вернулась к тому, что было до первого раза", || {
            world_bright(dev) == 70
        });
    }

    #[test]
    fn another_scene_replaces_the_return() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-OTHER";
        let mut card = card(dev);
        let mut settings = Settings::default();
        settings.profiles.push(scene("Dim", SceneSystem { bright: Some(30), ..Default::default() }));
        settings.profiles.push(scene("Night", SceneSystem { night: Some(true), ..Default::default() }));
        let (dim, night) = (settings.profiles[0].clone(), settings.profiles[1].clone());
        run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        wait("яркость поставлена", || world_bright(dev) == 30);
        assert!(card.scene_can_undo(&dim));
        run(&mut card, &mut settings, dev, Command::ApplyScene(1));
        assert!(!card.scene_can_undo(&dim) && card.scene_can_undo(&night));
        run(&mut card, &mut settings, dev, Command::UndoScene(0));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(world_bright(dev), 30, "чужой возврат яркость не трогает");
        // «Ночной свет» до сцены «Night» был выключен.
        run(&mut card, &mut settings, dev, Command::UndoScene(1));
        assert_eq!(nightlight::state(), Some(false));
        assert_eq!(world_bright(dev), 30, "яркость сцены «Night» не меняла");
    }

    #[test]
    fn save_scene_takes_brightness_of_the_first_monitor_and_night_light() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-SAVE";
        ddc::set(ddc::fake_handle(dev), 0, BRIGHTNESS, 40).unwrap();
        let mut card = card(dev);
        let mut settings = Settings::default();
        settings.profiles.push(Profile::new("N"));
        nightlight::set(true).unwrap();
        run(
            &mut card,
            &mut settings,
            dev,
            Command::SaveScene(0, vec![SceneRow::Bright, SceneRow::Night, SceneRow::Quiet]),
        );
        let s = &settings.profiles[0].system;
        assert_eq!((s.bright, s.night, s.quiet), (Some(40), Some(true), None), "чужие строки — не здесь");
        // Не просили — не трогаем.
        run(&mut card, &mut settings, dev, Command::SaveScene(0, vec![SceneRow::Night]));
        assert_eq!(settings.profiles[0].system.bright, Some(40));
    }

    /// Монитор после смены режима молчит секунду-другую: яркость сцены ждёт его повторами, а не пропускает.
    #[test]
    fn a_monitor_that_is_silent_for_a_moment_is_waited_for() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-SILENT";
        let t = texts::tr(Lang::En);
        let mut card = card(dev);
        let mut settings = Settings::default();
        settings.profiles.push(scene("Dim", SceneSystem { bright: Some(30), ..Default::default() }));
        let dim = settings.profiles[0].clone();
        // На три ближайших обращения (читать или ставить) монитор не ответит.
        ddc::fake_silence(dev, 3);
        let toasts = run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        assert!(toasts.is_empty(), "{toasts:?}");
        wait("яркость поставлена после повторов", || ddc::fake_peek(dev) == 30);
        assert!(card.problems(t).is_empty(), "справились — сказать нечего");
        assert!(card.scene_can_undo(&dim));
        // Возврат сразу после возврата режима: монитор снова молчит — повторы и тут.
        ddc::fake_silence(dev, 3);
        run(&mut card, &mut settings, dev, Command::UndoScene(0));
        wait("яркость вернулась после повторов", || ddc::fake_peek(dev) == 70);
        assert!(card.problems(t).is_empty());
    }

    /// Монитор так и не ответил — модуль говорит это своими словами (предупреждением), яркость не тронута; ответит позже —
    /// следующая постановка работает.
    #[test]
    fn a_monitor_that_stays_silent_gets_a_warning() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-MUTE";
        let t = texts::tr(Lang::En);
        let mut card = card(dev);
        let mut settings = Settings::default();
        settings.profiles.push(scene("Dim", SceneSystem { bright: Some(30), ..Default::default() }));
        ddc::fake_silence(dev, u32::MAX);
        run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        let mut told = Vec::new();
        wait("предупреждение", || {
            told.extend(card.problems(t));
            !told.is_empty()
        });
        assert_eq!(told, vec![(t.scene_bright_failed.to_string(), Tone::Warning)]);
        assert_eq!(ddc::fake_peek(dev), 70, "яркость не тронута");
        assert!(card.problems(t).is_empty(), "второй раз то же не повторяется");
        // Отвечает снова.
        ddc::fake_silence(dev, 0);
        run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        wait("яркость поставлена", || ddc::fake_peek(dev) == 30);
        assert!(card.problems(t).is_empty());
    }

    /// Ждут и предупреждают о мониторе, что уже отвечал яркостью; тот, что по DDC/CI не отвечал ни разу, пропускается
    /// молча, как и раньше, — иначе второй монитор без DDC/CI портил бы каждую сцену.
    #[test]
    fn a_monitor_that_never_answered_is_skipped_without_a_word() {
        let _world = world();
        let (a, b) = (r"\\.\TEST-BRIGHT-ANSWERS", r"\\.\TEST-BRIGHT-NEVER");
        let monitors = vec![(a.to_string(), ddc::fake_handle(a)), (b.to_string(), ddc::fake_handle(b))];
        ddc::fake_silence(b, u32::MAX);
        let shared = Arc::new(Mutex::new(Shared::default()));
        shared.lock().unwrap().answered.push(a.to_string());
        let started = Instant::now();
        scene_bright(&shared, &monitors, 30, false);
        assert!(started.elapsed() < SILENT_FOR, "молчуна, что не отвечал, не ждали");
        assert_eq!((ddc::fake_peek(a), ddc::fake_peek(b)), (30, 70));
        assert!(shared.lock().unwrap().problems.is_empty());
        // Пока не отвечал никто (программа только запущена), ждут всех: молчит тот, у кого есть что спросить.
        let cold = Arc::new(Mutex::new(Shared::default()));
        scene_bright(&cold, &monitors[1..], 30, false);
        assert_eq!(cold.lock().unwrap().problems, vec![Problem::SceneBright]);
        // А того, что отвечал, возврат ждёт с повторами и предупреждает, если не дождался.
        let back = Arc::new(Mutex::new(Shared::default()));
        back.lock().unwrap().bright_before.push((b.to_string(), 0, ddc::fake_handle(b), 70));
        assert_eq!(scene_bright_undo(&back).len(), 1);
        assert_eq!(back.lock().unwrap().problems, vec![Problem::SceneBright]);
    }

    /// Номер сцены, а не имя: переименование возврат не теряет, новая сцена со старым именем чужого не наследует.
    #[test]
    fn the_return_belongs_to_the_scene_not_to_its_name() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-IDENTITY";
        let mut card = card(dev);
        let mut settings = Settings::default();
        settings.profiles.push(scene("Dim", SceneSystem { bright: Some(30), ..Default::default() }));
        run(&mut card, &mut settings, dev, Command::ApplyScene(0));
        wait("яркость поставлена", || ddc::fake_peek(dev) == 30);
        settings.profiles[0].name = "Dusk".into();
        assert!(card.scene_can_undo(&settings.profiles[0]), "переименование возврат не теряет");
        assert!(!card.scene_can_undo(&scene("Dim", SceneSystem { bright: Some(30), ..Default::default() })));
    }

    #[test]
    fn rows_wait_for_the_first_reading() {
        let _world = world();
        let dev = r"\\.\TEST-BRIGHT-ROWS";
        let mut card = Brightness::default();
        assert_eq!(card.scene_rows(), vec![(SceneRow::Bright, false), (SceneRow::Night, false)]);
        card.refresh(dev);
        wait("прочитано", || card.scene_rows() == vec![(SceneRow::Bright, true), (SceneRow::Night, true)]);
    }
}
