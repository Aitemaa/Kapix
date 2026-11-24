//! Desktop notifications via `notify-send` (best-effort, non-blocking).

use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

/// Send a desktop notification if `notify-send` is installed. Never fails or blocks.
pub fn send(urgency: Urgency, title: &str, body: &str) {
    let mut cmd = Command::new("notify-send");
    cmd.arg("--app-name=Kapix")
        .arg("--icon=kapix")
        .arg("--expire-time=2500")
        .arg(match urgency {
            Urgency::Low => "--urgency=low",
            Urgency::Normal => "--urgency=normal",
            Urgency::Critical => "--urgency=critical",
        })
        .arg("--")
        .arg(title);
    if !body.is_empty() {
        cmd.arg(body);
    }
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
