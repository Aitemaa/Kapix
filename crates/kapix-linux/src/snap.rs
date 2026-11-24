//! Best-effort compositor queries for focused output, focused window, and window-snap rects
//! on Niri, Hyprland, and Sway (zero extra JSON crates).

use std::env;
use std::process::{Command, Stdio};

use kapix_core::Rect;

use crate::capture::{OutputInfo, logical_rect_to_buffer};

/// Name of the currently focused output (e.g. `"eDP-1"`), if the compositor exposes it.
pub fn focused_output_name() -> Option<String> {
    if env::var_os("NIRI_SOCKET").is_some()
        && let Some(out) = run_stdout("niri", &["msg", "-j", "focused-output"])
        && let Some(name) = json_string_field(&out, "name")
    {
        return Some(name);
    }
    if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        && let Some(out) = run_stdout("hyprctl", &["activeworkspace", "-j"])
        && let Some(name) = json_string_field(&out, "monitor")
    {
        return Some(name);
    }
    if env::var_os("SWAYSOCK").is_some()
        && let Some(out) = run_stdout("swaymsg", &["-t", "get_outputs"])
    {
        // Find the object containing `"focused": true`.
        for obj in split_top_objects(&out) {
            if json_bool_field(obj, "focused") == Some(true)
                && let Some(name) = json_string_field(obj, "name")
            {
                return Some(name);
            }
        }
    }
    None
}

/// Focused output buffer rect inside the captured frame (falls back to the first output).
pub fn focused_output_rect(outputs: &[OutputInfo]) -> Option<Rect> {
    if let Some(name) = focused_output_name()
        && let Some(o) = outputs.iter().find(|o| o.name == name)
    {
        return Some(o.buffer_rect);
    }
    outputs.first().map(|o| o.buffer_rect)
}

/// Focused window rect in buffer coordinates, if the compositor reports exact coordinates.
pub fn focused_window_rect(outputs: &[OutputInfo]) -> Option<Rect> {
    if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        && let Some(out) = run_stdout("hyprctl", &["activewindow", "-j"])
        && let (Some((x, y)), Some((w, h))) =
            (json_pair_field(&out, "at"), json_pair_field(&out, "size"))
        && w > 0
        && h > 0
    {
        return logical_rect_to_buffer(outputs, Rect::new(x, y, w, h));
    }

    if env::var_os("NIRI_SOCKET").is_some()
        && let Some(out) = run_stdout("niri", &["msg", "-j", "focused-window"])
        && let Some(rect) = parse_niri_window_rect(&out, outputs)
    {
        return Some(rect);
    }

    None
}

/// Visible window snap rectangles in buffer coordinates (best-effort across Hyprland & Niri).
pub fn window_rects(outputs: &[OutputInfo]) -> Vec<Rect> {
    let mut rects = Vec::new();

    if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        && let Some(out) = run_stdout("hyprctl", &["clients", "-j"])
    {
        for obj in split_top_objects(&out) {
            if json_bool_field(obj, "hidden") == Some(true)
                || json_bool_field(obj, "mapped") == Some(false)
            {
                continue;
            }
            if let (Some((x, y)), Some((w, h))) =
                (json_pair_field(obj, "at"), json_pair_field(obj, "size"))
                && w > 8
                && h > 8
                && let Some(r) = logical_rect_to_buffer(outputs, Rect::new(x, y, w, h))
            {
                rects.push(r);
            }
        }
        return rects;
    }

    if env::var_os("NIRI_SOCKET").is_some()
        && let Some(out) = run_stdout("niri", &["msg", "-j", "windows"])
    {
        for obj in split_top_objects(&out) {
            if let Some(r) = parse_niri_window_rect(obj, outputs) {
                rects.push(r);
            }
        }
    }

    rects
}

fn parse_niri_window_rect(obj: &str, outputs: &[OutputInfo]) -> Option<Rect> {
    // Niri reports exact screen coordinates in `tile_pos_in_workspace_view` for floating windows.
    let (x, y) = json_pair_field(obj, "tile_pos_in_workspace_view")?;
    let (w, h) = json_pair_field(obj, "window_size")?;
    if w <= 8 || h <= 8 {
        return None;
    }
    let base = focused_output_name()
        .and_then(|name| outputs.iter().find(|o| o.name == name))
        .or_else(|| outputs.first())?;
    let logical = Rect::new(base.logical_rect.x + x, base.logical_rect.y + y, w, h);
    logical_rect_to_buffer(outputs, logical)
}

fn run_stdout(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Extract a JSON string value for `"key": "value"`.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut rest = json;
    while let Some(pos) = rest.find(&needle) {
        rest = &rest[pos + needle.len()..];
        let after = rest.trim_start();
        let Some(after) = after.strip_prefix(':') else {
            continue;
        };
        let after = after.trim_start();
        let mut chars = after.strip_prefix('"').map(|s| s.chars())?;
        let mut out = String::new();
        while let Some(c) = chars.next() {
            if c == '"' {
                return Some(out);
            }
            if c == '\\'
                && let Some(esc) = chars.next()
            {
                out.push(esc);
            } else {
                out.push(c);
            }
        }
        return None;
    }
    None
}

fn json_bool_field(json: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after = json[pos + needle.len()..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

/// Extract a 2-element numeric JSON array `[a, b]` (accepts integers or floats).
fn json_pair_field(json: &str, key: &str) -> Option<(i32, i32)> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after = json[pos + needle.len()..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let inside = after.strip_prefix('[')?;
    let end = inside.find(']')?;
    let mut parts = inside[..end].split(',');
    let a = parts.next()?.trim().parse::<f64>().ok()?.round() as i32;
    let b = parts.next()?.trim().parse::<f64>().ok()?.round() as i32;
    Some((a, b))
}

/// Split top-level JSON objects inside a JSON array `[ {...}, {...} ]`.
fn split_top_objects(json: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in json.bytes().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0
                    && let Some(s) = start.take()
                {
                    out.push(&json[s..=i]);
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_fields() {
        let sample = r#"{"name":"eDP-1","focused":true,"at":[10, 20],"size":[1280.0, 720.0]}"#;
        assert_eq!(json_string_field(sample, "name").as_deref(), Some("eDP-1"));
        assert_eq!(json_bool_field(sample, "focused"), Some(true));
        assert_eq!(json_pair_field(sample, "at"), Some((10, 20)));
        assert_eq!(json_pair_field(sample, "size"), Some((1280, 720)));
    }

    #[test]
    fn splits_top_level_array_objects() {
        let arr = r#"[{"a":1,"b":{"c":"}x"}},{"a":2}]"#;
        let objs = split_top_objects(arr);
        assert_eq!(objs.len(), 2);
        assert!(objs[0].contains(r#""a":1"#));
        assert!(objs[1].contains(r#""a":2"#));
    }
}
