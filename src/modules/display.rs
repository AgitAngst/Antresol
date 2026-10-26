// Экран: разрешение, частота, масштаб и HDR. После смены режима — «Оставить?» с обратным отсчётом,
// как в Windows: если картинка пропала, через 15 с всё вернётся само.
// Избранное — до трёх разрешений на монитор чипами сверху. Частота запоминается для каждого
// разрешения (то, что сейчас стоит и не ждёт «Оставить»), и при переходе на него ставится она.
// HDR — кнопка в плашке режима: переключает только этот монитор, без «Оставить?» (тот же щелчок вернёт).
// Профили — режим, масштаб и HDR всех мониторов разом: сохраняются из текущего, ставятся щелчком или
// клавишей (из app.rs через `Command`), откат — тот же «Оставить?» для всех изменённых мониторов.
// Режим до «Оставить» стоит только на время (`display::apply` без записи в реестр) и в реестр попадает после ответа
// (`display::commit`): не ответили, закрыли программу (`shutdown`), она упала — настоящий режим остаётся прежним.

use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Palette, Tone};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Command, Ctx, Module, chip_grid, row};
use crate::config::{Profile, ProfileMonitor};
use crate::sys::display::{self, Mode, Scale};
use crate::sys::monitor;
use crate::texts::{self, Tr};

const REVERT_AFTER: Duration = Duration::from_secs(15);

/// Что вернуть при откате: прежний режим, масштаб и (или) HDR монитора.
struct Change {
    device: String,
    /// Режим до смены — его вернёт откат.
    mode: Option<Mode>,
    /// Режим после смены — его запомнит реестр, когда нажмут «Оставить».
    now: Option<Mode>,
    scale: Option<u32>,
    /// HDR до смены (его поменял профиль) — его вернёт откат.
    hdr: Option<bool>,
    /// Был основным этот монитор (сменили основной) — вернуть его.
    primary: Option<String>,
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
    /// HDR: `Some(включён)`, если монитор его умеет, иначе `None`.
    hdr: Option<bool>,
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
    /// HDR показанного монитора; `None` — не прочитать (кнопки нет).
    hdr: Option<display::Hdr>,
    /// Режимы, масштаб и HDR всех мониторов на момент последнего чтения.
    states: Vec<State>,
    pending: Option<Pending>,
    /// Показан полный список разрешений, а не избранное.
    all_res: bool,
}

/// Стоит ли профиль сейчас: у всех его мониторов тот же режим и (если заданы) масштаб и HDR. HDR сверяется там, где
/// монитор его умеет: чего поставить нельзя, за тем профиль не гонится. Монитора нет — профиль не стоит.
fn profile_active(p: &Profile, states: &[State]) -> bool {
    p.monitors.iter().all(|pm| {
        states.iter().any(|s| {
            s.device.eq_ignore_ascii_case(&pm.device)
                && (s.mode.width, s.mode.height, s.mode.hz) == (pm.width, pm.height, pm.hz)
                && pm.scale.is_none_or(|sc| s.scale == Some(sc))
                && pm.hdr.is_none_or(|on| s.hdr.is_none_or(|now| now == on))
        })
    })
}

/// HDR монитора для профилей: `Some(включён)`, если монитор его умеет.
fn hdr_state(device: &str) -> Option<bool> {
    display::hdr(device).filter(|h| h.supported).map(|h| h.enabled)
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

    /// Места мониторов: устройство и левый верхний угол в пикселях экрана.
    fn places(&self) -> Vec<(String, i32, i32)> {
        display::monitors(&self.panel).into_iter().map(|m| (m.device, m.rect.left, m.rect.top)).collect()
    }

    fn read_states(&mut self) {
        self.states = display::monitors(&self.panel)
            .into_iter()
            .filter_map(|m| {
                let mode = display::current(&m.device)?;
                let scale = display::scale(&m.device).map(|s| s.current);
                let hdr = hdr_state(&m.device);
                Some(State { device: m.device, mode, scale, hdr })
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
                let change = Change {
                    device: device.to_string(),
                    mode: Some(old),
                    now: Some(mode),
                    scale: None,
                    hdr: None,
                    primary: None,
                };
                self.pending = Some(Pending { changes: vec![change], at: Instant::now() });
                self.current = Some(mode);
            }
            Err(e) => {
                eprintln!("display mode change failed: {e}");
                cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
            }
        }
    }

    /// «HDR» в плашке: включить или выключить HDR этого монитора и перечитать. Без «Оставить?»: тот же щелчок вернёт
    /// назад, Windows тоже не спрашивает. Состояние перечитывается и при отказе — Windows могла передумать.
    fn toggle_hdr(&mut self, device: &str, on: bool, cx: &mut Ctx) {
        if let Err(e) = display::set_hdr(device, on) {
            eprintln!("HDR change failed: {e}");
            cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
        }
        self.hdr = display::hdr(device);
        self.read_states();
    }

    /// Профиль из того, что стоит сейчас на всех мониторах (HDR — у тех, что его умеют).
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
                hdr: s.hdr,
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
        let present = display::monitors(&self.panel);
        let mut changes = Vec::new();
        let mut missing = 0;
        let mut unavailable = 0;
        // HDR из профиля, который не поставился (Windows не даёт включить, сбой): профиль уже «не стоит».
        let mut hdr_refused = false;
        for pm in &profile.monitors {
            let Some(m) = present.iter().find(|m| m.device.eq_ignore_ascii_case(&pm.device)) else {
                missing += 1;
                continue;
            };
            let dev = m.device.clone();
            let mut change =
                Change { device: dev.clone(), mode: None, now: None, scale: None, hdr: None, primary: None };
            let want = mode_for(&display::modes(&dev), pm.width, pm.height, pm.hz);
            if want.is_none() {
                unavailable += 1;
            }
            if let Some(old) = display::current(&dev)
                && let Some(want) = want
                && want != old
            {
                match display::apply(&dev, want) {
                    Ok(()) => {
                        change.mode = Some(old);
                        change.now = Some(want);
                    }
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
            // HDR — после режима и масштаба. У профилей до 2.0 и у мониторов без HDR его не трогаем.
            if let Some(want) = pm.hdr
                && let Some(now) = display::hdr(&dev).filter(|h| h.supported)
                && want != now.enabled
            {
                if want && now.force_disabled {
                    hdr_refused = true;
                    cx.toasts.push((t.hdr_blocked.to_string(), Tone::Warning));
                } else {
                    match display::set_hdr(&dev, want) {
                        Ok(()) => change.hdr = Some(now.enabled),
                        Err(e) => {
                            hdr_refused = true;
                            eprintln!("profile HDR change failed on {dev}: {e}");
                            cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                        }
                    }
                }
            }
            if change.mode.is_some() || change.scale.is_some() || change.hdr.is_some() {
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
            if missing == 0 && unavailable == 0 && !hdr_refused {
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
        self.hdr = display::hdr(&device);
        self.read_states();
    }

    /// «Оставить»: режим, что стоит на время, — в реестр; откат отменён.
    fn keep(&mut self) {
        let Some(pending) = self.pending.take() else { return };
        for c in pending.changes {
            if let Some(m) = c.now
                && let Err(e) = display::commit(&c.device, m)
            {
                // Режим останется до перезагрузки, но в реестр не попал: сообщить некому (карточка уже закрыла окно).
                eprintln!("display mode commit failed on {}: {e}", c.device);
            }
        }
        self.after_change();
    }

    fn revert(&mut self) {
        let Some(pending) = self.pending.take() else { return };
        for c in pending.changes {
            // HDR ставился последним — возвращается первым; дальше режим и масштаб.
            if let Some(on) = c.hdr
                && let Err(e) = display::set_hdr(&c.device, on)
            {
                eprintln!("HDR revert failed on {}: {e}", c.device);
            }
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
            if let Some(old) = c.primary
                && let Err(e) = display::set_primary(&old, &self.places())
            {
                eprintln!("primary revert failed on {old}: {e}");
            }
        }
        self.after_change();
    }

    /// Сделать монитор основным — с тем же «Оставить?» и откатом через 15 с.
    fn make_primary(&mut self, device: &str, cx: &mut Ctx) {
        let Some(old) = display::monitors(&self.panel).into_iter().find(|m| m.primary).map(|m| m.device) else {
            return;
        };
        if old == device {
            return;
        }
        match display::set_primary(device, &self.places()) {
            Ok(()) => {
                let change = Change {
                    device: device.to_string(),
                    mode: None,
                    now: None,
                    scale: None,
                    hdr: None,
                    primary: Some(old),
                };
                self.pending = Some(Pending { changes: vec![change], at: Instant::now() });
                self.monitors = display::monitors(&self.panel);
                self.after_change();
            }
            Err(e) => {
                eprintln!("primary change failed: {e}");
                cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
            }
        }
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
                self.keep();
            }
            if w::button(ui, Kind::Secondary, None, t.revert).clicked() {
                self.revert();
            }
        });
    }

    /// Сохранить то, что стоит сейчас, профилем (шапка, «+»). Пустое имя — «Профиль N»;
    /// то же имя — обновить режимы, клавишу оставить.
    fn save_profile(&mut self, name: &str, cx: &mut Ctx) {
        let mut clean = Profile::clean_name(name);
        if clean.is_empty() {
            let free = (1..).find(|n| {
                let name = texts::profile_default(cx.lang, *n);
                !cx.settings.profiles.iter().any(|p| p.name == name)
            });
            clean = texts::profile_default(cx.lang, free.unwrap_or(1));
        }
        let fresh = self.snapshot(clean.clone());
        match cx.settings.profiles.iter_mut().find(|p| p.name == clean) {
            Some(p) => p.monitors = fresh.monitors,
            None => cx.settings.profiles.push(fresh),
        }
    }

    /// Строка избранного во всю ширину: звезда и «2560 × 1440 · 144»; стоящее — подложка акцента.
    fn favorite_row(ui: &mut Ui, on: bool, label: &str) -> egui::Response {
        let p = Palette::of(ui);
        let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
        let hovered = r.hovered();
        let fill = if on {
            p.badge_fill(p.accent)
        } else if hovered {
            p.hover
        } else {
            p.raised
        };
        let stroke = if on { p.accent } else { p.border };
        ui.painter().rect(rect, 6, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
        let star = Rect::from_center_size(Pos2::new(rect.left() + 17.0, rect.center().y), Vec2::splat(13.0));
        paint_star(ui.painter(), star, true, if on { p.accent_text } else { p.accent });
        let font = if on { anvil_ui::semibold(13.0) } else { egui::FontId::proportional(13.0) };
        let color = if on {
            p.accent_text
        } else if hovered {
            p.text
        } else {
            p.weak
        };
        ui.painter().text(
            Pos2::new(rect.left() + 32.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font,
            color,
        );
        w::focus_ring(ui, rect, &r, 6);
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// Ссылка-кнопка акцентным текстом со стрелкой: «Все разрешения ›», «‹ Избранное».
    fn link(ui: &mut Ui, text: &str, back: bool) -> bool {
        let p = Palette::of(ui);
        let font = egui::FontId::proportional(13.0);
        let label = if back { format!("‹ {text}") } else { format!("{text} ›") };
        let size = ui.painter().layout_no_wrap(label.clone(), font.clone(), p.accent_text).size();
        let (rect, r) = ui.allocate_exact_size(Vec2::new(size.x + 12.0, 26.0), Sense::click());
        if r.hovered() {
            ui.painter().rect_filled(rect, 6, p.hover);
        }
        ui.painter().text(
            rect.left_center() + Vec2::new(6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            font,
            p.accent_text,
        );
        w::focus_ring(ui, rect, &r, 6);
        r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    /// Кнопка «HDR Вкл» / «HDR Выкл» справа в плашке режима: высота 30, поля 10, скругление 6; «HDR» полужирным 12,5,
    /// состояние обычным. Включено — подложка акцента с рамкой акцента; выключено — `card` с рамкой `border_strong`;
    /// Windows не даёт включить — `faint` и не нажимается (подсказка — причина). Не хватает места слева — без слова
    /// состояния (оно видно по заливке); не хватает и на это — `None`, кнопки нет. Щелчок — `clicked()` ответа.
    fn hdr_button(ui: &mut Ui, hdr: display::Hdr, t: &Tr) -> Option<egui::Response> {
        let p = Palette::of(ui);
        let usable = !hdr.force_disabled;
        let (name_color, state_color) = match (hdr.enabled, usable) {
            (true, _) => (p.accent_text, p.accent_text),
            (false, true) => (p.text, p.weak),
            (false, false) => (p.faint, p.faint),
        };
        let name = ui.painter().layout_no_wrap(t.hdr.to_string(), anvil_ui::semibold(12.5), name_color);
        let state_text = if hdr.enabled { t.on } else { t.off };
        let state = ui.painter().layout_no_wrap(state_text.to_string(), egui::FontId::proportional(12.5), state_color);
        let (short, full) = (20.0 + name.size().x, 20.0 + name.size().x + 6.0 + state.size().x);
        let room = ui.available_width();
        let (width, with_state) = if full <= room {
            (full, true)
        } else if short <= room {
            (short, false)
        } else {
            return None;
        };
        let sense = if usable { Sense::click() } else { Sense::hover() };
        let (rect, r) = ui.allocate_exact_size(Vec2::new(width, 30.0), sense);
        let fill = if hdr.enabled {
            p.badge_fill(p.accent)
        } else if usable && r.hovered() {
            p.hover
        } else {
            p.card
        };
        let fill =
            anvil_ui::motion::Motion::of(ui.ctx()).color(ui.ctx(), r.id.with("fill"), fill, anvil_ui::motion::HOVER);
        let stroke = match (hdr.enabled, usable) {
            (true, _) => p.accent,
            (false, true) => p.border_strong,
            (false, false) => p.border,
        };
        ui.painter().rect(rect, 6, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
        let at = |x: f32, h: f32| Pos2::new(x, rect.center().y - h / 2.0);
        let name_at = at(rect.left() + 10.0, name.size().y);
        let state_at = at(name_at.x + name.size().x + 6.0, state.size().y);
        ui.painter().galley(name_at, name, name_color);
        if with_state {
            ui.painter().galley(state_at, state, state_color);
        }
        w::focus_ring(ui, rect, &r, 6);
        r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, usable, hdr.enabled, t.hdr));
        let r = r.on_hover_text(if usable { t.hdr_hint } else { t.hdr_blocked });
        Some(if usable { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r })
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

    /// 420, как было. Если у какого-то монитора есть HDR, плашке режима нужно место и под «HDR» — 480, а при выборе
    /// монитора (мониторов несколько) и под него, но не больше 560: полка «Экран и звук» в 1300 по-прежнему не листается.
    fn min_width(&self) -> f32 {
        if !self.states.iter().any(|s| s.hdr.is_some()) {
            return 420.0;
        }
        let picker = if self.monitors.len() > 1 { 28.0 * self.monitors.len() as f32 + 12.0 } else { 0.0 };
        (480.0 + picker).min(560.0)
    }

    fn refresh(&mut self, panel: &str) {
        if self.pending.is_some() {
            return;
        }
        self.panel = display::device_for(panel).to_string();
        self.monitors = display::monitors(&self.panel);
        self.read_states();
        if !self.monitors.iter().any(|m| m.device == self.picked) {
            self.picked.clear();
        }
        let device = self.device(&self.panel).to_string();
        self.name = display::monitor_name(&device);
        self.current = display::current(&device);
        self.modes = display::modes(&device);
        self.scale = display::scale(&device);
        // HDR — вместе с режимом: изменение извне (Win+Alt+B) видно при следующем чтении.
        self.hdr = display::hdr(&device);
    }

    fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Справа в заголовке — имя монитора («Monitor 27»); нет имени — «Монитор N» по номеру в шапке; основной при
    /// нескольких мониторах — «· основной». Устройство (`DISPLAYn`) — в подсказке.
    fn meta(&self, t: &Tr) -> String {
        let device = self.device(&self.panel);
        let n = self.monitors.iter().position(|m| m.device == device).map_or(1, |i| i + 1);
        let name = if self.name.is_empty() { format!("{} {n}", t.monitor) } else { self.name.clone() };
        let primary = self.monitors.iter().find(|m| m.device == device).is_some_and(|m| m.primary);
        if self.monitors.len() > 1 && primary { format!("{name} · {}", t.primary) } else { name }
    }

    /// Подсказка к имени: полное имя и устройство («Monitor 27 · DISPLAY1») — в заголовке имя может не поместиться.
    fn meta_hint(&self) -> String {
        let device = self.device(&self.panel).trim_start_matches(r"\\.\");
        if self.name.is_empty() { device.to_string() } else { format!("{} · {device}", self.name) }
    }

    fn active_profile(&self, profiles: &[Profile]) -> Option<usize> {
        profiles.iter().position(|p| profile_active(p, &self.states))
    }

    fn ticking(&self) -> bool {
        self.pending.is_some()
    }

    /// «Оставить этот режим?» без ответа 15 с — вернуть, даже если карточки сейчас не видно (открыта другая полка или
    /// настройки): откат не должен зависеть от того, рисуется ли она.
    fn background(&mut self, _settings: &crate::config::Settings) {
        if self.pending.as_ref().is_some_and(|p| p.at.elapsed() >= REVERT_AFTER) {
            self.revert();
        }
    }

    /// Программа закрывается, а «Оставить?» без ответа — вернуть прежний режим, масштаб и основной монитор: режим стоял
    /// только на время, но масштаб Windows запоминает сама, а закрытое окно уже не откатит ничего.
    fn shutdown(&mut self) {
        self.revert();
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        // Карточку на закрытой полке ещё не опрашивали: монитор панели — у команды.
        if self.panel.is_empty() {
            self.panel = display::device_for(cx.monitor).to_string();
        }
        match cmd {
            Command::ApplyProfile(i) => {
                if let Some(profile) = cx.settings.profiles.get(*i).cloned() {
                    self.apply_profile(&profile, cx);
                }
            }
            Command::SaveProfile(name) => self.save_profile(name, cx),
            Command::Dropped(_) | Command::Media(_) => {}
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let device = self.device(display::device_for(cx.monitor)).to_string();
        let p = Palette::of(ui);

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

        let right_w = 168.0;
        let mut make_primary = false;
        let mut toggle_hdr = None;
        let left_w = (ui.available_width() - right_w - 16.0).max(170.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.allocate_ui_with_layout(
                egui::vec2(left_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    // Что стоит сейчас — крупно; мониторов несколько — выбор справа.
                    egui::Frame::new()
                        .fill(p.raised)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(
                                        egui::RichText::new(format!("{} × {}", cur.width, cur.height))
                                            .font(anvil_ui::semibold(19.0))
                                            .color(p.text),
                                    );
                                    let scale = self.scale.as_ref().map(|s| s.current);
                                    ui.label(
                                        egui::RichText::new(texts::mode_line(cx.lang, cur.hz, scale))
                                            .size(12.5)
                                            .color(p.weak),
                                    );
                                    // Мониторов несколько и этот не основной — «Сделать основным».
                                    let primary =
                                        self.monitors.iter().find(|m| m.device == device).is_some_and(|m| m.primary);
                                    if self.monitors.len() > 1 && !primary && Self::link(ui, t.make_primary, false) {
                                        make_primary = true;
                                    }
                                });
                                // Справа по центру плашки: выбор монитора (крайний) и левее него «HDR» — если монитор
                                // его умеет.
                                let hdr = self.hdr.filter(|h| h.supported);
                                if self.monitors.len() > 1 || hdr.is_some() {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        if self.monitors.len() > 1 {
                                            let mut picked =
                                                self.monitors.iter().position(|m| m.device == device).unwrap_or(0);
                                            let names: Vec<String> =
                                                (1..=self.monitors.len()).map(|i| i.to_string()).collect();
                                            let opts: Vec<(usize, Option<anvil_ui::Icon>, &str)> = names
                                                .iter()
                                                .enumerate()
                                                .rev()
                                                .map(|(i, n)| (i, None, n.as_str()))
                                                .collect();
                                            let before = picked;
                                            w::segmented(ui, &mut picked, &opts).on_hover_text(t.monitor);
                                            if picked != before {
                                                self.picked = self.monitors[picked].device.clone();
                                                self.refresh(cx.monitor);
                                            }
                                        }
                                        if let Some(h) = hdr
                                            && Self::hdr_button(ui, h, t).is_some_and(|r| r.clicked())
                                        {
                                            toggle_hdr = Some(!h.enabled);
                                        }
                                    });
                                }
                            });
                        });
                    if self.all_res {
                        if Self::link(ui, t.favorites, true) {
                            self.all_res = false;
                        }
                        self.resolutions_ui(ui, cx, &device, cur);
                        return;
                    }
                    ui.horizontal(|ui| {
                        w::section_label(ui, t.favorites);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if Self::link(ui, t.all_res, false) {
                                self.all_res = true;
                            }
                        });
                    });
                    let favorites = cx.settings.favorites_of(&device);
                    if favorites.is_empty() {
                        w::note(ui, t.favorites_hint);
                    }
                    let mut want = None;
                    for (fw, fh) in favorites {
                        let hz = cx.settings.remembered_hz(&device, fw, fh).unwrap_or(cur.hz);
                        let m = mode_for(&self.modes, fw, fh, hz);
                        let label = match m {
                            Some(m) => format!("{fw} × {fh} · {}", m.hz),
                            None => format!("{fw} × {fh}"),
                        };
                        let on = (fw, fh) == (cur.width, cur.height);
                        if Self::favorite_row(ui, on, &label).clicked() && !on {
                            want = m;
                        }
                    }
                    if let Some(m) = want {
                        self.apply(&device, m, cx);
                    }
                },
            );

            // Справа — частота и масштаб сеткой по два.
            ui.allocate_ui_with_layout(
                egui::vec2(right_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    w::section_label(ui, t.refresh_hz);
                    let hz = rates(&self.modes, cur.width, cur.height);
                    let labels: Vec<String> = hz.iter().map(|h| h.to_string()).collect();
                    let opts: Vec<(u32, &str)> = hz.iter().zip(&labels).map(|(h, l)| (*h, l.as_str())).collect();
                    let mut want = cur.hz;
                    chip_grid(ui, &mut want, &opts, 3);
                    if want != cur.hz
                        && let Some(m) = mode_for(&self.modes, cur.width, cur.height, want)
                    {
                        self.apply(&device, m, cx);
                    }

                    ui.add_space(6.0);
                    w::section_label(ui, t.scale_pct);
                    if let Some(scale) = self.scale.clone() {
                        let labels: Vec<String> = scale.options.iter().map(|s| s.to_string()).collect();
                        let opts: Vec<(u32, &str)> =
                            scale.options.iter().zip(&labels).map(|(s, l)| (*s, l.as_str())).collect();
                        let mut want = scale.current;
                        chip_grid(ui, &mut want, &opts, 3);
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
                },
            );
        });
        if make_primary {
            self.make_primary(&device, cx);
        }
        if let Some(on) = toggle_hdr {
            self.toggle_hdr(&device, on, cx);
        }
    }
}

impl Display {
    /// Полный список разрешений со звёздочками — за ссылкой «Все разрешения».
    fn resolutions_ui(&mut self, ui: &mut Ui, cx: &mut Ctx, device: &str, cur: Mode) {
        let t = cx.t;
        egui::ScrollArea::vertical().id_salt("res").auto_shrink([false, false]).show(ui, |ui| {
            for (i, (rw, rh)) in resolutions(&self.modes).into_iter().enumerate() {
                let id = ui.id().with(("res", rw, rh));
                let on = rw == cur.width && rh == cur.height;
                let fav = cx.settings.is_favorite(device, rw, rh);
                let resp = anvil_ui::motion::widgets::enter(ui, ("res", rw, rh), i, |ui| {
                    row(ui, id, on, &format!("{rw} × {rh}"), aspect(rw, rh), 28.0)
                });
                let hint = if fav { t.star_remove } else { t.star_add };
                if star(ui, resp.rect, id.with("star"), fav, hint) {
                    if cx.settings.toggle_favorite(device, rw, rh).is_err() {
                        cx.toasts.push((t.favorites_full.to_string(), Tone::Warning));
                    }
                } else if resp.clicked() {
                    // Частота — та, что была у этого разрешения в прошлый раз.
                    let hz = cx.settings.remembered_hz(device, rw, rh).unwrap_or(cur.hz);
                    if let Some(m) = mode_for(&self.modes, rw, rh, hz) {
                        self.apply(device, m, cx);
                    }
                }
            }
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
    fn unanswered_keep_prompt_reverts_without_drawing() {
        let mut display = Display::default();
        let settings = crate::config::Settings::default();
        display.pending = Some(Pending { changes: Vec::new(), at: Instant::now() });
        display.background(&settings);
        assert!(display.pending.is_some(), "15 с ещё не прошло");
        let late = Instant::now() - REVERT_AFTER - Duration::from_secs(1);
        display.pending = Some(Pending { changes: Vec::new(), at: late });
        display.background(&settings);
        assert!(display.pending.is_none(), "без ответа 15 с — откат, хотя карточка не рисуется (другая полка)");
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
        State { device: device.into(), mode: Mode { width: w, height: h, hz, bits: 32 }, scale, hdr: None }
    }

    fn pm(device: &str, w: u32, h: u32, hz: u32, scale: Option<u32>) -> ProfileMonitor {
        ProfileMonitor { device: device.into(), width: w, height: h, hz, scale, hdr: None }
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

    #[test]
    fn active_profile_needs_the_same_hdr_where_it_is_set() {
        // A — HDR включён, B — монитор без HDR.
        let hdr_on = State { hdr: Some(true), ..st("A", 2560, 1440, 144, Some(100)) };
        let states = [hdr_on, st("B", 1920, 1080, 60, Some(125))];
        let with = |m: ProfileMonitor, hdr| ProfileMonitor { hdr, ..m };
        let a = || pm("A", 2560, 1440, 144, Some(100));
        let b = || pm("B", 1920, 1080, 60, Some(125));
        assert!(profile_active(&p(vec![a()]), &states), "HDR не записан (профиль до 2.0) — не мешает");
        assert!(profile_active(&p(vec![with(a(), Some(true))]), &states), "HDR тот же");
        assert!(!profile_active(&p(vec![with(a(), Some(false))]), &states), "HDR другой");
        assert!(profile_active(&p(vec![with(a(), Some(true)), b()]), &states));
        assert!(profile_active(&p(vec![with(b(), Some(true))]), &states), "у монитора нет HDR — гнаться не за чем");
    }
}

// ─── Откат без окна ─────────────────────────────────────────────────────────
//
// Выдуманный экран из `sys::display` помнит два режима — «сейчас» и «в реестре»: видно, что до «Оставить» в реестр
// ничего не попало и что закрытие программы возвращает прежнее. У каждого теста свой монитор — тесты друг другу не мешают.

#[cfg(test)]
mod revert_tests {
    use super::*;
    use anvil_ui::Lang;

    fn mode(width: u32, height: u32, hz: u32) -> Mode {
        Mode { width, height, hz, bits: 32 }
    }

    /// Карточка на выдуманном мониторе `device`: опрошена, как при показе панели.
    fn card(device: &str) -> Display {
        let mut display = Display::default();
        display.refresh(device);
        display
    }

    fn with_cx<R>(device: &str, settings: &mut crate::config::Settings, f: impl FnOnce(&mut Ctx) -> R) -> R {
        let mut toasts = Vec::new();
        let mut cx = Ctx { t: texts::tr(Lang::En), lang: Lang::En, monitor: device, toasts: &mut toasts, settings };
        f(&mut cx)
    }

    fn change(display: &mut Display, device: &str, want: Mode) {
        let mut settings = crate::config::Settings::default();
        with_cx(device, &mut settings, |cx| display.apply(device, want, cx));
        assert!(display.busy(), "после смены — «Оставить?»");
    }

    #[test]
    fn unconfirmed_mode_is_reverted_on_shutdown() {
        let dev = r"\\.\TEST-SHUTDOWN";
        let mut display = card(dev);
        let start = display::current(dev).unwrap();
        change(&mut display, dev, mode(1920, 1080, 144));
        assert_eq!(display::current(dev), Some(mode(1920, 1080, 144)));
        assert_eq!(display::saved(dev), start, "до «Оставить» режим стоит только на время — в реестре прежний");
        display.shutdown();
        assert!(!display.busy());
        assert_eq!(display::current(dev), Some(start), "закрыли программу без ответа — прежний режим вернулся");
        assert_eq!(display::saved(dev), start);
    }

    #[test]
    fn kept_mode_reaches_the_registry() {
        let dev = r"\\.\TEST-KEEP";
        let mut display = card(dev);
        let want = mode(2560, 1440, 120);
        change(&mut display, dev, want);
        display.keep();
        assert!(!display.busy());
        assert_eq!((display::current(dev), Some(display::saved(dev))), (Some(want), Some(want)));
        display.shutdown();
        assert_eq!(display::current(dev), Some(want), "подтверждённое выход не трогает");
    }

    #[test]
    fn revert_button_and_timeout_leave_the_registry_alone() {
        let dev = r"\\.\TEST-REVERT";
        let mut display = card(dev);
        let start = display::current(dev).unwrap();
        change(&mut display, dev, mode(1920, 1080, 60));
        display.revert();
        assert_eq!((display::current(dev), display::saved(dev)), (Some(start), start));
        // Без ответа 15 с — то же, даже если карточки не видно.
        change(&mut display, dev, mode(1280, 720, 60));
        let late = Instant::now() - REVERT_AFTER - Duration::from_secs(1);
        display.pending.as_mut().unwrap().at = late;
        display.background(&crate::config::Settings::default());
        assert!(!display.busy());
        assert_eq!((display::current(dev), display::saved(dev)), (Some(start), start));
    }

    #[test]
    fn profile_is_temporary_too_and_exit_returns_the_scale() {
        let dev = r"\\.\TEST-PROFILE";
        let mut display = card(dev);
        let start = display::current(dev).unwrap();
        let start_scale = display::scale(dev).unwrap().current;
        let mut settings = crate::config::Settings::default();
        let monitor =
            ProfileMonitor { device: dev.into(), width: 2560, height: 1440, hz: 144, scale: Some(200), hdr: None };
        settings.profiles.push(Profile { name: "Test".into(), hotkey: None, monitors: vec![monitor] });
        with_cx(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert!(display.busy());
        assert_eq!(display::current(dev), Some(mode(2560, 1440, 144)));
        assert_eq!(display::scale(dev).unwrap().current, 200);
        assert_eq!(display::saved(dev), start, "и у профиля режим до «Оставить» только на время");
        display.shutdown();
        assert_eq!(display::current(dev), Some(start));
        assert_eq!(
            display::scale(dev).unwrap().current,
            start_scale,
            "масштаб Windows помнит сама — вернуть надо вручную"
        );
        assert_eq!(display::saved(dev), start);
        // Тот же профиль и «Оставить»: режим в реестре, масштаб остаётся.
        with_cx(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        display.keep();
        assert_eq!(display::saved(dev), mode(2560, 1440, 144));
        assert_eq!(display::scale(dev).unwrap().current, 200);
        assert_eq!(display.active_profile(&settings.profiles), Some(0), "профиль стоит — его чип выделен");
    }

    /// Что карточка сказала бы уведомлениями, пока шло `f`.
    fn toasts_of(
        device: &str,
        settings: &mut crate::config::Settings,
        f: impl FnOnce(&mut Ctx),
    ) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        f(&mut Ctx { t: texts::tr(Lang::En), lang: Lang::En, monitor: device, toasts: &mut toasts, settings });
        toasts
    }

    /// Включён ли HDR у выдуманного монитора.
    fn hdr_is_on(device: &str) -> Option<bool> {
        display::hdr(device).map(|h| h.enabled)
    }

    fn profile_of(device: &str, hdr: Option<bool>) -> Profile {
        let (width, height, hz) = (3840, 2160, 120);
        let monitor = ProfileMonitor { device: device.into(), width, height, hz, scale: None, hdr };
        Profile { name: "Movie".into(), hotkey: None, monitors: vec![monitor] }
    }

    #[test]
    fn hdr_button_switches_only_this_monitor_and_does_not_ask() {
        let (dev, other) = (r"\\.\TEST-HDR-CARD", r"\\.\TEST-HDR-CARD-2");
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        assert_eq!(
            display.hdr.map(|h| (h.supported, h.enabled)),
            Some((true, false)),
            "у выдуманного монитора HDR есть"
        );
        assert!(toasts_of(dev, &mut settings, |cx| display.toggle_hdr(dev, true, cx)).is_empty());
        assert_eq!(display.hdr.map(|h| h.enabled), Some(true), "карточка перечитала HDR");
        assert!(!display.busy(), "HDR меняется без «Оставить?»");
        assert_eq!(hdr_is_on(other), Some(false), "другой монитор не тронут");
        // Тот же щелчок возвращает назад.
        toasts_of(dev, &mut settings, |cx| display.toggle_hdr(dev, false, cx));
        assert_eq!(hdr_is_on(dev), Some(false));
        // Изменение извне (Win+Alt+B) карточка видит при следующем чтении, раз в 1,5 с.
        display::set_hdr(dev, true).unwrap();
        assert_eq!(display.hdr.map(|h| h.enabled), Some(false));
        display.refresh(dev);
        assert_eq!(display.hdr.map(|h| h.enabled), Some(true));
    }

    #[test]
    fn hdr_refusal_is_a_danger_toast_and_changes_nothing() {
        let dev = r"\\.\TEST-HDR-REFUSED";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        display::fake_hdr(dev, display::Hdr { supported: true, enabled: false, force_disabled: true });
        display.refresh(dev);
        assert_eq!(display.hdr.map(|h| h.force_disabled), Some(true), "карточка знает: кнопка недоступна");
        let toasts = toasts_of(dev, &mut settings, |cx| display.toggle_hdr(dev, true, cx));
        assert!(matches!(toasts.as_slice(), [(_, Tone::Danger)]), "отказ — уведомление об ошибке");
        assert_eq!(hdr_is_on(dev), Some(false));
        assert!(!display.busy());
    }

    #[test]
    fn profile_remembers_sets_and_reverts_hdr() {
        let dev = r"\\.\TEST-HDR-PROFILE";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        // «+» при включённом HDR: профиль помнит его, и он стоит.
        display::set_hdr(dev, true).unwrap();
        toasts_of(dev, &mut settings, |cx| display.command(&Command::SaveProfile("Movie".into()), cx));
        assert_eq!(settings.profiles[0].monitors[0].hdr, Some(true));
        assert_eq!(display.active_profile(&settings.profiles), Some(0));
        // HDR выключили: профиль не стоит; щелчок по нему включает HDR (режим и масштаб те же) и спрашивает «Оставить?».
        display::set_hdr(dev, false).unwrap();
        display.refresh(dev);
        assert_eq!(display.active_profile(&settings.profiles), None, "HDR другой — чип не выделен");
        let toasts = toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert!(toasts.is_empty() && display.busy(), "{toasts:?}");
        assert_eq!(hdr_is_on(dev), Some(true));
        // «Вернуть» возвращает и HDR.
        display.revert();
        assert_eq!(hdr_is_on(dev), Some(false));
        // «Оставить» оставляет: HDR стоит, профиль стоит.
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        display.keep();
        assert_eq!((hdr_is_on(dev), display.active_profile(&settings.profiles)), (Some(true), Some(0)));
        // Профиль уже стоит целиком — «Этот профиль уже стоит».
        let again = toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert!(matches!(again.as_slice(), [(_, Tone::Neutral)]) && !display.busy());
        // Закрыли программу, не ответив, — HDR вернулся вместе с остальным.
        display::set_hdr(dev, false).unwrap();
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert_eq!(hdr_is_on(dev), Some(true));
        display.shutdown();
        assert_eq!(hdr_is_on(dev), Some(false));
    }

    #[test]
    fn profile_without_hdr_leaves_it_alone_and_sdr_turns_it_off() {
        let dev = r"\\.\TEST-HDR-OLD";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        display::set_hdr(dev, true).unwrap();
        // Профиль до 2.0: другой режим, HDR не записан — HDR не трогается ни при применении, ни при откате.
        let mut old = profile_of(dev, None);
        old.monitors[0].hz = 60;
        settings.profiles.push(old);
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert!(display.busy());
        assert_eq!(hdr_is_on(dev), Some(true));
        display.revert();
        assert_eq!(hdr_is_on(dev), Some(true));
        // `sdr` выключает HDR.
        settings.profiles.clear();
        settings.profiles.push(profile_of(dev, Some(false)));
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert_eq!(hdr_is_on(dev), Some(false));
        display.revert();
        assert_eq!(hdr_is_on(dev), Some(true), "и откат включает его обратно");
    }

    #[test]
    fn profile_hdr_that_windows_forbids_or_the_monitor_lacks_is_skipped() {
        let (dev, no_hdr) = (r"\\.\TEST-HDR-FORBIDS", r"\\.\TEST-HDR-NONE");
        let mut settings = crate::config::Settings::default();
        // Windows не даёт включить: предупреждение с причиной, HDR не тронут, «уже стоит» не говорим.
        let mut display = card(dev);
        display::fake_hdr(dev, display::Hdr { supported: true, enabled: false, force_disabled: true });
        settings.profiles.push(profile_of(dev, Some(true)));
        let toasts = toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert_eq!(toasts.len(), 1, "{toasts:?}");
        assert_eq!((toasts[0].0.as_str(), &toasts[0].1), (texts::tr(Lang::En).hdr_blocked, &Tone::Warning));
        assert!(!display.busy());
        assert_eq!(hdr_is_on(dev), Some(false));
        // У монитора нет HDR: профиль ничего не меняет, а чип выделен — гнаться не за чем.
        let mut display = card(no_hdr);
        display::fake_hdr(no_hdr, display::Hdr { supported: false, enabled: false, force_disabled: false });
        display.refresh(no_hdr);
        settings.profiles.clear();
        settings.profiles.push(profile_of(no_hdr, Some(true)));
        assert_eq!(display.active_profile(&settings.profiles), Some(0));
        let toasts = toasts_of(no_hdr, &mut settings, |cx| display.command(&Command::ApplyProfile(0), cx));
        assert!(matches!(toasts.as_slice(), [(_, Tone::Neutral)]) && !display.busy());
        // «+» у такого монитора HDR не пишет.
        toasts_of(no_hdr, &mut settings, |cx| display.command(&Command::SaveProfile("Plain".into()), cx));
        assert_eq!(settings.profiles.last().map(|p| p.monitors[0].hdr), Some(None));
    }

    #[test]
    fn fake_monitor_is_alone() {
        let dev = r"\\.\TEST-ALONE";
        let display = card(dev);
        assert_eq!(display.monitors.len(), 1, "в демо и тестах — один выдуманный монитор, настоящие не показываются");
        assert!(display.monitors[0].primary);
        assert_eq!(display.meta(texts::tr(Lang::En)), "Monitor 27");
        assert_eq!(display.meta_hint(), "Monitor 27 · TEST-ALONE", "в подсказке — полное имя и устройство");
        assert_eq!(Display::default().meta_hint(), "", "пока не опрашивали — подсказки нет");
    }
}
