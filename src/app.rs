// Окно панели — это и есть панель: того же размера, у верхнего края монитора. Выезд — окно
// само съезжает из-за края вниз (часть выше края экрана не видна). Нижние углы скругляет регион
// окна (SetWindowRgn), а не прозрачность: прозрачность окна OpenGL на Windows ненадёжна — на части
// систем прозрачное рисуется белым, а с «пропускать мышь» (WS_EX_LAYERED) белеет всё окно.
// Спрятанное окно уводим правее всех мониторов.

use std::time::{Duration, Instant};

use anvil_ui::motion::{self, Motion};
use anvil_ui::widgets::{self as w, Kind, Toasts};
use anvil_ui::{Icon, Lang, Palette, ThemeChoice, Tone, tr};
use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
};

use crate::config::{self, ALL_MONITORS, Hotkey, MAX_HEIGHT, MIN_HEIGHT, MIN_WIDTH, Settings};
use crate::modules::{self, Command, Ctx, Module};
use crate::panel::{self, Edge, Input};
use crate::sys::{self, monitor};
use crate::{look, texts};

/// Радиус нижних углов панели, в точках.
const CORNER: u8 = 14;
/// Шапка: вкладки стоят на её нижней линии; тело — от `BODY_TOP` до `BODY_BOTTOM` над нижним краем.
const HEADER: f32 = 48.0;
const BODY_TOP: f32 = 58.0;
const BODY_BOTTOM: f32 = 16.0;
const GRIP: f32 = 7.0;
/// Верхняя полоса карточки в правке полок: ручка, имя, крестик.
const EDIT_STRIP: f32 = 44.0;
/// Предел сужения подписи вкладки, точек: «мягкий» — пока справа остаются профили и выбор монитора, «жёсткий» — когда
/// уходят и они.
const TAB_SOFT: f32 = 72.0;
const TAB_FLOOR: f32 = 36.0;
/// Наименьшая ширина карточки в правке: её содержимое под вуалью всё равно недоступно, а ряд вместе с «Убранными»
/// должен уместиться, чтобы ничего не листать.
const EDIT_MIN_WIDTH: f32 = 150.0;
/// Наименьшая ширина блока «Убранные карточки».
const REMOVED_MIN_WIDTH: f32 = 220.0;
/// Клавиши 1…9 для Ctrl + цифра (открыть полку) и Alt + цифра (перенести карточку на полку).
const NUM_KEYS: [egui::Key; 9] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];
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

/// Правка полок: раскладка на входе — для «Как было».
struct Edit {
    shelves: Vec<config::Shelf>,
    hidden: Vec<String>,
}

/// Переименование полки: двойной щелчок по вкладке в правке.
struct Rename {
    shelf: usize,
    text: String,
    /// Поле только появилось — ему ещё нужно дать фокус.
    fresh: bool,
}

/// Карточку тянут за ручку: «груз» перетаскивания — её ключ.
#[derive(Clone, Copy)]
struct Dragged(&'static str);

/// Что правка просит сделать с раскладкой: набирается за кадр, применяется в конце (`App::apply_ops`).
enum Op {
    /// Поставить на место `at` открытой полки (см. `Settings::place_card`).
    Move { key: &'static str, at: usize },
    /// Перенести в конец другой полки.
    ToShelf { key: &'static str, shelf: usize },
    /// Убрать с полки.
    Hide(&'static str),
    /// Вернуть из убранных в конец открытой полки.
    Show(&'static str),
}

/// Что стоит в ряду открытой полки.
#[derive(Clone, Copy, PartialEq)]
enum Slot {
    /// Карточка: номер модуля.
    Card(usize),
    /// «Убранные карточки» — только в правке.
    Removed,
}

/// Геометрия ряда карточек: видимая область, откуда рисовать и высота карточек.
#[derive(Clone, Copy)]
struct Row {
    area: Rect,
    origin: Pos2,
    height: f32,
}

/// Карточка ряда в правке полок.
struct EditCard<'a> {
    lang: Lang,
    key: &'static str,
    title: &'a str,
    /// Место в ряду.
    pos: usize,
    /// Ключи карточек ряда по порядку.
    order: &'a [&'static str],
    /// Сколько полок и какая открыта — для Alt + цифра.
    shelves: usize,
    open: usize,
}

/// Сколько нужно вкладке: ширина подписи и всё остальное (`tab_rest`).
struct TabMetric {
    text: f32,
    rest: f32,
}

/// Что вернула вкладка за кадр.
struct TabOut {
    rect: Rect,
    click: bool,
    rename: bool,
    close: bool,
    /// Карточка, отпущенная над вкладкой.
    dropped: Option<&'static str>,
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
    /// Сколько места заняла правая часть шапки в прошлом кадре — левой достаётся остальное.
    header_right: f32,
    /// Где кончается место левой части шапки (дальше она обрезана) и где начинается правая (для проверки, что не наезжают).
    header_edges: (f32, f32),
    /// Открытая полка (номер в `settings.shelves`); помнится, пока работает программа, в файл не пишется.
    shelf: usize,
    /// Правка полок на месте: снимок раскладки на входе; `None` — не правим.
    editing: Option<Edit>,
    /// Переименовывают полку (в правке).
    renaming: Option<Rename>,
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
        // Для снимков: `ANTRESOL_SHELF=2` — сразу открыть вторую полку, `ANTRESOL_VIEW=edit` — сразу правку полок.
        let shelf = std::env::var("ANTRESOL_SHELF")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map_or(0, |n| n.saturating_sub(1))
            .min(settings.shelves.len().saturating_sub(1));
        let editing = (std::env::var("ANTRESOL_VIEW").as_deref() == Ok("edit"))
            .then(|| Edit { shelves: settings.shelves.clone(), hidden: settings.hidden.clone() });
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
            header_right: 0.0,
            header_edges: (0.0, f32::MAX),
            shelf,
            editing,
            renaming: None,
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
        // Снимки в открытый репозиторий: не выдавать, какие сочетания заняты на этом ПК.
        if sys::demo() {
            return Some(sys::HOTKEYS[0].0.join("+"));
        }
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
        if matches!(cmd, Command::ApplyProfile(_)) {
            self.reveal_busy();
        }
    }

    /// Модуль ждёт ответа (после профиля — «Оставить?» у «Экрана»), а карточки на открытой полке нет — открыть ту полку,
    /// где она: иначе вопрос остался бы невидимым, а отсчёт 15 секунд шёл бы.
    fn reveal_busy(&mut self) {
        let open = self.shelf_modules(self.shelf);
        if open.iter().any(|&m| self.modules[m].busy()) {
            return;
        }
        let shelf = self.modules.iter().find(|m| m.busy()).and_then(|m| self.settings.shelf_of(m.key()));
        if let Some(shelf) = shelf {
            self.set_shelf(shelf);
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

    /// Программа закрывается: модули возвращают то, что держали на время (режим экрана до «Оставить»).
    fn shutdown(&mut self) {
        for module in &mut self.modules {
            module.shutdown();
        }
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
            // Спряталась — правка полок кончена (раскладка сохраняется по ходу), как и настройки.
            self.editing = None;
            self.renaming = None;
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
            // Систему опрашивают карточки открытой полки; убранные и с закрытых полок — нет (их `background` идёт и так).
            for i in self.refresh_set() {
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
        // Клавишу нажали в правке — сначала выход из неё: под вуалью «Оставить этот режим?» не нажать, а карточка с ним
        // может стоять на другой полке — её и открываем.
        self.finish_edit();
        self.naming = None;
        self.reveal_busy();
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

    fn header(&mut self, ui: &mut Ui, ops: &mut Vec<Op>) {
        if self.view == View::Cards {
            self.header_cards(ui, ops);
            return;
        }
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let ctx = ui.ctx().clone();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(2.0);
            match self.view {
                View::Cards => {}
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
                ui.add_space(6.0);
                w::mono(ui, &format!("Antresol {}", env!("CARGO_PKG_VERSION")), Some(p.weak));
            });
        });
    }

    /// Шапка над карточками: слева значок, «Antresol» и вкладки, справа значки, монитор и профили. Левая часть рисуется
    /// по месту, что осталось после правой в прошлом кадре, и обрезана по нему — на правую не заезжает. Не помещается — по
    /// порядку: уходят подписи («Профиль», «Монитор»), слово «Antresol», подпись у «Полка», вкладки сужаются (многоточие,
    /// полное имя в подсказке), а если и так тесно — профили и выбор монитора справа (`header_right`).
    fn header_cards(&mut self, ui: &mut Ui, ops: &mut Vec<Op>) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let rect = ui.max_rect();
        let editing = self.editing.is_some();
        let tabs = self.tab_metrics(ui);
        let text_of = |sum_cap: f32| tabs.iter().map(|m| m.text.min(sum_cap)).sum::<f32>();
        // Отступ со значком, черта после слова, промежутки между вкладками и «Полка» (в правке) — что не сужается.
        let rest = 2.0
            + 24.0
            + 8.0
            + 1.0
            + 8.0
            + tabs.iter().map(|m| m.rest).sum::<f32>()
            + 2.0 * tabs.len().saturating_sub(1) as f32;
        let word = ui.painter().layout_no_wrap("Antresol".into(), anvil_ui::semibold(14.5), p.text).size().x + 8.0;
        let add_icon = if editing { 8.0 + 30.0 } else { 0.0 };
        let add_text = if editing {
            let label = ui.painter().layout_no_wrap(t.shelf_add.into(), egui::FontId::proportional(14.0), p.weak);
            8.0 + 10.0 + 16.0 + 7.0 + label.size().x + 12.0
        } else {
            0.0
        };
        // Правой части вкладки оставляют не меньше `floor` (подписи на пределе), а профили и монитор — не меньше `soft`.
        let floor = rest + text_of(TAB_FLOOR) + add_icon;
        let soft = rest + text_of(TAB_SOFT) + add_icon;

        // Слева — что осталось после правой части в прошлом кадре.
        let budget = (rect.width() - self.header_right - 16.0).max(0.0);
        let natural = text_of(f32::INFINITY);
        let (show_word, add_label, cap) = if rest + word + natural + add_text <= budget {
            (true, true, None)
        } else if rest + natural + add_text <= budget {
            (false, true, None)
        } else {
            let texts: Vec<f32> = tabs.iter().map(|m| m.text).collect();
            (false, false, tab_cap(&texts, budget - rest - add_icon, TAB_FLOOR))
        };
        let left_rect = Rect::from_min_max(rect.min, Pos2::new(rect.left() + budget, rect.bottom()));
        let mut left = ui.new_child(
            UiBuilder::new()
                .id_salt("header-left")
                .max_rect(left_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        left.set_clip_rect(left_rect.expand2(Vec2::new(0.0, 4.0)).intersect(ui.clip_rect()));
        left.spacing_mut().item_spacing.x = 8.0;
        left.add_space(2.0);
        anvil_ui::chrome::app_mark(&mut left, Icon::Layers, 24.0);
        if show_word {
            left.label(egui::RichText::new("Antresol").font(anvil_ui::semibold(14.5)).color(p.text));
        }
        vline(&mut left, p.border);
        self.shelf_tabs(&mut left, ops, cap);
        if editing {
            let room = self.settings.shelves.len() < config::MAX_SHELVES;
            let r = left.add_enabled_ui(room, |ui| {
                if add_label {
                    w::button(ui, Kind::Ghost, Some(Icon::Plus), t.shelf_add)
                } else {
                    w::icon_button(ui, Icon::Plus, t.shelf_add)
                }
            });
            if r.inner.clicked() {
                self.add_shelf();
            }
            if !room {
                // Девять полок — предел: кнопка серая, а почему — в подсказке.
                let _ = r.inner.on_disabled_hover_text(t.shelf_limit);
            }
        }

        // Справа налево: правой части отдано всё, кроме `floor` слева.
        let right_rect = Rect::from_min_max(Pos2::new(rect.left() + floor + 16.0, rect.top()), rect.max);
        let mut right = ui.new_child(
            UiBuilder::new()
                .id_salt("header-right")
                .max_rect(right_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 4.0;
        self.header_right(&mut right, soft - floor);
        let used = right.min_rect().width();
        if (used - self.header_right).abs() > 0.5 {
            // Правая часть поменялась — левой на следующем кадре другое место.
            self.header_right = used;
            ui.ctx().request_repaint();
        }
        self.header_edges = (left_rect.right(), right.min_rect().left());
    }

    /// Правая часть шапки над карточками: «спрятать», настройки, закрепить, правка полок; потом выбор монитора и профили
    /// экрана — если вкладкам остаётся хотя бы `soft_reserve` сверх наименьшего; в правке — «Готово» и «Как было».
    fn header_right(&mut self, ui: &mut Ui, soft_reserve: f32) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        if self.editing.is_some() {
            // Правка полок: справа только «Готово», «Как было» и подсказка.
            self.edit_controls(ui, soft_reserve);
            return;
        }
        if w::icon_button(ui, Icon::ChevronUp, t.hide).clicked() {
            self.edge.hide();
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
        if w::icon_button(ui, Icon::Pencil, t.shelves_edit).clicked() {
            self.start_edit();
        }
        // Профили экрана — слева от монитора панели; без карточки «Экран» их нет. Не помещаются — уходят (ввод имени
        // нового профиля остаётся: его начали, и он важнее вкладок).
        let monitors = self.monitors.len() > 1;
        let divider = 6.0 + 1.0 + 6.0 + 8.0;
        let mut room = ui.available_width() - soft_reserve - divider;
        let profiles_w = self.profiles_width(ui);
        let profiles = !self.settings.is_hidden("display") && (self.naming.is_some() || profiles_w <= room);
        if profiles {
            room -= profiles_w + divider;
        }
        let monitors = monitors && self.monitor_width(ui) <= room;
        if profiles || monitors {
            ui.add_space(6.0);
            vline(ui, p.border);
            ui.add_space(6.0);
        }
        if monitors {
            self.monitor_picker(ui);
            side_label(ui, t.monitor, p.weak, soft_reserve);
            if profiles {
                ui.add_space(6.0);
                vline(ui, p.border);
                ui.add_space(6.0);
            }
        }
        if profiles {
            self.profiles_bar(ui, soft_reserve);
        }
    }

    /// Сколько места нужно чипам профилей и «+» (с запасом на полужирный): для решения, помещаются ли они в шапку.
    fn profiles_width(&self, ui: &Ui) -> f32 {
        let p = Palette::of(ui);
        let font = anvil_ui::semibold(13.0);
        let chips: f32 = self
            .settings
            .profiles
            .iter()
            .map(|pr| ui.painter().layout_no_wrap(pr.name.clone(), font.clone(), p.text).size().x + 20.0 + 4.0)
            .sum();
        30.0 + 4.0 + chips
    }

    /// Сколько места нужно выбору монитора панели («Все» и номера).
    fn monitor_width(&self, ui: &Ui) -> f32 {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let font = anvil_ui::semibold(13.5);
        let names = std::iter::once(t.all_monitors.to_string()).chain((1..=self.monitors.len()).map(|i| i.to_string()));
        names.map(|n| ui.painter().layout_no_wrap(n, font.clone(), p.text).size().x + 20.0).sum::<f32>() + 4.0 + 8.0
    }

    /// Сколько места нужно вкладкам: ширина подписи (полужирной) и всё остальное у вкладки (поля, метка, крестик).
    fn tab_metrics(&self, ui: &Ui) -> Vec<TabMetric> {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let p = Palette::of(ui);
        let count = self.settings.shelves.len();
        let open = self.shelf.min(count.saturating_sub(1));
        let closable = self.editing.is_some() && count > 1;
        (0..count)
            .map(|i| {
                if self.renaming.as_ref().is_some_and(|r| r.shelf == i) {
                    // На месте вкладки — поле имени.
                    return TabMetric { text: 0.0, rest: 140.0 + 8.0 };
                }
                let name = texts::shelf_title(t, &self.settings.shelves[i].name);
                let text = ui.painter().layout_no_wrap(name.to_owned(), anvil_ui::semibold(13.5), p.text).size().x;
                let badge = if i == open {
                    None
                } else {
                    self.shelf_modules(i).into_iter().find_map(|m| self.modules[m].badge(t, lang))
                };
                TabMetric { text, rest: tab_rest(ui, badge.as_ref().map(|(b, _)| b.as_str()), closable) }
            })
            .collect()
    }

    /// Шапка в правке полок, справа налево: «Готово», «Как было», подсказка (если есть где; `soft_reserve` — запас вкладкам).
    fn edit_controls(&mut self, ui: &mut Ui, soft_reserve: f32) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        if w::button(ui, Kind::Primary, Some(Icon::Check), t.done).clicked() {
            self.finish_edit();
        }
        // «Как было» без подписи, если рядом с вкладками для неё нет места.
        let undo_w = 10.0
            + 16.0
            + 7.0
            + ui.painter()
                .layout_no_wrap(t.shelves_undo.to_string(), egui::FontId::proportional(14.0), p.weak)
                .size()
                .x
            + 12.0;
        let undo = if ui.available_width() - soft_reserve >= undo_w + 40.0 {
            w::button(ui, Kind::Ghost, Some(Icon::Undo), t.shelves_undo)
        } else {
            w::icon_button(ui, Icon::Undo, t.shelves_undo)
        };
        if undo.clicked() {
            self.undo_edit();
        }
        let hint = egui::RichText::new(t.shelves_hint).size(12.5).color(p.weak);
        let hint_w = ui.painter().layout_no_wrap(t.shelves_hint.to_string(), egui::FontId::proportional(12.5), p.weak);
        if ui.available_width() - soft_reserve >= hint_w.size().x + 24.0 {
            ui.add_space(6.0);
            ui.label(hint);
        }
    }

    /// Профили экрана чипами в шапке, «+» — сохранить то, что стоит сейчас. Рисуется справа налево: «+», чипы с
    /// конца, подпись «Профиль»; ввод имени — на их месте.
    fn profiles_bar(&mut self, ui: &mut Ui, soft_reserve: f32) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        if let Some(mut name) = self.naming.take() {
            let mut done = None;
            if w::button(ui, Kind::Ghost, None, t.cancel).clicked() {
                done = Some(false);
            }
            if w::button(ui, Kind::Secondary, Some(Icon::Save), t.save).clicked() {
                done = Some(true);
            }
            let edit = ui.add(egui::TextEdit::singleline(&mut name).hint_text(t.profile_name).desired_width(140.0));
            if name.is_empty() && !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                done = Some(true);
            }
            // Esc отменяет только ввод имени: клавишу забираем, иначе она спрятала бы и панель.
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                done = Some(false);
            }
            side_label(ui, t.profile, p.weak, soft_reserve);
            match done {
                Some(true) => self.command_all(Command::SaveProfile(name)),
                Some(false) => {}
                None => self.naming = Some(name),
            }
            return;
        }
        let active = self.modules.iter().find_map(|m| m.active_profile(&self.settings.profiles));
        let mut apply = None;
        if w::icon_button(ui, Icon::Plus, t.profile_save).clicked() {
            self.naming = Some(String::new());
        }
        for (i, profile) in self.settings.profiles.iter().enumerate().rev() {
            let on = active == Some(i);
            let mut r = modules::chip(ui, ("profile", i), on, &profile.name, None);
            if let Some(h) = &profile.hotkey {
                r = r.on_hover_text(h.to_string());
            }
            if r.clicked() && !on {
                apply = Some(i);
            }
        }
        side_label(ui, t.profile, p.weak, soft_reserve);
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

    /// Номер модуля по ключу.
    fn module_index(&self, key: &str) -> Option<usize> {
        self.modules.iter().position(|m| m.key() == key)
    }

    /// Карточки полки по порядку: номера в `self.modules`.
    fn shelf_modules(&self, shelf: usize) -> Vec<usize> {
        let Some(s) = self.settings.shelves.get(shelf) else { return Vec::new() };
        s.cards.iter().filter_map(|k| self.module_index(k)).collect()
    }

    /// Что опрашивать, пока панель открыта: карточки открытой полки и «Экран» — его читают чипы профилей в шапке.
    fn refresh_set(&self) -> Vec<usize> {
        let mut set = self.shelf_modules(self.shelf);
        if let Some(d) = self.module_index("display")
            && self.settings.shelf_of("display").is_some()
            && !set.contains(&d)
        {
            set.push(d);
        }
        set
    }

    /// В карточке открытой полки вводят текст (Esc отменяет ввод, а не прячет панель). С других полок не считается:
    /// ввода на экране нет.
    fn typing_here(&self) -> bool {
        self.view == View::Cards && self.shelf_modules(self.shelf).into_iter().any(|m| self.modules[m].typing())
    }

    /// Открыть полку `i` (номер в `settings.shelves`).
    fn set_shelf(&mut self, i: usize) {
        let i = i.min(self.settings.shelves.len().saturating_sub(1));
        if i != self.shelf {
            self.shelf = i;
            // У новой полки свои карточки: перечитать их и не тащить сюда прокрутку со старой.
            self.refreshed = None;
            self.scroll_to = None;
        }
    }

    // ─── Правка полок ───────────────────────────────────────────────────────

    fn start_edit(&mut self) {
        self.naming = None;
        self.editing = Some(Edit { shelves: self.settings.shelves.clone(), hidden: self.settings.hidden.clone() });
    }

    /// «Готово» и Esc: раскладка уже сохранена по ходу, остаётся выйти.
    fn finish_edit(&mut self) {
        self.editing = None;
        self.renaming = None;
    }

    /// «Как было»: раскладка, что стояла на входе в правку; сама правка продолжается.
    fn undo_edit(&mut self) {
        let Some(e) = &self.editing else { return };
        self.settings.shelves = e.shelves.clone();
        self.settings.hidden = e.hidden.clone();
        self.renaming = None;
        self.shelf = self.shelf.min(self.settings.shelves.len().saturating_sub(1));
        self.refreshed = None;
        self.save();
    }

    /// «Полка»: новая полка в конце, она же становится открытой.
    fn add_shelf(&mut self) {
        if let Some(i) = self.settings.add_shelf(self.lang()) {
            self.save();
            self.set_shelf(i);
        }
    }

    /// Крестик на вкладке: полка уходит, её карточки — в «Убранные». Открытой остаётся та же полка; убрали открытую —
    /// встаёт соседняя.
    fn remove_shelf(&mut self, i: usize) {
        if !self.settings.remove_shelf(i) {
            return;
        }
        if i < self.shelf {
            self.shelf -= 1;
        }
        self.shelf = self.shelf.min(self.settings.shelves.len().saturating_sub(1));
        self.refreshed = None;
        self.scroll_to = None;
        self.save();
    }

    /// Имя полки ввели: готовое имя без правки остаётся готовым (следует за языком), пустое не сохраняется.
    fn commit_rename(&mut self, i: usize, text: &str) {
        let t = texts::tr(self.lang());
        let Some(shelf) = self.settings.shelves.get(i) else { return };
        if config::Shelf::clean_name(text) == texts::shelf_title(t, &shelf.name) {
            return;
        }
        if self.settings.rename_shelf(i, text) {
            self.save();
        }
    }

    /// Применить то, что правка набрала за кадр, и сохранить раскладку.
    fn apply_ops(&mut self, ops: Vec<Op>) {
        if ops.is_empty() {
            return;
        }
        for op in ops {
            match op {
                Op::Move { key, at } => self.settings.place_card(key, self.shelf, Some(at)),
                Op::ToShelf { key, shelf } => self.settings.place_card(key, shelf, None),
                Op::Hide(key) => self.settings.hide_card(key),
                Op::Show(key) => self.settings.place_card(key, self.shelf, None),
            };
        }
        self.save();
    }

    /// Ctrl+Tab и Ctrl+Shift+Tab — следующая и предыдущая полка (по кругу), Ctrl + 1…9 — полка с этим номером.
    /// Не когда вводят текст.
    fn shelf_keys(&mut self, ctx: &egui::Context) {
        if self.view != View::Cards || self.renaming.is_some() || self.naming.is_some() || ctx.text_edit_focused() {
            return;
        }
        let count = self.settings.shelves.len();
        if count == 0 {
            return;
        }
        let (back, forward, jump) = ctx.input_mut(|i| {
            let back = i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Tab);
            let forward = !back && i.consume_key(egui::Modifiers::CTRL, egui::Key::Tab);
            let jump = NUM_KEYS.iter().position(|k| i.consume_key(egui::Modifiers::CTRL, *k));
            (back, forward, jump)
        });
        if forward {
            self.set_shelf((self.shelf + 1) % count);
        } else if back {
            self.set_shelf((self.shelf + count - 1) % count);
        } else if let Some(n) = jump
            && n < count
        {
            self.set_shelf(n);
        }
    }

    /// Вкладки полок в шапке. Щелчок и колесо мыши над ними (а также Ctrl+Tab и Ctrl + 1…9) открывают полку; на
    /// вкладке закрытой полки — метка её модуля (идущий таймер). В правке: двойной щелчок (или F2) переименовывает,
    /// крестик (или Delete) убирает полку, а на вкладку можно бросить карточку.
    fn shelf_tabs(&mut self, ui: &mut Ui, ops: &mut Vec<Op>, cap: Option<f32>) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let editing = self.editing.is_some();
        let count = self.settings.shelves.len();
        let open = self.shelf.min(count.saturating_sub(1));
        let (mut pick, mut rename, mut remove) = (None, None, None);
        let mut strip: Option<Rect> = None;
        let spacing = ui.spacing().item_spacing.x;
        ui.spacing_mut().item_spacing.x = 2.0;
        for i in 0..count {
            let name = texts::shelf_title(t, &self.settings.shelves[i].name).to_string();
            if self.renaming.as_ref().is_some_and(|r| r.shelf == i) {
                self.rename_field(ui, i);
                continue;
            }
            // Метка модуля — только на вкладке закрытой полки.
            let badge = if i == open {
                None
            } else {
                self.shelf_modules(i).into_iter().find_map(|m| self.modules[m].badge(t, lang))
            };
            let spec = TabSpec {
                name: &name,
                selected: i == open,
                badge: badge.as_ref(),
                edit: editing.then_some(count > 1),
                cap,
            };
            let out = tab(ui, i, t, &spec);
            strip = Some(strip.map_or(out.rect, |r| r.union(out.rect)));
            if out.click {
                pick = Some(i);
            }
            if out.rename {
                rename = Some((i, name));
            }
            if out.close {
                remove = Some(i);
            }
            if let Some(key) = out.dropped {
                ops.push(Op::ToShelf { key, shelf: i });
            }
        }
        ui.spacing_mut().item_spacing.x = spacing;
        // Колесо мыши над вкладками: вверх — предыдущая полка, вниз — следующая.
        if let Some(strip) = strip
            && ui.rect_contains_pointer(strip)
        {
            let wheel: f32 = ui.input(|inp| {
                inp.events
                    .iter()
                    .map(|e| match e {
                        egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta, .. } => delta.y,
                        _ => 0.0,
                    })
                    .sum()
            });
            if wheel > 0.0 && open > 0 {
                pick = Some(open - 1);
            } else if wheel < 0.0 && open + 1 < count {
                pick = Some(open + 1);
            }
        }
        if let Some(i) = pick {
            self.set_shelf(i);
        }
        if let Some((i, name)) = rename {
            self.renaming = Some(Rename { shelf: i, text: name, fresh: true });
        }
        if let Some(i) = remove {
            self.remove_shelf(i);
        }
    }

    /// Поле имени на месте вкладки: Enter или щелчок мимо — сохранить, Esc — отмена; пустое имя не сохраняется.
    fn rename_field(&mut self, ui: &mut Ui, i: usize) {
        let Some(mut r) = self.renaming.take() else { return };
        let edit =
            ui.add(egui::TextEdit::singleline(&mut r.text).desired_width(140.0).char_limit(config::MAX_SHELF_NAME));
        if r.fresh {
            edit.request_focus();
            r.fresh = false;
        }
        // Esc отменяет только переименование: клавишу забираем, иначе она кончила бы и всю правку.
        if ui.input_mut(|inp| inp.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            return;
        }
        if edit.lost_focus() {
            self.commit_rename(i, &r.text);
            return;
        }
        self.renaming = Some(r);
    }

    /// Пустая полка: пояснение и кнопка, что делать.
    fn empty_shelf(&mut self, ui: &mut Ui, area: Rect) {
        let t = texts::tr(self.lang());
        let mut child =
            ui.new_child(UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Center)));
        child.add_space((area.height() / 2.0 - 40.0).max(8.0));
        w::note(&mut child, t.shelf_empty);
        child.add_space(6.0);
        if w::button(&mut child, Kind::Ghost, Some(Icon::Pencil), t.shelves_edit).clicked() {
            self.start_edit();
        }
    }

    /// Карточки открытой полки в ряд по долям ширины (`fit_widths`); узкая панель — у каждой своя наименьшая ширина, а ряд
    /// листается вбок (колесо мыши с Shift, полоса снизу или «Ещё N ›»). В правке в конце ряда ещё «Убранные карточки», а
    /// карточки сжимаются до `EDIT_MIN_WIDTH`: содержимое под вуалью всё равно недоступно, зато ряд помещается.
    fn cards(&mut self, ui: &mut Ui, device: &str, ops: &mut Vec<Op>) {
        let area = ui.available_rect_before_wrap();
        let gap = GAP;
        let shelf = self.shelf;
        let editing = self.editing.is_some();
        let mut slots: Vec<Slot> = self.shelf_modules(shelf).into_iter().map(Slot::Card).collect();
        if editing {
            slots.push(Slot::Removed);
        }
        if slots.is_empty() {
            self.empty_shelf(ui, area);
            return;
        }
        // Доля ширины и наименьшая ширина каждого места в ряду.
        let sizes: Vec<(f32, f32)> = slots
            .iter()
            .map(|s| match s {
                Slot::Card(i) => {
                    let m = &self.modules[*i];
                    (m.weight(), if editing { EDIT_MIN_WIDTH.min(m.min_width()) } else { m.min_width() })
                }
                Slot::Removed => (1.0, REMOVED_MIN_WIDTH),
            })
            .collect();
        let free = area.width() - gap * (slots.len().saturating_sub(1)) as f32;
        let widths = fit_widths(free, &sizes);
        let content = widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1)) as f32;
        if content <= area.width() + 0.5 {
            let row = Row { area, origin: area.min, height: area.height() };
            self.card_row(ui, row, &slots, &widths, device, ops);
            return;
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        // У каждой полки своя прокрутка.
        let mut scroll = egui::ScrollArea::horizontal().id_salt(("cards", shelf)).auto_shrink([false, false]);
        if let Some(x) = self.scroll_to.take() {
            scroll = scroll.horizontal_scroll_offset(x);
        }
        let out = scroll.show(&mut child, |ui| {
            // Под полосу прокрутки — 12 снизу.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(content, area.height() - 12.0), Sense::hover());
            let row = Row { area, origin: rect.min, height: rect.height() };
            self.card_row(ui, row, &slots, &widths, device, ops);
        });
        let offset = out.state.offset.x;
        let max_offset = (content - area.width()).max(0.0);
        // В правке над карточкой лежит её слой, а полоса прокрутки ряда — ниже: колесо с Shift до неё не доходит.
        // Что она не забрала, передаём ей сами (на следующем кадре).
        if editing {
            let ctx = ui.ctx().clone();
            let over = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| area.contains(p));
            let dx = ctx.input(|i| i.smooth_scroll_delta().x);
            if over && dx != 0.0 {
                self.scroll_to = Some((offset - dx).clamp(0.0, max_offset));
                ctx.input_mut(|i| i.smooth_scroll_delta.x = 0.0);
                ctx.request_repaint();
            }
        }
        // Сколько карточек справа не видно целиком: затухание и «Ещё N ›».
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
        if let Some(target) = first_hidden {
            self.more_button(ui.ctx(), area, hidden, target.min(max_offset));
        }
    }

    /// Затухание у правого края и «Ещё N ›»: ряд не поместился. Рисуются в слое `Foreground` — выше слоёв правки
    /// (`Middle`): иначе в правке кнопка оказалась бы под вуалью карточки и не нажималась.
    fn more_button(&mut self, ctx: &egui::Context, area: Rect, hidden: usize, target: f32) {
        let t = texts::tr(self.lang());
        let p = Palette::of_ctx(ctx);
        let alpha = self.drawn;
        let fade = Rect::from_min_max(
            Pos2::new(area.right() - 120.0, area.top()),
            Pos2::new(area.right(), area.bottom() - 12.0),
        );
        let label = texts::more_cards(self.lang(), hidden);
        let font = egui::FontId::proportional(13.0);
        // Затухание — свой слой, созданный раньше кнопки (под ней) и не ловящий мышь: карточки под ним остаются живыми.
        let text_w = egui::Area::new(egui::Id::new("more-fade"))
            .order(egui::Order::Foreground)
            .fixed_pos(fade.min)
            .interactable(false)
            .constrain(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                let (rect, _) = ui.allocate_exact_size(fade.size(), Sense::hover());
                fade_right(ui.painter(), rect, p.surface);
                ui.painter().layout_no_wrap(label.clone(), font.clone(), p.text).size().x
            })
            .inner;
        let size = Vec2::new(text_w + 34.0, 32.0);
        let at = Pos2::new(area.right() - size.x - 2.0, fade.center().y - size.y / 2.0);
        let clicked = egui::Area::new(egui::Id::new("more-cards"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .constrain(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                let (btn, r) = ui.allocate_exact_size(size, Sense::click());
                let fill = if r.hovered() { p.hover } else { p.card };
                ui.painter().rect(btn, 16, fill, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
                ui.painter().text(
                    btn.left_center() + Vec2::new(12.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &label,
                    font,
                    p.text,
                );
                anvil_ui::icons::paint(
                    ui.painter(),
                    Rect::from_center_size(Pos2::new(btn.right() - 14.0, btn.center().y), Vec2::splat(14.0)),
                    Icon::ArrowRight,
                    p.text,
                );
                w::focus_ring(ui, btn, &r, 16);
                r.on_hover_text(t.more_hint).on_hover_cursor(CursorIcon::PointingHand).clicked()
            })
            .inner;
        if clicked {
            self.scroll_to = Some(target);
        }
    }

    fn card_row(&mut self, ui: &mut Ui, row: Row, slots: &[Slot], widths: &[f32], device: &str, ops: &mut Vec<Op>) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let editing = self.editing.is_some();
        // Ключи карточек ряда по порядку — для черты «куда встанет» и сдвига с клавиатуры.
        let order: Vec<&'static str> = slots
            .iter()
            .filter_map(|s| match s {
                Slot::Card(m) => Some(self.modules[*m].key()),
                Slot::Removed => None,
            })
            .collect();
        let (open, shelves) = (self.shelf, self.settings.shelves.len());
        let mut x = row.origin.x;
        let mut pos = 0;
        let mut toasts = Vec::new();
        let before = self.settings.clone();
        for (i, slot) in slots.iter().enumerate() {
            let rect = Rect::from_min_size(Pos2::new(x, row.origin.y), Vec2::new(widths[i], row.height));
            x += widths[i] + GAP;
            let m = match *slot {
                Slot::Card(m) => m,
                Slot::Removed => {
                    self.removed_block(ui, rect, ops);
                    continue;
                }
            };
            let module = &mut self.modules[m];
            let (key, title) = (module.key(), module.title(t));
            // Карточки входят по очереди: сдвиг и прозрачность, как строки списка.
            let (alpha, dy) = Motion::of(ui.ctx()).enter(ui.ctx(), ui.id().with(("card", key)), i);
            let shifted = rect.translate(Vec2::new(0.0, dy));
            let mut child = ui.new_child(UiBuilder::new().id(ui.id().with(("card-ui", key))).max_rect(shifted));
            child.set_opacity(alpha);
            w::card_frame(&child).show(&mut child, |ui| {
                ui.set_min_size(ui.available_size());
                ui.set_clip_rect(ui.max_rect().intersect(rect.expand(1.0)));
                if editing {
                    // Под вуалью содержимое не отвечает ни мыши, ни клавиатуре: фокус в него не заходит, Enter ничего не
                    // нажимает. Бледнеть второй раз ему не надо — вуаль уже приглушает.
                    ui.visuals_mut().disabled_alpha = 1.0;
                    ui.disable();
                }
                modules::card_header(ui, key, title, &module.meta(t), &module.meta_hint());
                ui.add_space(8.0);
                let mut cx = Ctx { t, lang, monitor: device, toasts: &mut toasts, settings: &mut self.settings };
                module.ui(ui, &mut cx);
            });
            if editing {
                let card = EditCard { lang, key, title, pos, order: &order, shelves, open };
                edit_overlay(ui.ctx(), &card, shifted, row.area, alpha, ops);
            }
            pos += 1;
        }
        for (text, tone) in toasts {
            self.toasts.push(text, tone);
        }
        if self.settings != before {
            self.save();
        }
    }

    /// «Убранные карточки»: в конце ряда, только в правке. Чип возвращает карточку на открытую полку; сюда же можно
    /// бросить карточку — она уйдёт с полки.
    fn removed_block(&self, ui: &mut Ui, rect: Rect, ops: &mut Vec<Op>) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let drop = ui.interact(rect, egui::Id::new("removed-block"), Sense::hover());
        let over = drop.dnd_hover_payload::<Dragged>().is_some();
        if over {
            ui.painter().rect_filled(rect, 10, p.soft(p.accent));
        }
        let line = Stroke::new(1.5, if over { p.accent } else { p.border_strong });
        dashed_rounded_rect(ui.painter(), rect.shrink(0.75), 10.0, line);
        if let Some(d) = drop.dnd_release_payload::<Dragged>() {
            ops.push(Op::Hide(d.0));
        }
        let mut child = ui.new_child(UiBuilder::new().id(ui.id().with("removed-ui")).max_rect(rect.shrink(14.0)));
        w::section_label(&mut child, t.removed_cards);
        child.add_space(2.0);
        egui::ScrollArea::vertical().id_salt("removed").auto_shrink([false, false]).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            if self.settings.hidden.is_empty() {
                w::note(ui, t.removed_none);
            } else {
                ui.horizontal_wrapped(|ui| {
                    for key in &self.settings.hidden {
                        let Some(m) = self.module_index(key) else { continue };
                        let module = &self.modules[m];
                        if w::button(ui, Kind::Secondary, Some(Icon::Plus), module.title(t)).clicked() {
                            ops.push(Op::Show(module.key()));
                        }
                    }
                });
            }
            ui.add_space(6.0);
            w::note(ui, t.removed_note);
        });
    }

    /// Настройки — три карточки; уже 930 точек — листаются вбок, а не сжимаются.
    fn settings_view(&mut self, ui: &mut Ui) {
        const MIN: f32 = 930.0;
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
        // Три карточки: «Панель», «Профили экрана», «Оформление». Какие карточки и в каком порядке — правка полок
        // на самой панели (карандаш в шапке), а не настройки.
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

        // Полок стало меньше, чем номер открытой, — берётся последняя.
        self.shelf = self.shelf.min(self.settings.shelves.len().saturating_sub(1));
        self.shelf_keys(&ctx);

        let inner = Rect::from_min_max(panel.min + Vec2::new(12.0, 0.0), panel.max - Vec2::new(12.0, BODY_BOTTOM));
        let mut content = ui.new_child(UiBuilder::new().max_rect(inner));
        content.set_opacity(t);
        content.set_clip_rect(panel);
        // Под шапкой — линия во всю ширину панели; вкладки стоят на ней.
        let line = egui::Rangef::new(panel.left() + 1.0, panel.right() - 1.0);
        content.painter().hline(line, HEADER - 0.5, Stroke::new(1.0, p.border));
        let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER));
        let mut ops = Vec::new();
        let mut top = content.new_child(UiBuilder::new().max_rect(header));
        self.header(&mut top, &mut ops);
        let body = Rect::from_min_max(Pos2::new(inner.left(), BODY_TOP), inner.max);
        let mut body_ui = content.new_child(UiBuilder::new().max_rect(body));
        let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
        match self.view {
            View::Cards => self.cards(&mut body_ui, &device, &mut ops),
            View::Settings => self.settings_view(&mut body_ui),
            View::Color => self.color_view(&mut body_ui),
        }
        self.apply_ops(ops);
        if t >= 1.0 {
            self.grips(ui, panel, ppp);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.grip.is_none()
            && self.capture.is_none()
            && !self.capture_panel
            && self.naming.is_none()
            && self.renaming.is_none()
            && !self.typing_here()
        {
            if self.editing.is_some() {
                // Esc в правке — как «Готово»; следующий Esc прячет панель.
                self.finish_edit();
            } else {
                if self.view == View::Color {
                    self.restore_accent(&ctx);
                }
                self.edge.hide();
            }
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

    /// Окно закрывается (Alt+F4, «Закрыть Antresol»): модули возвращают то, что держали на время, — режим экрана,
    /// который не успели подтвердить. Выход из сеанса сюда не доходит, но и не нужен: неподтверждённого режима нет в реестре.
    fn on_exit(&mut self) {
        self.shutdown();
    }
}

/// Ширины мест в ряду: `free` точек поровну по долям `weight`, но не меньше `min`. Кому доля мала, встаёт на `min`, а
/// остальные делят то, что осталось, — ряд помещается, пока помещаются все `min` (их сумма не больше `free`); иначе
/// все ширины — `min`, и ряд листается вбок.
fn fit_widths(free: f32, items: &[(f32, f32)]) -> Vec<f32> {
    let mut fixed = vec![false; items.len()];
    loop {
        let left = free - items.iter().zip(&fixed).filter(|(_, f)| **f).map(|((_, min), _)| *min).sum::<f32>();
        let weight: f32 = items.iter().zip(&fixed).filter(|(_, f)| !**f).map(|((w, _), _)| *w).sum();
        let share = |w: f32| if weight > 0.0 { left * w / weight } else { 0.0 };
        let mut changed = false;
        for (i, (w, min)) in items.iter().enumerate() {
            if !fixed[i] && share(*w) < *min {
                fixed[i] = true;
                changed = true;
            }
        }
        if !changed {
            return items.iter().enumerate().map(|(i, (w, min))| if fixed[i] { *min } else { share(*w) }).collect();
        }
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

/// Подпись в шапке («Профиль», «Монитор») — только если справа налево после неё ещё остаётся место (кроме `reserve`,
/// что отдано вкладкам).
fn side_label(ui: &mut Ui, text: &str, color: Color32, reserve: f32) {
    let width = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), color).size().x;
    if ui.available_width() - reserve >= width + 40.0 {
        ui.label(egui::RichText::new(text).size(12.0).color(color));
    }
}

/// Скруглённая рамка пунктиром: `Shape::dashed_line` по обходу прямоугольника с дугами в углах.
fn dashed_rounded_rect(painter: &egui::Painter, rect: Rect, radius: f32, stroke: Stroke) {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let mut pts: Vec<Pos2> = Vec::new();
    // По часовой стрелке от левого верхнего угла; в экранных координатах 270° — вверх, 90° — вниз.
    for (centre, from) in [
        (rect.left_top() + Vec2::new(r, r), 180.0_f32),
        (rect.right_top() + Vec2::new(-r, r), 270.0),
        (rect.right_bottom() + Vec2::new(-r, -r), 0.0),
        (rect.left_bottom() + Vec2::new(r, -r), 90.0),
    ] {
        for k in 0..=6 {
            let a = (from + 15.0 * k as f32).to_radians();
            pts.push(centre + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    pts.push(pts[0]);
    painter.extend(egui::Shape::dashed_line(&pts, stroke, 5.0, 4.0));
}

/// Сколько у вкладки места не под подпись: поля по 11, метка модуля и крестик «убрать полку» (в правке).
fn tab_rest(ui: &Ui, badge: Option<&str>, closable: bool) -> f32 {
    let p = Palette::of(ui);
    let mut rest = 11.0 + 11.0;
    if let Some(text) = badge {
        let w = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::monospace(11.5), p.text).size().x;
        rest += 7.0 + w + 14.0;
    }
    if closable {
        rest += 7.0 + 16.0;
    }
    rest
}

/// Предел ширины подписи вкладки, чтобы все подписи (`texts`, точек) вместе заняли не больше `avail`: короткие остаются
/// как есть, длинные делят остальное поровну, но не уже `floor`. Влезают без предела — `None`.
fn tab_cap(texts: &[f32], avail: f32, floor: f32) -> Option<f32> {
    if texts.iter().sum::<f32>() <= avail {
        return None;
    }
    let mut sorted = texts.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut left = avail;
    for (i, w) in sorted.iter().enumerate() {
        let share = left / (sorted.len() - i) as f32;
        if *w > share {
            return Some(share.max(floor));
        }
        left -= *w;
    }
    None
}

/// Что вкладке рисовать: имя, выбрана ли, метка модуля, правка и предел ширины подписи.
struct TabSpec<'a> {
    name: &'a str,
    selected: bool,
    badge: Option<&'a (String, String)>,
    /// `None` — обычный вид; `Some(можно_убрать)` — правка: крестик под мышью, пунктир под брошенной карточкой,
    /// переименование двойным щелчком.
    edit: Option<bool>,
    /// Подпись уже этого не бывает (тесно): обрезается многоточием, полное имя — в подсказке.
    cap: Option<f32>,
}

/// Одна вкладка полки: высота шапки, подпись (выбранная — полужирная, с чертой акцента у нижней линии) и метка
/// модуля.
fn tab(ui: &mut Ui, salt: usize, t: &texts::Tr, spec: &TabSpec) -> TabOut {
    let TabSpec { name, selected, badge, edit, cap } = *spec;
    let p = Palette::of(ui);
    let bold = anvil_ui::semibold(13.5);
    let mono = egui::FontId::monospace(11.5);
    // Ширину под подпись берём по полужирной: выбрали другую вкладку — остальные не сдвинулись. Тесно — подпись
    // обрезается многоточием до `cap`, а полное имя уходит в подсказку.
    let name_full = ui.painter().layout_no_wrap(name.to_owned(), bold.clone(), p.text).size().x;
    let name_w = cap.map_or(name_full, |c| name_full.min(c));
    let cut = name_w < name_full - 0.5;
    let badge_w =
        badge.map_or(0.0, |(text, _)| ui.painter().layout_no_wrap(text.clone(), mono.clone(), p.text).size().x + 14.0);
    let closable = edit == Some(true);
    let width = name_w + tab_rest(ui, badge.map(|(text, _)| text.as_str()), closable);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HEADER), Sense::hover());
    let r = ui.interact(rect, egui::Id::new(("shelf-tab", salt)), Sense::click());

    // Над вкладкой держат карточку: подложка и пунктир акцентом.
    if edit.is_some() && !selected && r.dnd_hover_payload::<Dragged>().is_some() {
        let pill = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 2.0, 32.0));
        ui.painter().rect_filled(pill, 6, p.hover);
        dashed_rounded_rect(ui.painter(), pill.shrink(0.75), 6.0, Stroke::new(1.5, p.accent));
    }
    let color = if selected || r.hovered() { p.text } else { p.weak };
    let font = if selected { bold } else { egui::FontId::proportional(13.5) };
    let galley = if cut {
        let mut job = egui::text::LayoutJob::simple_singleline(name.to_owned(), font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(name_w);
        ui.painter().layout_job(job)
    } else {
        ui.painter().layout_no_wrap(name.to_owned(), font, color)
    };
    let text_at =
        Pos2::new(rect.left() + 11.0 + (name_w - galley.size().x) / 2.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_at, galley, color);
    if selected {
        let bar = Rect::from_min_max(
            Pos2::new(rect.left() + 9.0, rect.bottom() - 2.0),
            Pos2::new(rect.right() - 9.0, rect.bottom()),
        );
        ui.painter().rect_filled(bar, 1, p.accent);
    }
    if let Some((text, _)) = badge {
        let at = Pos2::new(rect.left() + 11.0 + name_w + 7.0, rect.center().y - 9.5);
        let pill = Rect::from_min_size(at, Vec2::new(badge_w, 19.0));
        ui.painter().rect_filled(pill, 10, p.badge_fill(p.accent));
        ui.painter().text(pill.center(), egui::Align2::CENTER_CENTER, text, mono, p.accent_text);
    }
    w::focus_ring(ui, Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 32.0)), &r, 6);
    let label = match badge {
        Some((_, hint)) => format!("{name} — {hint}"),
        None => name.to_string(),
    };
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &label));

    // Крестик «убрать полку» — у вкладки под мышью (и у той, где он в фокусе).
    let mut close = false;
    if closable {
        let cr = Rect::from_center_size(Pos2::new(rect.right() - 11.0 - 8.0, rect.center().y), Vec2::splat(18.0));
        let c = ui.interact(cr, egui::Id::new(("shelf-close", salt)), Sense::click());
        if r.contains_pointer() || c.has_focus() {
            if c.hovered() {
                ui.painter().rect_filled(cr, 4, p.hover);
            }
            let color = if c.hovered() { p.text } else { p.weak };
            anvil_ui::icons::paint(
                ui.painter(),
                Rect::from_center_size(cr.center(), Vec2::splat(12.0)),
                Icon::Close,
                color,
            );
        }
        w::focus_ring(ui, cr, &c, 4);
        c.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, t.shelf_remove));
        close = c.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(t.shelf_remove).clicked();
    }

    let (f2, delete) = ui.input(|i| (i.key_pressed(egui::Key::F2), i.key_pressed(egui::Key::Delete)));
    let focused = edit.is_some() && r.has_focus();
    // Переименовать — только в правке: вне её двойной щелчок по вкладке ничего не делает (щелчок открывает полку).
    let rename = edit.is_some() && (r.double_clicked() || (focused && f2));
    close |= closable && focused && delete;
    // Брошенная на вкладку открытой полки карточка остаётся где была: переносить некуда.
    let dropped = if edit.is_some() && !selected { r.dnd_release_payload::<Dragged>().map(|d| d.0) } else { None };
    // Подсказка: полное имя, если подпись обрезана, и то, что вкладка умеет.
    let hint = match (edit, badge) {
        (Some(_), _) => Some(t.shelf_rename.to_string()),
        (None, Some((_, hint))) => Some(hint.clone()),
        (None, None) => None,
    };
    let hint = match (cut, hint) {
        (true, Some(h)) => Some(format!("{name} — {h}")),
        (true, None) => Some(name.to_string()),
        (false, h) => h,
    };
    let mut r = r.on_hover_cursor(CursorIcon::PointingHand);
    if let Some(h) = hint {
        r = r.on_hover_text(h);
    }
    TabOut { rect, click: r.clicked(), rename, close, dropped }
}

/// Правка полок: поверх карточки — вуаль, пунктирная рамка и непрозрачная полоса с ручкой, именем и крестиком.
/// Это свой слой (`egui::Area`): пока он лежит над карточкой, щелчки, колесо мыши и наведение до модуля не
/// доходят. `rect` — карточка, `view` — видимая часть ряда: за неё слой не рисуется и не отвечает.
fn edit_overlay(ctx: &egui::Context, card: &EditCard, rect: Rect, view: Rect, alpha: f32, ops: &mut Vec<Op>) {
    let t = texts::tr(card.lang);
    let p = Palette::of_ctx(ctx);
    let key = card.key;
    // Черта «куда встанет» стоит в зазоре между карточками — у первой и последней это уже за краем видимого.
    let clip = view.expand2(Vec2::new(8.0, 0.0));
    egui::Area::new(egui::Id::new(("edit-card", key)))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .constrain_to(clip)
        .constrain(false)
        .fade_in(false)
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            let (r, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
            // Куда бросают карточку: на эту — встанет на её место. Заводим первой, чтобы ручка и крестик были выше.
            let drop = ui.interact(r, egui::Id::new(("edit-drop", key)), Sense::hover());
            let painter = ui.painter().clone();

            // Вуаль под полосой; рамка пунктиром (под ней сплошная `surface` закрывает рамку самой карточки).
            let alpha_veil = if p.dark { 189 } else { 199 };
            let veil = Color32::from_rgba_unmultiplied(p.surface.r(), p.surface.g(), p.surface.b(), alpha_veil);
            let lower = Rect::from_min_max(Pos2::new(r.left(), r.top() + EDIT_STRIP + 1.0), r.max);
            painter.rect_filled(lower, CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 }, veil);
            let strip = Rect::from_min_max(r.min, Pos2::new(r.right(), r.top() + EDIT_STRIP));
            painter.rect_filled(strip, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, p.card);
            painter.hline(r.x_range(), r.top() + EDIT_STRIP + 0.5, Stroke::new(1.0, p.border));
            painter.rect_stroke(r, 10, Stroke::new(2.0, p.surface), StrokeKind::Inside);
            dashed_rounded_rect(&painter, r.shrink(0.75), 10.0, Stroke::new(1.5, p.border_strong));

            // Ручка слева, крестик справа, имя между ними.
            let mid = r.top() + 22.5;
            let grip = Rect::from_center_size(Pos2::new(r.left() + 9.5 + 14.0, mid), Vec2::splat(28.0));
            let close = Rect::from_center_size(Pos2::new(r.right() - 9.5 - 14.0, mid), Vec2::splat(28.0));

            let g = ui.interact(grip, egui::Id::new(("edit-grip", key)), Sense::click_and_drag());
            g.dnd_set_drag_payload(Dragged(key));
            let hot = g.hovered() || g.dragged() || g.has_focus();
            let (fill, edge) = (if hot { p.hover } else { p.card }, Stroke::new(1.0, p.border_strong));
            painter.rect(grip, 6, fill, edge, StrokeKind::Inside);
            paint_grip(&painter, grip.center(), p.text);
            w::focus_ring(ui, grip, &g, 6);
            let drag_name = texts::drag_card(card.lang, card.title);
            g.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &drag_name));
            let g = g.on_hover_text(t.grip_hint).on_hover_cursor(CursorIcon::Grab);
            if g.has_focus() {
                // С клавиатуры: Alt + ←/→ — сдвиг в ряду, Alt + цифра — на полку с этим номером, Delete — убрать.
                let (left, right, delete, digit) = ui.input(|i| {
                    let alt = i.modifiers.alt;
                    (
                        alt && i.key_pressed(egui::Key::ArrowLeft),
                        alt && i.key_pressed(egui::Key::ArrowRight),
                        i.key_pressed(egui::Key::Delete),
                        if alt { NUM_KEYS.iter().position(|k| i.key_pressed(*k)) } else { None },
                    )
                });
                if left && card.pos > 0 {
                    ops.push(Op::Move { key, at: card.pos - 1 });
                }
                if right && card.pos + 1 < card.order.len() {
                    ops.push(Op::Move { key, at: card.pos + 1 });
                }
                if delete {
                    ops.push(Op::Hide(key));
                }
                if let Some(n) = digit
                    && n < card.shelves
                    && n != card.open
                {
                    ops.push(Op::ToShelf { key, shelf: n });
                }
            }

            let left = grip.right() + 8.0;
            let mut job =
                egui::text::LayoutJob::simple_singleline(card.title.to_owned(), anvil_ui::semibold(13.5), p.text);
            job.wrap = egui::text::TextWrapping::truncate_at_width((close.left() - 8.0 - left).max(20.0));
            let galley = painter.layout_job(job);
            painter.galley(Pos2::new(left, mid - galley.size().y / 2.0), galley, p.text);

            let c = ui.interact(close, egui::Id::new(("edit-close", key)), Sense::click());
            let fill = if c.hovered() { p.hover } else { p.card };
            painter.rect(close, 6, fill, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
            let cross = Rect::from_center_size(close.center(), Vec2::splat(14.0));
            anvil_ui::icons::paint(&painter, cross, Icon::Close, p.text);
            w::focus_ring(ui, close, &c, 6);
            let remove_name = texts::remove_card(card.lang, card.title);
            c.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &remove_name));
            if c.on_hover_text(remove_name.as_str()).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                ops.push(Op::Hide(key));
            }

            // Тащат другую карточку этого ряда: черта в зазоре, куда она встанет (вправо — после этой, влево — перед).
            if let Some(d) = drop.dnd_hover_payload::<Dragged>()
                && d.0 != key
                && let Some(from) = card.order.iter().position(|k| *k == d.0)
            {
                let x = if from < card.pos { r.right() + 5.0 } else { r.left() - 5.0 };
                painter.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(2.0, p.accent));
            }
            if let Some(d) = drop.dnd_release_payload::<Dragged>()
                && d.0 != key
            {
                ops.push(Op::Move { key: d.0, at: card.pos });
            }
        });
}

// ─── Тесты правки полок ─────────────────────────────────────────────────────
//
// Панель без окна: egui гоняет кадры `App::draw` на ввод, который сочиняет тест (мышь, клавиши) — так проверяются
// и перетаскивание карточек, и клавиатура, и то, что вуаль правки не пускает ввод до модулей. Координаты берутся у
// самих виджетов (`Context::read_response`), а не считаются вручную.

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, PointerButton};

    /// Ширина и высота панели в тестах, в точках.
    const SIZE: Vec2 = Vec2::new(1300.0, 340.0);

    impl App {
        /// Панель для тестов: как после `new`, но без окна, горячих клавиш и чтения системы.
        fn for_test(settings: Settings) -> App {
            App {
                settings,
                edge: Edge::default(),
                modules: modules::all(),
                toasts: Toasts::default(),
                view: View::Cards,
                monitors: Vec::new(),
                monitors_at: None,
                placed: None,
                drawn: 1.0,
                region: None,
                focused: true,
                grip: None,
                refreshed: None,
                autostart: false,
                started: true,
                capture: None,
                sent_keys: None,
                capture_panel: false,
                sent_panel: None,
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
                header_right: 0.0,
                header_edges: (0.0, f32::MAX),
                shelf: 0,
                editing: None,
                renaming: None,
                color: ColorEdit::new(look::LIME.dark.fill),
                no_activate: None,
                shot: None,
            }
        }
    }

    struct Rig {
        ctx: egui::Context,
        app: App,
        time: f64,
        mouse: Pos2,
        size: Vec2,
    }

    impl Rig {
        fn new() -> Rig {
            Rig::sized(SIZE.x)
        }

        /// Панель другой ширины (высота та же).
        fn sized(width: f32) -> Rig {
            let settings = Settings { width, height: SIZE.y, ..Settings::default() };
            let ctx = egui::Context::default();
            anvil_ui::install(&ctx, look::accent_of(&settings.accent), settings.common.theme);
            let size = Vec2::new(width, SIZE.y);
            let mut rig = Rig { ctx, app: App::for_test(settings), time: 0.0, mouse: Pos2::new(5.0, 5.0), size };
            rig.settle();
            rig
        }

        /// Правка уже открыта.
        fn editing() -> Rig {
            let mut rig = Rig::new();
            rig.app.start_edit();
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
            let app = &mut self.app;
            let mut out = self.ctx.run_ui(input, |ui| app.draw(ui));
            // Рисовать некому: изменения текстур (шрифты) никто не применит, и egui без `clear` на этом упадёт.
            out.textures_delta.clear();
        }

        /// Несколько кадров без ввода: слои и раскладка успокаиваются (у нового слоя первый кадр — на замер).
        fn settle(&mut self) {
            for _ in 0..4 {
                self.frame(Vec::new());
            }
        }

        fn rect(&self, id: egui::Id) -> Rect {
            self.ctx.read_response(id).unwrap_or_else(|| panic!("виджета {id:?} нет на экране")).rect
        }

        fn grip(&self, key: &str) -> Rect {
            self.rect(egui::Id::new(("edit-grip", key)))
        }

        fn drop_zone(&self, key: &str) -> Rect {
            self.rect(egui::Id::new(("edit-drop", key)))
        }

        fn tab(&self, i: usize) -> Rect {
            self.rect(egui::Id::new(("shelf-tab", i)))
        }

        fn move_to(&mut self, pos: Pos2) {
            self.mouse = pos;
            self.frame(vec![Event::PointerMoved(pos)]);
        }

        fn button(&self, pressed: bool) -> Event {
            Event::PointerButton {
                pos: self.mouse,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }
        }

        fn click(&mut self, pos: Pos2) {
            self.move_to(pos);
            self.frame(vec![self.button(true)]);
            self.frame(vec![self.button(false)]);
            self.settle();
        }

        fn double_click(&mut self, pos: Pos2) {
            self.click(pos);
            self.move_to(pos);
            self.frame(vec![self.button(true)]);
            self.frame(vec![self.button(false)]);
            self.settle();
        }

        /// Взять ручку и отвести к `to`: несколько шагов, чтобы egui признал это перетаскиванием, и отпустить.
        fn drag(&mut self, from: Pos2, to: Pos2) {
            self.move_to(from);
            self.frame(vec![self.button(true)]);
            for step in 1..=10 {
                self.move_to(from + (to - from) * (step as f32 / 10.0));
            }
            self.move_to(to);
            self.frame(vec![self.button(false)]);
            self.settle();
        }

        fn key(&mut self, key: Key, modifiers: Modifiers) {
            let ev = |pressed| Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers };
            // Как egui-winit: сначала «нажали модификатор», потом клавишу.
            self.frame(vec![Event::ModifiersChanged(modifiers), ev(true)]);
            self.frame(vec![ev(false), Event::ModifiersChanged(Modifiers::NONE)]);
            self.settle();
        }

        /// Прямоугольник виджета, у которого сейчас фокус клавиатуры.
        fn focused_rect(&self) -> Option<Rect> {
            let id = self.ctx.memory(|m| m.focused())?;
            self.ctx.read_response(id).map(|r| r.rect)
        }

        fn cards(&self, shelf: usize) -> Vec<&str> {
            self.app.settings.shelves[shelf].cards.iter().map(String::as_str).collect()
        }
    }

    const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };
    const ALT: Modifiers = Modifiers { alt: true, ctrl: false, shift: false, mac_cmd: false, command: false };

    #[test]
    fn edit_veil_takes_input_from_modules() {
        let mut rig = Rig::editing();
        let card = rig.drop_zone("display");
        // Над карточкой — слой правки, а не панель: щелчки, колесо и наведение до модуля не дойдут.
        let inside = card.center() + Vec2::new(0.0, 30.0);
        rig.move_to(inside);
        assert_eq!(rig.ctx.layer_id_at(inside).map(|l| l.order), Some(egui::Order::Middle));
        let below = egui::LayerId::background();
        assert!(!rig.ctx.rect_contains_pointer(below, card), "нижний слой не видит мышь над вуалью");
        // Вне правки — наоборот.
        rig.app.finish_edit();
        rig.settle();
        rig.move_to(inside);
        assert_eq!(rig.ctx.layer_id_at(inside).map(|l| l.order), Some(egui::Order::Background));
    }

    #[test]
    fn edit_veil_blocks_clicks_to_modules() {
        // Первая кнопка «Таймеров» («5 мин»): щелчок заводит таймер, и модуль просит кадры (`ticking`).
        let ticking = |rig: &Rig| rig.app.modules.iter().any(|m| m.ticking());
        // Контроль: вне правки щелчок кнопку нажимает (пять карточек поровну; «Таймеры» — четвёртая).
        let mut rig = Rig::new();
        rig.app.set_shelf(2);
        rig.settle();
        let width = (SIZE.x - 24.0 - 4.0 * GAP) / 5.0;
        let timers = 12.0 + 3.0 * (width + GAP);
        assert!(!ticking(&rig));
        rig.click(Pos2::new(timers + 16.0 + 40.0, BODY_TOP + 70.0));
        assert!(ticking(&rig), "вне правки кнопка работает — иначе проверка ниже ничего не стоит");
        // В правке — тот же щелчок в самую кнопку (карточку находим по её слою) до модуля не доходит.
        let mut rig = Rig::new();
        rig.app.set_shelf(2);
        rig.app.start_edit();
        rig.settle();
        let card = rig.drop_zone("timers");
        rig.click(Pos2::new(card.left() + 16.0 + 40.0, card.top() + 70.0));
        assert!(!ticking(&rig), "в правке щелчок до кнопки не дошёл");
    }

    /// Колесо мыши: `lines` строк по вертикали, с модификаторами `modifiers`, над точкой `pos`.
    impl Rig {
        fn wheel(&mut self, pos: Pos2, lines: f32, modifiers: Modifiers) {
            self.move_to(pos);
            let wheel = Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0.0, lines),
                phase: egui::TouchPhase::Move,
                modifiers,
            };
            self.frame(vec![Event::ModifiersChanged(modifiers), wheel]);
            self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
            self.settle();
        }

        /// Левый край первой карточки полки на экране; `None` — её уже не видно (ряд пролистан).
        fn first_left(&self) -> Option<f32> {
            let key = self.app.settings.shelves[self.app.shelf].cards.first()?;
            self.ctx.read_response(egui::Id::new(("edit-drop", key.as_str()))).map(|r| r.rect.left())
        }

        /// Где «Ещё N ›»: у правого края ряда, посередине высоты.
        fn more_button(&self) -> Pos2 {
            Pos2::new(self.size.x - 12.0 - 20.0, (BODY_TOP + (self.size.y - BODY_BOTTOM - 12.0)) / 2.0)
        }
    }

    #[test]
    fn profile_key_leaves_editing_and_shows_the_confirm() {
        let dev = r"\\.\TEST-PROFILE-KEY";
        let mut rig = Rig::new();
        let rect = monitor::PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
        rig.app.settings.monitor = dev.into();
        let screen = config::ProfileMonitor { device: dev.into(), width: 2560, height: 1440, hz: 144, scale: None };
        rig.app.settings.profiles = vec![config::Profile { name: "P".into(), hotkey: None, monitors: vec![screen] }];
        // «Экран» — на первой полке; открыта третья, и идёт правка.
        rig.app.set_shelf(2);
        rig.app.start_edit();
        rig.settle();
        let _one_at_a_time = sys::KEYS_TEST.lock().unwrap_or_else(|e| e.into_inner());
        sys::press_profile(0);
        rig.app.profile_keys(&rig.ctx, dev, Instant::now());
        assert!(rig.app.editing.is_none(), "клавиша профиля вывела из правки");
        assert!(rig.app.modules.iter().any(|m| m.busy()), "режим сменился — ждём «Оставить?»");
        assert_eq!(rig.app.shelf, 0, "открылась полка, где «Экран»: вопрос виден");
        // Щелчок по чипу профиля с другой полки: тот же вывод — открывается полка с вопросом.
        let mut rig = Rig::new();
        rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
        rig.app.settings.monitor = dev.into();
        let other = config::ProfileMonitor { device: dev.into(), width: 1920, height: 1080, hz: 120, scale: None };
        rig.app.settings.profiles = vec![config::Profile { name: "Q".into(), hotkey: None, monitors: vec![other] }];
        rig.app.set_shelf(1);
        rig.app.command_all(Command::ApplyProfile(0));
        assert_eq!(rig.app.shelf, 0, "после щелчка по чипу профиля — тоже");
    }

    #[test]
    fn closing_the_app_reverts_an_unanswered_mode() {
        let dev = r"\\.\TEST-CLOSE";
        let mut rig = Rig::new();
        let rect = monitor::PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
        rig.app.settings.monitor = dev.into();
        let start = sys::display::current(dev).expect("выдуманный экран отвечает");
        let start_scale = sys::display::scale(dev).map(|s| s.current);
        let screen =
            config::ProfileMonitor { device: dev.into(), width: 1920, height: 1080, hz: 144, scale: Some(200) };
        rig.app.settings.profiles = vec![config::Profile { name: "P".into(), hotkey: None, monitors: vec![screen] }];
        rig.app.command_all(Command::ApplyProfile(0));
        assert!(rig.app.modules.iter().any(|m| m.busy()), "режим сменился, ответа нет");
        assert_ne!(sys::display::current(dev), Some(start));
        assert_eq!(sys::display::saved(dev), start, "в реестр неподтверждённое не попало");
        // Окно закрывается (`eframe::App::on_exit` зовёт `shutdown`): прежний режим и масштаб вернулись.
        rig.app.shutdown();
        assert!(!rig.app.modules.iter().any(|m| m.busy()));
        assert_eq!(sys::display::current(dev), Some(start));
        assert_eq!(sys::display::scale(dev).map(|s| s.current), start_scale);
        assert_eq!(sys::display::saved(dev), start);
    }

    #[test]
    fn header_parts_do_not_overlap_when_narrow() {
        let profile = |name: &str| config::Profile { name: name.into(), hotkey: None, monitors: Vec::new() };
        for width in [560.0, 640.0, 700.0, 900.0, 1300.0] {
            for editing in [false, true] {
                let mut rig = Rig::sized(width);
                rig.app.settings.profiles = vec![profile("Работа"), profile("Игры"), profile("Кино")];
                rig.app.settings.shelves[0].name = "Очень длинное имя полки".into();
                if editing {
                    rig.app.start_edit();
                }
                rig.settle();
                let (left, right) = rig.app.header_edges;
                assert!(
                    left <= right - 8.0,
                    "{width}/{editing}: левая часть шапки наезжает на правую: {left} > {right}"
                );
                for i in 0..3 {
                    let tab = rig.tab(i);
                    assert!(
                        tab.right() <= right - 8.0,
                        "{width}/{editing}: вкладка {i} {tab:?} заходит на правую часть {right}"
                    );
                    assert!(tab.width() > 30.0, "{width}/{editing}: вкладка {i} пропала");
                }
            }
        }
    }

    #[test]
    fn narrow_header_cuts_names_and_drops_the_word() {
        let mut wide = Rig::sized(1300.0);
        wide.app.settings.shelves[0].name = "Очень длинное имя полки".into();
        wide.settle();
        let natural = wide.tab(0).width();
        let mut rig = Rig::sized(560.0);
        rig.app.settings.shelves[0].name = "Очень длинное имя полки".into();
        rig.settle();
        assert!(rig.tab(0).width() < natural, "подпись обрезана многоточием");
        // Слово «Antresol» на узкой панели ушло, значок остался: вкладки начинаются левее, чем на широкой.
        assert!(rig.tab(0).left() < wide.tab(0).left() - 40.0);
        // Много полок с длинными именами и профили: ничего не наезжает и в самом тесном случае (девять вкладок).
        let mut rig = Rig::sized(560.0);
        for i in 0..9 {
            if rig.app.settings.shelves.len() <= i {
                rig.app
                    .settings
                    .shelves
                    .push(config::Shelf {
                        name: format!("Полка с длинным именем {i}"), cards: Vec::new()
                    });
            }
        }
        rig.settle();
        let (left, right) = rig.app.header_edges;
        assert!(left <= right + 0.5, "{left} > {right}");
    }

    #[test]
    fn fit_widths_rules() {
        // Поровну по долям.
        assert_eq!(fit_widths(300.0, &[(1.0, 50.0), (2.0, 50.0)]), [100.0, 200.0]);
        // Кому доля мала, встаёт на минимум, остальные делят остаток.
        assert_eq!(fit_widths(1000.0, &[(1.0, 400.0), (1.0, 100.0), (1.0, 100.0)]), [400.0, 300.0, 300.0]);
        // Несколько сразу.
        assert_eq!(fit_widths(900.0, &[(1.0, 400.0), (1.0, 350.0), (1.0, 100.0)]), [400.0, 350.0, 150.0]);
        // Все минимумы не помещаются — все на минимуме, ряд листается.
        assert_eq!(fit_widths(500.0, &[(1.0, 300.0), (1.0, 300.0)]), [300.0, 300.0]);
        assert!(fit_widths(0.0, &[]).is_empty());
        // Помещается — ровно в свободное место, и никто не меньше минимума.
        let items = [(1.8, 420.0), (1.0, 220.0), (1.0, 240.0), (1.0, 220.0), (1.0, 220.0)];
        let w = fit_widths(1400.0, &items);
        assert!((w.iter().sum::<f32>() - 1400.0).abs() < 0.01);
        assert!(w.iter().zip(items).all(|(w, (_, min))| *w >= min - 0.001), "{w:?}");
    }

    #[test]
    fn edit_row_fits_on_every_default_shelf_at_1300() {
        let mut rig = Rig::editing();
        for shelf in 0..3 {
            rig.app.set_shelf(shelf);
            rig.settle();
            let removed = rig.rect(egui::Id::new("removed-block"));
            assert!(removed.right() <= SIZE.x - 12.0 + 0.5, "полка {shelf}: «Убранные» за краем — {removed:?}");
            assert!(removed.width() >= REMOVED_MIN_WIDTH - 0.5, "полка {shelf}: {removed:?}");
            for key in rig.app.settings.shelves[shelf].cards.clone() {
                assert!(rig.drop_zone(&key).width() >= EDIT_MIN_WIDTH - 0.5, "полка {shelf}: «{key}» уже наименьшей");
            }
            assert_eq!(rig.first_left(), Some(12.0), "полка {shelf}: ряд не листается");
        }
    }

    #[test]
    fn more_button_and_shift_wheel_scroll_the_row_while_editing() {
        for width in [780.0, 1000.0] {
            for shelf in 0..3 {
                let mut rig = Rig::sized(width);
                rig.app.set_shelf(shelf);
                rig.app.start_edit();
                rig.settle();
                let cards = rig.app.settings.shelves[shelf].cards.clone();
                // Ряд вместе с «Убранными» при наименьших ширинах не помещается — тогда он листается.
                let mins = cards.len() as f32 * EDIT_MIN_WIDTH + REMOVED_MIN_WIDTH + cards.len() as f32 * GAP;
                let before = rig.first_left().expect("первая карточка видна");
                if mins <= width - 24.0 {
                    assert_eq!(before, 12.0, "{width}/{shelf}: помещается — не листается");
                    continue;
                }
                // «Ещё N ›» над слоями правки: щелчок листает ряд.
                rig.click(rig.more_button());
                let after = rig.first_left();
                assert!(
                    after.is_none_or(|x| x < before - 20.0),
                    "{width}/{shelf}: «Ещё» не листает: {before} → {after:?}"
                );
                // Shift + колесо над карточкой в правке: тоже листает (слой правки колесо не пускал).
                let mut rig = Rig::sized(width);
                rig.app.set_shelf(shelf);
                rig.app.start_edit();
                rig.settle();
                let over = rig.drop_zone(&cards[0]).center();
                rig.wheel(over, -3.0, Modifiers::SHIFT);
                let after = rig.first_left();
                assert!(
                    after.is_none_or(|x| x < before - 20.0),
                    "{width}/{shelf}: колесо с Shift не листает: {before} → {after:?}"
                );
                // Без Shift колесо ряд не двигает.
                let mut rig = Rig::sized(width);
                rig.app.set_shelf(shelf);
                rig.app.start_edit();
                rig.settle();
                let over = rig.drop_zone(&cards[0]).center();
                rig.wheel(over, -3.0, Modifiers::NONE);
                assert_eq!(rig.first_left(), Some(before), "{width}/{shelf}: колесо без Shift ряд не листает");
            }
        }
    }

    #[test]
    fn fade_at_the_edge_does_not_catch_the_mouse() {
        // Ряд не помещается: справа затухание и «Ещё N ›». Затухание мышь не ловит — карточка под ним остаётся живой,
        // а над самой кнопкой лежит её слой.
        let mut rig = Rig::sized(780.0);
        rig.settle();
        let above_button = Pos2::new(rig.size.x - 12.0 - 60.0, BODY_TOP + 20.0);
        rig.move_to(above_button);
        assert_eq!(rig.ctx.layer_id_at(above_button).map(|l| l.order), Some(egui::Order::Background));
        let button = rig.more_button();
        rig.move_to(button);
        assert_eq!(rig.ctx.layer_id_at(button).map(|l| l.order), Some(egui::Order::Foreground));
    }

    #[test]
    fn drag_card_within_the_row() {
        let mut rig = Rig::editing();
        assert_eq!(rig.cards(0), ["display", "brightness", "sound", "media"]);
        // «Яркость» вправо, на «Звук»: встаёт после него.
        let (from, to) = (rig.grip("brightness").center(), rig.drop_zone("sound").center());
        rig.drag(from, to);
        assert_eq!(rig.cards(0), ["display", "sound", "brightness", "media"]);
        // «Медиа» влево, на «Экран»: встаёт перед ним.
        let (from, to) = (rig.grip("media").center(), rig.drop_zone("display").center());
        rig.drag(from, to);
        assert_eq!(rig.cards(0), ["media", "display", "sound", "brightness"]);
        // На себя — ничего.
        let (from, to) = (rig.grip("sound").center(), rig.drop_zone("sound").center());
        rig.drag(from, to);
        assert_eq!(rig.cards(0), ["media", "display", "sound", "brightness"]);
    }

    #[test]
    fn drag_card_onto_another_shelf_tab() {
        let mut rig = Rig::editing();
        let (from, to) = (rig.grip("display").center(), rig.tab(1).center());
        rig.drag(from, to);
        assert_eq!(rig.cards(0), ["brightness", "sound", "media"]);
        assert_eq!(rig.cards(1).last(), Some(&"display"), "в конец той полки");
        assert_eq!(rig.app.shelf, 0, "сама полка не переключается");
        // На вкладку открытой полки — ничего.
        let (from, to) = (rig.grip("sound").center(), rig.tab(0).center());
        rig.drag(from, to);
        assert_eq!(rig.cards(0), ["brightness", "sound", "media"]);
    }

    #[test]
    fn drag_card_into_removed() {
        let mut rig = Rig::editing();
        let (from, to) = (rig.grip("sound").center(), rig.rect(egui::Id::new("removed-block")).center());
        rig.drag(from, to);
        assert_eq!(rig.cards(0), ["display", "brightness", "media"]);
        assert!(rig.app.settings.is_hidden("sound"));
    }

    #[test]
    fn cross_removes_and_chip_returns() {
        let mut rig = Rig::editing();
        let cross = rig.rect(egui::Id::new(("edit-close", "brightness"))).center();
        rig.click(cross);
        assert_eq!(rig.cards(0), ["display", "sound", "media"]);
        assert!(rig.app.settings.is_hidden("brightness"));
        // Чип в «Убранных»: щелчок — в конец открытой полки.
        let block = rig.rect(egui::Id::new("removed-block"));
        rig.click(Pos2::new(block.left() + 14.0 + 40.0, block.top() + 14.0 + 40.0));
        assert_eq!(rig.cards(0), ["display", "sound", "media", "brightness"]);
        assert!(!rig.app.settings.is_hidden("brightness"));
    }

    #[test]
    fn undo_and_done() {
        let mut rig = Rig::editing();
        let before = rig.app.settings.clone();
        let cross = rig.rect(egui::Id::new(("edit-close", "display"))).center();
        rig.click(cross);
        assert_ne!(rig.app.settings, before);
        rig.app.undo_edit();
        rig.settle();
        assert_eq!(rig.app.settings, before, "«Как было» возвращает раскладку с входа");
        assert!(rig.app.editing.is_some(), "а правка продолжается");
        // Esc — как «Готово»: правка кончилась, раскладка осталась.
        rig.app.settings.hide_card("tools");
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.editing.is_none());
        assert!(rig.app.settings.is_hidden("tools"));
    }

    #[test]
    fn rename_a_shelf() {
        let mut rig = Rig::editing();
        rig.double_click(rig.tab(1).center());
        assert!(rig.app.renaming.is_some(), "двойной щелчок по вкладке — поле имени");
        // Заменить всё и записать.
        rig.key(Key::A, CTRL);
        rig.frame(vec![Event::Text("Дом".into())]);
        rig.key(Key::Enter, Modifiers::NONE);
        assert!(rig.app.renaming.is_none());
        assert_eq!(rig.app.settings.shelves[1].name, "Дом");
        // Esc — отмена, и правка не кончается.
        rig.double_click(rig.tab(1).center());
        rig.frame(vec![Event::Text("Лишнее".into())]);
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.renaming.is_none() && rig.app.editing.is_some());
        assert_eq!(rig.app.settings.shelves[1].name, "Дом");
        // Пустое имя не сохраняется.
        rig.double_click(rig.tab(1).center());
        rig.key(Key::A, CTRL);
        rig.key(Key::Backspace, Modifiers::NONE);
        rig.key(Key::Enter, Modifiers::NONE);
        assert_eq!(rig.app.settings.shelves[1].name, "Дом");
    }

    #[test]
    fn double_click_renames_only_while_editing() {
        let mut rig = Rig::new();
        rig.double_click(rig.tab(1).center());
        assert!(rig.app.renaming.is_none(), "вне правки двойной щелчок по вкладке поле имени не открывает");
        assert_eq!(rig.app.shelf, 1, "а полку открывает, как обычный щелчок");
        // F2 на вкладке в фокусе — тоже только в правке.
        rig.ctx.memory_mut(|m| m.request_focus(egui::Id::new(("shelf-tab", 1))));
        rig.settle();
        rig.key(Key::F2, Modifiers::NONE);
        assert!(rig.app.renaming.is_none());
        rig.app.start_edit();
        rig.settle();
        rig.double_click(rig.tab(1).center());
        assert!(rig.app.renaming.is_some(), "в правке — открывает");
    }

    #[test]
    fn edit_keyboard_does_not_reach_modules() {
        let ticking = |rig: &Rig| rig.app.modules.iter().any(|m| m.ticking());
        // Контроль: вне правки Tab доводит фокус до кнопки «Таймеров» (четвёртая карточка третьей полки), Enter заводит таймер.
        let mut rig = Rig::new();
        rig.app.set_shelf(2);
        rig.settle();
        let width = (SIZE.x - 24.0 - 4.0 * GAP) / 5.0;
        let timers = Rect::from_min_size(Pos2::new(12.0 + 3.0 * (width + GAP), BODY_TOP), Vec2::new(width, 280.0));
        let content = Rect::from_min_max(timers.min + Vec2::new(0.0, 50.0), timers.max);
        let mut reached = false;
        for _ in 0..80 {
            rig.key(Key::Tab, Modifiers::NONE);
            if rig.focused_rect().is_some_and(|r| content.contains(r.center())) {
                reached = true;
                break;
            }
        }
        assert!(reached, "вне правки Tab доходит до карточки «Таймеры» — иначе проверка ниже ничего не стоит");
        rig.key(Key::Enter, Modifiers::NONE);
        assert!(ticking(&rig), "Enter на кнопке таймера её нажимает");

        // В правке содержимое карточек не отвечает: фокус ни Tab, ни Shift+Tab, ни стрелки в него не заводят, а Enter и
        // пробел ничего не заводят.
        let mut rig = Rig::new();
        rig.app.set_shelf(2);
        rig.app.start_edit();
        rig.settle();
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        let keys = rig.app.settings.shelves[2].cards.clone();
        for key in keys.iter().map(String::as_str) {
            // С ручки назад и с крестика вперёд: фокус выходит за слой карточки, и егui ищет следующий виджет в соседнем
            // слое — в нём лежит содержимое модуля под вуалью.
            for (from, press) in [("edit-grip", shift), ("edit-close", Modifiers::NONE)] {
                rig.ctx.memory_mut(|m| m.request_focus(egui::Id::new((from, key))));
                rig.settle();
                rig.key(Key::Tab, press);
                // Содержимое карточек — под полосой правки; кадр от кадра ряд может сдвинуться (фокус его прокручивает).
                if let Some(at) = rig.focused_rect() {
                    for card in &keys {
                        // Карточки, что ушли за край ряда, не рисуются — их слоя нет.
                        let Some(area) = rig.ctx.read_response(egui::Id::new(("edit-drop", card.as_str()))) else {
                            continue;
                        };
                        let under = Rect::from_min_max(area.rect.min + Vec2::new(0.0, EDIT_STRIP + 1.0), area.rect.max);
                        assert!(
                            !under.contains(at.center()),
                            "фокус ушёл под вуаль ({from} «{key}», карточка «{card}»): {at:?}"
                        );
                    }
                }
                rig.key(Key::Space, Modifiers::NONE);
                rig.key(Key::Enter, Modifiers::NONE);
                assert!(!ticking(&rig), "в правке клавиша дошла до кнопки таймера ({from} «{key}»)");
            }
        }
    }

    #[test]
    fn shelf_tabs_and_keys() {
        let mut rig = Rig::new();
        assert_eq!(rig.app.shelf, 0);
        rig.click(rig.tab(2).center());
        assert_eq!(rig.app.shelf, 2);
        rig.key(Key::Num1, CTRL);
        assert_eq!(rig.app.shelf, 0);
        rig.key(Key::Tab, CTRL);
        assert_eq!(rig.app.shelf, 1);
        rig.key(Key::Tab, Modifiers { shift: true, ..CTRL });
        assert_eq!(rig.app.shelf, 0);
        rig.key(Key::Tab, Modifiers { shift: true, ..CTRL });
        assert_eq!(rig.app.shelf, 2, "по кругу");
        rig.key(Key::Num9, CTRL);
        assert_eq!(rig.app.shelf, 2, "нет такой полки — остаётся");
    }

    #[test]
    fn grip_keyboard() {
        let mut rig = Rig::editing();
        let id = egui::Id::new(("edit-grip", "brightness"));
        rig.ctx.memory_mut(|m| m.request_focus(id));
        rig.settle();
        rig.key(Key::ArrowRight, ALT);
        assert_eq!(rig.cards(0), ["display", "sound", "brightness", "media"]);
        rig.key(Key::ArrowLeft, ALT);
        rig.key(Key::ArrowLeft, ALT);
        assert_eq!(rig.cards(0), ["brightness", "display", "sound", "media"]);
        rig.key(Key::ArrowLeft, ALT);
        assert_eq!(rig.cards(0), ["brightness", "display", "sound", "media"], "левее некуда");
        rig.key(Key::Num2, ALT);
        assert_eq!(rig.cards(1).last(), Some(&"brightness"), "Alt + 2 — на вторую полку");
        assert!(!rig.cards(0).contains(&"brightness"));
        // Фокус остался на ручке (её `Id` по ключу), но карточки уже нет на открытой полке — берём «Экран».
        let id = egui::Id::new(("edit-grip", "display"));
        rig.ctx.memory_mut(|m| m.request_focus(id));
        rig.settle();
        rig.key(Key::Delete, Modifiers::NONE);
        assert!(rig.app.settings.is_hidden("display"));
    }

    #[test]
    fn remove_a_shelf_by_its_cross() {
        let mut rig = Rig::editing();
        rig.app.set_shelf(2);
        rig.settle();
        let before = rig.app.settings.shelves[2].cards.clone();
        rig.click(rig.rect(egui::Id::new(("shelf-close", 2))).center());
        assert_eq!(rig.app.settings.shelves.len(), 2);
        for card in &before {
            assert!(rig.app.settings.is_hidden(card), "{card} ушла в «Убранные»");
        }
        assert_eq!(rig.app.shelf, 1, "открытой стала соседняя");
        // Единственную полку убрать нельзя: крестика нет.
        rig.app.settings.remove_shelf(1);
        rig.settle();
        assert!(rig.ctx.read_response(egui::Id::new(("shelf-close", 0))).is_none());
    }

    #[test]
    fn empty_shelf_offers_editing() {
        let mut rig = Rig::new();
        rig.app.settings.shelves.push(config::Shelf { name: "Пусто".into(), cards: Vec::new() });
        rig.app.set_shelf(3);
        rig.settle();
        assert!(rig.app.editing.is_none());
        assert!(rig.app.shelf_modules(3).is_empty());
    }
}
