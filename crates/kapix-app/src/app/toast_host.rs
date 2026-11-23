//! Toast window: layered, topmost, non-activating. The timer exists only while a toast is shown.

use std::cell::Cell;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, HGDIOBJ, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, KillTimer, SW_SHOWNOACTIVATE, SetTimer,
    ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_RBUTTONUP,
    WM_TIMER, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use kapix_ui::Font;
use kapix_ui::toast::{self, Toast, ToastKind};

use super::winutil;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ToastAction {
    #[default]
    None,
    /// Open Windows language settings (missing OCR pack).
    OpenLanguageSettings,
}

thread_local! {
    static CUR: Cell<Option<HWND>> = const { Cell::new(None) };
    static ACTION: Cell<ToastAction> = const { Cell::new(ToastAction::None) };
}

pub fn register() {
    winutil::register_class(w!("KapixToast"), Some(proc));
}

/// Show (or replace) the toast. `font` is reused from the caller's session when available and
/// always dropped before returning, so no font stays resident while idle.
pub fn show(t: Toast, action: ToastAction, font: Option<Font>) {
    let font = font.or_else(winutil::load_font);
    let cur = winutil::cursor_pos();
    let (work, scale) = kapix_win::capture::desktop_layout()
        .ok()
        .and_then(|d| d.monitor_at(cur).map(|m| (m.work, m.scale)))
        .unwrap_or((kapix_core::Rect::new(0, 0, 1920, 1080), 1.0));
    let img = toast::render(&t, font.as_ref(), scale);
    drop(font);
    if img.width == 0 || img.height == 0 {
        return;
    }
    let margin = (16.0 * scale) as i32;
    let x = work.right() - img.width as i32 - margin;
    let y = work.bottom() - img.height as i32 - margin;

    let hwnd = match CUR.with(|c| c.get()) {
        Some(h) => h,
        None => {
            let h = unsafe {
                CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    w!("KapixToast"),
                    PCWSTR::null(),
                    WS_POPUP,
                    x,
                    y,
                    1,
                    1,
                    None,
                    None,
                    Some(winutil::hinstance()),
                    None,
                )
            };
            let Ok(h) = h else { return };
            CUR.with(|c| c.set(Some(h)));
            h
        }
    };
    ACTION.with(|a| a.set(action));
    if present(hwnd, &img, x, y) {
        let ms = if t.kind == ToastKind::Error {
            4000
        } else {
            2500
        };
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetTimer(Some(hwnd), 1, ms, None); // same id: restarts the countdown
        }
    } else {
        dismiss(hwnd);
    }
}

/// Push premultiplied BGRA to the layered window at `(x, y)`.
fn present(hwnd: HWND, img: &kapix_core::Image, x: i32, y: i32) -> bool {
    unsafe {
        let mdc = CreateCompatibleDC(None);
        let info = winutil::bmi(img.width as i32, img.height as i32);
        let mut bits = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(Some(mdc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mdc);
            return false;
        };
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mdc);
            return false;
        }
        std::ptr::copy_nonoverlapping(img.data.as_ptr(), bits.cast::<u8>(), img.data.len());
        let old = SelectObject(mdc, HGDIOBJ(bmp.0));
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let ok = UpdateLayeredWindow(
            hwnd,
            None,
            Some(&POINT { x, y }),
            Some(&SIZE {
                cx: img.width as i32,
                cy: img.height as i32,
            }),
            Some(mdc),
            Some(&POINT { x: 0, y: 0 }),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )
        .is_ok();
        SelectObject(mdc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mdc);
        ok
    }
}

fn dismiss(hwnd: HWND) {
    if CUR.with(|c| c.get()) == Some(hwnd) {
        CUR.with(|c| c.set(None));
    }
    unsafe {
        let _ = KillTimer(Some(hwnd), 1);
        let _ = DestroyWindow(hwnd);
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let action = ACTION.with(|a| a.get());
            dismiss(hwnd);
            if action == ToastAction::OpenLanguageSettings {
                kapix_win::paths::shell_open("ms-settings:regionlanguage");
            }
            LRESULT(0)
        }
        WM_TIMER => {
            dismiss(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
