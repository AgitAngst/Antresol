// Настройки и свои кнопки. Оба файла — в `%APPDATA%\Antresol`, обычный текст, правятся руками:
// `config.txt` — строки `ключ=значение`, `buttons.txt` — по кнопке на строку.

use std::path::PathBuf;

use anvil_ui::{CommonSettings, Icon, Lang, ThemeChoice};

pub const MIN_WIDTH: f32 = 560.0;
pub const MIN_HEIGHT: f32 = 150.0;
pub const MAX_HEIGHT: f32 = 520.0;
/// Сколько любимых разрешений у одного монитора.
pub const MAX_FAVORITES: usize = 3;

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

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    /// Тема и язык — как во всех программах семьи.
    pub common: CommonSettings,
    /// Размер панели в точках интерфейса.
    pub width: f32,
    pub height: f32,
    /// Монитор, у верхнего края которого живёт панель: `\\.\DISPLAYn`; пусто — основной.
    pub monitor: String,
    /// Сколько держать мышь у края, прежде чем панель выедет.
    pub show_delay_ms: u32,
    /// Сколько панель ждёт, когда мышь ушла, прежде чем спрятаться.
    pub hide_delay_ms: u32,
    /// Не выезжать, когда на мониторе игра или видео во весь экран.
    pub fullscreen_guard: bool,
    /// Любимые разрешения, до трёх на монитор, в порядке добавления.
    pub favorites: Vec<Res>,
    /// Какая частота была у разрешения в последний раз — её и ставим, когда на него переходят.
    pub rates: Vec<(Res, u32)>,
}

impl Settings {
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            common: CommonSettings::default(),
            width: 1100.0,
            height: 300.0,
            monitor: String::new(),
            show_delay_ms: 150,
            hide_delay_ms: 450,
            fullscreen_guard: true,
            favorites: Vec::new(),
            rates: Vec::new(),
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

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
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
            _ => {}
        }
    }
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
    for f in &s.favorites {
        out.push_str(&format!("favorite={f}\n"));
    }
    for (r, hz) in &s.rates {
        out.push_str(&format!("rate={r} {hz}\n"));
    }
    out
}

pub fn save(s: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
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
            if let Some(d) = path.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&path, BUTTONS_TEMPLATE);
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
