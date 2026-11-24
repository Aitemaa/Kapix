//! Pure-Rust Wayland fullscreen capture overlay using `zwlr_layer_shell_v1`.
//!
//! Creates one `Layer::Overlay` surface per captured `wl_output` with exclusive keyboard focus
//! and `wp_viewporter` 1:1 physical pixel presentation. Feeds pointer/keyboard events into
//! `kapix_ui::Overlay` and blits updated regions into double-buffered `wl_shm` swapchains.

use std::os::fd::{AsRawFd, BorrowedFd};
use std::time::{Duration, Instant};

use kapix_core::{Config, Rect};
use kapix_linux::capture::{Capture, OutputInfo};
use kapix_linux::shm::ShmSwapchain;
use kapix_ui::{
    CursorKind, Font, Input, Mods, MouseButton, Outcome, Overlay, OverlayInit, OverlayMode, Palette,
};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayrs_client::global::GlobalExt;
use wayrs_client::protocol::{
    WlCallback, WlCompositor, WlKeyboard, WlOutput, WlPointer, WlSeat, WlShm, WlSurface,
    wl_callback, wl_keyboard, wl_pointer, wl_seat,
};
use wayrs_client::{Connection, EventCtx, IoMode};
use wayrs_protocols::cursor_shape_v1::{
    WpCursorShapeDeviceV1, WpCursorShapeManagerV1, wp_cursor_shape_device_v1,
};
use wayrs_protocols::viewporter::{WpViewport, WpViewporter};
use wayrs_protocols::wlr_layer_shell_unstable_v1::{
    ZwlrLayerShellV1, ZwlrLayerSurfaceV1, zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

use super::keymap::{
    KEY_CAPSLOCK, KEY_LEFTALT, KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_RIGHTALT, KEY_RIGHTCTRL,
    KEY_RIGHTSHIFT, evdev_to_char, evdev_to_key,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

struct OutputSurface {
    info: OutputInfo,
    surface: WlSurface,
    layer_surface: ZwlrLayerSurfaceV1,
    viewport: Option<WpViewport>,
    swapchain: ShmSwapchain,
    frame_cb: Option<WlCallback>,
    configured: bool,
    logical_w: u32,
    logical_h: u32,
    dirty: Option<Rect>,
    /// True once the initial full frame has been committed to this surface.
    first_committed: bool,
}

struct RepeatState {
    keycode: u32,
    next_at: Instant,
    interval: Duration,
}

struct OverlayApp {
    shm: WlShm,
    surfaces: Vec<OutputSurface>,
    overlay: Overlay,
    cursor_shape_mgr: Option<WpCursorShapeManagerV1>,
    cursor_shape_dev: Option<WpCursorShapeDeviceV1>,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,

    // Input state
    active_output: Option<usize>,
    pointer_pos_buf: (i32, i32),
    pointer_enter_serial: u32,
    last_cursor: Option<CursorKind>,
    scroll_v120_accum: i32,
    scroll_px_accum: f64,

    // Modifier tracking (XKB modifier bitmask + raw key state fallback)
    shift_key: bool,
    ctrl_key: bool,
    alt_key: bool,
    xkb_shift: bool,
    xkb_ctrl: bool,
    xkb_alt: bool,
    xkb_caps: bool,

    // Key repeat
    repeat_delay_ms: i32,
    repeat_rate_hz: i32,
    repeat: Option<RepeatState>,

    // Terminal outcome handling
    pending_outcome: Option<Outcome>,
    result: Option<Outcome>,
    should_exit: bool,
}

impl OverlayApp {
    fn mods(&self) -> Mods {
        Mods {
            ctrl: self.ctrl_key || self.xkb_ctrl,
            shift: self.shift_key || self.xkb_shift,
            alt: self.alt_key || self.xkb_alt,
        }
    }

    fn map_surface_pos(&self, surf_idx: usize, sx: f64, sy: f64) -> (i32, i32) {
        let Some(surf) = self.surfaces.get(surf_idx) else {
            return self.pointer_pos_buf;
        };
        let lw = surf.logical_w.max(1) as f64;
        let lh = surf.logical_h.max(1) as f64;
        let pw = surf.info.buffer_rect.w.max(1);
        let ph = surf.info.buffer_rect.h.max(1);
        let ox = ((sx * pw as f64 / lw).round() as i32).clamp(0, pw - 1);
        let oy = ((sy * ph as f64 / lh).round() as i32).clamp(0, ph - 1);
        (surf.info.buffer_rect.x + ox, surf.info.buffer_rect.y + oy)
    }

    fn feed(&mut self, conn: &mut Connection<Self>, input: Input) {
        if self.should_exit {
            return;
        }
        let is_down = matches!(input, Input::MouseDown { .. });
        if self.pending_outcome.is_some() {
            if matches!(input, Input::MouseUp { .. })
                && let Some(out) = self.pending_outcome.take()
            {
                self.finish(out);
            }
            return;
        }

        let out = self.overlay.handle(input);
        if out.is_terminal() {
            if is_down {
                self.pending_outcome = Some(out);
            } else {
                self.finish(out);
            }
            return;
        }

        if matches!(out, Outcome::Redraw) {
            self.paint_dirty();
        }
        self.sync_cursor(conn);
    }

    fn finish(&mut self, out: Outcome) {
        self.result = Some(out);
        self.should_exit = true;
    }

    fn paint_dirty(&mut self) {
        let Some(dirty) = self.overlay.paint() else {
            return;
        };
        for surf in &mut self.surfaces {
            if let Some(clipped) = dirty.intersect(&surf.info.buffer_rect) {
                let local = clipped.translate(-surf.info.buffer_rect.x, -surf.info.buffer_rect.y);
                surf.dirty = Some(match surf.dirty {
                    Some(prev) => prev.union(&local),
                    None => local,
                });
            }
        }
    }

    fn sync_cursor(&mut self, conn: &mut Connection<Self>) {
        let kind = self.overlay.cursor();
        if self.last_cursor == Some(kind) {
            return;
        }
        if let Some(dev) = self.cursor_shape_dev {
            let shape = match kind {
                CursorKind::Arrow => wp_cursor_shape_device_v1::Shape::Default,
                CursorKind::Cross => wp_cursor_shape_device_v1::Shape::Crosshair,
                CursorKind::Move => wp_cursor_shape_device_v1::Shape::Move,
                CursorKind::SizeNS => wp_cursor_shape_device_v1::Shape::NsResize,
                CursorKind::SizeWE => wp_cursor_shape_device_v1::Shape::EwResize,
                CursorKind::SizeNWSE => wp_cursor_shape_device_v1::Shape::NwseResize,
                CursorKind::SizeNESW => wp_cursor_shape_device_v1::Shape::NeswResize,
                CursorKind::IBeam => wp_cursor_shape_device_v1::Shape::Text,
                CursorKind::Hand => wp_cursor_shape_device_v1::Shape::Pointer,
            };
            dev.set_shape(conn, self.pointer_enter_serial, shape);
            self.last_cursor = Some(kind);
        }
    }

    fn present_all(&mut self, conn: &mut Connection<Self>) {
        let (frame_w, _) = self.overlay.size();
        let src_stride = frame_w as usize * 4;
        let src_buf = self.overlay.buffer();

        for i in 0..self.surfaces.len() {
            let surf = &mut self.surfaces[i];
            if !surf.configured || surf.dirty.is_none() {
                continue;
            }
            if surf.frame_cb.is_some() && surf.first_committed {
                continue;
            }
            if !surf.swapchain.has_free_slot() {
                continue;
            }

            let pw = surf.info.buffer_rect.w.max(1) as u32;
            let ph = surf.info.buffer_rect.h.max(1) as u32;
            let Ok(Some((_slot_idx, wl_buf, dst))) =
                surf.swapchain
                    .acquire(conn, self.shm, pw, ph, i, on_buffer_release)
            else {
                continue;
            };

            // Copy the output's slice of the overlay BGRA8 backbuffer into the SHM slot.
            // (We always copy the full output slice into the acquired slot because swapchain
            // slots alternate between two independent buffers.)
            let bx = surf.info.buffer_rect.x as usize;
            let by = surf.info.buffer_rect.y as usize;
            let row_bytes = pw as usize * 4;
            for row in 0..ph as usize {
                let src_off = (by + row) * src_stride + bx * 4;
                let dst_off = row * row_bytes;
                if src_off + row_bytes <= src_buf.len() && dst_off + row_bytes <= dst.len() {
                    dst[dst_off..dst_off + row_bytes]
                        .copy_from_slice(&src_buf[src_off..src_off + row_bytes]);
                }
            }

            let damage = surf
                .dirty
                .take()
                .unwrap_or_else(|| Rect::new(0, 0, pw as i32, ph as i32));

            if let Some(vp) = surf.viewport {
                vp.set_destination(
                    conn,
                    surf.logical_w.max(1) as i32,
                    surf.logical_h.max(1) as i32,
                );
            } else {
                let scale = surf.info.scale.round().max(1.0) as i32;
                surf.surface.set_buffer_scale(conn, scale);
            }

            surf.surface.attach(conn, Some(wl_buf), 0, 0);
            surf.surface
                .damage_buffer(conn, damage.x, damage.y, damage.w.max(1), damage.h.max(1));

            let cb = surf.surface.frame_with_cb(conn, move |ctx| {
                let wl_callback::Event::Done(_) = ctx.event;
                if let Some(s) = ctx.state.surfaces.get_mut(i) {
                    s.frame_cb = None;
                }
            });
            surf.frame_cb = Some(cb);
            surf.first_committed = true;
            surf.surface.commit(conn);
        }
    }

    fn process_key_press(&mut self, conn: &mut Connection<Self>, keycode: u32) {
        let mods = self.mods();
        let key = evdev_to_key(keycode);
        self.feed(conn, Input::KeyDown { key, mods });
        if !self.should_exit
            && self.pending_outcome.is_none()
            && !mods.ctrl
            && !mods.alt
            && let Some(ch) = evdev_to_char(keycode, mods.shift, self.xkb_caps)
        {
            self.feed(conn, Input::Char(ch));
        }
    }

    fn handle_repeat(&mut self, conn: &mut Connection<Self>) {
        let now = Instant::now();
        let due = self.repeat.as_ref().is_some_and(|r| now >= r.next_at);
        if !due {
            return;
        }
        let Some(r) = self.repeat.as_mut() else {
            return;
        };
        let keycode = r.keycode;
        r.next_at = now + r.interval;
        self.process_key_press(conn, keycode);
    }

    fn destroy_all(&mut self, conn: &mut Connection<Self>) {
        for surf in &mut self.surfaces {
            surf.swapchain.destroy_all(conn);
            if let Some(vp) = surf.viewport.take() {
                vp.destroy(conn);
            }
            surf.layer_surface.destroy(conn);
            surf.surface.destroy(conn);
        }
        if let Some(dev) = self.cursor_shape_dev.take() {
            dev.destroy(conn);
        }
        if let Some(kb) = self.keyboard.take() {
            kb.release(conn);
        }
        if let Some(ptr) = self.pointer.take() {
            ptr.release(conn);
        }
        let _ = conn.flush(IoMode::NonBlocking);
    }
}

/// Run the interactive Wayland capture overlay on a frozen [`Capture`] and return its [`Outcome`].
pub fn run_overlay(
    cap: Capture,
    mode: OverlayMode,
    cfg: &Config,
    font: Font,
    palette: Palette,
) -> Result<Outcome, String> {
    let Capture {
        frame,
        desktop,
        outputs,
    } = cap;

    let windows = if cfg.capture.window_snap {
        kapix_linux::snap::window_rects(&outputs)
    } else {
        Vec::new()
    };

    // Start cursor at the center of the focused output until `wl_pointer::Enter` arrives.
    let focused_rect = kapix_linux::snap::focused_output_rect(&outputs)
        .unwrap_or_else(|| Rect::new(0, 0, frame.width as i32, frame.height as i32));
    let init_cursor = (
        focused_rect.x + focused_rect.w / 2,
        focused_rect.y + focused_rect.h / 2,
    );

    let mut overlay = Overlay::new(OverlayInit {
        frame,
        desktop,
        windows,
        mode,
        default_action: cfg.capture.default_action,
        show_magnifier: cfg.capture.show_magnifier,
        window_snap: cfg.capture.window_snap,
        font,
        palette,
        cursor: init_cursor,
    });

    // Trigger initial composition so the first frame is ready the instant configure arrives.
    let initial_dirty = overlay.paint();

    let mut conn: Connection<OverlayApp> =
        Connection::connect().map_err(|e| format!("cannot connect to Wayland display: {e}"))?;
    conn.blocking_roundtrip()
        .map_err(|e| format!("Wayland initial roundtrip failed: {e}"))?;
    let globals = conn.globals().to_vec();

    let compositor: WlCompositor = conn
        .bind_singleton(4..=6)
        .or_else(|_| conn.bind_singleton(1..=6))
        .map_err(|e| format!("missing wl_compositor: {e}"))?;
    let shm: WlShm = conn
        .bind_singleton(1..=2)
        .map_err(|e| format!("missing wl_shm: {e}"))?;
    let layer_shell: ZwlrLayerShellV1 = conn.bind_singleton(1..=5).map_err(|e| {
        format!("compositor does not advertise zwlr_layer_shell_v1 (required for overlay): {e}")
    })?;
    let viewporter: Option<WpViewporter> = conn.bind_singleton(1..=1).ok();
    let cursor_shape_mgr: Option<WpCursorShapeManagerV1> = conn.bind_singleton(1..=2).ok();

    // Bind wl_seat to receive keyboard & pointer events.
    let _seat: WlSeat = conn
        .bind_singleton_with_cb(1..=9, wl_seat_cb)
        .map_err(|e| format!("missing wl_seat: {e}"))?;

    // Bind wl_output globals and create one Overlay layer-surface per captured output.
    let mut surfaces = Vec::with_capacity(outputs.len());
    for info in outputs {
        let wl_out: Option<WlOutput> = globals
            .iter()
            .find(|g| g.name == info.global_name && g.is::<WlOutput>())
            .and_then(|g| g.bind(&mut conn, 1..=4).ok());

        let idx = surfaces.len();
        let surface = compositor.create_surface(&mut conn);
        let viewport = viewporter.map(|vp| vp.get_viewport(&mut conn, surface));
        let layer_surface = layer_shell.get_layer_surface_with_cb(
            &mut conn,
            surface,
            wl_out,
            zwlr_layer_shell_v1::Layer::Overlay,
            c"kapix-overlay".to_owned(),
            move |ctx| layer_surface_cb(ctx, idx),
        );

        let all_edges = zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Bottom
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right;
        layer_surface.set_anchor(&mut conn, all_edges);
        layer_surface.set_exclusive_zone(&mut conn, -1);
        layer_surface.set_size(&mut conn, 0, 0);
        layer_surface.set_keyboard_interactivity(
            &mut conn,
            zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
        );
        surface.commit(&mut conn);

        let dirty = initial_dirty.and_then(|d| {
            d.intersect(&info.buffer_rect)
                .map(|r| r.translate(-info.buffer_rect.x, -info.buffer_rect.y))
        });

        surfaces.push(OutputSurface {
            logical_w: info.logical_rect.w.max(1) as u32,
            logical_h: info.logical_rect.h.max(1) as u32,
            info,
            surface,
            layer_surface,
            viewport,
            swapchain: ShmSwapchain::new(),
            frame_cb: None,
            configured: false,
            dirty,
            first_committed: false,
        });
    }

    let mut state = OverlayApp {
        shm,
        surfaces,
        overlay,
        cursor_shape_mgr,
        cursor_shape_dev: None,
        keyboard: None,
        pointer: None,
        active_output: None,
        pointer_pos_buf: init_cursor,
        pointer_enter_serial: 0,
        last_cursor: None,
        scroll_v120_accum: 0,
        scroll_px_accum: 0.0,
        shift_key: false,
        ctrl_key: false,
        alt_key: false,
        xkb_shift: false,
        xkb_ctrl: false,
        xkb_alt: false,
        xkb_caps: false,
        repeat_delay_ms: 400,
        repeat_rate_hz: 25,
        repeat: None,
        pending_outcome: None,
        result: None,
        should_exit: false,
    };

    conn.flush(IoMode::Blocking)
        .map_err(|e| format!("Wayland flush failed: {e}"))?;

    while !state.should_exit {
        // Compute poll timeout for active key repeat (None = sleep indefinitely, 0% idle CPU).
        let timeout = state.repeat.as_ref().map(|r| {
            let rem = r.next_at.saturating_duration_since(Instant::now());
            Timespec {
                tv_sec: rem.as_secs() as i64,
                tv_nsec: rem.subsec_nanos() as i64,
            }
        });

        // SAFETY: `conn` outlives `fds` and owns the Wayland socket fd.
        let wl_fd = unsafe { BorrowedFd::borrow_raw(conn.as_raw_fd()) };
        let mut fds = [PollFd::new(&wl_fd, PollFlags::IN)];
        match poll(&mut fds, timeout.as_ref()) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(e) => {
                state.destroy_all(&mut conn);
                return Err(format!("poll failed: {e}"));
            }
        }

        if fds[0].revents().contains(PollFlags::IN) {
            match conn.recv_events(IoMode::NonBlocking) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    state.destroy_all(&mut conn);
                    return Err(format!("Wayland connection lost: {e}"));
                }
            }
            conn.dispatch_events(&mut state);
        }

        if !state.should_exit {
            state.handle_repeat(&mut conn);
        }
        if !state.should_exit {
            state.present_all(&mut conn);
            if let Err(e) = conn.flush(IoMode::Blocking)
                && e.kind() != std::io::ErrorKind::WouldBlock
            {
                state.destroy_all(&mut conn);
                return Err(format!("Wayland flush failed: {e}"));
            }
        }
    }

    state.destroy_all(&mut conn);
    Ok(state.result.unwrap_or(Outcome::Cancel))
}

fn on_buffer_release(state: &mut OverlayApp, surf_idx: usize, slot_idx: usize) {
    if let Some(surf) = state.surfaces.get_mut(surf_idx) {
        surf.swapchain.mark_released(slot_idx);
    }
}

fn layer_surface_cb(ctx: EventCtx<OverlayApp, ZwlrLayerSurfaceV1>, idx: usize) {
    match ctx.event {
        zwlr_layer_surface_v1::Event::Configure(cfg) => {
            ctx.proxy.ack_configure(ctx.conn, cfg.serial);
            if let Some(surf) = ctx.state.surfaces.get_mut(idx) {
                if cfg.width > 0 {
                    surf.logical_w = cfg.width;
                }
                if cfg.height > 0 {
                    surf.logical_h = cfg.height;
                }
                surf.configured = true;
                if surf.dirty.is_none() {
                    surf.dirty = Some(Rect::new(
                        0,
                        0,
                        surf.info.buffer_rect.w,
                        surf.info.buffer_rect.h,
                    ));
                }
            }
        }
        zwlr_layer_surface_v1::Event::Closed => {
            ctx.state.finish(Outcome::Cancel);
        }
        _ => {}
    }
}

fn wl_seat_cb(ctx: EventCtx<OverlayApp, WlSeat>) {
    if let wl_seat::Event::Capabilities(caps) = ctx.event {
        if caps.contains(wl_seat::Capability::Keyboard) && ctx.state.keyboard.is_none() {
            let kb = ctx.proxy.get_keyboard_with_cb(ctx.conn, wl_keyboard_cb);
            ctx.state.keyboard = Some(kb);
        }
        if caps.contains(wl_seat::Capability::Pointer) && ctx.state.pointer.is_none() {
            let ptr = ctx.proxy.get_pointer_with_cb(ctx.conn, wl_pointer_cb);
            if let Some(mgr) = ctx.state.cursor_shape_mgr {
                ctx.state.cursor_shape_dev = Some(mgr.get_pointer(ctx.conn, ptr));
            }
            ctx.state.pointer = Some(ptr);
        }
    }
}

fn wl_keyboard_cb(ctx: EventCtx<OverlayApp, WlKeyboard>) {
    match ctx.event {
        wl_keyboard::Event::Modifiers(m) => {
            // Standard XKB depressed modifier bits: Shift=0x01, Lock=0x02, Ctrl=0x04, Mod1(Alt)=0x08.
            ctx.state.xkb_shift = (m.mods_depressed & 0x01) != 0;
            ctx.state.xkb_caps = (m.mods_locked & 0x02) != 0;
            ctx.state.xkb_ctrl = (m.mods_depressed & 0x04) != 0;
            ctx.state.xkb_alt = (m.mods_depressed & 0x08) != 0;
        }
        wl_keyboard::Event::RepeatInfo(info) => {
            ctx.state.repeat_rate_hz = info.rate;
            ctx.state.repeat_delay_ms = info.delay;
        }
        wl_keyboard::Event::Leave(_) => {
            ctx.state.repeat = None;
        }
        wl_keyboard::Event::Key(ev) => {
            let pressed = ev.state == wl_keyboard::KeyState::Pressed;
            match ev.key {
                KEY_LEFTSHIFT | KEY_RIGHTSHIFT => ctx.state.shift_key = pressed,
                KEY_LEFTCTRL | KEY_RIGHTCTRL => ctx.state.ctrl_key = pressed,
                KEY_LEFTALT | KEY_RIGHTALT => ctx.state.alt_key = pressed,
                KEY_CAPSLOCK if pressed => {}
                _ => {}
            }

            if pressed {
                ctx.state.process_key_press(ctx.conn, ev.key);
                let is_mod = matches!(
                    ev.key,
                    KEY_LEFTSHIFT
                        | KEY_RIGHTSHIFT
                        | KEY_LEFTCTRL
                        | KEY_RIGHTCTRL
                        | KEY_LEFTALT
                        | KEY_RIGHTALT
                        | KEY_CAPSLOCK
                );
                if !is_mod && ctx.state.repeat_rate_hz > 0 && !ctx.state.should_exit {
                    let delay = Duration::from_millis(ctx.state.repeat_delay_ms.max(100) as u64);
                    let interval =
                        Duration::from_micros(1_000_000 / ctx.state.repeat_rate_hz.max(1) as u64);
                    ctx.state.repeat = Some(RepeatState {
                        keycode: ev.key,
                        next_at: Instant::now() + delay,
                        interval,
                    });
                }
            } else {
                if ctx
                    .state
                    .repeat
                    .as_ref()
                    .is_some_and(|r| r.keycode == ev.key)
                {
                    ctx.state.repeat = None;
                }
                let mods = ctx.state.mods();
                let key = evdev_to_key(ev.key);
                ctx.state.feed(ctx.conn, Input::KeyUp { key, mods });
            }
        }
        _ => {}
    }
}

fn wl_pointer_cb(ctx: EventCtx<OverlayApp, WlPointer>) {
    match ctx.event {
        wl_pointer::Event::Enter(ev) => {
            ctx.state.pointer_enter_serial = ev.serial;
            ctx.state.last_cursor = None;
            let idx = ctx
                .state
                .surfaces
                .iter()
                .position(|s| s.surface == ev.surface);
            ctx.state.active_output = idx;
            if let Some(i) = idx {
                let pos =
                    ctx.state
                        .map_surface_pos(i, ev.surface_x.as_f64(), ev.surface_y.as_f64());
                ctx.state.pointer_pos_buf = pos;
                let mods = ctx.state.mods();
                ctx.state.feed(
                    ctx.conn,
                    Input::MouseMove {
                        x: pos.0,
                        y: pos.1,
                        mods,
                    },
                );
            }
            ctx.state.sync_cursor(ctx.conn);
        }
        wl_pointer::Event::Motion(ev) => {
            if let Some(i) = ctx.state.active_output {
                let pos =
                    ctx.state
                        .map_surface_pos(i, ev.surface_x.as_f64(), ev.surface_y.as_f64());
                ctx.state.pointer_pos_buf = pos;
                let mods = ctx.state.mods();
                ctx.state.feed(
                    ctx.conn,
                    Input::MouseMove {
                        x: pos.0,
                        y: pos.1,
                        mods,
                    },
                );
            }
        }
        wl_pointer::Event::Button(ev) => {
            let button = match ev.button {
                BTN_LEFT => MouseButton::Left,
                BTN_RIGHT => MouseButton::Right,
                BTN_MIDDLE => MouseButton::Middle,
                _ => return,
            };
            let (x, y) = ctx.state.pointer_pos_buf;
            let mods = ctx.state.mods();
            if ev.state == wl_pointer::ButtonState::Pressed {
                ctx.state
                    .feed(ctx.conn, Input::MouseDown { x, y, button, mods });
            } else {
                ctx.state
                    .feed(ctx.conn, Input::MouseUp { x, y, button, mods });
            }
        }
        wl_pointer::Event::AxisValue120(ev) => {
            if ev.axis == wl_pointer::Axis::VerticalScroll {
                // Wayland vertical scroll: negative = up (away from user); Overlay expects +1 = up.
                ctx.state.scroll_v120_accum -= ev.value120;
                let notches = ctx.state.scroll_v120_accum / 120;
                if notches != 0 {
                    ctx.state.scroll_v120_accum -= notches * 120;
                    let (x, y) = ctx.state.pointer_pos_buf;
                    let mods = ctx.state.mods();
                    ctx.state.feed(
                        ctx.conn,
                        Input::Wheel {
                            x,
                            y,
                            delta: notches as f32,
                            mods,
                        },
                    );
                }
            }
        }
        wl_pointer::Event::Axis(ev)
            if ev.axis == wl_pointer::Axis::VerticalScroll && ctx.state.scroll_v120_accum == 0 =>
        {
            ctx.state.scroll_px_accum -= ev.value.as_f64();
            let step = 15.0;
            let notches = (ctx.state.scroll_px_accum / step).trunc() as i32;
            if notches != 0 {
                ctx.state.scroll_px_accum -= notches as f64 * step;
                let (x, y) = ctx.state.pointer_pos_buf;
                let mods = ctx.state.mods();
                ctx.state.feed(
                    ctx.conn,
                    Input::Wheel {
                        x,
                        y,
                        delta: notches as f32,
                        mods,
                    },
                );
            }
        }
        _ => {}
    }
}
