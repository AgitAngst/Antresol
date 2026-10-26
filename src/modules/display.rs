// Экран: разрешение, частота, масштаб и HDR. После смены режима — «Оставить?» с обратным отсчётом,
// как в Windows: если картинка пропала, через 15 с всё вернётся само.
// Избранное — до трёх разрешений на монитор чипами сверху. Частота запоминается для каждого
// разрешения (то, что сейчас стоит и не ждёт «Оставить»), и при переходе на него ставится она.
// HDR — кнопка в плашке режима: переключает только этот монитор, без «Оставить?» (тот же щелчок вернёт).
// Сцены — экран (режим, масштаб и HDR всех мониторов разом) и системные настройки: экранную часть ставит эта карточка
// (из app.rs через `Command`), откат — тот же «Оставить?» для всех изменённых мониторов. Перед сменой карточка помнит,
// что было (`before`): второй щелчок по стоящей сцене возвращает это сразу, в реестр и без «Оставить?» — прежний режим
// был рабочим. Отказ от режима, что поставила сцена («Вернуть», 15 секунд, закрытие), — отказ от сцены целиком: карточка
// отдаёт её номер (`take_refused_scene`), и панель возвращает остальное из сцены, как вторым щелчком.
// Режим до «Оставить» стоит только на время (`display::apply` без записи в реестр) и в реестр попадает после ответа
// (`display::commit`): не ответили, закрыли программу (`shutdown`), она упала — настоящий режим остаётся прежним.

use std::time::{Duration, Instant};

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Lang, Palette, Tone};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Action, Command, Ctx, Module, chip_grid, mine, row};
use crate::config::{Profile, ProfileMonitor, SceneRow, Settings};
use crate::sys::display::{self, Mode, Scale};
use crate::sys::monitor;
use crate::texts::{self, Tr};

const REVERT_AFTER: Duration = Duration::from_secs(15);

/// `Id` кнопки «HDR» в плашке режима: «Экран» на панели один, так что хватает одного.
pub fn hdr_button_id() -> egui::Id {
    egui::Id::new("hdr-toggle")
}

/// Наименьшая ширина карточки при `monitors` мониторах. Без HDR ни у кого — 420, как было. С HDR плашке режима нужно место
/// под кнопку: у одного монитора — 480 (кнопка целиком рядом с «120 Гц · масштаб 150 %»), у двух — 548 (целиком и рядом с
/// номерами мониторов), у трёх и больше — столько, чтобы влезла двустрочная кнопка (448 и по 28 на монитор), но не меньше
/// 548. До четырёх мониторов это не больше 560 — полка «Экран и звук» в 1300 не листается; дальше шире.
pub fn min_width_for(monitors: usize, hdr: bool) -> f32 {
    if !hdr {
        return 420.0;
    }
    match monitors {
        0 | 1 => 480.0,
        n => (448.0 + 28.0 * n as f32).max(548.0),
    }
}

/// Что вернуть при откате: прежний режим, масштаб и (или) HDR монитора.
#[derive(Clone)]
struct Change {
    device: String,
    /// Режим до смены — его вернёт откат.
    mode: Option<Mode>,
    /// Режим после смены — его запомнит реестр, когда нажмут «Оставить».
    now: Option<Mode>,
    scale: Option<u32>,
    /// HDR до смены (его поменяла сцена) — его вернёт откат.
    hdr: Option<bool>,
    /// Был основным этот монитор (сменили основной) — вернуть его.
    primary: Option<String>,
}

/// Режим поменяли и ждём «Оставить»: что вернуть и когда поменяли.
struct Pending {
    changes: Vec<Change>,
    at: Instant,
    /// Номер сцены ([`Profile::id`]), что поставила этот режим (щелчок по чипу, клавиша); `None` — режим меняли в самой
    /// карточке.
    scene: Option<u64>,
}

/// Как было до сцены (экран): монитор — режим, масштаб и HDR, что стояли. Живёт, пока не вернули, не поставили другую
/// сцену или не отказались от режима («Вернуть», 15 секунд); перезапуск программы его забывает.
struct SceneBefore {
    /// Номер сцены: переименование возврат не теряет, новая сцена с тем же именем чужой не наследует.
    scene: u64,
    changes: Vec<Change>,
}

/// Что сейчас на мониторе — для «какая сцена стоит» без опроса Windows каждый кадр.
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
    /// Как было до сцены, что стоит сейчас, — на второй щелчок по ней.
    before: Option<SceneBefore>,
    /// Отказались от режима, что поставила эта сцена: ждёт, пока панель заберёт (`take_refused_scene`).
    refused: Option<u64>,
    /// Показан полный список разрешений, а не избранное.
    all_res: bool,
}

/// Стоит ли экранная часть сцены сейчас: у всех её мониторов тот же режим и (если заданы) масштаб и HDR. HDR сверяется там,
/// где монитор его умеет: чего поставить нельзя, за тем сцена не гонится. Монитора нет — не стоит.
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

/// Кнопку «HDR» можно нажать: выключить HDR можно всегда, а включить — пока Windows этого не запрещает.
fn hdr_usable(hdr: display::Hdr) -> bool {
    hdr.enabled || !hdr.force_disabled
}

/// HDR монитора для сцен: `Some(включён)`, если монитор его умеет.
fn hdr_state(device: &str) -> Option<bool> {
    display::hdr(device).filter(|h| h.supported).map(|h| h.enabled)
}

/// «125 %» / «125%» — масштаб в названии действия.
fn percent_text(lang: Lang, percent: u32) -> String {
    match lang {
        Lang::Ru => format!("{percent} %"),
        Lang::En => format!("{percent}%"),
    }
}

/// Что сказать об отказе смены HDR: своим текстом программы, а английский служебный — только в журнал. Windows не даёт
/// включить — `hdr_blocked` (предупреждение); её сбой на самом вызове — «Не получилось: код»; прочее — «Не получилось».
fn hdr_toast(e: &display::HdrError, t: &Tr) -> (String, Tone) {
    use display::HdrError::*;
    match e {
        Forbidden => (t.hdr_blocked.to_string(), Tone::Warning),
        Windows(code) => (format!("{}: {code}", t.failed), Tone::Danger),
        NotSupported | Unavailable => (t.failed.to_string(), Tone::Danger),
    }
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
                self.pending = Some(Pending { changes: vec![change], at: Instant::now(), scene: None });
                self.current = Some(mode);
            }
            Err(e) => {
                eprintln!("display mode change failed: {e}");
                cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
            }
        }
    }

    /// «HDR» в плашке: включить или выключить HDR этого монитора и перечитать. Без «Оставить?»: тот же щелчок вернёт
    /// назад, Windows тоже не спрашивает. Состояние перечитывается и при отказе — Windows могла передумать (например,
    /// запретила включать между чтением и щелчком: тогда карточка узнает об этом тут же).
    fn toggle_hdr(&mut self, device: &str, on: bool, cx: &mut Ctx) {
        if let Err(e) = display::set_hdr(device, on) {
            eprintln!("HDR change failed: {e}");
            cx.toasts.push(hdr_toast(&e, cx.t));
        }
        self.hdr = display::hdr(device);
        self.read_states();
    }

    /// Что стоит сейчас на всех мониторах — экранная часть сцены (HDR — у тех, что его умеют).
    fn current_monitors(&mut self) -> Vec<ProfileMonitor> {
        self.read_states();
        self.states
            .iter()
            .map(|s| ProfileMonitor {
                device: s.device.clone(),
                width: s.mode.width,
                height: s.mode.height,
                hz: s.mode.hz,
                scale: s.scale,
                hdr: s.hdr,
            })
            .collect()
    }

    /// Запомнить, как было до сцены `scene`: что она только что поменяла. Та же сцена ещё раз (поставили, а потом
    /// поставили снова) продолжает прежний «сеанс»: у монитора, что уже помним, остаётся то, что было до первой постановки;
    /// возврат вернёт всё к тому, что было до сцены, а не до её второго раза.
    fn remember(&mut self, scene: u64, changes: &[Change]) {
        let mut kept = self.before.take().filter(|b| b.scene == scene).map(|b| b.changes).unwrap_or_default();
        for c in changes {
            match kept.iter_mut().find(|k| k.device.eq_ignore_ascii_case(&c.device)) {
                Some(k) => {
                    k.mode = k.mode.or(c.mode);
                    k.scale = k.scale.or(c.scale);
                    k.hdr = k.hdr.or(c.hdr);
                }
                None => kept.push(c.clone()),
            }
        }
        self.before = (!kept.is_empty()).then_some(SceneBefore { scene, changes: kept });
    }

    /// Поставить экранную часть сцены на все её мониторы; изменённое — в откат и в `before`. Что поставить сцена не
    /// отметила (экрана нет) — ничего, а возврат прежней сцены пропадает.
    fn apply_scene(&mut self, scene: &Profile, cx: &mut Ctx) {
        let t = cx.t;
        if self.pending.is_some() {
            cx.toasts.push((t.answer_first.to_string(), Tone::Warning));
            return;
        }
        if self.before.as_ref().is_some_and(|b| b.scene != scene.id) {
            self.before = None;
        }
        if scene.monitors.is_empty() {
            return;
        }
        let present = display::monitors(&self.panel);
        let mut changes = Vec::new();
        let mut missing = 0;
        let mut unavailable = 0;
        for pm in &scene.monitors {
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
                        eprintln!("scene mode change failed on {dev}: {e}");
                        cx.toasts.push((texts::scene_failed(t, SceneRow::Screen), Tone::Danger));
                    }
                }
            }
            if let (Some(want), Some(old)) = (pm.scale, display::scale(&dev).map(|s| s.current))
                && want != old
            {
                match display::set_scale(&dev, want) {
                    Ok(()) => change.scale = Some(old),
                    Err(e) => {
                        eprintln!("scene scale change failed on {dev}: {e}");
                        cx.toasts.push((texts::scene_failed(t, SceneRow::Screen), Tone::Danger));
                    }
                }
            }
            // HDR — после режима и масштаба. У сцен (профилей) до 2.0 и у мониторов без HDR его не трогаем.
            if let Some(want) = pm.hdr
                && let Some(now) = display::hdr(&dev).filter(|h| h.supported)
                && want != now.enabled
            {
                match display::set_hdr(&dev, want) {
                    Ok(()) => change.hdr = Some(now.enabled),
                    Err(e) => {
                        eprintln!("scene HDR change failed on {dev}: {e}");
                        cx.toasts.push(hdr_toast(&e, t));
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
        // «Эта сцена уже стоит» говорит панель: стоит ли она вся, знает только она, а не одна карточка.
        if changes.is_empty() {
            return;
        }
        self.remember(scene.id, &changes);
        self.pending = Some(Pending { changes, at: Instant::now(), scene: Some(scene.id) });
        self.after_change();
    }

    /// Вернуть экран, как был до сцены: режим — сразу и в реестр (это прежний рабочий режим, «Оставить?» не нужен),
    /// масштаб и HDR — как были. Висит «Оставить?» от этой же сцены — это тот же откат, что «Вернуть», только панель сама
    /// возвращает и остальное из сцены, так что отказом это не считается. Чужой вопрос висит — сначала на него ответить,
    /// и возврат остаётся (панель до этого не доводит: `blocks_undo`).
    fn undo_scene(&mut self, scene: &Profile, cx: &mut Ctx) {
        if !self.scene_can_undo(scene) {
            return;
        }
        match &self.pending {
            Some(p) if p.scene == Some(scene.id) => {
                self.roll_back();
                return;
            }
            Some(_) => {
                cx.toasts.push((cx.t.answer_first.to_string(), Tone::Warning));
                return;
            }
            None => {}
        }
        let Some(before) = self.before.take() else { return };
        for c in before.changes {
            // Как в `revert`: HDR ставился последним — возвращается первым; дальше режим и масштаб.
            if let Some(on) = c.hdr
                && let Err(e) = display::set_hdr(&c.device, on)
            {
                eprintln!("HDR undo failed on {}: {e}", c.device);
                cx.toasts.push(hdr_toast(&e, cx.t));
            }
            if let Some(m) = c.mode
                && let Err(e) = display::apply(&c.device, m).and_then(|()| display::commit(&c.device, m))
            {
                eprintln!("display mode undo failed on {}: {e}", c.device);
                cx.toasts.push((texts::scene_failed(cx.t, SceneRow::Screen), Tone::Danger));
            }
            if let Some(s) = c.scale
                && let Err(e) = display::set_scale(&c.device, s)
            {
                eprintln!("scale undo failed on {}: {e}", c.device);
                cx.toasts.push((texts::scene_failed(cx.t, SceneRow::Screen), Tone::Danger));
            }
        }
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

    /// Отказ: «Вернуть», 15 секунд без ответа, закрытие программы. Всё, что ждало «Оставить?», возвращается как было. Если
    /// режим поставила сцена, это отказ от сцены целиком: номер ждёт панель (`take_refused_scene`), она вернёт остальное.
    fn revert(&mut self) {
        if let Some(scene) = self.roll_back() {
            self.refused = Some(scene);
        }
    }

    /// Вернуть всё, что ждало «Оставить?», как было. Ответ — номер сцены, если режим поставила она (экран ей уже
    /// возвращён, так что «как было» по экрану забывается).
    fn roll_back(&mut self) -> Option<u64> {
        let pending = self.pending.take()?;
        if let Some(scene) = pending.scene
            && self.before.as_ref().is_some_and(|b| b.scene == scene)
        {
            self.before = None;
        }
        let scene = pending.scene;
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
        scene
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
                self.pending = Some(Pending { changes: vec![change], at: Instant::now(), scene: None });
                self.monitors = display::monitors(&self.panel);
                self.after_change();
            }
            Err(e) => {
                eprintln!("primary change failed: {e}");
                cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
            }
        }
    }

    /// Масштаб монитора: чип в карточке и действие поиска. Windows запоминает его сама, «Оставить?» нет.
    fn change_scale(&mut self, device: &str, want: u32, cx: &mut Ctx) {
        match display::set_scale(device, want) {
            Ok(()) => {
                self.scale = display::scale(device);
                self.read_states();
                cx.toasts.push((cx.t.scale_note.to_string(), Tone::Neutral));
            }
            Err(e) => {
                eprintln!("scale change failed: {e}");
                cx.toasts.push((format!("{}: {e}", cx.t.failed), Tone::Danger));
            }
        }
    }

    /// Режимы монитора для поиска: сперва избранное (с запомненной частотой, как в карточке), дальше все остальные от
    /// большего разрешения к меньшему и от большей частоты к меньшей; стоящий сейчас не в счёт.
    fn modes_for_search(&self, device: &str, cur: Mode, settings: &Settings) -> Vec<(Mode, bool)> {
        let mut out: Vec<(Mode, bool)> = Vec::new();
        for (fw, fh) in settings.favorites_of(device) {
            let hz = settings.remembered_hz(device, fw, fh).unwrap_or(cur.hz);
            if let Some(m) = mode_for(&self.modes, fw, fh, hz)
                && m != cur
                && !out.iter().any(|(o, _)| *o == m)
            {
                out.push((m, true));
            }
        }
        let mut rest: Vec<Mode> = self.modes.iter().copied().filter(|m| *m != cur).collect();
        rest.sort_by_key(|m| std::cmp::Reverse((m.width * m.height, m.width, m.hz)));
        for m in rest {
            if !out.iter().any(|(o, _)| (o.width, o.height, o.hz) == (m.width, m.height, m.hz)) {
                out.push((m, false));
            }
        }
        out
    }

    /// Действие поиска «Экрана»: то же, что щелчок в карточке. Висит «Оставить?» — сначала ответ, как у сцен.
    fn run_action(&mut self, what: &str, cx: &mut Ctx) {
        let t = cx.t;
        if self.pending.is_some() {
            cx.toasts.push((t.answer_first.to_string(), Tone::Warning));
            return;
        }
        let device = self.device(&self.panel).to_string();
        let mut parts = what.split(':');
        match (parts.next(), parts.next()) {
            (Some("mode"), Some(mode)) => {
                let found = self.modes.iter().copied().find(|m| format!("{}x{}@{}", m.width, m.height, m.hz) == mode);
                if let Some(m) = found {
                    self.apply(&device, m, cx);
                }
            }
            (Some("scale"), Some(percent)) => {
                if let Ok(want) = percent.parse::<u32>()
                    && self.scale.as_ref().is_some_and(|s| s.current != want && s.options.contains(&want))
                {
                    self.change_scale(&device, want, cx);
                }
            }
            (Some("hdr"), None) => {
                if let Some(h) = self.hdr.filter(|h| h.supported) {
                    if hdr_usable(h) {
                        self.toggle_hdr(&device, !h.enabled, cx);
                    } else {
                        cx.toasts.push((t.hdr_blocked.to_string(), Tone::Warning));
                    }
                }
            }
            (Some("primary"), None) => self.make_primary(&device, cx),
            _ => {}
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
            let keep = w::button(ui, Kind::Primary, None, t.keep);
            super::anchor(ui, keep.rect, "keep-yes");
            if keep.clicked() {
                self.keep();
            }
            let revert = w::button(ui, Kind::Secondary, None, t.revert);
            super::anchor(ui, revert.rect, "keep-revert");
            if revert.clicked() {
                self.revert();
            }
        });
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

    /// Кнопка «HDR Вкл» / «HDR Выкл» справа в плашке режима: высота 30, скругление 6. Слово состояния есть всегда — цвет
    /// не единственный носитель смысла. Одной строкой: поля 10, «HDR» полужирным 12,5 и через 6 состояние обычным. Не
    /// хватает места слева (рядом с номерами мониторов) — двумя строками: «HDR», под ним состояние, обе 11, поля 8; не
    /// хватает и на это — `None`, кнопки нет. Включено — подложка акцента с рамкой акцента; выключено — `card` с рамкой
    /// `border_strong`; Windows не даёт включить — `faint` и не нажимается (подсказка — причина). Щелчок — `clicked()`.
    fn hdr_button(ui: &mut Ui, hdr: display::Hdr, t: &Tr) -> Option<egui::Response> {
        let p = Palette::of(ui);
        let usable = hdr_usable(hdr);
        let (name_color, state_color) = match (hdr.enabled, usable) {
            (true, _) => (p.accent_text, p.accent_text),
            (false, true) => (p.text, p.weak),
            (false, false) => (p.faint, p.faint),
        };
        let state_text = if hdr.enabled { t.on } else { t.off };
        let lay =
            |text: &str, font: egui::FontId, color: Color32| ui.painter().layout_no_wrap(text.to_string(), font, color);
        let proportional = egui::FontId::proportional;
        let (name, state) =
            (lay(t.hdr, anvil_ui::semibold(12.5), name_color), lay(state_text, proportional(12.5), state_color));
        let (name2, state2) =
            (lay(t.hdr, anvil_ui::semibold(11.0), name_color), lay(state_text, proportional(11.0), state_color));
        let full = 20.0 + name.size().x + 6.0 + state.size().x;
        let narrow = 16.0 + name2.size().x.max(state2.size().x);
        let room = ui.available_width();
        let two_lines = if full <= room {
            false
        } else if narrow <= room {
            true
        } else {
            return None;
        };
        let width = if two_lines { narrow } else { full };
        // Свой `Id` — чтобы тест без окна находил кнопку.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::hover());
        let r = ui.interact(rect, hdr_button_id(), if usable { Sense::click() } else { Sense::hover() });
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
        if two_lines {
            // «HDR» над состоянием, обе строки по центру кнопки.
            let top = rect.center().y - (name2.size().y + state2.size().y) / 2.0;
            let name_at = Pos2::new(rect.center().x - name2.size().x / 2.0, top);
            let state_at = Pos2::new(rect.center().x - state2.size().x / 2.0, top + name2.size().y);
            ui.painter().galley(name_at, name2, name_color);
            ui.painter().galley(state_at, state2, state_color);
        } else {
            let at = |x: f32, h: f32| Pos2::new(x, rect.center().y - h / 2.0);
            let name_at = at(rect.left() + 10.0, name.size().y);
            let state_at = at(name_at.x + name.size().x + 6.0, state.size().y);
            ui.painter().galley(name_at, name, name_color);
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

    /// 420, как было; если у какого-то монитора есть HDR — см. [`min_width_for`].
    fn min_width(&self) -> f32 {
        min_width_for(self.monitors.len(), self.states.iter().any(|s| s.hdr.is_some()))
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

    /// Экран сцены стоит: режим, масштаб и (где монитор его умеет) HDR всех её мониторов. Экрана в сцене нет — `None`.
    fn scene_part(&self, scene: &Profile) -> Option<bool> {
        (!scene.monitors.is_empty()).then(|| profile_active(scene, &self.states))
    }

    fn scene_can_undo(&self, scene: &Profile) -> bool {
        self.before.as_ref().is_some_and(|b| b.scene == scene.id)
    }

    /// «Экран»: строка сцены есть, если хоть один монитор прочитан.
    fn scene_rows(&self) -> Vec<(SceneRow, bool)> {
        vec![(SceneRow::Screen, !self.states.is_empty())]
    }

    /// «Оставить этот режим?» ждёт ответа — новую сцену не ставят.
    fn blocks_scene(&self) -> bool {
        self.pending.is_some()
    }

    /// Вопрос висит не от этой сцены (режим поменяли в самой карточке, «Сделать основным», другая сцена): возврат сцены
    /// ждёт ответа и не начинается ни у кого. Вопрос от самой сцены возврату не мешает — это тот же откат.
    fn blocks_undo(&self, scene: &Profile) -> bool {
        self.pending.as_ref().is_some_and(|p| p.scene != Some(scene.id))
    }

    fn take_refused_scene(&mut self) -> Option<u64> {
        self.refused.take()
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
            Command::ApplyScene(i) => {
                if let Some(scene) = cx.settings.profiles.get(*i).cloned() {
                    self.apply_scene(&scene, cx);
                }
            }
            Command::UndoScene(i) => {
                if let Some(scene) = cx.settings.profiles.get(*i).cloned() {
                    self.undo_scene(&scene, cx);
                }
            }
            Command::SaveScene(i, rows) => {
                if rows.contains(&SceneRow::Screen) {
                    let monitors = self.current_monitors();
                    // Не прочиталось ничего — прежнее в сцене не стираем.
                    if let Some(scene) = cx.settings.profiles.get_mut(*i).filter(|_| !monitors.is_empty()) {
                        scene.monitors = monitors;
                    }
                }
            }
            Command::RunAction(id) => {
                if let Some(what) = mine(self.key(), id) {
                    self.run_action(what, cx);
                }
            }
            Command::Dropped(_) | Command::Media(_) => {}
        }
    }

    /// Режимы монитора (избранные первыми), масштабы, HDR и «Сделать основным» — то, что в карточке. Пока висит «Оставить
    /// этот режим?», карточка отвечает только ему, и действия серые.
    fn actions(&self, t: &Tr, lang: Lang, settings: &Settings) -> Vec<Action> {
        let Some(cur) = self.current else { return Vec::new() };
        let device = self.device(&self.panel).to_string();
        let waiting = self.pending.is_some();
        let mut out = Vec::new();
        for (m, favorite) in self.modes_for_search(&device, cur, settings) {
            let mode = texts::scene_mode(lang, m.width, m.height, m.hz, None, None);
            let size = format!("{}x{}", m.width, m.height);
            let mut words =
                format!("разрешение режим частота resolution mode refresh {size} {size}@{} {}p", m.hz, m.height);
            if (m.width, m.height) == (3840, 2160) {
                words += " 4k uhd";
            }
            if favorite {
                words += " избранное favorite";
            }
            let title = format!("{}: {mode}", t.display);
            let action = Action::new(format!("display:mode:{size}@{}", m.hz), title, t.display, Icon::Monitor);
            out.push(action.words(&words).off_if(waiting, t.answer_first));
        }
        if let Some(scale) = &self.scale {
            for &percent in scale.options.iter().filter(|&&p| p != scale.current) {
                let title = format!("{}: {}", t.scale_word, percent_text(lang, percent));
                let action = Action::new(format!("display:scale:{percent}"), title, t.display, Icon::Monitor);
                out.push(action.words("масштаб увеличение dpi scale zoom").off_if(waiting, t.answer_first));
            }
        }
        if let Some(h) = self.hdr.filter(|h| h.supported) {
            let title = format!("{}: {}", t.hdr, if h.enabled { t.turn_off } else { t.turn_on });
            let action = Action::new("display:hdr", title, t.display, Icon::Monitor).words("hdr цвет color");
            out.push(action.off_if(!hdr_usable(h), t.hdr_blocked).off_if(waiting, t.answer_first));
        }
        let primary = self.monitors.iter().find(|m| m.device == device).is_some_and(|m| m.primary);
        if self.monitors.len() > 1 && !primary {
            let action = Action::new("display:primary", t.make_primary, t.display, Icon::Monitor);
            out.push(action.words("основной монитор primary main").off_if(waiting, t.answer_first));
        }
        out
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
                            self.change_scale(&device, want, cx);
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
        display.pending = Some(Pending { changes: Vec::new(), at: Instant::now(), scene: None });
        display.background(&settings);
        assert!(display.pending.is_some(), "15 с ещё не прошло");
        let late = Instant::now() - REVERT_AFTER - Duration::from_secs(1);
        display.pending = Some(Pending { changes: Vec::new(), at: late, scene: None });
        display.background(&settings);
        assert!(display.pending.is_none(), "без ответа 15 с — откат, хотя карточка не рисуется (другая полка)");
    }

    #[test]
    fn hdr_button_is_off_limits_only_when_windows_forbids_turning_it_on() {
        let hdr = |enabled, force_disabled| display::Hdr { supported: true, enabled, force_disabled };
        assert!(hdr_usable(hdr(false, false)) && hdr_usable(hdr(true, false)));
        assert!(!hdr_usable(hdr(false, true)), "Windows не даёт включить — кнопка недоступна");
        assert!(hdr_usable(hdr(true, true)), "включённый HDR выключить можно всегда");
    }

    #[test]
    fn card_is_wider_only_when_a_monitor_has_hdr() {
        let mut display = Display::default();
        assert_eq!(display.min_width(), 420.0, "пока ничего не прочитано — как было");
        let state = |hdr| State { device: "A".into(), mode: m(1920, 1080, 60), scale: None, hdr };
        display.states = vec![state(None)];
        assert_eq!(display.min_width(), 420.0, "HDR нет ни у одного монитора — как было");
        display.states = vec![state(Some(false))];
        assert_eq!(display.min_width(), 480.0, "кнопке «HDR» нужно место рядом с режимом");
        let rect = monitor::PxRect { left: 0, top: 0, right: 1, bottom: 1 };
        let mon = |device: &str| monitor::Monitor { device: device.into(), rect, primary: false, handle: 0 };
        display.monitors = vec![mon("A"), mon("B")];
        assert_eq!(display.min_width(), 548.0, "и выбору монитора, кнопка по-прежнему целиком");
        display.monitors = (0..4).map(|i| mon(&i.to_string())).collect();
        assert_eq!(display.min_width(), 560.0, "четыре монитора — двустрочной кнопке; «Экран и звук» влезает в 1300");
        display.monitors = (0..9).map(|i| mon(&i.to_string())).collect();
        assert!(display.min_width() > 560.0, "ещё больше мониторов — шире: двустрочной кнопке тоже нужно место");
        let widths: Vec<f32> = (1..=9).map(|n| min_width_for(n, true)).collect();
        assert!(widths.windows(2).all(|w| w[0] <= w[1]), "с мониторами карточка не уже: {widths:?}");
    }

    #[test]
    fn hdr_failures_are_told_in_the_programs_words() {
        use display::HdrError::*;
        for lang in [anvil_ui::Lang::Ru, anvil_ui::Lang::En] {
            let t = texts::tr(lang);
            // Windows не даёт включить — своя фраза (предупреждение), а не английское служебное.
            assert_eq!(hdr_toast(&Forbidden, t), (t.hdr_blocked.to_string(), Tone::Warning));
            // Сбой на самом вызове — «Не получилось» и код; прочее — просто «Не получилось».
            assert_eq!(hdr_toast(&Windows(87), t), (format!("{}: 87", t.failed), Tone::Danger));
            assert_eq!(hdr_toast(&NotSupported, t), (t.failed.to_string(), Tone::Danger));
            assert_eq!(hdr_toast(&Unavailable, t), (t.failed.to_string(), Tone::Danger));
            for e in [Forbidden, Windows(5), NotSupported, Unavailable] {
                let text = hdr_toast(&e, t).0;
                assert!(!text.contains("DisplayConfig") && !text.contains("HDR is not"), "служебное в окне: {text}");
            }
        }
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
        Profile { monitors, ..Profile::new("p") }
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

// ─── Кнопка «HDR» на узкой плашке ───────────────────────────────────────────

#[cfg(test)]
mod button_tests {
    use super::*;
    use anvil_ui::Lang;

    /// Кнопка «HDR» при `room` свободных точек слева от неё (справа налево, как в плашке): её прямоугольник и тексты,
    /// что на ней нарисованы.
    fn button(room: f32, hdr: display::Hdr, lang: Lang) -> (Option<Rect>, Vec<String>) {
        let settings = crate::config::Settings::default();
        let ctx = egui::Context::default();
        anvil_ui::install(&ctx, crate::look::accent_of(&settings.accent), settings.common.theme);
        let (mut rect, mut drawn) = (None, Vec::new());
        // Шрифты встают со следующего кадра, поэтому берём последний.
        for _ in 0..4 {
            let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 200.0));
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                let layout = egui::Layout::right_to_left(egui::Align::Center);
                ui.allocate_ui_with_layout(Vec2::new(room, 40.0), layout, |ui| {
                    rect = Display::hdr_button(ui, hdr, texts::tr(lang)).map(|r| r.rect);
                });
            });
            drawn = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                    _ => None,
                })
                .collect();
            out.textures_delta.clear();
        }
        (rect, drawn)
    }

    #[test]
    fn hdr_button_keeps_its_state_word_and_shrinks_to_two_lines() {
        let off = display::Hdr { supported: true, enabled: false, force_disabled: false };
        let on = display::Hdr { enabled: true, ..off };
        for (lang, hdr) in [(Lang::Ru, off), (Lang::En, off), (Lang::Ru, on), (Lang::En, on)] {
            let t = texts::tr(lang);
            let words = vec![t.hdr.to_string(), (if hdr.enabled { t.on } else { t.off }).to_string()];
            // Места вдоволь — одна строка: «HDR Выкл».
            let (full, drawn) = button(400.0, hdr, lang);
            let full = full.expect("места вдоволь");
            assert_eq!((full.height(), &drawn), (30.0, &words));
            // Чуть не хватает на одну строку — две: уже, высота та же, слово состояния не пропало.
            let (narrow, drawn) = button(full.width() - 1.0, hdr, lang);
            let narrow = narrow.expect("двустрочная помещается");
            assert!(narrow.width() < full.width() - 20.0 && narrow.height() == 30.0, "{narrow:?} против {full:?}");
            assert_eq!(drawn, words, "слово состояния не пропало, даже когда тесно");
            // Не хватает и на две — кнопки нет.
            assert_eq!(button(narrow.width() - 1.0, hdr, lang).0, None);
        }
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
        settings.profiles.push(Profile { monitors: vec![monitor], ..Profile::new("Test") });
        with_cx(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
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
        // Та же сцена и «Оставить»: режим в реестре, масштаб остаётся.
        with_cx(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        display.keep();
        assert_eq!(display::saved(dev), mode(2560, 1440, 144));
        assert_eq!(display::scale(dev).unwrap().current, 200);
        assert_eq!(display.scene_part(&settings.profiles[0]), Some(true), "сцена по экрану стоит — её чип выделен");
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
        Profile { monitors: vec![monitor], ..Profile::new("Movie") }
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
    fn hdr_refusal_is_told_in_the_programs_words_and_changes_nothing() {
        let (dev, no_hdr) = (r"\\.\TEST-HDR-REFUSED", r"\\.\TEST-HDR-NO-HDR");
        let mut settings = crate::config::Settings::default();
        let en = texts::tr(anvil_ui::Lang::En);
        let forbidden = display::Hdr { supported: true, enabled: false, force_disabled: true };
        // Карточка уже знает, что включить нельзя: кнопка недоступна (`hdr_usable`), а если щёлкнули — отказ по-человечески.
        let mut display = card(dev);
        display::fake_hdr(dev, forbidden);
        display.refresh(dev);
        assert_eq!(display.hdr.map(|h| h.force_disabled), Some(true));
        let toasts = toasts_of(dev, &mut settings, |cx| display.toggle_hdr(dev, true, cx));
        assert_eq!(
            toasts,
            vec![(en.hdr_blocked.to_string(), Tone::Warning)],
            "не английское служебное «…does not allow…»"
        );
        assert_eq!(hdr_is_on(dev), Some(false));
        assert!(!display.busy());
        // Запретили между чтением и щелчком: карточка ещё не знает, узнаёт на щелчке — тот же ответ, и состояние перечитано.
        let later = r"\\.\TEST-HDR-REFUSED-LATER";
        let mut display = card(later);
        display::fake_hdr(later, forbidden);
        assert_eq!(display.hdr.map(|h| h.force_disabled), Some(false), "пока не перечитали — прежнее");
        let toasts = toasts_of(later, &mut settings, |cx| display.toggle_hdr(later, true, cx));
        assert_eq!(toasts, vec![(en.hdr_blocked.to_string(), Tone::Warning)]);
        assert_eq!(
            display.hdr.map(|h| h.force_disabled),
            Some(true),
            "после отказа карточка знает, что включить нельзя"
        );
        // Монитору HDR не нужен вовсе (кнопки нет, но щёлкнули бы) — просто «Failed», без английского служебного.
        display::fake_hdr(no_hdr, display::Hdr { supported: false, ..forbidden });
        let mut display = card(no_hdr);
        let toasts = toasts_of(no_hdr, &mut settings, |cx| display.toggle_hdr(no_hdr, true, cx));
        assert_eq!(toasts, vec![(en.failed.to_string(), Tone::Danger)]);
    }

    #[test]
    fn profile_remembers_sets_and_reverts_hdr() {
        let dev = r"\\.\TEST-HDR-PROFILE";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        // «+» при включённом HDR: сцена помнит его, и он стоит.
        display::set_hdr(dev, true).unwrap();
        settings.profiles.push(Profile::new("Movie"));
        toasts_of(dev, &mut settings, |cx| display.command(&Command::SaveScene(0, vec![SceneRow::Screen]), cx));
        assert_eq!(settings.profiles[0].monitors[0].hdr, Some(true));
        assert_eq!(display.scene_part(&settings.profiles[0]), Some(true));
        // HDR выключили: сцена по экрану не стоит; щелчок по ней включает HDR (режим и масштаб те же) и спрашивает «Оставить?».
        display::set_hdr(dev, false).unwrap();
        display.refresh(dev);
        assert_eq!(display.scene_part(&settings.profiles[0]), Some(false), "HDR другой — чип не выделен");
        let toasts = toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert!(toasts.is_empty() && display.busy(), "{toasts:?}");
        assert_eq!(hdr_is_on(dev), Some(true));
        // «Вернуть» возвращает и HDR.
        display.revert();
        assert_eq!(hdr_is_on(dev), Some(false));
        // «Оставить» оставляет: HDR стоит, сцена стоит.
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        display.keep();
        assert_eq!((hdr_is_on(dev), display.scene_part(&settings.profiles[0])), (Some(true), Some(true)));
        // Сцена уже стоит целиком — менять нечего: вопроса нет, лишних слов нет («уже стоит» говорит панель).
        let again = toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert!(again.is_empty() && !display.busy(), "{again:?}");
        // Закрыли программу, не ответив, — HDR вернулся вместе с остальным.
        display::set_hdr(dev, false).unwrap();
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
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
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert!(display.busy());
        assert_eq!(hdr_is_on(dev), Some(true));
        display.revert();
        assert_eq!(hdr_is_on(dev), Some(true));
        // `sdr` выключает HDR.
        settings.profiles.clear();
        settings.profiles.push(profile_of(dev, Some(false)));
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
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
        let toasts = toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert_eq!(toasts.len(), 1, "{toasts:?}");
        assert_eq!((toasts[0].0.as_str(), &toasts[0].1), (texts::tr(Lang::En).hdr_blocked, &Tone::Warning));
        assert!(!display.busy());
        assert_eq!(hdr_is_on(dev), Some(false));
        // У монитора нет HDR: сцена ничего не меняет, а чип выделен — гнаться не за чем.
        let mut display = card(no_hdr);
        display::fake_hdr(no_hdr, display::Hdr { supported: false, enabled: false, force_disabled: false });
        display.refresh(no_hdr);
        settings.profiles.clear();
        settings.profiles.push(profile_of(no_hdr, Some(true)));
        assert_eq!(display.scene_part(&settings.profiles[0]), Some(true));
        let toasts = toasts_of(no_hdr, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert!(toasts.is_empty() && !display.busy(), "{toasts:?}");
        // «+» у такого монитора HDR не пишет.
        settings.profiles.push(Profile::new("Plain"));
        toasts_of(no_hdr, &mut settings, |cx| display.command(&Command::SaveScene(1, vec![SceneRow::Screen]), cx));
        assert_eq!(settings.profiles.last().map(|p| p.monitors[0].hdr), Some(None));
    }

    // ─── Сцены: как было и возврат ──────────────────────────────────────────

    /// Сцена с экраном: 2560 × 1440 · 144 Гц, масштаб 200 %, HDR включить.
    fn movie(dev: &str) -> Profile {
        let monitor = ProfileMonitor {
            device: dev.into(),
            width: 2560,
            height: 1440,
            hz: 144,
            scale: Some(200),
            hdr: Some(true),
        };
        Profile { monitors: vec![monitor], ..Profile::new("Movie") }
    }

    #[test]
    fn scene_undo_returns_the_screen_after_keep_into_the_registry() {
        let dev = r"\\.\TEST-SCENE-UNDO";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        let start = display::current(dev).unwrap();
        let start_scale = display::scale(dev).unwrap().current;
        assert!(!display.scene_can_undo(&settings.profiles[0]), "пока не ставили — вернуть нечего");
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        display.keep();
        assert_eq!(display::saved(dev), mode(2560, 1440, 144));
        assert!(display.scene_can_undo(&settings.profiles[0]), "«Оставить» возврат не стирает");
        assert_eq!(display.scene_part(&settings.profiles[0]), Some(true));
        // Второй щелчок: прежнее — сразу и в реестр, без «Оставить?».
        let toasts = toasts_of(dev, &mut settings, |cx| display.command(&Command::UndoScene(0), cx));
        assert!(toasts.is_empty() && !display.busy(), "{toasts:?}");
        assert_eq!((display::current(dev), display::saved(dev)), (Some(start), start));
        assert_eq!(display::scale(dev).unwrap().current, start_scale);
        assert_eq!(hdr_is_on(dev), Some(false));
        assert_eq!(display.scene_part(&settings.profiles[0]), Some(false), "сцена по экрану больше не стоит");
        // Возврат одноразовый: второй раз экран не трогают.
        assert!(!display.scene_can_undo(&settings.profiles[0]));
        display::apply(dev, mode(1920, 1080, 60)).unwrap();
        toasts_of(dev, &mut settings, |cx| display.command(&Command::UndoScene(0), cx));
        assert_eq!(display::current(dev), Some(mode(1920, 1080, 60)));
    }

    #[test]
    fn scene_undo_while_the_prompt_waits_is_the_same_as_revert() {
        let dev = r"\\.\TEST-SCENE-UNDO-PROMPT";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        let start = display::current(dev).unwrap();
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert!(display.busy() && display.blocks_scene(), "до ответа новую сцену не ставят");
        toasts_of(dev, &mut settings, |cx| display.command(&Command::UndoScene(0), cx));
        assert!(!display.busy() && !display.blocks_scene());
        assert_eq!((display::current(dev), display::saved(dev)), (Some(start), start));
        assert_eq!(hdr_is_on(dev), Some(false));
        assert!(!display.scene_can_undo(&settings.profiles[0]));
    }

    #[test]
    fn declining_the_mode_forgets_the_return_and_hands_the_scene_to_the_panel() {
        let dev = r"\\.\TEST-SCENE-DECLINE";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        let id = settings.profiles[0].id;
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert!(display.scene_can_undo(&settings.profiles[0]));
        assert_eq!(display.take_refused_scene(), None, "пока вопрос висит, отказа нет");
        // «Вернуть»: по экрану сцены нет, возвращать нечего; а остальное из сцены вернёт панель — ей отдан номер сцены.
        display.revert();
        assert!(!display.scene_can_undo(&settings.profiles[0]));
        assert_eq!(display.take_refused_scene(), Some(id));
        assert_eq!(display.take_refused_scene(), None, "номер отдаётся один раз");
    }

    /// Отказ от сцены — «Вернуть», 15 секунд без ответа и закрытие программы; не отказ — второй щелчок по сцене (панель сама
    /// возвращает всё) и режим, что поменяли в самой карточке.
    #[test]
    fn refusal_comes_from_the_button_the_timeout_and_the_shutdown_only() {
        let dev = r"\\.\TEST-SCENE-REFUSAL";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        let id = settings.profiles[0].id;
        let apply = |display: &mut Display, settings: &mut crate::config::Settings| {
            toasts_of(dev, settings, |cx| display.command(&Command::ApplyScene(0), cx));
            assert!(display.busy());
        };
        apply(&mut display, &mut settings);
        let late = Instant::now() - REVERT_AFTER - Duration::from_secs(1);
        display.pending.as_mut().unwrap().at = late;
        display.background(&settings);
        assert!(!display.busy());
        assert_eq!(display.take_refused_scene(), Some(id), "15 секунд без ответа");
        apply(&mut display, &mut settings);
        display.shutdown();
        assert_eq!(display.take_refused_scene(), Some(id), "закрытие программы");
        apply(&mut display, &mut settings);
        toasts_of(dev, &mut settings, |cx| display.command(&Command::UndoScene(0), cx));
        assert!(!display.busy());
        assert_eq!(display.take_refused_scene(), None, "второй щелчок — не отказ: панель возвращает сцену сама");
        change(&mut display, dev, mode(1920, 1080, 60));
        display.revert();
        assert_eq!(display.take_refused_scene(), None, "режим из самой карточки — не сцена");
    }

    /// Возврат сцены ждёт, пока висит чужой вопрос; свой вопрос сцены ему не мешает.
    #[test]
    fn a_foreign_question_blocks_the_return_but_the_scenes_own_does_not() {
        let dev = r"\\.\TEST-SCENE-BLOCKS-UNDO";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        settings.profiles.push(Profile::new("Other"));
        let (scene, other) = (settings.profiles[0].clone(), settings.profiles[1].clone());
        assert!(!display.blocks_undo(&scene), "вопроса нет — не мешает");
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert!(!display.blocks_undo(&scene), "вопрос от самой сцены");
        assert!(display.blocks_undo(&other), "а для другой сцены он чужой");
        display.keep();
        change(&mut display, dev, mode(1920, 1080, 60));
        assert!(display.blocks_undo(&scene), "режим из карточки — чужой вопрос");
        display.keep();
        assert!(!display.blocks_undo(&scene));
    }

    #[test]
    fn another_scene_replaces_the_return() {
        let dev = r"\\.\TEST-SCENE-OTHER";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        settings.profiles.push(Profile {
            system: crate::config::SceneSystem { quiet: Some(true), ..Default::default() },
            ..Profile::new("Quiet")
        });
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        display.keep();
        assert!(display.scene_can_undo(&settings.profiles[0]));
        // Другая сцена, даже без экрана: возврат прежней пропадает.
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(1), cx));
        assert!(!display.scene_can_undo(&settings.profiles[0]) && !display.scene_can_undo(&settings.profiles[1]));
        assert_eq!(display.scene_part(&settings.profiles[1]), None, "в сцене нет экрана — модулю нечего проверять");
    }

    #[test]
    fn the_same_scene_again_returns_to_what_was_before_the_first_time() {
        let dev = r"\\.\TEST-SCENE-AGAIN";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        let start = display::current(dev).unwrap();
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        display.keep();
        // Режим ушёл от сцены сам (поменяли руками), и сцену ставят снова.
        change(&mut display, dev, mode(1920, 1080, 60));
        display.keep();
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        assert_eq!(display::current(dev), Some(mode(2560, 1440, 144)));
        display.keep();
        // Возврат — к тому, что было до сцены, а не до её второго раза.
        toasts_of(dev, &mut settings, |cx| display.command(&Command::UndoScene(0), cx));
        assert_eq!(display::current(dev), Some(start));
    }

    #[test]
    fn scene_undo_waits_for_a_foreign_prompt() {
        let dev = r"\\.\TEST-SCENE-FOREIGN";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(movie(dev));
        toasts_of(dev, &mut settings, |cx| display.command(&Command::ApplyScene(0), cx));
        display.keep();
        // Режим поменяли в самой карточке и ещё не ответили: чужой вопрос не трогаем, возврат остаётся.
        change(&mut display, dev, mode(1920, 1080, 60));
        let toasts = toasts_of(dev, &mut settings, |cx| display.command(&Command::UndoScene(0), cx));
        assert_eq!(toasts, vec![(texts::tr(Lang::En).answer_first.to_string(), Tone::Warning)]);
        assert!(display.busy() && display.scene_can_undo(&settings.profiles[0]));
        assert_eq!(display::current(dev), Some(mode(1920, 1080, 60)));
    }

    #[test]
    fn save_scene_takes_the_screen_that_stands_now() {
        let dev = r"\\.\TEST-SCENE-SAVE";
        let mut display = card(dev);
        let mut settings = crate::config::Settings::default();
        settings.profiles.push(Profile::new("New"));
        change(&mut display, dev, mode(1920, 1080, 144));
        display.keep();
        display::set_hdr(dev, true).unwrap();
        // Другие строки карточка не трогает: просят только систему — экран в сцене не появляется.
        toasts_of(dev, &mut settings, |cx| display.command(&Command::SaveScene(0, vec![SceneRow::Quiet]), cx));
        assert!(settings.profiles[0].monitors.is_empty());
        toasts_of(dev, &mut settings, |cx| display.command(&Command::SaveScene(0, vec![SceneRow::Screen]), cx));
        let m = &settings.profiles[0].monitors;
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].width, m[0].height, m[0].hz, m[0].hdr), (1920, 1080, 144, Some(true)));
        assert_eq!(m[0].scale, display::scale(dev).map(|s| s.current));
        // Карточка видит сцену стоящей и говорит, что строка «Экран» ей по силам; номера, которого нет, не бывает.
        assert_eq!(display.scene_part(&settings.profiles[0]), Some(true));
        assert_eq!(display.scene_rows(), vec![(SceneRow::Screen, true)]);
        toasts_of(dev, &mut settings, |cx| display.command(&Command::SaveScene(7, vec![SceneRow::Screen]), cx));
        assert_eq!(settings.profiles.len(), 1);
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

// ─── Действия поиска ────────────────────────────────────────────────────────
//
// «Экран» отдаёт режимы, масштабы, HDR и «Сделать основным»; выполнение — те же функции, что щелчок в карточке. У каждого
// теста свой выдуманный монитор.

#[cfg(test)]
mod search_tests {
    use super::*;

    const EN: Lang = Lang::En;

    fn card(device: &str) -> Display {
        let mut display = Display::default();
        display.refresh(device);
        display
    }

    fn titles(actions: &[Action]) -> Vec<&str> {
        actions.iter().map(|a| a.title.as_str()).collect()
    }

    /// Выполнить действие; ответ — уведомления.
    fn run(display: &mut Display, settings: &mut Settings, dev: &str, id: &str) -> Vec<(String, Tone)> {
        let mut toasts = Vec::new();
        let mut cx = Ctx { t: texts::tr(EN), lang: EN, monitor: dev, toasts: &mut toasts, settings };
        display.command(&Command::RunAction(id.to_string()), &mut cx);
        toasts
    }

    #[test]
    fn modes_scales_and_hdr_are_offered_and_the_standing_mode_is_not() {
        let dev = r"\\.\TEST-SEARCH-LIST";
        let display = card(dev);
        let t = texts::tr(EN);
        let actions = display.actions(t, EN, &Settings::default());
        let list = titles(&actions);
        // Выдуманный монитор стоит на 4K 120 Гц: этот режим не предлагается, остальные — от большего к меньшему.
        assert_eq!(
            list[..4],
            [
                "Display: 3840 × 2160 · 60 Hz",
                "Display: 2560 × 1440 · 144 Hz",
                "Display: 2560 × 1440 · 120 Hz",
                "Display: 2560 × 1440 · 60 Hz"
            ]
        );
        assert_eq!(actions.iter().filter(|a| a.id.starts_with("display:mode:")).count(), 9);
        assert!(!list.contains(&"Display: 3840 × 2160 · 120 Hz"));
        // Масштабы, кроме стоящего (150 %), и HDR; монитор один — «Сделать основным» нет.
        assert!(list.contains(&"Scale: 100%") && list.contains(&"Scale: 225%") && !list.contains(&"Scale: 150%"));
        assert!(list.contains(&"HDR: turn on"));
        assert!(!list.contains(&t.make_primary));
        assert!(actions.iter().all(|a| a.enabled && a.reason.is_none() && a.place == t.display));
        // Русский: единицы переводятся.
        let ru = display.actions(texts::tr(Lang::Ru), Lang::Ru, &Settings::default());
        assert!(titles(&ru).contains(&"Экран: 1920 × 1080 · 144 Гц"));
        assert!(titles(&ru).contains(&"Масштаб: 125 %"));
        assert!(titles(&ru).contains(&"HDR: включить"));
        // Слова для поиска — оба языка и размер в одном виде («1080p», «1920x1080»).
        let mode = actions.iter().find(|a| a.id == "display:mode:1920x1080@144").expect("режим есть");
        for word in ["разрешение", "resolution", "1920x1080", "1080p"] {
            assert!(mode.words.contains(word), "нет слова «{word}»: {}", mode.words);
        }
        let uhd = actions.iter().find(|a| a.id == "display:mode:3840x2160@60").expect("режим есть");
        assert!(uhd.words.contains("4k"));
    }

    #[test]
    fn favorites_come_first_with_their_remembered_rate() {
        let dev = r"\\.\TEST-SEARCH-FAVORITES";
        let display = card(dev);
        let mut settings = Settings::default();
        settings.toggle_favorite(dev, 1920, 1080).unwrap();
        settings.toggle_favorite(dev, 1280, 720).unwrap();
        settings.remember_hz(dev, 1920, 1080, 120);
        let actions = display.actions(texts::tr(EN), EN, &settings);
        let list = titles(&actions);
        assert_eq!(list[..2], ["Display: 1920 × 1080 · 120 Hz", "Display: 1280 × 720 · 60 Hz"]);
        assert!(actions[0].words.contains("favorite") && actions[1].words.contains("избранное"));
        // Остальные режимы идут после избранного, и ни один режим не повторяется.
        assert!(list.contains(&"Display: 1920 × 1080 · 144 Hz"));
        let mut ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), actions.len());
    }

    #[test]
    fn a_mode_action_does_what_the_click_does_and_asks_keep() {
        let dev = r"\\.\TEST-SEARCH-MODE";
        let mut display = card(dev);
        let t = texts::tr(EN);
        let start = display::current(dev).unwrap();
        let mut settings = Settings::default();
        let toasts = run(&mut display, &mut settings, dev, "display:mode:1920x1080@144");
        assert!(toasts.is_empty(), "{toasts:?}");
        assert_eq!(display::current(dev).map(|m| (m.width, m.height, m.hz)), Some((1920, 1080, 144)));
        assert_eq!(display::saved(dev), start, "до «Оставить» режим стоит только на время, в реестре прежний");
        assert!(display.busy(), "и спрашивает «Оставить этот режим?»");
        // Пока вопрос висит, карточка отвечает только ему: действия серые, новое не выполняется.
        let actions = display.actions(t, EN, &settings);
        assert!(
            !actions.is_empty() && actions.iter().all(|a| !a.enabled && a.reason.as_deref() == Some(t.answer_first))
        );
        let toasts = run(&mut display, &mut settings, dev, "display:mode:1280x720@60");
        assert_eq!(toasts, vec![(t.answer_first.to_string(), Tone::Warning)]);
        assert_eq!(display::current(dev).map(|m| (m.width, m.hz)), Some((1920, 144)));
        // «Оставить» — режим в реестре; стоящий режим больше не предлагается, прежний — предлагается.
        display.keep();
        assert_eq!(display::saved(dev).hz, 144);
        let list = display.actions(t, EN, &settings);
        assert!(!titles(&list).contains(&"Display: 1920 × 1080 · 144 Hz"));
        assert!(titles(&list).contains(&"Display: 3840 × 2160 · 120 Hz"));
        // Режима, которого нет у монитора, не бывает: ничего не меняется.
        let before = display::current(dev);
        assert!(run(&mut display, &mut settings, dev, "display:mode:123x456@60").is_empty());
        assert_eq!(display::current(dev), before);
        assert!(!display.busy());
    }

    #[test]
    fn a_scale_action_does_what_the_chip_does() {
        let dev = r"\\.\TEST-SEARCH-SCALE";
        let mut display = card(dev);
        let t = texts::tr(EN);
        let mut settings = Settings::default();
        let toasts = run(&mut display, &mut settings, dev, "display:scale:125");
        assert_eq!(toasts, vec![(t.scale_note.to_string(), Tone::Neutral)]);
        assert_eq!(display::scale(dev).map(|s| s.current), Some(125));
        assert!(!display.busy(), "масштаб «Оставить?» не спрашивает");
        let list = display.actions(t, EN, &settings);
        assert!(titles(&list).contains(&"Scale: 150%") && !titles(&list).contains(&"Scale: 125%"));
        // Такого масштаба нет, или он уже стоит — ничего.
        assert!(run(&mut display, &mut settings, dev, "display:scale:130").is_empty());
        assert!(run(&mut display, &mut settings, dev, "display:scale:125").is_empty());
        assert_eq!(display::scale(dev).map(|s| s.current), Some(125));
    }

    #[test]
    fn the_hdr_action_flips_this_monitor_and_obeys_windows() {
        let dev = r"\\.\TEST-SEARCH-HDR";
        let mut display = card(dev);
        let t = texts::tr(EN);
        let mut settings = Settings::default();
        assert!(run(&mut display, &mut settings, dev, "display:hdr").is_empty());
        assert_eq!(display::hdr(dev).map(|h| h.enabled), Some(true));
        assert!(!display.busy(), "HDR без «Оставить?»: обратим тем же щелчком");
        let list = display.actions(t, EN, &settings);
        assert!(titles(&list).contains(&"HDR: turn off"));
        run(&mut display, &mut settings, dev, "display:hdr");
        assert_eq!(display::hdr(dev).map(|h| h.enabled), Some(false));
        // Windows не даёт включить: действие серое, с причиной; выполнение — предупреждение, HDR не тронут.
        display::fake_hdr(dev, display::Hdr { supported: true, enabled: false, force_disabled: true });
        display.refresh(dev);
        let list = display.actions(t, EN, &settings);
        let hdr = list.iter().find(|a| a.id == "display:hdr").expect("действие есть");
        assert!(!hdr.enabled && hdr.reason.as_deref() == Some(t.hdr_blocked));
        let toasts = run(&mut display, &mut settings, dev, "display:hdr");
        assert_eq!(toasts, vec![(t.hdr_blocked.to_string(), Tone::Warning)]);
        assert_eq!(display::hdr(dev).map(|h| h.enabled), Some(false));
        // У монитора нет HDR — и действия нет.
        display::fake_hdr(dev, display::Hdr { supported: false, enabled: false, force_disabled: false });
        display.refresh(dev);
        assert!(!display.actions(t, EN, &settings).iter().any(|a| a.id == "display:hdr"));
    }

    #[test]
    fn make_primary_is_offered_only_for_a_second_monitor_that_is_not_primary() {
        let dev = r"\\.\TEST-SEARCH-PRIMARY";
        let mut display = card(dev);
        let t = texts::tr(EN);
        let rect = monitor::PxRect { left: 0, top: 0, right: 1, bottom: 1 };
        let mon = |device: &str, primary| monitor::Monitor { device: device.into(), rect, primary, handle: 0 };
        // Мониторов два и этот основной — не предлагается; не основной — да.
        display.monitors = vec![mon(dev, true), mon("B", false)];
        assert!(!titles(&display.actions(t, EN, &Settings::default())).contains(&t.make_primary));
        display.monitors = vec![mon("B", true), mon(dev, false)];
        let list = display.actions(t, EN, &Settings::default());
        assert!(titles(&list).contains(&t.make_primary));
        // Карточку ещё не опрашивали — действий нет.
        assert!(Display::default().actions(t, EN, &Settings::default()).is_empty());
    }

    #[test]
    fn other_modules_actions_are_not_ours() {
        let dev = r"\\.\TEST-SEARCH-FOREIGN";
        let mut display = card(dev);
        let start = display::current(dev);
        let mut settings = Settings::default();
        assert!(run(&mut display, &mut settings, dev, "quick:wifi").is_empty());
        assert!(run(&mut display, &mut settings, dev, "displayx:mode:1920x1080@144").is_empty());
        assert_eq!(display::current(dev), start);
        assert!(!display.busy());
    }
}
