//! Window snap candidates for the overlay.

use kapix_core::Rect;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};
use windows::core::BOOL;

struct Ctx {
    pid: u32,
    out: Vec<Rect>,
}

/// Extended frame bounds (no shadow margins), falling back to `GetWindowRect`.
pub(crate) fn frame_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: `r` is a valid RECT of the size we pass.
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut r).cast(),
            size_of::<RECT>() as u32,
        )
        .is_ok()
            || GetWindowRect(hwnd, &mut r).is_ok()
    };
    ok.then(|| Rect::from_ltrb(r.left, r.top, r.right, r.bottom))
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Ctx` passed by `window_rects`, alive for the whole enumeration.
    let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
    // SAFETY: plain queries on a window handle supplied by EnumWindows.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return true.into();
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&raw mut cloaked).cast(),
            size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return true.into();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == ctx.pid {
            return true.into();
        }
        let mut class = [0u16; 32];
        let n = GetClassNameW(hwnd, &mut class).max(0) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if class == "Progman" || class == "WorkerW" {
            return true.into();
        }
    }
    if let Some(r) = frame_rect(hwnd).filter(|r| !r.is_empty()) {
        ctx.out.push(r);
    }
    true.into()
}

/// Visible top-level windows in z-order (top-most first), **virtual** coordinates, using
/// `DWMWA_EXTENDED_FRAME_BOUNDS` (excludes invisible shadow margins). Excludes: invisible,
/// minimized/iconic, DWM-cloaked (`DWMWA_CLOAKED`, e.g. other virtual desktops / suspended UWP),
/// zero-area, tool windows with no title that cover the whole desktop (shell/wallpaper
/// `Progman`/`WorkerW`), and windows belonging to `exclude_pid` (Kapix itself).
/// Child-control snapping is out of scope.
pub fn window_rects(exclude_pid: u32) -> Vec<Rect> {
    let mut ctx = Ctx {
        pid: exclude_pid,
        out: Vec::new(),
    };
    // SAFETY: the callback only dereferences `ctx`, which outlives the synchronous enumeration.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM((&raw mut ctx) as isize));
    }
    ctx.out
}
