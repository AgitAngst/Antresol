// Флешки и загрузки (2.4, плашки у края). У панели для них нет карточки — только события, поэтому они не модули, а
// наблюдатели окна. Каждый читает систему в своём потоке (кадр окна не ждёт): съёмные тома — раз в 2 с, папка «Загрузки» —
// раз в 3 с, и только пока плашки этого вида включены. Первое чтение (после запуска и после включения плашек) ничего не
// объявляет: что уже вставлено и лежит, не новое — плашка только о новом.
// «Извлечь» запускает безопасное извлечение в потоке (`usb::eject`, за заслоном `sys::blocked`); итог приходит плашкой с
// тем же ключом, что у плашки флешки, — она заменяет её на месте.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anvil_ui::Tone;

use crate::config::Settings;
use crate::plaque::{self, Do, Glyph, Look, Plaque, Say};
use crate::sys::{shell, usb};

/// Как часто спрашивать съёмные тома.
const USB_EVERY: Duration = Duration::from_secs(2);
/// Как часто смотреть папку «Загрузки».
const DOWNLOADS_EVERY: Duration = Duration::from_secs(3);
/// Сколько файл не должен меняться, чтобы считаться скачанным.
pub const STABLE: Duration = Duration::from_secs(2);

// ─── Флешки ─────────────────────────────────────────────────────────────────

/// Тома, что появились с прошлого чтения. `prev` — прошлое чтение; `None` — первое: оно ничего не объявляет. Том тот же,
/// пока тот же корень, метка и размер: другая флешка на той же букве — новый том.
pub fn new_volumes(prev: Option<&[usb::Volume]>, now: &[usb::Volume]) -> Vec<usb::Volume> {
    let Some(prev) = prev else { return Vec::new() };
    now.iter().filter(|v| !prev.contains(v)).cloned().collect()
}

/// Плашка «Флешка «DEMO»»: «Открыть» — корень диска в проводнике, «Извлечь» — безопасное извлечение.
pub fn usb_plaque(v: &usb::Volume) -> Plaque {
    Plaque::new(plaque::Kind::Usb, format!("usb:{}", v.root), Glyph::Usb, Tone::Accent, Say::Usb(v.name()))
        .sub(Say::UsbIn(v.total))
        .button(Say::Open, Look::Secondary, Do::Open(v.root.clone()))
        .button(Say::Eject, Look::Ghost, Do::Eject { root: v.root.clone(), name: v.name() })
}

/// Итог извлечения — плашка с тем же ключом, что у плашки флешки (заменяет её): «Можно вынимать» или «Не извлечь: диск занят»
/// (с той же кнопкой «Извлечь» — попробовать снова); другая причина (не нашли устройство, нет прав) — «Не получилось: имя», а
/// служебный текст — в журнал.
pub fn ejected_plaque(root: &str, name: &str, result: &Result<(), usb::EjectError>) -> Plaque {
    let key = format!("usb:{root}");
    let (tone, title) = match result {
        Ok(()) => (Tone::Accent, Say::Safe),
        Err(usb::EjectError::Busy) => (Tone::Warning, Say::Busy),
        Err(usb::EjectError::Failed(e)) => {
            eprintln!("eject {root} failed: {e}");
            (Tone::Warning, Say::Failed(name.to_string()))
        }
    };
    let plaque = Plaque::new(plaque::Kind::Usb, key, Glyph::Usb, tone, title).sub(match result {
        Err(usb::EjectError::Failed(_)) => Say::None,
        _ => Say::Usb(name.to_string()),
    });
    // Занят — «Извлечь» можно нажать снова, закрыв то, что держит диск: без кнопки плашка была бы тупиком.
    match result {
        Err(usb::EjectError::Busy) => {
            plaque.button(Say::Eject, Look::Ghost, Do::Eject { root: root.to_string(), name: name.to_string() })
        }
        _ => plaque,
    }
}

type Volumes = Arc<dyn Fn() -> Vec<usb::Volume> + Send + Sync>;

#[derive(Default)]
struct UsbShared {
    busy: bool,
    /// Свежее чтение, ещё не сверенное.
    read: Option<Vec<usb::Volume>>,
    /// Итоги извлечения: корень, имя, ответ.
    ejected: Vec<(String, String, Result<(), usb::EjectError>)>,
}

pub struct UsbWatch {
    shared: Arc<Mutex<UsbShared>>,
    source: Volumes,
    every: Duration,
    /// Прошлое чтение; `None` — следующее первое.
    prev: Option<Vec<usb::Volume>>,
    polled: Option<Instant>,
    on: bool,
}

impl UsbWatch {
    pub fn new() -> UsbWatch {
        UsbWatch::with(Arc::new(usb::volumes), USB_EVERY)
    }

    /// Свой источник томов и свой период — для тестов.
    pub fn with(source: Volumes, every: Duration) -> UsbWatch {
        UsbWatch { shared: Arc::default(), source, every, prev: None, polled: None, on: false }
    }

    /// Каждый кадр: прочитать тома, если пора и плашки включены; отдать новые плашки и итоги извлечения.
    pub fn events(&mut self, on: bool) -> Vec<Plaque> {
        if on != self.on {
            self.on = on;
            self.prev = None;
        }
        if on && self.polled.is_none_or(|at| at.elapsed() >= self.every) {
            self.poll();
        }
        let (read, ejected) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.read.take(), std::mem::take(&mut s.ejected))
        };
        let mut out = Vec::new();
        if let Some(list) = read
            && on
        {
            out.extend(new_volumes(self.prev.as_deref(), &list).iter().map(usb_plaque));
            self.prev = Some(list);
        }
        // Итог извлечения приходит, даже если плашки потом выключили: на «Извлечь» нажали, ответ нужен.
        out.extend(ejected.iter().map(|(root, name, result)| ejected_plaque(root, name, result)));
        out
    }

    fn poll(&mut self) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.busy {
                return;
            }
            s.busy = true;
        }
        self.polled = Some(Instant::now());
        let (shared, source) = (self.shared.clone(), self.source.clone());
        std::thread::spawn(move || {
            // Сбой чтения не должен оставить «занято» навсегда.
            let list = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source())).ok();
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.read = list;
            s.busy = false;
        });
    }

    /// Тест: чтение сразу, в этом потоке, — результат ждёт ближайшего `events`.
    #[cfg(test)]
    fn poll_now(&mut self) {
        let list = (self.source)();
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).read = Some(list);
    }

    /// «Извлечь»: безопасное извлечение в потоке, итог — плашкой (`events`).
    pub fn eject(&self, root: String, name: String) {
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| usb::eject(&root))
                .unwrap_or_else(|_| Err(usb::EjectError::Failed("panic".into())));
            shared.lock().unwrap_or_else(|e| e.into_inner()).ejected.push((root, name, result));
        });
    }
}

// ─── Загрузки ───────────────────────────────────────────────────────────────

/// Временный файл браузера: пока скачивается, он зовётся так, а закончив — переименовывается в настоящее имя.
pub fn is_temporary(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".crdownload", ".part", ".tmp", ".download"].iter().any(|ext| lower.ends_with(ext))
}

/// Файл в папке: имя, размер и время правки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub len: u64,
    pub modified: Option<SystemTime>,
}

/// Что лежит в папке: только обычные файлы — без папок, скрытых и системных (`desktop.ini`).
pub fn scan(dir: &Path) -> Vec<Entry> {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    read.filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() || meta.file_attributes() & (HIDDEN | SYSTEM) != 0 {
                return None;
            }
            Some(Entry {
                name: e.file_name().to_string_lossy().into_owned(),
                len: meta.len(),
                modified: meta.modified().ok(),
            })
        })
        .collect()
}

/// Каким файл увидели в последний раз и с какого момента он такой.
#[derive(Debug)]
struct Seen {
    len: u64,
    modified: Option<SystemTime>,
    since: Instant,
}

/// Слежка за одной папкой: что в ней уже было и что ещё докачивается.
#[derive(Debug)]
pub struct Folder {
    known: HashSet<String>,
    pending: HashMap<String, Seen>,
    stable: Duration,
}

impl Folder {
    /// Первое чтение: всё, что уже лежит, не новое.
    pub fn new(entries: &[Entry], stable: Duration) -> Folder {
        Folder { known: entries.iter().map(|e| e.name.clone()).collect(), pending: HashMap::new(), stable }
    }

    /// Следующее чтение (`now` — когда его сняли): какие файлы скачаны. Новый файл готов, когда он не временный и не менялся
    /// `stable`; менялся — отсчёт заново. Временный (`.crdownload`, `.part`, `.tmp`, `.download`) не считается вовсе: когда
    /// браузер переименует его в настоящее имя, это будет новый файл. Пропавший из папки забывается — скачают снова,
    /// и он снова новый.
    pub fn step(&mut self, entries: &[Entry], now: Instant) -> Vec<Entry> {
        let mut ready = Vec::new();
        for e in entries {
            if self.known.contains(&e.name) {
                continue;
            }
            if is_temporary(&e.name) {
                self.pending.remove(&e.name);
                continue;
            }
            match self.pending.get(&e.name) {
                Some(s) if s.len == e.len && s.modified == e.modified => {
                    if now.saturating_duration_since(s.since) >= self.stable {
                        ready.push(e.clone());
                        self.known.insert(e.name.clone());
                        self.pending.remove(&e.name);
                    }
                }
                _ => {
                    self.pending.insert(e.name.clone(), Seen { len: e.len, modified: e.modified, since: now });
                }
            }
        }
        let present: HashSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        self.pending.retain(|name, _| present.contains(name.as_str()));
        self.known.retain(|name| present.contains(name.as_str()));
        ready
    }
}

/// Плашка «отчёт.pdf скачан»: «Открыть», «В лоток», файл тянется с плашки в чат или папку; щелчок — на «Лоток».
pub fn download_plaque(dir: &Path, e: &Entry) -> Plaque {
    let path = dir.join(&e.name).display().to_string();
    Plaque::new(
        plaque::Kind::Downloads,
        format!("dl:{path}"),
        Glyph::Download,
        Tone::Accent,
        Say::Downloaded(e.name.clone()),
    )
    .sub(Say::InDownloads(e.len))
    .button(Say::Open, Look::Secondary, Do::Open(path.clone()))
    .button(Say::ToTray, Look::Ghost, Do::ToTray(path.clone()))
    .file(path)
    .shelf("tray")
}

type Where = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

#[derive(Default)]
struct DownloadsShared {
    busy: bool,
    /// Свежее чтение, ещё не сверенное: какая папка, что в ней и когда сняли.
    read: Option<(PathBuf, Vec<Entry>, Instant)>,
}

pub struct DownloadsWatch {
    shared: Arc<Mutex<DownloadsShared>>,
    place: Where,
    every: Duration,
    stable: Duration,
    /// Папка и слежка за ней; `None` — следующее чтение первое. Папку «Загрузки» перенесли — первое чтение снова.
    folder: Option<(PathBuf, Folder)>,
    polled: Option<Instant>,
    on: bool,
}

impl DownloadsWatch {
    pub fn new() -> DownloadsWatch {
        DownloadsWatch::with(Arc::new(shell::downloads_dir), DOWNLOADS_EVERY, STABLE)
    }

    /// Своя папка, свой период и своё время покоя — для тестов.
    pub fn with(place: Where, every: Duration, stable: Duration) -> DownloadsWatch {
        DownloadsWatch { shared: Arc::default(), place, every, stable, folder: None, polled: None, on: false }
    }

    /// Каждый кадр: посмотреть папку, если пора и плашки включены; отдать плашки о скачанных файлах.
    pub fn events(&mut self, on: bool) -> Vec<Plaque> {
        if on != self.on {
            self.on = on;
            self.folder = None;
        }
        if on && self.polled.is_none_or(|at| at.elapsed() >= self.every) {
            self.poll();
        }
        let read = self.shared.lock().unwrap_or_else(|e| e.into_inner()).read.take();
        let Some((dir, entries, at)) = read.filter(|_| on) else { return Vec::new() };
        match &mut self.folder {
            Some((known, folder)) if *known == dir => {
                folder.step(&entries, at).iter().map(|e| download_plaque(&dir, e)).collect()
            }
            _ => {
                self.folder = Some((dir, Folder::new(&entries, self.stable)));
                Vec::new()
            }
        }
    }

    /// Тест: чтение сразу, в этом потоке, — результат ждёт ближайшего `events`.
    #[cfg(test)]
    fn poll_now(&mut self) {
        let read = (self.place)().map(|dir| {
            let entries = scan(&dir);
            (dir, entries, Instant::now())
        });
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).read = read;
    }

    fn poll(&mut self) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.busy {
                return;
            }
            s.busy = true;
        }
        self.polled = Some(Instant::now());
        let (shared, place) = (self.shared.clone(), self.place.clone());
        std::thread::spawn(move || {
            let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                place().map(|dir| {
                    let entries = scan(&dir);
                    (dir, entries, Instant::now())
                })
            }))
            .ok()
            .flatten();
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.read = read;
            s.busy = false;
        });
    }
}

/// Все наблюдатели окна.
pub struct Watch {
    pub usb: UsbWatch,
    pub downloads: DownloadsWatch,
}

impl Watch {
    pub fn new() -> Watch {
        Watch { usb: UsbWatch::new(), downloads: DownloadsWatch::new() }
    }

    /// Новые плашки флешек и загрузок (какие включены — `Settings::plaques`).
    pub fn events(&mut self, settings: &Settings) -> Vec<Plaque> {
        let mut out = self.usb.events(settings.plaques.on(plaque::Kind::Usb));
        out.extend(self.downloads.events(settings.plaques.on(plaque::Kind::Downloads)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    fn volume(root: &str, label: &str) -> usb::Volume {
        usb::Volume { root: root.into(), label: label.into(), total: 32 << 30 }
    }

    fn entry(name: &str, len: u64) -> Entry {
        Entry { name: name.into(), len, modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(len)) }
    }

    /// Своя папка теста — на диске репозитория (E:), а не в системной временной (там у владельца другой диск).
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("plaque-test")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait_for<T>(what: &str, mut ask: impl FnMut() -> Option<T>) -> T {
        let start = Instant::now();
        loop {
            if let Some(found) = ask() {
                return found;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "не дождались: {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    // ─── Флешки ───

    #[test]
    fn only_a_volume_that_appeared_after_the_first_read_is_new() {
        let (a, b) = (volume("E:\\", "DEMO"), volume("F:\\", "WORK"));
        assert!(new_volumes(None, std::slice::from_ref(&a)).is_empty(), "первое чтение: что вставлено, то не новое");
        assert_eq!(new_volumes(Some(std::slice::from_ref(&a)), &[a.clone(), b.clone()]), std::slice::from_ref(&b));
        assert!(new_volumes(Some(&[a.clone(), b.clone()]), std::slice::from_ref(&a)).is_empty(), "вынули — не событие");
        // Другая флешка на той же букве — новая; та же вернулась — тоже (её вынимали, а потом вставили).
        let other = volume("E:\\", "OTHER");
        assert_eq!(new_volumes(Some(std::slice::from_ref(&a)), std::slice::from_ref(&other)), [other]);
        assert_eq!(new_volumes(Some(&[]), std::slice::from_ref(&a)), [a]);
    }

    #[test]
    fn the_usb_plaque_opens_the_drive_and_ejects_it_and_a_drive_without_a_label_is_named_by_its_letter() {
        let p = usb_plaque(&volume("E:\\", "DEMO"));
        assert_eq!(
            (p.key.as_str(), p.glyph, p.tone, p.sticky, p.shelf),
            ("usb:E:\\", Glyph::Usb, Tone::Accent, false, None)
        );
        assert_eq!((p.title.clone(), p.sub.clone()), (Say::Usb("DEMO".into()), Say::UsbIn(32 << 30)));
        assert_eq!(p.buttons.len(), 2);
        assert_eq!(
            (p.buttons[0].label.clone(), p.buttons[0].look, p.buttons[0].does.clone()),
            (Say::Open, Look::Secondary, Do::Open("E:\\".into()))
        );
        assert_eq!((p.buttons[1].label.clone(), p.buttons[1].look), (Say::Eject, Look::Ghost));
        assert_eq!(p.buttons[1].does, Do::Eject { root: "E:\\".into(), name: "DEMO".into() });
        assert_eq!(usb_plaque(&volume("G:\\", "")).title, Say::Usb("G:".into()));
    }

    #[test]
    fn the_result_of_ejecting_replaces_the_plaque_of_the_drive() {
        let drive = usb_plaque(&volume("E:\\", "DEMO"));
        let safe = ejected_plaque("E:\\", "DEMO", &Ok(()));
        assert_eq!(
            (safe.key.clone(), safe.tone, safe.title.clone(), safe.sub.clone()),
            (drive.key.clone(), Tone::Accent, Say::Safe, Say::Usb("DEMO".into()))
        );
        let busy = ejected_plaque("E:\\", "DEMO", &Err(usb::EjectError::Busy));
        assert_eq!((busy.key.clone(), busy.tone, busy.title.clone()), (drive.key.clone(), Tone::Warning, Say::Busy));
        let failed = ejected_plaque("E:\\", "DEMO", &Err(usb::EjectError::Failed("CM_Get_Parent: 37".into())));
        assert_eq!(
            (failed.key, failed.tone, failed.title, failed.sub),
            (drive.key, Tone::Warning, Say::Failed("DEMO".into()), Say::None)
        );
        assert!(!safe.sticky && safe.buttons.is_empty() && failed.buttons.is_empty());
        // Занят — «Извлечь» снова, тем же способом.
        assert_eq!(busy.buttons.len(), 1);
        assert_eq!((busy.buttons[0].label.clone(), busy.buttons[0].look), (Say::Eject, Look::Ghost));
        assert_eq!(busy.buttons[0].does, Do::Eject { root: "E:\\".into(), name: "DEMO".into() });
    }

    /// Наблюдатель, что сам не читает: период огромный, а чтение делает тест (`poll_now`) — без гонок с потоком.
    fn quiet_usb(world: &Arc<Mutex<Vec<usb::Volume>>>) -> UsbWatch {
        let source = world.clone();
        let mut watch = UsbWatch::with(Arc::new(move || source.lock().unwrap().clone()), Duration::from_secs(3600));
        watch.polled = Some(Instant::now());
        watch
    }

    /// Ждать, пока поток что-нибудь да принесёт: плашки копятся, ничего не теряется.
    fn gather(watch: &mut UsbWatch, on: bool) -> Vec<Plaque> {
        let mut got = Vec::new();
        wait_for("итог из потока", || {
            got.extend(watch.events(on));
            (!got.is_empty()).then_some(())
        });
        got
    }

    /// Первое чтение тихое, вставленная флешка — плашка один раз; выключили плашки — ничего не объявляется, а включили —
    /// снова с тихого первого чтения.
    #[test]
    fn the_watch_announces_a_plugged_drive_once() {
        let world: Arc<Mutex<Vec<usb::Volume>>> = Arc::new(Mutex::new(vec![volume("S:\\", "ALREADY")]));
        let mut watch = quiet_usb(&world);
        watch.poll_now();
        assert!(watch.events(true).is_empty(), "уже вставленная — не новая");
        world.lock().unwrap().push(volume("T:\\", "DEMO"));
        watch.poll_now();
        let told = watch.events(true);
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].title, Say::Usb("DEMO".into()));
        watch.poll_now();
        assert!(watch.events(true).is_empty(), "второй раз то же не объявляется");
        // Плашки выключены: что вставили — молчим, и чтение, что пришло, не копится.
        world.lock().unwrap().push(volume("V:\\", "OFF"));
        watch.poll_now();
        assert!(watch.events(false).is_empty() && watch.prev.is_none());
        // Включили: первое чтение снова тихое — V: уже вставлена; новая W: — плашка.
        watch.poll_now();
        assert!(watch.events(true).is_empty());
        world.lock().unwrap().push(volume("W:\\", "NEW"));
        watch.poll_now();
        assert_eq!(watch.events(true).iter().map(|p| p.title.clone()).collect::<Vec<_>>(), [Say::Usb("NEW".into())]);
    }

    /// «Извлечь» идёт в потоке; итог — плашкой с тем же ключом, и приходит, даже если плашки флешек потом выключили.
    #[test]
    fn ejecting_answers_with_a_plaque_in_every_case() {
        let world: Arc<Mutex<Vec<usb::Volume>>> = Arc::new(Mutex::new(Vec::new()));
        let mut watch = quiet_usb(&world);
        // Выдуманный мир: диск не занят — «Можно вынимать».
        watch.eject("T:\\".into(), "DEMO".into());
        let safe = gather(&mut watch, true);
        assert_eq!(
            (safe[0].key.as_str(), &safe[0].title, &safe[0].sub),
            ("usb:T:\\", &Say::Safe, &Say::Usb("DEMO".into()))
        );
        // Занятый том: «Не извлечь: диск занят».
        usb::fake_busy("U:\\", true);
        watch.eject("U:\\".into(), "BUSY".into());
        let busy = gather(&mut watch, true);
        assert_eq!((busy[0].title.clone(), busy[0].tone), (Say::Busy, Tone::Warning));
        usb::fake_busy("U:\\", false);
        // Плашки выключены — итог всё равно приходит: на «Извлечь» нажали, ответ нужен.
        watch.eject("V:\\".into(), "OFF".into());
        assert_eq!(gather(&mut watch, false)[0].title, Say::Safe);
        let log = crate::sys::BLOCKED.lock().unwrap_or_else(|e| e.into_inner());
        assert!(log.iter().any(|w| w == "usb::eject T:\\") && log.iter().any(|w| w == "usb::eject U:\\"));
    }

    /// Поток чтения сам: свой период, свой ответ — и сверка с первым чтением.
    #[test]
    fn the_reading_thread_hands_over_what_it_found() {
        let world: Arc<Mutex<Vec<usb::Volume>>> = Arc::new(Mutex::new(Vec::new()));
        let source = world.clone();
        let mut watch = UsbWatch::with(Arc::new(move || source.lock().unwrap().clone()), Duration::ZERO);
        // Первое чтение — пустое, молчит; потом вставили.
        wait_for("первое чтение", || {
            watch.events(true);
            watch.prev.is_some().then_some(())
        });
        world.lock().unwrap().push(volume("X:\\", "THREAD"));
        let told = gather(&mut watch, true);
        assert_eq!(told[0].title, Say::Usb("THREAD".into()));
    }

    // ─── Загрузки ───

    #[test]
    fn temporary_download_names_are_recognised_without_regard_to_case() {
        for name in ["a.crdownload", "A.CRDOWNLOAD", "b.part", "c.tmp", "d.download", "report.pdf.crdownload"] {
            assert!(is_temporary(name), "{name}");
        }
        for name in ["report.pdf", "part.txt", "tmp", "a.download.pdf", "downloader.exe"] {
            assert!(!is_temporary(name), "{name}");
        }
    }

    #[test]
    fn a_new_file_is_ready_when_it_has_not_changed_for_two_seconds() {
        let t0 = Instant::now();
        let old = entry("old.pdf", 100);
        let mut folder = Folder::new(std::slice::from_ref(&old), STABLE);
        assert!(folder.step(std::slice::from_ref(&old), t0).is_empty(), "уже лежавший — не новый");
        // Появился и растёт: каждое изменение — отсчёт заново.
        let (small, bigger, full) = (entry("new.zip", 10), entry("new.zip", 20), entry("new.zip", 30));
        assert!(folder.step(&[old.clone(), small.clone()], at(t0, 100)).is_empty());
        assert!(folder.step(&[old.clone(), bigger.clone()], at(t0, 3100)).is_empty(), "вырос — не готов");
        assert!(folder.step(&[old.clone(), full.clone()], at(t0, 6100)).is_empty());
        // Не менялся 1,9 с — ещё нет; 2 с — готов, и один раз.
        assert!(folder.step(&[old.clone(), full.clone()], at(t0, 8000)).is_empty());
        assert_eq!(folder.step(&[old.clone(), full.clone()], at(t0, 8100)), std::slice::from_ref(&full));
        assert!(folder.step(&[old, full], at(t0, 20_000)).is_empty(), "второй раз то же не объявляется");
    }

    #[test]
    fn a_temporary_file_is_ignored_until_the_browser_gives_it_its_real_name() {
        let t0 = Instant::now();
        let mut folder = Folder::new(&[], STABLE);
        let part = entry("report.pdf.crdownload", 500);
        for ms in [0, 3000, 6000, 9000] {
            assert!(
                folder.step(std::slice::from_ref(&part), at(t0, ms)).is_empty(),
                "временный — ни плашки, ни отсчёта"
            );
        }
        assert!(folder.pending.is_empty());
        // Переименован в настоящее имя (размер тот же): новый файл, готов через 2 с.
        let done = entry("report.pdf", 500);
        assert!(folder.step(std::slice::from_ref(&done), at(t0, 10_000)).is_empty());
        assert_eq!(folder.step(std::slice::from_ref(&done), at(t0, 13_000)), [done]);
    }

    #[test]
    fn a_file_that_vanished_is_forgotten_and_downloaded_again_it_is_new() {
        let t0 = Instant::now();
        let a = entry("a.pdf", 5);
        let mut folder = Folder::new(std::slice::from_ref(&a), STABLE);
        assert!(folder.step(&[], at(t0, 100)).is_empty(), "удалили");
        assert!(folder.step(std::slice::from_ref(&a), at(t0, 200)).is_empty(), "скачали снова — отсчёт");
        assert_eq!(folder.step(std::slice::from_ref(&a), at(t0, 3000)), std::slice::from_ref(&a));
        // Недокачанный, что пропал, не оставляет следа.
        let b = entry("b.pdf", 5);
        folder.step(&[a.clone(), b], at(t0, 4000));
        assert_eq!(folder.pending.len(), 1);
        folder.step(&[a], at(t0, 5000));
        assert!(folder.pending.is_empty());
    }

    /// Настоящая папка на диске: только обычные файлы — без папок и скрытых.
    #[test]
    fn scan_lists_ordinary_files_only() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("scan");
        std::fs::write(dir.join("report.pdf"), vec![0u8; 1234]).unwrap();
        std::fs::write(dir.join("клип.mp4"), b"x").unwrap();
        std::fs::create_dir(dir.join("Папка")).unwrap();
        std::fs::OpenOptions::new().write(true).create_new(true).attributes(0x2).open(dir.join("hidden.txt")).unwrap();
        let mut found = scan(&dir);
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            found.iter().map(|e| (e.name.as_str(), e.len)).collect::<Vec<_>>(),
            [("report.pdf", 1234), ("клип.mp4", 1)]
        );
        assert!(found.iter().all(|e| e.modified.is_some()));
        assert!(scan(&dir.join("нет такой")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_download_plaque_opens_trays_and_gives_the_file_to_drag() {
        let p = download_plaque(Path::new(r"C:\Demo\Downloads"), &entry("report.pdf", 1_258_291));
        let path = r"C:\Demo\Downloads\report.pdf";
        assert_eq!((p.glyph, p.tone, p.sticky, p.shelf), (Glyph::Download, Tone::Accent, false, Some("tray")));
        assert_eq!(p.key, format!("dl:{path}"));
        assert_eq!(
            (p.title.clone(), p.sub.clone()),
            (Say::Downloaded("report.pdf".into()), Say::InDownloads(1_258_291))
        );
        assert_eq!(p.file.as_deref(), Some(path), "файл тянется с плашки");
        assert_eq!((p.buttons[0].look, p.buttons[0].does.clone()), (Look::Secondary, Do::Open(path.into())));
        assert_eq!(
            (p.buttons[1].label.clone(), p.buttons[1].look, p.buttons[1].does.clone()),
            (Say::ToTray, Look::Ghost, Do::ToTray(path.into()))
        );
    }

    /// Наблюдатель, что сам не читает: период огромный, а чтение делает тест (`poll_now`); покой файла — 0.
    fn quiet_downloads(place: Where) -> DownloadsWatch {
        let mut watch = DownloadsWatch::with(place, Duration::from_secs(3600), Duration::ZERO);
        watch.polled = Some(Instant::now());
        watch
    }

    /// Настоящая временная папка: лежавший файл молчит, временный молчит, готовый — плашка один раз; выключили — ничего;
    /// включили — снова с тихого первого чтения.
    #[test]
    fn the_watch_announces_a_finished_download_once() {
        let dir = scratch("watch");
        std::fs::write(dir.join("before.txt"), b"old").unwrap();
        let place = dir.clone();
        let mut watch = quiet_downloads(Arc::new(move || Some(place.clone())));
        let ask = |watch: &mut DownloadsWatch, on: bool| {
            watch.poll_now();
            watch.events(on)
        };
        assert!(ask(&mut watch, true).is_empty(), "лежавший до запуска — не новый");
        std::fs::write(dir.join("report.pdf.crdownload"), b"half").unwrap();
        assert!(ask(&mut watch, true).is_empty() && ask(&mut watch, true).is_empty(), "временный молчит");
        std::fs::rename(dir.join("report.pdf.crdownload"), dir.join("report.pdf")).unwrap();
        // Первый взгляд на новый файл — отсчёт, второй (покой 0) — готов.
        assert!(ask(&mut watch, true).is_empty());
        let told = ask(&mut watch, true);
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].title, Say::Downloaded("report.pdf".into()));
        assert_eq!(told[0].file.as_deref(), Some(dir.join("report.pdf").display().to_string().as_str()));
        assert!(ask(&mut watch, true).is_empty(), "второй раз то же не объявляется");
        // Выключили: файл скачан — молчим; включили — он уже лежит, не новый.
        std::fs::write(dir.join("later.zip"), b"zip").unwrap();
        assert!(ask(&mut watch, false).is_empty() && watch.folder.is_none());
        assert!(ask(&mut watch, true).is_empty());
        assert!(ask(&mut watch, true).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Папку «Загрузки» перенесли (или не нашли): чтение другой папки — первое, тихое; нет папки — тишина.
    #[test]
    fn a_moved_downloads_folder_starts_a_quiet_first_read_and_no_folder_is_silence() {
        let (a, b) = (scratch("moved-a"), scratch("moved-b"));
        std::fs::write(b.join("already.txt"), b"x").unwrap();
        let place: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(Some(a.clone())));
        let source = place.clone();
        let mut watch = quiet_downloads(Arc::new(move || source.lock().unwrap().clone()));
        let ask = |watch: &mut DownloadsWatch| {
            watch.poll_now();
            watch.events(true)
        };
        assert!(ask(&mut watch).is_empty());
        *place.lock().unwrap() = Some(b.clone());
        assert!(ask(&mut watch).is_empty(), "новая папка — первое чтение: лежавшее не новое");
        assert!(ask(&mut watch).is_empty() && ask(&mut watch).is_empty());
        *place.lock().unwrap() = None;
        assert!(ask(&mut watch).is_empty() && ask(&mut watch).is_empty(), "папки нет — тишина");
        let _ = (std::fs::remove_dir_all(&a), std::fs::remove_dir_all(&b));
    }

    /// Поток чтения папки сам: свой период — и то, что нашёл, доходит.
    #[test]
    fn the_folder_thread_hands_over_a_finished_download() {
        let dir = scratch("thread");
        let place = dir.clone();
        let mut watch = DownloadsWatch::with(Arc::new(move || Some(place.clone())), Duration::ZERO, Duration::ZERO);
        wait_for("первое чтение папки", || {
            watch.events(true);
            watch.folder.is_some().then_some(())
        });
        std::fs::write(dir.join("fresh.zip"), b"zip").unwrap();
        let mut got = Vec::new();
        wait_for("плашка загрузки", || {
            got.extend(watch.events(true));
            (!got.is_empty()).then_some(())
        });
        assert_eq!(got[0].title, Say::Downloaded("fresh.zip".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Все наблюдатели в выдуманном мире: настоящих томов и «Загрузок» этого ПК нет — ничего не объявляется.
    #[test]
    fn the_real_watchers_see_nothing_in_the_invented_world() {
        let mut watch = Watch::new();
        let settings = Settings::default();
        for _ in 0..3 {
            assert!(watch.events(&settings).is_empty());
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
