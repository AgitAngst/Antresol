// Быстрые настройки: четыре плитки «включить / выключить» системного (как быстрые настройки Windows) —
// «Не спать», «Тёмная тема», «Тишина», «Wi‑Fi», — план питания и VPN Windows. Плитка — состояние, прочитанное
// при показе панели, и щелчок — поменять. Не узнали состояние — плитка «Недоступно» и не нажимается. Wi‑Fi и VPN
// отвечают медленно — их читает и меняет поток модуля. Ночной свет — выключателем в карточке «Яркость».
// Сцены ставят отсюда «Тишину», «Не спать», тёмную тему, Wi‑Fi и план питания: перед сменой модуль помнит, что было
// (`before`), и второй щелчок по стоящей сцене возвращает это; Wi‑Fi помнит и возвращает поток модуля.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::ShowNoted;
use super::{Action, Command, Ctx, Module, chips, mine};
use crate::config::{Profile, SceneRow, Settings};
use crate::sys::net::{self, Vpn, Wifi};
use crate::sys::system;
use crate::texts::{self, Tr};

enum Job {
    Read,
    Wifi(bool),
    Vpn(String, bool),
    /// Сцена: поставить Wi‑Fi, запомнив прежнее; `keep` — прежнее уже помним с первого раза этой же сцены.
    SceneWifi {
        on: bool,
        keep: bool,
    },
    /// Вернуть Wi‑Fi, как был до сцены.
    SceneWifiUndo,
}

/// Что не вышло в потоке модуля: панель скажет об этом своими словами (`Module::problems`), а английский текст ошибки
/// остаётся в журнале.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Failure {
    Wifi,
    /// Подключение VPN с этим именем.
    Vpn(String),
}

#[derive(Default)]
struct Shared {
    /// `None` — ещё не прочитано; `Some(None)` — радио Wi‑Fi нет.
    wifi: Option<Option<Wifi>>,
    vpns: Vec<Vpn>,
    /// Что не вышло с прошлого раза — панель скажет уведомлением, даже если карточку не рисуют.
    failed: Vec<Failure>,
    /// Был ли включён Wi‑Fi до сцены: запоминает поток, когда ставит его по сцене.
    wifi_before: Option<bool>,
}

/// Как было до сцены: что стояло в тех настройках, которые она поменяла (`None` — не меняла).
struct SceneBefore {
    /// Номер сцены ([`Profile::id`]): переименование возврат не теряет, новая сцена с тем же именем чужой не наследует.
    scene: u64,
    awake: Option<bool>,
    /// Обе темы Windows — программ и панели задач — как были, каждая своя.
    theme: Option<system::Theme>,
    quiet: Option<bool>,
    plan: Option<windows::core::GUID>,
    /// Wi‑Fi сцена трогала: прежнее лежит у потока (`Shared::wifi_before`).
    wifi: bool,
}

impl SceneBefore {
    fn new(scene: u64) -> SceneBefore {
        SceneBefore { scene, awake: None, theme: None, quiet: None, plan: None, wifi: false }
    }

    fn is_empty(&self) -> bool {
        self.awake.is_none() && self.theme.is_none() && self.quiet.is_none() && self.plan.is_none() && !self.wifi
    }
}

pub struct Quick {
    /// «Не спать» — только пока работает программа.
    awake: bool,
    /// Тёмная ли тема программ (плитка).
    dark: Option<bool>,
    /// Обе темы Windows, как прочитаны: сцена сверяет и ставит обе.
    theme: Option<system::Theme>,
    quiet: Option<bool>,
    plans: Vec<system::Plan>,
    plan: Option<windows::core::GUID>,
    /// Как было до сцены, что стоит сейчас, — на второй щелчок по ней.
    before: Option<SceneBefore>,
    jobs: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
}

impl Default for Quick {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let s = shared.clone();
        std::thread::spawn(move || worker(rx, s));
        Quick {
            awake: false,
            dark: None,
            theme: None,
            quiet: None,
            plans: Vec::new(),
            plan: None,
            before: None,
            jobs: tx,
            shared,
        }
    }
}

fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
    crate::sys::media::init_thread();
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
        }
        let mut failed: Vec<Failure> = Vec::new();
        for j in jobs {
            let what = match &j {
                Job::Vpn(name, _) => Failure::Vpn(name.clone()),
                _ => Failure::Wifi,
            };
            let r = match j {
                Job::Read => Ok(()),
                Job::Wifi(on) => net::set_wifi(on),
                Job::Vpn(name, on) => net::set_vpn(&name, on),
                Job::SceneWifi { on, keep } => match net::wifi() {
                    Some(now) => {
                        {
                            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                            if !(keep && s.wifi_before.is_some()) {
                                s.wifi_before = Some(now.on);
                            }
                        }
                        if now.on != on { net::set_wifi(on) } else { Ok(()) }
                    }
                    // Радио Wi‑Fi нет — сцене нечего ставить.
                    None => Ok(()),
                },
                Job::SceneWifiUndo => {
                    let before = shared.lock().unwrap_or_else(|e| e.into_inner()).wifi_before.take();
                    match (before, net::wifi()) {
                        (Some(want), Some(now)) if now.on != want => net::set_wifi(want),
                        _ => Ok(()),
                    }
                }
            };
            if let Err(e) = r {
                eprintln!("quick settings failed: {e}");
                if !failed.contains(&what) {
                    failed.push(what);
                }
            }
        }
        let (wifi, vpns) = (net::wifi(), net::vpns());
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.wifi = Some(wifi);
        s.vpns = vpns;
        for what in failed {
            if !s.failed.contains(&what) {
                s.failed.push(what);
            }
        }
    }
}

/// Плитка 2 строки: значок и состояние сверху, ниже название. Включена — подложка акцента.
/// `state` — `None`: недоступно (не нажимается). `on_text` — что написать вместо «Вкл», пока плитка включена
/// (имя сети, «без всплывающих»); длинное обрезается многоточием, целиком — в подсказке плитки. Ответ — щелчок.
pub fn tile(ui: &mut Ui, icon: Icon, label: &str, state: Option<bool>, on_text: Option<&str>, t: &Tr) -> bool {
    let p = Palette::of(ui);
    let size = Vec2::new(ui.available_width(), 48.0);
    let sense = if state.is_some() { Sense::click() } else { Sense::hover() };
    let (rect, r) = ui.allocate_exact_size(size, sense);
    let on = state == Some(true);
    let hovered = r.hovered() && state.is_some();
    let fill = if on {
        p.badge_fill(p.accent)
    } else if hovered {
        p.hover
    } else {
        p.raised
    };
    let stroke = if on { p.accent } else { p.border };
    ui.painter().rect(rect, 8, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let (fg, sub) = match state {
        Some(true) => (p.accent_text, on_text.unwrap_or(t.on)),
        Some(false) => (p.text, t.off),
        None => (p.faint, t.unavailable),
    };
    // Сверху значок и состояние, снизу название во всю ширину.
    let ic = Rect::from_min_size(rect.min + Vec2::new(10.0, 7.0), Vec2::splat(16.0));
    anvil_ui::icons::paint(ui.painter(), ic, icon, fg);
    let sub_color = if on { p.accent_text } else { p.weak };
    let mut sub_job =
        egui::text::LayoutJob::simple_singleline(sub.to_owned(), egui::FontId::proportional(12.0), sub_color);
    sub_job.wrap = egui::text::TextWrapping::truncate_at_width((rect.width() - 10.0 - 16.0 - 8.0 - 10.0).max(20.0));
    let sub_galley = ui.painter().layout_job(sub_job);
    let sub_at = Pos2::new(rect.right() - 10.0 - sub_galley.size().x, ic.center().y - sub_galley.size().y / 2.0);
    ui.painter().galley(sub_at, sub_galley, sub_color);
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), anvil_ui::semibold(13.0), fg);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 20.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(Pos2::new(rect.left() + 10.0, rect.bottom() - 7.0 - galley.size().y), galley, fg);
    w::focus_ring(ui, rect, &r, 8);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, state.is_some(), on, label));
    let mut r = if state.is_some() { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r };
    if on && on_text.is_some() {
        r = r.on_hover_text(sub);
    }
    r.clicked()
}

impl Module for Quick {
    fn key(&self) -> &'static str {
        "quick"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.quick
    }

    fn min_width(&self) -> f32 {
        240.0
    }

    fn refresh(&mut self, _monitor: &str) {
        self.theme = system::theme();
        self.dark = self.theme.map(|t| t.apps_dark);
        self.quiet = system::notifications_off();
        (self.plans, self.plan) = system::plans();
        let _ = self.jobs.send(Job::Read);
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
            Command::RunAction(id) => {
                if let Some(what) = mine(self.key(), id) {
                    self.run_action(what, cx);
                }
            }
            Command::Dropped(_) | Command::Media(_) => {}
        }
    }

    /// Плитки («Не спать», «Тёмная тема», «Тишина», «Wi‑Fi») — включить или выключить по тому, что стоит; планы питания,
    /// кроме стоящего; VPN — подключить или отключить (нет подключений в Windows — нет и действий). Плитка без значения
    /// (не прочиталось, нет радио Wi‑Fi) — серая, с причиной.
    fn actions(&self, t: &Tr, lang: anvil_ui::Lang, _settings: &Settings) -> Vec<Action> {
        let (wifi, vpns) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.wifi.clone(), s.vpns.clone())
        };
        let verb = |on: bool| if on { t.turn_off } else { t.turn_on };
        let tile = |id: &str, label: &str, on: Option<bool>, icon: Icon, words: &str, why: &str| {
            let title = match on {
                Some(on) => format!("{label}: {}", verb(on)),
                None => label.to_string(),
            };
            Action::new(id, title, t.quick, icon).words(words).off_if(on.is_none(), why)
        };
        let wifi_why = match &wifi {
            Some(None) => t.scene_why_wifi,
            _ => t.unavailable,
        };
        let mut out = vec![
            tile(
                "quick:awake",
                t.keep_awake,
                Some(self.awake),
                Icon::Sun,
                "сон экран не гасить sleep awake caffeine",
                "",
            ),
            tile(
                "quick:dark",
                t.dark_theme_win,
                self.dark,
                Icon::Image,
                "тема тёмная светлая dark light theme",
                t.scene_why_read,
            ),
            tile(
                "quick:quiet",
                t.quiet,
                self.quiet,
                Icon::Pause,
                "уведомления не беспокоить notifications silent",
                t.scene_why_read,
            ),
            tile(
                "quick:wifi",
                t.wifi,
                wifi.clone().flatten().map(|w| w.on),
                Icon::Broadcast,
                "вайфай сеть интернет wifi network",
                wifi_why,
            ),
        ];
        for plan in self.plans.iter().filter(|p| Some(p.guid) != self.plan) {
            let id = format!("quick:plan:{}", system::guid_string(&plan.guid));
            let title = format!("{}: {}", t.power_plan, plan.name);
            out.push(
                Action::new(id, title, t.quick, Icon::Gear).words("питание электропитание power plan performance"),
            );
        }
        for v in &vpns {
            let verb = if v.connected { t.disconnect } else { t.connect };
            let title = format!("VPN {}: {}", texts::scene_quoted(lang, &v.name), verb.to_lowercase());
            out.push(
                Action::new(format!("quick:vpn:{}", v.name), title, t.quick, Icon::Lock)
                    .words("впн туннель vpn tunnel"),
            );
        }
        out
    }

    /// Совпадает ли то, что сцена отметила здесь, с тем, что стоит: «Не спать» — своё состояние, остальное — прочитанное при
    /// показе панели; не прочиталось — не мешает. Ничего не отмечено или не прочитать — `None`.
    fn scene_part(&self, scene: &Profile) -> Option<bool> {
        let s = &scene.system;
        let mut same: Vec<bool> = Vec::new();
        if let Some(want) = s.awake {
            same.push(self.awake == want);
        }
        // Тема стоит, когда обе — программ и панели задач — такие, как в сцене.
        if let (Some(want), Some(now)) = (s.dark, self.theme) {
            same.push(now.is(want));
        }
        if let (Some(want), Some(now)) = (s.quiet, self.quiet) {
            same.push(want == now);
        }
        if let (Some(want), Some(now)) = (&s.plan, self.plan) {
            same.push(*want == system::guid_string(&now));
        }
        if let Some(want) = s.wifi
            && let Some(Some(now)) = self.shared.lock().unwrap_or_else(|e| e.into_inner()).wifi.clone()
        {
            same.push(now.on == want);
        }
        (!same.is_empty()).then(|| same.into_iter().all(|ok| ok))
    }

    fn scene_can_undo(&self, scene: &Profile) -> bool {
        self.before.as_ref().is_some_and(|b| b.scene == scene.id)
    }

    /// Строки быстрых настроек: «Не спать» всегда можно; остальное — если прочиталось (Wi‑Fi — есть радио).
    fn scene_rows(&self) -> Vec<(SceneRow, bool)> {
        let wifi = self.shared.lock().unwrap_or_else(|e| e.into_inner()).wifi.clone().flatten().is_some();
        vec![
            (SceneRow::Quiet, self.quiet.is_some()),
            (SceneRow::Awake, true),
            (SceneRow::Dark, self.dark.is_some()),
            (SceneRow::Wifi, wifi),
            (SceneRow::Plan, !self.plans.is_empty()),
        ]
    }

    /// Wi‑Fi или VPN не переключились в потоке модуля: «Не получилось: Wi‑Fi» — словами программы, английский текст ошибки
    /// остался в журнале. Панель берёт это сама каждый кадр, а не при рисовании карточки.
    fn problems(&mut self, t: &Tr) -> Vec<(String, Tone)> {
        let failed = std::mem::take(&mut self.shared.lock().unwrap_or_else(|e| e.into_inner()).failed);
        failed
            .into_iter()
            .map(|what| {
                let name = match what {
                    Failure::Wifi => t.wifi.to_string(),
                    Failure::Vpn(name) => format!("VPN {name}"),
                };
                (format!("{}: {name}", t.failed), Tone::Danger)
            })
            .collect()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let (wifi, vpns) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.wifi.clone(), s.vpns.clone())
        };
        let wifi_state = wifi.clone().flatten();
        // Название плитки одно — «Wi‑Fi»; сеть, через которую выход в интернет, — в строке состояния.
        let network = wifi_state.as_ref().filter(|wf| wf.on && !wf.network.is_empty()).map(|wf| wf.network.as_str());
        egui::ScrollArea::vertical().id_salt("quick").auto_shrink([false, false]).show_noted(ui, |ui| {
            // Четыре плитки по две в ряд.
            let tiles: [(Icon, &str, Option<bool>, Option<&str>); 4] = [
                (Icon::Sun, t.keep_awake, Some(self.awake), None),
                (Icon::Image, t.dark_theme_win, self.dark, None),
                (Icon::Pause, t.quiet, self.quiet, Some(t.quiet_on)),
                (Icon::Broadcast, "Wi‑Fi", wifi_state.as_ref().map(|wf| wf.on), network),
            ];
            let gap = 6.0;
            let half = ((ui.available_width() - gap) / 2.0).floor();
            let mut clicked = None;
            for (r, row) in tiles.chunks(2).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (c, (icon, label, state, on_text)) in row.iter().enumerate() {
                        ui.allocate_ui(Vec2::new(half, 48.0), |ui| {
                            if tile(ui, *icon, label, *state, *on_text, t) {
                                clicked = Some(r * 2 + c);
                            }
                        });
                    }
                });
            }
            if let Some(i) = clicked {
                self.toggle(i, wifi_state.as_ref(), cx);
            }

            if !self.plans.is_empty() {
                ui.add_space(8.0);
                w::section_label(ui, t.power_plan);
                let opts: Vec<(Option<windows::core::GUID>, &str)> =
                    self.plans.iter().map(|p| (Some(p.guid), p.name.as_str())).collect();
                let mut want = self.plan;
                if chips(ui, &mut want, &opts)
                    && let Some(g) = want
                {
                    self.set_plan(g, cx);
                }
            }

            if !vpns.is_empty() {
                ui.add_space(8.0);
                w::section_label(ui, "VPN");
                for v in &vpns {
                    ui.horizontal(|ui| {
                        w::dot(ui, if v.connected { Tone::Success } else { Tone::Neutral });
                        let p = Palette::of(ui);
                        ui.label(egui::RichText::new(&v.name).color(if v.connected { p.text } else { p.weak }));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = if v.connected { t.disconnect } else { t.connect };
                            if w::button(ui, Kind::Ghost, None, label).clicked() {
                                let _ = self.jobs.send(Job::Vpn(v.name.clone(), !v.connected));
                            }
                        });
                    });
                }
            }
        });
    }
}

/// Сбой части быстрых настроек: английский текст ошибки — в журнал, пользователю — «Не получилось: Тёмная тема».
fn part_failed(cx: &mut Ctx, row: SceneRow, e: &str) {
    eprintln!("quick settings: {row:?} failed: {e}");
    cx.toasts.push((texts::scene_failed(cx.t, row), Tone::Danger));
}

impl Quick {
    /// План питания: чип в карточке и действие поиска.
    fn set_plan(&mut self, guid: windows::core::GUID, cx: &mut Ctx) {
        match system::set_plan(&guid) {
            Ok(()) => self.plan = Some(guid),
            Err(e) => part_failed(cx, SceneRow::Plan, &e),
        }
    }

    /// Действие поиска «Быстрых настроек»: то же, что щелчок по плитке, чипу плана или кнопке VPN.
    fn run_action(&mut self, what: &str, cx: &mut Ctx) {
        let (wifi, vpns) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.wifi.clone().flatten(), s.vpns.clone())
        };
        match what {
            "awake" => self.toggle(0, wifi.as_ref(), cx),
            "dark" => self.toggle(1, wifi.as_ref(), cx),
            "quiet" => self.toggle(2, wifi.as_ref(), cx),
            "wifi" => self.toggle(3, wifi.as_ref(), cx),
            other => {
                if let Some(guid) = other.strip_prefix("plan:").and_then(system::guid_from)
                    && self.plans.iter().any(|p| p.guid == guid)
                {
                    self.set_plan(guid, cx);
                } else if let Some(name) = other.strip_prefix("vpn:")
                    && let Some(v) = vpns.iter().find(|v| v.name == name)
                {
                    let _ = self.jobs.send(Job::Vpn(v.name.clone(), !v.connected));
                }
            }
        }
    }

    /// Поставить то, что сцена отметила у быстрых настроек. Перед сменой — запомнить, что было. Возврат прежней сцены
    /// пропадает; та же сцена ещё раз продолжает прежний «сеанс»: что уже помним, то и остаётся.
    fn apply_scene(&mut self, scene: &Profile, cx: &mut Ctx) {
        let s = &scene.system;
        let mut before = match self.before.take() {
            Some(b) if b.scene == scene.id => b,
            _ => SceneBefore::new(scene.id),
        };
        // Тема стоит, когда обе — программ и панели задач — такие, как в сцене; ставятся обе, а в `before` лежат обе как были.
        if let Some(want) = s.dark
            && let Some(now) = system::theme()
        {
            before.theme = before.theme.or(Some(now));
            if !now.is(want) {
                match system::set_theme(system::Theme::both(want)) {
                    Ok(()) => {
                        self.theme = Some(system::Theme::both(want));
                        self.dark = Some(want);
                    }
                    Err(e) => part_failed(cx, SceneRow::Dark, &e),
                }
            }
        }
        if let Some(want) = s.quiet
            && let Some(now) = system::notifications_off()
        {
            before.quiet = before.quiet.or(Some(now));
            if now != want {
                match system::set_notifications_off(want) {
                    Ok(()) => self.quiet = Some(want),
                    Err(e) => part_failed(cx, SceneRow::Quiet, &e),
                }
            }
        }
        if let Some(want) = s.awake {
            before.awake = before.awake.or(Some(self.awake));
            if want != self.awake {
                if system::keep_awake(want) {
                    self.awake = want;
                } else {
                    part_failed(cx, SceneRow::Awake, "SetThreadExecutionState failed");
                }
            }
        }
        if let Some(want) = s.plan.as_deref().and_then(system::guid_from) {
            let (plans, active) = system::plans();
            if plans.iter().any(|p| p.guid == want) {
                before.plan = before.plan.or(active);
                if active != Some(want) {
                    match system::set_plan(&want) {
                        Ok(()) => self.plan = Some(want),
                        Err(e) => part_failed(cx, SceneRow::Plan, &e),
                    }
                }
            } else {
                cx.toasts.push((cx.t.scene_plan_missing.to_string(), Tone::Warning));
            }
        }
        if let Some(want) = s.wifi {
            let _ = self.jobs.send(Job::SceneWifi { on: want, keep: before.wifi });
            before.wifi = true;
        }
        self.before = (!before.is_empty()).then_some(before);
    }

    /// Вернуть то, что сцена поменяла, как было до неё. Одноразово.
    fn undo_scene(&mut self, scene: &Profile, cx: &mut Ctx) {
        if !self.scene_can_undo(scene) {
            return;
        }
        let Some(before) = self.before.take() else { return };
        // Обе темы — каждая как была: «смешанный» режим (программы светлые, панель задач тёмная) возвращается таким же.
        if let Some(theme) = before.theme
            && system::theme().is_some_and(|now| now != theme)
        {
            match system::set_theme(theme) {
                Ok(()) => {
                    self.theme = Some(theme);
                    self.dark = Some(theme.apps_dark);
                }
                Err(e) => part_failed(cx, SceneRow::Dark, &e),
            }
        }
        if let Some(quiet) = before.quiet
            && system::notifications_off().is_some_and(|now| now != quiet)
        {
            match system::set_notifications_off(quiet) {
                Ok(()) => self.quiet = Some(quiet),
                Err(e) => part_failed(cx, SceneRow::Quiet, &e),
            }
        }
        if let Some(awake) = before.awake
            && awake != self.awake
        {
            if system::keep_awake(awake) {
                self.awake = awake;
            } else {
                part_failed(cx, SceneRow::Awake, "SetThreadExecutionState failed");
            }
        }
        if let Some(plan) = before.plan
            && system::plans().1 != Some(plan)
        {
            match system::set_plan(&plan) {
                Ok(()) => self.plan = Some(plan),
                Err(e) => part_failed(cx, SceneRow::Plan, &e),
            }
        }
        if before.wifi {
            let _ = self.jobs.send(Job::SceneWifiUndo);
        }
    }

    /// Взять текущие значения этих строк в сцену: что можно прочитать прямо сейчас; что не прочиталось — как было.
    fn save_scene(&mut self, scene: &mut Profile, rows: &[SceneRow]) {
        let s = &mut scene.system;
        for row in rows {
            match row {
                SceneRow::Quiet => s.quiet = system::notifications_off().or(s.quiet),
                SceneRow::Awake => s.awake = Some(self.awake),
                SceneRow::Dark => s.dark = system::dark_theme().or(s.dark),
                SceneRow::Plan => s.plan = system::plans().1.map(|g| system::guid_string(&g)).or(s.plan.take()),
                SceneRow::Wifi => {
                    let now = self.shared.lock().unwrap_or_else(|e| e.into_inner()).wifi.clone().flatten();
                    s.wifi = now.map(|w| w.on).or(s.wifi);
                }
                SceneRow::Screen | SceneRow::Bright | SceneRow::Night => {}
            }
        }
    }

    /// Щелчок по плитке с номером `i` (порядок — как в `ui`).
    fn toggle(&mut self, i: usize, wifi: Option<&Wifi>, cx: &mut Ctx) {
        // Что нажали (для слов об ошибке) и чем кончилось; нет значения у плитки — ничего.
        let result = match i {
            0 => {
                let on = !self.awake;
                Some((
                    SceneRow::Awake,
                    if system::keep_awake(on) {
                        self.awake = on;
                        Ok(())
                    } else {
                        Err("SetThreadExecutionState failed".to_string())
                    },
                ))
            }
            1 => self.dark.map(|on| {
                let ok = system::set_dark_theme(!on).map(|()| {
                    self.dark = Some(!on);
                    self.theme = Some(system::Theme::both(!on));
                });
                (SceneRow::Dark, ok)
            }),
            2 => self
                .quiet
                .map(|on| (SceneRow::Quiet, system::set_notifications_off(!on).map(|()| self.quiet = Some(!on)))),
            3 => wifi.map(|wf| {
                let _ = self.jobs.send(Job::Wifi(!wf.on));
                (SceneRow::Wifi, Ok(()))
            }),
            _ => None,
        };
        if let Some((row, Err(e))) = result {
            part_failed(cx, row, &e);
        }
    }
}

// ─── Сцены без окна ─────────────────────────────────────────────────────────
//
// Выдуманная система (`sys::fake_reads`): тема, уведомления, план питания и Wi‑Fi помнят, что в них записали. Мир один на
// все тесты — каждый берёт `WORLD_TEST` и начинает с исходного.

#[cfg(test)]
mod tests {
    use std::sync::MutexGuard;
    use std::time::{Duration, Instant};

    use anvil_ui::Lang;
    use windows::core::GUID;

    use super::*;
    use crate::config::{SceneSystem, Settings};
    use crate::{sys, texts};

    /// Мир под замком, как вначале.
    fn world() -> MutexGuard<'static, ()> {
        let guard = sys::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        sys::reset_fake_world();
        guard
    }

    fn scene(name: &str, system: SceneSystem) -> Profile {
        Profile { system, ..Profile::new(name) }
    }

    /// Планы выдуманного мира: «Balanced» (1) стоит вначале, «High performance» (2), «Power saver» (3).
    fn plan(n: u128) -> String {
        system::guid_string(&GUID::from_u128(n))
    }

    /// Команда модулю; ответ — что он сказал уведомлениями.
    fn run(q: &mut Quick, settings: &mut Settings, cmd: Command) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        let mut cx =
            Ctx { t: texts::tr(Lang::En), lang: Lang::En, monitor: r"\\.\TEST-QUICK", toasts: &mut toasts, settings };
        q.command(&cmd, &mut cx);
        toasts
    }

    /// Ждёт, пока поток модуля сделает своё (Wi‑Fi идёт через него): условие или 3 секунды.
    fn wait(what: &str, mut ok: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ok() {
            assert!(start.elapsed() < Duration::from_secs(3), "не дождались: {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn scene_sets_what_it_checks_and_the_second_click_returns_it() {
        let _world = world();
        let mut q = Quick::default();
        q.refresh("");
        let mut settings = Settings::default();
        let checked = SceneSystem {
            dark: Some(false),
            quiet: Some(true),
            awake: Some(true),
            plan: Some(plan(2)),
            ..Default::default()
        };
        settings.profiles.push(scene("Movie", checked));
        let movie = settings.profiles[0].clone();
        // Вначале: тема тёмная, уведомления включены, план «Balanced», «Не спать» выключено.
        assert_eq!(q.scene_part(&movie), Some(false));
        assert!(!q.scene_can_undo(&movie));
        let toasts = run(&mut q, &mut settings, Command::ApplyScene(0));
        assert!(toasts.is_empty(), "{toasts:?}");
        assert_eq!((system::dark_theme(), system::notifications_off()), (Some(false), Some(true)));
        assert_eq!(system::plans().1, Some(GUID::from_u128(2)));
        assert!(q.awake);
        assert_eq!(q.scene_part(&movie), Some(true), "сцена стоит");
        assert!(q.scene_can_undo(&movie));
        // Не отмеченное не тронуто: Wi‑Fi как был.
        assert_eq!(net::wifi().map(|w| w.on), Some(true));
        // Второй щелчок: всё как было.
        let toasts = run(&mut q, &mut settings, Command::UndoScene(0));
        assert!(toasts.is_empty(), "{toasts:?}");
        assert_eq!((system::dark_theme(), system::notifications_off()), (Some(true), Some(false)));
        assert_eq!(system::plans().1, Some(GUID::from_u128(1)));
        assert!(!q.awake);
        assert_eq!(q.scene_part(&movie), Some(false));
        // Возврат одноразовый: поменяли руками — второй «возврат» этого не трогает.
        assert!(!q.scene_can_undo(&movie));
        system::set_notifications_off(true).unwrap();
        run(&mut q, &mut settings, Command::UndoScene(0));
        assert_eq!(system::notifications_off(), Some(true));
    }

    #[test]
    fn wifi_goes_through_the_worker_and_comes_back() {
        let _world = world();
        let mut q = Quick::default();
        let mut settings = Settings::default();
        settings.profiles.push(scene("Plane", SceneSystem { wifi: Some(false), ..Default::default() }));
        let plane = settings.profiles[0].clone();
        q.refresh("");
        wait("Wi‑Fi прочитан", || q.scene_part(&plane).is_some());
        assert_eq!(q.scene_part(&plane), Some(false), "Wi‑Fi включён — сцена не стоит");
        run(&mut q, &mut settings, Command::ApplyScene(0));
        wait("Wi‑Fi выключен", || net::wifi().is_some_and(|w| !w.on));
        wait("карточка это видит", || q.scene_part(&plane) == Some(true));
        assert!(q.scene_can_undo(&plane));
        run(&mut q, &mut settings, Command::UndoScene(0));
        wait("Wi‑Fi вернулся", || net::wifi().is_some_and(|w| w.on));
        wait("карточка это видит", || q.scene_part(&plane) == Some(false));
        // Включённый до сцены Wi‑Fi сцена «выключить» ставит и возвращает; уже выключенный — оставляет выключенным.
        net::set_wifi(false).unwrap();
        run(&mut q, &mut settings, Command::ApplyScene(0));
        run(&mut q, &mut settings, Command::UndoScene(0));
        q.refresh("");
        wait("прежнее состояние: выключен", || net::wifi().is_some_and(|w| !w.on));
        assert_eq!(net::wifi().map(|w| w.on), Some(false), "выключенный и остался");
    }

    #[test]
    fn another_scene_replaces_the_return() {
        let _world = world();
        let mut q = Quick::default();
        q.refresh("");
        let mut settings = Settings::default();
        settings.profiles.push(scene("A", SceneSystem { quiet: Some(true), ..Default::default() }));
        settings.profiles.push(scene("B", SceneSystem { quiet: Some(true), dark: Some(false), ..Default::default() }));
        let (a, b) = (settings.profiles[0].clone(), settings.profiles[1].clone());
        run(&mut q, &mut settings, Command::ApplyScene(0));
        assert!(q.scene_can_undo(&a));
        // Другая сцена: возврат прежней пропадает.
        run(&mut q, &mut settings, Command::ApplyScene(1));
        assert!(!q.scene_can_undo(&a) && q.scene_can_undo(&b));
        run(&mut q, &mut settings, Command::UndoScene(0));
        assert_eq!(system::notifications_off(), Some(true), "чужой возврат ничего не меняет");
        assert_eq!(system::dark_theme(), Some(false));
        // «Б» возвращает то, что было до неё: «Тишина» уже была включена «А», тема была тёмной.
        run(&mut q, &mut settings, Command::UndoScene(1));
        assert_eq!((system::notifications_off(), system::dark_theme()), (Some(true), Some(true)));
    }

    #[test]
    fn the_same_scene_again_returns_to_what_was_before_the_first_time() {
        let _world = world();
        let mut q = Quick::default();
        q.refresh("");
        let mut settings = Settings::default();
        settings.profiles.push(scene("C", SceneSystem { quiet: Some(true), dark: Some(false), ..Default::default() }));
        // До сцены: «Тишина» выключена, тема тёмная.
        run(&mut q, &mut settings, Command::ApplyScene(0));
        assert_eq!((system::notifications_off(), system::dark_theme()), (Some(true), Some(false)));
        // Тема ушла от сцены сама, и сцену ставят снова: «Тишина» к этому моменту уже включена сценой.
        system::set_dark_theme(true).unwrap();
        q.refresh("");
        assert_eq!(q.scene_part(&settings.profiles[0]), Some(false), "сцена не стоит");
        run(&mut q, &mut settings, Command::ApplyScene(0));
        assert_eq!(q.scene_part(&settings.profiles[0]), Some(true));
        // Возврат — к тому, что было до сцены, а не до её второго раза: «Тишина» снова выключена.
        run(&mut q, &mut settings, Command::UndoScene(0));
        assert_eq!((system::notifications_off(), system::dark_theme()), (Some(false), Some(true)));
    }

    /// У Windows 10 по умолчанию программы светлые, а панель задач тёмная: сцена ставит обе темы, а возврат возвращает
    /// каждую как была.
    #[test]
    fn a_mixed_theme_comes_back_the_way_it_was() {
        let _world = world();
        let mut q = Quick::default();
        let mut settings = Settings::default();
        settings.profiles.push(scene("Dark", SceneSystem { dark: Some(true), ..Default::default() }));
        settings.profiles.push(scene("Light", SceneSystem { dark: Some(false), ..Default::default() }));
        let (dark, light) = (settings.profiles[0].clone(), settings.profiles[1].clone());
        let mixed = system::Theme { apps_dark: false, system_dark: true };
        system::set_theme(mixed).unwrap();
        q.refresh("");
        assert_eq!(q.dark, Some(false), "плитка смотрит на программы");
        assert_eq!(
            (q.scene_part(&dark), q.scene_part(&light)),
            (Some(false), Some(false)),
            "ни одна тема не стоит целиком"
        );
        // «Тёмная»: обе тёмные; возврат — программы светлые, панель задач тёмная.
        run(&mut q, &mut settings, Command::ApplyScene(0));
        assert_eq!(system::theme(), Some(system::Theme::both(true)));
        assert_eq!(q.scene_part(&dark), Some(true));
        run(&mut q, &mut settings, Command::UndoScene(0));
        assert_eq!(system::theme(), Some(mixed), "панель задач осталась тёмной, как была");
        assert_eq!((q.dark, q.scene_part(&dark)), (Some(false), Some(false)));
        // «Светлая»: из смешанной обе светлые (панель задач тоже), и обратно.
        run(&mut q, &mut settings, Command::ApplyScene(1));
        assert_eq!(system::theme(), Some(system::Theme::both(false)));
        assert_eq!(q.scene_part(&light), Some(true));
        run(&mut q, &mut settings, Command::UndoScene(1));
        assert_eq!(system::theme(), Some(mixed));
        // Плитка меняет обе темы вместе.
        system::set_dark_theme(false).unwrap();
        q.refresh("");
        assert_eq!(system::theme(), Some(system::Theme::both(false)));
    }

    /// Сцену узнают по номеру: переименование возврат не теряет, новая сцена со старым именем чужого не наследует.
    #[test]
    fn the_return_belongs_to_the_scene_not_to_its_name() {
        let _world = world();
        let mut q = Quick::default();
        q.refresh("");
        let mut settings = Settings::default();
        let quiet = SceneSystem { quiet: Some(true), ..Default::default() };
        settings.profiles.push(scene("Quiet", quiet.clone()));
        run(&mut q, &mut settings, Command::ApplyScene(0));
        settings.profiles[0].name = "Hush".into();
        assert!(q.scene_can_undo(&settings.profiles[0]), "переименование возврат не теряет");
        assert!(!q.scene_can_undo(&scene("Quiet", quiet)), "новая сцена со старым именем возврата не наследует");
        run(&mut q, &mut settings, Command::UndoScene(0));
        assert_eq!(system::notifications_off(), Some(false));
    }

    /// Что не вышло, говорится словами программы (английский текст ошибки остаётся в журнале); Wi‑Fi — из потока модуля,
    /// куда бы ни смотрела карточка.
    #[test]
    fn what_fails_is_told_in_the_programs_words() {
        let _world = world();
        let t = texts::tr(Lang::En);
        let mut q = Quick::default();
        let mut settings = Settings::default();
        settings.profiles.push(scene("Plane", SceneSystem { wifi: Some(false), ..Default::default() }));
        q.refresh("");
        wait("строка Wi‑Fi стала доступна", || q.scene_rows().contains(&(SceneRow::Wifi, true)));
        net::FAKE_WIFI_FAILS.store(true, std::sync::atomic::Ordering::Relaxed);
        run(&mut q, &mut settings, Command::ApplyScene(0));
        let mut told = Vec::new();
        wait("слова про Wi‑Fi", || {
            told.extend(q.problems(t));
            !told.is_empty()
        });
        assert_eq!(told, vec![(format!("{}: Wi‑Fi", t.failed), Tone::Danger)]);
        assert!(q.problems(t).is_empty(), "второй раз то же не повторяется");
        // Сбой части сцены: «Не получилось: Тёмная тема», а не «cannot open …».
        let mut toasts = Vec::new();
        let mut cx =
            Ctx { t, lang: Lang::En, monitor: r"\\.\TEST-QUICK", toasts: &mut toasts, settings: &mut settings };
        part_failed(&mut cx, SceneRow::Dark, r"cannot open Software\Microsoft\Windows");
        assert_eq!(toasts, vec![(format!("{}: {}", t.failed, t.dark_theme_win), Tone::Danger)]);
        net::FAKE_WIFI_FAILS.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    #[test]
    fn a_plan_that_is_gone_is_skipped_with_a_warning() {
        let _world = world();
        let mut q = Quick::default();
        q.refresh("");
        let mut settings = Settings::default();
        settings.profiles.push(scene("P", SceneSystem { plan: Some(plan(0xFF)), ..Default::default() }));
        let toasts = run(&mut q, &mut settings, Command::ApplyScene(0));
        assert_eq!(toasts, vec![(texts::tr(Lang::En).scene_plan_missing.to_string(), Tone::Warning)]);
        assert_eq!(system::plans().1, Some(GUID::from_u128(1)), "план не тронут");
        assert!(!q.scene_can_undo(&settings.profiles[0]), "менять было нечего — вернуть тоже");
    }

    #[test]
    fn save_scene_takes_the_asked_rows_from_what_stands_now() {
        let _world = world();
        let mut q = Quick::default();
        let mut settings = Settings::default();
        settings.profiles.push(Profile::new("N"));
        settings.profiles.push(Profile::new("M"));
        system::set_dark_theme(false).unwrap();
        system::set_notifications_off(true).unwrap();
        system::set_plan(&GUID::from_u128(3)).unwrap();
        q.refresh("");
        wait("строка Wi‑Fi стала доступна", || q.scene_rows().contains(&(SceneRow::Wifi, true)));
        let all = vec![SceneRow::Quiet, SceneRow::Awake, SceneRow::Dark, SceneRow::Wifi, SceneRow::Plan];
        run(&mut q, &mut settings, Command::SaveScene(0, all));
        let s = &settings.profiles[0].system;
        assert_eq!((s.quiet, s.awake, s.dark, s.wifi), (Some(true), Some(false), Some(false), Some(true)));
        assert_eq!(s.plan, Some(plan(3)));
        assert!(s.night.is_none() && s.bright.is_none(), "чужие строки — не здесь");
        // Только то, что просили; экран и яркость — не дело быстрых настроек.
        run(&mut q, &mut settings, Command::SaveScene(1, vec![SceneRow::Dark, SceneRow::Screen, SceneRow::Bright]));
        let m = &settings.profiles[1].system;
        assert_eq!((m.dark, m.quiet, m.awake, m.wifi, m.plan.clone()), (Some(false), None, None, None, None));
    }

    #[test]
    fn rows_and_parts_of_this_card() {
        let _world = world();
        let mut q = Quick::default();
        // До первого чтения: «Не спать» можно всегда, остальное — когда прочиталось.
        let rows = q.scene_rows();
        assert!(rows.contains(&(SceneRow::Awake, true)) && rows.contains(&(SceneRow::Quiet, false)));
        q.refresh("");
        let rows = q.scene_rows();
        for row in [SceneRow::Quiet, SceneRow::Awake, SceneRow::Dark, SceneRow::Plan] {
            assert!(rows.contains(&(row, true)), "{row:?}");
        }
        assert!(!rows.iter().any(|(r, _)| matches!(r, SceneRow::Screen | SceneRow::Bright | SceneRow::Night)));
        // В сцене нет частей этой карточки — ей нечего проверять.
        let screen_only = Profile::new("S");
        assert_eq!(q.scene_part(&screen_only), None);
        assert!(!q.scene_can_undo(&screen_only));
    }
}

// ─── Действия поиска ────────────────────────────────────────────────────────
//
// Плитки, планы питания и VPN — то же, что щелчок в карточке. Выдуманный мир (тема, «Тишина», план, Wi‑Fi, VPN «Demo
// Office») один на все тесты — под `WORLD_TEST`.

#[cfg(test)]
mod search_tests {
    use std::sync::MutexGuard;
    use std::time::{Duration, Instant};

    use anvil_ui::Lang;
    use windows::core::GUID;

    use super::*;
    use crate::sys;

    const EN: Lang = Lang::En;

    fn world() -> MutexGuard<'static, ()> {
        let guard = sys::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        sys::reset_fake_world();
        guard
    }

    fn wait(what: &str, mut ok: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ok() {
            assert!(start.elapsed() < Duration::from_secs(3), "не дождались: {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn run(q: &mut Quick, id: &str) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        let mut settings = Settings::default();
        let mut cx = Ctx {
            t: texts::tr(EN),
            lang: EN,
            monitor: r"\\.\TEST-SEARCH-QUICK",
            toasts: &mut toasts,
            settings: &mut settings,
        };
        q.command(&Command::RunAction(id.to_string()), &mut cx);
        toasts
    }

    fn titles(actions: &[Action]) -> Vec<&str> {
        actions.iter().map(|a| a.title.as_str()).collect()
    }

    /// Карточка, опрошенная, как при показе панели; Wi‑Fi и VPN отвечают из потока модуля — ждём.
    fn card() -> Quick {
        let mut q = Quick::default();
        q.refresh("");
        wait("Wi‑Fi и VPN прочитаны", || {
            q.scene_rows().contains(&(SceneRow::Wifi, true)) && !q.shared.lock().unwrap().vpns.is_empty()
        });
        q
    }

    #[test]
    fn tiles_plans_and_vpn_are_offered() {
        let _world = world();
        let q = card();
        let t = texts::tr(EN);
        let actions = q.actions(t, EN, &Settings::default());
        // Вначале: «Не спать» выключено, тема тёмная, «Тишина» выключена, Wi‑Fi включён, план «Balanced», VPN отключён.
        assert_eq!(
            titles(&actions),
            [
                "Keep awake: turn on",
                "Dark mode: turn off",
                "Quiet: turn on",
                "Wi‑Fi: turn off",
                "Power plan: High performance",
                "Power plan: Power saver",
                "VPN “Demo Office”: connect",
            ]
        );
        assert!(actions.iter().all(|a| a.enabled && a.reason.is_none() && a.place == t.quick));
        let ru = q.actions(texts::tr(Lang::Ru), Lang::Ru, &Settings::default());
        assert_eq!(ru[0].title, "Не спать: включить");
        assert_eq!(ru[6].title, "VPN «Demo Office»: подключить");
        // Слова для поиска — оба языка.
        assert!(actions[2].words.contains("уведомления") && actions[2].words.contains("notifications"));
    }

    #[test]
    fn every_action_does_what_the_click_does() {
        let _world = world();
        let mut q = card();
        let t = texts::tr(EN);
        assert!(run(&mut q, "quick:awake").is_empty());
        assert!(q.awake, "«Не спать» включено");
        run(&mut q, "quick:dark");
        assert_eq!(sys::system::dark_theme(), Some(false));
        run(&mut q, "quick:quiet");
        assert_eq!(sys::system::notifications_off(), Some(true));
        run(&mut q, "quick:wifi");
        wait("Wi‑Fi выключен", || sys::net::wifi().is_some_and(|w| !w.on));
        let plan = format!("quick:plan:{}", system::guid_string(&GUID::from_u128(3)));
        assert!(run(&mut q, &plan).is_empty());
        assert_eq!(sys::system::plans().1, Some(GUID::from_u128(3)));
        run(&mut q, "quick:vpn:Demo Office");
        wait("VPN подключается", || {
            sys::BLOCKED.lock().unwrap().iter().any(|w| w.contains("set_vpn Demo Office true"))
        });
        // Названия следуют за состоянием: включённое предлагается выключить, и наоборот; стоящий план не предлагается.
        wait("карточка видит Wi‑Fi выключенным", || {
            titles(&q.actions(t, EN, &Settings::default())).contains(&"Wi‑Fi: turn on")
        });
        let list = q.actions(t, EN, &Settings::default());
        let list = titles(&list);
        for want in ["Keep awake: turn off", "Dark mode: turn on", "Quiet: turn off", "Power plan: Balanced"] {
            assert!(list.contains(&want), "нет «{want}»: {list:?}");
        }
        assert!(!list.contains(&"Power plan: Power saver"));
        // Те же щелчки вторым разом возвращают.
        run(&mut q, "quick:awake");
        run(&mut q, "quick:dark");
        run(&mut q, "quick:quiet");
        assert!(!q.awake);
        assert_eq!((sys::system::dark_theme(), sys::system::notifications_off()), (Some(true), Some(false)));
        // Чужое действие и несуществующий план ничего не делают.
        let before = sys::system::plans().1;
        assert!(run(&mut q, "display:hdr").is_empty() && run(&mut q, "quick:plan:{nope}").is_empty());
        assert_eq!(sys::system::plans().1, before);
    }

    #[test]
    fn an_unavailable_tile_is_grey_with_the_reason_and_does_nothing() {
        let _world = world();
        let mut q = card();
        let t = texts::tr(EN);
        // Радио Wi‑Fi нет: плитка серая — «Нет радио Wi‑Fi», название без «включить / выключить».
        q.shared.lock().unwrap().wifi = Some(None);
        let actions = q.actions(t, EN, &Settings::default());
        let wifi = actions.iter().find(|a| a.id == "quick:wifi").expect("плитка есть");
        assert!(!wifi.enabled && wifi.reason.as_deref() == Some(t.scene_why_wifi) && wifi.title == t.wifi);
        run(&mut q, "quick:wifi");
        assert_eq!(sys::net::wifi().map(|w| w.on), Some(true), "серая плитка ничего не выключила");
        // Ещё не прочиталось: «Недоступно».
        q.shared.lock().unwrap().wifi = None;
        let wifi = q.actions(t, EN, &Settings::default()).into_iter().find(|a| a.id == "quick:wifi").unwrap();
        assert!(!wifi.enabled && wifi.reason.as_deref() == Some(t.unavailable));
        // Windows не отдала тему и «Тишину»: серые, причина — «Windows не отдала значение».
        q.dark = None;
        q.quiet = None;
        let actions = q.actions(t, EN, &Settings::default());
        for id in ["quick:dark", "quick:quiet"] {
            let a = actions.iter().find(|a| a.id == id).unwrap();
            assert!(!a.enabled && a.reason.as_deref() == Some(t.scene_why_read), "{id}");
        }
        run(&mut q, "quick:dark");
        run(&mut q, "quick:quiet");
        assert_eq!((sys::system::dark_theme(), sys::system::notifications_off()), (Some(true), Some(false)));
        // «Не спать» можно всегда.
        assert!(actions.iter().find(|a| a.id == "quick:awake").unwrap().enabled);
    }

    #[test]
    fn without_vpn_connections_there_are_no_vpn_actions_and_no_plans_when_none() {
        let _world = world();
        let q = card();
        let t = texts::tr(EN);
        q.shared.lock().unwrap().vpns.clear();
        let actions = q.actions(t, EN, &Settings::default());
        assert!(!actions.iter().any(|a| a.id.starts_with("quick:vpn:")), "VPN-подключений нет — и действий нет");
        // Планов питания не прочиталось — их действий тоже нет (остаются плитки).
        let mut q = card();
        q.plans.clear();
        let actions = q.actions(t, EN, &Settings::default());
        assert_eq!(actions.len(), 5, "четыре плитки и VPN");
        assert!(!actions.iter().any(|a| a.id.starts_with("quick:plan:")));
    }
}
