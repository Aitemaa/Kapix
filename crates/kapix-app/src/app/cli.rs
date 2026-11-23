//! Hand-rolled command line: `kapix`, `kapix capture ...`, `--help`, `--version`.
//! CLI captures run in-process (no daemon needed). Exit codes: 0 ok, 1 cancelled, 2 error.

use std::io::Write;
use std::path::PathBuf;

use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HWND};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
    STD_OUTPUT_HANDLE, SetStdHandle,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, HWND_MESSAGE, WINDOW_EX_STYLE, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use kapix_core::{Image, ocr_text};
use kapix_ui::{Outcome, OverlayMode};

use super::overlay_host::{self, Sink};
use super::{actions, settings, winutil, with};

pub const HELP: &str = "\
Kapix - instant screenshot, annotation and OCR tool

USAGE:
    kapix                         start the tray daemon (a second launch starts a region capture)
    kapix capture [OPTIONS]       one-shot capture, no daemon needed
    kapix --help | --version

CAPTURE OPTIONS:
    --mode <region|ocr|window|fullscreen>   what to capture (default: region)
    --copy        copy the result to the clipboard (default when no --out/--stdout)
    --stdout      print OCR text (PNG bytes for image modes) to stdout
    --out <PATH>  write PNG, or JPEG for .jpg/.jpeg (text for ocr mode)
    --gdi         force the GDI capture backend

EXIT CODES: 0 ok, 1 cancelled / nothing captured, 2 error
";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Region,
    Ocr,
    Window,
    Fullscreen,
}

pub struct Opts {
    mode: Mode,
    copy: bool,
    stdout: bool,
    out: Option<PathBuf>,
    gdi: bool,
}

pub enum Cmd {
    Daemon,
    Help,
    Version,
    Capture(Opts),
}

pub fn parse(args: &[String]) -> Result<Cmd, String> {
    let Some(first) = args.first() else {
        return Ok(Cmd::Daemon);
    };
    match first.as_str() {
        "--help" | "-h" | "help" => Ok(Cmd::Help),
        "--version" | "-V" => Ok(Cmd::Version),
        "capture" => {
            let mut o = Opts {
                mode: Mode::Region,
                copy: false,
                stdout: false,
                out: None,
                gdi: false,
            };
            let mut it = args[1..].iter();
            while let Some(a) = it.next() {
                let (key, inline) = match a.split_once('=') {
                    Some((k, v)) => (k, Some(v.to_string())),
                    None => (a.as_str(), None),
                };
                let mut value = || {
                    inline
                        .clone()
                        .or_else(|| it.next().cloned())
                        .ok_or(format!("{key} needs a value"))
                };
                match key {
                    "--mode" => {
                        o.mode = match value()?.as_str() {
                            "region" => Mode::Region,
                            "ocr" => Mode::Ocr,
                            "window" => Mode::Window,
                            "fullscreen" => Mode::Fullscreen,
                            m => return Err(format!("unknown mode '{m}'")),
                        }
                    }
                    "--out" => o.out = Some(PathBuf::from(value()?)),
                    "--copy" => o.copy = true,
                    "--stdout" => o.stdout = true,
                    "--gdi" => o.gdi = true,
                    "--help" | "-h" => return Ok(Cmd::Help),
                    _ => return Err(format!("unknown option '{a}'")),
                }
            }
            Ok(Cmd::Capture(o))
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

/// Release builds are GUI-subsystem: attach to the parent console and make sure the standard
/// handles point somewhere (unless the caller redirected them).
pub fn attach_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            return; // no parent console (or a console build that already has one)
        }
        for (which, name) in [
            (STD_OUTPUT_HANDLE, w!("CONOUT$")),
            (STD_ERROR_HANDLE, w!("CONOUT$")),
        ] {
            ensure_handle(which, name);
        }
    }
}

unsafe fn ensure_handle(which: STD_HANDLE, name: PCWSTR) {
    unsafe {
        let unset = GetStdHandle(which).map_or(true, |h| h.is_invalid() || h.0.is_null());
        if unset
            && let Ok(h) = CreateFileW(
                name,
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        {
            let _ = SetStdHandle(which, h);
        }
    }
}

/// Print to stdout (after attaching the console); returns exit code 0.
pub fn print(s: &str) -> i32 {
    attach_console();
    let mut o = std::io::stdout();
    let _ = o.write_all(s.as_bytes());
    let _ = o.flush();
    0
}

pub fn eprint(s: &str) {
    let _ = std::io::stderr().write_all(s.as_bytes());
}

enum Res {
    Image(Image),
    Text(String),
    Cancelled,
}

pub fn run_capture(o: Opts) -> i32 {
    attach_console();
    super::daemon::register_classes();
    // Hidden message-only window: required as clipboard owner (SetClipboardData needs one).
    winutil::register_class(w!("KapixCli"), Some(winutil::def_proc));
    let owner: HWND = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("KapixCli"),
            w!("Kapix"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(winutil::hinstance()),
            None,
        )
    }
    .unwrap_or_default();
    with(|a| {
        a.hwnd = owner;
        a.cfg = settings::load_quiet();
    });
    let code = match capture(&o).and_then(|r| emit(&o, r)) {
        Ok(code) => code,
        Err(e) => {
            eprint(&format!("kapix: {e}\n"));
            2
        }
    };
    std::io::stdout().flush().ok();
    code
}

fn capture(o: &Opts) -> Result<Res, String> {
    match o.mode {
        Mode::Window | Mode::Fullscreen => {
            actions::capture_crop(o.mode == Mode::Fullscreen, o.gdi).map(Res::Image)
        }
        Mode::Region | Mode::Ocr => {
            let mode = if o.mode == Mode::Ocr {
                OverlayMode::Ocr
            } else {
                OverlayMode::Region
            };
            overlay_host::start(mode, o.gdi, Sink::Cli, &super::cfg())?;
            super::pump();
            match overlay_host::take_cli_result() {
                Some(Outcome::Copy(i) | Outcome::Save(i) | Outcome::Pin { image: i, .. }) => {
                    Ok(Res::Image(i))
                }
                Some(Outcome::Ocr(i)) => {
                    let lines = kapix_win::ocr::recognize(&i, &super::cfg().ocr.language)
                        .map_err(|e| e.to_string())?;
                    Ok(Res::Text(ocr_text::join_lines(&lines, true)))
                }
                Some(Outcome::CopyText(t)) => Ok(Res::Text(t)),
                _ => Ok(Res::Cancelled),
            }
        }
    }
}

/// Deliver the result per `--out` / `--stdout` / `--copy`; returns the exit code.
fn emit(o: &Opts, r: Res) -> Result<i32, String> {
    let copy = o.copy || (o.out.is_none() && !o.stdout);
    let owner = super::owner();
    match r {
        Res::Cancelled => Ok(1),
        Res::Image(img) => {
            if let Some(p) = &o.out {
                actions::write_image(p, &img)?;
            }
            if o.stdout {
                let png = img.encode_png().map_err(|e| e.to_string())?;
                std::io::stdout()
                    .write_all(&png)
                    .map_err(|e| e.to_string())?;
            }
            if copy {
                kapix_win::clipboard::set_image(owner, &img).map_err(|e| e.to_string())?;
            }
            Ok(0)
        }
        Res::Text(t) => {
            if o.mode == Mode::Ocr && t.trim().is_empty() {
                eprint("kapix: no text detected\n");
                return Ok(1);
            }
            if let Some(p) = &o.out {
                std::fs::write(p, &t).map_err(|e| e.to_string())?;
            }
            if o.stdout {
                std::io::stdout()
                    .write_all(t.as_bytes())
                    .map_err(|e| e.to_string())?;
                std::io::stdout()
                    .write_all(b"\n")
                    .map_err(|e| e.to_string())?;
            }
            if copy {
                kapix_win::clipboard::set_text(owner, &t).map_err(|e| e.to_string())?;
            }
            Ok(0)
        }
    }
}
