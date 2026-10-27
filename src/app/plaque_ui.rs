// Живые плашки у края (2.4): очередь и окно у края за кадр, вид плашки и её кнопки. Что такое плашка и как идёт очередь —
// `crate::plaque`; события отдают модули и `crate::watch`. Панель спрятана — событие плашкой, выдвинута — уведомлением набора.
// Окно плашки не забирает фокус (как полоска): щелчок по кнопке не уводит клавиатуру из программы пользователя.

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{Painter, Shape};

use super::*;
use crate::plaque::{self, Do, Glyph, Look, Plaque};

/// Что нажали на плашке за кадр.
enum Hit {
    Button(usize),
    Cross,
    /// Щелчок по телу плашки: открыть панель на полке события.
    Body,
    /// Потянули файл с плашки.
    Drag,
}

/// Можно ли плашке быть на экране: панель спрятана совсем (`panel_shown` — выдвинута, `panel_t` — доля выезда в прошлом кадре, пока
/// она ещё уезжает, окно принадлежит ей), на мониторе нет игры во весь экран (`fullscreen`) и программа не из «Не выезжать» (`excluded`).
pub(super) fn may_show(panel_shown: bool, panel_t: f32, excluded: bool, fullscreen: bool) -> bool {
    !panel_shown && panel_t <= 0.0 && !excluded && !fullscreen
}

impl App {
    /// Событие. Панель спрятана — плашка у края (очередь), выдвинута — уведомление набора, плашки нет.
    pub(super) fn post(&mut self, plaque: Plaque, now: Instant) {
        if self.edge.shown {
            self.toast_of(&plaque);
        } else {
            self.plaques.push(plaque, now);
        }
    }

    /// Слова плашки уведомлением: «название · пояснение», тон — как у плашки.
    fn toast_of(&mut self, plaque: &Plaque) {
        let lang = self.lang();
        let (title, sub) = (texts::plaque_text(lang, &plaque.title), texts::plaque_text(lang, &plaque.sub));
        let text = if sub.is_empty() { title } else { format!("{title} · {sub}") };
        self.toasts.push(text, plaque.tone);
    }

    /// Очередь плашек и окно у края за кадр. Время идёт, пока плашке можно быть на экране (`can_show`) и мышь не над ней
    /// (`hovered`); следующая встаёт, когда окно убрало прежнюю; выезд — `LAYOUT`, уход — `LEAVE` набора. Панель выдвинулась —
    /// обычные плашки уходят уведомлениями (важные ждут и покажутся после).
    pub(super) fn plaque_frame(&mut self, ctx: &egui::Context, now: Instant, hovered: bool, can_show: bool) {
        if self.edge.shown {
            for p in self.plaques.take_ordinary() {
                self.toast_of(&p);
            }
        }
        let idle = self.plaque_drawn.is_none();
        self.plaques.step(now, hovered, can_show, idle);
        let current = self.plaques.current().cloned();
        let want = can_show && current.is_some();
        self.plaque_t = Motion::of(ctx).toggle(ctx, egui::Id::new("plaque-shown"), want, motion::LAYOUT);
        if want {
            self.plaque_drawn = current;
        } else if self.plaque_t <= 0.0 {
            self.plaque_drawn = None;
        }
    }

    /// Открыть панель на полке карточки события (если она стоит на полке): щелчок по плашке. Плашка своё сказала — уходит.
    fn plaque_open(&mut self, ctx: &egui::Context, plaque: &Plaque) {
        self.plaques.dismiss();
        self.edge.reveal(Instant::now());
        self.refreshed = None;
        self.focus(ctx);
        if let Some(shelf) = plaque.shelf.and_then(|card| self.settings.shelf_of(card)) {
            self.set_shelf(shelf);
        }
    }

    /// Кнопка плашки `i`.
    fn plaque_do(&mut self, plaque: &Plaque, i: usize) {
        let Some(button) = plaque.buttons.get(i) else { return };
        match &button.does {
            Do::Done => self.plaques.dismiss(),
            Do::Open(target) => {
                if let Err(e) = sys::launch(target, "") {
                    eprintln!("open from plaque failed: {e}");
                    let failed = Plaque::new(
                        plaque.kind,
                        format!("failed:{}", plaque.key),
                        plaque.glyph,
                        Tone::Warning,
                        plaque::Say::Failed(short_name(target)),
                    );
                    self.plaques.dismiss();
                    self.plaques.push(failed, Instant::now());
                } else {
                    self.plaques.dismiss();
                }
            }
            Do::ToTray(path) => {
                self.drop_files(vec![path.clone()]);
                self.plaques.dismiss();
            }
            // Безопасное извлечение занимает время: плашка остаётся, кнопки не нажимаются, пока её не заменит итог.
            Do::Eject { root, name } => {
                self.watch.usb.eject(root.clone(), name.clone());
                self.plaques.set_busy(true);
            }
        }
    }

    /// Файл потянули с плашки: обычное перетаскивание Windows. Пока оно идёт, окно занято; кнопку отпустили внутри него, а
    /// egui этого не видел — отпускаем за него в следующем кадре (`App::release`). Плашка своё отдала — уходит.
    fn plaque_drag(&mut self, ctx: &egui::Context, plaque: &Plaque) {
        let Some(path) = &plaque.file else { return };
        if let Err(e) = sys::shell::drag_file(path) {
            eprintln!("drag from plaque failed: {e}");
        }
        self.release = Some(ctx.input(|i| i.pointer.latest_pos()).unwrap_or(Pos2::ZERO));
        self.plaques.dismiss();
        ctx.request_repaint();
    }

    /// Плашка у края: значок, название и пояснение, до двух кнопок, крестик у важной, полоска «сколько осталось».
    pub(super) fn plaque_strip(&mut self, ui: &mut Ui) {
        let Some(p) = self.plaque_drawn.clone() else { return };
        let lang = self.lang();
        let t = texts::tr(lang);
        let pal = Palette::of(ui);
        // Нажимается, только пока плашка в очереди «текущая»: уезжающая (и ждущая в игре) — просто картинка.
        let live = self.plaques.current().is_some_and(|c| c.key == p.key);
        let usable = live && !self.plaques.busy();
        let rect = Rect::from_min_size(Pos2::ZERO, PLAQUE);
        let mid = rect.center().y;
        let corner = CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 };
        ui.painter().rect(rect, corner, pal.surface, Stroke::new(1.0, pal.border), StrokeKind::Inside);

        // Всё тело плашки — щелчок и, у файла, перетаскивание; кнопки лежат поверх и берут свои щелчки.
        let sense = if live { Sense::click_and_drag() } else { Sense::hover() };
        let body = ui.interact(rect, ui.id().with("plaque-body"), sense);
        anchor(ui, rect, "plaque-body");
        let mut hit = None;

        // Значок 28: подложка и знак цветом тона.
        let (ink, fill) = match p.tone {
            Tone::Danger => (pal.danger, pal.badge_fill(pal.danger)),
            Tone::Warning => (pal.warning, pal.badge_fill(pal.warning)),
            _ => (pal.accent_text, pal.badge_fill(pal.accent)),
        };
        let mark = Rect::from_center_size(Pos2::new(12.0 + 14.0, mid), Vec2::splat(28.0));
        ui.painter().rect_filled(mark, 7, fill);
        paint_glyph(ui.painter(), mark.shrink(6.0), p.glyph, ink);

        // Справа налево: крестик (у важной плашки без своей кнопки «закрыть»), потом кнопки.
        let mut left = rect.right() - 10.0;
        if p.sticky && !p.has_done() {
            let cross = Rect::from_min_size(Pos2::new(rect.right() - 6.0 - 24.0, mid - 12.0), Vec2::splat(24.0));
            let r = ui.interact(cross, ui.id().with("plaque-cross"), Sense::click());
            let on = r.hovered() && live;
            if on {
                ui.painter().rect_filled(cross, 6, pal.hover);
            }
            let color = if on { pal.text } else { pal.weak };
            anvil_ui::icons::paint(
                ui.painter(),
                Rect::from_center_size(cross.center(), Vec2::splat(14.0)),
                Icon::Close,
                color,
            );
            w::focus_ring(ui, cross, &r, 6);
            anchor(ui, cross, "plaque-close");
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, live, t.plaque_close));
            if r.on_hover_text(t.plaque_close).on_hover_cursor(CursorIcon::PointingHand).clicked() && live {
                hit = Some(Hit::Cross);
            }
            left = cross.left() - 6.0;
        }
        for (i, b) in p.buttons.iter().enumerate().rev() {
            let label = texts::plaque_text(lang, &b.label);
            let width = button_width(ui, &label, b.look, b.external);
            let r = Rect::from_min_size(Pos2::new(left - width, mid - 14.0), Vec2::new(width, 28.0));
            let clicked = plaque_button(ui, ui.id().with(("plaque-button", i)), r, &label, b.look, b.external, usable);
            anchor(ui, r, &format!("plaque-button-{i}"));
            if clicked && usable {
                hit = Some(Hit::Button(i));
            }
            left = r.left() - 10.0;
        }

        // Название полужирным и пояснение под ним; не помещается — многоточие, а целиком — в подсказке.
        let text_left = mark.right() + 10.0;
        let room = (left - text_left).max(20.0);
        let (title, sub) = (texts::plaque_text(lang, &p.title), texts::plaque_text(lang, &p.sub));
        let line = |ui: &Ui, text: &str, font: egui::FontId, color: Color32| {
            let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
            job.wrap = TextWrapping::truncate_at_width(room);
            ui.painter().layout_job(job)
        };
        let full = |ui: &Ui, text: &str, font: egui::FontId| {
            ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER).size().x
        };
        let (title_font, sub_font) = (anvil_ui::semibold(13.5), egui::FontId::proportional(12.0));
        let cut =
            full(ui, &title, title_font.clone()) > room || (!sub.is_empty() && full(ui, &sub, sub_font.clone()) > room);
        let title_g = line(ui, &title, title_font, pal.text);
        let sub_g = (!sub.is_empty()).then(|| line(ui, &sub, sub_font, pal.weak));
        let height = title_g.size().y + sub_g.as_ref().map_or(0.0, |g| g.size().y);
        let top = mid - height / 2.0;
        let below = top + title_g.size().y;
        ui.painter().galley(Pos2::new(text_left, top), title_g, pal.text);
        if let Some(g) = sub_g {
            ui.painter().galley(Pos2::new(text_left, below), g, pal.weak);
        }

        // Полоска 2 внизу: сколько осталось обычной плашке; у важной её нет.
        if live && let Some(left_part) = self.plaques.fraction() {
            paint_bar(ui.painter(), rect, left_part, pal.accent);
        }

        // Тело: для диктора — слова плашки (рисуются без виджетов, сами диктору не видны), подсказка (обрезанное — целиком),
        // курсор; файл тянется, остальное — открывает панель.
        let said = if sub.is_empty() { title.clone() } else { format!("{title}. {sub}") };
        body.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, live, &said));
        let tip = if cut { format!("{title}\n{sub}") } else { t.open_panel.to_string() };
        let cursor = if p.file.is_some() { CursorIcon::Grab } else { CursorIcon::PointingHand };
        let body = body.on_hover_text(tip).on_hover_cursor(cursor);
        if live && p.file.is_some() && body.drag_started() {
            hit = Some(Hit::Drag);
        } else if live && body.clicked() && hit.is_none() {
            hit = Some(Hit::Body);
        }

        let ctx = ui.ctx().clone();
        match hit {
            Some(Hit::Button(i)) => self.plaque_do(&p, i),
            Some(Hit::Cross) => self.plaques.dismiss(),
            Some(Hit::Body) => self.plaque_open(&ctx, &p),
            Some(Hit::Drag) => self.plaque_drag(&ctx, &p),
            None => {}
        }
    }
}

/// Как назвать то, что не открылось, словами: файл — его имя, диск — «E:», адрес — сайт.
fn short_name(target: &str) -> String {
    if let Some(rest) = target.strip_prefix("https://").or_else(|| target.strip_prefix("http://")) {
        return rest.split('/').next().unwrap_or(rest).to_string();
    }
    let name = target.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(target);
    if name.is_empty() { target.to_string() } else { name.to_string() }
}

/// Ширина кнопки плашки: поля 10, текст 12,5 (у главной полужирный) и, у открывающей сайт, значок «наружу» 12 через 5.
fn button_width(ui: &Ui, label: &str, look: Look, external: bool) -> f32 {
    let font = if look == Look::Primary { anvil_ui::semibold(12.5) } else { egui::FontId::proportional(12.5) };
    let text = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER).size().x;
    10.0 + text + if external { 5.0 + 12.0 } else { 0.0 } + 10.0
}

/// Кнопка плашки: 28 высотой, скругление 6, на цветах набора (по макету — ниже и мельче кнопки набора). Ответ — нажали ли.
fn plaque_button(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    label: &str,
    look: Look,
    external: bool,
    enabled: bool,
) -> bool {
    let pal = Palette::of(ui);
    // Недоступная кнопка щелчок не отдаёт, но и не пропускает его на тело плашки (оно открыло бы панель).
    let r = ui.interact(rect, id, Sense::click());
    let hovered = r.hovered() && enabled;
    let pressed = r.is_pointer_button_down_on() && enabled;
    let clear = Color32::TRANSPARENT;
    let (fill, border, ink) = match look {
        Look::Primary => {
            let fill = if pressed {
                pal.accent.gamma_multiply(0.85)
            } else if hovered {
                pal.accent.lerp_to_gamma(if pal.dark { Color32::WHITE } else { Color32::BLACK }, 0.08)
            } else {
                pal.accent
            };
            (fill, clear, pal.on_accent)
        }
        Look::Secondary => (
            if pressed {
                pal.border
            } else if hovered {
                pal.hover
            } else {
                pal.raised
            },
            pal.border_strong,
            pal.text,
        ),
        Look::Ghost => (
            if pressed {
                pal.border
            } else if hovered {
                pal.hover
            } else {
                clear
            },
            clear,
            if hovered { pal.text } else { pal.weak },
        ),
    };
    // Недоступная кнопка любого вида — плоская и тихая.
    let (fill, border, ink) = match (enabled, look) {
        (true, _) => (fill, border, ink),
        (false, Look::Ghost) => (clear, clear, pal.faint),
        (false, _) => (pal.raised, pal.border, pal.faint),
    };
    ui.painter().rect(rect, 6, fill, Stroke::new(1.0, border), StrokeKind::Inside);
    let font = if look == Look::Primary { anvil_ui::semibold(12.5) } else { egui::FontId::proportional(12.5) };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER);
    let content = galley.size().x + if external { 5.0 + 12.0 } else { 0.0 };
    let x = rect.center().x - content / 2.0;
    let text_w = galley.size().x;
    ui.painter().galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, ink);
    if external {
        paint_external(
            ui.painter(),
            Rect::from_center_size(Pos2::new(x + text_w + 5.0 + 6.0, rect.center().y), Vec2::splat(12.0)),
            ink,
        );
    }
    w::focus_ring(ui, rect, &r, 6);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    let r = if enabled { r.on_hover_cursor(CursorIcon::PointingHand) } else { r };
    r.clicked() && enabled
}

/// Полоска внизу плашки: от левого края столько, сколько осталось (`left` — от 0 до 1). Нижние углы окна скруглены 12
/// (внутри рамки — 11): полоска идёт по их закруглению, как на макете (там её обрезает `overflow: hidden`).
fn paint_bar(painter: &Painter, rect: Rect, left: f32, color: Color32) {
    const R: f32 = 11.0;
    let (top, bottom) = (rect.bottom() - 3.0, rect.bottom() - 1.0);
    let right = rect.left() + 1.0 + (rect.width() - 2.0) * left.clamp(0.0, 1.0);
    let (cy, cx_left, cx_right) = (rect.bottom() - 1.0 - R, rect.left() + 1.0 + R, rect.right() - 1.0 - R);
    // На высоте `y` окружность угла отступает от своего центра на столько по горизонтали.
    let span = |y: f32| (R * R - (y - cy).powi(2)).max(0.0).sqrt();
    let ys: Vec<f32> = (0..=4).map(|i| top + (bottom - top) * i as f32 / 4.0).collect();
    let mut points: Vec<Pos2> = ys.iter().map(|&y| Pos2::new((cx_left - span(y)).min(right), y)).collect();
    points.extend(ys.iter().rev().map(|&y| Pos2::new((cx_right + span(y)).min(right), y)));
    if right - points[0].x > 0.5 {
        painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
}

/// Значок плашки в квадрате `rect`: свои знаки (наушники, флешка, батарея) — в той же сетке 24 и той же толщине линии, что у
/// значков набора; остальные — из набора.
fn paint_glyph(painter: &Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let kit = |icon: Icon| anvil_ui::icons::paint(painter, rect, icon, color);
    match glyph {
        Glyph::Device => kit(Icon::Broadcast),
        Glyph::Branch => kit(Icon::Branch),
        Glyph::Download => kit(Icon::Download),
        Glyph::Clock => kit(Icon::Clock),
        Glyph::Headphones | Glyph::Usb | Glyph::Battery => {
            let scale = rect.width().min(rect.height()) / 24.0;
            let origin = rect.center() - Vec2::splat(12.0) * scale;
            let at = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;
            let stroke = Stroke::new((1.7 * scale).max(1.2), color);
            let line = |points: Vec<Pos2>| painter.add(Shape::line(points, stroke));
            let closed = |points: Vec<Pos2>| painter.add(Shape::closed_line(points, stroke));
            let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| -> Vec<Pos2> {
                (0..=8)
                    .map(|i| {
                        let a = from + (to - from) * i as f32 / 8.0;
                        at(cx + r * a.cos(), cy + r * a.sin())
                    })
                    .collect()
            };
            use std::f32::consts::{FRAC_PI_2, PI, TAU};
            match glyph {
                // Дужка и две чашки: контур макета (`M3 14h3a2 2 …`) одной линией.
                Glyph::Headphones => {
                    let mut p = vec![at(3.0, 14.0), at(6.0, 14.0)];
                    p.extend(arc(6.0, 16.0, 2.0, -FRAC_PI_2, 0.0));
                    p.push(at(8.0, 19.0));
                    p.extend(arc(6.0, 19.0, 2.0, 0.0, FRAC_PI_2));
                    p.push(at(5.0, 21.0));
                    p.extend(arc(5.0, 19.0, 2.0, FRAC_PI_2, PI));
                    p.push(at(3.0, 12.0));
                    p.extend(arc(12.0, 12.0, 9.0, PI, TAU));
                    p.push(at(21.0, 19.0));
                    p.extend(arc(19.0, 19.0, 2.0, 0.0, FRAC_PI_2));
                    p.push(at(18.0, 21.0));
                    p.extend(arc(18.0, 19.0, 2.0, FRAC_PI_2, PI));
                    p.push(at(16.0, 16.0));
                    p.extend(arc(18.0, 16.0, 2.0, PI, PI + FRAC_PI_2));
                    p.push(at(21.0, 14.0));
                    line(p);
                }
                // Флешка: разъём, корпус со скруглённым низом и два огонька на разъёме.
                Glyph::Usb => {
                    closed(vec![at(8.0, 2.0), at(16.0, 2.0), at(16.0, 8.0), at(8.0, 8.0)]);
                    let mut p = vec![at(6.0, 8.0), at(18.0, 8.0), at(18.0, 18.0)];
                    p.extend(arc(14.0, 18.0, 4.0, 0.0, FRAC_PI_2));
                    p.push(at(10.0, 22.0));
                    p.extend(arc(10.0, 18.0, 4.0, FRAC_PI_2, PI));
                    closed(p);
                    for x in [10.5, 13.5] {
                        painter.add(Shape::circle_filled(at(x, 4.5), 1.1 * scale, color));
                    }
                }
                // Батарея: контур, «носик» и маленький остаток заряда.
                _ => {
                    painter.add(Shape::rect_stroke(
                        Rect::from_min_max(at(2.5, 7.0), at(19.0, 17.0)),
                        2.5 * scale,
                        stroke,
                        StrokeKind::Middle,
                    ));
                    painter.add(Shape::rect_filled(
                        Rect::from_min_max(at(20.2, 10.2), at(22.0, 13.8)),
                        0.8 * scale,
                        color,
                    ));
                    painter.add(Shape::rect_filled(
                        Rect::from_min_max(at(5.0, 10.0), at(8.2, 14.0)),
                        0.8 * scale,
                        color,
                    ));
                }
            }
        }
    }
}

/// Значок «наружу» (стрелка из квадрата) 12 × 12 — кнопка открывает сайт или другую программу. В наборе его нет.
fn paint_external(painter: &Painter, rect: Rect, color: Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let scale = rect.width() / 24.0;
    let origin = rect.center() - Vec2::splat(12.0) * scale;
    let at = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;
    let stroke = Stroke::new((1.7 * scale * 1.3).max(1.2), color);
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| -> Vec<Pos2> {
        (0..=4)
            .map(|i| {
                let a = from + (to - from) * i as f32 / 4.0;
                at(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    painter.add(Shape::line(vec![at(15.0, 3.0), at(21.0, 3.0), at(21.0, 9.0)], stroke));
    painter.add(Shape::line(vec![at(10.0, 14.0), at(21.0, 3.0)], stroke));
    let mut p = vec![at(18.0, 13.0), at(18.0, 19.0)];
    p.extend(arc(16.0, 19.0, 2.0, 0.0, FRAC_PI_2));
    p.push(at(5.0, 21.0));
    p.extend(arc(5.0, 19.0, 2.0, FRAC_PI_2, PI));
    p.push(at(3.0, 8.0));
    p.extend(arc(5.0, 8.0, 2.0, PI, PI + FRAC_PI_2));
    p.push(at(11.0, 6.0));
    painter.add(Shape::line(p, stroke));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_did_not_open_is_named_by_a_word() {
        assert_eq!(short_name(r"C:\Demo\Downloads\report.pdf"), "report.pdf");
        assert_eq!(short_name(r"E:\"), "E:");
        assert_eq!(short_name("https://github.com/demo/site/actions"), "github.com");
        assert_eq!(short_name("plain"), "plain");
    }
}
