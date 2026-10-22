// Яркость, контраст и вход внешних мониторов по DDC/CI. Мониторы отвечают медленно, поэтому
// все запросы — в своём потоке: окно шлёт ему «перечитай» и «поставь», а читает готовое из общего
// списка. Ползунок под рукой показывает своё значение, пока монитор не подтвердит новое.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Palette, Tone};
use eframe::egui::{self, Ui};

use super::{Ctx, Module, chips};
use crate::sys::ddc::{self, BRIGHTNESS, CONTRAST, INPUT, Physical};
use crate::sys::monitor;
use crate::texts::{self, Tr};

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
        if wheel != 0.0 && ui.rect_contains_pointer(row.response.rect) {
            pct = (pct + wheel.signum() * 5.0).clamp(0.0, 100.0);
            changed = true;
        }
        if changed {
            let v = (pct / 100.0 * f.max as f32).round() as u32;
            self.send_set(p, code, v);
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

    fn refresh(&mut self, _monitor: &str) {
        let monitors: Vec<(String, usize)> = monitor::monitors().into_iter().map(|m| (m.device, m.handle)).collect();
        // Входы — пока не ответили, до трёх раз: строка возможностей отвечает до секунды на монитор.
        let caps = self.inputs.is_empty() && self.caps_tries < 3;
        if caps {
            self.caps_tries += 1;
        }
        let _ = self.jobs.send(Job::Read { monitors, caps });
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
        if !loaded {
            ui.horizontal(|ui| {
                w::spinner(ui, 16.0);
                w::note(ui, t.ddc_reading);
            });
            return;
        }

        // Подтверждение смены входа: экран может погаснуть — вернуть можно только кнопками монитора.
        if let Some((device, index, list_i, code)) = self.confirm_input.clone() {
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
                let short = p.device.trim_start_matches(r"\\.\");
                w::mono(ui, &format!("{} · {short}", p.name), Some(Palette::of(ui).weak));
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
}
