// Что показывает шапка над карточками на данной ширине панели. Чистая логика без окна: размеры частей приходят числами,
// ответ — какие из них оставить. Порядок один на все ширины: каждая часть уходит на своей ширине и обратно не возвращается,
// пока панель сужается (слово «Antresol» или подпись не мигают), потому что решение берётся из ширины панели и размеров,
// а не из того, сколько места осталось от правой части в прошлом кадре.

/// Промежуток между элементами правой части (`item_spacing.x` у правой части шапки).
pub(super) const SPACING: f32 = 4.0;
/// Черта между группами справа: отступ 6, черта 1, отступ 6 и два промежутка (после соседа и после черты). `add_space` сам
/// промежутка не добавляет.
pub(super) const DIVIDER: f32 = 6.0 + 1.0 + 6.0 + 2.0 * SPACING;
/// Четыре значка справа: спрятать, настройки, закрепить, правка полок.
pub(super) const BUTTONS: f32 = 4.0 * 30.0 + 3.0 * SPACING;
/// Зазор между левой и правой частью.
pub(super) const GAP: f32 = 16.0;

/// Размеры частей шапки, точек.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Sizes {
    /// Правка полок: справа «Готово», «Как было» и подсказка, слева ещё кнопка «Полка».
    pub editing: bool,
    // ── Левая часть.
    /// Что не сужается: отступ, знак, черта, поля вкладок, промежутки.
    pub rest: f32,
    /// Подписи вкладок: как есть, каждая не шире «мягкого» предела и не шире «жёсткого».
    pub tabs: f32,
    pub tabs_soft: f32,
    pub tabs_floor: f32,
    /// Слово «Antresol» с промежутком.
    pub word: f32,
    /// В правке: «Полка» значком и с подписью (с промежутком).
    pub add_icon: f32,
    pub add_label: f32,
    // ── Правая часть, обычная.
    /// Есть выбор монитора (мониторов несколько) и сцены (карточка «Экран» на месте); ширина группы.
    pub monitors: Option<f32>,
    pub scenes: Option<f32>,
    /// Подписи «Монитор» и «Сцена» (с промежутком).
    pub label_monitors: f32,
    pub label_scenes: f32,
    /// Поле поиска целиком и значком.
    pub field: f32,
    pub icon: f32,
    // ── Правая часть, в правке.
    pub done: f32,
    pub undo_label: f32,
    pub undo_icon: f32,
    pub hint: f32,
}

/// Что показать.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Plan {
    /// Слово «Antresol».
    pub word: bool,
    /// В правке: «Полка» с подписью, а не значком.
    pub add_label: bool,
    /// Подписи «Сцена» и «Монитор».
    pub labels: bool,
    /// Поиск целым полем, а не значком.
    pub field: bool,
    pub monitors: bool,
    pub scenes: bool,
    /// В правке: «Как было» с подписью; подсказка.
    pub undo_label: bool,
    pub hint: bool,
    /// Сколько займёт правая часть.
    pub right: f32,
    /// Сколько останется подписям вкладок.
    pub tab_room: f32,
}

#[derive(Clone, Copy)]
enum Tabs {
    Natural,
    Soft,
    Floor,
}

/// Одно из состояний шапки: что в нём есть.
#[derive(Clone, Copy)]
struct State {
    word: bool,
    add_label: bool,
    labels: bool,
    field: bool,
    monitors: bool,
    scenes: bool,
    undo_label: bool,
    hint: bool,
    tabs: Tabs,
}

impl Sizes {
    /// Состояния от самого полного к самому тесному. Обычная шапка: уходят подписи «Сцена» и «Монитор», слово «Antresol»,
    /// поле поиска сжимается до значка, подписи вкладок сужаются многоточием (не уже «мягкого» предела), уходят выбор
    /// монитора, потом сцены, подписи вкладок сужаются дальше. Шапка правки: подсказка, подпись у «Как было», слово,
    /// подпись у «Полка», потом вкладки.
    fn chain(&self) -> Vec<State> {
        let mut st = State {
            word: true,
            add_label: self.editing,
            labels: !self.editing,
            field: !self.editing,
            monitors: self.monitors.is_some(),
            scenes: self.scenes.is_some(),
            undo_label: self.editing,
            hint: self.editing,
            tabs: Tabs::Natural,
        };
        let mut chain = vec![st];
        if self.editing {
            st.hint = false;
            chain.push(st);
            st.undo_label = false;
            chain.push(st);
            st.word = false;
            chain.push(st);
            st.add_label = false;
            chain.push(st);
        } else {
            st.labels = false;
            chain.push(st);
            st.word = false;
            chain.push(st);
            st.field = false;
            chain.push(st);
            st.tabs = Tabs::Soft;
            chain.push(st);
            st.monitors = false;
            chain.push(st);
            st.scenes = false;
            chain.push(st);
        }
        st.tabs = Tabs::Floor;
        chain.push(st);
        chain
    }

    /// Левая часть без подписей вкладок.
    fn left_fixed(&self, st: &State) -> f32 {
        let add = match (self.editing, st.add_label) {
            (false, _) => 0.0,
            (true, true) => self.add_label,
            (true, false) => self.add_icon,
        };
        self.rest + if st.word { self.word } else { 0.0 } + add
    }

    fn tabs_of(&self, tabs: Tabs) -> f32 {
        match tabs {
            Tabs::Natural => self.tabs,
            Tabs::Soft => self.tabs_soft,
            Tabs::Floor => self.tabs_floor,
        }
    }

    fn right(&self, st: &State) -> f32 {
        if self.editing {
            let undo = if st.undo_label { self.undo_label } else { self.undo_icon };
            let hint = if st.hint { SPACING + 6.0 + self.hint } else { 0.0 };
            return self.done + SPACING + undo + hint;
        }
        let mut right = BUTTONS + DIVIDER;
        if let Some(width) = self.monitors.filter(|_| st.monitors) {
            right += width + if st.labels { SPACING + self.label_monitors } else { 0.0 };
        }
        if st.monitors && st.scenes && self.monitors.is_some() {
            right += DIVIDER;
        }
        if let Some(width) = self.scenes.filter(|_| st.scenes) {
            right += width + if st.labels { SPACING + self.label_scenes } else { 0.0 };
        }
        if (st.monitors && self.monitors.is_some()) || (st.scenes && self.scenes.is_some()) {
            right += DIVIDER;
        }
        right + if st.field { self.field } else { self.icon }
    }
}

/// Выбрать, что показать на ширине `width`: первое по порядку состояние, что помещается; ничего не помещается — самое
/// тесное (левая часть тогда обрезана по своему месту).
pub(super) fn plan(sz: &Sizes, width: f32) -> Plan {
    let chain = sz.chain();
    let fits = |st: &State| sz.left_fixed(st) + sz.tabs_of(st.tabs) + sz.right(st) + GAP <= width;
    let st = chain.iter().find(|st| fits(st)).or(chain.last()).copied().unwrap_or(chain[0]);
    let right = sz.right(&st);
    Plan {
        word: st.word,
        add_label: st.add_label,
        labels: st.labels,
        field: st.field,
        monitors: st.monitors,
        scenes: st.scenes,
        undo_label: st.undo_label,
        hint: st.hint,
        right,
        tab_room: (width - GAP - right - sz.left_fixed(&st)).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal() -> Sizes {
        Sizes {
            rest: 113.0,
            tabs: 225.0,
            tabs_soft: 190.0,
            tabs_floor: 108.0,
            word: 83.0,
            monitors: Some(160.0),
            scenes: Some(250.0),
            label_monitors: 52.0,
            label_scenes: 36.0,
            field: 204.0,
            icon: 30.0,
            ..Sizes::default()
        }
    }

    fn editing() -> Sizes {
        Sizes {
            editing: true,
            rest: 113.0,
            tabs: 225.0,
            tabs_soft: 190.0,
            tabs_floor: 108.0,
            word: 83.0,
            add_icon: 38.0,
            add_label: 100.0,
            done: 100.0,
            undo_label: 130.0,
            undo_icon: 30.0,
            hint: 260.0,
            ..Sizes::default()
        }
    }

    fn every_width(sz: &Sizes) -> impl Iterator<Item = (f32, Plan)> + '_ {
        (0..=2400).map(|w| w as f32).map(|w| (w, plan(sz, w)))
    }

    /// Сужаем панель: каждая часть, что ушла, обратно не возвращается — ни слово, ни подпись, ни поле, ни сцены.
    #[test]
    fn nothing_comes_back_while_the_panel_narrows() {
        let shapes = [
            normal(),
            Sizes { monitors: None, ..normal() },
            Sizes { scenes: None, ..normal() },
            Sizes { monitors: None, scenes: None, ..normal() },
            editing(),
        ];
        for sz in shapes {
            let plans: Vec<(f32, Plan)> = every_width(&sz).collect();
            for pair in plans.windows(2) {
                let ((narrow_w, narrow), (wide_w, wide)) = (pair[0], pair[1]);
                for (name, a, b) in [
                    ("слово", narrow.word, wide.word),
                    ("подпись у «Полка»", narrow.add_label, wide.add_label),
                    ("подписи", narrow.labels, wide.labels),
                    ("поле", narrow.field, wide.field),
                    ("монитор", narrow.monitors, wide.monitors),
                    ("сцены", narrow.scenes, wide.scenes),
                    ("подпись «Как было»", narrow.undo_label, wide.undo_label),
                    ("подсказка", narrow.hint, wide.hint),
                ] {
                    assert!(!a || b, "{name}: есть на {narrow_w}, а на {wide_w} уже нет ({sz:?})");
                }
            }
        }
    }

    /// Порядок один: подписи, слово, поле, (подписи вкладок), монитор, сцены; в правке — подсказка, подпись, слово, «Полка».
    #[test]
    fn the_parts_go_in_one_order() {
        for sz in [normal(), Sizes { monitors: None, ..normal() }] {
            for (w, p) in every_width(&sz) {
                assert!(!p.labels || p.word, "{w}: подписи без слова");
                assert!(!p.word || p.field, "{w}: слово без поля");
                assert!(!p.field || p.scenes, "{w}: поле без сцен");
                assert!(!p.monitors || p.scenes, "{w}: монитор без сцен");
            }
        }
        let sz = editing();
        for (w, p) in every_width(&sz) {
            assert!(!p.hint || p.undo_label, "{w}: подсказка без подписи у «Как было»");
            assert!(!p.undo_label || p.word, "{w}: подпись у «Как было» без слова");
            assert!(!p.word || p.add_label, "{w}: слово без подписи у «Полка»");
        }
    }

    /// Ширины, на которых части уходят, различны и идут по убыванию в нужном порядке.
    #[test]
    fn the_thresholds_follow_the_order() {
        let sz = normal();
        let first = |pick: fn(&Plan) -> bool| {
            every_width(&sz).find(|(_, p)| pick(p)).map(|(w, _)| w).expect("на какой-то ширине есть")
        };
        let (labels, word, field, monitors, scenes) =
            (first(|p| p.labels), first(|p| p.word), first(|p| p.field), first(|p| p.monitors), first(|p| p.scenes));
        assert!(labels > word && word > field, "{labels} {word} {field}");
        assert!(monitors > scenes, "монитор уходит раньше сцен: {monitors} {scenes}");
        // Поле целиком — пока помещаются слово и подписи вкладок как есть; сцены держатся дольше всего.
        assert!(field > monitors, "поле сжимается раньше, чем уходят монитор и сцены");
    }

    /// Правая часть и подписи вкладок вместе с зазором и левым остатком помещаются, пока помещается хоть одно состояние.
    #[test]
    fn the_planned_parts_fit_when_they_can() {
        let sz = normal();
        let tightest = sz.chain().last().copied().map(|st| sz.left_fixed(&st) + sz.tabs_floor + sz.right(&st) + GAP);
        for (w, p) in every_width(&sz) {
            if w >= tightest.unwrap_or(0.0) {
                assert!(p.tab_room + 0.001 >= sz.tabs_floor, "{w}: вкладкам {} < {}", p.tab_room, sz.tabs_floor);
                assert!(p.right <= w, "{w}: правая часть {} шире панели", p.right);
            }
        }
        // Самая широкая панель — всё на месте.
        let wide = plan(&sz, 2400.0);
        assert!(wide.labels && wide.word && wide.field && wide.monitors && wide.scenes);
        // Совсем узкая — самое тесное, но не пусто: значок поиска есть всегда.
        let narrow = plan(&sz, 100.0);
        assert!(!narrow.labels && !narrow.word && !narrow.field && !narrow.monitors && !narrow.scenes);
    }
}
