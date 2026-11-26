//! Screen capture with DXGI Desktop Duplication. A capture thread takes each new
//! desktop image with the rectangles that changed (dirty and moved areas), copies just
//! those into a BGRA frame in memory and wakes the video thread, which runs the same
//! damage bookkeeping as XDamage: nothing is converted or encoded while the screen is
//! still, and x264 skips the macroblocks that didn't change. Duplication leaves the
//! cursor out, so clients draw it themselves (see desktop.rs).

use super::Geometry;
use crate::hub::{Waitable, Wake};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED,
    DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_WAIT_TIMEOUT,
    DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_MOVE_RECT,
};

pub static DAMAGE_EVENTS: AtomicU64 = AtomicU64::new(0);

extern "C" {
    /// libyuv: scales the `clip_*` part of the destination only.
    fn ARGBScaleClip(
        src_argb: *const u8,
        src_stride_argb: i32,
        src_width: i32,
        src_height: i32,
        dst_argb: *mut u8,
        dst_stride_argb: i32,
        dst_width: i32,
        dst_height: i32,
        clip_x: i32,
        clip_y: i32,
        clip_width: i32,
        clip_height: i32,
        filtering: i32,
    ) -> i32;
}
/// libyuv's kFilterBox: averages every source pixel, which keeps small text readable.
const FILTER_BOX: i32 = 3;

#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub y0: u32,
    pub y1: u32,
}

/// Handed from the capture thread to the video thread.
struct Shared {
    /// The desktop as BGRA, `width * 4` bytes per row.
    buf: Vec<u8>,
    /// Changed areas as (x0, y0, x1, y1) since the last pump.
    rects: Vec<(u32, u32, u32, u32)>,
    frames: u64,
    /// The display changed size or went away: capture must be set up again.
    rebuild: bool,
}

pub struct Capturer {
    pub width: u32,
    pub height: u32,
    geometry: Geometry,
    shared: Arc<Mutex<Shared>>,
    wake: Arc<Wake>,
    /// The frame conversions read from: rows are copied from `shared` at each grab.
    frame: Vec<u8>,
    /// `frame` scaled to `width` x `height` (BGRA, `width * 4` per row) when the display
    /// streams smaller than it is (--max-capture-pixels); then conversions read this.
    scaled: Option<Vec<u8>>,
    pub damaged: bool,
    /// When the first damage since the last take arrived.
    pub damaged_at: Option<Instant>,
    acc: Option<(u32, u32)>,
    mb_dirty: Vec<u8>,
    pub mb_cols: usize,
    /// Areas (x0, y0, x1, y1) changed since the video thread last took them, for `/wait`.
    pub changes: Vec<(u32, u32, u32, u32)>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    last_check: Instant,
    rebuild: bool,
}

/// What the capture thread needs, all of it created on that thread.
struct Dup {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    dup: IDXGIOutputDuplication,
    staging: ID3D11Texture2D,
    width: u32,
    height: u32,
}

fn open_dup() -> Result<Dup, String> {
    let out = super::output().ok_or("no display attached to the desktop")?;
    let mut device: Option<ID3D11Device> = None;
    let mut ctx: Option<ID3D11DeviceContext> = None;
    unsafe {
        D3D11CreateDevice(
            &out.adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut ctx),
        )
    }
    .map_err(|e| format!("D3D11CreateDevice: {e}"))?;
    let (device, ctx) = (device.ok_or("no D3D11 device")?, ctx.ok_or("no context")?);
    let output1: IDXGIOutput1 = super::cast(&out.output)?;
    let dup = unsafe { output1.DuplicateOutput(&device) }.map_err(|e| {
        format!(
            "DuplicateOutput: {e} (capture needs an interactive desktop session; \
             under Remote Desktop, a session that stays connected)"
        )
    })?;
    let desc = unsafe { dup.GetDesc() };
    let rot = desc.Rotation;
    if rot != DXGI_MODE_ROTATION_IDENTITY && rot != DXGI_MODE_ROTATION_UNSPECIFIED {
        log::warn!("the display is rotated; it streams in its unrotated orientation");
    }
    let (width, height) = (desc.ModeDesc.Width, desc.ModeDesc.Height);
    let tdesc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging = None;
    unsafe { device.CreateTexture2D(&tdesc, None, Some(&mut staging)) }
        .map_err(|e| format!("CreateTexture2D: {e}"))?;
    Ok(Dup {
        device,
        ctx,
        dup,
        staging: staging.ok_or("no staging texture")?,
        width,
        height,
    })
}

/// Changed rectangles of the frame just acquired, clipped to the screen.
fn frame_rects(d: &Dup, info: &DXGI_OUTDUPL_FRAME_INFO) -> Vec<(u32, u32, u32, u32)> {
    let (w, h) = (d.width as i32, d.height as i32);
    let mut out = Vec::new();
    let mut push = |r: RECT| {
        let (x0, y0) = (r.left.clamp(0, w), r.top.clamp(0, h));
        let (x1, y1) = (r.right.clamp(0, w), r.bottom.clamp(0, h));
        if x1 > x0 && y1 > y0 {
            out.push((x0 as u32, y0 as u32, x1 as u32, y1 as u32));
        }
    };
    let size = info.TotalMetadataBufferSize as usize;
    if size == 0 {
        return out;
    }
    // Moved areas: the destination changed (the image already shows the move).
    let mut moves = vec![DXGI_OUTDUPL_MOVE_RECT::default(); size / 24 + 1];
    let mut need = 0u32;
    let bytes = (moves.len() * std::mem::size_of::<DXGI_OUTDUPL_MOVE_RECT>()) as u32;
    if unsafe {
        d.dup
            .GetFrameMoveRects(bytes, moves.as_mut_ptr(), &mut need)
    }
    .is_ok()
    {
        let n = need as usize / std::mem::size_of::<DXGI_OUTDUPL_MOVE_RECT>();
        for m in &moves[..n] {
            push(m.DestinationRect);
        }
    }
    let mut dirty = vec![RECT::default(); size / 16 + 1];
    let bytes = (dirty.len() * std::mem::size_of::<RECT>()) as u32;
    if unsafe {
        d.dup
            .GetFrameDirtyRects(bytes, dirty.as_mut_ptr(), &mut need)
    }
    .is_ok()
    {
        let n = need as usize / std::mem::size_of::<RECT>();
        for &r in &dirty[..n] {
            push(r);
        }
    }
    out
}

/// Copies `rects` of the acquired desktop image into `shared`.
fn copy_rects(
    d: &Dup,
    image: &IDXGIResource,
    rects: &[(u32, u32, u32, u32)],
    shared: &Mutex<Shared>,
) -> Result<(), String> {
    let tex: ID3D11Texture2D = super::cast(image)?;
    unsafe {
        if rects.len() > 32 {
            d.ctx.CopyResource(&d.staging, &tex);
        } else {
            for &(x0, y0, x1, y1) in rects {
                let b = D3D11_BOX {
                    left: x0,
                    top: y0,
                    front: 0,
                    right: x1,
                    bottom: y1,
                    back: 1,
                };
                d.ctx
                    .CopySubresourceRegion(&d.staging, 0, x0, y0, 0, &tex, 0, Some(&b));
            }
        }
        let mut m = D3D11_MAPPED_SUBRESOURCE::default();
        d.ctx
            .Map(&d.staging, 0, D3D11_MAP_READ, 0, Some(&mut m))
            .map_err(|e| format!("Map: {e}"))?;
        let pitch = m.RowPitch as usize;
        let src = m.pData as *const u8;
        let row = d.width as usize * 4;
        {
            let mut s = shared.lock();
            for &(x0, y0, x1, y1) in rects {
                let (a, len) = (x0 as usize * 4, (x1 - x0) as usize * 4);
                for y in y0 as usize..y1 as usize {
                    std::ptr::copy_nonoverlapping(
                        src.add(y * pitch + a),
                        s.buf.as_mut_ptr().add(y * row + a),
                        len,
                    );
                }
            }
            s.rects.extend_from_slice(rects);
            s.frames += 1;
        }
        d.ctx.Unmap(&d.staging, 0);
    }
    Ok(())
}

/// Takes desktop images until `stop`; reopens duplication when Windows takes it away
/// (desktop switch, UAC prompt, mode change).
fn capture_thread(mut d: Dup, shared: Arc<Mutex<Shared>>, wake: Arc<Wake>, stop: Arc<AtomicBool>) {
    let debug = std::env::var("VOXL_DEBUG_DAMAGE").is_ok();
    let full = vec![(0, 0, d.width, d.height)];
    let mut first = true;
    while !stop.load(Ordering::Relaxed) {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut image: Option<IDXGIResource> = None;
        match unsafe { d.dup.AcquireNextFrame(100, &mut info, &mut image) } {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => continue,
            Err(e) => {
                // Access lost (or a broken device): open it again, at the same size or
                // have the pipeline rebuilt.
                log::info!("desktop duplication interrupted ({e}); reopening");
                // DXGI won't duplicate an output twice: release the old one first.
                drop(image);
                drop(d);
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    super::follow_input_desktop();
                    match open_dup() {
                        Ok(n) => {
                            d = n;
                            break;
                        }
                        Err(e) => log::debug!("reopening capture: {e}"),
                    }
                }
                if (d.width, d.height) != (full[0].2, full[0].3) {
                    shared.lock().rebuild = true;
                    wake.signal();
                    return;
                }
                first = true;
                continue;
            }
        }
        // Only the pointer moved: no new desktop image.
        if info.LastPresentTime == 0 && !first {
            let _ = unsafe { d.dup.ReleaseFrame() };
            continue;
        }
        let rects = if first {
            full.clone()
        } else {
            frame_rects(&d, &info)
        };
        if debug {
            log::info!("frame dirty {rects:?}");
        }
        let res = match &image {
            Some(img) if !rects.is_empty() => copy_rects(&d, img, &rects, &shared),
            _ => Ok(()),
        };
        drop(image);
        let _ = unsafe { d.dup.ReleaseFrame() };
        match res {
            Ok(()) if !rects.is_empty() => {
                first = false;
                wake.signal();
            }
            Ok(()) => {}
            Err(e) => log::warn!("capture: {e}"),
        }
    }
    // Keeps the device alive as long as the duplication that uses it.
    drop(d.device);
}

impl Capturer {
    pub fn new(_display: Option<&str>) -> Result<Self, String> {
        let wake = Arc::new(Wake::new().map_err(|e| e.to_string())?);
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(Shared {
            buf: Vec::new(),
            rects: Vec::new(),
            frames: 0,
            rebuild: false,
        }));
        // D3D objects stay on the thread that made them; it reports the size back.
        let (tx, rx) = std::sync::mpsc::channel::<Result<(u32, u32), String>>();
        let thread = {
            let (shared, wake, stop) = (shared.clone(), wake.clone(), stop.clone());
            std::thread::Builder::new()
                .name("capture".into())
                .spawn(move || {
                    super::follow_input_desktop();
                    match open_dup() {
                        Ok(d) => {
                            shared.lock().buf = vec![0; d.width as usize * d.height as usize * 4];
                            let _ = tx.send(Ok((d.width, d.height)));
                            capture_thread(d, shared, wake, stop);
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e));
                        }
                    }
                })
                .map_err(|e| e.to_string())?
        };
        let (width, height) = rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "timed out starting capture".to_string())??;
        let r = super::output().map(|o| o.rect()).unwrap_or_default();
        // x264 wants even sizes; the odd last row or column is left out. A display with
        // more pixels than --max-capture-pixels streams as a smaller copy.
        let (stream_width, stream_height) = super::stream_size(width, height);
        let geometry = Geometry {
            left: r.left,
            top: r.top,
            width,
            height,
            stream_width,
            stream_height,
        };
        *super::GEOMETRY.lock() = Some(geometry);
        let is_scaled = (stream_width, stream_height) != (width & !1, height & !1);
        let (width, height) = (stream_width, stream_height);
        let mut me = Capturer {
            width,
            height,
            geometry,
            shared,
            wake,
            frame: vec![0; geometry.width as usize * geometry.height as usize * 4],
            scaled: is_scaled.then(|| vec![0; width as usize * height as usize * 4]),
            damaged: true,
            damaged_at: None,
            acc: None,
            mb_dirty: vec![0; (width as usize).div_ceil(16) * (height as usize).div_ceil(16)],
            mb_cols: (width as usize).div_ceil(16),
            changes: Vec::new(),
            stop,
            thread: Some(thread),
            last_check: Instant::now(),
            rebuild: false,
        };
        // Duplication opens with a full frame; have it before anything is converted.
        let t = Instant::now();
        while me.shared.lock().frames == 0 && t.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
        }
        if me.shared.lock().frames == 0 {
            log::warn!("no frame from desktop duplication yet");
        }
        me.pump()?;
        me.grab(me.full())?;
        if is_scaled {
            log::info!(
                "capturing with DXGI Desktop Duplication at {}x{}, streaming {width}x{height}",
                geometry.width,
                geometry.height
            );
        } else {
            log::info!("capturing with DXGI Desktop Duplication at {width}x{height}");
        }
        Ok(me)
    }

    pub fn fd(&self) -> Waitable<'_> {
        self.wake.waitable()
    }

    /// Take changed areas that arrived since the last call.
    pub fn pump(&mut self) -> Result<(), String> {
        self.wake.drain();
        let rects = {
            let mut s = self.shared.lock();
            if s.rebuild {
                self.rebuild = true;
            }
            std::mem::take(&mut s.rects)
        };
        self.add_damage(rects);
        if self.last_check.elapsed() >= Duration::from_secs(1) {
            self.last_check = Instant::now();
            // The display moved on the virtual desktop (another display arranged).
            if let Some(r) = super::output().map(|o| o.rect()) {
                let (w, h) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
                if (w, h) != (self.geometry.width, self.geometry.height) {
                    log::info!("display changed; restarting capture");
                    self.rebuild = true;
                } else if (r.left, r.top) != (self.geometry.left, self.geometry.top) {
                    self.geometry.left = r.left;
                    self.geometry.top = r.top;
                    *super::GEOMETRY.lock() = Some(self.geometry);
                }
            }
        }
        Ok(())
    }

    /// Record changed areas: rows and blocks for the encoder, `changes` for /wait.
    fn add_damage(&mut self, rects: Vec<(u32, u32, u32, u32)>) {
        for rect in rects {
            let (x0, y0, x1, y1) = self.to_stream(rect);
            let (x1, y1) = (x1.min(self.width), y1.min(self.height));
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
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
    }

    /// A changed area of the display in streamed pixels, grown to whole pixels and by
    /// one more, since the box filter blends each streamed pixel with its neighbours.
    fn to_stream(&self, (x0, y0, x1, y1): (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
        if self.scaled.is_none() {
            return (x0, y0, x1, y1);
        }
        let (gw, gh) = (self.geometry.width as u64, self.geometry.height as u64);
        let (sw, sh) = (self.width as u64, self.height as u64);
        let down = |v: u32, s: u64, g: u64| ((v as u64 * s) / g).saturating_sub(1) as u32;
        let up = |v: u32, s: u64, g: u64| ((v as u64 * s).div_ceil(g) + 1).min(s) as u32;
        (down(x0, sw, gw), down(y0, sh, gh), up(x1, sw, gw), up(y1, sh, gh))
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

    /// Copy rows [y0, y1) of the newest desktop image into the frame conversions read.
    pub fn grab(&mut self, band: Band) -> Result<(), String> {
        let row = self.geometry.width as usize * 4;
        // A band is in streamed rows; a scaled one reads the display rows under it and one
        // more each side, for the filter.
        let (y0, y1) = match self.scaled {
            None => (band.y0 as usize, band.y1 as usize),
            Some(_) => {
                let (gh, sh) = (self.geometry.height as usize, self.height as usize);
                (
                    (band.y0 as usize * gh / sh).saturating_sub(1),
                    ((band.y1 as usize * gh).div_ceil(sh) + 1).min(gh),
                )
            }
        };
        let (a, b) = (y0 * row, y1 * row);
        {
            let s = self.shared.lock();
            if s.buf.len() >= b && self.frame.len() >= b {
                self.frame[a..b].copy_from_slice(&s.buf[a..b]);
            }
        }
        self.scale_rows(band.y0, band.y1);
        Ok(())
    }

    /// Scale streamed rows [y0, y1) of `scaled` from `frame`; nothing when not scaled.
    fn scale_rows(&mut self, y0: u32, y1: u32) {
        let (w, h) = (self.width, self.height);
        let (gw, gh) = (self.geometry.width, self.geometry.height);
        let Some(scaled) = self.scaled.as_mut() else {
            return;
        };
        let y1 = y1.min(h);
        if y1 <= y0 || self.frame.len() < gw as usize * gh as usize * 4 {
            return;
        }
        unsafe {
            ARGBScaleClip(
                self.frame.as_ptr(),
                (gw * 4) as i32,
                gw as i32,
                gh as i32,
                scaled.as_mut_ptr(),
                (w * 4) as i32,
                w as i32,
                h as i32,
                0,
                y0 as i32,
                w as i32,
                (y1 - y0) as i32,
                FILTER_BOX,
            );
        }
    }

    /// Convert rows [y0, y1) of the current frame into `dst`.
    pub fn convert(&self, dst: &mut crate::convert::I420, y0: usize, y1: usize, threads: usize) {
        let (src, stride) = match &self.scaled {
            Some(scaled) => (scaled.as_slice(), self.width as usize * 4),
            None => (self.frame.as_slice(), self.geometry.width as usize * 4),
        };
        crate::convert::convert_band(
            src,
            stride,
            dst,
            y0,
            y1.min(self.height as usize),
            threads,
        );
    }

    /// The current frame as packed BGRX (`width * 4` bytes per row), for screenshots.
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        // The capture thread updates the image and its changed areas together under this
        // lock, so taking both under it leaves `changes` holding the damage for exactly
        // these pixels.
        let rects = {
            let mut s = self.shared.lock();
            let n = s.buf.len().min(self.frame.len());
            self.frame[..n].copy_from_slice(&s.buf[..n]);
            std::mem::take(&mut s.rects)
        };
        self.add_damage(rects);
        if self.scaled.is_some() {
            self.scale_rows(0, self.height);
            return Ok(self.scaled.clone().unwrap_or_default());
        }
        let (row, h) = (self.width as usize * 4, self.height as usize);
        let stride = self.geometry.width as usize * 4;
        let mut out = Vec::with_capacity(row * h);
        for line in self.frame.chunks_exact(stride).take(h) {
            out.extend_from_slice(&line[..row]);
        }
        Ok(out)
    }

    /// The current frame as BGRX (BGRA, alpha unused) and its stride: the scaled copy
    /// when the display streams smaller than it is.
    pub fn bgrx(&self) -> Option<(&[u8], usize)> {
        match &self.scaled {
            Some(scaled) => Some((scaled, self.width as usize * 4)),
            None => Some((&self.frame, self.geometry.width as usize * 4)),
        }
    }

    /// The display changed or duplication ended: capture must be set up again.
    pub fn needs_rebuild(&self) -> bool {
        self.rebuild
    }
}

impl Drop for Capturer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
