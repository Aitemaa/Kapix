//! Windows OS services for Kapix. Compiles to an empty crate on non-Windows targets.
//!
//! All functions are synchronous. Threading contract:
//! - capture / snap / clipboard / dialog / registry / font: call from the UI thread.
//! - `ocr::recognize`: call from a **worker thread**; it initializes COM/WinRT (MTA) itself.
//!
//! Coordinates are physical pixels; the process is Per-Monitor-V2 DPI aware (set by the app).
#![cfg(windows)]

pub mod capture;
pub mod clipboard;
pub mod dialog;
pub mod encode;
pub mod ocr;
pub mod paths;
pub mod snap;
pub mod startup;
pub mod time;

/// Error with a human-readable message (HRESULT text included when from Win32/WinRT).
#[derive(Debug, Clone)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<windows::core::Error> for Error {
    fn from(e: windows::core::Error) -> Self {
        Error(format!("{} (0x{:08X})", e.message(), e.code().0 as u32))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// NUL-terminated UTF-16 copy of `s`.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `HWND` from the app's `isize` window handle (`0` = no window).
pub(crate) fn hwnd(h: isize) -> windows::Win32::Foundation::HWND {
    windows::Win32::Foundation::HWND(h as *mut core::ffi::c_void)
}

/// Initialize COM on this thread; an already-initialized apartment (any mode) is fine.
pub(crate) fn com_init(mta: bool) {
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoInitializeEx,
    };
    let mode = if mta {
        COINIT_MULTITHREADED
    } else {
        COINIT_APARTMENTTHREADED
    };
    // SAFETY: plain COM initialization; S_FALSE / RPC_E_CHANGED_MODE are benign and ignored.
    unsafe {
        let _ = CoInitializeEx(None, mode);
    }
}
