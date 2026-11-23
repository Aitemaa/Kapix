//! What happens after a trigger or an overlay outcome: clipboard, files, OCR, pins, toasts.

use std::path::{Path, PathBuf};

use kapix_core::config::ImageFormat;
use kapix_core::history::RecentCapture;
use kapix_core::template::{DateTime, render_filename, unique_name};
use kapix_core::{Config, Image, ocr_text};
use kapix_ui::toast::{Toast, ToastKind};
use kapix_ui::{Font, Outcome, OverlayMode};
use kapix_win::ocr::OcrError;

use super::overlay_host::{self, Sink};
use super::toast_host::{self, ToastAction};
use super::winutil::{self, post_boxed};
use super::{WM_WORKER, cfg, owner, pin, settings, with};

#[derive(Clone, Copy)]
pub enum Action {
    Region,
    Ocr,
    Fullscreen,
    ActiveWindow,
}

/// Messages posted from worker threads to the daemon window.
pub enum Worker {
    Recent(RecentCapture),
    Ocr(Result<Vec<String>, OcrError>),
    Error(String),
    /// A decoded history entry, ready to be put on the clipboard.
    Decoded(Image),
}

pub fn notify(
    kind: ToastKind,
    title: impl Into<String>,
    body: impl Into<String>,
    font: Option<Font>,
) {
    toast_host::show(
        Toast {
            kind,
            title: title.into(),
            body: body.into(),
        },
        ToastAction::None,
        font,
    );
}

/// Hotkey / tray / second-launch entry point.
pub fn trigger(a: Action) {
    winutil::trace_start("hotkey");
    settings::check_reload();
    let cfg = cfg();
    let res = match a {
        Action::Region => overlay_host::start(OverlayMode::Region, false, Sink::Daemon, &cfg),
        Action::Ocr => overlay_host::start(OverlayMode::Ocr, false, Sink::Daemon, &cfg),
        Action::Fullscreen | Action::ActiveWindow => {
            capture_crop(matches!(a, Action::Fullscreen), false).map(|img| {
                winutil::trace("capture+crop done");
                copy_image(img, None);
                winutil::trim_working_set();
            })
        }
    };
    if let Err(e) = res {
        notify(ToastKind::Error, "Capture failed", e, None);
    }
}

/// Capture the desktop and crop the monitor under the cursor / the foreground window.
pub fn capture_crop(fullscreen: bool, gdi: bool) -> Result<Image, String> {
    let cap = if gdi {
        kapix_win::capture::capture_desktop_gdi()
    } else {
        kapix_win::capture::capture_desktop()
    }
    .map_err(|e| e.to_string())?;
    let want = if fullscreen {
        kapix_win::capture::monitor_under_cursor(&cap.desktop).map(|m| m.rect)
    } else {
        kapix_win::capture::foreground_window_rect()
    };
    let r = want.map_or(cap.frame.bounds(), |r| cap.desktop.rect_to_buffer(r));
    let img = cap.frame.crop(r);
    if img.width == 0 || img.height == 0 {
        return Err("nothing to capture".into());
    }
    Ok(img)
}

/// Act on a terminal overlay outcome (overlay and its buffers are already gone).
pub fn handle(out: Outcome, font: Option<Font>) {
    match out {
        Outcome::Copy(img) => copy_image(img, font),
        Outcome::Save(img) => save_image(img, font),
        Outcome::Ocr(img) => start_ocr(img),
        Outcome::CopyText(text) => match kapix_win::clipboard::set_text(owner(), &text) {
            Ok(()) => notify(ToastKind::Success, format!("Copied {text}"), "", font),
            Err(e) => notify(ToastKind::Error, "Copy failed", e.to_string(), font),
        },
        Outcome::Pin { image, at } => pin::create(image, at),
        Outcome::Cancel | Outcome::None | Outcome::Redraw => {}
    }
}

fn auto_path(cfg: &Config, now: &DateTime) -> PathBuf {
    let dir = PathBuf::from(kapix_win::paths::expand(&cfg.save.directory));
    let name = unique_name(
        &render_filename(&cfg.save.filename_template, now),
        cfg.save.format.extension(),
        |n| dir.join(n).exists(),
    );
    dir.join(name)
}

/// Clipboard first (latency budget), then toast; PNG encode for history / auto-save runs on a
/// worker thread which owns the image from there on.
pub fn copy_image(img: Image, font: Option<Font>) {
    let (w, h) = (img.width, img.height);
    if let Err(e) = kapix_win::clipboard::set_image(owner(), &img) {
        notify(ToastKind::Error, "Copy failed", e.to_string(), font);
        return;
    }
    winutil::trace("clipboard ready");
    let cfg = cfg();
    let now = kapix_win::time::local_now();
    let path = cfg.save.auto_save.then(|| auto_path(&cfg, &now));
    let body = path
        .as_ref()
        .map_or(String::new(), |p| p.display().to_string());
    notify(ToastKind::Success, format!("Copied {w}×{h}"), body, font);
    // WIC (JPEG) wants the UI thread's COM state; PNG needs no OS services.
    let jpeg = path.is_some() && cfg.save.format == ImageFormat::Jpeg;
    let jpeg_bytes = if jpeg {
        kapix_win::encode::jpeg(&img, 0.92)
    } else {
        Err(kapix_win::Error(String::new()))
    };
    let hwnd = owner();
    std::thread::spawn(move || {
        let png = match img.encode_png() {
            Ok(p) => p,
            Err(e) => return post_boxed(hwnd, WM_WORKER, Worker::Error(e.to_string())),
        };
        if let Some(p) = path {
            let data = if jpeg {
                jpeg_bytes
                    .as_ref()
                    .map(|v| v.as_slice())
                    .map_err(|e| e.to_string())
            } else {
                Ok(&png[..])
            };
            let res = data.and_then(|d| {
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                std::fs::write(&p, d).map_err(|e| e.to_string())
            });
            if let Err(e) = res {
                post_boxed(
                    hwnd,
                    WM_WORKER,
                    Worker::Error(format!("Auto-save failed: {e}")),
                );
            }
        }
        let rc = RecentCapture {
            png,
            width: img.width,
            height: img.height,
            taken: now,
        };
        drop(img);
        post_boxed(hwnd, WM_WORKER, Worker::Recent(rc));
    });
}

/// Write `img` as PNG or JPEG depending on the extension of `path`.
pub fn write_image(path: &Path, img: &Image) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = if ext == "jpg" || ext == "jpeg" {
        kapix_win::encode::jpeg(img, 0.92).map_err(|e| e.to_string())?
    } else {
        img.encode_png().map_err(|e| e.to_string())?
    };
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn save_image(img: Image, font: Option<Font>) {
    winutil::trim_working_set(); // the dialog may stay open for a long time
    let cfg = cfg();
    let dir = PathBuf::from(kapix_win::paths::expand(&cfg.save.directory));
    let name = render_filename(&cfg.save.filename_template, &kapix_win::time::local_now());
    let jpeg = cfg.save.format == ImageFormat::Jpeg;
    let Some(path) = kapix_win::dialog::save_image_dialog(owner(), &dir, &name, jpeg) else {
        return;
    };
    match write_image(&path, &img) {
        Ok(()) => {
            let file = path
                .file_name()
                .map_or(String::new(), |f| f.to_string_lossy().into_owned());
            notify(ToastKind::Success, "Saved", file, font);
        }
        Err(e) => notify(ToastKind::Error, "Save failed", e, font),
    }
}

fn start_ocr(img: Image) {
    let (lang, hwnd) = (cfg().ocr.language, owner());
    std::thread::spawn(move || {
        let r = kapix_win::ocr::recognize(&img, &lang);
        drop(img);
        post_boxed(hwnd, WM_WORKER, Worker::Ocr(r));
    });
}

/// Re-copy history entry `i` (0 = newest): decode off-thread, copy when it comes back.
pub fn recopy_recent(i: usize) {
    let Some(png) = with(|a| a.recent.get(i).map(|r| r.png.clone())) else {
        return;
    };
    let hwnd = owner();
    std::thread::spawn(move || {
        let msg = decode_png(&png).map_or_else(
            || Worker::Error("Could not decode capture".into()),
            Worker::Decoded,
        );
        post_boxed(hwnd, WM_WORKER, msg);
    });
}

/// PNG -> BGRA via WIC (the OS decoder; no extra crate code in the binary).
fn decode_png(png: &[u8]) -> Option<Image> {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
        WICConvertBitmapSource, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize};
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().ok()?;
        let r = (|| -> windows::core::Result<Image> {
            let f: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let s = f.CreateStream()?;
            s.InitializeFromMemory(png)?;
            let dec =
                f.CreateDecoderFromStream(&s, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
            let frame = dec.GetFrame(0)?;
            let conv = WICConvertBitmapSource(&GUID_WICPixelFormat32bppBGRA, &frame)?;
            let (mut w, mut h) = (0u32, 0u32);
            conv.GetSize(&mut w, &mut h)?;
            let mut data = vec![0u8; w as usize * h as usize * 4];
            conv.CopyPixels(std::ptr::null(), w * 4, &mut data)?;
            Ok(Image::from_bgra(w, h, data))
        })();
        CoUninitialize();
        r.ok()
    }
}

/// Worker-thread result arrived on the UI thread.
pub fn handle_worker(msg: Worker) {
    match msg {
        Worker::Recent(rc) => {
            with(|a| a.recent.push(rc));
            winutil::trim_working_set();
        }
        Worker::Error(e) => notify(ToastKind::Error, "Kapix", e, None),
        Worker::Decoded(img) => match kapix_win::clipboard::set_image(owner(), &img) {
            Ok(()) => notify(
                ToastKind::Success,
                format!("Copied {}×{}", img.width, img.height),
                "",
                None,
            ),
            Err(e) => notify(ToastKind::Error, "Copy failed", e.to_string(), None),
        },
        Worker::Ocr(Err(OcrError::NoLanguagePack)) => toast_host::show(
            Toast {
                kind: ToastKind::Error,
                title: "Install an OCR language".into(),
                body: "Click to open Windows language settings".into(),
            },
            ToastAction::OpenLanguageSettings,
            None,
        ),
        Worker::Ocr(Err(e)) => notify(ToastKind::Error, "OCR failed", e.to_string(), None),
        Worker::Ocr(Ok(lines)) => {
            let text = ocr_text::join_lines(&lines, true);
            if text.trim().is_empty() {
                return notify(ToastKind::Info, "No text detected", "", None);
            }
            if let Err(e) = kapix_win::clipboard::set_text(owner(), &text) {
                return notify(ToastKind::Error, "Copy failed", e.to_string(), None);
            }
            winutil::trace("ocr text on clipboard");
            if cfg().ocr.copy_notification {
                let (chars, n) = ocr_text::stats(&text);
                let body = format!("{chars} chars · {n} line{}", if n == 1 { "" } else { "s" });
                notify(ToastKind::Success, ocr_text::preview(&text, 60), body, None);
            }
        }
    }
    winutil::trim_working_set();
}
