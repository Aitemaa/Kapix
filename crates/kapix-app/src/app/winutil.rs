//! Small Win32 helpers shared by every window host.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, POINT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateSolidBrush, DeleteObject, FillRect, HDC, HGDIOBJ,
};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    SETPROCESSWORKINGSETSIZEEX_FLAGS, SetProcessWorkingSetSizeEx,
};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, GetCursorPos, IDC_ARROW, LoadCursorW, RegisterClassExW, WNDCLASSEXW, WNDPROC,
};
use windows::core::PCWSTR;

use kapix_core::{Image, Point, Rect};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn hinstance() -> HINSTANCE {
    unsafe {
        GetModuleHandleW(None)
            .map(|m| HINSTANCE(m.0))
            .unwrap_or_default()
    }
}

/// Signed low/high words of an `LPARAM` (mouse coordinates may be negative).
pub fn lo(l: LPARAM) -> i32 {
    l.0 as i16 as i32
}
pub fn hi(l: LPARAM) -> i32 {
    (l.0 >> 16) as i16 as i32
}

pub fn register_class(name: PCWSTR, proc: WNDPROC) {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: proc,
            hInstance: hinstance(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: name,
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
}

pub fn cursor_pos() -> Point {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    Point::new(p.x, p.y)
}

pub fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) < 0 }
}

/// Top-down 32bpp BGRA header for a `w x h` pixel block.
pub fn bmi(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Fill a rect (client coords) with a solid `0x00BBGGRR` color.
pub fn fill(hdc: HDC, r: Rect, bgr: u32) {
    unsafe {
        let rc = windows::Win32::Foundation::RECT {
            left: r.x,
            top: r.y,
            right: r.right(),
            bottom: r.bottom(),
        };
        let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF(bgr));
        FillRect(hdc, &rc, br);
        let _ = DeleteObject(HGDIOBJ(br.0));
    }
}

pub fn image_bytes(img: &Image) -> *const core::ffi::c_void {
    img.data.as_ptr().cast()
}

/// PRD §4.2: give the working set back to the OS after big buffers were dropped.
pub fn trim_working_set() {
    unsafe {
        let _ = SetProcessWorkingSetSizeEx(
            GetCurrentProcess(),
            usize::MAX,
            usize::MAX,
            SETPROCESSWORKINGSETSIZEEX_FLAGS(0),
        );
    }
}

// ---- tracing (KAPIX_TRACE=1 -> OutputDebugStringW, view with DebugView) ----

static TRACE: AtomicBool = AtomicBool::new(false);
thread_local!(static T0: Cell<Option<Instant>> = const { Cell::new(None) });

pub fn init_trace() {
    TRACE.store(
        std::env::var_os("KAPIX_TRACE").is_some_and(|v| v == "1"),
        Ordering::Relaxed,
    );
}

/// Start a new latency measurement (hotkey received / selection released).
pub fn trace_start(label: &str) {
    if TRACE.load(Ordering::Relaxed) {
        T0.with(|t| t.set(Some(Instant::now())));
        trace(label);
    }
}

/// Log `label` with the time elapsed since [`trace_start`].
pub fn trace(label: &str) {
    if !TRACE.load(Ordering::Relaxed) {
        return;
    }
    let ms = T0
        .with(|t| t.get())
        .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0);
    let s = wide(&format!("kapix: {label} +{ms:.1} ms\n"));
    unsafe { OutputDebugStringW(PCWSTR(s.as_ptr())) }
}

/// The UI font: embedded Archivo, with Segoe UI (`%WINDIR%\Fonts`) as the fallback face for
/// characters Archivo lacks (non-Latin text in annotations and toasts). Missing Segoe UI only
/// loses that fallback.
pub fn load_font() -> kapix_ui::Font {
    kapix_ui::Font::new(kapix_win::paths::system_font("segoeui.ttf"))
}

/// Resolve the configured theme: `system` follows the Windows "app mode" setting.
pub fn palette(mode: kapix_core::config::ThemeMode) -> kapix_ui::Palette {
    kapix_ui::Palette::resolve(mode, kapix_win::theme::apps_use_light_theme)
}

/// `0x00BBGGRR` for GDI from an opaque color.
pub fn colorref(c: kapix_core::Rgba) -> u32 {
    (c.b as u32) << 16 | (c.g as u32) << 8 | c.r as u32
}

/// Run `f` on the UI thread's HWND-less helper: post a boxed message to `hwnd` (stored as
/// `isize` so it can cross threads). The receiver reclaims it with `Box::from_raw`.
pub fn post_boxed<T>(hwnd: isize, msg: u32, v: T) {
    use windows::Win32::Foundation::WPARAM;
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
    let p = Box::into_raw(Box::new(v));
    let h = HWND(hwnd as *mut core::ffi::c_void);
    if unsafe { PostMessageW(Some(h), msg, WPARAM(0), LPARAM(p as isize)) }.is_err() {
        drop(unsafe { Box::from_raw(p) });
    }
}

/// Window procedure of the hidden CLI clipboard-owner window.
pub extern "system" fn def_proc(
    h: HWND,
    m: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    unsafe { windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(h, m, w, l) }
}
