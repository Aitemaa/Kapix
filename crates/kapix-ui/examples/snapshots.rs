//! Renders scripted overlay states to PNGs in `snapshots-out/` for visual review.
//! Run: `cargo run -p kapix-ui --example snapshots`

mod common;

use common::*;
use kapix_core::config::DefaultAction;
use kapix_core::{Image, Rect};
use kapix_ui::toast::{self, Toast, ToastKind};
use kapix_ui::{Font, Input, Key, Mods, MouseButton, Outcome, Overlay, OverlayMode};

/// Script: select a region on the left monitor and use every tool once.
fn annotated(scene: &Scene, font: &Option<Font>) -> Overlay {
    let mut o = new_overlay(
        scene,
        font.clone(),
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

fn main() {
    let font = load_font();
    if font.is_none() {
        eprintln!("warning: no system font found, text will be missing");
    }
    let scene = scene(font.as_ref());
    save_png("00_desktop.png", W, H, &scene.frame.data);

    // 1. Selecting with loupe + window snap (right monitor, 150% scale; negative virtual coords on left).
    let mut o = new_overlay(
        &scene,
        font.clone(),
        (1700, 640),
        DefaultAction::Annotate,
        OverlayMode::Region,
    );
    mv(&mut o, 1712, 650);
    save_overlay("01_selecting_snap_loupe.png", &mut o);
    save_crop(
        "01_selecting_snap_loupe_crop.png",
        &o,
        Rect::new(1380, 480, 900, 520),
    );
    let mut o = new_overlay(
        &scene,
        font.clone(),
        (600, 400),
        DefaultAction::Annotate,
        OverlayMode::Region,
    );
    mv(&mut o, 610, 402);
    save_overlay("01b_selecting_snap_left.png", &mut o);
    save_crop(
        "01b_selecting_snap_left_crop.png",
        &o,
        Rect::new(380, 250, 700, 460),
    );

    // 2. Mid-drag freeform selection with the size label.
    let mut o = new_overlay(
        &scene,
        font.clone(),
        (300, 300),
        DefaultAction::Annotate,
        OverlayMode::Region,
    );
    mv(&mut o, 300, 250);
    down(&mut o, 300, 250);
    for i in 1..=10 {
        mv(&mut o, 300 + i * 52, 250 + i * 31);
    }
    save_overlay("02_dragging_freeform.png", &mut o);
    save_crop(
        "02_dragging_freeform_crop.png",
        &o,
        Rect::new(260, 200, 900, 520),
    );

    // 3. Annotating with every tool.
    let mut o = annotated(&scene, &font);
    let bar = o.toolbar_rect().expect("toolbar");
    mv(&mut o, bar.x + 6 + 2 * 30 + 14, bar.y + 20); // hover the pen button
    save_overlay("03_annotating_all_tools.png", &mut o);
    save_crop(
        "03_annotating_all_tools_crop.png",
        &o,
        Rect::new(60, 80, 1160, 1120),
    );
    save_crop("03_toolbar_crop.png", &o, bar.inflate(14));

    // 3b. The same on the 150% monitor.
    let mut o2 = new_overlay(
        &scene,
        font.clone(),
        (1500, 300),
        DefaultAction::Annotate,
        OverlayMode::Region,
    );
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
    save_overlay("03b_annotating_hidpi.png", &mut o2);
    save_crop(
        "03b_toolbar_hidpi_crop.png",
        &o2,
        o2.toolbar_rect().unwrap().inflate(20),
    );
    save_crop(
        "03b_annotating_hidpi_crop.png",
        &o2,
        Rect::new(1380, 160, 1100, 860),
    );

    // 3c. Handles + resize loupe while adjusting the selection.
    let mut o3 = new_overlay(
        &scene,
        font.clone(),
        (300, 300),
        DefaultAction::Annotate,
        OverlayMode::Region,
    );
    drag(&mut o3, (200, 300), (900, 700));
    mv(&mut o3, 900, 700);
    down(&mut o3, 900, 700);
    mv(&mut o3, 960, 760);
    save_crop(
        "03c_resizing_crop.png",
        &{
            o3.paint();
            o3
        },
        Rect::new(150, 250, 1000, 700),
    );

    // 4. Text editing with caret.
    let mut o = annotated(&scene, &font);
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
    type_text(&mut o, "growth column");
    save_overlay("04_text_editing.png", &mut o);
    save_crop(
        "04_text_editing_crop.png",
        &o,
        Rect::new(400, 480, 600, 220),
    );

    // 5. Flattened export of (3).
    let mut o = annotated(&scene, &font);
    match o.handle(Input::KeyDown {
        key: Key::Enter,
        mods: Mods::default(),
    }) {
        Outcome::Copy(img) => save_png("05_flattened_export.png", img.width, img.height, &img.data),
        other => panic!("expected Copy, got {other:?}"),
    }

    // 6. Toasts on a wallpaper-ish backdrop.
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
    ];
    let mut imgs: Vec<Image> = Vec::new();
    for (i, (k, t, b)) in toasts.iter().enumerate() {
        let scale = if i == 2 { 1.5 } else { 1.0 };
        imgs.push(toast::render(
            &Toast {
                kind: *k,
                title: (*t).into(),
                body: (*b).into(),
            },
            font.as_ref(),
            scale,
        ));
    }
    let (bw, bh) = (700u32, 420u32);
    let mut bg = scene.frame.crop(Rect::new(300, 300, bw as i32, bh as i32));
    let mut y = 24usize;
    for img in &imgs {
        let x = 40usize;
        for row in 0..img.height as usize {
            for col in 0..img.width as usize {
                let s = &img.data[(row * img.width as usize + col) * 4..][..4];
                let d = &mut bg.data[((y + row) * bw as usize + x + col) * 4..][..4];
                let inv = 255 - s[3] as u32;
                for c in 0..3 {
                    d[c] = (s[c] as u32 + d[c] as u32 * inv / 255).min(255) as u8;
                }
            }
        }
        y += img.height as usize + 20;
    }
    save_png("06_toasts.png", bw, bh, &bg.data);
    let _ = (MouseButton::Left,);
}
