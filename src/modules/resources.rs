// Ресурсы: процессор (с графиком за последнюю минуту), видеокарта, память, сеть, батарея, диски.
// Замер — при каждом перечитывании панели (раз в 1,5 с, пока она открыта): вызовы дешёвые.

use std::collections::VecDeque;

use anvil_ui::Palette;
use anvil_ui::widgets as w;
use eframe::egui::{self, Pos2, Sense, Stroke, Ui, Vec2};

use super::{Ctx, Module};
use crate::sys::res::{self, Sampler, Snapshot};
use crate::texts::Tr;

/// Сколько замеров процессора держит график: 40 × 1,5 с = минута.
const HISTORY: usize = 40;

#[derive(Default)]
pub struct Resources {
    sampler: Sampler,
    last: Option<Snapshot>,
    cpu: VecDeque<f32>,
    /// Когда замеряли в последний раз: панель долго была спрятана — график начинается заново.
    sampled: Option<std::time::Instant>,
}

/// Строка «подпись … значение» и полоса доли под ней.
fn meter(ui: &mut Ui, label: &str, value: &str, fraction: Option<f32>) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(p.weak));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            w::mono(ui, value, Some(p.text));
        });
    });
    if let Some(f) = fraction {
        w::progress(ui, Some(f.clamp(0.0, 1.0)), ui.available_width());
    }
    ui.add_space(4.0);
}

/// График процессора: линия акцентом по последним замерам.
fn sparkline(ui: &mut Ui, values: &VecDeque<f32>) {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 4, p.raised);
    if values.len() < 2 {
        return;
    }
    let step = rect.width() / (HISTORY - 1) as f32;
    let start = rect.right() - step * (values.len() - 1) as f32;
    let pts: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| Pos2::new(start + step * i as f32, rect.bottom() - 2.0 - v * (rect.height() - 4.0)))
        .collect();
    ui.painter().add(egui::Shape::line(pts, Stroke::new(1.5, p.accent)));
}

fn pct(f: f32) -> String {
    format!("{:.0}%", f * 100.0)
}

impl Module for Resources {
    fn key(&self) -> &'static str {
        "resources"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.resources
    }

    fn refresh(&mut self, _monitor: &str) {
        if self.sampled.is_some_and(|at| at.elapsed().as_secs() >= 5) {
            self.cpu.clear();
        }
        self.sampled = Some(std::time::Instant::now());
        let s = self.sampler.sample();
        if let Some(c) = s.cpu {
            self.cpu.push_back(c);
            while self.cpu.len() > HISTORY {
                self.cpu.pop_front();
            }
        }
        self.last = Some(s);
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let ru = cx.lang == anvil_ui::Lang::Ru;
        let Some(s) = self.last.clone() else {
            w::spinner(ui, 16.0);
            return;
        };
        egui::ScrollArea::vertical().id_salt("res").auto_shrink([false, false]).show(ui, |ui| {
            meter(ui, t.cpu, &s.cpu.map(pct).unwrap_or_else(|| "…".into()), None);
            sparkline(ui, &self.cpu);
            ui.add_space(4.0);
            if let Some(g) = s.gpu {
                meter(ui, t.gpu, &pct(g), Some(g));
            }
            if s.mem_total > 0 {
                let f = s.mem_used as f32 / s.mem_total as f32;
                let v = format!("{} / {}", res::bytes(s.mem_used as f64, ru), res::bytes(s.mem_total as f64, ru));
                meter(ui, t.memory, &v, Some(f));
            }
            if let Some((down, up)) = s.net {
                let per = if ru { "/с" } else { "/s" };
                let v = format!("↓ {}{per}  ↑ {}{per}", res::bytes(down, ru), res::bytes(up, ru));
                meter(ui, t.network, &v, None);
            }
            if let Some((charge, plugged)) = s.battery {
                let v = if plugged { format!("{charge}% · {}", t.plugged) } else { format!("{charge}%") };
                meter(ui, t.battery, &v, Some(charge as f32 / 100.0));
            }
            for (letter, free, total) in &s.disks {
                if *total == 0 {
                    continue;
                }
                let used = 1.0 - *free as f32 / *total as f32;
                let v = crate::texts::disk_free(cx.lang, &res::bytes(*free as f64, ru), &res::bytes(*total as f64, ru));
                meter(ui, &format!("{letter}:"), &v, Some(used));
            }
        });
    }
}
