// Модули панели: каждый — карточка со своими быстрыми настройками. Новый модуль — файл здесь,
// `impl Module` и строка в `all()`; остальное (раскладка, появление, обновление) панель делает сама.

pub mod bluetooth;
pub mod brightness;
pub mod buttons;
pub mod clipboard;
pub mod clocks;
pub mod display;
pub mod github;
pub mod media;
pub mod meetings;
pub mod quick;
pub mod resources;
pub mod sound;
pub mod timers;
pub mod tools;
pub mod tray;

use anvil_ui::{Icon, Tone};
use eframe::egui::{self, Ui};

use crate::config::{Profile, SceneRow, Settings};
use crate::plaque::Plaque;
use crate::texts::Tr;

/// Что модуль получает от панели на кадр.
pub struct Ctx<'a> {
    pub t: &'static Tr,
    pub lang: anvil_ui::Lang,
    /// Монитор, у которого живёт панель: `\\.\DISPLAYn`.
    pub monitor: &'a str,
    /// Сообщения внизу экрана: ошибки и «готово».
    pub toasts: &'a mut Vec<(String, Tone)>,
    /// Настройки программы: модуль может хранить в них своё (избранное, частоты).
    /// Изменил — панель сама сохранит файл.
    pub settings: &'a mut crate::config::Settings,
}

/// Команда модулю извне карточки: например, клавиша сцены нажата при спрятанной панели. Сцена — номер в
/// `settings.profiles`; каждый модуль делает свою часть, остальное пропускает.
pub enum Command {
    /// Поставить сцену: что она отметила у этого модуля. Перед тем модуль запоминает, что было (`before`), — на возврат.
    ApplyScene(usize),
    /// Вернуть то, что поменяла сцена, как было до неё. Одноразово: после возврата модуль `before` забывает.
    UndoScene(usize),
    /// Взять текущие значения этих строк в сцену (экран и система, как стоят сейчас): «+», «Запомнить текущие», галочка.
    SaveScene(usize, Vec<SceneRow>),
    /// Файлы бросили на панель (полные пути).
    Dropped(Vec<String>),
    /// Кнопка плеера на спрятанной полоске.
    Media(crate::sys::media::Action),
    /// Выполнить действие из поиска (`Action::id`): его берёт модуль, чей это ключ (`<ключ модуля>:…`), остальные пропускают.
    /// Делает то же, что щелчок по этому месту в карточке: те же функции, те же уведомления и «Оставить?».
    RunAction(String),
}

/// Действие, которое можно найти и выполнить с клавиатуры (поиск в шапке): то же, что щелчок в карточке.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// `<ключ модуля>:<что>` — по нему модуль узнаёт своё действие в `Command::RunAction`.
    pub id: String,
    /// Что написано в строке: «Ночной свет: включить».
    pub title: String,
    /// Где это в панели — карточка; полку допишет панель («Яркость · Экран и звук»).
    pub place: String,
    pub icon: Icon,
    /// Слова для поиска, кроме названия: оба языка сразу (по «night» находится «Ночной свет» и при русском интерфейсе).
    pub words: String,
    /// Можно ли сейчас. Нельзя — строка серая, с причиной, и Enter её не выполняет.
    pub enabled: bool,
    /// Почему нельзя (вторая строка серой строки).
    pub reason: Option<String>,
    /// Действию нужна мышь вне панели («Пипетка» берёт цвет под ней): пока мышь над панелью, оно серое.
    pub needs_free_pointer: bool,
}

impl Action {
    pub fn new(id: impl Into<String>, title: impl Into<String>, place: impl Into<String>, icon: Icon) -> Action {
        Action {
            id: id.into(),
            title: title.into(),
            place: place.into(),
            icon,
            words: String::new(),
            enabled: true,
            reason: None,
            needs_free_pointer: false,
        }
    }

    /// Слова для поиска (оба языка), через пробел.
    pub fn words(mut self, words: &str) -> Action {
        self.words = words.to_string();
        self
    }

    /// Недоступно сейчас: серая строка с причиной.
    pub fn off(mut self, reason: &str) -> Action {
        self.enabled = false;
        self.reason = Some(reason.to_string());
        self
    }

    /// Недоступно, если `cond`.
    pub fn off_if(self, cond: bool, reason: &str) -> Action {
        if cond { self.off(reason) } else { self }
    }

    /// Нужна мышь вне панели.
    pub fn free_pointer(mut self) -> Action {
        self.needs_free_pointer = true;
        self
    }
}

/// Остаток `id` после ключа модуля: `mine("quick", "quick:wifi")` — `Some("wifi")`. Чужое действие — `None`.
pub fn mine<'a>(key: &str, id: &'a str) -> Option<&'a str> {
    id.strip_prefix(key)?.strip_prefix(':')
}

/// Устойчивый номер для `Action::id` там, где у строки нет своего имени (закреплённый текст, своя кнопка): отпечаток
/// того, что в строке. Не номер в списке: убрали или добавили строку — «Недавние» поиска не сползают на другие. Одно и то
/// же в одной работе программы (в файл не пишется).
pub fn stable_key(parts: &[&str]) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Что играет — для спрятанной полоски.
#[derive(Clone)]
pub struct Playing {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub cover: Option<egui::TextureHandle>,
}

pub trait Module {
    /// Ключ для `Id` (появление карточки).
    fn key(&self) -> &'static str;
    fn title(&self, t: &Tr) -> &'static str;
    /// Доля ширины панели относительно соседей.
    fn weight(&self) -> f32 {
        1.0
    }
    /// Уже этого карточка не бывает: панель уже — ряд карточек листается вбок.
    fn min_width(&self) -> f32 {
        220.0
    }
    /// Перечитать состояние системы: при появлении панели и раз в 1,5 с, пока она открыта.
    fn refresh(&mut self, monitor: &str);
    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx);
    /// Панели нельзя прятаться: модуль ждёт ответа пользователя.
    fn busy(&self) -> bool {
        false
    }
    /// В карточке вводят текст: Esc отменяет ввод, а не прячет панель.
    fn typing(&self) -> bool {
        false
    }
    /// Модулю нужен кадр, даже если мышь стоит (обратный отсчёт).
    fn ticking(&self) -> bool {
        false
    }
    fn command(&mut self, _cmd: &Command, _cx: &mut Ctx) {}
    /// Что из этой карточки можно найти и выполнить из поиска: каждое — то же, что щелчок в карточке (`Command::RunAction`).
    /// Только по уже прочитанному (`refresh`), без обращений к Windows: список строится каждый кадр, пока поиск открыт.
    /// Недоступное отдаётся серым, с причиной; чего у системы нет совсем (нет VPN) — не отдаётся. У модуля без действий —
    /// пустой список.
    fn actions(&self, _t: &Tr, _lang: anvil_ui::Lang, _settings: &Settings) -> Vec<Action> {
        Vec::new()
    }
    /// Каждый кадр панели, даже спрятанной: дешёвая слежка (номер копирования буфера).
    fn background(&mut self, _settings: &crate::config::Settings) {}
    /// Программа закрывается (кнопка «Закрыть», Alt+F4, выход из сеанса): вернуть то, что держали на время, —
    /// «Экран» возвращает режим, который не успели подтвердить.
    fn shutdown(&mut self) {}
    /// Что случилось у модуля, пока никто не смотрел (таймер дошёл, наушники подключились, сборка упала): плашки у края
    /// (панель не выезжает; выдвинута — слова скажут уведомления). Забирает: второй раз то же не повторяется. Какие события
    /// включены — `Settings::plaques` (таймер — всегда).
    fn events(&mut self, _settings: &Settings) -> Vec<Plaque> {
        Vec::new()
    }
    /// Пояснение справа в заголовке карточки: монитор, программа плеера.
    fn meta(&self, _t: &Tr) -> String {
        String::new()
    }
    /// Подсказка к пояснению: то, что в заголовок не поместилось (имя устройства монитора).
    fn meta_hint(&self) -> String {
        String::new()
    }
    /// Короткая живая метка на вкладке полки, пока открыта другая: текст и подсказка (таймер — сколько осталось).
    fn badge(&self, _t: &Tr, _lang: anvil_ui::Lang) -> Option<(String, String)> {
        None
    }
    /// Совпадает ли то, что сцена отметила у этого модуля, с тем, что стоит сейчас (экран — режим, масштаб и HDR; система —
    /// каждая прочитанная настройка). `None` — модулю нечего проверять: в сцене нет его частей или их не прочитать.
    /// Сцена «стоит», когда есть хоть один ответ и все ответы «да» (шапка выделяет её чип).
    fn scene_part(&self, _scene: &Profile) -> Option<bool> {
        None
    }
    /// Помнит ли модуль, что было до этой сцены: есть что вернуть.
    fn scene_can_undo(&self, _scene: &Profile) -> bool {
        false
    }
    /// Какие строки сцены умеет ставить этот модуль и можно ли их сейчас (нет DDC/CI, нет Wi‑Fi — нельзя).
    fn scene_rows(&self) -> Vec<(SceneRow, bool)> {
        Vec::new()
    }
    /// Сцену пока не ставят: модуль ждёт ответа («Оставить этот режим?» у «Экрана»).
    fn blocks_scene(&self) -> bool {
        false
    }
    /// Сцену `scene` пока не возвращают: модуль ждёт ответа не от неё (чужое «Оставить этот режим?»). Возврат тогда не
    /// начинают ни у кого, иначе сцена вернулась бы наполовину.
    fn blocks_undo(&self, _scene: &Profile) -> bool {
        false
    }
    /// Пользователь не оставил режим, который поставила сцена («Вернуть», 15 секунд без ответа, закрытие программы): это
    /// отказ от сцены целиком. Отдаёт номер сцены ([`Profile::id`]) один раз — панель вернёт остальное из неё.
    fn take_refused_scene(&mut self) -> Option<u64> {
        None
    }
    /// Что не вышло в потоке модуля (Wi‑Fi не переключился, монитор не ответил по DDC/CI): слова программы. Панель говорит
    /// их уведомлением, даже когда карточку не рисуют. Забирает: второй раз то же не повторяется.
    fn problems(&mut self, _t: &Tr) -> Vec<(String, Tone)> {
        Vec::new()
    }
    /// Что играет — для спрятанной полоски («Медиа на спрятанной полоске»).
    fn playing(&mut self, _ctx: &egui::Context) -> Option<Playing> {
        None
    }
}

/// Свой `Id` у чужого виджета: тест без окна находит его через `Context::read_response`. Щелчков не ловит (`hover`).
pub fn anchor(ui: &Ui, rect: egui::Rect, key: &str) {
    ui.interact(rect, egui::Id::new(("anchor", key)), egui::Sense::hover());
}

/// Куда карточка пишет, сколько её тела не влезло в прокрутку (`take_overflow` читает и стирает).
fn overflow_id() -> egui::Id {
    egui::Id::new("card-overflow")
}

/// Сколько содержимого вертикальной прокрутки тела карточки не влезло (меньше нуля — осталось лишнее место): по этому панель
/// подбирает высоту полки. Зовётся после каждой такой прокрутки; из нескольких в одной карточке берётся наибольшее.
pub fn note_overflow<R>(ui: &Ui, out: &egui::scroll_area::ScrollAreaOutput<R>) {
    let over = out.content_size.y - out.inner_rect.height();
    ui.ctx().data_mut(|d| {
        let v = d.get_temp_mut_or(overflow_id(), f32::NEG_INFINITY);
        *v = v.max(over);
    });
}

/// Прокрутка тела карточки, что сама говорит панели, сколько не влезло (`note_overflow`).
pub trait ShowNoted {
    fn show_noted<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::scroll_area::ScrollAreaOutput<R>;
}

impl ShowNoted for egui::ScrollArea {
    fn show_noted<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::scroll_area::ScrollAreaOutput<R> {
        let out = self.show(ui, add);
        note_overflow(ui, &out);
        out
    }
}

/// Тело карточки без прокрутки, что занимает место до низа (раскладка `bottom_up`): сколько в нём осталось пустого места —
/// иначе панель решила бы, что карточке нужна вся высота, и полка не стала бы ниже.
pub fn note_spare(ui: &Ui) {
    let over = -ui.available_height().max(0.0);
    ui.ctx().data_mut(|d| {
        let v = d.get_temp_mut_or(overflow_id(), f32::NEG_INFINITY);
        *v = v.max(over);
    });
}

/// Колесо над ползунком (`rect` — его строка): шаг значения. Пока мышь над строкой, колесо целиком её — ни список карточки,
/// ни ряд карточек его не получают (egui раздаёт плавное колесо на несколько кадров, поэтому гасим каждый кадр).
pub fn slider_wheel(ui: &Ui, rect: egui::Rect) -> f32 {
    if !ui.is_enabled() || !ui.rect_contains_pointer(rect) {
        return 0.0;
    }
    ui.input_mut(|i| {
        i.smooth_scroll_delta = egui::Vec2::ZERO;
        i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum()
    })
}

/// Что записали `note_overflow` с прошлого вызова; не было прокрутки — `None`.
pub fn take_overflow(ctx: &egui::Context) -> Option<f32> {
    ctx.data_mut(|d| d.remove_temp::<f32>(overflow_id())).filter(|v| v.is_finite())
}

/// Значок «наружу» (стрелка из квадрата) 12 × 12 — кнопка открывает сайт или другую программу. В kit-v0.6.1 его нет (появился в
/// kit-v0.8.0 как `Icon::External`): при переходе на новый набор заменить этот рисунок на значок набора.
pub fn paint_external(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let scale = rect.width() / 24.0;
    let origin = rect.center() - egui::Vec2::splat(12.0) * scale;
    let at = |x: f32, y: f32| origin + egui::Vec2::new(x, y) * scale;
    let stroke = egui::Stroke::new((1.7 * scale * 1.3).max(1.2), color);
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| -> Vec<egui::Pos2> {
        (0..=4)
            .map(|i| {
                let a = from + (to - from) * i as f32 / 4.0;
                at(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    painter.add(egui::Shape::line(vec![at(15.0, 3.0), at(21.0, 3.0), at(21.0, 9.0)], stroke));
    painter.add(egui::Shape::line(vec![at(10.0, 14.0), at(21.0, 3.0)], stroke));
    let mut p = vec![at(18.0, 13.0), at(18.0, 19.0)];
    p.extend(arc(16.0, 19.0, 2.0, 0.0, FRAC_PI_2));
    p.push(at(5.0, 21.0));
    p.extend(arc(5.0, 19.0, 2.0, FRAC_PI_2, PI));
    p.push(at(3.0, 8.0));
    p.extend(arc(5.0, 8.0, 2.0, PI, PI + FRAC_PI_2));
    p.push(at(11.0, 6.0));
    painter.add(egui::Shape::line(p, stroke));
}

/// Значок карточки в заголовке — по ключу модуля.
pub fn icon_of(key: &str) -> Icon {
    match key {
        "display" => Icon::Monitor,
        "brightness" => Icon::Sun,
        "media" => Icon::Play,
        "resources" => Icon::Server,
        "clipboard" => Icon::Copy,
        "tray" => Icon::Folder,
        "bluetooth" => Icon::Broadcast,
        "timers" => Icon::Clock,
        "github" => Icon::Branch,
        "quick" => Icon::Tiles,
        "tools" => Icon::Target,
        "clocks" => Icon::Clock,
        // У «Встреч» значок свой (календаря в наборе нет): `card_header` рисует `meetings::paint_calendar`; здесь — запасной.
        "meetings" => Icon::Clock,
        _ => Icon::Rocket,
    }
}

/// Заголовок карточки: значок, подпись заглавными и пояснение справа — всё цветом `weak`; `hint` — подсказка к
/// пояснению. У «Звука» значок свой — динамик (в наборе такого нет).
pub fn card_header(ui: &mut Ui, key: &str, title: &str, meta: &str, hint: &str) {
    let p = anvil_ui::Palette::of(ui);
    // Строка заголовка 20 высотой, а не 28 («интерактивная» по умолчанию): карточкам нужна каждая точка высоты.
    let interact = ui.spacing().interact_size.y;
    ui.spacing_mut().interact_size.y = 20.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        if key == "sound" {
            sound::paint_speaker(ui.painter(), rect, false, p.weak);
        } else if key == "meetings" {
            meetings::paint_calendar(ui.painter(), rect, p.weak);
        } else {
            anvil_ui::icons::paint(ui.painter(), rect, icon_of(key), p.weak);
        }
        anvil_ui::widgets::section_label(ui, title);
        if !meta.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut job =
                    egui::text::LayoutJob::simple_singleline(meta.to_owned(), egui::FontId::proportional(12.0), p.weak);
                job.wrap = egui::text::TextWrapping::truncate_at_width(ui.available_width().max(20.0));
                let r = ui.label(job);
                if !hint.is_empty() {
                    r.on_hover_text(hint);
                }
            });
        }
    });
    ui.spacing_mut().interact_size.y = interact;
}

/// Полки по умолчанию: ключ имени (`@…` — готовое, переводится при показе) и карточки по порядку. Здесь же —
/// полный список знакомых панели карточек: чего тут нет, того на полках не бывает.
pub const DEFAULT_SHELVES: [(&str, &[&str]); 4] = [
    ("@av", &["display", "brightness", "sound", "media"]),
    ("@sys", &["quick", "resources", "bluetooth", "github"]),
    ("@day", &["meetings", "timers", "clocks"]),
    ("@hand", &["clipboard", "tray", "tools", "buttons"]),
];

pub fn all() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(display::Display::default()),
        Box::new(quick::Quick::default()),
        Box::new(brightness::Brightness::default()),
        Box::new(media::Media::default()),
        Box::new(sound::Sound::default()),
        Box::new(resources::Resources::default()),
        Box::new(clipboard::Clipboard::default()),
        Box::new(tray::Tray::default()),
        Box::new(tools::Tools::default()),
        Box::new(bluetooth::Bluetooth::default()),
        Box::new(timers::Timers::default()),
        Box::new(clocks::Clocks::default()),
        Box::new(meetings::Meetings::default()),
        Box::new(github::Github::default()),
        Box::new(buttons::Buttons::default()),
    ]
}

/// Строка списка с текстом слева и пометкой справа; выбранная — с полоской акцента.
/// `reserve` — место справа под свой значок строки (звёздочку), пометка встаёт левее.
pub fn row(ui: &mut Ui, id: egui::Id, selected: bool, label: &str, trailing: &str, reserve: f32) -> egui::Response {
    let p = anvil_ui::Palette::of(ui);
    let r = anvil_ui::widgets::list_row(ui, id, selected, 28.0, label);
    let font = egui::FontId::proportional(13.5);
    let color = if selected { p.text } else { p.weak };
    let mut room = r.rect.width() - 22.0 - reserve;
    if !trailing.is_empty() {
        let right = r.rect.right_center() - egui::vec2(10.0 + reserve, 0.0);
        // Пометка — читаемый текст: `weak`, а не `faint` (тот — для недоступного и рисунков).
        let g =
            ui.painter().text(right, egui::Align2::RIGHT_CENTER, trailing, egui::FontId::proportional(12.0), p.weak);
        room -= g.width() + 8.0;
    }
    // Длинное имя — с многоточием, а не за край карточки.
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room.max(20.0));
    let galley = ui.painter().layout_job(job);
    let left = r.rect.left_center() + egui::vec2(12.0, -galley.size().y / 2.0);
    ui.painter().galley(left, galley, color);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Выбор одного из нескольких — «чипами», которые переносятся на новую строку, когда не влезают
/// (у `segmented` набора полоса одна и обрезается). Выбранный — мягкая подложка акцента с рамкой
/// `accent` и полужирным `accent_text`; остальные — `raised`, текст `weak`.
pub fn chips<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (i, (option, label)) in options.iter().enumerate() {
            let on = *value == *option;
            if chip(ui, ("chip", i, *label), on, label, None).clicked() && !on {
                *value = *option;
                changed = true;
            }
        }
    });
    changed
}

/// Те же чипы сеткой: `columns` равных колонок на всю ширину.
pub fn chip_grid<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], columns: usize) -> bool {
    let mut changed = false;
    let gap = 4.0;
    let width = ((ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32).floor();
    for (r, row) in options.chunks(columns.max(1)).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            for (c, (option, label)) in row.iter().enumerate() {
                let on = *value == *option;
                if chip(ui, ("grid", r, c, *label), on, label, Some(width)).clicked() && !on {
                    *value = *option;
                    changed = true;
                }
            }
        });
    }
    changed
}

/// Один чип: высота 28; `width` — своя ширина (иначе по тексту).
pub fn chip(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    on: bool,
    label: &str,
    width: Option<f32>,
) -> egui::Response {
    let p = anvil_ui::Palette::of(ui);
    let motion = anvil_ui::motion::Motion::of(ui.ctx());
    let font = if on { anvil_ui::semibold(13.0) } else { egui::FontId::proportional(13.0) };
    let text_w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), p.text).size().x;
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width.unwrap_or(text_w + 20.0), 28.0), egui::Sense::click());
    let hovered = r.hovered();
    let fill = if on {
        p.badge_fill(p.accent)
    } else if hovered {
        p.hover
    } else {
        p.raised
    };
    let id = ui.id().with(salt);
    let fill = motion.color(ui.ctx(), id.with("fill"), fill, anvil_ui::motion::HOVER);
    let stroke = if on { p.accent } else { p.border };
    ui.painter().rect(rect, 6, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let color = if on {
        p.accent_text
    } else if hovered {
        p.text
    } else {
        p.weak
    };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, font, color);
    anvil_ui::widgets::focus_ring(ui, rect, &r, 6);
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, label));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shelves_cover_all_modules() {
        let mut in_shelves: Vec<&str> = DEFAULT_SHELVES.iter().flat_map(|(_, cards)| cards.iter().copied()).collect();
        let mut keys: Vec<&'static str> = all().iter().map(|m| m.key()).collect();
        in_shelves.sort_unstable();
        keys.sort_unstable();
        assert_eq!(in_shelves, keys, "каждый модуль — на полке по умолчанию, и наоборот");
        let mut unique = keys.clone();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "ключи не повторяются");
    }

    /// Колесо над строкой ползунка — его: плавное колесо гасится, ни список, ни ряд его не получают; мимо строки — не трогает.
    #[test]
    fn the_slider_takes_the_whole_wheel() {
        let ctx = egui::Context::default();
        let row = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 28.0));
        let wheel = |at: egui::Pos2| egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(at),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -1.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        for (at, mine) in [(row.center(), true), (egui::pos2(400.0, 200.0), false)] {
            let mut step = 0.0;
            let mut left = egui::Vec2::ZERO;
            let mut out = ctx.run_ui(wheel(at), |ui| {
                step = slider_wheel(ui, row);
                left = ui.input(|i| i.smooth_scroll_delta());
            });
            out.textures_delta.clear();
            if mine {
                assert_eq!((step, left), (-1.0, egui::Vec2::ZERO));
            } else {
                assert_eq!(step, 0.0);
                assert_ne!(left, egui::Vec2::ZERO, "мимо строки колесо остаётся другим");
            }
        }
    }
}
