// В release-сборке прячем консольное окно (в debug оставляем для логов).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod ics;
mod layout;
mod look;
mod modules;
mod panel;
mod plaque;
mod search;
mod shot;
mod sys;
mod texts;
mod update;
mod watch;

fn main() -> eframe::Result<()> {
    // После «Перезапустить» — дождаться, пока прежняя версия закроется (она ещё держит сочетания клавиш).
    update::wait_for_previous();
    let s = config::load();
    // Окно без рамки, поверх всех и без кнопки на панели задач. Место и размер
    // App ставит сам у верхнего края монитора.
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Antresol")
        .with_decorations(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_resizable(false)
        // Файлы можно бросить на панель — в лоток.
        .with_drag_and_drop(true)
        .with_inner_size([s.width, s.height])
        // До первого кадра — выше края экрана, чтобы не мелькнуть в углу.
        .with_position([0.0, -s.height - 100.0]);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Antresol", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
