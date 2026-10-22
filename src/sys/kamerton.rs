// Связь с Kamerton — микшером громкости в трее (звук в самой панели не живёт). Kamerton слушает
// именованный канал `\\.\pipe\kamerton-ipc`: одна строка запроса, ответ до закрытия канала.
// Поля — через табуляцию, громкость — целые проценты. Протокол версии 1.

use std::io::{Read, Write};
use std::time::Duration;

const PIPE: &str = r"\\.\pipe\kamerton-ipc";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Device {
    pub name: String,
    pub volume: u32,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct App {
    pub key: String,
    pub name: String,
    pub volume: u32,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    pub version: String,
    /// Устройство вывода по умолчанию; нет — `None`.
    pub device: Option<Device>,
    pub apps: Vec<App>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Канала нет: Kamerton не запущен (или старый, без канала).
    NotRunning,
    /// Ответил ошибкой или непонятно.
    Failed(String),
}

/// Запрос к Kamerton: одна строка без `\n`, ответ целиком.
fn request(line: &str) -> Result<String, Error> {
    let mut pipe = None;
    // Канал занят другим клиентом — коротко подождать; так и не дождались — «занят», а не «не запущен».
    for _ in 0..5 {
        match std::fs::OpenOptions::new().read(true).write(true).open(PIPE) {
            Ok(p) => {
                pipe = Some(p);
                break;
            }
            Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(Duration::from_millis(40)),
            // Нет канала (2) — Kamerton не запущен; другое — ошибка с кодом.
            Err(e) if e.raw_os_error() == Some(2) => return Err(Error::NotRunning),
            Err(e) => return Err(Error::Failed(e.to_string())),
        }
    }
    let mut pipe = pipe.ok_or_else(|| Error::Failed("busy".into()))?;
    pipe.write_all(format!("{line}\n").as_bytes()).map_err(|e| Error::Failed(e.to_string()))?;
    let mut out = String::new();
    // Сервер отключает канал после ответа: «канал закрыт» (109) или «нет второй стороны» (233) — конец
    // ответа; прочитанное до него остаётся в буфере.
    // Ответ больше 1 МБ — не наш: дальше не читаем.
    let mut bytes = Vec::new();
    match (&mut pipe).take(1 << 20).read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(e) if matches!(e.raw_os_error(), Some(109 | 233)) && !bytes.is_empty() => {}
        Err(e) => return Err(Error::Failed(e.to_string())),
    }
    out.push_str(&String::from_utf8_lossy(&bytes));
    Ok(out)
}

/// Табуляции и переводы строк в поле — пробелы.
pub fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn flag(s: &str) -> bool {
    s == "1"
}

fn pct(s: &str) -> u32 {
    s.trim().parse::<u32>().unwrap_or(0).min(100)
}

/// Разобрать ответ на `state`.
pub fn parse_state(text: &str) -> Result<State, Error> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    if head.first() == Some(&"kamerton") && head.get(1) != Some(&"1") {
        return Err(Error::Failed(format!("protocol {}", head.get(1).unwrap_or(&"?"))));
    }
    if head.first() != Some(&"kamerton") {
        let msg = text.strip_prefix("err\t").unwrap_or(text).trim();
        return Err(Error::Failed(msg.to_string()));
    }
    let mut state = State { version: head.get(2).unwrap_or(&"").to_string(), ..Default::default() };
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["device", name, vol, muted, ..] => {
                state.device = Some(Device { name: name.to_string(), volume: pct(vol), muted: flag(muted) });
            }
            ["app", key, name, vol, muted, ..] => state.apps.push(App {
                key: key.to_string(),
                name: name.to_string(),
                volume: pct(vol),
                muted: flag(muted),
            }),
            ["end"] => break,
            _ => {}
        }
    }
    Ok(state)
}

/// Ответ на команду: `ok` или ошибка.
fn ok(text: &str) -> Result<(), Error> {
    let t = text.trim();
    if t == "ok" { Ok(()) } else { Err(Error::Failed(t.strip_prefix("err\t").unwrap_or(t).to_string())) }
}

pub fn state() -> Result<State, Error> {
    if super::demo() {
        return Ok(demo());
    }
    parse_state(&request("state")?)
}

/// Что сделать в Kamerton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Volume(u32),
    Mute(bool),
    AppVolume(String, u32),
    AppMute(String, bool),
    Open,
}

/// Строка запроса команды.
pub fn line(cmd: &Command) -> String {
    let b = |m: &bool| if *m { 1 } else { 0 };
    match cmd {
        Command::Volume(v) => format!("volume\t{}", v.min(&100)),
        Command::Mute(m) => format!("mute\t{}", b(m)),
        Command::AppVolume(k, v) => format!("app-volume\t{}\t{}", clean(k), v.min(&100)),
        Command::AppMute(k, m) => format!("app-mute\t{}\t{}", clean(k), b(m)),
        Command::Open => "open".to_string(),
    }
}

pub fn send(cmd: &Command) -> Result<(), Error> {
    if super::demo() {
        return Ok(());
    }
    ok(&request(&line(cmd))?)
}

/// Выдуманное состояние для снимков в открытый репозиторий.
fn demo() -> State {
    let app = |key: &str, name: &str, volume, muted| App { key: key.into(), name: name.into(), volume, muted };
    State {
        version: "0.2.1".into(),
        device: Some(Device { name: "Demo Speakers".into(), volume: 64, muted: false }),
        apps: vec![
            app("player.exe", "Demo Player", 80, false),
            app("browser.exe", "Demo Browser", 45, false),
            app("game.exe", "Demo Game", 100, true),
        ],
    }
}

/// Запустить Kamerton: путь из настроек.
pub fn launch(path: &str) -> Result<(), String> {
    super::launch(path, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_state() {
        let text =
            "kamerton\t1\t0.2.1\ndevice\tSpeakers\t64\t0\napp\tplayer.exe\tPlayer\t80\t1\napp\tx\ty\t250\t0\nend\n";
        let s = parse_state(text).unwrap();
        assert_eq!(s.version, "0.2.1");
        assert_eq!(s.device, Some(Device { name: "Speakers".into(), volume: 64, muted: false }));
        assert_eq!(s.apps.len(), 2);
        assert!(s.apps[0].muted);
        assert_eq!(s.apps[1].volume, 100, "больше 100 — 100");
        let none = parse_state("kamerton\t1\t0.2.1\nnodevice\nend\n").unwrap();
        assert_eq!(parse_state("kamerton\t2\tx\nend\n"), Err(Error::Failed("protocol 2".into())));
        assert!(none.device.is_none() && none.apps.is_empty());
        assert_eq!(parse_state("err\tboom\n"), Err(Error::Failed("boom".into())));
    }

    /// Живой Kamerton с каналом: только читает состояние. Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reads_running_kamerton() {
        let s = state().expect("Kamerton с каналом должен быть запущен");
        assert!(!s.version.is_empty());
    }

    #[test]
    fn command_lines() {
        assert_eq!(line(&Command::Volume(150)), "volume\t100");
        assert_eq!(line(&Command::Mute(true)), "mute\t1");
        assert_eq!(line(&Command::AppVolume("a\tb".into(), 30)), "app-volume\ta b\t30");
        assert_eq!(line(&Command::AppMute("k".into(), false)), "app-mute\tk\t0");
        assert_eq!(line(&Command::Open), "open");
        assert_eq!(ok("ok\n"), Ok(()));
        assert_eq!(ok("err\tno device"), Err(Error::Failed("no device".into())));
    }
}
