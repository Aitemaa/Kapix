//! Toast notification content (the host shows it in a layered, non-activating topmost window).

use kapix_core::{Image, Rgba};
use tiny_skia::{FillRule, Pixmap, Stroke, Transform};

use crate::canvas::{self, paint, rrect_path};
use crate::font::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub title: String,
    /// Optional second line (e.g. OCR text preview or "42 chars · 3 lines").
    pub body: String,
}

const CARD: Rgba = Rgba::rgb(0x1F, 0x29, 0x37);
const TITLE: Rgba = Rgba::rgb(0xF9, 0xFA, 0xFB);
const MUTED: Rgba = Rgba::rgb(0x9C, 0xA3, 0xAF);
const MAX_BODY_LINES: usize = 3;

fn accent(kind: ToastKind) -> Rgba {
    match kind {
        ToastKind::Success => Rgba::rgb(0x10, 0xB9, 0x81),
        ToastKind::Info => Rgba::rgb(0x38, 0xA1, 0xF5),
        ToastKind::Error => Rgba::rgb(0xEF, 0x44, 0x44),
    }
}

/// Render a rounded dark pill/card with an accent dot for `kind`, title and body.
/// Output is **premultiplied BGRA with transparent corners** suitable for
/// `UpdateLayeredWindow(ULW_ALPHA)`. `scale` = monitor DPI scale (1.0 = 96 DPI).
/// Width adapts to text (min 220, max 420 logical px). Without a font, renders the card only.
pub fn render(toast: &Toast, font: Option<&Font>, scale: f32) -> Image {
    let s = scale.max(0.5);
    let pad = 14.0 * s;
    let dot_col = 20.0 * s; // width of the dot gutter
    let title_px = 14.0 * s;
    let body_px = 12.5 * s;
    let (min_w, max_w) = (220.0 * s, 420.0 * s);
    let text_max = max_w - pad * 2.0 - dot_col;

    // Lay out body lines (word-wrapped, capped, ellipsized).
    let mut body_lines: Vec<String> = Vec::new();
    let mut title = toast.title.clone();
    if let Some(f) = font {
        title = ellipsize(f, &title, title_px, text_max, true);
        if !toast.body.trim().is_empty() {
            body_lines = wrap(f, &toast.body, body_px, text_max, MAX_BODY_LINES);
        }
    }

    let (title_w, title_h, body_w, body_lh, body_h) = match font {
        Some(f) => {
            let tw = f.measure_bold(&title, title_px);
            let th = f.measure("", title_px).1;
            let bw = body_lines
                .iter()
                .map(|l| f.measure(l, body_px).0)
                .fold(0.0, f32::max);
            let lh = f.line_height(body_px);
            let bh = if body_lines.is_empty() {
                0.0
            } else {
                lh * (body_lines.len() - 1) as f32 + f.measure("", body_px).1
            };
            (tw, th, bw, lh, bh)
        }
        None => (
            0.0,
            title_px,
            0.0,
            body_px,
            if toast.body.is_empty() { 0.0 } else { body_px },
        ),
    };
    let gap = if body_lines.is_empty() { 0.0 } else { 4.0 * s };
    let w = (pad * 2.0 + dot_col + title_w.max(body_w))
        .clamp(min_w, max_w)
        .ceil();
    let h = (pad * 2.0 + title_h + gap + body_h).max(44.0 * s).ceil();
    let (wi, hi) = (w as u32, h as u32);

    let mut pm = Pixmap::new(wi, hi).expect("non-empty toast");
    let radius = (12.0 * s).min(h / 2.0);
    if let Some(path) = rrect_path(0.0, 0.0, w, h, radius) {
        pm.fill_path(
            &path,
            &paint(CARD.with_alpha(250)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    // Hairline border for definition on dark backgrounds.
    if let Some(path) = rrect_path(0.5, 0.5, w - 1.0, h - 1.0, radius - 0.5) {
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pm.stroke_path(
            &path,
            &paint(Rgba::rgb(255, 255, 255).with_alpha(26)),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    let content_h = title_h + gap + body_h;
    let top = (h - content_h) / 2.0;
    // Accent dot, vertically aligned with the title's x-height center.
    let dot_r = 4.5 * s;
    let (dcx, dcy) = (pad + dot_r, top + title_h * 0.52);
    if let Some(path) = tiny_skia::PathBuilder::from_circle(dcx, dcy, dot_r + 3.0 * s) {
        pm.fill_path(
            &path,
            &paint(accent(toast.kind).with_alpha(46)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    if let Some(path) = tiny_skia::PathBuilder::from_circle(dcx, dcy, dot_r) {
        pm.fill_path(
            &path,
            &paint(accent(toast.kind)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    if let Some(f) = font {
        let x = pad + dot_col;
        f.draw_bold(&mut pm, &title, x, top, title_px, TITLE);
        let mut y = top + title_h + gap;
        for line in &body_lines {
            f.draw(&mut pm, line, x, y, body_px, MUTED);
            y += body_lh;
        }
    }
    canvas::pixmap_to_image(pm)
}

/// Shorten `text` with an ellipsis so it fits `max_w`.
fn ellipsize(f: &Font, text: &str, px: f32, max_w: f32, bold: bool) -> String {
    let width = |t: &str| {
        if bold {
            f.measure_bold(t, px)
        } else {
            f.measure(t, px).0
        }
    };
    let text = text.replace('\n', " ");
    if width(&text) <= max_w {
        return text;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        n -= 1;
        let cand: String = chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
        if width(&cand) <= max_w {
            return cand;
        }
    }
    "…".to_string()
}

/// Word-wrap `text` (honoring `\n`) to `max_w`, at most `max_lines` lines; the last line is
/// ellipsized when text remains.
fn wrap(f: &Font, text: &str, px: f32, max_w: f32, max_lines: usize) -> Vec<String> {
    let fits = |t: &str| f.measure(t, px).0 <= max_w;
    let mut lines: Vec<String> = Vec::new();
    for para in text.trim_end().split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let trial = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if fits(&trial) {
                cur = trial;
                continue;
            }
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            // A single word wider than the line: hard-cut it.
            let mut rest: Vec<char> = word.chars().collect();
            while !fits(&rest.iter().collect::<String>()) && rest.len() > 1 {
                let mut cut = rest.len() - 1;
                while cut > 1 && !fits(&rest[..cut].iter().collect::<String>()) {
                    cut -= 1;
                }
                lines.push(rest[..cut].iter().collect());
                rest.drain(..cut);
            }
            cur = rest.into_iter().collect();
        }
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            *last = ellipsize(f, &format!("{last}…"), px, max_w, false);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::test_font;

    fn t(kind: ToastKind, title: &str, body: &str) -> Toast {
        Toast {
            kind,
            title: title.into(),
            body: body.into(),
        }
    }

    #[test]
    fn card_without_font_has_transparent_corners_and_min_width() {
        let img = render(&t(ToastKind::Success, "Copied", ""), None, 1.0);
        assert_eq!(img.width, 220);
        assert_eq!(img.data.len(), (img.width * img.height * 4) as usize);
        assert_eq!(img.pixel(0, 0)[3], 0);
        let mid = img.pixel(img.width - 40, img.height / 2);
        assert!(mid[3] > 240, "card is (nearly) opaque inside: {mid:?}");
    }

    #[test]
    fn premultiplied_output() {
        let img = render(
            &t(ToastKind::Error, "Oops", "x"),
            test_font::load().as_ref(),
            1.0,
        );
        for p in img.data.chunks_exact(4) {
            assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{p:?}");
        }
    }

    #[test]
    fn accent_dot_is_the_kind_color_in_bgra() {
        let img = render(&t(ToastKind::Error, "Failed", ""), None, 1.0);
        let found = img
            .data
            .chunks_exact(4)
            .any(|p| p[2] > 220 && p[1] < 90 && p[0] < 90 && p[3] == 255);
        assert!(found, "red dot present");
    }

    #[test]
    fn width_adapts_and_is_capped_and_scaled() {
        let Some(f) = test_font::load() else { return };
        let short = render(&t(ToastKind::Info, "Hi", ""), Some(&f), 1.0);
        let long = render(
            &t(
                ToastKind::Info,
                "A rather long toast title that really cannot fit on a single line of this card at all",
                "body",
            ),
            Some(&f),
            1.0,
        );
        assert_eq!(short.width, 220);
        assert!((380..=420).contains(&long.width), "{}", long.width);
        let big = render(&t(ToastKind::Info, "Hi", ""), Some(&f), 1.5);
        assert_eq!(big.width, 330);
        let body = render(
            &t(
                ToastKind::Success,
                "Text copied",
                "line one\nline two with more words in it to wrap around the card for sure yes\nthree\nfour",
            ),
            Some(&f),
            1.0,
        );
        assert!(body.height > short.height + 20);
    }
}
