//! Renders scripted overlay states to PNGs in `snapshots-out/` for visual review.
//! Every scenario is rendered in both themes (`_dark` / `_light` suffix).
//! Run: `cargo run --release -p kapix-ui --example snapshots`

mod common;

use common::*;
use kapix_core::config::DefaultAction;
use kapix_core::{Image, Rect};
use kapix_ui::brand::render_app_icon;
use kapix_ui::toast::{self, Toast, ToastKind};
use kapix_ui::{Font, Input, Key, Mods, Outcome, Overlay, OverlayMode, Palette};

/// Script: select a region on the left monitor and use every tool once.
fn annotated(scene: &Scene, font: &Font, pal: Palette) -> Overlay {
    let mut o = new_overlay(
        scene,
        font.clone(),
        pal,
        (300, 300),
        DefaultAction::Annotate,
        OverlayMode::Region,
    );
    drag(&mut o, (100, 120), (1180, 1120));
    assert!(o.is_annotating());

    // Arrow (red) pointing at the highlighted table row.
    key(&mut o, 'A');
    drag(&mut o, (990, 390), (846, 452));
    // Rectangle (amber) around the row.
    key(&mut o, 'A'); // deselect arrow
    key(&mut o, 'R');
    key(&mut o, '2');
    drag(&mut o, (136, 434), (836, 484));
    // Pen (emerald): a hand-drawn underline below the heading.
    key(&mut o, 'R');
    key(&mut o, 'P');
    key(&mut o, '3');
    {
        mv(&mut o, 130, 212);
        down(&mut o, 130, 212);
        for i in 0..=40 {
            let x = 130 + i * 6;
            let y = 212 + ((i as f32 * 0.55).sin() * 4.0) as i32;
            mv(&mut o, x, y);
        }
        up(&mut o, 370, 212);
    }
    // Highlighter (amber) over a sentence.
    key(&mut o, 'P');
    key(&mut o, 'H');
    key(&mut o, '2');
    drag(&mut o, (130, 230), (640, 230));
    // Text with backing pill (white text on ink pill).
    key(&mut o, 'H');
    key(&mut o, 'T');
    key(&mut o, '5');
    click(&mut o, 790, 168);
    type_text(&mut o, "Growth is accelerating");
    o.handle(Input::KeyDown {
        key: Key::Enter,
        mods: Mods::default(),
    });
    // Pixelate over the secret token.
    key(&mut o, 'T');
    key(&mut o, 'B');
    let s = scene.secret;
    drag(&mut o, (s.x, s.y), (s.right(), s.bottom()));
    // Steps 1-3 (red).
    key(&mut o, 'B');
    key(&mut o, 'N');
    key(&mut o, '1');
    click(&mut o, 118, 176);
    click(&mut o, 118, 330);
    click(&mut o, 118, 1012);
    key(&mut o, 'N'); // deselect -> handles visible again
    o
}

/// Source-over a premultiplied BGRA `img` onto the opaque `bg` at `(x, y)`.
fn composite(bg: &mut Image, img: &Image, x: usize, y: usize) {
    let bw = bg.width as usize;
    for row in 0..img.height as usize {
        for col in 0..img.width as usize {
            let s = &img.data[(row * img.width as usize + col) * 4..][..4];
            let d = &mut bg.data[((y + row) * bw + x + col) * 4..][..4];
            let inv = 255 - s[3] as u32;
            for c in 0..3 {
                d[c] = (s[c] as u32 + (d[c] as u32 * inv + 127) / 255).min(255) as u8;
            }
        }
    }
}

fn solid(w: u32, h: u32, c: [u8; 3]) -> Image {
    Image::from_bgra(
        w,
        h,
        [c[2], c[1], c[0], 255].repeat(w as usize * h as usize),
    )
}

fn brand_icons() {
    let sizes = [16u32, 24, 32, 48, 256];
    let (w, h) = (
        16 + sizes.iter().map(|s| s + 16).sum::<u32>(),
        2 * (256 + 32),
    );
    let mut canvas = solid(w, h, [0xEF, 0xEE, 0xE9]);
    let dark = solid(w, h / 2, [0x14, 0x14, 0x14]);
    for row in 0..(h / 2) as usize {
        let o = (h as usize / 2 + row) * w as usize * 4;
        canvas.data[o..o + w as usize * 4]
            .copy_from_slice(&dark.data[row * w as usize * 4..(row + 1) * w as usize * 4]);
    }
    for band in 0..2usize {
        let mut x = 16usize;
        for &n in &sizes {
            let img = render_app_icon(n);
            let y = band * (h as usize / 2) + 16 + (256 - n as usize) / 2;
            composite(&mut canvas, &img, x, y);
            x += n as usize + 16;
        }
    }
    save_png("07_brand_icons.png", w, h, &canvas.data);
    // The same sizes magnified 8x (nearest) to judge the small renditions.
    let scale = 8usize;
    let small = [16u32, 24, 32];
    let mw = small
        .iter()
        .map(|s| *s as usize * scale + 16)
        .sum::<usize>()
        + 16;
    let mh = 32 * scale + 32;
    let mut big = solid(mw as u32, mh as u32, [0x1B, 0x1B, 0x1B]);
    let mut x = 16usize;
    for &n in &small {
        let img = render_app_icon(n);
        let mut up = Image::from_bgra(
            n * scale as u32,
            n * scale as u32,
            vec![0; (n as usize * scale).pow(2) * 4],
        );
        for yy in 0..n as usize * scale {
            for xx in 0..n as usize * scale {
                let s = img.pixel((xx / scale) as u32, (yy / scale) as u32);
                up.data[(yy * n as usize * scale + xx) * 4..][..4].copy_from_slice(&s);
            }
        }
        composite(&mut big, &up, x, 16);
        x += n as usize * scale + 16;
    }
    save_png(
        "07b_brand_icons_magnified.png",
        mw as u32,
        mh as u32,
        &big.data,
    );
}

fn scenarios(scene: &Scene, font: &Font, pal: Palette, suffix: &str) {
    let n = |base: &str| format!("{base}_{suffix}.png");
    let new = |cursor: (i32, i32)| {
        new_overlay(
            scene,
            font.clone(),
            pal,
            cursor,
            DefaultAction::Annotate,
            OverlayMode::Region,
        )
    };

    // 1. Selecting with loupe + window snap (right monitor, 150% scale; negative virtual coords on left).
    let mut o = new((1700, 640));
    mv(&mut o, 1712, 650);
    save_overlay(&n("01_selecting_snap_loupe"), &mut o);
    save_crop(
        &n("01_selecting_snap_loupe_crop"),
        &o,
        Rect::new(1380, 480, 900, 520),
    );
    let mut o = new((600, 400));
    mv(&mut o, 610, 402);
    save_overlay(&n("01b_selecting_snap_left"), &mut o);
    save_crop(
        &n("01b_selecting_snap_left_crop"),
        &o,
        Rect::new(380, 250, 700, 460),
    );

    // 2. Mid-drag freeform selection with the size label.
    let mut o = new((300, 300));
    mv(&mut o, 300, 250);
    down(&mut o, 300, 250);
    for i in 1..=10 {
        mv(&mut o, 300 + i * 52, 250 + i * 31);
    }
    save_overlay(&n("02_dragging_freeform"), &mut o);
    save_crop(
        &n("02_dragging_freeform_crop"),
        &o,
        Rect::new(260, 200, 900, 520),
    );

    // 3. Annotating with every tool.
    let mut o = annotated(scene, font, pal);
    let bar = o.toolbar_rect().expect("toolbar");
    mv(&mut o, bar.x + 6 + 2 * 34 + 16, bar.y + 22); // hover the pen button
    save_overlay(&n("03_annotating_all_tools"), &mut o);
    save_crop(
        &n("03_annotating_all_tools_crop"),
        &o,
        Rect::new(60, 80, 1160, 1120),
    );
    save_crop(&n("03_toolbar_crop"), &o, bar.inflate(40));
    // Active tool (lit pill) + rectangle fill toggled + undo available.
    key(&mut o, 'R');
    key(&mut o, 'F');
    mv(&mut o, bar.x + bar.w - 20, bar.y + 22); // hover Close (danger tint)
    save_crop(&n("03d_toolbar_active_rect_crop"), &o, bar.inflate(40));
    mv(&mut o, bar.x + bar.w - 56, bar.y + 22); // hover Copy (primary)
    save_crop(&n("03e_toolbar_hover_copy_crop"), &o, bar.inflate(40));

    // 3b. The same on the 150% monitor.
    let mut o2 = new((1500, 300));
    drag(&mut o2, (1420, 220), (2380, 880));
    key(&mut o2, 'R');
    key(&mut o2, '4');
    drag(&mut o2, (1470, 300), (2000, 420));
    key(&mut o2, 'R');
    key(&mut o2, 'A');
    key(&mut o2, '1');
    drag(&mut o2, (2300, 700), (2100, 480));
    key(&mut o2, 'A');
    mv(&mut o2, 1900, 920);
    save_overlay(&n("03b_annotating_hidpi"), &mut o2);
    save_crop(
        &n("03b_toolbar_hidpi_crop"),
        &o2,
        o2.toolbar_rect().unwrap().inflate(60),
    );
    save_crop(
        &n("03b_annotating_hidpi_crop"),
        &o2,
        Rect::new(1380, 160, 1100, 860),
    );

    // 3c. Handles + resize loupe while adjusting the selection.
    let mut o3 = new((300, 300));
    drag(&mut o3, (200, 300), (900, 700));
    mv(&mut o3, 900, 700);
    down(&mut o3, 900, 700);
    mv(&mut o3, 960, 760);
    o3.paint();
    save_crop(&n("03c_resizing_crop"), &o3, Rect::new(150, 250, 1000, 700));

    // 4. Text editing with caret (includes non-Latin text to exercise the fallback face).
    let mut o = annotated(scene, font, pal);
    key(&mut o, 'T');
    key(&mut o, '4');
    click(&mut o, 560, 560);
    type_text(&mut o, "Check the");
    o.handle(Input::KeyDown {
        key: Key::Enter,
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
    });
    type_text(&mut o, "growth колонка");
    save_overlay(&n("04_text_editing"), &mut o);
    save_crop(
        &n("04_text_editing_crop"),
        &o,
        Rect::new(400, 480, 600, 220),
    );

    // 5. Flattened export of (3): theme independent.
    if suffix == "dark" {
        let mut o = annotated(scene, font, pal);
        match o.handle(Input::KeyDown {
            key: Key::Enter,
            mods: Mods::default(),
        }) {
            Outcome::Copy(img) => {
                save_png("05_flattened_export.png", img.width, img.height, &img.data)
            }
            other => panic!("expected Copy, got {other:?}"),
        }
    }

    // 6. Toasts on a wallpaper-ish backdrop (shadow margins are transparent).
    let toasts = [
        (
            ToastKind::Success,
            "Copied to clipboard",
            "1080 × 1000 · 412 KB",
        ),
        (
            ToastKind::Info,
            "Text copied",
            "$ export API_TOKEN=sk_live_51NxQ7mRk0aZy3PqWd9 · 3 lines, 117 characters, recognized with en-US",
        ),
        (
            ToastKind::Error,
            "No OCR language pack",
            "Install a Windows language pack in Settings > Time & language.",
        ),
        (ToastKind::Success, "Saved", "Kapix_2026-11-23_14-02-11.png"),
    ];
    let (bw, bh) = (760u32, 560u32);
    let mut bg = scene.frame.crop(Rect::new(300, 300, bw as i32, bh as i32));
    let mut y = 8usize;
    for (i, (k, t, b)) in toasts.iter().enumerate() {
        let scale = if i == 2 { 1.5 } else { 1.0 };
        let r = toast::render(
            &Toast {
                kind: *k,
                title: (*t).into(),
                body: (*b).into(),
            },
            font,
            &pal,
            scale,
        );
        composite(&mut bg, &r.image, 20, y);
        y += r.image.height as usize - 8;
    }
    save_png(&n("06_toasts"), bw, bh, &bg.data);
}

fn main() {
    let font = load_font();
    let scene = scene(Some(&Font::builtin()));
    save_png("00_desktop.png", W, H, &scene.frame.data);
    scenarios(&scene, &font, Palette::DARK, "dark");
    scenarios(&scene, &font, Palette::LIGHT, "light");
    brand_icons();
}
