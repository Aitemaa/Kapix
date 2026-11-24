//! Native save dialog fallback on Linux (`zenity` or `kdialog`, when available).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogOutcome {
    /// User picked a target path.
    Chosen(PathBuf),
    /// User cancelled the dialog explicitly.
    Cancelled,
    /// Neither `zenity` nor `kdialog` is installed — caller can fall back to direct save.
    Unavailable,
}

/// Prompt the user for a save location via `zenity` or `kdialog`.
pub fn save_image_dialog(dir: &Path, default_name: &str, jpeg: bool) -> DialogOutcome {
    let ext = if jpeg { "jpg" } else { "png" };
    let suggested = dir.join(format!("{default_name}.{ext}"));

    if let Some(res) = try_zenity(&suggested, jpeg) {
        return res;
    }
    if let Some(res) = try_kdialog(&suggested, jpeg) {
        return res;
    }
    DialogOutcome::Unavailable
}

fn try_zenity(suggested: &Path, jpeg: bool) -> Option<DialogOutcome> {
    let filter = if jpeg {
        "JPEG image | *.jpg *.jpeg"
    } else {
        "PNG image | *.png"
    };
    let out = Command::new("zenity")
        .arg("--file-selection")
        .arg("--save")
        .arg("--confirm-overwrite")
        .arg("--title=Save screenshot — Kapix")
        .arg(format!("--filename={}", suggested.display()))
        .arg(format!("--file-filter={filter}"))
        .arg("--file-filter=All files | *")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return Some(DialogOutcome::Cancelled);
    }
    let chosen = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if chosen.is_empty() {
        Some(DialogOutcome::Cancelled)
    } else {
        Some(DialogOutcome::Chosen(PathBuf::from(chosen)))
    }
}

fn try_kdialog(suggested: &Path, jpeg: bool) -> Option<DialogOutcome> {
    let filter = if jpeg {
        "JPEG image (*.jpg *.jpeg);;PNG image (*.png)"
    } else {
        "PNG image (*.png);;JPEG image (*.jpg *.jpeg)"
    };
    let out = Command::new("kdialog")
        .arg("--title")
        .arg("Save screenshot — Kapix")
        .arg("--getsavefilename")
        .arg(suggested)
        .arg(filter)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return Some(DialogOutcome::Cancelled);
    }
    let chosen = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if chosen.is_empty() {
        Some(DialogOutcome::Cancelled)
    } else {
        Some(DialogOutcome::Chosen(PathBuf::from(chosen)))
    }
}
