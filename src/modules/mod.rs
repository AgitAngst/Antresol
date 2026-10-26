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
pub mod quick;
pub mod resources;
pub mod sound;
pub mod timers;
pub mod tools;
pub mod tray;

use anvil_ui::{Icon, Tone};
use eframe::egui::{self, Ui};

use crate::config::{Profile, SceneRow};
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
    /// Каждый кадр панели, даже спрятанной: дешёвая слежка (номер копирования буфера).
    fn background(&mut self, _settings: &crate::config::Settings) {}
    /// Программа закрывается (кнопка «Закрыть», Alt+F4, выход из сеанса): вернуть то, что держали на время, —
    /// «Экран» возвращает режим, который не успели подтвердить.
    fn shutdown(&mut self) {}
    /// Модулю есть что сказать (таймер дошёл): панель покажет уведомление и выедет.
    fn attention(&mut self, _t: &Tr) -> Option<String> {
        None
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
    /// Что играет — для спрятанной полоски («Медиа на спрятанной полоске»).
    fn playing(&mut self, _ctx: &egui::Context) -> Option<Playing> {
        None
    }
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
pub const DEFAULT_SHELVES: [(&str, &[&str]); 3] = [
    ("@av", &["display", "brightness", "sound", "media"]),
    ("@sys", &["quick", "resources", "bluetooth", "github", "clocks"]),
    ("@hand", &["clipboard", "tray", "tools", "timers", "buttons"]),
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
}
