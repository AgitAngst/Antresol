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

use crate::config::{self, ALL_MONITORS, Hotkey, MAX_HEIGHT, MIN_HEIGHT, MIN_WIDTH, Settings};
use crate::modules::{self, Command, Ctx, Module};
use crate::panel::{self, Edge, Input};
use crate::sys::{self, monitor};
use crate::texts;

pub const ACCENT: Accent = Accent::BLUE;
/// Радиус нижних углов панели, в точках.
const CORNER: u8 = 14;
const HEADER: f32 = 40.0;
const GRIP: f32 = 7.0;
/// Полоска у края, пока панель спрятана: ширина и высота в точках.
const STRIP: Vec2 = Vec2::new(120.0, 5.0);
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
    autostart: bool,
    started: bool,
    /// Ждём сочетание для профиля с этим номером (настройки, «Изменить…»).
    capture: Option<usize>,
    /// Какие клавиши профилей последними отдали потоку клавиш — шлём заново, только если поменялись.
    sent_keys: Option<Vec<Option<String>>>,
    /// Ждём сочетание для показа панели (настройки, «Изменить…»).
    capture_panel: bool,
    /// Какую свою клавишу показа панели последней отдали потоку клавиш.
    sent_panel: Option<Option<String>>,
    /// «Все мониторы»: у какого монитора панель сейчас.
    active_monitor: Option<String>,
    /// Последняя программа на переднем плане (не сама панель) и когда смотрели.
    foreground: Option<String>,
    foreground_at: Option<Instant>,
    /// Показывать полоску у края в этом кадре.
    strip_on: bool,
    /// `ANTRESOL_SHOT`: снять своё окно в файл и закрыться.
    shot: Option<crate::shot::Shot>,
}

/// Сочетание в виде, который ждёт поток клавиш: модификаторы и клавиша.
fn parts(h: &Hotkey) -> (Vec<&str>, &str) {
    let keys = h.keys();
    (keys[..keys.len() - 1].to_vec(), h.key.as_str())
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = config::load();
        anvil_ui::install(&cc.egui_ctx, ACCENT, settings.common.theme);
        settings.common.apply(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let hotkey = sys::start_hotkey(settings.panel_hotkey.as_ref().map(parts), move || ctx.request_repaint());
        match hotkey {
            sys::PanelKey::Auto(Some(i)) => eprintln!("hotkey: {}", sys::HOTKEYS[i].0.join("+")),
            sys::PanelKey::Auto(None) => eprintln!("all hotkeys are taken by other apps"),
            sys::PanelKey::Custom(ok) => eprintln!("custom hotkey registered: {ok}"),
        }
        let sent_panel = Some(settings.panel_hotkey.as_ref().map(|h| h.to_string()));
        App {
            settings,
            // Для снимков: закрепить сразу, чтобы панель после показа при запуске не уезжала.
            edge: {
                let mut edge = Edge::default();
                edge.pinned = std::env::var_os("ANTRESOL_PIN").is_some() || std::env::var_os("ANTRESOL_SHOT").is_some();
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
            autostart: sys::autostart(),
            started: false,
            capture: None,
            sent_keys: None,
            capture_panel: false,
            sent_panel,
            active_monitor: None,
            foreground: None,
            foreground_at: None,
            strip_on: false,
            shot: crate::shot::Shot::from_env(),
        }
    }

    /// Монитор панели: из настроек, а если его нет (отключили) — основной. «Все мониторы» — тот,
    /// к краю которого подошла мышь.
    fn monitor(&self) -> Option<&monitor::Monitor> {
        let wanted = if self.settings.monitor == ALL_MONITORS {
            self.active_monitor.as_deref().unwrap_or("")
        } else {
            self.settings.monitor.as_str()
        };
        self.monitors.iter().find(|m| m.device == wanted).or_else(|| self.monitors.first())
    }

    /// Сочетание показа панели, которое сейчас работает: своё или первое свободное из готовых.
    fn panel_combo(&self) -> Option<String> {
        match sys::panel_hotkey() {
            sys::PanelKey::Custom(_) => self.settings.panel_hotkey.as_ref().map(|h| h.to_string()),
            sys::PanelKey::Auto(Some(i)) => Some(sys::HOTKEYS[i].0.join("+")),
            sys::PanelKey::Auto(None) => None,
        }
    }

    fn lang(&self) -> Lang {
        self.settings.common.language
    }

    /// Поставить окно у края монитора: `t` — доля выезда (0 — целиком выше края: полоска у края или
    /// увести прочь).
    /// Шлёт команды, только если что-то поменялось.
    fn place(&mut self, ctx: &egui::Context, ppp: f32, t: f32) {
        let Some(m) = self.monitor() else { return };
        let r = m.rect;
        let mut size = Vec2::new(self.settings.width, self.settings.height);
        let left = |w: f32| (r.left as f32 / ppp + (r.width() as f32 / ppp - w) / 2.0).round();
        let mut pos = Pos2::new(left(size.x), (r.top as f32 / ppp - (1.0 - t) * size.y).round());
        if t <= 0.0 && self.strip_on {
            size = STRIP;
            pos = Pos2::new(left(size.x), r.top as f32 / ppp);
        } else if t <= 0.0 {
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
        let radius = ((CORNER as f32 * ppp) as i32).min(px.1);
        if self.region != Some(px) && sys::round_bottom_corners(px.0, px.1, radius) {
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
        if let Some(shot) = &mut self.shot {
            shot.tick(ctx);
        }
        let ppp = ctx.pixels_per_point();
        let now = Instant::now();
        if self.monitors_at.is_none_or(|at| at.elapsed() > Duration::from_secs(2)) {
            self.monitors = monitor::monitors();
            self.monitors_at = Some(now);
        }
        for module in &mut self.modules {
            module.background(&self.settings);
        }
        self.take_dropped(ctx);
        let cursor = monitor::cursor();
        // «Все мониторы»: пока панель спрятана, она живёт у монитора под курсором.
        if self.settings.monitor == ALL_MONITORS
            && !self.edge.shown
            && self.drawn <= 0.0
            && let Some(m) = self.monitors.iter().find(|m| m.rect.contains(cursor))
        {
            self.active_monitor = Some(m.device.clone());
        }
        // Программа на переднем плане — раз в полсекунды, пока панель спрятана (сама панель не в счёт).
        if !self.edge.shown && self.foreground_at.is_none_or(|at| at.elapsed() > Duration::from_millis(500)) {
            match monitor::foreground_exe() {
                // Сама панель — не в счёт: остаётся программа, что была перед ней.
                Some(exe) if exe == "antresol.exe" => {}
                // Не узнали — не держим старое имя, иначе исключение сработает не по той программе.
                other => self.foreground = other,
            }
            self.foreground_at = Some(now);
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
        let at_edge = panel::at_edge(cursor, m.rect.top, left, right);
        let excluded = self.foreground.as_deref().is_some_and(|e| self.settings.excluded(e));
        let fullscreen = self.settings.fullscreen_guard && monitor::fullscreen_on(m.rect);
        self.strip_on = self.settings.strip && !excluded && !fullscreen;
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        let busy = self.grip.is_some()
            || self.capture.is_some()
            || self.capture_panel
            || self.modules.iter().any(|m| m.busy());
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
            // Игра во весь экран или программа из списка «не выезжать» — от края не выезжать.
            fullscreen: at_edge && (fullscreen || excluded),
            lost_focus: self.focused && !focused,
            focused,
        };
        self.focused = focused;
        let show_delay = Duration::from_millis(self.settings.show_delay_ms as u64);
        let hide_delay = Duration::from_millis(self.settings.hide_delay_ms as u64);
        let changed = self.edge.step(input, now, show_delay, hide_delay);
        let changed = self.profile_keys(ctx, &m.device, now) || changed;
        // Пока ждём новое сочетание показа панели, текущее снято — иначе его нажатие спрятало бы панель.
        sys::suspend_panel_hotkey(self.capture_panel);
        let panel_key = self.settings.panel_hotkey.as_ref().map(|h| h.to_string());
        if self.sent_panel.as_ref() != Some(&panel_key) {
            sys::set_panel_hotkey(self.settings.panel_hotkey.as_ref().map(parts));
            self.sent_panel = Some(panel_key);
        }
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
        // Ждём клавишу панели — тоже снимаем: иначе сочетание профиля сработало бы вместо захвата.
        let capturing = self.capture.is_some() || self.capture_panel;
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
    /// Файлы бросили на панель — отдать лотку.
    fn take_dropped(&mut self, ctx: &egui::Context) {
        let files: Vec<String> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().display().to_string()).filter(|p| !p.is_empty()).collect()
        });
        if files.is_empty() {
            return;
        }
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
        let mut toasts = Vec::new();
        let cmd = Command::Dropped(files);
        for module in &mut self.modules {
            let mut cx = Ctx { t, lang, monitor: &device, toasts: &mut toasts, settings: &mut self.settings };
            module.command(&cmd, &mut cx);
        }
        for (text, tone) in toasts {
            self.toasts.push(text, tone);
        }
        self.save();
    }

    /// Ждём сочетание для показа панели: то же правило, что у профилей; сочетание профиля — нельзя.
    fn capture_panel_key(&mut self, ui: &Ui) {
        if !self.capture_panel {
            return;
        }
        let events = ui.input(|inp| inp.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            if key == egui::Key::Escape {
                self.capture_panel = false;
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
                let lang = self.lang();
                if let Some(p) = self.settings.profiles.iter().find(|p| p.hotkey.as_ref() == Some(&hotkey)) {
                    self.toasts.push(texts::key_is_profile(lang, &p.name), Tone::Warning);
                    return;
                }
                self.settings.panel_hotkey = Some(hotkey);
                self.capture_panel = false;
                return;
            }
        }
    }

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
                let panel = self.panel_combo();
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

    /// «Не выезжать над программами»: список с крестиками и «Добавить» последнюю программу на
    /// переднем плане.
    fn exclude_ui(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        ui.add_space(6.0);
        w::section_label(ui, t.exclude_title);
        if self.settings.exclude.is_empty() {
            w::note(ui, t.exclude_none);
        }
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for (i, exe) in self.settings.exclude.iter().enumerate() {
                if w::button(ui, Kind::Secondary, Some(Icon::Close), exe).on_hover_text(t.exclude_remove).clicked() {
                    remove = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            self.settings.exclude.remove(i);
        }
        if let Some(exe) = self.foreground.clone()
            && !self.settings.excluded(&exe)
            && w::button(ui, Kind::Ghost, Some(Icon::Plus), &texts::exclude_add(lang, &exe)).clicked()
        {
            self.settings.exclude.push(exe);
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

    /// Карточки в ряд по долям ширины; узкая панель — у каждой своя наименьшая ширина, а ряд
    /// листается вбок (колесо мыши с Shift или полоса снизу).
    fn cards(&mut self, ui: &mut Ui, device: &str) {
        let area = ui.available_rect_before_wrap();
        let gap = 10.0;
        let total: f32 = self.modules.iter().map(|m| m.weight()).sum();
        let free = area.width() - gap * (self.modules.len().saturating_sub(1)) as f32;
        let widths: Vec<f32> = self.modules.iter().map(|m| (free * m.weight() / total).max(m.min_width())).collect();
        let content = widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1)) as f32;
        if content <= area.width() + 0.5 {
            self.card_row(ui, area.min, &widths, area.height(), device);
            return;
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        egui::ScrollArea::horizontal().id_salt("cards").auto_shrink([false, false]).show(&mut child, |ui| {
            // Под полосу прокрутки — 12 снизу.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(content, area.height() - 12.0), Sense::hover());
            self.card_row(ui, rect.min, &widths, rect.height(), device);
        });
    }

    fn card_row(&mut self, ui: &mut Ui, origin: Pos2, widths: &[f32], height: f32, device: &str) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let gap = 10.0;
        let mut x = origin.x;
        let mut toasts = Vec::new();
        let before = self.settings.clone();
        let settings = &mut self.settings;
        for (i, module) in self.modules.iter_mut().enumerate() {
            let width = widths[i];
            let rect = Rect::from_min_size(Pos2::new(x, origin.y), Vec2::new(width, height));
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
                    // 0 — «Все», дальше — мониторы по порядку.
                    let mut opts: Vec<(usize, Option<Icon>, &str)> = vec![(0, None, t.all_monitors)];
                    opts.extend(names.iter().enumerate().map(|(i, n)| (i + 1, None, n.as_str())));
                    let mut at = if self.settings.monitor == ALL_MONITORS {
                        0
                    } else {
                        self.monitors.iter().position(|m| m.device == self.settings.monitor).unwrap_or(0) + 1
                    };
                    w::segmented(ui, &mut at, &opts);
                    if at == 0 {
                        // Перешли на «Все» при открытой панели — пусть остаётся там, где стоит.
                        if self.settings.monitor != ALL_MONITORS {
                            self.active_monitor = self.monitor().map(|m| m.device.clone());
                        }
                        self.settings.monitor = ALL_MONITORS.to_string();
                    } else if let Some(m) = self.monitors.get(at - 1) {
                        self.settings.monitor = m.device.clone();
                    }
                });
                if self.settings.monitor == ALL_MONITORS {
                    w::note(ui, t.all_monitors_note);
                }
                let mut on = self.autostart;
                w::switch(ui, &mut on, t.autostart);
                if on != self.autostart {
                    match sys::set_autostart(on) {
                        Ok(()) => self.autostart = on,
                        Err(e) => self.toasts.push(format!("{}: {e}", t.failed), Tone::Danger),
                    }
                }
                w::switch(ui, &mut self.settings.fullscreen_guard, t.fullscreen_guard);
                w::switch(ui, &mut self.settings.strip, t.strip);
                w::switch(ui, &mut self.settings.clipboard, t.clipboard_remember);
                self.exclude_ui(ui);
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
                self.capture_panel_key(ui);
                if self.capture_panel {
                    ui.horizontal_wrapped(|ui| {
                        w::badge(ui, t.press_keys, Tone::Accent);
                        if w::button(ui, Kind::Ghost, None, t.cancel).clicked() {
                            self.capture_panel = false;
                        }
                    });
                } else {
                    let combo = self.panel_combo();
                    let keys: Vec<&str> = match &combo {
                        Some(c) => c.split('+').collect(),
                        None => vec![t.no_key],
                    };
                    let status = match sys::panel_hotkey() {
                        sys::PanelKey::Custom(true) => Some((true, t.key_free)),
                        sys::PanelKey::Custom(false) => Some((false, t.key_busy)),
                        sys::PanelKey::Auto(None) => Some((false, t.hotkey_busy)),
                        sys::PanelKey::Auto(Some(_)) => None,
                    };
                    if w::hotkey_field(ui, &keys, t.change, status).clicked() {
                        self.capture_panel = true;
                    }
                    if self.settings.panel_hotkey.is_some() && w::button(ui, Kind::Ghost, None, t.key_auto).clicked() {
                        self.settings.panel_hotkey = None;
                    }
                }
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
            if self.strip_on {
                // Полоска у края; мышь у края — набирает акцент: панель вот-вот выедет.
                let p = Palette::of(ui);
                let target = if self.edge.counting() { p.accent } else { p.border_strong };
                let color = Motion::of(&ctx).color(&ctx, egui::Id::new("strip"), target, motion::HOVER);
                let rect = Rect::from_min_size(Pos2::ZERO, STRIP);
                ui.painter().rect_filled(rect, CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 }, color);
            }
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
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.grip.is_none()
            && self.capture.is_none()
            && !self.capture_panel
        {
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
