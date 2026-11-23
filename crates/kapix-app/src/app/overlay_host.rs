//! Win32 host for `kapix_ui::Overlay`: a borderless topmost window exactly covering the virtual
//! desktop. Client pixel (x, y) == overlay buffer coordinate (x, y).
//!
//! Re-entrancy: the session lives in a `RefCell` that is borrowed only around pure overlay
//! calls and plain GDI blits. Anything that can pump messages (`DestroyWindow`, clipboard,
//! toast creation, dialogs) runs after the borrow was released, in [`finish`].

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DIB_RGB_COLORS, EndPaint, GetDC, HDC, InvalidateRect, PAINTSTRUCT, ReleaseDC,
    ScreenToClient, SetDIBitsToDevice, UpdateWindow,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HCURSOR, IDC_ARROW, IDC_CROSS, IDC_HAND,
    IDC_IBEAM, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, LoadCursorW,
    MINMAXINFO, PostQuitMessage, SW_HIDE, SW_SHOW, SetCursor, SetForegroundWindow, ShowWindow,
    WM_CHAR, WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::w;

use kapix_core::{Config, Rect};
use kapix_ui::{
    CursorKind, Font, Input, Key, Mods, MouseButton, Outcome, Overlay, OverlayInit, OverlayMode,
};

use super::actions;
use super::winutil::{self, bmi, hi, lo};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sink {
    /// Act on the outcome in-process (clipboard, toast, ...).
    Daemon,
    /// Store the outcome and quit the CLI message loop.
    Cli,
}

struct Session {
    overlay: Overlay,
    hwnd: HWND,
    font: Font,
    sink: Sink,
    painted: bool,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static CLI_RESULT: RefCell<Option<Outcome>> = const { RefCell::new(None) };
    /// Terminal outcome produced by a button-down; applied on the matching button-up so the
    /// click is not leaked to the window underneath.
    static PENDING: RefCell<Option<Outcome>> = const { RefCell::new(None) };
    static BOUNDS: Cell<Rect> = const { Cell::new(Rect::new(0, 0, 0, 0)) };
    static HIGH_SURROGATE: Cell<u16> = const { Cell::new(0) };
}
static CURSORS: [AtomicIsize; 9] = [const { AtomicIsize::new(0) }; 9];

pub fn register() {
    winutil::register_class(w!("KapixOverlay"), Some(proc));
}

pub fn is_active() -> bool {
    SESSION.with(|s| s.try_borrow().map_or(true, |g| g.is_some()))
}

pub fn take_cli_result() -> Option<Outcome> {
    CLI_RESULT.with(|c| c.borrow_mut().take())
}

/// Capture the desktop and show the frozen overlay. Returns once the window is up; the
/// outcome is delivered later through the message loop.
pub fn start(mode: OverlayMode, gdi: bool, sink: Sink, cfg: &Config) -> Result<(), String> {
    if is_active() {
        return Ok(());
    }
    // Font parsing overlaps with the (slower) capture.
    let font_job = std::thread::spawn(winutil::load_font);
    let cap = if gdi {
        kapix_win::capture::capture_desktop_gdi()
    } else {
        kapix_win::capture::capture_desktop()
    }
    .map_err(|e| e.to_string())?;
    winutil::trace("capture done");
    let kapix_win::capture::Capture { frame, desktop, .. } = cap;
    let windows = if cfg.capture.window_snap {
        kapix_win::snap::window_rects(unsafe { GetCurrentProcessId() })
            .into_iter()
            .map(|r| desktop.rect_to_buffer(r))
            .collect()
    } else {
        Vec::new()
    };
    let font = font_job.join().unwrap_or_else(|_| Font::builtin());
    let cursor = desktop.to_buffer(winutil::cursor_pos());
    let b = desktop.bounds;
    let overlay = Overlay::new(OverlayInit {
        frame,
        desktop,
        windows,
        mode,
        default_action: cfg.capture.default_action,
        show_magnifier: cfg.capture.show_magnifier,
        window_snap: cfg.capture.window_snap,
        font: font.clone(),
        palette: winutil::palette(cfg.ui.theme),
        cursor: (cursor.x, cursor.y),
    });
    BOUNDS.with(|c| c.set(b));
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("KapixOverlay"),
            w!("Kapix"),
            WS_POPUP,
            b.x,
            b.y,
            b.w,
            b.h,
            None,
            None,
            Some(winutil::hinstance()),
            None,
        )
    }
    .map_err(|e| e.to_string())?;
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            overlay,
            hwnd,
            font,
            sink,
            painted: false,
        })
    });
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        let _ = UpdateWindow(hwnd); // synchronous first paint
    }
    Ok(())
}

/// Close the overlay, drop every buffer, then act on `out`.
fn finish(out: Outcome) {
    winutil::trace_start("selection released");
    PENDING.with(|p| p.borrow_mut().take());
    let Some(sess) = SESSION.with(|s| s.borrow_mut().take()) else {
        return;
    };
    unsafe {
        let _ = ReleaseCapture();
        let _ = ShowWindow(sess.hwnd, SW_HIDE);
        let _ = DestroyWindow(sess.hwnd);
    }
    let Session {
        overlay,
        font,
        sink,
        ..
    } = sess;
    #[allow(clippy::drop_non_drop)] // frees the frame + backbuffer right now (PRD 4.2)
    drop(overlay);
    match sink {
        Sink::Cli => {
            CLI_RESULT.with(|c| *c.borrow_mut() = Some(out));
            unsafe { PostQuitMessage(0) };
        }
        Sink::Daemon => {
            actions::handle(out, Some(font));
            winutil::trim_working_set();
        }
    }
}

fn mods() -> Mods {
    Mods {
        ctrl: winutil::key_down(0x11),
        shift: winutil::key_down(0x10),
        alt: winutil::key_down(0x12),
    }
}

/// Feed one input to the overlay, blit what changed, and finish on terminal outcomes.
fn send(input: Input) {
    let is_down = matches!(input, Input::MouseDown { .. });
    let out = SESSION.with(|s| {
        if PENDING.with(|p| p.borrow().is_some()) {
            return Outcome::None;
        }
        let Ok(mut g) = s.try_borrow_mut() else {
            return Outcome::None;
        };
        let Some(sess) = g.as_mut() else {
            return Outcome::None;
        };
        let out = sess.overlay.handle(input);
        if matches!(out, Outcome::Redraw)
            && let Some(d) = sess.overlay.paint()
        {
            unsafe {
                let dc = GetDC(Some(sess.hwnd));
                blit(dc, &sess.overlay, d);
                ReleaseDC(Some(sess.hwnd), dc);
            }
        }
        out
    });
    if out.is_terminal() {
        if is_down {
            PENDING.with(|p| *p.borrow_mut() = Some(out));
        } else {
            finish(out);
        }
    }
}

/// Blit `r` (buffer coords) of the overlay's top-down BGRA buffer to `hdc`.
fn blit(hdc: HDC, o: &Overlay, r: Rect) {
    let (w, h) = o.size();
    let Some(r) = r.intersect(&Rect::new(0, 0, w as i32, h as i32)) else {
        return;
    };
    let stride = w as usize * 4;
    let off = r.y as usize * stride;
    let buf = o.buffer();
    if buf.len() < off + r.h as usize * stride {
        return;
    }
    // Treat the rows from `r.y` on as a standalone top-down DIB of `r.h` rows: this avoids the
    // bottom-up origin quirk of ySrc/StartScan for negative-height DIBs.
    let info = bmi(w as i32, r.h);
    unsafe {
        SetDIBitsToDevice(
            hdc,
            r.x,
            r.y,
            r.w as u32,
            r.h as u32,
            r.x,
            0,
            0,
            r.h as u32,
            buf[off..].as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
        );
    }
}

fn on_paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    let rc = Rect::from_ltrb(
        ps.rcPaint.left,
        ps.rcPaint.top,
        ps.rcPaint.right,
        ps.rcPaint.bottom,
    );
    let (first, late) = SESSION.with(|s| {
        let Ok(mut g) = s.try_borrow_mut() else {
            return (false, None);
        };
        let Some(sess) = g.as_mut() else {
            return (false, None);
        };
        let late = sess.overlay.paint().filter(|d| !rc.contains_rect(d)); // pending compose
        blit(hdc, &sess.overlay, rc);
        (!std::mem::replace(&mut sess.painted, true), late)
    });
    unsafe {
        let _ = EndPaint(hwnd, &ps);
        if let Some(d) = late {
            let r = windows::Win32::Foundation::RECT {
                left: d.x,
                top: d.y,
                right: d.right(),
                bottom: d.bottom(),
            };
            let _ = InvalidateRect(Some(hwnd), Some(&r), false);
        }
    }
    if first {
        winutil::trace("overlay first paint");
    }
}

fn map_key(vk: u16) -> Key {
    match vk {
        0x1B => Key::Escape,
        0x0D => Key::Enter,
        0x08 => Key::Backspace,
        0x2E => Key::Delete,
        0x09 => Key::Tab,
        0x25 => Key::Left,
        0x26 => Key::Up,
        0x27 => Key::Right,
        0x28 => Key::Down,
        0x30..=0x39 | 0x41..=0x5A => Key::Char(vk as u8 as char),
        0x60..=0x69 => Key::Char((b'0' + (vk - 0x60) as u8) as char), // numpad digits
        _ => Key::Other,
    }
}

fn set_cursor() {
    let kind = SESSION.with(|s| {
        s.try_borrow()
            .ok()
            .and_then(|g| g.as_ref().map(|x| x.overlay.cursor()))
    });
    let (idx, id) = match kind {
        Some(CursorKind::Cross) => (1, IDC_CROSS),
        Some(CursorKind::Move) => (2, IDC_SIZEALL),
        Some(CursorKind::SizeNS) => (3, IDC_SIZENS),
        Some(CursorKind::SizeWE) => (4, IDC_SIZEWE),
        Some(CursorKind::SizeNWSE) => (5, IDC_SIZENWSE),
        Some(CursorKind::SizeNESW) => (6, IDC_SIZENESW),
        Some(CursorKind::IBeam) => (7, IDC_IBEAM),
        Some(CursorKind::Hand) => (8, IDC_HAND),
        _ => (0, IDC_ARROW),
    };
    let mut h = CURSORS[idx].load(Ordering::Relaxed);
    if h == 0 {
        h = unsafe { LoadCursorW(None, id) }.map_or(0, |c| c.0 as isize);
        CURSORS[idx].store(h, Ordering::Relaxed);
    }
    unsafe { SetCursor(Some(HCURSOR(h as *mut core::ffi::c_void))) };
}

/// WM_CHAR delivers UTF-16 units; join surrogate pairs and drop control characters.
fn decode_char(u: u16) -> Option<char> {
    if (0xD800..0xDC00).contains(&u) {
        HIGH_SURROGATE.with(|h| h.set(u));
        return None;
    }
    let c = if (0xDC00..0xE000).contains(&u) {
        let h = HIGH_SURROGATE.with(|h| h.replace(0));
        char::decode_utf16([h, u]).next()?.ok()?
    } else {
        char::from_u32(u as u32)?
    };
    (!c.is_control()).then_some(c)
}

fn editing() -> bool {
    SESSION.with(|s| {
        s.try_borrow()
            .is_ok_and(|g| g.as_ref().is_some_and(|x| x.overlay.is_editing_text()))
    })
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (x, y) = (lo(lp), hi(lp));
    let btn = |b| (b, mods());
    match msg {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => on_paint(hwnd),
        // Keep the exact virtual-desktop rect regardless of per-monitor DPI changes.
        WM_DPICHANGED => {}
        WM_GETMINMAXINFO => {
            let b = BOUNDS.with(|c| c.get());
            let mmi = unsafe { &mut *(lp.0 as *mut MINMAXINFO) };
            mmi.ptMaxSize = POINT { x: b.w, y: b.h };
            mmi.ptMaxPosition = POINT { x: b.x, y: b.y };
            mmi.ptMaxTrackSize = POINT { x: b.w, y: b.h };
        }
        WM_SETCURSOR if (lp.0 & 0xFFFF) == 1 => {
            set_cursor();
            return LRESULT(1);
        }
        WM_MOUSEMOVE => send(Input::MouseMove { x, y, mods: mods() }),
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let b = match msg {
                WM_LBUTTONDOWN => MouseButton::Left,
                WM_RBUTTONDOWN => MouseButton::Right,
                _ => MouseButton::Middle,
            };
            unsafe { SetCapture(hwnd) };
            let (button, mods) = btn(b);
            send(Input::MouseDown { x, y, button, mods });
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let b = match msg {
                WM_LBUTTONUP => MouseButton::Left,
                WM_RBUTTONUP => MouseButton::Right,
                _ => MouseButton::Middle,
            };
            if wp.0 & 0x13 == 0 {
                // MK_LBUTTON | MK_RBUTTON | MK_MBUTTON all released
                unsafe {
                    let _ = ReleaseCapture();
                }
            }
            if let Some(out) = PENDING.with(|p| p.borrow_mut().take()) {
                finish(out);
            } else {
                let (button, mods) = btn(b);
                send(Input::MouseUp { x, y, button, mods });
            }
        }
        WM_MOUSEWHEEL => {
            let mut p = POINT { x, y }; // screen coordinates
            unsafe {
                let _ = ScreenToClient(hwnd, &mut p);
            }
            let delta = (wp.0 >> 16) as u16 as i16 as f32 / 120.0;
            send(Input::Wheel {
                x: p.x,
                y: p.y,
                delta,
                mods: mods(),
            });
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => send(Input::KeyDown {
            key: map_key(wp.0 as u16),
            mods: mods(),
        }),
        WM_KEYUP | WM_SYSKEYUP => send(Input::KeyUp {
            key: map_key(wp.0 as u16),
            mods: mods(),
        }),
        WM_CHAR => {
            if editing()
                && let Some(c) = decode_char(wp.0 as u16)
            {
                send(Input::Char(c));
            }
        }
        WM_SYSCHAR => {}
        WM_CLOSE => finish(Outcome::Cancel),
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}
