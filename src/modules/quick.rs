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

use super::{Command, Ctx, Module, chips};
use crate::config::{Profile, SceneRow};
use crate::sys::net::{self, Vpn, Wifi};
use crate::sys::system;
use crate::texts::Tr;

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

#[derive(Default)]
struct Shared {
    /// `None` — ещё не прочитано; `Some(None)` — радио Wi‑Fi нет.
    wifi: Option<Option<Wifi>>,
    vpns: Vec<Vpn>,
    /// Ошибка последнего действия — показать уведомлением.
    error: Option<String>,
    /// Был ли включён Wi‑Fi до сцены: запоминает поток, когда ставит его по сцене.
    wifi_before: Option<bool>,
}

/// Как было до сцены: что стояло в тех настройках, которые она поменяла (`None` — не меняла).
struct SceneBefore {
    scene: String,
    awake: Option<bool>,
    dark: Option<bool>,
    quiet: Option<bool>,
    plan: Option<windows::core::GUID>,
    /// Wi‑Fi сцена трогала: прежнее лежит у потока (`Shared::wifi_before`).
    wifi: bool,
}

impl SceneBefore {
    fn new(scene: &str) -> SceneBefore {
        SceneBefore { scene: scene.to_string(), awake: None, dark: None, quiet: None, plan: None, wifi: false }
    }

    fn is_empty(&self) -> bool {
        self.awake.is_none() && self.dark.is_none() && self.quiet.is_none() && self.plan.is_none() && !self.wifi
    }
}

pub struct Quick {
    /// «Не спать» — только пока работает программа.
    awake: bool,
    dark: Option<bool>,
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
        Quick { awake: false, dark: None, quiet: None, plans: Vec::new(), plan: None, before: None, jobs: tx, shared }
    }
}

fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
    crate::sys::media::init_thread();
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
        }
        let mut error = None;
        for j in jobs {
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
                error = Some(e);
            }
        }
        let (wifi, vpns) = (net::wifi(), net::vpns());
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.wifi = Some(wifi);
        s.vpns = vpns;
        if error.is_some() {
            s.error = error;
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
        self.dark = system::dark_theme();
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
            Command::Dropped(_) | Command::Media(_) => {}
        }
    }

    /// Совпадает ли то, что сцена отметила здесь, с тем, что стоит: «Не спать» — своё состояние, остальное — прочитанное при
    /// показе панели; не прочиталось — не мешает. Ничего не отмечено или не прочитать — `None`.
    fn scene_part(&self, scene: &Profile) -> Option<bool> {
        let s = &scene.system;
        let mut same: Vec<bool> = Vec::new();
        if let Some(want) = s.awake {
            same.push(self.awake == want);
        }
        if let (Some(want), Some(now)) = (s.dark, self.dark) {
            same.push(want == now);
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
        self.before.as_ref().is_some_and(|b| b.scene == scene.name)
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

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let (wifi, vpns, error) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.wifi.clone(), s.vpns.clone(), s.error.take())
        };
        if let Some(e) = error {
            cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
        }
        let wifi_state = wifi.clone().flatten();
        // Название плитки одно — «Wi‑Fi»; сеть, через которую выход в интернет, — в строке состояния.
        let network = wifi_state.as_ref().filter(|wf| wf.on && !wf.network.is_empty()).map(|wf| wf.network.as_str());
        egui::ScrollArea::vertical().id_salt("quick").auto_shrink([false, false]).show(ui, |ui| {
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
                    match system::set_plan(&g) {
                        Ok(()) => self.plan = Some(g),
                        Err(e) => cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger)),
                    }
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

/// Сбой части сцены: в журнал по-английски, пользователю — «Не получилось: …».
fn scene_failed(cx: &mut Ctx, what: &str, e: String) {
    eprintln!("scene {what} failed: {e}");
    cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
}

impl Quick {
    /// Поставить то, что сцена отметила у быстрых настроек. Перед сменой — запомнить, что было. Возврат прежней сцены
    /// пропадает; та же сцена ещё раз продолжает прежний «сеанс»: что уже помним, то и остаётся.
    fn apply_scene(&mut self, scene: &Profile, cx: &mut Ctx) {
        let s = &scene.system;
        let mut before = match self.before.take() {
            Some(b) if b.scene == scene.name => b,
            _ => SceneBefore::new(&scene.name),
        };
        if let Some(want) = s.dark
            && let Some(now) = system::dark_theme()
        {
            before.dark = before.dark.or(Some(now));
            if now != want {
                match system::set_dark_theme(want) {
                    Ok(()) => self.dark = Some(want),
                    Err(e) => scene_failed(cx, "dark theme", e),
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
                    Err(e) => scene_failed(cx, "quiet", e),
                }
            }
        }
        if let Some(want) = s.awake {
            before.awake = before.awake.or(Some(self.awake));
            if want != self.awake {
                if system::keep_awake(want) {
                    self.awake = want;
                } else {
                    scene_failed(cx, "keep awake", "SetThreadExecutionState failed".to_string());
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
                        Err(e) => scene_failed(cx, "power plan", e),
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
        if let Some(dark) = before.dark
            && system::dark_theme().is_some_and(|now| now != dark)
        {
            match system::set_dark_theme(dark) {
                Ok(()) => self.dark = Some(dark),
                Err(e) => scene_failed(cx, "dark theme", e),
            }
        }
        if let Some(quiet) = before.quiet
            && system::notifications_off().is_some_and(|now| now != quiet)
        {
            match system::set_notifications_off(quiet) {
                Ok(()) => self.quiet = Some(quiet),
                Err(e) => scene_failed(cx, "quiet", e),
            }
        }
        if let Some(awake) = before.awake
            && awake != self.awake
        {
            if system::keep_awake(awake) {
                self.awake = awake;
            } else {
                scene_failed(cx, "keep awake", "SetThreadExecutionState failed".to_string());
            }
        }
        if let Some(plan) = before.plan
            && system::plans().1 != Some(plan)
        {
            match system::set_plan(&plan) {
                Ok(()) => self.plan = Some(plan),
                Err(e) => scene_failed(cx, "power plan", e),
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
        let t = cx.t;
        let result = match i {
            0 => {
                let on = !self.awake;
                Some(if system::keep_awake(on) {
                    self.awake = on;
                    Ok(())
                } else {
                    Err("SetThreadExecutionState failed".to_string())
                })
            }
            1 => self.dark.map(|on| system::set_dark_theme(!on).map(|()| self.dark = Some(!on))),
            2 => self.quiet.map(|on| system::set_notifications_off(!on).map(|()| self.quiet = Some(!on))),
            3 => wifi.map(|wf| {
                let _ = self.jobs.send(Job::Wifi(!wf.on));
                Ok(())
            }),
            _ => None,
        };
        if let Some(Err(e)) = result {
            eprintln!("quick settings failed: {e}");
            cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
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
