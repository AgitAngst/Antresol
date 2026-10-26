// Яркость, контраст и вход внешних мониторов по DDC/CI. Мониторы отвечают медленно, поэтому
// все запросы — в своём потоке: окно шлёт ему «перечитай» и «поставь», а читает готовое из общего
// списка. Ползунок под рукой показывает своё значение, пока монитор не подтвердит новое.
// Внизу карточки — «Ночной свет» выключателем: он от монитора не зависит и работает, даже если DDC/CI молчит.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

use super::{Ctx, Module, chips};
use crate::sys::ddc::{self, BRIGHTNESS, CONTRAST, INPUT, Physical};
use crate::sys::{self, display, monitor, nightlight};
use crate::texts::{self, Tr};

/// Высота строки «Ночной свет» внизу карточки (вместе с чертой над ней).
const NIGHT_H: f32 = 36.0;

/// Что поток делает по просьбе окна.
enum Job {
    /// Перечитать все мониторы; `caps` — ещё и входы (строка возможностей, медленно).
    Read {
        monitors: Vec<(String, usize)>,
        caps: bool,
    },
    Set {
        handle: usize,
        index: usize,
        code: u8,
        value: u32,
    },
}

/// Ключ значения: монитор, физический номер, код VCP.
type Key = (String, usize, u8);

#[derive(Default)]
struct Shared {
    list: Vec<Physical>,
    /// Прочитали хотя бы раз — до этого карточка показывает «опрашиваю».
    loaded: bool,
    errors: Vec<String>,
}

pub struct Brightness {
    jobs: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
    /// Что двигали сами и когда — показывать своё, пока монитор не догонит.
    local: HashMap<Key, (u32, Instant)>,
    /// Входы, прочитанные из возможностей, — помним между перечитываниями.
    inputs: HashMap<(String, usize), Vec<u32>>,
    /// Сколько раз уже спрашивали входы: не ответил — ещё раз, но не больше трёх (это медленно).
    caps_tries: u8,
    /// Ждём подтверждения смены входа: монитор, физический номер, код входа.
    confirm_input: Option<(String, usize, usize, u32)>,
    /// Ночной свет Windows: включён ли; `None` — не узнать.
    night: Option<bool>,
}

impl Default for Brightness {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let s = shared.clone();
        std::thread::spawn(move || worker(rx, s));
        Brightness {
            jobs: tx,
            shared,
            local: HashMap::new(),
            inputs: HashMap::new(),
            caps_tries: 0,
            confirm_input: None,
            night: None,
        }
    }
}

fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
    while let Ok(first) = rx.recv() {
        // Скопилось — ставим только последнее значение каждого ползунка, читаем один раз.
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
        }
        let mut sets: Vec<(usize, usize, u8, u32)> = Vec::new();
        let mut read: Option<(Vec<(String, usize)>, bool)> = None;
        for j in jobs {
            match j {
                Job::Set { handle, index, code, value } => {
                    sets.retain(|s| !(s.0 == handle && s.1 == index && s.2 == code));
                    sets.push((handle, index, code, value));
                }
                Job::Read { monitors, caps } => {
                    let caps = caps || read.as_ref().is_some_and(|r| r.1);
                    read = Some((monitors, caps));
                }
            }
        }
        for (handle, index, code, value) in sets {
            if let Err(e) = ddc::set(handle, index, code, value) {
                eprintln!("ddc set failed: {e}");
                shared.lock().unwrap_or_else(|e| e.into_inner()).errors.push(e);
            }
        }
        if let Some((monitors, caps)) = read {
            let list: Vec<Physical> = monitors.iter().flat_map(|(d, h)| ddc::read(d, *h, caps)).collect();
            if std::env::var_os("ANTRESOL_TRACE").is_some() {
                eprintln!("ddc: {} monitors, {} physical: {:?}", monitors.len(), list.len(), list);
            }
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.list = list;
            s.loaded = true;
        }
    }
}

impl Brightness {
    fn handle_of(device: &str) -> Option<usize> {
        // Демо и тесты: у выдуманного монитора ручки нет, а писатель `ddc::set` всё равно остановит заслон.
        if sys::isolated() {
            return Some(0);
        }
        monitor::monitors().into_iter().find(|m| m.device == device).map(|m| m.handle)
    }

    fn send_set(&mut self, p: &Physical, code: u8, value: u32) {
        let Some(handle) = Self::handle_of(&p.device) else { return };
        self.local.insert((p.device.clone(), p.index, code), (value, Instant::now()));
        let _ = self.jobs.send(Job::Set { handle, index: p.index, code, value });
    }

    /// Значение ползунка: своё, пока свежее, иначе — что ответил монитор.
    fn shown(&self, p: &Physical, code: u8, from_monitor: u32) -> u32 {
        match self.local.get(&(p.device.clone(), p.index, code)) {
            Some((v, at)) if at.elapsed() < Duration::from_secs(3) => *v,
            _ => from_monitor,
        }
    }

    /// Ползунок 0..max в процентах; колесо мыши над ним — шаг 5 %.
    fn slider(&mut self, ui: &mut Ui, label: &str, p: &Physical, code: u8, f: ddc::Feature) {
        if f.max == 0 {
            return;
        }
        let value = self.shown(p, code, f.current);
        let mut pct = (value as f32 * 100.0 / f.max as f32).round();
        let row = ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).color(Palette::of(ui).weak));
            ui.spacing_mut().slider_width = (ui.available_width() - 48.0).max(60.0);
            let r = ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).show_value(false));
            w::mono(ui, &format!("{pct:>3.0}%"), None);
            r
        });
        let mut changed = row.inner.changed();
        let wheel: f32 = ui.input(|i| {
            i.events.iter().map(|e| if let egui::Event::MouseWheel { delta, .. } = e { delta.y } else { 0.0 }).sum()
        });
        if wheel != 0.0 && ui.is_enabled() && ui.rect_contains_pointer(row.response.rect) {
            pct = (pct + wheel.signum() * 5.0).clamp(0.0, 100.0);
            changed = true;
        }
        if changed {
            let v = (pct / 100.0 * f.max as f32).round() as u32;
            self.send_set(p, code, v);
        }
    }
}

impl Brightness {
    /// Подтверждение смены входа: экран может погаснуть — вернуть можно только кнопками монитора. `false` — смены входа
    /// никто не просил, рисовать нечего.
    fn confirm_input_ui(&mut self, ui: &mut Ui, cx: &mut Ctx, list: &[Physical]) -> bool {
        let t = cx.t;
        let Some((device, index, list_i, code)) = self.confirm_input.clone() else { return false };
        let name = list.get(list_i).map(|p| p.name.clone()).unwrap_or_default();
        let body = texts::input_warning(cx.lang, &name, &ddc::input_name(code));
        w::banner(ui, Tone::Warning, t.input_title, &body, |ui| {
            if w::button(ui, Kind::Danger, None, t.input_switch).clicked() {
                if let Some(handle) = Self::handle_of(&device) {
                    let _ = self.jobs.send(Job::Set { handle, index, code: INPUT, value: code });
                }
                // Чип показывает новый вход сразу, не дожидаясь следующего опроса.
                self.local.insert((device.clone(), index, INPUT), (code, Instant::now()));
                self.confirm_input = None;
            }
            if w::button(ui, Kind::Secondary, None, t.cancel).clicked() {
                self.confirm_input = None;
            }
        });
        true
    }

    /// Ползунки и входы каждого монитора, что отвечает по DDC/CI; пока опрашиваются — «Опрашиваю…», никто не
    /// отвечает — пояснение.
    fn monitors_ui(&mut self, ui: &mut Ui, cx: &mut Ctx, list: &[Physical], loaded: bool) {
        let t = cx.t;
        if !loaded {
            ui.horizontal(|ui| {
                w::spinner(ui, 16.0);
                w::note(ui, t.ddc_reading);
            });
            return;
        }
        let answering: Vec<&Physical> = list.iter().filter(|p| p.brightness.is_some()).collect();
        if answering.is_empty() {
            w::note(ui, t.ddc_none);
            return;
        }
        egui::ScrollArea::vertical().id_salt("ddc").auto_shrink([false, false]).show(ui, |ui| {
            let mut first = true;
            for (i, p) in list.iter().enumerate() {
                let Some(b) = p.brightness else { continue };
                if !first {
                    ui.add_space(6.0);
                }
                first = false;
                // Имя монитора обычным текстом; устройство (`DISPLAYn`) — только в подсказке.
                let short = p.device.trim_start_matches(r"\\.\");
                if !p.name.is_empty() {
                    let name = egui::RichText::new(&p.name).size(12.5).color(Palette::of(ui).weak);
                    ui.label(name).on_hover_text(short);
                }
                self.slider(ui, t.brightness_short, p, BRIGHTNESS, b);
                if let Some(c) = p.contrast {
                    self.slider(ui, t.contrast, p, CONTRAST, c);
                }
                let inputs = self.inputs.get(&(p.device.clone(), p.index)).cloned().unwrap_or_default();
                if inputs.len() > 1 {
                    let names: Vec<String> = inputs.iter().map(|c| ddc::input_name(*c)).collect();
                    let opts: Vec<(u32, &str)> = inputs.iter().zip(&names).map(|(c, n)| (*c, n.as_str())).collect();
                    let mut want = self.shown(p, INPUT, p.input.unwrap_or(0));
                    if chips(ui, &mut want, &opts) {
                        self.confirm_input = Some((p.device.clone(), p.index, i, want));
                    }
                }
            }
        });
    }

    /// «Ночной свет» внизу карточки: черта, значок, подпись и выключатель. Не узнать состояние — строка недоступна.
    fn night_row(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
        let inner = Rect::from_min_max(rect.min + Vec2::new(0.0, 7.0), rect.max);
        let layout = egui::Layout::left_to_right(egui::Align::Center);
        let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(layout));
        row.spacing_mut().item_spacing.x = 10.0;
        let (icon, _) = row.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        anvil_ui::icons::paint(row.painter(), icon, Icon::Moon, p.weak);
        let known = self.night.is_some();
        let mut on = self.night.unwrap_or(false);
        let r = row.add_enabled_ui(known, |ui| w::switch(ui, &mut on, t.night_light)).inner;
        if !known {
            r.on_disabled_hover_text(t.unavailable);
        } else if r.changed() {
            match nightlight::set(on) {
                Ok(()) => self.night = Some(on),
                Err(e) => {
                    eprintln!("night light failed: {e}");
                    cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                }
            }
        }
    }
}

impl Module for Brightness {
    fn key(&self) -> &'static str {
        "brightness"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.brightness
    }

    fn refresh(&mut self, panel: &str) {
        let monitors: Vec<(String, usize)> =
            display::monitors(panel).into_iter().map(|m| (m.device, m.handle)).collect();
        // Входы — пока не ответили, до трёх раз: строка возможностей отвечает до секунды на монитор.
        let caps = self.inputs.is_empty() && self.caps_tries < 3;
        if caps {
            self.caps_tries += 1;
        }
        let _ = self.jobs.send(Job::Read { monitors, caps });
        self.night = nightlight::state();
    }

    fn busy(&self) -> bool {
        self.confirm_input.is_some()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let (list, loaded, errors) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.list.clone(), s.loaded, std::mem::take(&mut s.errors))
        };
        if !errors.is_empty() {
            cx.toasts.push((t.ddc_failed.to_string(), Tone::Danger));
        }
        for p in &list {
            if !p.inputs.is_empty() {
                self.inputs.insert((p.device.clone(), p.index), p.inputs.clone());
            }
        }

        // Внизу — «Ночной свет» (всегда: и пока мониторы опрашиваются, и если по DDC/CI никто не отвечает, и пока висит
        // подтверждение смены входа), выше — мониторы или это подтверждение.
        let full = ui.available_rect_before_wrap();
        let night = Rect::from_min_max(Pos2::new(full.left(), (full.bottom() - NIGHT_H).max(full.top())), full.max);
        let body = Rect::from_min_max(full.min, Pos2::new(full.right(), (night.top() - 6.0).max(full.top() + 24.0)));
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        if !self.confirm_input_ui(&mut body_ui, cx, &list) {
            self.monitors_ui(&mut body_ui, cx, &list, loaded);
        }
        self.night_row(ui, night, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;

    /// Весь текст, что нарисовали за кадр.
    fn painted_text(shapes: &[egui::epaint::ClippedShape]) -> String {
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
        shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    }

    fn frame(card: &mut Brightness) -> String {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        anvil_ui::install(&ctx, crate::look::accent_of(&settings.accent), settings.common.theme);
        let mut toasts = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 300.0))),
            ..Default::default()
        };
        let mut shown = String::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(input.clone(), |ui| {
                let mut cx = Ctx {
                    t: texts::tr(anvil_ui::Lang::En),
                    lang: anvil_ui::Lang::En,
                    monitor: r"\\.\TEST-BRIGHTNESS",
                    toasts: &mut toasts,
                    settings: &mut settings,
                };
                card.ui(ui, &mut cx);
            });
            out.textures_delta.clear();
            shown = painted_text(&out.shapes);
        }
        shown
    }

    #[test]
    fn night_light_row_stays_while_input_is_confirmed() {
        let mut card = Brightness::default();
        assert!(frame(&mut card).contains("Night light"), "без подтверждения строка есть (контроль)");
        card.confirm_input = Some((r"\\.\TEST-BRIGHTNESS".into(), 0, 0, 0x0F));
        let shown = frame(&mut card);
        assert!(shown.contains("Night light"), "пока висит подтверждение входа, «Ночной свет» виден: {shown}");
        assert!(shown.contains(texts::tr(anvil_ui::Lang::En).input_title), "и само подтверждение тоже: {shown}");
    }
}
