// Модули панели: каждый — карточка со своими быстрыми настройками. Новый модуль — файл здесь,
// `impl Module` и строка в `all()`; остальное (раскладка, появление, обновление) панель делает сама.

pub mod audio;
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
}

pub trait Module {
    /// Ключ для `Id` и для порядка в настройках.
    fn key(&self) -> &'static str;
    fn title(&self, t: &Tr) -> &'static str;
    /// Доля ширины панели относительно соседей.
    fn weight(&self) -> f32 {
        1.0
    }
    /// Перечитать состояние системы: при появлении панели и раз в пару секунд, пока она открыта.
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
}

pub fn all() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(display::Display::default()),
        Box::new(audio::Audio::default()),
        Box::new(buttons::Buttons::default()),
    ]
}

/// Строка списка с текстом слева и пометкой справа; выбранная — с полоской акцента.
pub fn row(ui: &mut Ui, id: egui::Id, selected: bool, label: &str, trailing: &str) -> egui::Response {
    let p = anvil_ui::Palette::of(ui);
    let r = anvil_ui::widgets::list_row(ui, id, selected, 28.0, label);
    let font = egui::FontId::proportional(13.5);
    let color = if selected { p.text } else { p.weak };
    let mut room = r.rect.width() - 22.0;
    if !trailing.is_empty() {
        let right = r.rect.right_center() - egui::vec2(10.0, 0.0);
        let g =
            ui.painter().text(right, egui::Align2::RIGHT_CENTER, trailing, egui::FontId::proportional(12.0), p.faint);
        room -= g.width() + 8.0;
    }
    // Длинное имя (устройства звука) — с многоточием, а не за край карточки.
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room.max(20.0));
    let galley = ui.painter().layout_job(job);
    let left = r.rect.left_center() + egui::vec2(12.0, -galley.size().y / 2.0);
    ui.painter().galley(left, galley, color);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}
