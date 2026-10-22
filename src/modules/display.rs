// Экран: разрешение, частота и масштаб. После смены режима — «Оставить?» с обратным отсчётом,
// как в Windows: если картинка пропала, через 15 с всё вернётся само.
// Избранное — до трёх разрешений на монитор чипами сверху. Частота запоминается для каждого
// разрешения (то, что сейчас стоит и не ждёт «Оставить»), и при переходе на него ставится она.
// Профили — режим и масштаб всех мониторов разом: сохраняются из текущего, ставятся щелчком или
// клавишей (из app.rs через `Command`), откат — тот же «Оставить?» для всех изменённых мониторов.

use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Palette, Tone};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Command, Ctx, Module, chips, row};
use crate::config::{Profile, ProfileMonitor};
use crate::sys::display::{self, Mode, Scale};
use crate::sys::monitor;
use crate::texts::{self, Tr};

const REVERT_AFTER: Duration = Duration::from_secs(15);

/// Что вернуть при откате: прежний режим и (или) масштаб монитора.
struct Change {
    device: String,
    mode: Option<Mode>,
    scale: Option<u32>,
}

/// Режим поменяли и ждём «Оставить»: что вернуть и когда поменяли.
struct Pending {
    changes: Vec<Change>,
    at: Instant,
}

/// Что сейчас на мониторе — для «какой профиль стоит» без опроса Windows каждый кадр.
#[derive(Clone)]
struct State {
    device: String,
    mode: Mode,
    scale: Option<u32>,
}

#[derive(Default)]
pub struct Display {
    /// Какой монитор показываем; пусто — тот, у которого панель.
    picked: String,
    /// Монитор панели с прошлого `refresh` — чтобы перечитать после отката.
    panel: String,
    monitors: Vec<monitor::Monitor>,
    name: String,
    current: Option<Mode>,
    modes: Vec<Mode>,
    scale: Option<Scale>,
    /// Режимы и масштаб всех мониторов на момент последнего чтения.
    states: Vec<State>,
    pending: Option<Pending>,
    /// Вводят имя нового профиля.
    naming: Option<String>,
}

/// Стоит ли профиль сейчас: у всех его мониторов тот же режим и (если задан) масштаб.
/// Монитора нет — профиль не стоит.
fn profile_active(p: &Profile, states: &[State]) -> bool {
    p.monitors.iter().all(|pm| {
        states.iter().any(|s| {
            s.device.eq_ignore_ascii_case(&pm.device)
                && (s.mode.width, s.mode.height, s.mode.hz) == (pm.width, pm.height, pm.hz)
                && pm.scale.is_none_or(|sc| s.scale == Some(sc))
        })
    })
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

    fn read_states(&mut self) {
        self.states = monitor::monitors()
            .into_iter()
            .filter_map(|m| {
                let mode = display::current(&m.device)?;
                let scale = display::scale(&m.device).map(|s| s.current);
                Some(State { device: m.device, mode, scale })
            })
            .collect();
    }

    fn apply(&mut self, device: &str, mode: Mode, cx: &mut Ctx) {
        let Some(old) = self.current else { return };
        if mode == old {
            return;
        }
        match display::apply(device, mode) {
            Ok(()) => {
                let change = Change { device: device.to_string(), mode: Some(old), scale: None };
                self.pending = Some(Pending { changes: vec![change], at: Instant::now() });
                self.current = Some(mode);
            }
            Err(e) => {
                eprintln!("display mode change failed: {e}");
                cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
            }
        }
    }

    /// Профиль из того, что стоит сейчас на всех мониторах.
    fn snapshot(&mut self, name: String) -> Profile {
        self.read_states();
        let monitors = self
            .states
            .iter()
            .map(|s| ProfileMonitor {
                device: s.device.clone(),
                width: s.mode.width,
                height: s.mode.height,
                hz: s.mode.hz,
                scale: s.scale,
            })
            .collect();
        Profile { name, hotkey: None, monitors }
    }

    /// Поставить профиль на все его мониторы; изменённое — в откат.
    fn apply_profile(&mut self, profile: &Profile, cx: &mut Ctx) {
        let t = cx.t;
        if self.pending.is_some() {
            cx.toasts.push((t.answer_first.to_string(), Tone::Warning));
            return;
        }
        let present = monitor::monitors();
        let mut changes = Vec::new();
        let mut missing = 0;
        let mut unavailable = 0;
        for pm in &profile.monitors {
            let Some(m) = present.iter().find(|m| m.device.eq_ignore_ascii_case(&pm.device)) else {
                missing += 1;
                continue;
            };
            let dev = m.device.clone();
            let mut change = Change { device: dev.clone(), mode: None, scale: None };
            let want = mode_for(&display::modes(&dev), pm.width, pm.height, pm.hz);
            if want.is_none() {
                unavailable += 1;
            }
            if let Some(old) = display::current(&dev)
                && let Some(want) = want
                && want != old
            {
                match display::apply(&dev, want) {
                    Ok(()) => change.mode = Some(old),
                    Err(e) => {
                        eprintln!("profile mode change failed on {dev}: {e}");
                        cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                    }
                }
            }
            if let (Some(want), Some(old)) = (pm.scale, display::scale(&dev).map(|s| s.current))
                && want != old
            {
                match display::set_scale(&dev, want) {
                    Ok(()) => change.scale = Some(old),
                    Err(e) => {
                        eprintln!("profile scale change failed on {dev}: {e}");
                        cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                    }
                }
            }
            if change.mode.is_some() || change.scale.is_some() {
                changes.push(change);
            }
        }
        if missing > 0 {
            cx.toasts.push((texts::monitors_missing(cx.lang, missing), Tone::Warning));
        }
        if unavailable > 0 {
            cx.toasts.push((t.mode_unavailable.to_string(), Tone::Warning));
        }
        if changes.is_empty() {
            if missing == 0 && unavailable == 0 {
                cx.toasts.push((t.profile_already.to_string(), Tone::Neutral));
            }
            return;
        }
        self.pending = Some(Pending { changes, at: Instant::now() });
        self.after_change();
    }

    /// Перечитать то, что показываем, после смены или отката.
    fn after_change(&mut self) {
        let device = self.device(&self.panel).to_string();
        self.current = display::current(&device);
        self.modes = display::modes(&device);
        self.scale = display::scale(&device);
        self.read_states();
    }

    fn revert(&mut self) {
        let Some(pending) = self.pending.take() else { return };
        for c in pending.changes {
            if let Some(m) = c.mode
                && let Err(e) = display::apply(&c.device, m)
            {
                eprintln!("display mode revert failed on {}: {e}", c.device);
            }
            if let Some(s) = c.scale
                && let Err(e) = display::set_scale(&c.device, s)
            {
                eprintln!("scale revert failed on {}: {e}", c.device);
            }
        }
        self.after_change();
    }

    fn keep_ui(&mut self, ui: &mut Ui, t: &Tr, lang: anvil_ui::Lang) {
        let Some(at) = self.pending.as_ref().map(|p| p.at) else { return };
        let left = REVERT_AFTER.saturating_sub(at.elapsed());
        if left.is_zero() {
            self.revert();
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        let secs = left.as_secs() + 1;
        w::banner(ui, Tone::Warning, t.keep_title, &texts::revert_in(lang, secs), |ui| {
            if w::button(ui, Kind::Primary, None, t.keep).clicked() {
                self.pending = None;
                self.after_change();
            }
            if w::button(ui, Kind::Secondary, None, t.revert).clicked() {
                self.revert();
            }
        });
    }

    /// Профили чипами и «+» — сохранить текущее под именем.
    fn profiles_ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        w::section_label(ui, t.profiles);
        if let Some(mut name) = self.naming.take() {
            let mut done = None;
            ui.horizontal_wrapped(|ui| {
                let edit = ui.add(egui::TextEdit::singleline(&mut name).hint_text(t.profile_name).desired_width(140.0));
                if name.is_empty() && !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if w::button(ui, Kind::Secondary, Some(anvil_ui::Icon::Save), t.save).clicked() || enter {
                    done = Some(true);
                }
                if w::button(ui, Kind::Ghost, None, t.cancel).clicked() {
                    done = Some(false);
                }
            });
            match done {
                Some(true) => {
                    let mut clean = Profile::clean_name(&name);
                    if clean.is_empty() {
                        let free = (1..).find(|n| {
                            let name = texts::profile_default(cx.lang, *n);
                            !cx.settings.profiles.iter().any(|p| p.name == name)
                        });
                        clean = texts::profile_default(cx.lang, free.unwrap_or(1));
                    }
                    let fresh = self.snapshot(clean.clone());
                    // То же имя — обновить режимы, клавишу оставить.
                    match cx.settings.profiles.iter_mut().find(|p| p.name == clean) {
                        Some(p) => p.monitors = fresh.monitors,
                        None => cx.settings.profiles.push(fresh),
                    }
                }
                Some(false) => {}
                None => self.naming = Some(name),
            }
            return;
        }
        let mut apply = None;
        ui.horizontal_wrapped(|ui| {
            if cx.settings.profiles.is_empty() {
                w::note(ui, t.profiles_hint);
            } else {
                let names: Vec<String> = cx.settings.profiles.iter().map(|p| p.name.clone()).collect();
                let active = cx.settings.profiles.iter().position(|p| profile_active(p, &self.states));
                let opts: Vec<(Option<usize>, &str)> =
                    names.iter().enumerate().map(|(i, n)| (Some(i), n.as_str())).collect();
                let mut want = active;
                if chips(ui, &mut want, &opts) {
                    apply = want;
                }
            }
            if w::icon_button(ui, anvil_ui::Icon::Plus, t.profile_save).clicked() {
                self.naming = Some(String::new());
            }
        });
        if let Some(i) = apply {
            let profile = cx.settings.profiles[i].clone();
            self.apply_profile(&profile, cx);
        }
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

    fn min_width(&self) -> f32 {
        420.0
    }

    fn refresh(&mut self, panel: &str) {
        if self.pending.is_some() {
            return;
        }
        self.panel = panel.to_string();
        self.monitors = monitor::monitors();
        self.read_states();
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

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        match cmd {
            Command::ApplyProfile(i) => {
                if let Some(profile) = cx.settings.profiles.get(*i).cloned() {
                    self.apply_profile(&profile, cx);
                }
            }
            Command::Dropped(_) => {}
        }
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
            self.keep_ui(ui, t, cx.lang);
            return;
        }
        let Some(cur) = self.current else {
            w::note(ui, t.failed);
            return;
        };
        // Стоит и не ждёт ответа — значит, это частота этого разрешения.
        cx.settings.remember_hz(&device, cur.width, cur.height, cur.hz);

        let list_w = (ui.available_width() * 0.48).max(170.0);
        ui.horizontal_top(|ui| {
            // Слева — разрешения списком.
            ui.allocate_ui_with_layout(
                egui::vec2(list_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
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
                self.profiles_ui(ui, cx);
                ui.add_space(10.0);
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
                                self.read_states();
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

#[cfg(test)]
mod profile_tests {
    use super::*;

    fn st(device: &str, w: u32, h: u32, hz: u32, scale: Option<u32>) -> State {
        State { device: device.into(), mode: Mode { width: w, height: h, hz, bits: 32 }, scale }
    }

    fn pm(device: &str, w: u32, h: u32, hz: u32, scale: Option<u32>) -> ProfileMonitor {
        ProfileMonitor { device: device.into(), width: w, height: h, hz, scale }
    }

    fn p(monitors: Vec<ProfileMonitor>) -> Profile {
        Profile { name: "p".into(), hotkey: None, monitors }
    }

    #[test]
    fn active_profile() {
        let states = [st("A", 2560, 1440, 144, Some(100)), st("B", 1920, 1080, 60, Some(125))];
        assert!(profile_active(&p(vec![pm("A", 2560, 1440, 144, Some(100))]), &states));
        assert!(profile_active(&p(vec![pm("a", 2560, 1440, 144, None), pm("B", 1920, 1080, 60, Some(125))]), &states));
        assert!(!profile_active(&p(vec![pm("A", 2560, 1440, 60, None)]), &states), "другая частота");
        assert!(!profile_active(&p(vec![pm("A", 2560, 1440, 144, Some(125))]), &states), "другой масштаб");
        assert!(!profile_active(&p(vec![pm("C", 800, 600, 60, None)]), &states), "монитора нет");
    }
}
