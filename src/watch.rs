// Флешки и загрузки (2.4, плашки у края). У панели для них нет карточки — только события, поэтому они не модули, а
// наблюдатели окна. Каждый читает систему в своём потоке (кадр окна не ждёт): съёмные тома — раз в 2 с, папка «Загрузки» —
// раз в 3 с, и только пока плашки этого вида включены. Первое чтение (после запуска и после включения плашек) ничего не
// объявляет: что уже вставлено и лежит, не новое — плашка только о новом.
// Флешка пропала из списка не сразу: том считается вынутым, когда его нет в двух чтениях подряд (после сна Windows
// отвечает неполным списком), и тогда его плашка уходит из очереди. Тома одного диска (флешка с несколькими разделами) —
// одна плашка.
// Файл в «Загрузках» объявляется, когда дописан: не временный, не пустой, рядом нет его временного двойника, не менялся 2 с,
// и пробное открытие (`shell::probe_file`) не говорит «занят другой программой». Несколько готовых за одно чтение — одна плашка.
// Чтение папки идёт целиком в потоке (там же открываются файлы); окно получает только готовые плашки.
// «Извлечь» запускает безопасное извлечение в потоке (`usb::eject`, за заслоном `sys::blocked`); итог приходит плашкой с
// тем же ключом, что у плашки флешки, — она заменяет её на месте.

use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anvil_ui::Tone;

use crate::config::Settings;
use crate::plaque::{self, Do, Glyph, Look, Plaque, Say};
use crate::sys::shell::{self, Probe};
use crate::sys::usb;

/// Как часто спрашивать съёмные тома.
const USB_EVERY: Duration = Duration::from_secs(2);
/// Как часто смотреть папку «Загрузки».
const DOWNLOADS_EVERY: Duration = Duration::from_secs(3);
/// Сколько файл не должен меняться, чтобы считаться скачанным.
pub const STABLE: Duration = Duration::from_secs(2);
/// Сколько чтений подряд без тома нужно, чтобы считать его вынутым.
const MISSED_TO_GONE: u8 = 2;

/// Что наблюдатели нашли за кадр: новые плашки и ключи плашек, чей предмет пропал (их надо убрать из очереди).
#[derive(Default)]
pub struct Found {
    pub plaques: Vec<Plaque>,
    pub gone: Vec<String>,
}

// ─── Флешки ─────────────────────────────────────────────────────────────────

/// Тома, что появились с прошлого чтения. `prev` — прошлое чтение; `None` — первое: оно ничего не объявляет. Том тот же,
/// пока тот же корень, серийный номер и размер: другая флешка на той же букве — новый том (`usb::Volume::same`).
pub fn new_volumes(prev: Option<&[usb::Volume]>, now: &[usb::Volume]) -> Vec<usb::Volume> {
    let Some(prev) = prev else { return Vec::new() };
    now.iter().filter(|v| !prev.iter().any(|p| p.same(v))).cloned().collect()
}

/// Тома одной флешки (общий диск) — один том для плашки: первый называет её, размеры складываются, «Извлечь» — только если
/// можно всем. Том, чей диск не узнали, сам по себе.
pub fn group_by_disk(volumes: &[usb::Volume]) -> Vec<usb::Volume> {
    let mut out: Vec<usb::Volume> = Vec::new();
    for v in volumes {
        match v.disk.and_then(|disk| out.iter_mut().find(|o| o.disk == Some(disk))) {
            Some(first) => {
                first.total += v.total;
                first.can_eject &= v.can_eject;
            }
            None => out.push(v.clone()),
        }
    }
    out
}

/// Плашка «Флешка «DEMO»»: «Открыть» — корень диска в проводнике, «Извлечь» — безопасное извлечение (карте во встроенном
/// картридере его не предлагают). Привязана к тому: вынули — плашка уходит.
pub fn usb_plaque(v: &usb::Volume) -> Plaque {
    let plaque =
        Plaque::new(plaque::Kind::Usb, format!("usb:{}", v.root), Glyph::Usb, Tone::Accent, Say::Usb(v.name()))
            .sub(Say::UsbIn(v.total))
            .button(Say::Open, Look::Secondary, Do::Open(OsString::from(&v.root)))
            .external()
            .bound();
    if v.can_eject {
        plaque.button(Say::Eject, Look::Ghost, Do::Eject { root: v.root.clone(), name: v.name(), serial: v.serial })
    } else {
        plaque
    }
}

/// Итог извлечения — плашка с тем же ключом, что у плашки флешки (заменяет её): «Можно вынимать»; «Не извлечь: диск занят»
/// (с той же кнопкой «Извлечь» — попробовать снова); «Флешку уже вынули» (на её букве теперь ничего или другой диск; ничего не
/// извлекалось); другая причина (не нашли устройство, нет прав) — «Не получилось: имя», а служебный текст — в журнал.
/// «Можно вынимать» и «уже вынули» не привязаны к тому: тома уже нет, а плашка должна дожить.
pub fn ejected_plaque(root: &str, name: &str, serial: u32, result: &Result<(), usb::EjectError>) -> Plaque {
    let key = format!("usb:{root}");
    let (tone, title) = match result {
        Ok(()) => (Tone::Accent, Say::Safe),
        Err(usb::EjectError::Busy) => (Tone::Warning, Say::Busy),
        Err(usb::EjectError::Gone) => (Tone::Warning, Say::Gone),
        Err(usb::EjectError::Failed(e)) => {
            eprintln!("eject {root} failed: {e}");
            (Tone::Warning, Say::Failed(name.to_string()))
        }
    };
    let plaque = Plaque::new(plaque::Kind::Usb, key, Glyph::Usb, tone, title).sub(match result {
        Err(usb::EjectError::Failed(_)) => Say::None,
        _ => Say::Usb(name.to_string()),
    });
    match result {
        // Занят — «Извлечь» можно нажать снова, закрыв то, что держит диск: без кнопки плашка была бы тупиком.
        Err(usb::EjectError::Busy) => plaque
            .button(Say::Eject, Look::Ghost, Do::Eject { root: root.to_string(), name: name.to_string(), serial })
            .bound(),
        Err(usb::EjectError::Failed(_)) => plaque.bound(),
        _ => plaque,
    }
}

type Volumes = Arc<dyn Fn() -> Vec<usb::Volume> + Send + Sync>;

/// Итог извлечения: корень, имя, серийный номер и ответ.
type Ejected = (String, String, u32, Result<(), usb::EjectError>);

#[derive(Default)]
struct UsbShared {
    busy: bool,
    /// Свежее чтение, ещё не сверенное.
    read: Option<Vec<usb::Volume>>,
    ejected: Vec<Ejected>,
}

/// Что изменилось между чтениями томов.
#[derive(Default, Debug, PartialEq)]
pub struct Settled {
    /// Появились.
    pub new: Vec<usb::Volume>,
    /// Пропали (нет в двух чтениях подряд).
    pub gone: Vec<usb::Volume>,
}

pub struct UsbWatch {
    shared: Arc<Mutex<UsbShared>>,
    source: Volumes,
    every: Duration,
    /// Что считаем вставленным: прошлое чтение и «призраки» — тома, что пропали из одного чтения и ещё не пропали из второго.
    /// `None` — следующее чтение первое.
    prev: Option<Vec<usb::Volume>>,
    /// Сколько чтений подряд без тома (по его букве).
    missed: HashMap<String, u8>,
    polled: Option<Instant>,
    on: bool,
}

impl UsbWatch {
    pub fn new() -> UsbWatch {
        UsbWatch::with(Arc::new(usb::volumes), USB_EVERY)
    }

    /// Свой источник томов и свой период — для тестов.
    pub fn with(source: Volumes, every: Duration) -> UsbWatch {
        UsbWatch { shared: Arc::default(), source, every, prev: None, missed: HashMap::new(), polled: None, on: false }
    }

    /// Сверить новое чтение с тем, что считали вставленным. Том, пропавший из одного чтения, ещё «вставлен» (после сна список
    /// бывает неполным); из двух подряд — вынут. На букве уже другой том — прежний вынут сразу, а новый объявится. Первое
    /// чтение ничего не объявляет.
    pub fn settle(&mut self, list: Vec<usb::Volume>) -> Settled {
        let Some(prev) = self.prev.take() else {
            self.missed.clear();
            self.prev = Some(list);
            return Settled::default();
        };
        let mut believed = list.clone();
        let mut gone = Vec::new();
        for old in &prev {
            let letter = old.root.to_ascii_uppercase();
            if list.iter().any(|v| v.root.eq_ignore_ascii_case(&old.root)) {
                // Том на месте или на его букве уже другой — считать нечего.
                self.missed.remove(&letter);
                continue;
            }
            let missed = self.missed.entry(letter.clone()).or_insert(0);
            *missed += 1;
            if *missed >= MISSED_TO_GONE {
                self.missed.remove(&letter);
                gone.push(old.clone());
            } else {
                believed.push(old.clone());
            }
        }
        let new = new_volumes(Some(&prev), &list);
        self.prev = Some(believed);
        Settled { new, gone }
    }

    /// Каждый кадр: прочитать тома, если пора и плашки включены; отдать новые плашки, ключи плашек пропавших томов и итоги
    /// извлечения.
    pub fn events(&mut self, on: bool) -> Found {
        if on != self.on {
            self.on = on;
            self.prev = None;
            self.missed.clear();
        }
        if on && self.polled.is_none_or(|at| at.elapsed() >= self.every) {
            self.poll();
        }
        let (read, ejected) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.read.take(), std::mem::take(&mut s.ejected))
        };
        let mut found = Found::default();
        if let Some(list) = read
            && on
        {
            let settled = self.settle(list);
            found.plaques.extend(group_by_disk(&settled.new).iter().map(usb_plaque));
            found.gone = settled.gone.iter().map(|v| format!("usb:{}", v.root)).collect();
        }
        // Итог извлечения приходит, даже если плашки потом выключили: на «Извлечь» нажали, ответ нужен.
        found
            .plaques
            .extend(ejected.iter().map(|(root, name, serial, result)| ejected_plaque(root, name, *serial, result)));
        found
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

    /// «Извлечь»: безопасное извлечение в потоке, итог — плашкой (`events`). `serial` — номер тома на плашке: вынули флешку, а
    /// на её букву встал другой диск — извлекать его нельзя.
    pub fn eject(&self, root: String, name: String, serial: u32) {
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| usb::eject(&root, serial))
                .unwrap_or_else(|_| Err(usb::EjectError::Failed("panic".into())));
            shared.lock().unwrap_or_else(|e| e.into_inner()).ejected.push((root, name, serial, result));
        });
    }
}

// ─── Загрузки ───────────────────────────────────────────────────────────────

/// Временные расширения браузеров и загрузчиков: пока файл скачивается, он зовётся так (или рядом с ним лежит такой двойник), а
/// закончив — получает настоящее имя. Chrome и Edge — `.crdownload`, Firefox — `.part`, Opera — `.opdownload`, старый Edge —
/// `.partial`, Safari — `.download`, торренты — `.!ut` и `.!qB`.
const TEMPORARY: [&str; 8] = [".crdownload", ".part", ".tmp", ".download", ".opdownload", ".partial", ".!ut", ".!qb"];

/// Временный ли файл по имени (без учёта регистра).
pub fn is_temporary(name: impl AsRef<OsStr>) -> bool {
    let lower = name.as_ref().to_string_lossy().to_lowercase();
    TEMPORARY.iter().any(|ext| lower.ends_with(ext))
}

/// Имя строчными буквами: имена файлов в Windows без учёта регистра.
fn lowered(name: &OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}

/// Лежит ли рядом временный двойник файла (`имя.crdownload`, `имя.part`…): значит, его ещё качают — Firefox, например,
/// создаёт пустой файл с настоящим именем и пишет в `имя.part`.
fn has_temporary_sibling(name: &OsStr, names: &HashSet<String>) -> bool {
    let lower = lowered(name);
    TEMPORARY.iter().any(|ext| names.contains(&format!("{lower}{ext}")))
}

/// Файл в папке: имя (как есть, без перевода в текст), размер и время правки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: OsString,
    pub len: u64,
    pub modified: Option<SystemTime>,
}

impl Entry {
    /// Можно ли ждать, что файл скачан: не временный, не пустой и без временного двойника рядом.
    fn complete(&self, names: &HashSet<String>) -> bool {
        !is_temporary(&self.name) && self.len > 0 && !has_temporary_sibling(&self.name, names)
    }
}

/// Что лежит в папке: только обычные файлы — без папок, скрытых и системных (`desktop.ini`). Папку прочитать не удалось (нет
/// доступа, сеть пропала, диск отошёл) — `None`: это не «папка пуста», и шаг пропускают, а известное не забывается.
pub fn scan(dir: &Path) -> Option<Vec<Entry>> {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir).ok()? {
        // Ошибка посреди перебора — список неполный, верить ему нельзя.
        let item = item.ok()?;
        // Пропал между перебором и вопросом — просто нет в списке.
        let Ok(meta) = item.metadata() else { continue };
        if !meta.is_file() || meta.file_attributes() & (HIDDEN | SYSTEM) != 0 {
            continue;
        }
        out.push(Entry { name: item.file_name(), len: meta.len(), modified: meta.modified().ok() });
    }
    Some(out)
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
    known: HashSet<OsString>,
    pending: HashMap<OsString, Seen>,
    stable: Duration,
}

impl Folder {
    /// Первое чтение: всё, что уже лежит готовым, не новое. Недокачанное (временное, пустое, с двойником) известным не
    /// становится: дойдёт — это будет событие.
    pub fn new(entries: &[Entry], stable: Duration) -> Folder {
        let names: HashSet<String> = entries.iter().map(|e| lowered(&e.name)).collect();
        let known = entries.iter().filter(|e| e.complete(&names)).map(|e| e.name.clone()).collect();
        Folder { known, pending: HashMap::new(), stable }
    }

    /// Следующее чтение (`now` — когда его сняли): какие файлы скачаны. Новый файл готов, когда он не временный (`.crdownload`,
    /// `.part`…), не пустой, без временного двойника рядом, пробное открытие (`probe`) его не отвергает («занят другой
    /// программой» — пишут) и ни размер, ни время правки (по самому файлу, а не по записи каталога) не менялись `stable`;
    /// менялись — отсчёт заново. Когда браузер переименует временный файл в настоящее имя, это будет новый файл. Пропавший из
    /// папки забывается — скачают снова, и он снова новый.
    pub fn step(&mut self, entries: &[Entry], now: Instant, probe: &mut dyn FnMut(&Entry) -> Probe) -> Vec<Entry> {
        let names: HashSet<String> = entries.iter().map(|e| lowered(&e.name)).collect();
        let mut ready = Vec::new();
        for e in entries {
            if self.known.contains(&e.name) {
                continue;
            }
            if !e.complete(&names) {
                self.pending.remove(&e.name);
                continue;
            }
            let (len, modified) = match probe(e) {
                Probe::Busy | Probe::Gone => {
                    self.pending.remove(&e.name);
                    continue;
                }
                Probe::Unknown => (e.len, e.modified),
                Probe::Ready { len, modified } => (len, modified),
            };
            if len == 0 {
                self.pending.remove(&e.name);
                continue;
            }
            match self.pending.get(&e.name) {
                Some(s) if s.len == len && s.modified == modified => {
                    if now.saturating_duration_since(s.since) >= self.stable {
                        ready.push(Entry { name: e.name.clone(), len, modified });
                        self.known.insert(e.name.clone());
                        self.pending.remove(&e.name);
                    }
                }
                _ => {
                    self.pending.insert(e.name.clone(), Seen { len, modified, since: now });
                }
            }
        }
        let present: HashSet<&OsStr> = entries.iter().map(|e| e.name.as_os_str()).collect();
        self.pending.retain(|name, _| present.contains(name.as_os_str()));
        self.known.retain(|name| present.contains(name.as_os_str()));
        ready
    }
}

/// Плашка «отчёт.pdf: скачано»: «Открыть», «В лоток», файл тянется с плашки в чат или папку; щелчок — на «Лоток». Файл, что при
/// открытии запускается (программа, скрипт, установщик, ярлык, образ — `shell::is_dangerous`), вместо «Открыть» получает
/// «Показать в папке»: он только что пришёл из сети, и случайный щелчок у края не должен его запускать. «В лоток» у такого
/// файла нет: рядом с «Показать в папке» на название оставалось бы около 100 точек, и «setup.exe» не поместился бы; тянуть
/// файл с плашки по-прежнему можно. «В лоток» есть, если путь записывается текстом без потерь (лоток хранит текст).
pub fn download_plaque(dir: &Path, e: &Entry) -> Plaque {
    let path = dir.join(&e.name);
    let plaque = Plaque::new(
        plaque::Kind::Downloads,
        format!("dl:{}", path.display()),
        Glyph::Download,
        Tone::Accent,
        Say::Downloaded(e.name.to_string_lossy().into_owned()),
    )
    .sub(Say::InDownloads(e.len));
    let dangerous = shell::is_dangerous(&e.name);
    let plaque = if dangerous {
        plaque.button(Say::Reveal, Look::Secondary, Do::Reveal(path.clone())).external()
    } else {
        plaque.button(Say::Open, Look::Secondary, Do::Open(path.clone().into_os_string())).external()
    };
    let plaque = match path.to_str() {
        Some(text) if !dangerous => plaque.button(Say::ToTray, Look::Ghost, Do::ToTray(text.to_string())),
        _ => plaque,
    };
    plaque.file(path).shelf("tray")
}

/// Одна плашка на несколько готовых за раз файлов (распаковали архив, сохранили пачку): «Скачано файлов: 7» и размер всех;
/// «Показать в папке» открывает «Загрузки». Файла для перетаскивания нет.
pub fn many_plaque(dir: &Path, files: &[Entry]) -> Plaque {
    let total: u64 = files.iter().map(|e| e.len).sum();
    Plaque::new(
        plaque::Kind::Downloads,
        format!("dl:many:{}", dir.display()),
        Glyph::Download,
        Tone::Accent,
        Say::DownloadedMany(files.len()),
    )
    .sub(Say::InDownloads(total))
    .button(Say::Reveal, Look::Secondary, Do::Open(dir.as_os_str().to_owned()))
    .external()
    .shelf("tray")
}

/// Плашки о готовых за одно чтение: один файл — своя плашка, несколько — одна на всех.
fn plaques_of(dir: &Path, ready: &[Entry]) -> Vec<Plaque> {
    match ready {
        [] => Vec::new(),
        [one] => vec![download_plaque(dir, one)],
        many => vec![many_plaque(dir, many)],
    }
}

type Where = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;
/// Пробное открытие файла: свой — для тестов.
type Prober = Arc<dyn Fn(&Path) -> Probe + Send + Sync>;

/// Что нашло одно чтение папки.
struct Batch {
    /// Какому «включению» плашек оно принадлежит: прочитанное до выключения и включения заново не объявляется.
    generation: u64,
    dir: PathBuf,
    ready: Vec<Entry>,
}

#[derive(Default)]
struct DownloadsShared {
    busy: bool,
    batches: Vec<Batch>,
}

/// Слежка за папкой: живёт в потоке чтения (там же файлы открываются), окно её не трогает.
struct Watching {
    dir: PathBuf,
    folder: Folder,
    generation: u64,
}

/// Одно чтение папки: прочитать, сверить с известным, пробой открыть то, что вот-вот объявится. Папки нет или её не удалось
/// прочитать — `None`: шаг пропущен, известное не забыто. Другая папка («Загрузки» перенесли) или другое «включение» — первое
/// чтение, тихое.
fn read_once(
    place: &Where,
    probe: &Prober,
    state: &Mutex<Option<Watching>>,
    generation: u64,
    stable: Duration,
) -> Option<Batch> {
    let dir = place()?;
    let entries = scan(&dir)?;
    let now = Instant::now();
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    match state.as_mut() {
        Some(w) if w.dir == dir && w.generation == generation => {
            let ready = w.folder.step(&entries, now, &mut |e| probe(&dir.join(&e.name)));
            Some(Batch { generation, dir, ready })
        }
        _ => {
            *state = Some(Watching { dir: dir.clone(), folder: Folder::new(&entries, stable), generation });
            Some(Batch { generation, dir, ready: Vec::new() })
        }
    }
}

pub struct DownloadsWatch {
    shared: Arc<Mutex<DownloadsShared>>,
    state: Arc<Mutex<Option<Watching>>>,
    place: Where,
    probe: Prober,
    every: Duration,
    stable: Duration,
    /// Растёт, когда плашки включают или выключают: чтение прежнего «включения» не объявляется.
    generation: u64,
    polled: Option<Instant>,
    on: bool,
}

impl DownloadsWatch {
    pub fn new() -> DownloadsWatch {
        DownloadsWatch::with(Arc::new(shell::downloads_dir), DOWNLOADS_EVERY, STABLE)
    }

    /// Своя папка, свой период и своё время покоя — для тестов.
    pub fn with(place: Where, every: Duration, stable: Duration) -> DownloadsWatch {
        DownloadsWatch {
            shared: Arc::default(),
            state: Arc::default(),
            place,
            probe: Arc::new(shell::probe_file),
            every,
            stable,
            generation: 0,
            polled: None,
            on: false,
        }
    }

    /// Включили или выключили плашки: следующее чтение — первое, тихое.
    fn sync(&mut self, on: bool) {
        if on != self.on {
            self.on = on;
            self.generation += 1;
        }
    }

    /// Каждый кадр: посмотреть папку, если пора и плашки включены; отдать плашки о скачанных файлах.
    pub fn events(&mut self, on: bool) -> Vec<Plaque> {
        self.sync(on);
        if on && self.polled.is_none_or(|at| at.elapsed() >= self.every) {
            self.poll();
        }
        let batches = std::mem::take(&mut self.shared.lock().unwrap_or_else(|e| e.into_inner()).batches);
        if !on {
            return Vec::new();
        }
        batches.iter().filter(|b| b.generation == self.generation).flat_map(|b| plaques_of(&b.dir, &b.ready)).collect()
    }

    /// Тест: чтение сразу, в этом потоке, — результат ждёт ближайшего `events`.
    #[cfg(test)]
    fn poll_now(&mut self, on: bool) {
        self.sync(on);
        let batch = read_once(&self.place, &self.probe, &self.state, self.generation, self.stable);
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).batches.extend(batch);
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
        let (shared, state) = (self.shared.clone(), self.state.clone());
        let (place, probe) = (self.place.clone(), self.probe.clone());
        let (generation, stable) = (self.generation, self.stable);
        std::thread::spawn(move || {
            let batch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                read_once(&place, &probe, &state, generation, stable)
            }))
            .ok()
            .flatten();
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.batches.extend(batch);
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

    /// Новые плашки флешек и загрузок (какие включены — `Settings::plaques`) и ключи плашек пропавших флешек.
    pub fn events(&mut self, settings: &Settings) -> Found {
        let mut found = self.usb.events(settings.plaques.on(plaque::Kind::Usb));
        found.plaques.extend(self.downloads.events(settings.plaques.on(plaque::Kind::Downloads)));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    fn volume(root: &str, label: &str) -> usb::Volume {
        usb::Volume::invented(root, label, 32 << 30)
    }

    fn entry(name: &str, len: u64) -> Entry {
        Entry { name: name.into(), len, modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(len)) }
    }

    /// Пробное открытие, что видит файл таким, каким его показал каталог.
    fn as_listed(e: &Entry) -> Probe {
        Probe::Ready { len: e.len, modified: e.modified }
    }

    fn step(folder: &mut Folder, entries: &[Entry], now: Instant) -> Vec<Entry> {
        folder.step(entries, now, &mut as_listed)
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
        // Другая флешка на той же букве (другой серийный номер) — новая; та же вернулась — тоже (её вынимали, а потом вставили).
        let other = usb::Volume { serial: a.serial + 1, ..volume("E:\\", "OTHER") };
        assert_eq!(new_volumes(Some(std::slice::from_ref(&a)), std::slice::from_ref(&other)), [other]);
        assert_eq!(new_volumes(Some(&[]), std::slice::from_ref(&a)), std::slice::from_ref(&a));
        // Переименовали метку — флешка та же.
        assert!(new_volumes(Some(std::slice::from_ref(&a)), &[volume("E:\\", "RENAMED")]).is_empty());
    }

    #[test]
    fn volumes_of_one_disk_are_one_plaque_named_by_the_first() {
        let on = |root: &str, label: &str, total: u64, disk: (u32, u32)| usb::Volume {
            total,
            disk: Some(disk),
            ..usb::Volume::invented(root, label, total)
        };
        let list = [
            on("E:\\", "DOCS", 8 << 30, (7, 3)),
            on("F:\\", "PHOTOS", 24 << 30, (7, 3)),
            on("G:\\", "OTHER", 16 << 30, (7, 4)),
            volume("H:\\", "NODISK"),
            volume("I:\\", "NODISK2"),
        ];
        let grouped = group_by_disk(&list);
        assert_eq!(grouped.iter().map(|v| v.root.as_str()).collect::<Vec<_>>(), ["E:\\", "G:\\", "H:\\", "I:\\"]);
        assert_eq!(grouped[0].total, 32 << 30, "размеры складываются");
        assert_eq!(grouped[0].label, "DOCS");
        // Диск не узнали — каждый сам по себе. «Извлечь» — только если можно всем томам диска.
        let card = usb::Volume { can_eject: false, ..on("J:\\", "SD", 1 << 30, (7, 3)) };
        assert!(!group_by_disk(&[list[0].clone(), card])[0].can_eject);
    }

    #[test]
    fn the_usb_plaque_opens_the_drive_and_ejects_it_and_a_drive_without_a_label_is_named_by_its_letter() {
        let drive = volume("E:\\", "DEMO");
        let p = usb_plaque(&drive);
        assert_eq!(
            (p.key.as_str(), p.glyph, p.tone, p.sticky, p.shelf),
            ("usb:E:\\", Glyph::Usb, Tone::Accent, false, None)
        );
        assert!(p.bound, "вынули флешку — плашка уходит");
        assert_eq!((p.title.clone(), p.sub.clone()), (Say::Usb("DEMO".into()), Say::UsbIn(32 << 30)));
        assert_eq!(p.buttons.len(), 2);
        assert_eq!(
            (p.buttons[0].label.clone(), p.buttons[0].look, p.buttons[0].does.clone(), p.buttons[0].external),
            (Say::Open, Look::Secondary, Do::Open("E:\\".into()), true),
            "«Открыть» запускает проводник — значок «наружу»"
        );
        assert_eq!((p.buttons[1].label.clone(), p.buttons[1].look), (Say::Eject, Look::Ghost));
        assert_eq!(
            p.buttons[1].does,
            Do::Eject { root: "E:\\".into(), name: "DEMO".into(), serial: drive.serial },
            "номер тома едет с кнопкой: извлекается именно эта флешка"
        );
        assert_eq!(usb_plaque(&volume("G:\\", "")).title, Say::Usb("G:".into()));
        // Карта во встроенном картридере: «Открыть» есть, «Извлечь» — нет.
        let card = usb_plaque(&usb::Volume { can_eject: false, ..drive });
        assert_eq!(card.buttons.len(), 1);
        assert_eq!(card.buttons[0].label, Say::Open);
    }

    #[test]
    fn the_result_of_ejecting_replaces_the_plaque_of_the_drive() {
        let drive = volume("E:\\", "DEMO");
        let plaque = usb_plaque(&drive);
        let safe = ejected_plaque("E:\\", "DEMO", drive.serial, &Ok(()));
        assert_eq!(
            (safe.key.clone(), safe.tone, safe.title.clone(), safe.sub.clone()),
            (plaque.key.clone(), Tone::Accent, Say::Safe, Say::Usb("DEMO".into()))
        );
        let busy = ejected_plaque("E:\\", "DEMO", drive.serial, &Err(usb::EjectError::Busy));
        assert_eq!((busy.key.clone(), busy.tone, busy.title.clone()), (plaque.key.clone(), Tone::Warning, Say::Busy));
        let failed = ejected_plaque("E:\\", "DEMO", 7, &Err(usb::EjectError::Failed("CM_Get_Parent: 37".into())));
        assert_eq!(
            (failed.key.clone(), failed.tone, failed.title.clone(), failed.sub.clone()),
            (plaque.key.clone(), Tone::Warning, Say::Failed("DEMO".into()), Say::None)
        );
        let gone = ejected_plaque("E:\\", "DEMO", drive.serial, &Err(usb::EjectError::Gone));
        assert_eq!(
            (gone.key.clone(), gone.tone, gone.title.clone(), gone.sub.clone()),
            (plaque.key.clone(), Tone::Warning, Say::Gone, Say::Usb("DEMO".into()))
        );
        assert!(!safe.sticky && safe.buttons.is_empty() && failed.buttons.is_empty() && gone.buttons.is_empty());
        // «Можно вынимать» и «уже вынули» дожидаются своих 4 с, хотя тома уже нет; «занят» и «не вышло» — о томе, что ещё есть.
        assert!(!safe.bound && !gone.bound && busy.bound && failed.bound);
        // Занят — «Извлечь» снова, тем же способом и с тем же номером тома.
        assert_eq!(busy.buttons.len(), 1);
        assert_eq!((busy.buttons[0].label.clone(), busy.buttons[0].look), (Say::Eject, Look::Ghost));
        assert_eq!(busy.buttons[0].does, Do::Eject { root: "E:\\".into(), name: "DEMO".into(), serial: drive.serial });
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
            got.extend(watch.events(on).plaques);
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
        assert!(watch.events(true).plaques.is_empty(), "уже вставленная — не новая");
        world.lock().unwrap().push(volume("T:\\", "DEMO"));
        watch.poll_now();
        let told = watch.events(true).plaques;
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].title, Say::Usb("DEMO".into()));
        watch.poll_now();
        assert!(watch.events(true).plaques.is_empty(), "второй раз то же не объявляется");
        // Плашки выключены: что вставили — молчим, и чтение, что пришло, не копится.
        world.lock().unwrap().push(volume("V:\\", "OFF"));
        watch.poll_now();
        assert!(watch.events(false).plaques.is_empty() && watch.prev.is_none());
        // Включили: первое чтение снова тихое — V: уже вставлена; новая W: — плашка.
        watch.poll_now();
        assert!(watch.events(true).plaques.is_empty());
        world.lock().unwrap().push(volume("W:\\", "NEW"));
        watch.poll_now();
        let told = watch.events(true).plaques;
        assert_eq!(told.iter().map(|p| p.title.clone()).collect::<Vec<_>>(), [Say::Usb("NEW".into())]);
    }

    /// Флешка с тремя разделами — одна плашка, а не три.
    #[test]
    fn a_drive_with_several_partitions_is_announced_by_one_plaque() {
        let world: Arc<Mutex<Vec<usb::Volume>>> = Arc::new(Mutex::new(Vec::new()));
        let mut watch = quiet_usb(&world);
        watch.poll_now();
        watch.events(true);
        let part = |root: &str, label: &str| usb::Volume {
            disk: Some((7, 12)),
            ..usb::Volume::invented(root, label, 8 << 30)
        };
        world.lock().unwrap().extend([part("K:\\", "A"), part("L:\\", "B"), part("M:\\", "C")]);
        watch.poll_now();
        let told = watch.events(true).plaques;
        assert_eq!(told.len(), 1, "три раздела одной флешки — одна плашка");
        assert_eq!(told[0].key, "usb:K:\\");
        assert_eq!(told[0].sub, Say::UsbIn(24 << 30));
    }

    /// Том пропал из одного чтения — ещё вставлен (после сна список бывает неполным), из двух подряд — вынут, и об этом
    /// сообщается ключом плашки; вернулся после одного пропуска — не «вставлена» заново.
    #[test]
    fn a_drive_is_gone_only_after_two_reads_without_it() {
        let world: Arc<Mutex<Vec<usb::Volume>>> = Arc::new(Mutex::new(Vec::new()));
        let (keep, blip) = (volume("N:\\", "KEEP"), volume("O:\\", "BLIP"));
        world.lock().unwrap().extend([keep, blip.clone()]);
        let mut watch = quiet_usb(&world);
        watch.poll_now();
        watch.events(true);
        // Одно чтение без O: — тихо, вставленной она и считается.
        world.lock().unwrap().retain(|v| v.root != "O:\\");
        watch.poll_now();
        let found = watch.events(true);
        assert!(found.plaques.is_empty() && found.gone.is_empty(), "один пропуск — не вынули");
        // Вернулась — не новая.
        world.lock().unwrap().push(blip.clone());
        watch.poll_now();
        let found = watch.events(true);
        assert!(found.plaques.is_empty() && found.gone.is_empty(), "вернулась после пропуска — не «вставлена» снова");
        // Два пропуска подряд — вынули: плашка уходит по ключу.
        world.lock().unwrap().retain(|v| v.root != "O:\\");
        watch.poll_now();
        assert!(watch.events(true).gone.is_empty());
        watch.poll_now();
        let found = watch.events(true);
        assert_eq!(found.gone, ["usb:O:\\"]);
        assert!(found.plaques.is_empty());
        // И теперь её возвращение — событие.
        world.lock().unwrap().push(blip.clone());
        watch.poll_now();
        assert_eq!(watch.events(true).plaques.len(), 1);
        // Счётчик пропусков не копится через возвращения: пропуск, возврат, пропуск — всё ещё не вынули.
        for _ in 0..2 {
            world.lock().unwrap().retain(|v| v.root != "O:\\");
            watch.poll_now();
            assert!(watch.events(true).gone.is_empty());
            world.lock().unwrap().push(blip.clone());
            watch.poll_now();
            assert!(watch.events(true).gone.is_empty());
        }
    }

    /// На букве вынутой флешки встала другая: прежняя вынута сразу (без ключа — плашку новой тот же ключ заменит), новая — событие.
    #[test]
    fn another_drive_on_the_same_letter_replaces_the_old_one_at_once() {
        let world: Arc<Mutex<Vec<usb::Volume>>> = Arc::new(Mutex::new(vec![volume("P:\\", "OLD")]));
        let mut watch = quiet_usb(&world);
        watch.poll_now();
        watch.events(true);
        let other = usb::Volume { serial: 99, ..volume("P:\\", "NEW") };
        *world.lock().unwrap() = vec![other.clone()];
        watch.poll_now();
        let found = watch.events(true);
        assert_eq!(found.plaques.len(), 1);
        assert_eq!(found.plaques[0].key, "usb:P:\\");
        assert!(found.gone.is_empty(), "ключ тот же — вынутой она не объявляется, чтобы не снять плашку новой");
        watch.poll_now();
        let found = watch.events(true);
        assert!(found.plaques.is_empty() && found.gone.is_empty());
    }

    /// «Извлечь» идёт в потоке; итог — плашкой с тем же ключом, и приходит, даже если плашки флешек потом выключили.
    #[test]
    fn ejecting_answers_with_a_plaque_in_every_case() {
        let world: Arc<Mutex<Vec<usb::Volume>>> = Arc::new(Mutex::new(Vec::new()));
        let mut watch = quiet_usb(&world);
        // Выдуманный мир: диск не занят — «Можно вынимать».
        let free = volume("T:\\", "DEMO");
        usb::fake_add(free.clone());
        watch.eject("T:\\".into(), "DEMO".into(), free.serial);
        let safe = gather(&mut watch, true);
        assert_eq!(
            (safe[0].key.as_str(), &safe[0].title, &safe[0].sub),
            ("usb:T:\\", &Say::Safe, &Say::Usb("DEMO".into()))
        );
        // Занятый том: «Не извлечь: диск занят».
        let busy_drive = volume("U:\\", "BUSY");
        usb::fake_add(busy_drive.clone());
        usb::fake_busy("U:\\", true);
        watch.eject("U:\\".into(), "BUSY".into(), busy_drive.serial);
        let busy = gather(&mut watch, true);
        assert_eq!((busy[0].title.clone(), busy[0].tone), (Say::Busy, Tone::Warning));
        usb::fake_busy("U:\\", false);
        // Флешку вынули (или на её букве другой диск): «Флешку уже вынули», ничего не извлекалось.
        watch.eject("U:\\".into(), "BUSY".into(), busy_drive.serial + 1);
        let gone = gather(&mut watch, true);
        assert_eq!((gone[0].title.clone(), gone[0].tone, gone[0].buttons.len()), (Say::Gone, Tone::Warning, 0));
        assert!(usb::volumes().iter().any(|v| v.root == "U:\\"), "чужой серийный номер ничего не извлёк");
        // Плашки выключены — итог всё равно приходит: на «Извлечь» нажали, ответ нужен.
        let off = volume("V:\\", "OFF");
        usb::fake_add(off.clone());
        watch.eject("V:\\".into(), "OFF".into(), off.serial);
        assert_eq!(gather(&mut watch, false)[0].title, Say::Safe);
        usb::fake_remove("U:\\");
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
        for name in [
            "a.crdownload",
            "A.CRDOWNLOAD",
            "b.part",
            "c.tmp",
            "d.download",
            "report.pdf.crdownload",
            "e.opdownload",
            "f.partial",
            "g.zip.!ut",
            "h.iso.!qB",
            "h.iso.!QB",
        ] {
            assert!(is_temporary(name), "{name}");
        }
        for name in ["report.pdf", "part.txt", "tmp", "a.download.pdf", "downloader.exe", "a.ut", "qB.txt"] {
            assert!(!is_temporary(name), "{name}");
        }
    }

    #[test]
    fn a_new_file_is_ready_when_it_has_not_changed_for_two_seconds() {
        let t0 = Instant::now();
        let old = entry("old.pdf", 100);
        let mut folder = Folder::new(std::slice::from_ref(&old), STABLE);
        assert!(step(&mut folder, std::slice::from_ref(&old), t0).is_empty(), "уже лежавший — не новый");
        // Появился и растёт: каждое изменение — отсчёт заново.
        let (small, bigger, full) = (entry("new.zip", 10), entry("new.zip", 20), entry("new.zip", 30));
        assert!(step(&mut folder, &[old.clone(), small.clone()], at(t0, 100)).is_empty());
        assert!(step(&mut folder, &[old.clone(), bigger.clone()], at(t0, 3100)).is_empty(), "вырос — не готов");
        assert!(step(&mut folder, &[old.clone(), full.clone()], at(t0, 6100)).is_empty());
        // Не менялся 1,9 с — ещё нет; 2 с — готов, и один раз.
        assert!(step(&mut folder, &[old.clone(), full.clone()], at(t0, 8000)).is_empty());
        assert_eq!(step(&mut folder, &[old.clone(), full.clone()], at(t0, 8100)), std::slice::from_ref(&full));
        assert!(step(&mut folder, &[old, full], at(t0, 20_000)).is_empty(), "второй раз то же не объявляется");
    }

    #[test]
    fn a_temporary_file_is_ignored_until_the_browser_gives_it_its_real_name() {
        let t0 = Instant::now();
        let mut folder = Folder::new(&[], STABLE);
        let part = entry("report.pdf.crdownload", 500);
        for ms in [0, 3000, 6000, 9000] {
            assert!(
                step(&mut folder, std::slice::from_ref(&part), at(t0, ms)).is_empty(),
                "временный — ни плашки, ни отсчёта"
            );
        }
        assert!(folder.pending.is_empty());
        // Переименован в настоящее имя (размер тот же): новый файл, готов через 2 с.
        let done = entry("report.pdf", 500);
        assert!(step(&mut folder, std::slice::from_ref(&done), at(t0, 10_000)).is_empty());
        assert_eq!(step(&mut folder, std::slice::from_ref(&done), at(t0, 13_000)), [done]);
    }

    /// Firefox создаёт пустой файл с настоящим именем и пишет в `имя.part`: пока `.part` лежит рядом, пустой файл — не
    /// скачанный, а когда `.part` встал на его место, — скачанный.
    #[test]
    fn the_empty_file_of_firefox_with_its_part_beside_is_not_a_finished_download() {
        let t0 = Instant::now();
        let mut folder = Folder::new(&[], STABLE);
        let (placeholder, part) = (entry("movie.mkv", 0), entry("movie.mkv.part", 5000));
        for ms in [0, 3000, 6000, 9000] {
            assert!(
                step(&mut folder, &[placeholder.clone(), part.clone()], at(t0, ms)).is_empty(),
                "пока качается — ни плашки, ни отсчёта"
            );
        }
        assert!(folder.pending.is_empty() && folder.known.is_empty(), "и пустой файл не стал «известным»");
        // `.part` переименован поверх пустого файла: теперь это обычный новый файл.
        let done = entry("movie.mkv", 5000);
        assert!(step(&mut folder, std::slice::from_ref(&done), at(t0, 10_000)).is_empty());
        assert_eq!(step(&mut folder, std::slice::from_ref(&done), at(t0, 13_000)), [done]);
    }

    /// Двойник рядом — и у файла с содержимым: пока рядом `.opdownload`, `.!ut` или другое временное, файл не объявляется.
    #[test]
    fn a_file_with_a_temporary_twin_beside_it_waits_for_the_twin_to_go() {
        for twin in ["clip.mp4.opdownload", "clip.mp4.!ut", "clip.mp4.!qB", "clip.mp4.partial", "CLIP.MP4.CRDOWNLOAD"] {
            let t0 = Instant::now();
            let mut folder = Folder::new(&[], STABLE);
            let (file, twin) = (entry("clip.mp4", 700), entry(twin, 900));
            for ms in [0, 3000, 6000] {
                assert!(
                    step(&mut folder, &[file.clone(), twin.clone()], at(t0, ms)).is_empty(),
                    "{}",
                    twin.name.to_string_lossy()
                );
            }
            // Двойник пропал: отсчёт покоя идёт с этого момента, а не с прошлых чтений.
            assert!(step(&mut folder, std::slice::from_ref(&file), at(t0, 7000)).is_empty());
            assert_eq!(step(&mut folder, std::slice::from_ref(&file), at(t0, 9100)), std::slice::from_ref(&file));
        }
    }

    /// Пустой файл (0 Б) не объявляется и не считается известным: наполнился — это событие. Лежавший пустым при запуске —
    /// тоже не известный.
    #[test]
    fn an_empty_file_is_not_announced_and_is_not_known_either() {
        let t0 = Instant::now();
        let empty = entry("new.txt", 0);
        let mut folder = Folder::new(std::slice::from_ref(&empty), STABLE);
        assert!(folder.known.is_empty());
        for ms in [0, 3000, 6000] {
            assert!(step(&mut folder, std::slice::from_ref(&empty), at(t0, ms)).is_empty());
        }
        let filled = entry("new.txt", 12);
        assert!(step(&mut folder, std::slice::from_ref(&filled), at(t0, 9000)).is_empty());
        assert_eq!(step(&mut folder, std::slice::from_ref(&filled), at(t0, 12_000)), [filled]);
        // Свежие данные файла бывают нулевыми, хотя в каталоге ещё старый размер: не объявляем.
        let mut folder = Folder::new(&[], STABLE);
        let listed = entry("zero.bin", 40);
        let mut zero = |_: &Entry| Probe::Ready { len: 0, modified: None };
        assert!(folder.step(std::slice::from_ref(&listed), t0, &mut zero).is_empty());
        assert!(folder.step(std::slice::from_ref(&listed), at(t0, 9000), &mut zero).is_empty());
    }

    /// Файл, что пишут под настоящим именем (curl, PowerShell), занят: пробное открытие не проходит — не объявляется, пока
    /// пишут, и считается покой только после.
    #[test]
    fn a_file_somebody_still_writes_is_not_announced_until_the_probe_passes() {
        let t0 = Instant::now();
        let mut folder = Folder::new(&[], STABLE);
        let file = entry("big.iso", 4000);
        let mut busy = |_: &Entry| Probe::Busy;
        for ms in [0, 3000, 6000, 9000] {
            assert!(folder.step(std::slice::from_ref(&file), at(t0, ms), &mut busy).is_empty(), "пишут — занят");
        }
        assert!(folder.pending.is_empty());
        // Дописали и закрыли: покой считается с первой удачной пробы.
        assert!(step(&mut folder, std::slice::from_ref(&file), at(t0, 12_000)).is_empty());
        assert!(step(&mut folder, std::slice::from_ref(&file), at(t0, 13_900)).is_empty());
        assert_eq!(step(&mut folder, std::slice::from_ref(&file), at(t0, 14_100)), [file]);
    }

    /// Размер и время берутся у самого файла (пробой), а не из записи каталога, которую NTFS обновляет не сразу: каталог
    /// «не менялся», а файл растёт — не готов; объявляется то, что показала проба.
    #[test]
    fn the_size_comes_from_the_file_itself_and_not_from_the_directory_entry() {
        let t0 = Instant::now();
        let mut folder = Folder::new(&[], STABLE);
        let listed = entry("grow.bin", 100);
        let at_probe =
            |len: u64| Probe::Ready { len, modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(len)) };
        for (ms, len) in [(0, 100), (3000, 200), (6000, 300)] {
            let mut grows = |_: &Entry| at_probe(len);
            assert!(folder.step(std::slice::from_ref(&listed), at(t0, ms), &mut grows).is_empty(), "{len}");
        }
        let mut done = |_: &Entry| at_probe(300);
        let ready = folder.step(std::slice::from_ref(&listed), at(t0, 9000), &mut done);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].len, 300, "в плашку — свежий размер, а не 100 из каталога");
        // Файл пропал между списком и пробой — не объявляется.
        let mut folder = Folder::new(&[], STABLE);
        let mut gone = |_: &Entry| Probe::Gone;
        assert!(folder.step(std::slice::from_ref(&listed), t0, &mut gone).is_empty());
        // Пробой судить нечем (нет прав) — по каталогу, как раньше.
        let mut unknown = |_: &Entry| Probe::Unknown;
        assert!(folder.step(std::slice::from_ref(&listed), at(t0, 100), &mut unknown).is_empty());
        assert_eq!(folder.step(std::slice::from_ref(&listed), at(t0, 3000), &mut unknown), [listed]);
    }

    #[test]
    fn a_file_that_vanished_is_forgotten_and_downloaded_again_it_is_new() {
        let t0 = Instant::now();
        let a = entry("a.pdf", 5);
        let mut folder = Folder::new(std::slice::from_ref(&a), STABLE);
        assert!(step(&mut folder, &[], at(t0, 100)).is_empty(), "удалили");
        assert!(step(&mut folder, std::slice::from_ref(&a), at(t0, 200)).is_empty(), "скачали снова — отсчёт");
        assert_eq!(step(&mut folder, std::slice::from_ref(&a), at(t0, 3000)), std::slice::from_ref(&a));
        // Недокачанный, что пропал, не оставляет следа.
        let b = entry("b.pdf", 5);
        step(&mut folder, &[a.clone(), b], at(t0, 4000));
        assert_eq!(folder.pending.len(), 1);
        step(&mut folder, &[a], at(t0, 5000));
        assert!(folder.pending.is_empty());
    }

    /// Настоящая папка на диске: только обычные файлы — без папок и скрытых; имя — как есть, а не текстом.
    #[test]
    fn scan_lists_ordinary_files_only() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("scan");
        std::fs::write(dir.join("report.pdf"), vec![0u8; 1234]).unwrap();
        std::fs::write(dir.join("клип.mp4"), b"x").unwrap();
        std::fs::create_dir(dir.join("Папка")).unwrap();
        std::fs::OpenOptions::new().write(true).create_new(true).attributes(0x2).open(dir.join("hidden.txt")).unwrap();
        let mut found = scan(&dir).expect("папка читается");
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            found.iter().map(|e| (e.name.to_string_lossy().into_owned(), e.len)).collect::<Vec<_>>(),
            [("report.pdf".to_string(), 1234), ("клип.mp4".to_string(), 1)]
        );
        assert!(found.iter().all(|e| e.modified.is_some()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Не прочитали папку — это не «папка пуста»: список не отдаётся, и известное не забывается.
    #[test]
    fn a_folder_that_cannot_be_read_is_not_an_empty_one() {
        let dir = scratch("unreadable");
        assert_eq!(scan(&dir.join("нет такой")), None);
        assert_eq!(scan(&dir.join("a.txt")), None);
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(scan(&dir.join("a.txt")), None, "файл — не папка");
        assert_eq!(scan(&dir).map(|list| list.len()), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_download_plaque_opens_trays_and_gives_the_file_to_drag() {
        let p = download_plaque(Path::new(r"C:\Demo\Downloads"), &entry("report.pdf", 1_258_291));
        let path = PathBuf::from(r"C:\Demo\Downloads\report.pdf");
        assert_eq!((p.glyph, p.tone, p.sticky, p.shelf), (Glyph::Download, Tone::Accent, false, Some("tray")));
        assert_eq!(p.key, format!("dl:{}", path.display()));
        assert_eq!(
            (p.title.clone(), p.sub.clone()),
            (Say::Downloaded("report.pdf".into()), Say::InDownloads(1_258_291))
        );
        assert_eq!(p.file.as_deref(), Some(path.as_path()), "файл тянется с плашки");
        assert_eq!(
            (p.buttons[0].label.clone(), p.buttons[0].look, p.buttons[0].external, p.buttons[0].does.clone()),
            (Say::Open, Look::Secondary, true, Do::Open(path.clone().into_os_string())),
            "«Открыть» ведёт в другую программу — значок «наружу»"
        );
        assert_eq!(
            (p.buttons[1].label.clone(), p.buttons[1].look, p.buttons[1].does.clone()),
            (Say::ToTray, Look::Ghost, Do::ToTray(path.display().to_string()))
        );
    }

    /// Файл, что при открытии запускается, получает «Показать в папке» вместо «Открыть»; щелчок у края не запустит скачанное.
    #[test]
    fn a_file_that_runs_when_opened_is_shown_in_its_folder_instead() {
        for name in
            ["setup.exe", "run.BAT", "tool.msi", "macro.vbs", "link.lnk", "disk.iso", "script.ps1", "annual.pdf.exe"]
        {
            let p = download_plaque(Path::new(r"C:\Demo\Downloads"), &entry(name, 1000));
            let b = &p.buttons[0];
            assert_eq!(
                (b.label.clone(), b.look, b.external, b.does.clone()),
                (Say::Reveal, Look::Secondary, true, Do::Reveal(PathBuf::from(r"C:\Demo\Downloads").join(name))),
                "{name}"
            );
            assert!(p.buttons.iter().all(|b| !matches!(b.does, Do::Open(_))), "{name}: «Открыть» нет вовсе");
            assert_eq!(p.buttons.len(), 1, "{name}: «В лоток» нет — на название не осталось бы места");
            assert!(p.file.is_some(), "{name}: тянуть файл можно");
        }
        for name in ["report.pdf", "photo.jpg", "notes.txt", "pack.zip"] {
            let p = download_plaque(Path::new(r"C:\Demo\Downloads"), &entry(name, 1000));
            assert_eq!(p.buttons[0].label, Say::Open, "{name}");
        }
    }

    /// Имя с непарным суррогатом в текст не переводится без потерь: «Открыть», «Показать» и перетаскивание получают путь как
    /// есть, а «В лоток» (лоток хранит текст) не предлагается.
    #[test]
    fn a_name_that_is_not_valid_text_keeps_its_exact_path() {
        use std::os::windows::ffi::OsStringExt;
        let name = OsString::from_wide(&[0x72, 0xD800, 0x2E, 0x70, 0x64, 0x66]); // r<одиночный суррогат>.pdf
        assert!(name.to_str().is_none());
        let e = Entry { name: name.clone(), len: 10, modified: None };
        let p = download_plaque(Path::new(r"C:\Demo\Downloads"), &e);
        let exact = Path::new(r"C:\Demo\Downloads").join(&name);
        assert_eq!(p.buttons[0].does, Do::Open(exact.clone().into_os_string()));
        assert_eq!(p.file.as_deref(), Some(exact.as_path()));
        assert!(p.buttons.iter().all(|b| b.label != Say::ToTray), "в лоток — только то, что пишется текстом");
        // Слова — с заменой плохого знака, но путь не тронут.
        assert!(matches!(&p.title, Say::Downloaded(shown) if shown.contains('\u{FFFD}')));
    }

    #[test]
    fn several_files_ready_at_once_are_one_plaque_with_the_folder() {
        let dir = Path::new(r"C:\Demo\Downloads");
        let files = [entry("a.jpg", 1000), entry("b.jpg", 2000), entry("c.txt", 500)];
        let p = many_plaque(dir, &files);
        assert_eq!(
            (p.key.as_str(), p.title.clone(), p.sub.clone()),
            (r"dl:many:C:\Demo\Downloads", Say::DownloadedMany(3), Say::InDownloads(3500))
        );
        assert_eq!(p.buttons.len(), 1);
        assert_eq!(
            (p.buttons[0].label.clone(), p.buttons[0].does.clone(), p.buttons[0].external),
            (Say::Reveal, Do::Open(dir.as_os_str().to_owned()), true)
        );
        assert!(p.file.is_none(), "тянуть пачку нечем");
        assert!(plaques_of(dir, &[]).is_empty());
        assert_eq!(plaques_of(dir, &files[..1])[0].title, Say::Downloaded("a.jpg".into()));
        assert_eq!(plaques_of(dir, &files).len(), 1);
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
            watch.poll_now(on);
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
        assert_eq!(told[0].file.as_deref(), Some(dir.join("report.pdf").as_path()));
        assert!(ask(&mut watch, true).is_empty(), "второй раз то же не объявляется");
        // Выключили: файл скачан — молчим; включили — он уже лежит, не новый.
        std::fs::write(dir.join("later.zip"), b"zip").unwrap();
        assert!(ask(&mut watch, false).is_empty());
        assert!(ask(&mut watch, true).is_empty());
        assert!(ask(&mut watch, true).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Распаковали архив: десяток файлов за одно чтение — одна плашка, а не десять.
    #[test]
    fn a_pile_of_files_at_once_is_one_plaque() {
        let dir = scratch("pile");
        let place = dir.clone();
        let mut watch = quiet_downloads(Arc::new(move || Some(place.clone())));
        let ask = |watch: &mut DownloadsWatch| {
            watch.poll_now(true);
            watch.events(true)
        };
        assert!(ask(&mut watch).is_empty());
        for i in 0..12 {
            std::fs::write(dir.join(format!("photo-{i}.jpg")), vec![0u8; 100]).unwrap();
        }
        assert!(ask(&mut watch).is_empty(), "первый взгляд — отсчёт");
        let told = ask(&mut watch);
        assert_eq!(told.len(), 1, "одна плашка на всех");
        assert_eq!((told[0].title.clone(), told[0].sub.clone()), (Say::DownloadedMany(12), Say::InDownloads(1200)));
        assert!(ask(&mut watch).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Настоящий файл, который кто-то ещё пишет, не объявляется (проба «занят»), а закрытый — объявляется.
    #[test]
    fn a_file_held_open_for_writing_is_announced_after_it_is_closed() {
        use std::io::Write;
        let dir = scratch("held");
        let place = dir.clone();
        let mut watch = quiet_downloads(Arc::new(move || Some(place.clone())));
        let ask = |watch: &mut DownloadsWatch| {
            watch.poll_now(true);
            watch.events(true)
        };
        assert!(ask(&mut watch).is_empty());
        let mut writer = std::fs::File::create(dir.join("stream.bin")).unwrap();
        writer.write_all(&[1u8; 500]).unwrap();
        for _ in 0..3 {
            assert!(ask(&mut watch).is_empty(), "пишут — не готов, сколько бы ни ждали");
        }
        drop(writer);
        assert!(ask(&mut watch).is_empty(), "закрыли — отсчёт");
        let told = ask(&mut watch);
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].sub, Say::InDownloads(500));
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
            watch.poll_now(true);
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

    /// Папка на миг не прочиталась (диск отошёл, сеть моргнула): это не «всё пропало», и когда она вернулась, лежавшее не
    /// объявляется пачкой заново.
    #[test]
    fn a_failed_read_of_the_folder_does_not_make_the_old_files_new() {
        let dir = scratch("flaky");
        for name in ["one.pdf", "two.pdf", "three.pdf"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let place: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(Some(dir.clone())));
        let source = place.clone();
        let mut watch = quiet_downloads(Arc::new(move || source.lock().unwrap().clone()));
        let ask = |watch: &mut DownloadsWatch| {
            watch.poll_now(true);
            watch.events(true)
        };
        assert!(ask(&mut watch).is_empty());
        // Папка «отошла»: чтение не удалось.
        let away = dir.with_file_name(format!("{}-away", dir.file_name().unwrap().to_string_lossy()));
        std::fs::rename(&dir, &away).unwrap();
        assert!(ask(&mut watch).is_empty() && ask(&mut watch).is_empty());
        std::fs::rename(&away, &dir).unwrap();
        assert!(ask(&mut watch).is_empty(), "лежавшее — не новое");
        assert!(ask(&mut watch).is_empty());
        // А новый файл после этого объявляется как обычно.
        std::fs::write(dir.join("four.pdf"), b"x").unwrap();
        assert!(ask(&mut watch).is_empty());
        assert_eq!(ask(&mut watch).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Прочитанное до выключения и включения плашек заново не объявляется: «включение» — новое.
    #[test]
    fn a_read_from_before_the_plaques_were_switched_off_and_on_is_dropped() {
        let dir = scratch("generation");
        let place = dir.clone();
        let mut watch = quiet_downloads(Arc::new(move || Some(place.clone())));
        watch.poll_now(true);
        assert!(watch.events(true).is_empty());
        std::fs::write(dir.join("a.zip"), b"x").unwrap();
        watch.poll_now(true);
        assert!(watch.events(true).is_empty(), "отсчёт");
        // Чтение, что успело дойти уже после выключения-включения.
        std::fs::write(dir.join("b.zip"), b"x").unwrap();
        watch.poll_now(true);
        watch.sync(false);
        watch.sync(true);
        assert!(watch.events(true).is_empty(), "чтение прежнего «включения» не объявляется");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Поток чтения папки сам: свой период — и то, что нашёл, доходит.
    #[test]
    fn the_folder_thread_hands_over_a_finished_download() {
        let dir = scratch("thread");
        let place = dir.clone();
        let mut watch = DownloadsWatch::with(Arc::new(move || Some(place.clone())), Duration::ZERO, Duration::ZERO);
        wait_for("первое чтение папки", || {
            watch.events(true);
            watch.state.lock().unwrap().is_some().then_some(())
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
            let found = watch.events(&settings);
            assert!(found.plaques.is_empty() && found.gone.is_empty());
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
