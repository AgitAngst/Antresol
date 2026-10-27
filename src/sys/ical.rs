// Календарь по https (этап 2.5 «Встречи»): GET файла .ics по WinHTTP, как `gh.rs`, но хост и путь — из ссылки владельца.
// Ссылка — секрет: ни она, ни хост, ни путь не попадают в ошибки и в журнал; ошибка — короткий код («winhttp 0x…»,
// «http 404»). Перенаправления — по правилам WinHTTP: на https можно, с https на http нельзя (правило по умолчанию не
// ослаблено). Ответ — не больше 32 МБ (лишнее отрезается, об этом говорит `Got::truncated`), ждём не дольше полуминуты.

use std::time::{Duration, Instant};

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpAddRequestHeaders, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

/// Ответ больше не читаем: календарь тяжелее — только его начало. Рабочий календарь за годы, с участниками и вложениями,
/// бывает в десятки мегабайт (секретный адрес Google отдаёт всю историю); разбор линейный — 32 МБ это доли секунды.
pub const MAX_BODY: usize = 32 << 20;
/// Всего на один запрос: медленная отдача по капле не держит поток вечно.
const TOTAL: Duration = Duration::from_secs(30);

/// Почему не вышло. Текст — без адреса.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// Не соединились, не дождались, сервер ответил не так: код WinHTTP.
    Network(String),
    /// Сервер ответил кодом, что не 200: «http 404».
    Status(u32),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Network(code) => f.write_str(code),
            Fail::Status(code) => write!(f, "http {code}"),
        }
    }
}

/// Что получили.
pub struct Got {
    pub body: Vec<u8>,
    /// Ответ больше [`MAX_BODY`]: взято начало.
    pub truncated: bool,
}

fn code(e: windows::core::Error) -> Fail {
    Fail::Network(format!("winhttp {:#010x}", e.code().0 as u32))
}

/// GET `https://{host}:{port}{path}` → тело ответа 200.
pub fn get(host: &str, port: u16, path: &str) -> Result<Got, Fail> {
    let started = Instant::now();
    unsafe {
        let session =
            WinHttpOpen(w!("Antresol"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if session.is_null() {
            return Err(Fail::Network("winhttp open".into()));
        }
        let _ = WinHttpSetTimeouts(session, 5000, 5000, 5000, 10000);
        let result = (|| {
            let h = super::wide(host);
            let connect = WinHttpConnect(session, PCWSTR(h.as_ptr()), port, 0);
            if connect.is_null() {
                return Err(Fail::Network("winhttp connect".into()));
            }
            let p = super::wide(path);
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
                    return Err(Fail::Network("winhttp request".into()));
                }
                let headers: Vec<u16> =
                    "Accept: text/calendar, text/plain;q=0.8, */*;q=0.5\r\n".encode_utf16().collect();
                WinHttpAddRequestHeaders(request, &headers, 0x2000_0000).map_err(code)?;
                WinHttpSendRequest(request, None, None, 0, 0, 0).map_err(code)?;
                WinHttpReceiveResponse(request, std::ptr::null_mut()).map_err(code)?;
                let mut status = 0u32;
                let mut len = 4u32;
                WinHttpQueryHeaders(
                    request,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    PCWSTR::null(),
                    Some(&mut status as *mut u32 as *mut _),
                    &mut len,
                    std::ptr::null_mut(),
                )
                .map_err(code)?;
                if status != 200 {
                    return Err(Fail::Status(status));
                }
                let mut body = Vec::new();
                let mut buf = [0u8; 16384];
                let mut truncated = false;
                loop {
                    if started.elapsed() > TOTAL {
                        return Err(Fail::Network("timeout".into()));
                    }
                    let mut read = 0u32;
                    WinHttpReadData(request, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read).map_err(code)?;
                    if read == 0 {
                        break;
                    }
                    // Сверх предела не берём ни байта: память под лишнее не растёт.
                    let take = (read as usize).min(MAX_BODY - body.len());
                    body.extend_from_slice(&buf[..take]);
                    if take < read as usize {
                        truncated = true;
                        break;
                    }
                }
                Ok(Got { body, truncated })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Сеть: открытая страница (не календарь) — только чтобы проверить сам запрос: хост, путь, https, ответ 200 и тело. Не в
    /// общем прогоне — `cargo test -- --ignored`. Настоящие календари тестом не читаются.
    #[test]
    #[ignore]
    fn reads_a_public_page_over_https() {
        let got = get("example.com", 443, "/").expect("ответ 200");
        assert!(!got.truncated && String::from_utf8_lossy(&got.body).contains("Example Domain"));
        // Нет такой страницы — код ответа без адреса.
        assert_eq!(get("example.com", 443, "/no-such-page-for-antresol").err(), Some(Fail::Status(404)));
    }

    #[test]
    fn failures_name_a_code_and_never_an_address() {
        assert_eq!(Fail::Status(404).to_string(), "http 404");
        assert_eq!(Fail::Network("timeout".into()).to_string(), "timeout");
        assert_eq!(MAX_BODY, 32 * 1024 * 1024);
    }
}
