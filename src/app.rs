// Окно панели — это и есть панель: того же размера, у верхнего края монитора. Выезд — окно
// само съезжает из-за края вниз (часть выше края экрана не видна). Нижние углы скругляет регион
// окна (SetWindowRgn), а не прозрачность: прозрачность окна OpenGL на Windows ненадёжна — на части
// систем прозрачное рисуется белым, а с «пропускать мышь» (WS_EX_LAYERED) белеет всё окно.
// Спрятанное окно уводим правее всех мониторов.

use std::time::{Duration, Instant};

use anvil_ui::motion::{self, Motion};
use anvil_ui::widgets::{self as w, Kind, Toasts};
use anvil_ui::{Icon, Lang, Palette, ThemeChoice, Tone, tr};
use eframe::egui::{self, CornerRadius, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

use crate::config::{self, ALL_MONITORS, Hotkey, MAX_HEIGHT, MIN_HEIGHT, MIN_WIDTH, Settings};
use crate::modules::{self, Command, Ctx, Module};
use crate::panel::{self, Edge, Input};
use crate::sys::{self, monitor};
use crate::{look, texts};

/// Радиус нижних углов панели, в точках.
const CORNER: u8 = 14;
const HEADER: f32 = 48.0;
const GRIP: f32 = 7.0;
/// Зазор между карточками.
const GAP: f32 = 10.0;
/// Задержка выезда в настройках — чипами, мс.
const DELAYS: [u32; 4] = [0, 150, 300, 600];
/// Полоска у края, пока панель спрятана: ширина и высота в точках.
const STRIP: Vec2 = Vec2::new(120.0, 5.0);
/// Полоска, пока мышь у края и идёт отсчёт выезда: шире и наполняется.
const STRIP_HOT: Vec2 = Vec2::new(160.0, 6.0);
/// Полоска с медиа: обложка, трек и три кнопки.
const MEDIA_STRIP: Vec2 = Vec2::new(372.0, 34.0);
/// Плашка у края: таймер дошёл, пока панель спрятана.
const BANNER: Vec2 = Vec2::new(320.0, 48.0);
/// Как часто перечитывать систему, пока панель открыта.
const REFRESH: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq)]
enum View {
    Cards,
    Settings,
    /// «Свой цвет…» — вместо настроек.
    Color,
}

/// Что видно у края, пока панель спрятана.
#[derive(Clone, Copy, PartialEq, Debug)]
enum StripKind {
    /// Ничего: окно уведено прочь.
    Off,
    Plain,
    Media,
    Banner,
}

/// «Свой цвет…»: что выбрано сейчас и что в поле кода.
struct ColorEdit {
    h: f32,
    s: f32,
    v: f32,
    code: String,
}

impl ColorEdit {
    fn new(c: egui::Color32) -> Self {
        let (h, s, v) = look::to_hsv(c);
        ColorEdit { h, s, v, code: look::hex(c) }
    }

    fn color(&self) -> egui::Color32 {
        look::hsv(self.h, self.s, self.v)
    }

    fn set(&mut self, c: egui::Color32) {
        *self = ColorEdit::new(c);
    }
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
    /// Что у края, пока панель спрятана.
    strip_kind: StripKind,
    /// Отсчёт выезда идёт: какая доля задержки прошла.
    strip_fill: Option<f32>,
    /// Что играет — для полоски с медиа.
    playing: Option<modules::Playing>,
    /// Плашка таймера у края.
    banner: Option<String>,
    /// Вводят имя нового профиля (шапка, «+»).
    naming: Option<String>,
    /// «Ещё N»: куда пролистать ряд карточек в следующем кадре.
    scroll_to: Option<f32>,
    /// «Свой цвет…».
    color: ColorEdit,
    /// Что последним поставили окну: не забирать фокус при щелчке (пока панель спрятана).
    no_activate: Option<bool>,
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
        anvil_ui::install(&cc.egui_ctx, look::accent_of(&settings.accent), settings.common.theme);
        settings.common.apply(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let hotkey = sys::start_hotkey(settings.panel_hotkey.as_ref().map(parts), move || ctx.request_repaint());
        match hotkey {
            sys::PanelKey::Auto(Some(i)) => eprintln!("hotkey: {}", sys::HOTKEYS[i].0.join("+")),
            sys::PanelKey::Auto(None) => eprintln!("all hotkeys are taken by other apps"),
            sys::PanelKey::Custom(ok) => eprintln!("custom hotkey registered: {ok}"),
        }
        let sent_panel = Some(settings.panel_hotkey.as_ref().map(|h| h.to_string()));
        let strip_view = std::env::var("ANTRESOL_VIEW").ok().filter(|v| v.starts_with("strip-"));
        let mut settings = settings;
        if strip_view.as_deref() == Some("strip-media") {
            settings.media_strip = true;
        }
        let mut app = App {
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
            view: match std::env::var("ANTRESOL_VIEW").as_deref() {
                Ok("settings") => View::Settings,
                Ok("color") => View::Color,
                _ => View::Cards,
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
            strip_kind: StripKind::Off,
            strip_fill: None,
            playing: None,
            banner: None,
            naming: None,
            scroll_to: None,
            color: ColorEdit::new(look::LIME.dark.fill),
            no_activate: None,
            shot: crate::shot::Shot::from_env(),
        };
        if let Some(v) = strip_view {
            app.edge.pinned = false;
            app.started = true;
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            if v == "strip-banner" {
                app.banner = Some(texts::tr(app.lang()).timer_done.to_string());
            }
        }
        if app.view == View::Color {
            app.open_color(&cc.egui_ctx);
        }
        app
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
        if t <= 0.0 && self.strip_kind != StripKind::Off {
            size = self.strip_size();
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
        let corner = match self.strip_kind {
            _ if t > 0.0 => CORNER,
            StripKind::Media => 10,
            StripKind::Banner => 12,
            _ => 3,
        };
        let radius = ((corner as f32 * ppp) as i32).min(px.1);
        if self.region != Some(px) && sys::round_bottom_corners(px.0, px.1, radius) {
            self.region = Some(px);
        }
    }

    /// Дать окну фокус (панель выехала): сначала снять «не забирать фокус», иначе Windows не активирует окно.
    fn focus(&mut self, ctx: &egui::Context) {
        self.set_no_activate(false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Спрятанная панель — окно не забирает фокус при щелчке; выехала — забирает. Шлёт, только если поменялось.
    fn set_no_activate(&mut self, on: bool) {
        if self.no_activate != Some(on) && sys::set_no_activate(on) {
            self.no_activate = Some(on);
        }
    }

    fn strip_size(&self) -> Vec2 {
        match self.strip_kind {
            StripKind::Media => MEDIA_STRIP,
            StripKind::Banner => BANNER,
            _ if self.strip_fill.is_some() => STRIP_HOT,
            _ => STRIP,
        }
    }

    /// Отдать команду всем модулям; их уведомления — на экран, изменённые настройки — в файл.
    fn command_all(&mut self, cmd: Command) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
        let mut toasts = Vec::new();
        let before = self.settings.clone();
        for module in &mut self.modules {
            let mut cx = Ctx { t, lang, monitor: &device, toasts: &mut toasts, settings: &mut self.settings };
            module.command(&cmd, &mut cx);
        }
        for (text, tone) in toasts {
            self.toasts.push(text, tone);
        }
        if self.settings != before {
            self.save();
        }
    }

    /// Открыть «Свой цвет…» с тем цветом, что стоит сейчас.
    fn open_color(&mut self, ctx: &egui::Context) {
        let base = look::parse_hex(&self.settings.accent).unwrap_or_else(|| {
            look::accent_of(&self.settings.accent).swatch(ctx.global_style().visuals.dark_mode).fill
        });
        self.color.set(base);
        self.view = View::Color;
    }

    /// Вернуть цвет из настроек (ушли из «Своего цвета» без «Применить»).
    fn restore_accent(&self, ctx: &egui::Context) {
        anvil_ui::set_accent(ctx, look::accent_of(&self.settings.accent));
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
        let t = texts::tr(self.lang());
        let mut calls = Vec::new();
        for module in &mut self.modules {
            module.background(&self.settings);
            if let Some(msg) = module.attention(t) {
                calls.push(msg);
            }
        }
        // Модуль зовёт (таймер дошёл) — уведомление и выезд, как по клавише.
        if !calls.is_empty() && !self.edge.shown && self.strip_on {
            self.banner = Some(calls.join(" · "));
        } else if !calls.is_empty() {
            for msg in calls {
                self.toasts.push(msg, Tone::Accent);
            }
            if !self.edge.shown {
                self.edge.reveal(Instant::now());
                self.refreshed = None;
                self.focus(ctx);
            }
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
            self.focus(ctx);
        }
        if changed && !self.edge.shown {
            if self.view == View::Color {
                self.restore_accent(ctx);
            }
            self.view = View::Cards;
            self.naming = None;
        }
        if self.edge.shown {
            self.banner = None;
        }
        self.strip_fill = self.edge.edge_progress(now, show_delay);
        self.playing = if self.settings.media_strip && self.strip_on && !self.edge.shown {
            self.modules.iter_mut().find_map(|m| m.playing(ctx))
        } else {
            None
        };
        self.strip_kind = if !self.strip_on {
            StripKind::Off
        } else if self.banner.is_some() {
            StripKind::Banner
        } else if self.playing.is_some() && self.strip_fill.is_none() {
            StripKind::Media
        } else {
            StripKind::Plain
        };

        if self.edge.shown && self.refreshed.is_none_or(|at| at.elapsed() > REFRESH) {
            // Выключенные карточки систему не опрашивают (их `background` идёт и так).
            for i in self.visible_cards() {
                self.modules[i].refresh(&m.device);
            }
            self.refreshed = Some(now);
        }

        let t = Motion::of(ctx).toggle(ctx, egui::Id::new("panel-shown"), self.edge.shown, motion::ENTER);
        self.drawn = t;
        self.set_no_activate(!self.edge.shown);
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
        if self.view == View::Color {
            self.restore_accent(ctx);
        }
        self.view = View::Cards;
        if !was {
            self.refreshed = None;
            self.focus(ctx);
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

    /// «Карточки панели»: включить и выключить, сдвинуть выше или ниже. Порядок пишется целиком.
    fn cards_card(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        w::section_label(ui, t.cards_title);
        let keys: Vec<&'static str> = self.modules.iter().map(|m| m.key()).collect();
        let arranged = config::arrange(&keys, &self.settings.order, &self.settings.hidden);
        let mut order: Vec<String> = arranged.iter().map(|(i, _)| keys[*i].to_string()).collect();
        let mut moved = false;
        let p = Palette::of(ui);
        for (pos, (i, on)) in arranged.iter().enumerate() {
            let title = self.modules[*i].title(t);
            let key = keys[*i];
            let row = Vec2::new(ui.available_width(), 32.0);
            let (rect, row_r) = ui.allocate_exact_size(row, Sense::hover());
            // Ручка слева: тянуть мышью; в фокусе — Alt + стрелки.
            let handle = Rect::from_min_size(rect.min + Vec2::new(0.0, 4.0), Vec2::new(22.0, 24.0));
            let hr = ui.interact(handle, ui.id().with(("card-drag", key)), Sense::click_and_drag());
            hr.dnd_set_drag_payload(pos);
            let hot = hr.hovered() || hr.dragged() || hr.has_focus();
            if hot {
                ui.painter().rect_filled(handle, 4, p.hover);
            }
            paint_grip(ui.painter(), handle.center(), if hot { p.text } else { p.weak });
            w::focus_ring(ui, handle, &hr, 4);
            let hr = hr.on_hover_text(t.drag).on_hover_cursor(CursorIcon::Grab);
            if hr.has_focus() {
                let (up, down) = ui.input(|i| {
                    (
                        i.modifiers.alt && i.key_pressed(egui::Key::ArrowUp),
                        i.modifiers.alt && i.key_pressed(egui::Key::ArrowDown),
                    )
                });
                if up && pos > 0 {
                    order.swap(pos, pos - 1);
                    moved = true;
                } else if down && pos + 1 < arranged.len() {
                    order.swap(pos, pos + 1);
                    moved = true;
                }
            }
            // Сюда бросили другую строку — она встаёт на это место.
            if let Some(from) = row_r.dnd_hover_payload::<usize>()
                && *from != pos
            {
                let y = if *from < pos { rect.bottom() } else { rect.top() };
                ui.painter()
                    .line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(2.0, p.accent));
            }
            if let Some(from) = row_r.dnd_release_payload::<usize>()
                && *from != pos
            {
                let k = order.remove(*from);
                order.insert(pos, k);
                moved = true;
            }
            let mut v = *on;
            let switch_rect = Rect::from_min_max(Pos2::new(handle.right() + 8.0, rect.top()), rect.max);
            let mut sw = ui.new_child(
                UiBuilder::new().max_rect(switch_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            w::switch(&mut sw, &mut v, title);
            if v != *on {
                if v {
                    self.settings.hidden.retain(|h| h != key);
                } else {
                    self.settings.hidden.push(key.to_string());
                }
            }
        }
        if moved {
            self.settings.order = order;
        }
        w::note(ui, t.cards_note);
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
        let ctx = ui.ctx().clone();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            match self.view {
                View::Cards => {
                    anvil_ui::chrome::app_mark(ui, Icon::Layers, 24.0);
                    ui.label(egui::RichText::new("Antresol").font(anvil_ui::semibold(14.5)).color(p.text));
                    if !self.settings.hidden.iter().any(|h| h == "display") {
                        vline(ui, p.border);
                        self.profiles_bar(ui);
                    }
                }
                View::Settings => {
                    if w::button(ui, Kind::Ghost, Some(Icon::ArrowLeft), t.settings).on_hover_text(t.back).clicked() {
                        self.view = View::Cards;
                    }
                }
                View::Color => {
                    if w::button(ui, Kind::Ghost, Some(Icon::ArrowLeft), t.settings).on_hover_text(t.back).clicked() {
                        self.restore_accent(&ctx);
                        self.view = View::Settings;
                    }
                    ui.label(egui::RichText::new("/").color(p.weak));
                    ui.label(
                        egui::RichText::new(t.custom_color.trim_end_matches('…'))
                            .font(anvil_ui::semibold(14.5))
                            .color(p.text),
                    );
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if w::icon_button(ui, Icon::ChevronUp, t.hide).clicked() {
                    if self.view == View::Color {
                        self.restore_accent(&ctx);
                    }
                    self.edge.hide();
                }
                if self.view != View::Cards {
                    ui.add_space(6.0);
                    w::mono(ui, &format!("Antresol {}", env!("CARGO_PKG_VERSION")), Some(p.weak));
                    return;
                }
                if w::icon_button(ui, Icon::Gear, t.settings).clicked() {
                    self.view = View::Settings;
                }
                let pinned = self.edge.pinned;
                let r = w::icon_button(ui, Icon::Pin, if pinned { t.unpin } else { t.pin });
                if pinned {
                    // Закреплена — значок на мягкой подложке акцента.
                    ui.painter().rect_filled(r.rect, 6, p.badge_fill(p.accent));
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
                if w::icon_button(ui, Icon::Tiles, t.edit_cards).clicked() {
                    self.view = View::Settings;
                }
                ui.add_space(6.0);
                vline(ui, p.border);
                ui.add_space(6.0);
                if self.monitors.len() > 1 {
                    self.monitor_picker(ui);
                    ui.label(egui::RichText::new(t.monitor).size(12.0).color(p.weak));
                }
            });
        });
    }

    /// Профили экрана чипами в шапке, «+» — сохранить то, что стоит сейчас.
    fn profiles_bar(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        ui.label(egui::RichText::new(t.profile).size(12.0).color(p.weak));
        if let Some(mut name) = self.naming.take() {
            let mut done = None;
            let edit = ui.add(egui::TextEdit::singleline(&mut name).hint_text(t.profile_name).desired_width(140.0));
            if name.is_empty() && !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if w::button(ui, Kind::Secondary, Some(Icon::Save), t.save).clicked() || enter {
                done = Some(true);
            }
            if w::button(ui, Kind::Ghost, None, t.cancel).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = Some(false);
            }
            match done {
                Some(true) => self.command_all(Command::SaveProfile(name)),
                Some(false) => {}
                None => self.naming = Some(name),
            }
            return;
        }
        let active = self.modules.iter().find_map(|m| m.active_profile(&self.settings.profiles));
        let mut apply = None;
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, profile) in self.settings.profiles.iter().enumerate() {
            let on = active == Some(i);
            let mut r = modules::chip(ui, ("profile", i), on, &profile.name, None);
            if let Some(h) = &profile.hotkey {
                r = r.on_hover_text(h.to_string());
            }
            if r.clicked() && !on {
                apply = Some(i);
            }
        }
        if w::icon_button(ui, Icon::Plus, t.profile_save).clicked() {
            self.naming = Some(String::new());
        }
        if let Some(i) = apply {
            self.command_all(Command::ApplyProfile(i));
        }
    }

    /// Монитор панели: «Все» и номера мониторов (в шапке, справа).
    fn monitor_picker(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let names: Vec<String> = (1..=self.monitors.len()).map(|i| i.to_string()).collect();
        // 0 — «Все», дальше — мониторы по порядку; справа налево — с конца.
        let mut opts: Vec<(usize, Option<Icon>, &str)> = vec![(0, None, t.all_monitors)];
        opts.extend(names.iter().enumerate().map(|(i, n)| (i + 1, None, n.as_str())));
        opts.reverse();
        let mut at = if self.settings.monitor == ALL_MONITORS {
            0
        } else {
            self.monitors.iter().position(|m| m.device == self.settings.monitor).unwrap_or(0) + 1
        };
        let before = at;
        w::segmented(ui, &mut at, &opts).on_hover_text(t.all_monitors_note);
        if at == before {
            return;
        }
        if at == 0 {
            // Перешли на «Все» при открытой панели — пусть остаётся там, где стоит.
            self.active_monitor = self.monitor().map(|m| m.device.clone());
            self.settings.monitor = ALL_MONITORS.to_string();
        } else if let Some(m) = self.monitors.get(at - 1) {
            self.settings.monitor = m.device.clone();
        }
        self.save();
    }

    /// Карточки в порядке из настроек, включённые: номера в `self.modules`.
    fn visible_cards(&self) -> Vec<usize> {
        let keys: Vec<&str> = self.modules.iter().map(|m| m.key()).collect();
        config::arrange(&keys, &self.settings.order, &self.settings.hidden)
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(i, _)| i)
            .collect()
    }

    /// Карточки в ряд по долям ширины; узкая панель — у каждой своя наименьшая ширина, а ряд
    /// листается вбок (колесо мыши с Shift или полоса снизу).
    fn cards(&mut self, ui: &mut Ui, device: &str) {
        let area = ui.available_rect_before_wrap();
        let gap = GAP;
        let visible = self.visible_cards();
        if visible.is_empty() {
            let t = texts::tr(self.lang());
            w::note(ui, t.no_cards);
            return;
        }
        let total: f32 = visible.iter().map(|&i| self.modules[i].weight()).sum();
        let free = area.width() - gap * (visible.len().saturating_sub(1)) as f32;
        let widths: Vec<f32> = visible
            .iter()
            .map(|&i| (free * self.modules[i].weight() / total).max(self.modules[i].min_width()))
            .collect();
        let content = widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1)) as f32;
        if content <= area.width() + 0.5 {
            self.card_row(ui, area.min, &visible, &widths, area.height(), device);
            return;
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        let mut scroll = egui::ScrollArea::horizontal().id_salt("cards").auto_shrink([false, false]);
        if let Some(x) = self.scroll_to.take() {
            scroll = scroll.horizontal_scroll_offset(x);
        }
        let out = scroll.show(&mut child, |ui| {
            // Под полосу прокрутки — 12 снизу.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(content, area.height() - 12.0), Sense::hover());
            self.card_row(ui, rect.min, &visible, &widths, rect.height(), device);
        });
        // Сколько карточек справа не видно целиком: затухание и «Ещё N ›».
        let offset = out.state.offset.x;
        let view_right = offset + area.width();
        let mut x = 0.0;
        let mut first_hidden = None;
        let mut hidden = 0;
        for w in &widths {
            if x + w > view_right + 1.0 {
                hidden += 1;
                first_hidden.get_or_insert(x);
            }
            x += w + gap;
        }
        let Some(target) = first_hidden else { return };
        let p = Palette::of(ui);
        let fade = Rect::from_min_max(
            Pos2::new(area.right() - 120.0, area.top()),
            Pos2::new(area.right(), area.bottom() - 12.0),
        );
        fade_right(ui.painter(), fade, p.surface);
        let t = texts::tr(self.lang());
        let label = texts::more_cards(self.lang(), hidden);
        let font = egui::FontId::proportional(13.0);
        let text_w = ui.painter().layout_no_wrap(label.clone(), font.clone(), p.text).size().x;
        let size = Vec2::new(text_w + 34.0, 32.0);
        let btn = Rect::from_min_size(Pos2::new(area.right() - size.x - 2.0, fade.center().y - size.y / 2.0), size);
        let r = ui.interact(btn, ui.id().with("more-cards"), Sense::click());
        let fill = if r.hovered() { p.hover } else { p.card };
        ui.painter().rect(btn, 16, fill, Stroke::new(1.0, p.border_strong), egui::StrokeKind::Inside);
        ui.painter().text(btn.left_center() + Vec2::new(12.0, 0.0), egui::Align2::LEFT_CENTER, &label, font, p.text);
        anvil_ui::icons::paint(
            ui.painter(),
            Rect::from_center_size(Pos2::new(btn.right() - 14.0, btn.center().y), Vec2::splat(14.0)),
            Icon::ArrowRight,
            p.text,
        );
        w::focus_ring(ui, btn, &r, 16);
        if r.on_hover_text(t.more_hint).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.scroll_to = Some(target.min(content - area.width()).max(0.0));
        }
    }

    fn card_row(&mut self, ui: &mut Ui, origin: Pos2, visible: &[usize], widths: &[f32], height: f32, device: &str) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let gap = 10.0;
        let mut x = origin.x;
        let mut toasts = Vec::new();
        let before = self.settings.clone();
        let settings = &mut self.settings;
        for (i, &m) in visible.iter().enumerate() {
            let module = &mut self.modules[m];
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
                modules::card_header(ui, modules::icon_of(module.key()), module.title(t), &module.meta());
                ui.add_space(8.0);
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

    /// Настройки — четыре карточки; уже 1240 точек — листаются вбок, а не сжимаются.
    fn settings_view(&mut self, ui: &mut Ui) {
        const MIN: f32 = 1240.0;
        let area = ui.available_rect_before_wrap();
        if area.width() >= MIN {
            self.settings_columns(ui, area);
            return;
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        egui::ScrollArea::horizontal().id_salt("settings").auto_shrink([false, false]).show(&mut child, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(MIN, area.height() - 12.0), Sense::hover());
            self.settings_columns(ui, rect);
        });
    }

    fn settings_columns(&mut self, ui: &mut Ui, area: Rect) {
        let t = texts::tr(self.lang());
        let ctx = ui.ctx().clone();
        let before = self.settings.clone();
        // Четыре карточки: «Панель», «Профили экрана», «Карточки панели», «Оформление».
        let quarter = (area.width() - 30.0) / 4.0;
        let col = |i: f32| {
            Rect::from_min_size(
                Pos2::new(area.left() + i * (quarter + 10.0), area.top()),
                Vec2::new(quarter, area.height()),
            )
        };
        let (left, middle, third, right) = (col(0.0), col(1.0), col(2.0), col(3.0));

        let mut c3 = ui.new_child(UiBuilder::new().max_rect(third));
        w::card_frame(&c3).show(&mut c3, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("set-c").auto_shrink([false, false]).show(ui, |ui| {
                self.cards_card(ui);
            });
        });

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
                ui.add_enabled_ui(self.settings.strip, |ui| {
                    w::switch(ui, &mut self.settings.media_strip, t.media_strip);
                });
                w::switch(ui, &mut self.settings.clipboard, t.clipboard_remember);
                self.exclude_ui(ui);
                ui.horizontal(|ui| {
                    ui.label(t.show_delay);
                    let labels: Vec<String> = DELAYS.iter().map(|d| d.to_string()).collect();
                    let opts: Vec<(u32, &str)> = DELAYS.iter().zip(&labels).map(|(d, l)| (*d, l.as_str())).collect();
                    modules::chips(ui, &mut self.settings.show_delay_ms, &opts);
                    ui.label(egui::RichText::new(t.ms).color(Palette::of(ui).weak));
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
                ui.add_space(6.0);
                if w::button(ui, Kind::Ghost, Some(Icon::Close), t.quit).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        let mut r = ui.new_child(UiBuilder::new().max_rect(right));
        w::card_frame(&r).show(&mut r, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("set-r").auto_shrink([false, false]).show(ui, |ui| {
                w::section_label(ui, t.appearance);
                setting_row(ui, tr(&ctx, "Тема"), |ui| {
                    w::segmented(
                        ui,
                        &mut self.settings.common.theme,
                        &[
                            (ThemeChoice::Dark, Some(Icon::Moon), tr(&ctx, "Тёмная")),
                            (ThemeChoice::Light, Some(Icon::Sun), tr(&ctx, "Светлая")),
                            (ThemeChoice::System, Some(Icon::Monitor), tr(&ctx, "Система")),
                        ],
                    );
                });
                setting_row(ui, tr(&ctx, "Язык"), |ui| {
                    let opts: Vec<(Lang, Option<Icon>, &str)> =
                        Lang::ALL.iter().rev().map(|l| (*l, None, l.native_name())).collect();
                    w::segmented(ui, &mut self.settings.common.language, &opts);
                });
                let mut open = false;
                setting_row(ui, t.color, |ui| {
                    open = self.swatches(ui);
                });
                if open {
                    self.open_color(&ctx);
                }
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
            });
        });

        if self.settings != before {
            if self.settings.common != before.common {
                self.settings.common.apply(&ctx);
            }
            self.save();
        }
    }

    /// Пять образцов и «+» (свой цвет). Возвращает: открыть «Свой цвет…».
    fn swatches(&mut self, ui: &mut Ui) -> bool {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let ctx = ui.ctx().clone();
        let mut open = false;
        ui.spacing_mut().item_spacing.x = 6.0;
        // Справа налево: «+», потом образцы с конца.
        let custom = look::parse_hex(&self.settings.accent);
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
        match custom {
            Some(c) => {
                ui.painter().circle_filled(rect.center(), 10.0, c);
                ui.painter().circle_stroke(rect.center(), 12.0, Stroke::new(2.0, p.text));
            }
            None => {
                ui.painter().circle_stroke(rect.center(), 10.0, Stroke::new(1.0, p.border_strong));
                anvil_ui::icons::paint(ui.painter(), rect.shrink(7.0), Icon::Plus, p.weak);
            }
        }
        w::focus_ring(ui, rect, &r, 12);
        if r.on_hover_text(t.custom_color).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            open = true;
        }
        for (i, a) in look::PRESETS.iter().enumerate().rev() {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
            let on = self.settings.accent == a.name;
            ui.painter().circle_filled(rect.center(), 10.0, a.swatch(p.dark).fill);
            if on {
                ui.painter().circle_stroke(rect.center(), 12.0, Stroke::new(2.0, p.text));
            }
            w::focus_ring(ui, rect, &r, 12);
            let r = r.on_hover_text(t.accents[i]).on_hover_cursor(CursorIcon::PointingHand);
            r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, t.accents[i]));
            if r.clicked() && !on {
                self.settings.accent = a.name.to_string();
                anvil_ui::set_accent(&ctx, *a);
            }
        }
        open
    }

    /// «Свой цвет…»: поле насыщенности и яркости, оттенок, код, готовые, цвет Windows; справа —
    /// образцы в обеих темах с контрастом. Панель перекрашивается сразу; «Отмена» — вернуть.
    fn color_view(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let ctx = ui.ctx().clone();
        let area = ui.available_rect_before_wrap();
        let before = self.color.color();
        let mut card = ui.new_child(UiBuilder::new().max_rect(area));
        w::card_frame(&card).show(&mut card, |ui| {
            ui.set_min_size(ui.available_size());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 24.0;
                // Поле и оттенок.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    let field = Vec2::new(236.0, (ui.available_height() - 50.0).clamp(80.0, 168.0));
                    let (rect, r) = ui.allocate_exact_size(field, Sense::click_and_drag());
                    paint_sv(ui.painter(), rect, self.color.h);
                    if let Some(pos) = r.interact_pointer_pos()
                        && (r.dragged() || r.clicked() || r.is_pointer_button_down_on())
                    {
                        self.color.s = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                        self.color.v = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                        self.color.code = look::hex(self.color.color());
                    }
                    let mark = Pos2::new(
                        rect.left() + self.color.s * rect.width(),
                        rect.top() + (1.0 - self.color.v) * rect.height(),
                    );
                    ui.painter().circle(mark, 7.0, self.color.color(), Stroke::new(2.0, egui::Color32::WHITE));
                    ui.painter().circle_stroke(mark, 8.0, Stroke::new(1.0, egui::Color32::from_black_alpha(150)));
                    r.on_hover_text(t.sat_value).on_hover_cursor(CursorIcon::Crosshair);
                    w::section_label(ui, t.hue);
                    let (bar, r) = ui.allocate_exact_size(Vec2::new(236.0, 14.0), Sense::click_and_drag());
                    paint_hue(ui.painter(), bar);
                    if let Some(pos) = r.interact_pointer_pos()
                        && (r.dragged() || r.clicked() || r.is_pointer_button_down_on())
                    {
                        self.color.h = ((pos.x - bar.left()) / bar.width()).clamp(0.0, 0.999) * 360.0;
                        self.color.code = look::hex(self.color.color());
                    }
                    let x = bar.left() + self.color.h / 360.0 * bar.width();
                    ui.painter().circle(
                        Pos2::new(x, bar.center().y),
                        9.0,
                        egui::Color32::WHITE,
                        Stroke::new(1.0, egui::Color32::from_black_alpha(150)),
                    );
                });
                // Код, готовые, цвет Windows.
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.horizontal(|ui| {
                        let (sw, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
                        ui.painter().rect_filled(sw, 10, self.color.color());
                        ui.vertical(|ui| {
                            w::section_label(ui, t.color_code);
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.color.code)
                                    .font(egui::FontId::monospace(13.0))
                                    .desired_width(150.0),
                            );
                            if r.changed()
                                && let Some(c) = look::parse_hex(&self.color.code)
                            {
                                let code = self.color.code.clone();
                                self.color.set(c);
                                self.color.code = code;
                            }
                        });
                    });
                    w::section_label(ui, t.presets);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let dark = Palette::of(ui).dark;
                        for (i, a) in look::PRESETS.iter().enumerate() {
                            let c = a.swatch(dark).fill;
                            let (rect, r) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                            ui.painter().circle_filled(rect.center(), 12.0, c);
                            if self.color.color() == c {
                                ui.painter().circle_stroke(rect.center(), 14.0, Stroke::new(2.0, Palette::of(ui).text));
                            }
                            w::focus_ring(ui, rect, &r, 13);
                            if r.on_hover_text(t.accents[i]).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                self.color.set(c);
                            }
                        }
                    });
                    if w::button(ui, Kind::Secondary, Some(Icon::Monitor), t.windows_color).clicked() {
                        match sys::windows_accent() {
                            Some((r, g, b)) => self.color.set(egui::Color32::from_rgb(r, g, b)),
                            None => self.toasts.push(t.windows_color_none, Tone::Warning),
                        }
                    }
                });
                // Образцы в обеих темах и кнопки.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    let base = self.color.color();
                    ui.columns(2, |cols| {
                        preview(&mut cols[0], base, true, t, lang);
                        preview(&mut cols[1], base, false, t, lang);
                    });
                    ui.horizontal(|ui| {
                        let note_w = (ui.available_width() - 230.0).max(120.0);
                        ui.allocate_ui(Vec2::new(note_w, 40.0), |ui| w::note(ui, t.color_note));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if w::button(ui, Kind::Primary, Some(Icon::Check), t.apply).clicked() {
                                let dark = Palette::of(ui).dark;
                                let accent = look::PRESETS
                                    .iter()
                                    .find(|a| a.swatch(dark).fill == base)
                                    .copied()
                                    .unwrap_or_else(|| look::custom(base));
                                self.settings.accent =
                                    if accent.name == "custom" { look::hex(base) } else { accent.name.to_string() };
                                self.save();
                                anvil_ui::set_accent(&ctx, accent);
                                self.view = View::Settings;
                            }
                            if w::button(ui, Kind::Secondary, None, t.cancel).clicked() {
                                self.restore_accent(&ctx);
                                self.view = View::Settings;
                            }
                        });
                    });
                });
            });
        });
        // Панель перекрашивается вслед за выбором.
        if self.view == View::Color && self.color.color() != before {
            anvil_ui::set_accent(&ctx, look::custom(self.color.color()));
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

    /// Полоска у края. Мышь у края — шире и наполняется акцентом за время задержки выезда.
    fn plain_strip(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        let corner = CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 };
        match self.strip_fill {
            Some(fill) => {
                let rect = Rect::from_min_size(Pos2::ZERO, STRIP_HOT);
                ui.painter().rect_filled(rect, corner, p.raised);
                let done = Rect::from_min_size(Pos2::ZERO, Vec2::new(STRIP_HOT.x * fill, STRIP_HOT.y));
                ui.painter().rect_filled(done, corner, p.accent);
            }
            None => {
                ui.painter().rect_filled(Rect::from_min_size(Pos2::ZERO, STRIP), corner, p.border_strong);
            }
        }
    }

    /// Полоска с медиа: обложка, «название · исполнитель», «назад», «играть/пауза», «вперёд».
    /// Кнопки работают без выезда панели; щелчок по названию — открыть панель.
    fn media_strip(&mut self, ui: &mut Ui) {
        let Some(now) = self.playing.clone() else { return };
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let rect = Rect::from_min_size(Pos2::ZERO, MEDIA_STRIP);
        let corner = CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 };
        ui.painter().rect(rect, corner, p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        // Справа налево: «вперёд», «играть/пауза», «назад» по 26.
        let mut action = None;
        let buttons = Rect::from_min_max(Pos2::new(rect.right() - 4.0 - 26.0 * 3.0 - 4.0, rect.top()), rect.max);
        let mut row =
            ui.new_child(UiBuilder::new().max_rect(buttons).layout(egui::Layout::left_to_right(egui::Align::Center)));
        row.spacing_mut().item_spacing.x = 2.0;
        if modules::media::skip(&mut row, false, t.media_previous, 26.0).clicked() {
            action = Some(sys::media::Action::Previous);
        }
        let (icon, hint) = if now.playing { (Icon::Pause, t.media_pause) } else { (Icon::Play, t.media_play) };
        if modules::media::play_button(&mut row, icon, hint, 26.0).clicked() {
            action = Some(sys::media::Action::PlayPause);
        }
        if modules::media::skip(&mut row, true, t.media_next, 26.0).clicked() {
            action = Some(sys::media::Action::Next);
        }
        // Обложка и название — одна область: щелчок открывает панель.
        let info = Rect::from_min_max(rect.min + Vec2::new(6.0, 0.0), Pos2::new(buttons.left() - 6.0, rect.bottom()));
        let cover = Rect::from_center_size(Pos2::new(info.left() + 11.0, info.center().y), Vec2::splat(22.0));
        match &now.cover {
            Some(tex) => egui::Image::new(tex).corner_radius(5).paint_at(ui, cover),
            None => {
                ui.painter().rect_filled(cover, 5, p.badge_fill(p.accent));
                anvil_ui::icons::paint(ui.painter(), cover.shrink(5.0), Icon::Play, p.accent_text);
            }
        }
        let mut job = egui::text::LayoutJob::default();
        let title = if now.title.is_empty() { t.media_untitled } else { now.title.as_str() };
        job.append(title, 0.0, egui::TextFormat::simple(anvil_ui::semibold(12.5), p.text));
        if !now.artist.is_empty() {
            job.append(
                &format!(" · {}", now.artist),
                0.0,
                egui::TextFormat::simple(egui::FontId::proportional(12.5), p.weak),
            );
        }
        job.wrap = egui::text::TextWrapping::truncate_at_width(info.right() - cover.right() - 8.0);
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(Pos2::new(cover.right() + 8.0, info.center().y - galley.size().y / 2.0), galley, p.text);
        let r = ui.interact(info, ui.id().with("strip-open"), Sense::click());
        if r.on_hover_text(t.open_panel).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.edge.reveal(Instant::now());
            self.refreshed = None;
            self.focus(ui.ctx());
        }
        if let Some(a) = action {
            self.command_all(Command::Media(a));
        }
    }

    /// Плашка у края: таймер дошёл. «Готово» — убрать, щелчок по тексту — открыть панель.
    fn banner_strip(&mut self, ui: &mut Ui) {
        let Some(text) = self.banner.clone() else { return };
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let rect = Rect::from_min_size(Pos2::ZERO, BANNER);
        let corner = CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 };
        ui.painter().rect(rect, corner, p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        let mark = Rect::from_center_size(Pos2::new(rect.left() + 26.0, rect.center().y), Vec2::splat(28.0));
        ui.painter().rect_filled(mark, 7, p.badge_fill(p.accent));
        anvil_ui::icons::paint(ui.painter(), mark.shrink(6.0), Icon::Clock, p.accent_text);
        let done_w = 76.0;
        let btn = Rect::from_min_size(
            Pos2::new(rect.right() - done_w - 10.0, rect.center().y - 14.0),
            Vec2::new(done_w, 28.0),
        );
        let mut b = ui.new_child(UiBuilder::new().max_rect(btn));
        if w::button_sized(&mut b, Kind::Primary, None, t.done, btn.size()).clicked() {
            self.banner = None;
        }
        let info =
            Rect::from_min_max(Pos2::new(mark.right() + 10.0, rect.top()), Pos2::new(btn.left() - 8.0, rect.bottom()));
        let mut job = egui::text::LayoutJob::simple_singleline(text, anvil_ui::semibold(13.0), p.text);
        job.wrap = egui::text::TextWrapping::truncate_at_width(info.width());
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(Pos2::new(info.left(), info.center().y - galley.size().y / 2.0), galley, p.text);
        let r = ui.interact(info, ui.id().with("banner-open"), Sense::click());
        if r.on_hover_text(t.open_panel).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.banner = None;
            self.edge.reveal(Instant::now());
            self.refreshed = None;
            self.focus(ui.ctx());
        }
    }

    fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let t = self.drawn;
        if t <= 0.0 {
            match self.strip_kind {
                StripKind::Off => {}
                StripKind::Plain => self.plain_strip(ui),
                StripKind::Media => self.media_strip(ui),
                StripKind::Banner => self.banner_strip(ui),
            }
            return;
        }
        let p = Palette::of(ui);
        let panel = Rect::from_min_size(Pos2::ZERO, Vec2::new(self.settings.width, self.settings.height));
        let corner = CornerRadius { nw: 0, ne: 0, sw: CORNER, se: CORNER };
        ui.painter().rect(panel, corner, p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);

        let inner = Rect::from_min_max(panel.min + Vec2::new(12.0, 4.0), panel.max - Vec2::new(12.0, 14.0));
        let mut content = ui.new_child(UiBuilder::new().max_rect(inner));
        content.set_opacity(t);
        content.set_clip_rect(panel);
        let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER - 8.0));
        let mut top = content.new_child(UiBuilder::new().max_rect(header));
        self.header(&mut top);
        let body = Rect::from_min_max(Pos2::new(inner.left(), header.bottom() + 4.0), inner.max);
        let mut body_ui = content.new_child(UiBuilder::new().max_rect(body));
        let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
        match self.view {
            View::Cards => self.cards(&mut body_ui, &device),
            View::Settings => self.settings_view(&mut body_ui),
            View::Color => self.color_view(&mut body_ui),
        }
        if t >= 1.0 {
            self.grips(ui, panel, ppp);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.grip.is_none()
            && self.capture.is_none()
            && !self.capture_panel
            && self.naming.is_none()
        {
            if self.view == View::Color {
                self.restore_accent(&ctx);
            }
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
        anvil_ui::chrome::clear_color(visuals, look::accent_of(&self.settings.accent))
    }
}

/// Тонкая вертикальная черта в шапке.
fn vline(ui: &mut Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, color);
}

/// Затухание к правому краю: от прозрачного к `color`.
fn fade_right(painter: &egui::Painter, rect: Rect, color: egui::Color32) {
    let mut mesh = egui::Mesh::default();
    let clear = egui::Color32::TRANSPARENT;
    let solid_from = rect.left() + rect.width() * 0.7;
    for (x, c) in [(rect.left(), clear), (solid_from, color), (rect.right(), color)] {
        mesh.colored_vertex(Pos2::new(x, rect.top()), c);
        mesh.colored_vertex(Pos2::new(x, rect.bottom()), c);
    }
    for i in 0..2u32 {
        let b = i * 2;
        mesh.add_triangle(b, b + 1, b + 2);
        mesh.add_triangle(b + 1, b + 2, b + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Строка настроек: подпись слева, управление справа.
fn setting_row(ui: &mut Ui, label: &str, right: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 32.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
        },
    );
}

/// Ручка перетаскивания: шесть точек.
fn paint_grip(painter: &egui::Painter, c: Pos2, color: egui::Color32) {
    for dx in [-3.0, 3.0] {
        for dy in [-5.0, 0.0, 5.0] {
            painter.circle_filled(c + Vec2::new(dx, dy), 1.4, color);
        }
    }
}

/// Поле насыщенности (вправо) и яркости (вверх) для оттенка `h` — сеткой из треугольников.
fn paint_sv(painter: &egui::Painter, rect: Rect, h: f32) {
    const N: u32 = 16;
    let mut mesh = egui::Mesh::default();
    for j in 0..=N {
        for i in 0..=N {
            let (s, v) = (i as f32 / N as f32, 1.0 - j as f32 / N as f32);
            let pos = Pos2::new(rect.left() + s * rect.width(), rect.top() + (1.0 - v) * rect.height());
            mesh.colored_vertex(pos, look::hsv(h, s, v));
        }
    }
    for j in 0..N {
        for i in 0..N {
            let a = j * (N + 1) + i;
            mesh.add_triangle(a, a + 1, a + N + 1);
            mesh.add_triangle(a + 1, a + N + 1, a + N + 2);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Полоса оттенков 0…360.
fn paint_hue(painter: &egui::Painter, rect: Rect) {
    const N: u32 = 36;
    let mut mesh = egui::Mesh::default();
    for i in 0..=N {
        let x = rect.left() + i as f32 / N as f32 * rect.width();
        let c = look::hsv(i as f32 / N as f32 * 360.0, 1.0, 1.0);
        mesh.colored_vertex(Pos2::new(x, rect.top()), c);
        mesh.colored_vertex(Pos2::new(x, rect.bottom()), c);
    }
    for i in 0..N {
        let b = i * 2;
        mesh.add_triangle(b, b + 1, b + 2);
        mesh.add_triangle(b + 1, b + 2, b + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Образец цвета в одной теме: чип, главная кнопка, переключатель, ссылка и контраст.
fn preview(ui: &mut Ui, base: egui::Color32, dark: bool, t: &texts::Tr, lang: Lang) {
    let sw = look::derive(base, dark);
    let accent = anvil_ui::Accent { name: "preview", dark: sw, light: sw };
    let p = Palette::new(dark, accent);
    let soft = look::soft(sw.fill, dark);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(
                egui::RichText::new(if dark { t.dark_theme } else { t.light_theme }).size(11.0).strong().color(p.weak),
            );
            ui.horizontal(|ui| {
                let chip = |ui: &mut Ui, text: &str, fill, stroke, color| {
                    let font = anvil_ui::semibold(12.5);
                    let w = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color).size().x + 18.0;
                    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::hover());
                    ui.painter().rect(r, 6, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, font, color);
                };
                chip(ui, t.sample, soft, p.accent, p.accent_text);
                chip(ui, t.done, p.accent, p.accent, p.on_accent);
                let (r, _) = ui.allocate_exact_size(Vec2::new(34.0, 20.0), Sense::hover());
                ui.painter().rect_filled(r, 10, p.accent);
                ui.painter().circle_filled(Pos2::new(r.right() - 10.0, r.center().y), 7.0, p.on_accent);
            });
            ui.label(egui::RichText::new(format!("{} ›", t.all_res)).size(12.5).color(p.accent_text));
            let line = |ui: &mut Ui, label: &str, ratio: f32| {
                ui.horizontal(|ui| {
                    let ok = ratio >= 4.5;
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(13.0), Sense::hover());
                    anvil_ui::icons::paint(
                        ui.painter(),
                        r,
                        if ok { Icon::Check } else { Icon::Warning },
                        if ok { p.success } else { p.danger },
                    );
                    ui.label(egui::RichText::new(label).size(12.0).color(p.weak));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(texts::ratio(lang, ratio))
                                .font(egui::FontId::monospace(12.0))
                                .color(p.text),
                        );
                    });
                });
            };
            let text_ratio = [p.surface, p.card, p.raised, soft]
                .into_iter()
                .map(|g| look::contrast(sw.text, g))
                .fold(21.0, f32::min);
            line(ui, t.contrast_text, text_ratio);
            line(ui, t.contrast_on, look::contrast(sw.on_fill, sw.fill));
            let moved = sw.text != base || sw.fill != base;
            ui.label(egui::RichText::new(if moved { t.adjusted } else { t.as_picked }).size(12.0).color(p.weak));
        });
}
