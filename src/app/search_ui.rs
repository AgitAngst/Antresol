// Поиск действий: поле в шапке, вид поиска вместо карточек и выполнение выбранного. Что искать — действия карточек
// (`Module::actions`), сцены и свои действия панели; как искать, считать и разбирать запрос — `crate::search`.
// Действие делает то же, что щелчок на своём месте: тем же модулем, с теми же уведомлениями и «Оставить?».

use eframe::egui::text::{LayoutJob, TextFormat, TextWrapping};
use eframe::egui::{Event, Key, Modifiers};

use super::*;
use crate::modules::Action;
use crate::search::{self, SHOWN};

/// Ширина поля-кнопки в шапке над карточками (макет: 204 × 30).
pub(super) const FIELD_W: f32 = 204.0;
/// Высота строки результата.
const ROW_H: f32 = 40.0;
/// Уже этого вид поиска не показывает «Можно так»: результатам нужно всё место.
const TIPS_MIN_AREA: f32 = 640.0;

/// Поиск открыт: что набрано, какая строка выбрана. Живёт, пока открыт вид поиска; ушли — забыто.
#[derive(Default)]
pub(super) struct Search {
    pub query: String,
    /// Номер выбранной строки среди показанных; первая — 0.
    pub selected: usize,
    /// Отдать полю фокус в этом кадре: поиск только что открыли, щёлкнули по примеру, стёрли запрос.
    pub focus: bool,
    /// Стрелки за кадр: ↓ плюс один, ↑ минус один. Применяются, когда известно, сколько строк.
    step: i32,
    /// Выбранная строка сдвинулась с клавиатуры — пролистать список к ней.
    scroll: bool,
    /// В поле нажали Enter.
    enter: bool,
}

/// Что сделает строка по Enter или щелчку.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Run {
    /// Действие панели или модуля по `Action::id`.
    Action(String),
    /// Положить результат калькулятора в буфер обмена.
    Copy(String),
}

/// Строка результата: что написано и что сделает.
pub(super) struct Hit {
    pub title: String,
    /// Что выделить в названии: куски `(с, по)` в буквах.
    pub marks: Vec<(usize, usize)>,
    /// Вторая строка: где это в панели, а у серой — почему нельзя.
    pub sub: String,
    /// Полное пояснение для подсказки (причина целиком: во второй строке она обрезается).
    pub hint: Option<String>,
    pub icon: Icon,
    pub enabled: bool,
    pub run: Run,
}

/// Что показано в списке.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Listing {
    /// Запрос пуст, есть недавние действия.
    Recent,
    /// Найденное по запросу.
    Results,
    /// Запрос пуст и недавних нет — подсказка «Начните печатать».
    Start,
}

pub(super) struct Found {
    pub listing: Listing,
    /// Показанные строки — не больше `SHOWN`.
    pub hits: Vec<Hit>,
    /// Сколько всего нашлось («Найдено · N»).
    pub total: usize,
}

fn field_id() -> egui::Id {
    egui::Id::new("search-field")
}

/// Размер текста в точках.
fn text_w(ui: &Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER).size().x
}

impl App {
    // ─── Открыть и закрыть ──────────────────────────────────────────────────

    /// Открыть поиск вместо карточек; `initial` — с чего начинается запрос (набранная буква). Данные карточек всех полок
    /// перечитываются: действия строятся по ним.
    pub(super) fn open_search(&mut self, initial: &str) {
        self.search = Search { query: initial.to_string(), focus: true, ..Search::default() };
        self.view = View::Search;
        self.refreshed = None;
    }

    /// Назад к карточкам (Esc, крестик, выполнили строку). Набранное забывается.
    pub(super) fn close_search(&mut self) {
        self.search = Search::default();
        if self.view == View::Search {
            self.view = View::Cards;
        }
    }

    /// Можно ли открыть поиск: у панели есть клавиатура (её вызвали клавишей или в ней нажали мышью; выдвинутая мышью у края
    /// её не забирает, и набранное в чужой программе в поиск не попадает), поверх карточек, не в правке полок и сцены, не пока
    /// вводят текст или ждут сочетание.
    fn can_open_search(&self) -> bool {
        self.edge.keyboard()
            && self.view == View::Cards
            && self.editing.is_none()
            && self.renaming.is_none()
            && self.capture.is_none()
            && !self.capture_panel
            && !self.typing_here()
    }

    /// Ctrl+K и Ctrl+F открывают поиск, пока фокус не в поле ввода (в «owner/repo» у GitHub они не нужны); печатная буква
    /// или цифра, пока панель открыта и фокус не в поле ввода, — тоже, с этой буквой, но не пока какой-то модуль ждёт
    /// ответа («Оставить этот режим?»): набор не должен уводить вопрос под поиск. Открыт — те же сочетания возвращают фокус
    /// полю.
    pub(super) fn search_keys(&mut self, ctx: &egui::Context) {
        if self.view == View::Search {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::K) | i.consume_key(Modifiers::CTRL, Key::F)) {
                self.search.focus = true;
            }
            return;
        }
        if !self.can_open_search() || ctx.text_edit_focused() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::K) | i.consume_key(Modifiers::CTRL, Key::F)) {
            self.open_search("");
            return;
        }
        if self.modules.iter().any(|m| m.busy()) {
            return;
        }
        // Всё, что напечатали за кадр (быстрый набор приносит несколько букв сразу).
        let typed: String = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    Event::Text(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect()
        });
        if typed.chars().next().is_some_and(char::is_alphanumeric) {
            // Буквы забираем себе: иначе поле вставило бы их ещё раз.
            ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Text(_))));
            self.open_search(&typed);
        }
    }

    // ─── Что искать ─────────────────────────────────────────────────────────

    /// Все действия по порядку: сцены, потом карточки по полкам (полка дописывается к месту), потом своё у панели. Карточки,
    /// которых нет ни на одной полке, не дают ничего.
    pub(super) fn all_actions(&self) -> Vec<Action> {
        let (t, lang) = (texts::tr(self.lang()), self.lang());
        let mut out: Vec<Action> = Vec::new();

        // Сцены: то же, что щелчок по чипу в шапке.
        let waiting = self.modules.iter().any(|m| m.blocks_scene());
        let active = self.active_scene();
        for (i, scene) in self.settings.profiles.iter().enumerate() {
            let stands = active == Some(i);
            let set = texts::scene_set(lang, &scene.name);
            let one_mode = match scene.monitors.as_slice() {
                [m] => Some(texts::scene_mode(lang, m.width, m.height, m.hz, None, None)),
                _ => None,
            };
            let title = match (stands && scene.undo, one_mode) {
                (true, _) => format!("{set}: {}", t.search_undo),
                (false, Some(mode)) => format!("{set}: {mode}"),
                (false, None) => set,
            };
            let place = match &scene.hotkey {
                Some(key) => format!("{} · {key}", t.scenes),
                None => t.scenes.to_string(),
            };
            let mut words = String::from("сцена scene");
            for row in SceneRow::ALL.into_iter().filter(|r| scene.has(*r)) {
                for l in [Lang::Ru, Lang::En] {
                    words += " ";
                    words += texts::scene_row_label(texts::tr(l), row);
                }
            }
            for m in &scene.monitors {
                words += &format!(" {}x{} {}x{}@{}", m.width, m.height, m.width, m.height, m.hz);
            }
            // Стоящая сцена возвращается (если возврат включён), остальные ставятся; пока висит чужое «Оставить?» — ни то ни
            // другое.
            let blocked =
                if stands { scene.undo && self.modules.iter().any(|m| m.blocks_undo(scene)) } else { waiting };
            let id = format!("scene:{}", scene.id);
            out.push(Action::new(id, title, place, Icon::Layers).words(&words).off_if(blocked, t.answer_first));
        }

        // Карточки: каждая даёт то, что умеет; место — «карточка · полка».
        for shelf in &self.settings.shelves {
            let shelf_name = texts::shelf_title(t, &shelf.name);
            for key in &shelf.cards {
                let Some(m) = self.module_index(key) else { continue };
                for mut action in self.modules[m].actions(t, lang, &self.settings) {
                    // По имени карточки тоже находится («яркость» — и ночной свет, он в карточке «Яркость»).
                    action.words = format!("{} {}", action.words, action.place);
                    action.place = format!("{} · {shelf_name}", action.place);
                    out.push(action);
                }
            }
        }

        // Своё у панели. Полка называется своим именем из файла (`@sys`, свои как есть), а не местом среди вкладок: переставили
        // или добавили полку — «Недавние» не сползают.
        let panel = "Antresol";
        for shelf in self.settings.shelves.iter().enumerate().filter(|(i, _)| *i != self.shelf).map(|(_, s)| s) {
            let title = texts::search_open_shelf(lang, texts::shelf_title(t, &shelf.name));
            let id = format!("shelf:{}", shelf.name);
            out.push(Action::new(id, title, panel, Icon::Tiles).words("полка вкладка shelf tab"));
        }
        let edit = Action::new("app:edit", t.shelves_edit, panel, Icon::Pencil);
        out.push(edit.words("карточки полки порядок править edit shelves cards layout"));
        let settings = Action::new("app:settings", t.settings, panel, Icon::Gear);
        out.push(settings.words("параметры тема язык цвет автозапуск preferences theme language color autostart"));
        out.push(Action::new("app:hide", t.search_hide, panel, Icon::ChevronUp).words("закрыть убрать close hide"));

        // Пока мышь над самой панелью, то, что берёт что-то под мышью, брать нечем.
        for action in out.iter_mut().filter(|a| a.needs_free_pointer && self.pointer_over) {
            action.enabled = false;
            action.reason = Some(t.search_pointer.to_string());
        }
        out
    }

    /// Строки для запроса: пустой — недавние; иначе найденное (`crate::search::find`), а перед ним (или после первого, если
    /// что-то нашлось: «яркость 50» — это яркость, а не таймер «яркость») результат калькулятора или таймер из запроса.
    pub(super) fn found(&self) -> Found {
        let (t, lang) = (texts::tr(self.lang()), self.lang());
        let actions = self.all_actions();
        let query = self.search.query.trim();
        let hit_of = |a: &Action, marks: Vec<(usize, usize)>| Hit {
            title: a.title.clone(),
            marks,
            sub: a.reason.clone().unwrap_or_else(|| a.place.clone()),
            hint: a.reason.clone(),
            icon: a.icon,
            enabled: a.enabled,
            run: Run::Action(a.id.clone()),
        };
        if query.is_empty() {
            let hits: Vec<Hit> = self
                .recent
                .iter()
                .filter_map(|id| actions.iter().find(|a| a.id == *id))
                .take(SHOWN)
                .map(|a| hit_of(a, Vec::new()))
                .collect();
            let listing = if hits.is_empty() { Listing::Start } else { Listing::Recent };
            let total = hits.len();
            return Found { listing, hits, total };
        }
        let items: Vec<(&str, &str)> = actions.iter().map(|a| (a.title.as_str(), a.words.as_str())).collect();
        let recent: Vec<usize> = self.recent.iter().filter_map(|id| actions.iter().position(|a| a.id == *id)).collect();
        let mut hits: Vec<Hit> = search::find(query, &items, &recent)
            .into_iter()
            .map(|i| hit_of(&actions[i], search::marks(query, &actions[i].title)))
            .collect();
        if let Some(value) = search::calc(query, lang) {
            let result = search::number(value, lang);
            let title = format!("{query} = {result}");
            let end = title.chars().count();
            let marks = vec![(end - result.chars().count(), end)];
            let (sub, run) = (t.search_calc.to_string(), Run::Copy(result));
            hits.insert(0, Hit { title, marks, sub, hint: None, icon: Icon::Tiles, enabled: true, run });
        } else if let Some(timer) = search::timer(query)
            // Таймер заводит карточка «Таймеры»: убрана с полок — строки нет (как у остальных её действий).
            && let Some(shelf) = self.settings.shelf_of("timers").and_then(|s| self.settings.shelves.get(s))
        {
            let name = timer.name.clone().unwrap_or_default();
            let id = format!("timers:new:{}:{name}", timer.minutes);
            let sub = format!("{} · {}", t.timers, texts::shelf_title(t, &shelf.name));
            let title = texts::search_timer(lang, timer.name.as_deref(), timer.minutes);
            let hit = Hit {
                title,
                marks: Vec::new(),
                sub,
                hint: None,
                icon: Icon::Clock,
                enabled: true,
                run: Run::Action(id),
            };
            hits.insert(usize::from(!hits.is_empty()), hit);
        }
        let total = hits.len();
        hits.truncate(SHOWN);
        Found { listing: Listing::Results, hits, total }
    }

    // ─── Выполнить ──────────────────────────────────────────────────────────

    /// Выполнить строку и вернуться к карточкам.
    pub(super) fn run_hit(&mut self, run: Run) {
        let t = texts::tr(self.lang());
        self.close_search();
        match run {
            Run::Copy(text) => match sys::clip::write_text(&text) {
                Ok(()) => self.toasts.push(t.clipboard_done, Tone::Success),
                Err(e) => {
                    eprintln!("clipboard write failed: {e}");
                    self.toasts.push(format!("{}: {e}", t.failed), Tone::Danger);
                }
            },
            Run::Action(id) => {
                self.remember_action(&id);
                self.run_action(&id);
            }
        }
    }

    /// Недавнее действие — наверх списка; помнится, пока работает программа.
    pub(super) fn remember_action(&mut self, id: &str) {
        // Таймер из запроса каждый раз свой («чай 5»): в недавние его не берём.
        if id.starts_with("timers:new:") {
            return;
        }
        self.recent.retain(|r| r != id);
        self.recent.insert(0, id.to_string());
        self.recent.truncate(search::KEEP_RECENT);
    }

    /// Своё действие панели или команда модулю — то, что сделал бы щелчок на своём месте.
    pub(super) fn run_action(&mut self, id: &str) {
        if let Some(scene_id) = id.strip_prefix("scene:").and_then(|n| n.parse::<u64>().ok()) {
            if let Some(i) = self.settings.profiles.iter().position(|p| p.id == scene_id) {
                let device = self.monitor().map(|m| m.device.clone()).unwrap_or_default();
                self.scene_click(i, &device);
            }
        } else if let Some(name) = id.strip_prefix("shelf:") {
            // Такой полки уже нет (убрали, переименовали) — ничего.
            if let Some(i) = self.settings.shelves.iter().position(|s| s.name == name) {
                self.set_shelf(i);
            }
        } else if id == "app:edit" {
            self.start_edit();
        } else if id == "app:settings" {
            self.view = View::Settings;
        } else if id == "app:hide" {
            self.edge.hide();
        } else {
            self.command_all(Command::RunAction(id.to_string()));
            // Ждёт ответа («Оставить этот режим?») или открыл ввод (пояс для часов), а карточки нет на открытой полке —
            // открыть ту полку, где она: иначе вопрос остался бы невидимым.
            self.reveal_busy();
            self.reveal_typing();
        }
    }

    /// Модуль что-то вводит (поиск пояса у «Часов»), а его карточки нет на открытой полке — открыть полку с ней.
    fn reveal_typing(&mut self) {
        if self.shelf_modules(self.shelf).into_iter().any(|m| self.modules[m].typing()) {
            return;
        }
        let shelf = self.modules.iter().find(|m| m.typing()).and_then(|m| self.settings.shelf_of(m.key()));
        if let Some(shelf) = shelf {
            self.set_shelf(shelf);
        }
    }

    // ─── Шапка ──────────────────────────────────────────────────────────────

    /// Поле-кнопка в шапке над карточками: «Найти действие» и «Ctrl K»; `full` — полное, иначе только значок `Search`.
    /// Щелчок открывает поиск.
    pub(super) fn search_button(&mut self, ui: &mut Ui, full: bool) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let clicked = if full {
            let (rect, r) = ui.allocate_exact_size(Vec2::new(FIELD_W, 30.0), Sense::click());
            let fill = if r.hovered() { p.hover } else { p.raised };
            ui.painter().rect(rect, 7, fill, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
            let icon = Rect::from_center_size(Pos2::new(rect.left() + 9.0 + 7.0, rect.center().y), Vec2::splat(14.0));
            anvil_ui::icons::paint(ui.painter(), icon, Icon::Search, p.weak);
            // Клавиша справа: моноширинный 11 в рамке.
            let key_font = egui::FontId::monospace(11.0);
            let key_w = text_w(ui, "Ctrl K", key_font.clone()) + 10.0;
            let key = Rect::from_min_size(
                Pos2::new(rect.right() - 5.0 - key_w, rect.center().y - 9.0),
                Vec2::new(key_w, 18.0),
            );
            ui.painter().rect(key, 4, p.card, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
            ui.painter().text(key.center(), egui::Align2::CENTER_CENTER, "Ctrl K", key_font, p.weak);
            // Подпись между значком и клавишей, с многоточием, если не влезла.
            let left = icon.right() + 7.0;
            let mut job =
                LayoutJob::simple_singleline(t.search_field.to_owned(), egui::FontId::proportional(13.0), p.weak);
            job.wrap = TextWrapping::truncate_at_width((key.left() - 6.0 - left).max(10.0));
            let galley = ui.painter().layout_job(job);
            ui.painter().galley(Pos2::new(left, rect.center().y - galley.size().y / 2.0), galley, p.weak);
            w::focus_ring(ui, rect, &r, 7);
            anchor(ui, rect, "search-open");
            // Имя для диктора — «Найти действие»; подсказка с горячей клавишей — в подсказке.
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, t.search_field));
            r.on_hover_text(t.search_tip).on_hover_cursor(CursorIcon::PointingHand).clicked()
        } else {
            let r = w::icon_button(ui, Icon::Search, &format!("{}\n{}", t.search_field, t.search_tip));
            anchor(ui, r.rect, "search-open");
            r.clicked()
        };
        if clicked {
            self.open_search("");
        }
    }

    /// Шапка вида поиска вместо шапки карточек: знак, «Antresol», поле запроса, подсказка клавиш и крестик «Закрыть поиск».
    /// Вкладок, сцен и кнопок нет. Тесно — слово «Antresol» уходит первым, потом подсказка: слово стоит, только пока
    /// подсказка помещается и с ним, а подсказка — пока помещается без него; от ширины панели зависит одно решение за
    /// другим, так что при сужении слово не возвращается.
    pub(super) fn search_header(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        // Стрелки — до поля: иначе оно двигало бы в себе курсор.
        let (up, down) = ui.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::ArrowDown))
        });
        self.search.step += i32::from(down) - i32::from(up);
        let width = ui.max_rect().width();
        let hint_w = text_w(ui, t.search_keys, egui::FontId::proportional(12.5));
        let word_w = text_w(ui, "Antresol", anvil_ui::semibold(14.5)) + 8.0;
        let close_w = 30.0;
        // Что остаётся за знаком (отступ 2, знак 24) и чертой (1), с промежутками по 8, — без слова и со словом.
        let room_bare = width - (2.0 + 8.0 + 24.0 + 8.0 + 1.0 + 8.0);
        let hint_fits = |room: f32| room - close_w - hint_w - 16.0 >= 260.0;
        let (show_hint, show_word) = (hint_fits(room_bare), hint_fits(room_bare - word_w));
        let mut close = false;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(2.0);
            anvil_ui::chrome::app_mark(ui, Icon::Layers, 24.0);
            if show_word {
                ui.label(egui::RichText::new("Antresol").font(anvil_ui::semibold(14.5)).color(p.text));
            }
            vline(ui, p.border);
            let room = ui.available_width();
            let tail = close_w + 8.0 + if show_hint { hint_w + 8.0 } else { 0.0 };
            let free = (room - tail - 12.0).max(120.0);
            // Поле — половина места между чертой и подсказкой (как на макете), не шире 620; без подсказки — всё место.
            let field_w = if show_hint { (free / 2.0).clamp(200.0, 620.0).min(free) } else { free.min(620.0) };
            self.search_field(ui, field_w);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let r = w::icon_button(ui, Icon::Close, t.search_close);
                anchor(ui, r.rect, "search-close");
                close = r.clicked();
                if show_hint {
                    ui.label(egui::RichText::new(t.search_keys).size(12.5).color(p.weak));
                }
            });
        });
        if close {
            self.close_search();
        }
    }

    /// Поле запроса: рамка 1,5 `accent`, значок лупы, текст 14,5 и «Стереть», когда есть что стирать.
    fn search_field(&mut self, ui: &mut Ui, width: f32) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::hover());
        ui.painter().rect(rect, 8, p.card, Stroke::new(1.5, p.accent), StrokeKind::Inside);
        let inner = Rect::from_min_max(rect.min + Vec2::new(10.0, 0.0), rect.max - Vec2::new(4.0, 0.0));
        let mut row = ui.new_child(
            UiBuilder::new()
                .id_salt("search-field-row")
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.spacing_mut().item_spacing.x = 8.0;
        let (icon, _) = row.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        anvil_ui::icons::paint(row.painter(), icon, Icon::Search, p.accent_text);
        let can_clear = !self.search.query.is_empty();
        let edit_w = (row.available_width() - if can_clear { 26.0 + 8.0 } else { 0.0 }).max(40.0);
        let edit = egui::TextEdit::singleline(&mut self.search.query)
            .id(field_id())
            .hint_text(egui::RichText::new(t.search_field).color(p.weak))
            .font(egui::FontId::proportional(14.5))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .char_limit(80)
            .desired_width(edit_w);
        let r = row.add(edit);
        if r.changed() {
            self.search.selected = 0;
            self.search.scroll = true;
        }
        // Enter у одной строки поля: оно отпускает фокус — берём это за «выполнить».
        if r.lost_focus() && row.input(|i| i.key_pressed(Key::Enter)) {
            self.search.enter = true;
        }
        if can_clear {
            let (rect, clear) = row.allocate_exact_size(Vec2::splat(26.0), Sense::click());
            if clear.hovered() {
                row.painter().rect_filled(rect, 6, p.hover);
            }
            let glyph = Rect::from_center_size(rect.center(), Vec2::splat(13.0));
            anvil_ui::icons::paint(row.painter(), glyph, Icon::Close, p.weak);
            w::focus_ring(&row, rect, &clear, 6);
            anchor(&row, rect, "search-clear");
            clear.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, t.search_clear));
            if clear.on_hover_text(t.search_clear).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.search.query.clear();
                self.search.selected = 0;
                self.search.scroll = true;
                self.search.focus = true;
            }
        }
        // Поле не отпускает фокус: пока ни у кого другого его нет, печатать — сюда.
        if std::mem::take(&mut self.search.focus) || row.memory(|m| m.focused().is_none()) {
            r.request_focus();
        }
        anchor(&row, r.rect, "search-input");
    }

    // ─── Вид ────────────────────────────────────────────────────────────────

    /// Вид поиска вместо карточек: слева «Найдено · N» и строки, справа «Можно так».
    pub(super) fn search_view(&mut self, ui: &mut Ui) {
        let t = texts::tr(self.lang());
        let p = Palette::of(ui);
        let area = ui.available_rect_before_wrap();
        let found = self.found();

        // Стрелки двигают выбор (по кругу не ходят); Enter выполняет выбранное, если оно доступно.
        let last = found.hits.len().saturating_sub(1);
        let step = std::mem::take(&mut self.search.step);
        if step != 0 {
            self.search.selected = (self.search.selected as i64 + i64::from(step)).clamp(0, last as i64) as usize;
            self.search.scroll = true;
        }
        self.search.selected = self.search.selected.min(last);
        let selected = self.search.selected;
        let scroll = std::mem::take(&mut self.search.scroll);
        let mut to_run: Option<Run> = None;
        if std::mem::take(&mut self.search.enter)
            && let Some(hit) = found.hits.get(selected).filter(|h| h.enabled)
        {
            to_run = Some(hit.run.clone());
        }

        // Две карточки в долях 2,3 : 1 (макет), зазор 10; узко — только результаты.
        let gap = 10.0;
        let tips = area.width() >= TIPS_MIN_AREA;
        let unit = (area.width() - gap) / 3.3;
        let left_w = if tips { unit * 2.3 } else { area.width() };
        let left = Rect::from_min_size(area.min, Vec2::new(left_w, area.height()));
        let right = Rect::from_min_max(Pos2::new(left.right() + gap, area.top()), area.max);

        let mut card = ui.new_child(UiBuilder::new().id_salt("search-results").max_rect(left));
        w::card_frame(&card).inner_margin(egui::Margin::symmetric(8, 12)).show(&mut card, |ui| {
            ui.set_min_size(ui.available_size());
            // Подпись и пояснения — строкой своей высоты, а не 28 «интерактивных» (как заголовок карточки).
            let line = |ui: &mut Ui, indent: f32, text: &mut dyn FnMut(&mut Ui)| {
                let interact = ui.spacing().interact_size.y;
                ui.spacing_mut().interact_size.y = 16.0;
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    text(ui);
                });
                ui.spacing_mut().interact_size.y = interact;
            };
            match found.listing {
                Listing::Start => line(ui, 8.0, &mut |ui| {
                    w::note(ui, t.search_start);
                }),
                Listing::Recent | Listing::Results => {
                    let label = if found.listing == Listing::Recent {
                        t.search_recent.to_string()
                    } else {
                        format!("{} · {}", t.search_found, found.total)
                    };
                    line(ui, 8.0, &mut |ui| w::section_label(ui, &label));
                    ui.add_space(4.0);
                    if found.hits.is_empty() {
                        line(ui, 10.0, &mut |ui| {
                            w::note(ui, t.search_none);
                        });
                    }
                    egui::ScrollArea::vertical().id_salt("search-rows").auto_shrink([false, false]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (i, hit) in found.hits.iter().enumerate() {
                            let r = search_row(ui, i, hit, i == selected);
                            if i == selected && scroll {
                                r.scroll_to_me(None);
                            }
                            if r.clicked() && hit.enabled {
                                to_run = Some(hit.run.clone());
                            }
                        }
                    });
                }
            }
        });

        // «Можно так»: щелчок по запросу подставляет его.
        let mut pick: Option<String> = None;
        if tips {
            let mut card = ui.new_child(UiBuilder::new().id_salt("search-tips").max_rect(right));
            w::card_frame(&card).inner_margin(egui::Margin::same(14)).show(&mut card, |ui| {
                ui.set_min_size(ui.available_size());
                ui.spacing_mut().item_spacing.y = 6.0;
                w::section_label(ui, t.search_try);
                for (i, (query, what)) in t.tips.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.set_min_height(28.0);
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let font = egui::FontId::monospace(12.5);
                        let width = (text_w(ui, query, font.clone()) + 16.0).max(72.0);
                        let (rect, r) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
                        let fill = if r.hovered() { p.hover } else { p.raised };
                        ui.painter().rect(rect, 6, fill, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
                        ui.painter().text(
                            rect.left_center() + Vec2::new(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            query,
                            font,
                            p.text,
                        );
                        w::focus_ring(ui, rect, &r, 6);
                        anchor(ui, rect, &format!("search-tip-{i}"));
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *query));
                        if r.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            pick = Some((*query).to_string());
                        }
                        let mut job =
                            LayoutJob::simple_singleline((*what).to_owned(), egui::FontId::proportional(12.5), p.weak);
                        job.wrap = TextWrapping::truncate_at_width(ui.available_width().max(10.0));
                        ui.label(job);
                    });
                }
            });
        }

        if let Some(query) = pick {
            self.search.query = query;
            self.search.selected = 0;
            self.search.scroll = true;
            self.search.focus = true;
        }
        if let Some(run) = to_run {
            self.run_hit(run);
        }
    }
}

/// Одна строка результата, 40 высотой: значок в квадрате 28, название с выделенным совпадением, под ним «где» (у серой —
/// причина); выбранная — подложка `raised`, полоска `accent` и справа «Enter». Недоступная — `faint` и не нажимается.
fn search_row(ui: &mut Ui, n: usize, hit: &Hit, selected: bool) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::hover());
    let id = ui.id().with(("search-row", n));
    let r = ui.interact(rect, id, if hit.enabled { Sense::click() } else { Sense::hover() });
    if selected {
        ui.painter().rect_filled(rect, 6, p.raised);
        let bar = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 14.0), Vec2::new(3.0, 28.0));
        ui.painter().rect_filled(bar, 2, p.accent);
    } else if hit.enabled && r.hovered() {
        ui.painter().rect_filled(rect, 6, p.hover);
    }
    // Значок в квадрате 28: выбранная — подложка акцента.
    let mark = Rect::from_min_size(Pos2::new(rect.left() + 10.0, rect.center().y - 14.0), Vec2::splat(28.0));
    ui.painter().rect_filled(mark, 7, if selected && hit.enabled { p.badge_fill(p.accent) } else { p.raised });
    let icon_color = match (hit.enabled, selected) {
        (false, _) => p.faint,
        (true, true) => p.accent_text,
        (true, false) => p.weak,
    };
    anvil_ui::icons::paint(
        ui.painter(),
        Rect::from_center_size(mark.center(), Vec2::splat(15.0)),
        hit.icon,
        icon_color,
    );
    // «Enter» справа у выбранной доступной.
    let mut right = rect.right() - 10.0;
    if selected && hit.enabled {
        let font = egui::FontId::monospace(11.5);
        let w_key = text_w(ui, "Enter", font.clone()) + 12.0;
        let key = Rect::from_min_size(Pos2::new(right - w_key, rect.center().y - 10.0), Vec2::new(w_key, 20.0));
        ui.painter().rect(key, 4, p.card, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
        ui.painter().hline(
            egui::Rangef::new(key.left() + 3.0, key.right() - 3.0),
            key.bottom() - 1.5,
            Stroke::new(1.5, p.border_strong),
        );
        ui.painter().text(key.center() - Vec2::new(0.0, 0.5), egui::Align2::CENTER_CENTER, "Enter", font, p.weak);
        right = key.left() - 10.0;
    }
    let x = mark.right() + 10.0;
    let room = (right - x).max(20.0);
    // Название: кусок запроса — полужирным цветом `accent_text`.
    let (plain_color, mark_color) = if hit.enabled { (p.text, p.accent_text) } else { (p.faint, p.faint) };
    let plain = TextFormat::simple(egui::FontId::proportional(13.5), plain_color);
    let bold = TextFormat::simple(anvil_ui::semibold(13.5), mark_color);
    let chars: Vec<char> = hit.title.chars().collect();
    let mut job = LayoutJob::default();
    let mut at = 0;
    for &(from, to) in &hit.marks {
        let (from, to) = (from.min(chars.len()), to.min(chars.len()));
        if from < at || to <= from {
            continue;
        }
        job.append(&chars[at..from].iter().collect::<String>(), 0.0, plain.clone());
        job.append(&chars[from..to].iter().collect::<String>(), 0.0, bold.clone());
        at = to;
    }
    job.append(&chars[at..].iter().collect::<String>(), 0.0, plain);
    job.wrap = TextWrapping::truncate_at_width(room);
    let title = ui.painter().layout_job(job);
    let mut sub_job = LayoutJob::simple_singleline(hit.sub.clone(), egui::FontId::proportional(12.0), p.weak);
    sub_job.wrap = TextWrapping::truncate_at_width(room);
    let sub = ui.painter().layout_job(sub_job);
    // Обрезано многоточием — полное название (и причину, если серая) в подсказке.
    let (title_cut, sub_cut) = (title.elided, sub.elided);
    let total = title.size().y + sub.size().y;
    let top = rect.center().y - total / 2.0;
    let sub_at = Pos2::new(x, top + title.size().y);
    ui.painter().galley(Pos2::new(x, top), title, plain_color);
    ui.painter().galley(sub_at, sub, p.weak);
    anchor(ui, rect, &format!("search-row-{n}"));
    // Строка берёт фокус при обходе Tab (серая — нет): рамка фокуса, как у остальных кнопок.
    if hit.enabled {
        w::focus_ring(ui, rect, &r, 6);
    }
    let title_text = hit.title.clone();
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, hit.enabled, selected, &title_text));
    let r = match row_tip(hit, title_cut, sub_cut) {
        Some(tip) => r.on_hover_text(tip),
        None => r,
    };
    if hit.enabled { r.on_hover_cursor(CursorIcon::PointingHand) } else { r }
}

/// Подсказка строки: полное название, если оно обрезано; у серой — и причина целиком, у остальных — полное «где», если
/// обрезано оно. Всё влезло и причины нет — подсказки нет.
fn row_tip(hit: &Hit, title_cut: bool, sub_cut: bool) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    if title_cut {
        lines.push(&hit.title);
    }
    match &hit.hint {
        Some(reason) => lines.push(reason),
        None if sub_cut => lines.push(&hit.sub),
        None => {}
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}
