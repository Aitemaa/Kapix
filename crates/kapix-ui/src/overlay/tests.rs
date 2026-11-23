use super::*;
use crate::font::test_font;
use kapix_core::{Monitor, Point};

const FW: u32 = 640;
const FH: u32 = 400;

fn frame() -> Image {
    let mut data = Vec::with_capacity((FW * FH * 4) as usize);
    for y in 0..FH {
        for x in 0..FW {
            data.extend_from_slice(&[(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255]);
        }
    }
    Image::from_bgra(FW, FH, data)
}

fn desktop() -> VirtualDesktop {
    let r = Rect::new(-100, 50, FW as i32, FH as i32);
    VirtualDesktop {
        bounds: r,
        monitors: vec![Monitor {
            rect: r,
            work: r,
            scale: 1.0,
            primary: true,
        }],
    }
}

fn init(action: DefaultAction, mode: OverlayMode, font: Option<Font>) -> OverlayInit {
    OverlayInit {
        frame: frame(),
        desktop: desktop(),
        windows: vec![Rect::new(300, 100, 200, 150), Rect::new(20, 20, 580, 360)],
        mode,
        default_action: action,
        show_magnifier: true,
        window_snap: true,
        font,
        cursor: (10, 10),
    }
}

fn overlay() -> Overlay {
    Overlay::new(init(
        DefaultAction::Annotate,
        OverlayMode::Region,
        test_font::load(),
    ))
}

const M: Mods = Mods {
    ctrl: false,
    shift: false,
    alt: false,
};

fn mv(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    o.handle(Input::MouseMove { x, y, mods: M })
}
fn down(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    o.handle(Input::MouseDown {
        x,
        y,
        button: MouseButton::Left,
        mods: M,
    })
}
fn up_m(o: &mut Overlay, x: i32, y: i32, mods: Mods) -> Outcome {
    o.handle(Input::MouseUp {
        x,
        y,
        button: MouseButton::Left,
        mods,
    })
}
fn up(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    up_m(o, x, y, M)
}
fn drag(o: &mut Overlay, a: (i32, i32), b: (i32, i32)) -> Outcome {
    mv(o, a.0, a.1);
    down(o, a.0, a.1);
    for i in 1..=5 {
        mv(o, a.0 + (b.0 - a.0) * i / 5, a.1 + (b.1 - a.1) * i / 5);
    }
    let out = up(o, b.0, b.1);
    o.paint();
    out
}
fn key(o: &mut Overlay, k: Key) -> Outcome {
    o.handle(Input::KeyDown { key: k, mods: M })
}
fn ch(o: &mut Overlay, c: char) -> Outcome {
    key(o, Key::Char(c))
}
fn ctrl(o: &mut Overlay, c: char) -> Outcome {
    o.handle(Input::KeyDown {
        key: Key::Char(c),
        mods: Mods { ctrl: true, ..M },
    })
}
fn click(o: &mut Overlay, x: i32, y: i32) -> Outcome {
    mv(o, x, y);
    down(o, x, y);
    up(o, x, y)
}
fn annotating() -> Overlay {
    let mut o = overlay();
    assert_eq!(drag(&mut o, (100, 100), (300, 250)), Outcome::Redraw);
    assert!(o.is_annotating());
    o
}
fn button_center(o: &Overlay, b: Btn) -> Point {
    let l = o.toolbar_layout().expect("toolbar visible");
    l.items
        .iter()
        .find(|(_, x)| *x == b)
        .map(|(r, _)| r.center())
        .unwrap()
}

#[test]
fn drag_creates_selection_and_enters_annotating() {
    let mut o = overlay();
    let out = drag(&mut o, (100, 100), (300, 250));
    assert_eq!(out, Outcome::Redraw);
    assert!(o.is_annotating());
    assert_eq!(o.selection(), Some(Rect::new(100, 100, 200, 150)));
    // Reverse direction normalizes.
    let mut o = overlay();
    drag(&mut o, (300, 250), (100, 100));
    assert_eq!(o.selection(), Some(Rect::new(100, 100, 200, 150)));
}

#[test]
fn selection_is_clamped_to_frame() {
    let mut o = overlay();
    drag(&mut o, (500, 300), (900, 700));
    assert_eq!(
        o.selection(),
        Some(Rect::new(500, 300, FW as i32 - 500, FH as i32 - 300))
    );
}

#[test]
fn zero_area_drag_is_ignored() {
    let mut o = Overlay::new(OverlayInit {
        windows: vec![],
        ..init(DefaultAction::Annotate, OverlayMode::Region, None)
    });
    // Press/release without movement.
    assert!(!click(&mut o, 120, 120).is_terminal());
    assert!(!o.is_annotating());
    assert_eq!(o.selection(), None);
    // A long but flat drag has zero height.
    drag(&mut o, (50, 60), (200, 60));
    assert!(!o.is_annotating());
    assert_eq!(o.selection(), None);
}

#[test]
fn instant_copy_returns_exact_raw_crop() {
    let mut o = Overlay::new(init(DefaultAction::InstantCopy, OverlayMode::Region, None));
    match drag(&mut o, (100, 100), (300, 250)) {
        Outcome::Copy(img) => {
            assert_eq!((img.width, img.height), (200, 150));
            assert_eq!(
                img.crop(Rect::new(0, 0, 1, 1)).data,
                frame().crop(Rect::new(100, 100, 1, 1)).data
            );
            assert_eq!(img.data, frame().crop(Rect::new(100, 100, 200, 150)).data);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn alt_on_release_enters_annotating_in_instant_mode() {
    let mut o = Overlay::new(init(DefaultAction::InstantCopy, OverlayMode::Region, None));
    mv(&mut o, 100, 100);
    down(&mut o, 100, 100);
    mv(&mut o, 200, 200);
    let out = up_m(&mut o, 300, 250, Mods { alt: true, ..M });
    assert_eq!(out, Outcome::Redraw);
    assert!(o.is_annotating());
}

#[test]
fn ocr_mode_returns_raw_crop() {
    let mut o = Overlay::new(init(DefaultAction::Annotate, OverlayMode::Ocr, None));
    match drag(&mut o, (50, 60), (150, 120)) {
        Outcome::Ocr(img) => {
            assert_eq!((img.width, img.height), (100, 60));
            assert_eq!(img.data, frame().crop(Rect::new(50, 60, 100, 60)).data);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn snap_click_selects_topmost_window_rect() {
    let mut o = overlay();
    mv(&mut o, 350, 150); // inside both windows; the first (top-most) wins
    assert_eq!(o.snap, Some(Rect::new(300, 100, 200, 150)));
    assert_eq!(o.bright_rect(), Some(Rect::new(300, 100, 200, 150)));
    down(&mut o, 350, 150);
    mv(&mut o, 352, 151); // under the click slop
    let out = up(&mut o, 352, 151);
    assert_eq!(out, Outcome::Redraw);
    assert!(o.is_annotating());
    assert_eq!(o.selection(), Some(Rect::new(300, 100, 200, 150)));
}

#[test]
fn drag_beyond_slop_ignores_snap_and_is_freeform() {
    let mut o = overlay();
    drag(&mut o, (350, 150), (400, 200));
    assert_eq!(o.selection(), Some(Rect::new(350, 150, 50, 50)));
}

#[test]
fn snap_disabled_click_does_nothing() {
    let mut o = Overlay::new(OverlayInit {
        window_snap: false,
        ..init(DefaultAction::Annotate, OverlayMode::Region, None)
    });
    click(&mut o, 350, 150);
    assert!(!o.is_annotating());
}

#[test]
fn escape_and_right_click_cancel() {
    let mut o = overlay();
    assert_eq!(key(&mut o, Key::Escape), Outcome::Cancel);
    let mut o = overlay();
    assert_eq!(
        o.handle(Input::MouseUp {
            x: 1,
            y: 1,
            button: MouseButton::Right,
            mods: M
        }),
        Outcome::Cancel
    );
    let mut o = annotating();
    assert_eq!(key(&mut o, Key::Escape), Outcome::Cancel);
    let mut o = annotating();
    assert_eq!(
        o.handle(Input::MouseUp {
            x: 1,
            y: 1,
            button: MouseButton::Right,
            mods: M
        }),
        Outcome::Cancel
    );
    assert!(Outcome::Cancel.is_terminal());
}

#[test]
fn c_copies_hex_of_pixel_under_cursor() {
    let mut o = overlay();
    mv(&mut o, 33, 44);
    let expect = Rgba::from_bgra(frame().pixel(33, 44)).hex();
    assert_eq!(ch(&mut o, 'C'), Outcome::CopyText(expect));
}

#[test]
fn enter_and_ctrl_c_copy_flattened_selection() {
    let mut o = annotating();
    match key(&mut o, Key::Enter) {
        Outcome::Copy(img) => assert_eq!((img.width, img.height), (200, 150)),
        other => panic!("{other:?}"),
    }
    let mut o = annotating();
    assert!(matches!(ctrl(&mut o, 'C'), Outcome::Copy(_)));
    let mut o = annotating();
    assert!(matches!(ctrl(&mut o, 'S'), Outcome::Save(_)));
    let mut o = annotating();
    match ctrl(&mut o, 'P') {
        // Buffer (100,100) is virtual (0,150) with this desktop.
        Outcome::Pin { image, at } => {
            assert_eq!(at, Rect::new(0, 150, 200, 150));
            assert_eq!((image.width, image.height), (200, 150));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn o_key_returns_raw_crop_without_annotations() {
    let mut o = annotating();
    ch(&mut o, 'R');
    drag(&mut o, (120, 120), (200, 200));
    assert_eq!(o.document().items().len(), 1);
    match ch(&mut o, 'O') {
        Outcome::Ocr(img) => assert_eq!(img.data, frame().crop(Rect::new(100, 100, 200, 150)).data),
        other => panic!("{other:?}"),
    }
}

#[test]
fn tool_keys_toggle_and_color_keys_select() {
    let mut o = annotating();
    ch(&mut o, 'A');
    assert_eq!(o.tool(), Some(Tool::Arrow));
    ch(&mut o, 'A');
    assert_eq!(o.tool(), None);
    for (k, t) in [
        ('R', Tool::Rect),
        ('P', Tool::Pen),
        ('H', Tool::Highlighter),
        ('T', Tool::Text),
        ('B', Tool::Pixelate),
        ('N', Tool::Step),
    ] {
        ch(&mut o, k);
        assert_eq!(o.tool(), Some(t));
    }
    ch(&mut o, '4');
    assert_eq!(o.style.color, PRESETS[3]);
    ch(&mut o, '6');
    assert_eq!(o.style.color, PRESETS[5]);
}

#[test]
fn drawing_tools_commit_annotations() {
    let mut o = annotating();
    ch(&mut o, 'A');
    drag(&mut o, (120, 120), (220, 180));
    ch(&mut o, 'A');
    ch(&mut o, 'R');
    drag(&mut o, (130, 130), (180, 160));
    ch(&mut o, 'R');
    ch(&mut o, 'P');
    drag(&mut o, (130, 200), (250, 210));
    ch(&mut o, 'P');
    ch(&mut o, 'H');
    drag(&mut o, (130, 220), (250, 220));
    ch(&mut o, 'H');
    ch(&mut o, 'B');
    drag(&mut o, (140, 140), (190, 170));
    let kinds: Vec<_> = o
        .document()
        .items()
        .iter()
        .map(std::mem::discriminant)
        .collect();
    assert_eq!(kinds.len(), 5);
    assert!(matches!(o.document().items()[0], Annotation::Arrow { .. }));
    assert!(matches!(o.document().items()[1], Annotation::Rect { .. }));
    assert!(matches!(o.document().items()[2], Annotation::Pen { .. }));
    assert!(matches!(
        o.document().items()[3],
        Annotation::Highlighter { .. }
    ));
    assert!(matches!(
        o.document().items()[4],
        Annotation::Pixelate { .. }
    ));
}

#[test]
fn tiny_drags_do_not_commit() {
    let mut o = annotating();
    ch(&mut o, 'A');
    click(&mut o, 150, 150);
    ch(&mut o, 'A');
    ch(&mut o, 'R');
    drag(&mut o, (150, 150), (151, 151));
    assert!(o.document().is_empty());
}

#[test]
fn shift_snaps_arrow_and_squares_rect() {
    let mut o = annotating();
    ch(&mut o, 'A');
    mv(&mut o, 120, 120);
    down(&mut o, 120, 120);
    o.handle(Input::MouseMove {
        x: 220,
        y: 130,
        mods: Mods { shift: true, ..M },
    });
    o.handle(Input::MouseUp {
        x: 220,
        y: 130,
        button: MouseButton::Left,
        mods: Mods { shift: true, ..M },
    });
    match &o.document().items()[0] {
        Annotation::Arrow { to, .. } => {
            assert!((to.y - 120.0).abs() < 0.5, "snapped to horizontal: {to:?}")
        }
        other => panic!("{other:?}"),
    }
    ch(&mut o, 'A');
    ch(&mut o, 'R');
    mv(&mut o, 150, 150);
    down(&mut o, 150, 150);
    o.handle(Input::MouseMove {
        x: 210,
        y: 180,
        mods: Mods { shift: true, ..M },
    });
    o.handle(Input::MouseUp {
        x: 210,
        y: 180,
        button: MouseButton::Left,
        mods: Mods { shift: true, ..M },
    });
    match &o.document().items()[1] {
        Annotation::Rect { rect, .. } => assert_eq!((rect.w, rect.h), (60, 60)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn f_toggles_fill_for_rect() {
    let mut o = annotating();
    ch(&mut o, 'R');
    ch(&mut o, 'F');
    drag(&mut o, (130, 130), (180, 160));
    assert!(matches!(
        o.document().items()[0],
        Annotation::Rect { filled: true, .. }
    ));
}

#[test]
fn undo_redo_through_keys() {
    let mut o = annotating();
    ch(&mut o, 'R');
    drag(&mut o, (130, 130), (180, 160));
    drag(&mut o, (140, 140), (190, 190));
    assert_eq!(o.document().items().len(), 2);
    assert_eq!(ctrl(&mut o, 'Z'), Outcome::Redraw);
    assert_eq!(o.document().items().len(), 1);
    assert_eq!(ctrl(&mut o, 'Z'), Outcome::Redraw);
    assert!(o.document().is_empty());
    assert_eq!(ctrl(&mut o, 'Z'), Outcome::None);
    assert!(
        o.composed.is_none(),
        "composed layer released when nothing is left"
    );
    assert_eq!(ctrl(&mut o, 'Y'), Outcome::Redraw);
    assert_eq!(o.document().items().len(), 1);
    // Ctrl+Shift+Z also redoes.
    o.handle(Input::KeyDown {
        key: Key::Char('Z'),
        mods: Mods {
            ctrl: true,
            shift: true,
            alt: false,
        },
    });
    assert_eq!(o.document().items().len(), 2);
    // New annotation clears redo.
    ctrl(&mut o, 'Z');
    drag(&mut o, (150, 150), (170, 170));
    assert_eq!(ctrl(&mut o, 'Y'), Outcome::None);
}

#[test]
fn step_tool_numbers_increment() {
    let mut o = annotating();
    ch(&mut o, 'N');
    click(&mut o, 120, 120);
    click(&mut o, 150, 150);
    click(&mut o, 180, 180);
    let nums: Vec<u32> = o
        .document()
        .items()
        .iter()
        .filter_map(|a| {
            if let Annotation::Step { number, .. } = a {
                Some(*number)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(nums, vec![1, 2, 3]);
    ctrl(&mut o, 'Z');
    click(&mut o, 200, 200);
    assert!(matches!(
        o.document().items().last(),
        Some(Annotation::Step { number: 3, .. })
    ));
}

#[test]
fn arrow_keys_nudge_selection_with_clamping() {
    let mut o = annotating();
    key(&mut o, Key::Right);
    assert_eq!(o.selection().unwrap().x, 101);
    key(&mut o, Key::Down);
    assert_eq!(o.selection().unwrap().y, 101);
    o.handle(Input::KeyDown {
        key: Key::Left,
        mods: Mods { shift: true, ..M },
    });
    assert_eq!(o.selection().unwrap().x, 91);
    o.handle(Input::KeyDown {
        key: Key::Up,
        mods: Mods { shift: true, ..M },
    });
    assert_eq!(o.selection().unwrap().y, 91);
    for _ in 0..30 {
        o.handle(Input::KeyDown {
            key: Key::Up,
            mods: Mods { shift: true, ..M },
        });
        o.handle(Input::KeyDown {
            key: Key::Left,
            mods: Mods { shift: true, ..M },
        });
    }
    assert_eq!(o.selection(), Some(Rect::new(0, 0, 200, 150)));
    for _ in 0..100 {
        o.handle(Input::KeyDown {
            key: Key::Right,
            mods: Mods { shift: true, ..M },
        });
        o.handle(Input::KeyDown {
            key: Key::Down,
            mods: Mods { shift: true, ..M },
        });
    }
    assert_eq!(
        o.selection(),
        Some(Rect::new(FW as i32 - 200, FH as i32 - 150, 200, 150))
    );
}

#[test]
fn text_editing_commit_backspace_newline_cancel() {
    let mut o = annotating();
    ch(&mut o, 'T');
    assert!(!o.is_editing_text());
    click(&mut o, 150, 150);
    assert!(o.is_editing_text());
    // Letter keys type instead of switching tools while editing.
    ch(&mut o, 'A');
    assert_eq!(o.tool(), Some(Tool::Text));
    for c in "Hey".chars() {
        o.handle(Input::Char(c));
    }
    o.handle(Input::Char('\u{8}')); // control char ignored
    key(&mut o, Key::Backspace);
    o.handle(Input::KeyDown {
        key: Key::Enter,
        mods: Mods { shift: true, ..M },
    });
    for c in "ok".chars() {
        o.handle(Input::Char(c));
    }
    assert_eq!(key(&mut o, Key::Enter), Outcome::Redraw);
    assert!(!o.is_editing_text());
    match &o.document().items()[0] {
        Annotation::Text { text, .. } => assert_eq!(text, "He\nok"),
        other => panic!("{other:?}"),
    }
    // Esc cancels only the edit.
    click(&mut o, 150, 200);
    o.handle(Input::Char('x'));
    assert_eq!(key(&mut o, Key::Escape), Outcome::Redraw);
    assert!(!o.is_editing_text());
    assert_eq!(o.document().items().len(), 1);
    // Clicking elsewhere commits.
    click(&mut o, 150, 220);
    o.handle(Input::Char('z'));
    click(&mut o, 250, 120);
    assert_eq!(o.document().items().len(), 2);
    assert!(
        o.is_editing_text(),
        "text tool starts a fresh box at the click"
    );
    // Empty text is never committed.
    key(&mut o, Key::Enter);
    assert_eq!(o.document().items().len(), 2);
    // Right click cancels only the edit while editing.
    click(&mut o, 150, 140);
    o.handle(Input::Char('q'));
    assert_eq!(
        o.handle(Input::MouseUp {
            x: 1,
            y: 1,
            button: MouseButton::Right,
            mods: M
        }),
        Outcome::Redraw
    );
    assert!(!o.is_editing_text());
}

#[test]
fn copy_commits_open_text_edit_first() {
    let mut o = annotating();
    ch(&mut o, 'T');
    click(&mut o, 150, 150);
    o.handle(Input::Char('h'));
    match ctrl(&mut o, 'C') {
        Outcome::Copy(_) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn toolbar_copy_button_copies() {
    let mut o = annotating();
    o.paint();
    let c = button_center(&o, Btn::Copy);
    mv(&mut o, c.x, c.y);
    down(&mut o, c.x, c.y);
    match up(&mut o, c.x, c.y) {
        Outcome::Copy(img) => assert_eq!((img.width, img.height), (200, 150)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn toolbar_buttons_dispatch() {
    let mut o = annotating();
    let c = button_center(&o, Btn::Tool(Tool::Rect));
    click(&mut o, c.x, c.y);
    assert_eq!(o.tool(), Some(Tool::Rect));
    click(&mut o, c.x, c.y);
    assert_eq!(o.tool(), None);
    let c = button_center(&o, Btn::Color(2));
    click(&mut o, c.x, c.y);
    assert_eq!(o.style.color, PRESETS[2]);
    // Press on one button, release on another: nothing happens.
    let a = button_center(&o, Btn::Close);
    let b = button_center(&o, Btn::Undo);
    mv(&mut o, a.x, a.y);
    down(&mut o, a.x, a.y);
    mv(&mut o, b.x, b.y);
    assert_eq!(up(&mut o, b.x, b.y), Outcome::Redraw);
    let c = button_center(&o, Btn::Close);
    assert_eq!(click(&mut o, c.x, c.y), Outcome::Cancel);
    let mut o = annotating();
    let c = button_center(&o, Btn::Ocr);
    assert!(matches!(click(&mut o, c.x, c.y), Outcome::Ocr(_)));
    let mut o = annotating();
    let c = button_center(&o, Btn::Save);
    assert!(matches!(click(&mut o, c.x, c.y), Outcome::Save(_)));
    let mut o = annotating();
    let c = button_center(&o, Btn::Pin);
    assert!(matches!(click(&mut o, c.x, c.y), Outcome::Pin { .. }));
}

#[test]
fn toolbar_does_not_start_drawing_and_stays_on_monitor() {
    let mut o = annotating();
    ch(&mut o, 'R');
    let c = button_center(&o, Btn::Undo);
    drag(&mut o, (c.x, c.y), (c.x + 3, c.y + 3));
    assert!(o.document().is_empty());
    // Selection at the very bottom: toolbar flips above and remains inside the frame.
    let mut o = overlay();
    drag(&mut o, (200, 300), (400, 399));
    let r = o.toolbar_rect().unwrap();
    assert!(
        Rect::new(0, 0, FW as i32, FH as i32).contains_rect(&r),
        "{r:?}"
    );
    assert!(r.bottom() <= 300, "above the selection: {r:?}");
    // Tall selection: toolbar goes inside the bottom.
    let mut o = overlay();
    drag(&mut o, (20, 5), (620, 395));
    let r = o.toolbar_rect().unwrap();
    assert!(
        Rect::new(0, 0, FW as i32, FH as i32).contains_rect(&r),
        "{r:?}"
    );
    assert!(o.selection().unwrap().contains_rect(&r), "inside: {r:?}");
    // Narrow selection near the right edge is clamped horizontally.
    let mut o = overlay();
    drag(&mut o, (600, 100), (636, 150));
    let r = o.toolbar_rect().unwrap();
    assert!(r.right() <= FW as i32, "{r:?}");
}

#[test]
fn wheel_adjusts_tool_size_within_range() {
    let mut o = annotating();
    ch(&mut o, 'P');
    let (lo, hi) = Tool::Pen.size_range();
    let start = o.style.sizes[tool_idx(Tool::Pen)];
    o.handle(Input::Wheel {
        x: 150,
        y: 150,
        delta: 1.0,
        mods: M,
    });
    assert!(o.style.sizes[tool_idx(Tool::Pen)] > start);
    for _ in 0..200 {
        o.handle(Input::Wheel {
            x: 150,
            y: 150,
            delta: 1.0,
            mods: M,
        });
    }
    assert_eq!(o.style.sizes[tool_idx(Tool::Pen)], hi);
    for _ in 0..400 {
        o.handle(Input::Wheel {
            x: 150,
            y: 150,
            delta: -1.0,
            mods: M,
        });
    }
    assert_eq!(o.style.sizes[tool_idx(Tool::Pen)], lo);
    // No tool: ignored.
    ch(&mut o, 'P');
    assert_eq!(
        o.handle(Input::Wheel {
            x: 150,
            y: 150,
            delta: 1.0,
            mods: M
        }),
        Outcome::None
    );
}

#[test]
fn move_and_resize_selection() {
    let mut o = annotating();
    // Drag inside moves.
    drag(&mut o, (200, 175), (230, 195));
    assert_eq!(o.selection(), Some(Rect::new(130, 120, 200, 150)));
    // Drag the SE corner.
    let sel = o.selection().unwrap();
    drag(
        &mut o,
        (sel.right(), sel.bottom()),
        (sel.right() + 40, sel.bottom() + 10),
    );
    assert_eq!(o.selection(), Some(Rect::new(130, 120, 240, 160)));
    // Drag the W edge.
    let sel = o.selection().unwrap();
    drag(&mut o, (sel.x, sel.y + 80), (sel.x + 30, sel.y + 80));
    assert_eq!(o.selection(), Some(Rect::new(160, 120, 210, 160)));
    // Moving can't leave the frame.
    drag(&mut o, (250, 200), (-500, -500));
    assert_eq!(o.selection().unwrap().x, 0);
    assert_eq!(o.selection().unwrap().y, 0);
    // Drag outside starts a new selection.
    drag(&mut o, (500, 300), (600, 380));
    assert_eq!(o.selection(), Some(Rect::new(500, 300, 100, 80)));
    // A click outside (no drag) keeps the selection.
    click(&mut o, 400, 380);
    assert_eq!(o.selection(), Some(Rect::new(500, 300, 100, 80)));
}

#[test]
fn annotations_stay_with_screen_content_when_selection_moves() {
    let mut o = annotating();
    ch(&mut o, 'R');
    drag(&mut o, (120, 120), (180, 160));
    ch(&mut o, 'R');
    let before = o.document().items().to_vec();
    drag(&mut o, (250, 230), (260, 240));
    assert_eq!(o.document().items(), &before[..]);
}

#[test]
fn cursor_kinds() {
    let mut o = overlay();
    assert_eq!(o.cursor(), CursorKind::Cross);
    drag(&mut o, (100, 100), (300, 250));
    mv(&mut o, 200, 175);
    assert_eq!(o.cursor(), CursorKind::Move);
    mv(&mut o, 100, 100);
    assert_eq!(o.cursor(), CursorKind::SizeNWSE);
    mv(&mut o, 300, 100);
    assert_eq!(o.cursor(), CursorKind::SizeNESW);
    mv(&mut o, 200, 100);
    assert_eq!(o.cursor(), CursorKind::SizeNS);
    mv(&mut o, 300, 175);
    assert_eq!(o.cursor(), CursorKind::SizeWE);
    mv(&mut o, 50, 50);
    assert_eq!(o.cursor(), CursorKind::Cross);
    let c = button_center(&o, Btn::Copy);
    mv(&mut o, c.x, c.y);
    assert_eq!(o.cursor(), CursorKind::Hand);
    ch(&mut o, 'T');
    mv(&mut o, 150, 150);
    assert_eq!(o.cursor(), CursorKind::IBeam);
    ch(&mut o, 'R');
    assert_eq!(o.cursor(), CursorKind::Cross);
}

// ---- painting -----------------------------------------------------------------------------

fn px(o: &Overlay, x: i32, y: i32) -> [u8; 4] {
    let i = ((y as usize * o.w as usize) + x as usize) * 4;
    o.buffer()[i..i + 4].try_into().unwrap()
}

#[test]
fn first_paint_is_full_then_idle_paint_is_none() {
    let mut o = overlay();
    assert_eq!(o.buffer().len(), (FW * FH * 4) as usize);
    assert_eq!(o.paint(), Some(Rect::new(0, 0, FW as i32, FH as i32)));
    assert_eq!(o.paint(), None);
    mv(&mut o, 14, 12);
    let d = o.paint().expect("loupe moved");
    assert!(d.w * d.h < (FW * FH) as i32 / 2);
}

#[test]
fn selecting_dims_outside_and_keeps_bright_inside_selection() {
    let mut o = Overlay::new(init(DefaultAction::Annotate, OverlayMode::Region, None));
    o.handle(Input::MouseMove {
        x: 5,
        y: 5,
        mods: M,
    });
    o.paint();
    let f = frame();
    let dimmed = px(&o, 5, 5);
    let raw = f.pixel(5, 5);
    for c in 0..3 {
        assert!(
            dimmed[c] <= (raw[c] as f32 * 0.5) as u8 + 1,
            "dimmed: {dimmed:?} raw: {raw:?}"
        );
    }
    assert_eq!(dimmed[3], 255);
    drag(&mut o, (100, 100), (300, 250));
    // Interior (away from border/handles/toolbar/label) is the exact raw frame.
    assert_eq!(px(&o, 200, 175), f.pixel(200, 175));
    assert_ne!(px(&o, 400, 200), f.pixel(400, 200));
}

#[test]
fn overlay_selection_matches_flatten() {
    for font in [None, test_font::load()] {
        let mut o = Overlay::new(init(
            DefaultAction::Annotate,
            OverlayMode::Region,
            font.clone(),
        ));
        drag(&mut o, (100, 60), (500, 300));
        ch(&mut o, 'A');
        drag(&mut o, (150, 100), (300, 200));
        ch(&mut o, 'A');
        ch(&mut o, 'R');
        drag(&mut o, (320, 90), (400, 150));
        ch(&mut o, 'R');
        ch(&mut o, 'P');
        drag(&mut o, (150, 220), (260, 250));
        ch(&mut o, 'P');
        ch(&mut o, 'B');
        // Pixelate straddling the selection's left edge.
        drag(&mut o, (80, 120), (180, 170));
        ch(&mut o, 'B');
        ch(&mut o, 'N');
        click(&mut o, 420, 250);
        ch(&mut o, 'N');
        ch(&mut o, 'T');
        click(&mut o, 340, 200);
        for c in "Hello".chars() {
            o.handle(Input::Char(c));
        }
        key(&mut o, Key::Enter);
        ch(&mut o, 'T');
        o.paint();
        let sel = o.selection().unwrap();
        let flat = render::flatten(&frame(), sel, o.document().items(), font.as_ref());
        // Compare the region strictly inside the border/handles and above the toolbar.
        let inner = sel.inflate(-12);
        let mut checked = 0;
        for y in inner.y..inner.bottom() {
            for x in inner.x..inner.right() {
                assert_eq!(
                    px(&o, x, y),
                    flat.pixel((x - sel.x) as u32, (y - sel.y) as u32),
                    "pixel {x},{y}"
                );
                checked += 1;
            }
        }
        assert!(checked > 10_000);
    }
}

#[test]
fn export_pixelate_is_destructive() {
    let mut o = annotating();
    ch(&mut o, 'B');
    drag(&mut o, (120, 120), (200, 180));
    match key(&mut o, Key::Enter) {
        Outcome::Copy(img) => {
            let raw = frame().crop(Rect::new(100, 100, 200, 150));
            assert_ne!(img.data, raw.data);
            // Outside the pixelated rect everything is untouched.
            assert_eq!(img.pixel(150, 100), raw.pixel(150, 100));
            // Every cell is uniform.
            let b = Tool::Pixelate.default_size().round().max(2.0) as u32;
            let (x0, y0) = (20u32, 20u32);
            let p0 = img.pixel(x0, y0);
            for dy in 0..b.min(60) {
                for dx in 0..b.min(80) {
                    assert_eq!(img.pixel(x0 + dx, y0 + dy), p0);
                }
            }
        }
        other => panic!("{other:?}"),
    }
}

/// Incremental dirty-rect painting must produce exactly the same pixels as a clean full repaint.
#[test]
fn incremental_paint_equals_full_repaint() {
    for font in [None, test_font::load()] {
        let mut o = Overlay::new(init(DefaultAction::Annotate, OverlayMode::Region, font));
        let check = |o: &mut Overlay, what: &str| {
            o.paint();
            let inc = o.buffer().to_vec();
            o.painted = false;
            o.items.clear();
            o.paint();
            assert!(inc == o.buffer(), "incremental != full after {what}");
        };
        // Selecting: hovering, snapping, dragging.
        for (x, y) in [
            (310, 120),
            (350, 160),
            (50, 50),
            (600, 380),
            (620, 390),
            (5, 5),
        ] {
            mv(&mut o, x, y);
            check(&mut o, "hover");
        }
        mv(&mut o, 90, 70);
        down(&mut o, 90, 70);
        for (x, y) in [(120, 100), (300, 200), (450, 300), (200, 150)] {
            mv(&mut o, x, y);
            check(&mut o, "drag-select");
        }
        up(&mut o, 200, 150);
        check(&mut o, "release");
        // Annotating: hover toolbar, tools, previews, commits.
        let bar = o.toolbar_rect().unwrap();
        for dx in [10, 40, 90, 200, 300] {
            mv(&mut o, bar.x + dx, bar.y + 20);
            check(&mut o, "toolbar hover");
        }
        ch(&mut o, 'A');
        check(&mut o, "tool select");
        mv(&mut o, 100, 80);
        check(&mut o, "ring");
        down(&mut o, 100, 80);
        for (x, y) in [(150, 100), (180, 130)] {
            mv(&mut o, x, y);
            check(&mut o, "arrow preview");
        }
        up(&mut o, 180, 130);
        check(&mut o, "arrow commit");
        ch(&mut o, 'B');
        mv(&mut o, 100, 100);
        down(&mut o, 100, 100);
        for (x, y) in [(160, 140), (140, 170)] {
            mv(&mut o, x, y);
            check(&mut o, "pixelate preview");
        }
        up(&mut o, 140, 170);
        check(&mut o, "pixelate commit");
        ch(&mut o, 'P');
        mv(&mut o, 60, 60);
        down(&mut o, 60, 60);
        for (x, y) in [(80, 90), (120, 80), (140, 120)] {
            mv(&mut o, x, y);
            check(&mut o, "pen outside selection");
        }
        up(&mut o, 140, 120);
        check(&mut o, "pen commit");
        ch(&mut o, 'T');
        click(&mut o, 250, 120);
        o.handle(Input::Char('H'));
        o.handle(Input::Char('i'));
        check(&mut o, "text edit");
        key(&mut o, Key::Enter);
        check(&mut o, "text commit");
        ch(&mut o, 'T');
        ctrl(&mut o, 'Z');
        check(&mut o, "undo");
        ctrl(&mut o, 'Y');
        check(&mut o, "redo");
        o.handle(Input::KeyDown {
            key: Key::Down,
            mods: Mods { shift: true, ..M },
        });
        check(&mut o, "nudge");
        for (x, y) in [(300, 180), (420, 200)] {
            drag(&mut o, (x, y), (x + 30, y + 15));
            check(&mut o, "move selection");
        }
        let sel = o.selection().unwrap();
        mv(&mut o, sel.right(), sel.bottom());
        down(&mut o, sel.right(), sel.bottom());
        for d in [10, 30, 60] {
            mv(&mut o, sel.right() + d, sel.bottom() + d);
            check(&mut o, "resize");
        }
        up(&mut o, sel.right() + 60, sel.bottom() + 60);
        check(&mut o, "resize end");
    }
}
