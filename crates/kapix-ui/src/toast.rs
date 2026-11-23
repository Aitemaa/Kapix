//! Toast notification content (the host shows it in a layered, non-activating topmost window).

use kapix_core::{Image, Rect};
use tiny_skia::Pixmap;

use crate::canvas::{self, fill_rrect, stroke_rrect};
use crate::font::Font;
use crate::icons::{self, Icon};
use crate::shadow;
use crate::theme::{Palette, RADIUS_PAD};

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

/// A rendered toast: the bitmap (card plus transparent shadow margins) and where the visible
/// card sits inside it, so the host can keep the *card* — not the shadow — at its screen margin.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// Premultiplied BGRA for `UpdateLayeredWindow(ULW_ALPHA)`.
    pub image: Image,
    /// The card rect inside `image`.
    pub card: Rect,
}

const MAX_BODY_LINES: usize = 3;

fn icon_for(kind: ToastKind, p: &Palette) -> (Icon, kapix_core::Rgba) {
    match kind {
        ToastKind::Success => (Icon::Check, p.text),
        ToastKind::Info => (Icon::Info, p.muted),
        ToastKind::Error => (Icon::Alert, p.red_text),
    }
}

/// Render a Kavox popover card: `panel` fill, 1 px `line` border, radius 16, soft shadow, a
/// 20 px lucide-style status icon, Medium title and Regular muted body (max 3 lines).
/// `scale` = monitor DPI scale (1.0 = 96 DPI). Card width adapts to the text (min 220, max 420
/// logical px).
pub fn render(toast: &Toast, font: &Font, p: &Palette, scale: f32) -> Rendered {
    let s = scale.max(0.5);
    let pad = 14.0 * s;
    let icon_sz = 20.0 * s;
    let icon_gap = 12.0 * s;
    let gutter = icon_sz + icon_gap;
    let title_px = 14.0 * s;
    let body_px = 12.0 * s;
    let (min_w, max_w) = (220.0 * s, 420.0 * s);
    let text_max = max_w - pad * 2.0 - gutter;
    let (title_f, body_f) = (font.medium(), font.regular());

    let title = ellipsize(&title_f, &toast.title, title_px, text_max);
    let body_lines = if toast.body.trim().is_empty() {
        Vec::new()
    } else {
        wrap(&body_f, &toast.body, body_px, text_max, MAX_BODY_LINES)
    };

    let (title_w, title_h) = title_f.measure(&title, title_px);
    let body_w = body_lines
        .iter()
        .map(|l| body_f.measure(l, body_px).0)
        .fold(0.0, f32::max);
    let body_lh = body_f.line_height(body_px);
    let body_h = if body_lines.is_empty() {
        0.0
    } else {
        body_lh * (body_lines.len() - 1) as f32 + body_f.measure("", body_px).1
    };
    let gap = if body_lines.is_empty() { 0.0 } else { 3.0 * s };
    let w = (pad * 2.0 + gutter + title_w.max(body_w))
        .clamp(min_w, max_w)
        .ceil();
    let h = (pad * 2.0 + (title_h + gap + body_h).max(icon_sz))
        .max(48.0 * s)
        .ceil();

    let (ml, mt, mr, mb) = shadow::margins(s);
    let (wi, hi) = (w as i32 + ml + mr, h as i32 + mt + mb);
    let card = Rect::new(ml, mt, w as i32, h as i32);
    let mut pm = Pixmap::new(wi as u32, hi as u32).expect("non-empty toast");
    let radius = RADIUS_PAD * s;
    shadow::draw(&mut pm, card, radius, s, p.shadow);
    fill_rrect(&mut pm, card, radius, p.panel);
    stroke_rrect(&mut pm, card, radius, 1.0, p.line);

    let (cx, cy) = (card.x as f32, card.y as f32);
    let content_h = title_h + gap + body_h;
    let top = cy + (h - content_h) / 2.0;
    let (icon, icon_col) = icon_for(toast.kind, p);
    icons::draw(
        &mut pm,
        icon,
        cx + pad + icon_sz / 2.0,
        top + title_h / 2.0,
        icon_sz,
        icon_col,
        false,
    );
    let x = cx + pad + gutter;
    title_f.draw(&mut pm, &title, x, top, title_px, p.text);
    let mut y = top + title_h + gap;
    for line in &body_lines {
        body_f.draw(&mut pm, line, x, y, body_px, p.muted);
        y += body_lh;
    }
    Rendered {
        image: canvas::pixmap_to_image(pm),
        card,
    }
}

/// Shorten `text` with an ellipsis so it fits `max_w`.
fn ellipsize(f: &Font, text: &str, px: f32, max_w: f32) -> String {
    let width = |t: &str| f.measure(t, px).0;
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
            *last = ellipsize(f, &format!("{last}…"), px, max_w);
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

    fn r(kind: ToastKind, title: &str, body: &str, p: &Palette, s: f32) -> Rendered {
        render(&t(kind, title, body), &test_font::load(), p, s)
    }

    #[test]
    fn card_has_transparent_margins_and_min_width() {
        for p in [Palette::DARK, Palette::LIGHT] {
            let out = r(ToastKind::Success, "Copied", "", &p, 1.0);
            let img = &out.image;
            assert_eq!(out.card.w, 220);
            assert_eq!(img.data.len(), (img.width * img.height * 4) as usize);
            assert_eq!(img.pixel(0, 0)[3], 0, "outer shadow margin is transparent");
            // Card corner is rounded (transparent-ish), card body is opaque panel.
            let mid = img.pixel(
                (out.card.right() - 20) as u32,
                (out.card.y + out.card.h / 2) as u32,
            );
            assert_eq!(mid[3], 255, "card is opaque inside: {mid:?}");
            assert_eq!([mid[2], mid[1], mid[0]], [p.panel.r, p.panel.g, p.panel.b]);
            // Shadow shows below the card.
            let below = img.pixel(img.width / 2, (out.card.bottom() + 4) as u32);
            assert!(below[3] > 0, "shadow under the card");
        }
    }

    #[test]
    fn premultiplied_output() {
        let out = r(ToastKind::Error, "Oops", "x", &Palette::LIGHT, 1.0);
        for p in out.image.data.chunks_exact(4) {
            assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{p:?}");
        }
    }

    #[test]
    fn error_icon_uses_red_text() {
        let p = Palette::DARK;
        let out = r(ToastKind::Error, "Failed", "", &p, 2.0);
        let found = out.image.data.chunks_exact(4).any(|q| {
            q[3] == 255
                && (q[2] as i32 - p.red_text.r as i32).abs() < 12
                && (q[1] as i32 - p.red_text.g as i32).abs() < 12
                && (q[0] as i32 - p.red_text.b as i32).abs() < 12
        });
        assert!(found, "red_text pixels present");
    }

    #[test]
    fn width_adapts_and_is_capped_and_scaled() {
        let p = Palette::DARK;
        let short = r(ToastKind::Info, "Hi", "", &p, 1.0);
        let long = r(
            ToastKind::Info,
            "A rather long toast title that really cannot fit on a single line of this card at all",
            "body",
            &p,
            1.0,
        );
        assert_eq!(short.card.w, 220);
        assert!((380..=420).contains(&long.card.w), "{}", long.card.w);
        let big = r(ToastKind::Info, "Hi", "", &p, 1.5);
        assert_eq!(big.card.w, 330);
        let body = r(
            ToastKind::Success,
            "Text copied",
            "line one\nline two with more words in it to wrap around the card for sure yes\nthree\nfour",
            &p,
            1.0,
        );
        assert!(body.card.h > short.card.h + 20);
    }

    #[test]
    fn non_latin_text_uses_the_fallback() {
        let Some(bytes) = test_font::system_fallback() else {
            return;
        };
        let f = Font::new(Some(bytes));
        let plain = render(
            &t(ToastKind::Info, "ж", ""),
            &Font::builtin(),
            &Palette::DARK,
            1.0,
        );
        let with = render(&t(ToastKind::Info, "ж", ""), &f, &Palette::DARK, 1.0);
        let ink = |o: &Rendered| {
            o.image
                .data
                .chunks_exact(4)
                .filter(|q| q[2] > 150 && q[1] > 150 && q[3] == 255)
                .count()
        };
        assert!(ink(&with) > 10, "fallback glyph is drawn");
        let _ = plain;
    }
}
