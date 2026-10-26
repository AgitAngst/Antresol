// Буфер обмена: сверху закреплённые тексты (бывшие «Заготовки», `snippet=` в настройках), ниже — последние скопированные:
// текст, ссылки, цвета и картинки; щелчок по строке — положить обратно в буфер, булавка — закрепить текст или открепить.
// Чипы над историей фильтруют по виду; у текста, набранного не в той раскладке, — «Исправить раскладку» (`layout.rs`).
// История только в памяти (на диск не пишется) и собирается, даже пока панель спрятана: номер копирования проверяется
// каждый кадр панели — это ничего не стоит. Приватные копии (менеджеры паролей) не запоминаются; запоминание выключается в
// настройках (закреплённое при этом остаётся, а уже запомненное лежит в памяти до выхода, но не показывается).
//
// Цвет «Пипетки» в историю кладёт сам буфер: «Пипетка» кладёт в него код `#rrggbb`, а история узнаёт в нём цвет.

use std::collections::VecDeque;
use std::time::Instant;

use anvil_ui::widgets::{self as w, Kind};
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{Action, Command, Ctx, Module, anchor, chips, mine, row, stable_key};
use crate::config::{MAX_PINNED, Settings};
use crate::layout;
use crate::sys::{self, clip, ocr};
use crate::texts::{self, Tr};

/// Сколько записей помнить.
pub const KEEP: usize = 20;
/// Из них картинок — не больше: старые уходят первыми.
pub const KEEP_IMAGES: usize = 5;
/// Длиннее не храним: огромные копии (логи, файлы) — не то, что вставляют из истории.
const MAX_LEN: usize = 4000;
/// Место справа в строке под булавку.
const PIN_ROOM: f32 = 30.0;
/// И под «Открыть в браузере» у ссылки: кнопка левее булавки.
const OPEN_ROOM: f32 = 26.0;
/// Миниатюра картинки в строке, точек; у самой миниатюры пикселей вдвое больше — чётко при масштабе 200 %.
const THUMB: Vec2 = Vec2::new(36.0, 24.0);
const THUMB_PX: (usize, usize) = (72, 48);
/// Подписи справа и слева от текста строки не должны съесть его целиком: меньше этого места остаётся — «Исправить раскладку»
/// уступает значку.
const MIN_LABEL: f32 = 70.0;

/// Цвет: красный, зелёный, синий.
pub type Rgb = [u8; 3];

/// Что запомнено. Вид текста — по самому тексту (`Clip::from_text`).
#[derive(Debug, PartialEq)]
enum Clip {
    Text(String),
    Link(String),
    Color(Rgb),
    Image(ClipImage),
}

/// Картинка в истории: байты как пришли, миниатюра (пиксели — до первого показа, потом — текстура).
struct ClipImage {
    image: clip::Image,
    thumb_px: Option<egui::ColorImage>,
    thumb: Option<egui::TextureHandle>,
}

// Картинки равны, если равны байты: миниатюра из них же и получена.
impl PartialEq for ClipImage {
    fn eq(&self, other: &ClipImage) -> bool {
        self.image == other.image
    }
}

// У текстуры отладочной печати нет, а печатать байты картинки незачем.
impl std::fmt::Debug for ClipImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({} × {})", self.image.width, self.image.height)
    }
}

/// Строка истории: что, когда скопировано и что предложено исправить.
struct Entry {
    clip: Clip,
    at: Instant,
    /// «Исправить раскладку» у текста: что получится (считается один раз, при копировании).
    fix: Option<layout::Fix>,
}

/// Вид записей, как их фильтруют чипы.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    All,
    Text,
    Links,
    Images,
    Colors,
}

impl Filter {
    fn shows(self, kind: Filter) -> bool {
        self == Filter::All || self == kind
    }
}

impl Clip {
    /// Вид записи по тексту: цвет, ссылка или просто текст.
    fn from_text(text: String) -> Clip {
        if let Some(rgb) = parse_color(&text) {
            Clip::Color(rgb)
        } else if link_of(&text).is_some() {
            Clip::Link(text)
        } else {
            Clip::Text(text)
        }
    }

    fn image(image: clip::Image) -> Clip {
        let thumb_px = make_thumb(&image.bytes);
        Clip::Image(ClipImage { image, thumb_px, thumb: None })
    }

    /// Как запись лежит в буфере текстом: у цвета — код `#rrggbb`; у картинки текста нет.
    fn text(&self) -> Option<String> {
        match self {
            Clip::Text(s) | Clip::Link(s) => Some(s.clone()),
            Clip::Color(c) => Some(hex(*c)),
            Clip::Image(_) => None,
        }
    }

    fn kind(&self) -> Filter {
        match self {
            Clip::Text(_) => Filter::Text,
            Clip::Link(_) => Filter::Links,
            Clip::Color(_) => Filter::Colors,
            Clip::Image(_) => Filter::Images,
        }
    }

    /// Та же запись (повтор поднимается, а не дублируется).
    fn same(&self, other: &Clip) -> bool {
        self == other
    }
}

/// Код цвета «#4f7a12».
fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Цвет из текста: `#rrggbb`, `#rgb` (если в нём есть буква: «#123» — скорее номер, чем цвет) и `rgb(r, g, b)`.
fn parse_color(text: &str) -> Option<Rgb> {
    let t = text.trim();
    if let Some(digits) = t.strip_prefix('#') {
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |s: &str| u8::from_str_radix(s, 16).ok();
        return match digits.len() {
            6 => Some([byte(&digits[0..2])?, byte(&digits[2..4])?, byte(&digits[4..6])?]),
            3 if digits.chars().any(|c| c.is_ascii_alphabetic()) => {
                let (r, g, b) = (byte(&digits[0..1])?, byte(&digits[1..2])?, byte(&digits[2..3])?);
                Some([r * 17, g * 17, b * 17])
            }
            _ => None,
        };
    }
    let inner = t.get(..4).filter(|p| p.eq_ignore_ascii_case("rgb("))?;
    let inner = t[inner.len()..].strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return None;
    }
    let mut rgb = [0u8; 3];
    for (slot, part) in rgb.iter_mut().zip(parts) {
        if part.is_empty() || part.len() > 3 || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        *slot = part.parse().ok()?;
    }
    Some(rgb)
}

/// Ссылка: одна строка без пробелов, начинается с `http://`, `https://` или `www.`. Ответ — что открыть, сайт и остальное.
fn link_of(text: &str) -> Option<(String, String, String)> {
    let t = text.trim();
    if t.is_empty() || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let (open, body, www) = if lower.starts_with("https://") {
        (t.to_string(), &t[8..], false)
    } else if lower.starts_with("http://") {
        (t.to_string(), &t[7..], false)
    } else if lower.starts_with("www.") {
        (format!("https://{t}"), t, true)
    } else {
        return None;
    };
    let (site, rest) = body.split_at(body.find(['/', '?', '#']).unwrap_or(body.len()));
    // «https://» и «www.» без сайта — не ссылки; у «www.» после него должна быть ещё точка («www.example.com»).
    if site.is_empty() || site.starts_with('.') || (www && !site[4..].contains('.')) {
        return None;
    }
    Some((open, site.to_string(), rest.to_string()))
}

/// Миниатюра для строки: середина картинки по пропорции 3 : 2, усреднением по площади до `THUMB_PX`. Не разобрать
/// (палитра, сжатие) — `None`: строка нарисует значок вместо неё.
fn make_thumb(dib: &[u8]) -> Option<egui::ColorImage> {
    let img = ocr::dib_to_bgra(dib)?;
    let (w, h) = (img.width as usize, img.height as usize);
    let (tw, th) = THUMB_PX;
    // Окно 3 : 2 по центру: у широкой картинки срезаются края слева и справа, у высокой — сверху и снизу.
    let (cw, ch) = if w * th >= h * tw { ((h * tw / th).max(1), h) } else { (w, (w * th / tw).max(1)) };
    let (x0, y0) = ((w - cw) / 2, (h - ch) / 2);
    let mut rgba = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let (sy0, sy1) = (y0 + ty * ch / th, (y0 + (ty + 1) * ch / th).max(y0 + ty * ch / th + 1).min(y0 + ch));
        for tx in 0..tw {
            let (sx0, sx1) = (x0 + tx * cw / tw, (x0 + (tx + 1) * cw / tw).max(x0 + tx * cw / tw + 1).min(x0 + cw));
            let (mut sum, mut n) = ([0u32; 3], 0u32);
            for y in sy0..sy1 {
                for x in sx0..sx1 {
                    let i = (y * w + x) * 4;
                    // BGRA → по каналам RGB.
                    sum[0] += u32::from(img.bgra[i + 2]);
                    sum[1] += u32::from(img.bgra[i + 1]);
                    sum[2] += u32::from(img.bgra[i]);
                    n += 1;
                }
            }
            let n = n.max(1);
            rgba.extend_from_slice(&[(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255]);
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied([tw, th], &rgba))
}

/// Миниатюра демо-картинки: наклонный переход синего в золотой, как на макете. Выдуманная, настоящих снимков экрана нет.
fn demo_thumb() -> egui::ColorImage {
    const STOPS: [(f32, [f32; 3]); 3] =
        [(0.0, [59.0, 111.0, 182.0]), (0.45, [106.0, 160.0, 216.0]), (1.0, [232.0, 180.0, 94.0])];
    let (tw, th) = THUMB_PX;
    let mut rgba = Vec::with_capacity(tw * th * 4);
    for y in 0..th {
        for x in 0..tw {
            let t = (x as f32 / (tw - 1) as f32 + y as f32 / (th - 1) as f32) / 2.0;
            let k = if t < STOPS[1].0 { 0 } else { 1 };
            let ((t0, c0), (t1, c1)) = (STOPS[k], STOPS[k + 1]);
            let f = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
            let px = |i: usize| (c0[i] + (c1[i] - c0[i]) * f).round() as u8;
            rgba.extend_from_slice(&[px(0), px(1), px(2), 255]);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([tw, th], &rgba)
}

/// То, что было в буфере до запуска, тоже возьмём — с первым кадром. В демо-режиме история выдуманная (`demo_rows`), своя
/// не собирается.
#[derive(Default)]
pub struct Clipboard {
    history: VecDeque<Entry>,
    seen: u32,
    filter: Filter,
    /// Текстура выдуманной картинки демо-режима: одна на все кадры.
    demo_image: Option<egui::TextureHandle>,
}

/// Положить запись наверх истории: повтор поднимается, а не дублируется; картинок не больше `KEEP_IMAGES`.
fn remember(history: &mut VecDeque<Entry>, clip: Clip) {
    remember_at(history, clip, Instant::now());
}

fn remember_at(history: &mut VecDeque<Entry>, clip: Clip, at: Instant) {
    if let Clip::Text(t) | Clip::Link(t) = &clip
        && (t.trim().is_empty() || t.chars().count() > MAX_LEN)
    {
        return;
    }
    history.retain(|e| !e.clip.same(&clip));
    let fix = if let Clip::Text(t) = &clip { layout::suggest(t) } else { None };
    history.push_front(Entry { clip, at, fix });
    // Сверху новые: оставляем первые пять картинок, остальные — старые — уходят.
    let mut images = 0;
    history.retain(|e| {
        let image = matches!(e.clip, Clip::Image(_));
        images += usize::from(image);
        !image || images <= KEEP_IMAGES
    });
    history.truncate(KEEP);
}

/// Запись `i` — снова наверх: её только что положили в буфер.
fn raise(history: &mut VecDeque<Entry>, i: usize) {
    if let Some(mut e) = history.remove(i) {
        e.at = Instant::now();
        history.push_front(e);
    }
}

/// Одна строка для списка: переводы строк и табуляции — пробелами.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Закреплённый текст в строке списка: первая непустая строка.
pub fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
}

/// Первая строка закреплённого текста для названия действия: не длиннее `n` букв, дальше многоточие.
fn short(text: &str, n: usize) -> String {
    let line = first_line(text);
    if line.chars().count() <= n {
        return line.to_string();
    }
    let cut: String = line.chars().take(n.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// Закреплённое в демо-режиме — выдуманные строки по языку сцены (настройки пользователя не трогаются).
fn demo_pinned(lang: anvil_ui::Lang) -> Vec<String> {
    texts::demo_clip_pinned(lang).map(String::from).to_vec()
}

/// Что нарисовать в строке недавнего.
#[derive(Clone)]
enum Look {
    Text(String),
    /// Сайт полужирным, остальное — тише.
    Link {
        site: String,
        rest: String,
    },
    Color {
        rgb: Rgb,
        code: String,
    },
    Image {
        width: u32,
        height: u32,
        thumb: Option<egui::TextureHandle>,
    },
}

/// Строка недавнего к показу: копия того, что нужно для кадра (байты картинок в ней не лежат).
#[derive(Clone)]
struct Row {
    /// Место в истории; у выдуманной строки демо-режима — нет.
    index: Option<usize>,
    look: Look,
    /// Сколько секунд назад скопировано.
    age: u64,
    /// Последнее скопированное: подложка и полоска акцента.
    current: bool,
    /// «Исправить раскладку»: что получится из текста.
    fix: Option<layout::Fix>,
}

impl Row {
    fn kind(&self) -> Filter {
        match self.look {
            Look::Text(_) => Filter::Text,
            Look::Link { .. } => Filter::Links,
            Look::Color { .. } => Filter::Colors,
            Look::Image { .. } => Filter::Images,
        }
    }
}

/// Булавка (и другие значки в правом конце) в строке `row`: центр значка на `from_right` левее правого края строки. Видна
/// пока `show`, с клавиатурным фокусом и когда `active` (закреплённая — цветом акцента); щелчок по ней — ответ.
/// `salt` — для `Id`.
fn row_button(
    ui: &mut Ui,
    row: Rect,
    from_right: f32,
    salt: impl std::hash::Hash + std::fmt::Debug,
    icon: Icon,
    (active, show): (bool, bool),
    hint: &str,
) -> egui::Response {
    let p = Palette::of(ui);
    let rect = Rect::from_center_size(Pos2::new(row.right() - from_right, row.center().y), Vec2::splat(24.0));
    let r = ui.interact(rect, ui.id().with(("row-button", salt)), Sense::click());
    if active || show || r.has_focus() {
        if r.hovered() {
            ui.painter().rect_filled(rect, 6, p.hover);
        }
        let color = if active {
            p.accent_text
        } else if r.hovered() {
            p.text
        } else {
            p.weak
        };
        anvil_ui::icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(15.0)), icon, color);
    }
    w::focus_ring(ui, rect, &r, 6);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, hint));
    r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint)
}

/// Булавка в правом конце строки `row`: закреплённая — всегда, цветом акцента (щелчок открепляет); в истории — только пока
/// мышь над строкой (щелчок закрепляет).
fn pin_button(
    ui: &mut Ui,
    row: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    pinned: bool,
    hint: &str,
) -> egui::Response {
    let over = ui.rect_contains_pointer(row);
    row_button(ui, row, 16.0, ("pin", salt), Icon::Pin, (pinned, over), hint)
}

/// Ширина текста одной строкой.
fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER).size().x
}

/// Подпись строки одной строкой, обрезанная многоточием до `room`, с началом в `at` по середине высоты.
fn paint_label(ui: &Ui, mut job: egui::text::LayoutJob, at: Pos2, room: f32, color: Color32) {
    job.wrap = egui::text::TextWrapping::truncate_at_width(room.max(20.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(at - Vec2::new(0.0, galley.size().y / 2.0), galley, color);
}

/// Что сделал пользователь с одной строкой недавнего.
#[derive(Default)]
struct RowOut {
    copy: bool,
    pin: bool,
    open: bool,
    fix: bool,
}

/// Строка недавнего: слева кружок цвета или миниатюра, подпись по виду записи, справа — сколько назад скопировано; под
/// мышью вместо времени — «Открыть в браузере» (у ссылки) и булавка (у всего, кроме картинки); «Исправить раскладку» —
/// у последнего скопированного всегда, у остальных текстов под мышью. Что где стоит, считается от самой строки, поэтому
/// подпись не прыгает, когда кнопки появляются.
fn history_row(ui: &mut Ui, n: usize, row: &Row, t: &Tr, lang: anvil_ui::Lang) -> RowOut {
    let p = Palette::of(ui);
    let image = matches!(row.look, Look::Image { .. });
    let height = if image { 34.0 } else { 28.0 };
    let (name, tip) = match &row.look {
        Look::Text(s) => (s.clone(), t.clipboard_put.to_string()),
        Look::Link { site, rest } => (format!("{site}{rest}"), format!("{}\n{site}{rest}", t.clipboard_put)),
        Look::Color { code, .. } => (code.clone(), t.clipboard_put.to_string()),
        Look::Image { width, height, .. } => {
            let label = texts::clip_image(lang, *width, *height);
            (label.clone(), format!("{}\n{label}", t.clipboard_put))
        }
    };
    let id = ui.id().with(("clip", n));
    let r = w::list_row(ui, id, row.current, height, &name);
    let rect = r.rect;
    anchor(ui, rect, &format!("clip-row-{n}"));
    let mut out = RowOut::default();
    let over = ui.rect_contains_pointer(rect);
    let (pinnable, openable) = (!image, matches!(row.look, Look::Link { .. }));
    let age = texts::clip_age(lang, row.age);
    let cy = rect.center().y;

    // Правый край: слева направо — подпись, «Исправить раскладку», «Открыть», булавка; время — на месте кнопок.
    let tools = if pinnable { PIN_ROOM } else { 0.0 } + if openable { OPEN_ROOM } else { 0.0 };
    let age_room = text_width(ui, &age, FontId::proportional(12.0)) + 10.0;
    let left = if image {
        8.0 + THUMB.x + 9.0
    } else if matches!(row.look, Look::Color { .. }) {
        12.0 + 14.0 + 8.0
    } else {
        12.0
    };
    let chip_label = t.clip_fix;
    let chip_full = text_width(ui, chip_label, anvil_ui::semibold(12.0)) + 16.0;
    // Полная «Исправить раскладку» — пока подписи остаётся место; иначе — значок на той же кнопке.
    let right_of_label = |chip: f32| tools.max(age_room) + if chip > 0.0 { chip + 6.0 } else { 0.0 } + 8.0;
    let compact = row.fix.is_some() && rect.width() - left - right_of_label(chip_full) < MIN_LABEL;
    let chip_w = match (&row.fix, compact) {
        (None, _) => 0.0,
        (Some(_), true) => 24.0,
        (Some(_), false) => chip_full,
    };
    let room = rect.width() - left - right_of_label(chip_w);

    // Слева: миниатюра или кружок.
    match &row.look {
        Look::Color { rgb, .. } => {
            let center = Pos2::new(rect.left() + 12.0 + 7.0, cy);
            ui.painter().circle(
                center,
                7.0,
                Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                Stroke::new(1.0, p.border_strong),
            );
        }
        Look::Image { thumb, .. } => {
            let box_ = Rect::from_min_size(Pos2::new(rect.left() + 8.0, cy - THUMB.y / 2.0), THUMB);
            match thumb {
                Some(tex) => {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    ui.painter()
                        .add(egui::epaint::RectShape::filled(box_, 4, Color32::WHITE).with_texture(tex.id(), uv));
                }
                None => {
                    ui.painter().rect_filled(box_, 4, p.raised);
                    let icon = Rect::from_center_size(box_.center(), Vec2::splat(14.0));
                    anvil_ui::icons::paint(ui.painter(), icon, Icon::Image, p.weak);
                }
            }
            ui.painter().rect_stroke(box_, 4, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
        }
        _ => {}
    }

    // Подпись.
    let at = Pos2::new(rect.left() + left, cy);
    let ink = if row.current { p.text } else { p.weak };
    let plain =
        |text: String, font: FontId, color: Color32| egui::text::LayoutJob::simple_singleline(text, font, color);
    match &row.look {
        Look::Text(s) => paint_label(ui, plain(s.clone(), FontId::proportional(13.5), ink), at, room, ink),
        Look::Image { width, height, .. } => {
            let label = texts::clip_image(lang, *width, *height);
            paint_label(ui, plain(label, FontId::proportional(13.5), ink), at, room, ink)
        }
        Look::Color { code, .. } => {
            paint_label(ui, plain(code.clone(), FontId::monospace(13.0), p.text), at, room, p.text)
        }
        Look::Link { site, rest } => {
            let mut job = egui::text::LayoutJob::default();
            job.append(site, 0.0, egui::text::TextFormat::simple(anvil_ui::semibold(13.5), p.text));
            job.append(rest, 0.0, egui::text::TextFormat::simple(FontId::proportional(13.5), p.weak));
            paint_label(ui, job, at, room, p.weak)
        }
    }

    // Ответ на щелчок по самой строке — до кнопок поверх неё.
    out.copy = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked();

    // Кнопки: «Открыть в браузере» левее булавки, «Исправить раскладку» левее них — слева от времени их нет, на их месте оно.
    let mut x_from_right = 16.0;
    let mut focused = false;
    if pinnable {
        let pin = pin_button(ui, rect, ("clip", n), false, t.clip_pin);
        (out.pin, focused) = (pin.clicked(), focused || pin.has_focus());
        anchor(
            ui,
            Rect::from_center_size(Pos2::new(rect.right() - x_from_right, cy), Vec2::splat(24.0)),
            &format!("clip-pin-{n}"),
        );
        x_from_right += OPEN_ROOM;
    }
    if openable {
        let b = row_button(ui, rect, x_from_right, ("open", n), Icon::Expand, (false, over), t.clip_open);
        anchor(ui, b.rect, &format!("clip-open-{n}"));
        (out.open, focused) = (b.clicked(), focused || b.has_focus());
    }
    let mut chip_visible = false;
    if let Some(fix) = &row.fix {
        let right = rect.right() - tools - 2.0;
        let chip = Rect::from_min_max(Pos2::new(right - chip_w, cy - 12.0), Pos2::new(right, cy + 12.0));
        let r = ui.interact(chip, ui.id().with(("fix", n)), Sense::click());
        chip_visible = row.current || over || r.has_focus();
        if chip_visible {
            let fill = if r.hovered() {
                anvil_ui::theme::over(p.accent.gamma_multiply(if p.dark { 0.30 } else { 0.22 }), p.card)
            } else {
                p.badge_fill(p.accent)
            };
            ui.painter().rect(chip, 6, fill, Stroke::new(1.0, p.accent), StrokeKind::Inside);
            if compact {
                anvil_ui::icons::paint(
                    ui.painter(),
                    Rect::from_center_size(chip.center(), Vec2::splat(14.0)),
                    Icon::Refresh,
                    p.accent_text,
                );
            } else {
                ui.painter().text(
                    chip.center(),
                    egui::Align2::CENTER_CENTER,
                    chip_label,
                    anvil_ui::semibold(12.0),
                    p.accent_text,
                );
            }
        }
        w::focus_ring(ui, chip, &r, 6);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, chip_label));
        anchor(ui, chip, &format!("clip-fix-{n}"));
        let tip = texts::clip_fix_tip(lang, fix.to == layout::To::Russian, &short(&fix.text, 60));
        focused |= r.has_focus();
        out.fix = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked();
    }

    // Время — на месте кнопок, пока их нет (ни под мышью, ни с клавиатуры); у строки с «Исправить раскладку» на виду его нет
    // (место занято).
    if !over && !focused && !chip_visible {
        ui.painter().text(
            Pos2::new(rect.right() - 10.0, cy),
            egui::Align2::RIGHT_CENTER,
            age,
            FontId::proportional(12.0),
            p.weak,
        );
    }
    out
}

/// Что сделал пользователь в списке за кадр.
#[derive(Default)]
struct Picked {
    /// Положить в буфер: (закреплённое?, номер в своём списке).
    copy: Option<(bool, usize)>,
    /// Закрепить запись истории.
    pin: Option<usize>,
    /// Открепить закреплённую.
    unpin: Option<usize>,
    /// Открыть ссылку из истории.
    open: Option<usize>,
    /// Исправить раскладку у текста из истории.
    fix: Option<usize>,
}

impl Clipboard {
    /// Положить текст в буфер обмена. Удалось — запоминаем номер копирования (своя запись — не новое копирование) и
    /// говорим «В буфере — вставляйте».
    fn put_text(&mut self, text: &str, cx: &mut Ctx) -> bool {
        if sys::demo() {
            return false;
        }
        let result = clip::write_text(text);
        self.written(result, cx)
    }

    /// Запись истории `i` — снова в буфер, и она поднимается наверх: текст, ссылка и цвет — текстом, картинка — как пришла.
    fn put_entry(&mut self, i: usize, cx: &mut Ctx) {
        if sys::demo() {
            return;
        }
        let result = match self.history.get(i).map(|e| &e.clip) {
            Some(Clip::Image(img)) => clip::write_image(&img.image),
            Some(other) => clip::write_text(&other.text().unwrap_or_default()),
            None => return,
        };
        if self.written(result, cx) {
            raise(&mut self.history, i);
        }
    }

    fn written(&mut self, result: Result<(), String>, cx: &mut Ctx) -> bool {
        let t = cx.t;
        match result {
            Ok(()) => {
                self.seen = clip::sequence();
                cx.toasts.push((t.clipboard_done.to_string(), Tone::Success));
                true
            }
            Err(e) => {
                eprintln!("clipboard write failed: {e}");
                cx.toasts.push((format!("{}: {e}", t.failed), Tone::Danger));
                false
            }
        }
    }

    /// «Исправить раскладку»: исправленный текст — в буфер и наверх истории; исходный остаётся ниже.
    fn apply_fix(&mut self, i: usize, cx: &mut Ctx) {
        let Some(fix) = self.history.get(i).and_then(|e| e.fix.clone()) else { return };
        if self.put_text(&fix.text, cx) {
            remember(&mut self.history, Clip::from_text(fix.text));
        }
    }

    /// Открыть ссылку из истории в браузере по умолчанию.
    fn open_link(&mut self, i: usize, cx: &mut Ctx) {
        if sys::demo() {
            return;
        }
        let Some(Clip::Link(text)) = self.history.get(i).map(|e| &e.clip) else { return };
        let Some((target, _, _)) = link_of(text) else { return };
        if let Err(e) = sys::launch(&target, "") {
            eprintln!("open link failed: {e}");
            cx.toasts.push((cx.t.failed.to_string(), Tone::Danger));
        }
    }

    /// Закрепить текст: наверх закреплённого. Полно — уведомление, ничего не меняется.
    fn pin(&mut self, text: String, cx: &mut Ctx) {
        if sys::demo() {
            return;
        }
        if cx.settings.pinned.len() >= MAX_PINNED {
            cx.toasts.push((cx.t.clip_pinned_full.to_string(), Tone::Warning));
            return;
        }
        cx.settings.pinned.retain(|p| *p != text);
        self.history.retain(|e| e.clip.text().as_deref() != Some(text.as_str()));
        cx.settings.pinned.insert(0, text);
    }

    /// Открепить: текст возвращается наверх недавнего (если история включена). Выключена — текст пропадает насовсем, и об
    /// этом говорит уведомление «Откреплено».
    fn unpin(&mut self, i: usize, cx: &mut Ctx) {
        if sys::demo() || i >= cx.settings.pinned.len() {
            return;
        }
        let text = cx.settings.pinned.remove(i);
        if cx.settings.clipboard {
            remember(&mut self.history, Clip::from_text(text));
        } else {
            cx.toasts.push((cx.t.clip_unpinned.to_string(), Tone::Neutral));
        }
    }

    /// «Закрепить из буфера» — когда история выключена и брать закрепляемое больше неоткуда.
    fn pin_current(&mut self, cx: &mut Ctx) {
        match clip::read_text() {
            Ok(Some(text)) if !text.trim().is_empty() => self.pin(text, cx),
            _ => cx.toasts.push((cx.t.clip_no_text.to_string(), Tone::Warning)),
        }
    }

    /// Новое из буфера — в историю; закреплённый текст не идёт: он и так наверху карточки.
    fn take(&mut self, copied: Option<clip::Copied>, settings: &Settings) {
        match copied {
            Some(clip::Copied::Text(text)) if !settings.pinned.contains(&text) => {
                remember(&mut self.history, Clip::from_text(text));
            }
            Some(clip::Copied::Image(image)) => remember(&mut self.history, Clip::image(image)),
            _ => {}
        }
    }

    /// Строки недавнего к показу: подходящие под фильтр, у картинок — с текстурой миниатюры (делается при первом показе).
    fn rows_of(&mut self, ctx: &egui::Context, filter: Filter) -> Vec<Row> {
        let mut rows = Vec::new();
        for (index, entry) in self.history.iter_mut().enumerate() {
            if !filter.shows(entry.clip.kind()) {
                continue;
            }
            let look = match &mut entry.clip {
                Clip::Text(s) => Look::Text(one_line(s)),
                Clip::Link(s) => match link_of(s) {
                    Some((_, site, rest)) => Look::Link { site, rest },
                    None => Look::Text(one_line(s)),
                },
                Clip::Color(c) => Look::Color { rgb: *c, code: hex(*c) },
                Clip::Image(img) => {
                    if img.thumb.is_none()
                        && let Some(px) = img.thumb_px.take()
                    {
                        img.thumb = Some(ctx.load_texture("clip-thumb", px, egui::TextureOptions::LINEAR));
                    }
                    Look::Image { width: img.image.width, height: img.image.height, thumb: img.thumb.clone() }
                }
            };
            rows.push(Row {
                index: Some(index),
                look,
                age: entry.at.elapsed().as_secs(),
                current: index == 0,
                fix: entry.fix.clone(),
            });
        }
        rows
    }

    /// Недавнее в демо-режиме: выдуманные текст не в той раскладке, картинка, ссылка и цвет, с возрастом как на макете.
    fn demo_rows(&mut self, ctx: &egui::Context, filter: Filter) -> Vec<Row> {
        let [wrong, link, color] = texts::demo_clip_recent();
        let tex = self
            .demo_image
            .get_or_insert_with(|| ctx.load_texture("clip-demo-thumb", demo_thumb(), egui::TextureOptions::LINEAR))
            .clone();
        let looks = [
            (Look::Text(wrong.to_string()), 0, layout::suggest(wrong)),
            (Look::Image { width: 1280, height: 720, thumb: Some(tex) }, 120, None),
            (
                match link_of(link) {
                    Some((_, site, rest)) => Look::Link { site, rest },
                    None => Look::Text(link.to_string()),
                },
                300,
                None,
            ),
            (
                parse_color(color)
                    .map_or_else(|| Look::Text(color.to_string()), |rgb| Look::Color { rgb, code: hex(rgb) }),
                480,
                None,
            ),
        ];
        looks
            .into_iter()
            .enumerate()
            .map(|(i, (look, age, fix))| Row { index: None, look, age, current: i == 0, fix })
            .filter(|r| filter.shows(r.kind()))
            .collect()
    }

    /// Строки: закреплённые, черта, недавние. Ответ — что выбрали.
    fn list(ui: &mut Ui, t: &Tr, lang: anvil_ui::Lang, pinned: &[String], rows: &[Row]) -> Picked {
        let mut picked = Picked::default();
        for (i, text) in pinned.iter().enumerate() {
            let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
            let more = if lines > 1 { format!("+{}", lines - 1) } else { String::new() };
            let id = ui.id().with(("pinned", i));
            let r = row(ui, id, false, first_line(text), &more, PIN_ROOM);
            let rect = r.rect;
            if r.on_hover_text(text.as_str()).clicked() {
                picked.copy = Some((true, i));
            }
            if pin_button(ui, rect, ("pinned", i), true, t.unpin).clicked() {
                picked.unpin = Some(i);
            }
        }
        if !pinned.is_empty() && !rows.is_empty() {
            ui.add_space(3.0);
            w::divider(ui);
            ui.add_space(3.0);
        }
        for (n, row) in rows.iter().enumerate() {
            let out = history_row(ui, n, row, t, lang);
            let Some(index) = row.index else { continue };
            if out.copy {
                picked.copy = Some((false, index));
            }
            if out.pin {
                picked.pin = Some(index);
            }
            if out.open {
                picked.open = Some(index);
            }
            if out.fix {
                picked.fix = Some(index);
            }
        }
        picked
    }
}

impl Module for Clipboard {
    fn key(&self) -> &'static str {
        "clipboard"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.clipboard
    }

    fn refresh(&mut self, _monitor: &str) {}

    /// «Скопировать «…»» у каждого закреплённого текста: щелчок по его строке. Найти можно по любому слову текста. Номер
    /// действия — отпечаток текста, а не место в списке: открепили другой текст — действие не сползает на чужой.
    fn actions(&self, t: &Tr, lang: anvil_ui::Lang, settings: &Settings) -> Vec<Action> {
        let pinned: Vec<String> = if sys::demo() { demo_pinned(lang) } else { settings.pinned.clone() };
        pinned
            .iter()
            .map(|text| {
                let title = texts::search_copy(lang, &short(text, 40));
                Action::new(format!("clipboard:copy:{}", stable_key(&[text])), title, t.clipboard, Icon::Copy).words(
                    &format!(
                        "закреплённое заготовка pinned snippet {}",
                        one_line(text).chars().take(300).collect::<String>()
                    ),
                )
            })
            .collect()
    }

    fn command(&mut self, cmd: &Command, cx: &mut Ctx) {
        let Command::RunAction(id) = cmd else { return };
        if let Some(rest) = mine(self.key(), id)
            && let Some(key) = rest.strip_prefix("copy:")
        {
            let pinned: Vec<String> = if sys::demo() { demo_pinned(cx.lang) } else { cx.settings.pinned.clone() };
            if let Some(text) = pinned.into_iter().find(|text| stable_key(&[text]) == key) {
                self.put_text(&text, cx);
            }
        }
    }

    fn background(&mut self, settings: &Settings) {
        if sys::demo() {
            return;
        }
        let seq = clip::sequence();
        if seq == self.seen {
            return;
        }
        if !settings.clipboard {
            self.seen = seq;
            return;
        }
        // Буфер занят (`Err`) — номер не запоминаем: прочитаем в следующем кадре.
        if let Ok(copied) = clip::read() {
            self.seen = seq;
            self.take(copied, settings);
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let demo = sys::demo();
        let pinned: Vec<String> = if demo { demo_pinned(cx.lang) } else { cx.settings.pinned.clone() };
        let history_on = cx.settings.clipboard;
        // Фильтр — про историю: выключена — закреплённое показывается целиком.
        let filter = if history_on { self.filter } else { Filter::All };
        let rows = match (history_on, demo) {
            (false, _) => Vec::new(),
            (true, true) => self.demo_rows(ui.ctx(), filter),
            (true, false) => self.rows_of(ui.ctx(), filter),
        };
        let remembered = if demo { 4 } else { self.history.len() };
        // Закреплённые — тексты: при «Все» и «Текст».
        let shown_pinned: &[String] = if matches!(filter, Filter::All | Filter::Text) { &pinned } else { &[] };

        // История выключена: закреплённое работает, вместо недавнего — пояснение и «Закрепить из буфера» внизу карточки.
        // Пусто совсем — подсказка, что делать. Подсказка и «Очистить» (чистит всё недавнее, а не только показанное) — в конце
        // прокручиваемого списка: на карточке 340 высотой каждая точка нужна строкам.
        let mut picked = Picked::default();
        let mut pin_now = false;
        let mut clear = false;
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            if !history_on {
                let full = cx.settings.pinned.len() >= MAX_PINNED && !demo;
                ui.add_enabled_ui(!full, |ui| {
                    let r = w::button(ui, Kind::Ghost, Some(Icon::Plus), t.clip_pin_from);
                    if r.clicked() {
                        pin_now = true;
                    }
                    if full {
                        r.on_disabled_hover_text(t.clip_pinned_full);
                    }
                });
                w::note(ui, t.clipboard_off);
            }
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                // Чипы — пока есть что фильтровать (и пока выбран не «Все»: иначе вернуться было бы нечем).
                if history_on && (remembered > 0 || self.filter != Filter::All) {
                    let options = [
                        (Filter::All, t.clip_all),
                        (Filter::Text, t.clip_text),
                        (Filter::Links, t.clip_links),
                        (Filter::Images, t.clip_images),
                        (Filter::Colors, t.clip_colors),
                    ];
                    chips(ui, &mut self.filter, &options);
                    ui.add_space(2.0);
                }
                if pinned.is_empty() && remembered == 0 {
                    if history_on {
                        w::note(ui, t.clipboard_empty);
                    }
                    return;
                }
                egui::ScrollArea::vertical().id_salt("clip").auto_shrink([false, false]).show(ui, |ui| {
                    if shown_pinned.is_empty() && rows.is_empty() {
                        w::note(ui, t.clip_nothing);
                    } else {
                        picked = Self::list(ui, t, cx.lang, shown_pinned, &rows);
                    }
                    if history_on && remembered > 0 {
                        ui.add_space(4.0);
                        w::note(ui, t.clip_hint);
                        if w::button(ui, Kind::Ghost, Some(Icon::Trash), t.clipboard_clear).clicked() {
                            clear = true;
                        }
                    }
                });
            });
        });

        if let Some((from_pinned, i)) = picked.copy {
            if from_pinned {
                if let Some(text) = pinned.get(i).cloned() {
                    self.put_text(&text, cx);
                }
            } else {
                self.put_entry(i, cx);
            }
        }
        if let Some(i) = picked.pin
            && let Some(text) = self.history.get(i).and_then(|e| e.clip.text())
        {
            self.pin(text, cx);
        }
        if let Some(i) = picked.unpin {
            self.unpin(i, cx);
        }
        if let Some(i) = picked.open {
            self.open_link(i, cx);
        }
        if let Some(i) = picked.fix {
            self.apply_fix(i, cx);
        }
        if pin_now {
            self.pin_current(cx);
        }
        if clear {
            self.history.clear();
            self.filter = Filter::All;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use anvil_ui::Lang;
    use eframe::egui::{Event, Modifiers, PointerButton};

    use super::*;

    const EN: Lang = Lang::En;
    const RU: Lang = Lang::Ru;

    fn ago(secs: u64) -> Instant {
        Instant::now().checked_sub(Duration::from_secs(secs)).unwrap_or_else(Instant::now)
    }

    /// Текст записей сверху вниз; у картинки — пометка с размером.
    fn shown(history: &VecDeque<Entry>) -> Vec<String> {
        history.iter().map(|e| e.clip.text().unwrap_or_else(|| format!("{:?}", e.clip))).collect()
    }

    fn kinds(history: &VecDeque<Entry>) -> Vec<Filter> {
        history.iter().map(|e| e.clip.kind()).collect()
    }

    fn kind(text: &str) -> Filter {
        Clip::from_text(text.to_string()).kind()
    }

    /// DIB в 24 бита `w` × `h`: строки снизу вверх, цвет пикселя (x, y сверху) — `color`.
    fn dib24(w: usize, h: usize, color: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let stride = (w * 3).div_ceil(4) * 4;
        let mut b = vec![0u8; 40];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[4..8].copy_from_slice(&(w as i32).to_le_bytes());
        b[8..12].copy_from_slice(&(h as i32).to_le_bytes());
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        b[14..16].copy_from_slice(&24u16.to_le_bytes());
        for y in (0..h).rev() {
            let mut line = Vec::with_capacity(stride);
            for x in 0..w {
                let [r, g, bl] = color(x, y);
                line.extend_from_slice(&[bl, g, r]);
            }
            line.resize(stride, 0);
            b.extend_from_slice(&line);
        }
        b
    }

    /// Картинка из выдуманных пикселей; `seed` делает одну от другой.
    fn picture(w: usize, h: usize, seed: u8) -> clip::Image {
        clip::image_of(dib24(w, h, |x, y| [seed, x as u8, y as u8]), false).expect("DIB в пределах")
    }

    fn seed_of(e: &Entry) -> u8 {
        match &e.clip {
            // Красный канал первого пикселя нижней строки: после заголовка в 40 байт идут B, G, R.
            Clip::Image(img) => img.image.bytes[42],
            other => panic!("не картинка: {other:?}"),
        }
    }

    #[test]
    fn the_kind_of_a_record_follows_its_text() {
        for link in [
            "https://example.com/report",
            "http://example.com",
            "HTTPS://Example.com/x",
            "www.example.com/a?b=1#c",
            "  https://example.com/trim \n",
            "https://localhost:3000/x",
        ] {
            assert_eq!(kind(link), Filter::Links, "{link:?}");
        }
        for text in [
            "https://",
            "http://",
            "www.",
            "www.localhost",
            "https://example.com/a b",
            "https://example.com\nhttps://example.org",
            "ftp://example.com/file",
            "example.com/report",
            "see https://example.com",
            "mailto:me@example.com",
            "https:///x",
            "Спасибо, получил!",
        ] {
            assert_eq!(kind(text), Filter::Text, "{text:?}");
        }
        for color in [
            "#4f7a12",
            "#4F7A12",
            "#fff",
            "#abc",
            "  #4f7a12\n",
            "rgb(79, 122, 18)",
            "RGB(0,0,0)",
            "rgb( 255 , 255 , 255 )",
        ] {
            assert_eq!(kind(color), Filter::Colors, "{color:?}");
        }
        for text in [
            "#123",
            "#333",
            "#12345",
            "#1234567",
            "#gggggg",
            "rgb(256, 0, 0)",
            "rgb(1, 2)",
            "rgb(1, 2, 3, 4)",
            "rgb(-1, 0, 0)",
            "rgba(1, 2, 3, 1)",
            "color #4f7a12",
            "#4f7a12 and more",
            "4f7a12",
            "",
        ] {
            assert_ne!(kind(text), Filter::Colors, "{text:?}");
        }
    }

    #[test]
    fn a_link_is_split_into_the_site_and_the_rest() {
        let parts = |t: &str| link_of(t).expect(t);
        assert_eq!(
            parts("https://example.com/report"),
            ("https://example.com/report".into(), "example.com".into(), "/report".into())
        );
        assert_eq!(
            parts("http://localhost:3000?x=1"),
            ("http://localhost:3000?x=1".into(), "localhost:3000".into(), "?x=1".into())
        );
        // «www.» открывается по https.
        assert_eq!(
            parts("www.example.com"),
            ("https://www.example.com".into(), "www.example.com".into(), String::new())
        );
        assert_eq!(parts(" https://example.org/a#b \n").1, "example.org", "по краям пробелы не в счёт");
    }

    #[test]
    fn a_color_goes_back_as_rrggbb() {
        let text = |t: &str| Clip::from_text(t.to_string()).text();
        assert_eq!(text("#4F7A12").as_deref(), Some("#4f7a12"));
        assert_eq!(text("#fff").as_deref(), Some("#ffffff"));
        assert_eq!(text("rgb(79, 122, 18)").as_deref(), Some("#4f7a12"));
        assert_eq!(text("https://example.com").as_deref(), Some("https://example.com"), "ссылка — как была");
        assert_eq!(Clip::image(picture(3, 2, 1)).text(), None, "у картинки текста нет");
        // Один и тот же цвет — одна запись, как бы его ни записали.
        let mut h = VecDeque::new();
        for t in ["#FFF", "#ffffff", "rgb(255,255,255)"] {
            remember(&mut h, Clip::from_text(t.into()));
        }
        assert_eq!(shown(&h), ["#ffffff"]);
    }

    /// Цвет «Пипетки» попадает в историю через буфер: она кладёт в него код (`tools::hex`), а история узнаёт в нём цвет.
    #[test]
    fn a_color_taken_with_the_picker_lands_in_the_history_as_a_color() {
        let picked = Color32::from_rgb(0x2a, 0x5f, 0xd0);
        let code = super::super::tools::hex(picked);
        assert_eq!(Clip::from_text(code.clone()), Clip::Color([0x2a, 0x5f, 0xd0]));
        let mut clip = Clipboard::default();
        clip.take(Some(clip::Copied::Text(code)), &Settings::default());
        assert_eq!(kinds(&clip.history), [Filter::Colors]);
        assert_eq!(shown(&clip.history), ["#2a5fd0"]);
    }

    #[test]
    fn history_rules() {
        let mut h = VecDeque::new();
        remember(&mut h, Clip::from_text("a".into()));
        remember(&mut h, Clip::from_text("b".into()));
        remember(&mut h, Clip::from_text("a".into()));
        assert_eq!(shown(&h), ["a", "b"], "повтор поднимается наверх, не дублируется");
        remember(&mut h, Clip::from_text("   ".into()));
        remember(&mut h, Clip::from_text("x".repeat(MAX_LEN + 1)));
        assert_eq!(h.len(), 2, "пустое и огромное не храним");
        for i in 0..30 {
            remember(&mut h, Clip::from_text(i.to_string()));
        }
        assert_eq!(h.len(), KEEP);
        assert_eq!(shown(&h)[0], "29");
    }

    #[test]
    fn at_most_five_images_stay_and_the_oldest_leave_first() {
        let mut h = VecDeque::new();
        for seed in 0..7 {
            remember(&mut h, Clip::image(picture(4, 3, seed)));
            remember(&mut h, Clip::from_text(format!("text {seed}")));
        }
        let seeds: Vec<u8> = h.iter().filter(|e| e.clip.kind() == Filter::Images).map(seed_of).collect();
        assert_eq!(seeds, [6, 5, 4, 3, 2], "новые сверху; две самые старые ушли");
        assert_eq!(h.len(), KEEP_IMAGES + 7, "тексты картинками не вытесняются");
        // Та же картинка второй раз — не дубль, а наверх.
        remember(&mut h, Clip::image(picture(4, 3, 3)));
        let seeds: Vec<u8> = h.iter().filter(|e| e.clip.kind() == Filter::Images).map(seed_of).collect();
        assert_eq!((seeds, seed_of(&h[0])), (vec![3, 6, 5, 4, 2], 3));
        // Всего записей — не больше `KEEP`, картинки среди них или нет.
        for seed in 10..40 {
            remember(&mut h, Clip::image(picture(2, 2, seed)));
            remember(&mut h, Clip::from_text(format!("more {seed}")));
        }
        assert_eq!(h.len(), KEEP);
        assert_eq!(h.iter().filter(|e| e.clip.kind() == Filter::Images).count(), KEEP_IMAGES);
        // Картинка, как ни странно с виду, с таким же размером, но другими пикселями — другая запись.
        assert_ne!(Clip::image(picture(2, 2, 1)), Clip::image(picture(2, 2, 2)));
        assert_eq!(Clip::image(picture(2, 2, 1)), Clip::image(picture(2, 2, 1)));
    }

    #[test]
    fn a_text_gets_its_fix_when_it_is_remembered() {
        let mut h = VecDeque::new();
        remember(&mut h, Clip::from_text("Ghbdtn, rfr ltkf?".into()));
        remember(&mut h, Clip::from_text("Hello, world".into()));
        remember(&mut h, Clip::from_text("https://example.com/ghbdtn".into()));
        let fixes: Vec<Option<String>> = h.iter().map(|e| e.fix.as_ref().map(|f| f.text.clone())).collect();
        assert_eq!(
            fixes,
            [None, None, Some("Привет, как дела?".to_string())],
            "сверху вниз: ссылка, обычный текст, не та раскладка"
        );
    }

    #[test]
    fn a_thumbnail_is_the_middle_of_the_picture_in_three_by_two() {
        // 300 × 100 шире 3 : 2 — края срезаются; слева красное, справа синее, граница посередине кадра миниатюры.
        let wide = dib24(300, 100, |x, _| if x < 150 { [255, 0, 0] } else { [0, 0, 255] });
        let thumb = make_thumb(&wide).expect("24 бита разбираются");
        assert_eq!(thumb.size, [72, 48]);
        let at = |x: usize, y: usize| thumb.pixels[y * 72 + x];
        assert_eq!(
            (at(0, 0), at(35, 20), at(36, 20), at(71, 47)),
            (Color32::RED, Color32::RED, Color32::BLUE, Color32::BLUE)
        );
        // Усреднение: пиксель на границе серединного цвета нет (граница на целом пикселе), у градиента — переход плавный.
        let ramp = dib24(144, 96, |x, _| [(x * 255 / 143) as u8, 0, 0]);
        let thumb = make_thumb(&ramp).unwrap();
        let reds: Vec<u8> = (0..72).map(|x| thumb.pixels[x].r()).collect();
        assert!(reds.windows(2).all(|w| w[0] <= w[1]) && reds[0] < 8 && reds[71] > 247, "{reds:?}");
        // Высокая: срезаются верх и низ; крошечная и в один пиксель — не падает.
        assert!(make_thumb(&dib24(100, 300, |_, y| [y as u8, 0, 0])).is_some());
        assert!(
            make_thumb(&dib24(1, 1, |_, _| [9, 9, 9])).is_some()
                && make_thumb(&dib24(3, 1, |_, _| [9, 9, 9])).is_some()
        );
        // Палитра и чужие байты — не разобрать: строка нарисует значок.
        let mut palette = dib24(4, 4, |_, _| [1, 2, 3]);
        palette[14..16].copy_from_slice(&8u16.to_le_bytes());
        assert!(make_thumb(&palette).is_none() && make_thumb(&[1, 2, 3]).is_none());
        // Выдуманная картинка демо — синее сверху слева, золотое снизу справа.
        let demo = demo_thumb();
        assert_eq!(demo.size, [72, 48]);
        let (tl, br) = (demo.pixels[0], demo.pixels[72 * 48 - 1]);
        assert!(tl.b() > 150 && tl.r() < 80 && br.r() > 200 && br.b() < 110, "{tl:?} {br:?}");
    }

    fn text(i: usize) -> String {
        format!("text {i}")
    }

    #[test]
    fn pin_moves_text_up_and_unpin_brings_it_back() {
        let mut settings = Settings::default();
        let mut toasts = Vec::new();
        let mut clip = Clipboard::default();
        for i in 0..3 {
            remember(&mut clip.history, Clip::from_text(text(i)));
        }
        let mut cx = Ctx { t: texts::tr(RU), lang: RU, monitor: "", toasts: &mut toasts, settings: &mut settings };
        // Закрепить «text 1»: наверх закреплённого и из истории — уходит.
        clip.pin(text(1), &mut cx);
        clip.pin(text(0), &mut cx);
        assert_eq!(cx.settings.pinned, [text(0), text(1)], "новое закреплённое — сверху");
        assert_eq!(shown(&clip.history), [text(2)]);
        // Открепить: возвращается наверх недавнего.
        clip.unpin(0, &mut cx);
        assert_eq!(cx.settings.pinned, [text(1)]);
        assert_eq!(shown(&clip.history), [text(0), text(2)]);
        // Историю выключили — открепленное просто уходит.
        cx.settings.clipboard = false;
        clip.unpin(0, &mut cx);
        assert!(cx.settings.pinned.is_empty());
        assert_eq!(shown(&clip.history), [text(0), text(2)]);
        assert_eq!(*cx.toasts, [("Откреплено".to_string(), Tone::Neutral)], "текст пропал — об этом уведомление");
    }

    /// Закреплённая ссылка и цвет открепляются тем же видом, каким были.
    #[test]
    fn a_pinned_link_comes_back_as_a_link_and_a_color_as_a_color() {
        let mut settings = Settings::default();
        let mut toasts = Vec::new();
        let mut clip = Clipboard::default();
        let mut cx = Ctx { t: texts::tr(EN), lang: EN, monitor: "", toasts: &mut toasts, settings: &mut settings };
        remember(&mut clip.history, Clip::from_text("https://example.com/pin-test".into()));
        remember(&mut clip.history, Clip::from_text("#4f7a12".into()));
        clip.pin("https://example.com/pin-test".into(), &mut cx);
        clip.pin("#4f7a12".into(), &mut cx);
        assert!(clip.history.is_empty(), "закреплённые ушли из истории");
        clip.unpin(0, &mut cx);
        clip.unpin(0, &mut cx);
        assert_eq!(kinds(&clip.history), [Filter::Links, Filter::Colors]);
    }

    #[test]
    fn pinned_limit_warns() {
        let mut settings = Settings { pinned: (0..MAX_PINNED).map(text).collect(), ..Default::default() };
        let mut toasts = Vec::new();
        let mut clip = Clipboard::default();
        let mut cx = Ctx { t: texts::tr(EN), lang: EN, monitor: "", toasts: &mut toasts, settings: &mut settings };
        clip.pin("one more".into(), &mut cx);
        assert_eq!(cx.settings.pinned.len(), MAX_PINNED, "полно — ничего не добавилось");
        assert!(!cx.settings.pinned.contains(&"one more".to_string()));
        assert_eq!(*cx.toasts, [("30 pinned — unpin some first".to_string(), Tone::Warning)]);
    }

    #[test]
    fn first_lines() {
        assert_eq!(first_line("\n  Привет\nмир"), "Привет");
        assert_eq!(first_line(""), "");
        assert_eq!(one_line("a\n\tb   c"), "a b c");
    }

    fn was_written(text: &str) -> bool {
        let want = format!("clip::write_text {text}");
        was_blocked(&want)
    }

    fn was_blocked(what: &str) -> bool {
        sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|w| w == what)
    }

    #[test]
    fn short_titles_cut_the_first_line() {
        assert_eq!(short("Спасибо, получил!", 40), "Спасибо, получил!");
        assert_eq!(short("первая\nвторая", 40), "первая");
        let long = "а".repeat(60);
        let cut = short(&long, 40);
        assert!(cut.ends_with('…') && cut.chars().count() == 40, "{cut}");
    }

    #[test]
    fn pinned_texts_are_offered_and_copied_like_a_click_on_the_row() {
        let (en, ru) = (texts::tr(EN), texts::tr(RU));
        let marker = "search-test-pinned-7731";
        let mut settings = Settings {
            pinned: vec!["Thanks, got it!".into(), format!("{marker}\nsecond line about keys")],
            ..Default::default()
        };
        let mut clip = Clipboard::default();
        let actions = clip.actions(en, EN, &settings);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        let second = format!("Copy “{marker}”");
        assert_eq!(titles, ["Copy “Thanks, got it!”", second.as_str()]);
        assert_eq!(clip.actions(ru, RU, &settings)[0].title, "Скопировать «Thanks, got it!»");
        assert!(actions.iter().all(|a| a.enabled && a.place == en.clipboard));
        // Найти можно по любому слову текста, не только по первой строке.
        assert!(actions[1].words.contains("second line about keys"));
        // Выполнение — то же, что щелчок по закреплённой строке: текст в буфере, «В буфере — вставляйте».
        let mut toasts = Vec::new();
        let mut cx = Ctx { t: en, lang: EN, monitor: "", toasts: &mut toasts, settings: &mut settings };
        clip.command(&Command::RunAction(actions[1].id.clone()), &mut cx);
        assert_eq!(*cx.toasts, [(en.clipboard_done.to_string(), Tone::Success)]);
        assert!(was_written(&format!("{marker}\nsecond line about keys")), "в буфер ушёл весь текст, не первая строка");
        // Закреплённое в историю не идёт, незнакомое и чужое — ничего.
        assert!(clip.history.is_empty());
        clip.command(&Command::RunAction("clipboard:copy:9".into()), &mut cx);
        clip.command(&Command::RunAction("quick:wifi".into()), &mut cx);
        assert_eq!(cx.toasts.len(), 1);
        // Закреплённого нет — и действий нет.
        assert!(Clipboard::default().actions(en, EN, &Settings::default()).is_empty());
    }

    /// Открепили или закрепили другой текст — действие осталось при своём тексте (в «Недавних» не сползает).
    #[test]
    fn a_pinned_text_keeps_its_action_when_the_list_changes() {
        let en = texts::tr(EN);
        let (first, second) = ("search-test first 6631".to_string(), "search-test second 6631".to_string());
        let mut settings = Settings { pinned: vec![first.clone(), second.clone()], ..Default::default() };
        let mut clip = Clipboard::default();
        let id_of_second = clip.actions(en, EN, &settings)[1].id.clone();
        // Закрепили новое — наверху, места старых сдвинулись; первое открепили.
        settings.pinned.insert(0, "search-test third 6631".into());
        settings.pinned.retain(|p| *p != first);
        let mut toasts = Vec::new();
        let mut cx = Ctx { t: en, lang: EN, monitor: "", toasts: &mut toasts, settings: &mut settings };
        clip.command(&Command::RunAction(id_of_second), &mut cx);
        assert!(was_written(&second), "скопирован тот же текст, что был назван");
        assert!(!was_written(&first) && !was_written("search-test third 6631"));
        // Открепили сам текст — его действия нет, а старое «номер» ничего не копирует.
        let id_of_third = clip.actions(en, EN, cx.settings)[0].id.clone();
        cx.settings.pinned.retain(|p| p != "search-test third 6631");
        clip.command(&Command::RunAction(id_of_third), &mut cx);
        assert!(!was_written("search-test third 6631"), "убранный текст не копируется");
        // Одинаковый текст — одинаковый номер; разный — разный.
        assert_eq!(stable_key(&["a"]), stable_key(&["a"]));
        assert_ne!(stable_key(&["a"]), stable_key(&["b"]));
        assert_ne!(stable_key(&["ab", "c"]), stable_key(&["a", "bc"]));
    }

    #[test]
    fn what_is_copied_goes_to_the_history_unless_it_is_pinned() {
        let mut clip = Clipboard::default();
        let settings = Settings { pinned: vec!["pinned text".into()], ..Default::default() };
        clip.take(Some(clip::Copied::Text("pinned text".into())), &settings);
        clip.take(Some(clip::Copied::Text("https://example.com/take-test".into())), &settings);
        clip.take(Some(clip::Copied::Image(picture(5, 4, 9))), &settings);
        clip.take(Some(clip::Copied::Text("   ".into())), &settings);
        clip.take(None, &settings);
        assert_eq!(kinds(&clip.history), [Filter::Images, Filter::Links], "закреплённое, пустое и «ничего» не берём");
    }

    #[test]
    fn turning_remembering_off_takes_nothing_new_and_keeps_the_old_hidden() {
        let mut clip = Clipboard::default();
        remember(&mut clip.history, Clip::from_text("kept 7741".into()));
        remember(&mut clip.history, Clip::image(picture(4, 3, 1)));
        // Номер копирования другой, чем запомнен: «выключено» отрабатывает, буфер при этом не читается.
        let other = clip::sequence().wrapping_add(1000);
        clip.seen = other;
        clip.background(&Settings { clipboard: false, ..Default::default() });
        assert_ne!(clip.seen, other, "номер запомнен — «выключено» отработало");
        assert_eq!(clip.history.len(), 2, "уже запомненное лежит в памяти — включили снова, оно на месте");
    }

    // ─── Карточка в кадрах: ряды, фильтр, кнопки ────────────────────────────

    /// Весь текст, что нарисован в кадре, и где.
    fn painted(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Pos2)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Pos2)>) {
            match shape {
                egui::Shape::Text(t) => {
                    out.push((t.galley.text().to_string(), t.pos + t.galley.rect.center().to_vec2()))
                }
                egui::Shape::Vec(list) => list.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    }

    struct Rig {
        ctx: egui::Context,
        card: Clipboard,
        settings: Settings,
        toasts: Vec<(String, Tone)>,
        lang: Lang,
        time: f64,
        size: Vec2,
        shapes: Vec<egui::epaint::ClippedShape>,
    }

    impl Rig {
        fn new(lang: Lang, width: f32) -> Rig {
            let settings = Settings::default();
            let ctx = egui::Context::default();
            anvil_ui::install(&ctx, crate::look::accent_of(&settings.accent), settings.common.theme);
            let mut rig = Rig {
                ctx,
                card: Clipboard::default(),
                settings,
                toasts: Vec::new(),
                lang,
                time: 0.0,
                size: Vec2::new(width, 260.0),
                shapes: Vec::new(),
            };
            rig.settle();
            rig
        }

        fn frame(&mut self, events: Vec<Event>) {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            self.time += 1.0 / 30.0;
            let (card, settings, toasts, lang) = (&mut self.card, &mut self.settings, &mut self.toasts, self.lang);
            let mut out = self.ctx.run_ui(input, |ui| {
                let mut cx =
                    Ctx { t: texts::tr(lang), lang, monitor: "", toasts: &mut *toasts, settings: &mut *settings };
                card.ui(ui, &mut cx);
            });
            // Рисовать некому: изменения текстур никто не применит, и egui без `clear` на этом упадёт.
            out.textures_delta.clear();
            self.shapes = out.shapes;
        }

        fn settle(&mut self) {
            for _ in 0..4 {
                self.frame(Vec::new());
            }
        }

        /// Что нарисовано в последнем кадре, строками через перевод строки.
        fn text(&self) -> String {
            painted(&self.shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>().join("\n")
        }

        fn has_text(&self, word: &str) -> bool {
            painted(&self.shapes).iter().any(|(t, _)| t == word)
        }

        /// Середина надписи `word` на экране.
        fn at(&self, word: &str) -> Pos2 {
            painted(&self.shapes)
                .into_iter()
                .find(|(t, _)| t == word)
                .unwrap_or_else(|| panic!("нет надписи «{word}»:\n{}", self.text()))
                .1
        }

        /// Прямоугольник виджета, помеченного `anchor` (на прошлом кадре); нет на экране — `None`.
        fn rect(&self, key: &str) -> Option<Rect> {
            self.ctx.read_response(egui::Id::new(("anchor", key))).map(|r| r.rect)
        }

        fn move_to(&mut self, pos: Pos2) {
            self.frame(vec![Event::PointerMoved(pos)]);
            self.settle();
        }

        fn click(&mut self, pos: Pos2) {
            self.move_to(pos);
            let button = |pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        /// Мышь за пределы карточки: ничего не наведено.
        fn away(&mut self) {
            self.move_to(Pos2::new(self.size.x + 50.0, self.size.y + 50.0));
        }

        fn remember(&mut self, clip: Clip, age: u64) {
            remember_at(&mut self.card.history, clip, ago(age));
        }
    }

    /// Все четыре вида, как в демо: текст не в той раскладке сверху (последнее скопированное), потом картинка, ссылка, цвет.
    fn filled(rig: &mut Rig) {
        rig.remember(Clip::from_text("#4f7a12".into()), 480);
        rig.remember(Clip::from_text("https://example.com/report".into()), 300);
        rig.remember(Clip::image(picture(6, 4, 7)), 120);
        rig.remember(Clip::from_text("Ghbdtn, rfr ltkf?".into()), 5);
        rig.settle();
    }

    #[test]
    fn the_card_shows_a_row_for_every_kind() {
        let mut rig = Rig::new(EN, 440.0);
        filled(&mut rig);
        let shown = rig.text();
        for word in ["All", "Text", "Links", "Images", "Colors"] {
            assert!(rig.has_text(word), "чип «{word}»:\n{shown}");
        }
        assert!(
            rig.has_text("Ghbdtn, rfr ltkf?") && rig.has_text("Fix layout"),
            "текст и «Исправить раскладку» у последнего:\n{shown}"
        );
        assert!(rig.has_text("Image · 6 × 4") && rig.has_text("2 min"), "картинка с размером и временем:\n{shown}");
        assert!(shown.contains("example.com/report") && rig.has_text("5 min"), "ссылка: сайт и остальное:\n{shown}");
        assert!(rig.has_text("#4f7a12") && rig.has_text("8 min"), "цвет с кодом:\n{shown}");
        assert!(!rig.has_text("now"), "у строки с «Исправить раскладку» времени нет — там кнопка:\n{shown}");
        // Миниатюра — текстура у закруглённой рамки.
        fn textured(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Rect(r) => r.fill_texture_id() != egui::TextureId::default(),
                egui::Shape::Vec(list) => list.iter().any(textured),
                _ => false,
            }
        }
        assert!(rig.shapes.iter().any(|s| textured(&s.shape)), "миниатюра картинки нарисована");
        assert!(
            rig.has_text("Click to copy again. Pin to keep.") && rig.has_text("Clear"),
            "низ карточки как был:\n{shown}"
        );
        // На русском — свои слова.
        let mut ru = Rig::new(RU, 440.0);
        filled(&mut ru);
        for word in ["Все", "Текст", "Ссылки", "Картинки", "Цвета", "Исправить раскладку", "Картинка · 6 × 4", "2 мин"]
        {
            assert!(ru.has_text(word), "«{word}»:\n{}", ru.text());
        }
    }

    #[test]
    fn the_time_is_now_for_a_fresh_copy_and_hours_for_old_ones() {
        let mut rig = Rig::new(EN, 440.0);
        rig.remember(Clip::from_text("#112233".into()), 2 * 3600 + 5);
        rig.remember(Clip::from_text("Hello, world".into()), 3);
        rig.settle();
        assert!(rig.has_text("now") && rig.has_text("2 h"), "{}", rig.text());
    }

    #[test]
    fn chips_filter_the_history_by_kind() {
        let mut rig = Rig::new(EN, 440.0);
        rig.settings.pinned = vec!["Thanks, got it!".into()];
        filled(&mut rig);
        assert!(rig.has_text("Thanks, got it!"), "закреплённое при «Все» — на месте");
        // Картинки: только картинка, закреплённого нет.
        rig.click(rig.at("Images"));
        let shown = rig.text();
        assert!(rig.has_text("Image · 6 × 4"), "{shown}");
        for gone in ["Ghbdtn, rfr ltkf?", "example.com", "#4f7a12", "Thanks, got it!"] {
            assert!(!shown.contains(gone), "«{gone}» при «Картинки»:\n{shown}");
        }
        // Ссылки и цвета — каждая своя; у ссылки нет картинки.
        rig.click(rig.at("Links"));
        assert!(
            rig.text().contains("example.com") && !rig.has_text("Image · 6 × 4") && !rig.has_text("#4f7a12"),
            "{}",
            rig.text()
        );
        rig.click(rig.at("Colors"));
        assert!(rig.has_text("#4f7a12") && !rig.text().contains("example.com"), "{}", rig.text());
        // «Текст»: закреплённое остаётся, ссылка и цвет — нет.
        rig.click(rig.at("Text"));
        assert!(rig.has_text("Ghbdtn, rfr ltkf?") && rig.has_text("Thanks, got it!"), "{}", rig.text());
        assert!(
            !rig.text().contains("example.com") && !rig.has_text("#4f7a12") && !rig.has_text("Image · 6 × 4"),
            "{}",
            rig.text()
        );
        // Пусто в выбранном — пояснение, а чипы на месте: вернуться есть чем.
        rig.card.history.retain(|e| e.clip.kind() == Filter::Text);
        rig.click(rig.at("Colors"));
        assert!(
            rig.has_text("Nothing here yet") && rig.has_text("All") && !rig.has_text("Thanks, got it!"),
            "{}",
            rig.text()
        );
        rig.click(rig.at("All"));
        assert!(
            rig.has_text("Ghbdtn, rfr ltkf?") && rig.has_text("Thanks, got it!") && !rig.has_text("Nothing here yet"),
            "{}",
            rig.text()
        );
    }

    #[test]
    fn an_empty_history_has_no_chips_and_clear_returns_to_all() {
        let mut rig = Rig::new(EN, 440.0);
        assert!(
            rig.has_text("Copy some text or an image — it will show up here.") && !rig.has_text("Links"),
            "{}",
            rig.text()
        );
        filled(&mut rig);
        rig.click(rig.at("Images"));
        assert_eq!(rig.card.filter, Filter::Images);
        rig.click(rig.at("Clear"));
        assert!(rig.card.history.is_empty() && rig.card.filter == Filter::All);
        assert!(
            rig.has_text("Copy some text or an image — it will show up here.") && !rig.has_text("Links"),
            "{}",
            rig.text()
        );
    }

    #[test]
    fn remembering_off_shows_the_pinned_only() {
        let mut rig = Rig::new(EN, 440.0);
        rig.settings.pinned = vec!["Thanks, got it!".into()];
        filled(&mut rig);
        rig.click(rig.at("Colors"));
        rig.settings.clipboard = false;
        rig.settle();
        let shown = rig.text();
        assert!(rig.has_text("Thanks, got it!") && rig.has_text("Remembering is off in settings."), "{shown}");
        assert!(!rig.has_text("Links") && !rig.has_text("#4f7a12") && !rig.has_text("Fix layout"), "{shown}");
    }

    #[test]
    fn fix_layout_puts_the_fixed_text_on_the_clipboard_and_on_top() {
        let mut rig = Rig::new(EN, 440.0);
        let (wrong, fixed) = ("Ghbdtn, rfr ltkf?", "Привет, как дела?");
        // Старый неверный текст ниже и новый обычный сверху: у нового кнопки нет, у старого — только под мышью.
        rig.remember(Clip::from_text("ghbdtn vbh".into()), 60);
        rig.remember(Clip::from_text("Hello, world".into()), 3);
        rig.settle();
        rig.away();
        assert!(!rig.has_text("Fix layout"), "без мыши кнопки нет:\n{}", rig.text());
        let old = rig.rect("clip-row-1").expect("вторая строка");
        rig.move_to(old.left_center() + Vec2::new(40.0, 0.0));
        assert!(rig.has_text("Fix layout"), "под мышью — есть:\n{}", rig.text());
        rig.away();
        // Последнее скопированное — не в той раскладке: кнопка на виду. Щелчок: в буфере исправленное, оно сверху.
        rig.remember(Clip::from_text(wrong.into()), 0);
        rig.settle();
        rig.away();
        assert!(rig.has_text("Fix layout"), "у последнего скопированного — всегда:\n{}", rig.text());
        let chip = rig.rect("clip-fix-0").expect("кнопка у первой строки");
        rig.click(chip.center());
        assert!(was_written(fixed), "в буфер ушёл исправленный текст");
        assert_eq!(rig.toasts, [(texts::tr(EN).clipboard_done.to_string(), Tone::Success)]);
        assert_eq!(
            shown(&rig.card.history)[..2],
            [fixed.to_string(), wrong.to_string()],
            "исправленное сверху, исходное ниже"
        );
        // У ссылки, цвета и картинки такой кнопки нет никогда, даже под мышью.
        let mut other = Rig::new(EN, 440.0);
        other.remember(Clip::from_text("https://example.com/ghbdtn".into()), 3);
        other.remember(Clip::from_text("#4f7a12".into()), 3);
        other.remember(Clip::image(picture(6, 4, 7)), 3);
        other.settle();
        for n in 0..3 {
            let row = other.rect(&format!("clip-row-{n}")).expect("строка");
            other.move_to(row.center());
            assert!(!other.has_text("Fix layout") && other.rect(&format!("clip-fix-{n}")).is_none(), "строка {n}");
        }
    }

    #[test]
    fn the_fix_button_shrinks_to_an_icon_when_the_card_is_narrow() {
        let wrong = "ghbdtn vbh";
        let mut wide = Rig::new(RU, 460.0);
        wide.remember(Clip::from_text(wrong.into()), 0);
        wide.settle();
        let full = wide.rect("clip-fix-0").expect("кнопка");
        assert!(wide.has_text("Исправить раскладку") && full.width() > 100.0, "{}", wide.text());
        let mut narrow = Rig::new(RU, 230.0);
        narrow.remember(Clip::from_text(wrong.into()), 0);
        narrow.settle();
        let small = narrow.rect("clip-fix-0").expect("кнопка осталась");
        assert!(!narrow.has_text("Исправить раскладку") && small.width() == 24.0, "{}", narrow.text());
        assert!(narrow.has_text(wrong), "и текст на месте — он не уступил кнопке:\n{}", narrow.text());
        // Щелчок по значку делает то же, что по надписи.
        narrow.click(small.center());
        assert!(was_written("привет мир"));
    }

    #[test]
    fn a_click_on_a_row_puts_it_back_in_its_own_form_and_lifts_it() {
        let mut rig = Rig::new(EN, 440.0);
        let link = "https://example.com/put-back-4471";
        rig.remember(Clip::image(picture(11, 7, 3)), 400);
        rig.remember(Clip::from_text("#3c8a21".into()), 300);
        rig.remember(Clip::from_text(link.into()), 200);
        rig.remember(Clip::from_text("Plain text 4471".into()), 100);
        rig.settle();
        // Ряды сверху вниз: текст, ссылка, цвет, картинка. Щелчок — слева от подписи, где кнопок нет.
        let click_row = |rig: &mut Rig, n: usize| {
            let row = rig.rect(&format!("clip-row-{n}")).expect("строка");
            rig.click(row.left_center() + Vec2::new(60.0, 0.0));
        };
        click_row(&mut rig, 3);
        assert!(was_blocked("clip::write_image 11x7 292 bytes"), "картинка — как картинка");
        assert_eq!(kinds(&rig.card.history)[0], Filter::Images, "и поднялась наверх");
        click_row(&mut rig, 3);
        assert!(was_written("#3c8a21"), "цвет — кодом");
        assert_eq!(shown(&rig.card.history)[0], "#3c8a21");
        click_row(&mut rig, 3);
        assert!(was_written(link), "ссылка — текстом");
        assert_eq!(shown(&rig.card.history)[0], link);
        assert_eq!(rig.toasts.len(), 3, "каждый раз «В буфере — вставляйте»");
        assert_eq!(rig.card.history.len(), 4, "повтора не вышло");
    }

    #[test]
    fn a_link_opens_in_the_browser_from_a_button_under_the_mouse() {
        let mut rig = Rig::new(EN, 440.0);
        let link = "https://example.com/open-test-6612";
        rig.remember(Clip::from_text("Plain text 6612".into()), 100);
        rig.remember(Clip::from_text("www.example.org/open-test-6612".into()), 50);
        rig.remember(Clip::from_text(link.into()), 10);
        rig.settle();
        rig.away();
        let (row0, row1) = (rig.rect("clip-row-0").expect("строка"), rig.rect("clip-row-1").expect("строка"));
        // Без мыши кнопок нет видно, но под мышью появляется «Открыть в браузере» левее булавки; у текста её нет совсем.
        assert!(rig.rect("clip-open-2").is_none(), "у текста «Открыть» нет");
        rig.move_to(row0.left_center() + Vec2::new(60.0, 0.0));
        let open = rig.rect("clip-open-0").expect("кнопка у ссылки");
        let pin = rig.rect("clip-pin-0").expect("булавка");
        assert!(open.right() <= pin.left() && (open.center().y - row0.center().y).abs() < 1.0, "{open:?} {pin:?}");
        rig.click(open.center());
        assert!(was_blocked(&format!("launch {link} ")), "открылась именно эта ссылка");
        assert!(rig.toasts.is_empty(), "ничего не копировали — уведомления нет");
        assert_eq!(rig.card.history.len(), 3);
        // «www.» открывается по https.
        rig.move_to(row1.left_center() + Vec2::new(60.0, 0.0));
        let open = rig.rect("clip-open-1").expect("кнопка у второй ссылки");
        rig.click(open.center());
        assert!(was_blocked("launch https://www.example.org/open-test-6612 "));
    }

    #[test]
    fn a_pin_under_the_mouse_pins_texts_links_and_colors_but_not_images() {
        let mut rig = Rig::new(EN, 440.0);
        rig.remember(Clip::image(picture(9, 5, 4)), 400);
        rig.remember(Clip::from_text("#3c8a21".into()), 300);
        rig.remember(Clip::from_text("https://example.com/pin-4471".into()), 200);
        rig.remember(Clip::from_text("Plain text for pin 4471".into()), 100);
        rig.settle();
        rig.away();
        let image_row = rig.rect("clip-row-3").expect("строка картинки");
        rig.move_to(image_row.left_center() + Vec2::new(60.0, 0.0));
        assert!(rig.rect("clip-pin-3").is_none(), "картинку не закрепить: в настройки на диск она не идёт");
        for (n, pinned) in [(2usize, "#3c8a21"), (1, "https://example.com/pin-4471"), (0, "Plain text for pin 4471")] {
            let row = rig.rect(&format!("clip-row-{n}")).expect("строка");
            rig.move_to(row.left_center() + Vec2::new(60.0, 0.0));
            let pin = rig.rect(&format!("clip-pin-{n}")).expect("булавка");
            rig.click(pin.center());
            assert_eq!(rig.settings.pinned[0], pinned);
        }
        assert_eq!(kinds(&rig.card.history), [Filter::Images], "закреплённое из истории ушло");
        assert!(
            rig.has_text("Plain text for pin 4471") && rig.has_text("https://example.com/pin-4471"),
            "и стоит наверху:\n{}",
            rig.text()
        );
    }

    #[test]
    fn the_demo_card_has_one_row_of_each_kind() {
        let mut card = Clipboard::default();
        let ctx = egui::Context::default();
        let rows = card.demo_rows(&ctx, Filter::All);
        assert_eq!(
            rows.iter().map(Row::kind).collect::<Vec<_>>(),
            [Filter::Text, Filter::Images, Filter::Links, Filter::Colors]
        );
        assert_eq!(rows.iter().map(|r| r.age).collect::<Vec<_>>(), [0, 120, 300, 480]);
        assert!(rows[0].current && rows[1..].iter().all(|r| !r.current) && rows.iter().all(|r| r.index.is_none()));
        // «Ghbdtn…» — настоящий разбор, а не заготовка; русских демо-текстов в английском нет.
        assert_eq!(rows[0].fix.as_ref().map(|f| f.text.as_str()), Some("Привет, как дела?"));
        assert!(matches!(&rows[1].look, Look::Image { width: 1280, height: 720, thumb: Some(_) }));
        assert!(matches!(&rows[2].look, Look::Link { site, rest } if site == "example.com" && rest == "/report"));
        assert!(matches!(&rows[3].look, Look::Color { rgb: [0x4f, 0x7a, 0x12], .. }));
        assert_eq!(card.demo_rows(&ctx, Filter::Links).len(), 1);
        assert!(card.demo_rows(&ctx, Filter::Text).iter().all(|r| r.kind() == Filter::Text));
    }
}
