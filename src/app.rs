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

use crate::config::{
    self, ALL_MONITORS, AUTO_MAX_HEIGHT, Hotkey, MAX_HEIGHT, MIN_HEIGHT, MIN_WIDTH, Profile, SceneRow, Settings,
};
use crate::modules::{self, Command, Ctx, Module, anchor};
use crate::panel::{self, Edge, Input};
use crate::plaque::{self, Plaque};
use crate::sys::{self, monitor};
use crate::{look, texts, watch};

mod header_plan;
mod plaque_ui;
mod search_ui;
use search_ui::Search;

/// Радиус нижних углов панели, в точках.
const CORNER: u8 = 14;
/// Шапка: вкладки стоят на её нижней линии; тело — от `BODY_TOP` до `BODY_BOTTOM` над нижним краем.
const HEADER: f32 = 48.0;
const BODY_TOP: f32 = 58.0;
const BODY_BOTTOM: f32 = 16.0;
const GRIP: f32 = 7.0;
/// Верхняя полоса карточки в правке полок: ручка, имя, крестик.
const EDIT_STRIP: f32 = 44.0;
/// Предел сужения подписи вкладки, точек: «мягкий» — пока справа остаются сцены и выбор монитора, «жёсткий» — когда
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
/// Сколько точек плавного колеса над вкладками — как один щелчок колеса: соседняя полка.
const TAB_WHEEL: f32 = 40.0;
/// Задержка выезда в настройках — чипами, мс.
const DELAYS: [u32; 4] = [0, 150, 300, 600];
/// Полоска у края, пока панель спрятана: ширина и высота в точках при размере 100 % (`Settings::strip_scale`).
const STRIP: Vec2 = Vec2::new(120.0, 5.0);
/// Полоска, пока мышь у края и идёт отсчёт выезда: шире и наполняется.
const STRIP_HOT: Vec2 = Vec2::new(160.0, 6.0);
/// Чипы настроек края: зона вдоль края (%, 0 — весь край), толщина зоны (пикс.), размер полоски (%).
const EDGE_ZONES: [u32; 4] = [100, 150, 200, 0];
const EDGE_DEPTHS: [u32; 4] = [1, 2, 4, 8];
const STRIP_SCALES: [u32; 4] = [100, 150, 200, 300];
/// Полоска с медиа: обложка, трек и три кнопки.
const MEDIA_STRIP: Vec2 = Vec2::new(372.0, 34.0);
/// Плашка у края — событие, пока панель спрятана: значок, название и пояснение, до двух кнопок (`crate::plaque`).
const PLAQUE: Vec2 = Vec2::new(plaque::WIDTH, plaque::HEIGHT);
/// Как часто перечитывать систему, пока панель открыта.
const REFRESH: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq)]
enum View {
    Cards,
    Settings,
    /// «Свой цвет…» — вместо настроек.
    Color,
    /// Правка сцены — вместо карточек (шапка «+», правый щелчок по чипу) или настроек («Изменить…»).
    Scene,
    /// Поиск действий — вместо карточек (поле в шапке, Ctrl+K, Ctrl+F, просто начать печатать).
    Search,
}

/// Что видно у края, пока панель спрятана.
#[derive(Clone, Copy, PartialEq, Debug)]
enum StripKind {
    /// Ничего: окно уведено прочь.
    Off,
    Plain,
    Media,
    /// Плашка события (`plaque_drawn`).
    Plaque,
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

/// Правка сцены. Сцена правится на месте — прямо в `settings.profiles`, каждое изменение сохраняется сразу; здесь то, что
/// нужно самому экрану правки.
struct SceneEdit {
    /// Номер сцены в `settings.profiles`.
    index: usize,
    /// Что в поле имени: оно бывает негодным (пусто, такое уже есть) — сцене имя попадает, только пока годное.
    name: String,
    /// Сцена новая («+»): уйти без имени или без отмеченных строк — её не будет.
    fresh: bool,
    /// Сцена на входе: если уйти с негодным именем или без строк, она возвращается такой.
    original: Profile,
    /// Фокус полю имени у новой сцены: 2 — попросить фокус, 1 — выделить имя целиком (поле заводится кадром позже).
    focus: u8,
    /// Висит вопрос «Удалить сцену?».
    confirm_delete: bool,
    /// Куда вернуться: карточки или настройки.
    back: View,
    /// Планы питания (GUID строкой и имя) — для строки «План питания»; читаются на входе.
    plans: Vec<(String, String)>,
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
    /// Ждём сочетание для сцены с этим номером (настройки, правка сцены).
    capture: Option<usize>,
    /// Какие клавиши сцен последними отдали потоку клавиш — шлём заново, только если поменялись.
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
    /// Очередь плашек событий у края: по одной, остальные ждут (`plaque::Queue`).
    plaques: plaque::Queue,
    /// Что рисует окно у края: показанная плашка, а пока она уезжает — ещё и после того, как очередь её отдала.
    plaque_drawn: Option<Plaque>,
    /// Доля выезда плашки в последнем кадре: 0 — у края её нет, 1 — вся на месте.
    plaque_t: f32,
    /// Флешки и загрузки: наблюдатели без карточек (`watch.rs`).
    watch: watch::Watch,
    /// Файл потянули с плашки, и egui не видел, как отпустили кнопку: отпустить за него в следующем кадре (`raw_input_hook`).
    release: Option<Pos2>,
    /// Какой файл тянули с плашки и когда: окно — цель и своего же броска, winit отдаёт его как брошенный на панель, а в лоток
    /// он не идёт (`App::dropped_but_not_own`).
    own_drop: Option<(std::path::PathBuf, Instant)>,
    /// Можно ли сейчас показывать уведомления (`sys::shell::accepts_notifications`) и когда спрашивали.
    notify_ok: bool,
    notify_at: Option<Instant>,
    /// Правка сцены: открыта, пока `view == View::Scene`.
    scene: Option<SceneEdit>,
    /// Номер сцены ([`Profile::id`]), что ставили последней: из подходящих по состоянию чипов выделен этот (иначе первый).
    last_scene: Option<u64>,
    /// В настройках спросили «Удалить сцену?» — номер этой сцены; пока не ответили, строка сцены — вопрос.
    ask_delete: Option<u64>,
    /// Вопрос только что задан: карточка «Сцены» листается к нему (у последней сцены он иначе оказался бы под краем).
    ask_scroll: bool,
    /// «Ещё N»: куда пролистать ряд карточек в следующем кадре.
    scroll_to: Option<f32>,
    /// Высота панели в этом кадре, в точках: идёт к нужной (`target_height`) за `motion::LAYOUT`.
    height: f32,
    /// Сколько нужно каждой полке по замеру её карточек (номер — как в `settings.shelves`); только в памяти.
    fit: Vec<Option<f32>>,
    /// Вид без полки (настройки, поиск, сцена) или правка полок: высота, что держится, пока они открыты.
    held: Option<f32>,
    /// Колесо над вкладками: сколько накрутили, пока не набралось на соседнюю полку.
    tab_wheel: f32,
    /// Что показывает шапка над карточками на этой ширине (`header_plan`): для проверки в тестах.
    header_plan: header_plan::Plan,
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
    /// Поиск действий: запрос и выбранная строка; живёт, пока открыт вид поиска.
    search: Search,
    /// Недавние действия поиска (`Action::id`), самое недавнее первым; только в памяти.
    recent: Vec<String>,
    /// Мышь над самой панелью (с прошлого кадра): «Пипетка» поиска берёт цвет под мышью, и над панелью брать нечего.
    pointer_over: bool,
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
        let height = settings.height;
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
                Ok("settings" | "settings-ask") => View::Settings,
                Ok("color") => View::Color,
                Ok("scene" | "scene-ask" | "scene-name") => View::Scene,
                Ok("search") => View::Search,
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
            plaques: plaque::Queue::default(),
            plaque_drawn: None,
            plaque_t: 0.0,
            watch: watch::Watch::new(),
            release: None,
            own_drop: None,
            notify_ok: true,
            notify_at: None,
            scene: None,
            last_scene: None,
            ask_delete: None,
            ask_scroll: false,
            scroll_to: None,
            height,
            fit: Vec::new(),
            held: None,
            tab_wheel: 0.0,
            header_plan: header_plan::Plan::default(),
            header_edges: (0.0, f32::MAX),
            shelf,
            editing,
            renaming: None,
            color: ColorEdit::new(look::LIME.dark.fill),
            search: Search::default(),
            recent: Vec::new(),
            pointer_over: false,
            no_activate: None,
            shot: crate::shot::Shot::from_env(),
        };
        // Для снимков: `ANTRESOL_VIEW=search` — сразу вид поиска, `ANTRESOL_QUERY=1080` — с этим запросом.
        if app.view == View::Search {
            app.search.query = std::env::var("ANTRESOL_QUERY").unwrap_or_default();
            app.search.focus = true;
        }
        if let Some(v) = strip_view {
            app.edge.pinned = false;
            app.started = true;
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            // Для снимков: `strip-plaque-<вид>` (bluetooth, battery, usb, ejecting, eject, busy, gone, github, downloads,
            // downloads-many, downloads-danger, timer) — плашка с выдуманными данными на языке кадра; обычная замерла на 62 %
            // полоски, как на макете, а «ejecting» — с нажатой кнопкой «Извлечь».
            if let Some(kind) = v.strip_prefix("strip-plaque-")
                && let Some(plaque) = plaque::sample(kind, app.lang())
            {
                app.plaques.hold(plaque, 0.62, Instant::now());
                app.plaques.set_busy(kind == "ejecting");
            }
        }
        if app.view == View::Color {
            app.open_color(&cc.egui_ctx);
        }
        // Для снимков: `ANTRESOL_VIEW=scene` — сразу правка сцены `ANTRESOL_SCENE` (по порядку, с единицы; по умолчанию первой);
        // `scene-ask` — с вопросом «Удалить сцену?», `scene-name` — с именем, что уже есть у другой сцены, `settings-ask` —
        // настройки с тем же вопросом у этой сцены.
        let hook = std::env::var("ANTRESOL_VIEW").unwrap_or_default();
        let n = std::env::var("ANTRESOL_SCENE").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
        let scenes = app.settings.profiles.len();
        let shown = n.saturating_sub(1).min(scenes.saturating_sub(1));
        if app.view == View::Scene {
            app.view = View::Cards;
            if scenes > 0 {
                app.open_scene(shown, false, View::Cards);
            }
        }
        if let Some(edit) = &mut app.scene {
            edit.confirm_delete = hook == "scene-ask";
            if hook == "scene-name"
                && let Some(other) = app.settings.profiles.iter().enumerate().find(|(j, _)| *j != shown)
            {
                edit.name = other.1.name.clone();
            }
        }
        if hook == "settings-ask" && scenes > 0 {
            (app.ask_delete, app.ask_scroll) = (Some(app.settings.profiles[shown].id), true);
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

    /// Какой высоты панели быть: в виде без полки и в правке полок — той, что была на входе; на полке — своей высоты полки, а
    /// нет её — сколько нужно карточкам по замеру (`fit`); пока замера нет — `height=` из файла.
    fn target_height(&self) -> f32 {
        if let Some(h) = self.held {
            return h;
        }
        let own = self.settings.shelves.get(self.shelf).and_then(|s| s.height).map(f32::from);
        let fit = self.fit.get(self.shelf).copied().flatten();
        own.or(fit).unwrap_or(self.settings.height).clamp(MIN_HEIGHT, MAX_HEIGHT)
    }

    /// Высота этого кадра: к нужной за `motion::LAYOUT`; спрятанная панель и панель, которую тянут за край, — сразу.
    fn step_height(&mut self, ctx: &egui::Context) {
        let free = self.view == View::Cards && self.editing.is_none();
        if free {
            self.held = None;
        } else if self.held.is_none() {
            self.held = Some(self.height);
        }
        let target = self.target_height();
        let secs = if self.drawn <= 0.0 || self.grip.is_some() { 0.0 } else { motion::LAYOUT };
        self.height = Motion::of(ctx).value(ctx, egui::Id::new("panel-height"), target, secs);
    }

    /// Ряд открытой полки нарисован: `extra` — сколько не хватило самой высокой карточке (меньше нуля — сколько лишнего).
    /// Нужная полке высота — нынешняя плюс это; она не зависит от нынешней (выросла панель — лишнего столько же меньше),
    /// поэтому не качается. В правке и в видах без полки не меряем: там карточки не те или их нет.
    fn note_fit(&mut self, extra: Option<f32>) {
        let Some(extra) = extra else { return };
        if self.editing.is_some() || self.view != View::Cards {
            return;
        }
        let need = (self.height + extra).round().clamp(MIN_HEIGHT, AUTO_MAX_HEIGHT);
        if self.fit.len() < self.settings.shelves.len() {
            self.fit.resize(self.settings.shelves.len(), None);
        }
        if let Some(slot) = self.fit.get_mut(self.shelf)
            && slot.is_none_or(|h| (h - need).abs() >= 1.0)
        {
            *slot = Some(need);
        }
    }

    /// Поставить окно у края монитора: `t` — доля выезда (0 — целиком выше края: полоска у края или
    /// увести прочь).
    /// Шлёт команды, только если что-то поменялось.
    fn place(&mut self, ctx: &egui::Context, ppp: f32, t: f32) {
        let Some(m) = self.monitor() else { return };
        let r = m.rect;
        let mut size = Vec2::new(self.settings.width, self.height);
        let left = |w: f32| (r.left as f32 / ppp + (r.width() as f32 / ppp - w) / 2.0).round();
        let mut pos = Pos2::new(left(size.x), (r.top as f32 / ppp - (1.0 - t) * size.y).round());
        if t <= 0.0 && self.strip_kind != StripKind::Off {
            size = self.strip_size();
            // Плашка выезжает и уезжает за верхний край монитора (доля `plaque_t`), как панель.
            let slide = if self.strip_kind == StripKind::Plaque { (1.0 - self.plaque_t) * size.y } else { 0.0 };
            pos = Pos2::new(left(size.x), r.top as f32 / ppp - slide);
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
            StripKind::Plaque => 12,
            _ => 3,
        };
        let radius = ((corner as f32 * ppp) as i32).min(px.1);
        if self.region != Some(px) && sys::round_bottom_corners(px.0, px.1, radius) {
            self.region = Some(px);
        }
    }

    /// Дать окну клавиатуру (панель вызвана клавишей, открыта щелчком по плашке, в ней нажали мышью): сначала снять «не
    /// забирать фокус», иначе Windows не активирует окно.
    fn focus(&mut self, ctx: &egui::Context) {
        self.edge.take_keyboard();
        self.set_no_activate(false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Нажали мышью внутри панели, у которой клавиатуры нет (выдвинута мышью у края): окно становится активным и забирает
    /// клавиатуру. Само нажатие работает как обычный щелчок — поля ввода, кнопки, вкладки.
    fn take_keyboard_on_press(&mut self, ctx: &egui::Context) {
        if self.edge.shown && !self.edge.keyboard() && ctx.input(|i| i.pointer.any_pressed()) {
            self.focus(ctx);
        }
    }

    /// Окно не должно активироваться при щелчке (`WS_EX_NOACTIVATE`): панель спрятана или выдвинута мышью у края и клавиатура
    /// осталась у программы пользователя. Появилась клавиатура у панели — стиль снимается.
    fn wants_no_activate(&self) -> bool {
        !(self.edge.shown && self.edge.keyboard())
    }

    /// Окно не забирает фокус при щелчке, пока у панели нет клавиатуры (`wants_no_activate`). Шлёт, только если поменялось.
    fn set_no_activate(&mut self, on: bool) {
        if self.no_activate != Some(on) && sys::set_no_activate(on) {
            self.no_activate = Some(on);
        }
    }

    fn strip_size(&self) -> Vec2 {
        match self.strip_kind {
            StripKind::Media => MEDIA_STRIP,
            StripKind::Plaque => PLAQUE,
            _ => self.plain_size(self.strip_fill.is_some()),
        }
    }

    /// Простая полоска у края в размере из настроек; `hot` — идёт отсчёт выезда.
    fn plain_size(&self, hot: bool) -> Vec2 {
        (if hot { STRIP_HOT } else { STRIP }) * (self.settings.strip_scale as f32 / 100.0)
    }

    /// Мышь над простой полоской у края (спрятанная панель): наведение на полоску выдвигает панель, как край.
    fn over_plain_strip(&self, cursor: (i32, i32), ppp: f32) -> bool {
        if self.edge.shown || self.strip_kind != StripKind::Plain {
            return false;
        }
        let Some((pos, size)) = self.placed else { return false };
        let px = |v: f32| (v * ppp).round() as i32;
        panel::inside(cursor, (px(pos.x), px(pos.y), px(pos.x + size.x), px(pos.y + size.y)))
    }

    /// Отдать команду всем модулям (`device` — монитор панели); их уведомления — на экран, изменённые настройки — в файл.
    fn command_on(&mut self, cmd: &Command, device: &str) {
        let t = texts::tr(self.lang());
        let lang = self.lang();
        let mut toasts = Vec::new();
        let before = self.settings.clone();
        for module in &mut self.modules {
            let mut cx = Ctx { t, lang, monitor: device, toasts: &mut toasts, settings: &mut self.settings };
            module.command(cmd, &mut cx);
        }
        for (text, tone) in toasts {
            self.toasts.push(text, tone);
        }
        if self.settings != before {
            self.save();
        }
    }

    /// То же на мониторе панели.
    fn command_all(&mut self, cmd: Command) {
        let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
        self.command_on(&cmd, &device);
        if matches!(cmd, Command::ApplyScene(_)) {
            self.reveal_busy();
        }
    }

    // ─── Сцены ──────────────────────────────────────────────────────────────

    /// Какая сцена стоит сейчас (шапка выделяет её чип): та, у которой каждый модуль, что может её проверить, ответил «да», и
    /// хоть один ответил. Подходят несколько (одна — часть другой) — берётся та, что ставили последней, иначе первая.
    fn active_scene(&self) -> Option<usize> {
        let stands = |s: &Profile| {
            let parts: Vec<bool> = self.modules.iter().filter_map(|m| m.scene_part(s)).collect();
            !parts.is_empty() && parts.iter().all(|ok| *ok)
        };
        let last = self.last_scene.and_then(|id| self.settings.profiles.iter().position(|p| p.id == id));
        last.filter(|&i| stands(&self.settings.profiles[i])).or_else(|| self.settings.profiles.iter().position(stands))
    }

    /// Какой-то модуль ждёт ответа («Оставить этот режим?»): сцену не ставят и не правят, пока на него не ответят. Говорит
    /// об этом и отвечает — ждёт ли.
    fn scene_blocked(&mut self) -> bool {
        let wait = self.modules.iter().any(|m| m.blocks_scene());
        if wait {
            self.toasts.push(texts::tr(self.lang()).answer_first, Tone::Warning);
        }
        wait
    }

    /// Щелчок по чипу сцены или её клавиша. Стоящая сцена при включённом возврате — вернуть как было (модули помнят, что
    /// было, пока работает программа: нечего — так и сказать; висит чужое «Оставить?» — сначала ответить, и не начинать:
    /// иначе сцена вернулась бы наполовину), при выключенном — сказать, что стоит. Не стоящая — поставить: экран, потом
    /// система; «Оставить этот режим?» ждёт ответа — сначала ответить.
    fn scene_click(&mut self, i: usize, device: &str) {
        let Some(scene) = self.settings.profiles.get(i).cloned() else { return };
        let (t, lang) = (texts::tr(self.lang()), self.lang());
        if self.active_scene() == Some(i) {
            if !scene.undo {
                self.toasts.push(t.scene_already, Tone::Neutral);
            } else if !self.modules.iter().any(|m| m.scene_can_undo(&scene)) {
                self.toasts.push(t.scene_nothing_undo, Tone::Neutral);
            } else if self.modules.iter().any(|m| m.blocks_undo(&scene)) {
                self.toasts.push(t.answer_first, Tone::Warning);
            } else {
                self.command_on(&Command::UndoScene(i), device);
                self.last_scene = None;
                self.toasts.push(t.scene_undone, Tone::Neutral);
            }
            return;
        }
        if self.scene_blocked() {
            return;
        }
        self.toasts.push(texts::scene_set(lang, &scene.name), Tone::Neutral);
        self.last_scene = Some(scene.id);
        self.command_on(&Command::ApplyScene(i), device);
        self.reveal_busy();
    }

    /// Новая сцена («+» в шапке): из того, что стоит сейчас, отмечен только экран; имя «Сцена N» (первый свободный номер).
    fn new_scene(&mut self) {
        if self.scene_blocked() {
            return;
        }
        let lang = self.lang();
        let name = (1usize..)
            .map(|n| texts::scene_n(lang, n))
            .find(|n| !self.settings.profiles.iter().any(|p| &p.name == n))
            .unwrap_or_default();
        self.settings.profiles.push(Profile::new(&name));
        let i = self.settings.profiles.len() - 1;
        self.command_all(Command::SaveScene(i, vec![SceneRow::Screen]));
        self.open_scene(i, true, View::Cards);
    }

    /// Открыть правку сцены `i`; `fresh` — только что заведённая; `back` — куда вернуться.
    fn open_scene(&mut self, i: usize, fresh: bool, back: View) {
        let Some(scene) = self.settings.profiles.get(i).cloned() else { return };
        let plans = crate::sys::system::plans()
            .0
            .into_iter()
            .map(|p| (crate::sys::system::guid_string(&p.guid), p.name))
            .collect();
        self.scene = Some(SceneEdit {
            index: i,
            name: scene.name.clone(),
            fresh,
            original: scene,
            focus: if fresh { 2 } else { 0 },
            confirm_delete: false,
            back,
            plans,
        });
        self.capture = None;
        self.view = View::Scene;
        // Строки правки ждут свежих значений: экран, яркость и быстрые настройки читаются сразу.
        self.refreshed = None;
    }

    /// Что не так с именем сцены `index`: пусто или такое уже есть у другой сцены.
    fn name_problem(&self, name: &str, index: usize) -> Option<&'static str> {
        let t = texts::tr(self.lang());
        let clean = Profile::clean_name(name);
        if clean.is_empty() {
            Some(t.scene_name_empty)
        } else if self.settings.profiles.iter().enumerate().any(|(j, p)| j != index && p.name == clean) {
            Some(t.scene_name_taken)
        } else {
            None
        }
    }

    /// Что мешает выйти из правки сохранив: имя не годится или в сцене ничего не отмечено.
    fn scene_problem(&self) -> Option<&'static str> {
        self.scene.as_ref().and_then(|edit| self.scene_problem_of(edit))
    }

    /// Выйти из правки сцены («Готово», Esc, «‹ Сцены», спряталась панель, нажата клавиша сцены). Сцена правится на месте и
    /// уже сохранена, остаётся убрать негодное: новая без годного имени или без отмеченных строк пропадает; прежняя без
    /// отмеченных строк возвращается такой, какой вошли; прежняя с негодным именем остаётся под последним годным.
    fn finish_scene(&mut self) {
        let Some(edit) = self.scene.take() else { return };
        if let Some(scene) = self.settings.profiles.get(edit.index) {
            let (bad_name, empty) = (self.name_problem(&edit.name, edit.index).is_some(), scene.is_empty());
            if edit.fresh && (bad_name || empty) {
                self.settings.profiles.remove(edit.index);
            } else if empty {
                self.settings.profiles[edit.index] = edit.original.clone();
            }
        }
        self.capture = None;
        if self.view == View::Scene {
            self.view = edit.back;
        }
        self.save();
    }

    /// Что не так с этой правкой (её могли уже снять с `self`).
    fn scene_problem_of(&self, edit: &SceneEdit) -> Option<&'static str> {
        let t = texts::tr(self.lang());
        self.name_problem(&edit.name, edit.index).or_else(|| {
            let empty = self.settings.profiles.get(edit.index).is_none_or(|s| s.is_empty());
            empty.then_some(t.scene_rows_none)
        })
    }

    /// Удалить сцену `i` (корзина в настройках и «Удалить сцену» в правке — оба после вопроса); её клавиша освобождается
    /// сама: набор клавиш сцен окно пересобирает каждый кадр. Модулям говорить нечего: они помнят сцену по номеру, а
    /// номера удалённой больше ни у кого нет.
    fn delete_scene(&mut self, i: usize) {
        if i >= self.settings.profiles.len() {
            return;
        }
        self.settings.profiles.remove(i);
        self.capture = None;
        self.save();
    }

    /// Отказались от режима, что поставила сцена («Вернуть», 15 секунд без ответа, закрытие программы), — это отказ от
    /// сцены целиком: её системная часть возвращается так же, как вторым щелчком (и когда «Повторный щелчок — вернуть как
    /// было» выключен: тот про второй щелчок, а не про отказ), и панель говорит, что вернула.
    fn undo_refused_scenes(&mut self) {
        let refused: Vec<u64> = self.modules.iter_mut().filter_map(|m| m.take_refused_scene()).collect();
        if refused.is_empty() {
            return;
        }
        let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
        let t = texts::tr(self.lang());
        for id in refused {
            let Some(i) = self.settings.profiles.iter().position(|p| p.id == id) else { continue };
            self.command_on(&Command::UndoScene(i), &device);
            if self.last_scene == Some(id) {
                self.last_scene = None;
            }
            self.toasts.push(t.scene_undone, Tone::Neutral);
        }
    }

    /// Что модули делают сами, без щелчка, каждый кадр: следят (`background`: «Оставить?» без ответа 15 секунд — откат,
    /// устройства Bluetooth и прогоны GitHub для плашек), говорят, что случилось (`events`: таймер дошёл, наушники
    /// подключились, сборка упала), говорят, что не вышло в их потоках (`problems`: Wi‑Fi, DDC/CI), — это уведомления, даже
    /// когда карточку не рисуют, — и отдают отказ от сцены (`undo_refused_scenes`). Ответ — события: из них плашки.
    fn modules_background(&mut self) -> Vec<Plaque> {
        let t = texts::tr(self.lang());
        let (mut events, mut problems) = (Vec::new(), Vec::new());
        for module in &mut self.modules {
            module.background(&self.settings);
            events.extend(module.events(&self.settings));
            problems.extend(module.problems(t));
        }
        for (text, tone) in problems {
            self.toasts.push(text, tone);
        }
        self.undo_refused_scenes();
        events
    }

    /// Панели нельзя прятаться самой (ни когда мышь ушла, ни когда окно потеряло фокус): тянут край, ждут сочетание, модуль
    /// ждёт ответа («Оставить этот режим?») или открыт поиск — «Пипетке» из него нужна мышь вне панели, а набранное не
    /// должно пропасть. Поиск закрывают Esc, крестик «Закрыть поиск», клавиша показа панели или выполненная строка.
    fn panel_busy(&self) -> bool {
        self.grip.is_some()
            || self.capture.is_some()
            || self.capture_panel
            || self.view == View::Search
            || self.modules.iter().any(|m| m.busy())
    }

    /// Модуль ждёт ответа (после сцены — «Оставить?» у «Экрана»), а карточки на открытой полке нет — открыть ту полку,
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

    /// Программа закрывается: модули возвращают то, что держали на время (режим экрана до «Оставить»). Не ответили на
    /// «Оставить?» после сцены — это отказ от сцены, как «Вернуть»: её системная часть возвращается тоже (что делает
    /// поток модуля — Wi‑Fi, яркость по DDC/CI, — закрытие может и не дождаться).
    fn shutdown(&mut self) {
        for module in &mut self.modules {
            module.shutdown();
        }
        self.undo_refused_scenes();
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
            let h = ((cy - m.rect.top) as f32 / ppp).clamp(MIN_HEIGHT, MAX_HEIGHT);
            self.set_height(h);
        }
    }

    /// Своя высота: на полке — этой полки, в виде без полки — та, что держится, и `height=`.
    fn set_height(&mut self, h: f32) {
        if self.held.is_some() {
            self.held = Some(h);
            self.settings.height = h;
        } else if let Some(sh) = self.settings.shelves.get_mut(self.shelf) {
            sh.height = Some(h.round() as u16);
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
        // События модулей и наблюдателей (таймер дошёл, наушники подключились, флешка вставлена, сборка упала, файл
        // скачан): панель спрятана — плашка у края, выдвинута — уведомление. Панель при этом не выезжает и клавиатуру не берёт.
        let mut events = self.modules_background();
        let found = self.watch.events(&self.settings);
        events.extend(found.plaques);
        // Флешку вынули — её плашка, показанная или ждущая, уходит (кроме «Можно вынимать»: она свои 4 с доживает).
        for key in found.gone {
            self.plaques.remove(&key);
        }
        for plaque in events {
            self.post(plaque, now);
        }
        self.take_dropped(ctx);
        let cursor = monitor::cursor();
        // «Все мониторы»: пока панель спрятана, она живёт у монитора под курсором. Показанная плашка остаётся на своём
        // мониторе: за мышью, ушедшей на другой, она не прыгает.
        if plaque_ui::follows_mouse(
            self.settings.monitor == ALL_MONITORS,
            self.edge.shown,
            self.drawn,
            self.plaque_drawn.is_some(),
        ) && let Some(m) = self.monitors.iter().find(|m| m.rect.contains(cursor))
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
        let bottom = m.rect.top + (self.height * ppp) as i32;
        // Мышь над плашкой — не у края панели: бросок мыши вверх через плашку не должен выдвигать панель и превращать её в
        // уведомление.
        let over_plaque = self.plaque_drawn.is_some() && plaque::over(cursor, &m.rect, ppp);
        let (zone_left, zone_right) =
            panel::edge_span((m.rect.left, m.rect.right), (left, right), self.settings.edge_zone);
        let near = panel::at_edge(cursor, m.rect.top, self.settings.edge_depth, zone_left, zone_right)
            || self.over_plain_strip(cursor, ppp);
        let at_edge = plaque_ui::counts_as_edge(near, over_plaque);
        let excluded = self.foreground.as_deref().is_some_and(|e| self.settings.excluded(e));
        let fullscreen = self.settings.fullscreen_guard && monitor::fullscreen_on(m.rect);
        self.strip_on = self.settings.strip && !excluded && !fullscreen;
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        let busy = self.panel_busy();
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
        self.pointer_over = input.over;
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
            // Только что выехала: свежие данные. Клавиатуру (фокус) берёт, только если её вызвали клавишей: выдвинутая
            // мышью у края оставляет её программе пользователя, пока в панели не нажмут мышью (`take_keyboard_on_press`).
            self.refreshed = None;
            if self.edge.keyboard() {
                self.focus(ctx);
            }
        }
        if changed && !self.edge.shown {
            if self.view == View::Color {
                self.restore_accent(ctx);
            }
            // Спряталась — правка сцены кончена (негодная сцена убирается), как и настройки; поиск тоже, набранное забыто.
            self.finish_scene();
            self.view = View::Cards;
            self.search = Search::default();
            // Правка полок кончена тоже (раскладка сохраняется по ходу).
            self.editing = None;
            self.renaming = None;
        }
        // Плашкам можно быть на экране, пока панель спрятана совсем, на мониторе не игра во весь экран и программа не из «Не
        // выезжать»; пока пользователю не до уведомлений (показ экрана, «не беспокоить») — только таймеру; мышь над плашкой —
        // её время стоит. Полоска у края от этого не зависит: плашка видна и при выключенной полоске.
        let welcome = self.notices_welcome(now);
        let gate = plaque_ui::may_show(self.edge.shown, self.drawn, excluded, fullscreen, welcome);
        self.plaque_frame(ctx, now, over_plaque, gate);
        self.strip_fill = self.edge.edge_progress(now, show_delay);
        self.playing = if self.settings.media_strip && self.strip_on && !self.edge.shown {
            self.modules.iter_mut().find_map(|m| m.playing(ctx))
        } else {
            None
        };
        self.strip_kind = if self.plaque_drawn.is_some() {
            StripKind::Plaque
        } else if !self.strip_on {
            StripKind::Off
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
        self.step_height(ctx);
        self.set_no_activate(self.wants_no_activate());
        self.place(ctx, ppp, t);

        // Спрятана — смотрим на мышь 25 раз в секунду; открыта — чаще, ради отсчёта ухода.
        let ticking = self.modules.iter().any(|m| m.ticking()) || self.plaque_drawn.is_some();
        let period = if self.edge.shown || self.edge.counting() || ticking { 33 } else { 40 };
        ctx.request_repaint_after(Duration::from_millis(period));
    }

    /// Клавиши сцен: отдать потоку клавиш новый набор, если он поменялся, и выполнить нажатые (`scene_click`: поставить
    /// или вернуть). Нажали при спрятанной панели — она выезжает с вопросом «Оставить?» и с уведомлением. Ответ — выехала ли.
    fn profile_keys(&mut self, ctx: &egui::Context, device: &str, now: Instant) -> bool {
        // Пока ждём сочетание, клавиши сцен сняты: иначе Windows перехватит уже назначенное
        // сочетание раньше окна и вместо захвата сработает сцена.
        // Ждём клавишу панели — тоже снимаем: иначе сочетание сцены сработало бы вместо захвата.
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
        let pressed = sys::take_profile_presses();
        if !pressed.iter().any(|&i| i < self.settings.profiles.len()) {
            return false;
        }
        // Клавишу нажали в правке сцены — сначала выход из неё: негодная сцена уберётся, и номера надо считать заново.
        self.finish_scene();
        // Панель могла быть спрятана — её данные старые: стоит ли сцена, решают по свежему (экран, тема, «Тишина» читаются
        // сразу; яркость и Wi‑Fi читает поток модуля — для них берётся то, что он прочитал в последний раз).
        for key in ["display", "quick", "brightness"] {
            if let Some(i) = self.module_index(key) {
                self.modules[i].refresh(device);
            }
        }
        let count = self.settings.profiles.len();
        for i in pressed.into_iter().filter(|&i| i < count) {
            self.scene_click(i, device);
        }
        let was = self.edge.shown;
        self.edge.reveal(now);
        if self.view == View::Color {
            self.restore_accent(ctx);
        }
        self.view = View::Cards;
        self.search = Search::default();
        // Клавишу нажали в правке — сначала выход из неё: под вуалью «Оставить этот режим?» не нажать, а карточка с ним
        // может стоять на другой полке — её и открываем.
        self.finish_edit();
        self.reveal_busy();
        if !was {
            self.refreshed = None;
            self.focus(ctx);
        }
        !was
    }

    /// Файлы бросили на панель — отдать лотку.
    fn take_dropped(&mut self, ctx: &egui::Context) {
        let dropped: Vec<std::path::PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if dropped.is_empty() {
            return;
        }
        // Файл, что только что тянули с плашки и отпустили над её же окном, в лоток не идёт.
        let files: Vec<String> = self
            .dropped_but_not_own(dropped)
            .iter()
            .map(|p| p.display().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        if !files.is_empty() {
            self.drop_files(files);
        }
    }

    /// Файлы — в лоток: бросили на панель или нажали «В лоток» на плашке загрузки.
    fn drop_files(&mut self, files: Vec<String>) {
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

    /// Ждём сочетание для показа панели: то же правило, что у сцен; сочетание сцены — нельзя.
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
                    self.toasts.push(texts::key_is_scene(lang, &p.name), Tone::Warning);
                    return;
                }
                self.settings.panel_hotkey = Some(hotkey);
                self.capture_panel = false;
                return;
            }
        }
    }

    /// Ждём сочетание для сцены: первая клавиша с Ctrl или Alt; Esc — отмена (клавишу забираем: иначе она вышла бы
    /// заодно из правки или спрятала панель).
    fn capture_keys(&mut self, ui: &Ui) {
        let Some(i) = self.capture else { return };
        let events = ui.input(|inp| inp.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            if key == egui::Key::Escape {
                ui.input_mut(|inp| inp.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
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
                // Одно сочетание — у одной сцены: у прежнего хозяина снимаем.
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

    /// Карточка «Сцены»: имя, «Изменить…» (правка сцены), клавиша (захват, «занято»), убрать клавишу, удалить — корзина
    /// спрашивает «Удалить сцену?» тут же, на месте строки, тем же вопросом, что и правка сцены.
    fn scenes_card(&mut self, ui: &mut Ui) {
        let (t, lang) = (texts::tr(self.lang()), self.lang());
        w::section_label(ui, t.scenes);
        if self.settings.profiles.is_empty() {
            w::note(ui, t.scenes_none);
            return;
        }
        self.capture_keys(ui);
        let (mut delete, mut open, mut ask) = (None, None, None);
        for i in 0..self.settings.profiles.len() {
            let name = self.settings.profiles[i].name.clone();
            let hotkey = self.settings.profiles[i].hotkey.clone();
            let id = self.settings.profiles[i].id;
            if self.ask_delete == Some(id) {
                let (yes, no, rect) = delete_question(ui, lang, &name);
                if std::mem::take(&mut self.ask_scroll) {
                    ui.scroll_to_rect(rect.expand(8.0), None);
                }
                if yes {
                    delete = Some(i);
                } else if no {
                    self.ask_delete = None;
                }
                ui.add_space(6.0);
                continue;
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&name).font(anvil_ui::semibold(13.5)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let trash = w::icon_button(ui, Icon::Trash, t.scene_delete);
                    anchor(ui, trash.rect, &format!("scene-trash-{i}"));
                    if trash.clicked() {
                        ask = Some(id);
                    }
                    let edit = w::button(ui, Kind::Ghost, Some(Icon::Pencil), t.change).on_hover_text(t.scene_edit);
                    anchor(ui, edit.rect, &format!("scene-open-{i}"));
                    if edit.clicked() {
                        open = Some(i);
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
        if ask.is_some() {
            (self.ask_delete, self.ask_scroll) = (ask, true);
        }
        if let Some(i) = delete {
            self.ask_delete = None;
            self.delete_scene(i);
        }
        if let Some(i) = open
            && !self.scene_blocked()
        {
            self.open_scene(i, false, View::Settings);
        }
    }

    /// Край, от которого выезжает панель: зона вдоль края, её толщина и размер видимой полоски — чипами, как «Выезжать через».
    /// Значение, вписанное в `config.txt` руками и не совпавшее ни с одним чипом, остаётся: просто ни один чип не выбран.
    fn edge_ui(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let weak = Palette::of(ui).weak;
        let pct: Vec<String> = EDGE_ZONES.iter().map(|z| format!("{z} %")).collect();
        let opts: Vec<(u32, &str)> =
            EDGE_ZONES.iter().zip(&pct).map(|(z, l)| (*z, if *z == 0 { t.edge_whole } else { l.as_str() })).collect();
        ui.horizontal(|ui| {
            ui.label(t.edge_zone).on_hover_text(t.edge_zone_hint);
            modules::chips(ui, &mut self.settings.edge_zone, &opts);
        });
        let labels: Vec<String> = EDGE_DEPTHS.iter().map(|d| d.to_string()).collect();
        let opts: Vec<(u32, &str)> = EDGE_DEPTHS.iter().zip(&labels).map(|(d, l)| (*d, l.as_str())).collect();
        ui.horizontal(|ui| {
            ui.label(t.edge_depth).on_hover_text(t.edge_depth_hint);
            modules::chips(ui, &mut self.settings.edge_depth, &opts);
            ui.label(egui::RichText::new(t.px).color(weak));
        });
        let labels: Vec<String> = STRIP_SCALES.iter().map(|s| s.to_string()).collect();
        let opts: Vec<(u32, &str)> = STRIP_SCALES.iter().zip(&labels).map(|(s, l)| (*s, l.as_str())).collect();
        ui.add_enabled_ui(self.settings.strip, |ui| {
            ui.horizontal(|ui| {
                ui.label(t.strip_scale);
                modules::chips(ui, &mut self.settings.strip_scale, &opts);
                ui.label(egui::RichText::new("%").color(weak));
            });
        });
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
        match self.view {
            View::Cards => return self.header_cards(ui, ops),
            View::Scene => return self.scene_header(ui),
            View::Search => return self.search_header(ui),
            View::Settings | View::Color => {}
        }
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let ctx = ui.ctx().clone();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(2.0);
            match self.view {
                View::Cards | View::Scene | View::Search => {}
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

    /// Шапка над карточками: слева значок, «Antresol» и вкладки, справа значки, монитор, сцены и поиск. Что показать,
    /// решает `header_plan` по ширине панели, один порядок на все ширины: уходят подписи («Сцена», «Монитор»), слово
    /// «Antresol», поле поиска сжимается до значка, подписи вкладок сужаются (многоточие, полное имя в подсказке), потом
    /// уходят выбор монитора и сцены; в правке — подсказка, подпись у «Как было», слово, подпись у «Полка». Ушедшее при
    /// сужении обратно не возвращается. Левая часть обрезана по своему месту и на правую не наезжает.
    fn header_cards(&mut self, ui: &mut Ui, ops: &mut Vec<Op>) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let rect = ui.max_rect();
        let editing = self.editing.is_some();
        let tabs = self.tab_metrics(ui);
        let plan = header_plan::plan(&self.header_sizes(ui, &tabs), rect.width());
        self.header_plan = plan;
        let widths: Vec<f32> = tabs.iter().map(|m| m.text).collect();
        let cap = tab_cap(&widths, plan.tab_room, TAB_FLOOR);

        // Слева — всё, кроме правой части.
        let budget = (rect.width() - plan.right - header_plan::GAP).max(0.0);
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
        if plan.word {
            left.label(egui::RichText::new("Antresol").font(anvil_ui::semibold(14.5)).color(p.text));
        }
        vline(&mut left, p.border);
        self.shelf_tabs(&mut left, ops, cap);
        if editing {
            let room = self.settings.shelves.len() < config::MAX_SHELVES;
            let r = left.add_enabled_ui(room, |ui| {
                if plan.add_label {
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

        // Справа налево.
        let mut right = ui.new_child(
            UiBuilder::new()
                .id_salt("header-right")
                .max_rect(rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.spacing_mut().item_spacing.x = header_plan::SPACING;
        self.header_right(&mut right, &plan);
        self.header_edges = (left_rect.right(), right.min_rect().left());
    }

    /// Размеры частей шапки над карточками, как они нарисуются: по ним `header_plan` решает, что оставить.
    fn header_sizes(&self, ui: &Ui, tabs: &[TabMetric]) -> header_plan::Sizes {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let measure =
            |text: &str, font: egui::FontId| ui.painter().layout_no_wrap(text.to_owned(), font, p.text).size().x;
        let text_of = |cap: f32| tabs.iter().map(|m| m.text.min(cap)).sum::<f32>();
        // Отступ, знак, черта и промежутки между ними (по 8); у вкладок — поля и метки, между ними по 2.
        let rest = 2.0
            + 24.0
            + 8.0
            + 1.0
            + 8.0
            + tabs.iter().map(|m| m.rest).sum::<f32>()
            + 2.0 * tabs.len().saturating_sub(1) as f32;
        let mut sizes = header_plan::Sizes {
            editing: self.editing.is_some(),
            rest,
            tabs: text_of(f32::INFINITY),
            tabs_soft: text_of(TAB_SOFT),
            tabs_floor: text_of(TAB_FLOOR),
            word: measure("Antresol", anvil_ui::semibold(14.5)) + 8.0,
            field: search_ui::FIELD_W,
            icon: 30.0,
            ..header_plan::Sizes::default()
        };
        // Кнопка с подписью: поля по 12, значок 16, промежуток 7 (`w::button`).
        let button = |text: &str, font: egui::FontId| 12.0 * 2.0 + 16.0 + 7.0 + measure(text, font);
        if sizes.editing {
            sizes.add_icon = 8.0 + 30.0;
            sizes.add_label = 8.0 + button(t.shelf_add, egui::FontId::proportional(14.0));
            sizes.done = button(t.done, anvil_ui::semibold(14.0));
            sizes.undo_label = button(t.shelves_undo, egui::FontId::proportional(14.0));
            sizes.undo_icon = 30.0;
            sizes.hint = measure(t.shelves_hint, egui::FontId::proportional(12.5));
        } else {
            // Сцены — без карточки «Экран» их нет; монитор — когда мониторов несколько.
            sizes.scenes = (!self.settings.is_hidden("display")).then(|| self.profiles_width(ui));
            sizes.monitors = (self.monitors.len() > 1).then(|| self.monitor_width(ui));
            sizes.label_monitors = measure(t.monitor, egui::FontId::proportional(12.0));
            sizes.label_scenes = measure(t.scene, egui::FontId::proportional(12.0));
        }
        sizes
    }

    /// Правая часть шапки над карточками, справа налево: «спрятать», настройки, закрепить, правка полок; потом то, что
    /// оставил `header_plan`: выбор монитора, сцены (с подписями «Монитор» и «Сцена») и поиск — целым полем или значком, он
    /// есть всегда. В правке — «Готово», «Как было» и подсказка.
    fn header_right(&mut self, ui: &mut Ui, plan: &header_plan::Plan) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        if self.editing.is_some() {
            // Правка полок: справа только «Готово», «Как было» и подсказка.
            self.edit_controls(ui, plan);
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
        ui.add_space(6.0);
        vline(ui, p.border);
        ui.add_space(6.0);
        if plan.monitors {
            self.monitor_picker(ui);
            if plan.labels {
                ui.label(egui::RichText::new(t.monitor).size(12.0).color(p.weak));
            }
            if plan.scenes {
                ui.add_space(6.0);
                vline(ui, p.border);
                ui.add_space(6.0);
            }
        }
        if plan.scenes {
            self.scenes_bar(ui, plan.labels);
        }
        if plan.scenes || plan.monitors {
            ui.add_space(6.0);
            vline(ui, p.border);
            ui.add_space(6.0);
        }
        self.search_button(ui, plan.field);
    }

    /// Сколько места нужно «+» и чипам сцен (с запасом на полужирный: стоящая сцена выделена им): для решения, помещаются ли
    /// они в шапку. Чип — подпись и по 10 с боков, перед каждым промежуток.
    fn profiles_width(&self, ui: &Ui) -> f32 {
        let p = Palette::of(ui);
        let font = anvil_ui::semibold(13.0);
        let chips: f32 = self
            .settings
            .profiles
            .iter()
            .map(|pr| ui.painter().layout_no_wrap(pr.name.clone(), font.clone(), p.text).size().x + 20.0 + 4.0)
            .sum();
        30.0 + chips
    }

    /// Сколько места нужно выбору монитора панели («Все» и номера): столько занимает `segmented`.
    fn monitor_width(&self, ui: &Ui) -> f32 {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let font = egui::FontId::proportional(13.0);
        let names = std::iter::once(t.all_monitors.to_string()).chain((1..=self.monitors.len()).map(|i| i.to_string()));
        names.map(|n| ui.painter().layout_no_wrap(n, font.clone(), p.text).size().x + 20.0).sum::<f32>() + 4.0
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

    /// Шапка в правке полок, справа налево: «Готово», «Как было» (с подписью или значком) и подсказка — как решил
    /// `header_plan`.
    fn edit_controls(&mut self, ui: &mut Ui, plan: &header_plan::Plan) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        if w::button(ui, Kind::Primary, Some(Icon::Check), t.done).clicked() {
            self.finish_edit();
        }
        let undo = if plan.undo_label {
            w::button(ui, Kind::Ghost, Some(Icon::Undo), t.shelves_undo)
        } else {
            w::icon_button(ui, Icon::Undo, t.shelves_undo)
        };
        if undo.clicked() {
            self.undo_edit();
        }
        if plan.hint {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(t.shelves_hint).size(12.5).color(p.weak));
        }
    }

    /// Сцены чипами в шапке, «+» — новая сцена из того, что стоит сейчас (откроется правка). Рисуется справа налево: «+», чипы
    /// с конца, подпись «Сцена» (если `label`). Щелчок по чипу ставит сцену, а по стоящей — возвращает как было (если
    /// возврат включён); правый щелчок — правка. Подсказка чипа: клавиша и что делают щелчки.
    fn scenes_bar(&mut self, ui: &mut Ui, label: bool) {
        let (t, lang) = (texts::tr(self.lang()), self.lang());
        let p = Palette::of(ui);
        let active = self.active_scene();
        let (mut click, mut open, mut new) = (None, None, false);
        let plus = w::icon_button(ui, Icon::Plus, t.scene_new);
        anchor(ui, plus.rect, "scene-new");
        new |= plus.clicked();
        for (i, scene) in self.settings.profiles.iter().enumerate().rev() {
            let on = active == Some(i);
            let r = modules::chip(ui, ("scene", i), on, &scene.name, None);
            // Свой `Id`: тест без окна находит чип по нему.
            anchor(ui, r.rect, &format!("scene-chip-{i}"));
            let key = scene.hotkey.as_ref().map(|h| h.to_string());
            let r = r.on_hover_text(texts::scene_tip(lang, key.as_deref(), scene.undo));
            if r.clicked() {
                click = Some(i);
            }
            if r.secondary_clicked() {
                open = Some(i);
            }
        }
        if label {
            ui.label(egui::RichText::new(t.scene).size(12.0).color(p.weak));
        }
        if new {
            self.new_scene();
        } else if let Some(i) = open {
            if !self.scene_blocked() {
                self.open_scene(i, false, View::Cards);
            }
        } else if let Some(i) = click {
            let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
            self.scene_click(i, &device);
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

    /// Что опрашивать, пока панель открыта: карточки открытой полки; «Экран» — его читают чипы сцен в шапке; «Быстрые
    /// настройки» и «Яркость» — если какая-то сцена что-то отмечает у них (чип стоящей сцены узнают по ним, где бы ни лежали
    /// карточки и убраны ли они); при правке сцены — все три: строкам нужны свежие значения.
    fn refresh_set(&self) -> Vec<usize> {
        // Поиск строит действия по каждой карточке на любой полке: пока он открыт, перечитываются все.
        let mut set: Vec<usize> = if self.view == View::Search {
            self.settings.shelves.iter().flat_map(|s| s.cards.iter()).filter_map(|k| self.module_index(k)).collect()
        } else {
            self.shelf_modules(self.shelf)
        };
        let editing = self.view == View::Scene;
        let scenes = &self.settings.profiles;
        let wants = |key: &str| match key {
            "display" => editing || self.settings.shelf_of("display").is_some(),
            "quick" => {
                editing
                    || scenes.iter().any(|s| {
                        let y = &s.system;
                        y.quiet.is_some() || y.dark.is_some() || y.wifi.is_some() || y.plan.is_some()
                    })
            }
            "brightness" => editing || scenes.iter().any(|s| s.system.bright.is_some() || s.system.night.is_some()),
            _ => false,
        };
        for key in ["display", "quick", "brightness"] {
            if let Some(m) = self.module_index(key)
                && wants(key)
                && !set.contains(&m)
            {
                set.push(m);
            }
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
        if self.view != View::Cards || self.renaming.is_some() || ctx.text_edit_focused() {
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
        // Колесо мыши над вкладками: вверх — предыдущая полка, вниз — следующая. Щелчок колеса — сразу полка; плавное
        // (тачпад, точное колесо) копится, пока не наберётся на щелчок (`TAB_WHEEL`).
        if let Some(strip) = strip
            && ui.rect_contains_pointer(strip)
        {
            let (lines, points): (f32, f32) = ui.input(|inp| {
                inp.events.iter().fold((0.0, 0.0), |(l, p), e| match e {
                    egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta, .. } => (l + delta.y, p),
                    egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, .. } => (l, p + delta.y),
                    _ => (l, p),
                })
            });
            let mut wheel = lines;
            if lines == 0.0 {
                self.tab_wheel += points;
                if self.tab_wheel.abs() >= TAB_WHEEL {
                    wheel = self.tab_wheel;
                }
            }
            if wheel != 0.0 {
                self.tab_wheel = 0.0;
            }
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
            let extra = self.card_row(ui, row, &slots, &widths, device, ops);
            self.note_fit(extra);
            return;
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        wheel_sideways(&mut child);
        // У каждой полки своя прокрутка.
        let mut scroll = egui::ScrollArea::horizontal().id_salt(("cards", shelf)).auto_shrink([false, false]);
        if let Some(x) = self.scroll_to.take() {
            scroll = scroll.horizontal_scroll_offset(x);
        }
        let out = scroll.show(&mut child, |ui| {
            wheel_inside(ui);
            // Под полосу прокрутки — 12 снизу.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(content, area.height() - 12.0), Sense::hover());
            let row = Row { area, origin: rect.min, height: rect.height() };
            self.card_row(ui, row, &slots, &widths, device, ops)
        });
        self.note_fit(out.inner);
        let offset = out.state.offset.x;
        let max_offset = (content - area.width()).max(0.0);
        // В правке над карточкой лежит её слой, а полоса прокрутки ряда — ниже: колесо до неё не доходит. Что она не
        // забрала, передаём ей сами (на следующем кадре): и Shift + колесо, и простое колесо — карточкам под вуалью
        // листать нечего.
        if editing {
            let ctx = ui.ctx().clone();
            let over = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| area.contains(p));
            let dx = ctx.input(|i| i.smooth_scroll_delta().x + i.smooth_scroll_delta().y);
            if over && dx != 0.0 {
                self.scroll_to = Some((offset - dx).clamp(0.0, max_offset));
                ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
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

    /// Ответ — сколько не хватило самой высокой карточке ряда (`note_fit`); карточек нет — `None`.
    fn card_row(
        &mut self,
        ui: &mut Ui,
        row: Row,
        slots: &[Slot],
        widths: &[f32],
        device: &str,
        ops: &mut Vec<Op>,
    ) -> Option<f32> {
        let t = texts::tr(self.lang());
        let mut extra: Option<f32> = None;
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
            modules::anchor(&child, shifted, &format!("card-{key}"));
            let frame = w::card_frame(&child).show(&mut child, |ui| {
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
                // Сколько карточке не хватило: низ занятого плюс то, что не влезло в её прокрутку, минус низ карточки.
                let bottom = ui.max_rect().bottom();
                modules::take_overflow(ui.ctx());
                let used = ui.scope(|ui| module.ui(ui, &mut cx)).response.rect.bottom();
                used + modules::take_overflow(ui.ctx()).unwrap_or(0.0) - bottom
            });
            extra = Some(extra.map_or(frame.inner, |e: f32| e.max(frame.inner)));
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
        extra
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
        wheel_sideways(&mut child);
        egui::ScrollArea::horizontal().id_salt("settings").auto_shrink([false, false]).show(&mut child, |ui| {
            wheel_inside(ui);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(MIN, area.height() - 12.0), Sense::hover());
            self.settings_columns(ui, rect);
        });
    }

    fn settings_columns(&mut self, ui: &mut Ui, area: Rect) {
        let t = texts::tr(self.lang());
        let ctx = ui.ctx().clone();
        let before = self.settings.clone();
        // Три карточки: «Панель», «Сцены», «Оформление». Какие карточки и в каком порядке — правка полок
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
                self.scenes_card(ui);
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
                // Живые плашки у края: какие события показывать; таймер — всегда.
                ui.add_space(6.0);
                w::section_label(ui, t.plaques);
                self.plaque_chips(ui);
                self.exclude_ui(ui);
                ui.horizontal(|ui| {
                    ui.label(t.show_delay);
                    let labels: Vec<String> = DELAYS.iter().map(|d| d.to_string()).collect();
                    let opts: Vec<(u32, &str)> = DELAYS.iter().zip(&labels).map(|(d, l)| (*d, l.as_str())).collect();
                    modules::chips(ui, &mut self.settings.show_delay_ms, &opts);
                    ui.label(egui::RichText::new(t.ms).color(Palette::of(ui).weak));
                });
                self.edge_ui(ui);
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

    // ─── Правка сцены ───────────────────────────────────────────────────────

    /// Шапка правки сцены вместо шапки карточек: слева «‹ Сцены / «Кино»», справа «Удалить сцену», черта и «Готово». «Готово»
    /// недоступна, пока имя не годится или ничего не отмечено (в подсказке — что именно). «‹ Сцены» и «Готово» сохраняют и
    /// выходят.
    fn scene_header(&mut self, ui: &mut Ui) {
        let (t, lang) = (texts::tr(self.lang()), self.lang());
        let p = Palette::of(ui);
        let Some(edit) = &self.scene else { return };
        let shown = if Profile::clean_name(&edit.name).is_empty() {
            self.settings.profiles.get(edit.index).map(|s| s.name.clone()).unwrap_or_default()
        } else {
            Profile::clean_name(&edit.name)
        };
        let title = texts::scene_quoted(lang, &shown);
        let problem = self.scene_problem();
        // Справа — «Удалить сцену», черта и «Готово», слева — «‹ Сцены» и «/»: их ширина известна заранее, а имени в заголовке
        // остаётся остальное — длинное обрезается многоточием, полное — в подсказке.
        let width =
            |text: &str, font: egui::FontId| ui.painter().layout_no_wrap(text.to_owned(), font, p.text).size().x;
        let button = |text: &str, font: egui::FontId| 12.0 + 16.0 + 7.0 + width(text, font) + 12.0;
        let sides = 2.0
            + button(t.scenes, egui::FontId::proportional(14.0))
            + 8.0 * 3.0
            + width("/", egui::FontId::proportional(14.0))
            + button(t.scene_delete, egui::FontId::proportional(14.0))
            + 8.0 * 2.0
            + 1.0
            + button(t.done, anvil_ui::semibold(14.0));
        let room = (ui.available_width() - sides - 16.0).max(40.0);
        let cut = width(&title, anvil_ui::semibold(14.5)) > room;
        let (mut leave, mut ask_delete) = (false, false);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(2.0);
            let back = w::button(ui, Kind::Ghost, Some(Icon::ArrowLeft), t.scenes).on_hover_text(t.back);
            anchor(ui, back.rect, "scene-back");
            leave |= back.clicked();
            ui.label(egui::RichText::new("/").color(p.weak));
            // Имя рисуем сами: `ui.label` в строке без переноса растягивает текст и обрезку по `room` не слушает — имя
            // заходило на «Удалить сцену».
            let mut job = egui::text::LayoutJob::simple_singleline(title.clone(), anvil_ui::semibold(14.5), p.text);
            job.wrap = egui::text::TextWrapping::truncate_at_width(room);
            let galley = ui.painter().layout_job(job);
            let (rect, name) = ui.allocate_exact_size(galley.size(), Sense::hover());
            ui.painter().galley(rect.min, galley, p.text);
            anchor(ui, rect, "scene-title");
            if cut {
                name.on_hover_text(&title);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let done =
                    ui.add_enabled_ui(problem.is_none(), |ui| w::button(ui, Kind::Primary, Some(Icon::Check), t.done));
                anchor(ui, done.inner.rect, "scene-done");
                if let Some(why) = problem {
                    done.inner.on_disabled_hover_text(why);
                } else {
                    leave |= done.inner.clicked();
                }
                vline(ui, p.border);
                let delete = ghost_danger(ui, Icon::Trash, t.scene_delete);
                anchor(ui, delete.rect, "scene-delete");
                ask_delete |= delete.clicked();
            });
        });
        if let Some(edit) = &mut self.scene
            && ask_delete
        {
            edit.confirm_delete = true;
        }
        if leave {
            self.finish_scene();
        }
    }

    /// Правка сцены — вместо карточек: три карточки; уже 1270 точек (хватает панели 1300 по умолчанию) — листаются вбок, а
    /// не сжимаются: подписи и значения строк иначе обрезались бы.
    fn scene_view(&mut self, ui: &mut Ui) {
        const MIN: f32 = 1270.0;
        let mut area = ui.available_rect_before_wrap();
        // Вопрос «Удалить сцену?» — над карточками, по видимой ширине панели, а не по ширине карточек: листаются они, а не
        // вопрос с его кнопками.
        if self.scene.as_ref().is_some_and(|e| e.confirm_delete) {
            let name = self
                .scene
                .as_ref()
                .and_then(|e| self.settings.profiles.get(e.index))
                .map(|s| s.name.clone())
                .unwrap_or_default();
            let mut question = ui.new_child(UiBuilder::new().max_rect(area));
            let (yes, no, _) = delete_question(&mut question, self.lang(), &name);
            area.min.y = question.min_rect().bottom() + 8.0;
            if yes {
                if let Some(edit) = self.scene.take() {
                    self.view = edit.back;
                    self.delete_scene(edit.index);
                }
                return;
            }
            if no && let Some(edit) = &mut self.scene {
                edit.confirm_delete = false;
            }
        }
        if area.width() >= MIN {
            self.scene_columns(ui, area);
            return;
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        wheel_sideways(&mut child);
        egui::ScrollArea::horizontal().id_salt("scene").auto_shrink([false, false]).show(&mut child, |ui| {
            wheel_inside(ui);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(MIN, area.height() - 12.0), Sense::hover());
            self.scene_columns(ui, rect);
        });
    }

    /// Значение строки сцены, как оно записано в ней: что напишет строка справа и подсказка к ней.
    fn scene_value(&self, row: SceneRow, scene: &Profile) -> (String, String) {
        let (t, lang) = (texts::tr(self.lang()), self.lang());
        let s = &scene.system;
        let flag = |on: Option<bool>| (if on == Some(true) { t.on } else { t.off }).to_string();
        let mode = |m: &config::ProfileMonitor| texts::scene_mode(lang, m.width, m.height, m.hz, m.scale, m.hdr);
        match row {
            SceneRow::Screen => match scene.monitors.as_slice() {
                [m] => (mode(m), format!("{} · {}", m.device.trim_start_matches(r"\\.\"), mode(m))),
                many => {
                    let lines: Vec<String> = many
                        .iter()
                        .map(|m| format!("{} · {}", m.device.trim_start_matches(r"\\.\"), mode(m)))
                        .collect();
                    (texts::scene_monitors(lang, many.len()), lines.join("\n"))
                }
            },
            SceneRow::Bright => (texts::scene_percent(lang, s.bright.unwrap_or(0)), String::new()),
            SceneRow::Night => (flag(s.night), String::new()),
            SceneRow::Quiet => (flag(s.quiet), String::new()),
            SceneRow::Awake => (flag(s.awake), String::new()),
            SceneRow::Dark => (flag(s.dark), String::new()),
            SceneRow::Wifi => (flag(s.wifi), String::new()),
            SceneRow::Plan => {
                let name = self.scene.as_ref().and_then(|e| {
                    let guid = s.plan.as_deref()?;
                    e.plans.iter().find(|(g, _)| g == guid).map(|(_, n)| n.clone())
                });
                (name.unwrap_or_else(|| "—".to_string()), String::new())
            }
        }
    }

    /// Подпись строки сцены.
    fn scene_label(&self, row: SceneRow) -> &'static str {
        texts::scene_row_label(texts::tr(self.lang()), row)
    }

    /// Почему строку нельзя отметить (в подсказке недоступной строки): яркость — про DDC/CI, Wi‑Fi — нет радио, остальное —
    /// Windows не отдала значение.
    fn scene_why(&self, row: SceneRow) -> &'static str {
        let t = texts::tr(self.lang());
        match row {
            SceneRow::Bright => t.ddc_none,
            SceneRow::Wifi => t.scene_why_wifi,
            _ => t.scene_why_read,
        }
    }

    fn scene_columns(&mut self, ui: &mut Ui, area: Rect) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let Some(index) = self.scene.as_ref().map(|e| e.index) else { return };
        let Some(scene) = self.settings.profiles.get(index).cloned() else { return };
        let ctx = ui.ctx().clone();

        // Три карточки по долям 2,45 : 1 : 1,32, зазор 10. В макете 2,3 : 1 : 1,15, но на 1300 так узка и первая («Display»
        // в английской подписи сжимается до «Displ…» рядом со значением экрана), и третья (подпись выключателя «Повторный
        // щелчок — вернуть как было» в одну строку не влезает).
        let unit = (area.width() - 20.0) / 4.77;
        let card1 = Rect::from_min_size(area.min, Vec2::new(unit * 2.45, area.height()));
        let card2 = Rect::from_min_size(Pos2::new(card1.right() + 10.0, area.top()), Vec2::new(unit, area.height()));
        let card3 =
            Rect::from_min_size(Pos2::new(card2.right() + 10.0, area.top()), Vec2::new(unit * 1.32, area.height()));
        let rows_state: Vec<(SceneRow, bool)> = self.modules.iter().flat_map(|m| m.scene_rows()).collect();
        let (mut toggle, mut take) = (None, false);
        let before = self.settings.clone();

        // Что меняет сцена: строки с галочками в две колонки, сверху вниз.
        let mut c1 = ui.new_child(UiBuilder::new().max_rect(card1));
        w::card_frame(&c1).show(&mut c1, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("scene-1").auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    w::section_label(ui, t.scene_what);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let r = w::button(ui, Kind::Ghost, Some(Icon::Refresh), t.scene_take);
                        anchor(ui, r.rect, "scene-take");
                        take = r.clicked();
                    });
                });
                ui.add_space(4.0);
                let gap = 20.0;
                let col_w = ((ui.available_width() - gap) / 2.0).floor();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                    for col in 0..2 {
                        ui.allocate_ui_with_layout(
                            Vec2::new(col_w, 4.0 * 32.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                for (k, row) in SceneRow::ALL[col * 4..col * 4 + 4].iter().enumerate() {
                                    let row = *row;
                                    let on = scene.has(row);
                                    let available = rows_state.iter().any(|(r, ok)| *r == row && *ok);
                                    let (value, hint) = if on {
                                        self.scene_value(row, &scene)
                                    } else if available {
                                        (t.scene_untouched.to_string(), String::new())
                                    } else {
                                        // Недоступно в системе: в подсказке причина.
                                        (t.unavailable.to_string(), self.scene_why(row).to_string())
                                    };
                                    let id = egui::Id::new(("scene-row", col * 4 + k));
                                    let r =
                                        scene_row(ui, id, self.scene_label(row), &value, &hint, on, available || on);
                                    if r.clicked() {
                                        toggle = Some(row);
                                    }
                                }
                            },
                        );
                    }
                });
                if scene.is_empty() {
                    ui.add_space(6.0);
                    w::note(ui, t.scene_rows_none);
                }
            });
        });

        // Имя и клавиша.
        let mut name = self.scene.as_ref().map(|e| e.name.clone()).unwrap_or_default();
        let focus = self.scene.as_ref().map_or(0, |e| e.focus);
        let (mut done, mut remove_key) = (false, false);
        let mut c2 = ui.new_child(UiBuilder::new().max_rect(card2));
        w::card_frame(&c2).show(&mut c2, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("scene-2").auto_shrink([false, false]).show(ui, |ui| {
                w::section_label(ui, t.scene_name_key);
                ui.add_space(4.0);
                ui.label(egui::RichText::new(t.scene_name).size(12.5).color(p.weak));
                let id = egui::Id::new("scene-name");
                let field = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .id(id)
                        .char_limit(config::MAX_SCENE_NAME)
                        .desired_width(ui.available_width()),
                );
                if focus == 2 {
                    field.request_focus();
                } else if focus == 1
                    && let Some(mut state) = egui::TextEdit::load_state(&ctx, id)
                {
                    let end = egui::text::CCursor::new(name.chars().count());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), end)));
                    state.store(&ctx, id);
                }
                // Enter в поле — «Готово».
                done = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(why) = self.name_problem(&name, index) {
                    // Красная рамка поля и под ним строка со значком (общий гайдлайн §5).
                    ui.painter().rect_stroke(field.rect, 6, Stroke::new(1.5, p.danger), StrokeKind::Inside);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        anvil_ui::icons::paint(ui.painter(), rect, Tone::Danger.icon(), p.danger);
                        ui.label(egui::RichText::new(why).size(12.5).color(p.danger));
                    });
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(t.scene_key).size(12.5).color(p.weak));
                self.capture_keys(ui);
                if self.capture == Some(index) {
                    ui.horizontal_wrapped(|ui| {
                        w::badge(ui, t.press_keys, Tone::Accent);
                        if w::button(ui, Kind::Ghost, None, t.cancel).clicked() {
                            self.capture = None;
                        }
                    });
                } else {
                    let keys: Vec<&str> = match &scene.hotkey {
                        Some(h) => h.keys(),
                        None => vec![t.no_key],
                    };
                    let status = match (&scene.hotkey, sys::profile_hotkey_status(index)) {
                        (Some(_), Some(true)) => Some((true, t.key_free)),
                        (Some(_), Some(false)) => Some((false, t.key_busy)),
                        _ => None,
                    };
                    if w::hotkey_field(ui, &keys, t.change, status).clicked() {
                        self.capture = Some(index);
                    }
                    if scene.hotkey.is_some() && w::button(ui, Kind::Ghost, Some(Icon::Close), t.remove_key).clicked() {
                        remove_key = true;
                    }
                }
            });
        });

        // Выход из сцены: возврат и пояснения.
        let mut undo = scene.undo;
        let mut c3 = ui.new_child(UiBuilder::new().max_rect(card3));
        w::card_frame(&c3).show(&mut c3, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical().id_salt("scene-3").auto_shrink([false, false]).show(ui, |ui| {
                w::section_label(ui, t.scene_exit);
                ui.add_space(4.0);
                let sw = w::switch(ui, &mut undo, t.scene_undo);
                anchor(ui, sw.rect, "scene-undo");
                w::note(ui, t.scene_undo_note);
                ui.add_space(6.0);
                w::divider(ui);
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    anvil_ui::icons::paint(ui.painter(), rect.translate(Vec2::new(0.0, 2.0)), Icon::Info, p.weak);
                    // Переносится по ширине карточки: без этого карточка раздвинулась бы за край панели.
                    ui.add(egui::Label::new(egui::RichText::new(t.scene_confirm_note).size(13.0).color(p.weak)).wrap());
                });
            });
        });

        // Что изменили за кадр — в сцену на месте, и в файл.
        if let Some(edit) = &mut self.scene {
            edit.name = name.clone();
            edit.focus = focus.saturating_sub(1);
        }
        if self.name_problem(&name, index).is_none() {
            let clean = Profile::clean_name(&name);
            if self.settings.profiles[index].name != clean {
                self.settings.profiles[index].name = clean;
            }
        }
        self.settings.profiles[index].undo = undo;
        if remove_key {
            self.settings.profiles[index].hotkey = None;
        }
        if let Some(row) = toggle
            && scene.has(row)
        {
            self.settings.profiles[index].clear(row);
            toggle = None;
        }
        if self.settings != before {
            self.save();
        }
        // Отметить строку и «Запомнить текущие» — значения берут у модулей: они же сохранят файл.
        if let Some(row) = toggle {
            self.command_all(Command::SaveScene(index, vec![row]));
        }
        if take {
            let rows: Vec<SceneRow> = SceneRow::ALL.into_iter().filter(|r| scene.has(*r)).collect();
            self.command_all(Command::SaveScene(index, rows));
        }
        if done && self.scene_problem().is_none() {
            self.finish_scene();
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
            let sense = if grip == Grip::Bottom { Sense::click_and_drag() } else { Sense::drag() };
            let r = ui.interact(rect, ui.id().with(("grip", grip as u8)), sense).on_hover_cursor(cursor);
            // Двойной щелчок по нижнему краю — снова «по полке».
            if grip == Grip::Bottom
                && r.double_clicked()
                && self.held.is_none()
                && let Some(sh) = self.settings.shelves.get_mut(self.shelf)
                && sh.height.take().is_some()
            {
                self.save();
            }
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
        // Под мышью нижний край говорит, чья высота: подсказка у края окна обрезалась бы, поэтому подпись рядом с чёрточкой.
        if hot && self.held.is_none() {
            let t = texts::tr(self.lang());
            let own = self.settings.shelves.get(self.shelf).is_some_and(|s| s.height.is_some());
            ui.painter().text(
                Pos2::new(c.x + 30.0, b - 8.0),
                egui::Align2::LEFT_CENTER,
                if own { t.height_own } else { t.height_fit },
                egui::FontId::proportional(11.5),
                p.weak,
            );
        }
    }

    /// Полоска у края. Мышь у края — шире и наполняется акцентом за время задержки выезда.
    fn plain_strip(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        let corner = CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 };
        match self.strip_fill {
            Some(fill) => {
                let hot = self.plain_size(true);
                ui.painter().rect_filled(Rect::from_min_size(Pos2::ZERO, hot), corner, p.raised);
                let done = Rect::from_min_size(Pos2::ZERO, Vec2::new(hot.x * fill, hot.y));
                ui.painter().rect_filled(done, corner, p.accent);
            }
            None => {
                ui.painter().rect_filled(
                    Rect::from_min_size(Pos2::ZERO, self.plain_size(false)),
                    corner,
                    p.border_strong,
                );
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

    fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let t = self.drawn;
        if t <= 0.0 {
            match self.strip_kind {
                StripKind::Off => {}
                StripKind::Plain => self.plain_strip(ui),
                StripKind::Media => self.media_strip(ui),
                StripKind::Plaque => self.plaque_strip(ui),
            }
            return;
        }
        self.take_keyboard_on_press(&ctx);
        let p = Palette::of(ui);
        let panel = Rect::from_min_size(Pos2::ZERO, Vec2::new(self.settings.width, self.height));
        let corner = CornerRadius { nw: 0, ne: 0, sw: CORNER, se: CORNER };
        ui.painter().rect(panel, corner, p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);

        // Полок стало меньше, чем номер открытой, — берётся последняя.
        self.shelf = self.shelf.min(self.settings.shelves.len().saturating_sub(1));
        // Вопрос «Удалить сцену?» живёт в настройках: ушли оттуда — вопрос снят.
        if self.view != View::Settings {
            self.ask_delete = None;
        }
        self.shelf_keys(&ctx);
        self.search_keys(&ctx);

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
            View::Scene => self.scene_view(&mut body_ui),
            View::Search => self.search_view(&mut body_ui),
        }
        self.apply_ops(ops);
        if t >= 1.0 {
            self.grips(ui, panel, ppp);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.grip.is_none()
            && self.capture.is_none()
            && !self.capture_panel
            && self.renaming.is_none()
            && !self.typing_here()
        {
            if self.view == View::Search {
                // Esc в поиске — назад к карточкам; следующий прячет панель.
                self.close_search();
            } else if self.view == View::Scene {
                // Esc в правке сцены: сперва снимает вопрос «Удалить сцену?», потом — как «Готово» (следующий Esc прячет панель).
                match &mut self.scene {
                    Some(edit) if edit.confirm_delete => edit.confirm_delete = false,
                    _ => self.finish_scene(),
                }
            } else if self.editing.is_some() {
                // Esc в правке — как «Готово»; следующий Esc прячет панель.
                self.finish_edit();
            } else if self.view == View::Settings && self.ask_delete.is_some() {
                // Esc в настройках сперва снимает вопрос «Удалить сцену?»; следующий прячет панель.
                self.ask_delete = None;
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

    /// Файл потянули с плашки: пока шло перетаскивание Windows, кнопку отпустили, а egui этого не видел и считал бы её
    /// нажатой, пока в окне не щёлкнут снова. Отпускаем за него.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(pos) = self.release.take() {
            raw_input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        anvil_ui::chrome::clear_color(visuals, look::accent_of(&self.settings.accent))
    }

    /// Окно закрывается (Alt+F4, «Закрыть Antresol»): открытая правка сцены кончается, как по «Готово» (негодная сцена
    /// убирается, а у прежней без отмеченных строк остаётся такая, какой вошли, — иначе её не было бы в файле), а модули
    /// возвращают то, что держали на время, — режим экрана, который не успели подтвердить. Выход из сеанса сюда не
    /// доходит, но и не нужен: неподтверждённого режима нет в реестре.
    fn on_exit(&mut self) {
        self.finish_scene();
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

/// Вопрос «Удалить сцену «…»?» на месте: плашка предупреждения, вопрос слева, справа «Отмена» и «Удалить» (`Danger`). Не
/// помещается в одну строку (узкая карточка настроек, длинное имя) — кнопки встают под вопросом, слева (колонка настроек
/// на узкой панели листается вбок, а правый край у неё уходит из виду), а имя в вопросе обрезается многоточием. Ответ:
/// (удалить, отмена, место плашки). Кнопки с якорями `scene-delete-yes` и `scene-delete-no`.
fn delete_question(ui: &mut Ui, lang: Lang, name: &str) -> (bool, bool, Rect) {
    let t = texts::tr(lang);
    let p = Palette::of(ui);
    let color = Tone::Warning.color(&p);
    let painter = ui.painter().clone();
    let measure = |text: &str, font: egui::FontId| painter.layout_no_wrap(text.to_owned(), font, p.text).size().x;
    // Кнопка набора — 12 по бокам и текст; между кнопками обычный зазор.
    let gap = ui.spacing().item_spacing.x;
    let buttons = 24.0
        + measure(t.scene_delete_yes, egui::FontId::proportional(14.0))
        + gap
        + 24.0
        + measure(t.cancel, egui::FontId::proportional(14.0));
    let (yes, no) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let frame = egui::Frame::new()
        .fill(p.soft(color))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(anvil_ui::theme::radius::CARD)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let room = ui.available_width();
            // Значок 18, отступ 4 до вопроса и зазоры между ними; вопрос не шире места после них — длинное имя обрезается.
            let lead = 18.0 + gap + 4.0 + gap;
            let wide_enough =
                |question: &str| lead + measure(question, anvil_ui::semibold(14.0)) + gap + buttons <= room;
            let full = texts::scene_delete_q(lang, name);
            let one_row = wide_enough(&full);
            let question = if one_row { full } else { fit_question(lang, name, room - lead, &measure) };
            let delete_button = |ui: &mut Ui| {
                let delete = w::button(ui, Kind::Danger, None, t.scene_delete_yes);
                anchor(ui, delete.rect, "scene-delete-yes");
                yes.set(delete.clicked());
            };
            let cancel_button = |ui: &mut Ui| {
                let cancel = w::button(ui, Kind::Secondary, None, t.cancel);
                anchor(ui, cancel.rect, "scene-delete-no");
                no.set(cancel.clicked());
            };
            let ask = |ui: &mut Ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                anvil_ui::icons::paint(ui.painter(), rect, Tone::Warning.icon(), color);
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&question).font(anvil_ui::semibold(14.0)).color(p.text));
            };
            if one_row {
                ui.horizontal(|ui| {
                    ask(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        delete_button(ui);
                        cancel_button(ui);
                    });
                });
            } else {
                ui.horizontal(ask);
                ui.horizontal(|ui| {
                    cancel_button(ui);
                    delete_button(ui);
                });
            }
        });
    (yes.get(), no.get(), frame.response.rect)
}

/// Вопрос «Удалить сцену «…»?» не шире `max` точек: у длинного имени отрезается конец и ставится многоточие.
fn fit_question(lang: Lang, name: &str, max: f32, measure: &impl Fn(&str, egui::FontId) -> f32) -> String {
    let chars: Vec<char> = name.chars().collect();
    for keep in (0..=chars.len()).rev() {
        let shown: String =
            if keep == chars.len() { name.to_string() } else { chars[..keep].iter().collect::<String>() + "…" };
        let question = texts::scene_delete_q(lang, &shown);
        if measure(&question, anvil_ui::semibold(14.0)) <= max {
            return question;
        }
    }
    texts::scene_delete_q(lang, "…")
}

/// Тихая кнопка цветом `danger` со значком — «Удалить сцену» в шапке правки. В наборе красная кнопка только с подложкой
/// (`Kind::Danger`), а тихой красной нет.
fn ghost_danger(ui: &mut Ui, icon: Icon, text: &str) -> egui::Response {
    let p = Palette::of(ui);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(14.0), p.danger);
    let size = Vec2::new(12.0 + 16.0 + 7.0 + galley.size().x + 12.0, 30.0);
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    if r.hovered() || r.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 6, p.hover);
    }
    anvil_ui::icons::paint(
        ui.painter(),
        Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.center().y - 8.0), Vec2::splat(16.0)),
        icon,
        p.danger,
    );
    let at = Pos2::new(rect.left() + 12.0 + 16.0 + 7.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, p.danger);
    w::focus_ring(ui, rect, &r, 6);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    r.on_hover_cursor(CursorIcon::PointingHand)
}

/// Строка сцены: галочка 16, подпись 13,5 слева и значение 13 справа. Отмечена — галочка на акценте, подпись и значение
/// `text`; не отмечена — рамка `border_strong`, подпись и «не трогать» `weak`; недоступна (`usable` ложно) — `faint`, не
/// нажимается, в подсказке причина. В наборе галочки нет — своя, на цветах набора (гайдлайн §11).
fn scene_row(
    ui: &mut Ui,
    id: egui::Id,
    label: &str,
    value: &str,
    hint: &str,
    on: bool,
    usable: bool,
) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    let r = ui.interact(rect, id, if usable { Sense::click() } else { Sense::hover() });
    if usable && r.hovered() {
        ui.painter().rect_filled(rect, 6, p.hover);
    }
    let boxed = Rect::from_center_size(Pos2::new(rect.left() + 6.0 + 8.0, rect.center().y), Vec2::splat(16.0));
    if on {
        ui.painter().rect_filled(boxed, 4, p.accent);
        anvil_ui::icons::paint(ui.painter(), boxed.shrink(2.5), Icon::Check, p.on_accent);
    } else {
        let edge = if usable { p.border_strong } else { p.border };
        ui.painter().rect_stroke(boxed, 4, Stroke::new(1.5, edge), StrokeKind::Inside);
    }
    let color = if on {
        p.text
    } else if usable {
        p.weak
    } else {
        p.faint
    };
    let value = ui.painter().layout_no_wrap(value.to_owned(), egui::FontId::proportional(13.0), color);
    let value_left = rect.right() - 8.0 - value.size().x;
    let left = boxed.right() + 9.0;
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), egui::FontId::proportional(13.5), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width((value_left - 8.0 - left).max(20.0));
    let label_galley = ui.painter().layout_job(job);
    ui.painter().galley(Pos2::new(left, rect.center().y - label_galley.size().y / 2.0), label_galley, color);
    ui.painter().galley(Pos2::new(value_left, rect.center().y - value.size().y / 2.0), value, color);
    w::focus_ring(ui, rect, &r, 6);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, usable, on, label));
    let r = if hint.is_empty() { r } else { r.on_hover_text(hint) };
    if usable { r.on_hover_cursor(CursorIcon::PointingHand) } else { r }
}

/// Тонкая вертикальная черта в шапке.
fn vline(ui: &mut Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, color);
}

/// Затухание к правому краю: от прозрачного к `color`.
/// Ряд, что листается только вбок: простое колесо листает его тоже (egui `always_scroll_the_only_direction`). Прокрутки
/// внутри (карточки, колонки) обрабатывают колесо раньше ряда и забирают его, только если им есть куда листать, — остаток
/// достаётся ряду. Внутри флаг снимает `wheel_inside`.
fn wheel_sideways(ui: &mut Ui) {
    ui.style_mut().always_scroll_the_only_direction = true;
}

/// Содержимое ряда: обычное колесо. Иначе вертикальные прокрутки карточек забирали бы и Shift + колесо.
fn wheel_inside(ui: &mut Ui) {
    ui.style_mut().always_scroll_the_only_direction = false;
}

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
            // В тестах у панели есть клавиатура (как у вызванной клавишей): печать и Ctrl+K открывают поиск.
            let mut edge = Edge::default();
            edge.take_keyboard();
            let height = settings.height;
            App {
                settings,
                edge,
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
                plaques: plaque::Queue::default(),
                plaque_drawn: None,
                plaque_t: 0.0,
                watch: watch::Watch::new(),
                release: None,
                own_drop: None,
                notify_ok: true,
                notify_at: None,
                scene: None,
                last_scene: None,
                ask_delete: None,
                ask_scroll: false,
                scroll_to: None,
                height,
                fit: Vec::new(),
                held: None,
                tab_wheel: 0.0,
                header_plan: header_plan::Plan::default(),
                header_edges: (0.0, f32::MAX),
                shelf: 0,
                editing: None,
                renaming: None,
                color: ColorEdit::new(look::LIME.dark.fill),
                search: Search::default(),
                recent: Vec::new(),
                pointer_over: false,
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
        /// Что нарисовано в последнем кадре.
        shapes: Vec<egui::epaint::ClippedShape>,
        /// Что услышал бы диктор в последнем кадре (щелчки по кнопкам с именами).
        events: Vec<egui::output::OutputEvent>,
        /// Какой курсор попросило окно в последнем кадре.
        cursor: egui::CursorIcon,
    }

    impl Rig {
        fn new() -> Rig {
            Rig::sized(SIZE.x)
        }

        /// Панель другой ширины (высота та же).
        fn sized(width: f32) -> Rig {
            let mut settings = Settings { width, height: SIZE.y, ..Settings::default() };
            // Прежние тесты рисуют панель 340 высотой: у полок своя высота. Тесты высоты по полке снимают её (`fitted`).
            for sh in &mut settings.shelves {
                sh.height = Some(SIZE.y as u16);
            }
            let ctx = egui::Context::default();
            anvil_ui::install(&ctx, look::accent_of(&settings.accent), settings.common.theme);
            let size = Vec2::new(width, SIZE.y);
            let mut rig = Rig {
                ctx,
                app: App::for_test(settings),
                time: 0.0,
                mouse: Pos2::new(5.0, 5.0),
                size,
                shapes: Vec::new(),
                events: Vec::new(),
                cursor: egui::CursorIcon::Default,
            };
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
            // Как `tick` перед кадром: высота панели, и окно (`screen_rect`) — её высоты.
            self.app.step_height(&self.ctx);
            self.size.y = self.app.height;
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
            self.shapes = out.shapes;
            self.cursor = out.platform_output.cursor_icon;
            self.events = std::mem::take(&mut out.platform_output.events);
        }

        /// Рамки цвета `color` в последнем кадре: прямоугольники с обводкой этого цвета.
        fn outlines(&self, color: Color32) -> Vec<Rect> {
            fn walk(shape: &egui::Shape, color: Color32, out: &mut Vec<Rect>) {
                match shape {
                    egui::Shape::Rect(r) if r.stroke.color == color && r.stroke.width > 0.0 => out.push(r.rect),
                    egui::Shape::Vec(list) => list.iter().for_each(|s| walk(s, color, out)),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            self.shapes.iter().for_each(|s| walk(&s.shape, color, &mut out));
            out
        }

        fn frames(&mut self, n: usize) {
            for _ in 0..n {
                self.frame(Vec::new());
            }
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

        /// Щелчок; ответ — что услышал бы диктор в кадре, когда кнопку отпустили.
        fn click_events(&mut self, pos: Pos2) -> Vec<egui::output::OutputEvent> {
            self.move_to(pos);
            self.frame(vec![self.button(true)]);
            self.frame(vec![self.button(false)]);
            let events = std::mem::take(&mut self.events);
            self.settle();
            events
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
        // Контроль: вне правки щелчок кнопку нажимает («День»: «Встречи» полторы доли, «Таймеры» и «Часы» по одной).
        let mut rig = Rig::new();
        rig.app.set_shelf(2);
        rig.settle();
        let timers = 12.0 + (SIZE.x - 24.0 - 2.0 * GAP) * 1.5 / 3.5 + GAP;
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
        let screen =
            config::ProfileMonitor { device: dev.into(), width: 2560, height: 1440, hz: 144, scale: None, hdr: None };
        rig.app.settings.profiles = vec![Profile { monitors: vec![screen], ..Profile::new("P") }];
        // «Экран» — на первой полке; открыта третья, и идёт правка.
        rig.app.set_shelf(2);
        rig.app.start_edit();
        rig.settle();
        let _one_at_a_time = sys::KEYS_TEST.lock().unwrap_or_else(|e| e.into_inner());
        sys::press_profile(0);
        rig.app.profile_keys(&rig.ctx, dev, Instant::now());
        assert!(rig.app.editing.is_none(), "клавиша сцены вывела из правки");
        assert!(rig.app.modules.iter().any(|m| m.busy()), "режим сменился — ждём «Оставить?»");
        assert_eq!(rig.app.shelf, 0, "открылась полка, где «Экран»: вопрос виден");
        // Щелчок по чипу сцены с другой полки: тот же вывод — открывается полка с вопросом.
        let mut rig = Rig::new();
        rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
        rig.app.settings.monitor = dev.into();
        let other =
            config::ProfileMonitor { device: dev.into(), width: 1920, height: 1080, hz: 120, scale: None, hdr: None };
        rig.app.settings.profiles = vec![Profile { monitors: vec![other], ..Profile::new("Q") }];
        rig.app.set_shelf(1);
        rig.app.command_all(Command::ApplyScene(0));
        assert_eq!(rig.app.shelf, 0, "после щелчка по чипу сцены — тоже");
    }

    #[test]
    fn hdr_button_flips_the_invented_hdr_and_the_veil_blocks_it() {
        let dev = r"\\.\TEST-HDR-BUTTON";
        let hdr_on = || sys::display::hdr(dev).map(|h| h.enabled);
        let rect = monitor::PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        let setup = |mut rig: Rig| {
            rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
            rig.app.settings.monitor = dev.into();
            // «Экран» опрашивает `update`, а тест гоняет только рисование: опрашиваем сами.
            rig.app.modules.iter_mut().find(|m| m.key() == "display").unwrap().refresh(dev);
            rig.settle();
            rig
        };
        let mut rig = setup(Rig::new());
        let button = rig.rect(modules::display::hdr_button_id());
        assert_eq!(button.height(), 30.0);
        assert_eq!(hdr_on(), Some(false), "выдуманный монитор стартует с выключенным HDR");
        rig.click(button.center());
        assert_eq!(hdr_on(), Some(true), "щелчок по кнопке включил HDR выдуманного монитора");
        rig.click(button.center());
        assert_eq!(hdr_on(), Some(false), "и выключил");
        // В правке содержимое карточки отключено: щелчок до кнопки не доходит (панель пошире: в правке карточки уже, и
        // на 1300 кнопке не хватило бы места).
        let mut rig = setup(Rig::sized(1800.0));
        rig.click(rig.rect(modules::display::hdr_button_id()).center());
        assert_eq!(
            hdr_on(),
            Some(true),
            "вне правки и на широкой панели кнопка работает — иначе проверка ниже ничего не стоит"
        );
        rig.click(rig.rect(modules::display::hdr_button_id()).center());
        assert_eq!(hdr_on(), Some(false));
        rig.app.start_edit();
        rig.settle();
        rig.click(rig.rect(modules::display::hdr_button_id()).center());
        assert_eq!(hdr_on(), Some(false), "в правке щелчок до кнопки не дошёл");
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
        let screen = config::ProfileMonitor {
            device: dev.into(),
            width: 1920,
            height: 1080,
            hz: 144,
            scale: Some(200),
            hdr: None,
        };
        rig.app.settings.profiles = vec![Profile { monitors: vec![screen], ..Profile::new("P") }];
        rig.app.command_all(Command::ApplyScene(0));
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
        let profile = |name: &str| Profile::new(name);
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
        // Много полок с длинными именами и сцены: ничего не наезжает и в самом тесном случае (девять вкладок).
        let mut rig = Rig::sized(560.0);
        for i in 0..9 {
            if rig.app.settings.shelves.len() <= i {
                rig.app.settings.shelves.push(config::Shelf {
                    name: format!("Полка с длинным именем {i}"),
                    cards: Vec::new(),
                    height: None,
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
    fn first_shelf_fits_in_1300_while_the_display_card_has_hdr_up_to_four_monitors() {
        // «Экран» с HDR и выбором монитора — до четырёх мониторов не шире 560 (`min_width_for`): «Экран и звук» остаётся в
        // 1300 без прокрутки, хотя запас всего несколько точек.
        let keys = modules::DEFAULT_SHELVES[0].1;
        assert_eq!(keys, ["display", "brightness", "sound", "media"], "полка «Экран и звук» по умолчанию");
        let all = modules::all();
        let area = SIZE.x - 24.0;
        let free = area - GAP * (keys.len() - 1) as f32;
        for monitors in 1..=4 {
            let sizes: Vec<(f32, f32)> = keys
                .iter()
                .map(|key| {
                    let m = all.iter().find(|m| m.key() == *key).expect("карточка есть");
                    let min =
                        if *key == "display" { modules::display::min_width_for(monitors, true) } else { m.min_width() };
                    (m.weight(), min)
                })
                .collect();
            let widths = fit_widths(free, &sizes);
            let row = widths.iter().sum::<f32>() + GAP * (widths.len() - 1) as f32;
            assert!(row <= area + 0.5, "{monitors} мон.: ряд {row} шире места {area}, полка листалась бы");
        }
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
                // И простое колесо: карточкам под вуалью листать нечего — листается ряд.
                let mut rig = Rig::sized(width);
                rig.app.set_shelf(shelf);
                rig.app.start_edit();
                rig.settle();
                let over = rig.drop_zone(&cards[0]).center();
                rig.wheel(over, -3.0, Modifiers::NONE);
                let after = rig.first_left();
                assert!(
                    after.is_none_or(|x| x < before - 20.0),
                    "{width}/{shelf}: колесо без Shift не листает: {before} → {after:?}"
                );
            }
        }
    }

    /// Панель «по полке»: своей высоты у полок нет, кадры идут, пока высота не встанет.
    fn fitted(width: f32) -> Rig {
        let mut rig = Rig::sized(width);
        for sh in &mut rig.app.settings.shelves {
            sh.height = None;
        }
        rig.app.fit.clear();
        rig.frames(30);
        rig
    }

    fn day(rig: &Rig) -> usize {
        rig.app.settings.shelves.iter().position(|s| s.name == "@day").expect("полка «День»")
    }

    /// Высота по полке встаёт и больше не качается: панель выросла — лишнего у карточек столько же меньше.
    #[test]
    fn the_panel_fits_the_shelf_and_stays_put() {
        for shelf in 0..4 {
            let mut rig = fitted(1300.0);
            rig.app.set_shelf(shelf);
            rig.frames(30);
            let h = rig.app.height;
            assert!((MIN_HEIGHT..=AUTO_MAX_HEIGHT).contains(&h), "{shelf}: {h}");
            assert_eq!(rig.app.fit[shelf], Some(h), "{shelf}: высота — замер полки");
            for _ in 0..20 {
                rig.frame(Vec::new());
                assert!((rig.app.height - h).abs() < 0.5, "{shelf}: качается {h} → {}", rig.app.height);
            }
        }
    }

    /// Больше строк в карточке — выше полка (до `AUTO_MAX_HEIGHT`); строк меньше — снова ниже.
    #[test]
    fn more_rows_make_the_shelf_taller() {
        let zones = ["Tokyo Standard Time", "Eastern Standard Time", "GMT Standard Time", "Russian Standard Time"];
        let mut rig = fitted(1300.0);
        let day = day(&rig);
        rig.app.settings.clocks = vec![config::Clock { name: "A".into(), zone: zones[0].into() }];
        rig.app.set_shelf(day);
        rig.frames(30);
        let few = rig.app.height;
        rig.app.settings.clocks =
            (0..8).map(|i| config::Clock { name: format!("C{i}"), zone: zones[i % 4].into() }).collect();
        rig.frames(30);
        let many = rig.app.height;
        assert!(many > few + 40.0 || many == AUTO_MAX_HEIGHT, "{few} → {many}");
        assert!(many <= AUTO_MAX_HEIGHT);
        rig.app.settings.clocks.truncate(1);
        rig.frames(30);
        assert!((rig.app.height - few).abs() < 1.0, "назад к {few}: {}", rig.app.height);
    }

    /// Смена полки: высота идёт к нужной плавно (`LAYOUT`), без анимации Windows — сразу.
    #[test]
    fn switching_shelves_animates_the_height() {
        let mut rig = fitted(1300.0);
        rig.app.settings.shelves[1].height = Some(500);
        let from = rig.app.height;
        rig.app.set_shelf(1);
        rig.frames(2);
        let mid = rig.app.height;
        assert!(mid > from + 1.0 && mid < 499.0, "{from} → {mid} → 500");
        rig.frames(20);
        assert_eq!(rig.app.height, 500.0);
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.set_shelf(0);
        rig.frames(2);
        assert_eq!(Some(rig.app.height), rig.app.fit[0], "без анимации — сразу");
    }

    /// Потянули нижний край — своя высота этой полки; двойной щелчок по краю — снова «по полке».
    #[test]
    fn dragged_height_belongs_to_the_shelf_and_double_click_fits_again() {
        let mut rig = fitted(1300.0);
        rig.app.settings.common.language = Lang::Ru;
        rig.app.set_height(470.0);
        assert_eq!(rig.app.settings.shelves[0].height, Some(470));
        assert_eq!(rig.app.settings.shelves[1].height, None, "другая полка — по полке");
        rig.frames(20);
        assert_eq!(rig.app.height, 470.0);
        let edge = Pos2::new(rig.size.x / 2.0 + 100.0, rig.size.y - 3.0);
        rig.move_to(edge);
        assert!(painted(&rig).contains("Своя высота"), "подпись у края");
        rig.double_click(edge);
        assert_eq!(rig.app.settings.shelves[0].height, None);
        rig.frames(20);
        assert_eq!(Some(rig.app.height), rig.app.fit[0]);
        rig.move_to(Pos2::new(rig.size.x / 2.0 + 100.0, rig.size.y - 3.0));
        assert!(painted(&rig).contains("Высота по полке"));
    }

    /// Настройки, поиск и правка полок держат высоту, что была: панель не прыгает под их содержимое.
    #[test]
    fn views_without_a_shelf_keep_the_height() {
        for open in [View::Settings, View::Search] {
            let mut rig = fitted(1300.0);
            rig.app.settings.shelves[0].height = Some(480);
            rig.frames(20);
            rig.app.view = open;
            rig.app.settings.shelves[0].height = Some(200);
            rig.frames(20);
            assert_eq!(rig.app.height, 480.0, "{}", open as u8);
            rig.app.view = View::Cards;
            rig.frames(20);
            assert_eq!(rig.app.height, 200.0, "{}: назад на полку — её высота", open as u8);
        }
        let mut rig = fitted(1300.0);
        let h = rig.app.height;
        rig.app.start_edit();
        rig.frames(20);
        assert_eq!(rig.app.height, h, "правка полок");
    }

    /// Своя высота полок — в `config.txt` строкой `shelf_heights=` по порядку полок; «по полке» — пусто.
    #[test]
    fn shelf_heights_survive_the_file() {
        let mut s = Settings::default();
        s.shelves[1].height = Some(470);
        let text = config::format(&s);
        assert!(text.contains("shelf_heights=,470,,\n"), "{text}");
        let back = config::parse(&text);
        let heights: Vec<Option<u16>> = back.shelves.iter().map(|s| s.height).collect();
        assert_eq!(heights, [None, Some(470), None, None]);
        assert!(!config::format(&Settings::default()).contains("shelf_heights"), "все по полке — строки нет");
    }

    /// Колесо без Shift над рядом, что не помещается: над карточкой без прокрутки листает ряд.
    #[test]
    fn the_wheel_scrolls_the_row_without_shift() {
        let mut rig = Rig::sized(700.0);
        let first = rig.app.settings.shelves[0].cards[0].clone();
        let anchor = format!("card-{first}");
        let left = |rig: &Rig| rig.rect(egui::Id::new(("anchor", anchor.as_str()))).left();
        let before = left(&rig);
        // «Экран» без монитора — одна строка, листать ему нечего.
        let over = rig.rect(egui::Id::new(("anchor", anchor.as_str()))).center();
        rig.wheel(over, -3.0, Modifiers::NONE);
        assert!(left(&rig) < before - 20.0, "ряд не листается: {before} → {}", left(&rig));
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
        // Контроль: вне правки Tab доводит фокус до кнопки «Таймеров» (вторая карточка полки «День»), Enter заводит таймер.
        let mut rig = Rig::new();
        rig.app.set_shelf(2);
        rig.settle();
        let width = (SIZE.x - 24.0 - 2.0 * GAP) / 3.5;
        let timers = Rect::from_min_size(Pos2::new(12.0 + 1.5 * width + GAP, BODY_TOP), Vec2::new(width, 280.0));
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
        assert_eq!(rig.app.shelf, 3, "по кругу: с первой — на четвёртую («Под рукой»)");
        rig.key(Key::Num9, CTRL);
        assert_eq!(rig.app.shelf, 3, "нет такой полки — остаётся");
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
        assert_eq!(rig.app.settings.shelves.len(), 3);
        for card in &before {
            assert!(rig.app.settings.is_hidden(card), "{card} ушла в «Убранные»");
        }
        assert_eq!(rig.app.shelf, 2, "открытой стала та, что встала на её место");
        // Единственную полку убрать нельзя: крестика нет.
        rig.app.settings.remove_shelf(1);
        rig.app.settings.remove_shelf(1);
        rig.settle();
        assert!(rig.ctx.read_response(egui::Id::new(("shelf-close", 0))).is_none());
    }

    #[test]
    fn empty_shelf_offers_editing() {
        let mut rig = Rig::new();
        rig.app.settings.shelves.push(config::Shelf { name: "Пусто".into(), cards: Vec::new(), height: None });
        rig.app.set_shelf(4);
        rig.settle();
        assert!(rig.app.editing.is_none());
        assert!(rig.app.shelf_modules(4).is_empty());
    }

    // ─── Сцены ──────────────────────────────────────────────────────────────
    //
    // Выдуманный мир (экран, яркость, тема, «Тишина», Wi‑Fi): сцены ставятся и возвращаются без Windows и без окна. Мир
    // один на все тесты — под `WORLD_TEST`; у каждого теста свой выдуманный монитор.

    /// Мир под замком, как вначале.
    fn world() -> std::sync::MutexGuard<'static, ()> {
        let guard = sys::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        sys::reset_fake_world();
        guard
    }

    /// Сцена «Movie»: экран 2560 × 1440 · 144 Гц на `dev` и «Тишина».
    fn movie(dev: &str) -> Profile {
        let screen =
            config::ProfileMonitor { device: dev.into(), width: 2560, height: 1440, hz: 144, scale: None, hdr: None };
        let system = config::SceneSystem { quiet: Some(true), ..Default::default() };
        Profile { monitors: vec![screen], system, ..Profile::new("Movie") }
    }

    /// Сцена из одной системной настройки.
    fn only(name: &str, system: config::SceneSystem) -> Profile {
        Profile { system, ..Profile::new(name) }
    }

    /// Панель с выдуманным монитором `dev` и сценами; то, что читают сцены (экран, яркость, быстрые настройки), опрошено, как
    /// при показе панели.
    fn scene_rig(dev: &str, scenes: Vec<Profile>) -> Rig {
        let mut rig = Rig::new();
        let rect = monitor::PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
        rig.app.settings.monitor = dev.into();
        rig.app.settings.profiles = scenes;
        rig.refresh_scene_modules(dev);
        rig.settle();
        rig
    }

    impl Rig {
        /// Опросить то, что читает сцены. Яркость и Wi‑Fi читает поток модуля — ждём, пока ответят.
        fn refresh_scene_modules(&mut self, dev: &str) {
            for key in ["display", "quick", "brightness"] {
                let i = self.app.module_index(key).expect("модуль есть");
                self.app.modules[i].refresh(dev);
            }
            let start = Instant::now();
            while !self.app.modules.iter().flat_map(|m| m.scene_rows()).all(|(_, ok)| ok) {
                assert!(start.elapsed() < Duration::from_secs(3), "модули не ответили");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn right_click(&mut self, pos: Pos2) {
            self.move_to(pos);
            let ev = |pressed| Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![ev(true)]);
            self.frame(vec![ev(false)]);
            self.settle();
        }

        /// Место виджета с `anchor`.
        fn anchor(&self, key: &str) -> Rect {
            self.rect(egui::Id::new(("anchor", key)))
        }

        fn click_anchor(&mut self, key: &str) {
            let at = self.anchor(key).center();
            self.click(at);
        }

        fn type_text(&mut self, text: &str) {
            self.frame(vec![Event::Text(text.into())]);
            self.settle();
        }

        /// Строка правки сцены с номером в `SceneRow::ALL`.
        fn scene_row(&self, n: usize) -> Rect {
            self.rect(egui::Id::new(("scene-row", n)))
        }

        /// Открыть правку первой сцены правым щелчком по её чипу.
        fn open_editor(&mut self, chip: usize) {
            let at = self.anchor(&format!("scene-chip-{chip}")).center();
            self.right_click(at);
            assert!(self.app.view == View::Scene && self.app.scene.is_some(), "правка не открылась");
        }
    }

    #[test]
    fn a_click_on_the_chip_sets_the_scene_and_a_second_click_returns_it() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-CHIP";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        let (t, lang) = (texts::tr(rig.app.lang()), rig.app.lang());
        let start = sys::display::current(dev).unwrap();
        assert_eq!(rig.app.active_scene(), None);
        let (chip, before) = (rig.anchor("scene-chip-0"), Instant::now());
        // Первый щелчок: экран ждёт «Оставить?», «Тишина» включена, чип выделен, уведомление называет сцену.
        rig.click(chip.center());
        assert!(rig.app.modules.iter().any(|m| m.busy()));
        assert_eq!(sys::system::notifications_off(), Some(true));
        assert_ne!(sys::display::current(dev), Some(start));
        assert_eq!(rig.app.active_scene(), Some(0));
        assert_eq!(rig.app.toasts.since(before), vec![(texts::scene_set(lang, "Movie"), Tone::Neutral)]);
        // Второй щелчок — как было: экран и «Тишина»; вопрос снят.
        let before = Instant::now();
        rig.click(chip.center());
        assert!(!rig.app.modules.iter().any(|m| m.busy()));
        assert_eq!(sys::display::current(dev), Some(start));
        assert_eq!(sys::system::notifications_off(), Some(false));
        assert_eq!(rig.app.active_scene(), None);
        assert_eq!(rig.app.toasts.since(before), vec![(t.scene_undone.to_string(), Tone::Neutral)]);
    }

    #[test]
    fn the_movie_scene_sets_the_screen_hdr_quiet_and_awake_and_nothing_else() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-SPEC";
        let screen = config::ProfileMonitor {
            device: dev.into(),
            width: 3840,
            height: 2160,
            hz: 60,
            scale: None,
            hdr: Some(true),
        };
        let system = config::SceneSystem { quiet: Some(true), awake: Some(true), ..Default::default() };
        let mut rig = scene_rig(dev, vec![Profile { monitors: vec![screen], system, ..Profile::new("Movie") }]);
        let start = sys::display::current(dev).unwrap();
        assert_eq!(sys::display::hdr(dev).map(|h| h.enabled), Some(false));
        rig.click_anchor("scene-chip-0");
        // Режим — с «Оставить этот режим?», HDR, «Тишина» и «Не спать» включены (чип выделен: «Не спать» стоит тоже).
        assert!(rig.app.modules.iter().any(|m| m.busy()));
        assert_eq!(sys::display::current(dev).map(|m| (m.width, m.height, m.hz)), Some((3840, 2160, 60)));
        assert_eq!(sys::display::hdr(dev).map(|h| h.enabled), Some(true));
        assert_eq!(sys::system::notifications_off(), Some(true));
        assert_eq!(rig.app.active_scene(), Some(0));
        // Остальное не тронуто: тема, план питания, Wi‑Fi, ночной свет.
        assert_eq!(sys::system::dark_theme(), Some(true));
        assert_eq!(sys::system::plans().1, Some(windows::core::GUID::from_u128(1)));
        assert_eq!((sys::net::wifi().map(|w| w.on), sys::nightlight::state()), (Some(true), Some(false)));
        // Снова «Кино» — всё как до неё, в том числе HDR и режим.
        let before = Instant::now();
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::display::current(dev), Some(start));
        assert_eq!(sys::display::hdr(dev).map(|h| h.enabled), Some(false));
        assert_eq!(sys::system::notifications_off(), Some(false));
        assert_eq!(rig.app.active_scene(), None);
        assert_eq!(
            rig.app.toasts.since(before),
            vec![(texts::tr(rig.app.lang()).scene_undone.to_string(), Tone::Neutral)]
        );
    }

    #[test]
    fn a_profile_saved_before_scenes_changes_the_screen_only() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-OLD-LINE";
        let line = format!("profile=Old | | {dev} 2560x1440@144 125%\n");
        let scenes = config::parse(&line).profiles;
        assert!(scenes[0].system.is_empty() && scenes[0].undo);
        let mut rig = scene_rig(dev, scenes);
        let start = sys::display::current(dev).unwrap();
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::display::current(dev).map(|m| (m.width, m.hz)), Some((2560, 144)));
        assert_eq!(sys::display::scale(dev).map(|s| s.current), Some(125));
        // Система — как была: тема, «Тишина», план, Wi‑Fi, ночной свет.
        assert_eq!((sys::system::dark_theme(), sys::system::notifications_off()), (Some(true), Some(false)));
        assert_eq!((sys::net::wifi().map(|w| w.on), sys::nightlight::state()), (Some(true), Some(false)));
        // Второй щелчок возвращает экран (профили до 2.1 так не умели; у сцены возврат включён по умолчанию).
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::display::current(dev), Some(start));
    }

    #[test]
    fn the_key_sets_and_returns_the_scene_and_brings_the_panel_out() {
        let _world = world();
        let _keys = sys::KEYS_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let dev = r"\\.\TEST-SCENE-KEY";
        let mut scene = movie(dev);
        scene.hotkey = Hotkey::parse("Ctrl+Alt+1");
        let mut rig = scene_rig(dev, vec![scene]);
        let start = sys::display::current(dev).unwrap();
        assert!(!rig.app.edge.shown);
        sys::press_profile(0);
        let came = rig.app.profile_keys(&rig.ctx, dev, Instant::now());
        assert!(came && rig.app.edge.shown, "панель выехала");
        assert_eq!(sys::system::notifications_off(), Some(true));
        assert!(rig.app.modules.iter().any(|m| m.busy()), "режим экрана ждёт ответа");
        // Та же клавиша ещё раз — вернуть.
        sys::press_profile(0);
        rig.app.profile_keys(&rig.ctx, dev, Instant::now());
        assert_eq!((sys::display::current(dev), sys::system::notifications_off()), (Some(start), Some(false)));
        // Клавишу нажали в правке сцены — сначала выход из неё, потом сцена ставится.
        rig.app.open_scene(0, false, View::Cards);
        sys::press_profile(0);
        rig.app.profile_keys(&rig.ctx, dev, Instant::now());
        assert!(rig.app.scene.is_none() && rig.app.view == View::Cards);
        assert_eq!(sys::system::notifications_off(), Some(true));
    }

    #[test]
    fn a_scene_without_return_only_says_it_already_stands() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-NO-UNDO";
        let mut scene = movie(dev);
        scene.undo = false;
        let mut rig = scene_rig(dev, vec![scene]);
        let t = texts::tr(rig.app.lang());
        rig.click_anchor("scene-chip-0");
        let stands = (sys::display::current(dev), sys::system::notifications_off());
        let before = Instant::now();
        rig.click_anchor("scene-chip-0");
        assert_eq!((sys::display::current(dev), sys::system::notifications_off()), stands, "ничего не вернулось");
        assert_eq!(rig.app.toasts.since(before), vec![(t.scene_already.to_string(), Tone::Neutral)]);
    }

    #[test]
    fn a_scene_that_stands_by_itself_has_nothing_to_return() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-BY-ITSELF";
        // Тема тёмная вначале: сцена «тёмная тема» стоит, но её не ставили (скажем, программу перезапустили).
        let mut rig =
            scene_rig(dev, vec![only("Dark", config::SceneSystem { dark: Some(true), ..Default::default() })]);
        let t = texts::tr(rig.app.lang());
        assert_eq!(rig.app.active_scene(), Some(0));
        let before = Instant::now();
        rig.click_anchor("scene-chip-0");
        assert_eq!(rig.app.toasts.since(before), vec![(t.scene_nothing_undo.to_string(), Tone::Neutral)]);
        assert_eq!(sys::system::dark_theme(), Some(true));
    }

    #[test]
    fn a_keep_prompt_blocks_another_scene_the_editor_and_the_plus() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-BLOCKED";
        let dark = only("Dark", config::SceneSystem { dark: Some(false), ..Default::default() });
        let mut rig = scene_rig(dev, vec![movie(dev), dark]);
        let t = texts::tr(rig.app.lang());
        rig.click_anchor("scene-chip-0");
        assert!(rig.app.modules.iter().any(|m| m.busy()));
        // Пока ждут ответа: другая сцена не ставится, правка и «+» не открываются — каждый раз «Сначала ответьте…».
        let warn = (t.answer_first.to_string(), Tone::Warning);
        let before = Instant::now();
        rig.click_anchor("scene-chip-1");
        assert_eq!(sys::system::dark_theme(), Some(true));
        let at = rig.anchor("scene-chip-1").center();
        rig.right_click(at);
        rig.click_anchor("scene-new");
        assert!(rig.app.view == View::Cards && rig.app.settings.profiles.len() == 2);
        assert_eq!(rig.app.toasts.since(before), vec![warn.clone(), warn.clone(), warn.clone()]);
        // «Изменить…» в настройках — тоже.
        rig.app.view = View::Settings;
        rig.settle();
        let before = Instant::now();
        rig.click_anchor("scene-open-1");
        assert!(rig.app.view == View::Settings && rig.app.scene.is_none());
        assert_eq!(rig.app.toasts.since(before), vec![warn]);
        rig.app.view = View::Cards;
        rig.settle();
        // Ответили («Вернуть» — вторым щелчком по первой сцене), и другая сцена ставится.
        rig.click_anchor("scene-chip-0");
        rig.click_anchor("scene-chip-1");
        assert_eq!(sys::system::dark_theme(), Some(false));
    }

    #[test]
    fn of_two_matching_scenes_the_one_set_last_is_marked() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-OVERLAP";
        let quiet = only("A", config::SceneSystem { quiet: Some(true), ..Default::default() });
        let both = only("B", config::SceneSystem { quiet: Some(true), awake: Some(true), ..Default::default() });
        let mut rig = scene_rig(dev, vec![quiet, both]);
        rig.app.scene_click(1, dev);
        // Подходят обе («А» — часть «Б»): выделена та, что ставили.
        assert_eq!(rig.app.active_scene(), Some(1));
        rig.app.last_scene = None;
        assert_eq!(rig.app.active_scene(), Some(0), "не помним, какую ставили, — первая подходящая");
        // «Б» стоит, и второй щелчок по ней возвращает именно её (а не «А»).
        rig.app.last_scene = Some(rig.app.settings.profiles[1].id);
        rig.app.scene_click(1, dev);
        assert_eq!((sys::system::notifications_off(), rig.app.active_scene()), (Some(false), None));
    }

    #[test]
    fn right_click_opens_the_editor_and_done_escape_or_back_leave_it() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-EDITOR";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        rig.open_editor(0);
        for key in ["scene-back", "scene-done", "scene-delete"] {
            rig.anchor(key);
        }
        rig.click_anchor("scene-done");
        assert!(rig.app.view == View::Cards && rig.app.scene.is_none());
        rig.open_editor(0);
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.view == View::Cards && rig.app.scene.is_none(), "Esc — как «Готово»");
        rig.open_editor(0);
        rig.click_anchor("scene-back");
        assert!(rig.app.view == View::Cards && rig.app.scene.is_none(), "«‹ Сцены» — сохранить и выйти");
        assert_eq!(rig.app.settings.profiles[0], movie(dev), "сцена цела");
    }

    #[test]
    fn plus_makes_a_scene_from_what_stands_now_and_names_it() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-PLUS";
        let lang = Rig::new().app.lang();
        let (first, third) = (texts::scene_n(lang, 1), texts::scene_n(lang, 3));
        let (mut a, mut b) = (movie(dev), movie(dev));
        (a.name, b.name) = (first, third);
        let mut rig = scene_rig(dev, vec![a, b]);
        rig.click_anchor("scene-new");
        assert!(rig.app.view == View::Scene);
        let edit = rig.app.scene.as_ref().expect("правка открыта");
        let i = edit.index;
        assert!(edit.fresh && i == 2);
        let s = rig.app.settings.profiles[i].clone();
        assert_eq!(s.name, texts::scene_n(lang, 2), "первый свободный номер");
        // Отмечен только экран, и в нём то, что стоит сейчас (4K, 120 Гц).
        for row in SceneRow::ALL {
            assert_eq!(s.has(row), row == SceneRow::Screen, "{row:?}");
        }
        assert_eq!((s.monitors[0].width, s.monitors[0].height, s.monitors[0].hz), (3840, 2160, 120));
        // Отметить «Тишину» (строка 3), назвать сцену: фокус в поле имени и имя выделено — печатаем, и оно заменяется.
        rig.click(rig.scene_row(3).center());
        rig.click(rig.rect(egui::Id::new("scene-name")).center());
        rig.key(Key::A, CTRL);
        rig.type_text("Report");
        assert_eq!(rig.app.settings.profiles[i].name, "Report");
        rig.click_anchor("scene-done");
        assert!(rig.app.view == View::Cards && rig.app.settings.profiles.len() == 3);
        rig.anchor("scene-chip-2");
        let made = rig.app.settings.profiles[2].clone();
        assert!(made.has(SceneRow::Screen) && made.system.quiet == Some(false) && !made.has(SceneRow::Awake));
        // Чип нового экрана-и-«Тишины» ставит и то и другое.
        rig.app.settings.profiles[2].system.quiet = Some(true);
        rig.app.settings.profiles[2].monitors[0].hz = 60;
        rig.click_anchor("scene-chip-2");
        assert_eq!(sys::system::notifications_off(), Some(true));
        assert_eq!(sys::display::current(dev).map(|m| m.hz), Some(60));
        rig.app.scene_click(2, dev);
        assert_eq!(sys::system::notifications_off(), Some(false), "второй щелчок вернул");
        // Новая сцена без отмеченных строк или без имени — уходит, как не было.
        rig.click_anchor("scene-new");
        let i = rig.app.scene.as_ref().unwrap().index;
        rig.app.settings.profiles[i].clear(SceneRow::Screen);
        rig.settle();
        assert!(rig.app.scene_problem().is_some(), "«Готово» серая, пока ничего не отмечено");
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.view == View::Cards && rig.app.settings.profiles.len() == 3);
        rig.click_anchor("scene-new");
        rig.key(Key::A, CTRL);
        rig.key(Key::Backspace, Modifiers::NONE);
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.view == View::Cards && rig.app.settings.profiles.len() == 3, "без имени новая не сохраняется");
    }

    #[test]
    fn rows_check_take_current_values_and_come_off() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-ROWS";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        rig.open_editor(0);
        let s = rig.app.settings.profiles[0].clone();
        assert!(s.has(SceneRow::Screen) && s.has(SceneRow::Quiet) && !s.has(SceneRow::Dark));
        // Снять «Тишину» (строка 3).
        rig.click(rig.scene_row(3).center());
        assert!(!rig.app.settings.profiles[0].has(SceneRow::Quiet));
        // Отметить «Тёмная тема» (5) и «План питания» (7): берутся текущие значения — тема тёмная, план «Balanced».
        rig.click(rig.scene_row(5).center());
        rig.click(rig.scene_row(7).center());
        let s = rig.app.settings.profiles[0].clone();
        assert_eq!(s.system.dark, Some(true));
        assert_eq!(s.system.plan, Some(sys::system::guid_string(&windows::core::GUID::from_u128(1))));
        // Мир изменился; «Запомнить текущие» берёт новое только у отмеченных строк.
        sys::system::set_dark_theme(false).unwrap();
        sys::system::set_notifications_off(true).unwrap();
        rig.click_anchor("scene-take");
        let s = rig.app.settings.profiles[0].clone();
        assert_eq!(s.system.dark, Some(false));
        assert!(!s.has(SceneRow::Quiet), "«Тишина» не отмечена — не берётся");
        assert_eq!((s.monitors[0].width, s.monitors[0].hz), (3840, 120), "экран — тот, что стоит сейчас");
        // Снять отмеченное — «не трогать».
        rig.click(rig.scene_row(5).center());
        assert!(!rig.app.settings.profiles[0].has(SceneRow::Dark));
    }

    #[test]
    fn rows_say_their_value_in_the_scenes_words() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-VALUES";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        let mut scene = movie(dev);
        scene.monitors[0].scale = Some(150);
        scene.monitors[0].hdr = Some(true);
        scene.system = config::SceneSystem {
            night: Some(false),
            quiet: Some(true),
            bright: Some(80),
            plan: Some(sys::system::guid_string(&windows::core::GUID::from_u128(2))),
            ..Default::default()
        };
        rig.open_editor(0);
        for (lang, mode, on, off, percent) in [
            (Lang::En, "2560 × 1440 · 144 Hz · 150% · HDR", "On", "Off", "80%"),
            (Lang::Ru, "2560 × 1440 · 144 Гц · 150 % · HDR", "Вкл", "Выкл", "80 %"),
        ] {
            rig.app.settings.common.language = lang;
            let value = |row| rig.app.scene_value(row, &scene).0;
            assert_eq!(value(SceneRow::Screen), mode);
            assert_eq!((value(SceneRow::Quiet), value(SceneRow::Night)), (on.to_string(), off.to_string()));
            assert_eq!(value(SceneRow::Bright), percent);
            assert_eq!(value(SceneRow::Plan), "High performance");
        }
        // Несколько мониторов — коротко, полное — в подсказке.
        rig.app.settings.common.language = Lang::En;
        let mut two = scene.clone();
        two.monitors.push(config::ProfileMonitor { device: r"\\.\DISPLAY2".into(), ..two.monitors[0].clone() });
        let (short, hint) = rig.app.scene_value(SceneRow::Screen, &two);
        assert_eq!(short, "2 monitors");
        assert!(hint.lines().count() == 2 && hint.contains("DISPLAY2 · 2560 × 1440"), "{hint}");
        // Плана нет среди планов Windows — тире.
        let mut gone = scene.clone();
        gone.system.plan = Some("{00000000-0000-0000-0000-0000000000FF}".into());
        assert_eq!(rig.app.scene_value(SceneRow::Plan, &gone).0, "—");
    }

    #[test]
    fn the_name_must_be_there_and_must_be_free() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-NAME";
        let t = texts::tr(Rig::new().app.lang());
        let mut rig = scene_rig(
            dev,
            vec![movie(dev), only("Work", config::SceneSystem { dark: Some(false), ..Default::default() })],
        );
        rig.open_editor(0);
        rig.click(rig.rect(egui::Id::new("scene-name")).center());
        rig.key(Key::A, CTRL);
        rig.type_text("Work");
        assert_eq!(rig.app.settings.profiles[0].name, "Movie", "такое имя уже есть — сцене оно не попадает");
        assert_eq!(rig.app.scene_problem(), Some(t.scene_name_taken));
        // «Готово» серая: щелчок ничего не делает.
        rig.click_anchor("scene-done");
        assert!(rig.app.view == View::Scene);
        // Пусто — тоже негодно (щелчок по кнопке увёл фокус из поля — возвращаем).
        rig.click(rig.rect(egui::Id::new("scene-name")).center());
        rig.key(Key::A, CTRL);
        rig.key(Key::Backspace, Modifiers::NONE);
        assert_eq!(rig.app.scene_problem(), Some(t.scene_name_empty));
        // Годное имя применяется на месте.
        rig.type_text("Doklad");
        assert_eq!((rig.app.settings.profiles[0].name.as_str(), rig.app.scene_problem()), ("Doklad", None));
        // Вышли с негодным именем: прежняя сцена остаётся под последним годным, а не пропадает.
        rig.key(Key::A, CTRL);
        rig.key(Key::Backspace, Modifiers::NONE);
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.view == View::Cards);
        assert_eq!(rig.app.settings.profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Doklad", "Work"]);
        // Имя из знаков, которыми размечена строка файла, чистится.
        rig.open_editor(0);
        rig.click(rig.rect(egui::Id::new("scene-name")).center());
        rig.key(Key::A, CTRL);
        rig.type_text("a|b;c");
        assert_eq!(rig.app.settings.profiles[0].name, "a b c");
    }

    #[test]
    fn delete_asks_in_place_and_removes_the_scene_with_its_key() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-DELETE";
        let mut first = movie(dev);
        first.hotkey = Hotkey::parse("Ctrl+Alt+1");
        let mut rig =
            scene_rig(dev, vec![first, only("Work", config::SceneSystem { dark: Some(false), ..Default::default() })]);
        let _keys = sys::KEYS_TEST.lock().unwrap_or_else(|e| e.into_inner());
        rig.app.profile_keys(&rig.ctx, dev, Instant::now());
        rig.open_editor(0);
        assert!(rig.ctx.read_response(egui::Id::new(("anchor", "scene-delete-yes"))).is_none(), "вопроса пока нет");
        rig.click_anchor("scene-delete");
        assert!(rig.app.scene.as_ref().unwrap().confirm_delete);
        rig.anchor("scene-delete-yes");
        // «Отмена» и Esc снимают вопрос, правка остаётся.
        rig.click_anchor("scene-delete-no");
        assert!(!rig.app.scene.as_ref().unwrap().confirm_delete && rig.app.settings.profiles.len() == 2);
        rig.click_anchor("scene-delete");
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.view == View::Scene && !rig.app.scene.as_ref().unwrap().confirm_delete);
        // «Удалить»: сцены нет — ни в шапке, ни в списке, клавиша свободна.
        rig.click_anchor("scene-delete");
        rig.click_anchor("scene-delete-yes");
        assert!(rig.app.view == View::Cards && rig.app.scene.is_none());
        assert_eq!(rig.app.settings.profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Work"]);
        assert!(rig.app.settings.profiles.iter().all(|p| p.hotkey.is_none()));
        assert!(rig.ctx.read_response(egui::Id::new(("anchor", "scene-chip-1"))).is_none());
        rig.anchor("scene-chip-0");
        // Клавиша освобождена: набор клавиш сцен окно пересобирает без неё.
        assert_eq!(rig.app.sent_keys, Some(vec![Some("Ctrl+Alt+1".to_string()), None]), "до удаления");
        rig.app.profile_keys(&rig.ctx, dev, Instant::now());
        assert_eq!(rig.app.sent_keys, Some(vec![None]));
        assert_eq!(sys::profile_hotkey_status(0), None);
    }

    #[test]
    fn the_undo_switch_and_the_key_in_the_editor() {
        let _world = world();
        let _keys = sys::KEYS_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let dev = r"\\.\TEST-SCENE-SWITCH";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        rig.open_editor(0);
        assert!(rig.app.settings.profiles[0].undo);
        rig.click_anchor("scene-undo");
        assert!(!rig.app.settings.profiles[0].undo);
        rig.click_anchor("scene-undo");
        assert!(rig.app.settings.profiles[0].undo);
        // Esc во время ожидания сочетания отменяет только его, из правки не выходит.
        rig.app.capture = Some(0);
        rig.settle();
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.capture.is_none() && rig.app.view == View::Scene);
        // Сочетание с Ctrl и Alt назначается; то же сочетание у другой сцены переходит к этой.
        rig.app.settings.profiles.push(only("Other", config::SceneSystem { dark: Some(false), ..Default::default() }));
        rig.app.settings.profiles[1].hotkey = Hotkey::parse("Ctrl+Alt+7");
        rig.app.capture = Some(0);
        rig.settle();
        rig.key(Key::Num7, Modifiers { alt: true, ..CTRL });
        assert_eq!(rig.app.settings.profiles[0].hotkey, Hotkey::parse("Ctrl+Alt+7"));
        assert_eq!(rig.app.settings.profiles[1].hotkey, None);
        assert!(rig.app.capture.is_none() && rig.app.view == View::Scene);
    }

    #[test]
    fn the_editor_fits_narrow_and_wide_panels() {
        let _world = world();
        for width in [560.0, 800.0, 1000.0, 1300.0, 1800.0] {
            let dev = r"\\.\TEST-SCENE-NARROW";
            let mut rig = Rig::sized(width);
            let rect = monitor::PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
            rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
            rig.app.settings.monitor = dev.into();
            rig.app.settings.profiles = vec![movie(dev)];
            rig.refresh_scene_modules(dev);
            rig.settle();
            // На узкой панели чипов в шапке нет — правку открывают из настроек.
            rig.app.open_scene(0, false, View::Cards);
            rig.settle();
            for n in 0..8 {
                rig.scene_row(n);
            }
            // Вопрос про удаление — над карточками, и обе кнопки видны целиком, а не где-то в листаемой части: с коротким
            // именем в одну строку, с самым длинным (24 широких буквы) на узкой панели — кнопки под вопросом.
            for name in ["Movie".to_string(), "W".repeat(config::MAX_SCENE_NAME)] {
                rig.app.settings.profiles[0].name = name.clone();
                rig.click_anchor("scene-delete");
                rig.settle();
                let visible = Rect::from_min_max(Pos2::new(12.0, 0.0), Pos2::new(width - 12.0, SIZE.y));
                let (yes, no) = (rig.anchor("scene-delete-yes"), rig.anchor("scene-delete-no"));
                for (key, button) in [("Delete", yes), ("Cancel", no)] {
                    assert!(visible.contains_rect(button), "{width} «{name}»: «{key}» за краем: {button:?}");
                }
                assert!(no.right() <= yes.left() || no.bottom() <= yes.top(), "{width}: кнопки наехали друг на друга");
                rig.click_anchor("scene-delete-no");
                assert!(!rig.app.scene.as_ref().unwrap().confirm_delete, "{width}: «Отмена» вопрос не сняла");
            }
            let done = rig.anchor("scene-done");
            assert!(done.right() <= width && done.left() > 0.0, "{width}: «Готово» за краем: {done:?}");
        }
    }

    #[test]
    fn a_long_name_in_the_editor_header_does_not_cover_the_buttons() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-LONG-NAME";
        // Длинное имя и самое широкое из возможных (24 «Ш»): название обрезается многоточием, а не заходит на кнопки.
        let names = ["Очень длинное имя сцены для шапки", "ШШШШШШШШШШШШШШШШШШШШШШШШ"];
        for (width, name) in [560.0, 700.0, 1300.0].into_iter().flat_map(|w| names.map(|n| (w, n))) {
            let mut rig = Rig::sized(width);
            let mut scene = movie(dev);
            scene.name = name.into();
            rig.app.settings.profiles = vec![scene];
            rig.app.open_scene(0, false, View::Cards);
            rig.settle();
            let (back, delete, done) = (rig.anchor("scene-back"), rig.anchor("scene-delete"), rig.anchor("scene-done"));
            assert!(
                back.right() < delete.left() && delete.right() < done.left(),
                "{width}: кнопки наехали друг на друга"
            );
            // Имя между «‹ Сцены» и «Удалить сцену»: ничто не лезет за правый край и на кнопку.
            assert!(done.right() <= width, "{width}: «Готово» за краем: {done:?}");
            let title = rig.anchor("scene-title");
            assert!(
                title.left() >= back.right() && title.right() <= delete.left(),
                "{width} «{name}»: имя заходит на кнопки: {title:?} при {delete:?}"
            );
        }
    }

    #[test]
    fn what_is_asked_of_the_system_follows_the_scenes() {
        let mut rig = Rig::new();
        rig.app.set_shelf(2);
        let keys = |rig: &Rig| -> Vec<&'static str> {
            rig.app.refresh_set().into_iter().map(|i| rig.app.modules[i].key()).collect()
        };
        // Сцен нет: открытая полка и «Экран» (его читают чипы в шапке).
        assert!(
            !keys(&rig).contains(&"quick") && !keys(&rig).contains(&"brightness") && keys(&rig).contains(&"display")
        );
        // Сцена с «Тишиной» — быстрые настройки; с яркостью или ночным светом — «Яркость»; где бы ни лежали карточки.
        rig.app.settings.profiles = vec![only("A", config::SceneSystem { quiet: Some(true), ..Default::default() })];
        assert!(keys(&rig).contains(&"quick") && !keys(&rig).contains(&"brightness"));
        rig.app.settings.profiles = vec![only("B", config::SceneSystem { night: Some(true), ..Default::default() })];
        assert!(!keys(&rig).contains(&"quick") && keys(&rig).contains(&"brightness"));
        // «Не спать» — своё состояние «Быстрых настроек», опрашивать нечего.
        rig.app.settings.profiles = vec![only("C", config::SceneSystem { awake: Some(true), ..Default::default() })];
        assert!(!keys(&rig).contains(&"quick") && !keys(&rig).contains(&"brightness"));
        // Правка сцены читает всё.
        rig.app.settings.profiles = vec![movie("X")];
        rig.app.open_scene(0, false, View::Cards);
        let k = keys(&rig);
        assert!(k.contains(&"display") && k.contains(&"quick") && k.contains(&"brightness"), "{k:?}");
        // «Экран» убран — чипов нет, но правка сцены всё равно его читает (строка «Экран»).
        rig.app.settings.hide_card("display");
        assert!(keys(&rig).contains(&"display"));
        rig.app.finish_scene();
        assert!(!keys(&rig).contains(&"display"));
    }

    #[test]
    fn the_settings_card_opens_the_editor_and_deletes_scenes() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-SETTINGS";
        let mut rig = scene_rig(
            dev,
            vec![movie(dev), only("Work", config::SceneSystem { dark: Some(false), ..Default::default() })],
        );
        rig.app.view = View::Settings;
        rig.settle();
        // «Изменить…» — правка, назад — в настройки, а не к карточкам.
        rig.click_anchor("scene-open-1");
        assert!(rig.app.view == View::Scene && rig.app.scene.as_ref().unwrap().index == 1);
        rig.click_anchor("scene-done");
        assert!(rig.app.view == View::Settings && rig.app.scene.is_none());
        rig.click_anchor("scene-open-0");
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.view == View::Settings);
        // Корзина спрашивает на месте строки — тем же вопросом, что правка сцены; «Отмена» и Esc сцену оставляют.
        let names = |rig: &Rig| rig.app.settings.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        rig.click_anchor("scene-trash-0");
        assert_eq!(names(&rig), ["Movie", "Work"], "от щелчка по корзине сцена не пропадает");
        assert!(rig.app.ask_delete.is_some());
        rig.anchor("scene-delete-yes");
        rig.click_anchor("scene-delete-no");
        assert!(rig.app.ask_delete.is_none() && names(&rig).len() == 2);
        rig.app.edge.reveal(Instant::now());
        rig.click_anchor("scene-trash-0");
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(rig.app.ask_delete.is_none() && names(&rig).len() == 2, "Esc снял вопрос");
        assert!(rig.app.view == View::Settings && rig.app.edge.shown, "а панель не спряталась");
        rig.key(Key::Escape, Modifiers::NONE);
        assert!(!rig.app.edge.shown, "следующий Esc прячет её");
        // Ушли из настроек с вопросом — он снят.
        rig.click_anchor("scene-trash-1");
        assert!(rig.app.ask_delete.is_some());
        rig.app.view = View::Cards;
        rig.settle();
        rig.app.view = View::Settings;
        rig.settle();
        assert!(rig.app.ask_delete.is_none() && names(&rig).len() == 2);
        // «Удалить» убирает сцену, и вопрос уходит вместе с ней.
        rig.click_anchor("scene-trash-0");
        rig.click_anchor("scene-delete-yes");
        assert_eq!(names(&rig), ["Work"]);
        assert!(rig.app.ask_delete.is_none());
        rig.click_anchor("scene-trash-0");
        rig.click_anchor("scene-delete-yes");
        assert!(rig.app.settings.profiles.is_empty());
        rig.settle();
    }

    /// Вопрос «Удалить сцену?» в настройках виден целиком и на самой узкой панели, и с самым длинным именем.
    #[test]
    fn the_settings_question_fits_the_narrowest_card() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-SETTINGS-NARROW";
        for width in [560.0, 930.0, 1300.0] {
            let mut rig = Rig::sized(width);
            let mut scene = movie(dev);
            scene.name = "W".repeat(config::MAX_SCENE_NAME);
            rig.app.settings.profiles = vec![scene];
            rig.app.view = View::Settings;
            // Корзина на узкой панели в листаемой части: вопрос просим сами.
            rig.app.ask_delete = Some(rig.app.settings.profiles[0].id);
            rig.settle();
            let visible = Rect::from_min_max(Pos2::new(12.0, 0.0), Pos2::new(width - 12.0, SIZE.y));
            let (yes, no) = (rig.anchor("scene-delete-yes"), rig.anchor("scene-delete-no"));
            for (key, button) in [("Delete", yes), ("Cancel", no)] {
                assert!(visible.contains_rect(button), "{width}: «{key}» за краем: {button:?}");
            }
            assert!(no.right() <= yes.left() || no.bottom() <= yes.top(), "{width}: кнопки наехали друг на друга");
        }
    }

    // ─── Сцены: отказ от режима, возврат при чужом вопросе, номер сцены, слова об ошибках ───

    /// Отказ от режима, что поставила сцена («Вернуть»), — отказ от сцены целиком: возвращается и экран, и «Тишина»; это так
    /// и у сцены без «Повторный щелчок — вернуть как было».
    #[test]
    fn declining_the_mode_returns_the_whole_scene() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-REFUSE";
        let mut one_way = movie(dev);
        (one_way.name, one_way.undo) = ("Fixed".into(), false);
        let mut rig = scene_rig(dev, vec![movie(dev), one_way]);
        let t = texts::tr(rig.app.lang());
        let start = sys::display::current(dev).unwrap();
        for chip in 0..2 {
            rig.click_anchor(&format!("scene-chip-{chip}"));
            assert!(rig.app.modules.iter().any(|m| m.busy()), "экран ждёт «Оставить?»");
            assert_eq!(sys::system::notifications_off(), Some(true));
            let before = Instant::now();
            rig.click_anchor("keep-revert");
            rig.app.modules_background();
            assert!(!rig.app.modules.iter().any(|m| m.busy()));
            assert_eq!((sys::display::current(dev), sys::system::notifications_off()), (Some(start), Some(false)));
            assert_eq!(rig.app.active_scene(), None, "чип не выделен: сцены нет");
            assert_eq!(rig.app.toasts.since(before), vec![(t.scene_undone.to_string(), Tone::Neutral)]);
            let scene = rig.app.settings.profiles[chip].clone();
            assert!(!rig.app.modules.iter().any(|m| m.scene_can_undo(&scene)), "возвращать больше нечего");
        }
    }

    /// «Оставить» — сцена стоит, второй щелчок возвращает её; отказа не было, ничего само не возвращается.
    #[test]
    fn keeping_the_mode_keeps_the_scene() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-KEEP";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        let start = sys::display::current(dev).unwrap();
        rig.click_anchor("scene-chip-0");
        rig.click_anchor("keep-yes");
        let before = Instant::now();
        rig.app.modules_background();
        assert_eq!((rig.app.active_scene(), sys::system::notifications_off()), (Some(0), Some(true)));
        assert!(rig.app.toasts.since(before).is_empty());
        rig.click_anchor("scene-chip-0");
        assert_eq!((sys::display::current(dev), sys::system::notifications_off()), (Some(start), Some(false)));
    }

    /// Закрыли программу, не ответив на «Оставить?», — то же, что «Вернуть»: возвращается вся сцена.
    #[test]
    fn closing_the_program_without_an_answer_returns_the_whole_scene() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-CLOSE";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        let start = sys::display::current(dev).unwrap();
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(true));
        eframe::App::on_exit(&mut rig.app);
        assert_eq!((sys::display::current(dev), sys::system::notifications_off()), (Some(start), Some(false)));
    }

    /// Заглушка: модуль, что ждёт чужого ответа, пока флаг поднят.
    struct Waiting(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl Module for Waiting {
        fn key(&self) -> &'static str {
            "waiting"
        }

        fn title(&self, _t: &texts::Tr) -> &'static str {
            "Waiting"
        }

        fn refresh(&mut self, _monitor: &str) {}

        fn ui(&mut self, _ui: &mut Ui, _cx: &mut Ctx) {}

        fn blocks_undo(&self, _scene: &Profile) -> bool {
            self.0.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    /// Висит чужое «Оставить?»: возврат сцены не начинается ни у кого (иначе она вернулась бы наполовину) и не говорит
    /// «вернул»; ответили — возвращается.
    #[test]
    fn a_foreign_question_stops_the_return_before_it_starts() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-FOREIGN-QUESTION";
        let waits = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let quiet = only("Quiet", config::SceneSystem { quiet: Some(true), ..Default::default() });
        let mut rig = scene_rig(dev, vec![quiet]);
        rig.app.modules.push(Box::new(Waiting(waits.clone())));
        let t = texts::tr(rig.app.lang());
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(true));
        waits.store(true, std::sync::atomic::Ordering::Relaxed);
        let before = Instant::now();
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(true), "ничего не вернулось");
        assert_eq!(rig.app.toasts.since(before), vec![(t.answer_first.to_string(), Tone::Warning)]);
        let scene = rig.app.settings.profiles[0].clone();
        assert!(rig.app.modules.iter().any(|m| m.scene_can_undo(&scene)), "возврат остался");
        waits.store(false, std::sync::atomic::Ordering::Relaxed);
        let before = Instant::now();
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(false));
        assert_eq!(rig.app.toasts.since(before), vec![(t.scene_undone.to_string(), Tone::Neutral)]);
    }

    /// Сцену узнают по номеру, не по имени: переименование возврат не теряет, а новая сцена со старым именем чужого не
    /// наследует.
    #[test]
    fn a_renamed_scene_keeps_its_return_and_a_new_one_with_the_old_name_has_none() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-IDENTITY";
        let quiet = |name: &str| only(name, config::SceneSystem { quiet: Some(true), ..Default::default() });
        let mut rig = scene_rig(dev, vec![quiet("Quiet")]);
        let t = texts::tr(rig.app.lang());
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(true));
        // Переименовали в правке — возврат на месте.
        rig.open_editor(0);
        rig.click(rig.rect(egui::Id::new("scene-name")).center());
        rig.key(Key::A, CTRL);
        rig.type_text("Hush");
        rig.click_anchor("scene-done");
        assert_eq!(rig.app.settings.profiles[0].name, "Hush");
        assert_eq!(rig.app.active_scene(), Some(0));
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(false), "вернулось, хотя имя другое");
        // Поставили, удалили, завели новую с тем же именем: её возврат — не чужой.
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(true));
        rig.app.delete_scene(0);
        rig.app.settings.profiles.push(quiet("Hush"));
        rig.settle();
        let fresh = rig.app.settings.profiles[0].clone();
        assert!(!rig.app.modules.iter().any(|m| m.scene_can_undo(&fresh)), "у новой сцены чужого возврата нет");
        let before = Instant::now();
        rig.click_anchor("scene-chip-0");
        assert_eq!(sys::system::notifications_off(), Some(true), "«Тишину» прежней сцены новая не гасит");
        assert_eq!(rig.app.toasts.since(before), vec![(t.scene_nothing_undo.to_string(), Tone::Neutral)]);
    }

    /// Что не вышло в потоках модулей (Wi‑Fi, DDC/CI), панель говорит словами программы и когда карточек этих модулей на
    /// экране нет.
    #[test]
    fn what_fails_in_a_module_thread_is_told_even_when_its_card_is_not_shown() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-THREADS";
        let system = config::SceneSystem { wifi: Some(false), bright: Some(30), ..Default::default() };
        let mut rig = scene_rig(dev, vec![only("Trip", system)]);
        let t = texts::tr(rig.app.lang());
        // Открыта третья полка («День»): ни «Быстрых настроек», ни «Яркости» на ней нет.
        rig.app.set_shelf(2);
        rig.settle();
        assert!(rig.app.shelf_modules(2).iter().all(|&m| !["quick", "brightness"].contains(&rig.app.modules[m].key())));
        sys::net::FAKE_WIFI_FAILS.store(true, std::sync::atomic::Ordering::Relaxed);
        sys::ddc::fake_silence(dev, u32::MAX);
        let before = Instant::now();
        rig.click_anchor("scene-chip-0");
        let start = Instant::now();
        while rig.app.toasts.since(before).len() < 3 {
            rig.app.modules_background();
            assert!(start.elapsed() < Duration::from_secs(5), "не дождались слов: {:?}", rig.app.toasts.since(before));
            std::thread::sleep(Duration::from_millis(20));
        }
        let told = rig.app.toasts.since(before);
        assert!(told.contains(&(format!("{}: {}", t.failed, t.wifi), Tone::Danger)), "{told:?}");
        assert!(told.contains(&(t.scene_bright_failed.to_string(), Tone::Warning)), "{told:?}");
        // Английского служебного («radio access: 1») в окне нет.
        assert!(told.iter().all(|(text, _)| !text.contains("radio access")), "служебное в окне: {told:?}");
        sys::net::FAKE_WIFI_FAILS.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Закрыли программу в правке сцены, с которой сняли всё, — она остаётся такой, какой вошли (иначе из файла пропала
    /// бы вместе с клавишей); новая без годного имени уходит.
    #[test]
    fn closing_the_program_ends_the_editor_like_done() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-EXIT";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        rig.open_editor(0);
        for row in SceneRow::ALL {
            rig.app.settings.profiles[0].clear(row);
        }
        eframe::App::on_exit(&mut rig.app);
        assert!(rig.app.scene.is_none());
        assert_eq!(rig.app.settings.profiles[0], movie(dev));
        rig.settle();
        rig.click_anchor("scene-new");
        let i = rig.app.scene.as_ref().unwrap().index;
        rig.app.settings.profiles[i].clear(SceneRow::Screen);
        eframe::App::on_exit(&mut rig.app);
        assert_eq!(rig.app.settings.profiles.len(), 1, "пустая новая сцена ушла");
    }

    /// У недоступной строки сцены в подсказке причина, а не снова «Недоступно».
    #[test]
    fn an_unavailable_row_says_why() {
        let mut rig = Rig::new();
        for lang in [Lang::Ru, Lang::En] {
            rig.app.settings.common.language = lang;
            let t = texts::tr(lang);
            assert_eq!(rig.app.scene_why(SceneRow::Bright), t.ddc_none);
            assert_eq!(rig.app.scene_why(SceneRow::Wifi), t.scene_why_wifi);
            for row in SceneRow::ALL {
                let why = rig.app.scene_why(row);
                assert!(!why.is_empty() && why != t.unavailable, "{row:?}: {why}");
            }
        }
    }

    /// Негодное имя: поле в красной рамке, под ним строка.
    #[test]
    fn a_bad_name_gets_a_red_frame() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-NAME-FRAME";
        let work = only("Work", config::SceneSystem { dark: Some(false), ..Default::default() });
        let mut rig = scene_rig(dev, vec![movie(dev), work]);
        let danger = Palette::of_ctx(&rig.ctx).danger;
        rig.open_editor(0);
        assert!(rig.outlines(danger).is_empty(), "имя годное — красного нет");
        rig.click(rig.rect(egui::Id::new("scene-name")).center());
        rig.key(Key::A, CTRL);
        rig.type_text("Work");
        let field = rig.rect(egui::Id::new("scene-name"));
        assert!(rig.outlines(danger).contains(&field), "рамка поля красная: {:?} у {field:?}", rig.outlines(danger));
        rig.type_text("2");
        assert!(rig.outlines(danger).is_empty(), "имя исправили — красная рамка ушла");
    }

    #[test]
    fn a_hidden_panel_ends_the_editor_and_drops_a_bad_scene() {
        let _world = world();
        let dev = r"\\.\TEST-SCENE-HIDE";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        rig.click_anchor("scene-new");
        let i = rig.app.scene.as_ref().unwrap().index;
        rig.app.settings.profiles[i].clear(SceneRow::Screen);
        // Панель спряталась (`tick` зовёт то же): правка кончена, пустая новая сцена убрана.
        rig.app.finish_scene();
        rig.app.view = View::Cards;
        rig.settle();
        assert!(rig.app.scene.is_none() && rig.app.settings.profiles.len() == 1);
        // Прежняя сцена, из которой сняли всё, возвращается такой, какой вошли.
        rig.open_editor(0);
        for row in SceneRow::ALL {
            rig.app.settings.profiles[0].clear(row);
        }
        rig.app.finish_scene();
        assert_eq!(rig.app.settings.profiles[0], movie(dev));
    }

    #[test]
    fn a_checkbox_row_clicks_only_when_it_can() {
        let ctx = egui::Context::default();
        anvil_ui::install(&ctx, look::LIME, ThemeChoice::Dark);
        // Время кадров идёт только вперёд — на оба прогона один счётчик.
        let frame = std::cell::Cell::new(0u32);
        let run = |usable: bool, click: bool| {
            let mut clicked = false;
            for step in 0..4 {
                let (pos, pressed) = match (click, step) {
                    (true, 2) => (Pos2::new(40.0, 15.0), Some(true)),
                    (true, 3) => (Pos2::new(40.0, 15.0), Some(false)),
                    _ => (Pos2::new(40.0, 15.0), None),
                };
                let mut events = vec![Event::PointerMoved(pos)];
                events.extend(pressed.map(|p| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: p,
                    modifiers: Modifiers::NONE,
                }));
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 100.0))),
                    time: Some(frame.get() as f64 / 30.0),
                    events,
                    ..Default::default()
                };
                frame.set(frame.get() + 1);
                let mut out = ctx.run_ui(input, |ui| {
                    let r = scene_row(ui, egui::Id::new("row"), "Label", "value", "", false, usable);
                    clicked |= r.clicked();
                });
                out.textures_delta.clear();
            }
            clicked
        };
        assert!(run(true, true), "доступную строку щелчок нажимает");
        assert!(!run(false, true), "недоступную — нет");
    }

    // ─── Поиск действий ─────────────────────────────────────────────────────
    //
    // Панель без окна в выдуманном мире: поиск открывается с клавиатуры, действия делают то же, что щелчок в карточке.
    // Язык кадров задаёт `search_rig`; тесты, что смотрят на русские слова, просят `Lang::Ru`.

    const NO_KEYS: Modifiers = Modifiers::NONE;

    /// Панель с выдуманным монитором: открыта, карточки опрошены, как при показе; поиск закрыт.
    fn search_rig(dev: &str, lang: Lang) -> Rig {
        let mut rig = scene_rig(dev, Vec::new());
        rig.app.settings.common.language = lang;
        rig.app.edge.shown = true;
        // Остальные карточки (часы, буфер, таймеры…) опрашивать нечем — они читают настройки.
        rig.settle();
        rig
    }

    /// Весь текст, что нарисован в последнем кадре.
    fn painted(rig: &Rig) -> String {
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
        rig.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    }

    fn titles(rig: &Rig) -> Vec<String> {
        rig.app.found().hits.into_iter().map(|h| h.title).collect()
    }

    /// Заглушка с недоступным и доступным действием: считает, сколько раз ей велели выполнить.
    struct Stuck(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl Module for Stuck {
        fn key(&self) -> &'static str {
            "stuck"
        }

        fn title(&self, _t: &texts::Tr) -> &'static str {
            "Stuck"
        }

        fn refresh(&mut self, _monitor: &str) {}

        fn ui(&mut self, _ui: &mut Ui, _cx: &mut Ctx) {}

        fn actions(&self, _t: &texts::Tr, _lang: Lang, _settings: &Settings) -> Vec<modules::Action> {
            vec![
                modules::Action::new("stuck:closed", "Stuck: closed", "Stuck", Icon::Gear)
                    .words("stuck")
                    .off("Not now"),
                modules::Action::new("stuck:open", "Stuck: open", "Stuck", Icon::Gear).words("stuck"),
            ]
        }

        fn command(&mut self, cmd: &Command, _cx: &mut Ctx) {
            if let Command::RunAction(id) = cmd
                && modules::mine("stuck", id).is_some()
            {
                self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn ctrl_k_and_ctrl_f_open_the_search_and_the_second_esc_hides_the_panel() {
        for key in [Key::K, Key::F] {
            let mut rig = Rig::new();
            rig.app.edge.shown = true;
            rig.key(key, CTRL);
            assert!(rig.app.view == View::Search, "{key:?}");
            assert!(rig.app.search.query.is_empty());
            // Поле в фокусе: можно печатать сразу.
            assert_eq!(rig.ctx.memory(|m| m.focused()), Some(egui::Id::new("search-field")));
            // Esc — назад к карточкам, а не прятать панель; второй прячет, как раньше.
            rig.key(Key::Escape, NO_KEYS);
            assert!(rig.app.view == View::Cards && rig.app.edge.shown && rig.app.search.query.is_empty());
            rig.key(Key::Escape, NO_KEYS);
            assert!(!rig.app.edge.shown);
        }
    }

    #[test]
    fn a_letter_or_a_digit_opens_the_search_with_it_and_the_rest_goes_into_the_field() {
        let mut rig = Rig::new();
        rig.app.edge.shown = true;
        rig.type_text("н");
        assert!(rig.app.view == View::Search);
        assert_eq!(rig.app.search.query, "н", "буква не потеряна и не задвоилась");
        rig.type_text("очн");
        assert_eq!(rig.app.search.query, "ночн");
        // Цифра тоже; быстрый набор — несколько букв за кадр — приходит целиком.
        let mut rig = Rig::new();
        rig.type_text("10");
        assert_eq!(rig.app.search.query, "10");
        let mut rig = Rig::new();
        rig.type_text("eng");
        rig.type_text("lish");
        assert_eq!(rig.app.search.query, "english");
    }

    #[test]
    fn typing_does_not_open_the_search_where_it_must_not() {
        // Пробел и знаки — не буква и не цифра.
        let mut rig = Rig::new();
        for text in [" ", "+", "-", "("] {
            rig.type_text(text);
            assert!(rig.app.view == View::Cards, "«{text}» открыло поиск");
        }
        // В правке полок поиск не открывается — ни буквой, ни Ctrl+K.
        let mut rig = Rig::editing();
        rig.type_text("н");
        rig.key(Key::K, CTRL);
        assert!(rig.app.view == View::Cards && rig.app.editing.is_some());
        // Пока вводят имя полки или ждут сочетание — тоже.
        let mut rig = Rig::new();
        rig.app.renaming = Some(Rename { shelf: 0, text: String::new(), fresh: false });
        rig.type_text("н");
        assert!(rig.app.view == View::Cards);
        rig.app.renaming = None;
        rig.app.capture = Some(0);
        rig.type_text("н");
        rig.app.capture = None;
        rig.app.capture_panel = true;
        rig.type_text("н");
        assert!(rig.app.view == View::Cards);
        // Пока в карточке вводят текст (поиск пояса у «Часов»), буквы — туда.
        let mut rig = Rig::new();
        rig.app.set_shelf(1);
        rig.app.run_action("clocks:add");
        rig.settle();
        assert!(rig.app.typing_here());
        rig.type_text("н");
        assert!(rig.app.view == View::Cards, "печатают в поле пояса, а не в поиск");
        // В правке сцены и в настройках — тоже нет.
        let _world = world();
        let dev = r"\\.\TEST-SEARCH-NOT-HERE";
        let mut rig = scene_rig(dev, vec![movie(dev)]);
        rig.open_editor(0);
        rig.key(Key::K, CTRL);
        assert!(rig.app.view == View::Scene);
        rig.app.finish_scene();
        rig.app.view = View::Settings;
        rig.settle();
        rig.key(Key::K, CTRL);
        rig.type_text("н");
        assert!(rig.app.view == View::Settings);
    }

    /// Сценарий «Ночной свет с клавиатуры»: «ночн» и Enter — включён, вид вернулся к карточкам.
    #[test]
    fn night_light_from_the_keyboard() {
        let _world = world();
        let dev = r"\\.\TEST-SEARCH-NIGHT";
        let mut rig = search_rig(dev, Lang::Ru);
        assert_eq!(sys::nightlight::state(), Some(false));
        rig.key(Key::K, CTRL);
        rig.type_text("ночн");
        let found = rig.app.found();
        assert_eq!(found.hits[0].title, "Ночной свет: включить");
        assert_eq!(found.hits[0].marks, vec![(0, 4)], "совпадение выделено");
        assert_eq!(found.hits[0].sub, "Яркость · Экран и звук", "где это: карточка и полка");
        assert!(rig.app.view == View::Search);
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(sys::nightlight::state(), Some(true));
        assert!(rig.app.view == View::Cards && rig.app.search.query.is_empty());
        // Недавнее: пустой запрос показывает это действие, уже «выключить».
        rig.key(Key::K, CTRL);
        let found = rig.app.found();
        assert_eq!(found.listing, search_ui::Listing::Recent);
        assert_eq!(found.hits[0].title, "Ночной свет: выключить");
    }

    /// Сценарий «Режим экрана»: «1080», режим из списка — с «Оставить этот режим?», как щелчком в карточке «Экран».
    #[test]
    fn a_display_mode_from_the_keyboard_asks_keep() {
        let _world = world();
        let dev = r"\\.\TEST-SEARCH-APP-MODE";
        let mut rig = search_rig(dev, Lang::En);
        let start = sys::display::current(dev).unwrap();
        rig.key(Key::K, CTRL);
        rig.type_text("1080");
        let list = titles(&rig);
        assert_eq!(
            list,
            ["Display: 1920 × 1080 · 144 Hz", "Display: 1920 × 1080 · 120 Hz", "Display: 1920 × 1080 · 60 Hz"]
        );
        // Одна ↓ — второй режим; Enter ставит его.
        rig.key(Key::ArrowDown, NO_KEYS);
        assert_eq!(rig.app.search.selected, 1);
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(sys::display::current(dev).map(|m| (m.width, m.height, m.hz)), Some((1920, 1080, 120)));
        assert_ne!(sys::display::current(dev), Some(start));
        assert_eq!(sys::display::saved(dev), start, "в реестр — только после «Оставить»");
        assert!(rig.app.modules.iter().any(|m| m.busy()), "ждёт ответа «Оставить этот режим?»");
        assert!(rig.app.view == View::Cards);
        // Пока вопрос висит, в поиске режимы серые, с причиной, и Enter их не выполняет.
        rig.key(Key::K, CTRL);
        rig.type_text("1080");
        let found = rig.app.found();
        let t = texts::tr(Lang::En);
        assert!(found.hits.iter().all(|h| !h.enabled && h.sub == t.answer_first), "{:?}", titles(&rig));
        let now = sys::display::current(dev);
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(sys::display::current(dev), now);
        assert!(rig.app.view == View::Search, "недоступное не выполняется и поиск не закрывает");
    }

    #[test]
    fn the_calculator_row_is_first_and_enter_copies_the_result() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-CALC", Lang::Ru);
        rig.key(Key::K, CTRL);
        rig.type_text("21*3,5");
        let found = rig.app.found();
        assert_eq!(found.hits[0].title, "21*3,5 = 73,5");
        assert_eq!(found.hits[0].marks, vec![(9, 13)], "результат выделен");
        assert_eq!(found.hits[0].sub, texts::tr(Lang::Ru).search_calc);
        assert!(painted(&rig).contains("21*3,5 = 73,5"));
        let before = Instant::now();
        rig.key(Key::Enter, NO_KEYS);
        let written = sys::BLOCKED.lock().unwrap().iter().any(|w| w == "clip::write_text 73,5");
        assert!(written, "в буфер ушёл результат, как он написан");
        assert_eq!(rig.app.toasts.since(before), vec![(texts::tr(Lang::Ru).clipboard_done.to_string(), Tone::Success)]);
        assert!(rig.app.view == View::Cards);
        // По-английски — с точкой.
        rig.app.settings.common.language = Lang::En;
        rig.key(Key::K, CTRL);
        rig.type_text("21*3.5");
        assert_eq!(rig.app.found().hits[0].title, "21*3.5 = 73.5");
        // Деление на ноль и переполнение — строки нет.
        for bad in ["1/0", "99999999999999*99999999999999"] {
            rig.app.search.query = bad.to_string();
            assert!(!titles(&rig).iter().any(|t| t.contains(" = ")), "{bad}");
        }
    }

    #[test]
    fn a_word_and_a_number_start_a_named_timer() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-TIMER", Lang::Ru);
        rig.key(Key::K, CTRL);
        rig.type_text("чай 5");
        let found = rig.app.found();
        assert_eq!(found.hits[0].title, "Таймер «чай» на 5 мин");
        assert_eq!(found.hits[0].sub, "Таймеры · День");
        rig.key(Key::Enter, NO_KEYS);
        assert!(rig.app.view == View::Cards);
        let (t, lang) = (texts::tr(Lang::Ru), Lang::Ru);
        let timers = rig.app.module_index("timers").unwrap();
        let (_, hint) = rig.app.modules[timers].badge(t, lang).expect("таймер идёт");
        assert!(hint.starts_with("Идёт таймер «чай», осталось 0"), "{hint}");
        // Таймер из запроса в недавние не идёт: он каждый раз свой.
        assert!(rig.app.recent.is_empty());
        // «5 мин» — без имени, как быстрые таймеры.
        rig.key(Key::K, CTRL);
        rig.type_text("7 мин");
        assert_eq!(rig.app.found().hits[0].title, "Таймер на 7 мин");
    }

    /// «Яркость 50» — это яркость, а не таймер «яркость»: таймер встаёт следом за лучшим совпадением.
    #[test]
    fn a_timer_row_does_not_push_a_real_match_out_of_first_place() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-TIMER-ORDER", Lang::Ru);
        rig.key(Key::K, CTRL);
        rig.type_text("яркость 50");
        let list = titles(&rig);
        assert_eq!(list[..2], ["Яркость: 50 %", "Таймер «яркость» на 50 мин"]);
    }

    #[test]
    fn an_unavailable_action_is_grey_with_its_reason_and_enter_does_not_run_it() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-GREY", Lang::En);
        let runs = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        rig.app.modules.push(Box::new(Stuck(runs.clone())));
        rig.app.settings.shelves[0].cards.push("stuck".into());
        rig.settle();
        rig.key(Key::K, CTRL);
        rig.type_text("stuck");
        let found = rig.app.found();
        assert_eq!(found.hits.iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), ["Stuck: closed", "Stuck: open"]);
        assert!(!found.hits[0].enabled && found.hits[0].sub == "Not now", "серая — причина во второй строке");
        assert!(found.hits[1].enabled && found.hits[1].sub == "Stuck · Display & sound");
        let shown = painted(&rig);
        assert!(shown.contains("Not now") && shown.contains("Stuck · Display & sound"), "{shown}");
        // Enter по серой — ничего, поиск остаётся; щелчок по ней — тоже.
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(runs.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert!(rig.app.view == View::Search);
        let first = rig.anchor("search-row-0").center();
        rig.click(first);
        assert_eq!(runs.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert!(rig.app.view == View::Search);
        // Следующая — доступная: Enter выполняет.
        rig.key(Key::ArrowDown, NO_KEYS);
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(runs.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(rig.app.view == View::Cards);
    }

    #[test]
    fn a_click_on_a_row_runs_it_and_the_arrows_stop_at_the_ends() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-CLICK", Lang::En);
        rig.key(Key::K, CTRL);
        rig.type_text("turn");
        let list = titles(&rig);
        let quiet = list.iter().position(|t| t == "Quiet: turn on").expect("«Тишина» найдена");
        let count = list.len();
        assert!(count > 3, "{list:?}");
        // ↑ на первой и ↓ на последней по кругу не ходят.
        rig.key(Key::ArrowUp, NO_KEYS);
        assert_eq!(rig.app.search.selected, 0);
        for _ in 0..count + 3 {
            rig.key(Key::ArrowDown, NO_KEYS);
        }
        assert_eq!(rig.app.search.selected, count - 1);
        // Щелчок по строке «Тишина» включает её, а не выбранную стрелками.
        assert_eq!(sys::system::notifications_off(), Some(false));
        let row = rig.anchor(&format!("search-row-{quiet}")).center();
        rig.click(row);
        assert_eq!(sys::system::notifications_off(), Some(true));
        assert!(rig.app.view == View::Cards);
    }

    #[test]
    fn scenes_are_found_set_and_returned_from_the_search() {
        let _world = world();
        let dev = r"\\.\TEST-SEARCH-SCENE";
        let mut scene = movie(dev);
        scene.hotkey = Hotkey::parse("Ctrl+Alt+3");
        let mut rig = scene_rig(dev, vec![scene]);
        rig.app.settings.common.language = Lang::En;
        rig.app.edge.shown = true;
        let start = sys::display::current(dev).unwrap();
        rig.key(Key::K, CTRL);
        rig.type_text("movie");
        let found = rig.app.found();
        assert_eq!(found.hits[0].title, "Scene “Movie”: 2560 × 1440 · 144 Hz");
        assert_eq!(found.hits[0].sub, "Scenes · Ctrl+Alt+3");
        // Найти можно и по режиму, и по тому, что сцена меняет.
        for q in ["1440", "quiet", "тишина"] {
            rig.app.search.query = q.to_string();
            assert!(titles(&rig).iter().any(|t| t.starts_with("Scene “Movie”")), "{q}");
        }
        rig.app.search.query = "movie".into();
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(rig.app.active_scene(), Some(0), "сцена стоит, как после щелчка по чипу");
        assert_eq!(sys::system::notifications_off(), Some(true));
        // Стоящая сцена — «вернуть как было»: тот же второй щелчок.
        rig.key(Key::K, CTRL);
        rig.type_text("movie");
        assert_eq!(rig.app.found().hits[0].title, "Scene “Movie”: undo");
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(sys::display::current(dev), Some(start));
        assert_eq!(sys::system::notifications_off(), Some(false));
        assert_eq!(rig.app.active_scene(), None);
    }

    #[test]
    fn the_panels_own_actions_do_what_their_buttons_do() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-OWN", Lang::En);
        // «Настройки».
        rig.key(Key::K, CTRL);
        rig.type_text("settings");
        assert_eq!(titles(&rig)[0], "Settings");
        rig.key(Key::Enter, NO_KEYS);
        assert!(rig.app.view == View::Settings);
        // «Открыть полку».
        rig.app.view = View::Cards;
        rig.key(Key::K, CTRL);
        rig.type_text("shelf");
        assert_eq!(
            titles(&rig),
            ["Open the “System” shelf", "Open the “Today” shelf", "Open the “At hand” shelf"],
            "открытую полку не предлагаем"
        );
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(rig.app.shelf, 1);
        // «Изменить полки».
        rig.key(Key::K, CTRL);
        rig.type_text("edit shelves");
        assert_eq!(titles(&rig)[0], "Edit shelves");
        rig.key(Key::Enter, NO_KEYS);
        assert!(rig.app.editing.is_some());
        rig.app.finish_edit();
        // «Спрятать».
        rig.settle();
        rig.key(Key::K, CTRL);
        rig.type_text("hide");
        assert_eq!(titles(&rig)[0], "Hide the panel");
        rig.key(Key::Enter, NO_KEYS);
        assert!(!rig.app.edge.shown);
    }

    #[test]
    fn a_card_not_on_a_shelf_gives_no_actions_and_the_place_names_the_shelf() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-SHELVES", Lang::En);
        let place_of = |rig: &Rig, id: &str| rig.app.all_actions().into_iter().find(|a| a.id == id).map(|a| a.place);
        assert_eq!(place_of(&rig, "media:next").as_deref(), Some("Media · Display & sound"));
        assert_eq!(place_of(&rig, "timers:min:5").as_deref(), Some("Timers · Today"));
        assert_eq!(place_of(&rig, "bluetooth:settings").as_deref(), Some("Bluetooth · System"));
        // Убрали карточку с полок — её действий нет; вернули — есть.
        rig.app.settings.hide_card("media");
        assert!(place_of(&rig, "media:next").is_none());
        rig.app.settings.place_card("media", 3, None);
        assert_eq!(place_of(&rig, "media:next").as_deref(), Some("Media · At hand"));
        // Имя своей полки в месте — как на вкладке.
        rig.app.settings.rename_shelf(3, "Мой стол");
        assert_eq!(place_of(&rig, "media:next").as_deref(), Some("Media · Мой стол"));
    }

    #[test]
    fn the_card_name_finds_its_actions_but_the_shelf_name_does_not() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-CARD-NAME", Lang::Ru);
        rig.key(Key::K, CTRL);
        // «Яркость» — имя карточки: находятся и шаги яркости, и «Ночной свет», что лежит в этой карточке.
        rig.type_text("яркость");
        let list = titles(&rig);
        assert!(
            list.contains(&"Яркость: 25 %".to_string()) && list.contains(&"Ночной свет: включить".to_string()),
            "{list:?}"
        );
        // «Экран и звук» — полка, а не название действия: её имя действия не находит.
        rig.app.search.query = "звук".into();
        assert!(
            !titles(&rig).iter().any(|t| t.starts_with("Ночной свет")),
            "по полке находятся только «Открыть полку…»"
        );
    }

    #[test]
    fn the_picker_waits_for_a_free_pointer_and_the_clock_search_opens_its_shelf() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-POINTER", Lang::En);
        let t = texts::tr(Lang::En);
        let pick = |rig: &Rig| rig.app.all_actions().into_iter().find(|a| a.id == "tools:pick").unwrap();
        assert!(pick(&rig).enabled);
        rig.app.pointer_over = true;
        let grey = pick(&rig);
        assert!(!grey.enabled && grey.reason.as_deref() == Some(t.search_pointer));
        // «Добавить пояс» открывает поиск пояса в карточке — и полку, где она: «Часы» на «Дне».
        assert_eq!(rig.app.shelf, 0);
        rig.app.run_action("clocks:add");
        assert_eq!(rig.app.shelf, 2, "карточка «Часы» должна быть на виду");
        assert!(rig.app.typing_here());
    }

    #[test]
    fn recent_actions_are_shown_for_an_empty_query_and_win_a_tie() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-RECENT", Lang::En);
        rig.key(Key::K, CTRL);
        let found = rig.app.found();
        assert_eq!(
            (found.listing, found.hits.len()),
            (search_ui::Listing::Start, 0),
            "ни одного недавнего — подсказка"
        );
        assert!(painted(&rig).contains(texts::tr(Lang::En).search_start));
        rig.key(Key::Escape, NO_KEYS);
        // Два действия подряд — недавнее первым.
        for query in ["quiet", "dark"] {
            rig.key(Key::K, CTRL);
            rig.type_text(query);
            rig.key(Key::Enter, NO_KEYS);
        }
        rig.key(Key::K, CTRL);
        let found = rig.app.found();
        assert_eq!(found.listing, search_ui::Listing::Recent);
        let list: Vec<&str> = found.hits.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(list, ["Dark mode: turn on", "Quiet: turn off"]);
        assert!(painted(&rig).contains(&texts::tr(Lang::En).search_recent.to_uppercase()));
        // Равное совпадение — недавнее выше: «turn» есть у обоих и у «Не спать», но «Тёмная тема» была последней.
        rig.type_text("turn");
        let list = titles(&rig);
        assert_eq!(list[..2], ["Dark mode: turn on", "Quiet: turn off"], "{list:?}");
        // Повтор не дублируется, помнится не больше положенного.
        for _ in 0..3 {
            rig.app.remember_action("quick:dark");
        }
        assert_eq!(rig.app.recent.iter().filter(|r| *r == "quick:dark").count(), 1);
        for i in 0..40 {
            rig.app.remember_action(&format!("x:{i}"));
        }
        assert_eq!(rig.app.recent.len(), crate::search::KEEP_RECENT);
    }

    #[test]
    fn the_view_lists_at_most_seven_rows_counts_them_all_and_says_when_nothing_is_found() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-COUNT", Lang::En);
        rig.key(Key::K, CTRL);
        rig.type_text("display");
        let found = rig.app.found();
        assert_eq!(found.hits.len(), crate::search::SHOWN);
        assert!(found.total > crate::search::SHOWN);
        let shown = painted(&rig);
        assert!(shown.contains(&format!("RESULTS · {}", found.total)), "{shown}");
        // Выбранная строка называет клавишу.
        assert!(shown.lines().any(|l| l == "Enter"), "{shown}");
        // Ничего нет — так и сказано, число — ноль.
        rig.app.search.query = "zzzz-nothing".into();
        rig.settle();
        let shown = painted(&rig);
        assert!(shown.contains(texts::tr(Lang::En).search_none) && shown.contains("RESULTS · 0"), "{shown}");
        assert!(!shown.lines().any(|l| l == "Enter"), "строк нет — и клавиши нет");
    }

    #[test]
    fn the_examples_fill_the_query_and_the_buttons_clear_and_close() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-BUTTONS", Lang::En);
        rig.key(Key::K, CTRL);
        // Крестика «Стереть» пока нет: стирать нечего.
        assert!(rig.ctx.read_response(egui::Id::new(("anchor", "search-clear"))).is_none());
        // Пример из «Можно так» подставляет запрос; найденное — по нему.
        rig.click_anchor("search-tip-2");
        assert_eq!(rig.app.search.query, "tea 5");
        assert_eq!(titles(&rig)[0], "Timer “tea” for 5 min");
        assert_eq!(rig.ctx.memory(|m| m.focused()), Some(egui::Id::new("search-field")), "поле снова в фокусе");
        // «Стереть» очищает запрос и остаётся в поиске.
        rig.click_anchor("search-clear");
        assert!(rig.app.view == View::Search && rig.app.search.query.is_empty());
        // Крестик «Закрыть поиск» — назад к карточкам.
        rig.click_anchor("search-close");
        assert!(rig.app.view == View::Cards);
        // На узкой панели «Можно так» нет: результатам нужно всё место.
        let mut rig = Rig::sized(600.0);
        rig.key(Key::K, CTRL);
        assert!(rig.ctx.read_response(egui::Id::new(("anchor", "search-tip-0"))).is_none());
        // А строки есть, и фокус в поле.
        rig.type_text("night");
        assert!(rig.ctx.read_response(egui::Id::new(("anchor", "search-row-0"))).is_some());
    }

    #[test]
    fn the_header_has_the_field_wide_and_the_icon_narrow_and_a_click_opens_the_search() {
        for width in [560.0, 640.0, 700.0, 900.0, 1300.0] {
            let mut rig = Rig::sized(width);
            rig.app.settings.profiles = vec![Profile::new("Работа"), Profile::new("Игры"), Profile::new("Кино")];
            rig.settle();
            let button = rig.anchor("search-open");
            let (_, right) = rig.app.header_edges;
            assert!(button.left() >= right - 0.5, "{width}: поиск заходит на левую часть шапки: {button:?} / {right}");
            assert!(button.right() <= width - 12.0 + 0.5, "{width}: поиск за краем панели: {button:?}");
            let wide = button.width() > 100.0;
            assert_eq!(button.width(), if wide { search_ui::FIELD_W } else { 30.0 }, "{width}: поле или значок");
            // Три сцены и четыре кнопки справа занимают место: поле целиком — на 1300, а уже — только значок.
            assert_eq!(wide, width >= 1300.0, "{width}");
            for i in 0..3 {
                assert!(rig.tab(i).right() <= right - 8.0, "{width}: вкладка {i} наезжает на правую часть");
            }
            rig.click(button.center());
            assert!(rig.app.view == View::Search, "{width}: щелчок открывает поиск");
        }
    }

    #[test]
    fn the_search_header_replaces_the_tabs_and_scenes() {
        let mut rig = Rig::new();
        rig.app.settings.profiles = vec![Profile::new("Работа")];
        rig.key(Key::K, CTRL);
        let shown = painted(&rig);
        let t = texts::tr(rig.app.lang());
        assert!(shown.contains(t.search_keys) && shown.contains(t.search_field), "{shown}");
        assert!(!shown.contains("Работа") && !shown.contains(t.scene), "сцен в шапке поиска нет");
        assert!(rig.ctx.read_response(egui::Id::new(("shelf-tab", 0))).is_none(), "вкладок нет");
        assert!(shown.contains("Antresol"), "на широкой панели слово есть");
        // Узкая панель: подсказка (по-русски она длиннее) и слово «Antresol» уходят, поле и крестик остаются рядом.
        let mut narrow = Rig::sized(560.0);
        narrow.app.settings.common.language = Lang::Ru;
        narrow.key(Key::K, CTRL);
        let shown = painted(&narrow);
        assert!(!shown.contains(texts::tr(Lang::Ru).search_keys) && !shown.contains("Antresol"), "{shown}");
        let (field, close) = (narrow.rect(egui::Id::new("search-field")), narrow.anchor("search-close"));
        assert!(field.right() <= close.left() && close.right() <= 560.0 - 12.0 + 0.5, "{field:?} {close:?}");
        assert!(field.width() >= 150.0, "поле не схлопнулось: {field:?}");
    }

    // ─── Поиск действий: круг исправлений после судьи ───────────────────────

    /// Мониторы для шапки: `n` выдуманных, подряд слева направо.
    fn fake_monitors(n: usize) -> Vec<monitor::Monitor> {
        (0..n as i32)
            .map(|i| monitor::Monitor {
                device: format!(r"\\.\TEST-HEADER-{i}"),
                rect: monitor::PxRect { left: 1920 * i, top: 0, right: 1920 * (i + 1), bottom: 1080 },
                primary: i == 0,
                handle: 0,
            })
            .collect()
    }

    /// Шапка на всех ширинах от 1400 до 560: слово, подписи, поле, монитор и сцены уходят по одному порядку и обратно не
    /// возвращаются, нарисовано то, что решено, а правая часть занимает не больше отданного ей места.
    #[test]
    fn the_header_collapses_in_one_order_and_nothing_comes_back_while_it_narrows() {
        // (сцен, мониторов, правка полок)
        for (scenes, monitors, editing) in [(3, 0, false), (3, 3, false), (1, 2, false), (0, 0, false), (0, 0, true)] {
            let mut last: Option<(f32, header_plan::Plan)> = None;
            for width in (560..=1400).rev().step_by(10) {
                let width = width as f32;
                let mut rig = Rig::sized(width);
                rig.app.settings.profiles =
                    ["Работа", "Игры", "Кино"].into_iter().take(scenes).map(Profile::new).collect();
                rig.app.monitors = fake_monitors(monitors);
                if editing {
                    rig.app.start_edit();
                }
                rig.settle();
                let (plan, tag) =
                    (rig.app.header_plan, format!("{scenes} сцен, {monitors} мон., правка {editing}, {width}"));
                assert_eq!(painted(&rig).contains("Antresol"), plan.word, "{tag}: слово нарисовано не так, как решено");
                // Правая часть влезает в отданное ей место и не стоит заметно меньше, чем ей отдали.
                let used = width - 12.0 - rig.app.header_edges.1;
                assert!(used <= plan.right + 0.5, "{tag}: правая часть {used} шире плана {}", plan.right);
                assert!(plan.right - used <= 5.0, "{tag}: плану {} нужно больше, чем правой части {used}", plan.right);
                // Левая часть не наезжает на правую.
                assert!(
                    rig.app.header_edges.0 <= rig.app.header_edges.1 + 0.5,
                    "{tag}: части шапки наезжают друг на друга"
                );
                if let Some((wider, was)) = last {
                    for (name, before, now) in [
                        ("слово", was.word, plan.word),
                        ("подпись у «Полка»", was.add_label, plan.add_label),
                        ("подписи «Сцена» и «Монитор»", was.labels, plan.labels),
                        ("поле поиска", was.field, plan.field),
                        ("выбор монитора", was.monitors, plan.monitors),
                        ("сцены", was.scenes, plan.scenes),
                        ("подпись у «Как было»", was.undo_label, plan.undo_label),
                        ("подсказка правки", was.hint, plan.hint),
                    ] {
                        assert!(before || !now, "{tag}: {name} ушло(ла) на {wider} и вернулось(ась) на {width}");
                    }
                }
                last = Some((width, plan));
            }
        }
    }

    /// В поиске слово «Antresol» уходит первым, подсказка клавиш — потом; ни одно не возвращается при сужении.
    #[test]
    fn the_search_header_drops_the_word_then_the_hint_and_neither_comes_back() {
        for lang in [Lang::Ru, Lang::En] {
            let t = texts::tr(lang);
            let mut last: Option<(bool, bool)> = None;
            let mut seen = (false, false);
            for width in (560..=1400).rev().step_by(10) {
                let mut rig = Rig::sized(width as f32);
                rig.app.settings.common.language = lang;
                rig.key(Key::K, CTRL);
                let shown = painted(&rig);
                let (word, hint) = (shown.contains("Antresol"), shown.contains(t.search_keys));
                assert!(!word || hint, "{lang:?} {width}: слово есть, а подсказки уже нет");
                if let Some((was_word, was_hint)) = last {
                    assert!(was_word || !word, "{lang:?}: слово вернулось на {width}");
                    assert!(was_hint || !hint, "{lang:?}: подсказка вернулась на {width}");
                }
                last = Some((word, hint));
                seen = (seen.0 | word, seen.1 | hint);
            }
            assert!(seen.0 && seen.1, "{lang:?}: на широкой панели есть и слово, и подсказка");
            // На самой узкой панели слова нет; подсказка есть, лишь если она помещается (по-английски она короче).
            assert!(last.is_some_and(|(word, hint)| !word && (!hint || lang == Lang::En)), "{lang:?}: {last:?}");
        }
    }

    /// Поиск открыт — панель сама не прячется (ни когда мышь ушла, ни когда окно потеряло фокус): «Пипетке» из поиска нужна
    /// мышь вне панели. Закрывают его Esc, крестик, клавиша показа панели или выполненная строка.
    #[test]
    fn an_open_search_keeps_the_panel_from_hiding_itself() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-BUSY", Lang::En);
        assert!(!rig.app.panel_busy());
        let (show, hide) = (Duration::from_millis(150), Duration::from_millis(450));
        let now = Instant::now();
        let at = |ms: u64| now + Duration::from_millis(ms);
        // Панель выехала от края, мышь побывала на ней и ушла.
        let mut edge = Edge::default();
        edge.step(Input { at_edge: true, ..Input::default() }, now, show, hide);
        edge.step(Input { at_edge: true, ..Input::default() }, at(200), show, hide);
        edge.step(Input { over: true, focused: true, ..Input::default() }, at(300), show, hide);
        assert!(edge.shown);
        rig.key(Key::K, CTRL);
        assert!(rig.app.view == View::Search && rig.app.panel_busy());
        for (ms, lost_focus) in [(400, false), (900, true), (5_000, false), (60_000, true)] {
            let input = Input { busy: rig.app.panel_busy(), lost_focus, ..Input::default() };
            edge.step(input, at(ms), show, hide);
            assert!(edge.shown, "{ms} мс (фокус потерян: {lost_focus}): поиск открыт — панель не прячется сама");
        }
        // Закрыли поиск крестиком — панель снова прячется как обычно, когда мышь далеко.
        rig.click_anchor("search-close");
        assert!(rig.app.view == View::Cards && !rig.app.panel_busy());
        edge.step(Input::default(), at(61_000), show, hide);
        edge.step(Input::default(), at(61_600), show, hide);
        assert!(!edge.shown, "поиск закрыт — мыши нет, панель спряталась");

        // Вызвана клавишей: потеря фокуса её не прячет, пока поиск открыт; а клавиша показа панели прячет и с поиском.
        let mut edge = Edge::default();
        edge.step(Input { hotkey: true, ..Input::default() }, at(70_000), show, hide);
        rig.key(Key::K, CTRL);
        edge.step(Input { lost_focus: true, busy: rig.app.panel_busy(), ..Input::default() }, at(71_000), show, hide);
        assert!(edge.shown, "вызвана клавишей, поиск открыт, фокус потерян — остаётся");
        edge.step(Input { hotkey: true, busy: rig.app.panel_busy(), ..Input::default() }, at(72_000), show, hide);
        assert!(!edge.shown, "клавиша показа панели прячет и открытую с поиском");
        // Esc и выполненная строка тоже возвращают панели прежнее поведение.
        rig.key(Key::Escape, NO_KEYS);
        assert!(!rig.app.panel_busy());
        rig.key(Key::K, CTRL);
        rig.type_text("dark");
        rig.key(Key::Enter, NO_KEYS);
        assert!(rig.app.view == View::Cards && !rig.app.panel_busy());
    }

    /// Таймер из запроса заводит карточка «Таймеры»: убрана с полок — строки таймера нет (как у остальных её действий).
    #[test]
    fn the_timer_row_needs_the_timers_card_on_a_shelf() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-TIMER-HIDDEN", Lang::Ru);
        rig.key(Key::K, CTRL);
        rig.type_text("чай 5");
        assert_eq!(titles(&rig)[0], "Таймер «чай» на 5 мин");
        // Карточку убрали: строки нет, и Enter не заводит таймер.
        rig.app.settings.hide_card("timers");
        assert!(titles(&rig).is_empty(), "{:?}", titles(&rig));
        rig.key(Key::Enter, NO_KEYS);
        assert!(!rig.app.modules.iter().any(|m| m.ticking()), "таймер не заведён");
        assert!(rig.app.view == View::Search, "строк нет — Enter ничего не делает");
        // Калькулятор карточке не нужен.
        rig.app.search.query = "2+2".into();
        assert_eq!(titles(&rig)[0], "2+2 = 4");
        // Вернули карточку на полку — строка снова есть, и место в ней — новая полка.
        rig.app.settings.place_card("timers", 0, None);
        rig.app.search.query = "чай 5".into();
        let found = rig.app.found();
        assert_eq!(
            (found.hits[0].title.as_str(), found.hits[0].sub.as_str()),
            ("Таймер «чай» на 5 мин", "Таймеры · Экран и звук")
        );
    }

    /// Пока висит «Оставить этот режим?», печать поиск не открывает (вопрос не уйдёт под него), а Ctrl+K открывает.
    #[test]
    fn typing_does_not_open_the_search_over_a_question_but_ctrl_k_does() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-QUESTION", Lang::En);
        rig.key(Key::K, CTRL);
        rig.type_text("1080");
        rig.key(Key::ArrowDown, NO_KEYS);
        rig.key(Key::Enter, NO_KEYS);
        assert!(rig.app.modules.iter().any(|m| m.busy()), "режим сменился — ждём ответа");
        assert!(rig.app.view == View::Cards);
        for text in ["н", "night", "1"] {
            rig.type_text(text);
            assert!(rig.app.view == View::Cards, "«{text}» открыло поиск поверх вопроса");
        }
        rig.key(Key::K, CTRL);
        assert!(rig.app.view == View::Search, "Ctrl+K открывает и при вопросе");
    }

    /// Поле ввода у карточки («owner/repo» у GitHub) забирает и буквы, и Ctrl+K, и Ctrl+F.
    #[test]
    fn ctrl_k_and_letters_stay_in_a_text_field_of_a_card() {
        let mut rig = Rig::new();
        rig.app.edge.shown = true;
        // GitHub — на «Системе», четвёртая карточка из четырёх; поле «owner/repo» — в нижней строке карточки.
        rig.app.set_shelf(1);
        rig.settle();
        let x = 12.0 + 3.0 * ((SIZE.x - 24.0 - 3.0 * GAP) / 4.0 + GAP) + 60.0;
        let mut focused = false;
        for y in (120..330).step_by(4) {
            rig.click(Pos2::new(x, y as f32));
            if rig.ctx.text_edit_focused() {
                focused = true;
                break;
            }
        }
        assert!(focused, "поле «owner/repo» нашлось щелчком — иначе проверка ниже ничего не стоит");
        rig.key(Key::K, CTRL);
        assert!(rig.app.view == View::Cards, "Ctrl+K открыл поиск поверх поля ввода");
        rig.key(Key::F, CTRL);
        rig.type_text("н");
        assert!(rig.app.view == View::Cards, "Ctrl+F или буква открыли поиск поверх поля ввода");
        // Фокус ушёл из поля — поиск открывается как обычно.
        rig.ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        rig.settle();
        assert!(!rig.ctx.text_edit_focused());
        rig.key(Key::K, CTRL);
        assert!(rig.app.view == View::Search);
    }

    /// Esc в поиске пояса у «Часов» отменяет поиск пояса, а не прячет панель; следующий Esc прячет, как обычно.
    #[test]
    fn esc_in_the_clock_zone_search_cancels_it_and_keeps_the_panel() {
        let mut rig = Rig::new();
        rig.app.edge.shown = true;
        rig.app.set_shelf(1);
        rig.app.run_action("clocks:add");
        rig.settle();
        assert!(rig.app.typing_here());
        rig.key(Key::Escape, NO_KEYS);
        assert!(!rig.app.typing_here(), "Esc отменил поиск пояса");
        assert!(rig.app.edge.shown, "панель осталась на месте");
        rig.key(Key::Escape, NO_KEYS);
        assert!(!rig.app.edge.shown, "следующий Esc прячет панель, как обычно");
    }

    /// «Недавние» держатся за сами вещи (файл лотка, закреплённый текст, полку), а не за их места в списках.
    #[test]
    fn recent_actions_follow_their_things_when_the_lists_change() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-STABLE", Lang::En);
        let (a, b, new) =
            (r"C:\search-test\alpha-8841.txt", r"C:\search-test\beta-8841.txt", r"C:\search-test\gamma-8841.txt");
        rig.app.settings.tray = vec![a.into(), b.into()];
        rig.app.settings.pinned = vec!["search-test one 8841".into(), "search-test two 8841".into()];
        rig.settle();
        // Открыли первый файл, скопировали второй закреплённый текст, открыли полку «Система».
        for query in ["alpha-8841", "two 8841", "shelf system"] {
            rig.key(Key::K, CTRL);
            rig.type_text(query);
            rig.key(Key::Enter, NO_KEYS);
        }
        assert_eq!(rig.app.shelf, 1);
        assert_eq!(rig.app.recent[0], "shelf:@sys");
        // Списки поменялись: файл лотка встал наверх, закрепили новый текст, полки переставили.
        modules::tray::add(&mut rig.app.settings.tray, &[new.into()]);
        rig.app.settings.pinned.insert(0, "search-test zero 8841".into());
        rig.app.settings.shelves.swap(0, 1);
        rig.app.shelf = 1;
        rig.key(Key::K, CTRL);
        let found = rig.app.found();
        assert_eq!(found.listing, search_ui::Listing::Recent);
        // Полка «Система» теперь первая, а открыта вторая — действие осталось при «Системе».
        assert_eq!(
            titles(&rig),
            ["Open the “System” shelf", "Copy “search-test two 8841”", "Open “alpha-8841.txt”"],
            "в недавних те же вещи, что открывали, а не их соседи по номеру"
        );
        rig.key(Key::Enter, NO_KEYS);
        assert_eq!(rig.app.shelf, 0, "открылась «Система», а не та, что теперь под прежним номером");
        // Пропавшее не показывается: файл убрали из лотка, текст открепили; открытую полку не предлагают — остаётся подсказка.
        rig.app.settings.tray.retain(|p| p != a);
        rig.app.settings.pinned.retain(|p| p != "search-test two 8841");
        rig.key(Key::K, CTRL);
        let found = rig.app.found();
        assert_eq!((found.listing, found.hits.len()), (search_ui::Listing::Start, 0), "{:?}", titles(&rig));
    }

    /// Запятая в числе — по языку; ответ — до двенадцати значащих цифр; всё это — в строке поиска.
    #[test]
    fn the_calculator_reads_commas_by_language_and_rounds_the_noise() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-CALC-LANG", Lang::En);
        rig.key(Key::K, CTRL);
        for (query, title) in [
            ("1,000*2", "1,000*2 = 2000"),
            ("0.1+0.2-0.3", "0.1+0.2-0.3 = 0"),
            ("100/3", "100/3 = 33.3333333333"),
            ("1/3000000", "1/3000000 = 3.33333333333e-7"),
        ] {
            rig.app.search.query = query.to_string();
            assert_eq!(titles(&rig)[0], title);
        }
        rig.app.search.query = "1,5+1".into();
        assert!(!titles(&rig).iter().any(|t| t.contains(" = ")), "запятая не на месте — строки нет");
        // По-русски запятая десятичная.
        rig.app.settings.common.language = Lang::Ru;
        rig.app.search.query = "1,000*2".into();
        assert_eq!(titles(&rig)[0], "1,000*2 = 2");
        rig.app.search.query = "2/3".into();
        assert_eq!(titles(&rig)[0], "2/3 = 0,666666666667");
        // Enter кладёт в буфер то же, что написано справа от «=».
        rig.app.search.query = "1/3000000".into();
        rig.key(Key::Enter, NO_KEYS);
        let written = sys::BLOCKED.lock().unwrap().iter().any(|w| w == "clip::write_text 3,33333333333e-7");
        assert!(written, "в буфер ушёл результат, как он написан");
    }

    /// Длинное название обрезается многоточием, а полное — в подсказке; короткое подсказки не просит.
    #[test]
    fn a_cut_title_gets_a_tooltip_with_the_full_text() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-SEARCH-TIP", Lang::En);
        let long = format!("{}.txt", "long".repeat(40));
        rig.app.settings.tray = vec![format!(r"C:\search-test\{long}"), r"C:\search-test\short-8851.txt".into()];
        rig.settle();
        rig.key(Key::K, CTRL);
        rig.type_text("long");
        let full = format!("Open “{long}”");
        let count = |rig: &Rig| painted(rig).matches(full.as_str()).count();
        let before = count(&rig);
        rig.move_to(rig.anchor("search-row-0").center());
        for _ in 0..40 {
            rig.frame(Vec::new());
        }
        assert!(count(&rig) > before, "задержались над строкой — полное название в подсказке");
        // Короткое название влезло — подсказки нет.
        rig.move_to(Pos2::new(5.0, 5.0));
        rig.app.search.query = "short-8851".into();
        rig.settle();
        let short = "Open “short-8851.txt”";
        rig.move_to(rig.anchor("search-row-0").center());
        for _ in 0..40 {
            rig.frame(Vec::new());
        }
        assert_eq!(painted(&rig).matches(short).count(), 1, "влезло — подсказки нет");
    }

    /// Поле в шапке и «Стереть» называют себя диктору; строки результатов берут фокус с клавиатуры и показывают рамку.
    #[test]
    fn the_search_controls_have_names_and_the_rows_show_the_focus() {
        let mut rig = Rig::new();
        let heard = |events: &[egui::output::OutputEvent]| -> Vec<String> {
            events
                .iter()
                .filter_map(|e| match e {
                    egui::output::OutputEvent::Clicked(info) => info.label.clone(),
                    _ => None,
                })
                .collect()
        };
        let t = texts::tr(rig.app.lang());
        // Поле целиком (на широкой панели) называется «Найти действие».
        let field = rig.anchor("search-open");
        assert_eq!(field.width(), search_ui::FIELD_W, "на 1300 поле целиком");
        let events = rig.click_events(field.center());
        assert_eq!(heard(&events), [t.search_field.to_string()]);
        assert!(rig.app.view == View::Search);
        // «Стереть» называется так, как подписано.
        rig.type_text("night");
        let events = rig.click_events(rig.anchor("search-clear").center());
        assert_eq!(heard(&events), [t.search_clear.to_string()]);
        // Строки: Tab из поля доводит фокус до строки, и у неё рамка.
        rig.type_text("turn");
        let rows = rig.app.found().hits.len();
        let accent_text = Palette::of_ctx(&rig.ctx).accent_text;
        let mut on_row = None;
        for _ in 0..12 {
            rig.key(Key::Tab, NO_KEYS);
            if let Some(at) = rig.focused_rect()
                && let Some(i) = (0..rows).find(|i| rig.anchor(&format!("search-row-{i}")) == at)
            {
                on_row = Some((i, at));
                break;
            }
        }
        let (row, at) = on_row.expect("Tab довёл фокус до строки результата");
        assert!(rig.outlines(accent_text).contains(&at.expand(2.0)), "у строки {row} нет рамки фокуса");
    }

    // ─── Кто дал клавиатуру ─────────────────────────────────────────────────
    //
    // Панель, выдвинутая мышью у края, клавиатуру у программы пользователя не забирает (окно неактивно, `WS_EX_NOACTIVATE`),
    // пока в ней не нажмут мышью; вызванная клавишей показа или клавишей сцены берёт её сразу. Печать открывает поиск только
    // у панели с клавиатурой: набранное в чужой программе в панель не попадает, и случайный Enter ничего не выполнит.

    /// Выдвинуть панель мышью у края, как это делает `tick`: мышь на верхней строке, прошла задержка показа.
    fn pull_out_by_the_mouse(rig: &mut Rig) {
        let (show, hide) = (Duration::from_millis(150), Duration::from_millis(450));
        let (now, edge) = (Instant::now(), Input { at_edge: true, ..Input::default() });
        rig.app.edge = Edge::default();
        rig.app.edge.step(edge, now, show, hide);
        rig.app.edge.step(edge, now + Duration::from_millis(200), show, hide);
        assert!(rig.app.edge.shown, "выехала от края");
    }

    #[test]
    fn a_panel_pulled_out_by_the_mouse_gets_the_keyboard_only_with_a_press_inside() {
        let mut rig = Rig::new();
        pull_out_by_the_mouse(&mut rig);
        assert!(!rig.app.edge.keyboard(), "мышь у края клавиатуру не даёт");
        assert!(rig.app.wants_no_activate(), "окно не активируется: клавиатура остаётся у программы пользователя");
        // Набор, даже если бы дошёл до окна, поиск не открывает; Ctrl+K тоже.
        rig.type_text("н");
        rig.key(Key::K, CTRL);
        assert!(rig.app.view == View::Cards, "без клавиатуры печать и Ctrl+K поиск не открывают");
        assert!(!rig.app.edge.keyboard(), "ни наведение мыши, ни кадры клавиатуру не дают");
        rig.move_to(Pos2::new(300.0, 200.0));
        assert!(!rig.app.edge.keyboard(), "мышь над панелью — ещё не нажатие");
        // Первое нажатие внутри: окно активно, клавиатура у панели, а само нажатие сработало как обычный щелчок.
        rig.click(rig.tab(1).center());
        assert!(rig.app.edge.keyboard() && !rig.app.wants_no_activate());
        assert_eq!(rig.app.shelf, 1, "первое нажатие — обычный щелчок по вкладке");
        rig.type_text("н");
        assert!(rig.app.view == View::Search, "с клавиатурой печать открывает поиск");
        // Нажатие в любом месте панели даёт клавиатуру (и по пустому месту карточки, и правой кнопкой).
        for secondary in [false, true] {
            let mut rig = Rig::new();
            pull_out_by_the_mouse(&mut rig);
            // Пустое место шапки между вкладками и кнопками: нажатие туда ничего не делает, кроме как даёт клавиатуру.
            let at = Pos2::new(650.0, 24.0);
            if secondary {
                rig.right_click(at);
            } else {
                rig.click(at);
            }
            assert!(rig.app.edge.keyboard(), "нажатие (правой кнопкой: {secondary}) даёт клавиатуру");
        }
    }

    #[test]
    fn a_panel_called_by_a_key_has_the_keyboard_at_once_and_loses_it_when_hidden() {
        let mut rig = Rig::new();
        let (show, hide) = (Duration::from_millis(150), Duration::from_millis(450));
        rig.app.edge = Edge::default();
        rig.app.edge.step(Input { hotkey: true, ..Input::default() }, Instant::now(), show, hide);
        assert!(rig.app.edge.shown && rig.app.edge.keyboard(), "клавиша показа дала клавиатуру сразу");
        assert!(!rig.app.wants_no_activate());
        rig.type_text("н");
        assert!(rig.app.view == View::Search && rig.app.search.query == "н", "печать открыла поиск с этой буквой");
        // Спрятали — клавиатуры нет, окно снова не активируется; выехала мышью — снова без неё.
        rig.app.close_search();
        rig.app.edge.hide();
        assert!(!rig.app.edge.keyboard() && rig.app.wants_no_activate());
        pull_out_by_the_mouse(&mut rig);
        assert!(!rig.app.edge.keyboard() && rig.app.wants_no_activate());
        // Спрятана совсем — окно не активируется, как у полоски.
        rig.app.edge.hide();
        assert!(rig.app.wants_no_activate());
    }

    #[test]
    fn search_reads_every_card_on_every_shelf_and_the_search_forgets_itself_when_the_panel_hides() {
        let mut rig = Rig::new();
        let open_only = rig.app.refresh_set();
        let all_cards: usize = rig.app.settings.shelves.iter().map(|s| s.cards.len()).sum();
        assert!(open_only.len() < all_cards, "без поиска — открытая полка");
        rig.app.view = View::Search;
        assert_eq!(rig.app.refresh_set().len(), all_cards, "в поиске — все карточки с полок");
        // Спряталась или нажали клавишу сцены — набранное забыто.
        rig.app.search.query = "abc".into();
        rig.app.close_search();
        assert!(rig.app.view == View::Cards && rig.app.search.query.is_empty());
    }

    // ─── Живые плашки у края (2.4) ──────────────────────────────────────────────────────────────────────────────────

    use std::path::{Path, PathBuf};

    use crate::modules::timers::{self, Phase};
    use crate::plaque::{Gate, Say};
    use crate::sys::usb;

    impl Rig {
        /// Кадр жизни плашки: очередь ставит её, окно рисует (панель спрятана, плашке можно, мышь не над ней).
        fn show_plaque(&mut self) {
            self.app.plaque_frame(&self.ctx, Instant::now(), false, Gate::Open);
            self.app.strip_kind = StripKind::Plaque;
            self.settle();
        }

        /// Мышь над кнопкой плашки `n` и несколько кадров на месте: какой курсор попросило окно.
        fn cursor_over(&mut self, anchor: &str) -> egui::CursorIcon {
            let at = self.anchor(anchor).center();
            self.move_to(at);
            self.settle();
            self.cursor
        }
    }

    /// Окно плашки без окна ОС: размер как у плашки, панель спрятана, движение выключено (всё сразу), плашка показана, но кнопки
    /// ещё не принимают щелчок (отсчёт `ARM` не вышел и не выйдет: час).
    fn unarmed_rig(plaque: Plaque) -> Rig {
        let mut rig = Rig::new();
        rig.size = PLAQUE;
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        rig.app.drawn = 0.0;
        rig.app.plaques.set_arm(Duration::from_secs(3600));
        rig.app.plaques.push(plaque, Instant::now());
        rig.show_plaque();
        rig
    }

    /// То же, но плашка уже приняла щелчок: тесты кнопок не ждут 400 мс. Отсчёт `ARM` проверяет отдельный тест.
    fn plaque_rig(plaque: Plaque) -> Rig {
        let mut rig = unarmed_rig(plaque);
        rig.app.plaques.set_arm(Duration::ZERO);
        rig.settle();
        rig
    }

    /// Просили ли заслон открыть `what` (адрес, диск, файл) — такой записью в журнале (`sys::launch`).
    fn launched(what: &str) -> bool {
        let want = format!("launch {what} ");
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).contains(&want)
    }

    fn logged(line: &str) -> usize {
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|w| *w == line).count()
    }

    fn drive(root: &str, label: &str) -> usb::Volume {
        usb::Volume::invented(root, label, 32 << 30)
    }

    /// Загрузка с выдуманным именем: у каждого теста свой файл, чтобы метки в общем журнале заслона не путались.
    fn download(name: &str) -> Plaque {
        let entry = watch::Entry { name: name.into(), len: 1_258_291, modified: None };
        watch::download_plaque(Path::new(r"C:\Demo\Downloads"), &entry)
    }

    #[test]
    fn the_plaque_says_its_words_and_has_its_buttons_in_both_languages() {
        for (lang, words) in [
            (Lang::Ru, ["Флешка «DEMO»", "32 ГБ · вставлена", "Открыть", "Извлечь"]),
            (Lang::En, ["USB drive “DEMO”", "32 GB · plugged in", "Open", "Eject"]),
        ] {
            let mut rig = plaque_rig(plaque::sample("usb", lang).unwrap());
            rig.app.settings.common.language = lang;
            rig.settle();
            let text = painted(&rig);
            for word in words {
                assert!(text.contains(word), "{lang:?}: нет «{word}» в {text:?}");
            }
            // Кнопки — слева направо «Открыть», «Извлечь»; креста у обычной плашки нет.
            let (open, eject) = (rig.anchor("plaque-button-0"), rig.anchor("plaque-button-1"));
            assert!(open.right() < eject.left() && eject.right() <= PLAQUE.x - 10.0 + 0.5, "{open:?} {eject:?}");
            assert!(open.height() == 28.0 && eject.height() == 28.0);
            assert!(rig.ctx.read_response(egui::Id::new(("anchor", "plaque-close"))).is_none());
        }
    }

    /// Слова плашки собираются при показе: сменили язык — плашка заговорила на другом, а не осталась со старыми словами.
    #[test]
    fn a_plaque_that_hangs_changes_its_language_with_the_panel() {
        let mut rig = plaque_rig(plaque::sample("github", Lang::Ru).unwrap());
        rig.app.settings.common.language = Lang::Ru;
        rig.settle();
        assert!(painted(&rig).contains("demo/site: сборка упала"));
        rig.app.settings.common.language = Lang::En;
        rig.settle();
        assert!(painted(&rig).contains("demo/site: build failed"));
    }

    #[test]
    fn a_click_on_the_plaque_opens_the_panel_on_the_shelf_of_its_card() {
        let mut rig = plaque_rig(plaque::sample("bluetooth", Lang::En).unwrap());
        rig.app.settings.common.language = Lang::En;
        rig.settle();
        assert!(!rig.app.edge.shown && rig.app.shelf == 0);
        // Диктор слышит слова плашки: она рисуется без виджетов, и имя у её тела — название и пояснение.
        let events = rig.click_events(Pos2::new(200.0, 24.0));
        let said = events.iter().any(|e| {
            matches!(e, egui::output::OutputEvent::Clicked(info) if info.label.as_deref() == Some("Headphones: connected. Bluetooth · battery 80%"))
        });
        assert!(said, "{events:?}");
        assert!(rig.app.edge.shown && rig.app.edge.keyboard(), "щелчок по плашке — как вызов клавишей");
        assert_eq!(rig.app.shelf, 1, "карточка Bluetooth стоит на полке «Система»");
        assert!(rig.app.plaques.current().is_none(), "плашка своё сказала");
        // Карточку убрали с полок — панель просто открывается на той полке, что была.
        let mut rig = plaque_rig(plaque::sample("bluetooth", Lang::En).unwrap());
        rig.app.settings.hide_card("bluetooth");
        rig.click(Pos2::new(200.0, 24.0));
        assert!(rig.app.edge.shown && rig.app.shelf == 0);
    }

    /// Плашка появляется сама: щелчок сразу после выезда, что целился в окно под ней, ни кнопку, ни крестик, ни тело, ни
    /// перетаскивание не задевает — гасится; через 400 мс всё работает. Под мышью до того кнопка не светится и «руки» нет.
    #[test]
    fn a_plaque_swallows_clicks_until_it_has_stood_for_a_moment() {
        let mut rig = unarmed_rig(watch::usb_plaque(&drive(r"J:\", "SWALLOW")));
        assert_eq!(rig.cursor_over("plaque-button-0"), egui::CursorIcon::Default, "рука — только когда нажимается");
        rig.click_anchor("plaque-button-0");
        assert!(!launched(r"J:\"), "щелчок сразу после появления ничего не открыл");
        rig.click_anchor("plaque-button-1");
        assert!(!rig.app.plaques.busy(), "и не извлёк");
        rig.click(Pos2::new(120.0, 24.0));
        assert!(!rig.app.edge.shown && rig.app.plaques.current().is_some(), "и по телу панель не открылась");
        // Простояла своё — щелчок работает, и рука появилась.
        rig.app.plaques.set_arm(Duration::ZERO);
        rig.settle();
        assert_eq!(rig.cursor_over("plaque-button-0"), egui::CursorIcon::PointingHand);
        rig.click_anchor("plaque-button-0");
        assert!(launched(r"J:\"));
        // Крестик и перетаскивание — так же.
        let mut rig = unarmed_rig(plaque::sample("github", Lang::En).unwrap());
        rig.click_anchor("plaque-close");
        assert!(rig.app.plaques.current().is_some(), "крестик сразу не закрыл");
        let mut rig = unarmed_rig(download("swallow-8841.pdf"));
        rig.drag(Pos2::new(100.0, 24.0), Pos2::new(220.0, 40.0));
        assert_eq!(logged(r"drag_file C:\Demo\Downloads\swallow-8841.pdf"), 0, "и тянуть сразу не начало");
        assert!(rig.app.plaques.current().is_some());
    }

    /// Отсчёт идёт от того, как окно встало на место: каждый кадр `plaque_frame` с полным выездом его запускает один раз.
    #[test]
    fn the_countdown_to_taking_clicks_starts_when_the_window_is_all_the_way_out() {
        let mut rig = Rig::new();
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        let t0 = Instant::now();
        rig.app.post(plaque::sample("usb", Lang::En).unwrap(), t0);
        rig.app.plaque_frame(&rig.ctx, t0, false, Gate::Open);
        assert!(!rig.app.plaques.armed(t0 + Duration::from_millis(399)));
        rig.app.plaque_frame(&rig.ctx, t0 + Duration::from_millis(200), false, Gate::Open);
        assert!(!rig.app.plaques.armed(t0 + Duration::from_millis(399)), "повторные кадры отсчёт не сдвигают");
        assert!(rig.app.plaques.armed(t0 + Duration::from_millis(400)));
    }

    #[test]
    fn the_open_button_opens_the_drive_and_ends_the_plaque_without_pulling_the_panel_out() {
        let mut rig = plaque_rig(watch::usb_plaque(&drive(r"K:\", "KEY")));
        rig.click_anchor("plaque-button-0");
        assert!(launched(r"K:\"), "диск открылся, как двойным щелчком");
        assert!(rig.app.plaques.current().is_none() && !rig.app.edge.shown);
    }

    /// «Извлечь»: плашка остаётся, кнопки не нажимаются, вместо пояснения «Извлекаю…»; итог (поток) заменяет её на месте —
    /// «Можно вынимать» или «Не извлечь: диск занят»; второй щелчок до итога ничего не запускает.
    #[test]
    fn eject_keeps_the_plaque_with_dead_buttons_until_the_answer_replaces_it() {
        usb::fake_busy(r"N:\", true);
        for (root, label, answer, tone) in [
            (r"M:\", "FREE", "Safe to remove", Tone::Accent),
            (r"N:\", "BUSY", "Can’t eject: the drive is busy", Tone::Warning),
        ] {
            let volume = drive(root, label);
            usb::fake_add(volume.clone());
            let mut rig = plaque_rig(watch::usb_plaque(&volume));
            rig.app.settings.common.language = Lang::En;
            rig.settle();
            assert!(!painted(&rig).contains("Ejecting…"));
            rig.click_anchor("plaque-button-1");
            assert!(rig.app.plaques.busy(), "кнопки ждут итога");
            assert!(painted(&rig).contains("Ejecting…"), "ход виден: {}", painted(&rig));
            rig.click_anchor("plaque-button-1");
            rig.click_anchor("plaque-button-0");
            assert!(!launched(root), "пока ждём — и «Открыть» не нажимается");
            // Итог приходит из потока плашкой с тем же ключом.
            let key = format!("usb:{root}");
            let start = Instant::now();
            let result = loop {
                let found = rig.app.watch.events(&rig.app.settings).plaques.into_iter().find(|p| p.key == key);
                if let Some(found) = found {
                    break found;
                }
                assert!(start.elapsed() < Duration::from_secs(5), "итога извлечения нет");
                std::thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(logged(&format!("usb::eject {root}")), 1, "извлечение запущено один раз");
            rig.app.post(result, Instant::now());
            assert!(!rig.app.plaques.busy(), "итог заменил плашку — кнопки снова живые");
            rig.show_plaque();
            assert_eq!(rig.app.plaques.current().map(|c| c.tone), Some(tone));
            assert!(painted(&rig).contains(answer), "{answer}: {}", painted(&rig));
            assert!(!painted(&rig).contains("Ejecting…"));
        }
        usb::fake_busy(r"N:\", false);
        usb::fake_remove(r"N:\");
    }

    /// Флешку вынули, пока плашка висела, а на её букву встал другой диск: «Извлечь» ничего не извлекает и говорит об этом.
    #[test]
    fn eject_of_a_drive_that_is_already_gone_says_so_and_ejects_nothing() {
        let old = drive(r"H:\", "GONE");
        let other = usb::Volume { serial: old.serial + 1, ..drive(r"H:\", "OTHER") };
        usb::fake_add(other.clone());
        let mut rig = plaque_rig(watch::usb_plaque(&old));
        rig.app.settings.common.language = Lang::En;
        rig.click_anchor("plaque-button-1");
        let start = Instant::now();
        let result = loop {
            let found = rig.app.watch.events(&rig.app.settings).plaques.into_iter().find(|p| p.key == r"usb:H:\");
            if let Some(found) = found {
                break found;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "ответа нет");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(result.title, Say::Gone);
        rig.app.post(result, Instant::now());
        rig.show_plaque();
        assert!(painted(&rig).contains("The drive is already gone"), "{}", painted(&rig));
        assert!(usb::volumes().iter().any(|v| v.root == r"H:\" && v.serial == other.serial), "чужой диск на месте");
        usb::fake_remove(r"H:\");
    }

    #[test]
    fn a_failed_build_stays_has_a_cross_and_opens_the_run() {
        let mut rig = plaque_rig(plaque::sample("github", Lang::En).unwrap());
        // Висит: через час всё ещё здесь, полоски нет.
        rig.app.plaque_frame(&rig.ctx, Instant::now() + Duration::from_secs(3600), false, Gate::Open);
        assert!(rig.app.plaques.current().is_some() && rig.app.plaques.fraction().is_none());
        rig.click_anchor("plaque-button-0");
        assert!(launched("https://github.com/demo/site/actions/runs/7001"), "сам прогон в браузере");
        assert!(rig.app.plaques.current().is_none());
        // Крестик закрывает, не открывая ничего.
        let mut rig = plaque_rig(plaque::sample("github", Lang::En).unwrap());
        rig.click_anchor("plaque-close");
        assert!(rig.app.plaques.current().is_none() && !rig.app.edge.shown);
    }

    #[test]
    fn the_timer_plaque_has_done_and_no_cross_and_does_not_pull_the_panel_out() {
        let mut rig = plaque_rig(timers::done_plaque(Phase::Plain, Some("tea".into())));
        rig.app.settings.common.language = Lang::En;
        rig.settle();
        assert!(painted(&rig).contains("Timer “tea” is up") && painted(&rig).contains("Done"));
        assert!(
            rig.ctx.read_response(egui::Id::new(("anchor", "plaque-close"))).is_none(),
            "«Готово» закрывает, крест не нужен"
        );
        rig.click_anchor("plaque-button-0");
        assert!(rig.app.plaques.current().is_none() && !rig.app.edge.shown && !rig.app.edge.keyboard());
    }

    /// Скачанная программа не запускается щелчком у края: вместо «Открыть» — «Показать в папке», и это открывает проводник с
    /// выделенным файлом, а не сам файл.
    #[test]
    fn a_downloaded_program_is_shown_in_its_folder_and_never_launched_by_the_plaque() {
        let mut rig = plaque_rig(download("setup-4471.exe"));
        rig.app.settings.common.language = Lang::En;
        rig.settle();
        let text = painted(&rig);
        assert!(text.contains("Show in folder") && !text.contains("Open"), "{text}");
        rig.click_anchor("plaque-button-0");
        assert_eq!(logged(r#"launch explorer.exe /select,"C:\Demo\Downloads\setup-4471.exe""#), 1);
        assert!(!launched(r"C:\Demo\Downloads\setup-4471.exe"), "сам файл не запускался");
        assert!(rig.app.plaques.current().is_none());
        // Обычный файл — «Открыть», как раньше.
        let mut rig = plaque_rig(download("memo-4471.pdf"));
        rig.click_anchor("plaque-button-0");
        assert!(launched(r"C:\Demo\Downloads\memo-4471.pdf"));
    }

    /// Несколько файлов сразу: «Показать в папке» открывает «Загрузки».
    #[test]
    fn the_plaque_of_several_files_shows_the_downloads_folder() {
        let mut rig = plaque_rig(plaque::sample("downloads-many", Lang::En).unwrap());
        rig.app.settings.common.language = Lang::En;
        rig.settle();
        assert!(painted(&rig).contains("3 files downloaded") && painted(&rig).contains("Show in folder"));
        rig.click_anchor("plaque-button-0");
        assert!(launched(r"C:\Demo\Downloads"), "папка открылась");
        assert!(rig.app.plaques.current().is_none());
    }

    /// Длинное имя режется посередине: расширение и «downloaded» на виду; диктор слышит имя целиком.
    #[test]
    fn a_long_file_name_is_cut_in_the_middle_and_the_narrator_hears_all_of_it() {
        let name = "annual_report_2026_final_version_of_the_big_document.pdf";
        let mut rig = plaque_rig(download(name));
        rig.app.settings.common.language = Lang::En;
        rig.settle();
        let text = painted(&rig);
        assert!(!text.contains(name), "целиком не поместилось: {text}");
        assert!(text.contains("…") && text.contains(".pdf: downloaded"), "расширение и слово на виду: {text}");
        assert!(text.contains("annu"), "и начало имени: {text}");
        let events = rig.click_events(Pos2::new(120.0, 24.0));
        let said = events.iter().any(|e| {
            matches!(e, egui::output::OutputEvent::Clicked(info) if info.label.as_deref().is_some_and(|l| l.starts_with(&format!("{name}: downloaded"))))
        });
        assert!(said, "диктор слышит имя целиком: {events:?}");
    }

    /// В окне плашки 372 × 48 подсказке не поместиться: её нет ни у тела, ни у крестика.
    #[test]
    fn the_plaque_has_no_tooltip_inside_its_window() {
        let tooltips = |rig: &Rig| {
            rig.ctx.memory(|m| m.areas().visible_layer_ids().iter().filter(|l| l.order == egui::Order::Tooltip).count())
        };
        let mut rig = plaque_rig(plaque::sample("github", Lang::En).unwrap());
        for anchor in ["plaque-body", "plaque-close"] {
            let at = if anchor == "plaque-body" { Pos2::new(120.0, 24.0) } else { rig.anchor(anchor).center() };
            rig.move_to(at);
            for _ in 0..40 {
                rig.frame(Vec::new());
            }
            assert_eq!(tooltips(&rig), 0, "подсказка у {anchor}");
        }
    }

    /// Курсор: «рука» у плашки без файла, «взять» у плашки с файлом — когда она нажимается; недоступная кнопка (ждём итог) — обычный.
    #[test]
    fn the_cursor_shows_a_hand_or_a_grab_only_where_the_plaque_can_be_clicked() {
        let mut rig = plaque_rig(plaque::sample("downloads", Lang::En).unwrap());
        rig.move_to(Pos2::new(100.0, 24.0));
        rig.settle();
        assert_eq!(rig.cursor, egui::CursorIcon::Grab, "файл можно тянуть");
        let mut rig = plaque_rig(plaque::sample("bluetooth", Lang::En).unwrap());
        rig.move_to(Pos2::new(100.0, 24.0));
        rig.settle();
        assert_eq!(rig.cursor, egui::CursorIcon::PointingHand);
        // Ждём итог «Извлечь»: кнопки мертвы — рука над ними не нужна.
        let mut rig = plaque_rig(watch::usb_plaque(&drive(r"G:\", "WAIT")));
        rig.app.plaques.set_busy(true);
        rig.settle();
        assert_eq!(rig.cursor_over("plaque-button-1"), egui::CursorIcon::Default);
    }

    #[test]
    fn a_file_dragged_from_the_plaque_starts_the_windows_drag_and_closes_the_plaque() {
        let path = r"C:\Demo\Downloads\report.pdf";
        let mut rig = plaque_rig(plaque::sample("downloads", Lang::En).unwrap());
        rig.drag(Pos2::new(100.0, 24.0), Pos2::new(220.0, 40.0));
        assert_eq!(logged(&format!("drag_file {path}")), 1, "перетаскивание файла началось");
        assert!(rig.app.plaques.current().is_none(), "плашка отдала файл — уходит");
        // egui не видел, как отпустили кнопку, — следующий кадр отпускает её за него.
        assert!(rig.app.release.is_some());
        let mut raw = egui::RawInput::default();
        eframe::App::raw_input_hook(&mut rig.app, &rig.ctx, &mut raw);
        assert!(matches!(
            raw.events.as_slice(),
            [Event::PointerButton { pressed: false, button: PointerButton::Primary, .. }]
        ));
        assert!(rig.app.release.is_none());
        eframe::App::raw_input_hook(&mut rig.app, &rig.ctx, &mut raw);
        assert_eq!(raw.events.len(), 1, "один раз");
        // Без файла (наушники) протяжка ничего не тянет, и плашка остаётся.
        let mut rig = plaque_rig(plaque::sample("bluetooth", Lang::En).unwrap());
        rig.drag(Pos2::new(100.0, 24.0), Pos2::new(220.0, 40.0));
        assert!(rig.app.plaques.current().is_some() && rig.app.release.is_none());
    }

    /// Тянуть начинают, только когда мышь сдвинулась и кнопка ещё нажата: долгое нажатие без движения и отпускание в том же
    /// кадре, что и сдвиг, перетаскивания не начинают (бросок ушёл бы программе под курсором).
    #[test]
    fn a_long_press_or_a_move_on_release_does_not_start_the_drag() {
        let from = Pos2::new(100.0, 24.0);
        // Держат дольше 0,8 с, не двигая мышь.
        let mut rig = plaque_rig(download("long-press-3310.pdf"));
        rig.move_to(from);
        rig.frame(vec![rig.button(true)]);
        for _ in 0..45 {
            rig.frame(Vec::new());
        }
        rig.frame(vec![rig.button(false)]);
        rig.settle();
        assert_eq!(logged(r"drag_file C:\Demo\Downloads\long-press-3310.pdf"), 0, "долгое нажатие не тянет");
        assert!(rig.app.plaques.current().is_some() && rig.app.release.is_none());
        // Сдвиг и отпускание в одном кадре.
        let mut rig = plaque_rig(download("release-move-3310.pdf"));
        rig.move_to(from);
        rig.frame(vec![rig.button(true)]);
        let to = Pos2::new(240.0, 40.0);
        rig.mouse = to;
        rig.frame(vec![Event::PointerMoved(to), rig.button(false)]);
        rig.settle();
        assert_eq!(logged(r"drag_file C:\Demo\Downloads\release-move-3310.pdf"), 0, "кнопку уже отпустили");
        // Для сравнения: настоящее перетаскивание начинается.
        let mut rig = plaque_rig(download("real-drag-3310.pdf"));
        rig.drag(from, to);
        assert_eq!(logged(r"drag_file C:\Demo\Downloads\real-drag-3310.pdf"), 1);
    }

    /// Файл, что тянули с плашки и отпустили над её же окном, winit отдаёт как брошенный на панель: в лоток он не идёт — три
    /// секунды; чужие файлы идут всегда.
    #[test]
    fn a_file_dropped_back_on_its_own_plaque_does_not_go_to_the_tray() {
        let mut rig = Rig::new();
        let own = PathBuf::from(r"C:\Demo\Downloads\own-drop-5521.pdf");
        let other = PathBuf::from(r"C:\Demo\Downloads\other-5521.pdf");
        assert_eq!(
            rig.app.dropped_but_not_own(vec![own.clone()]),
            std::slice::from_ref(&own),
            "ничего не тянули — идёт"
        );
        rig.app.own_drop = Some((own.clone(), Instant::now()));
        let same_in_other_case = PathBuf::from(r"c:\demo\downloads\OWN-DROP-5521.PDF");
        assert_eq!(
            rig.app.dropped_but_not_own(vec![own.clone(), other.clone(), same_in_other_case]),
            std::slice::from_ref(&other)
        );
        // Прошло больше трёх секунд — это уже новый бросок.
        let long_ago = Instant::now().checked_sub(Duration::from_secs(4)).unwrap_or_else(Instant::now);
        rig.app.own_drop = Some((own.clone(), long_ago));
        assert_eq!(rig.app.dropped_but_not_own(vec![own.clone()]), [own], "три секунды вышли");
        assert!(rig.app.own_drop.is_none());
    }

    /// «В лоток» кладёт файл в лоток (в настройки) и закрывает плашку.
    #[test]
    fn to_tray_puts_the_file_into_the_tray() {
        let path = r"C:\Demo\Downloads\report.pdf";
        let mut rig = plaque_rig(plaque::sample("downloads", Lang::En).unwrap());
        assert!(rig.app.settings.tray.is_empty());
        rig.click_anchor("plaque-button-1");
        assert_eq!(rig.app.settings.tray, [path]);
        assert!(rig.app.plaques.current().is_none());
    }

    /// Панель выдвинута — событие уведомлением набора, без плашки; спрятана — плашкой.
    #[test]
    fn with_the_panel_out_an_event_is_a_notice_and_with_it_hidden_a_plaque() {
        let mut rig = Rig::new();
        rig.app.settings.common.language = Lang::En;
        rig.app.edge.shown = true;
        let before = Instant::now();
        rig.app.post(plaque::sample("usb", Lang::En).unwrap(), Instant::now());
        rig.app.post(plaque::sample("github", Lang::En).unwrap(), Instant::now());
        assert!(rig.app.plaques.current().is_none() && rig.app.plaques.is_empty());
        assert_eq!(
            rig.app.toasts.since(before),
            vec![
                ("USB drive “DEMO” · 32 GB · plugged in".to_string(), Tone::Accent),
                ("demo/site: build failed · main · 1 min ago".to_string(), Tone::Danger),
            ]
        );
        rig.app.edge.shown = false;
        rig.app.post(plaque::sample("usb", Lang::En).unwrap(), Instant::now());
        assert_eq!(rig.app.plaques.waiting(), 1);
    }

    /// Панель выдвинулась, пока плашки ждали: обычные уходят уведомлениями, важные остаются и показываются, когда панель
    /// спрячется.
    #[test]
    fn a_panel_that_comes_out_takes_the_ordinary_plaques_and_leaves_the_important_ones() {
        let mut rig = Rig::new();
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        let (usb, fail) = (plaque::sample("usb", Lang::En).unwrap(), plaque::sample("github", Lang::En).unwrap());
        rig.app.post(usb, Instant::now());
        rig.app.post(fail, Instant::now());
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        assert!(rig.app.plaque_drawn.is_some(), "флешка на экране");
        let before = Instant::now();
        rig.app.edge.shown = true;
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Closed);
        assert!(rig.app.plaque_drawn.is_none(), "окно плашки ушло");
        assert_eq!(rig.app.toasts.since(before).len(), 1, "флешка — уведомлением, сборка осталась");
        // Панель спряталась — важная встаёт.
        rig.app.edge.shown = false;
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        assert_eq!(rig.app.plaque_drawn.as_ref().map(|p| p.key.as_str()), Some("gh:demo/site"));
    }

    /// Панель вызвали клавишей (или в ней взяли клавиатуру): плашку из другого окна с клавиатуры не закрыть — важные тоже
    /// пересказываются уведомлениями и уходят, а не висят до закрытия панели.
    #[test]
    fn a_panel_with_the_keyboard_tells_the_important_plaques_too_and_clears_them() {
        let mut rig = Rig::new();
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        rig.app.post(plaque::sample("github", Lang::En).unwrap(), Instant::now());
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        assert_eq!(rig.app.plaque_drawn.as_ref().map(|p| p.key.as_str()), Some("gh:demo/site"));
        rig.app.post(timers::done_plaque(Phase::Plain, None), Instant::now());
        let before = Instant::now();
        rig.app.edge.take_keyboard();
        rig.app.edge.shown = true;
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Closed);
        assert_eq!(rig.app.toasts.since(before).len(), 2, "и упавшая сборка, и таймер — уведомлениями");
        assert!(rig.app.plaques.is_empty(), "очередь чиста: после панели ничего не вскочит");
    }

    /// Обычная плашка, что постояла на виду дольше секунды, увидена: выдвинувшаяся панель не пересказывает её уведомлением.
    #[test]
    fn a_plaque_that_was_seen_is_not_told_again_by_the_panel() {
        for (shown_for_ms, told) in [(400, 1), (1500, 0)] {
            let mut rig = Rig::new();
            anvil_ui::motion::set_reduced(&rig.ctx, true);
            rig.app.edge = Edge::default();
            let t0 = Instant::now();
            rig.app.post(plaque::sample("usb", Lang::En).unwrap(), t0);
            rig.app.plaque_frame(&rig.ctx, t0, false, Gate::Open);
            assert!(rig.app.plaque_drawn.is_some());
            let before = Instant::now();
            rig.app.edge.shown = true;
            rig.app.plaque_frame(&rig.ctx, t0 + Duration::from_millis(shown_for_ms), false, Gate::Closed);
            assert_eq!(rig.app.toasts.since(before).len(), told, "на виду {shown_for_ms} мс");
        }
    }

    /// Игра во весь экран или программа из «Не выезжать»: плашка копится и не показывается, показанная прячется и ждёт.
    #[test]
    fn a_plaque_waits_while_it_may_not_show_and_comes_when_it_may() {
        let mut rig = Rig::new();
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        rig.app.post(plaque::sample("usb", Lang::En).unwrap(), Instant::now());
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Closed);
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Closed);
        assert!(rig.app.plaque_drawn.is_none() && rig.app.plaques.current().is_none(), "копится");
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        assert!(rig.app.plaque_drawn.is_some());
        // Игра началась посреди показа: плашка уехала, но осталась в очереди.
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Closed);
        assert!(rig.app.plaque_drawn.is_none() && rig.app.plaques.current().is_some());
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        assert!(rig.app.plaque_drawn.is_some());
    }

    /// Показ экрана, «не беспокоить»: события ждут, а таймер проходит; кончилось — ждавшие встают.
    #[test]
    fn while_notices_are_unwelcome_the_events_wait_and_only_the_timer_shows() {
        let mut rig = Rig::new();
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        rig.app.post(plaque::sample("downloads", Lang::En).unwrap(), Instant::now());
        for _ in 0..2 {
            rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::TimerOnly);
        }
        assert!(rig.app.plaque_drawn.is_none(), "загрузка ждёт");
        rig.app.post(timers::done_plaque(Phase::Work, None), Instant::now());
        for _ in 0..2 {
            rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::TimerOnly);
        }
        assert_eq!(rig.app.plaque_drawn.as_ref().map(|p| p.title.clone()), Some(Say::WorkDone), "таймер прошёл вперёд");
        rig.app.plaques.dismiss();
        for _ in 0..4 {
            rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::TimerOnly);
        }
        assert!(rig.app.plaque_drawn.is_none(), "таймер ушёл, загрузке всё ещё нельзя");
        for _ in 0..2 {
            rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        }
        assert_eq!(rig.app.plaque_drawn.as_ref().map(|p| p.kind), Some(plaque::Kind::Downloads));
    }

    /// Плашке можно быть на экране, только когда панель спрятана совсем: ни выдвинута, ни ещё уезжает (иначе плашка вскочила бы
    /// на место, пока окно ещё принадлежит панели), и на мониторе нет игры во весь экран и программы из «Не выезжать». Пока
    /// пользователю не до уведомлений — только таймеру.
    #[test]
    fn a_plaque_may_show_only_when_the_panel_is_gone_and_nothing_forbids_it() {
        use plaque_ui::may_show;
        assert_eq!(may_show(false, 0.0, false, false, true), Gate::Open);
        assert_eq!(may_show(true, 1.0, false, false, true), Gate::Closed, "панель выдвинута");
        assert_eq!(may_show(false, 0.4, false, false, true), Gate::Closed, "панель ещё уезжает");
        assert_eq!(may_show(false, 0.0, true, false, true), Gate::Closed, "программа из «Не выезжать»");
        assert_eq!(may_show(false, 0.0, false, true, true), Gate::Closed, "игра во весь экран");
        assert_eq!(may_show(false, 0.0, false, false, false), Gate::TimerOnly, "показ экрана, «не беспокоить»");
        assert_eq!(may_show(true, 1.0, false, false, false), Gate::Closed, "и выдвинутая панель важнее");
    }

    /// «Все мониторы»: панель следует за мышью, пока она спрятана совсем и на экране нет плашки (та остаётся на своём мониторе,
    /// а не прыгает за мышью); мышь у края над плашкой — не у края панели.
    #[test]
    fn the_panel_follows_the_mouse_between_monitors_but_the_plaque_stays_put() {
        use plaque_ui::{counts_as_edge, follows_mouse};
        assert!(follows_mouse(true, false, 0.0, false));
        assert!(!follows_mouse(true, false, 0.0, true), "плашка на экране — монитор держим");
        assert!(!follows_mouse(true, true, 1.0, false) && !follows_mouse(true, false, 0.3, false));
        assert!(!follows_mouse(false, false, 0.0, false), "не «Все мониторы»");
        assert!(counts_as_edge(true, false));
        assert!(!counts_as_edge(true, true), "бросок мыши вверх через плашку панель не выдвигает");
        assert!(!counts_as_edge(false, false) && !counts_as_edge(false, true));
    }

    /// Выезд плашки — `LAYOUT` (200 мс), уход — `LEAVE` (120 мс) по шкале набора; пока она уезжает, рисуется прежняя, а следующая
    /// встаёт, только когда окно её убрало. Движение включено (в других тестах оно выключено ради мгновенных кадров).
    #[test]
    fn the_plaque_slides_in_over_layout_and_out_over_leave_and_the_next_waits_for_the_window() {
        let mut rig = Rig::new();
        // Движение — прямо в состоянии набора (`Motion` под ключом `anvil-ui-motion`, как в `anvil_ui::motion`):
        // `set_reduced(false)` не включит его, если анимации выключены в Windows, как на машинах сборки GitHub.
        // Сначала ставится «выключено» — так ключ проверяется на любой машине.
        let key = egui::Id::new("anvil-ui-motion");
        rig.ctx.data_mut(|d| d.insert_temp(key, anvil_ui::motion::Motion { enabled: false }));
        assert!(!anvil_ui::motion::Motion::of(&rig.ctx).enabled, "набор хранит движение под другим ключом");
        rig.ctx.data_mut(|d| d.insert_temp(key, anvil_ui::motion::Motion { enabled: true }));
        rig.app.edge = Edge::default();
        let t0 = Instant::now();
        let mut frame = 0u32;
        let mut step = |rig: &mut Rig| -> f32 {
            // Время кадров идёт дальше от того, где остановился стенд (egui в первом кадре не знает, сколько прошло).
            let now = t0 + Duration::from_millis(33 * u64::from(frame));
            let time = rig.time;
            rig.time += 1.0 / 30.0;
            frame += 1;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PLAQUE)),
                time: Some(time),
                ..Default::default()
            };
            let app = &mut rig.app;
            let mut out = rig.ctx.run_ui(input, |ui| app.plaque_frame(ui.ctx(), now, false, Gate::Open));
            // Рисовать некому: изменения текстур никто не применит (как в `Rig::frame`).
            out.textures_delta.clear();
            rig.app.plaque_t
        };
        // Событий нет — окна у края нет.
        for _ in 0..3 {
            assert_eq!(step(&mut rig), 0.0);
        }
        // Событие: доля растёт плавно до 1 примерно за 200 мс (шесть-семь кадров по 33 мс), а не прыжком.
        rig.app.plaques.push(plaque::sample("usb", Lang::En).unwrap(), t0);
        let rise: Vec<f32> = (0..10).map(|_| step(&mut rig)).collect();
        assert!(rise.windows(2).all(|w| w[0] <= w[1]), "доля не убывает: {rise:?}");
        assert!(rise.iter().any(|&t| t > 0.0 && t < 1.0), "выезд идёт по кадрам, а не прыжком: {rise:?}");
        assert!(rise[..2].iter().all(|&t| t < 1.0) && rise[7..].iter().all(|&t| t == 1.0), "200 мс: {rise:?}");
        assert!(rig.app.plaque_drawn.is_some());
        // Плашка ушла: окно доезжает вверх за 120 мс (три-четыре кадра), пока рисуется прежняя; новая ждёт, пока окно её не уберёт.
        rig.app.plaques.dismiss();
        rig.app.plaques.push(plaque::sample("github", Lang::En).unwrap(), t0);
        let mut fall = Vec::new();
        loop {
            let t = step(&mut rig);
            if t == 0.0 {
                break;
            }
            fall.push(t);
            assert!(fall.len() <= 6, "уход затянулся: {fall:?}");
            assert_eq!(
                rig.app.plaque_drawn.as_ref().map(|p| p.key.as_str()),
                Some("usb:E:\\"),
                "пока уезжает, рисуется прежняя"
            );
            assert!(rig.app.plaques.current().is_none(), "следующая ждёт, пока окно её не убрало");
        }
        assert!(!fall.is_empty() && fall.windows(2).all(|w| w[0] >= w[1]), "уход идёт по кадрам: {fall:?}");
        assert!(fall[0] < 1.0, "{fall:?}");
        assert!(rig.app.plaque_drawn.is_none(), "окно убрало прежнюю");
        // Теперь встала следующая и выезжает.
        let again: Vec<f32> = (0..10).map(|_| step(&mut rig)).collect();
        assert_eq!(rig.app.plaques.current().map(|p| p.key.as_str()), Some("gh:demo/site"));
        assert_eq!(*again.last().unwrap(), 1.0, "{again:?}");
        assert_eq!(rig.app.plaque_drawn.as_ref().map(|p| p.key.as_str()), Some("gh:demo/site"));
    }

    /// Заглушка с событием: отдаёт плашку один раз.
    struct Chime(Option<Plaque>);

    impl Module for Chime {
        fn key(&self) -> &'static str {
            "chime"
        }

        fn title(&self, _t: &texts::Tr) -> &'static str {
            "Chime"
        }

        fn refresh(&mut self, _monitor: &str) {}

        fn ui(&mut self, _ui: &mut Ui, _cx: &mut Ctx) {}

        fn events(&mut self, _settings: &Settings) -> Vec<Plaque> {
            self.0.take().into_iter().collect()
        }
    }

    /// Раньше таймер при выключенной полоске выдвигал панель и брал клавиатуру; теперь — только плашка, и полоска ни при чём.
    #[test]
    fn an_event_never_pulls_the_panel_out_even_with_the_strip_off() {
        let mut rig = Rig::new();
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        rig.app.settings.strip = false;
        rig.app.modules = vec![Box::new(Chime(Some(timers::done_plaque(Phase::Work, None))))];
        let events = rig.app.modules_background();
        assert_eq!(events.len(), 1);
        assert!(rig.app.modules_background().is_empty(), "второй раз то же не отдаётся");
        for plaque in events {
            rig.app.post(plaque, Instant::now());
        }
        assert!(!rig.app.edge.shown && !rig.app.edge.keyboard(), "панель не выехала и клавиатуры не взяла");
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        rig.app.plaque_frame(&rig.ctx, Instant::now(), false, Gate::Open);
        assert_eq!(rig.app.plaque_drawn.as_ref().map(|p| p.title.clone()), Some(Say::WorkDone), "плашка — без полоски");
    }

    /// Окно плашки: размер плашки, по центру верхнего края; выезжает и уезжает за край долей `plaque_t`.
    #[test]
    fn the_plaque_window_is_as_big_as_the_plaque_and_slides_over_the_top_edge() {
        let dev = r"\\.\TEST-PLAQUE-WINDOW";
        let mut rig = Rig::new();
        let rect = monitor::PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
        rig.app.settings.monitor = dev.into();
        rig.app.strip_kind = StripKind::Plaque;
        for (t, y) in [(1.0, 0.0), (0.5, -24.0), (0.0, -48.0)] {
            rig.app.plaque_t = t;
            rig.app.place(&rig.ctx, 1.0, 0.0);
            let (pos, size) = rig.app.placed.expect("окно поставлено");
            assert_eq!((pos, size), (Pos2::new(774.0, y), Vec2::new(372.0, 48.0)), "доля {t}");
        }
        // Нет плашки и нет полоски — окно уведено прочь, а не стоит у края.
        rig.app.strip_kind = StripKind::Off;
        rig.app.place(&rig.ctx, 1.0, 0.0);
        assert!(rig.app.placed.unwrap().0.x > 1920.0);
    }

    /// Полоска у края — в размере из настроек (по умолчанию 120 × 5, как было); наведение на неё выдвигает панель, как край.
    #[test]
    fn the_plain_strip_scales_and_hovering_it_counts_as_the_edge() {
        let dev = r"\\.\TEST-PLAIN-STRIP";
        let mut rig = Rig::new();
        let rect = monitor::PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        rig.app.monitors = vec![monitor::Monitor { device: dev.into(), rect, primary: true, handle: 0 }];
        rig.app.settings.monitor = dev.into();
        rig.app.strip_kind = StripKind::Plain;
        rig.app.place(&rig.ctx, 1.0, 0.0);
        assert_eq!(rig.app.placed, Some((Pos2::new(900.0, 0.0), Vec2::new(120.0, 5.0))), "по умолчанию — как было");
        assert!(rig.app.over_plain_strip((960, 4), 1.0) && !rig.app.over_plain_strip((960, 5), 1.0));
        rig.app.settings.strip_scale = 200;
        rig.app.place(&rig.ctx, 1.0, 0.0);
        assert_eq!(rig.app.placed, Some((Pos2::new(840.0, 0.0), Vec2::new(240.0, 10.0))));
        // Масштаб экрана 150 %: окно в точках, мышь в пикселях.
        assert!(rig.app.over_plain_strip((1300, 14), 1.5) && !rig.app.over_plain_strip((1300, 15), 1.5));
        // Медиа на полоске и плашка — не край: там кнопки.
        rig.app.strip_kind = StripKind::Media;
        assert!(!rig.app.over_plain_strip((960, 4), 1.0));
        rig.app.strip_kind = StripKind::Plain;
        rig.app.edge.shown = true;
        assert!(!rig.app.over_plain_strip((960, 4), 1.0), "выдвинутая панель — уже не полоска");
    }

    /// Настройки края в «Панели»: зона, толщина и размер полоски — чипами; выбранное по умолчанию — прежнее.
    #[test]
    fn the_settings_card_has_edge_zone_rows() {
        for (lang, words) in [
            (Lang::En, ["Edge zone", "Whole edge", "Zone depth", "Strip size"]),
            (Lang::Ru, ["Зона у края", "Весь край", "Толщина зоны", "Размер полоски"]),
        ] {
            let mut rig = Rig::new();
            // Высокое окно — весь столбец «Панель» на виду, без прокрутки.
            rig.app.held = Some(900.0);
            rig.app.settings.common.language = lang;
            rig.app.view = View::Settings;
            rig.settle();
            let text = painted(&rig);
            for word in words {
                assert!(text.contains(word), "нет «{word}»");
            }
        }
    }

    /// «Живые плашки» в «Панели»: подпись и по чипу на вид событий, каждый пишет в настройки; таймера среди них нет.
    #[test]
    fn the_settings_card_has_a_chip_for_every_kind_of_plaque() {
        let mut rig = Rig::new();
        // Выше панель — все чипы на виду (при 340 «Живые плашки» листают колесом).
        rig.app.held = Some(MAX_HEIGHT);
        rig.app.settings.common.language = Lang::En;
        rig.app.view = View::Settings;
        rig.settle();
        let text = painted(&rig);
        // Подпись раздела набор рисует заглавными.
        for word in ["LIVE PLAQUES", "Bluetooth", "USB drives", "GitHub", "Downloads", "Meetings"] {
            assert!(text.contains(word), "нет «{word}»");
        }
        assert!(!text.contains("Timer"), "у таймера чипа нет");
        assert_eq!(rig.app.settings.plaques, config::Plaques::default());
        // Чипы в одном ряду, пока помещаются.
        let (first, last) = (rig.anchor("plaque-chip-bluetooth"), rig.anchor("plaque-chip-downloads"));
        assert!(first.height() == 28.0 && last.height() == 28.0);
        rig.click_anchor("plaque-chip-usb");
        rig.click_anchor("plaque-chip-downloads");
        assert_eq!(
            rig.app.settings.plaques,
            config::Plaques { bluetooth: true, usb: false, github: true, downloads: false, meetings: true }
        );
        // Выключили и включили обратно.
        rig.click_anchor("plaque-chip-usb");
        assert!(rig.app.settings.plaques.usb);
        assert!(rig.app.settings.plaques.on(plaque::Kind::Timer));
        // «Встречи» (2.5) — пятый чип: выключает плашки встреч, и только их.
        assert!(rig.app.settings.plaques.on(plaque::Kind::Meeting));
        rig.click_anchor("plaque-chip-meetings");
        assert!(!rig.app.settings.plaques.on(plaque::Kind::Meeting) && rig.app.settings.plaques.usb);
        assert!(config::format(&rig.app.settings).contains("\nplaques_off=downloads,meetings\n"));
        rig.click_anchor("plaque-chip-meetings");
        assert!(rig.app.settings.plaques.on(plaque::Kind::Meeting));
    }

    /// Чипы переносятся на новую строку, когда в карточке тесно, а не обрезаются и не уходят за край.
    #[test]
    fn the_plaque_chips_wrap_in_a_narrow_card() {
        let mut rig = Rig::sized(930.0);
        rig.app.held = Some(MAX_HEIGHT);
        rig.app.settings.common.language = Lang::Ru;
        rig.app.view = View::Settings;
        rig.settle();
        let chips: Vec<Rect> = ["bluetooth", "usb", "github", "downloads", "meetings"]
            .iter()
            .map(|k| rig.anchor(&format!("plaque-chip-{k}")))
            .collect();
        let card_right = 930.0 / 3.0;
        assert!(chips.iter().all(|c| c.right() <= card_right + 0.5), "чипы в пределах карточки: {chips:?}");
        assert!(chips.iter().any(|c| c.top() > chips[0].top()), "не поместились в ряд — перенеслись: {chips:?}");
    }

    // ─── Встречи (2.5.0) ────────────────────────────────────────────────────
    //
    // Календарь читает поток модуля; в тестах — выдуманный (без сети): «сейчас» 10:18 по UTC, «Планёрка» (Zoom) в 10:30, «Обзор
    // макетов» (Meet) в 14:00, «Созвон по выпуску» (Teams) в 16:30, завтра — «Ретроспектива» без ссылки. Читает его `background`,
    // которого у окна теста нет, — зовём сами.

    const CALENDAR: &str = "https://calendar.example.com/work/demo.ics";
    const ZOOM: &str = "https://us02web.zoom.us/j/123456789";

    impl Rig {
        /// Кадры, пока на экране нет `word`: поток чтения отвечает не сразу.
        fn wait_for(&mut self, word: &str) {
            let start = Instant::now();
            while !painted(self).contains(word) {
                assert!(start.elapsed() < Duration::from_secs(3), "на экране так и нет «{word}»");
                self.app.modules_background();
                self.frame(Vec::new());
                std::thread::sleep(Duration::from_millis(10));
            }
            self.settle();
        }
    }

    /// Панель на полке «День» с выдуманным календарём, дождавшаяся встреч.
    fn day_rig(lang: Lang) -> Rig {
        let mut rig = Rig::new();
        rig.app.settings.common.language = lang;
        rig.app.settings.calendars = vec![CALENDAR.into()];
        rig.app.edge.shown = true;
        rig.app.set_shelf(2);
        rig.wait_for(if lang == Lang::Ru { "Планёрка" } else { "Daily stand-up" });
        rig
    }

    #[test]
    fn the_day_shelf_is_meetings_timers_and_clocks_and_the_first_card_is_wider() {
        let mut rig = Rig::editing();
        rig.app.set_shelf(2);
        rig.settle();
        let (meetings, timers, clocks) = (rig.drop_zone("meetings"), rig.drop_zone("timers"), rig.drop_zone("clocks"));
        assert!(
            meetings.right() < timers.left() && timers.right() < clocks.left(),
            "по порядку: встречи, таймеры, часы"
        );
        assert!((meetings.width() / timers.width() - 1.5).abs() < 0.05, "«Встречи» — полторы доли, как на макете");
        assert!((timers.width() - clocks.width()).abs() < 0.5);
        assert_eq!(rig.app.settings.shelves[2].name, "@day");
    }

    #[test]
    fn the_meetings_card_shows_the_next_meeting_and_the_rest_of_the_day_and_join_opens_the_link() {
        let mut rig = day_rig(Lang::Ru);
        let text = painted(&rig);
        for word in [
            "ВСТРЕЧИ",
            "Календарь «Работа»",
            "Планёрка",
            "Zoom",
            "10:30–10:45 · через 12 мин",
            "Подключиться",
            "ДАЛЬШЕ СЕГОДНЯ",
            "14:00",
            "Обзор макетов",
            "Meet",
            "16:30",
            "Созвон по выпуску",
            "Teams",
        ] {
            assert!(text.contains(word), "нет «{word}» в {text:?}");
        }
        assert!(
            !text.contains("Ретроспектива") && !text.contains("Завтра"),
            "завтра, пока есть сегодня, не показывается"
        );
        assert!(!text.contains("calendar.example.com/work"), "ссылка календаря в окне — только сокращённо");
        // «Подключиться» открывает ссылку, как двойной щелчок; строка со ссылкой — то же.
        rig.click_anchor("meetings-join");
        assert!(launched(ZOOM));
        rig.click_anchor("meetings-row-0");
        assert!(launched("https://meet.google.com/abc-defg-hij"));
        rig.click_anchor("meetings-row-1");
        assert!(launched("https://teams.microsoft.com/l/meetup-join/demo"));
        // Кнопка и строки называют себя для диктора.
        let events = {
            let at = rig.anchor("meetings-row-0").center();
            rig.click_events(at)
        };
        assert!(
            events.iter().any(|e| matches!(e, egui::output::OutputEvent::Clicked(info) if info.label.as_deref() == Some("Обзор макетов, 14:00–15:00"))),
            "{events:?}"
        );
    }

    #[test]
    fn the_meetings_card_in_english_has_no_russian_words() {
        let rig = day_rig(Lang::En);
        let text = painted(&rig);
        for word in [
            "MEETINGS",
            "“Work” calendar",
            "Daily stand-up",
            "10:30–10:45 · in 12 min",
            "Join",
            "LATER TODAY",
            "Mockup review",
            "Release call",
        ] {
            assert!(text.contains(word), "нет «{word}» в {text:?}");
        }
        assert!(
            !text.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)),
            "русские буквы на английском кадре: {text:?}"
        );
    }

    #[test]
    fn without_a_calendar_the_card_says_how_to_connect_and_a_link_is_added_from_the_field() {
        let mut rig = Rig::new();
        rig.app.settings.common.language = Lang::Ru;
        rig.app.edge.shown = true;
        rig.app.set_shelf(2);
        rig.settle();
        let text = painted(&rig);
        for word in [
            "Покажу ближайшие встречи и кнопку «Подключиться» из календаря Google или Outlook по секретной ссылке .ics",
            "Ссылка на календарь",
            "Добавить",
            "Google: Настройки календаря → Секретный адрес в формате iCal",
            "Outlook: Параметры → Календарь → Общие календари → Опубликовать календарь → ICS",
        ] {
            assert!(text.contains(word), "нет «{word}» в {text:?}");
        }
        // Негодная ссылка: слова под полем, календаря нет, «Добавить» не работает.
        rig.click_anchor("meetings-field");
        assert!(rig.app.typing_here(), "пока фокус в поле, Esc отменяет ввод, а не прячет панель");
        rig.type_text("не ссылка");
        assert!(painted(&rig).contains("Нужна ссылка вида https://… или webcal://…"));
        rig.click_anchor("meetings-add");
        assert!(rig.app.settings.calendars.is_empty());
        // Esc в поле стирает набранное и не прячет панель.
        rig.click_anchor("meetings-field");
        rig.key(Key::Escape, NO_KEYS);
        assert!(rig.app.edge.shown, "Esc в поле спрятал панель");
        assert!(!painted(&rig).contains("Нужна ссылка вида"), "Esc не стёр набранное");
        assert!(!rig.app.typing_here(), "ввод кончен: следующий Esc спрячет панель, как обычно");
        // Годная ссылка: набранное видно точками, а под полем — сайт и конец; «Добавить» кладёт её в настройки.
        rig.click_anchor("meetings-field");
        rig.type_text("webcal://calendar.example.com/ical/me%40example.com/private-SECRET0123/basic.ics");
        let text = painted(&rig);
        assert!(text.contains("calendar.example.com/…/basic.ics"), "{text:?}");
        assert!(!text.contains("SECRET") && !text.contains("private"), "секрет в окне не виден: {text:?}");
        rig.click_anchor("meetings-add");
        assert_eq!(
            rig.app.settings.calendars,
            ["https://calendar.example.com/ical/me%40example.com/private-SECRET0123/basic.ics"],
            "webcal:// читается как https://"
        );
        assert!(!painted(&rig).contains("Нужна ссылка"), "поле очищено");
        assert_eq!(
            modules::clipboard::forgotten(),
            ["webcal://calendar.example.com/ical/me%40example.com/private-SECRET0123/basic.ics"],
            "добавленная ссылка уходит из истории «Буфера обмена» (как её скопировали)"
        );
        let saved = config::format(&rig.app.settings);
        assert!(saved.contains("\ncalendar=https://calendar.example.com/ical/"), "ссылка уходит в config.txt");
    }

    #[test]
    fn the_same_calendar_is_not_added_twice_and_a_added_link_leaves_the_clipboard_history() {
        let mut rig = day_rig(Lang::Ru);
        rig.click_anchor("meetings-calendars");
        rig.click_anchor("meetings-field");
        rig.type_text(CALENDAR);
        let text = painted(&rig);
        assert!(text.contains("Этот календарь уже есть"), "{text:?}");
        rig.click_anchor("meetings-add");
        assert_eq!(rig.app.settings.calendars, [CALENDAR], "второй раз та же ссылка не добавляется");
        assert!(modules::clipboard::forgotten().is_empty(), "ничего не добавлено — из истории ничего не убрано");
        // Другая ссылка: слов про повтор нет, «Добавить» работает, а скопированная ссылка уходит из истории буфера.
        rig.click_anchor("meetings-field");
        rig.key(Key::Escape, NO_KEYS);
        rig.click_anchor("meetings-field");
        let other = "https://calendar.example.com/home/other.ics";
        rig.type_text(other);
        assert!(!painted(&rig).contains("Этот календарь уже есть"));
        rig.click_anchor("meetings-add");
        assert_eq!(rig.app.settings.calendars.len(), 2);
        assert_eq!(modules::clipboard::forgotten(), [other]);
    }

    #[test]
    fn the_calendars_button_and_a_right_click_open_the_list_where_a_calendar_is_removed() {
        let mut rig = day_rig(Lang::Ru);
        rig.click_anchor("meetings-calendars");
        let text = painted(&rig);
        for word in ["КАЛЕНДАРИ", "Работа", "calendar.example.com/…/demo.ics", "Готово", "ДОБАВИТЬ КАЛЕНДАРЬ"]
        {
            assert!(text.contains(word), "нет «{word}» в {text:?}");
        }
        assert!(!text.contains("Планёрка"), "вместо встреч — список календарей");
        // «Готово» возвращает встречи.
        let done = rig.app.settings.calendars.clone();
        rig.click_anchor("meetings-remove-0");
        assert!(rig.app.settings.calendars.is_empty() && done.len() == 1, "крестик убрал календарь");
        let text = painted(&rig);
        assert!(text.contains("Покажу ближайшие встречи"), "календарей нет — как подключить: {text:?}");
        // Правый щелчок по карточке — то же, что значок.
        let mut rig = day_rig(Lang::Ru);
        let at = rig.anchor("meetings-plate").center();
        rig.right_click(at);
        assert!(painted(&rig).contains("КАЛЕНДАРИ") && !painted(&rig).contains("Планёрка"));
        rig.right_click(Pos2::new(60.0, BODY_TOP + 200.0));
        assert!(painted(&rig).contains("Планёрка"), "второй правый щелчок возвращает встречи");
    }

    #[test]
    fn the_plaque_of_a_meeting_has_words_a_join_button_and_a_cross_in_both_languages() {
        // Часы слов плашки стоят: «через 2 мин» не зависит от того, как быстро нарисован кадр.
        texts::set_clock(Some(1_800_000_000));
        for (lang, words) in [
            (Lang::Ru, ["Планёрка через 2 мин", "10:30 · Zoom", "Подключиться"]),
            (Lang::En, ["Daily stand-up in 2 min", "10:30 · Zoom", "Join"]),
        ] {
            let mut rig = plaque_rig(plaque::sample("meeting", lang).unwrap());
            rig.app.settings.common.language = lang;
            rig.settle();
            let text = painted(&rig);
            for word in words {
                assert!(text.contains(word), "{lang:?}: нет «{word}» в {text:?}");
            }
            // Одна главная кнопка и крестик (плашка висит, пока не закроют или не начнётся встреча); полоски «сколько осталось» нет.
            let (button, cross) = (rig.anchor("plaque-button-0"), rig.anchor("plaque-close"));
            assert!(button.right() < cross.left() && button.height() == 28.0);
            // «Подключиться» открывает ссылку и убирает плашку, панель не выезжает.
            rig.click_anchor("plaque-button-0");
            assert!(launched(ZOOM));
            assert!(rig.app.plaques.current().is_none() && !rig.app.edge.shown);
        }
        // Крестик убирает, ничего не открывая.
        let mut rig = plaque_rig(plaque::sample("meeting", Lang::En).unwrap());
        rig.click_anchor("plaque-close");
        assert!(rig.app.plaques.current().is_none());
        // Щелчок по телу открывает панель на полке «День» (там карточка «Встречи»).
        let mut rig = plaque_rig(plaque::sample("meeting", Lang::En).unwrap());
        rig.click(Pos2::new(120.0, 24.0));
        assert!(rig.app.edge.shown);
        assert_eq!(rig.app.shelf, 2);
    }

    #[test]
    fn a_meeting_plaque_opens_only_a_link_that_passes_the_check_again() {
        texts::set_clock(Some(1_800_000_000));
        // Подмена в памяти: не служба — не открывается, плашка остаётся.
        let evil = "https://evil.example/j/1";
        let mut p = plaque::sample("meeting", Lang::En).unwrap();
        p.buttons[0].does = plaque::Do::Open(evil.into());
        let mut rig = plaque_rig(p);
        rig.click_anchor("plaque-button-0");
        assert!(!launched(evil));
        assert!(rig.app.plaques.current().is_some(), "плашка ничего не открыла и не ушла");
        // Служба — открывается, и плашка уходит.
        let mut rig = plaque_rig(plaque::sample("meeting", Lang::En).unwrap());
        rig.click_anchor("plaque-button-0");
        assert!(launched(ZOOM));
        assert!(rig.app.plaques.current().is_none());
    }

    #[test]
    fn with_the_panel_open_a_meeting_is_told_by_a_notice_not_a_plaque() {
        texts::set_clock(Some(1_800_000_000));
        let mut rig = Rig::new();
        rig.app.settings.common.language = Lang::En;
        rig.app.edge.shown = true;
        rig.app.post(plaque::sample("meeting", Lang::En).unwrap(), Instant::now());
        assert!(rig.app.plaques.current().is_none() && !rig.app.plaques.has_any());
        let told: Vec<String> =
            rig.app.toasts.since(Instant::now() - Duration::from_secs(5)).into_iter().map(|(t, _)| t).collect();
        assert!(told.iter().any(|t| t == "Daily stand-up in 2 min · 10:30 · Zoom"), "{told:?}");
    }

    #[test]
    fn search_finds_join_for_the_nearest_meetings_and_enter_opens_the_link() {
        let _world = world();
        let mut rig = search_rig(r"\\.\TEST-MEETINGS-SEARCH", Lang::Ru);
        rig.app.settings.calendars = vec![CALENDAR.into()];
        let start = Instant::now();
        while !rig.app.all_actions().iter().any(|a| a.id.starts_with("meetings:join:")) {
            assert!(start.elapsed() < Duration::from_secs(3), "календарь не прочитался");
            rig.app.modules_background();
            rig.frame(Vec::new());
            std::thread::sleep(Duration::from_millis(10));
        }
        rig.key(Key::K, CTRL);
        rig.type_text("планёрка");
        assert_eq!(titles(&rig)[0], "Подключиться к встрече «Планёрка»");
        assert_eq!(rig.app.found().hits[0].sub, "Встречи · День");
        rig.key(Key::Enter, NO_KEYS);
        assert!(launched(ZOOM));
        // По слову «созвон» находятся и встреча, и «Подключиться» — а «Ретроспектива» без ссылки не предлагается.
        rig.key(Key::K, CTRL);
        rig.type_text("ретроспектива");
        assert!(!titles(&rig).iter().any(|t| t.contains("Ретроспектива")));
    }

    #[test]
    fn a_meeting_plaque_waits_for_the_screen_to_be_free_and_the_panel_to_be_hidden() {
        let mut rig = Rig::new();
        anvil_ui::motion::set_reduced(&rig.ctx, true);
        rig.app.edge = Edge::default();
        rig.app.drawn = 0.0;
        let t0 = Instant::now();
        rig.app.post(plaque::sample("meeting", Lang::En).unwrap(), t0);
        // Показ экрана или игра: плашка с названием встречи не всплывает (таймеру можно, ей нет).
        rig.app.plaque_frame(&rig.ctx, t0, false, Gate::TimerOnly);
        assert!(rig.app.plaque_drawn.is_none() && rig.app.plaques.has_any());
        rig.app.plaque_frame(&rig.ctx, t0 + Duration::from_millis(100), false, Gate::Open);
        assert!(rig.app.plaque_drawn.is_some());
    }
}
