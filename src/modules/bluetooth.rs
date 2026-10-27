// Bluetooth: сопряжённые устройства — подключено ли, заряд. Перечисление идёт через SetupAPI и может
// занять заметное время — в своём потоке, окно читает готовое. Подключать — в «Параметрах Windows».
// Плашки у края (2.4): аудиоустройство подключилось («Наушники: подключено», заряд) и любое устройство разряжается (15 % и
// ниже, раз за разряд: «Мышь: разряжается»). О мыши и клавиатуре «подключено» не говорят — они засыпают и просыпаются сами, и
// каждое пробуждение было бы плашкой. Устройство считается отключённым, когда оно не подключено в двух чтениях подряд: после
// сна Windows отвечает неполным списком, и одно такое чтение не должно давать ложных «подключено».
// Для этого, пока плашки Bluetooth включены, устройства читаются и при спрятанной панели — раз в несколько секунд; первое
// чтение (после запуска и после включения плашек) ничего не объявляет: плашка только о новом.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Ui};

use super::{Action, Command, Ctx, Module, mine};
use crate::config::Settings;
use crate::plaque::{self, Glyph, Plaque, Say};
use crate::sys::{self, bt};
use crate::texts::Tr;

/// Как часто читать устройства ради плашек, пока панель спрятана.
const WATCH_EVERY: Duration = Duration::from_secs(5);
/// Заряд, с которого устройство «разряжается» (включительно), и заряд, выше которого оно снова «в порядке»: пока не поднялось
/// выше второго, о разряде не повторяют.
pub const LOW: u8 = 15;
pub const RECOVERED: u8 = 20;
/// Сколько чтений подряд подключённое устройство должно быть не подключено, чтобы считаться отключённым.
const OFF_READS: u8 = 2;

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
}

impl Shared {
    /// Новое чтение: сверить с прошлым, а список как есть — в карточку.
    fn apply(&mut self, list: Vec<bt::Device>) {
        let believed = settle(self.prev.as_deref(), list.clone(), &mut self.off);
        let found = changes(self.prev.as_deref(), &believed, &mut self.warned);
        self.pending.extend(found);
        self.prev = Some(believed);
        self.list = list;
        self.loaded = true;
    }
}

#[derive(Default)]
pub struct Bluetooth {
    shared: Arc<Mutex<Shared>>,
    /// Когда начали последнее чтение.
    read_at: Option<Instant>,
    /// Плашки Bluetooth включены, и мы следим: включили заново — прежнему чтению сверять уже не с чем.
    watching: bool,
}

impl Bluetooth {
    /// Прочитать устройства в потоке (не двигая окно): список для карточки и сверка для плашек.
    fn read(&mut self) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.busy {
                return;
            }
            s.busy = true;
        }
        self.read_at = Some(Instant::now());
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let list = bt::devices();
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.apply(list);
            s.busy = false;
        });
    }
}

impl Module for Bluetooth {
    fn key(&self) -> &'static str {
        "bluetooth"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.bluetooth
    }

    fn refresh(&mut self, _monitor: &str) {
        self.read();
    }

    /// Слежка для плашек: пока они включены, устройства читаются и при спрятанной панели.
    fn background(&mut self, settings: &Settings) {
        let on = settings.plaques.on(plaque::Kind::Bluetooth);
        if on != self.watching {
            self.watching = on;
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.prev = None;
            shared.off.clear();
        }
        if on && self.read_at.is_none_or(|at| at.elapsed() >= WATCH_EVERY) {
            self.read();
        }
    }

    fn events(&mut self, settings: &Settings) -> Vec<Plaque> {
        let pending = std::mem::take(&mut self.shared.lock().unwrap_or_else(|e| e.into_inner()).pending);
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

    /// «Параметры Bluetooth» — та же кнопка, что внизу карточки.
    fn actions(&self, t: &Tr, _lang: anvil_ui::Lang, _settings: &Settings) -> Vec<Action> {
        let action = Action::new("bluetooth:settings", t.bt_settings, t.bluetooth, Icon::Gear);
        vec![action.words("блютуз устройства наушники сопряжение bluetooth devices pair headphones")]
    }

    fn command(&mut self, cmd: &Command, _cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        if mine(self.key(), id) == Some("settings")
            && !sys::demo()
            && let Err(e) = sys::launch("ms-settings:bluetooth", "")
        {
            eprintln!("open bluetooth settings failed: {e}");
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        let (list, loaded) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.list.clone(), s.loaded)
        };
        if !loaded {
            w::spinner(ui, 16.0);
            return;
        }
        let settings_h = 34.0;
        if list.is_empty() {
            w::note(ui, t.bt_none);
        } else {
            egui::ScrollArea::vertical()
                .id_salt("bt")
                .auto_shrink([false, false])
                .max_height(ui.available_height() - settings_h)
                .show(ui, |ui| {
                    for d in &list {
                        ui.horizontal(|ui| {
                            w::dot(ui, if d.connected { Tone::Success } else { Tone::Neutral });
                            let color = if d.connected { p.text } else { p.weak };
                            ui.label(egui::RichText::new(&d.name).color(color));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if let Some(b) = d.battery {
                                    let tone = if b <= 20 { Tone::Warning } else { Tone::Neutral };
                                    w::badge(ui, &format!("{b}%"), tone);
                                }
                            });
                        });
                        ui.add_space(2.0);
                    }
                });
        }
        if w::button(ui, Kind::Ghost, Some(Icon::Gear), t.bt_settings).clicked()
            && !sys::demo()
            && let Err(e) = sys::launch("ms-settings:bluetooth", "")
        {
            eprintln!("open bluetooth settings failed: {e}");
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
        let start = Instant::now();
        while !module.shared.lock().unwrap().loaded {
            assert!(start.elapsed() < Duration::from_secs(3), "поток чтения не ответил");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(module.events(&Settings::default()).is_empty());
        let list = module.shared.lock().unwrap().list.clone();
        assert_eq!(list.len(), 3, "выдуманные «Headphones», «Mouse», «Keyboard»");
    }
}
