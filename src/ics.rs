// Календарь .ics (этап 2.5 «Встречи»): разбор файла — строки, события, часовые пояса (`VTIMEZONE`), повторы, исключения,
// отмены, — ссылки встреч и ссылка на сам календарь. Чистые функции без Windows и сети: проверяются тестами; время ПК
// и загрузка — снаружи (`sys::tz`, `sys::ical`).
// Ссылка на календарь — секрет, ссылка встречи (в ней бывает пароль, `pwd=`) — тоже: ни та ни другая не идёт в журнал и
// в ошибки; в окне ссылка календаря — только сокращённо (`short_url`).

use std::collections::HashSet;

/// Событий в календаре не больше: остальные пропускаются (прочитанное остаётся).
pub const MAX_EVENTS: usize = 2000;
/// Ссылка (календаря или встречи) длиннее не бывает.
pub const MAX_LINK: usize = 2000;
/// Название встречи и место в окне — не длиннее (в знаках): остальное из чужого файла не нужно.
const MAX_TITLE: usize = 200;
const MAX_PLACE: usize = 160;
const MAX_NAME: usize = 60;
/// Название пояса не длиннее (в знаках): оно идёт в журнал.
const MAX_ZONE_NAME: usize = 64;

// Пределы чужого файла: календарь по ссылке — чужие данные, и всё, что в нём можно растянуть, ограничено. Ни один предел не
// режет настоящий календарь (Google, Outlook, iCloud, Apple), и каждый держит время или память.
/// Значение свойства (и его параметры), кроме `DESCRIPTION`, не длиннее: длинное (вложение, мусор) пропускается целиком.
const MAX_VALUE: usize = 8 * 1024;
/// Ссылку в описании ищем только в его начале.
const MAX_DESCRIPTION: usize = 64 * 1024;
/// Списки правила повторения: `BYDAY` (без повторов), `BYMONTHDAY`, `BYSETPOS` — длиннее правило негодное.
const MAX_BY_DAY: usize = 64;
const MAX_BY_MONTH_DAY: usize = 62;
const MAX_BY_SET_POS: usize = 32;
/// Пояс: наблюдений (`STANDARD`, `DAYLIGHT`) и дат `RDATE` в каждом — лишние пропускаются; поясов в файле — сверх предела
/// время читается как местное.
const MAX_OBSERVANCES: usize = 64;
const MAX_RDATES: usize = 256;
const MAX_ZONES: usize = 64;
/// Исключений (`EXDATE`) у события — лишние пропускаются; из них у окна смотрится не больше такого числа.
const MAX_EXDATES: usize = 1000;
const MAX_NEAR_EXDATES: usize = 64;
/// Отказавших участников у события запоминается не больше.
const MAX_DECLINED: usize = 32;
/// Шагов на одну серию за один разворот. Шаг — примерно одно сравнение дня с правилом: период стоит от шести шагов, а с
/// длинными списками правила дороже (`Rule::period_cost`). Серия со счётом, что за это не дошла до окна, считается
/// кончившейся; у остальных (и у серии, чей счёт до окна не кончится) перебор идёт от самого окна и тратит единицы шагов.
const SERIES_STEPS: u32 = 50_000;
/// Событие длиннее — не встреча (отпуск, поездка) и не показывается; заодно серии с огромной длительностью не заполняют окно.
const MAX_LENGTH: i64 = 31 * DAY;
/// Вхождений одной серии в окне не больше (в настоящем календаре — по числу дней окна).
const MAX_PER_SERIES: usize = 24;
/// Самый большой шаг правила (`INTERVAL`): больше — правило негодное.
const MAX_INTERVAL: i64 = 10_000;
const DAY: i64 = 86_400;

/// Смещение пояса этого ПК от UTC в секундах на момент UTC (секунды с 1970 года).
pub type LocalFn<'a> = &'a dyn Fn(i64) -> i32;

// ─── Даты ───────────────────────────────────────────────────────────────────

/// Дней от 1970-01-01 по гражданскому календарю (алгоритм Хиннанта).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Обратное к [`days_from_civil`]: год, месяц, день.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(y) => 29,
        _ => 28,
    }
}

/// День недели по номеру дня от 1970-01-01: 0 — понедельник … 6 — воскресенье.
fn weekday(days: i64) -> i64 {
    (days + 3).rem_euclid(7)
}

/// Момент в UTC по местному стенному времени (`wall` — секунды с 1970 года «как если бы это был UTC»), когда смещение
/// пояса в каждый момент UTC даёт `offset_at`. Час, что повторяется осенью, берётся первый (до перевода часов); час,
/// которого весной нет, — по смещению до перехода (так велит RFC 5545).
pub fn wall_to_utc(wall: i64, offset_at: &dyn Fn(i64) -> i32) -> i64 {
    let before = i64::from(offset_at(wall - DAY));
    let after = i64::from(offset_at(wall + DAY));
    // Смещение за сутки до и за сутки после одно — перевода часов рядом нет, и вторая проверка дала бы тот же ответ.
    if before == after {
        return wall - before;
    }
    let (a, b) = (wall - before, wall - after);
    let good_a = i64::from(offset_at(a)) == before;
    let good_b = i64::from(offset_at(b)) == after;
    match (good_a, good_b) {
        (true, true) => a.min(b),
        (false, true) => b,
        _ => a,
    }
}

// ─── Типы ───────────────────────────────────────────────────────────────────

/// В каком поясе записано время.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Utc,
    /// Без пояса («плавающее»): по времени пояса этого ПК.
    Local,
    /// Пояс из `VTIMEZONE` того же файла (номер в таблице).
    Named(usize),
}

/// Время события: целый день или стенное время в поясе.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Time {
    /// Целый день: номер дня от 1970-01-01.
    Date(i64),
    /// Секунды от 1970 года по стенному времени пояса (`10:30` в Москве — как `10:30` в UTC).
    At(i64, Zone),
}

/// Служба видеосвязи, по сайту ссылки.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Service {
    Zoom,
    Meet,
    Teams,
    Telemost,
    Jazz,
    Jitsi,
    Webex,
    Whereby,
    Discord,
}

/// Проверенная ссылка встречи: только `https://` на сайт службы.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub service: Service,
}

/// Одна встреча: вхождение события в окне времени.
#[derive(Clone, Debug, PartialEq)]
pub struct Meeting {
    /// Номер события в календаре (для ключа плашки); у замены одного повтора тот же, что у всей серии.
    pub uid: String,
    pub title: String,
    /// Место (`LOCATION`) одной строкой; пусто — нет.
    pub place: String,
    /// Начало и конец — секунды с 1970 года, UTC. Конец равен началу, если длительности в файле нет.
    pub start: i64,
    pub end: i64,
    pub link: Option<Link>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Правило повторения (`RRULE`).
#[derive(Clone, Debug, PartialEq)]
struct Rule {
    freq: Freq,
    interval: i64,
    count: Option<u32>,
    until: Option<Time>,
    /// (номер в месяце, день недели 0–6); номер 0 — каждый такой день.
    by_day: Vec<(i64, i64)>,
    by_month_day: Vec<i64>,
    by_month: Vec<i64>,
    by_set_pos: Vec<i64>,
    /// С какого дня начинается неделя (для `WEEKLY` с шагом больше 1).
    wkst: i64,
    /// Те же списки битами, чтобы проверка дня не зависела от длины списка (`Rule::seal`): дни недели из `BYDAY` (бит 0 —
    /// понедельник), месяцы (бит `m`), дни месяца с начала (бит `d`) и с конца (бит `d` — день `-d`).
    weekdays: u8,
    months: u16,
    days_from_start: u32,
    days_from_end: u32,
}

/// Событие из файла (то, что нужно для показа; описание и участники не хранятся).
#[derive(Clone, Debug, PartialEq)]
struct Event {
    uid: String,
    title: String,
    place: String,
    start: Time,
    end: Option<Time>,
    /// `DURATION`, секунды.
    duration: Option<i64>,
    link: Option<Link>,
    rule: Option<Rule>,
    exdates: Vec<Time>,
    /// Замена одного повтора серии: какого именно.
    recurrence_id: Option<Time>,
    cancelled: bool,
    /// Адреса участников с `PARTSTAT=DECLINED` (маленькими буквами): по ним, когда файл разобран и имя календаря известно,
    /// решается, отклонил ли приглашение сам владелец (`Calendar::parse`).
    declined: Vec<String>,
}

/// Правила одного пояса из `VTIMEZONE`.
#[derive(Clone, Debug, PartialEq)]
struct TzRules {
    obs: Vec<Observance>,
    /// Переходы на годы вокруг «сейчас» файла (`TzRules::tabulate`): смещение рядом с окном берётся из готового списка, а не
    /// считается заново по всем наблюдениям пояса — иначе чужой пояс из десятков наблюдений тормозил бы каждый перевод времени.
    table: Option<Table>,
}

/// Переходы пояса на отрезке `lo..hi` (моменты UTC).
#[derive(Clone, Debug, PartialEq)]
struct Table {
    lo: i64,
    hi: i64,
    /// Смещение на `lo`.
    base: i32,
    /// Переходы внутри отрезка по возрастанию: (момент, смещение после него).
    steps: Vec<(i64, i32)>,
}

/// `STANDARD` или `DAYLIGHT`: с какого момента и по какому правилу пояс переходит на смещение `to`.
#[derive(Clone, Debug, PartialEq)]
struct Observance {
    /// Смещение до перехода и после, секунды.
    from: i32,
    to: i32,
    /// Первый переход — стенное время «до перехода».
    start: i64,
    rule: Option<Rule>,
    rdates: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq)]
struct ZoneDef {
    name: String,
    /// Нет `VTIMEZONE` в файле — `None`: время такого пояса читается как местное.
    rules: Option<TzRules>,
}

/// Календарь: события и пояса из одного файла.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Calendar {
    /// `X-WR-CALNAME`.
    pub name: Option<String>,
    events: Vec<Event>,
    zones: Vec<ZoneDef>,
    /// Событий в файле больше [`MAX_EVENTS`]: остальные пропущены.
    pub overflow: bool,
}

// ─── Строки файла ───────────────────────────────────────────────────────────

/// Строка файла: имя свойства, параметры (всё между первой `;` и двоеточием) и значение.
struct Raw<'a> {
    name: &'a str,
    params: &'a str,
    value: &'a str,
}

/// Делит строку по `sep` вне кавычек.
fn split_unquoted(s: &str, sep: char) -> Vec<&str> {
    let (mut out, mut start, mut quoted) = (Vec::new(), 0, false);
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == sep && !quoted {
            out.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&s[start..]);
    out
}

fn split_line(line: &str) -> Option<Raw<'_>> {
    let mut quoted = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let head = &line[..colon];
    let (name, params) = head.split_once(';').unwrap_or((head, ""));
    let name = name.trim();
    (!name.is_empty()).then_some(Raw { name, params, value: &line[colon + 1..] })
}

/// Значение параметра (`TZID`, `VALUE`) без кавычек.
fn param(params: &str, key: &str) -> Option<String> {
    split_unquoted(params, ';').into_iter().find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.trim().eq_ignore_ascii_case(key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// Текст из файла: `\n`, `\,`, `\;`, `\\`.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Одна строка для окна: управляющие знаки и переводы строк — пробелами, не длиннее `max` знаков.
fn tidy(s: &str, max: usize) -> String {
    let one: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    one.trim().chars().take(max).collect::<String>().trim_end().to_string()
}

/// Начало текста не длиннее `max` байт (по границе знака).
fn head(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Есть ли `needle` в `hay` без учёта регистра ASCII.
fn has_ci(hay: &str, needle: &str) -> bool {
    find_ci(hay.as_bytes(), needle.as_bytes()).is_some()
}

// ─── Время и правила из файла ───────────────────────────────────────────────

/// `20261027T103000`, `20261027T073000Z` или `20261027`: стенные секунды, «целый день», «в UTC».
fn parse_stamp(v: &str) -> Option<(i64, bool, bool)> {
    let b = v.trim().as_bytes();
    let num = |s: &[u8]| -> Option<i64> {
        if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(s).ok()?.parse().ok()
    };
    let (date_only, utc) = match b.len() {
        8 => (true, false),
        15 if b[8] == b'T' => (false, false),
        16 if b[8] == b'T' && b[15] == b'Z' => (false, true),
        _ => return None,
    };
    let (y, mo, d) = (num(&b[0..4])?, num(&b[4..6])?, num(&b[6..8])?);
    if !(1601..=9999).contains(&y) || !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) {
        return None;
    }
    let days = days_from_civil(y, mo, d);
    if date_only {
        return Some((days * DAY, true, false));
    }
    let (h, mi, s) = (num(&b[9..11])?, num(&b[11..13])?, num(&b[13..15])?);
    if h > 23 || mi > 59 || s > 60 {
        return None;
    }
    Some((days * DAY + h * 3600 + mi * 60 + s.min(59), false, utc))
}

/// Пояса из `TZID` этого файла: имя → номер; правила появятся, когда встретится `VTIMEZONE` с таким именем. Поясов не больше
/// [`MAX_ZONES`]: сверх них время читается как местное.
#[derive(Default)]
struct ZoneTable {
    list: Vec<ZoneDef>,
}

impl ZoneTable {
    fn intern(&mut self, name: &str) -> Option<usize> {
        let name = tidy(name, MAX_ZONE_NAME);
        if let Some(i) = self.list.iter().position(|z| z.name.eq_ignore_ascii_case(&name)) {
            return Some(i);
        }
        if self.list.len() >= MAX_ZONES {
            return None;
        }
        self.list.push(ZoneDef { name, rules: None });
        Some(self.list.len() - 1)
    }
}

/// Пояс времени из параметров свойства: `TZID` (имя из файла) или местное — пояса нет, имя пустое, поясов в файле уже слишком много.
fn zone_of(params: &str, zones: &mut ZoneTable) -> Zone {
    param(params, "TZID").filter(|id| !id.is_empty()).and_then(|id| zones.intern(&id)).map_or(Zone::Local, Zone::Named)
}

/// Время значения в поясе `zone`: целый день (8 цифр), UTC (`Z`) или стенное время этого пояса.
fn time_in(zone: impl FnOnce() -> Zone, value: &str) -> Option<Time> {
    let (secs, date, utc) = parse_stamp(value)?;
    if date {
        return Some(Time::Date(secs.div_euclid(DAY)));
    }
    Some(Time::At(secs, if utc { Zone::Utc } else { zone() }))
}

/// Время свойства: целый день (`VALUE=DATE` или 8 цифр), UTC (`Z`), пояс из `TZID` или плавающее.
fn parse_time(params: &str, value: &str, zones: &mut ZoneTable) -> Option<Time> {
    time_in(|| zone_of(params, zones), value)
}

/// `PT1H30M`, `P1D`, `P1W`, `-PT15M` → секунды.
fn parse_duration(v: &str) -> Option<i64> {
    let v = v.trim();
    let (neg, v) = match v.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix('P')?;
    let (mut total, mut digits, mut in_time) = (0i64, String::new(), false);
    for c in v.chars() {
        let unit = match (c, in_time) {
            ('T', _) => {
                in_time = true;
                continue;
            }
            ('0'..='9', _) => {
                digits.push(c);
                continue;
            }
            ('W', false) => 7 * DAY,
            ('D', false) => DAY,
            ('H', true) => 3600,
            ('M', true) => 60,
            ('S', true) => 1,
            _ => return None,
        };
        let n: i64 = digits.parse().ok().filter(|n| *n <= 1_000_000)?;
        digits.clear();
        total += n * unit;
    }
    digits.is_empty().then_some(if neg { -total } else { total })
}

/// `+0300`, `-0530`, `+053000` → секунды.
fn parse_offset(v: &str) -> Option<i32> {
    let v = v.trim();
    let (sign, digits) = match v.as_bytes().first()? {
        b'+' => (1, &v[1..]),
        b'-' => (-1, &v[1..]),
        _ => return None,
    };
    if !(digits.len() == 4 || digits.len() == 6) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| digits.get(r)?.parse::<i32>().ok();
    let secs = n(0..2)? * 3600 + n(2..4)? * 60 + if digits.len() == 6 { n(4..6)? } else { 0 };
    Some(sign * secs)
}

fn parse_weekday(s: &str) -> Option<i64> {
    ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|d| d.eq_ignore_ascii_case(s)).map(|i| i as i64)
}

/// Добавить в список без повторов; список полон — `None` (правило негодное).
fn push_unique<T: PartialEq>(list: &mut Vec<T>, item: T, max: usize) -> Option<()> {
    if !list.contains(&item) {
        if list.len() >= max {
            return None;
        }
        list.push(item);
    }
    Some(())
}

/// `FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10` → правило. Частота, которой панель не знает (каждый час), — `None`: повторов нет. Списки
/// длиннее пределов (`BYDAY` — 64 разных, `BYMONTHDAY` — 62, `BYSETPOS` — 32) и числа вне допустимого — тоже `None`.
fn parse_rule(value: &str) -> Option<Rule> {
    let mut rule = Rule {
        freq: Freq::Daily,
        interval: 1,
        count: None,
        until: None,
        by_day: Vec::new(),
        by_month_day: Vec::new(),
        by_month: Vec::new(),
        by_set_pos: Vec::new(),
        wkst: 0,
        weekdays: 0,
        months: 0,
        days_from_start: 0,
        days_from_end: 0,
    };
    let mut freq = None;
    for part in value.split(';') {
        let Some((key, val)) = part.split_once('=') else { continue };
        let val = val.trim();
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                freq = match val.to_ascii_uppercase().as_str() {
                    "DAILY" => Some(Freq::Daily),
                    "WEEKLY" => Some(Freq::Weekly),
                    "MONTHLY" => Some(Freq::Monthly),
                    "YEARLY" => Some(Freq::Yearly),
                    _ => return None,
                }
            }
            "INTERVAL" => rule.interval = val.parse::<i64>().ok().filter(|i| (1..=MAX_INTERVAL).contains(i))?,
            "COUNT" => rule.count = Some(val.parse::<u32>().ok().filter(|c| *c >= 1)?),
            "UNTIL" => {
                let (secs, date, utc) = parse_stamp(val)?;
                rule.until = Some(match (date, utc) {
                    (true, _) => Time::Date(secs.div_euclid(DAY)),
                    (false, true) => Time::At(secs, Zone::Utc),
                    (false, false) => Time::At(secs, Zone::Local),
                });
            }
            "BYDAY" => {
                for item in val.split(',') {
                    let item = item.trim();
                    let at = item.len().saturating_sub(2);
                    let (num, day) = (item.get(..at)?, parse_weekday(item.get(at..)?)?);
                    let n = if num.is_empty() { 0 } else { num.trim_start_matches('+').parse::<i64>().ok()? };
                    if !(-53..=53).contains(&n) {
                        return None;
                    }
                    push_unique(&mut rule.by_day, (n, day), MAX_BY_DAY)?;
                }
            }
            "BYMONTHDAY" => {
                for item in val.split(',') {
                    let d = item.trim().parse::<i64>().ok().filter(|d| *d != 0 && (-31..=31).contains(d))?;
                    push_unique(&mut rule.by_month_day, d, MAX_BY_MONTH_DAY)?;
                }
            }
            "BYMONTH" => {
                for item in val.split(',') {
                    let m = item.trim().parse::<i64>().ok().filter(|m| (1..=12).contains(m))?;
                    push_unique(&mut rule.by_month, m, 12)?;
                }
            }
            "BYSETPOS" => {
                for item in val.split(',') {
                    let p = item.trim().parse::<i64>().ok().filter(|p| *p != 0 && (-366..=366).contains(p))?;
                    push_unique(&mut rule.by_set_pos, p, MAX_BY_SET_POS)?;
                }
            }
            "WKST" => rule.wkst = parse_weekday(val)?,
            _ => {}
        }
    }
    rule.freq = freq?;
    rule.seal();
    Some(rule)
}

impl Rule {
    /// После разбора: месяцы по порядку и те же списки битами, чтобы проверка дня не зависела от длины списка.
    fn seal(&mut self) {
        self.by_month.sort_unstable();
        for (_, wd) in &self.by_day {
            self.weekdays |= 1u8 << *wd;
        }
        for m in &self.by_month {
            self.months |= 1u16 << *m;
        }
        for d in &self.by_month_day {
            if *d > 0 {
                self.days_from_start |= 1u32 << *d;
            } else {
                self.days_from_end |= 1u32 << -*d;
            }
        }
    }

    /// Подходит ли день `d` по дням недели из `BYDAY` (списка нет — любой).
    fn weekday_ok(&self, d: i64) -> bool {
        self.weekdays == 0 || self.weekdays >> weekday(d) & 1 == 1
    }

    /// Подходит ли месяц `m` по `BYMONTH` (списка нет — любой).
    fn month_ok(&self, m: i64) -> bool {
        self.months == 0 || self.months >> m & 1 == 1
    }

    /// Сколько вхождений в одном периоде не бывает больше (верхняя оценка): по ней видно, что счёт `COUNT` до окна не кончится.
    fn max_per_period(&self) -> i64 {
        let by_days: i64 = self.by_day.iter().map(|(ord, _)| if *ord == 0 { 5 } else { 1 }).sum();
        let month = if !self.by_month_day.is_empty() {
            self.by_month_day.len() as i64
        } else if !self.by_day.is_empty() {
            by_days
        } else {
            1
        };
        let most = match self.freq {
            Freq::Daily => 1,
            Freq::Weekly => {
                if self.weekdays == 0 {
                    1
                } else {
                    i64::from(self.weekdays.count_ones())
                }
            }
            Freq::Monthly => month,
            Freq::Yearly => self.by_month.len().max(1) as i64 * month,
        };
        let most = if self.by_set_pos.is_empty() { most } else { most.min(self.by_set_pos.len() as i64) };
        most.clamp(1, 366)
    }

    /// Во сколько шагов обходится один период: чем длиннее списки правила, тем дороже. По этой цене серии тратят свой запас
    /// шагов ([`SERIES_STEPS`]).
    fn period_cost(&self) -> u32 {
        let by_days: u32 = self.by_day.iter().map(|(ord, _)| if *ord == 0 { 5 } else { 1 }).sum();
        let month = 2 + if self.by_month_day.is_empty() { by_days.max(1) } else { self.by_month_day.len() as u32 };
        let per = match self.freq {
            Freq::Daily => 2,
            Freq::Weekly => 8,
            Freq::Monthly => month,
            Freq::Yearly => 2 + self.by_month.len().max(1) as u32 * month,
        };
        4 + per + self.by_set_pos.len() as u32
    }
}

// ─── Повторы ────────────────────────────────────────────────────────────────

/// Дни месяца `y-m`, что подходят правилу (`MONTHLY`, а для `YEARLY` — каждого из месяцев), по возрастанию. Нет ни дней
/// месяца, ни дней недели — день начала серии (`dom0`); его нет в месяце (31-е в апреле) — месяц пропускается.
fn month_days(rule: &Rule, y: i64, m: i64, dom0: i64) -> Vec<i64> {
    let n = days_in_month(y, m);
    let first = days_from_civil(y, m, 1);
    let mut out = Vec::new();
    if !rule.by_month_day.is_empty() {
        for md in &rule.by_month_day {
            let day = if *md > 0 { *md } else { n + 1 + md };
            if (1..=n).contains(&day) && rule.weekday_ok(first + day - 1) {
                out.push(first + day - 1);
            }
        }
    } else if !rule.by_day.is_empty() {
        for (ord, wd) in &rule.by_day {
            // Первый такой день недели месяца — через `off` дней от первого числа; всего таких дней 4 или 5.
            let off = (wd - weekday(first)).rem_euclid(7);
            let count = (n - off + 6) / 7;
            let at = |i: i64| first + off + 7 * i;
            match *ord {
                0 => out.extend((0..count).map(at)),
                o if o > 0 && o <= count => out.push(at(o - 1)),
                o if o < 0 && -o <= count => out.push(at(count + o)),
                _ => {}
            }
        }
    } else if dom0 <= n {
        out.push(first + dom0 - 1);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Из отсортированных дней периода — только на местах `BYSETPOS` (счёт с единицы, минус — с конца).
fn set_pos(days: Vec<i64>, pos: &[i64]) -> Vec<i64> {
    if pos.is_empty() {
        return days;
    }
    let mut out: Vec<i64> = pos
        .iter()
        .filter_map(|p| {
            let i = if *p > 0 { *p as usize - 1 } else { days.len().checked_sub(p.unsigned_abs() as usize)? };
            days.get(i).copied()
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Начало недели (по `WKST`), в которой лежит день `d`.
fn week_start(d: i64, wkst: i64) -> i64 {
    d - (weekday(d) - wkst).rem_euclid(7)
}

/// Первый день периода номер `k` (период — день, неделя, месяц или год от начала серии `d0`, с шагом `interval`).
fn period_first(rule: &Rule, d0: i64, k: i64) -> Option<i64> {
    let step = k.checked_mul(rule.interval)?;
    Some(match rule.freq {
        Freq::Daily => d0.checked_add(step)?,
        Freq::Weekly => week_start(d0, rule.wkst).checked_add(step.checked_mul(7)?)?,
        Freq::Monthly => {
            let (y0, m0, _) = civil_from_days(d0);
            let mi = y0 * 12 + (m0 - 1) + step;
            days_from_civil(mi.div_euclid(12), mi.rem_euclid(12) + 1, 1)
        }
        Freq::Yearly => days_from_civil(civil_from_days(d0).0 + step, 1, 1),
    })
}

/// Номер периода, в который попадает день `day` (раньше начала серии — отрицательный).
fn period_index(rule: &Rule, d0: i64, day: i64) -> i64 {
    let step = rule.interval;
    match rule.freq {
        Freq::Daily => (day - d0).div_euclid(step),
        Freq::Weekly => (day - week_start(d0, rule.wkst)).div_euclid(7).div_euclid(step),
        Freq::Monthly => {
            let ((y0, m0, _), (y, m, _)) = (civil_from_days(d0), civil_from_days(day));
            ((y * 12 + m) - (y0 * 12 + m0)).div_euclid(step)
        }
        Freq::Yearly => (civil_from_days(day).0 - civil_from_days(d0).0).div_euclid(step),
    }
}

/// Дни периода номер `k`, что подходят правилу (уже без тех, что раньше начала серии `d0`), по возрастанию.
fn period_days(rule: &Rule, d0: i64, k: i64) -> Vec<i64> {
    let Some(first) = period_first(rule, d0, k) else { return Vec::new() };
    let (_, m0, dom0) = civil_from_days(d0);
    let days: Vec<i64> = match rule.freq {
        Freq::Daily => {
            let (y, m, d) = civil_from_days(first);
            let by_month_day = (rule.days_from_start == 0 && rule.days_from_end == 0)
                || rule.days_from_start >> d & 1 == 1
                || rule.days_from_end >> (days_in_month(y, m) + 1 - d) & 1 == 1;
            if rule.weekday_ok(first) && rule.month_ok(m) && by_month_day { vec![first] } else { Vec::new() }
        }
        Freq::Weekly => {
            let want = |d: i64| if rule.by_day.is_empty() { weekday(d) == weekday(d0) } else { rule.weekday_ok(d) };
            let month_ok = |d: i64| rule.months == 0 || rule.month_ok(civil_from_days(d).1);
            (first..first + 7).filter(|d| want(*d) && month_ok(*d)).collect()
        }
        Freq::Monthly => {
            let (y, m, _) = civil_from_days(first);
            if rule.month_ok(m) { month_days(rule, y, m, dom0) } else { Vec::new() }
        }
        Freq::Yearly => {
            let y = civil_from_days(first).0;
            let mut all: Vec<i64> = Vec::new();
            if rule.by_month.is_empty() {
                all.extend(month_days(rule, y, m0, dom0));
            } else {
                for m in &rule.by_month {
                    all.extend(month_days(rule, y, *m, dom0));
                }
            }
            all.sort_unstable();
            all.dedup();
            all
        }
    };
    set_pos(days, &rule.by_set_pos).into_iter().filter(|d| *d >= d0).collect()
}

/// С какого периода начинать, чтобы не перебирать годы до окна: со стоящего перед окном (на один раньше — с запасом). Со
/// `COUNT` считать надо с самого начала серии — если только счёт не может кончиться до конца окна, даже когда в каждом
/// периоде самое большое число дней: тогда он окну не мешает, и серия идёт как бесконечная.
fn first_period(rule: &Rule, d0: i64, lo_day: i64, hi_day: i64) -> i64 {
    if let Some(count) = rule.count {
        let periods = period_index(rule, d0, hi_day).saturating_add(1).max(0);
        if i64::from(count) < periods.saturating_mul(rule.max_per_period()) {
            return 0;
        }
    }
    (period_index(rule, d0, lo_day) - 1).max(0)
}

/// Вхождения правила от `start` (стенные секунды первого вхождения) в окне `lo..=hi` (тоже стенные): для каждого — `emit`
/// (вернул ложь — хватит). `until` — тоже стенное. Правило из чужого файла не вечно: серия тратит не больше [`SERIES_STEPS`]
/// шагов, потом считается кончившейся; запас у каждой серии свой, чужие серии его не расходуют. Ответ — сколько шагов потрачено.
fn expand(rule: &Rule, start: i64, until: Option<i64>, lo: i64, hi: i64, emit: &mut dyn FnMut(i64) -> bool) -> u32 {
    let (d0, tod) = (start.div_euclid(DAY), start.rem_euclid(DAY));
    let (lo_day, hi_day) = (lo.div_euclid(DAY) - 1, hi.div_euclid(DAY) + 1);
    let (cost, end_day) = (rule.period_cost(), days_from_civil(10_000, 1, 1));
    let mut left = SERIES_STEPS;
    let mut seen = 0u32;
    for k in first_period(rule, d0, lo_day, hi_day).. {
        if left < cost {
            break;
        }
        left -= cost;
        match period_first(rule, d0, k) {
            Some(first) if first <= hi_day && first < end_day => {}
            _ => break,
        }
        for day in period_days(rule, d0, k) {
            seen = seen.saturating_add(1);
            if rule.count.is_some_and(|c| seen > c) || day > hi_day {
                return SERIES_STEPS - left;
            }
            let wall = day * DAY + tod;
            if until.is_some_and(|u| wall > u) {
                return SERIES_STEPS - left;
            }
            if (lo..=hi).contains(&wall) && !emit(wall) {
                return SERIES_STEPS - left;
            }
        }
    }
    SERIES_STEPS - left
}

// ─── Часовые пояса файла ────────────────────────────────────────────────────

impl Observance {
    /// `UNTIL` правила пояса — в UTC; стенное время «до перехода» — со смещением до перехода.
    fn until_of(&self, rule: &Rule) -> Option<i64> {
        rule.until.map(|u| match u {
            Time::Date(d) => (d + 1) * DAY - 1,
            Time::At(secs, Zone::Utc) => secs + i64::from(self.from),
            Time::At(secs, _) => secs,
        })
    }

    /// Все переходы наблюдения от `lo` (включая) до `hi` — моменты UTC.
    fn onsets(&self, lo: i64, hi: i64, emit: &mut dyn FnMut(i64)) {
        let from = i64::from(self.from);
        let mut take = |wall: i64| {
            let at = wall - from;
            if (lo..hi).contains(&at) {
                emit(at);
            }
        };
        take(self.start);
        let first = self.rdates.partition_point(|r| *r - from < lo);
        for r in self.rdates[first..].iter().take_while(|r| **r - from < hi) {
            take(*r);
        }
        if let Some(rule) = &self.rule {
            expand(rule, self.start, self.until_of(rule), lo + from, hi + from, &mut |w| {
                take(w);
                true
            });
        }
    }

    /// Стенное время (до перехода) последнего перехода, что не позже `hi_wall`.
    fn latest_onset(&self, hi_wall: i64) -> Option<i64> {
        let mut best: Option<i64> = None;
        let mut take = |w: i64| {
            if w <= hi_wall {
                best = Some(best.map_or(w, |b| b.max(w)));
            }
        };
        take(self.start);
        // `RDATE` по возрастанию (`Parser::end`): последний не позже `hi_wall` находится делением пополам.
        if let Some(i) = self.rdates.partition_point(|r| *r <= hi_wall).checked_sub(1) {
            take(self.rdates[i]);
        }
        if let Some(rule) = &self.rule {
            // Переходы бывают раз в год (шаг правила — лет), поэтому смотрим на пару шагов назад.
            let span = (rule.interval + 1) * 366 * DAY;
            // Правило пояса — годовое без счёта (см. `obs_prop`): до последнего перехода считанные шаги.
            expand(rule, self.start, self.until_of(rule), hi_wall - span, hi_wall, &mut |w| {
                take(w);
                true
            });
        }
        best
    }
}

/// Переходов в готовом списке не больше (чужой пояс с тысячами дат остаётся без списка и считается по наблюдениям).
const MAX_STEPS: usize = 4096;

impl TzRules {
    /// Смещение пояса от UTC в секундах на момент UTC `utc`: то, что поставил последний переход.
    fn offset_at(&self, utc: i64) -> i32 {
        if let Some(t) = &self.table
            && (t.lo..t.hi).contains(&utc)
        {
            return match t.steps.partition_point(|(at, _)| *at <= utc).checked_sub(1) {
                Some(i) => t.steps[i].1,
                None => t.base,
            };
        }
        self.offset_slow(utc)
    }

    /// Составить список переходов на три года вокруг `now`: прошлый, нынешний и следующий. Те же ответы, что даёт
    /// [`TzRules::offset_slow`] (проверяется тестом), только без обхода всех наблюдений на каждый вопрос.
    fn tabulate(&mut self, now: i64) {
        let year = civil_from_days(now.div_euclid(DAY)).0;
        let (lo, hi) = (days_from_civil(year - 1, 1, 1) * DAY, days_from_civil(year + 2, 1, 1) * DAY);
        let mut steps: Vec<(i64, i32)> = Vec::new();
        for o in &self.obs {
            o.onsets(lo, hi, &mut |at| steps.push((at, o.to)));
            if steps.len() > MAX_STEPS {
                return;
            }
        }
        // По времени; два перехода в один миг — первый по порядку наблюдений (как в `offset_slow`).
        steps.sort_by_key(|s| s.0);
        steps.dedup_by_key(|s| s.0);
        self.table = Some(Table { lo, hi, base: self.offset_slow(lo), steps });
    }

    /// То же по наблюдениям: у каждого — его последний переход, побеждает самый поздний.
    fn offset_slow(&self, utc: i64) -> i32 {
        let mut best: Option<(i64, i32)> = None;
        for o in &self.obs {
            if let Some(w) = o.latest_onset(utc + i64::from(o.from)) {
                let onset = w - i64::from(o.from);
                if best.is_none_or(|(b, _)| onset > b) {
                    best = Some((onset, o.to));
                }
            }
        }
        match best {
            Some((_, to)) => to,
            // Раньше самого раннего перехода: смещение до него.
            None => self.obs.iter().min_by_key(|o| o.start).map_or(0, |o| o.from),
        }
    }
}

// ─── Разбор файла ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Calendar,
    Event,
    Zone,
    Observance,
    Other,
}

#[derive(Default)]
struct EventB {
    uid: String,
    title: String,
    place: String,
    start: Option<Time>,
    end: Option<Time>,
    duration: Option<i64>,
    /// Ссылка из каждого места: выбирается по порядку `URL`, `LOCATION`, службы `X-…`, `DESCRIPTION`.
    url_link: Option<Link>,
    place_link: Option<Link>,
    x_link: Option<Link>,
    text_link: Option<Link>,
    rule: Option<Rule>,
    exdates: Vec<Time>,
    recurrence_id: Option<Time>,
    cancelled: bool,
    declined: Vec<String>,
}

#[derive(Default)]
struct ObsB {
    from: Option<i32>,
    to: Option<i32>,
    start: Option<i64>,
    rule: Option<Rule>,
    rdates: Vec<i64>,
}

struct Parser {
    /// Что кончилось раньше этого момента (с запасом в двое суток) — не нужно, и в счёт событий не идёт.
    keep_from: i64,
    saw_calendar: bool,
    name: Option<String>,
    events: Vec<Event>,
    overflow: bool,
    zones: ZoneTable,
    stack: Vec<Scope>,
    event: Option<EventB>,
    zone_id: Option<String>,
    zone_obs: Vec<Observance>,
    obs: Option<ObsB>,
}

/// Правило перехода пояса: годовое, без счёта и с короткими списками (настоящие правила — «последнее воскресенье марта»).
fn zone_rule_ok(rule: &Rule) -> bool {
    rule.freq == Freq::Yearly
        && rule.count.is_none()
        && rule.by_day.len() <= 8
        && rule.by_month_day.len() <= 31
        && rule.by_month.len() <= 4
        && rule.by_set_pos.len() <= 4
}

impl Parser {
    fn line(&mut self, line: &str) {
        let Some(raw) = split_line(line) else { return };
        let name = raw.name.to_ascii_uppercase();
        // Длинное значение (вложение, мусор) пропускается целиком; описание — нет, но ссылку в нём ищут лишь в начале.
        if raw.params.len() > MAX_VALUE || (raw.value.len() > MAX_VALUE && name != "DESCRIPTION") {
            return;
        }
        match name.as_str() {
            "BEGIN" => self.begin(raw.value.trim()),
            "END" => self.end(),
            _ => match self.stack.last() {
                Some(Scope::Calendar) if name == "X-WR-CALNAME" => {
                    self.name = Some(tidy(&unescape(raw.value), MAX_NAME)).filter(|n| !n.is_empty());
                }
                Some(Scope::Event) => self.event_prop(&name, &raw),
                Some(Scope::Zone) if name == "TZID" => self.zone_id = Some(raw.value.trim().to_string()),
                Some(Scope::Observance) => self.obs_prop(&name, &raw),
                _ => {}
            },
        }
    }

    fn begin(&mut self, kind: &str) {
        let kind = kind.to_ascii_uppercase();
        let parent = self.stack.last().copied();
        let scope = match (kind.as_str(), parent) {
            ("VCALENDAR", _) => {
                self.saw_calendar = true;
                Scope::Calendar
            }
            ("VEVENT", Some(Scope::Calendar)) => {
                self.event = Some(EventB::default());
                Scope::Event
            }
            ("VTIMEZONE", Some(Scope::Calendar)) => {
                self.zone_id = None;
                self.zone_obs.clear();
                Scope::Zone
            }
            ("STANDARD" | "DAYLIGHT", Some(Scope::Zone)) => {
                self.obs = Some(ObsB::default());
                Scope::Observance
            }
            _ => Scope::Other,
        };
        self.stack.push(scope);
    }

    fn end(&mut self) {
        match self.stack.pop() {
            Some(Scope::Event) => self.end_event(),
            Some(Scope::Observance) => {
                if let Some(mut b) = self.obs.take()
                    && let (Some(from), Some(to), Some(start)) = (b.from, b.to, b.start)
                    && self.zone_obs.len() < MAX_OBSERVANCES
                {
                    b.rdates.sort_unstable();
                    b.rdates.dedup();
                    self.zone_obs.push(Observance { from, to, start, rule: b.rule, rdates: b.rdates });
                }
            }
            Some(Scope::Zone) => {
                let obs = std::mem::take(&mut self.zone_obs);
                if let Some(id) = self.zone_id.take().filter(|id| !id.is_empty())
                    && !obs.is_empty()
                    && let Some(i) = self.zones.intern(&id)
                {
                    self.zones.list[i].rules = Some(TzRules { obs, table: None });
                }
            }
            _ => {}
        }
    }

    fn obs_prop(&mut self, name: &str, raw: &Raw<'_>) {
        let Some(b) = self.obs.as_mut() else { return };
        match name {
            "DTSTART" => b.start = parse_stamp(raw.value).map(|(secs, _, _)| secs),
            "TZOFFSETFROM" => b.from = parse_offset(raw.value),
            "TZOFFSETTO" => b.to = parse_offset(raw.value),
            // Правила поясов — годовые, без счёта и с короткими списками; другие (чужой файл) не читаются: пояс тогда стоит
            // по первому переходу и `RDATE`.
            "RRULE" => b.rule = parse_rule(raw.value).filter(zone_rule_ok),
            "RDATE" => {
                for v in raw.value.split(',') {
                    if b.rdates.len() >= MAX_RDATES {
                        break;
                    }
                    if let Some((secs, _, _)) = parse_stamp(v) {
                        b.rdates.push(secs);
                    }
                }
            }
            _ => {}
        }
    }

    fn event_prop(&mut self, name: &str, raw: &Raw<'_>) {
        let Some(b) = self.event.as_mut() else { return };
        match name {
            "UID" => b.uid = raw.value.trim().chars().take(200).collect(),
            "SUMMARY" => b.title = tidy(&unescape(raw.value), MAX_TITLE),
            "LOCATION" => {
                let text = unescape(raw.value);
                b.place_link = find_link(&text);
                b.place = tidy(&text, MAX_PLACE);
            }
            "URL" => b.url_link = find_link(raw.value),
            "DESCRIPTION" => b.text_link = find_link(&unescape(head(raw.value, MAX_DESCRIPTION))),
            // Службы, что кладут ссылку на встречу в своё свойство.
            "X-GOOGLE-CONFERENCE"
            | "X-MICROSOFT-SKYPETEAMSMEETINGURL"
            | "X-MICROSOFT-ONLINEMEETINGEXTERNALLINK"
            | "X-MICROSOFT-ONLINEMEETINGCONFLINK" => {
                if b.x_link.is_none() {
                    b.x_link = find_link(raw.value);
                }
            }
            "STATUS" => b.cancelled = raw.value.trim().eq_ignore_ascii_case("CANCELLED"),
            "DTSTART" => b.start = parse_time(raw.params, raw.value, &mut self.zones),
            "DTEND" => b.end = parse_time(raw.params, raw.value, &mut self.zones),
            "DURATION" => b.duration = parse_duration(raw.value),
            "RRULE" => b.rule = parse_rule(raw.value),
            // Пояс исключений — один на свойство (не заново для каждого значения); исключений у события не больше `MAX_EXDATES`.
            "EXDATE" => {
                let mut zone = None;
                for v in raw.value.split(',') {
                    if b.exdates.len() >= MAX_EXDATES {
                        break;
                    }
                    if let Some(t) = time_in(|| *zone.get_or_insert_with(|| zone_of(raw.params, &mut self.zones)), v) {
                        b.exdates.push(t);
                    }
                }
            }
            "ATTENDEE" => {
                if b.declined.len() < MAX_DECLINED
                    && has_ci(raw.params, "DECLINED")
                    && param(raw.params, "PARTSTAT").is_some_and(|v| v.eq_ignore_ascii_case("DECLINED"))
                {
                    let v = raw.value.trim();
                    if v.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("mailto:")) {
                        b.declined.push(v[7..].trim().to_ascii_lowercase());
                    }
                }
            }
            "RECURRENCE-ID" => b.recurrence_id = parse_time(raw.params, raw.value, &mut self.zones),
            _ => {}
        }
    }

    fn end_event(&mut self) {
        let Some(b) = self.event.take() else { return };
        // Событие на целый день — не встреча со временем: не нужно.
        let Some(start @ Time::At(..)) = b.start else { return };
        let event = Event {
            uid: b.uid,
            title: b.title,
            place: b.place,
            start,
            end: b.end,
            duration: b.duration,
            link: b.url_link.or(b.place_link).or(b.x_link).or(b.text_link),
            rule: b.rule,
            exdates: b.exdates,
            recurrence_id: b.recurrence_id,
            cancelled: b.cancelled,
            declined: b.declined,
        };
        // Что кончилось давно — не нужно. Отменённое одиночное событие и отменённая серия ничего не покажут и ничего не
        // заменяют; а отменённая замена одного повтора нужна: по ней вхождение серии убирается.
        if !event.is_current_or_future(self.keep_from) || (event.cancelled && event.recurrence_id.is_none()) {
            return;
        }
        if self.events.len() >= MAX_EVENTS {
            self.overflow = true;
        } else {
            self.events.push(event);
        }
    }
}

impl Event {
    /// Может ли что-то от этого события быть позже `keep_from` (с запасом в двое суток на пояс): серия без конца — да,
    /// остальное — по концу последнего вхождения (замена одного повтора — и по тому повтору, что она заменяет).
    fn is_current_or_future(&self, keep_from: i64) -> bool {
        let wall = |t: Time| match t {
            Time::Date(d) => d * DAY,
            Time::At(secs, _) => secs,
        };
        let slack = 2 * DAY;
        if let Some(rule) = &self.rule {
            return rule.until.is_none_or(|u| wall(u) + slack >= keep_from);
        }
        let start = wall(self.start);
        let end = match (self.end, self.duration) {
            (Some(e), _) => wall(e).max(start),
            (None, Some(d)) => start + d.max(0),
            (None, None) => start,
        };
        let latest = self.recurrence_id.map_or(end, |r| end.max(wall(r)));
        latest + slack >= keep_from
    }
}

impl Calendar {
    /// Разобрать файл. `keep_from` — «сейчас» (секунды с 1970): что кончилось больше двух суток назад, не хранится, чтобы
    /// старые годы календаря не съедали предел событий. Не календарь (нет `BEGIN:VCALENDAR`) — `None`.
    pub fn parse(text: &str, keep_from: i64) -> Option<Calendar> {
        let mut p = Parser {
            keep_from,
            saw_calendar: false,
            name: None,
            events: Vec::new(),
            overflow: false,
            zones: ZoneTable::default(),
            stack: Vec::new(),
            event: None,
            zone_id: None,
            zone_obs: Vec::new(),
            obs: None,
        };
        // Перенесённые строки склеиваются: следующая начинается пробелом или табуляцией.
        let mut buf = String::new();
        for line in text.trim_start_matches('\u{feff}').split(['\r', '\n']).filter(|l| !l.is_empty()) {
            if (line.starts_with(' ') || line.starts_with('\t')) && !buf.is_empty() {
                buf.push_str(&line[1..]);
                continue;
            }
            if !buf.is_empty() {
                p.line(&buf);
                buf.clear();
            }
            buf.push_str(line);
        }
        if !buf.is_empty() {
            p.line(&buf);
        }
        // Отклонённое владельцем приглашение (его адрес — имя основного календаря Google) — не встреча: событие пропадает, а
        // отклонённый один повтор серии становится отменой этого повтора.
        let me = p.name.as_deref().map(str::to_ascii_lowercase);
        let mut events = p.events;
        events.retain_mut(|e| {
            let declined = std::mem::take(&mut e.declined);
            if me.as_ref().is_some_and(|me| declined.contains(me)) {
                e.cancelled = true;
                return e.recurrence_id.is_some();
            }
            true
        });
        // Пояса с готовым списком переходов на годы вокруг «сейчас».
        let mut zones = p.zones.list;
        for rules in zones.iter_mut().filter_map(|z| z.rules.as_mut()) {
            rules.tabulate(keep_from);
        }
        p.saw_calendar.then_some(Calendar { name: p.name, events, zones, overflow: p.overflow })
    }

    /// Пояса, что названы в событиях, а `VTIMEZONE` для них в файле нет: их время читается как местное.
    pub fn unknown_zones(&self) -> Vec<String> {
        self.zones.iter().filter(|z| z.rules.is_none()).map(|z| z.name.clone()).collect()
    }

    /// Момент события в UTC; целый день — `None`.
    fn utc_of(&self, t: Time, local: LocalFn<'_>) -> Option<i64> {
        match t {
            Time::Date(_) => None,
            Time::At(w, Zone::Utc) => Some(w),
            Time::At(w, Zone::Named(i)) => match self.zones.get(i).and_then(|z| z.rules.as_ref()) {
                Some(rules) => Some(wall_to_utc(w, &|u| rules.offset_at(u))),
                None => Some(wall_to_utc(w, local)),
            },
            Time::At(w, Zone::Local) => Some(wall_to_utc(w, local)),
        }
    }

    /// Смещение пояса `zone` на момент UTC `utc`, секунды.
    fn offset_in(&self, zone: Zone, utc: i64, local: LocalFn<'_>) -> i64 {
        match zone {
            Zone::Utc => 0,
            Zone::Named(i) => match self.zones.get(i).and_then(|z| z.rules.as_ref()) {
                Some(rules) => i64::from(rules.offset_at(utc)),
                None => i64::from(local(utc)),
            },
            Zone::Local => i64::from(local(utc)),
        }
    }

    /// Встречи, что идут в окне `from..to` (секунды с 1970, UTC) или начнутся в нём: повторы развёрнуты, исключения и
    /// отмены учтены, замены одного повтора поставлены на место. По началу, потом по концу и названию. Событие длиннее
    /// месяца — не встреча; у серии в окне не больше [`MAX_PER_SERIES`] вхождений, а перебор её правила — не больше
    /// [`SERIES_STEPS`] шагов: чужой файл не растягивает ни время, ни память.
    pub fn meetings(&self, from: i64, to: i64, local: LocalFn<'_>) -> Vec<Meeting> {
        // Какие повторы каких серий заменены отдельными событиями: (номер серии, начало повтора по UTC).
        let mut replaced: HashSet<(&str, i64)> = HashSet::new();
        for ev in &self.events {
            if let Some(rid) = ev.recurrence_id
                && !ev.uid.is_empty()
                && let Some(at) = self.utc_of(rid, local)
            {
                replaced.insert((ev.uid.as_str(), at));
            }
        }
        let mut out: Vec<Meeting> = Vec::new();
        for ev in self.events.iter().filter(|e| !e.cancelled) {
            let Some(start) = self.utc_of(ev.start, local) else { continue };
            let length = self.length_of(ev, start, local);
            if length > MAX_LENGTH {
                continue;
            }
            let shows = |s: i64| s < to && (s + length > from || (length == 0 && s >= from));
            let make = |s: i64| Meeting {
                uid: ev.uid.clone(),
                title: ev.title.clone(),
                place: ev.place.clone(),
                start: s,
                end: s + length,
                link: ev.link.clone(),
            };
            let (Some(rule), None, Time::At(w0, zone)) = (&ev.rule, ev.recurrence_id, ev.start) else {
                if shows(start) {
                    out.push(make(start));
                }
                continue;
            };
            let until = rule.until.map(|u| match u {
                Time::Date(d) => (d + 1) * DAY - 1,
                Time::At(secs, Zone::Utc) => secs + self.offset_in(zone, secs, local),
                Time::At(secs, _) => secs,
            });
            // Окно в стенном времени — с запасом на смещение пояса и на длину встречи.
            let (lo, hi) = (from - length - 2 * DAY, to + 2 * DAY);
            // Исключения: смотрятся только те, что лежат у окна (остальные ни при чём), и не больше `MAX_NEAR_EXDATES` каждого
            // рода. В том же поясе, что серия, они сверяются по стенному времени (переводить в UTC незачем), в другом — по UTC.
            let near = |wall: i64| (lo - DAY..=hi + DAY).contains(&wall);
            let (mut walls, mut excluded, mut days): (Vec<i64>, Vec<i64>, Vec<i64>) =
                (Vec::new(), Vec::new(), Vec::new());
            for t in &ev.exdates {
                match *t {
                    Time::At(w, z) if near(w) && z == zone && walls.len() < MAX_NEAR_EXDATES => walls.push(w),
                    Time::At(w, z) if near(w) && z != zone && excluded.len() < MAX_NEAR_EXDATES => {
                        excluded.extend(self.utc_of(*t, local));
                    }
                    Time::Date(d) if near(d * DAY) && days.len() < MAX_NEAR_EXDATES => days.push(d),
                    _ => {}
                }
            }
            let mut emitted = 0usize;
            expand(rule, w0, until, lo, hi, &mut |wall| {
                if walls.contains(&wall) || days.contains(&wall.div_euclid(DAY)) {
                    return true;
                }
                let Some(s) = self.utc_of(Time::At(wall, zone), local) else { return true };
                if excluded.contains(&s) || (!ev.uid.is_empty() && replaced.contains(&(ev.uid.as_str(), s))) {
                    return true;
                }
                if shows(s) {
                    out.push(make(s));
                    emitted += 1;
                }
                emitted < MAX_PER_SERIES
            });
        }
        out.sort_by(|a, b| (a.start, a.end, &a.title).cmp(&(b.start, b.end, &b.title)));
        out
    }

    /// Длительность события, секунды: `DTEND`, иначе `DURATION`, иначе нет.
    fn length_of(&self, ev: &Event, start: i64, local: LocalFn<'_>) -> i64 {
        if let Some(end) = ev.end.and_then(|t| self.utc_of(t, local)) {
            return (end - start).max(0);
        }
        ev.duration.unwrap_or(0).max(0)
    }
}

// ─── Ссылки ─────────────────────────────────────────────────────────────────

/// Сайты служб: хост ссылки — он сам или его поддомен (`us02web.zoom.us`, но не `zoom.us.evil.example`).
const SERVICES: [(&str, Service); 11] = [
    ("zoom.us", Service::Zoom),
    ("meet.google.com", Service::Meet),
    ("teams.microsoft.com", Service::Teams),
    ("teams.live.com", Service::Teams),
    ("telemost.yandex.ru", Service::Telemost),
    ("jazz.sber.ru", Service::Jazz),
    ("meet.jit.si", Service::Jitsi),
    ("webex.com", Service::Webex),
    ("whereby.com", Service::Whereby),
    ("discord.gg", Service::Discord),
    ("discord.com", Service::Discord),
];

fn host_ok(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Знак адреса, который годится внутри ссылки: видимый ASCII без скобок-ограничителей и кавычек.
fn url_char(c: char) -> bool {
    c.is_ascii_graphic() && !matches!(c, '<' | '>' | '"' | '\'' | '`' | '\\' | '|' | '{' | '}' | '^' | '[' | ']')
}

/// Ссылка встречи — чужие данные: только `https://`, хост — сайт службы или его поддомен, без `user@`, порта, пробелов и
/// управляющих знаков, до [`MAX_LINK`] знаков. Годную отдаёт с хостом маленькими буквами и службой; негодная — `None`.
pub fn check_link(url: &str) -> Option<Link> {
    if url.len() > MAX_LINK || !url.chars().all(url_char) {
        return None;
    }
    let rest = url.get(..8).filter(|p| p.eq_ignore_ascii_case("https://")).map(|_| &url[8..])?;
    let at = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = rest[..at].to_ascii_lowercase();
    if host.contains(['@', ':']) || !host_ok(&host) {
        return None;
    }
    let service = SERVICES.iter().find(|(site, _)| host == *site || host.ends_with(&format!(".{site}")))?.1;
    Some(Link { url: format!("https://{host}{}", &rest[at..]), service })
}

/// Где в `hay` первое вхождение `needle` без учёта регистра ASCII.
fn find_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle))
}

/// Первая годная ссылка службы в тексте (описание, место, свойство): адрес идёт до пробела, скобки или кавычки, хвостовая
/// пунктуация не в счёт. Адрес длиннее [`MAX_LINK`] знаков не годится: его не обрезают, а пропускают. Каждый адрес смотрится
/// не дальше этого предела от своего начала, поэтому поиск не растёт квадратом от длины текста (`https://https://…`).
pub fn find_link(text: &str) -> Option<Link> {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(at) = find_ci(&bytes[from..], b"https://") {
        let start = from + at;
        from = start + 8;
        let limit = (start + MAX_LINK + 1).min(bytes.len());
        let end = bytes[start..limit].iter().position(|b| !url_char(char::from(*b))).map_or(limit, |e| start + e);
        if end - start > MAX_LINK {
            continue;
        }
        let candidate = text[start..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '*']);
        if let Some(link) = check_link(candidate) {
            return Some(link);
        }
    }
    None
}

/// Ссылка на календарь — из настроек или поля: `webcal://` (так дают iCloud и часть Outlook) читается как `https://`;
/// только https, без пробелов и управляющих знаков, до [`MAX_LINK`] знаков, с сайтом, без `user@`; порт допустим. Хост
/// приводится к маленьким буквам, `#…` отбрасывается. Негодное — `None`.
pub fn calendar_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.len() > MAX_LINK || !raw.chars().all(url_char) {
        return None;
    }
    let lower = raw.to_ascii_lowercase();
    let rest =
        ["https://", "webcals://", "webcal://"].iter().find(|p| lower.starts_with(*p)).map(|p| &raw[p.len()..])?;
    let rest = rest.split('#').next().unwrap_or(rest);
    let at = rest.find(['/', '?']).unwrap_or(rest.len());
    let (host, port) = match rest[..at].split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (&rest[..at], None),
    };
    // `user@сайт` и `user:пароль@сайт` не проходят: в имени сайта таких знаков нет, а порт — только число.
    let host = host.to_ascii_lowercase();
    let port = match port {
        Some(p) => Some(p.parse::<u16>().ok().filter(|p| *p != 0)?),
        None => None,
    };
    if !host_ok(&host) {
        return None;
    }
    let port = port.map(|p| format!(":{p}")).unwrap_or_default();
    Some(format!("https://{host}{port}{}", &rest[at..]))
}

/// Куда стучаться: хост, порт и путь с запросом — из ссылки, уже прошедшей [`calendar_url`].
pub fn split_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("https://")?;
    let at = rest.find(['/', '?']).unwrap_or(rest.len());
    let (host, port) = match rest[..at].split_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().ok()?),
        None => (&rest[..at], 443),
    };
    let tail = &rest[at..];
    let path = if tail.starts_with('/') { tail.to_string() } else { format!("/{tail}") };
    (!host.is_empty()).then(|| (host.to_string(), port, path))
}

/// Ссылка на календарь для окна: сайт и конец («calendar.google.com/…/basic.ics»); середина и запрос — секрет, их нет.
/// Конец показывается, только если это общее имя файла `.ics` — короткое, из строчных букв (`basic.ics`, `calendar.ics`):
/// в другом имени мог бы быть секрет.
pub fn short_url(url: &str) -> String {
    let rest = url.strip_prefix("https://").unwrap_or(url);
    let (site, path) = match rest.find(['/', '?']) {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, ""),
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let file = segments.last().filter(|s| {
        let cut = s.len().saturating_sub(4);
        (5..=16).contains(&s.len())
            && s.is_char_boundary(cut)
            && s[cut..].eq_ignore_ascii_case(".ics")
            && s[..cut].bytes().all(|b| b.is_ascii_lowercase() || b == b'-' || b == b'_')
    });
    match (segments.len(), file) {
        (0, _) => site.to_string(),
        (1, Some(file)) => format!("{site}/{file}"),
        (_, Some(file)) => format!("{site}/…/{file}"),
        (_, None) => format!("{site}/…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Стенные секунды: 2026-10-27 10:30.
    fn w(y: i64, mo: i64, d: i64, h: i64, mi: i64) -> i64 {
        days_from_civil(y, mo, d) * DAY + h * 3600 + mi * 60
    }

    fn utc0(_: i64) -> i32 {
        0
    }

    fn plus3(_: i64) -> i32 {
        3 * 3600
    }

    /// Файл календаря с событиями `body`: разделитель строк — `\r\n`, как в настоящих файлах.
    fn cal(body: &str) -> String {
        format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//demo//EN\r\n{}END:VCALENDAR\r\n",
            body.replace('\n', "\r\n")
        )
    }

    fn parse(body: &str) -> Calendar {
        Calendar::parse(&cal(body), 0).expect("календарь")
    }

    /// Встречи на сутки с `day` (стенные 00:00 в UTC) — по названиям и началу «чч:мм».
    fn titles(c: &Calendar, day_start: i64, local: LocalFn<'_>) -> Vec<(String, i64)> {
        c.meetings(day_start, day_start + DAY, local).into_iter().map(|m| (m.title, m.start - day_start)).collect()
    }

    #[test]
    fn dates_round_trip_and_weekdays() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1) * DAY, 951_868_800);
        for d in [-1000, 0, 1, 365, 10_000, 20_000, 2_932_896] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(weekday(0), 3, "1970-01-01 — четверг");
        assert_eq!(weekday(days_from_civil(2026, 10, 27)), 1, "27.10.2026 — вторник");
        assert_eq!(weekday(days_from_civil(2026, 10, 26)), 0);
        assert!(is_leap(2000) && !is_leap(1900) && is_leap(2024) && !is_leap(2026));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
    }

    #[test]
    fn stamps_are_checked() {
        assert_eq!(parse_stamp("20261027T103000"), Some((w(2026, 10, 27, 10, 30), false, false)));
        assert_eq!(parse_stamp("20261027T073000Z"), Some((w(2026, 10, 27, 7, 30), false, true)));
        assert_eq!(parse_stamp("20261027"), Some((w(2026, 10, 27, 0, 0), true, false)));
        for bad in
            ["", "2026", "20261327T000000", "20260230T000000", "20261027T250000", "20261027 103000", "x0261027T000000"]
        {
            assert_eq!(parse_stamp(bad), None, "{bad:?}");
        }
        assert_eq!(
            parse_stamp("20261027T235960"),
            Some((w(2026, 10, 27, 23, 59) + 59, false, false)),
            "високосная секунда"
        );
    }

    #[test]
    fn durations_and_offsets() {
        assert_eq!(parse_duration("PT45M"), Some(2700));
        assert_eq!(parse_duration("PT1H30M"), Some(5400));
        assert_eq!(parse_duration("P1D"), Some(DAY));
        assert_eq!(parse_duration("P1W"), Some(7 * DAY));
        assert_eq!(parse_duration("P1DT2H"), Some(DAY + 7200));
        assert_eq!(parse_duration("-PT15M"), Some(-900));
        for bad in ["", "45M", "PT", "P1H", "PT1X", "PT99999999999M"] {
            let got = parse_duration(bad);
            assert!(got.is_none() || bad == "PT", "{bad:?} -> {got:?}");
        }
        assert_eq!(parse_offset("+0300"), Some(10_800));
        assert_eq!(parse_offset("-0530"), Some(-19_800));
        assert_eq!(parse_offset("+053000"), Some(19_800));
        assert_eq!(parse_offset("0300"), None);
        assert_eq!(parse_offset("+03"), None);
    }

    #[test]
    fn lines_are_unfolded_and_text_unescaped() {
        let c = Calendar::parse(
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nDTSTART:20261027T103000Z\nSUMMARY:Plan\\, part\\;\n  one \\\\ two\n\t more\nEND:VEVENT\nEND:VCALENDAR\n",
            0,
        )
        .expect("календарь");
        let m = c.meetings(0, i64::MAX / 2, &utc0);
        assert_eq!(m.len(), 1);
        assert_eq!(
            m[0].title, "Plan, part; one \\ two more",
            "склейка (первый пробел продолжения — отступ) и экранирование"
        );
        // Старый перенос строки и BOM в начале файла.
        let bom = Calendar::parse("\u{feff}BEGIN:VCALENDAR\rEND:VCALENDAR\r", 0);
        assert!(bom.is_some());
    }

    #[test]
    fn not_a_calendar() {
        assert!(Calendar::parse("<html><body>Not found</body></html>", 0).is_none());
        assert!(Calendar::parse("", 0).is_none());
        let empty = Calendar::parse("BEGIN:VCALENDAR\r\nEND:VCALENDAR", 0).expect("пустой календарь — тоже календарь");
        assert!(empty.meetings(0, i64::MAX / 2, &utc0).is_empty());
    }

    #[test]
    fn utc_event_with_end_duration_and_name() {
        let c = Calendar::parse(
            &cal("X-WR-CALNAME:Работа\nBEGIN:VEVENT\nUID:1\nSUMMARY:Планёрка\nDTSTART:20261027T073000Z\nDTEND:20261027T074500Z\nLOCATION:Переговорная 2\nEND:VEVENT\nBEGIN:VEVENT\nUID:2\nSUMMARY:Обзор\nDTSTART:20261027T110000Z\nDURATION:PT1H\nEND:VEVENT\n"),
            0,
        )
        .unwrap();
        assert_eq!(c.name.as_deref(), Some("Работа"));
        let m = c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0);
        assert_eq!(m.len(), 2);
        assert_eq!(
            (m[0].title.as_str(), m[0].start, m[0].end),
            ("Планёрка", w(2026, 10, 27, 7, 30), w(2026, 10, 27, 7, 45))
        );
        assert_eq!(m[0].place, "Переговорная 2");
        assert_eq!((m[1].start, m[1].end), (w(2026, 10, 27, 11, 0), w(2026, 10, 27, 12, 0)), "DURATION");
        // Идущая в начале окна — есть, закончившаяся ровно в его начале — нет.
        assert_eq!(c.meetings(w(2026, 10, 27, 7, 40), w(2026, 10, 27, 8, 0), &utc0).len(), 1);
        assert_eq!(c.meetings(w(2026, 10, 27, 7, 45), w(2026, 10, 27, 8, 0), &utc0).len(), 0, "конец — не внутри");
    }

    #[test]
    fn an_event_without_an_end_is_a_moment() {
        let c = parse("BEGIN:VEVENT\nUID:1\nSUMMARY:Звонок\nDTSTART:20261027T100000Z\nEND:VEVENT\n");
        let m = c.meetings(w(2026, 10, 27, 10, 0), w(2026, 10, 27, 11, 0), &utc0);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start, m[0].end, "длительности нет — это момент");
        assert!(c.meetings(w(2026, 10, 27, 10, 1), w(2026, 10, 27, 11, 0), &utc0).is_empty());
    }

    #[test]
    fn all_day_and_cancelled_events_are_not_meetings() {
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Отпуск\nDTSTART;VALUE=DATE:20261027\nDTEND;VALUE=DATE:20261028\nEND:VEVENT\nBEGIN:VEVENT\nUID:2\nSUMMARY:Отменена\nSTATUS:CANCELLED\nDTSTART:20261027T100000Z\nDTEND:20261027T110000Z\nEND:VEVENT\nBEGIN:VEVENT\nUID:3\nSUMMARY:Живая\nSTATUS:CONFIRMED\nDTSTART:20261027T120000Z\nDTEND:20261027T130000Z\nEND:VEVENT\n",
        );
        assert_eq!(titles(&c, w(2026, 10, 27, 0, 0), &utc0), [("Живая".to_string(), 12 * 3600)]);
    }

    #[test]
    fn a_floating_time_is_the_local_time_of_this_pc() {
        let c =
            parse("BEGIN:VEVENT\nUID:1\nSUMMARY:Local\nDTSTART:20261027T103000\nDTEND:20261027T113000\nEND:VEVENT\n");
        let at = |local: LocalFn<'_>| c.meetings(0, i64::MAX / 2, local)[0].start;
        assert_eq!(at(&utc0), w(2026, 10, 27, 10, 30));
        assert_eq!(at(&plus3), w(2026, 10, 27, 7, 30), "ПК в UTC+3: 10:30 по месту — 07:30 UTC");
    }

    /// Москва без перехода на летнее время и Нью-Йорк с ним — как в настоящих файлах Google.
    const ZONES: &str = "BEGIN:VTIMEZONE\nTZID:Europe/Moscow\nX-LIC-LOCATION:Europe/Moscow\nBEGIN:STANDARD\nTZOFFSETFROM:+0300\nTZOFFSETTO:+0300\nTZNAME:MSK\nDTSTART:19700101T000000\nEND:STANDARD\nEND:VTIMEZONE\nBEGIN:VTIMEZONE\nTZID:America/New_York\nBEGIN:DAYLIGHT\nTZOFFSETFROM:-0500\nTZOFFSETTO:-0400\nTZNAME:EDT\nDTSTART:19700308T020000\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU\nEND:DAYLIGHT\nBEGIN:STANDARD\nTZOFFSETFROM:-0400\nTZOFFSETTO:-0500\nTZNAME:EST\nDTSTART:19701101T020000\nRRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU\nEND:STANDARD\nEND:VTIMEZONE\n";

    #[test]
    fn a_zone_from_the_file_sets_the_utc_time() {
        let c = parse(&format!(
            "{ZONES}BEGIN:VEVENT\nUID:1\nSUMMARY:Msk\nDTSTART;TZID=Europe/Moscow:20261027T103000\nDTEND;TZID=Europe/Moscow:20261027T104500\nEND:VEVENT\n"
        ));
        let m = c.meetings(0, i64::MAX / 2, &plus3);
        assert_eq!(
            (m[0].start, m[0].end),
            (w(2026, 10, 27, 7, 30), w(2026, 10, 27, 7, 45)),
            "10:30 в Москве — 07:30 UTC"
        );
        // Пояс файла важнее пояса ПК.
        assert_eq!(c.meetings(0, i64::MAX / 2, &|_| -8 * 3600)[0].start, w(2026, 10, 27, 7, 30));
        assert!(c.unknown_zones().is_empty());
    }

    /// Нью-Йорк: летом UTC−4, зимой UTC−5; границы — вторая неделя марта и первое воскресенье ноября.
    #[test]
    fn daylight_saving_comes_from_the_rules_of_the_file() {
        let c = parse(&format!(
            "{ZONES}BEGIN:VEVENT\nUID:s\nSUMMARY:Summer\nDTSTART;TZID=America/New_York:20260715T100000\nEND:VEVENT\nBEGIN:VEVENT\nUID:w\nSUMMARY:Winter\nDTSTART;TZID=America/New_York:20261215T100000\nEND:VEVENT\nBEGIN:VEVENT\nUID:b\nSUMMARY:Before\nDTSTART;TZID=America/New_York:20261101T003000\nEND:VEVENT\nBEGIN:VEVENT\nUID:a\nSUMMARY:After\nDTSTART;TZID=America/New_York:20261101T030000\nEND:VEVENT\nBEGIN:VEVENT\nUID:m\nSUMMARY:March\nDTSTART;TZID=America/New_York:20260308T090000\nEND:VEVENT\n"
        ));
        let by: std::collections::HashMap<String, i64> =
            c.meetings(0, i64::MAX / 2, &utc0).into_iter().map(|m| (m.title, m.start)).collect();
        assert_eq!(by["Summer"], w(2026, 7, 15, 14, 0), "EDT: +4");
        assert_eq!(by["Winter"], w(2026, 12, 15, 15, 0), "EST: +5");
        assert_eq!(by["Before"], w(2026, 11, 1, 4, 30), "до перевода часов 1 ноября — ещё EDT");
        assert_eq!(by["After"], w(2026, 11, 1, 8, 0), "после — EST");
        assert_eq!(by["March"], w(2026, 3, 8, 13, 0), "8 марта 2026 в 9:00 уже EDT (переход в 2:00)");
        // Пропавший весной час читается по смещению до перехода; повторённый осенью — первый.
        let gap = parse(&format!(
            "{ZONES}BEGIN:VEVENT\nUID:g\nSUMMARY:Gap\nDTSTART;TZID=America/New_York:20260308T023000\nEND:VEVENT\nBEGIN:VEVENT\nUID:r\nSUMMARY:Twice\nDTSTART;TZID=America/New_York:20261101T013000\nEND:VEVENT\n"
        ));
        let by: std::collections::HashMap<String, i64> =
            gap.meetings(0, i64::MAX / 2, &utc0).into_iter().map(|m| (m.title, m.start)).collect();
        assert_eq!(by["Gap"], w(2026, 3, 8, 7, 30), "02:30 EST");
        assert_eq!(by["Twice"], w(2026, 11, 1, 5, 30), "первое вхождение 01:30 — EDT");
    }

    #[test]
    fn a_zone_in_outlook_style_works_too() {
        // Windows-имя пояса, DTSTART 1601 года и «INTERVAL=1» — как в файлах Outlook.
        let c = parse(
            "BEGIN:VTIMEZONE\nTZID:Pacific Standard Time\nBEGIN:STANDARD\nDTSTART:16010101T020000\nTZOFFSETFROM:-0700\nTZOFFSETTO:-0800\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\nEND:STANDARD\nBEGIN:DAYLIGHT\nDTSTART:16010101T020000\nTZOFFSETFROM:-0800\nTZOFFSETTO:-0700\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\nEND:DAYLIGHT\nEND:VTIMEZONE\nBEGIN:VEVENT\nUID:s\nSUMMARY:Summer\nDTSTART;TZID=Pacific Standard Time:20260715T100000\nEND:VEVENT\nBEGIN:VEVENT\nUID:w\nSUMMARY:Winter\nDTSTART;TZID=Pacific Standard Time:20261215T100000\nEND:VEVENT\n",
        );
        let by: std::collections::HashMap<String, i64> =
            c.meetings(0, i64::MAX / 2, &utc0).into_iter().map(|m| (m.title, m.start)).collect();
        assert_eq!(by["Summer"], w(2026, 7, 15, 17, 0), "PDT: +7");
        assert_eq!(by["Winter"], w(2026, 12, 15, 18, 0), "PST: +8");
    }

    /// Европа и юг: «последнее воскресенье» (`-1SU`) в правилах пояса и лето в январе (Сидней).
    #[test]
    fn last_sunday_rules_and_the_southern_summer() {
        let berlin = "BEGIN:VTIMEZONE\nTZID:W. Europe Standard Time\nBEGIN:STANDARD\nDTSTART:16010101T030000\nTZOFFSETFROM:+0200\nTZOFFSETTO:+0100\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=10\nEND:STANDARD\nBEGIN:DAYLIGHT\nDTSTART:16010101T020000\nTZOFFSETFROM:+0100\nTZOFFSETTO:+0200\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=3\nEND:DAYLIGHT\nEND:VTIMEZONE\n";
        let at = |tzid: &str, y: i64, mo: i64, d: i64| {
            format!(
                "BEGIN:VEVENT\nUID:{y}{mo}{d}\nSUMMARY:{y}-{mo}-{d}\nDTSTART;TZID={tzid}:{y}{mo:02}{d:02}T100000\nEND:VEVENT\n"
            )
        };
        let c = parse(&format!(
            "{berlin}{}{}{}{}",
            at("W. Europe Standard Time", 2026, 3, 28),
            at("W. Europe Standard Time", 2026, 3, 30),
            at("W. Europe Standard Time", 2026, 10, 24),
            at("W. Europe Standard Time", 2026, 10, 26)
        ));
        let start =
            |title: &str| c.meetings(0, i64::MAX / 2, &utc0).into_iter().find(|m| m.title == title).unwrap().start;
        assert_eq!(start("2026-3-28"), w(2026, 3, 28, 9, 0), "до последнего воскресенья марта (29-го) — зима, +1");
        assert_eq!(start("2026-3-30"), w(2026, 3, 30, 8, 0), "после — лето, +2");
        assert_eq!(start("2026-10-24"), w(2026, 10, 24, 8, 0), "до последнего воскресенья октября (25-го) — лето");
        assert_eq!(start("2026-10-26"), w(2026, 10, 26, 9, 0), "после — зима");
        // Сидней: лето с первого воскресенья октября по первое воскресенье апреля — январь в летнем времени (+11), июль в зимнем (+10).
        let sydney = "BEGIN:VTIMEZONE\nTZID:AUS Eastern Standard Time\nBEGIN:STANDARD\nDTSTART:16010101T030000\nTZOFFSETFROM:+1100\nTZOFFSETTO:+1000\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=4\nEND:STANDARD\nBEGIN:DAYLIGHT\nDTSTART:16010101T020000\nTZOFFSETFROM:+1000\nTZOFFSETTO:+1100\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=10\nEND:DAYLIGHT\nEND:VTIMEZONE\n";
        let c = parse(&format!(
            "{sydney}{}{}",
            at("AUS Eastern Standard Time", 2027, 1, 15),
            at("AUS Eastern Standard Time", 2026, 7, 15)
        ));
        let start =
            |title: &str| c.meetings(0, i64::MAX / 2, &utc0).into_iter().find(|m| m.title == title).unwrap().start;
        assert_eq!(start("2027-1-15"), w(2027, 1, 14, 23, 0), "январь — +11: 10:00 → 23:00 накануне");
        assert_eq!(start("2026-7-15"), w(2026, 7, 15, 0, 0), "июль — +10");
    }

    /// Файлы, похожие на настоящие (выдуманное содержимое): Google с серией в поясе из `VTIMEZONE`, исключением и замещением, ссылкой
    /// Meet в перенесённом описании; Outlook с Windows-именем пояса и ссылкой Teams в угловых скобках.
    #[test]
    fn files_shaped_like_google_and_outlook_ones() {
        let google = "BEGIN:VCALENDAR\r\nPRODID:-//Google Inc//Google Calendar 70.9054//EN\r\nVERSION:2.0\r\nCALSCALE:GREGORIAN\r\nMETHOD:PUBLISH\r\nX-WR-CALNAME:demo@example.com\r\nX-WR-TIMEZONE:Europe/Moscow\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Moscow\r\nX-LIC-LOCATION:Europe/Moscow\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:+0300\r\nTZOFFSETTO:+0300\r\nTZNAME:MSK\r\nDTSTART:19700101T000000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nDTSTART;TZID=Europe/Moscow:20260901T103000\r\nDTEND;TZID=Europe/Moscow:20260901T104500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\r\nEXDATE;TZID=Europe/Moscow:20261028T103000\r\nDTSTAMP:20261026T120000Z\r\nORGANIZER;CN=Demo:mailto:demo@example.com\r\nUID:standup-1@google.com\r\nATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;CN=Demo;X-NUM-GUESTS=0:mailto:demo@example.com\r\nCREATED:20260801T080000Z\r\nDESCRIPTION:Join with Google Meet: https://meet.google.com/abc-d\r\n efg-hij\\n\\nLearn more about Meet at: https://support.google.com/a/users/an\r\n swer/9282720\\n\\nPlease do not edit this section.\r\nLAST-MODIFIED:20260801T080000Z\r\nLOCATION:\r\nSEQUENCE:0\r\nSTATUS:CONFIRMED\r\nSUMMARY:Daily stand-up\r\nTRANSP:OPAQUE\r\nX-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:This is an event reminder\r\nTRIGGER:-P0DT0H10M0S\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;TZID=Europe/Moscow:20261029T120000\r\nDTEND;TZID=Europe/Moscow:20261029T123000\r\nRECURRENCE-ID;TZID=Europe/Moscow:20261029T103000\r\nUID:standup-1@google.com\r\nSTATUS:CONFIRMED\r\nSUMMARY:Daily stand-up (moved)\r\nDESCRIPTION:Join with Google Meet: https://meet.google.com/abc-defg-hij\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let c = Calendar::parse(google, 0).expect("Google");
        assert_eq!(c.name.as_deref(), Some("demo@example.com"));
        let day = |d: i64| {
            c.meetings(w(2026, 10, d, 0, 0), w(2026, 10, d + 1, 0, 0), &plus3)
                .into_iter()
                .map(|m| (m.title, m.start - w(2026, 10, d, 0, 0)))
                .collect::<Vec<_>>()
        };
        assert_eq!(day(27), [("Daily stand-up".to_string(), 7 * 3600 + 30 * 60)], "10:30 по Москве — 07:30 UTC");
        assert!(day(28).is_empty(), "EXDATE");
        assert_eq!(
            day(29),
            [("Daily stand-up (moved)".to_string(), 9 * 3600)],
            "перенесено на 12:00; прежнего места нет"
        );
        let m = &c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &plus3)[0];
        assert_eq!(
            m.link.as_ref().map(|l| l.url.as_str()),
            Some("https://meet.google.com/abc-defg-hij"),
            "ссылка из свойства службы целая, а в описании её разорвал перенос строки"
        );
        assert_eq!(m.place, "", "места нет");
        // Только описание, ссылка разорвана переносом.
        let only_description = google.replace("X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r\n", "");
        let c = Calendar::parse(&only_description, 0).unwrap();
        let m = &c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &plus3)[0];
        assert_eq!(m.link.as_ref().map(|l| l.url.as_str()), Some("https://meet.google.com/abc-defg-hij"));

        let outlook = "BEGIN:VCALENDAR\r\nMETHOD:PUBLISH\r\nPRODID:Microsoft Exchange Server 2010\r\nVERSION:2.0\r\nX-WR-CALNAME:Calendar\r\nBEGIN:VTIMEZONE\r\nTZID:Russian Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T000000\r\nTZOFFSETFROM:+0300\r\nTZOFFSETTO:+0300\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nDTSTART:16010101T000000\r\nTZOFFSETFROM:+0300\r\nTZOFFSETTO:+0300\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nCLASS:PUBLIC\r\nDESCRIPTION:Join the meeting now: <https://teams.microsoft.com/l/meetup-join/19%3ameeting_demo%40thread.v2/0?context=%7b%22Tid%22%3a%22demo%22%7d>\\n\\nMeeting ID: 123 456 789\r\nDTEND;TZID=Russian Standard Time:20261027T140000\r\nDTSTART;TZID=Russian Standard Time:20261027T133000\r\nLOCATION:Microsoft Teams Meeting\r\nPRIORITY:5\r\nSEQUENCE:0\r\nSUMMARY;LANGUAGE=en-us:Release call\r\nTRANSP:OPAQUE\r\nUID:040000008200E00074C5B7101A82E00800000000\r\nX-MICROSOFT-CDO-BUSYSTATUS:BUSY\r\nX-MICROSOFT-ONLINEMEETINGEXTERNALLINK:\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let c = Calendar::parse(outlook, 0).expect("Outlook");
        let m = &c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &plus3)[0];
        assert_eq!(
            (m.title.as_str(), m.start, m.end),
            ("Release call", w(2026, 10, 27, 10, 30), w(2026, 10, 27, 11, 0)),
            "название с LANGUAGE=, пояс «Russian Standard Time»"
        );
        assert_eq!(m.place, "Microsoft Teams Meeting");
        let link = m.link.as_ref().expect("ссылка в угловых скобках");
        assert_eq!(link.service, Service::Teams);
        assert!(
            link.url
                .starts_with("https://teams.microsoft.com/l/meetup-join/19%3ameeting_demo%40thread.v2/0?context=%7b"),
            "{}",
            link.url
        );
    }

    #[test]
    fn an_unknown_zone_is_the_local_time_and_is_named() {
        let c = parse("BEGIN:VEVENT\nUID:1\nSUMMARY:X\nDTSTART;TZID=Mars/Olympus:20261027T103000\nEND:VEVENT\n");
        assert_eq!(c.unknown_zones(), ["Mars/Olympus"]);
        assert_eq!(c.meetings(0, i64::MAX / 2, &plus3)[0].start, w(2026, 10, 27, 7, 30));
    }

    #[test]
    fn a_weekly_series_comes_on_its_days_in_its_zone() {
        // По будням в 10:30 по Москве; сегодня 27.10.2026 — вторник.
        let c = parse(&format!(
            "{ZONES}BEGIN:VEVENT\nUID:p\nSUMMARY:Планёрка\nDTSTART;TZID=Europe/Moscow:20260901T103000\nDTEND;TZID=Europe/Moscow:20260901T104500\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\nEND:VEVENT\n"
        ));
        let day = |d: i64| titles(&c, w(2026, 10, d, 0, 0), &utc0);
        assert_eq!(day(27), [("Планёрка".to_string(), 7 * 3600 + 30 * 60)]);
        assert!(day(24).is_empty(), "24.10.2026 — суббота: серия по будням не приходит");
    }

    #[test]
    fn a_weekly_series_skips_weekends() {
        let c = parse(
            "BEGIN:VEVENT\nUID:p\nSUMMARY:Stand-up\nDTSTART:20260901T073000Z\nDTEND:20260901T074500Z\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\nEND:VEVENT\n",
        );
        let count = |d: i64| titles(&c, w(2026, 10, d, 0, 0), &utc0).len();
        // 24.10 — суббота, 25 — воскресенье, 26 — понедельник, 27 — вторник.
        assert_eq!([count(23), count(24), count(25), count(26), count(27)], [1, 0, 0, 1, 1]);
    }

    #[test]
    fn exdate_and_a_replacement_and_a_cancelled_repeat() {
        let c = parse(
            "BEGIN:VEVENT\nUID:p\nSUMMARY:Stand-up\nDTSTART:20261026T073000Z\nDTEND:20261026T074500Z\nRRULE:FREQ=DAILY;COUNT=6\nEXDATE:20261028T073000Z\nEND:VEVENT\nBEGIN:VEVENT\nUID:p\nRECURRENCE-ID:20261027T073000Z\nSUMMARY:Stand-up (later)\nDTSTART:20261027T090000Z\nDTEND:20261027T091500Z\nEND:VEVENT\nBEGIN:VEVENT\nUID:p\nRECURRENCE-ID:20261029T073000Z\nSTATUS:CANCELLED\nSUMMARY:Stand-up\nDTSTART:20261029T073000Z\nEND:VEVENT\n",
        );
        let all = c.meetings(w(2026, 10, 25, 0, 0), w(2026, 11, 3, 0, 0), &utc0);
        let got: Vec<(String, i64)> = all.iter().map(|m| (m.title.clone(), m.start)).collect();
        assert_eq!(
            got,
            [
                ("Stand-up".to_string(), w(2026, 10, 26, 7, 30)),
                ("Stand-up (later)".to_string(), w(2026, 10, 27, 9, 0)), // замена 27-го: повтор в 7:30 не показан
                // 28-е — EXDATE, 29-е — отменённая замена
                ("Stand-up".to_string(), w(2026, 10, 30, 7, 30)),
                ("Stand-up".to_string(), w(2026, 10, 31, 7, 30)),
            ],
            "COUNT=6: с 26-го по 31-е; минус 28-е и 29-е; 27-е заменено"
        );
    }

    #[test]
    fn count_until_and_interval() {
        // Раз в две недели по вторникам, 3 раза: 6.10, 20.10, 3.11.
        let c = parse(
            "BEGIN:VEVENT\nUID:b\nSUMMARY:Biweekly\nDTSTART:20261006T100000Z\nDTEND:20261006T110000Z\nRRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=3\nEND:VEVENT\n",
        );
        let dates: Vec<i64> = c.meetings(0, i64::MAX / 2, &utc0).iter().map(|m| m.start).collect();
        assert_eq!(dates, [w(2026, 10, 6, 10, 0), w(2026, 10, 20, 10, 0), w(2026, 11, 3, 10, 0)]);
        // UNTIL включает последний день.
        let c = parse(
            "BEGIN:VEVENT\nUID:u\nSUMMARY:Until\nDTSTART:20261026T100000Z\nDTEND:20261026T110000Z\nRRULE:FREQ=DAILY;UNTIL=20261028T100000Z\nEND:VEVENT\n",
        );
        assert_eq!(c.meetings(0, i64::MAX / 2, &utc0).len(), 3, "26, 27 и 28 в 10:00");
        let c = parse(
            "BEGIN:VEVENT\nUID:u\nSUMMARY:Until\nDTSTART:20261026T100000Z\nDTEND:20261026T110000Z\nRRULE:FREQ=DAILY;UNTIL=20261028\nEND:VEVENT\n",
        );
        assert_eq!(c.meetings(0, i64::MAX / 2, &utc0).len(), 3, "UNTIL датой — весь тот день");
        // INTERVAL=3 ежедневно.
        let c = parse(
            "BEGIN:VEVENT\nUID:i\nSUMMARY:Every3\nDTSTART:20261001T100000Z\nDTEND:20261001T110000Z\nRRULE:FREQ=DAILY;INTERVAL=3;COUNT=5\nEND:VEVENT\n",
        );
        let days: Vec<i64> =
            c.meetings(0, i64::MAX / 2, &utc0).iter().map(|m| (m.start - w(2026, 10, 1, 10, 0)) / DAY).collect();
        assert_eq!(days, [0, 3, 6, 9, 12]);
    }

    #[test]
    fn monthly_rules() {
        let day = |y: i64, mo: i64, d: i64| w(y, mo, d, 10, 0);
        let all = |c: &Calendar| -> Vec<i64> { c.meetings(0, i64::MAX / 2, &utc0).iter().map(|m| m.start).collect() };
        // Вторник второй недели месяца: 13.10.2026, 10.11.2026.
        let c = parse(
            "BEGIN:VEVENT\nUID:m\nSUMMARY:M\nDTSTART:20261013T100000Z\nDTEND:20261013T110000Z\nRRULE:FREQ=MONTHLY;BYDAY=2TU;COUNT=3\nEND:VEVENT\n",
        );
        assert_eq!(all(&c), [day(2026, 10, 13), day(2026, 11, 10), day(2026, 12, 8)]);
        // То же через BYSETPOS.
        let c = parse(
            "BEGIN:VEVENT\nUID:m\nSUMMARY:M\nDTSTART:20261013T100000Z\nDTEND:20261013T110000Z\nRRULE:FREQ=MONTHLY;BYDAY=TU;BYSETPOS=2;COUNT=3\nEND:VEVENT\n",
        );
        assert_eq!(all(&c), [day(2026, 10, 13), day(2026, 11, 10), day(2026, 12, 8)]);
        // Последняя пятница.
        let c = parse(
            "BEGIN:VEVENT\nUID:m\nSUMMARY:M\nDTSTART:20261030T100000Z\nDTEND:20261030T110000Z\nRRULE:FREQ=MONTHLY;BYDAY=-1FR;COUNT=3\nEND:VEVENT\n",
        );
        assert_eq!(all(&c), [day(2026, 10, 30), day(2026, 11, 27), day(2026, 12, 25)]);
        // 31-е: в ноябре его нет — месяц пропускается.
        let c = parse(
            "BEGIN:VEVENT\nUID:m\nSUMMARY:M\nDTSTART:20261031T100000Z\nDTEND:20261031T110000Z\nRRULE:FREQ=MONTHLY;COUNT=3\nEND:VEVENT\n",
        );
        assert_eq!(all(&c), [day(2026, 10, 31), day(2026, 12, 31), day(2027, 1, 31)]);
        // Последний день месяца: BYMONTHDAY=-1.
        let c = parse(
            "BEGIN:VEVENT\nUID:m\nSUMMARY:M\nDTSTART:20261031T100000Z\nDTEND:20261031T110000Z\nRRULE:FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=3\nEND:VEVENT\n",
        );
        assert_eq!(all(&c), [day(2026, 10, 31), day(2026, 11, 30), day(2026, 12, 31)]);
        // Раз в два месяца, 15-го.
        let c = parse(
            "BEGIN:VEVENT\nUID:m\nSUMMARY:M\nDTSTART:20261015T100000Z\nDTEND:20261015T110000Z\nRRULE:FREQ=MONTHLY;INTERVAL=2;COUNT=3\nEND:VEVENT\n",
        );
        assert_eq!(all(&c), [day(2026, 10, 15), day(2026, 12, 15), day(2027, 2, 15)]);
    }

    #[test]
    fn yearly_rules() {
        let c = parse(
            "BEGIN:VEVENT\nUID:y\nSUMMARY:Y\nDTSTART:20240229T100000Z\nDTEND:20240229T110000Z\nRRULE:FREQ=YEARLY;COUNT=3\nEND:VEVENT\n",
        );
        let got: Vec<i64> = c.meetings(0, i64::MAX / 2, &utc0).iter().map(|m| m.start).collect();
        assert_eq!(
            got,
            [w(2024, 2, 29, 10, 0), w(2028, 2, 29, 10, 0), w(2032, 2, 29, 10, 0)],
            "29 февраля бывает не каждый год: годы без него пропускаются и в счёт не идут"
        );
        let c = parse(
            "BEGIN:VEVENT\nUID:y\nSUMMARY:Y\nDTSTART:20250310T100000Z\nDTEND:20250310T110000Z\nRRULE:FREQ=YEARLY;BYMONTH=3,10;BYMONTHDAY=10\nEND:VEVENT\n",
        );
        let got: Vec<i64> =
            c.meetings(w(2026, 1, 1, 0, 0), w(2027, 1, 1, 0, 0), &utc0).iter().map(|m| m.start).collect();
        assert_eq!(got, [w(2026, 3, 10, 10, 0), w(2026, 10, 10, 10, 0)]);
    }

    #[test]
    fn an_old_series_is_found_without_walking_every_day() {
        // Серия 1990 года без конца: в окне 2026 года — одно вхождение, и быстро.
        let c = parse(
            "BEGIN:VEVENT\nUID:o\nSUMMARY:Old\nDTSTART:19900101T100000Z\nDTEND:19900101T110000Z\nRRULE:FREQ=DAILY\nEND:VEVENT\n",
        );
        let got = c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0);
        assert_eq!(got.len(), 1);
        // Серия с COUNT, что кончилась давно, — ничего; идущая — вхождения только из окна.
        let c = parse(
            "BEGIN:VEVENT\nUID:o\nSUMMARY:Old\nDTSTART:20200101T100000Z\nDTEND:20200101T110000Z\nRRULE:FREQ=DAILY;COUNT=30\nEND:VEVENT\n",
        );
        assert!(c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0).is_empty());
        let c = parse(
            "BEGIN:VEVENT\nUID:o\nSUMMARY:Old\nDTSTART:20200101T100000Z\nDTEND:20200101T110000Z\nRRULE:FREQ=DAILY;COUNT=100000\nEND:VEVENT\n",
        );
        assert_eq!(c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0).len(), 1);
    }

    #[test]
    fn a_series_keeps_its_wall_time_over_the_clock_change() {
        // 10:00 в Нью-Йорке каждый день: до перевода часов (1.11.2026) 14:00 UTC, после — 15:00 UTC.
        let c = parse(&format!(
            "{ZONES}BEGIN:VEVENT\nUID:d\nSUMMARY:Daily\nDTSTART;TZID=America/New_York:20261026T100000\nDTEND;TZID=America/New_York:20261026T103000\nRRULE:FREQ=DAILY;COUNT=10\nEND:VEVENT\n"
        ));
        let at =
            |d: i64| c.meetings(w(2026, 11, d, 0, 0), w(2026, 11, d + 1, 0, 0), &utc0)[0].start - w(2026, 11, d, 0, 0);
        assert_eq!(at(1), 15 * 3600, "1 ноября в 10:00 EST — это 15:00 UTC");
        let before = c.meetings(w(2026, 10, 31, 0, 0), w(2026, 11, 1, 0, 0), &utc0)[0].start;
        assert_eq!(before, w(2026, 10, 31, 14, 0), "до перевода — 14:00 UTC");
    }

    #[test]
    fn an_override_moved_to_another_day_shows_on_that_day_only() {
        let c = parse(
            "BEGIN:VEVENT\nUID:p\nSUMMARY:Review\nDTSTART:20261026T100000Z\nDTEND:20261026T110000Z\nRRULE:FREQ=WEEKLY\nEND:VEVENT\nBEGIN:VEVENT\nUID:p\nRECURRENCE-ID:20261102T100000Z\nSUMMARY:Review (moved)\nDTSTART:20261103T150000Z\nDTEND:20261103T160000Z\nEND:VEVENT\n",
        );
        let got: Vec<(String, i64)> = c
            .meetings(w(2026, 10, 30, 0, 0), w(2026, 11, 10, 0, 0), &utc0)
            .into_iter()
            .map(|m| (m.title, m.start))
            .collect();
        assert_eq!(
            got,
            [("Review (moved)".to_string(), w(2026, 11, 3, 15, 0)), ("Review".to_string(), w(2026, 11, 9, 10, 0))]
        );
    }

    #[test]
    fn old_events_are_dropped_before_they_count_against_the_limit() {
        let mut body = String::new();
        for i in 0..2500 {
            body.push_str(&format!(
                "BEGIN:VEVENT\nUID:old{i}\nSUMMARY:Old\nDTSTART:20200101T100000Z\nDTEND:20200101T110000Z\nEND:VEVENT\n"
            ));
        }
        body.push_str(
            "BEGIN:VEVENT\nUID:now\nSUMMARY:Now\nDTSTART:20261027T100000Z\nDTEND:20261027T110000Z\nEND:VEVENT\n",
        );
        let c = Calendar::parse(&cal(&body), w(2026, 10, 27, 9, 0)).unwrap();
        assert!(!c.overflow);
        assert_eq!(c.events.len(), 1, "прошлое не хранится");
        // А если событий нужных больше предела — лишние пропущены, прочитанное остаётся.
        let mut body = String::new();
        for i in 0..2100 {
            body.push_str(&format!(
                "BEGIN:VEVENT\nUID:f{i}\nSUMMARY:Future\nDTSTART:20271027T100000Z\nDTEND:20271027T110000Z\nEND:VEVENT\n"
            ));
        }
        let c = Calendar::parse(&cal(&body), w(2026, 10, 27, 9, 0)).unwrap();
        assert!(c.overflow);
        assert_eq!(c.events.len(), MAX_EVENTS);
    }

    #[test]
    fn an_alarm_inside_an_event_does_not_spoil_it() {
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Real\nDTSTART:20261027T100000Z\nDTEND:20261027T110000Z\nBEGIN:VALARM\nACTION:DISPLAY\nDESCRIPTION:Reminder text\nTRIGGER:-PT10M\nEND:VALARM\nDESCRIPTION:Join https://meet.google.com/abc-defg-hij now\nEND:VEVENT\n",
        );
        let m = c.meetings(0, i64::MAX / 2, &utc0);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].link.as_ref().map(|l| l.service), Some(Service::Meet), "описание события, а не напоминания");
    }

    #[test]
    fn the_link_comes_from_url_then_location_then_the_service_property_then_the_description() {
        let ev = |props: &str| {
            let c = parse(&format!("BEGIN:VEVENT\nUID:1\nSUMMARY:X\nDTSTART:20261027T100000Z\n{props}END:VEVENT\n"));
            c.meetings(0, i64::MAX / 2, &utc0).remove(0).link
        };
        let desc = "DESCRIPTION:Join https://meet.google.com/aaa-bbbb-ccc\\nor dial\n";
        let loc = "LOCATION:https://us02web.zoom.us/j/123456789?pwd=SECRET\n";
        let url = "URL:https://teams.microsoft.com/l/meetup-join/xyz\n";
        let x = "X-GOOGLE-CONFERENCE:https://meet.google.com/xxx-yyyy-zzz\n";
        assert_eq!(ev(&format!("{desc}{loc}{url}")).map(|l| l.service), Some(Service::Teams), "URL — первым");
        assert_eq!(ev(&format!("{desc}{loc}")).map(|l| l.service), Some(Service::Zoom), "потом LOCATION");
        assert_eq!(
            ev(&format!("{desc}{x}")).unwrap().url,
            "https://meet.google.com/xxx-yyyy-zzz",
            "потом свойство службы"
        );
        assert_eq!(ev(desc).unwrap().url, "https://meet.google.com/aaa-bbbb-ccc", "потом описание");
        assert_eq!(ev("DESCRIPTION:Обсуждаем план\n"), None, "ссылки нет — кнопки нет");
        // Ссылка в месте целиком, вместе с паролем — как есть.
        assert_eq!(ev(loc).unwrap().url, "https://us02web.zoom.us/j/123456789?pwd=SECRET");
    }

    #[test]
    fn links_are_checked_as_foreign_data() {
        let ok = |u: &str| check_link(u).map(|l| (l.service, l.url));
        assert_eq!(ok("https://zoom.us/j/1"), Some((Service::Zoom, "https://zoom.us/j/1".into())));
        assert_eq!(ok("https://us02web.zoom.us/j/1?pwd=abc").map(|r| r.0), Some(Service::Zoom), "поддомен");
        assert_eq!(
            ok("HTTPS://Meet.Google.COM/abc-defg-hij"),
            Some((Service::Meet, "https://meet.google.com/abc-defg-hij".into())),
            "регистр хоста и схемы"
        );
        for (url, service) in [
            ("https://teams.microsoft.com/l/meetup-join/1", Service::Teams),
            ("https://teams.live.com/meet/1", Service::Teams),
            ("https://telemost.yandex.ru/j/1", Service::Telemost),
            ("https://jazz.sber.ru/abc", Service::Jazz),
            ("https://meet.jit.si/Room", Service::Jitsi),
            ("https://company.webex.com/meet/x", Service::Webex),
            ("https://whereby.com/room", Service::Whereby),
            ("https://discord.gg/abc", Service::Discord),
            ("https://discord.com/channels/1/2", Service::Discord),
        ] {
            assert_eq!(ok(url).map(|r| r.0), Some(service), "{url}");
        }
        for bad in [
            "http://zoom.us/j/1",               // не https
            "https://zoom.us.evil.example/j/1", // чужой хост с похожим началом
            "https://evilzoom.us/j/1",          // не поддомен
            "https://evil.example/zoom.us",     // сайт службы только в пути
            "https://zoom.us@evil.example/j/1", // user@
            "https://user:pw@zoom.us/j/1",      // user:pw@
            "https://zoom.us:8443/j/1",         // порт
            "https://zoom.us\\@evil.example/",  // обратная косая
            "https://zoom.us/j/1 2",            // пробел
            "https://zoom.us/j/1\u{7}",         // управляющий знак
            "https://zoom.us/j/\"x\"",          // кавычки
            "https://zoom.us./j/1",             // хвостовая точка
            "https://zоom.us/j/1",              // кириллическая «о»
            "https://-zoom.us/j/1",
            "https://",
            "zoom.us/j/1",
            "javascript:alert(1)",
            "file:///c:/windows",
        ] {
            assert_eq!(check_link(bad), None, "{bad:?}");
        }
        let long = format!("https://zoom.us/j/{}", "a".repeat(MAX_LINK));
        assert_eq!(check_link(&long), None, "длиннее предела");
        assert!(check_link(&long[..MAX_LINK]).is_some(), "ровно предел — годна");
    }

    #[test]
    fn a_link_is_found_in_free_text() {
        let found = |t: &str| find_link(t).map(|l| l.url);
        assert_eq!(
            found("Join Zoom Meeting\nhttps://us02web.zoom.us/j/123?pwd=x\n\nMeeting ID: 123").as_deref(),
            Some("https://us02web.zoom.us/j/123?pwd=x")
        );
        assert_eq!(
            found("<https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0>").as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0"),
            "в скобках <…>"
        );
        assert_eq!(
            found("(https://meet.google.com/abc-defg-hij).").as_deref(),
            Some("https://meet.google.com/abc-defg-hij"),
            "скобка и точка в конце"
        );
        assert_eq!(
            found("см. https://example.com/doc и потом https://telemost.yandex.ru/j/42, спасибо").as_deref(),
            Some("https://telemost.yandex.ru/j/42"),
            "чужие сайты пропущены"
        );
        assert_eq!(
            found("https://meet.jit.si/Room-Имя").as_deref(),
            Some("https://meet.jit.si/Room-"),
            "на русской букве адрес кончается"
        );
        assert_eq!(
            found("https://zoom.us.evil.example/j/1 https://zoom.us/j/2").as_deref(),
            Some("https://zoom.us/j/2"),
            "негодную пропускаем, ищем дальше"
        );
        assert_eq!(found("только слова"), None);
        assert_eq!(found(""), None);
    }

    #[test]
    fn calendar_links_are_normalised() {
        let ok = |u: &str| calendar_url(u);
        assert_eq!(
            ok("https://calendar.example.com/ical/demo.ics").as_deref(),
            Some("https://calendar.example.com/ical/demo.ics")
        );
        assert_eq!(
            ok("  https://Calendar.Example.com/ical/demo.ics \n").as_deref(),
            Some("https://calendar.example.com/ical/demo.ics"),
            "пробелы и регистр хоста"
        );
        assert_eq!(
            ok("webcal://p01.example.com/published/2/demo").as_deref(),
            Some("https://p01.example.com/published/2/demo"),
            "webcal:// — это https://"
        );
        assert_eq!(ok("WEBCAL://p01.example.com/x").as_deref(), Some("https://p01.example.com/x"));
        assert_eq!(
            ok("https://example.com:8443/cal.ics").as_deref(),
            Some("https://example.com:8443/cal.ics"),
            "свой порт"
        );
        assert_eq!(
            ok("https://example.com/cal.ics?token=abc#frag").as_deref(),
            Some("https://example.com/cal.ics?token=abc"),
            "якорь отброшен"
        );
        assert_eq!(ok("https://example.com").as_deref(), Some("https://example.com"));
        for bad in [
            "",
            "calendar.example.com/demo.ics",
            "http://calendar.example.com/demo.ics",
            "ftp://calendar.example.com/demo.ics",
            "https://",
            "https:///demo.ics",
            "https://user:pw@calendar.example.com/demo.ics",
            "https://calendar.example.com:0/demo.ics",
            "https://calendar.example.com:99999/demo.ics",
            "https://calendar.example.com:abc/demo.ics",
            "https://calendar example.com/demo.ics",
            "https://calendar.example.com/de mo.ics",
            "https://calendar.example.com/demo.ics\nhttps://other.example.com/",
            "https://календарь.рф/demo.ics",
            "https://[::1]/demo.ics",
        ] {
            assert_eq!(ok(bad), None, "{bad:?}");
        }
        assert_eq!(ok(&format!("https://example.com/{}", "a".repeat(MAX_LINK))), None, "длиннее предела");
    }

    #[test]
    fn a_calendar_link_is_split_for_the_request() {
        assert_eq!(
            split_url("https://calendar.example.com/ical/demo.ics?x=1"),
            Some(("calendar.example.com".into(), 443, "/ical/demo.ics?x=1".into()))
        );
        assert_eq!(
            split_url("https://example.com:8443/cal.ics"),
            Some(("example.com".into(), 8443, "/cal.ics".into()))
        );
        assert_eq!(split_url("https://example.com"), Some(("example.com".into(), 443, "/".into())));
        assert_eq!(split_url("https://example.com?x=1"), Some(("example.com".into(), 443, "/?x=1".into())));
        assert_eq!(split_url("http://example.com/x"), None);
    }

    #[test]
    fn a_calendar_link_is_shown_by_site_and_end_only() {
        assert_eq!(
            short_url("https://calendar.google.com/calendar/ical/me%40example.com/private-0123456789abcdef/basic.ics"),
            "calendar.google.com/…/basic.ics"
        );
        assert_eq!(short_url("https://calendar.example.com/…/demo.ics"), "calendar.example.com/…/demo.ics");
        assert_eq!(short_url("https://calendar.example.com/demo.ics"), "calendar.example.com/demo.ics");
        assert_eq!(
            short_url("https://outlook.example.com/owa/calendar/GUID/TOKEN/calendar.ics?x=secret"),
            "outlook.example.com/…/calendar.ics",
            "запрос не показывается"
        );
        // Конец — не файл .ics: он мог бы быть секретом, не показывается.
        assert_eq!(
            short_url("https://p01-caldav.example.com/published/2/Ab3dEf9GhIjKlMnOp"),
            "p01-caldav.example.com/…"
        );
        assert_eq!(short_url("https://example.com/Ab3dEf9GhIjKlMnOp"), "example.com/…");
        assert_eq!(short_url("https://example.com/"), "example.com");
        assert_eq!(short_url("https://example.com?token=secret"), "example.com");
        // Имя с цифрами или заглавными буквами похоже на токен: не показывается, как и длинное.
        assert_eq!(short_url("https://calendar.example.com/s3cr3tt0k3nABCDEF1234567.ics"), "calendar.example.com/…");
        assert_eq!(short_url("https://calendar.example.com/AbCdEfGhIj.ics"), "calendar.example.com/…");
        assert_eq!(short_url("https://calendar.example.com/private-0123456789abcdef.ics"), "calendar.example.com/…");
        // Длинное «имя файла» — тоже не файл.
        assert_eq!(short_url(&format!("https://example.com/a/{}.ics", "x".repeat(40))), "example.com/…");
        let secret = "SECRET123TOKEN";
        for url in [
            format!("https://calendar.google.com/calendar/ical/me/private-{secret}/basic.ics"),
            format!("https://example.com/{secret}/calendar.ics?token={secret}"),
            format!("https://example.com/{secret}"),
        ] {
            assert!(!short_url(&url).contains(secret), "{url}");
        }
    }

    #[test]
    fn rules_from_the_file_are_checked() {
        assert!(parse_rule("FREQ=WEEKLY;BYDAY=MO,WE").is_some());
        assert!(parse_rule("FREQ=HOURLY").is_none(), "частота, которой панель не знает — повторов нет");
        assert!(parse_rule("BYDAY=MO").is_none(), "без FREQ");
        assert!(parse_rule("FREQ=DAILY;INTERVAL=0").is_none());
        assert!(parse_rule("FREQ=DAILY;INTERVAL=99999999").is_none());
        assert!(parse_rule("FREQ=DAILY;COUNT=0").is_none());
        assert!(parse_rule("FREQ=WEEKLY;BYDAY=XX").is_none());
        assert!(parse_rule("FREQ=MONTHLY;BYMONTHDAY=0").is_none());
        assert!(parse_rule("FREQ=MONTHLY;BYMONTHDAY=40").is_none());
        assert!(parse_rule("FREQ=YEARLY;BYMONTH=13").is_none());
        let r = parse_rule("freq=monthly;byday=-1fr,2MO,+3TU;wkst=su").unwrap();
        assert_eq!((r.freq, r.by_day.clone(), r.wkst), (Freq::Monthly, vec![(-1, 4), (2, 0), (3, 1)], 6));
        // Незнакомая часть правила пропускается.
        assert!(parse_rule("FREQ=DAILY;X-FUTURE=1").is_some());
        // Серия с негодным правилом показывается одним вхождением.
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Odd\nDTSTART:20261027T100000Z\nDTEND:20261027T110000Z\nRRULE:FREQ=HOURLY\nEND:VEVENT\n",
        );
        assert_eq!(c.meetings(0, i64::MAX / 2, &utc0).len(), 1);
    }

    /// Сколько шагов потратила серия `rule` с началом `start` (стенные секунды) на окно `lo..=hi` и сколько вхождений дала.
    fn steps_of(rule: &str, start: i64, lo: i64, hi: i64) -> (u32, usize) {
        let rule = parse_rule(rule).expect("правило");
        let mut found = 0;
        let steps = expand(&rule, start, None, lo, hi, &mut |_| {
            found += 1;
            true
        });
        (steps, found)
    }

    /// Вхождения правила «в лоб»: с самого начала серии, без запаса шагов и без прыжка к окну.
    fn expand_naive(rule: &Rule, start: i64, lo: i64, hi: i64) -> Vec<i64> {
        let (d0, tod) = (start.div_euclid(DAY), start.rem_euclid(DAY));
        let hi_day = hi.div_euclid(DAY) + 1;
        let (mut out, mut seen) = (Vec::new(), 0u32);
        'periods: for k in 0.. {
            match period_first(rule, d0, k) {
                Some(first) if first <= hi_day => {}
                _ => break,
            }
            for day in period_days(rule, d0, k) {
                seen += 1;
                if rule.count.is_some_and(|c| seen > c) || day > hi_day {
                    break 'periods;
                }
                let wall = day * DAY + tod;
                if (lo..=hi).contains(&wall) {
                    out.push(wall);
                }
            }
        }
        out
    }

    #[test]
    fn a_count_that_cannot_run_out_before_the_window_costs_almost_nothing_and_the_answer_stays_exact() {
        let (lo, hi) = (w(2026, 10, 25, 0, 0), w(2026, 10, 30, 0, 0));
        let start = w(1601, 1, 1, 10, 0);
        for rule in [
            "FREQ=DAILY;COUNT=4000000000",
            "FREQ=WEEKLY;COUNT=4000000000;BYDAY=MO,TU,WE,TH,FR,SA,SU",
            "FREQ=MONTHLY;COUNT=4000000000;BYMONTHDAY=1,2,3,4,5,6,7,8,9,10",
            "FREQ=YEARLY;COUNT=4000000000;BYMONTH=1,2,3,4,5,6,7,8,9,10,11,12",
        ] {
            let (steps, _) = steps_of(rule, start, lo, hi);
            assert!(steps < 400, "{rule}: {steps} шагов");
        }
        assert_eq!(steps_of("FREQ=DAILY;COUNT=4000000000", start, lo, hi).1, 5, "25, 26, 27, 28 и 29 октября в 10:00");
        // Счёт, что кончается до окна: серия кончилась, а труд — не больше запаса шагов серии.
        for rule in [
            "FREQ=DAILY;COUNT=100000",
            "FREQ=WEEKLY;BYDAY=MO;COUNT=20000",
            "FREQ=MONTHLY;COUNT=4000;BYMONTHDAY=1,15",
            "FREQ=YEARLY;COUNT=300;BYMONTH=1,2,3,4,5,6,7,8,9,10,11,12;BYMONTHDAY=1,2,3,4,5,6,7,8,9,10",
        ] {
            let (steps, found) = steps_of(rule, start, lo, hi);
            assert!(steps <= SERIES_STEPS && found == 0, "{rule}: {steps} шагов, {found} вхождений");
        }
        // Пропуск с прыжком и без него — одно и то же, и у самой границы счёта.
        let from = w(2020, 1, 31, 9, 0);
        let (lo, hi) = (w(2026, 10, 20, 0, 0), w(2026, 11, 20, 0, 0));
        for base in [
            "FREQ=DAILY",
            "FREQ=DAILY;INTERVAL=3",
            "FREQ=WEEKLY",
            "FREQ=WEEKLY;BYDAY=MO,WE;INTERVAL=2;WKST=SU",
            "FREQ=MONTHLY",
            "FREQ=MONTHLY;BYDAY=2TU,-1FR",
            "FREQ=MONTHLY;BYMONTHDAY=-1,15",
            "FREQ=YEARLY",
            "FREQ=YEARLY;BYMONTH=1,10,11;BYMONTHDAY=31",
        ] {
            let all = parse_rule(base).unwrap();
            let total = expand_naive(&all, from, from, hi).len() as u32;
            let counts =
                [1, 2, 5, total.saturating_sub(2).max(1), total.saturating_sub(1).max(1), total, total + 1, total + 2];
            for count in counts {
                let rule = parse_rule(&format!("{base};COUNT={count}")).unwrap();
                let mut got = Vec::new();
                expand(&rule, from, None, lo, hi, &mut |wall| {
                    got.push(wall);
                    true
                });
                assert_eq!(got, expand_naive(&rule, from, lo, hi), "{base};COUNT={count}");
            }
        }
    }

    #[test]
    fn one_hostile_series_does_not_hide_the_meetings_of_the_owner() {
        let mut body = String::new();
        for i in 0..200 {
            body += &format!(
                "BEGIN:VEVENT\nUID:h{i}\nSUMMARY:h{i}\nDTSTART:17000101T100000Z\nDTEND:17000101T110000Z\nRRULE:FREQ=DAILY;COUNT=100000\nEND:VEVENT\n"
            );
        }
        body += "BEGIN:VEVENT\nUID:mine\nSUMMARY:Mine\nDTSTART:20261001T143000Z\nDTEND:20261001T150000Z\nRRULE:FREQ=DAILY\nEND:VEVENT\n";
        let c = parse(&body);
        let got = c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0);
        assert_eq!(got.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(), ["Mine"]);
    }

    #[test]
    fn the_numbers_of_a_rule_are_checked_without_a_panic_and_its_lists_are_bounded() {
        // Самые большие числа не роняют разбор (в отладочной сборке `abs()` от минимума падало).
        for bad in [
            "FREQ=MONTHLY;BYMONTHDAY=-9223372036854775808",
            "FREQ=MONTHLY;BYDAY=-9223372036854775808MO",
            "FREQ=MONTHLY;BYDAY=MO;BYSETPOS=-9223372036854775808",
            "FREQ=MONTHLY;BYMONTH=9223372036854775807",
            "FREQ=MONTHLY;BYMONTHDAY=9223372036854775807",
            "FREQ=MONTHLY;BYDAY=54MO",
            "FREQ=MONTHLY;BYSETPOS=367;BYDAY=MO",
        ] {
            assert!(parse_rule(bad).is_none(), "{bad}");
        }
        // 64 разных дней — можно (63 порядковых и «каждый понедельник»), 65-й — нет; повторы не считаются.
        let mut days: Vec<String> =
            (1..=9).flat_map(|n| ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].map(|d| format!("{n}{d}"))).collect();
        days.push("MO".into());
        assert_eq!(days.len(), MAX_BY_DAY);
        assert!(parse_rule(&format!("FREQ=MONTHLY;BYDAY={}", days.join(","))).is_some());
        days.push("10MO".into());
        assert!(parse_rule(&format!("FREQ=MONTHLY;BYDAY={}", days.join(","))).is_none());
        let repeats = format!("FREQ=WEEKLY;BYDAY={}TU", "MO,".repeat(5000));
        assert_eq!(parse_rule(&repeats).map(|r| r.by_day.len()), Some(2), "повторы сворачиваются");
        // Дни месяца: все 62 возможных помещаются; повторы свёрнуты.
        let month_days: Vec<String> = (1..=31).flat_map(|d| [d.to_string(), (-d).to_string()]).collect();
        assert_eq!(
            parse_rule(&format!("FREQ=MONTHLY;BYMONTHDAY={}", month_days.join(","))).map(|r| r.by_month_day.len()),
            Some(62)
        );
        // Места в наборе: 32 можно, 33-е — нет.
        let pos: Vec<String> = (1..=33).map(|p| p.to_string()).collect();
        assert!(parse_rule(&format!("FREQ=MONTHLY;BYDAY=MO;BYSETPOS={}", pos[..32].join(","))).is_some());
        assert!(parse_rule(&format!("FREQ=MONTHLY;BYDAY=MO;BYSETPOS={}", pos.join(","))).is_none());
        // Правило с негодным списком — серия показывается одним вхождением, а не пропадает и не вешает.
        let c = parse(&format!(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Odd\nDTSTART:20261027T100000Z\nDTEND:20261027T110000Z\nRRULE:FREQ=WEEKLY;BYDAY={}\nEND:VEVENT\n",
            days.join(",")
        ));
        assert_eq!(c.meetings(0, i64::MAX / 2, &utc0).len(), 1);
    }

    #[test]
    fn the_zones_and_their_lists_of_a_foreign_file_are_bounded() {
        // Поясов в файле не больше 64: у остальных время читается как местное, даже если им дали VTIMEZONE.
        let mut body = String::new();
        for i in 0..=MAX_ZONES {
            body += &format!(
                "BEGIN:VTIMEZONE\nTZID:Zone{i}\nBEGIN:STANDARD\nDTSTART:19700101T000000\nTZOFFSETFROM:+0500\nTZOFFSETTO:+0500\nEND:STANDARD\nEND:VTIMEZONE\n"
            );
        }
        body += "BEGIN:VEVENT\nUID:a\nSUMMARY:First\nDTSTART;TZID=Zone0:20261027T100000\nEND:VEVENT\n";
        body +=
            &format!("BEGIN:VEVENT\nUID:b\nSUMMARY:Beyond\nDTSTART;TZID=Zone{MAX_ZONES}:20261027T100000\nEND:VEVENT\n");
        let c = parse(&body);
        assert_eq!(c.zones.len(), MAX_ZONES);
        let by: std::collections::HashMap<String, i64> =
            c.meetings(0, i64::MAX / 2, &plus3).into_iter().map(|m| (m.title, m.start)).collect();
        assert_eq!(by["First"], w(2026, 10, 27, 5, 0), "пояс из файла: +05");
        assert_eq!(by["Beyond"], w(2026, 10, 27, 7, 0), "сверх предела — местное время ПК: +03");
        // Наблюдений в поясе не больше 64, дат RDATE в наблюдении — 256.
        let mut zone = String::from(
            "BEGIN:VTIMEZONE\nTZID:Big\nBEGIN:DAYLIGHT\nDTSTART:19700101T000000\nTZOFFSETFROM:+0100\nTZOFFSETTO:+0200\n",
        );
        zone += &format!(
            "RDATE:{}\nEND:DAYLIGHT\n",
            (0..300).map(|i| format!("{}0101T000000", 1700 + i)).collect::<Vec<_>>().join(",")
        );
        for i in 0..(MAX_OBSERVANCES + 6) {
            zone += &format!(
                "BEGIN:STANDARD\nDTSTART:{:04}0101T000000\nTZOFFSETFROM:+0100\nTZOFFSETTO:+0100\nEND:STANDARD\n",
                1900 + i
            );
        }
        zone += "END:VTIMEZONE\n";
        let c = parse(&format!("{zone}BEGIN:VEVENT\nUID:x\nSUMMARY:x\nDTSTART;TZID=Big:20261027T100000\nEND:VEVENT\n"));
        let rules = c.zones[0].rules.as_ref().unwrap();
        assert_eq!(rules.obs.len(), MAX_OBSERVANCES);
        assert_eq!(rules.obs[0].rdates.len(), MAX_RDATES, "из 300 дат RDATE взято 256");
        // Имя пояса в журнале — не длиннее 64 знаков.
        let long = "Z".repeat(300);
        let c = parse(&format!("BEGIN:VEVENT\nUID:1\nSUMMARY:x\nDTSTART;TZID={long}:20261027T100000\nEND:VEVENT\n"));
        assert_eq!(c.unknown_zones(), ["Z".repeat(MAX_ZONE_NAME)]);
    }

    #[test]
    fn a_long_value_is_skipped_but_the_description_is_read_from_its_start() {
        let big = "x".repeat(MAX_VALUE + 1);
        let c = parse(&format!(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:{big}\nLOCATION:{big}\nDTSTART:20261027T100000Z\nDTEND:20261027T110000Z\nEND:VEVENT\n"
        ));
        let m = &c.meetings(0, i64::MAX / 2, &utc0)[0];
        assert!(m.title.is_empty() && m.place.is_empty(), "значение длиннее 8 КБ пропущено целиком");
        let c = parse(&format!(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:{}\nDTSTART:20261027T100000Z\nEND:VEVENT\n",
            "y".repeat(MAX_VALUE)
        ));
        assert!(!c.meetings(0, i64::MAX / 2, &utc0)[0].title.is_empty(), "ровно 8 КБ — читается");
        // Описание бывает длинным: ссылка в начале находится, а за 64 КБ — нет.
        let head = format!("Join https://us02web.zoom.us/j/1 {}", "z ".repeat(MAX_DESCRIPTION));
        let tail = format!("{} Join https://us02web.zoom.us/j/1", "z ".repeat(MAX_DESCRIPTION));
        for (description, found) in [(head, true), (tail, false)] {
            let c = parse(&format!(
                "BEGIN:VEVENT\nUID:1\nSUMMARY:x\nDTSTART:20261027T100000Z\nDESCRIPTION:{description}\nEND:VEVENT\n"
            ));
            assert_eq!(c.meetings(0, i64::MAX / 2, &utc0)[0].link.is_some(), found);
        }
        // Вложение в десять мегабайт — не наше дело: разбор идёт дальше.
        let c = parse(&format!(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:x\nATTACH;ENCODING=BASE64;VALUE=BINARY:{}\nDTSTART:20261027T100000Z\nEND:VEVENT\n",
            "QUJD".repeat(2_500_000)
        ));
        assert_eq!(c.meetings(0, i64::MAX / 2, &utc0).len(), 1);
    }

    #[test]
    fn a_link_is_looked_for_in_bounded_steps_and_never_cut() {
        // Описание из одних «https://»: поиск не растёт квадратом от длины.
        let started = std::time::Instant::now();
        assert_eq!(find_link(&"https://".repeat(20_000)), None);
        let c = parse(&format!(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:x\nDTSTART:20261027T100000Z\nDESCRIPTION:{}\nEND:VEVENT\n",
            "https://".repeat(130_000)
        ));
        assert!(c.meetings(0, i64::MAX / 2, &utc0)[0].link.is_none());
        assert!(started.elapsed().as_secs() < 30, "поиск ссылки ограничен: {:?}", started.elapsed());
        // Адрес ровно предельной длины годен, на знак длиннее — нет, и обрезком его не принимают, даже с точкой на конце.
        let exact = format!("https://zoom.us/j/{}", "a".repeat(MAX_LINK - 18));
        assert_eq!(exact.len(), MAX_LINK);
        assert!(find_link(&exact).is_some());
        assert!(find_link(&format!("{exact}a")).is_none());
        assert!(
            find_link(&format!("https://zoom.us/j/{}...", "a".repeat(MAX_LINK - 18 + 1))).is_none(),
            "обрезок не годится"
        );
        // За длинным негодным адресом находится следующий годный.
        assert_eq!(
            find_link(&format!("https://zoom.us/{} https://zoom.us/j/2", "b".repeat(5000))).map(|l| l.url).as_deref(),
            Some("https://zoom.us/j/2")
        );
    }

    #[test]
    fn evil_hosts_with_a_service_name_after_a_separator_are_not_links() {
        for bad in [
            "https://evil.example\\.zoom.us/",
            "https://evil.example#.zoom.us",
            "https://evil.example?.zoom.us/",
            "https://evil.example/.zoom.us",
        ] {
            assert_eq!(check_link(bad), None, "{bad:?}");
            assert_eq!(find_link(bad), None, "{bad:?} в тексте");
            assert_eq!(find_link(&format!("Join: {bad} today")), None, "{bad:?} в предложении");
        }
    }

    #[test]
    fn exdates_are_bounded_resolved_once_and_compared_in_the_zone_of_the_series() {
        // Исключений у события не больше 1000; пояс на всё свойство — один.
        let values: Vec<String> = (0..1500).map(|i| format!("{:04}0101T100000", 1700 + i % 300)).collect();
        let lines: String = values
            .chunks(400)
            .map(|c| {
                format!(
                    "EXDATE;TZID=Somewhere:{}
",
                    c.join(",")
                )
            })
            .collect();
        let c = parse(&format!(
            "BEGIN:VEVENT
UID:1
SUMMARY:x
DTSTART;TZID=Somewhere:20261001T100000
RRULE:FREQ=DAILY
{lines}END:VEVENT
"
        ));
        assert_eq!(c.events[0].exdates.len(), MAX_EXDATES);
        assert_eq!(c.zones.len(), 1, "пояс «Somewhere» — один на все исключения");
        // В поясе серии исключение сверяется по стенному времени: 10:00 в Москве выпало, 11:00 осталось.
        let c = parse(&format!(
            "{ZONES}BEGIN:VEVENT\nUID:1\nSUMMARY:Daily\nDTSTART;TZID=Europe/Moscow:20261025T100000\nDTEND;TZID=Europe/Moscow:20261025T103000\nRRULE:FREQ=DAILY\nEXDATE;TZID=Europe/Moscow:20261027T100000\nEXDATE;TZID=Europe/Moscow:20261028T110000\nEND:VEVENT\n"
        ));
        let days: Vec<i64> = c
            .meetings(w(2026, 10, 25, 0, 0), w(2026, 10, 30, 0, 0), &utc0)
            .into_iter()
            .map(|m| m.start.div_euclid(DAY) - days_from_civil(2026, 10, 25))
            .collect();
        assert_eq!(days, [0, 1, 3, 4], "27 октября выпало, 28-е с исключением на другое время — осталось");
        // Исключение в UTC у серии в поясе тоже находит свой повтор (10:00 в Москве — 07:00 UTC).
        let c = parse(&format!(
            "{ZONES}BEGIN:VEVENT\nUID:1\nSUMMARY:Daily\nDTSTART;TZID=Europe/Moscow:20261025T100000\nRRULE:FREQ=DAILY\nEXDATE:20261026T070000Z\nEND:VEVENT\n"
        ));
        let got = c.meetings(w(2026, 10, 25, 0, 0), w(2026, 10, 28, 0, 0), &utc0);
        assert_eq!(got.iter().map(|m| m.start).collect::<Vec<_>>(), [w(2026, 10, 25, 7, 0), w(2026, 10, 27, 7, 0)]);
        // Далёкие исключения окну не мешают и ничего не стоят.
        let far: Vec<String> = (0..450).map(|i| format!("{:04}0101T070000Z", 1700 + i % 300)).collect();
        let c = parse(&format!(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Daily\nDTSTART:20261025T070000Z\nRRULE:FREQ=DAILY\nEXDATE:{}\nEND:VEVENT\n",
            far.join(",")
        ));
        assert_eq!(c.meetings(w(2026, 10, 25, 0, 0), w(2026, 10, 28, 0, 0), &utc0).len(), 3);
    }

    #[test]
    fn a_very_long_event_is_not_a_meeting_and_a_series_fills_the_window_only_so_far() {
        // Событие длиннее месяца — отпуск, а не встреча.
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Holiday\nDTSTART:20261001T080000Z\nDTEND:20261201T080000Z\nEND:VEVENT\nBEGIN:VEVENT\nUID:2\nSUMMARY:Trip\nDTSTART:20261026T080000Z\nDTEND:20261030T080000Z\nEND:VEVENT\n",
        );
        let got = c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0);
        assert_eq!(got.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(), ["Trip"], "четыре дня — ещё встреча");
        // Серия каждый день с длительностью в миллион недель не заполняет память.
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Huge\nDTSTART:16010101T080000Z\nDURATION:P1000000W\nRRULE:FREQ=DAILY\nEND:VEVENT\n",
        );
        assert!(c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0).is_empty());
        // А тридцать дней в каждом повторе — допустимо, и вхождений в окне не больше предела.
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Long\nDTSTART:20260101T080000Z\nDURATION:P30D\nRRULE:FREQ=DAILY\nEND:VEVENT\n",
        );
        let n = c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0).len();
        assert!(n > 0 && n <= MAX_PER_SERIES, "{n}");
    }

    #[test]
    fn what_the_owner_declined_is_not_a_meeting() {
        let owner = "X-WR-CALNAME:Me@Example.com\n";
        let event = |uid: &str, who: &str, state: &str| {
            format!(
                "BEGIN:VEVENT\nUID:{uid}\nSUMMARY:{uid}\nDTSTART:20261027T100000Z\nDTEND:20261027T110000Z\nATTENDEE;CN=\"Some One\";PARTSTAT={state};RSVP=TRUE:mailto:{who}\nEND:VEVENT\n"
            )
        };
        let body = format!(
            "{owner}{}{}{}{}",
            event("declined", "me@example.com", "DECLINED"),
            event("accepted", "me@example.com", "ACCEPTED"),
            event("other-declined", "colleague@example.com", "DECLINED"),
            event("shouted", "ME@EXAMPLE.COM", "declined"),
        );
        let got: Vec<String> = parse(&body).meetings(0, i64::MAX / 2, &utc0).into_iter().map(|m| m.title).collect();
        assert_eq!(got, ["accepted", "other-declined"], "отклонил сам владелец — события нет (регистр не важен)");
        // Календарь без имени — нечего сверять: всё остаётся.
        let no_name = body.replacen(owner, "", 1);
        assert_eq!(parse(&no_name).meetings(0, i64::MAX / 2, &utc0).len(), 4);
        // Отклонён один повтор серии: он отменён, остальные идут; отклонена вся серия — её нет.
        let series = format!(
            "{owner}BEGIN:VEVENT\nUID:s\nSUMMARY:Daily\nDTSTART:20261026T100000Z\nDTEND:20261026T103000Z\nRRULE:FREQ=DAILY\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:s\nSUMMARY:Daily\nRECURRENCE-ID:20261027T100000Z\nDTSTART:20261027T100000Z\nDTEND:20261027T103000Z\nATTENDEE;PARTSTAT=DECLINED:mailto:me@example.com\nEND:VEVENT\n"
        );
        let got = parse(&series).meetings(w(2026, 10, 26, 0, 0), w(2026, 10, 29, 0, 0), &utc0);
        assert_eq!(
            got.iter().map(|m| m.start.div_euclid(DAY) - days_from_civil(2026, 10, 26)).collect::<Vec<_>>(),
            [0, 2],
            "27 октября отклонено, остальные дни идут"
        );
        let whole = format!(
            "{owner}BEGIN:VEVENT\nUID:s\nSUMMARY:Daily\nDTSTART:20261026T100000Z\nRRULE:FREQ=DAILY\nATTENDEE;PARTSTAT=DECLINED:mailto:me@example.com\nEND:VEVENT\n"
        );
        assert!(parse(&whole).meetings(w(2026, 10, 26, 0, 0), w(2026, 10, 29, 0, 0), &utc0).is_empty());
    }

    /// Старая проверка перехода часов: сутки до и после, обе кандидатуры.
    fn wall_to_utc_reference(wall: i64, offset_at: &dyn Fn(i64) -> i32) -> i64 {
        let before = i64::from(offset_at(wall - DAY));
        let after = i64::from(offset_at(wall + DAY));
        let (a, b) = (wall - before, wall - after);
        let good_a = i64::from(offset_at(a)) == before;
        let good_b = i64::from(offset_at(b)) == after;
        match (good_a, good_b) {
            (true, true) => a.min(b),
            (false, true) => b,
            _ => a,
        }
    }

    /// Пояс ПК, похожий на берлинский: UTC+1, летом UTC+2 (с последнего воскресенья марта по последнее воскресенье октября).
    fn berlin(utc: i64) -> i32 {
        let year = civil_from_days(utc.div_euclid(DAY)).0;
        let last_sunday = |m: i64| {
            let last = days_from_civil(year, m, 31);
            last - (weekday(last) - 6).rem_euclid(7)
        };
        if (last_sunday(3) * DAY + 3600..last_sunday(10) * DAY + 3600).contains(&utc) { 7200 } else { 3600 }
    }

    #[test]
    fn the_shortcut_of_the_wall_clock_conversion_gives_the_old_answers() {
        let zones: [&dyn Fn(i64) -> i32; 3] = [&utc0, &plus3, &berlin];
        for offset_at in zones {
            for wall in (w(2026, 1, 1, 0, 0)..w(2027, 1, 1, 0, 0)).step_by(15 * 60) {
                assert_eq!(wall_to_utc(wall, offset_at), wall_to_utc_reference(wall, offset_at), "{wall}");
            }
        }
    }

    /// Ответы готового списка переходов и ответы по наблюдениям — одни и те же, в том числе у самих границ и вне списка.
    fn check_table(zone: &str, step: usize) {
        let now = w(2026, 10, 27, 0, 0);
        let c = Calendar::parse(
            &cal(&format!("{zone}BEGIN:VEVENT\nUID:x\nSUMMARY:x\nDTSTART:20261028T100000Z\nEND:VEVENT\n")),
            now,
        )
        .unwrap();
        let all: Vec<&TzRules> = c.zones.iter().filter_map(|z| z.rules.as_ref()).collect();
        assert!(!all.is_empty(), "пояс с правилами");
        for rules in all {
            let table = rules.table.as_ref().expect("готовый список переходов");
            assert!(table.lo <= now && now < table.hi);
            let mut probes: Vec<i64> = (table.lo - 400 * DAY..table.hi + 400 * DAY).step_by(step).collect();
            for (at, _) in &table.steps {
                probes.extend([at - 1, *at, at + 1]);
            }
            probes.extend([table.lo - 1, table.lo, table.lo + 1, table.hi - 1, table.hi, table.hi + 1]);
            for u in probes {
                assert_eq!(rules.offset_at(u), rules.offset_slow(u), "{u}");
            }
        }
    }

    #[test]
    fn the_transition_table_answers_like_the_rules_of_the_zone() {
        // Нью-Йорк и Москва из образца выше.
        check_table(ZONES, 3_001);
        // Берлин: последнее воскресенье марта и октября.
        check_table(
            "BEGIN:VTIMEZONE\nTZID:Europe/Berlin\nBEGIN:DAYLIGHT\nTZOFFSETFROM:+0100\nTZOFFSETTO:+0200\nDTSTART:19700329T020000\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\nEND:DAYLIGHT\nBEGIN:STANDARD\nTZOFFSETFROM:+0200\nTZOFFSETTO:+0100\nDTSTART:19701025T030000\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\nEND:STANDARD\nEND:VTIMEZONE\n",
            3_001,
        );
        // Сидней: лето — с октября по апрель.
        check_table(
            "BEGIN:VTIMEZONE\nTZID:Australia/Sydney\nBEGIN:DAYLIGHT\nTZOFFSETFROM:+1000\nTZOFFSETTO:+1100\nDTSTART:19701004T020000\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=1SU\nEND:DAYLIGHT\nBEGIN:STANDARD\nTZOFFSETFROM:+1100\nTZOFFSETTO:+1000\nDTSTART:19700405T030000\nRRULE:FREQ=YEARLY;BYMONTH=4;BYDAY=1SU\nEND:STANDARD\nEND:VTIMEZONE\n",
            3_001,
        );
        // Только RDATE, без правил (так пишут некоторые экспортёры).
        check_table(
            "BEGIN:VTIMEZONE\nTZID:Odd/Dates\nBEGIN:STANDARD\nTZOFFSETFROM:+0100\nTZOFFSETTO:+0100\nDTSTART:20180101T000000\nEND:STANDARD\nBEGIN:DAYLIGHT\nTZOFFSETFROM:+0100\nTZOFFSETTO:+0000\nDTSTART:20250316T020000\nRDATE:20250316T020000,20260315T020000,20270314T020000,20280312T020000\nEND:DAYLIGHT\nBEGIN:STANDARD\nTZOFFSETFROM:+0000\nTZOFFSETTO:+0100\nDTSTART:20251026T030000\nRDATE:20251026T030000,20261025T030000,20271031T030000\nEND:STANDARD\nEND:VTIMEZONE\n",
            3_001,
        );
        // Правило с концом (UNTIL) в 2025 и новое с 2026 года, как в истории настоящих поясов.
        check_table(
            "BEGIN:VTIMEZONE\nTZID:Odd/Until\nBEGIN:STANDARD\nTZOFFSETFROM:+0300\nTZOFFSETTO:+0200\nDTSTART:19961027T030000\nRRULE:FREQ=YEARLY;UNTIL=20251231T000000Z;BYMONTH=10;BYDAY=-1SU\nEND:STANDARD\nBEGIN:STANDARD\nTZOFFSETFROM:+0200\nTZOFFSETTO:+0300\nDTSTART:20260101T000000\nEND:STANDARD\nBEGIN:DAYLIGHT\nTZOFFSETFROM:+0200\nTZOFFSETTO:+0300\nDTSTART:19700329T020000\nRRULE:FREQ=YEARLY;UNTIL=20251231T000000Z;BYMONTH=3;BYDAY=-1SU\nEND:DAYLIGHT\nEND:VTIMEZONE\n",
            3_001,
        );
        // Чужой пояс из десятков наблюдений: ответы те же.
        let mut hostile = String::from("BEGIN:VTIMEZONE\nTZID:Hostile\n");
        for i in 0..MAX_OBSERVANCES {
            hostile += &format!(
                "BEGIN:STANDARD\nDTSTART:{:04}0101T000000\nTZOFFSETFROM:+0100\nTZOFFSETTO:+{:02}00\nRRULE:FREQ=YEARLY;BYMONTH={};BYDAY=-1SU\nEND:STANDARD\n",
                1900 + 2 * i,
                1 + i % 5,
                1 + i % 12,
            );
        }
        hostile += "END:VTIMEZONE\n";
        check_table(&hostile, 40_009);
        // Список не больше предела; пояс сверх него остаётся без списка и считается по наблюдениям.
        let c = parse(&format!("{ZONES}BEGIN:VEVENT\nUID:x\nSUMMARY:x\nDTSTART:20261028T100000Z\nEND:VEVENT\n"));
        assert!(c.zones.iter().all(|z| z.rules.as_ref().is_none_or(|r| r.table.is_some())));
    }

    #[test]
    fn a_hostile_calendar_costs_a_bounded_amount_of_work() {
        // Пояс из многих наблюдений и сотня серий разных видов: у каждой — свой запас шагов, у пояса — готовый список переходов.
        let mut body = String::from("BEGIN:VTIMEZONE\nTZID:Hostile\n");
        for i in 0..(MAX_OBSERVANCES + 20) {
            body += &format!(
                "BEGIN:STANDARD\nDTSTART:{:04}0101T000000\nTZOFFSETFROM:+0300\nTZOFFSETTO:+0300\nRRULE:FREQ=YEARLY;BYMONTH={};BYDAY=-1SU\nEND:STANDARD\n",
                1700 + i,
                1 + i % 12
            );
        }
        body += "END:VTIMEZONE\n";
        let ords: Vec<String> =
            (1..=9).flat_map(|n| ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].map(|d| format!("{n}{d}"))).collect();
        let kinds = [
            "FREQ=DAILY;COUNT=4000000000".to_string(),
            "FREQ=DAILY;COUNT=100000".to_string(),
            "FREQ=WEEKLY;COUNT=20000;BYDAY=MO,TU,WE,TH,FR,SA,SU".to_string(),
            "FREQ=MONTHLY;COUNT=4000;BYMONTHDAY=1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,-1,-2,-3"
                .to_string(),
            format!("FREQ=MONTHLY;COUNT=4000;BYDAY={}", ords.join(",")),
            format!("FREQ=YEARLY;COUNT=250;BYMONTH=1,2,3,4,5,6,7,8,9,10,11,12;BYDAY={}", ords.join(",")),
        ];
        let exdates: Vec<String> = (0..MAX_EXDATES).map(|i| format!("202610{:02}T100000", 1 + i % 28)).collect();
        let exdate_lines: String = exdates
            .chunks(400)
            .map(|c| {
                format!(
                    "EXDATE:{}
",
                    c.join(",")
                )
            })
            .collect();
        for i in 0..120 {
            body += &format!(
                "BEGIN:VEVENT
UID:h{i}
SUMMARY:H
DTSTART;TZID=Hostile:16010101T100000
DTEND;TZID=Hostile:16010101T110000
RRULE:{}
{exdate_lines}END:VEVENT
",
                kinds[i % kinds.len()],
            );
        }
        let c = Calendar::parse(&cal(&body), w(2026, 10, 27, 9, 0)).unwrap();
        let started = std::time::Instant::now();
        let _ = c.meetings(w(2026, 10, 26, 0, 0), w(2026, 10, 31, 0, 0), &utc0);
        // Настоящий замер на 2000 таких серий (release) — в отчёте этапа; здесь запас в десятки раз, чтобы не зависеть от нагрузки.
        assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
        // Каждый вид правила стоит не больше своего запаса шагов.
        let start = w(1601, 1, 1, 10, 0);
        for rule in &kinds {
            let (steps, _) = steps_of(rule, start, w(2026, 10, 26, 0, 0), w(2026, 10, 31, 0, 0));
            assert!(steps <= SERIES_STEPS, "{rule}: {steps}");
        }
        // Правило пояса, которого не бывает у настоящих поясов (не годовое или со счётом), не читается: пояс — по первому переходу.
        let z = parse(
            "BEGIN:VTIMEZONE\nTZID:Hostile\nBEGIN:STANDARD\nDTSTART:16010101T000000\nTZOFFSETFROM:+0300\nTZOFFSETTO:+0300\nRRULE:FREQ=DAILY;COUNT=4000000000\nEND:STANDARD\nEND:VTIMEZONE\nBEGIN:VEVENT\nUID:1\nSUMMARY:Z\nDTSTART;TZID=Hostile:20261027T103000\nEND:VEVENT\n",
        );
        assert_eq!(z.meetings(0, i64::MAX / 2, &utc0)[0].start, w(2026, 10, 27, 7, 30));
    }

    #[test]
    fn hostile_input_does_not_hang() {
        // Бесконечное правило с огромным счётом далеко в прошлом: перебор ограничен.
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Long\nDTSTART:16010101T100000Z\nDTEND:16010101T110000Z\nRRULE:FREQ=DAILY;COUNT=4000000000\nEND:VEVENT\n",
        );
        let started = std::time::Instant::now();
        let _ = c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0);
        assert!(started.elapsed().as_secs() < 5, "перебор правила ограничен");
        // Правило, у которого нет ни одного дня (31 февраля), не вешает.
        let c = parse(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:None\nDTSTART:20260131T100000Z\nDTEND:20260131T110000Z\nRRULE:FREQ=MONTHLY;BYMONTH=2;BYMONTHDAY=31\nEND:VEVENT\n",
        );
        let started = std::time::Instant::now();
        assert!(c.meetings(w(2026, 10, 27, 0, 0), w(2026, 10, 28, 0, 0), &utc0).is_empty());
        assert!(started.elapsed().as_secs() < 5);
        // Длинная строка без двоеточия, кавычки без пары и обрыв посреди события.
        let junk = format!(
            "BEGIN:VCALENDAR\r\n{}\r\nX-A;B=\"c:d\r\nBEGIN:VEVENT\r\nUID:1\r\nDTSTART:20261027T100000Z\r\n",
            "x".repeat(100_000)
        );
        let c = Calendar::parse(&junk, 0).unwrap();
        assert!(c.meetings(0, i64::MAX / 2, &utc0).is_empty(), "событие без END не читается");
    }

    #[test]
    fn the_title_is_one_short_line() {
        let c = parse(&format!(
            "BEGIN:VEVENT\nUID:1\nSUMMARY:Line one\\nLine\\ttwo {}\nDTSTART:20261027T100000Z\nEND:VEVENT\n",
            "я".repeat(300)
        ));
        let m = c.meetings(0, i64::MAX / 2, &utc0);
        assert!(!m[0].title.contains('\n') && m[0].title.chars().count() <= MAX_TITLE);
        assert!(m[0].title.starts_with("Line one Line"), "{:?}", m[0].title);
    }
}
