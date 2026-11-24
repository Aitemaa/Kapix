//! Pure-Rust `wl_shm` shared-memory buffers and double-buffered swapchain (`memfd_create` + `mmap`).

use std::ffi::CStr;
use std::os::fd::OwnedFd;
use std::ptr::NonNull;
use std::slice;

use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use rustix::mm::{MapFlags, ProtFlags, mmap, munmap};
use wayrs_client::Connection;
use wayrs_client::protocol::{WlBuffer, WlShm, WlShmPool, wl_buffer, wl_shm};

use crate::{Error, Result};

/// Single-use `wl_shm` buffer (used for `zwlr_screencopy_frame_v1::copy` capture targets).
pub struct ShmBuffer {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: wl_shm::Format,
    byte_len: usize,
    ptr: NonNull<u8>,
    _fd: OwnedFd,
    pool: WlShmPool,
    pub buffer: WlBuffer,
}

impl ShmBuffer {
    pub fn new<D>(
        conn: &mut Connection<D>,
        shm: WlShm,
        width: u32,
        height: u32,
        stride: u32,
        format: wl_shm::Format,
    ) -> Result<Self> {
        let byte_len = (stride as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| Error("shm buffer size overflow".into()))?;
        let alloc_len = byte_len.max(4);

        let name: &CStr = c"kapix-capture";
        let fd = memfd_create(name, MemfdFlags::CLOEXEC)
            .map_err(|e| Error(format!("memfd_create failed: {e}")))?;
        ftruncate(&fd, alloc_len as u64).map_err(|e| Error(format!("ftruncate failed: {e}")))?;

        // SAFETY: `fd` is a valid anonymous memfd sized to `alloc_len` bytes; RW/SHARED mmap
        // returns an aligned allocation owned by this `ShmBuffer` until `Drop`.
        let raw_ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                alloc_len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &fd,
                0,
            )
        }
        .map_err(|e| Error(format!("mmap failed: {e}")))?;

        let ptr = NonNull::new(raw_ptr.cast::<u8>())
            .ok_or_else(|| Error("mmap returned null pointer".into()))?;

        let pool_fd = fd
            .try_clone()
            .map_err(|e| Error(format!("dup memfd failed: {e}")))?;
        let pool = shm.create_pool(conn, pool_fd, alloc_len as i32);
        let buffer =
            pool.create_buffer(conn, 0, width as i32, height as i32, stride as i32, format);

        Ok(Self {
            width,
            height,
            stride,
            format,
            byte_len: alloc_len,
            ptr,
            _fd: fd,
            pool,
            buffer,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `self.ptr` points to a valid `self.byte_len`-byte mapping.
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.byte_len) }
    }

    pub fn destroy_wl<D>(&self, conn: &mut Connection<D>) {
        self.buffer.destroy(conn);
        self.pool.destroy(conn);
    }
}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        // SAFETY: `self.ptr` and `self.byte_len` came directly from `mmap` in `ShmBuffer::new`.
        let _ = unsafe { munmap(self.ptr.as_ptr().cast(), self.byte_len) };
    }
}

struct ShmSlot {
    width: u32,
    height: u32,
    byte_len: usize,
    ptr: NonNull<u8>,
    _fd: OwnedFd,
    pool: WlShmPool,
    buffer: WlBuffer,
    busy: bool,
}

impl ShmSlot {
    fn new<D: 'static>(
        conn: &mut Connection<D>,
        shm: WlShm,
        width: u32,
        height: u32,
        slot_index: usize,
        surface_id: usize,
        on_release: fn(&mut D, usize, usize),
    ) -> Result<Self> {
        let stride = width
            .checked_mul(4)
            .ok_or_else(|| Error("framebuffer stride overflow".into()))?;
        let byte_len = (stride as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| Error("framebuffer size overflow".into()))?;
        let alloc_len = byte_len.max(4);

        let name: &CStr = c"kapix-shm";
        let fd = memfd_create(name, MemfdFlags::CLOEXEC)
            .map_err(|e| Error(format!("memfd_create failed: {e}")))?;
        ftruncate(&fd, alloc_len as u64).map_err(|e| Error(format!("ftruncate failed: {e}")))?;

        // SAFETY: `fd` is a valid anonymous memfd sized to `alloc_len` bytes; RW/SHARED mmap
        // returns an aligned allocation owned by this `ShmSlot` until `Drop`.
        let raw_ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                alloc_len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &fd,
                0,
            )
        }
        .map_err(|e| Error(format!("mmap failed: {e}")))?;

        let ptr = NonNull::new(raw_ptr.cast::<u8>())
            .ok_or_else(|| Error("mmap returned null pointer".into()))?;

        let pool_fd = fd
            .try_clone()
            .map_err(|e| Error(format!("dup memfd failed: {e}")))?;
        let pool = shm.create_pool(conn, pool_fd, alloc_len as i32);
        let buffer = pool.create_buffer_with_cb(
            conn,
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            move |ctx| {
                let wl_buffer::Event::Release = ctx.event;
                on_release(ctx.state, surface_id, slot_index);
            },
        );

        Ok(Self {
            width,
            height,
            byte_len: alloc_len,
            ptr,
            _fd: fd,
            pool,
            buffer,
            busy: false,
        })
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        // SAFETY: `self.ptr` points to a valid `self.byte_len`-byte RW mmap, and `&mut self`
        // guarantees exclusive access (we only call this when `!self.busy`).
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.byte_len) }
    }

    fn destroy_wl<D>(&self, conn: &mut Connection<D>) {
        self.buffer.destroy(conn);
        self.pool.destroy(conn);
    }
}

impl Drop for ShmSlot {
    fn drop(&mut self) {
        // SAFETY: `self.ptr` and `self.byte_len` came directly from `mmap` in `ShmSlot::new`.
        let _ = unsafe { munmap(self.ptr.as_ptr().cast(), self.byte_len) };
    }
}

/// Double-buffered `wl_shm` swapchain. Never overwrites a buffer still held by the compositor.
pub struct ShmSwapchain {
    slots: [Option<ShmSlot>; 2],
}

impl Default for ShmSwapchain {
    fn default() -> Self {
        Self::new()
    }
}

impl ShmSwapchain {
    pub fn new() -> Self {
        Self {
            slots: [None, None],
        }
    }

    pub fn mark_released(&mut self, index: usize) {
        if let Some(Some(slot)) = self.slots.get_mut(index) {
            slot.busy = false;
        }
    }

    /// True if at least one slot is free (or unallocated) for drawing.
    pub fn has_free_slot(&self) -> bool {
        self.slots
            .iter()
            .any(|s| s.as_ref().is_none_or(|slot| !slot.busy))
    }

    /// Acquire a free slot matching `(width, height)`, recreating it if resized.
    /// Returns `Ok(None)` if both slots are still held by the compositor (caller should skip this
    /// frame and wait for `wl_buffer::Release`).
    pub fn acquire<D: 'static>(
        &mut self,
        conn: &mut Connection<D>,
        shm: WlShm,
        width: u32,
        height: u32,
        surface_id: usize,
        on_release: fn(&mut D, usize, usize),
    ) -> Result<Option<(usize, WlBuffer, &mut [u8])>> {
        let w = width.max(1);
        let h = height.max(1);

        let idx = self
            .slots
            .iter()
            .position(
                |s| matches!(s, Some(slot) if !slot.busy && slot.width == w && slot.height == h),
            )
            .or_else(|| {
                self.slots
                    .iter()
                    .position(|s| s.as_ref().is_none_or(|slot| !slot.busy))
            });

        let Some(idx) = idx else {
            return Ok(None);
        };

        let needs_alloc = match &self.slots[idx] {
            Some(slot) => slot.width != w || slot.height != h,
            None => true,
        };

        if needs_alloc {
            if let Some(old) = self.slots[idx].take() {
                old.destroy_wl(conn);
            }
            let fresh = ShmSlot::new(conn, shm, w, h, idx, surface_id, on_release)?;
            self.slots[idx] = Some(fresh);
        }

        let slot = self.slots[idx]
            .as_mut()
            .ok_or_else(|| Error("shm slot missing after allocation".into()))?;
        slot.busy = true;
        let buffer = slot.buffer;
        let pixels = slot.pixels_mut();
        Ok(Some((idx, buffer, pixels)))
    }

    pub fn destroy_all<D>(&mut self, conn: &mut Connection<D>) {
        for slot in &mut self.slots {
            if let Some(s) = slot.take() {
                s.destroy_wl(conn);
            }
        }
    }
}
