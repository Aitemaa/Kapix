//! Pin-to-screen windows: borderless topmost reference images.
//!
//! Each window owns a `Box<Pin>` (via `GWLP_USERDATA`) holding its image; the proc only ever
//! takes `&Pin` (mutable state is in `Cell`s), so re-entrant messages sent by `SetWindowPos`
//! etc. cannot alias a `&mut`. The box is freed on `WM_NCDESTROY`.

use std::cell::Cell;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DIB_RGB_COLORS, EndPaint, HALFTONE, InvalidateRect, PAINTSTRUCT, SRCCOPY,
    SetBrushOrgEx, SetStretchBltMode, StretchDIBits,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect,
    GetWindowLongPtrW, LWA_ALPHA, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
    SetForegroundWindow, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WM_ERASEBKGND, WM_KEYDOWN, WM_MBUTTONUP, WM_MOUSEWHEEL, WM_NCDESTROY, WM_NCHITTEST,
    WM_NCLBUTTONDBLCLK, WM_NCMBUTTONUP, WM_PAINT, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};
use windows::core::w;

use kapix_core::{Image, Rect};
use kapix_ui::toast::{Toast, ToastKind};

use super::toast_host;
use super::winutil::{self, bmi};

struct Pin {
    image: Image,
    zoom: Cell<f32>,
    alpha: Cell<u8>,
}

pub fn register() {
    winutil::register_class(w!("KapixPin"), Some(proc));
}

/// Create a pin showing `image` at virtual rect `at` (takes ownership; freed on close).
pub fn create(image: Image, at: Rect) {
    if image.width == 0 || image.height == 0 {
        return;
    }
    let zoom = (at.w.max(1) as f32 / image.width as f32).clamp(0.1, 4.0);
    let pin = Box::into_raw(Box::new(Pin {
        image,
        zoom: Cell::new(zoom),
        alpha: Cell::new(255),
    }));
    let res = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            w!("KapixPin"),
            w!("Kapix pin"),
            WS_POPUP,
            at.x,
            at.y,
            at.w.max(1),
            at.h.max(1),
            None,
            None,
            Some(winutil::hinstance()),
            None,
        )
    };
    match res {
        Ok(hwnd) => unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, pin as isize);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        },
        Err(_) => drop(unsafe { Box::from_raw(pin) }),
    }
}

fn paint(hwnd: HWND, pin: &Pin) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (iw, ih) = (pin.image.width as i32, pin.image.height as i32);
        if (pin.zoom.get() - 1.0).abs() > 0.001 {
            SetStretchBltMode(hdc, HALFTONE);
            let _ = SetBrushOrgEx(hdc, 0, 0, None);
        }
        StretchDIBits(
            hdc,
            0,
            0,
            rc.right,
            rc.bottom,
            0,
            0,
            iw,
            ih,
            Some(winutil::image_bytes(&pin.image)),
            &bmi(iw, ih),
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        // 1 px accent border (#0EA5E9).
        let (w, h) = (rc.right, rc.bottom);
        for r in [
            Rect::new(0, 0, w, 1),
            Rect::new(0, h - 1, w, 1),
            Rect::new(0, 0, 1, h),
            Rect::new(w - 1, 0, 1, h),
        ] {
            winutil::fill(hdc, r, 0x00E9_A50E);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

fn wheel(hwnd: HWND, pin: &Pin, wp: WPARAM) {
    let notches = (wp.0 >> 16) as u16 as i16 as f32 / 120.0;
    unsafe {
        if wp.0 & 0x0008 != 0 {
            // Ctrl+wheel: opacity 20%..100%
            let a = (pin.alpha.get() as f32 + notches * 25.5)
                .round()
                .clamp(51.0, 255.0) as u8;
            pin.alpha.set(a);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), a, LWA_ALPHA);
        } else {
            let z = (pin.zoom.get() * 1.1f32.powf(notches)).clamp(0.1, 4.0);
            pin.zoom.set(z);
            let nw = (pin.image.width as f32 * z).round().max(1.0) as i32;
            let nh = (pin.image.height as f32 * z).round().max(1.0) as i32;
            // Top-left stays anchored (SWP_NOMOVE).
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                nw,
                nh,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn copy(hwnd: HWND, pin: &Pin) {
    let (w, h) = (pin.image.width, pin.image.height);
    let t = match kapix_win::clipboard::set_image(hwnd.0 as isize, &pin.image) {
        Ok(()) => Toast {
            kind: ToastKind::Success,
            title: format!("Copied {w}×{h}"),
            body: String::new(),
        },
        Err(e) => Toast {
            kind: ToastKind::Error,
            title: "Copy failed".into(),
            body: e.to_string(),
        },
    };
    toast_host::show(t, Default::default(), None);
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Pin;
        if msg == WM_NCDESTROY {
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr)); // frees the image
            }
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        let Some(pin) = ptr.as_ref() else {
            return DefWindowProcW(hwnd, msg, wp, lp);
        };
        match msg {
            WM_ERASEBKGND => return LRESULT(1),
            WM_NCHITTEST => return LRESULT(2), // HTCAPTION: drag anywhere
            WM_PAINT => paint(hwnd, pin),
            WM_MOUSEWHEEL => wheel(hwnd, pin, wp),
            WM_NCLBUTTONDBLCLK => copy(hwnd, pin),
            // `pin` must not be used after DestroyWindow (WM_NCDESTROY frees it).
            WM_NCMBUTTONUP | WM_MBUTTONUP => {
                let _ = DestroyWindow(hwnd);
            }
            WM_KEYDOWN if wp.0 == 0x1B => {
                let _ = DestroyWindow(hwnd);
            }
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
        LRESULT(0)
    }
}
