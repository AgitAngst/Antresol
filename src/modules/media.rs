// Медиа: что играет сейчас (системный медиа-сеанс Windows) и кнопки «назад», «пауза», «вперёд».
// Запросы WinRT асинхронные и могут подвисать — всё в своём потоке, окно читает готовое.
// Обложку поток берёт только когда сменился трек: картинка дорогая.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{Ctx, Module};
use crate::sys::media::{self, Action, NowPlaying};
use crate::texts::Tr;

enum Job {
    Read,
    Act(Action),
}

#[derive(Default)]
struct Shared {
    now: Option<NowPlaying>,
    loaded: bool,
    failed: bool,
}

pub struct Media {
    jobs: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
    /// Обложка в egui и для какого трека.
    cover: Option<(String, egui::TextureHandle)>,
}

impl Default for Media {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let s = shared.clone();
        std::thread::spawn(move || worker(rx, s));
        Media { jobs: tx, shared, cover: None }
    }
}

/// Ключ трека — по нему видно, что трек сменился и нужна новая обложка.
fn track_key(n: &NowPlaying) -> String {
    format!("{}\u{1}{}\u{1}{}", n.app, n.artist, n.title)
}

fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
    media::init_thread();
    // Для какого трека обложка уже есть (или больше не просим) и сколько раз просили для текущего:
    // плеер отдаёт обложку не сразу после смены трека.
    let mut cover_key = String::new();
    let mut asked_key = String::new();
    let mut tries = 0u8;
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
        }
        let mut failed = false;
        for j in &jobs {
            if let Job::Act(a) = j
                && let Err(e) = media::act(*a)
            {
                eprintln!("media action failed: {e}");
                failed = true;
            }
        }
        let mut now = media::read(false);
        if let Some(n) = &now {
            let key = track_key(n);
            if key != cover_key {
                if key != asked_key {
                    asked_key = key.clone();
                    tries = 0;
                }
                tries += 1;
                // Нет обложки для этого трека — перечитать с ней; не дал за три раза — без неё.
                if let Some(with) = media::read(true)
                    && track_key(&with) == key
                {
                    if with.cover.is_some() || tries >= 3 {
                        cover_key = key;
                    }
                    now = Some(with);
                }
            } else if let Some(old) = shared.lock().unwrap_or_else(|e| e.into_inner()).now.as_ref() {
                // Тот же трек — обложка прежняя (это `Arc`, картинка не копируется).
                if let Some(n) = now.as_mut() {
                    n.cover = old.cover.clone();
                }
            }
        } else {
            cover_key.clear();
            asked_key.clear();
        }
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.now = now;
        s.loaded = true;
        s.failed |= failed;
    }
}

/// Значок «назад» или «вперёд»: треугольник и черта. Своё: в наборе таких нет.
fn skip_button(ui: &mut Ui, forward: bool, hint: &str) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let hovered = r.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 6, p.hover);
    }
    let color = if hovered { p.text } else { p.weak };
    let c = rect.center();
    let (dir, s) = (if forward { 1.0 } else { -1.0 }, 5.0);
    let tri =
        vec![Pos2::new(c.x - dir * s, c.y - s), Pos2::new(c.x + dir * s * 0.8, c.y), Pos2::new(c.x - dir * s, c.y + s)];
    ui.painter().add(egui::Shape::convex_polygon(tri, color, Stroke::NONE));
    let bar = c.x + dir * (s * 0.8 + 1.5);
    ui.painter().line_segment([Pos2::new(bar, c.y - s), Pos2::new(bar, c.y + s)], Stroke::new(1.8, color));
    r.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl Module for Media {
    fn key(&self) -> &'static str {
        "media"
    }

    fn title(&self, t: &Tr) -> &'static str {
        t.media
    }

    fn refresh(&mut self, _monitor: &str) {
        let _ = self.jobs.send(Job::Read);
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx) {
        let t = cx.t;
        let p = Palette::of(ui);
        let (now, loaded, failed) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let failed = std::mem::take(&mut s.failed);
            (s.now.clone(), s.loaded, failed)
        };
        if failed {
            cx.toasts.push((t.media_failed.to_string(), Tone::Danger));
        }
        if !loaded {
            ui.horizontal(|ui| {
                w::spinner(ui, 16.0);
            });
            return;
        }
        let Some(now) = now else {
            self.cover = None;
            w::note(ui, t.media_none);
            return;
        };

        // Обложка в текстуру — только когда сменился трек.
        let key = track_key(&now);
        if self.cover.as_ref().is_none_or(|(k, _)| *k != key) {
            self.cover = now.cover.as_ref().map(|(cw, ch, rgba)| {
                let img = egui::ColorImage::from_rgba_unmultiplied([*cw as usize, *ch as usize], &rgba[..]);
                (key.clone(), ui.ctx().load_texture("media-cover", img, egui::TextureOptions::LINEAR))
            });
        }

        ui.horizontal(|ui| {
            let size = Vec2::splat(64.0);
            match &self.cover {
                Some((_, tex)) => {
                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    egui::Image::new(tex).corner_radius(8).paint_at(ui, rect);
                }
                None => {
                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    ui.painter().rect_filled(rect, 8, p.raised);
                    anvil_ui::icons::paint(
                        ui.painter(),
                        Rect::from_center_size(rect.center(), Vec2::splat(24.0)),
                        Icon::Play,
                        p.faint,
                    );
                }
            }
            ui.vertical(|ui| {
                let width = ui.available_width();
                let line = |ui: &mut Ui, text: &str, font: egui::FontId, color: Color32| {
                    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
                    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
                    ui.label(job);
                };
                let title = if now.title.is_empty() { t.media_untitled } else { now.title.as_str() };
                line(ui, title, anvil_ui::semibold(14.0), p.text);
                if !now.artist.is_empty() {
                    line(ui, &now.artist, egui::FontId::proportional(13.0), p.weak);
                }
                if !now.app.is_empty() {
                    line(ui, &now.app, egui::FontId::proportional(12.0), p.faint);
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if skip_button(ui, false, t.media_previous).clicked() {
                let _ = self.jobs.send(Job::Act(Action::Previous));
            }
            let (icon, hint) = if now.playing { (Icon::Pause, t.media_pause) } else { (Icon::Play, t.media_play) };
            if w::icon_button(ui, icon, hint).clicked() {
                let _ = self.jobs.send(Job::Act(Action::PlayPause));
            }
            if skip_button(ui, true, t.media_next).clicked() {
                let _ = self.jobs.send(Job::Act(Action::Next));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_changes_with_track() {
        let a = NowPlaying { title: "A".into(), artist: "X".into(), app: "P".into(), ..Default::default() };
        let b = NowPlaying { title: "B".into(), ..a.clone() };
        assert_ne!(track_key(&a), track_key(&b));
        let paused = NowPlaying { playing: true, ..a.clone() };
        assert_eq!(track_key(&a), track_key(&paused), "пауза — не новый трек");
    }
}
