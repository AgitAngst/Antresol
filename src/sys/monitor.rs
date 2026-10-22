// Мониторы и курсор в пикселях экрана: где край, куда упёрлась мышь, не идёт ли игра во весь экран.

use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CLR_INVALID, EnumDisplayMonitors, GetDC, GetMonitorInfoW, GetPixel, HDC, HMONITOR, MONITORINFOEXW, ReleaseDC,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect,
    MONITORINFOF_PRIMARY,
};
use windows::core::BOOL;

/// Прямоугольник в физических пикселях экрана.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PxRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PxRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn contains(&self, (x, y): (i32, i32)) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

impl From<RECT> for PxRect {
    fn from(r: RECT) -> Self {
        PxRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// Имя устройства GDI: `\\.\DISPLAY1`.
    pub device: String,
    pub rect: PxRect,
    pub primary: bool,
    /// HMONITOR как число — для DDC/CI из другого потока; живёт, пока монитор подключён.
    pub handle: usize,
}

pub fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

/// Цвет пикселя экрана в точке (физические пиксели) как (r, g, b); не прочитать — `None`.
pub fn pixel_at(x: i32, y: i32) -> Option<(u8, u8, u8)> {
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let c = GetPixel(dc, x, y);
        ReleaseDC(None, dc);
        // COLORREF — 0x00BBGGRR.
        (c.0 != CLR_INVALID).then_some((c.0 as u8, (c.0 >> 8) as u8, (c.0 >> 16) as u8))
    }
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let out = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(h, &mut info.monitorInfo) }.as_bool() {
        let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
        out.push(Monitor {
            device: String::from_utf16_lossy(&info.szDevice[..end]),
            rect: info.monitorInfo.rcMonitor.into(),
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            handle: h.0 as usize,
        });
    }
    BOOL(1)
}

/// Все мониторы; основной — первым.
pub fn monitors() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by_key(|m| !m.primary);
    out
}

/// Окно на переднем плане занимает весь монитор — игра или видео во весь экран.
/// Рабочий стол и оболочку не считаем.
pub fn fullscreen_on(rect: PxRect) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() || fg == GetDesktopWindow() || fg == GetShellWindow() {
            return false;
        }
        let mut class = [0u16; 32];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if class == "WorkerW" || class == "Progman" {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let r = PxRect::from(r);
        r.left <= rect.left && r.top <= rect.top && r.right >= rect.right && r.bottom >= rect.bottom
    }
}

/// Имя файла программы на переднем плане, маленькими буквами: «chrome.exe». `None` — не узнать.
pub fn foreground_exe() -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    if super::demo() {
        return Some("player.exe".to_string());
    }
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Some(exe_name(&path))
    }
}

/// Имя файла из полного пути, маленькими буквами.
pub fn exe_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_names() {
        assert_eq!(exe_name(r"C:\Program Files\Google\Chrome\chrome.exe"), "chrome.exe");
        assert_eq!(exe_name("Game.EXE"), "game.exe");
    }

    #[test]
    fn rect_contains() {
        let r = PxRect { left: 0, top: 0, right: 100, bottom: 50 };
        assert!(r.contains((0, 0)) && r.contains((99, 49)));
        assert!(!r.contains((100, 10)) && !r.contains((10, -1)));
    }
}
