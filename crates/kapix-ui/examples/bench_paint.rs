//! Times `handle(MouseMove) + paint()` at 3840×2160. Run with `--release`.

use std::time::Instant;

use kapix_core::config::DefaultAction;
use kapix_core::{Image, Monitor, Rect, VirtualDesktop};
use kapix_ui::{Font, Input, Key, Mods, MouseButton, Overlay, OverlayInit, OverlayMode, Palette};

const W: u32 = 3840;
const H: u32 = 2160;

fn overlay(font: Font, palette: Palette) -> Overlay {
    let mut data = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            data.extend_from_slice(&[(x / 8) as u8, (y / 6) as u8, ((x ^ y) & 0xFF) as u8, 255]);
        }
    }
    let r = Rect::new(0, 0, W as i32, H as i32);
    Overlay::new(OverlayInit {
        frame: Image::from_bgra(W, H, data),
        desktop: VirtualDesktop {
            bounds: r,
            monitors: vec![Monitor {
                rect: r,
                work: r,
                scale: 1.5,
                primary: true,
            }],
        },
        windows: vec![Rect::new(200, 200, 1800, 1200)],
        mode: OverlayMode::Region,
        default_action: DefaultAction::Annotate,
        show_magnifier: true,
        window_snap: true,
        font,
        palette,
        cursor: (100, 100),
    })
}

const M: Mods = Mods {
    ctrl: false,
    shift: false,
    alt: false,
};

fn stats(name: &str, mut t: Vec<f64>) {
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = t[t.len() / 2];
    let p95 = t[(t.len() as f64 * 0.95) as usize];
    println!(
        "{name:<44} median {med:6.3} ms   p95 {p95:6.3} ms   max {:6.3} ms",
        t[t.len() - 1]
    );
}

fn run(o: &mut Overlay, n: usize, mut pos: impl FnMut(usize) -> (i32, i32)) -> Vec<f64> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y) = pos(i);
        let t = Instant::now();
        o.handle(Input::MouseMove { x, y, mods: M });
        o.paint();
        out.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    out
}

fn wobble(i: usize, cx: i32, cy: i32, r: i32) -> (i32, i32) {
    let a = i as f32 * 0.07;
    (
        cx + (a.cos() * r as f32) as i32,
        cy + (a.sin() * r as f32 * 0.6) as i32,
    )
}

fn main() {
    // Archivo (embedded) + DejaVu as the fallback when present; the theme comes from argv.
    let font = Font::new(std::fs::read("/usr/share/fonts/TTF/DejaVuSans.ttf").ok());
    let light = std::env::args().any(|a| a == "--light");
    let palette = if light { Palette::LIGHT } else { Palette::DARK };
    println!(
        "frame {W}x{H}, theme: {}",
        if light { "light" } else { "dark" }
    );

    let mut o = overlay(font.clone(), palette);
    let t = Instant::now();
    o.paint();
    println!(
        "first (full) paint: {:.2} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );

    stats(
        "Selecting: hover + loupe + snap",
        run(&mut o, 1500, |i| wobble(i, 1900, 1000, 900)),
    );

    let mut o = overlay(font.clone(), palette);
    o.paint();
    o.handle(Input::MouseMove {
        x: 300,
        y: 200,
        mods: M,
    });
    o.handle(Input::MouseDown {
        x: 300,
        y: 200,
        button: MouseButton::Left,
        mods: M,
    });
    stats(
        "Selecting: dragging a large selection",
        run(&mut o, 1500, |i| wobble(i, 2900, 1700, 400)),
    );
    o.handle(Input::MouseUp {
        x: 2900,
        y: 1700,
        button: MouseButton::Left,
        mods: M,
    });

    // Annotating state with a few annotations.
    stats(
        "Annotating: hover, no tool (handles)",
        run(&mut o, 1500, |i| wobble(i, 1500, 1000, 700)),
    );
    o.handle(Input::KeyDown {
        key: Key::Char('R'),
        mods: M,
    });
    stats(
        "Annotating: hover, rect tool (size ring)",
        run(&mut o, 1500, |i| wobble(i, 1500, 1000, 700)),
    );
    o.handle(Input::MouseDown {
        x: 800,
        y: 600,
        button: MouseButton::Left,
        mods: M,
    });
    stats(
        "Annotating: dragging a rectangle",
        run(&mut o, 1500, |i| wobble(i, 1500, 1000, 600)),
    );
    o.handle(Input::MouseUp {
        x: 1500,
        y: 1000,
        button: MouseButton::Left,
        mods: M,
    });
    o.handle(Input::KeyDown {
        key: Key::Char('R'),
        mods: M,
    });
    o.handle(Input::KeyDown {
        key: Key::Char('B'),
        mods: M,
    });
    o.handle(Input::MouseDown {
        x: 900,
        y: 700,
        button: MouseButton::Left,
        mods: M,
    });
    stats(
        "Annotating: dragging a pixelate region",
        run(&mut o, 600, |i| wobble(i, 1500, 1000, 500)),
    );
    o.handle(Input::MouseUp {
        x: 1500,
        y: 1000,
        button: MouseButton::Left,
        mods: M,
    });
    o.handle(Input::KeyDown {
        key: Key::Char('B'),
        mods: M,
    });
    o.handle(Input::KeyDown {
        key: Key::Char('P'),
        mods: M,
    });
    o.handle(Input::MouseDown {
        x: 1000,
        y: 800,
        button: MouseButton::Left,
        mods: M,
    });
    stats(
        "Annotating: drawing a pen stroke",
        run(&mut o, 1500, |i| wobble(i, 1500, 1000, 500)),
    );
    o.handle(Input::MouseUp {
        x: 1500,
        y: 1000,
        button: MouseButton::Left,
        mods: M,
    });
    o.handle(Input::KeyDown {
        key: Key::Char('P'),
        mods: M,
    });
    o.handle(Input::MouseMove {
        x: 1500,
        y: 1000,
        mods: M,
    });
    o.handle(Input::MouseDown {
        x: 1500,
        y: 1000,
        button: MouseButton::Left,
        mods: M,
    });
    stats(
        "Annotating: moving the selection",
        run(&mut o, 1500, |i| wobble(i, 1500, 1000, 300)),
    );
}
