// В release-сборке прячем консольное окно (в debug оставляем для логов).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;
mod panel;
mod shot;
mod sys;
mod texts;

fn main() -> eframe::Result<()> {
    let s = config::load();
    // Окно без рамки, поверх всех и без кнопки на панели задач. Место и размер
    // App ставит сам у верхнего края монитора.
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Antresol")
        .with_decorations(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_resizable(false)
        .with_inner_size([s.width, s.height])
        // До первого кадра — выше края экрана, чтобы не мелькнуть в углу.
        .with_position([0.0, -s.height - 100.0]);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Antresol", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
