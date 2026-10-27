// GitHub: состояние последнего прогона CI и число открытых пулл-реквестов репозитория — только
// чтение, через REST API (`api.github.com`) по WinHTTP, без своих библиотек. CI отсюда не запускается.
// Без токена GitHub отвечает только про открытые репозитории и не чаще 60 раз в час; токен (свой,
// вписанный владельцем в `config.txt`) — для закрытых.

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpAddRequestHeaders, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

/// Последний прогон CI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    /// Номер прогона на GitHub (0 — не прочитался).
    pub id: u64,
    pub name: String,
    /// `queued`, `in_progress`, `completed`.
    pub status: String,
    /// `success`, `failure`, `cancelled`… — когда `completed`.
    pub conclusion: String,
    /// `2026-10-22T05:12:00Z`.
    pub updated: String,
    /// Ветка, на которой шёл прогон.
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Repo {
    pub name: String,
    pub run: Option<Run>,
    pub open_prs: Option<usize>,
    /// Что пошло не так с прогонами: «404», «403» (нет доступа или лимит), сеть.
    pub error: Option<String>,
    /// То же с пулл-реквестами: они читаются отдельно (и реже), и их сбой не мешает видеть прогоны.
    pub pr_error: Option<String>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// GET `https://api.github.com{path}` → (код, тело).
fn get(path: &str, token: Option<&str>) -> Result<(u32, String), String> {
    unsafe {
        let session =
            WinHttpOpen(w!("Antresol"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if session.is_null() {
            return Err("WinHttpOpen failed".into());
        }
        let _ = WinHttpSetTimeouts(session, 5000, 5000, 5000, 10000);
        let result = (|| {
            let connect = WinHttpConnect(session, w!("api.github.com"), 443, 0);
            if connect.is_null() {
                return Err("connect failed".to_string());
            }
            let p = wide(path);
            let request = WinHttpOpenRequest(
                connect,
                w!("GET"),
                PCWSTR(p.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            );
            let r = (|| {
                if request.is_null() {
                    return Err("request failed".to_string());
                }
                let mut headers =
                    "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n".to_string();
                if let Some(t) = token {
                    headers.push_str(&format!("Authorization: Bearer {t}\r\n"));
                }
                let h: Vec<u16> = headers.encode_utf16().collect();
                WinHttpAddRequestHeaders(request, &h, 0x2000_0000).map_err(|e| e.to_string())?;
                WinHttpSendRequest(request, None, None, 0, 0, 0).map_err(|e| e.to_string())?;
                WinHttpReceiveResponse(request, std::ptr::null_mut()).map_err(|e| e.to_string())?;
                let mut code = 0u32;
                let mut len = 4u32;
                WinHttpQueryHeaders(
                    request,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    PCWSTR::null(),
                    Some(&mut code as *mut u32 as *mut _),
                    &mut len,
                    std::ptr::null_mut(),
                )
                .map_err(|e| e.to_string())?;
                let mut body = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    let mut read = 0u32;
                    WinHttpReadData(request, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read)
                        .map_err(|e| e.to_string())?;
                    if read == 0 {
                        break;
                    }
                    body.extend_from_slice(&buf[..read as usize]);
                    if body.len() > 4 << 20 {
                        break;
                    }
                }
                Ok((code, String::from_utf8_lossy(&body).into_owned()))
            })();
            if !request.is_null() {
                let _ = WinHttpCloseHandle(request);
            }
            let _ = WinHttpCloseHandle(connect);
            r
        })();
        let _ = WinHttpCloseHandle(session);
        result
    }
}

/// Значение строкового поля `"key": "value"` (первое после `from`); `null` — пусто.
pub fn field(json: &str, key: &str, from: usize) -> Option<String> {
    let pat = format!("\"{key}\"");
    let at = json[from..].find(&pat)? + from + pat.len();
    let rest = json[at..].trim_start().strip_prefix(':')?.trim_start();
    if rest.starts_with("null") {
        return Some(String::new());
    }
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push(' '),
                't' => out.push(' '),
                other => out.push(other),
            },
            _ => out.push(c),
        }
    }
    None
}

/// Число в поле `"key": 123` (первое после `from`); не число или нет поля — `None`.
pub fn number(json: &str, key: &str, from: usize) -> Option<u64> {
    let pat = format!("\"{key}\"");
    let at = json[from..].find(&pat)? + from + pat.len();
    let rest = json[at..].trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Первый прогон из ответа `/actions/runs`; прогонов нет — `None`.
pub fn parse_run(json: &str) -> Option<Run> {
    let start = json.find("\"workflow_runs\"")?;
    let list = json[start..].find('[')? + start;
    if json[list + 1..].trim_start().starts_with(']') {
        return None;
    }
    let first = json[list..].find('{')? + list;
    Some(Run {
        // `id` — первое поле прогона; ветка — `head_branch`.
        id: number(json, "id", first).unwrap_or_default(),
        branch: field(json, "head_branch", first).unwrap_or_default(),
        name: field(json, "name", first).unwrap_or_default(),
        status: field(json, "status", first).unwrap_or_default(),
        conclusion: field(json, "conclusion", first).unwrap_or_default(),
        updated: field(json, "updated_at", first).unwrap_or_default(),
    })
}

/// Число пулл-реквестов в ответе `/pulls` — по полю `"number":` верхнего уровня у каждого.
pub fn count_prs(json: &str) -> usize {
    // У каждого PR поле "number" ровно одно на верхнем уровне объекта; вложенных "number" у PR нет.
    let (mut depth, mut count, mut i) = (0i32, 0usize, 0usize);
    let b = json.as_bytes();
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => {
                    // Ключ, а не строковое значение «number»: после кавычки — двоеточие.
                    if depth == 2 && json[i..].starts_with("\"number\"") && json[i + 8..].trim_start().starts_with(':')
                    {
                        count += 1;
                    }
                    in_str = true;
                }
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    count
}

/// Секунды с 1970 года по времени `2026-10-22T05:12:00Z` (UTC).
pub fn iso_to_unix(s: &str) -> Option<i64> {
    let (date, time) = s.trim_end_matches('Z').split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(|x| x.parse::<i64>());
    let (hh, mm, ss) = (t.next()?.ok()?, t.next()?.ok()?, t.next()?.ok()?);
    // Дни от 1970-01-01 по гражданскому календарю (алгоритм Хиннанта).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

/// Только последний прогон репозитория `owner/name` — один запрос: так следят за прогонами, пока панель спрятана (без токена
/// GitHub даёт 60 запросов в час). Число PR не читается (`open_prs` — `None`).
pub fn read_runs(repo: &str, token: Option<&str>) -> Repo {
    let mut out = Repo { name: repo.to_string(), ..Default::default() };
    match get(&format!("/repos/{repo}/actions/runs?per_page=1"), token) {
        Ok((200, body)) => out.run = parse_run(&body),
        Ok((code, _)) => out.error = Some(code.to_string()),
        Err(e) => out.error = Some(e),
    }
    out
}

/// Только открытые пулл-реквесты репозитория `owner/name` — один запрос; прогон не читается (`run` — `None`). Нужны карточке, и
/// читаются отдельно от прогонов, не чаще раза в 5 минут.
pub fn read_prs(repo: &str, token: Option<&str>) -> Repo {
    let mut out = Repo { name: repo.to_string(), ..Default::default() };
    match get(&format!("/repos/{repo}/pulls?state=open&per_page=100"), token) {
        Ok((200, body)) => out.open_prs = Some(count_prs(&body)),
        Ok((code, _)) => out.pr_error = Some(code.to_string()),
        Err(e) => out.pr_error = Some(e),
    }
    out
}

/// Прочитать репозиторий `owner/name`: последний прогон и открытые PR (два запроса; PR не читаются, если прогон не прочитался —
/// лимит или сеть ответили бы так же).
pub fn read(repo: &str, token: Option<&str>) -> Repo {
    let mut out = read_runs(repo, token);
    if out.error.is_none() {
        let prs = read_prs(repo, token);
        out.open_prs = prs.open_prs;
        out.pr_error = prs.pr_error;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNS: &str = r#"{"total_count":2,"workflow_runs":[{"id":1,"name":"CI","head_branch":"main","status":"completed","conclusion":"failure","updated_at":"2026-10-22T05:12:00Z","repository":{"name":"x"}},{"id":2,"name":"Old","status":"completed","conclusion":"success"}]}"#;

    #[test]
    fn first_run() {
        let r = parse_run(RUNS).unwrap();
        assert_eq!((r.name.as_str(), r.status.as_str(), r.conclusion.as_str()), ("CI", "completed", "failure"));
        assert_eq!(r.updated, "2026-10-22T05:12:00Z");
        assert_eq!((r.id, r.branch.as_str()), (1, "main"), "номер и ветка первого прогона, а не второго");
        assert!(parse_run(r#"{"total_count":0,"workflow_runs":[]}"#).is_none(), "прогонов нет");
    }

    /// Сеть: открытый репозиторий самой панели. Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reads_public_repo() {
        let r = read("AgitAngst/Antresol", None);
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(r.open_prs.is_some());
    }

    #[test]
    fn iso_time() {
        assert_eq!(iso_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_to_unix("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(iso_to_unix("2026-10-22T05:12:00Z"), Some(1_792_645_920));
        assert_eq!(iso_to_unix("nope"), None);
    }

    #[test]
    fn numbers_in_fields() {
        let j = r#"{"id": 7412, "node_id": "x", "run_number": 12, "big": 99999999999999999999, "s": "5"}"#;
        assert_eq!(number(j, "id", 0), Some(7412), "именно `id`, а не `node_id`");
        assert_eq!(number(j, "run_number", 0), Some(12));
        assert_eq!(number(j, "big", 0), None, "не влезло в число");
        assert_eq!(number(j, "s", 0), None, "строка — не число");
        assert_eq!(number(j, "nope", 0), None);
    }

    #[test]
    fn null_and_escapes() {
        let j = r#"{"a":null,"b":"x \"y\" z"}"#;
        assert_eq!(field(j, "a", 0).as_deref(), Some(""));
        assert_eq!(field(j, "b", 0).as_deref(), Some("x \"y\" z"));
        assert_eq!(field(j, "c", 0), None);
    }

    #[test]
    fn pr_count() {
        let pulls = r#"[{"number":3,"head":{"repo":{"id":1}},"title":"a \"number\": 9","label":"number"},{"number":4,"user":{"login":"x"}}]"#;
        assert_eq!(count_prs(pulls), 2);
        assert_eq!(count_prs("[]"), 0);
    }
}
