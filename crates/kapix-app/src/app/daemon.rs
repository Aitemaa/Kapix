//! The tray daemon: hidden top-level window + plain `GetMessageW` loop (no timers, no polling).

use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW,
    PostMessageW, PostQuitMessage, RegisterWindowMessageW, WINDOW_EX_STYLE, WM_CONTEXTMENU,
    WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WS_OVERLAPPED,
};
use windows::core::{PCWSTR, w};

use kapix_ui::toast::ToastKind;

use super::actions::{self, Action};
use super::{
    WM_START_REGION, WM_TRAY, WM_WORKER, overlay_host, pin, settings, toast_host, tray, winutil,
    with,
};

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn register_classes() {
    winutil::register_class(w!("KapixDaemon"), Some(proc));
    overlay_host::register();
    toast_host::register();
    pin::register();
}

pub fn run() -> i32 {
    unsafe {
        let _mutex = CreateMutexW(None, false, w!("Local\\Kapix.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Already running: ask that instance to start a region capture.
            if let Ok(h) = FindWindowW(w!("KapixDaemon"), PCWSTR::null()) {
                // Hand our foreground right to the running instance so its overlay gets focus.
                let _ = AllowSetForegroundWindow(u32::MAX); // ASFW_ANY
                let _ = PostMessageW(Some(h), WM_START_REGION, WPARAM(0), LPARAM(0));
            }
            return 0;
        }
        register_classes();
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("KapixDaemon"),
            w!("Kapix"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(winutil::hinstance()),
            None,
        ) else {
            return 2;
        };
        with(|a| a.hwnd = hwnd);
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );
        settings::init(hwnd);
        tray::add(hwnd);
    }
    winutil::trim_working_set();
    super::pump();
    0
}

fn on_menu(hwnd: HWND) {
    settings::check_reload();
    let cmd = tray::show_menu(hwnd);
    // Let the menu's fade-out leave the screen before a capture grabs the desktop.
    let capture_delay = || std::thread::sleep(std::time::Duration::from_millis(150));
    match cmd {
        0 => {}
        tray::IDM_REGION | tray::IDM_WINDOW => {
            capture_delay();
            actions::trigger(Action::Region);
        }
        tray::IDM_OCR => {
            capture_delay();
            actions::trigger(Action::Ocr);
        }
        tray::IDM_FULLSCREEN => {
            capture_delay();
            actions::trigger(Action::Fullscreen);
        }
        tray::IDM_EDIT_CONFIG => {
            if let Ok(p) = kapix_win::paths::config_path() {
                if !p.exists() {
                    let _ = std::fs::write(&p, kapix_core::Config::default_toml());
                }
                kapix_win::paths::shell_open(&p.to_string_lossy());
            }
        }
        tray::IDM_RELOAD => settings::reload(true),
        tray::IDM_STARTUP => {
            let on = !kapix_win::startup::is_enabled();
            if let Err(e) = kapix_win::startup::set_enabled(on) {
                actions::notify(
                    ToastKind::Error,
                    "Startup setting failed",
                    e.to_string(),
                    None,
                );
            }
        }
        tray::IDM_EXIT => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        c if c >= tray::IDM_RECENT => actions::recopy_recent((c - tray::IDM_RECENT) as usize),
        _ => {}
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => match wp.0 as i32 {
            settings::HK_REGION => actions::trigger(Action::Region),
            settings::HK_OCR => actions::trigger(Action::Ocr),
            settings::HK_FULLSCREEN => actions::trigger(Action::Fullscreen),
            settings::HK_WINDOW => actions::trigger(Action::ActiveWindow),
            _ => {}
        },
        WM_START_REGION => actions::trigger(Action::Region),
        WM_TRAY => match (lp.0 as u32) & 0xFFFF {
            WM_LBUTTONUP => actions::trigger(Action::Region),
            WM_RBUTTONUP | WM_CONTEXTMENU => on_menu(hwnd),
            _ => {}
        },
        WM_WORKER => {
            // SAFETY: posted by `post_boxed::<Worker>`; ownership returns here exactly once.
            let msg = unsafe { Box::from_raw(lp.0 as *mut actions::Worker) };
            actions::handle_worker(*msg);
        }
        WM_DESTROY => {
            settings::unregister_all(hwnd);
            tray::remove(hwnd);
            unsafe { PostQuitMessage(0) };
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => tray::add(hwnd), // Explorer restarted
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}
