// Мониторы и курсор в пикселях экрана: где край, куда упёрлась мышь, не идёт ли игра во весь экран.

use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
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
}

pub fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
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
