//! Linux (Wayland) application entrypoint: interactive region/OCR overlay, fullscreen & window
//! capture, floating pin window, CLI, and XDG config loading.

mod keymap;
mod overlay_host;
mod pin_host;

use std::io::Write;
use std::path::{Path, PathBuf};

use kapix_core::config::ImageFormat;
use kapix_core::template::{render_filename, unique_name};
use kapix_core::{Config, Image, ocr_text};
use kapix_linux::dialog::DialogOutcome;
use kapix_linux::notify::Urgency;
use kapix_ui::{Font, Outcome, OverlayMode, Palette};

pub const HELP: &str = "\
Kapix - instant screenshot, annotation and OCR tool (Linux / Wayland)

USAGE:
    kapix                         start an interactive region capture
    kapix capture [OPTIONS]       one-shot capture (composable for compositor binds & scripts)
    kapix --help | --version

CAPTURE OPTIONS:
    --mode <region|ocr|window|fullscreen>   what to capture (default: region)
    --copy        copy the result to the clipboard (default when no --out/--stdout)
    --stdout      print OCR text (PNG bytes for image modes) to stdout
    --out <PATH>  write PNG, or JPEG for .jpg/.jpeg (text for ocr mode)

EXIT CODES: 0 ok, 1 cancelled / nothing captured, 2 error
";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Region,
    Ocr,
    Window,
    Fullscreen,
}

struct Opts {
    mode: Mode,
    copy: bool,
    stdout: bool,
    out: Option<PathBuf>,
    interactive: bool,
}

enum Cmd {
    Help,
    Version,
    Capture(Opts),
}

fn parse(args: &[String]) -> Result<Cmd, String> {
    let Some(first) = args.first() else {
        // Bare `kapix` on Wayland runs an interactive region capture (compositor owns hotkeys).
        return Ok(Cmd::Capture(Opts {
            mode: Mode::Region,
            copy: true,
            stdout: false,
            out: None,
            interactive: true,
        }));
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
                interactive: false,
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
                        .ok_or_else(|| format!("{key} needs a value"))
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
                    "--gdi" => {} // Windows-only flag; ignored cleanly for script compatibility.
                    "--help" | "-h" => return Ok(Cmd::Help),
                    _ => return Err(format!("unknown option '{a}'")),
                }
            }
            Ok(Cmd::Capture(o))
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Cmd::Help) => {
            print!("{HELP}");
            0
        }
        Ok(Cmd::Version) => {
            println!("kapix {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok(Cmd::Capture(o)) => run_capture(o),
        Err(e) => {
            eprintln!("kapix: {e}\nTry `kapix --help`.");
            2
        }
    }
}

fn load_config(write_default: bool) -> Config {
    let Ok(p) = kapix_linux::paths::config_path() else {
        return Config::default();
    };
    if !p.exists() {
        if write_default {
            let _ = std::fs::write(&p, kapix_linux::paths::default_config_toml());
        }
        return Config::default();
    }
    match std::fs::read_to_string(&p) {
        Ok(text) => Config::parse(text.trim_start_matches('\u{feff}')).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn load_font() -> Font {
    Font::new(kapix_linux::paths::system_font())
}

fn resolve_palette(cfg: &Config) -> Palette {
    Palette::resolve(cfg.ui.theme, kapix_linux::theme::is_system_light)
}

fn write_image(path: &Path, img: &Image) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = if ext == "jpg" || ext == "jpeg" {
        kapix_linux::encode::jpeg(img, 0.92).map_err(|e| e.to_string())?
    } else {
        img.encode_png().map_err(|e| e.to_string())?
    };
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn auto_save_path(cfg: &Config) -> PathBuf {
    let dir = PathBuf::from(kapix_linux::paths::expand(&cfg.save.directory));
    let now = kapix_linux::time::local_now();
    let name = unique_name(
        &render_filename(&cfg.save.filename_template, &now),
        cfg.save.format.extension(),
        |n| dir.join(n).exists(),
    );
    dir.join(name)
}

fn run_capture(o: Opts) -> i32 {
    let cfg = load_config(o.interactive);
    let palette = resolve_palette(&cfg);

    // Notify on capture error in interactive mode (when launched from a compositor hotkey with
    // no terminal attached).
    let notify_err = |msg: &str| {
        eprintln!("kapix: {msg}");
        if o.interactive {
            kapix_linux::notify::send(Urgency::Critical, "Capture failed", msg);
        }
    };

    let res = execute(&o, &cfg, palette);
    let _ = std::io::stdout().flush();
    match res {
        Ok(code) => code,
        Err(e) => {
            notify_err(&e);
            2
        }
    }
}

fn execute(o: &Opts, cfg: &Config, palette: Palette) -> Result<i32, String> {
    let explicit_sink = o.out.is_some() || o.stdout;
    let should_copy = o.copy || !explicit_sink;
    let should_notify = !o.stdout && o.out.is_none();

    match o.mode {
        Mode::Fullscreen | Mode::Window => {
            let cap = kapix_linux::capture::capture_desktop().map_err(|e| e.to_string())?;
            let crop_rect = if o.mode == Mode::Window {
                kapix_linux::snap::focused_window_rect(&cap.outputs)
                    .or_else(|| kapix_linux::snap::focused_output_rect(&cap.outputs))
            } else {
                kapix_linux::snap::focused_output_rect(&cap.outputs)
            };
            let img = match crop_rect {
                Some(r) => cap.frame.crop(r),
                None => cap.frame,
            };
            if img.width == 0 || img.height == 0 {
                return Err("nothing to capture".into());
            }
            emit_image(o, cfg, &img, should_copy, should_notify)?;
            Ok(0)
        }
        Mode::Region | Mode::Ocr => {
            // Overlap system fallback font read with screen capture.
            let font_job = std::thread::spawn(load_font);
            let cap = kapix_linux::capture::capture_desktop().map_err(|e| e.to_string())?;
            let font = font_job.join().unwrap_or_else(|_| Font::builtin());
            let overlay_mode = if o.mode == Mode::Ocr {
                OverlayMode::Ocr
            } else {
                OverlayMode::Region
            };
            let out = overlay_host::run_overlay(cap, overlay_mode, cfg, font, palette)?;
            match out {
                Outcome::Cancel | Outcome::None | Outcome::Redraw => Ok(1),
                Outcome::Copy(img) => {
                    emit_image(o, cfg, &img, should_copy, should_notify)?;
                    Ok(0)
                }
                Outcome::Save(img) => {
                    if explicit_sink {
                        emit_image(o, cfg, &img, o.copy, false)?;
                        return Ok(0);
                    }
                    save_interactive(cfg, &img)?;
                    Ok(0)
                }
                Outcome::Pin { image, .. } => {
                    if explicit_sink {
                        emit_image(o, cfg, &image, o.copy, false)?;
                        return Ok(0);
                    }
                    pin_host::run_pin(image, palette)?;
                    Ok(0)
                }
                Outcome::Ocr(img) => {
                    let lines = kapix_linux::ocr::recognize(&img, &cfg.ocr.language)
                        .map_err(|e| e.to_string())?;
                    let text = ocr_text::join_lines(&lines, true);
                    if text.trim().is_empty() {
                        eprintln!("kapix: no text detected");
                        if should_notify {
                            kapix_linux::notify::send(Urgency::Low, "No text detected", "");
                        }
                        return Ok(1);
                    }
                    emit_text(o, cfg, &text, should_copy, should_notify)?;
                    Ok(0)
                }
                Outcome::CopyText(text) => {
                    emit_text(o, cfg, &text, should_copy, should_notify)?;
                    Ok(0)
                }
            }
        }
    }
}

fn emit_image(o: &Opts, cfg: &Config, img: &Image, copy: bool, notify: bool) -> Result<(), String> {
    if let Some(p) = &o.out {
        write_image(p, img)?;
    }
    if o.stdout {
        let png = img.encode_png().map_err(|e| e.to_string())?;
        std::io::stdout()
            .write_all(&png)
            .map_err(|e| e.to_string())?;
    }
    if copy {
        kapix_linux::clipboard::set_image(img).map_err(|e| e.to_string())?;
    }
    let auto_saved = if copy && cfg.save.auto_save && o.out.is_none() {
        let path = auto_save_path(cfg);
        write_image(&path, img)?;
        Some(path)
    } else {
        None
    };
    if notify {
        let body = auto_saved
            .as_ref()
            .map_or_else(String::new, |p| p.display().to_string());
        kapix_linux::notify::send(
            Urgency::Normal,
            &format!("Copied {}×{}", img.width, img.height),
            &body,
        );
    }
    Ok(())
}

fn emit_text(o: &Opts, cfg: &Config, text: &str, copy: bool, notify: bool) -> Result<(), String> {
    if let Some(p) = &o.out {
        if let Some(dir) = p.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, text).map_err(|e| e.to_string())?;
    }
    if o.stdout {
        std::io::stdout()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        std::io::stdout()
            .write_all(b"\n")
            .map_err(|e| e.to_string())?;
    }
    if copy {
        kapix_linux::clipboard::set_text(text).map_err(|e| e.to_string())?;
    }
    if notify && cfg.ocr.copy_notification {
        let (chars, lines) = ocr_text::stats(text);
        let title = ocr_text::preview(text, 60);
        let body = if lines > 1 || chars > 12 {
            format!(
                "{chars} chars · {lines} line{}",
                if lines == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        };
        kapix_linux::notify::send(Urgency::Normal, &title, &body);
    }
    Ok(())
}

fn save_interactive(cfg: &Config, img: &Image) -> Result<(), String> {
    let dir = PathBuf::from(kapix_linux::paths::expand(&cfg.save.directory));
    let _ = std::fs::create_dir_all(&dir);
    let now = kapix_linux::time::local_now();
    let name = render_filename(&cfg.save.filename_template, &now);
    let jpeg = cfg.save.format == ImageFormat::Jpeg;

    let path = match kapix_linux::dialog::save_image_dialog(&dir, &name, jpeg) {
        DialogOutcome::Chosen(p) => p,
        DialogOutcome::Cancelled => return Ok(()),
        DialogOutcome::Unavailable => {
            let unique = unique_name(&name, cfg.save.format.extension(), |n| dir.join(n).exists());
            dir.join(unique)
        }
    };

    write_image(&path, img)?;
    let file = path
        .file_name()
        .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
    kapix_linux::notify::send(Urgency::Normal, "Saved", &file);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn parses_cli_modes() {
        assert!(matches!(
            parse(&s(&[])).unwrap(),
            Cmd::Capture(Opts {
                mode: Mode::Region,
                interactive: true,
                ..
            })
        ));
        assert!(matches!(parse(&s(&["--help"])).unwrap(), Cmd::Help));
        assert!(matches!(parse(&s(&["--version"])).unwrap(), Cmd::Version));
        let Cmd::Capture(o) = parse(&s(&["capture", "--mode", "ocr", "--stdout"])).unwrap() else {
            panic!("expected capture");
        };
        assert!(o.mode == Mode::Ocr && o.stdout && !o.copy);
        let Cmd::Capture(o) =
            parse(&s(&["capture", "--mode=fullscreen", "--out=/tmp/a.png"])).unwrap()
        else {
            panic!("expected capture");
        };
        assert!(o.mode == Mode::Fullscreen && o.out == Some(PathBuf::from("/tmp/a.png")));
        assert!(parse(&s(&["capture", "--mode", "nope"])).is_err());
    }
}
