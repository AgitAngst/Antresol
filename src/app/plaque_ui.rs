// Живые плашки у края (2.4): очередь и окно у края за кадр, вид плашки и её кнопки. Что такое плашка и как идёт очередь —
// `crate::plaque`; события отдают модули и `crate::watch`. Панель спрятана — событие плашкой, выдвинута — уведомлением набора.
// Окно плашки не забирает фокус (как полоска): щелчок по кнопке не уводит клавиатуру из программы пользователя. Плашка
// появляется сама, и поэтому её кнопки, крестик и тело принимают щелчок не сразу, а через 400 мс после того, как она встала
// (`plaque::ARM`): щелчок, что целился в окно под ней (вкладки браузера у верхнего края), не должен ничего запустить.

use std::path::PathBuf;

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{Painter, Shape};

use super::*;
use crate::ics;
use crate::plaque::{self, Do, Gate, Glyph, Look, Plaque, Say};

/// Что нажали на плашке за кадр.
enum Hit {
    Button(usize),
    Cross,
    /// Щелчок по телу плашки: открыть панель на полке события.
    Body,
    /// Потянули файл с плашки.
    Drag,
}

/// Насколько надо сдвинуть мышь с нажатой кнопкой, чтобы это было «потянули», а не долгое нажатие (как порог щелчка в egui).
const DRAG_MIN: f32 = 6.0;
/// Сколько после перетаскивания с плашки брошенный на своё же окно файл не принимается в лоток.
const OWN_DROP: Duration = Duration::from_secs(3);
/// Как часто спрашивать, можно ли сейчас показывать уведомления.
const NOTIFY_EVERY: Duration = Duration::from_millis(500);

/// Каким плашкам можно быть на экране: панель спрятана совсем (`panel_shown` — выдвинута, `panel_t` — доля выезда в прошлом
/// кадре, пока она ещё уезжает, окно принадлежит ей), на мониторе нет игры во весь экран (`fullscreen`) и программа не из «Не
/// выезжать» (`excluded`) — иначе никаким. Пользователь не принимает уведомления (`accepts` ложь: показ экрана, игра,
/// «не беспокоить») — ждут все, кроме таймера.
pub(super) fn may_show(panel_shown: bool, panel_t: f32, excluded: bool, fullscreen: bool, accepts: bool) -> Gate {
    if panel_shown || panel_t > 0.0 || excluded || fullscreen {
        Gate::Closed
    } else if accepts {
        Gate::Open
    } else {
        Gate::TimerOnly
    }
}

/// «Все мониторы»: панель следует за мышью, пока спрятана совсем и на экране нет плашки — показанная плашка остаётся на своём
/// мониторе и за мышью, ушедшей на другой, не прыгает.
pub(super) fn follows_mouse(all_monitors: bool, panel_shown: bool, panel_t: f32, plaque_on_screen: bool) -> bool {
    all_monitors && !panel_shown && panel_t <= 0.0 && !plaque_on_screen
}

/// Мышь у верхнего края считается краем панели, только когда она не над плашкой: бросок мыши вверх через плашку не должен
/// выдвигать панель (и превращать плашку в уведомление).
pub(super) fn counts_as_edge(at_edge: bool, over_plaque: bool) -> bool {
    at_edge && !over_plaque
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

    /// Можно ли сейчас показывать уведомления (`SHQueryUserNotificationState`): спрашиваем раз в полсекунды и только пока
    /// есть что показывать.
    pub(super) fn notices_welcome(&mut self, now: Instant) -> bool {
        if !self.plaques.has_any() {
            return true;
        }
        if self.notify_at.is_none_or(|at| now.saturating_duration_since(at) >= NOTIFY_EVERY) {
            self.notify_ok = sys::shell::accepts_notifications();
            self.notify_at = Some(now);
        }
        self.notify_ok
    }

    /// Очередь плашек и окно у края за кадр. Время идёт, пока плашке можно быть на экране (`gate`) и мышь не над ней
    /// (`hovered`); следующая встаёт, когда окно убрало прежнюю; выезд — `LAYOUT`, уход — `LEAVE` набора. Панель выдвинулась —
    /// плашки уходят уведомлениями (обычная, что уже постояла на виду, — молча; важные ждут и покажутся после, а у панели с
    /// клавиатурой уходят и они: плашку из другого окна с клавиатуры не закрыть).
    pub(super) fn plaque_frame(&mut self, ctx: &egui::Context, now: Instant, hovered: bool, gate: Gate) {
        if self.edge.shown {
            for p in self.plaques.take(now, self.edge.keyboard()) {
                self.toast_of(&p);
            }
        }
        let idle = self.plaque_drawn.is_none();
        self.plaques.step(now, hovered, gate, idle);
        let current = self.plaques.current().cloned();
        let want = current.as_ref().is_some_and(|c| gate.allows(c.kind));
        self.plaque_t = Motion::of(ctx).toggle(ctx, egui::Id::new("plaque-shown"), want, motion::LAYOUT);
        if want {
            self.plaque_drawn = current;
        } else if self.plaque_t <= 0.0 {
            self.plaque_drawn = None;
        }
        // Окно выехало до конца — с этого момента идёт отсчёт, когда плашка примет щелчок.
        if want
            && self.plaque_t >= 1.0
            && self.plaque_drawn.as_ref().map(|d| &d.key) == self.plaques.current().map(|c| &c.key)
        {
            self.plaques.stand(now);
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
                // Плашка встречи: ссылка перед запуском проверяется ещё раз, как у кнопки в карточке «Встречи»; не прошла —
                // не открывается.
                let launch = if plaque.kind == plaque::Kind::Meeting {
                    match target.to_str().and_then(ics::check_link) {
                        Some(link) => std::ffi::OsString::from(link.url),
                        None => return,
                    }
                } else {
                    target.clone()
                };
                let opened = sys::launch_os(&launch, std::ffi::OsStr::new(""));
                self.finish_open(plaque, opened, std::path::Path::new(target));
            }
            Do::Reveal(path) => {
                let shown = sys::shell::reveal(path);
                self.finish_open(plaque, shown, path);
            }
            Do::ToTray(path) => {
                self.drop_files(vec![path.clone()]);
                self.plaques.dismiss();
            }
            // Безопасное извлечение занимает время: плашка остаётся, кнопки не нажимаются, пока её не заменит итог.
            Do::Eject { root, name, serial } => {
                self.watch.usb.eject(root.clone(), name.clone(), *serial);
                self.plaques.set_busy(true);
            }
        }
    }

    /// Открыли или показали — плашка уходит; не вышло — вместо неё «Не получилось: имя».
    fn finish_open(&mut self, plaque: &Plaque, result: Result<(), String>, target: &std::path::Path) {
        match result {
            Ok(()) => self.plaques.dismiss(),
            Err(e) => {
                eprintln!("open from plaque failed: {e}");
                let failed = Plaque::new(
                    plaque.kind,
                    format!("failed:{}", plaque.key),
                    plaque.glyph,
                    Tone::Warning,
                    Say::Failed(short_name(self.lang(), target)),
                );
                self.plaques.dismiss();
                self.plaques.push(failed, Instant::now());
            }
        }
    }

    /// Файл потянули с плашки: обычное перетаскивание Windows. Пока оно идёт, окно занято; кнопку отпустили внутри него, а
    /// egui этого не видел — отпускаем за него в следующем кадре (`App::release`). Плашка своё отдала — уходит. Окно — цель
    /// и своего же броска: файл, отпущенный над ним, в лоток не идёт (`take_dropped`).
    fn plaque_drag(&mut self, ctx: &egui::Context, plaque: &Plaque) {
        let Some(path) = &plaque.file else { return };
        if let Err(e) = sys::shell::drag_file(path) {
            eprintln!("drag from plaque failed: {e}");
        }
        self.own_drop = Some((path.clone(), Instant::now()));
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
        // Нажимается, только пока плашка в очереди «текущая»: уезжающая (и ждущая в игре) — просто картинка. И только когда
        // простояла на месте `ARM`: раньше щелчок лишь поглощается (окно сверху, он целился в то, что под ним).
        let live = self.plaques.current().is_some_and(|c| c.key == p.key);
        let armed = live && self.plaques.armed(Instant::now());
        let busy = live && self.plaques.busy();
        let usable = armed && !busy;
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
        // Где лежат кнопки и крестик: над ними курсор спрашивают они сами, а не тело под ними.
        let mut controls: Vec<Rect> = Vec::new();
        if p.sticky && !p.has_done() {
            let cross = Rect::from_min_size(Pos2::new(rect.right() - 6.0 - 24.0, mid - 12.0), Vec2::splat(24.0));
            let r = ui.interact(cross, ui.id().with("plaque-cross"), Sense::click());
            let on = r.hovered() && armed;
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
            controls.push(cross);
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, live, t.plaque_close));
            // Подсказки внутри окна 372 × 48 нет: она не помещалась бы в него. Имя для диктора — «Закрыть».
            let r = if armed { r.on_hover_cursor(CursorIcon::PointingHand) } else { r };
            if r.clicked() && armed {
                hit = Some(Hit::Cross);
            }
            left = cross.left() - 6.0;
        }
        for (i, b) in p.buttons.iter().enumerate().rev() {
            let label = texts::plaque_text(lang, &b.label);
            let width = button_width(ui, &label, b.look, b.external);
            let r = Rect::from_min_size(Pos2::new(left - width, mid - 14.0), Vec2::new(width, 28.0));
            let id = ui.id().with(("plaque-button", i));
            let clicked = plaque_button(ui, id, r, &label, b.look, b.external, !busy, armed);
            anchor(ui, r, &format!("plaque-button-{i}"));
            controls.push(r);
            if clicked && usable {
                hit = Some(Hit::Button(i));
            }
            left = r.left() - 10.0;
        }

        // Название полужирным и пояснение под ним; не помещается — имя режется посередине (расширение файла остаётся на виду),
        // а что режется нечем, — многоточие в конце. Пока ждём итог «Извлечь», пояснение — «Извлекаю…».
        let text_left = mark.right() + 10.0;
        let room = (left - text_left).max(20.0);
        let sub_say = if busy { Say::Ejecting } else { p.sub.clone() };
        let (title_font, sub_font) = (anvil_ui::semibold(13.5), egui::FontId::proportional(12.0));
        let (title, sub) = (
            fit(ui, lang, &p.title, &title_font, room),
            if matches!(sub_say, Say::None) { String::new() } else { fit(ui, lang, &sub_say, &sub_font, room) },
        );
        let line = |ui: &Ui, text: &str, font: egui::FontId, color: Color32| {
            let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
            job.wrap = TextWrapping::truncate_at_width(room);
            ui.painter().layout_job(job)
        };
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

        // Тело: для диктора — слова плашки целиком (рисуются без виджетов, сами диктору не видны; имя не сокращено), курсор —
        // только когда плашка и вправду нажимается; файл тянется, остальное — открывает панель. Подсказки в окне плашки нет.
        let (full_title, full_sub) = (texts::plaque_text(lang, &p.title), texts::plaque_text(lang, &sub_say));
        let said = if full_sub.is_empty() { full_title } else { format!("{full_title}. {full_sub}") };
        body.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, live, &said));
        let over_control =
            ui.input(|i| i.pointer.latest_pos()).is_some_and(|at| controls.iter().any(|c| c.contains(at)));
        let body = if armed && !over_control {
            body.on_hover_cursor(if p.file.is_some() { CursorIcon::Grab } else { CursorIcon::PointingHand })
        } else {
            body
        };
        // Потянули — когда мышь сдвинулась и кнопка всё ещё нажата: долгое нажатие без движения тянуть не начинает, а кадр
        // отпускания не должен отдать бросок программе под курсором.
        let pulled = ui.input(|i| {
            i.pointer.primary_down()
                && i.pointer
                    .press_origin()
                    .zip(i.pointer.latest_pos())
                    .is_some_and(|(from, to)| from.distance(to) >= DRAG_MIN)
        });
        if armed && p.file.is_some() && body.dragged() && pulled {
            hit = Some(Hit::Drag);
        } else if armed && body.clicked() && hit.is_none() {
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

    /// «Живые плашки» в настройках: по чипу на вид событий — включён (подложка акцента) или выключен; чипы переносятся на
    /// новую строку, когда не помещаются. Таймера среди них нет — его плашка всегда.
    pub(super) fn plaque_chips(&mut self, ui: &mut Ui) {
        use plaque::Kind;
        let t = texts::tr(self.lang());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            for (kind, label, key) in [
                (Kind::Bluetooth, t.bluetooth, "bluetooth"),
                (Kind::Usb, t.plaques_usb, "usb"),
                (Kind::Github, t.github, "github"),
                (Kind::Downloads, t.plaques_downloads, "downloads"),
                (Kind::Meeting, t.meetings, "meetings"),
            ] {
                let on = self.settings.plaques.on(kind);
                let r = modules::chip(ui, ("plaque-chip", key), on, label, None);
                anchor(ui, r.rect, &format!("plaque-chip-{key}"));
                if r.clicked() {
                    self.settings.plaques.set(kind, !on);
                }
            }
        });
    }

    /// Бросили файлы на панель: свой же бросок (файл тянули с плашки, а отпустили над её окном) в лоток не идёт.
    pub(super) fn dropped_but_not_own(&mut self, dropped: Vec<PathBuf>) -> Vec<PathBuf> {
        if self.own_drop.as_ref().is_some_and(|(_, at)| at.elapsed() >= OWN_DROP) {
            self.own_drop = None;
        }
        let own = self.own_drop.as_ref().map(|(path, _)| path.clone());
        dropped
            .into_iter()
            .filter(|path| own.as_ref().is_none_or(|own| !path.as_os_str().eq_ignore_ascii_case(own.as_os_str())))
            .collect()
    }
}

/// Как назвать то, что не открылось, словами: файл — его имя, диск — «E:», адрес — сайт, а адрес службы встреч — её имя
/// («Zoom»), как в карточке «Встречи».
fn short_name(lang: Lang, target: &std::path::Path) -> String {
    let target = target.to_string_lossy();
    if let Some(rest) = target.strip_prefix("https://").or_else(|| target.strip_prefix("http://")) {
        if let Some(link) = ics::check_link(&target) {
            return texts::service_name(lang, link.service).to_string();
        }
        return rest.split('/').next().unwrap_or(rest).to_string();
    }
    let name = target.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(&target);
    if name.is_empty() { target.to_string() } else { name.to_string() }
}

/// Слова плашки так, чтобы поместились в `room`: не помещаются — имя внутри слов (файл, диск, устройство, репозиторий) режется
/// посередине (`plaque::middle`), и расширение остаётся на виду; имени нет — слова как есть (концевое многоточие поставит
/// отрисовка).
fn fit(ui: &Ui, lang: Lang, say: &Say, font: &egui::FontId, room: f32) -> String {
    let width = |text: &str| ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Color32::PLACEHOLDER).size().x;
    let full = texts::plaque_text(lang, say);
    let Some(name) = say.name() else { return full };
    if width(&full) <= room {
        return full;
    }
    let shown = |keep: usize| texts::plaque_text(lang, &say.with_name(plaque::middle(name, keep)));
    // Наибольшее число знаков имени, при котором слова помещаются: чем больше знаков, тем шире.
    let (mut lo, mut hi) = (0usize, name.chars().count().saturating_sub(1));
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if width(&shown(mid)) <= room {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    shown(lo)
}

/// Ширина кнопки плашки: поля 10, текст 12,5 (у главной полужирный) и, у открывающей сайт или другую программу, значок «наружу»
/// 12 через 5.
fn button_width(ui: &Ui, label: &str, look: Look, external: bool) -> f32 {
    let font = if look == Look::Primary { anvil_ui::semibold(12.5) } else { egui::FontId::proportional(12.5) };
    let text = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER).size().x;
    10.0 + text + if external { 5.0 + 12.0 } else { 0.0 } + 10.0
}

/// Кнопка плашки: 28 высотой, скругление 6, на цветах набора (по макету — ниже и мельче кнопки набора). `enabled` — кнопка
/// живая (недоступная — `faint`, щелчок не отдаёт, но и на тело не пропускает: оно открыло бы панель); `accept` — плашка
/// уже принимает щелчок (стоит на месте `ARM`): до того под мышью кнопка не светится, рука не появляется и щелчок гасится.
/// Ответ — нажали ли.
#[allow(clippy::too_many_arguments)]
fn plaque_button(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    label: &str,
    look: Look,
    external: bool,
    enabled: bool,
    accept: bool,
) -> bool {
    let pal = Palette::of(ui);
    let r = ui.interact(rect, id, Sense::click());
    let live = enabled && accept;
    let hovered = r.hovered() && live;
    let pressed = r.is_pointer_button_down_on() && live;
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
        // Как `Secondary` набора: рамка `border`, под мышью `border_strong`.
        Look::Secondary => (
            if pressed {
                pal.border
            } else if hovered {
                pal.hover
            } else {
                pal.raised
            },
            if hovered { pal.border_strong } else { pal.border },
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
        modules::paint_external(
            ui.painter(),
            Rect::from_center_size(Pos2::new(x + text_w + 5.0 + 6.0, rect.center().y), Vec2::splat(12.0)),
            ink,
        );
    }
    w::focus_ring(ui, rect, &r, 6);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    let r = if live { r.on_hover_cursor(CursorIcon::PointingHand) } else { r };
    r.clicked() && live
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
        Glyph::Calendar => modules::meetings::paint_calendar(painter, rect, color),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_did_not_open_is_named_by_a_word() {
        use std::path::Path;
        let ru = Lang::Ru;
        assert_eq!(short_name(ru, Path::new(r"C:\Demo\Downloads\report.pdf")), "report.pdf");
        assert_eq!(short_name(ru, Path::new(r"E:\")), "E:");
        assert_eq!(short_name(ru, Path::new("https://github.com/demo/site/actions")), "github.com");
        assert_eq!(short_name(ru, Path::new("plain")), "plain");
        // Ссылка встречи называется службой, как в карточке: «Не получилось: Zoom».
        assert_eq!(short_name(ru, Path::new("https://us02web.zoom.us/j/123456789?pwd=abc")), "Zoom");
        assert_eq!(short_name(Lang::En, Path::new("https://teams.microsoft.com/l/meetup-join/x")), "Teams");
        assert_eq!(short_name(ru, Path::new("https://evil.example/j/1")), "evil.example");
    }
}
