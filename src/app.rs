// Окно панели — это и есть панель: того же размера, у верхнего края монитора. Выезд — окно
// само съезжает из-за края вниз (часть выше края экрана не видна). Нижние углы скругляет регион
// окна (SetWindowRgn), а не прозрачность: прозрачность окна OpenGL на Windows ненадёжна — на части
// систем прозрачное рисуется белым, а с «пропускать мышь» (WS_EX_LAYERED) белеет всё окно.
// Спрятанное окно уводим правее всех мониторов.

use std::time::{Duration, Instant};

use anvil_ui::motion::{self, Motion};
use anvil_ui::widgets::{self as w, Kind, Toasts};
use anvil_ui::{Accent, Icon, Lang, Palette, ThemeChoice, Tone, tr};
use eframe::egui::{self, CornerRadius, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

use crate::config::{self, Hotkey, MAX_HEIGHT, MIN_HEIGHT, MIN_WIDTH, Settings};
use crate::modules::{self, Command, Ctx, Module};
use crate::panel::{self, Edge, Input};
use crate::sys::{self, monitor};
use crate::texts;

pub const ACCENT: Accent = Accent::BLUE;
/// Радиус нижних углов панели, в точках.
const CORNER: u8 = 14;
const HEADER: f32 = 40.0;
const GRIP: f32 = 7.0;
/// Как часто перечитывать систему, пока панель открыта.
const REFRESH: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq)]
enum View {
    Cards,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum Grip {
    Bottom,
    Left,
    Right,
    BottomLeft,
    BottomRight,
}

pub struct App {
    settings: Settings,
    edge: Edge,
    modules: Vec<Box<dyn Module>>,
    toasts: Toasts,
    view: View,
    monitors: Vec<monitor::Monitor>,
    monitors_at: Option<Instant>,
    /// Что последним отправили окну: позиция и размер в точках.
    placed: Option<(Pos2, Vec2)>,
    /// Доля выезда в прошлом кадре: 0 — панель целиком спрятана и окно можно увести.
    drawn: f32,
    /// Под какой размер окна в пикселях поставлен регион со скруглёнными углами.
    region: Option<(i32, i32)>,
    focused: bool,
    grip: Option<Grip>,
    refreshed: Option<Instant>,
    /// Какое сочетание из `sys::HOTKEYS` удалось занять.
    hotkey: Option<usize>,
    autostart: bool,
    started: bool,
    /// Ждём сочетание для профиля с этим номером (настройки, «Изменить…»).
    capture: Option<usize>,
    /// Какие клавиши профилей последними отдали потоку клавиш — шлём заново, только если поменялись.
    sent_keys: Option<Vec<Option<String>>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = config::load();
        anvil_ui::install(&cc.egui_ctx, ACCENT, settings.common.theme);
        settings.common.apply(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let hotkey = sys::start_hotkey(move || ctx.request_repaint());
        match hotkey {
            Some(i) => eprintln!("hotkey: {}", sys::HOTKEYS[i].0.join("+")),
            None => eprintln!("all hotkeys are taken by other apps"),
        }
        App {
            settings,
            // Для снимков: закрепить сразу, чтобы панель после показа при запуске не уезжала.
            edge: {
                let mut edge = Edge::default();
                edge.pinned = std::env::var_os("ANTRESOL_PIN").is_some();
                edge
            },
            modules: modules::all(),
            toasts: Toasts::default(),
            // Для снимков: открыть сразу настройки, не щёлкая мышью.
            view: if std::env::var("ANTRESOL_VIEW").is_ok_and(|v| v == "settings") {
                View::Settings
            } else {
                View::Cards
            },
            monitors: Vec::new(),
            monitors_at: None,
            placed: None,
            drawn: 0.0,
            region: None,
            focused: false,
            grip: None,
            refreshed: None,
            hotkey,
            autostart: sys::autostart(),
            started: false,
            capture: None,
            sent_keys: None,
        }
    }

    /// Монитор панели: из настроек, а если его нет (отключили) — основной.
    fn monitor(&self) -> Option<&monitor::Monitor> {
        let by_name = self.monitors.iter().find(|m| m.device == self.settings.monitor);
        by_name.or_else(|| self.monitors.first())
    }

    fn lang(&self) -> Lang {
        self.settings.common.language
    }

    /// Поставить окно у края монитора по размеру панели. Шлёт команды, только если что-то поменялось.
    /// Поставить окно у края монитора: `t` — доля выезда (0 — целиком выше края, увести прочь).
    /// Шлёт команды, только если что-то поменялось.
    fn place(&mut self, ctx: &egui::Context, ppp: f32, t: f32) {
        let Some(m) = self.monitor() else { return };
        let r = m.rect;
        let size = Vec2::new(self.settings.width, self.settings.height);
        let left = r.left as f32 / ppp + (r.width() as f32 / ppp - size.x) / 2.0;
        let mut pos = Pos2::new(left.round(), (r.top as f32 / ppp - (1.0 - t) * size.y).round());
        if t <= 0.0 {
            let far = self.monitors.iter().map(|m| m.rect.right).max().unwrap_or(r.right);
            pos = Pos2::new(far as f32 / ppp + 200.0, r.top as f32 / ppp);
        }
        if self.placed != Some((pos, size)) {
            if std::env::var_os("ANTRESOL_TRACE").is_some() {
                eprintln!(
                    "place t={t:.2} pos={pos:?} size={size:?} ppp={ppp} theme={:?} pref={:?}",
                    ctx.theme(),
                    ctx.options(|o| o.theme_preference)
                );
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            self.placed = Some((pos, size));
        }
        let px = ((size.x * ppp).round() as i32, (size.y * ppp).round() as i32);
        if self.region != Some(px) && sys::round_bottom_corners(px.0, px.1, (CORNER as f32 * ppp) as i32) {
            self.region = Some(px);
        }
    }

    fn save(&self) {
        config::save(&self.settings);
    }

    /// Тянут край панели: новый размер считаем от курсора на экране, а не от окна —
    /// окно само меняется под рукой.
    fn drag_resize(&mut self, grip: Grip, ppp: f32) {
        let Some(m) = self.monitor().cloned() else { return };
        let (cx, cy) = monitor::cursor();
        let center = m.rect.left as f32 + m.rect.width() as f32 / 2.0;
        let max_w = m.rect.width() as f32 / ppp;
        if matches!(grip, Grip::Left | Grip::Right | Grip::BottomLeft | Grip::BottomRight) {
            self.settings.width = (2.0 * (cx as f32 - center).abs() / ppp).clamp(MIN_WIDTH, max_w.max(MIN_WIDTH));
        }
        if matches!(grip, Grip::Bottom | Grip::BottomLeft | Grip::BottomRight) {
            self.settings.height = ((cy - m.rect.top) as f32 / ppp).clamp(MIN_HEIGHT, MAX_HEIGHT);
        }
    }

    fn tick(&mut self, ctx: &egui::Context) {
        motion::tick(ctx);
        let ppp = ctx.pixels_per_point();
        let now = Instant::now();
        if self.monitors_at.is_none_or(|at| at.elapsed() > Duration::from_secs(2)) {
            self.monitors = monitor::monitors();
            self.monitors_at = Some(now);
        }
        let Some(m) = self.monitor().cloned() else {
            ctx.request_repaint_after(Duration::from_millis(500));
            return;
        };

        // Где панель на экране, в пикселях.
        let center = m.rect.left + m.rect.width() / 2;
        let half = (self.settings.width * ppp / 2.0) as i32;
        let (left, right) = (center - half, center + half);
        let bottom = m.rect.top + (self.settings.height * ppp) as i32;
        let cursor = monitor::cursor();
        let at_edge = panel::at_edge(cursor, m.rect.top, left, right);
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        let busy = self.grip.is_some() || self.capture.is_some() || self.modules.iter().any(|m| m.busy());
        let hotkey = sys::take_hotkey();
        if !self.started {
            self.started = true;
            self.edge.intro(now);
            // eframe держит новое окно невидимым до первой отрисовки, а невидимому Windows не
            // шлёт перерисовку — без явного показа окно ждало бы первого движения мыши.
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        let input = Input {
            at_edge,
            over: self.edge.shown
                && cursor.0 >= left
                && cursor.0 < right
                && cursor.1 >= m.rect.top
                && cursor.1 < bottom,
            hotkey,
            busy,
            fullscreen: at_edge && self.settings.fullscreen_guard && monitor::fullscreen_on(m.rect),
            lost_focus: self.focused && !focused,
            focused,
        };
        self.focused = focused;
        let show_delay = Duration::from_millis(self.settings.show_delay_ms as u64);
        let hide_delay = Duration::from_millis(self.settings.hide_delay_ms as u64);
        let changed = self.edge.step(input, now, show_delay, hide_delay);
        let changed = self.profile_keys(ctx, &m.device, now) || changed;
        if changed && self.edge.shown {
            // Только что выехала: свежие данные и фокус, чтобы клавиатура и Esc работали.
            self.refreshed = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if changed && !self.edge.shown {
            self.view = View::Cards;
        }

        if self.edge.shown && self.refreshed.is_none_or(|at| at.elapsed() > REFRESH) {
            for module in &mut self.modules {
                module.refresh(&m.device);
            }
            self.refreshed = Some(now);
        }

        let t = Motion::of(ctx).toggle(ctx, egui::Id::new("panel-shown"), self.edge.shown, motion::ENTER);
        self.drawn = t;
        self.place(ctx, ppp, t);

        // Спрятана — смотрим на мышь 25 раз в секунду; открыта — чаще, ради отсчёта ухода.
        let ticking = self.modules.iter().any(|m| m.ticking());
        let period = if self.edge.shown || self.edge.counting() || ticking { 33 } else { 40 };
        ctx.request_repaint_after(Duration::from_millis(period));
    }

    /// Клавиши профилей: отдать потоку клавиш новый набор, если он поменялся, и выполнить нажатые.
    /// Нажали при спрятанной панели — она выезжает с вопросом «Оставить?». Ответ — выехала ли.
    fn profile_keys(&mut self, ctx: &egui::Context, device: &str, now: Instant) -> bool {
        // Пока ждём сочетание, клавиши профилей сняты: иначе Windows перехватит уже назначенное
        // сочетание раньше окна и вместо захвата сработает профиль.
        let capturing = self.capture.is_some();
        let keys: Vec<Option<String>> = self
            .settings
            .profiles
            .iter()
            .map(|p| p.hotkey.as_ref().filter(|_| !capturing).map(|h| h.to_string()))
            .collect();
        if self.sent_keys.as_ref() != Some(&keys) {
            let list: Vec<Option<(Vec<&str>, &str)>> = self
                .settings
                .profiles
                .iter()
                .map(|p| {
                    let h = p.hotkey.as_ref().filter(|_| !capturing)?;
                    let keys = h.keys();
                    Some((keys[..keys.len() - 1].to_vec(), h.key.as_str()))
                })
                .collect();
            sys::set_profile_hotkeys(&list);
            self.sent_keys = Some(keys);
        }
        let count = self.settings.profiles.len();
        let pressed: Vec<usize> = sys::take_profile_presses().into_iter().filter(|&i| i < count).collect();
        if pressed.is_empty() {
            return false;
        }
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let mut toasts = Vec::new();
        let before = self.settings.clone();
        for i in pressed {
            for module in &mut self.modules {
                let mut cx = Ctx { t, lang, monitor: device, toasts: &mut toasts, settings: &mut self.settings };
                module.command(&Command::ApplyProfile(i), &mut cx);
            }
        }
        for (text, tone) in toasts {
            self.toasts.push(text, tone);
        }
        if self.settings != before {
            self.save();
        }
        let was = self.edge.shown;
        self.edge.reveal(now);
        self.view = View::Cards;
        if !was {
            self.refreshed = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        !was
    }

    /// Ждём сочетание для профиля: первая клавиша с Ctrl или Alt; Esc — отмена.
    fn capture_keys(&mut self, ui: &Ui) {
        let Some(i) = self.capture else { return };
        let events = ui.input(|inp| inp.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            if key == egui::Key::Escape {
                self.capture = None;
                return;
            }
            let name = key.name();
            let hotkey = Hotkey {
                ctrl: modifiers.ctrl,
                alt: modifiers.alt,
                shift: modifiers.shift,
                win: false,
                key: name.to_string(),
            };
            if hotkey.valid() && sys::vk_of(name).is_some() {
                let panel = self.hotkey.map(|k| sys::HOTKEYS[k].0.join("+"));
                if panel.as_deref() == Some(hotkey.to_string().as_str()) {
                    let t = texts::tr(self.lang());
                    self.toasts.push(t.key_is_panel, Tone::Warning);
                    return;
                }
                // Одно сочетание — у одного профиля: у прежнего хозяина снимаем.
                for (j, p) in self.settings.profiles.iter_mut().enumerate() {
                    if j != i && p.hotkey.as_ref() == Some(&hotkey) {
                        p.hotkey = None;
                    }
                }
                if let Some(p) = self.settings.profiles.get_mut(i) {
                    p.hotkey = Some(hotkey);
                }
                self.capture = None;
                return;
            }
        }
    }

    /// Карточка «Профили экрана»: имя, клавиша (захват, «занято»), убрать клавишу, удалить.
    fn profiles_card(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        w::section_label(ui, t.profiles_settings);
        if self.settings.profiles.is_empty() {
            w::note(ui, t.profiles_none);
            return;
        }
        self.capture_keys(ui);
        let mut delete = None;
        for i in 0..self.settings.profiles.len() {
            let name = self.settings.profiles[i].name.clone();
            let hotkey = self.settings.profiles[i].hotkey.clone();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&name).font(anvil_ui::semibold(13.5)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::icon_button(ui, Icon::Trash, t.delete).clicked() {
                        delete = Some(i);
                    }
                    if hotkey.is_some() && w::icon_button(ui, Icon::Close, t.remove_key).clicked() {
                        self.settings.profiles[i].hotkey = None;
                    }
                });
            });
            if self.capture == Some(i) {
                ui.horizontal_wrapped(|ui| {
                    w::badge(ui, t.press_keys, Tone::Accent);
                    if w::button(ui, Kind::Ghost, None, t.cancel).clicked() {
                        self.capture = None;
                    }
                });
            } else {
                let keys: Vec<&str> = match &hotkey {
                    Some(h) => h.keys(),
                    None => vec![t.no_key],
                };
                let status = match (&hotkey, sys::profile_hotkey_status(i)) {
                    (Some(_), Some(true)) => Some((true, t.key_free)),
                    (Some(_), Some(false)) => Some((false, t.key_busy)),
                    _ => None,
                };
                if w::hotkey_field(ui, &keys, t.change, status).clicked() {
                    self.capture = Some(i);
                }
            }
            ui.add_space(6.0);
        }
        if let Some(i) = delete {
            self.settings.profiles.remove(i);
            self.capture = None;
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        ui.horizontal(|ui| {
            anvil_ui::chrome::app_mark(ui, Icon::Layers, 22.0);
            ui.label(egui::RichText::new("Antresol").font(anvil_ui::semibold(14.0)).color(p.text));
            if self.view == View::Settings {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(t.settings).color(p.weak));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::icon_button(ui, Icon::Close, t.quit).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let (icon, hint) =
                    if self.view == View::Settings { (Icon::ArrowLeft, t.back) } else { (Icon::Gear, t.settings) };
                if w::icon_button(ui, icon, hint).clicked() {
                    self.view = if self.view == View::Settings { View::Cards } else { View::Settings };
                }
                let pinned = self.edge.pinned;
                let r = w::icon_button(ui, Icon::Pin, if pinned { t.unpin } else { t.pin });
                if pinned {
                    // Закреплена — значок на мягкой подложке акцента.
                    ui.painter().rect_filled(r.rect, 6, p.soft(p.accent));
                    anvil_ui::icons::paint(
                        ui.painter(),
                        Rect::from_center_size(r.rect.center(), Vec2::splat(17.0)),
                        Icon::Pin,
                        p.accent_text,
                    );
                }
                if r.clicked() {
                    self.edge.pinned = !pinned;
                }
            });
        });
    }

    fn cards(&mut self, ui: &mut Ui, device: &str) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let area = ui.available_rect_before_wrap();
        let gap = 10.0;
        let total: f32 = self.modules.iter().map(|m| m.weight()).sum();
        let free = area.width() - gap * (self.modules.len().saturating_sub(1)) as f32;
        let mut x = area.left();
        let mut toasts = Vec::new();
        let before = self.settings.clone();
        let settings = &mut self.settings;
        for (i, module) in self.modules.iter_mut().enumerate() {
            let width = free * module.weight() / total;
            let rect = Rect::from_min_size(Pos2::new(x, area.top()), Vec2::new(width, area.height()));
            x += width + gap;
            // Карточки входят по очереди: сдвиг и прозрачность, как строки списка.
            let (alpha, dy) = Motion::of(ui.ctx()).enter(ui.ctx(), ui.id().with(("card", module.key())), i);
            let mut child = ui.new_child(UiBuilder::new().max_rect(rect.translate(Vec2::new(0.0, dy))));
            child.set_opacity(alpha);
            w::card_frame(&child).show(&mut child, |ui| {
                ui.set_min_size(ui.available_size());
                ui.set_clip_rect(ui.max_rect().intersect(rect.expand(1.0)));
                w::section_label(ui, module.title(t));
                ui.add_space(4.0);
                let mut cx = Ctx { t, lang, monitor: device, toasts: &mut toasts, settings: &mut *settings };
                module.ui(ui, &mut cx);
            });
        }
        for (text, tone) in toasts {
            self.toasts.push(text, tone);
        }
        if self.settings != before {
            self.save();
        }
    }

    fn settings_view(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let ctx = ui.ctx().clone();
        let before = self.settings.clone();
        let area = ui.available_rect_before_wrap();
        // Три карточки: «Панель», «Профили экрана», «Оформление».
        let third = (area.width() - 20.0) / 3.0;
        let col = |i: f32| {
            Rect::from_min_size(
                Pos2::new(area.left() + i * (third + 10.0), area.top()),
                Vec2::new(third, area.height()),
            )
        };
        let (left, middle, right) = (col(0.0), col(1.0), col(2.0));

        let mut mid = ui.new_child(UiBuilder::new().max_rect(middle));
        w::card_frame(&mid).show(&mut mid, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("set-m").auto_shrink([false, false]).show(ui, |ui| {
                self.profiles_card(ui);
            });
        });

        let mut l = ui.new_child(UiBuilder::new().max_rect(left));
        w::card_frame(&l).show(&mut l, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("set-l").auto_shrink([false, false]).show(ui, |ui| {
                w::section_label(ui, t.panel);
                ui.horizontal(|ui| {
                    ui.label(t.monitor);
                    let names: Vec<String> = self
                        .monitors
                        .iter()
                        .enumerate()
                        .map(
                            |(i, m)| {
                                if m.primary { format!("{} · {}", i + 1, t.primary) } else { (i + 1).to_string() }
                            },
                        )
                        .collect();
                    let opts: Vec<(usize, Option<Icon>, &str)> =
                        names.iter().enumerate().map(|(i, n)| (i, None, n.as_str())).collect();
                    let mut at = self.monitors.iter().position(|m| m.device == self.settings.monitor).unwrap_or(0);
                    w::segmented(ui, &mut at, &opts);
                    if let Some(m) = self.monitors.get(at) {
                        self.settings.monitor = m.device.clone();
                    }
                });
                let mut on = self.autostart;
                w::switch(ui, &mut on, t.autostart);
                if on != self.autostart {
                    match sys::set_autostart(on) {
                        Ok(()) => self.autostart = on,
                        Err(e) => self.toasts.push(format!("{}: {e}", t.failed), Tone::Danger),
                    }
                }
                w::switch(ui, &mut self.settings.fullscreen_guard, t.fullscreen_guard);
                ui.horizontal(|ui| {
                    ui.label(t.show_delay);
                    ui.add(
                        egui::Slider::new(&mut self.settings.show_delay_ms, 0..=1000)
                            .step_by(50.0)
                            .suffix(format!(" {}", t.ms)),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(t.hide_delay);
                    ui.add(
                        egui::Slider::new(&mut self.settings.hide_delay_ms, 100..=3000)
                            .step_by(50.0)
                            .suffix(format!(" {}", t.ms)),
                    );
                });
                w::note(ui, t.resize_hint);
            });
        });

        let mut r = ui.new_child(UiBuilder::new().max_rect(right));
        w::card_frame(&r).show(&mut r, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("set-r").auto_shrink([false, false]).show(ui, |ui| {
                w::section_label(ui, tr(&ctx, "Оформление"));
                ui.horizontal(|ui| {
                    ui.label(tr(&ctx, "Тема"));
                    w::segmented(
                        ui,
                        &mut self.settings.common.theme,
                        &[
                            (ThemeChoice::System, Some(Icon::Monitor), tr(&ctx, "Система")),
                            (ThemeChoice::Light, Some(Icon::Sun), tr(&ctx, "Светлая")),
                            (ThemeChoice::Dark, Some(Icon::Moon), tr(&ctx, "Тёмная")),
                        ],
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(tr(&ctx, "Язык"));
                    let opts: Vec<(Lang, Option<Icon>, &str)> =
                        Lang::ALL.iter().map(|l| (*l, None, l.native_name())).collect();
                    w::segmented(ui, &mut self.settings.common.language, &opts);
                });
                ui.add_space(8.0);
                w::section_label(ui, t.hotkey);
                ui.horizontal(|ui| match self.hotkey {
                    Some(i) => {
                        for key in sys::HOTKEYS[i].0 {
                            w::kbd(ui, key);
                        }
                    }
                    None => {
                        w::badge(ui, t.hotkey_busy, Tone::Warning);
                    }
                });
                ui.add_space(8.0);
                w::mono(ui, &format!("Antresol {}", env!("CARGO_PKG_VERSION")), None);
            });
        });

        if self.settings != before {
            if self.settings.common != before.common {
                self.settings.common.apply(&ctx);
            }
            self.save();
        }
    }

    /// Ручки размера: нижний край, боковые края и нижние углы панели.
    fn grips(&mut self, ui: &mut Ui, panel: Rect, ppp: f32) {
        let b = panel.bottom();
        let zones = [
            (
                Grip::Bottom,
                Rect::from_min_max(Pos2::new(panel.left() + 16.0, b - GRIP), Pos2::new(panel.right() - 16.0, b + 2.0)),
                CursorIcon::ResizeVertical,
            ),
            (
                Grip::BottomLeft,
                Rect::from_min_max(Pos2::new(panel.left() - 2.0, b - 16.0), Pos2::new(panel.left() + 16.0, b + 2.0)),
                CursorIcon::ResizeNeSw,
            ),
            (
                Grip::BottomRight,
                Rect::from_min_max(Pos2::new(panel.right() - 16.0, b - 16.0), Pos2::new(panel.right() + 2.0, b + 2.0)),
                CursorIcon::ResizeNwSe,
            ),
            (
                Grip::Left,
                Rect::from_min_max(
                    Pos2::new(panel.left() - 2.0, panel.top() + HEADER),
                    Pos2::new(panel.left() + GRIP, b - 16.0),
                ),
                CursorIcon::ResizeHorizontal,
            ),
            (
                Grip::Right,
                Rect::from_min_max(
                    Pos2::new(panel.right() - GRIP, panel.top() + HEADER),
                    Pos2::new(panel.right() + 2.0, b - 16.0),
                ),
                CursorIcon::ResizeHorizontal,
            ),
        ];
        for (grip, rect, cursor) in zones {
            let r = ui.interact(rect, ui.id().with(("grip", grip as u8)), Sense::drag()).on_hover_cursor(cursor);
            if r.drag_started() {
                self.grip = Some(grip);
            }
            if r.dragged() && self.grip == Some(grip) {
                ui.ctx().set_cursor_icon(cursor);
                self.drag_resize(grip, ppp);
            }
            if r.drag_stopped() && self.grip == Some(grip) {
                self.grip = None;
                self.save();
            }
        }
        // Тонкая чёрточка посередине низа — видно, за что тянуть.
        let p = Palette::of(ui);
        let hot = self.grip.is_some() || ui.rect_contains_pointer(zones[0].1);
        let color = Motion::of(ui.ctx()).color(
            ui.ctx(),
            ui.id().with("grip-c"),
            if hot { p.accent } else { p.border_strong },
            motion::HOVER,
        );
        let c = Pos2::new(panel.center().x, b - 4.0);
        ui.painter().line_segment([c - Vec2::new(18.0, 0.0), c + Vec2::new(18.0, 0.0)], Stroke::new(3.0, color));
    }

    fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let t = self.drawn;
        if t <= 0.0 {
            return;
        }
        let p = Palette::of(ui);
        let panel = Rect::from_min_size(Pos2::ZERO, Vec2::new(self.settings.width, self.settings.height));
        let corner = CornerRadius { nw: 0, ne: 0, sw: CORNER, se: CORNER };
        ui.painter().rect(panel, corner, p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);

        let inner = Rect::from_min_max(panel.min + Vec2::new(12.0, 6.0), panel.max - Vec2::new(12.0, 14.0));
        let mut content = ui.new_child(UiBuilder::new().max_rect(inner));
        content.set_opacity(t);
        content.set_clip_rect(panel);
        let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER - 6.0));
        let mut top = content.new_child(UiBuilder::new().max_rect(header));
        self.header(&mut top);
        let body = Rect::from_min_max(Pos2::new(inner.left(), header.bottom() + 2.0), inner.max);
        let mut body_ui = content.new_child(UiBuilder::new().max_rect(body));
        let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
        match self.view {
            View::Cards => self.cards(&mut body_ui, &device),
            View::Settings => self.settings_view(&mut body_ui),
        }
        if t >= 1.0 {
            self.grips(ui, panel, ppp);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.grip.is_none() && self.capture.is_none() {
            self.edge.hide();
        }
        self.toasts.show(&ctx);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        anvil_ui::chrome::clear_color(visuals, ACCENT)
    }
}
