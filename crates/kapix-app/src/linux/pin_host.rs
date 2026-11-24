//! Pure-Rust Wayland floating pin-to-screen window (`app_id = "kapix.pin"`).
//!
//! Controls match Windows `pin.rs`:
//! - Left drag anywhere: move window (`xdg_toplevel::move_`)
//! - Double-click left: copy pinned image to clipboard
//! - Scroll wheel: zoom 10%..400%
//! - Ctrl + scroll wheel: opacity 20%..100%
//! - Middle-click or `Esc`: close pin

use std::os::fd::{AsRawFd, BorrowedFd};
use std::time::{Duration, Instant};

use kapix_core::{Image, Rgba};
use kapix_linux::notify::Urgency;
use kapix_linux::shm::ShmSwapchain;
use kapix_ui::Palette;
use rustix::event::{PollFd, PollFlags, poll};
use wayrs_client::protocol::{
    WlCompositor, WlKeyboard, WlPointer, WlSeat, WlShm, WlSurface, wl_keyboard, wl_pointer, wl_seat,
};
use wayrs_client::{Connection, EventCtx, IoMode};
use wayrs_protocols::cursor_shape_v1::{
    WpCursorShapeDeviceV1, WpCursorShapeManagerV1, wp_cursor_shape_device_v1,
};
use wayrs_protocols::fractional_scale_v1::{
    WpFractionalScaleManagerV1, WpFractionalScaleV1, wp_fractional_scale_v1,
};
use wayrs_protocols::viewporter::{WpViewport, WpViewporter};
use wayrs_protocols::xdg_decoration_unstable_v1::{
    ZxdgDecorationManagerV1, ZxdgToplevelDecorationV1, zxdg_toplevel_decoration_v1,
};
use wayrs_protocols::xdg_shell::{
    XdgSurface, XdgToplevel, XdgWmBase, xdg_surface, xdg_toplevel, xdg_wm_base,
};

use super::keymap::{KEY_LEFTCTRL, KEY_RIGHTCTRL};

const BTN_LEFT: u32 = 0x110;
const BTN_MIDDLE: u32 = 0x112;
const KEY_ESC: u32 = 1;
const DOUBLE_CLICK_GAP: Duration = Duration::from_millis(400);

struct PinApp {
    shm: WlShm,
    seat: WlSeat,
    surface: WlSurface,
    xdg_surface: XdgSurface,
    toplevel: XdgToplevel,
    decoration: Option<ZxdgToplevelDecorationV1>,
    viewport: Option<WpViewport>,
    fractional_scale: Option<WpFractionalScaleV1>,
    cursor_shape_mgr: Option<WpCursorShapeManagerV1>,
    cursor_shape_dev: Option<WpCursorShapeDeviceV1>,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    swapchain: ShmSwapchain,

    image: Image,
    border: Rgba,
    zoom: f32,
    alpha: u8,
    scale_120: u32,
    configured: bool,
    needs_redraw: bool,
    should_exit: bool,

    ctrl_down: bool,
    xkb_ctrl: bool,
    last_click: Option<Instant>,
    scroll_v120_accum: i32,
    scroll_px_accum: f64,
}

impl PinApp {
    fn logical_size(&self) -> (u32, u32) {
        let base_scale = (self.scale_120 as f32 / 120.0).max(1.0);
        let w = (self.image.width as f32 * self.zoom / base_scale)
            .round()
            .clamp(32.0, 4096.0) as u32;
        let h = (self.image.height as f32 * self.zoom / base_scale)
            .round()
            .clamp(32.0, 4096.0) as u32;
        (w, h)
    }

    fn phys_size(&self) -> (u32, u32) {
        let (lw, lh) = self.logical_size();
        let pw = ((lw as u64 * self.scale_120 as u64 + 60) / 120).max(1) as u32;
        let ph = ((lh as u64 * self.scale_120 as u64 + 60) / 120).max(1) as u32;
        (pw, ph)
    }

    fn render(&mut self, conn: &mut Connection<Self>) {
        if !self.configured || !self.needs_redraw || !self.swapchain.has_free_slot() {
            return;
        }
        let (lw, lh) = self.logical_size();
        let (pw, ph) = self.phys_size();
        let Ok(Some((_idx, wl_buf, dst))) =
            self.swapchain
                .acquire(conn, self.shm, pw, ph, 0, on_pin_buffer_release)
        else {
            return;
        };

        blit_scaled_premul(&self.image, dst, pw, ph, self.alpha, self.border);

        if let Some(vp) = self.viewport {
            vp.set_destination(conn, lw as i32, lh as i32);
        } else {
            let int_scale = ((self.scale_120 + 60) / 120).max(1) as i32;
            self.surface.set_buffer_scale(conn, int_scale);
        }
        self.surface.attach(conn, Some(wl_buf), 0, 0);
        self.surface.damage_buffer(conn, 0, 0, pw as i32, ph as i32);
        self.surface.commit(conn);
        self.needs_redraw = false;
    }

    fn on_scroll(&mut self, notches: f32) {
        if self.ctrl_down || self.xkb_ctrl {
            let a = (self.alpha as f32 + notches * 25.5)
                .round()
                .clamp(51.0, 255.0) as u8;
            self.alpha = a;
        } else {
            self.zoom = (self.zoom * 1.1f32.powf(notches)).clamp(0.1, 4.0);
        }
        self.needs_redraw = true;
    }

    fn copy_image(&self) {
        let (w, h) = (self.image.width, self.image.height);
        match kapix_linux::clipboard::set_image(&self.image) {
            Ok(()) => kapix_linux::notify::send(Urgency::Normal, &format!("Copied {w}×{h}"), ""),
            Err(e) => kapix_linux::notify::send(Urgency::Critical, "Copy failed", &e.to_string()),
        }
    }

    fn destroy_all(&mut self, conn: &mut Connection<Self>) {
        self.swapchain.destroy_all(conn);
        if let Some(d) = self.decoration.take() {
            d.destroy(conn);
        }
        if let Some(vp) = self.viewport.take() {
            vp.destroy(conn);
        }
        if let Some(fs) = self.fractional_scale.take() {
            fs.destroy(conn);
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
        self.toplevel.destroy(conn);
        self.xdg_surface.destroy(conn);
        self.surface.destroy(conn);
        let _ = conn.flush(IoMode::NonBlocking);
    }
}

/// Show a floating pin window for `image` until the user closes it.
pub fn run_pin(image: Image, palette: Palette) -> Result<(), String> {
    if image.width == 0 || image.height == 0 {
        return Ok(());
    }

    let mut conn: Connection<PinApp> =
        Connection::connect().map_err(|e| format!("cannot connect to Wayland display: {e}"))?;
    conn.blocking_roundtrip()
        .map_err(|e| format!("Wayland initial roundtrip failed: {e}"))?;

    let compositor: WlCompositor = conn
        .bind_singleton(4..=6)
        .or_else(|_| conn.bind_singleton(1..=6))
        .map_err(|e| format!("missing wl_compositor: {e}"))?;
    let shm: WlShm = conn
        .bind_singleton(1..=2)
        .map_err(|e| format!("missing wl_shm: {e}"))?;
    let xdg_wm_base: XdgWmBase = conn
        .bind_singleton_with_cb(1..=7, |ctx: EventCtx<'_, PinApp, XdgWmBase>| {
            if let xdg_wm_base::Event::Ping(serial) = ctx.event {
                ctx.proxy.pong(ctx.conn, serial);
            }
        })
        .map_err(|e| format!("missing xdg_wm_base: {e}"))?;
    let seat: WlSeat = conn
        .bind_singleton_with_cb(1..=9, pin_seat_cb)
        .map_err(|e| format!("missing wl_seat: {e}"))?;
    let decoration_mgr: Option<ZxdgDecorationManagerV1> = conn.bind_singleton(1..=1).ok();
    let viewporter: Option<WpViewporter> = conn.bind_singleton(1..=1).ok();
    let frac_mgr: Option<WpFractionalScaleManagerV1> = conn.bind_singleton(1..=1).ok();
    let cursor_shape_mgr: Option<WpCursorShapeManagerV1> = conn.bind_singleton(1..=2).ok();

    let surface = compositor.create_surface(&mut conn);
    let viewport = viewporter.map(|vp| vp.get_viewport(&mut conn, surface));
    let fractional_scale = frac_mgr.map(|fm| {
        fm.get_fractional_scale_with_cb(&mut conn, surface, |ctx| {
            if let wp_fractional_scale_v1::Event::PreferredScale(scale) = ctx.event {
                let s = scale.max(120);
                if ctx.state.scale_120 != s {
                    ctx.state.scale_120 = s;
                    ctx.state.needs_redraw = true;
                }
            }
        })
    });
    let xdg_surface = xdg_wm_base.get_xdg_surface_with_cb(&mut conn, surface, |ctx| {
        if let xdg_surface::Event::Configure(serial) = ctx.event {
            ctx.proxy.ack_configure(ctx.conn, serial);
            ctx.state.configured = true;
            ctx.state.needs_redraw = true;
        }
    });
    let toplevel = xdg_surface.get_toplevel_with_cb(&mut conn, |ctx| {
        if let xdg_toplevel::Event::Close = ctx.event {
            ctx.state.should_exit = true;
        }
    });
    toplevel.set_app_id(&mut conn, c"kapix.pin".to_owned());
    toplevel.set_title(&mut conn, c"Kapix pin".to_owned());

    let decoration = decoration_mgr.map(|mgr| {
        let dec = mgr.get_toplevel_decoration(&mut conn, toplevel);
        dec.set_mode(&mut conn, zxdg_toplevel_decoration_v1::Mode::ClientSide);
        dec
    });

    surface.commit(&mut conn);

    let mut state = PinApp {
        shm,
        seat,
        surface,
        xdg_surface,
        toplevel,
        decoration,
        viewport,
        fractional_scale,
        cursor_shape_mgr,
        cursor_shape_dev: None,
        keyboard: None,
        pointer: None,
        swapchain: ShmSwapchain::new(),
        image,
        border: palette.line_strong,
        zoom: 1.0,
        alpha: 255,
        scale_120: 120,
        configured: false,
        needs_redraw: true,
        should_exit: false,
        ctrl_down: false,
        xkb_ctrl: false,
        last_click: None,
        scroll_v120_accum: 0,
        scroll_px_accum: 0.0,
    };

    conn.flush(IoMode::Blocking)
        .map_err(|e| format!("Wayland flush failed: {e}"))?;

    while !state.should_exit {
        // SAFETY: `conn` outlives `fds` and owns the Wayland socket fd.
        let wl_fd = unsafe { BorrowedFd::borrow_raw(conn.as_raw_fd()) };
        let mut fds = [PollFd::new(&wl_fd, PollFlags::IN)];
        match poll(&mut fds, None) {
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
                Err(_) => break,
            }
            conn.dispatch_events(&mut state);
        }

        if !state.should_exit {
            state.render(&mut conn);
            if let Err(e) = conn.flush(IoMode::Blocking)
                && e.kind() != std::io::ErrorKind::WouldBlock
            {
                break;
            }
        }
    }

    state.destroy_all(&mut conn);
    Ok(())
}

fn on_pin_buffer_release(state: &mut PinApp, _surf: usize, slot_idx: usize) {
    state.swapchain.mark_released(slot_idx);
}

fn pin_seat_cb(ctx: EventCtx<PinApp, WlSeat>) {
    if let wl_seat::Event::Capabilities(caps) = ctx.event {
        if caps.contains(wl_seat::Capability::Keyboard) && ctx.state.keyboard.is_none() {
            ctx.state.keyboard = Some(ctx.proxy.get_keyboard_with_cb(ctx.conn, pin_keyboard_cb));
        }
        if caps.contains(wl_seat::Capability::Pointer) && ctx.state.pointer.is_none() {
            let ptr = ctx.proxy.get_pointer_with_cb(ctx.conn, pin_pointer_cb);
            if let Some(mgr) = ctx.state.cursor_shape_mgr {
                ctx.state.cursor_shape_dev = Some(mgr.get_pointer(ctx.conn, ptr));
            }
            ctx.state.pointer = Some(ptr);
        }
    }
}

fn pin_keyboard_cb(ctx: EventCtx<PinApp, WlKeyboard>) {
    match ctx.event {
        wl_keyboard::Event::Modifiers(m) => {
            ctx.state.xkb_ctrl = (m.mods_depressed & 0x04) != 0;
        }
        wl_keyboard::Event::Key(ev) => {
            let pressed = ev.state == wl_keyboard::KeyState::Pressed;
            match ev.key {
                KEY_LEFTCTRL | KEY_RIGHTCTRL => ctx.state.ctrl_down = pressed,
                KEY_ESC if pressed => ctx.state.should_exit = true,
                _ => {}
            }
        }
        _ => {}
    }
}

fn pin_pointer_cb(ctx: EventCtx<PinApp, WlPointer>) {
    match ctx.event {
        wl_pointer::Event::Enter(ev) => {
            if let Some(dev) = ctx.state.cursor_shape_dev {
                dev.set_shape(ctx.conn, ev.serial, wp_cursor_shape_device_v1::Shape::Move);
            }
        }
        wl_pointer::Event::Button(ev) => {
            if ev.state != wl_pointer::ButtonState::Pressed {
                return;
            }
            match ev.button {
                BTN_LEFT => {
                    let now = Instant::now();
                    if ctx
                        .state
                        .last_click
                        .is_some_and(|prev| now.duration_since(prev) <= DOUBLE_CLICK_GAP)
                    {
                        ctx.state.last_click = None;
                        ctx.state.copy_image();
                    } else {
                        ctx.state.last_click = Some(now);
                        ctx.state
                            .toplevel
                            .r#move(ctx.conn, ctx.state.seat, ev.serial);
                    }
                }
                BTN_MIDDLE => {
                    ctx.state.should_exit = true;
                }
                _ => {}
            }
        }
        wl_pointer::Event::AxisValue120(ev) => {
            if ev.axis == wl_pointer::Axis::VerticalScroll {
                ctx.state.scroll_v120_accum -= ev.value120;
                let notches = ctx.state.scroll_v120_accum / 120;
                if notches != 0 {
                    ctx.state.scroll_v120_accum -= notches * 120;
                    ctx.state.on_scroll(notches as f32);
                }
            }
        }
        wl_pointer::Event::Axis(ev)
            if ev.axis == wl_pointer::Axis::VerticalScroll && ctx.state.scroll_v120_accum == 0 =>
        {
            ctx.state.scroll_px_accum -= ev.value.as_f64();
            let notches = (ctx.state.scroll_px_accum / 15.0).trunc() as i32;
            if notches != 0 {
                ctx.state.scroll_px_accum -= notches as f64 * 15.0;
                ctx.state.on_scroll(notches as f32);
            }
        }
        _ => {}
    }
}

/// Bilinearly scale `src` (BGRA8) into `dst` (premultiplied ARGB8888 / BGRA8 bytes) at `alpha`,
/// with a 1 px Kavox border.
fn blit_scaled_premul(src: &Image, dst: &mut [u8], dw: u32, dh: u32, alpha: u8, border: Rgba) {
    let sw = src.width as usize;
    let sh = src.height as usize;
    let dw_u = dw as usize;
    let dh_u = dh as usize;
    let a = u32::from(alpha);

    let premul = |c: u8| -> u8 { ((u32::from(c) * a + 127) / 255) as u8 };
    let border_px = [premul(border.b), premul(border.g), premul(border.r), alpha];

    for y in 0..dh_u {
        let is_edge_y = y == 0 || y + 1 == dh_u;
        let fy = if dh_u > 1 {
            (y as f32 + 0.5) * (sh as f32 / dh_u as f32) - 0.5
        } else {
            0.0
        };
        let y0 = (fy.floor() as isize).clamp(0, sh as isize - 1) as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = (fy - y0 as f32).clamp(0.0, 1.0);

        for x in 0..dw_u {
            let di = (y * dw_u + x) * 4;
            if di + 4 > dst.len() {
                return;
            }
            if is_edge_y || x == 0 || x + 1 == dw_u {
                dst[di..di + 4].copy_from_slice(&border_px);
                continue;
            }
            if sw == dw_u && sh == dh_u {
                let si = (y * sw + x) * 4;
                dst[di] = premul(src.data[si]);
                dst[di + 1] = premul(src.data[si + 1]);
                dst[di + 2] = premul(src.data[si + 2]);
                dst[di + 3] = alpha;
                continue;
            }
            let fx = (x as f32 + 0.5) * (sw as f32 / dw_u as f32) - 0.5;
            let x0 = (fx.floor() as isize).clamp(0, sw as isize - 1) as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = (fx - x0 as f32).clamp(0.0, 1.0);

            let p00 = &src.data[(y0 * sw + x0) * 4..][..4];
            let p10 = &src.data[(y0 * sw + x1) * 4..][..4];
            let p01 = &src.data[(y1 * sw + x0) * 4..][..4];
            let p11 = &src.data[(y1 * sw + x1) * 4..][..4];

            let w00 = (1.0 - wx) * (1.0 - wy);
            let w10 = wx * (1.0 - wy);
            let w01 = (1.0 - wx) * wy;
            let w11 = wx * wy;

            for c in 0..3 {
                let v = (p00[c] as f32 * w00
                    + p10[c] as f32 * w10
                    + p01[c] as f32 * w01
                    + p11[c] as f32 * w11)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                dst[di + c] = premul(v);
            }
            dst[di + 3] = alpha;
        }
    }
}
