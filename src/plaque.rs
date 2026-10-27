// Живые плашки у края (2.4): событие показывается узкой плашкой у верхнего края монитора, не выдвигая панель. Здесь —
// что такое плашка и её очередь: чистая логика без окна и без Windows, проверяется тестами. События отдают модули
// (`Module::events`) и `watch.rs` (флешки, загрузки); рисует плашку `app/plaque_ui.rs`; слова собирает `texts.rs`.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anvil_ui::Tone;

/// Размер окна плашки, в точках: как плашка таймера была, но шире (значок, два текста, до двух кнопок).
pub const WIDTH: f32 = 372.0;
pub const HEIGHT: f32 = 48.0;
/// Сколько висит обычная плашка; пока мышь над ней или идёт её дело («Извлекаю…»), время стоит.
pub const LIFE: Duration = Duration::from_secs(4);
/// Сколько плашек ждёт своей очереди: больше не копится, старые обычные уходят первыми.
pub const WAIT_MAX: usize = 5;
/// Через сколько после того, как плашка встала на место, её кнопки принимают щелчок: плашка появляется сама, и щелчок,
/// что целился в окно под ней (вкладки браузера у верхнего края), не должен запустить файл или извлечь флешку.
pub const ARM: Duration = Duration::from_millis(400);
/// Обычная плашка, что простояла на месте дольше этого, увидена: когда выдвигается панель, её не пересказывают уведомлением.
pub const SEEN: Duration = Duration::from_secs(1);

/// Вид события — по нему выключатель в настройках («Живые плашки»); таймер выключить нельзя.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bluetooth,
    Usb,
    Github,
    Downloads,
    Timer,
}

/// Значок плашки. Своих знаков (наушники, флешка, батарея) в наборе нет — рисует `app/plaque_ui.rs`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    Headphones,
    /// Устройство Bluetooth, не наушники: значок «Bluetooth» набора.
    Device,
    Battery,
    Usb,
    Branch,
    Download,
    Clock,
}

/// Слова плашки — ключ и числа, а не готовая строка: смена языка не требует повторной работы, а висящая долго плашка
/// («сборка упала») меняет язык вместе с панелью. Строки — `texts::plaque_text`.
#[derive(Clone, Debug, PartialEq)]
pub enum Say {
    /// Ничего: второй строки нет.
    None,
    /// «Таймер «чай» дошёл» / «Таймер дошёл».
    TimerDone(Option<String>),
    WorkDone,
    RestDone,
    /// «Наушники: подключено» — имя устройства.
    Connected(String),
    /// «Bluetooth · заряд 80 %»; заряда не знаем — просто «Bluetooth».
    BtCharge(Option<u8>),
    /// «Мышь: разряжается».
    Low(String),
    /// «Заряд 12 %».
    Battery(u8),
    /// «Флешка «DEMO»».
    Usb(String),
    /// «32 ГБ · вставлена» — размер в байтах.
    UsbIn(u64),
    /// «Можно вынимать».
    Safe,
    /// «Не извлечь: диск занят».
    Busy,
    /// «Извлекаю…»: пока ждём итог, это вместо пояснения.
    Ejecting,
    /// «Флешку уже вынули»: на «Извлечь» нажали, а такого тома уже нет (или на его букве другой).
    Gone,
    /// «demo/site: сборка упала».
    BuildFailed(String),
    BuildPassed(String),
    /// «main · 5 мин назад»: ветка и когда прогон обновился (секунды с 1970); сколько назад, считается при показе.
    RunAge {
        branch: String,
        at: Option<i64>,
    },
    /// «отчёт.pdf: скачано».
    Downloaded(String),
    /// «Скачано файлов: 7» — когда за одно чтение готово несколько.
    DownloadedMany(usize),
    /// «1,2 МБ · Загрузки» — размер в байтах.
    InDownloads(u64),
    /// «Не получилось: report.pdf».
    Failed(String),
    // Кнопки.
    Open,
    /// «Показать в папке»: тот же текст, что у лотка.
    Reveal,
    Eject,
    ToTray,
    Done,
}

impl Say {
    /// Имя внутри слов — устройство, диск, файл, репозиторий, таймер: когда слова не помещаются, режут его, а не конец.
    pub fn name(&self) -> Option<&str> {
        match self {
            Say::TimerDone(Some(n))
            | Say::Connected(n)
            | Say::Low(n)
            | Say::Usb(n)
            | Say::BuildFailed(n)
            | Say::BuildPassed(n)
            | Say::Downloaded(n)
            | Say::Failed(n) => Some(n),
            _ => None,
        }
    }

    /// Те же слова с другим (сокращённым) именем.
    pub fn with_name(&self, name: String) -> Say {
        match self {
            Say::TimerDone(Some(_)) => Say::TimerDone(Some(name)),
            Say::Connected(_) => Say::Connected(name),
            Say::Low(_) => Say::Low(name),
            Say::Usb(_) => Say::Usb(name),
            Say::BuildFailed(_) => Say::BuildFailed(name),
            Say::BuildPassed(_) => Say::BuildPassed(name),
            Say::Downloaded(_) => Say::Downloaded(name),
            Say::Failed(_) => Say::Failed(name),
            other => other.clone(),
        }
    }
}

/// Имя, сокращённое посередине до `keep` знаков и многоточия: «annual…al.pdf» — расширение остаётся на виду, а не пропадает за
/// концевым многоточием. Короче `keep` — как есть.
pub fn middle(name: &str, keep: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= keep {
        return name.to_string();
    }
    if keep <= 1 {
        return format!("{}…", chars.iter().take(keep).collect::<String>());
    }
    // Хвост: расширение с точкой и пара знаков перед ним, но не меньше половины и не всё (голова остаётся). Места мало
    // (меньше расширения с точкой и ещё знака) — только конец: расширение важнее начала имени.
    let ext = name.rfind('.').filter(|&at| at > 0).map_or(0, |at| name[at..].chars().count());
    let tail = if keep > ext { (ext + 2).max(keep / 2).min(keep - 1) } else { keep };
    let head = keep - tail;
    format!("{}…{}", chars[..head].iter().collect::<String>(), chars[chars.len() - tail..].iter().collect::<String>())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    /// Главное: залита акцентом, одна на плашку («Готово»).
    Primary,
    Secondary,
    /// Тихое: без рамки («Извлечь», «В лоток»).
    Ghost,
}

/// Что делает кнопка плашки; исполняет окно (`App::plaque_do`). Пути — `OsString` и `PathBuf`, а не текст: имя файла с
/// непарным суррогатом при переводе в текст искажается, и «Открыть» файла не нашла бы.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    /// Открыть, как двойным щелчком: диск, файл или адрес.
    Open(OsString),
    /// Показать файл в проводнике (`explorer /select`) — вместо «Открыть» у файлов, что при открытии запускаются.
    Reveal(PathBuf),
    /// Положить файл в «Лоток» (лоток хранит текст — путь только из обычных знаков).
    ToTray(String),
    /// Безопасно извлечь съёмный том: корень (`E:\`), имя для слов и серийный номер тома — вынули флешку, а на её букву встал
    /// другой диск, не извлечётся он.
    Eject { root: String, name: String, serial: u32 },
    /// Убрать плашку.
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: Say,
    pub look: Look,
    /// Открывает другое окно или сайт: рядом значок «наружу».
    pub external: bool,
    pub does: Do,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plaque {
    pub kind: Kind,
    /// Повтор с тем же ключом заменяет плашку, а не встаёт следом.
    pub key: String,
    pub glyph: Glyph,
    /// Акцент — обычное, `Danger` — беда, `Warning` — оговорка.
    pub tone: Tone,
    pub title: Say,
    pub sub: Say,
    pub buttons: Vec<Button>,
    /// Висит, пока не закроют (крестиком или своей кнопкой): сборка упала, таймер дошёл.
    pub sticky: bool,
    /// Карточка события: щелчок по плашке открывает панель на её полке (если она стоит на полке).
    pub shelf: Option<&'static str>,
    /// Файл, который можно потянуть с плашки в чат или папку.
    pub file: Option<PathBuf>,
    /// Привязана к предмету события («Флешка вставлена»): предмет пропал — плашка не нужна (`Queue::remove`).
    pub bound: bool,
}

impl Plaque {
    pub fn new(kind: Kind, key: impl Into<String>, glyph: Glyph, tone: Tone, title: Say) -> Plaque {
        Plaque {
            kind,
            key: key.into(),
            glyph,
            tone,
            title,
            sub: Say::None,
            buttons: Vec::new(),
            sticky: false,
            shelf: None,
            file: None,
            bound: false,
        }
    }

    pub fn sub(mut self, sub: Say) -> Plaque {
        self.sub = sub;
        self
    }

    pub fn button(mut self, label: Say, look: Look, does: Do) -> Plaque {
        self.buttons.push(Button { label, look, external: false, does });
        self
    }

    /// Последняя кнопка открывает сайт или другую программу.
    pub fn external(mut self) -> Plaque {
        if let Some(b) = self.buttons.last_mut() {
            b.external = true;
        }
        self
    }

    pub fn sticky(mut self) -> Plaque {
        self.sticky = true;
        self
    }

    pub fn shelf(mut self, card: &'static str) -> Plaque {
        self.shelf = Some(card);
        self
    }

    pub fn file(mut self, path: impl Into<PathBuf>) -> Plaque {
        self.file = Some(path.into());
        self
    }

    /// Плашка о предмете, что может пропасть (флешка): пропал — она уходит.
    pub fn bound(mut self) -> Plaque {
        self.bound = true;
        self
    }

    /// Есть своя кнопка, что убирает плашку («Готово»): крестик тогда не нужен.
    pub fn has_done(&self) -> bool {
        self.buttons.iter().any(|b| b.does == Do::Done)
    }
}

/// Можно ли плашкам быть на экране — решает окно каждый кадр.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gate {
    /// Можно любым.
    Open,
    /// Пользователю сейчас не до уведомлений (показ экрана, игра, «не беспокоить»): ждут все, кроме таймера — он свой.
    TimerOnly,
    /// Нельзя никаким: панель выдвинута или ещё уезжает, на мониторе игра во весь экран, на переднем плане программа из «Не
    /// выезжать».
    Closed,
}

impl Gate {
    /// Можно ли показывать плашку этого вида.
    pub fn allows(self, kind: Kind) -> bool {
        match self {
            Gate::Open => true,
            Gate::TimerOnly => kind == Kind::Timer,
            Gate::Closed => false,
        }
    }
}

/// Плашка, что висит сейчас.
#[derive(Debug)]
struct Shown {
    plaque: Plaque,
    /// Сколько осталось обычной плашке.
    left: Duration,
    /// Когда считали время в последний раз.
    last: Instant,
    /// Кнопка нажата, ответа ждём («Извлечь»): кнопки не нажимаются, пока не заменит итог, а время стоит, как под мышью.
    busy: bool,
    /// Для снимков: доля, на которой плашка замерла.
    held: Option<f32>,
    /// С какого момента плашка стоит на месте (окно выехало до конца): кнопки принимают щелчок не сразу (`Queue::armed`), а
    /// простоявшая обычная плашка считается увиденной (`Queue::take`).
    stood: Option<Instant>,
}

impl Shown {
    fn new(plaque: Plaque, now: Instant) -> Shown {
        Shown { plaque, left: LIFE, last: now, busy: false, held: None, stood: None }
    }
}

/// Очередь плашек: одна на экране, остальные ждут. Время тикает, пока плашка может быть на экране и мышь не над ней.
#[derive(Debug)]
pub struct Queue {
    shown: Option<Shown>,
    wait: VecDeque<Plaque>,
    /// Через сколько после выезда плашка принимает щелчок ([`ARM`]; тесты ставят своё).
    arm: Duration,
}

impl Default for Queue {
    fn default() -> Queue {
        Queue { shown: None, wait: VecDeque::new(), arm: ARM }
    }
}

impl Queue {
    /// Новое событие. Тот же ключ, что у показанной или ждущей, — замена на месте (показанная живёт снова целых 4 с и снова
    /// ждёт, пока не примет щелчок); иначе — в конец очереди. Очередь полна — уходит самая старая обычная из ждущих (важные
    /// берегутся), а нет обычных — самая старая.
    pub fn push(&mut self, plaque: Plaque, now: Instant) {
        if let Some(s) = self.shown.as_mut().filter(|s| s.plaque.key == plaque.key) {
            *s = Shown::new(plaque, now);
            return;
        }
        if let Some(w) = self.wait.iter_mut().find(|w| w.key == plaque.key) {
            *w = plaque;
            return;
        }
        if self.wait.len() >= WAIT_MAX {
            let at = self.wait.iter().position(|w| !w.sticky).unwrap_or(0);
            self.wait.remove(at);
        }
        self.wait.push_back(plaque);
    }

    /// Предмет события пропал (флешку выдернули): его плашка — показанная или ждущая — уходит. Плашки без привязки
    /// (`Plaque::bound`) остаются: «Можно вынимать» после удачного извлечения должно дожить свои 4 с, хотя том уже пропал.
    pub fn remove(&mut self, key: &str) {
        if self.shown.as_ref().is_some_and(|s| s.plaque.key == key && s.plaque.bound) {
            self.shown = None;
        }
        self.wait.retain(|p| !(p.key == key && p.bound));
    }

    /// Показать сразу и держать на доле `fraction` (0…1): для снимков, время не идёт.
    pub fn hold(&mut self, plaque: Plaque, fraction: f32, now: Instant) {
        self.shown = Some(Shown { held: Some(fraction.clamp(0.0, 1.0)), ..Shown::new(plaque, now) });
    }

    /// Шаг времени. `hovered` — мышь над плашкой (время стоит); `gate` — каким плашкам сейчас можно быть на экране (панель
    /// спрятана, не игра во весь экран, не программа из «Не выезжать», пользователь не на показе экрана: иначе плашки копятся,
    /// а время стоит); `idle` — окно уже убрало прежнюю плашку, можно ставить следующую. Следующая — первая из ждущих,
    /// которой можно (таймер обходит остальных, пока им нельзя).
    pub fn step(&mut self, now: Instant, hovered: bool, gate: Gate, idle: bool) {
        if let Some(s) = self.shown.as_mut() {
            let dt = now.saturating_duration_since(s.last);
            s.last = now;
            if gate.allows(s.plaque.kind) && !hovered && !s.busy && !s.plaque.sticky && s.held.is_none() {
                s.left = s.left.saturating_sub(dt);
            }
            if s.left.is_zero() && !s.plaque.sticky && s.held.is_none() {
                self.shown = None;
            }
        }
        // Показанной нельзя, а ждущей можно (таймер при показе экрана): показанная уступает место и встанет заново, когда можно.
        if idle
            && self.shown.as_ref().is_some_and(|s| !gate.allows(s.plaque.kind))
            && self.wait.iter().any(|p| gate.allows(p.kind))
            && let Some(s) = self.shown.take()
        {
            self.wait.push_front(s.plaque);
        }
        if self.shown.is_none()
            && idle
            && let Some(at) = self.wait.iter().position(|p| gate.allows(p.kind))
            && let Some(plaque) = self.wait.remove(at)
        {
            self.shown = Some(Shown::new(plaque, now));
        }
    }

    pub fn current(&self) -> Option<&Plaque> {
        self.shown.as_ref().map(|s| &s.plaque)
    }

    /// Сколько ещё ждёт своей очереди.
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        self.wait.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.shown.is_none() && self.wait.is_empty()
    }

    /// Есть ли что показывать или ждать: пока пусто, окну незачем спрашивать систему, можно ли сейчас показывать.
    pub fn has_any(&self) -> bool {
        self.shown.is_some() || !self.wait.is_empty()
    }

    /// Сколько осталось обычной плашке: 1 — только показали, 0 — уходит. У важной полоски нет.
    pub fn fraction(&self) -> Option<f32> {
        let s = self.shown.as_ref().filter(|s| !s.plaque.sticky)?;
        Some(s.held.unwrap_or_else(|| s.left.as_secs_f32() / LIFE.as_secs_f32()))
    }

    /// Показанную плашку убрали (кнопка, крестик, щелчок): следующая встанет, когда окно её уберёт.
    pub fn dismiss(&mut self) {
        self.shown = None;
    }

    /// Нажата кнопка, что ждёт ответа: пока итог не пришёл, кнопки не нажимаются, а время плашки стоит.
    pub fn set_busy(&mut self, on: bool) {
        if let Some(s) = self.shown.as_mut() {
            s.busy = on;
        }
    }

    pub fn busy(&self) -> bool {
        self.shown.as_ref().is_some_and(|s| s.busy)
    }

    /// Окно сообщает, что показанная плашка стоит на месте (выехала до конца): с этого момента идёт [`ARM`].
    pub fn stand(&mut self, now: Instant) {
        if let Some(s) = self.shown.as_mut() {
            s.stood.get_or_insert(now);
        }
    }

    /// Принимают ли кнопки плашки щелчок: она простояла на месте [`ARM`]. Плашка появляется сама, и щелчок, что целился в
    /// окно под ней, не должен запустить файл или извлечь флешку.
    pub fn armed(&self, now: Instant) -> bool {
        self.shown.as_ref().and_then(|s| s.stood).is_some_and(|at| now.saturating_duration_since(at) >= self.arm)
    }

    /// Тест: плашка принимает щелчок через `after`, а не через [`ARM`].
    #[cfg(test)]
    pub fn set_arm(&mut self, after: Duration) {
        self.arm = after;
    }

    /// Выдвинулась панель: она и есть то место, где смотрят, — плашки (показанная и ждущие) уходят, их слова скажут
    /// уведомления. Обычная, что уже простояла на виду дольше [`SEEN`], увидена — её не пересказывают. Важные остаются и
    /// показываются после, пока их не закроют; `important` — панель вызвали клавишей: клавиатура у панели, а плашку из
    /// другого окна с клавиатуры не закрыть, — тогда уходят и важные, пересказанные уведомлением.
    pub fn take(&mut self, now: Instant, important: bool) -> Vec<Plaque> {
        let mut out = Vec::new();
        if let Some(s) = self.shown.take_if(|s| important || !s.plaque.sticky) {
            let seen = !s.plaque.sticky && s.stood.is_some_and(|at| now.saturating_duration_since(at) >= SEEN);
            if !seen {
                out.push(s.plaque);
            }
        }
        let (keep, gone): (VecDeque<Plaque>, VecDeque<Plaque>) =
            self.wait.drain(..).partition(|p| p.sticky && !important);
        self.wait = keep;
        out.extend(gone);
        out
    }
}

/// Мышь над плашкой у края: окно стоит по центру верхнего края монитора (`monitor` — его прямоугольник в пикселях).
pub fn over(cursor: (i32, i32), monitor: &crate::sys::monitor::PxRect, ppp: f32) -> bool {
    let half = (WIDTH * ppp / 2.0) as i32;
    let center = monitor.left + monitor.width() / 2;
    let rect = crate::sys::monitor::PxRect {
        left: center - half,
        top: monitor.top,
        right: center + half,
        bottom: monitor.top + (HEIGHT * ppp) as i32,
    };
    rect.contains(cursor)
}

/// Плашка с выдуманными данными для снимков (`ANTRESOL_VIEW=strip-plaque-<вид>`) и для документации: «bluetooth», «battery»,
/// «usb», «ejecting» (та же флешка, кнопка «Извлечь» нажата), «eject», «busy», «gone», «github», «downloads»,
/// «downloads-many», «downloads-danger» (файл, что при открытии запускается: «Показать в папке» вместо «Открыть»), «timer».
/// Имена — по языку кадра; настоящих устройств, дисков и файлов нет.
pub fn sample(kind: &str, lang: anvil_ui::Lang) -> Option<Plaque> {
    use std::path::Path;

    use crate::modules::bluetooth;
    use crate::modules::github::{self, RunEvent};
    use crate::modules::timers::{self, Phase};
    use crate::sys::{bt, gh, usb};
    use crate::watch;

    let ru = lang == anvil_ui::Lang::Ru;
    let device = |name: &str, battery: u8, audio: bool| bt::Device {
        name: name.into(),
        connected: true,
        battery: Some(battery),
        address: 1,
        audio,
    };
    let drive = usb::Volume::invented("E:\\", "DEMO", 32 << 30);
    let downloads = Path::new(r"C:\Demo\Downloads");
    let file = |name: &str, len: u64| watch::Entry { name: name.into(), len, modified: None };
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    Some(match kind {
        "bluetooth" => bluetooth::connected_plaque(&device(if ru { "Наушники" } else { "Headphones" }, 80, true)),
        "battery" => bluetooth::low_plaque(&device(if ru { "Мышь" } else { "Mouse" }, 12, false)),
        "usb" | "ejecting" => watch::usb_plaque(&drive),
        "eject" => watch::ejected_plaque(&drive.root, &drive.name(), drive.serial, &Ok(())),
        "busy" => watch::ejected_plaque(&drive.root, &drive.name(), drive.serial, &Err(usb::EjectError::Busy)),
        "gone" => watch::ejected_plaque(&drive.root, &drive.name(), drive.serial, &Err(usb::EjectError::Gone)),
        "github" => {
            let run = gh::Run {
                id: 7001,
                name: "CI".into(),
                status: "completed".into(),
                conclusion: "failure".into(),
                updated: String::new(),
                branch: "main".into(),
            };
            let mut p = github::run_plaque("demo/site", &run, RunEvent::Failed);
            // Прогон закончился минуту назад: «main · 1 мин назад».
            p.sub = Say::RunAge { branch: "main".into(), at: Some(now - 70) };
            p
        }
        "downloads" => watch::download_plaque(downloads, &file(if ru { "отчёт.pdf" } else { "report.pdf" }, 1_258_291)),
        "downloads-many" => watch::many_plaque(
            downloads,
            &[file("photo-1.jpg", 2_411_724), file("photo-2.jpg", 3_145_728), file("notes.txt", 12_288)],
        ),
        "downloads-danger" => {
            watch::download_plaque(downloads, &file("annual_report_2026_final_version.pdf.exe", 48_234_496))
        }
        "timer" => timers::done_plaque(Phase::Plain, None),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(key: &str) -> Plaque {
        Plaque::new(Kind::Usb, key, Glyph::Usb, Tone::Accent, Say::Safe)
    }

    fn sticky(key: &str) -> Plaque {
        p(key).sticky()
    }

    fn of(kind: Kind, key: &str) -> Plaque {
        Plaque::new(kind, key, Glyph::Clock, Tone::Accent, Say::Safe)
    }

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// Ключи по порядку: показанная, потом ждущие.
    fn keys(q: &Queue) -> Vec<String> {
        q.current().map(|c| c.key.clone()).into_iter().chain(q.wait.iter().map(|w| w.key.clone())).collect()
    }

    /// Шаг, как у окна: плашку можно показать, мышь не над ней, окно свободно.
    fn tick(q: &mut Queue, now: Instant) {
        q.step(now, false, Gate::Open, true);
    }

    #[test]
    fn an_ordinary_plaque_lives_four_seconds_and_the_next_comes_after_the_window_hides_it() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        q.push(p("b"), t0);
        assert!(q.current().is_none(), "пока шага не было, ничего не показано");
        tick(&mut q, t0);
        assert_eq!(keys(&q), ["a", "b"], "по одной: показана первая, вторая ждёт");
        assert_eq!(q.waiting(), 1);
        tick(&mut q, at(t0, 3900));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("a"), "ещё не 4 с");
        // Ровно 4 с — ушла; окно ещё уезжает (`idle` ложь), следующая ждёт.
        q.step(at(t0, 4000), false, Gate::Open, false);
        assert!(q.current().is_none());
        assert_eq!(q.waiting(), 1);
        // Окно убрало плашку — встаёт следующая и живёт свои 4 с с этого момента.
        tick(&mut q, at(t0, 4100));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("b"));
        tick(&mut q, at(t0, 7900));
        assert!(q.current().is_some(), "с момента показа прошло 3,8 с");
        tick(&mut q, at(t0, 8100));
        assert!(q.is_empty());
    }

    #[test]
    fn the_bar_runs_down_in_a_straight_line_and_an_important_plaque_has_none() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        assert_eq!(q.fraction(), None, "пока ничего не показано");
        tick(&mut q, t0);
        assert_eq!(q.fraction(), Some(1.0));
        tick(&mut q, at(t0, 1000));
        assert!((q.fraction().unwrap() - 0.75).abs() < 1e-3);
        tick(&mut q, at(t0, 3000));
        assert!((q.fraction().unwrap() - 0.25).abs() < 1e-3);
        let mut q = Queue::default();
        q.push(sticky("s"), t0);
        tick(&mut q, t0);
        assert!(q.current().is_some() && q.fraction().is_none(), "у важной полоски нет");
    }

    #[test]
    fn the_pointer_over_the_plaque_stops_the_clock() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        tick(&mut q, t0);
        q.step(at(t0, 2000), false, Gate::Open, true);
        // Мышь стоит над плашкой десять секунд: время не идёт.
        q.step(at(t0, 2100), true, Gate::Open, true);
        q.step(at(t0, 12_000), true, Gate::Open, true);
        assert!((q.fraction().unwrap() - 0.5).abs() < 1e-3, "{:?}", q.fraction());
        // Ушла — остаётся то, что оставалось: 2 с.
        q.step(at(t0, 12_100), false, Gate::Open, true);
        q.step(at(t0, 13_900), false, Gate::Open, true);
        assert!(q.current().is_some());
        q.step(at(t0, 14_100), false, Gate::Open, true);
        assert!(q.current().is_none());
    }

    /// Ждём итог («Извлекаю…») — время стоит, как под мышью; итог заменяет плашку, и она живёт снова целых 4 с.
    #[test]
    fn a_plaque_that_waits_for_an_answer_stands_like_one_under_the_pointer() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        tick(&mut q, t0);
        tick(&mut q, at(t0, 1000));
        q.set_busy(true);
        assert!(q.busy());
        tick(&mut q, at(t0, 1100));
        tick(&mut q, at(t0, 30_000));
        assert!(q.current().is_some(), "ответа нет — плашка на месте");
        assert!((q.fraction().unwrap() - 0.75).abs() < 1e-3, "{:?}", q.fraction());
        // Итог пришёл: плашка с тем же ключом, кнопки живые, время идёт с начала.
        q.push(p("a"), at(t0, 30_100));
        assert!(!q.busy());
        tick(&mut q, at(t0, 33_000));
        assert!(q.current().is_some());
        tick(&mut q, at(t0, 34_200));
        assert!(q.current().is_none());
    }

    #[test]
    fn an_important_plaque_stays_until_it_is_closed_and_the_rest_wait_behind_it() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(sticky("fail"), t0);
        q.push(p("later"), t0);
        tick(&mut q, t0);
        tick(&mut q, at(t0, 3_600_000));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("fail"), "час спустя всё ещё висит");
        assert_eq!(q.waiting(), 1);
        q.dismiss();
        tick(&mut q, at(t0, 3_600_010));
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("later"));
    }

    #[test]
    fn the_same_key_replaces_in_place() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        for key in ["a", "b", "c"] {
            q.push(p(key), t0);
        }
        tick(&mut q, t0);
        // Ждущая заменяется на своём месте.
        q.push(Plaque::new(Kind::Usb, "c", Glyph::Usb, Tone::Warning, Say::Busy), t0);
        assert_eq!(keys(&q), ["a", "b", "c"]);
        assert_eq!(q.wait[1].title, Say::Busy);
        // Показанная заменяется и живёт снова целых 4 с.
        tick(&mut q, at(t0, 3000));
        assert!((q.fraction().unwrap() - 0.25).abs() < 1e-3);
        q.set_busy(true);
        q.push(Plaque::new(Kind::Usb, "a", Glyph::Usb, Tone::Accent, Say::Busy), at(t0, 3000));
        assert_eq!(q.current().map(|c| c.title.clone()), Some(Say::Busy));
        assert_eq!(q.fraction(), Some(1.0));
        assert!(!q.busy(), "итог заменил плашку — кнопки снова живые");
        assert_eq!(keys(&q), ["a", "b", "c"], "очередь не выросла");
    }

    #[test]
    fn the_queue_holds_five_and_drops_the_oldest_ordinary_one_first() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("shown"), t0);
        tick(&mut q, t0);
        q.push(sticky("important"), t0);
        for i in 0..7 {
            q.push(p(&format!("n{i}")), t0);
        }
        // Ждёт пять: важная осталась, обычные — четыре самых новых.
        assert_eq!(keys(&q), ["shown", "important", "n3", "n4", "n5", "n6"]);
        // Одни важные: уходит самая старая.
        let mut q = Queue::default();
        for i in 0..7 {
            q.push(sticky(&format!("s{i}")), t0);
        }
        assert_eq!(keys(&q), ["s2", "s3", "s4", "s5", "s6"]);
    }

    #[test]
    fn nothing_comes_up_while_it_may_not_and_the_clock_stands() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        q.push(p("b"), t0);
        // Игра во весь экран: копится, на экран не встаёт.
        q.step(t0, false, Gate::Closed, true);
        q.step(at(t0, 60_000), false, Gate::Closed, true);
        assert!(q.current().is_none() && q.waiting() == 2);
        // Можно — встаёт первая, и время идёт с этого момента.
        q.step(at(t0, 60_100), false, Gate::Open, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("a"));
        q.step(at(t0, 62_100), false, Gate::Open, true);
        // Игра началась посреди показа: плашка замерла и ждёт, а не сгорает.
        q.step(at(t0, 62_200), false, Gate::Closed, true);
        q.step(at(t0, 200_000), false, Gate::Closed, true);
        assert!((q.fraction().unwrap() - 0.5).abs() < 1e-3, "{:?}", q.fraction());
        q.step(at(t0, 200_100), false, Gate::Open, true);
        q.step(at(t0, 202_000), false, Gate::Open, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("b"), "a досчитала свои 4 с — встала b");
    }

    #[test]
    fn a_gate_lets_in_the_kinds_it_allows() {
        for kind in [Kind::Bluetooth, Kind::Usb, Kind::Github, Kind::Downloads, Kind::Timer] {
            assert!(Gate::Open.allows(kind) && !Gate::Closed.allows(kind), "{kind:?}");
            assert_eq!(Gate::TimerOnly.allows(kind), kind == Kind::Timer, "{kind:?}");
        }
    }

    /// Показ экрана, игра, «не беспокоить»: все ждут, а таймер проходит вперёд них — он свой, а не событие извне.
    #[test]
    fn while_notices_are_unwelcome_only_the_timer_passes_the_others() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(of(Kind::Bluetooth, "bt"), t0);
        q.push(of(Kind::Timer, "timer"), t0);
        q.push(of(Kind::Downloads, "dl"), t0);
        q.step(t0, false, Gate::TimerOnly, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("timer"), "обошла Bluetooth");
        // Таймер отзвучал; остальным нельзя — они копятся.
        q.step(at(t0, 4000), false, Gate::TimerOnly, true);
        q.step(at(t0, 4100), false, Gate::TimerOnly, true);
        assert!(q.current().is_none() && q.waiting() == 2);
        // Можно — встают по порядку.
        q.step(at(t0, 4200), false, Gate::Open, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("bt"));
        // Показанная не-таймерная застывает, когда пользователю стало не до уведомлений.
        q.step(at(t0, 6200), false, Gate::Open, true);
        q.step(at(t0, 6300), false, Gate::TimerOnly, true);
        q.step(at(t0, 60_000), false, Gate::TimerOnly, true);
        assert!((q.fraction().unwrap() - 0.5).abs() < 1e-3, "{:?}", q.fraction());
    }

    /// Таймер сработал, когда уже висела (и застыла на показе экрана) чужая плашка: она уступает ему место и встаёт после.
    #[test]
    fn a_timer_that_fires_behind_a_frozen_plaque_takes_its_place_for_a_while() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(of(Kind::Bluetooth, "bt"), t0);
        q.step(t0, false, Gate::Open, true);
        q.push(of(Kind::Timer, "timer"), at(t0, 500));
        // Пока можно всем — таймер ждёт своей очереди, как все.
        q.step(at(t0, 600), false, Gate::Open, true);
        assert_eq!(keys(&q), ["bt", "timer"]);
        // Показ экрана: окно спрятало застывшую плашку (`idle`), таймер проходит.
        q.step(at(t0, 700), false, Gate::TimerOnly, false);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("bt"), "окно ещё уезжает — места не уступили");
        q.step(at(t0, 800), false, Gate::TimerOnly, true);
        assert_eq!(keys(&q), ["timer", "bt"], "таймер впереди, прежняя ждёт");
        // Таймер отзвучал, показ кончился — прежняя встаёт.
        q.dismiss();
        q.step(at(t0, 900), false, Gate::TimerOnly, true);
        assert!(q.current().is_none(), "ей по-прежнему нельзя");
        q.step(at(t0, 1000), false, Gate::Open, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("bt"));
        // Важная (упала сборка) уступает так же.
        let mut q = Queue::default();
        q.push(Plaque { kind: Kind::Github, ..sticky("fail") }, t0);
        q.push(of(Kind::Timer, "timer"), t0);
        q.step(t0, false, Gate::Open, true);
        q.step(at(t0, 100), false, Gate::TimerOnly, true);
        assert_eq!(keys(&q), ["timer", "fail"]);
    }

    #[test]
    fn the_next_plaque_waits_while_the_window_has_not_hidden_the_last_one() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("a"), t0);
        q.push(p("b"), t0);
        q.step(t0, false, Gate::Open, true);
        q.dismiss();
        q.step(at(t0, 10), false, Gate::Open, false);
        assert!(q.current().is_none(), "окно ещё уезжает");
        q.step(at(t0, 300), false, Gate::Open, true);
        assert_eq!(q.current().map(|c| c.key.as_str()), Some("b"));
    }

    /// Кнопки принимают щелчок не сразу: плашка появляется сама, и щелчок в окно под ней не должен ничего запускать.
    #[test]
    fn a_plaque_takes_clicks_only_after_it_has_stood_in_place_for_a_moment() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        assert!(!q.armed(t0), "ничего не показано");
        q.push(p("a"), t0);
        tick(&mut q, t0);
        assert!(!q.armed(at(t0, 1000)), "ещё выезжает: окно не сказало, что она встала");
        q.stand(at(t0, 200));
        assert!(!q.armed(at(t0, 200)) && !q.armed(at(t0, 599)));
        assert!(q.armed(at(t0, 600)), "ровно 400 мс на месте");
        // Повторное «встала» отсчёт не сдвигает.
        q.stand(at(t0, 900));
        assert!(q.armed(at(t0, 900)));
        // Заменили (итог извлечения) — отсчёт сначала: новая кнопка под курсором так же не сработает от случайного щелчка.
        q.push(p("a"), at(t0, 1000));
        assert!(!q.armed(at(t0, 1000)));
        q.stand(at(t0, 1000));
        assert!(!q.armed(at(t0, 1399)) && q.armed(at(t0, 1400)));
        // Следующая показанная — со своим отсчётом.
        q.dismiss();
        q.push(p("b"), at(t0, 2000));
        tick(&mut q, at(t0, 2000));
        assert!(!q.armed(at(t0, 5000)));
    }

    /// Предмет пропал (флешку выдернули): его плашка уходит, показанная или ждущая; «Можно вынимать» без привязки доживает.
    #[test]
    fn a_plaque_whose_subject_is_gone_leaves_unless_it_is_free_of_it() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("usb:E:\\").bound(), t0);
        q.push(p("usb:F:\\").bound(), t0);
        q.push(p("usb:G:\\"), t0);
        tick(&mut q, t0);
        q.remove("usb:E:\\");
        assert!(q.current().is_none(), "показанная ушла");
        q.remove("usb:F:\\");
        assert_eq!(keys(&q), ["usb:G:\\"], "ждущая ушла");
        q.remove("usb:G:\\");
        assert_eq!(keys(&q), ["usb:G:\\"], "без привязки — остаётся");
        q.remove("usb:Z:\\");
        assert_eq!(keys(&q), ["usb:G:\\"]);
    }

    #[test]
    fn the_panel_takes_the_ordinary_plaques_and_leaves_the_important_ones() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(p("shown"), t0);
        q.push(p("a"), t0);
        q.push(sticky("fail"), t0);
        q.push(p("b"), t0);
        tick(&mut q, t0);
        let gone: Vec<String> = q.take(at(t0, 100), false).into_iter().map(|g| g.key).collect();
        assert_eq!(gone, ["shown", "a", "b"], "по порядку: показанная и ждущие");
        assert!(q.current().is_none());
        assert_eq!(keys(&q), ["fail"]);
        // Важная уже показана — она остаётся показанной (окно спрячет её само, пока панель открыта).
        tick(&mut q, at(t0, 100));
        assert!(q.take(at(t0, 200), false).is_empty());
        assert_eq!(keys(&q), ["fail"]);
    }

    /// Панель вызвали клавишей: клавиатура у панели, а плашку из другого окна с неё не закрыть — важные тоже пересказываются и
    /// уходят.
    #[test]
    fn a_panel_called_by_the_key_takes_the_important_plaques_too() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(sticky("fail"), t0);
        q.push(p("a"), t0);
        q.push(sticky("fail2"), t0);
        tick(&mut q, t0);
        q.stand(at(t0, 100));
        let gone: Vec<String> = q.take(at(t0, 5000), true).into_iter().map(|g| g.key).collect();
        assert_eq!(gone, ["fail", "a", "fail2"], "и показанная важная, хоть и висела давно");
        assert!(q.current().is_none() && q.waiting() == 0);
    }

    /// Обычная плашка, что постояла на виду дольше секунды, увидена: уведомлением её не повторяют.
    #[test]
    fn a_shown_plaque_that_was_seen_is_not_told_again_when_the_panel_comes_out() {
        let t0 = Instant::now();
        for (stood, now, told) in [
            (Some(0), 900, true),     // успела выехать, но разглядеть не успели
            (Some(0), 1000, false),   // ровно секунда на месте — увидена
            (Some(200), 1100, true),  // встала позже: на месте меньше секунды
            (Some(200), 5000, false), // давно на месте
            (None, 5000, true),       // ещё выезжала
        ] {
            let mut q = Queue::default();
            q.push(p("shown"), t0);
            q.push(p("next"), t0);
            tick(&mut q, t0);
            if let Some(ms) = stood {
                q.stand(at(t0, ms));
            }
            let keys: Vec<String> = q.take(at(t0, now), false).into_iter().map(|g| g.key).collect();
            // Ждущая не видена вовсе — пересказывается всегда.
            assert_eq!(keys, if told { vec!["shown", "next"] } else { vec!["next"] }, "{stood:?} {now}");
        }
    }

    #[test]
    fn a_plaque_held_for_a_snapshot_does_not_run_down() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.hold(p("a"), 0.62, t0);
        q.step(at(t0, 100_000), false, Gate::Open, true);
        assert_eq!(q.fraction(), Some(0.62));
        assert!(q.current().is_some());
    }

    #[test]
    fn a_plaque_with_a_button_of_its_own_that_closes_it_needs_no_cross() {
        let done = p("t").button(Say::Done, Look::Primary, Do::Done);
        assert!(done.has_done());
        let open = p("u").button(Say::Open, Look::Secondary, Do::Open("E:\\".into())).external();
        assert!(!open.has_done() && open.buttons[0].external);
    }

    #[test]
    fn a_long_name_is_cut_in_the_middle_and_keeps_its_extension() {
        assert_eq!(middle("report.pdf", 20), "report.pdf", "короткое — как есть");
        assert_eq!(middle("report.pdf", 10), "report.pdf");
        // Расширение (даже двойное) остаётся на виду: «.exe» нельзя спрятать за многоточием.
        assert_eq!(middle("annual_report_2026_final_version.pdf.exe", 14), "annual_…pdf.exe");
        assert_eq!(middle("очень_длинное_имя_файла.docx", 12), "очень…ла.docx");
        // Без расширения — поровну; точка в начале — не расширение.
        assert_eq!(middle("abcdefghij", 6), "abc…hij");
        assert_eq!(middle(".gitignore_of_a_long_name", 8), ".git…name");
        // Места мало — расширение важнее начала имени: «….exe», а не «a…xe».
        assert_eq!(middle("setup_installer_2026.exe", 4), "….exe");
        assert_eq!(middle("setup_installer_2026.exe", 5), "s….exe");
        assert_eq!(middle("setup_installer_2026.exe", 3), "…exe");
        // Совсем мало места.
        assert_eq!(middle("abcdef", 0), "…");
        assert_eq!(middle("abcdef", 1), "a…");
        assert_eq!(middle("abcdef", 2), "a…f");
        // Сокращённое всегда на знак длиннее `keep` (многоточие) и короче исходного.
        for keep in 0..20 {
            let cut = middle("some_quite_long_file_name.tar.gz", keep);
            assert_eq!(cut.chars().count(), keep + 1, "{keep}: {cut}");
        }
    }

    #[test]
    fn the_name_inside_the_words_can_be_read_and_swapped() {
        let say = Say::Downloaded("annual.pdf".into());
        assert_eq!(say.name(), Some("annual.pdf"));
        assert_eq!(say.with_name("a…f".into()), Say::Downloaded("a…f".into()));
        assert_eq!(Say::TimerDone(Some("tea".into())).with_name("t".into()), Say::TimerDone(Some("t".into())));
        for say in [Say::Safe, Say::TimerDone(None), Say::Open, Say::DownloadedMany(3), Say::Gone, Say::Ejecting] {
            assert_eq!(say.name(), None, "{say:?}");
            assert_eq!(say.with_name("x".into()), say, "{say:?}");
        }
    }

    /// Образцы для снимков есть на каждый вид, и в них только выдуманное: корень диска «E:\\», файл в «C:\\Demo».
    #[test]
    fn every_sample_for_the_snapshots_exists_and_is_invented() {
        for lang in [anvil_ui::Lang::Ru, anvil_ui::Lang::En] {
            for kind in [
                "bluetooth",
                "battery",
                "usb",
                "ejecting",
                "eject",
                "busy",
                "gone",
                "github",
                "downloads",
                "downloads-many",
                "downloads-danger",
                "timer",
            ] {
                let p = sample(kind, lang).unwrap_or_else(|| panic!("нет образца {kind}"));
                let title = crate::texts::plaque_text(lang, &p.title);
                assert!(!title.is_empty(), "{kind}");
            }
        }
        assert!(sample("nope", anvil_ui::Lang::Ru).is_none());
        // Русские слова — только в русском кадре, английские — в английском.
        let words = |lang, kind| crate::texts::plaque_text(lang, &sample(kind, lang).unwrap().title);
        assert_eq!(words(anvil_ui::Lang::Ru, "bluetooth"), "Наушники: подключено");
        assert_eq!(words(anvil_ui::Lang::En, "bluetooth"), "Headphones: connected");
        assert_eq!(words(anvil_ui::Lang::Ru, "downloads"), "отчёт.pdf: скачано");
        assert_eq!(words(anvil_ui::Lang::En, "downloads"), "report.pdf: downloaded");
        assert_eq!(words(anvil_ui::Lang::En, "downloads-many"), "3 files downloaded");
        assert_eq!(words(anvil_ui::Lang::Ru, "gone"), "Флешку уже вынули");
        assert_eq!(words(anvil_ui::Lang::En, "github"), "demo/site: build failed");
        let github = sample("github", anvil_ui::Lang::En).unwrap();
        assert!(github.sticky);
        // Обычный файл открывается, а тот, что при открытии запускается, — показывается в папке.
        let (safe, danger) =
            (sample("downloads", anvil_ui::Lang::En).unwrap(), sample("downloads-danger", anvil_ui::Lang::En).unwrap());
        assert!(matches!(safe.buttons[0].does, Do::Open(_)) && safe.buttons[0].label == Say::Open);
        assert!(matches!(danger.buttons[0].does, Do::Reveal(_)) && danger.buttons[0].label == Say::Reveal);
        // Несколько файлов за раз — одна плашка с папкой, без одного файла для перетаскивания.
        let many = sample("downloads-many", anvil_ui::Lang::En).unwrap();
        assert!(many.file.is_none() && many.buttons.len() == 1 && many.buttons[0].label == Say::Reveal);
    }

    #[test]
    fn the_pointer_is_over_the_plaque_only_inside_its_window_at_the_top_edge() {
        use crate::sys::monitor::PxRect;
        let m = PxRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        // 372 × 48 по центру: от 774 до 1146 по x.
        assert!(over((960, 0), &m, 1.0) && over((774, 47), &m, 1.0));
        assert!(!over((773, 10), &m, 1.0) && !over((1146, 10), &m, 1.0) && !over((960, 48), &m, 1.0));
        // Масштаб 150 %: окно 558 × 72 точки экрана.
        assert!(over((960, 71), &m, 1.5) && !over((960, 72), &m, 1.5));
        assert!(over((681, 10), &m, 1.5) && !over((680, 10), &m, 1.5));
        // Второй монитор правее: окно у его края, а не у первого.
        let second = PxRect { left: 1920, top: 0, right: 3840, bottom: 1080 };
        assert!(over((2880, 10), &second, 1.0) && !over((960, 10), &second, 1.0));
    }
}
