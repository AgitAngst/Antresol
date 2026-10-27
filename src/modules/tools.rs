// Инструменты: пипетка (тянуть круг на любой цвет экрана — код цвета в буфер) и текст с экрана
// (выделить область «Ножницами» — распознанный текст в буфер). Распознавание — в потоке модуля.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Color32, Sense, Stroke, Ui, Vec2};

use super::ShowNoted;
use super::{Action, Command, Ctx, Module, mine};
use crate::config::Settings;
use crate::sys::{self, clip, monitor, ocr};
use crate::texts::Tr;

/// Сколько ждать снимка «Ножниц».
const SNIP_WAIT: Duration = Duration::from_secs(60);
/// Сколько цветов помнить.
const HISTORY: usize = 6;

#[derive(Clone, PartialEq)]
enum Snip {
    Idle,
    /// Ждём, пока выделят область.
    Waiting,
    Done(String),
    Failed(String),
}

pub struct Tools {
    picked: Option<Color32>,
    live: Option<Color32>,
    dragging: bool,
    history: Vec<Color32>,
    jobs: Sender<()>,
    snip: Arc<Mutex<Snip>>,
}

impl Default for Tools {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let snip = Arc::new(Mutex::new(Snip::Idle));
        let s = snip.clone();
        std::thread::spawn(move || worker(rx, s));
        let mut tools = Tools { picked: None, live: None, dragging: false, history: Vec::new(), jobs: tx, snip };
        if sys::demo() {
            tools.picked = Some(Color32::from_rgb(0x4f, 0x7a, 0x12));
            tools.history = vec![
                Color32::from_rgb(0x4f, 0x7a, 0x12),
                Color32::from_rgb(0x2a, 0x5f, 0xd0),
                Color32::from_rgb(0xe2, 0x7a, 0xd8),
            ];
            *tools.snip.lock().unwrap_or_else(|e| e.into_inner()) =
                Snip::Done("Demo text from the screen\nsecond line".into());
        }
        tools
    }
}

/// Код цвета «#4f7a12».
pub fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn set(snip: &Mutex<Snip>, v: Snip) {
    *snip.lock().unwrap_or_else(|e| e.into_inner()) = v;
}

fn worker(rx: Receiver<()>, snip: Arc<Mutex<Snip>>) {
    sys::media::init_thread();
    while rx.recv().is_ok() {
        // Сбой внутри (чужие данные в буфере) не убивает поток: карточка покажет ошибку.
        let s = snip.clone();
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| one_snip(&s))).is_err() {
            set(&snip, Snip::Failed("internal error".into()));
        }
    }
}

/// Ждём ли ещё снимка (не отменили).
fn waiting(snip: &Mutex<Snip>) -> bool {
    *snip.lock().unwrap_or_else(|e| e.into_inner()) == Snip::Waiting
}

fn one_snip(snip: &Mutex<Snip>) {
    {
        // Дать панели уехать, чтобы она не попала в снимок.
        std::thread::sleep(Duration::from_millis(500));
        let before = clip::sequence();
        if let Err(e) = sys::launch("ms-screenclip:", "") {
            set(snip, Snip::Failed(e));
            return;
        }
        let start = Instant::now();
        let image = loop {
            if start.elapsed() > SNIP_WAIT || !waiting(snip) {
                break None;
            }
            std::thread::sleep(Duration::from_millis(300));
            if clip::sequence() != before
                && let Some(img) = ocr::clipboard_image()
            {
                break Some(img);
            }
        };
        let Some(img) = image else {
            if waiting(snip) {
                set(snip, Snip::Idle);
            }
            return;
        };
        match ocr::recognize(&img) {
            Ok(text) if !text.trim().is_empty() => {
                let _ = clip::write_text(&text);
                set(snip, Snip::Done(text));
            }
            Ok(_) => set(snip, Snip::Failed(String::new())),
            Err(e) => set(snip, Snip::Failed(e.message())),
        }
    }
}

impl Tools {
    /// Цвет взят — отпустили круг пипетки или действие поиска: он выбран, запоминается в истории и уходит в буфер.
    fn take(&mut self, c: Color32, cx: &mut Ctx) {
        self.picked = Some(c);
        self.history.retain(|h| *h != c);
        self.history.insert(0, c);
        self.history.truncate(HISTORY);
        self.copy(c, cx);
    }

    /// «Выделить область»: «Ножницы» и распознавание в потоке модуля (в демо-режиме ничего не запускается).
    fn start_snip(&mut self) {
        if sys::demo() {
            return;
        }
        set(&self.snip, Snip::Waiting);
        let _ = self.jobs.send(());
    }

    fn snip_waiting(&self) -> bool {
        waiting(&self.snip)
    }

    fn copy(&self, c: Color32, cx: &mut Ctx) {
        let t = cx.t;
        match clip::write_text(&hex(c)) {
            Ok(()) => cx.toasts.push((format!("{} {}", hex(c), t.color_copied), Tone::Neutral)),
            Err(e) => cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger)),
        }
    }

    fn picker(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        w::section_label(ui, t.picker);
        let mut done = None;
        ui.horizontal(|ui| {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::drag());
            if r.dragged() {
                self.dragging = true;
                let (x, y) = monitor::cursor();
                self.live = monitor::pixel_at(x, y).map(|(r, g, b)| Color32::from_rgb(r, g, b));
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
            if r.drag_stopped() {
                self.dragging = false;
                done = self.live.take();
            }
            let shown = if self.dragging { self.live } else { self.picked };
            match shown {
                Some(c) => {
                    ui.painter().circle_filled(rect.center(), 19.0, c);
                    ui.painter().circle_stroke(rect.center(), 19.5, Stroke::new(1.0, p.border_strong));
                }
                None => {
                    ui.painter().circle(rect.center(), 19.0, p.raised, Stroke::new(1.0, p.border_strong));
                    anvil_ui::icons::paint(ui.painter(), rect.shrink(12.0), Icon::Target, p.weak);
                }
            }
            r.on_hover_text(t.picker_hint).on_hover_cursor(egui::CursorIcon::Grab);
            ui.vertical(|ui| match shown {
                Some(c) => {
                    ui.label(egui::RichText::new(hex(c)).font(egui::FontId::monospace(15.0)).color(p.text));
                    ui.label(
                        egui::RichText::new(format!("rgb({}, {}, {})", c.r(), c.g(), c.b())).size(12.0).color(p.weak),
                    );
                }
                None => {
                    w::note(ui, t.picker_hint);
                }
            });
        });
        if let Some(c) = done {
            self.take(c, cx);
        }
        if !self.history.is_empty() {
            let mut again = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for c in &self.history {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                    ui.painter().circle(rect.center(), 9.0, *c, Stroke::new(1.0, p.border));
                    w::focus_ring(ui, rect, &r, 10);
                    if r.on_hover_text(hex(*c)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        again = Some(*c);
                    }
                }
            });
            if let Some(c) = again {
                self.picked = Some(c);
                self.copy(c, cx);
            }
        }
    }

    fn snip_ui(&mut self, ui: &mut Ui, t: &Tr) {
        let p = Palette::of(ui);
        ui.add_space(8.0);
        w::section_label(ui, t.screen_text);
        let state = self.snip.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if state == Snip::Waiting {
            ui.horizontal(|ui| {
                w::spinner(ui, 16.0);
                w::note(ui, t.screen_text_wait);
            });
            if w::button(ui, Kind::Ghost, None, t.cancel).clicked() {
                set(&self.snip, Snip::Idle);
            }
            return;
        }
        if w::button(ui, Kind::Secondary, Some(Icon::Search), t.screen_text_pick).clicked() {
            self.start_snip();
        }
        match state {
            Snip::Done(text) => {
                let mut job =
                    egui::text::LayoutJob::simple(text, egui::FontId::proportional(12.5), p.weak, ui.available_width());
                job.wrap.max_rows = 3;
                ui.label(job);
                w::note(ui, t.screen_text_done);
            }
            Snip::Failed(e) if e.is_empty() => {
                w::note(ui, t.screen_text_none);
            }
            Snip::Failed(e) if e == ocr::NO_LANGUAGE => {
                w::note(ui, t.screen_text_no_lang);
            }
            Snip::Failed(e) => {
                w::note(ui, format!("{}: {e}", t.failed));
            }
            _ => {}
        }
    }
}

impl Module for Tools {
    fn key(&self) -> &'static str {
        "tools"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.tools
    }

    fn refresh(&mut self, _monitor: &str) {}

    fn busy(&self) -> bool {
        self.dragging
    }

    fn ticking(&self) -> bool {
        self.dragging || *self.snip.lock().unwrap_or_else(|e| e.into_inner()) == Snip::Waiting
    }

    /// «Пипетка: цвет под мышью» — то же, что отпустить круг над этим местом экрана — и «Выделить область».
    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        match mine(self.key(), id) {
            Some("pick") => {
                let (x, y) = monitor::cursor();
                match monitor::pixel_at(x, y) {
                    Some((r, g, b)) => self.take(Color32::from_rgb(r, g, b), cx),
                    None => cx.toasts.push((cx.t.failed.to_string(), Tone::Danger)),
                }
            }
            Some("snip") if !self.snip_waiting() => self.start_snip(),
            _ => {}
        }
    }

    /// Пипетка берёт цвет под мышью, пока мышь не над самой панелью (иначе взяла бы цвет панели); «Выделить область» ждёт,
    /// пока идёт прошлое выделение.
    fn actions(&self, t: &Tr, _lang: anvil_ui::Lang, _settings: &Settings) -> Vec<Action> {
        let pick = Action::new("tools:pick", format!("{}: {}", t.picker, t.search_pick), t.tools, Icon::Target);
        let snip = Action::new(
            "tools:snip",
            format!("{}: {}", t.screen_text, t.screen_text_pick.to_lowercase()),
            t.tools,
            Icon::Search,
        );
        vec![
            pick.words("цвет пипетка пиксель color picker eyedropper pixel").free_pointer(),
            snip.words("текст экран распознать ocr скриншот область text screen recognize snip")
                .off_if(self.snip_waiting(), t.screen_text_wait),
        ]
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        egui::ScrollArea::vertical().id_salt("tools").auto_shrink([false, false]).show_noted(ui, |ui| {
            self.picker(ui, cx);
            self.snip_ui(ui, t);
        });
        if self.ticking() {
            ui.ctx().request_repaint_after(Duration::from_millis(30));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_code() {
        assert_eq!(hex(Color32::from_rgb(0x4f, 0x7a, 0x12)), "#4f7a12");
    }

    const EN: anvil_ui::Lang = anvil_ui::Lang::En;

    /// Карточка без потока «Ножниц»: команда на выделение области уходит в наш канал, а не в настоящий снимок экрана.
    fn card() -> (Tools, Receiver<()>) {
        let mut tools = Tools::default();
        let (tx, rx) = mpsc::channel();
        tools.jobs = tx;
        (tools, rx)
    }

    fn run(tools: &mut Tools, id: &str) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        let mut settings = Settings::default();
        let mut cx =
            Ctx { t: crate::texts::tr(EN), lang: EN, monitor: "", toasts: &mut toasts, settings: &mut settings };
        tools.command(&Command::RunAction(id.to_string()), &mut cx);
        toasts
    }

    #[test]
    fn the_picker_and_the_area_are_offered() {
        let (tools, _rx) = card();
        let (en, ru) = (crate::texts::tr(EN), crate::texts::tr(anvil_ui::Lang::Ru));
        let actions = tools.actions(en, EN, &Settings::default());
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Color picker: color under the mouse", "Text from screen: select an area"]);
        let ru_titles: Vec<String> =
            tools.actions(ru, anvil_ui::Lang::Ru, &Settings::default()).into_iter().map(|a| a.title).collect();
        assert_eq!(ru_titles, ["Пипетка: цвет под мышью", "Текст с экрана: выделить область"]);
        assert!(actions.iter().all(|a| a.enabled && a.place == en.tools));
        // Пипетка берёт цвет под мышью: над самой панелью она серая (это решает панель по этому признаку).
        assert!(actions[0].needs_free_pointer && !actions[1].needs_free_pointer);
        assert!(actions[1].words.contains("ocr") && actions[0].words.contains("eyedropper"));
    }

    #[test]
    fn the_picker_action_does_what_releasing_the_circle_does() {
        let (mut tools, _rx) = card();
        // Выдуманный мир отдаёт один и тот же цвет — экран этого ПК не читается.
        let toasts = run(&mut tools, "tools:pick");
        assert_eq!(toasts, vec![("#2a5fd0 copied".to_string(), Tone::Neutral)]);
        let color = Color32::from_rgb(0x2a, 0x5f, 0xd0);
        assert_eq!((tools.picked, tools.history.clone()), (Some(color), vec![color]));
        let want = "clip::write_text #2a5fd0";
        assert!(
            sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|w| w == want),
            "код цвета не ушёл в буфер"
        );
        // Тот же цвет второй раз — в истории он один и сверху.
        tools.history.push(Color32::from_rgb(1, 2, 3));
        run(&mut tools, "tools:pick");
        assert_eq!(tools.history, vec![color, Color32::from_rgb(1, 2, 3)]);
        // Чужое и неизвестное — ничего.
        assert!(run(&mut tools, "quick:wifi").is_empty() && run(&mut tools, "tools:zoom").is_empty());
    }

    #[test]
    fn the_area_action_starts_one_snip_like_the_button_and_waits_for_it() {
        let (mut tools, rx) = card();
        let en = crate::texts::tr(EN);
        assert!(run(&mut tools, "tools:snip").is_empty());
        assert!(tools.snip_waiting(), "ждём выделения");
        assert!(rx.try_recv().is_ok(), "поток получил задание");
        // Пока ждём, действие серое: «Выделите область на экране…», и второй запуск не посылает задания.
        let actions = tools.actions(en, EN, &Settings::default());
        assert!(!actions[1].enabled && actions[1].reason.as_deref() == Some(en.screen_text_wait));
        run(&mut tools, "tools:snip");
        assert!(rx.try_recv().is_err());
        // Отменили — снова можно.
        set(&tools.snip, Snip::Idle);
        assert!(tools.actions(en, EN, &Settings::default())[1].enabled);
    }
}
