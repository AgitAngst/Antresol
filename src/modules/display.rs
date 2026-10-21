// Экран: разрешение, частота и масштаб. После смены режима — «Оставить?» с обратным отсчётом,
// как в Windows: если картинка пропала, через 15 с всё вернётся само.
// Избранное — до трёх разрешений на монитор чипами сверху. Частота запоминается для каждого
// разрешения (то, что сейчас стоит и не ждёт «Оставить»), и при переходе на него ставится она.

use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Palette, Tone};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Ctx, Module, chips, row};
use crate::sys::display::{self, Mode, Scale};
use crate::sys::monitor;
use crate::texts::{self, Tr};

const REVERT_AFTER: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct Display {
    /// Какой монитор показываем; пусто — тот, у которого панель.
    picked: String,
    monitors: Vec<monitor::Monitor>,
    name: String,
    current: Option<Mode>,
    modes: Vec<Mode>,
    scale: Option<Scale>,
    /// Режим поменяли и ждём «Оставить»: прежний режим и когда поменяли.
    pending: Option<(Mode, Instant)>,
}

/// Соотношение сторон для подписи: «16:9», «21:9»; странные — пусто.
pub fn aspect(w: u32, h: u32) -> &'static str {
    let r = w as f32 / h as f32;
    let near = |x: f32| (r - x).abs() < 0.02;
    if near(16.0 / 9.0) {
        "16:9"
    } else if near(16.0 / 10.0) {
        "16:10"
    } else if near(4.0 / 3.0) {
        "4:3"
    } else if near(5.0 / 4.0) {
        "5:4"
    } else if near(21.0 / 9.0) || near(64.0 / 27.0) || near(43.0 / 18.0) {
        "21:9"
    } else if near(32.0 / 9.0) {
        "32:9"
    } else {
        ""
    }
}

/// Разрешения без повторов, от большего к меньшему.
pub fn resolutions(modes: &[Mode]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for m in modes {
        if !out.contains(&(m.width, m.height)) {
            out.push((m.width, m.height));
        }
    }
    out.sort_by_key(|&(w, h)| std::cmp::Reverse((w * h, w)));
    out
}

/// Частоты для разрешения, по возрастанию.
pub fn rates(modes: &[Mode], w: u32, h: u32) -> Vec<u32> {
    let mut out: Vec<u32> = modes.iter().filter(|m| m.width == w && m.height == h).map(|m| m.hz).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Режим для нового разрешения: та же частота, если есть, иначе самая высокая.
pub fn mode_for(modes: &[Mode], w: u32, h: u32, hz: u32) -> Option<Mode> {
    let same = modes.iter().find(|m| m.width == w && m.height == h && m.hz == hz);
    same.or_else(|| modes.iter().filter(|m| m.width == w && m.height == h).max_by_key(|m| m.hz)).copied()
}

/// Пятиконечная звезда: залитая — избранное, контур — можно добавить. Своя: в наборе такой нет.
fn paint_star(painter: &egui::Painter, rect: Rect, filled: bool, color: Color32) {
    let c = rect.center();
    let (outer, inner) = (rect.height() * 0.48, rect.height() * 0.2);
    let pts: Vec<Pos2> = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    if filled {
        // Звезда не выпуклая — заливаем треугольниками от центра.
        for i in 0..10 {
            let tri = vec![c, pts[i], pts[(i + 1) % 10]];
            painter.add(egui::Shape::convex_polygon(tri, color, Stroke::NONE));
        }
    }
    painter.add(egui::Shape::closed_line(pts, Stroke::new(1.3, color)));
}

/// Звёздочка в правом конце строки разрешения. Возвращает щелчок по ней.
fn star(ui: &mut Ui, row: Rect, id: egui::Id, favorite: bool, hint: &str) -> bool {
    let p = Palette::of(ui);
    let rect = Rect::from_center_size(Pos2::new(row.right() - 18.0, row.center().y), Vec2::splat(24.0));
    let r = ui.interact(rect, id, Sense::click());
    let row_hot = ui.rect_contains_pointer(row);
    // Не избранное — звёздочка видна только под мышью, чтобы список не пестрел.
    if favorite || row_hot {
        let color = if favorite {
            p.accent
        } else if r.hovered() {
            p.text
        } else {
            p.faint
        };
        paint_star(ui.painter(), rect.shrink(5.0), favorite, color);
    }
    r.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

impl Display {
    fn device<'a>(&'a self, panel: &'a str) -> &'a str {
        if self.picked.is_empty() { panel } else { &self.picked }
    }

    fn apply(&mut self, device: &str, mode: Mode, cx: &mut Ctx) {
        let Some(old) = self.current else { return };
        if mode == old {
            return;
        }
        match display::apply(device, mode) {
            Ok(()) => {
                self.pending = Some((old, Instant::now()));
                self.current = Some(mode);
            }
            Err(e) => {
                eprintln!("display mode change failed: {e}");
                cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
            }
        }
    }

    fn revert(&mut self, device: &str) {
        if let Some((old, _)) = self.pending.take() {
            if let Err(e) = display::apply(device, old) {
                eprintln!("display mode revert failed: {e}");
            }
            self.current = display::current(device);
        }
    }

    fn keep_ui(&mut self, ui: &mut Ui, device: &str, t: &Tr, lang: anvil_ui::Lang) {
        let Some((_, at)) = self.pending else { return };
        let left = REVERT_AFTER.saturating_sub(at.elapsed());
        if left.is_zero() {
            self.revert(device);
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        let secs = left.as_secs() + 1;
        w::banner(ui, Tone::Warning, t.keep_title, &texts::revert_in(lang, secs), |ui| {
            if w::button(ui, Kind::Primary, None, t.keep).clicked() {
                self.pending = None;
            }
            if w::button(ui, Kind::Secondary, None, t.revert).clicked() {
                self.revert(device);
            }
        });
    }
}

impl Module for Display {
    fn key(&self) -> &'static str {
        "display"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.display
    }

    fn weight(&self) -> f32 {
        1.8
    }

    fn refresh(&mut self, panel: &str) {
        if self.pending.is_some() {
            return;
        }
        self.monitors = monitor::monitors();
        if !self.monitors.iter().any(|m| m.device == self.picked) {
            self.picked.clear();
        }
        let device = self.device(panel).to_string();
        self.name = display::monitor_name(&device);
        self.current = display::current(&device);
        self.modes = display::modes(&device);
        self.scale = display::scale(&device);
    }

    fn busy(&self) -> bool {
        self.pending.is_some()
    }

    fn ticking(&self) -> bool {
        self.pending.is_some()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let device = self.device(cx.monitor).to_string();
        let p = Palette::of(ui);

        // Шапка: имя монитора и выбор монитора, если их несколько.
        ui.horizontal(|ui| {
            let short = device.trim_start_matches(r"\\.\");
            let label = if self.name.is_empty() { short.to_string() } else { format!("{} · {short}", self.name) };
            w::mono(ui, &label, Some(p.weak));
            if self.monitors.len() > 1 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut picked = self.monitors.iter().position(|m| m.device == device).unwrap_or(0);
                    let names: Vec<String> = (1..=self.monitors.len()).map(|i| i.to_string()).collect();
                    let opts: Vec<(usize, Option<anvil_ui::Icon>, &str)> =
                        names.iter().enumerate().rev().map(|(i, n)| (i, None, n.as_str())).collect();
                    let before = picked;
                    w::segmented(ui, &mut picked, &opts);
                    if picked != before && self.pending.is_none() {
                        self.picked = self.monitors[picked].device.clone();
                        self.refresh(cx.monitor);
                    }
                });
            }
        });
        ui.add_space(6.0);

        if self.pending.is_some() {
            self.keep_ui(ui, &device, t, cx.lang);
            return;
        }
        let Some(cur) = self.current else {
            w::note(ui, t.failed);
            return;
        };
        // Стоит и не ждёт ответа — значит, это частота этого разрешения.
        cx.settings.remember_hz(&device, cur.width, cur.height, cur.hz);

        // Избранное: чипы «2560 × 1440 · 144 Гц», щелчок — сразу этот режим.
        let favorites = cx.settings.favorites_of(&device);
        w::section_label(ui, t.favorites);
        if favorites.is_empty() {
            w::note(ui, t.favorites_hint);
        } else {
            let targets: Vec<(u32, u32, Option<Mode>)> = favorites
                .iter()
                .map(|&(fw, fh)| {
                    let hz = cx.settings.remembered_hz(&device, fw, fh).unwrap_or(cur.hz);
                    (fw, fh, mode_for(&self.modes, fw, fh, hz))
                })
                .collect();
            let labels: Vec<String> = targets
                .iter()
                .map(|(fw, fh, m)| match m {
                    Some(m) => format!("{fw} × {fh} · {} {}", m.hz, t.hz),
                    None => format!("{fw} × {fh}"),
                })
                .collect();
            let opts: Vec<((u32, u32), &str)> =
                targets.iter().zip(&labels).map(|((fw, fh, _), l)| ((*fw, *fh), l.as_str())).collect();
            let mut want = (cur.width, cur.height);
            if chips(ui, &mut want, &opts)
                && let Some((_, _, Some(m))) = targets.iter().find(|(fw, fh, _)| (*fw, *fh) == want)
            {
                self.apply(&device, *m, cx);
            }
        }
        ui.add_space(8.0);

        let list_w = (ui.available_width() * 0.48).max(170.0);
        ui.horizontal_top(|ui| {
            // Слева — разрешения списком.
            ui.allocate_ui_with_layout(
                egui::vec2(list_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    w::section_label(ui, t.resolution);
                    egui::ScrollArea::vertical().id_salt("res").auto_shrink([false, false]).show(ui, |ui| {
                        for (i, (rw, rh)) in resolutions(&self.modes).into_iter().enumerate() {
                            let id = ui.id().with(("res", rw, rh));
                            let on = rw == cur.width && rh == cur.height;
                            let fav = cx.settings.is_favorite(&device, rw, rh);
                            let resp = anvil_ui::motion::widgets::enter(ui, ("res", rw, rh), i, |ui| {
                                row(ui, id, on, &format!("{rw} × {rh}"), aspect(rw, rh), 28.0)
                            });
                            let hint = if fav { t.star_remove } else { t.star_add };
                            if star(ui, resp.rect, id.with("star"), fav, hint) {
                                if cx.settings.toggle_favorite(&device, rw, rh).is_err() {
                                    cx.toasts.push((t.favorites_full.to_string(), Tone::Warning));
                                }
                            } else if resp.clicked() {
                                // Частота — та, что была у этого разрешения в прошлый раз.
                                let hz = cx.settings.remembered_hz(&device, rw, rh).unwrap_or(cur.hz);
                                if let Some(m) = mode_for(&self.modes, rw, rh, hz) {
                                    self.apply(&device, m, cx);
                                }
                            }
                        }
                    });
                },
            );
            ui.add_space(12.0);

            // Справа — частота и масштаб; ширина задана, чтобы пояснения переносились.
            let right = egui::vec2(ui.available_width(), ui.available_height());
            ui.allocate_ui_with_layout(right, egui::Layout::top_down(egui::Align::Min), |ui| {
                w::section_label(ui, t.refresh);
                let hz = rates(&self.modes, cur.width, cur.height);
                let labels: Vec<String> = hz.iter().map(|h| format!("{h} {}", t.hz)).collect();
                let opts: Vec<(u32, &str)> = hz.iter().zip(&labels).map(|(h, l)| (*h, l.as_str())).collect();
                let mut want = cur.hz;
                chips(ui, &mut want, &opts);
                if want != cur.hz
                    && let Some(m) = mode_for(&self.modes, cur.width, cur.height, want)
                {
                    self.apply(&device, m, cx);
                }

                ui.add_space(10.0);
                w::section_label(ui, t.scale);
                if let Some(scale) = self.scale.clone() {
                    let labels: Vec<String> = scale.options.iter().map(|s| format!("{s}%")).collect();
                    let opts: Vec<(u32, &str)> =
                        scale.options.iter().zip(&labels).map(|(s, l)| (*s, l.as_str())).collect();
                    let mut want = scale.current;
                    chips(ui, &mut want, &opts);
                    if want != scale.current {
                        match display::set_scale(&device, want) {
                            Ok(()) => {
                                self.scale = display::scale(&device);
                                cx.toasts.push((t.scale_note.to_string(), Tone::Neutral));
                            }
                            Err(e) => {
                                eprintln!("scale change failed: {e}");
                                cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                            }
                        }
                    }
                    w::note(ui, format!("{}: {}%", t.recommended, scale.recommended));
                } else {
                    w::note(ui, t.failed);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(width: u32, height: u32, hz: u32) -> Mode {
        Mode { width, height, hz, bits: 32 }
    }

    #[test]
    fn lists_and_picks() {
        let modes = [m(1920, 1080, 60), m(1920, 1080, 144), m(2560, 1440, 60), m(1280, 1024, 75)];
        assert_eq!(resolutions(&modes), vec![(2560, 1440), (1920, 1080), (1280, 1024)]);
        assert_eq!(rates(&modes, 1920, 1080), vec![60, 144]);
        assert_eq!(mode_for(&modes, 1920, 1080, 144).unwrap().hz, 144);
        // На 1440p нет 144 Гц — берём лучшее, что есть.
        assert_eq!(mode_for(&modes, 2560, 1440, 144).unwrap().hz, 60);
        assert!(mode_for(&modes, 800, 600, 60).is_none());
    }

    #[test]
    fn aspects() {
        assert_eq!(aspect(1920, 1080), "16:9");
        assert_eq!(aspect(1920, 1200), "16:10");
        assert_eq!(aspect(3440, 1440), "21:9");
        assert_eq!(aspect(1280, 1024), "5:4");
        assert_eq!(aspect(1366, 768), "16:9");
        assert_eq!(aspect(1000, 1000), "");
    }
}
