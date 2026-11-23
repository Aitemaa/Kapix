//! config.toml loading, change detection (mtime compared lazily, no watcher/timer) and hotkeys.

use std::path::PathBuf;
use std::time::SystemTime;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};

use kapix_core::{Config, Hotkey};
use kapix_ui::toast::{Toast, ToastKind};

use super::{toast_host, tray, with};

pub const HK_REGION: i32 = 1;
pub const HK_OCR: i32 = 2;
pub const HK_FULLSCREEN: i32 = 3;
pub const HK_WINDOW: i32 = 4;

fn path() -> Option<PathBuf> {
    kapix_win::paths::config_path().ok()
}

fn mtime() -> Option<SystemTime> {
    std::fs::metadata(path()?).ok()?.modified().ok()
}

/// Read + parse the config. A missing file yields defaults (and is created when `write_default`).
fn read(write_default: bool) -> Result<Config, String> {
    let p = path().ok_or("cannot locate %APPDATA%")?;
    if !p.exists() {
        if write_default {
            let _ = std::fs::write(&p, Config::default_toml());
        }
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    Config::parse(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

/// CLI: best-effort config (never writes, never fails).
pub fn load_quiet() -> Config {
    read(false).unwrap_or_default()
}

/// Daemon startup: load (creating the default file), bind hotkeys, report problems.
pub fn init(hwnd: HWND) {
    let mut errs = Vec::new();
    match read(true) {
        Ok(c) => with(|a| a.cfg = c),
        Err(e) => errs.push(("Config error — using defaults", e)),
    }
    with(|a| a.cfg_mtime = mtime());
    apply(hwnd, &mut errs);
    report(errs, false);
}

/// Reload when the file changed since the last load (called before every trigger).
pub fn check_reload() {
    if mtime() != with(|a| a.cfg_mtime) {
        reload(false);
    }
}

pub fn reload(explicit: bool) {
    let mut errs = Vec::new();
    match read(false) {
        Ok(c) => with(|a| a.cfg = c),
        Err(e) => errs.push(("Config error — keeping previous", e)),
    }
    with(|a| a.cfg_mtime = mtime());
    let hwnd = with(|a| a.hwnd);
    apply(hwnd, &mut errs);
    tray::update_tip();
    report(errs, explicit);
}

fn report(errs: Vec<(&'static str, String)>, announce: bool) {
    if let Some((title, body)) = errs.into_iter().next() {
        let body: String = body.lines().next().unwrap_or("").chars().take(90).collect();
        toast_host::show(
            Toast {
                kind: ToastKind::Error,
                title: title.into(),
                body,
            },
            Default::default(),
            None,
        );
    } else if announce {
        toast_host::show(
            Toast {
                kind: ToastKind::Info,
                title: "Config reloaded".into(),
                body: String::new(),
            },
            Default::default(),
            None,
        );
    }
}

/// (Re)bind all hotkeys for the current config; push problems into `errs`.
fn apply(hwnd: HWND, errs: &mut Vec<(&'static str, String)>) {
    let cfg = super::cfg();
    for p in cfg.validate() {
        errs.push(("Invalid hotkey in config", p));
    }
    let list = [
        (HK_REGION, "region", &cfg.hotkeys.region),
        (HK_OCR, "ocr", &cfg.hotkeys.ocr),
        (HK_FULLSCREEN, "fullscreen", &cfg.hotkeys.fullscreen),
        (HK_WINDOW, "active window", &cfg.hotkeys.active_window),
    ];
    let mut taken = Vec::new();
    for (id, name, s) in list {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
        let Ok(h) = Hotkey::parse(s) else { continue };
        let mut m = MOD_NOREPEAT;
        for (on, flag) in [
            (h.ctrl, MOD_CONTROL),
            (h.shift, MOD_SHIFT),
            (h.alt, MOD_ALT),
            (h.win, MOD_WIN),
        ] {
            if on {
                m |= flag;
            }
        }
        if unsafe { RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(m.0), h.vk as u32) }.is_err() {
            taken.push(format!("{} ({name})", h.display()));
        }
    }
    if !taken.is_empty() {
        errs.push(("Hotkey unavailable (already in use)", taken.join(", ")));
    }
}

pub fn unregister_all(hwnd: HWND) {
    for id in [HK_REGION, HK_OCR, HK_FULLSCREEN, HK_WINDOW] {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
    }
}
