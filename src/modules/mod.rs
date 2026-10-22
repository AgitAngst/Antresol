// Модули панели: каждый — карточка со своими быстрыми настройками. Новый модуль — файл здесь,
// `impl Module` и строка в `all()`; остальное (раскладка, появление, обновление) панель делает сама.

pub mod brightness;
pub mod buttons;
pub mod display;

use anvil_ui::Tone;
use eframe::egui::{self, Ui};

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

/// Команда модулю извне карточки: например, клавиша профиля нажата при спрятанной панели.
pub enum Command {
    ApplyProfile(usize),
}

pub trait Module {
    /// Ключ для `Id` (появление карточки).
    fn key(&self) -> &'static str;
    fn title(&self, t: &Tr) -> &'static str;
    /// Доля ширины панели относительно соседей.
    fn weight(&self) -> f32 {
        1.0
    }
    /// Перечитать состояние системы: при появлении панели и раз в 1,5 с, пока она открыта.
    fn refresh(&mut self, monitor: &str);
    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx);
    /// Панели нельзя прятаться: модуль ждёт ответа пользователя.
    fn busy(&self) -> bool {
        false
    }
    /// Модулю нужен кадр, даже если мышь стоит (обратный отсчёт).
    fn ticking(&self) -> bool {
        false
    }
    fn command(&mut self, _cmd: &Command, _cx: &mut Ctx) {}
}

pub fn all() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(display::Display::default()),
        Box::new(brightness::Brightness::default()),
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
        let g =
            ui.painter().text(right, egui::Align2::RIGHT_CENTER, trailing, egui::FontId::proportional(12.0), p.faint);
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
/// (у `segmented` набора полоса одна и обрезается). Выбранный — как ползунок `segmented`:
/// `card` с рамкой `border_strong`, текст `text`; остальные — `raised`, текст `weak`.
pub fn chips<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = anvil_ui::Palette::of(ui);
    let motion = anvil_ui::motion::Motion::of(ui.ctx());
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (i, (option, label)) in options.iter().enumerate() {
            let font = egui::FontId::proportional(13.0);
            let text_w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), p.text).size().x;
            let (rect, r) = ui.allocate_exact_size(egui::vec2(text_w + 20.0, 28.0), egui::Sense::click());
            let on = *value == *option;
            let hovered = r.hovered();
            let fill = if on {
                p.card
            } else if hovered {
                p.hover
            } else {
                p.raised
            };
            let id = ui.id().with(("chip", i, *label));
            let fill = motion.color(ui.ctx(), id.with("fill"), fill, anvil_ui::motion::HOVER);
            let stroke = if on { p.border_strong } else { p.border };
            ui.painter().rect(rect, 6, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            let color = if on || hovered { p.text } else { p.weak };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, *label, font, color);
            anvil_ui::widgets::focus_ring(ui, rect, &r, 6);
            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
            r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, *label));
            if r.clicked() && !on {
                *value = *option;
                changed = true;
            }
        }
    });
    changed
}
