//! Знак Антресоли в интерфейсе: монитор с яркой панелью сверху на светлой плитке. Форма — из
//! `assets/icon/antresol-small.svg` (числа скопированы вместе со сдвигом центровки «Середина»), цвета от темы окна не
//! зависят: это знак программы, а не деталь интерфейса. Набор рисует в «О программе» свой значок из `Icon`, поэтому
//! окно тут собрано заново из его частей (`ICONS.md`, раздел 7).
//!
//! Анимация знака — четыре сцены (`docs/ui-guideline.md`, раздел 7): каждая — чистая функция времени от начала в
//! [`Pose`] (положение, высота и яркость панели, метки), без состояния и без egui — их проверяют тесты. `Player` ведёт
//! текущую сцену: одну на событие, один раз; движение в Windows выключено — знак сразу в конечном состоянии.

use anvil_ui::chrome::{self, AboutAction, AppInfo};
use anvil_ui::motion::{self, Motion, ease_out, ease_standard};
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, semibold};
use eframe::egui::{self, Color32, Painter, Rect, Response, RichText, Sense, Ui, Vec2, pos2, vec2};

/// Плитка — середина её градиента `#F6F8EF → #DCE5CB`.
const TILE: Color32 = Color32::from_rgb(0xE9, 0xEE, 0xDD);
/// Знак `#5E9A1A`.
const GLYPH: (u8, u8, u8) = (0x5E, 0x9A, 0x1A);
/// Метки сцены C светлее плитки.
const LIGHT: Color32 = Color32::from_rgb(0xF6, 0xF8, 0xEF);

/// Размеры в единицах SVG (плитка 1024×1024). Тело монитора, панель, ножка, подставка; общий сдвиг центровки.
const SHIFT_Y: f32 = 23.0;
const BODY: (f32, f32, f32, f32, f32) = (190.0, 220.0, 644.0, 440.0, 60.0);
const PANEL_X: f32 = 250.0;
const PANEL_W: f32 = 524.0;
/// Панель в покое: верх, высота, радиус.
const PANEL_TOP: f32 = 280.0;
const PANEL_H: f32 = 130.0;
const PANEL_R: f32 = 30.0;
/// Панель в сцене B «Прятки»: тонкая полоска.
const STRIP_H: f32 = 26.0;
const STRIP_R: f32 = 13.0;
const STEM: (f32, f32, f32, f32) = (452.0, 660.0, 120.0, 64.0);
const FOOT: (f32, f32, f32, f32) = (352.0, 724.0, 320.0, 56.0);
const BODY_ALPHA: f32 = 0.5;
const STAND_ALPHA: f32 = 0.55;
/// Метки сцены C: диаметр и расстояние между центрами.
const LAMP: f32 = 44.0;
const LAMP_STEP: f32 = 120.0;

/// Сцена A «Выезд»: панель едет из-за кромки экрана знака на место.
pub const ENTER: f32 = motion::ENTER;
/// Сцена C «Модули»: шаг между метками и длина круга, пока идёт проверка (три метки, пауза, гашение).
pub const LAMP_EVERY: f32 = 0.4;
pub const MODULES_PERIOD: f32 = LAMP_EVERY * 3.0 + motion::LEAVE;
/// Сцена D «Пульс»: два удара по 0,6 с; на пике панель тускнеет до этой яркости.
pub const BEAT: f32 = 0.6;
pub const PULSE: f32 = BEAT * 2.0;
const DIM: f32 = 0.45;

/// Положение и вид панели знака в единицах SVG.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Верх панели (в покое 280; выше 220 — за кромкой экрана знака, обрезана).
    pub top: f32,
    pub height: f32,
    pub radius: f32,
    /// Яркость панели: 1 — как в значке.
    pub alpha: f32,
    /// Метки сцены C: 0 — погашена, 1 — горит.
    pub lamps: [f32; 3],
    /// Доля цвета «ошибка» в панели (сцена D при ошибке).
    pub error: f32,
}

impl Pose {
    /// Знак как в значке: панель на месте, раскрыта.
    pub const REST: Pose =
        Pose { top: PANEL_TOP, height: PANEL_H, radius: PANEL_R, alpha: 1.0, lamps: [0.0; 3], error: 0.0 };

    /// Сцена B: `open` — 0 тонкая полоска, 1 раскрыта.
    pub fn open(open: f32) -> Pose {
        let open = open.clamp(0.0, 1.0);
        Pose { height: lerp(STRIP_H, PANEL_H, open), radius: lerp(STRIP_R, PANEL_R, open), ..Pose::REST }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Сцены: что играет и сколько.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    /// A «Выезд»: запуск и открытие «О программе».
    Enter,
    /// D «Пульс»: напоминание или встреча.
    Pulse,
    /// D «Пульс» в цвете «ошибка».
    Alarm,
}

impl Scene {
    /// Длина сцены, с.
    pub fn secs(self) -> f32 {
        match self {
            Scene::Enter => ENTER,
            Scene::Pulse | Scene::Alarm => PULSE,
        }
    }

    /// Поза через `secs` после начала; после конца — покой.
    pub fn pose(self, secs: f32) -> Pose {
        match self {
            Scene::Enter => enter(secs),
            Scene::Pulse => pulse(secs, false),
            Scene::Alarm => pulse(secs, true),
        }
    }
}

/// A «Выезд»: панель стоит выше кромки экрана знака и едет на место (`ease_out`).
pub fn enter(secs: f32) -> Pose {
    let t = ease_out((secs / ENTER).clamp(0.0, 1.0));
    Pose { top: lerp(BODY.1 - PANEL_H, PANEL_TOP, t), ..Pose::REST }
}

/// B «Прятки»: ход раскрытия `v` (0..1 по времени) с кривой `ease_standard`.
pub fn hide(v: f32) -> Pose {
    Pose::open(ease_standard(v.clamp(0.0, 1.0)))
}

/// C «Модули»: три метки загораются по очереди слева направо, потом гаснут разом за `LEAVE`; линейно.
pub fn modules(secs: f32) -> Pose {
    let on = |i: usize| ((secs - i as f32 * LAMP_EVERY) / 0.1).clamp(0.0, 1.0);
    let off = 1.0 - ((secs - LAMP_EVERY * 3.0) / motion::LEAVE).clamp(0.0, 1.0);
    Pose { lamps: [on(0) * off, on(1) * off, on(2) * off], ..Pose::REST }
}

/// D «Пульс»: яркость панели 1 → 0,45 → 1 → 0,45 → 1 (`ease_standard` на каждом спуске и подъёме); при ошибке панель
/// заодно красится в цвет «ошибка».
pub fn pulse(secs: f32, error: bool) -> Pose {
    if !(0.0..PULSE).contains(&secs) {
        return Pose::REST;
    }
    let u = (secs % BEAT) / BEAT;
    let dip = if u < 0.5 { ease_standard(u * 2.0) } else { 1.0 - ease_standard(u * 2.0 - 1.0) };
    Pose { alpha: 1.0 - (1.0 - DIM) * dip, error: if error { dip } else { 0.0 }, ..Pose::REST }
}

/// Ведёт сцену знака: одну на событие, один раз. Один на программу — шапка настроек и «О программе» показывают одно и
/// то же.
#[derive(Default)]
pub struct Player {
    /// Что играет и когда началось (время egui); `None` во втором поле — начнётся в ближайшем `tick`.
    scene: Option<(Scene, Option<f64>)>,
    /// Снимки (`ANTRESOL_VIEW=mark-…`): поза, замершая на нужном кадре сцены.
    held: Option<Pose>,
}

impl Player {
    /// Запуск программы: сцена A, отсчёт — с первого кадра.
    pub fn launched() -> Player {
        Player { scene: Some((Scene::Enter, None)), held: None }
    }

    /// Событие: сцена играет заново с первого кадра (прежняя обрывается).
    pub fn start(&mut self, scene: Scene) {
        self.scene = Some((scene, None));
    }

    /// Для снимков: замереть в позе `pose`, сцены не играют.
    pub fn hold(&mut self, pose: Pose) {
        self.scene = None;
        self.held = Some(pose);
    }

    /// Для снимков: кадр сцены `name` (`A`, `B`, `C`, `D`; `E` — «Пульс» при ошибке) в доле `at` (0..1) её длины.
    pub fn frame(name: &str, at: f32) -> Option<Pose> {
        let at = at.clamp(0.0, 1.0);
        Some(match name {
            "A" => enter(at * ENTER),
            "B" => hide(at),
            "C" => modules(at * LAMP_EVERY * 3.0),
            "D" => pulse(at * BEAT, false),
            "E" => pulse(at * BEAT, true),
            _ => return None,
        })
    }

    /// В начале кадра: поставить время начала новой сцене, снять доигранную.
    pub fn tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some((scene, since)) = &mut self.scene {
            let at = *since.get_or_insert(now);
            if !Motion::of(ctx).enabled || (now - at) as f32 >= scene.secs() {
                self.scene = None;
            }
        }
    }

    /// Поза знака сейчас. Первым играет событие, потом `busy` (сцена C, пока идёт проверка или загрузка), иначе знак
    /// следует за панелью (`shown`, сцена B). Движение выключено — всегда покой.
    pub fn pose(&self, ctx: &egui::Context, shown: bool, busy: bool) -> Pose {
        if let Some(held) = self.held {
            return held;
        }
        let motion = Motion::of(ctx);
        if !motion.enabled {
            return Pose::REST;
        }
        let now = ctx.input(|i| i.time);
        if let Some((scene, Some(at))) = self.scene {
            ctx.request_repaint();
            return scene.pose((now - at) as f32);
        }
        if busy {
            ctx.request_repaint();
            return modules((now as f32) % MODULES_PERIOD);
        }
        // B: вверх `ENTER`, вниз `LAYOUT`; ход по времени, форма — `ease_standard`.
        let secs = if shown { motion::ENTER } else { motion::LAYOUT };
        let v = motion.value(ctx, egui::Id::new("mark-open"), if shown { 1.0 } else { 0.0 }, secs);
        hide(v)
    }
}

fn glyph(alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(GLYPH.0, GLYPH.1, GLYPH.2, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Знак в квадрате `rect`: плитка и монитор в покое.
#[cfg(test)]
fn paint(painter: &Painter, rect: Rect) {
    paint_pose(painter, rect, &Pose::REST, Color32::RED);
}

/// Знак в квадрате `rect` с панелью в позе `pose`; `error` — цвет «ошибка» набора.
pub fn paint_pose(painter: &Painter, rect: Rect, pose: &Pose, error: Color32) {
    let k = rect.width() / 1024.0;
    let at = |x: f32, y: f32| pos2(rect.min.x + x * k, rect.min.y + (y + SHIFT_Y) * k);
    let rr = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), vec2(w * k, h * k));
    painter.rect_filled(rect, (rect.width() * 0.2578).round() as u8, TILE);

    let (bx, by, bw, bh, br) = BODY;
    let body = rr(bx, by, bw, bh);
    painter.rect_filled(body, (br * k).round() as u8, glyph(BODY_ALPHA));
    // Панель выезжает из-за кромки экрана знака: за ней она обрезана.
    let inner = painter.with_clip_rect(body.intersect(painter.clip_rect()));
    let fill = glyph(pose.alpha).lerp_to_gamma(
        Color32::from_rgba_unmultiplied(error.r(), error.g(), error.b(), (pose.alpha * 255.0).round() as u8),
        pose.error,
    );
    let panel = rr(PANEL_X, pose.top, PANEL_W, pose.height);
    inner.rect_filled(panel, (pose.radius * k).round() as u8, fill);
    for (i, lit) in pose.lamps.iter().enumerate() {
        if *lit > 0.0 {
            let c = at(PANEL_X + PANEL_W / 2.0 + (i as f32 - 1.0) * LAMP_STEP, pose.top + pose.height / 2.0);
            inner.circle_filled(c, LAMP / 2.0 * k, LIGHT.gamma_multiply(*lit));
        }
    }
    painter.rect_filled(rr(STEM.0, STEM.1, STEM.2, STEM.3), 0, glyph(STAND_ALPHA));
    painter.rect_filled(rr(FOOT.0, FOOT.1, FOOT.2, FOOT.3), (FOOT.3 / 2.0 * k).round() as u8, glyph(STAND_ALPHA));
}

/// Знак `size` на `size` в потоке интерфейса; `sense` — щелчок или только наведение.
pub fn app_mark(ui: &mut Ui, size: f32, pose: &Pose, sense: Sense) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    paint_pose(ui.painter(), rect, pose, Palette::of(ui).danger);
    response
}

/// Знак 28 px в шапке; щелчок открывает «О программе» (ответ нужен вызывающему).
pub fn brand(ui: &mut Ui, pose: &Pose) -> Response {
    app_mark(ui, 28.0, pose, Sense::click())
}

/// Что показывает «О программе» кроме `AppInfo`.
pub struct AboutExtra<'a> {
    /// Номер сборки (число коммитов); пусто — строки нет.
    pub build: &'a str,
    pub build_label: &'a str,
    /// Строка `anvil-update` о проверке.
    pub status: Option<&'a str>,
}

/// «О программе» — как `chrome::about` набора, но со своим знаком и номером сборки.
pub fn about(
    ctx: &egui::Context,
    open: &mut bool,
    info: &AppInfo,
    extra: &AboutExtra,
    pose: &Pose,
) -> Option<AboutAction> {
    let mut action = None;
    chrome::dialog(ctx, "antresol-about", anvil_ui::tr(ctx, "О программе"), 400.0, open, |ui| {
        let p = Palette::of(ui);
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            app_mark(ui, 56.0, pose, Sense::hover());
            ui.add_space(10.0);
            ui.label(RichText::new(info.name).font(semibold(20.0)).color(p.text));
            let version = format!("{} {}", anvil_ui::tr(ctx, "Версия"), info.version);
            let version = if extra.build.is_empty() {
                version
            } else {
                format!("{version} · {} {}", extra.build_label, extra.build)
            };
            ui.label(RichText::new(version).size(13.0).color(p.weak));
            ui.add_space(6.0);
            ui.label(RichText::new(info.tagline).color(p.text));
            ui.add_space(12.0);
            if w::button(ui, Kind::Secondary, Some(Icon::Refresh), anvil_ui::tr(ctx, "Проверить обновления")).clicked()
            {
                action = Some(AboutAction::CheckUpdates);
            }
            if let Some(status) = extra.status {
                ui.add_space(4.0);
                w::note(ui, status);
            }
            ui.add_space(10.0);
            if !info.repository.is_empty() {
                ui.hyperlink_to(RichText::new(anvil_ui::tr(ctx, "Исходный код")).size(13.0), info.repository);
            }
            ui.label(RichText::new(anvil_ui::tr(ctx, "Часть семьи Anvil")).size(12.0).color(p.faint));
        });
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Pos2;

    #[test]
    fn rest_is_the_icon() {
        assert_eq!(Pose::open(1.0), Pose::REST);
        assert_eq!((Pose::REST.top, Pose::REST.height, Pose::REST.radius), (280.0, 130.0, 30.0));
    }

    #[test]
    fn the_glyph_stays_inside_the_tile() {
        let (bx, by, bw, _, _) = BODY;
        assert!(bx > 0.0 && bx + bw < 1024.0 && by + SHIFT_Y > 0.0 && FOOT.1 + FOOT.3 + SHIFT_Y < 1024.0);
        assert!(PANEL_X >= bx && PANEL_X + PANEL_W <= bx + bw, "панель внутри экрана");
    }

    #[test]
    fn enter_comes_from_behind_the_edge_and_stands() {
        // В начале панель целиком выше кромки экрана знака (верх экрана — 220), в конце на месте.
        let start = enter(0.0);
        assert!(start.top + start.height <= BODY.1 + 1e-3);
        assert_eq!(enter(ENTER), Pose::REST);
        assert_eq!(enter(ENTER * 3.0), Pose::REST);
        assert!(enter(ENTER / 2.0).top > start.top && enter(ENTER / 2.0).top < PANEL_TOP);
        assert_eq!(Scene::Enter.secs(), 0.24);
    }

    #[test]
    fn hide_shrinks_to_a_strip_and_back() {
        let strip = hide(0.0);
        assert_eq!((strip.height, strip.radius), (STRIP_H, STRIP_R));
        assert_eq!(hide(1.0), Pose::REST);
        let mid = hide(0.5);
        assert!(mid.height > STRIP_H && mid.height < PANEL_H);
        // Верх панели не двигается — сжимается к кромке.
        assert_eq!(mid.top, PANEL_TOP);
    }

    #[test]
    fn modules_light_up_in_turn_and_go_out_together() {
        assert_eq!(modules(0.0).lamps, [0.0; 3]);
        let a = modules(LAMP_EVERY * 0.5 + 0.1);
        assert!(a.lamps[0] > 0.99 && a.lamps[1] == 0.0 && a.lamps[2] == 0.0, "{a:?}");
        let b = modules(LAMP_EVERY + 0.15);
        assert!(b.lamps[0] > 0.99 && b.lamps[1] > 0.99 && b.lamps[2] == 0.0, "{b:?}");
        let all = modules(LAMP_EVERY * 2.0 + 0.15);
        assert!(all.lamps.iter().all(|l| *l > 0.99), "{all:?}");
        // Гаснут разом, плавно.
        let fade = modules(LAMP_EVERY * 3.0 + motion::LEAVE / 2.0);
        assert!(
            fade.lamps[0] == fade.lamps[1]
                && fade.lamps[1] == fade.lamps[2]
                && fade.lamps[0] > 0.0
                && fade.lamps[0] < 1.0
        );
        assert_eq!(modules(MODULES_PERIOD).lamps, [0.0; 3]);
    }

    #[test]
    fn pulse_blinks_twice_and_ends_bright() {
        assert_eq!(pulse(0.0, false).alpha, 1.0);
        assert!((pulse(BEAT / 2.0, false).alpha - DIM).abs() < 1e-3, "первый провал");
        assert!(pulse(BEAT, false).alpha > 0.999, "между ударами яркость вернулась");
        assert!((pulse(BEAT * 1.5, false).alpha - DIM).abs() < 1e-3, "второй провал");
        assert_eq!(pulse(PULSE, false), Pose::REST);
        assert_eq!(pulse(BEAT / 2.0, false).error, 0.0);
        assert!((pulse(BEAT / 2.0, true).error - 1.0).abs() < 1e-3, "ошибка красит панель на провале");
        assert_eq!(pulse(0.0, true).error, 0.0, "а на старте панель ещё зелёная");
    }

    #[test]
    fn player_plays_once_and_stops() {
        let ctx = egui::Context::default();
        let mut player = Player::launched();
        let frame = |ctx: &egui::Context, time: f64, player: &mut Player| {
            let input = egui::RawInput { time: Some(time), ..Default::default() };
            let mut pose = Pose::REST;
            let mut out = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx();
                player.tick(ctx);
                pose = player.pose(ctx, true, false);
            });
            out.textures_delta.clear();
            pose
        };
        let first = frame(&ctx, 10.0, &mut player);
        assert_eq!(first.top, enter(0.0).top, "начало с первого кадра, а не от запуска");
        let mid = frame(&ctx, 10.12, &mut player);
        assert!(mid.top > first.top && mid.top < PANEL_TOP);
        assert_eq!(frame(&ctx, 10.5, &mut player), Pose::REST);
        assert!(player.scene.is_none(), "сыграна один раз и снята");
        assert_eq!(frame(&ctx, 11.0, &mut player), Pose::REST, "второй раз сама не играет");
    }

    #[test]
    fn frames_for_screenshots() {
        assert_eq!(Player::frame("A", 1.0), Some(Pose::REST));
        assert_eq!(Player::frame("B", 0.0).map(|p| p.height), Some(STRIP_H));
        assert!(Player::frame("C", 1.0).is_some_and(|p| p.lamps.iter().all(|l| *l > 0.99)));
        assert!(Player::frame("D", 0.5).is_some_and(|p| p.alpha < 0.5));
        assert!(Player::frame("E", 0.5).is_some_and(|p| p.error > 0.99));
        assert_eq!(Player::frame("Z", 0.0), None);
    }

    #[test]
    fn without_motion_the_mark_stands_in_its_final_state() {
        let ctx = egui::Context::default();
        motion::set_reduced(&ctx, true);
        let mut player = Player::launched();
        player.start(Scene::Alarm);
        let mut got = None;
        let mut out = ctx.run_ui(egui::RawInput { time: Some(5.0), ..Default::default() }, |ui| {
            let ctx = ui.ctx();
            player.tick(ctx);
            // Ни событие, ни загрузка, ни спрятанная панель не двигают знак.
            got = Some((player.pose(ctx, false, true), player.pose(ctx, true, false)));
        });
        out.textures_delta.clear();
        let (a, b) = got.unwrap();
        assert_eq!((a, b), (Pose::REST, Pose::REST));
        assert!(player.scene.is_none());
    }

    fn render(dark: bool, size: f32) -> String {
        let ctx = egui::Context::default();
        if dark {
            ctx.set_visuals(egui::Visuals::dark());
        }
        let mut out = ctx.run_ui(Default::default(), |ui| {
            paint(ui.painter(), Rect::from_min_size(Pos2::ZERO, Vec2::splat(size)));
        });
        out.textures_delta.clear();
        format!("{:?}", out.shapes)
    }

    #[test]
    fn the_mark_is_the_same_in_both_themes_and_at_both_sizes() {
        for size in [28.0, 56.0] {
            let (light, dark) = (render(false, size), render(true, size));
            assert!(light.contains("Rect"));
            assert_eq!(light, dark, "тема не меняет знак ({size} px)");
        }
        assert_ne!(render(false, 28.0), render(false, 56.0), "размеры разные");
    }
}
