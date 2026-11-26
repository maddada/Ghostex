//! Screen capture with ScreenCaptureKit. Frames arrive as NV12 (converted on the GPU,
//! BT.601 like the Linux path) together with the rectangles that changed, which drive
//! the same damage bookkeeping as XDamage: nothing is converted or encoded while the
//! screen is still, and x264 skips the macroblocks that didn't change.

use super::ffi::*;
use super::stream::{self, OutputType};
use super::Geometry;
use nix::unistd::{read, write};
use objc2_core_foundation::CFString;
use objc2_screen_capture_kit::{
    SCCaptureResolutionType, SCStreamFrameInfoDirtyRects, SCStreamFrameInfoStatus,
};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub static DAMAGE_EVENTS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub y0: u32,
    pub y1: u32,
}

/// A retained ScreenCaptureKit frame, read-locked while it is the current one.
struct Frame {
    pb: CVPixelBufferRef,
    locked: bool,
}
unsafe impl Send for Frame {}

impl Frame {
    fn retain(pb: CVPixelBufferRef) -> Self {
        unsafe { CVPixelBufferRetain(pb) };
        Frame { pb, locked: false }
    }
    fn lock(&mut self) {
        if !self.locked {
            unsafe { CVPixelBufferLockBaseAddress(self.pb, kCVPixelBufferLock_ReadOnly) };
            self.locked = true;
        }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            if self.locked {
                CVPixelBufferUnlockBaseAddress(self.pb, kCVPixelBufferLock_ReadOnly);
            }
            CVPixelBufferRelease(self.pb);
        }
    }
}

/// Handed from the capture queue to the capture thread.
#[derive(Default)]
struct Incoming {
    frame: Option<Frame>,
    /// Changed areas as (x0, y0, x1, y1) in frame pixels.
    rects: Vec<(u32, u32, u32, u32)>,
}

pub struct Capturer {
    pub width: u32,
    pub height: u32,
    geometry: Geometry,
    incoming: Arc<Mutex<Incoming>>,
    wake_r: OwnedFd,
    /// The frame conversions read from.
    current: Option<Frame>,
    /// Newest frame, adopted at the next grab.
    next: Option<Frame>,
    pub damaged: bool,
    /// When the first damage since the last take arrived.
    pub damaged_at: Option<Instant>,
    acc: Option<(u32, u32)>,
    mb_dirty: Vec<u8>,
    pub mb_cols: usize,
    /// Areas (x0, y0, x1, y1) changed since the video thread last took them, for `/wait`.
    pub changes: Vec<(u32, u32, u32, u32)>,
    stream: stream::Stream,
    last_check: Instant,
    rebuild: bool,
}

/// Number from a CFNumber value of a frame-info dictionary.
unsafe fn dict_i64(dict: CFDictionaryRef, key: *const c_void) -> Option<i64> {
    let v = CFDictionaryGetValue(dict, key);
    let mut out = 0i64;
    (!v.is_null() && CFNumberGetValue(v, kCFNumberSInt64Type, &mut out as *mut i64 as *mut c_void))
        .then_some(out)
}

impl Capturer {
    pub fn new(_display: Option<&str>) -> Result<Self, String> {
        let display = super::display();
        let (width, height) = super::capture_size(display);
        let geometry = Geometry {
            display,
            bounds: unsafe { CGDisplayBounds(display) },
            width,
        };
        *super::GEOMETRY.lock() = Some(geometry);
        let fps = super::fps();
        let incoming = Arc::new(Mutex::new(Incoming::default()));
        let (wake_r, wake_w) = crate::hub::wake_pipe().map_err(|e| e.to_string())?;
        let debug = std::env::var("VOXL_DEBUG_DAMAGE").is_ok();
        let inc = incoming.clone();
        let (w, h) = (width, height);
        let on_frame = move |sb: CMSampleBufferRef, ty: OutputType| unsafe {
            if ty != OutputType::Screen || !CMSampleBufferIsValid(sb) {
                return;
            }
            let att = CMSampleBufferGetSampleAttachmentsArray(sb, false);
            if att.is_null() || CFArrayGetCount(att) == 0 {
                return;
            }
            let info = CFArrayGetValueAtIndex(att, 0);
            let status_key = SCStreamFrameInfoStatus as *const _ as *const c_void;
            // Only complete frames carry new pixels (idle ones repeat the last).
            if dict_i64(info, status_key) != Some(0) {
                return;
            }
            let pb = CMSampleBufferGetImageBuffer(sb);
            if pb.is_null() {
                return;
            }
            let mut rects = Vec::new();
            let list = CFDictionaryGetValue(info, SCStreamFrameInfoDirtyRects as *const _ as _);
            if list.is_null() {
                rects.push((0, 0, w, h));
            } else {
                for i in 0..CFArrayGetCount(list) {
                    let mut r = CGRect::default();
                    if CGRectMakeWithDictionaryRepresentation(
                        CFArrayGetValueAtIndex(list, i),
                        &mut r,
                    ) {
                        let x0 = r.origin.x.floor().clamp(0.0, w as f64) as u32;
                        let y0 = r.origin.y.floor().clamp(0.0, h as f64) as u32;
                        let x1 = (r.origin.x + r.size.width).ceil().clamp(0.0, w as f64) as u32;
                        let y1 = (r.origin.y + r.size.height).ceil().clamp(0.0, h as f64) as u32;
                        if x1 > x0 && y1 > y0 {
                            rects.push((x0, y0, x1, y1));
                        }
                    }
                }
            }
            if debug {
                log::info!(
                    "frame {}x{} dirty {:?}",
                    CVPixelBufferGetWidth(pb),
                    CVPixelBufferGetHeight(pb),
                    rects
                );
            }
            {
                let mut inc = inc.lock();
                inc.frame = Some(Frame::retain(pb));
                inc.rects.extend(rects);
            }
            let _ = write(&wake_w, &[1u8]);
        };
        let stream = stream::start(
            display,
            "capture",
            &[OutputType::Screen],
            |cfg| unsafe {
                cfg.setWidth(width as usize);
                cfg.setHeight(height as usize);
                cfg.setMinimumFrameInterval(objc2_core_media::CMTime {
                    value: 1,
                    timescale: fps as i32,
                    flags: objc2_core_media::CMTimeFlags::Valid,
                    epoch: 0,
                });
                cfg.setPixelFormat(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
                cfg.setColorMatrix(&*(kCGDisplayStreamYCbCrMatrix_ITU_R_601_4 as *const CFString));
                // Convert to sRGB on the GPU: without this, wide-gamut displays (P3) are
                // captured in their native space and shown desaturated as BT.709/sRGB.
                cfg.setColorSpaceName(&*(kCGColorSpaceSRGB as *const CFString));
                // Clients draw the cursor themselves (see desktop.rs), so pointer motion
                // costs no encode and has no stream latency.
                // If the system cursor can't be read, or --show-cursor is set (e.g. for viewers),
                // it has to be in the video.
                cfg.setShowsCursor(super::show_cursor() || !super::desktop::cursor_readable());
                cfg.setCaptureResolution(SCCaptureResolutionType::Best);
                cfg.setQueueDepth(6);
            },
            on_frame,
        )?;
        let mut me = Capturer {
            width,
            height,
            geometry,
            incoming,
            wake_r,
            current: None,
            next: None,
            damaged: true,
            damaged_at: None,
            acc: None,
            mb_dirty: vec![0; (width as usize).div_ceil(16) * (height as usize).div_ceil(16)],
            mb_cols: (width as usize).div_ceil(16),
            changes: Vec::new(),
            stream,
            last_check: Instant::now(),
            rebuild: false,
        };
        // The stream opens with a full frame; have it before anything is converted.
        let t = Instant::now();
        while me.next.is_none() && t.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
            me.pump()?;
        }
        if me.next.is_none() {
            log::warn!("no frame from ScreenCaptureKit yet");
        }
        me.grab(me.full())?;
        log::info!("capturing display {display} with ScreenCaptureKit at {width}x{height}");
        Ok(me)
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        self.wake_r.as_fd()
    }

    /// Take frames and changed areas that arrived since the last call.
    pub fn pump(&mut self) -> Result<(), String> {
        let mut drain = [0u8; 64];
        while matches!(read(&self.wake_r, &mut drain), Ok(n) if n > 0) {}
        let rects = {
            let mut inc = self.incoming.lock();
            if let Some(f) = inc.frame.take() {
                self.next = Some(f);
            }
            std::mem::take(&mut inc.rects)
        };
        for (x0, y0, x1, y1) in rects {
            DAMAGE_EVENTS.fetch_add(1, Ordering::Relaxed);
            let (c0, c1) = (x0 as usize / 16, (x1 as usize).div_ceil(16));
            for row in (y0 as usize / 16)..(y1 as usize).div_ceil(16) {
                self.mb_dirty[row * self.mb_cols + c0..row * self.mb_cols + c1].fill(1);
            }
            self.acc = Some(match self.acc {
                Some((a, b)) => (a.min(y0), b.max(y1)),
                None => (y0, y1),
            });
            self.changes.push((x0, y0, x1, y1));
            self.damaged = true;
            self.damaged_at.get_or_insert_with(Instant::now);
        }
        if self.last_check.elapsed() >= Duration::from_secs(1) {
            self.last_check = Instant::now();
            // Resolution changed (System Settings, another display arrangement).
            let b = unsafe { CGDisplayBounds(self.geometry.display) };
            if b.size != self.geometry.bounds.size
                || super::capture_size(self.geometry.display) != (self.width, self.height)
            {
                log::info!("display changed; restarting capture");
                self.rebuild = true;
            } else if b.origin != self.geometry.bounds.origin {
                self.geometry.bounds = b;
                *super::GEOMETRY.lock() = Some(self.geometry);
            }
        }
        Ok(())
    }

    /// Take accumulated damage as a row band.
    pub fn take_damage(&mut self) -> Result<Option<Band>, String> {
        self.damaged = false;
        self.damaged_at = None;
        let Some((y0, y1)) = self.acc.take() else {
            return Ok(None);
        };
        Ok(Some(Band {
            y0: y0 & !1,
            y1: ((y1 + 1) & !1).min(self.height),
        }))
    }

    /// Take the dirty-macroblock map accumulated with the current damage.
    pub fn take_mb_map(&mut self) -> Vec<u8> {
        let n = self.mb_dirty.len();
        std::mem::replace(&mut self.mb_dirty, vec![0; n])
    }

    pub fn full(&self) -> Band {
        Band {
            y0: 0,
            y1: self.height,
        }
    }

    /// Make the newest frame the current one. Every ScreenCaptureKit frame is a whole
    /// screen, so the band doesn't matter.
    pub fn grab(&mut self, _band: Band) -> Result<(), String> {
        if let Some(mut f) = self.next.take() {
            f.lock();
            self.current = Some(f);
        }
        Ok(())
    }

    /// Convert rows [y0, y1) of the current frame into `dst`.
    pub fn convert(&self, dst: &mut crate::convert::I420, y0: usize, y1: usize, threads: usize) {
        let Some(f) = &self.current else {
            return;
        };
        unsafe {
            let pb = f.pb;
            if CVPixelBufferGetWidth(pb) != self.width as usize
                || CVPixelBufferGetHeight(pb) != self.height as usize
            {
                return;
            }
            crate::convert::convert_nv12_band(
                CVPixelBufferGetBaseAddressOfPlane(pb, 0),
                CVPixelBufferGetBytesPerRowOfPlane(pb, 0),
                CVPixelBufferGetBaseAddressOfPlane(pb, 1),
                CVPixelBufferGetBytesPerRowOfPlane(pb, 1),
                dst,
                y0,
                y1.min(self.height as usize),
                threads,
            );
        }
    }

    /// The current frame as packed BGRX (`width * 4` bytes per row), for screenshots.
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        // Each frame arrives together with its changed areas, so taking them and then
        // adopting the newest frame leaves `changes` holding the damage for exactly
        // these pixels.
        self.pump()?;
        self.grab(self.full())?;
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = vec![0u8; w * h * 4];
        let Some(f) = &self.current else {
            return Ok(out);
        };
        unsafe {
            let pb = f.pb;
            if CVPixelBufferGetWidth(pb) != w || CVPixelBufferGetHeight(pb) != h {
                return Ok(out);
            }
            let (ys, uvs) = (
                CVPixelBufferGetBytesPerRowOfPlane(pb, 0),
                CVPixelBufferGetBytesPerRowOfPlane(pb, 1),
            );
            let y = std::slice::from_raw_parts(CVPixelBufferGetBaseAddressOfPlane(pb, 0), ys * h);
            let uv = std::slice::from_raw_parts(
                CVPixelBufferGetBaseAddressOfPlane(pb, 1),
                uvs * h.div_ceil(2),
            );
            crate::convert::nv12_to_bgrx(y, ys, uv, uvs, w, h, &mut out);
        }
        Ok(out)
    }

    /// Frames are NV12 here, so the BGRX debug marker probe doesn't apply.
    pub fn bgrx(&self) -> Option<(&[u8], usize)> {
        None
    }

    /// The display changed or the stream ended: capture must be set up again.
    pub fn needs_rebuild(&self) -> bool {
        self.rebuild || self.stream.stopped.load(Ordering::Relaxed)
    }
}
