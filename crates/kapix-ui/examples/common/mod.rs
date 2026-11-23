//! Shared helpers for the examples: a synthetic two-monitor desktop and scripted input.
#![allow(dead_code)]

use kapix_core::config::DefaultAction;
use kapix_core::{Image, Monitor, Rect, Rgba, VirtualDesktop};
use kapix_ui::canvas::{image_to_pixmap, paint, pixmap_to_image, sk_color};
use kapix_ui::{Font, Input, Key, Mods, MouseButton, Outcome, Overlay, OverlayInit, OverlayMode};
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, PathBuilder, Pixmap, Point, SpreadMode, Transform,
};

pub const W: u32 = 2560;
pub const H: u32 = 1440;

pub fn load_font() -> Option<Font> {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        "C:\\Windows\\Fonts\\segoeui.ttf".to_string(),
        format!("{home}/.local/share/fonts/inir/inter/Inter-Regular.ttf"),
        "/usr/share/fonts/TTF/Roboto-Regular.ttf".to_string(),
        "/usr/share/fonts/TTF/OpenSans-Regular.ttf".to_string(),
        "/usr/share/fonts/TTF/DejaVuSans.ttf".to_string(),
    ];
    candidates
        .iter()
        .find_map(|p| std::fs::read(p).ok().and_then(Font::from_bytes))
}

/// Two 1280×1440 monitors; the left one sits at negative virtual X (scale 1.0), the right one is
/// primary at scale 1.5.
pub fn desktop() -> VirtualDesktop {
    VirtualDesktop {
        bounds: Rect::new(-1280, 0, 2560, 1440),
        monitors: vec![
            Monitor {
                rect: Rect::new(-1280, 0, 1280, 1440),
                work: Rect::new(-1280, 0, 1280, 1400),
                scale: 1.0,
                primary: false,
            },
            Monitor {
                rect: Rect::new(0, 0, 1280, 1440),
                work: Rect::new(0, 0, 1280, 1400),
                scale: 1.5,
                primary: true,
            },
        ],
    }
}

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let k = r * 0.5523;
    let (x1, y1) = (x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x1 - r, y);
    pb.cubic_to(x1 - r + k, y, x1, y + r - k, x1, y + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x + r, y1);
    pb.cubic_to(x + r - k, y1, x, y1 - r + k, x, y1 - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

fn fill(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgba) {
    pm.fill_path(
        &rrect(x, y, w, h, r),
        &paint(c),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn window(
    pm: &mut Pixmap,
    r: Rect,
    title: &str,
    body: Rgba,
    bar: Rgba,
    font: Option<&Font>,
    title_col: Rgba,
) {
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    for i in (1..=14).rev() {
        let a = (2 + (14 - i) / 3) as u8;
        fill(
            pm,
            x - i as f32,
            y - i as f32 + 8.0,
            w + 2.0 * i as f32,
            h + 2.0 * i as f32,
            10.0 + i as f32,
            Rgba::rgb(0, 0, 0).with_alpha(a),
        );
    }
    fill(pm, x, y, w, h, 10.0, body);
    // Title bar (top corners rounded by drawing a rounded block then a square block below).
    fill(pm, x, y, w, 40.0, 10.0, bar);
    fill(pm, x, y + 20.0, w, 20.0, 0.0, bar);
    for (i, c) in [
        Rgba::rgb(0xFF, 0x5F, 0x57),
        Rgba::rgb(0xFE, 0xBC, 0x2E),
        Rgba::rgb(0x28, 0xC8, 0x40),
    ]
    .iter()
    .enumerate()
    {
        if let Some(p) = PathBuilder::from_circle(x + 22.0 + i as f32 * 22.0, y + 20.0, 6.5) {
            pm.fill_path(
                &p,
                &paint(*c),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
    if let Some(f) = font {
        let (tw, th) = f.measure(title, 14.0);
        f.draw(
            pm,
            title,
            x + (w - tw) / 2.0,
            y + (40.0 - th) / 2.0,
            14.0,
            title_col,
        );
    }
}

pub struct Scene {
    pub frame: Image,
    pub windows: Vec<Rect>,
    /// Region containing the "secret" token in the terminal (for pixelate).
    pub secret: Rect,
}

pub fn scene(font: Option<&Font>) -> Scene {
    let mut pm = Pixmap::new(W, H).unwrap();
    // Wallpaper: deep gradient.
    let shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(W as f32, H as f32),
        vec![
            GradientStop::new(0.0, sk_color(Rgba::rgb(0x1E, 0x1B, 0x4B))),
            GradientStop::new(0.5, sk_color(Rgba::rgb(0x31, 0x2E, 0x81))),
            GradientStop::new(1.0, sk_color(Rgba::rgb(0x0F, 0x76, 0x6E))),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    let p = tiny_skia::Paint {
        shader,
        ..Default::default()
    };
    pm.fill_rect(
        tiny_skia::Rect::from_xywh(0.0, 0.0, W as f32, H as f32).unwrap(),
        &p,
        Transform::identity(),
        None,
    );
    // Soft blobs for visual texture.
    for (cx, cy, r, c) in [
        (300.0, 1250.0, 420.0, Rgba::rgb(0xEC, 0x48, 0x99)),
        (2100.0, 200.0, 360.0, Rgba::rgb(0x38, 0xBD, 0xF8)),
    ] {
        for i in 0..24 {
            let rr = r * (1.0 - i as f32 / 24.0);
            if let Some(path) = PathBuilder::from_circle(cx, cy, rr) {
                pm.fill_path(
                    &path,
                    &paint(c.with_alpha(5)),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    let ink = Rgba::rgb(0x1F, 0x29, 0x37);
    let grey = Rgba::rgb(0xD1, 0xD5, 0xDB);

    // --- Browser (left monitor) ---
    let browser = Rect::new(80, 90, 1100, 680);
    window(
        &mut pm,
        browser,
        "Quarterly report - Browser",
        Rgba::rgb(0xF8, 0xFA, 0xFC),
        Rgba::rgb(0xE5, 0xE7, 0xEB),
        font,
        ink,
    );
    if let Some(f) = font {
        f.draw(&mut pm, "Quarterly report", 130.0, 170.0, 30.0, ink);
        f.draw(
            &mut pm,
            "Revenue grew 18% year over year, driven by the new self-serve plan.",
            130.0,
            220.0,
            16.0,
            Rgba::rgb(0x4B, 0x55, 0x63),
        );
        f.draw(
            &mut pm,
            "Churn dropped to 2.1% after the onboarding redesign shipped in March.",
            130.0,
            248.0,
            16.0,
            Rgba::rgb(0x4B, 0x55, 0x63),
        );
        let rows = [
            ("Region", "Revenue", "Growth"),
            ("North America", "$4.2M", "+21%"),
            ("Europe", "$2.9M", "+14%"),
            ("Asia Pacific", "$1.7M", "+26%"),
            ("Latin America", "$0.6M", "+9%"),
        ];
        for (i, (a, b, c)) in rows.iter().enumerate() {
            let y = 310.0 + i as f32 * 46.0;
            if i == 0 {
                fill(
                    &mut pm,
                    130.0,
                    y - 8.0,
                    700.0,
                    40.0,
                    6.0,
                    Rgba::rgb(0xE0, 0xE7, 0xFF),
                );
            } else if i % 2 == 0 {
                fill(
                    &mut pm,
                    130.0,
                    y - 8.0,
                    700.0,
                    40.0,
                    6.0,
                    Rgba::rgb(0xF1, 0xF5, 0xF9),
                );
            }
            let col = if i == 0 {
                Rgba::rgb(0x31, 0x2E, 0x81)
            } else {
                ink
            };
            f.draw(&mut pm, a, 150.0, y, 16.0, col);
            f.draw(&mut pm, b, 440.0, y, 16.0, col);
            f.draw(
                &mut pm,
                c,
                650.0,
                y,
                16.0,
                if i == 0 {
                    col
                } else {
                    Rgba::rgb(0x04, 0x78, 0x57)
                },
            );
        }
    }
    // Chart bars.
    for (i, hh) in [90.0, 130.0, 110.0, 170.0, 150.0, 200.0].iter().enumerate() {
        fill(
            &mut pm,
            880.0 + i as f32 * 48.0,
            570.0 - hh,
            34.0,
            *hh,
            5.0,
            Rgba::rgb(0x63, 0x66, 0xF1).with_alpha(if i == 5 { 255 } else { 150 }),
        );
    }
    for i in 0..3 {
        fill(
            &mut pm,
            130.0,
            560.0 + i as f32 * 28.0,
            520.0 - i as f32 * 90.0,
            10.0,
            5.0,
            grey,
        );
    }

    // --- Terminal (left monitor, below) ---
    let term = Rect::new(150, 850, 980, 440);
    window(
        &mut pm,
        term,
        "bash - deploy",
        Rgba::rgb(0x0F, 0x17, 0x2A),
        Rgba::rgb(0x1E, 0x29, 0x3B),
        font,
        Rgba::rgb(0x94, 0xA3, 0xB8),
    );
    let mut secret = Rect::new(0, 0, 0, 0);
    if let Some(f) = font {
        let lines = [
            ("$ git push origin main", Rgba::rgb(0xE2, 0xE8, 0xF0)),
            (
                "Enumerating objects: 42, done.",
                Rgba::rgb(0x94, 0xA3, 0xB8),
            ),
            (
                "remote: Resolving deltas: 100% (17/17), done.",
                Rgba::rgb(0x94, 0xA3, 0xB8),
            ),
            (
                "$ export API_TOKEN=sk_live_51NxQ7mRk0aZy3PqWd9",
                Rgba::rgb(0xFB, 0xBF, 0x24),
            ),
            ("$ ./deploy --env production", Rgba::rgb(0xE2, 0xE8, 0xF0)),
            (
                "deploying kapix-api v2.4.1 ... ok",
                Rgba::rgb(0x34, 0xD3, 0x99),
            ),
            ("health check passed (3/3)", Rgba::rgb(0x34, 0xD3, 0x99)),
        ];
        for (i, (t, c)) in lines.iter().enumerate() {
            let y = 910.0 + i as f32 * 34.0;
            f.draw(&mut pm, t, 180.0, y, 17.0, *c);
            if i == 3 {
                let (pre, _) = f.measure("$ export API_TOKEN=", 17.0);
                let (full, h) = f.measure(t, 17.0);
                secret = Rect::new(
                    (180.0 + pre) as i32 - 2,
                    y as i32 - 3,
                    (full - pre) as i32 + 6,
                    h as i32 + 6,
                );
            }
        }
    }

    // --- Editor (right monitor) ---
    let editor = Rect::new(1400, 160, 1000, 760);
    window(
        &mut pm,
        editor,
        "main.rs - Editor",
        Rgba::rgb(0x1E, 0x1E, 0x2E),
        Rgba::rgb(0x18, 0x18, 0x25),
        font,
        Rgba::rgb(0xA6, 0xAD, 0xC8),
    );
    if let Some(f) = font {
        let code = [
            ("fn main() {", Rgba::rgb(0xCB, 0xA6, 0xF7)),
            (
                "    let overlay = Overlay::new(init);",
                Rgba::rgb(0xCD, 0xD6, 0xF4),
            ),
            (
                "    while let Some(ev) = events.next() {",
                Rgba::rgb(0xCD, 0xD6, 0xF4),
            ),
            (
                "        match overlay.handle(ev) {",
                Rgba::rgb(0xCD, 0xD6, 0xF4),
            ),
            (
                "            Outcome::Copy(img) => clipboard::set(img),",
                Rgba::rgb(0xA6, 0xE3, 0xA1),
            ),
            (
                "            Outcome::Cancel => break,",
                Rgba::rgb(0xF3, 0x8B, 0xA8),
            ),
            ("            _ => {}", Rgba::rgb(0x93, 0x99, 0xB2)),
            ("        }", Rgba::rgb(0xCD, 0xD6, 0xF4)),
            ("    }", Rgba::rgb(0xCD, 0xD6, 0xF4)),
            ("}", Rgba::rgb(0xCB, 0xA6, 0xF7)),
        ];
        for (i, (t, c)) in code.iter().enumerate() {
            f.draw(
                &mut pm,
                &format!("{:>2}", i + 1),
                1430.0,
                220.0 + i as f32 * 34.0,
                16.0,
                Rgba::rgb(0x58, 0x5B, 0x70),
            );
            f.draw(&mut pm, t, 1480.0, 220.0 + i as f32 * 34.0, 17.0, *c);
        }
    }
    // --- Small dialog (right monitor, on top of the editor) ---
    let dialog = Rect::new(1620, 520, 520, 260);
    window(
        &mut pm,
        dialog,
        "Settings",
        Rgba::rgb(0xFF, 0xFF, 0xFF),
        Rgba::rgb(0xF3, 0xF4, 0xF6),
        font,
        ink,
    );
    if let Some(f) = font {
        f.draw(&mut pm, "Launch at startup", 1650.0, 590.0, 17.0, ink);
        f.draw(&mut pm, "Show magnifier", 1650.0, 640.0, 17.0, ink);
        fill(
            &mut pm,
            2040.0,
            588.0,
            70.0,
            28.0,
            14.0,
            Rgba::rgb(0x10, 0xB9, 0x81),
        );
        fill(
            &mut pm,
            2040.0,
            638.0,
            70.0,
            28.0,
            14.0,
            Rgba::rgb(0xD1, 0xD5, 0xDB),
        );
        fill(
            &mut pm,
            2020.0 + 40.0,
            590.0,
            24.0,
            24.0,
            12.0,
            Rgba::rgb(255, 255, 255),
        );
        fill(
            &mut pm,
            2042.0,
            640.0,
            24.0,
            24.0,
            12.0,
            Rgba::rgb(255, 255, 255),
        );
        fill(
            &mut pm,
            2010.0,
            710.0,
            110.0,
            40.0,
            8.0,
            Rgba::rgb(0x4F, 0x46, 0xE5),
        );
        f.draw(
            &mut pm,
            "Save",
            2043.0,
            720.0,
            17.0,
            Rgba::rgb(255, 255, 255),
        );
    }

    Scene {
        frame: pixmap_to_image(pm),
        windows: vec![dialog, editor, term, browser],
        secret,
    }
}

pub fn new_overlay(
    scene: &Scene,
    font: Option<Font>,
    cursor: (i32, i32),
    action: DefaultAction,
    mode: OverlayMode,
) -> Overlay {
    Overlay::new(OverlayInit {
        frame: scene.frame.clone(),
        desktop: desktop(),
        windows: scene.windows.clone(),
        mode,
        default_action: action,
        show_magnifier: true,
        window_snap: true,
        font,
        cursor,
    })
}

// ---- scripted input ----------------------------------------------------------------------

pub fn no_mods() -> Mods {
    Mods::default()
}

pub fn mv(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    let out = o.handle(Input::MouseMove {
        x,
        y,
        mods: no_mods(),
    });
    o.paint();
    out
}
pub fn down(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    o.handle(Input::MouseDown {
        x,
        y,
        button: MouseButton::Left,
        mods: no_mods(),
    })
}
pub fn up(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    o.handle(Input::MouseUp {
        x,
        y,
        button: MouseButton::Left,
        mods: no_mods(),
    })
}
pub fn drag(o: &mut Overlay, from: (i32, i32), to: (i32, i32)) -> Outcome {
    mv(o, from.0, from.1);
    down(o, from.0, from.1);
    let steps = 12;
    for i in 1..=steps {
        mv(
            o,
            from.0 + (to.0 - from.0) * i / steps,
            from.1 + (to.1 - from.1) * i / steps,
        );
    }
    up(o, to.0, to.1)
}
pub fn key(o: &mut Overlay, c: char) -> Outcome {
    let out = o.handle(Input::KeyDown {
        key: Key::Char(c),
        mods: no_mods(),
    });
    o.paint();
    out
}
pub fn type_text(o: &mut Overlay, s: &str) {
    for c in s.chars() {
        o.handle(Input::Char(c));
    }
    o.paint();
}
pub fn click(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    mv(o, x, y);
    down(o, x, y);
    let out = up(o, x, y);
    o.paint();
    out
}

pub fn save_png(name: &str, w: u32, h: u32, bgra: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../snapshots-out");
    std::fs::create_dir_all(&dir).unwrap();
    let img = Image::from_bgra(w, h, bgra.to_vec());
    let path = dir.join(name);
    std::fs::write(&path, img.encode_png().unwrap()).unwrap();
    println!("wrote {}", path.canonicalize().unwrap().display());
}

pub fn save_overlay(name: &str, o: &mut Overlay) {
    o.paint();
    let (w, h) = o.size();
    save_png(name, w, h, o.buffer());
}

pub fn save_crop(name: &str, o: &Overlay, r: Rect) {
    let (w, h) = o.size();
    let img = Image::from_bgra(w, h, o.buffer().to_vec()).crop(r);
    save_png(name, img.width, img.height, &img.data);
}

pub fn unused() {
    let _ = image_to_pixmap;
}
