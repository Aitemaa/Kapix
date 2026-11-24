//! Windows application: tray, hotkeys, overlay/toast/pin window hosts, CLI dispatch.
//!
//! Threading / re-entrancy model: everything UI runs on the main thread. Shared state lives in
//! thread-locals behind `RefCell`s and a borrow is **never** held across a call that can pump
//! messages (`ShowWindow`, `DestroyWindow`, `SetForegroundWindow`, `TrackPopupMenu`, dialogs,
//! clipboard, toast creation...): handlers copy what they need out, release, then call Win32.
//! Worker threads (PNG encode, OCR, image decode) only talk back via `PostMessageW` with a boxed
//! payload.

mod actions;
mod cli;
mod daemon;
mod icon;
mod overlay_host;
mod pin;
mod settings;
mod toast_host;
mod tray;
mod winutil;

use std::cell::RefCell;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use kapix_core::Config;
use kapix_core::history::Recent;

/// Daemon window messages.
pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_START_REGION: u32 = WM_APP + 2;
pub const WM_WORKER: u32 = WM_APP + 3;

/// Process-wide application state (main thread only).
pub struct App {
    pub cfg: Config,
    /// Window used as clipboard owner / hotkey sink (daemon window, or hidden helper in CLI mode).
    pub hwnd: HWND,
    pub recent: Recent,
    pub cfg_mtime: Option<std::time::SystemTime>,
    pub icon: windows::Win32::UI::WindowsAndMessaging::HICON,
}

thread_local! {
    pub static APP: RefCell<App> = RefCell::new(App {
        cfg: Config::default(),
        hwnd: HWND::default(),
        recent: Recent::new(10),
        cfg_mtime: None,
        icon: Default::default(),
    });
}

/// Run `f` with the app state. Keep `f` free of Win32 calls that may pump messages.
pub fn with<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|a| f(&mut a.borrow_mut()))
}

pub fn cfg() -> Config {
    with(|a| a.cfg.clone())
}

/// Clipboard owner handle for `kapix_win::clipboard`.
pub fn owner() -> isize {
    with(|a| a.hwnd.0 as isize)
}

pub fn run() -> i32 {
    winutil::init_trace();
    unsafe {
        // The embedded manifest already requests PerMonitorV2; this covers odd launch paths.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&args) {
        Ok(cli::Cmd::Daemon) => daemon::run(),
        Ok(cli::Cmd::Quit) => daemon::quit(),
        Ok(cli::Cmd::Help) => cli::print(cli::HELP),
        Ok(cli::Cmd::Version) => cli::print(concat!("kapix ", env!("CARGO_PKG_VERSION"), "\n")),
        Ok(cli::Cmd::Capture(o)) => cli::run_capture(o),
        Err(e) => {
            cli::attach_console();
            cli::eprint(&format!("kapix: {e}\nTry `kapix --help`.\n"));
            2
        }
    }
}

/// Plain blocking message loop (returns on `WM_QUIT`).
pub fn pump() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, TranslateMessage,
    };
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
