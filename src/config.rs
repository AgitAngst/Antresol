// Настройки и свои кнопки. Оба файла — в `%APPDATA%\Antresol`, обычный текст, правятся руками:
// `config.txt` — строки `ключ=значение`, `buttons.txt` — по кнопке на строку.

use std::path::PathBuf;

use anvil_ui::{CommonSettings, Icon, Lang, ThemeChoice};

pub const MIN_WIDTH: f32 = 560.0;
pub const MIN_HEIGHT: f32 = 150.0;
pub const MAX_HEIGHT: f32 = 520.0;
/// Сколько любимых разрешений у одного монитора.
pub const MAX_FAVORITES: usize = 3;
/// `monitor=*` — панель у того монитора, к краю которого подошла мышь.
pub const ALL_MONITORS: &str = "*";
/// Длиннее имя сцены в поле правки не бывает, в символах (иначе чип не влезет в шапку); из файла читается любое.
pub const MAX_SCENE_NAME: usize = 24;

/// Разрешение конкретного монитора: `\\.\DISPLAYn`, ширина, высота.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Res {
    pub device: String,
    pub width: u32,
    pub height: u32,
}

impl Res {
    pub fn new(device: &str, width: u32, height: u32) -> Res {
        Res { device: device.to_string(), width, height }
    }

    fn matches(&self, device: &str, width: u32, height: u32) -> bool {
        self.device.eq_ignore_ascii_case(device) && self.width == width && self.height == height
    }

    /// Разбор `\\.\DISPLAY1 2560x1440` — то, что стоит после `favorite=` и в начале `rate=`.
    fn parse(words: &[&str]) -> Option<Res> {
        let (w, h) = words.get(1)?.split_once('x')?;
        Some(Res { device: words.first()?.to_string(), width: w.parse().ok()?, height: h.parse().ok()? })
    }
}

impl std::fmt::Display for Res {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}x{}", self.device, self.width, self.height)
    }
}

/// Избранное переполнено: больше [`MAX_FAVORITES`] у монитора не бывает.
#[derive(Debug, PartialEq)]
pub struct FavoritesFull;

/// Сколько полок можно завести: столько же сочетаний Ctrl + 1…9 и Alt + 1…9.
pub const MAX_SHELVES: usize = 9;
/// Длиннее имя полки не бывает, в символах (иначе вкладка не влезет в шапку).
pub const MAX_SHELF_NAME: usize = 24;

/// Полка — вкладка в шапке: имя и карточки по порядку (ключи модулей).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Shelf {
    /// `@av`, `@sys`, `@hand` — имя по умолчанию (переводится при показе: смена языка файл не трогает);
    /// иначе — своё имя как есть.
    pub name: String,
    pub cards: Vec<String>,
}

impl Shelf {
    /// Имя без знаков, которыми размечена строка файла, не длиннее [`MAX_SHELF_NAME`]; пустое — не годится.
    pub fn clean_name(name: &str) -> String {
        let one = name.replace(['|', '\n', '\r', '\t'], " ");
        one.trim().chars().take(MAX_SHELF_NAME).collect::<String>().trim().to_string()
    }
}

/// Полки по умолчанию: «Экран и звук», «Система», «Под рукой».
pub fn default_shelves() -> Vec<Shelf> {
    crate::modules::DEFAULT_SHELVES
        .iter()
        .map(|(name, cards)| Shelf {
            name: (*name).to_string(),
            cards: cards.iter().map(|c| (*c).to_string()).collect(),
        })
        .collect()
}

/// Знакомый ли ключ карточки.
fn known_card(key: &str) -> bool {
    crate::modules::DEFAULT_SHELVES.iter().any(|(_, cards)| cards.contains(&key))
}

/// Какие плашки событий показывать у края («Живые плашки» в карточке «Панель»). В файле — строка `plaques_off=` со списком
/// выключенного (`bluetooth,usb,github,downloads`): нет строки или она пустая — включено всё. Список выключенного, а не
/// включённого, чтобы новый вид событий (в 2.5 — «Встречи») у тех, кто уже сохранял настройки, был включён сам. Таймер
/// выключить нельзя.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Plaques {
    pub bluetooth: bool,
    pub usb: bool,
    pub github: bool,
    pub downloads: bool,
}

impl Default for Plaques {
    fn default() -> Self {
        Plaques { bluetooth: true, usb: true, github: true, downloads: true }
    }
}

impl Plaques {
    /// Список выключенного из файла: знакомое имя выключает вид, остальное пропускается.
    fn parse(value: &str) -> Plaques {
        let off = list(value);
        let has = |name: &str| off.iter().any(|v| v == name);
        Plaques { bluetooth: !has("bluetooth"), usb: !has("usb"), github: !has("github"), downloads: !has("downloads") }
    }

    /// Включить или выключить плашки этого вида (таймер — всегда включён).
    pub fn set(&mut self, kind: crate::plaque::Kind, on: bool) {
        use crate::plaque::Kind;
        match kind {
            Kind::Bluetooth => self.bluetooth = on,
            Kind::Usb => self.usb = on,
            Kind::Github => self.github = on,
            Kind::Downloads => self.downloads = on,
            Kind::Timer => {}
        }
    }

    /// Показывать ли плашки этого вида.
    pub fn on(&self, kind: crate::plaque::Kind) -> bool {
        use crate::plaque::Kind;
        match kind {
            Kind::Bluetooth => self.bluetooth,
            Kind::Usb => self.usb,
            Kind::Github => self.github,
            Kind::Downloads => self.downloads,
            Kind::Timer => true,
        }
    }
}

impl std::fmt::Display for Plaques {
    /// Только выключенное, по порядку: `bluetooth,usb,github,downloads`; всё включено — пусто.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let all =
            [("bluetooth", self.bluetooth), ("usb", self.usb), ("github", self.github), ("downloads", self.downloads)];
        let off: Vec<&str> = all.iter().filter(|(_, on)| !*on).map(|(name, _)| *name).collect();
        f.write_str(&off.join(","))
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    /// Тема и язык — как во всех программах семьи.
    pub common: CommonSettings,
    /// Размер панели в точках интерфейса.
    pub width: f32,
    pub height: f32,
    /// Монитор, у верхнего края которого живёт панель: `\\.\DISPLAYn`; пусто — основной;
    /// [`ALL_MONITORS`] — тот, к краю которого подошла мышь.
    pub monitor: String,
    /// Сколько держать мышь у края, прежде чем панель выедет.
    pub show_delay_ms: u32,
    /// Сколько панель ждёт, когда мышь ушла, прежде чем спрятаться.
    pub hide_delay_ms: u32,
    /// Не выезжать, когда на мониторе игра или видео во весь экран.
    pub fullscreen_guard: bool,
    /// Тонкая полоска у края, пока панель спрятана: видно, что она есть.
    pub strip: bool,
    /// Программы (имя файла маленькими буквами), над которыми панель не выезжает от края.
    pub exclude: Vec<String>,
    /// Своя клавиша показа панели; `None` — первая свободная из готовых.
    pub panel_hotkey: Option<Hotkey>,
    /// Запоминать скопированные тексты (только в памяти, на диск не пишутся).
    pub clipboard: bool,
    /// Лоток файлов: полные пути, новые сверху.
    pub tray: Vec<String>,
    /// Полки с карточками по порядку (`shelf=`). Всегда хотя бы одна; каждая знакомая карточка стоит на одной полке
    /// или убрана (`hidden`).
    pub shelves: Vec<Shelf>,
    /// Убранные карточки (ключи модулей): ни на какой полке не стоят, но работают в фоне.
    pub hidden: Vec<String>,
    /// Репозитории GitHub для карточки «GitHub»: `владелец/имя`.
    pub github: Vec<String>,
    /// Свой токен GitHub (вписывает владелец сам) — для закрытых репозиториев и большего лимита.
    pub github_token: Option<String>,
    /// Любимые разрешения, до трёх на монитор, в порядке добавления.
    pub favorites: Vec<Res>,
    /// Какая частота была у разрешения в последний раз — её и ставим, когда на него переходят.
    pub rates: Vec<(Res, u32)>,
    /// Сцены (в файле — строки `profile=`): имя, клавиша, экран, система и возврат.
    pub profiles: Vec<Profile>,
    /// Цвет программы: имя готового (`lime`, `violet`…) или свой `#rrggbb` — см. `look`.
    pub accent: String,
    /// Пока что-то играет, спрятанная полоска шире: трек и кнопки плеера.
    pub media_strip: bool,
    /// Какие события показывать плашками у края (`plaques_off=` — список выключенного).
    pub plaques: Plaques,
    /// Где лежит `kamerton.exe` — для «Запустить Kamerton» в карточке «Звук»; пусто — не знаем.
    pub kamerton: String,
    /// Закреплённые тексты «Буфера обмена» (`snippet=`, бывшие «Заготовки»): щелчок кладёт в буфер обмена.
    pub pinned: Vec<String>,
    /// Часы других поясов (карточка «Часы»).
    pub clocks: Vec<Clock>,
}

/// Закреплённый текст в одну строку файла: `\\` — обратная косая, `\n` — перевод строки.
pub fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\r', "").replace('\n', "\\n")
}

pub fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Сколько закреплённых текстов держим.
pub const MAX_PINNED: usize = 30;

/// Часы другого пояса: своё название и ключ пояса Windows («Tokyo Standard Time»).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Clock {
    pub name: String,
    pub zone: String,
}

/// Сколько часов держим.
pub const MAX_CLOCKS: usize = 8;

// ─── Сцены (бывшие профили экрана) ──────────────────────────────────────────────

/// Сочетание клавиш: модификаторы и одна клавиша по имени — «A», «1», «F5», «Space».
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: String,
}

impl Hotkey {
    /// «Ctrl+Alt+1». Без Ctrl, Alt и Win сочетание не годится: одиночная клавиша мешала бы печатать.
    pub fn parse(s: &str) -> Option<Hotkey> {
        let mut h = Hotkey::default();
        for part in s.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" => h.ctrl = true,
                "alt" => h.alt = true,
                "shift" => h.shift = true,
                "win" => h.win = true,
                "" => return None,
                _ if h.key.is_empty() => h.key = part.to_string(),
                _ => return None,
            }
        }
        (h.valid()).then_some(h)
    }

    pub fn valid(&self) -> bool {
        (self.ctrl || self.alt || self.win) && !self.key.is_empty()
    }

    /// Клавиши по отдельности для `kbd`: `["Ctrl", "Alt", "1"]`.
    pub fn keys(&self) -> Vec<&str> {
        let mods = [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")];
        let mut out: Vec<&str> = mods.iter().filter(|(on, _)| *on).map(|(_, k)| *k).collect();
        out.push(&self.key);
        out
    }
}

impl std::fmt::Display for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.keys().join("+"))
    }
}

/// Режим одного монитора в сцене. Масштаб и HDR `None` — не трогать (HDR не записан у профилей, сохранённых до 2.0,
/// и у мониторов без HDR).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProfileMonitor {
    pub device: String,
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    pub scale: Option<u32>,
    /// HDR монитора: `Some(true)` — включить (`hdr` в файле), `Some(false)` — выключить (`sdr`).
    pub hdr: Option<bool>,
}

/// Что сцена меняет в системе, кроме экрана; `None` — не трогать. В файле — четвёртая часть строки `profile=`:
/// `night=0; quiet=1; awake=1; dark=1; wifi=1; bright=80; plan={GUID}` (вкл / выкл — `1` / `0`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SceneSystem {
    /// «Ночной свет» Windows.
    pub night: Option<bool>,
    /// «Тишина» — уведомления Windows выключены.
    pub quiet: Option<bool>,
    /// «Не спать».
    pub awake: Option<bool>,
    /// Тёмная тема Windows.
    pub dark: Option<bool>,
    pub wifi: Option<bool>,
    /// План питания: GUID вида `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` ([`normalize_guid`]).
    pub plan: Option<String>,
    /// Яркость, 0–100, — у всех мониторов, что отвечают по DDC/CI.
    pub bright: Option<u8>,
}

/// GUID плана питания в одном виде — `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` заглавными; на скобки и регистр в файле
/// не смотрим. Не GUID — `None`.
pub fn normalize_guid(s: &str) -> Option<String> {
    let s = s.trim();
    let s = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')).unwrap_or(s);
    let groups: Vec<&str> = s.split('-').collect();
    let sizes = [8, 4, 4, 4, 12];
    let good = groups.len() == sizes.len()
        && groups.iter().zip(sizes).all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()));
    good.then(|| format!("{{{}}}", s.to_ascii_uppercase()))
}

impl SceneSystem {
    /// Ничего не отмечено.
    pub fn is_empty(&self) -> bool {
        *self == SceneSystem::default()
    }

    /// Одно `ключ=значение` из файла: знакомый ключ с годным значением ставится, остальное пропускается (прежнее
    /// значение при этом не трогается). Ответ — принято ли.
    fn set(&mut self, key: &str, value: &str) -> bool {
        let flag = match value {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        };
        match key {
            "night" if flag.is_some() => self.night = flag,
            "quiet" if flag.is_some() => self.quiet = flag,
            "awake" if flag.is_some() => self.awake = flag,
            "dark" if flag.is_some() => self.dark = flag,
            "wifi" if flag.is_some() => self.wifi = flag,
            "plan" if normalize_guid(value).is_some() => self.plan = normalize_guid(value),
            "bright" if value.parse::<u8>().is_ok_and(|b| b <= 100) => self.bright = value.parse().ok(),
            _ => return false,
        }
        true
    }
}

impl std::fmt::Display for SceneSystem {
    /// Ключи в одном порядке: `night; quiet; awake; dark; wifi; bright; plan`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let flag = |on: bool| if on { 1 } else { 0 };
        let mut items: Vec<String> = Vec::new();
        for (key, value) in [
            ("night", self.night),
            ("quiet", self.quiet),
            ("awake", self.awake),
            ("dark", self.dark),
            ("wifi", self.wifi),
        ] {
            if let Some(on) = value {
                items.push(format!("{key}={}", flag(on)));
            }
        }
        if let Some(b) = self.bright {
            items.push(format!("bright={b}"));
        }
        if let Some(p) = &self.plan {
            items.push(format!("plan={p}"));
        }
        f.write_str(&items.join("; "))
    }
}

/// Строка сцены в редакторе: что в ней можно отметить.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SceneRow {
    /// Режим, масштаб и HDR мониторов.
    Screen,
    Bright,
    Night,
    Quiet,
    Awake,
    Dark,
    Wifi,
    Plan,
}

impl SceneRow {
    /// Все строки по порядку на экране правки: две колонки по четыре, сверху вниз.
    pub const ALL: [SceneRow; 8] = [
        SceneRow::Screen,
        SceneRow::Bright,
        SceneRow::Night,
        SceneRow::Quiet,
        SceneRow::Awake,
        SceneRow::Dark,
        SceneRow::Wifi,
        SceneRow::Plan,
    ];
}

/// Сцена — экран и системные настройки, что ставятся одним щелчком или клавишей и (по желанию) возвращаются вторым
/// щелчком. В файле — строка `profile=`, имя в коде осталось от профилей экрана: профиль до 2.1 — это сцена, которая
/// меняет только экран.
#[derive(Clone, Debug)]
pub struct Profile {
    /// Номер сцены на время работы программы: у каждой свой, переименование его не меняет, новая сцена с тем же именем
    /// получает другой. По нему модули помнят, что вернуть. В файл не пишется и при сравнении сцен не учитывается.
    pub id: u64,
    pub name: String,
    pub hotkey: Option<Hotkey>,
    /// Экран: режим, масштаб и HDR мониторов; пусто — экран сцена не меняет.
    pub monitors: Vec<ProfileMonitor>,
    pub system: SceneSystem,
    /// Щелчок по стоящей сцене или её клавиша возвращает всё, что она поменяла (по умолчанию — да).
    pub undo: bool,
}

/// Сцены равны, когда равно то, что записано в файле: номер (`id`) у прочитанной заново сцены другой.
impl PartialEq for Profile {
    fn eq(&self, other: &Profile) -> bool {
        self.name == other.name
            && self.hotkey == other.hotkey
            && self.monitors == other.monitors
            && self.system == other.system
            && self.undo == other.undo
    }
}

impl Eq for Profile {}

/// Следующий свободный номер сцены ([`Profile::id`]).
fn next_scene_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Profile {
    /// Сцена без частей: потом в неё отмечают, что менять.
    pub fn new(name: &str) -> Profile {
        Profile {
            id: next_scene_id(),
            name: name.to_string(),
            hotkey: None,
            monitors: Vec::new(),
            system: SceneSystem::default(),
            undo: true,
        }
    }

    /// Имя без знаков, которыми размечена строка файла.
    pub fn clean_name(name: &str) -> String {
        name.replace(['|', ';', '\n', '\r'], " ").trim().to_string()
    }

    /// Отмечена ли строка.
    pub fn has(&self, row: SceneRow) -> bool {
        let s = &self.system;
        match row {
            SceneRow::Screen => !self.monitors.is_empty(),
            SceneRow::Bright => s.bright.is_some(),
            SceneRow::Night => s.night.is_some(),
            SceneRow::Quiet => s.quiet.is_some(),
            SceneRow::Awake => s.awake.is_some(),
            SceneRow::Dark => s.dark.is_some(),
            SceneRow::Wifi => s.wifi.is_some(),
            SceneRow::Plan => s.plan.is_some(),
        }
    }

    /// Снять отметку со строки: сцена её больше не трогает.
    pub fn clear(&mut self, row: SceneRow) {
        let s = &mut self.system;
        match row {
            SceneRow::Screen => self.monitors.clear(),
            SceneRow::Bright => s.bright = None,
            SceneRow::Night => s.night = None,
            SceneRow::Quiet => s.quiet = None,
            SceneRow::Awake => s.awake = None,
            SceneRow::Dark => s.dark = None,
            SceneRow::Wifi => s.wifi = None,
            SceneRow::Plan => s.plan = None,
        }
    }

    /// Ничего не отмечено — ставить нечего: такая сцена не сохраняется.
    pub fn is_empty(&self) -> bool {
        self.monitors.is_empty() && self.system.is_empty()
    }

    /// `Кино | Ctrl+Alt+3 | \\.\DISPLAY1 3840x2160@60 150% hdr ; … | quiet=1; awake=1; bright=80 | undo=0`.
    /// Имя, клавиша и экран — как у профиля: мониторы через `;`, после режима по желанию масштаб (`100%`) и `hdr` или
    /// `sdr`, в любом порядке, незнакомое слово или повтор — строка негодная; экрана может не быть (пустая часть). Четвёртая
    /// часть — система (`ключ=значение` через `;`; незнакомый ключ и кривое значение пропускаются, сцена остаётся),
    /// пятая — `undo=0`, если возврат выключен. Старая строка из трёх частей — сцена, что меняет только экран. Сцена,
    /// в которой нечего ставить, негодная.
    fn parse(value: &str) -> Option<Profile> {
        let mut parts = value.split('|').map(str::trim);
        let name = Profile::clean_name(parts.next()?);
        let hotkey = Hotkey::parse(parts.next()?);
        let mut monitors = Vec::new();
        for m in parts.next()?.split(';').map(str::trim).filter(|m| !m.is_empty()) {
            let words: Vec<&str> = m.split_whitespace().collect();
            let (size, hz) = words.get(1)?.split_once('@')?;
            let (w, h) = size.split_once('x')?;
            let (mut scale, mut hdr) = (None, None);
            for word in &words[2..] {
                let hdr_word = word.eq_ignore_ascii_case("hdr") || word.eq_ignore_ascii_case("sdr");
                if hdr_word && hdr.is_none() {
                    hdr = Some(word.eq_ignore_ascii_case("hdr"));
                } else if !hdr_word && scale.is_none() {
                    scale = Some(word.trim_end_matches('%').parse().ok()?);
                } else {
                    return None;
                }
            }
            monitors.push(ProfileMonitor {
                device: words.first()?.to_string(),
                width: w.parse().ok()?,
                height: h.parse().ok()?,
                hz: hz.parse().ok()?,
                scale,
                hdr,
            });
        }
        // Четвёртая часть — система, пятая — `undo=0`; `undo` читается и в четвёртой (руками пишут по-всякому).
        let (mut system, mut undo) = (SceneSystem::default(), true);
        for part in parts.take(2) {
            for pair in part.split(';') {
                let Some((key, value)) = pair.split_once('=') else { continue };
                let (key, value) = (key.trim(), value.trim());
                match (key, value) {
                    ("undo", "0") => undo = false,
                    ("undo", "1") => undo = true,
                    _ => {
                        system.set(key, value);
                    }
                }
            }
        }
        let scene = Profile { id: next_scene_id(), name, hotkey, monitors, system, undo };
        (!scene.name.is_empty() && !scene.is_empty()).then_some(scene)
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = self.hotkey.as_ref().map(|h| h.to_string()).unwrap_or_default();
        let monitors: Vec<String> = self
            .monitors
            .iter()
            .map(|m| {
                let scale = m.scale.map(|s| format!(" {s}%")).unwrap_or_default();
                let hdr = m.hdr.map(|on| if on { " hdr" } else { " sdr" }).unwrap_or_default();
                format!("{} {}x{}@{}{scale}{hdr}", m.device, m.width, m.height, m.hz)
            })
            .collect();
        write!(f, "{} | {key} | {}", self.name, monitors.join(" ; "))?;
        // Системная и пятая части — только если есть что писать: старая строка остаётся как была.
        let system = self.system.to_string();
        if !system.is_empty() || !self.undo {
            write!(f, " | {system}")?;
        }
        if !self.undo {
            f.write_str(" | undo=0")?;
        }
        Ok(())
    }
}

impl Settings {
    /// Программа в списке «не выезжать над ней».
    pub fn excluded(&self, exe: &str) -> bool {
        self.exclude.iter().any(|e| e.eq_ignore_ascii_case(exe))
    }

    pub fn favorites_of(&self, device: &str) -> Vec<(u32, u32)> {
        self.favorites.iter().filter(|f| f.device.eq_ignore_ascii_case(device)).map(|f| (f.width, f.height)).collect()
    }

    pub fn is_favorite(&self, device: &str, width: u32, height: u32) -> bool {
        self.favorites.iter().any(|f| f.matches(device, width, height))
    }

    /// Поставить или снять звёздочку. Ответ — стало ли разрешение избранным.
    pub fn toggle_favorite(&mut self, device: &str, width: u32, height: u32) -> Result<bool, FavoritesFull> {
        if let Some(i) = self.favorites.iter().position(|f| f.matches(device, width, height)) {
            self.favorites.remove(i);
            return Ok(false);
        }
        if self.favorites_of(device).len() >= MAX_FAVORITES {
            return Err(FavoritesFull);
        }
        self.favorites.push(Res::new(device, width, height));
        Ok(true)
    }

    pub fn remembered_hz(&self, device: &str, width: u32, height: u32) -> Option<u32> {
        self.rates.iter().find(|(r, _)| r.matches(device, width, height)).map(|(_, hz)| *hz)
    }

    /// Запомнить частоту разрешения. Ответ — изменилось ли что-нибудь (надо ли сохранять).
    pub fn remember_hz(&mut self, device: &str, width: u32, height: u32, hz: u32) -> bool {
        match self.rates.iter_mut().find(|(r, _)| r.matches(device, width, height)) {
            Some((_, old)) if *old == hz => false,
            Some((_, old)) => {
                *old = hz;
                true
            }
            None => {
                self.rates.push((Res::new(device, width, height), hz));
                true
            }
        }
    }

    // ─── Полки ──────────────────────────────────────────────────────────────

    /// Номер полки, на которой стоит карточка; убранная — `None`.
    pub fn shelf_of(&self, key: &str) -> Option<usize> {
        self.shelves.iter().position(|s| s.cards.iter().any(|c| c == key))
    }

    /// Карточка убрана пользователем (она не на полке, но работает в фоне).
    pub fn is_hidden(&self, key: &str) -> bool {
        self.hidden.iter().any(|h| h == key)
    }

    /// Поставить карточку на полку `shelf` на место `at` (`None` — в конец) — откуда бы она ни была: с другой полки, с
    /// этой же или из убранных. `at` считается в списке полки без самой карточки. Незнакомый ключ и нет такой
    /// полки — ничего не делает. Ответ — сделано ли.
    pub fn place_card(&mut self, key: &str, shelf: usize, at: Option<usize>) -> bool {
        if shelf >= self.shelves.len() || !known_card(key) {
            return false;
        }
        for s in &mut self.shelves {
            s.cards.retain(|c| c != key);
        }
        self.hidden.retain(|h| h != key);
        let cards = &mut self.shelves[shelf].cards;
        let at = at.unwrap_or(cards.len()).min(cards.len());
        cards.insert(at, key.to_string());
        true
    }

    /// Убрать карточку с полки: она уходит в `hidden`.
    pub fn hide_card(&mut self, key: &str) -> bool {
        if !known_card(key) {
            return false;
        }
        for s in &mut self.shelves {
            s.cards.retain(|c| c != key);
        }
        if !self.is_hidden(key) {
            self.hidden.push(key.to_string());
        }
        true
    }

    /// Новая полка в конце, «Полка N» с первым свободным N. Ответ — её номер; больше [`MAX_SHELVES`] не бывает.
    pub fn add_shelf(&mut self, lang: Lang) -> Option<usize> {
        if self.shelves.len() >= MAX_SHELVES {
            return None;
        }
        let name =
            (1usize..).map(|n| crate::texts::shelf_n(lang, n)).find(|n| !self.shelves.iter().any(|s| s.name == *n))?;
        self.shelves.push(Shelf { name, cards: Vec::new() });
        Some(self.shelves.len() - 1)
    }

    /// Убрать полку: её карточки уходят в `hidden`. Последнюю полку убрать нельзя.
    pub fn remove_shelf(&mut self, i: usize) -> bool {
        if self.shelves.len() <= 1 || i >= self.shelves.len() {
            return false;
        }
        for c in self.shelves.remove(i).cards {
            if !self.is_hidden(&c) {
                self.hidden.push(c);
            }
        }
        true
    }

    /// Переименовать полку. Пустое имя и то же самое — ничего не делают. Ответ — изменилось ли имя.
    pub fn rename_shelf(&mut self, i: usize, name: &str) -> bool {
        let name = Shelf::clean_name(name);
        match self.shelves.get_mut(i) {
            Some(s) if !name.is_empty() && s.name != name => {
                s.name = name;
                true
            }
            _ => false,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            common: CommonSettings::default(),
            width: 1300.0,
            height: 340.0,
            monitor: String::new(),
            show_delay_ms: 150,
            hide_delay_ms: 450,
            fullscreen_guard: true,
            strip: true,
            exclude: Vec::new(),
            panel_hotkey: None,
            clipboard: true,
            tray: Vec::new(),
            shelves: default_shelves(),
            hidden: Vec::new(),
            github: Vec::new(),
            github_token: None,
            favorites: Vec::new(),
            rates: Vec::new(),
            profiles: Vec::new(),
            accent: crate::look::LIME.name.to_string(),
            media_strip: false,
            plaques: Plaques::default(),
            kamerton: String::new(),
            pinned: Vec::new(),
            clocks: Vec::new(),
        }
    }
}

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("Antresol"))
}

pub fn settings_path() -> Option<PathBuf> {
    dir().map(|d| d.join("config.txt"))
}

pub fn buttons_path() -> Option<PathBuf> {
    dir().map(|d| d.join("buttons.txt"))
}

pub fn load() -> Settings {
    let text = settings_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    parse(&text)
}

/// Список через запятую: без пустых и повторов.
fn list(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in value.split(',').map(str::trim).filter(|v| !v.is_empty()) {
        if !out.iter().any(|o| o == v) {
            out.push(v.to_string());
        }
    }
    out
}

/// Раскладка из файла. `lines` — полки из `shelf=` (имя и ключи, как написаны), `legacy` — порядок из старого
/// `modules=`. Незнакомые ключи и повторы отбрасываются (первая встреча побеждает); убранная карточка (`hidden`) на
/// полке не стоит; знакомая карточка, которой нет ни на полке, ни среди убранных, встаёт в конец своей полки по
/// умолчанию, а нет такой полки — в конец последней. Полок нет совсем (старый файл) — полки по умолчанию, порядок
/// внутри них — по `legacy`: кто был раньше в ленте, тот раньше на полке.
fn build_shelves(lines: Vec<(String, Vec<String>)>, hidden: &[String], legacy: &[String]) -> Vec<Shelf> {
    let mut shelves: Vec<Shelf> = Vec::new();
    for (name, cards) in lines {
        let name = Shelf::clean_name(&name);
        if name.is_empty() || shelves.len() >= MAX_SHELVES {
            continue;
        }
        let mut kept: Vec<String> = Vec::new();
        for c in cards {
            let placed = shelves.iter().any(|s| s.cards.contains(&c));
            if known_card(&c) && !hidden.contains(&c) && !placed && !kept.contains(&c) {
                kept.push(c);
            }
        }
        shelves.push(Shelf { name, cards: kept });
    }
    if shelves.is_empty() {
        shelves = default_shelves();
        for sh in &mut shelves {
            sh.cards.retain(|c| !hidden.contains(c));
            // Сортировка стабильная: кого в `modules=` нет, тот остаётся после названных, в прежнем порядке.
            sh.cards.sort_by_key(|c| legacy.iter().position(|l| l == c).unwrap_or(usize::MAX));
        }
    }
    for (home, cards) in crate::modules::DEFAULT_SHELVES {
        for key in cards {
            if hidden.iter().any(|h| h == key) || shelves.iter().any(|s| s.cards.iter().any(|c| c == key)) {
                continue;
            }
            let at = shelves.iter().position(|s| s.name == home).unwrap_or(shelves.len() - 1);
            shelves[at].cards.push((*key).to_string());
        }
    }
    shelves
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    // Полки и порядок старого `modules=` собираем на выходе: от порядка строк в файле раскладка не зависит.
    let mut shelf_lines: Vec<(String, Vec<String>)> = Vec::new();
    let mut legacy: Vec<String> = Vec::new();
    // Старый файл: убрана ли была карточка «Заготовки» (`snippets`) — теперь это закреплённое в «Буфере обмена».
    let mut snippets_hidden = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "lang" => s.common.language = if value == "en" { Lang::En } else { Lang::Ru },
            "theme" => {
                s.common.theme = match value {
                    "light" => ThemeChoice::Light,
                    "dark" => ThemeChoice::Dark,
                    _ => ThemeChoice::System,
                }
            }
            "width" => s.width = value.parse().unwrap_or(s.width),
            "height" => s.height = value.parse().unwrap_or(s.height),
            "monitor" => s.monitor = value.to_string(),
            "show_delay_ms" => s.show_delay_ms = value.parse().unwrap_or(s.show_delay_ms),
            "hide_delay_ms" => s.hide_delay_ms = value.parse().unwrap_or(s.hide_delay_ms),
            "fullscreen_guard" => s.fullscreen_guard = value != "false",
            "strip" => s.strip = value != "false",
            "exclude" => {
                s.exclude = value.split(',').map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()).collect();
                s.exclude.dedup();
            }
            "panel_hotkey" => s.panel_hotkey = Hotkey::parse(value),
            "clipboard" => s.clipboard = value != "false",
            "accent" if crate::look::valid(value) => s.accent = value.to_lowercase(),
            "media_strip" => s.media_strip = value == "true",
            "plaques_off" => s.plaques = Plaques::parse(value),
            "kamerton" => s.kamerton = value.to_string(),
            "snippet" if !value.is_empty() && s.pinned.len() < MAX_PINNED => s.pinned.push(unescape(value)),
            "clock" if s.clocks.len() < MAX_CLOCKS => {
                if let Some((name, zone)) = value.split_once('|') {
                    let (name, zone) = (name.trim(), zone.trim());
                    if !name.is_empty() && !zone.is_empty() {
                        s.clocks.push(Clock { name: name.replace('|', " "), zone: zone.to_string() });
                    }
                }
            }
            // Полка: `имя|ключ,ключ`; имя `@av`, `@sys`, `@hand` — готовое. Пустой список — пустая полка.
            "shelf" => {
                let (name, cards) = value.split_once('|').unwrap_or((value, ""));
                shelf_lines.push((name.to_string(), list(cards)));
            }
            // Порядок из версий до 1.9: нужен только для перехода на полки, обратно не пишется.
            "modules" => legacy = list(value),
            "hidden" => {
                let raw = list(value);
                snippets_hidden = raw.iter().any(|k| k == "snippets");
                s.hidden = raw.into_iter().filter(|k| known_card(k)).collect();
            }
            // Только похожее на «владелец/имя»: кривое из файла не идёт в адрес запроса.
            "github" => {
                s.github = list(value).into_iter().filter(|r| crate::modules::github::valid_repo(r)).collect();
            }
            "github_token" => s.github_token = Some(value.to_string()).filter(|t| !t.is_empty()),
            "tray" if !value.is_empty() && s.tray.len() < crate::modules::tray::KEEP => s.tray.push(value.to_string()),
            "favorite" => {
                let words: Vec<&str> = value.split_whitespace().collect();
                if let Some(r) = Res::parse(&words)
                    && s.favorites_of(&r.device).len() < MAX_FAVORITES
                    && !s.is_favorite(&r.device, r.width, r.height)
                {
                    s.favorites.push(r);
                }
            }
            "rate" => {
                let words: Vec<&str> = value.split_whitespace().collect();
                if let (Some(r), Some(Ok(hz))) = (Res::parse(&words), words.get(2).map(|h| h.parse())) {
                    s.remember_hz(&r.device, r.width, r.height, hz);
                }
            }
            "profile" => {
                if let Some(p) = Profile::parse(value)
                    && !s.profiles.iter().any(|q| q.name == p.name)
                {
                    s.profiles.push(p);
                }
            }
            _ => {}
        }
    }
    // Старый файл (полок нет): «Заготовки» были на виду, а «Буфер обмена» убран — теперь закреплённое лежит в нём, и
    // убранным он остался бы невидимым. Возвращаем его: на ту полку, где он стоит по умолчанию.
    if shelf_lines.is_empty() && !snippets_hidden {
        s.hidden.retain(|k| k != "clipboard");
    }
    s.shelves = build_shelves(shelf_lines, &s.hidden, &legacy);
    s.width = s.width.max(MIN_WIDTH);
    s.height = s.height.clamp(MIN_HEIGHT, MAX_HEIGHT);
    s
}

pub fn format(s: &Settings) -> String {
    let theme = match s.common.theme {
        ThemeChoice::Light => "light",
        ThemeChoice::Dark => "dark",
        ThemeChoice::System => "system",
    };
    let lang = if s.common.language == Lang::En { "en" } else { "ru" };
    let mut out = format!(
        "lang={lang}\ntheme={theme}\nwidth={}\nheight={}\nmonitor={}\nshow_delay_ms={}\nhide_delay_ms={}\nfullscreen_guard={}\n",
        s.width.round(),
        s.height.round(),
        s.monitor,
        s.show_delay_ms,
        s.hide_delay_ms,
        s.fullscreen_guard
    );
    out.push_str(&format!("strip={}\nexclude={}\nclipboard={}\n", s.strip, s.exclude.join(","), s.clipboard));
    out.push_str(&format!("accent={}\nmedia_strip={}\nplaques_off={}\n", s.accent, s.media_strip, s.plaques));
    if !s.kamerton.is_empty() {
        out.push_str(&format!("kamerton={}\n", s.kamerton));
    }
    for sn in &s.pinned {
        out.push_str(&format!("snippet={}\n", escape(sn)));
    }
    for c in &s.clocks {
        out.push_str(&format!("clock={} | {}\n", c.name.replace(['|', '\n'], " "), c.zone));
    }
    for sh in &s.shelves {
        out.push_str(&format!("shelf={}|{}\n", Shelf::clean_name(&sh.name), sh.cards.join(",")));
    }
    out.push_str(&format!("hidden={}\ngithub={}\n", s.hidden.join(","), s.github.join(",")));
    if let Some(t) = &s.github_token {
        out.push_str(&format!("github_token={t}\n"));
    }
    for t in &s.tray {
        out.push_str(&format!("tray={t}\n"));
    }
    if let Some(h) = &s.panel_hotkey {
        out.push_str(&format!("panel_hotkey={h}\n"));
    }
    for f in &s.favorites {
        out.push_str(&format!("favorite={f}\n"));
    }
    for (r, hz) in &s.rates {
        out.push_str(&format!("rate={r} {hz}\n"));
    }
    // Сцена, в которой нечего ставить (новая, пока в неё ничего не отметили), в файл не идёт: прочесть её нельзя.
    for p in s.profiles.iter().filter(|p| !p.is_empty()) {
        out.push_str(&format!("profile={p}\n"));
    }
    out
}

pub fn save(s: &Settings) {
    // Тесты гоняют панель целиком (см. `app::tests`), а демо-режим — выдуманный мир: файл настоящих настроек они трогать
    // не должны (демо с настоящей папкой настроек иначе записал бы в неё выдуманные частоты).
    if crate::sys::isolated() {
        return;
    }
    let Some(path) = settings_path() else { return };
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    crate::sys::real_write("config.txt");
    if let Err(e) = std::fs::write(&path, format(s)) {
        eprintln!("cannot save settings: {e}");
    }
}

// ─── Свои кнопки ────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct Button {
    pub name: String,
    /// Что открыть: программа, папка, файл или адрес; `%ПЕРЕМЕННЫЕ%` уже раскрыты.
    pub target: String,
    pub args: String,
    pub icon: Icon,
}

pub const BUTTONS_TEMPLATE: &str = "\
# Antresol: свои кнопки / custom buttons
# Строка — кнопка / one line per button:
#   название | что открыть | аргументы | значок
#   name     | what to open | arguments | icon
# Открыть можно программу, папку, файл или адрес. Аргументы и значок — по желанию.
# Значки / icons: folder file terminal code window rocket gear hammer image film cube gamepad
#                 server search clock target layers package broadcast
# %ПЕРЕМЕННЫЕ% раскрываются / %VARIABLES% are expanded.

Загрузки | %USERPROFILE%\\Downloads
Блокнот | notepad.exe
Терминал | powershell.exe | -NoExit | terminal
";

/// Значок по имени из файла кнопок.
fn icon_named(name: &str) -> Option<Icon> {
    Some(match name.to_ascii_lowercase().as_str() {
        "folder" => Icon::Folder,
        "file" => Icon::File,
        "terminal" => Icon::Terminal,
        "code" => Icon::Code,
        "window" => Icon::Window,
        "rocket" => Icon::Rocket,
        "gear" => Icon::Gear,
        "hammer" => Icon::Hammer,
        "image" => Icon::Image,
        "film" => Icon::Film,
        "cube" => Icon::Cube,
        "gamepad" => Icon::Gamepad,
        "server" => Icon::Server,
        "search" => Icon::Search,
        "clock" => Icon::Clock,
        "target" => Icon::Target,
        "layers" => Icon::Layers,
        "package" => Icon::Package,
        "broadcast" => Icon::Broadcast,
        _ => return None,
    })
}

/// Значок, если в файле не задан: по тому, что открывается.
fn icon_for(target: &str) -> Icon {
    let t = target.to_ascii_lowercase();
    if t.starts_with("http://") || t.starts_with("https://") {
        Icon::Broadcast
    } else if t.ends_with(".exe") || t.ends_with(".bat") || t.ends_with(".cmd") || t.ends_with(".ps1") {
        Icon::Window
    } else if t.ends_with('\\') || t.ends_with('/') || !t.rsplit(['\\', '/']).next().unwrap_or("").contains('.') {
        Icon::Folder
    } else {
        Icon::File
    }
}

/// Раскрыть `%ИМЯ%` переменными среды; неизвестные остаются как есть.
pub fn expand(s: &str, var: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match var(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn parse_buttons(text: &str, var: impl Fn(&str) -> Option<String>) -> Vec<Button> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        let (Some(name), Some(target)) = (parts.first(), parts.get(1)) else { continue };
        if name.is_empty() || target.is_empty() {
            continue;
        }
        let target = expand(target, &var);
        let args = parts.get(2).map(|a| expand(a, &var)).unwrap_or_default();
        let icon = parts.get(3).and_then(|i| icon_named(i)).unwrap_or_else(|| icon_for(&target));
        out.push(Button { name: name.to_string(), target, args, icon });
    }
    out
}

/// Прочитать кнопки; файла нет — положить образец и прочитать его.
pub fn load_buttons() -> Vec<Button> {
    let Some(path) = buttons_path() else { return Vec::new() };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            // Образец кладём на диск только настоящей программе: демо и тесты файлов не заводят.
            if !crate::sys::isolated() {
                if let Some(d) = path.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                crate::sys::real_write("buttons.txt");
                let _ = std::fs::write(&path, BUTTONS_TEMPLATE);
            }
            BUTTONS_TEMPLATE.to_string()
        }
    };
    parse_buttons(&text, |n| std::env::var(n).ok())
}

/// Время правки файла кнопок — чтобы перечитать его, как только его сохранили.
pub fn buttons_stamp() -> Option<std::time::SystemTime> {
    std::fs::metadata(buttons_path()?).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(n: &str) -> Option<String> {
        (n == "USERPROFILE").then(|| r"C:\Users\me".to_string())
    }

    #[test]
    fn settings_round_trip() {
        let mut s =
            Settings { width: 1000.0, monitor: r"\\.\DISPLAY2".into(), fullscreen_guard: false, ..Default::default() };
        s.common.theme = ThemeChoice::Dark;
        s.common.language = Lang::En;
        assert_eq!(parse(&format(&s)), s);
    }

    /// «Живые плашки»: без строки (и с пустой) включено всё, строка `plaques_off=` перечисляет выключенное; таймер выключить
    /// нельзя. Список выключенного, а не включённого: новый вид событий у старого `config.txt` включён сам.
    #[test]
    fn plaques_are_all_on_by_default_and_the_file_lists_what_is_off() {
        use crate::plaque::Kind;
        let none = Plaques { bluetooth: false, usb: false, github: false, downloads: false };
        assert_eq!(parse("").plaques, Plaques::default());
        assert_eq!(parse("plaques_off=\n").plaques, Plaques::default(), "пустая строка — выключенного нет");
        assert_eq!(
            parse("plaques_off=usb, github\n").plaques,
            Plaques { bluetooth: true, usb: false, github: false, downloads: true }
        );
        assert_eq!(parse("plaques_off=bluetooth,usb,github,downloads\n").plaques, none);
        assert_eq!(
            parse("plaques_off=usb,what,usb\n").plaques,
            Plaques { usb: false, ..Plaques::default() },
            "незнакомое пропускается"
        );
        // Старая строка `plaques=` (список включённого, до выпуска её не было) ничего не значит: всё включено.
        assert_eq!(parse("plaques=usb\n").plaques, Plaques::default());
        let s = Settings {
            plaques: Plaques { bluetooth: false, downloads: false, ..Plaques::default() },
            ..Default::default()
        };
        assert!(format(&s).contains("\nplaques_off=bluetooth,downloads\n"));
        assert_eq!(parse(&format(&s)).plaques, s.plaques);
        assert_eq!(parse(&format(&Settings::default())).plaques, Plaques::default());
        assert!(format(&Settings::default()).contains("\nplaques_off=\n"));
        assert!(none.on(Kind::Timer) && !none.on(Kind::Usb) && Plaques::default().on(Kind::Downloads));
        // `set` включает и выключает вид; таймер не трогает.
        let mut p = Plaques::default();
        p.set(Kind::Github, false);
        p.set(Kind::Timer, false);
        assert!(!p.on(Kind::Github) && p.on(Kind::Timer) && p.on(Kind::Usb));
        p.set(Kind::Github, true);
        assert_eq!(p, Plaques::default());
    }

    #[test]
    fn hotkeys() {
        let h = Hotkey::parse("Ctrl+Alt+1").unwrap();
        assert!(h.ctrl && h.alt && !h.shift && h.key == "1");
        assert_eq!(h.to_string(), "Ctrl+Alt+1");
        assert_eq!(Hotkey::parse("ctrl + shift + F5").unwrap().to_string(), "Ctrl+Shift+F5");
        assert!(Hotkey::parse("A").is_none(), "без Ctrl/Alt/Win мешала бы печатать");
        assert!(Hotkey::parse("Shift+A").is_none());
        assert!(Hotkey::parse("Ctrl+A+B").is_none());
        assert!(Hotkey::parse("").is_none());
    }

    #[test]
    fn profiles_round_trip() {
        let p = Profile {
            id: 0,
            name: "Игры".into(),
            hotkey: Hotkey::parse("Ctrl+Alt+1"),
            monitors: vec![
                ProfileMonitor {
                    device: r"\\.\DISPLAY1".into(),
                    width: 1920,
                    height: 1080,
                    hz: 144,
                    scale: Some(100),
                    hdr: Some(true),
                },
                ProfileMonitor {
                    device: r"\\.\DISPLAY2".into(),
                    width: 2560,
                    height: 1440,
                    hz: 60,
                    scale: None,
                    hdr: None,
                },
                ProfileMonitor {
                    device: r"\\.\DISPLAY3".into(),
                    width: 1280,
                    height: 720,
                    hz: 60,
                    scale: None,
                    hdr: Some(false),
                },
            ],
            system: SceneSystem::default(),
            undo: true,
        };
        let line = p.to_string();
        assert_eq!(
            line,
            r"Игры | Ctrl+Alt+1 | \\.\DISPLAY1 1920x1080@144 100% hdr ; \\.\DISPLAY2 2560x1440@60 ; \\.\DISPLAY3 1280x720@60 sdr"
        );
        let s = Settings {
            profiles: vec![p.clone(), Profile { name: "Работа".into(), hotkey: None, ..p }],
            ..Default::default()
        };
        assert_eq!(parse(&format(&s)), s);
        // Битые строки и повтор имени — пропуск: остаётся одна годная.
        let bad = "profile=X | | \nprofile= | | A 1x1@60\nprofile=Y | | A 1x1\nprofile=Z | | A 1x1@60\nprofile=Z | | B 2x2@60\n";
        let names: Vec<String> = parse(bad).profiles.into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Z".to_string()]);
        assert_eq!(Profile::clean_name(" a|b;c "), "a b c");
    }

    /// Сцена: экран и система в строке `profile=`.
    fn scene() -> Profile {
        let monitor = ProfileMonitor {
            device: r"\\.\DISPLAY1".into(),
            width: 3840,
            height: 2160,
            hz: 60,
            scale: Some(150),
            hdr: Some(true),
        };
        Profile {
            id: 0,
            name: "Кино".into(),
            hotkey: Hotkey::parse("Ctrl+Alt+3"),
            monitors: vec![monitor],
            system: SceneSystem {
                night: Some(false),
                quiet: Some(true),
                awake: Some(true),
                dark: Some(true),
                wifi: Some(false),
                plan: Some("{381B4222-F694-41F0-9685-FF5BB260DF2E}".into()),
                bright: Some(80),
            },
            undo: true,
        }
    }

    #[test]
    fn scene_line_has_screen_system_and_undo() {
        let p = scene();
        // Система в одном порядке; возврат включён — слова `undo` нет.
        let line = p.to_string();
        assert_eq!(
            line,
            r"Кино | Ctrl+Alt+3 | \\.\DISPLAY1 3840x2160@60 150% hdr | night=0; quiet=1; awake=1; dark=1; wifi=0; bright=80; plan={381B4222-F694-41F0-9685-FF5BB260DF2E}"
        );
        assert_eq!(Profile::parse(&line), Some(p.clone()));
        // Возврат выключен — пятая часть `undo=0`.
        let off = Profile { undo: false, ..p.clone() };
        assert!(off.to_string().ends_with(" | undo=0"), "{off}");
        assert_eq!(Profile::parse(&off.to_string()), Some(off.clone()));
        // Вся строка файла туда и обратно, вместе с экраном без системы и системой без экрана.
        let only_screen = Profile { name: "Экран".into(), system: SceneSystem::default(), ..p.clone() };
        let only_system = Profile { name: "Тишина".into(), hotkey: None, monitors: Vec::new(), ..p.clone() };
        let off = Profile { name: "Без возврата".into(), ..off };
        let s = Settings { profiles: vec![p, off, only_screen, only_system], ..Default::default() };
        s.profiles.iter().for_each(|p| assert!(!p.is_empty()));
        let text = format(&s);
        assert_eq!(text.lines().filter(|l| l.starts_with("profile=")).count(), 4);
        assert_eq!(parse(&text), s);
    }

    /// Номер сцены живёт, пока работает программа: переименование его не меняет, новая сцена с тем же именем получает
    /// другой, в файл он не попадает и сравнению сцен не мешает.
    #[test]
    fn a_scene_has_a_number_that_the_file_does_not_know() {
        let (a, b) = (Profile::new("A"), Profile::new("A"));
        assert_ne!(a.id, b.id, "каждая новая сцена со своим номером, хоть имя то же");
        assert_eq!(a, b, "сцены сравниваются по записанному в файле");
        assert_eq!(Profile { name: "B".into(), ..a.clone() }.id, a.id, "переименование номер не меняет");
        let p = scene();
        let again = Profile::parse(&p.to_string()).expect("строка читается");
        assert_eq!(again, p);
        assert_ne!(again.id, p.id, "прочитанная заново — это новый номер");
        assert!(!p.to_string().contains("id="), "в файл номер не пишется: {p}");
        let (x, y) = (parse("profile=X | | D 1x1@60\n").profiles, parse("profile=X | | D 1x1@60\n").profiles);
        assert_ne!(x[0].id, y[0].id);
    }

    /// Строка до 2.1 — три части — читается как сцена, что меняет только экран; при записи остаётся такой же.
    #[test]
    fn old_profile_line_is_a_screen_only_scene() {
        let d = r"\\.\DISPLAY1";
        let old = format!("Игры | Ctrl+Alt+1 | {d} 1920x1080@144 100% hdr");
        let p = Profile::parse(&old).expect("старая строка читается");
        assert!(p.system.is_empty() && p.undo, "системы нет, возврат включён");
        assert!(p.has(SceneRow::Screen) && !p.has(SceneRow::Quiet));
        assert_eq!(p.to_string(), old, "при записи старая строка не меняется");
        // Версии до 2.1 читают только три первые части: у новой строки они те же, что у старой.
        let new = Profile { system: SceneSystem { quiet: Some(true), ..Default::default() }, undo: false, ..p };
        let line = new.to_string();
        let first_three: Vec<&str> = line.split('|').take(3).map(str::trim).collect();
        assert_eq!(first_three, old.split('|').map(str::trim).collect::<Vec<_>>());
    }

    /// Незнакомые ключи пропускаются, кривые значения — тоже, а сцена остаётся.
    #[test]
    fn scene_system_skips_unknown_keys_and_bad_values() {
        let d = r"\\.\DISPLAY1";
        let line = |system: &str| format!("S | | {d} 1x1@60 | {system}");
        let read = |system: &str| Profile::parse(&line(system)).expect("сцена остаётся").system;
        let good = SceneSystem { quiet: Some(true), bright: Some(50), ..Default::default() };
        assert_eq!(read("quiet=1; sound=1; bright=50; vol=3; =; nonsense"), good, "незнакомые ключи");
        let bad = "quiet=1; awake=2; dark=; wifi=yes; night=on; bright=101; bright=-5; bright=x; plan=zzz; bright=50";
        assert_eq!(read(bad), good, "кривые значения");
        assert_eq!(read(" quiet = 1 ;; bright = 50 ; "), good, "пробелы и пустые части");
        // Повтор ключа — побеждает последний годный; негодный повтор прежнего не стирает.
        assert_eq!(read("quiet=1; quiet=0").quiet, Some(false));
        assert_eq!(read("quiet=1; quiet=bad").quiet, Some(true));
        // План — GUID со скобками или без, любым регистром; пишется всегда одинаково.
        let plan = |v: &str| read(&format!("quiet=1; plan={v}")).plan;
        let guid = Some("{381B4222-F694-41F0-9685-FF5BB260DF2E}".to_string());
        assert_eq!(plan("{381b4222-f694-41f0-9685-ff5bb260df2e}"), guid);
        assert_eq!(plan("381B4222-F694-41F0-9685-FF5BB260DF2E"), guid);
        assert_eq!(plan("{381B4222-F694-41F0-9685}"), None);
        assert_eq!(plan("{381B4222-F694-41F0-9685-FF5BB260DF2G}"), None);
        // Ни одной годной части системы, а экран есть: сцена остаётся экранной.
        let screen_only = Profile::parse(&line("nonsense=1")).expect("экран есть");
        assert!(screen_only.system.is_empty());
        // `undo` читается и в четвёртой части (руками пишут по-всякому); не 0 и не 1 — как не писали.
        assert!(!Profile::parse(&line("undo=0")).unwrap().undo);
        assert!(!Profile::parse(&format!("{} | undo=0", line("quiet=1"))).unwrap().undo);
        assert!(Profile::parse(&format!("{} | undo=1", line("quiet=1"))).unwrap().undo);
        assert!(Profile::parse(&format!("{} | undo=maybe", line("quiet=1"))).unwrap().undo);
    }

    /// Сцена без экрана (третья часть пуста) читается и пишется; без экрана и без системы — нет.
    #[test]
    fn scene_without_screen_and_scene_without_anything() {
        let only_system = Profile::parse("Тишина | Ctrl+Alt+2 |  | quiet=1; awake=1").expect("без экрана");
        assert!(only_system.monitors.is_empty());
        assert_eq!(only_system.system.quiet, Some(true));
        assert_eq!(only_system.to_string(), "Тишина | Ctrl+Alt+2 |  | quiet=1; awake=1");
        // Возврат выключен, системы нет: четвёртая часть пуста, пятая есть.
        let screen_off = Profile::parse("A | | X 1x1@60 | | undo=0").unwrap();
        assert!(!screen_off.undo && screen_off.system.is_empty());
        assert_eq!(screen_off.to_string(), "A |  | X 1x1@60 |  | undo=0");
        // Нечего ставить: ни экрана, ни годной системы.
        for bad in ["A | | ", "A | |  | ", "A | |  | nonsense=1", "A | |  | quiet=2; bright=300", "A | |  |  | undo=0"]
        {
            assert_eq!(Profile::parse(bad), None, "{bad:?}");
        }
        // В файл такая сцена тоже не идёт — иначе при следующем чтении она пропала бы вместе с именем.
        let mut s = Settings::default();
        s.profiles.push(Profile::new("Пока пусто"));
        s.profiles.push(Profile::parse("B | | X 1x1@60").unwrap());
        let text = format(&s);
        assert!(!text.contains("Пока пусто") && text.contains("profile=B |  | X 1x1@60"), "{text}");
        assert_eq!(parse(&text).profiles.len(), 1);
    }

    /// Строки сцены: отметить, снять, проверить; `is_empty` — когда не отмечено ничего.
    #[test]
    fn scene_rows_are_checked_and_cleared() {
        let mut p = scene();
        for row in SceneRow::ALL {
            assert!(p.has(row), "{row:?} отмечена");
        }
        assert_eq!(SceneRow::ALL.len(), 8);
        for row in SceneRow::ALL {
            p.clear(row);
            assert!(!p.has(row), "{row:?} снята");
        }
        assert!(p.is_empty() && p.system.is_empty() && p.monitors.is_empty());
        assert!(Profile::new("N").is_empty() && Profile::new("N").undo && Profile::new("N").hotkey.is_none());
    }

    #[test]
    fn guid_is_written_one_way() {
        let canon = Some("{381B4222-F694-41F0-9685-FF5BB260DF2E}".to_string());
        assert_eq!(normalize_guid("381b4222-f694-41f0-9685-ff5bb260df2e"), canon);
        assert_eq!(normalize_guid(" {381B4222-F694-41F0-9685-FF5BB260DF2E} "), canon);
        for bad in ["", "{}", "{381B4222F69441F09685FF5BB260DF2E}", "{381B4222-F694-41F0-9685-FF5BB260DF2E", "x"] {
            assert_eq!(normalize_guid(bad), None, "{bad:?}");
        }
    }

    /// `hdr` / `sdr` после режима монитора в `profile=`: порядок слов не важен, старые строки читаются как раньше.
    #[test]
    fn profile_hdr_words() {
        let d = r"\\.\DISPLAY1";
        let one = |tail: &str| {
            let line = format!("profile=A | | {d} 1920x1080@60{tail}\n");
            parse(&line).profiles.into_iter().next().map(|p| p.monitors)
        };
        let pm =
            |scale, hdr| Some(vec![ProfileMonitor { device: d.into(), width: 1920, height: 1080, hz: 60, scale, hdr }]);
        // Строки до 2.0 — без слова: HDR не задан, его не трогают.
        assert_eq!(one(""), pm(None, None));
        assert_eq!(one(" 100%"), pm(Some(100), None));
        // `hdr` — включить, `sdr` — выключить; до масштаба, после него или без него.
        assert_eq!(one(" hdr"), pm(None, Some(true)));
        assert_eq!(one(" sdr"), pm(None, Some(false)));
        assert_eq!(one(" 125% hdr"), pm(Some(125), Some(true)));
        assert_eq!(one(" sdr 125%"), pm(Some(125), Some(false)), "порядок слов не важен");
        assert_eq!(one(" 150 HDR"), pm(Some(150), Some(true)), "знак % и регистр не важны");
        // Негодное — строка пропущена: незнакомое слово, повтор, два противоположных слова.
        for bad in [" hdr sdr", " hdr hdr", " 100% 125%", " foo", " 100% hdr foo", " hdr%"] {
            assert_eq!(one(bad), None, "{bad:?}");
        }
        // Старая строка при записи не меняется, новая читается обратно как есть.
        let d2 = r"\\.\DISPLAY2";
        let old = format!("A |  | {d} 1920x1080@60 100% ; {d2} 1280x720@60");
        assert_eq!(Profile::parse(&old).unwrap().to_string(), old);
        let new = format!("A |  | {d} 1920x1080@60 100% hdr ; {d2} 1280x720@60 sdr");
        assert_eq!(Profile::parse(&new).unwrap().to_string(), new);
    }

    #[test]
    fn favorites_and_rates_round_trip() {
        let d1 = r"\\.\DISPLAY1";
        let mut s = Settings::default();
        assert_eq!(s.toggle_favorite(d1, 2560, 1440), Ok(true));
        assert_eq!(s.toggle_favorite(d1, 1920, 1080), Ok(true));
        assert!(s.remember_hz(d1, 2560, 1440, 144));
        assert!(!s.remember_hz(d1, 2560, 1440, 144), "та же частота — сохранять нечего");
        assert!(s.remember_hz(d1, 1920, 1080, 60));
        let back = parse(&format(&s));
        assert_eq!(back, s);
        assert_eq!(back.favorites_of(d1), vec![(2560, 1440), (1920, 1080)]);
        assert_eq!(back.remembered_hz(d1, 2560, 1440), Some(144));
        assert_eq!(back.remembered_hz(r"\\.\DISPLAY2", 2560, 1440), None);
    }

    #[test]
    fn favorites_limit_per_monitor() {
        let (d1, d2) = (r"\\.\DISPLAY1", r"\\.\DISPLAY2");
        let mut s = Settings::default();
        for (w, h) in [(2560, 1440), (1920, 1080), (1280, 720)] {
            assert_eq!(s.toggle_favorite(d1, w, h), Ok(true));
        }
        assert_eq!(s.toggle_favorite(d1, 800, 600), Err(FavoritesFull));
        // У другого монитора — своё избранное.
        assert_eq!(s.toggle_favorite(d2, 800, 600), Ok(true));
        // Сняли звёздочку — место освободилось.
        assert_eq!(s.toggle_favorite(d1, 1920, 1080), Ok(false));
        assert_eq!(s.toggle_favorite(d1, 800, 600), Ok(true));
        // Лишние строки, дописанные в файл руками, — не больше трёх и без повторов.
        let text = "favorite=A 1x1\nfavorite=A 2x2\nfavorite=A 3x3\nfavorite=A 4x4\nfavorite=A 1x1\n";
        assert_eq!(parse(text).favorites_of("A").len(), 3);
    }

    #[test]
    fn panel_options_round_trip() {
        let s = Settings {
            strip: false,
            exclude: vec!["game.exe".into(), "obs64.exe".into()],
            panel_hotkey: Hotkey::parse("Ctrl+Alt+Q"),
            monitor: ALL_MONITORS.into(),
            clipboard: false,
            tray: vec![r"C:\Demo\a.txt".into(), r"D:\b c.pdf".into()],
            accent: "#123456".into(),
            media_strip: true,
            kamerton: r"C:\Demo\kamerton.exe".into(),
            pinned: vec!["Привет!\nСтрока 2".into(), r"C:\path\n".into(), "с пробелами внутри".into()],
            clocks: vec![Clock { name: "Токио".into(), zone: "Tokyo Standard Time".into() }],
            ..Default::default()
        };
        let back = parse(&format(&s));
        assert_eq!(back, s);
        assert!(back.excluded("GAME.EXE") && !back.excluded("chrome.exe"));
        assert!(parse("exclude= A.exe , ,b.exe\n").excluded("a.exe"));
        assert!(Settings::default().strip, "полоска по умолчанию есть");
        assert_eq!(Settings::default().accent, "lime");
        assert_eq!(parse("accent=red\n").accent, "lime", "незнакомый цвет — по умолчанию");
        assert_eq!(parse("accent=SKY\n").accent, "sky");
    }

    #[test]
    fn github_and_token() {
        let g = parse("github=a/b, c/d\ngithub_token=\n");
        assert_eq!(g.github, ["a/b", "c/d"]);
        assert_eq!(g.github_token, None, "пустой токен — нет токена");
    }

    #[test]
    fn pinned_limit() {
        let text: String = (0..40).map(|i| format!("snippet=text {i}\n")).collect();
        assert_eq!(parse(&text).pinned.len(), MAX_PINNED);
    }

    // ─── Полки ──────────────────────────────────────────────────────────────

    /// Имена полок по порядку.
    fn names(s: &Settings) -> Vec<&str> {
        s.shelves.iter().map(|sh| sh.name.as_str()).collect()
    }

    /// Каждая знакомая карточка стоит ровно на одной полке или убрана; лишнего нет.
    fn assert_whole(s: &Settings) {
        let mut all: Vec<String> = s.shelves.iter().flat_map(|sh| sh.cards.clone()).collect();
        all.extend(s.hidden.clone());
        all.sort();
        let mut want: Vec<String> =
            crate::modules::DEFAULT_SHELVES.iter().flat_map(|(_, cards)| cards.iter().map(|c| c.to_string())).collect();
        want.sort();
        assert_eq!(all, want, "карточки потерялись или задвоились");
        assert!(!s.shelves.is_empty());
    }

    #[test]
    fn default_shelves_round_trip() {
        let s = Settings::default();
        assert_eq!(names(&s), ["@av", "@sys", "@hand"]);
        let text = format(&s);
        assert!(text.contains("shelf=@av|display,brightness,sound,media\n"));
        assert!(text.contains("shelf=@sys|quick,resources,bluetooth,github,clocks\n"));
        assert!(text.contains("shelf=@hand|clipboard,tray,tools,timers,buttons\n"));
        assert!(!text.contains("modules="), "старый порядок больше не пишется");
        assert_eq!(parse(&text), s);
        assert_eq!(parse("").shelves, default_shelves(), "пустой файл — полки по умолчанию");
        assert_whole(&s);
    }

    #[test]
    fn custom_shelves_round_trip() {
        let mut s = Settings::default();
        assert!(s.place_card("media", 1, None));
        assert!(s.hide_card("tools"));
        assert_eq!(s.add_shelf(Lang::Ru), Some(3));
        assert!(s.rename_shelf(3, "Дом|Работа"));
        assert_eq!(s.shelves[3], Shelf { name: "Дом Работа".into(), cards: Vec::new() });
        let text = format(&s);
        assert!(text.contains("shelf=Дом Работа|\n"), "пустая полка остаётся");
        assert!(text.contains("hidden=tools\n"));
        assert_eq!(parse(&text), s);
        assert_whole(&s);
    }

    #[test]
    fn old_file_moves_to_shelves() {
        // Сценарий из спецификации: «Медиа» убрана, «Кнопки» стояли раньше «Буфера обмена».
        let s = parse("modules=buttons,clipboard,timers,tray\nhidden=media,snippets\n");
        assert_eq!(names(&s), ["@av", "@sys", "@hand"]);
        assert_eq!(s.shelves[0].cards, ["display", "brightness", "sound"]);
        assert_eq!(s.shelves[2].cards, ["buttons", "clipboard", "timers", "tray", "tools"]);
        assert_eq!(s.hidden, ["media"], "«Заготовки» из старого файла пропущены");
        assert_whole(&s);
        let text = format(&s);
        assert!(!text.contains("modules=") && !text.contains("snippets"));
        assert_eq!(parse(&text), s, "после первого сохранения файл уже новый");
        // Полный порядок 1.7: «Медиа» стояла раньше «Звука», «Заготовки» между «Буфером» и «Лотком».
        let full = "modules=display,quick,brightness,media,sound,resources,clipboard,snippets,tray,tools,bluetooth,timers,clocks,github,buttons\n";
        // Файл 1.7, где порядок никто не трогал: `modules=` и `hidden=` пустые — полки по умолчанию.
        assert_eq!(parse("modules=\nhidden=\n").shelves, default_shelves());
        let s = parse(full);
        assert_eq!(s.shelves[0].cards, ["display", "brightness", "media", "sound"]);
        assert_eq!(s.shelves[1].cards, ["quick", "resources", "bluetooth", "clocks", "github"]);
        assert_eq!(s.shelves[2].cards, ["clipboard", "tray", "tools", "timers", "buttons"]);
        assert!(s.hidden.is_empty());
    }

    #[test]
    fn old_hidden_clipboard_with_visible_snippets_comes_back() {
        // До 1.9 «Заготовки» — отдельная карточка. «Буфер обмена» убран, а «Заготовки» на виду: после перехода
        // закреплённое лежит в «Буфере обмена», и он возвращается домой.
        let s = parse("snippet=Спасибо\nhidden=clipboard\n");
        assert!(s.hidden.is_empty());
        assert_eq!(s.shelf_of("clipboard"), Some(2));
        assert_eq!(s.pinned, ["Спасибо"]);
        // Убраны обе — так и остаётся.
        let s = parse("hidden=clipboard,snippets\n");
        assert_eq!(s.hidden, ["clipboard"]);
        assert_eq!(s.shelf_of("clipboard"), None);
        // Файл 1.9 с полками: убранный «Буфер обмена» — выбор владельца, его не трогаем.
        let s = parse("shelf=@hand|tray\nhidden=clipboard\n");
        assert_eq!(s.hidden, ["clipboard"]);
        assert_eq!(s.shelf_of("clipboard"), None);
        // Убрана не она — ничего не меняется.
        assert_eq!(parse("hidden=media\n").hidden, ["media"]);
    }

    #[test]
    fn shelf_lines_are_cleaned() {
        let text =
            "shelf=A|display,display,nope,media\nshelf=B|media,quick, bluetooth\nshelf=|tray\nhidden=github,nope\n";
        let s = parse(text);
        assert_eq!(names(&s), ["A", "B"], "полка без имени пропущена");
        assert_eq!(s.shelves[0].cards, ["display", "media"], "повтор и незнакомый ключ отброшены");
        assert_eq!(s.hidden, ["github"], "незнакомый ключ среди убранных отброшен");
        // Не названные нигде карточки встают в конец последней полки (родных `@…` здесь нет), в порядке программы.
        assert_eq!(
            s.shelves[1].cards,
            [
                "quick",
                "bluetooth",
                "brightness",
                "sound",
                "resources",
                "clocks",
                "clipboard",
                "tray",
                "tools",
                "timers",
                "buttons"
            ]
        );
        assert_whole(&s);
        // Убранная и поставленная на полку одновременно — убрана.
        let s = parse("shelf=A|display,media\nhidden=media\n");
        assert!(s.is_hidden("media") && s.shelf_of("media").is_none());
        assert_whole(&s);
        // Полок больше девяти не бывает.
        let many: String = (0..12).map(|i| format!("shelf=P{i}|\n")).collect();
        assert_eq!(parse(&many).shelves.len(), MAX_SHELVES);
    }

    #[test]
    fn new_card_goes_to_its_shelf() {
        // «Часы», «Кнопки» и «Медиа» в этом файле нет (новые для него): «Часы» и «Кнопки» — домой, а у «Медиа» дома
        // нет (первая полка названа по-своему) — в конец последней.
        let text = "shelf=Моя|display,brightness,sound\nshelf=@sys|quick,resources,bluetooth,github\n\
                    shelf=@hand|clipboard,tray,tools,timers\n";
        let s = parse(text);
        assert_eq!(s.shelves[0].cards, ["display", "brightness", "sound"]);
        assert_eq!(s.shelves[1].cards, ["quick", "resources", "bluetooth", "github", "clocks"]);
        assert_eq!(s.shelves[2].cards, ["clipboard", "tray", "tools", "timers", "media", "buttons"]);
        assert_whole(&s);
        // Убранная новой не считается и сама не возвращается.
        let s = parse("shelf=@sys|quick\nhidden=clocks\n");
        assert!(s.is_hidden("clocks") && s.shelf_of("clocks").is_none());
    }

    #[test]
    fn move_and_hide_cards() {
        let mut s = Settings::default();
        // Тянут влево, на место «Экрана»: встаёт перед ним.
        assert!(s.place_card("sound", 0, Some(0)));
        assert_eq!(s.shelves[0].cards, ["sound", "display", "brightness", "media"]);
        // Тянут вправо, на «Яркость» (место 2 до перестановки): встаёт после неё.
        assert!(s.place_card("sound", 0, Some(2)));
        assert_eq!(s.shelves[0].cards, ["display", "brightness", "sound", "media"]);
        // На другую полку — в конец.
        assert!(s.place_card("media", 1, None));
        assert_eq!(s.shelves[0].cards, ["display", "brightness", "sound"]);
        assert_eq!(s.shelf_of("media"), Some(1));
        assert_eq!(s.shelves[1].cards.last().map(String::as_str), Some("media"));
        // Убрать и вернуть на другую полку.
        assert!(s.hide_card("brightness") && s.hide_card("brightness"));
        assert_eq!((s.shelf_of("brightness"), s.hidden.as_slice()), (None, ["brightness".to_string()].as_slice()));
        assert!(s.place_card("brightness", 2, None));
        assert!(!s.is_hidden("brightness"));
        assert_eq!(s.shelves[2].cards.last().map(String::as_str), Some("brightness"));
        // Незнакомое и несуществующая полка — ничего.
        let before = s.clone();
        assert!(!s.place_card("nope", 0, None) && !s.place_card("tray", 9, None) && !s.hide_card("nope"));
        assert_eq!(s, before);
        assert_whole(&s);
        assert_eq!(parse(&format(&s)), s);
    }

    #[test]
    fn add_rename_remove_shelves() {
        let mut s = Settings::default();
        assert_eq!(s.add_shelf(Lang::Ru), Some(3));
        assert_eq!(s.add_shelf(Lang::Ru), Some(4));
        assert_eq!((s.shelves[3].name.as_str(), s.shelves[4].name.as_str()), ("Полка 1", "Полка 2"));
        // «Полка 1» освободилась — следующая берёт её номер.
        assert!(s.rename_shelf(3, "Работа"));
        assert_eq!(s.add_shelf(Lang::En), Some(5));
        assert_eq!(s.shelves[5].name, "Shelf 1");
        assert_eq!(s.add_shelf(Lang::Ru), Some(6));
        assert_eq!(s.shelves[6].name, "Полка 1");
        // Пустое имя и то же самое — ничего; больше 24 знаков — обрезается.
        assert!(!s.rename_shelf(3, "  ") && !s.rename_shelf(3, "Работа") && !s.rename_shelf(9, "x"));
        assert!(s.rename_shelf(4, &"я".repeat(40)));
        assert_eq!(s.shelves[4].name.chars().count(), MAX_SHELF_NAME);
        // Убрали полку — её карточки в убранных, остальные полки целы.
        assert!(s.place_card("tray", 3, None) && s.place_card("tools", 3, None));
        assert!(s.remove_shelf(3));
        assert_eq!(s.hidden, ["tray", "tools"]);
        assert_eq!(s.shelves.len(), 6);
        assert_whole(&s);
        // Больше девяти не заводится.
        while s.add_shelf(Lang::En).is_some() {}
        assert_eq!(s.shelves.len(), MAX_SHELVES);
        assert_eq!(s.add_shelf(Lang::En), None);
        // Последнюю полку убрать нельзя.
        let mut one = parse("shelf=Одна|\n");
        assert_eq!(one.shelves.len(), 1);
        assert!(!one.remove_shelf(0));
        assert_whole(&one);
        assert_eq!(Shelf::clean_name(" a|b\tc "), "a b c");
        assert_eq!(Shelf::clean_name(" | "), "");
    }

    #[test]
    fn settings_clamped() {
        let s = parse("width=10\nheight=9999\n");
        assert_eq!((s.width, s.height), (MIN_WIDTH, MAX_HEIGHT));
    }

    #[test]
    fn expand_vars() {
        assert_eq!(expand(r"%USERPROFILE%\Downloads", env), r"C:\Users\me\Downloads");
        assert_eq!(expand("100% %NOPE% x", env), "100% %NOPE% x");
    }

    #[test]
    fn template_buttons() {
        let b = parse_buttons(BUTTONS_TEMPLATE, env);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].target, r"C:\Users\me\Downloads");
        assert_eq!(b[0].icon, Icon::Folder);
        assert_eq!(b[1].icon, Icon::Window);
        assert_eq!((b[2].args.as_str(), b[2].icon), ("-NoExit", Icon::Terminal));
    }

    #[test]
    fn bad_lines_skipped() {
        let b = parse_buttons("only name\n | x\nSite | https://example.com\n", env);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].icon, Icon::Broadcast);
    }
}
