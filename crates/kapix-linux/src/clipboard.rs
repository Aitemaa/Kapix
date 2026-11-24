//! Wayland clipboard via `wl-copy` (from `wl-clipboard`).
//!
//! `wl-copy` forks a tiny background helper that serves the Wayland data selection even after
//! `kapix` exits, keeping zero lingering Kapix processes.

use std::io::Write;
use std::process::{Command, Stdio};

use kapix_core::Image;

use crate::{Error, Result};

/// Copy `img` to the Wayland clipboard as `image/png`.
pub fn set_image(img: &Image) -> Result<()> {
    let png = img.encode_png().map_err(|e| Error(e.to_string()))?;
    run_wl_copy(&["--type", "image/png"], &png)
}

/// Copy UTF-8 `text` to the Wayland clipboard.
pub fn set_text(text: &str) -> Result<()> {
    if text.is_empty() {
        let status = Command::new("wl-copy")
            .arg("--clear")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(map_spawn_err)?;
        return if status.success() {
            Ok(())
        } else {
            Err(Error(format!("wl-copy --clear failed ({status})")))
        };
    }
    run_wl_copy(&["--type", "text/plain;charset=utf-8"], text.as_bytes())
}

fn run_wl_copy(args: &[&str], payload: &[u8]) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(map_spawn_err)?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload)
            .map_err(|e| Error(format!("writing to wl-copy failed: {e}")))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| Error(format!("waiting for wl-copy failed: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error(if stderr.is_empty() {
            format!("wl-copy exited with {}", out.status)
        } else {
            format!("wl-copy failed: {stderr}")
        }));
    }
    Ok(())
}

fn map_spawn_err(e: std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        Error("wl-copy not found — install `wl-clipboard` to copy to clipboard".into())
    } else {
        Error(format!("failed to run wl-copy: {e}"))
    }
}
