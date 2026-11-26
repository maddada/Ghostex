//! Cursor shape tracking and two-way clipboard sync (text and images) on Windows.
//! Clients draw the cursor themselves from the images sent here, as on Linux.

use crate::hub::{Hub, Out};
use bytes::Bytes;
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, HICON, ICONINFO,
};

/// Clipboard contents exchanged with clients.
pub enum ClipContent {
    Text(String),
    /// PNG bytes.
    Image(Vec<u8>),
}

pub struct ClipboardMode {
    pub to_server: bool,
    pub to_client: bool,
}

/// Largest image moved through the clipboard.
const MAX_IMAGE: usize = 15 << 20;

#[derive(Default)]
struct ClipState {
    /// Clipboard sequence number after our own last write (not echoed back to clients).
    own_change: u32,
    last_image: Option<u64>,
}

/// The "PNG" clipboard format browsers and Office use for images with transparency.
fn png_format() -> u32 {
    let name = super::wide("PNG");
    unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) }
}

/// The clipboard, open for as long as this lives. Another app may hold it for a
/// moment, so opening retries briefly.
struct Open;

impl Open {
    fn new() -> Option<Self> {
        for _ in 0..20 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Some(Open);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// Puts `data` on the open clipboard as `format`.
unsafe fn put(format: u32, data: &[u8]) -> bool {
    let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) else {
        return false;
    };
    let p = GlobalLock(mem) as *mut u8;
    if p.is_null() {
        return false;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
    let _ = GlobalUnlock(mem);
    // The clipboard owns the memory from here on.
    SetClipboardData(format, Some(HANDLE(mem.0))).is_ok()
}

/// Bytes of `format` on the open clipboard.
unsafe fn get(format: u32) -> Option<Vec<u8>> {
    let h = GetClipboardData(format).ok()?;
    let mem = HGLOBAL(h.0);
    let p = GlobalLock(mem) as *const u8;
    if p.is_null() {
        return None;
    }
    let v = std::slice::from_raw_parts(p, GlobalSize(mem)).to_vec();
    let _ = GlobalUnlock(mem);
    Some(v)
}

/// Sets the clipboard; returns once it holds the new contents, so a paste shortcut
/// injected right afterwards pastes them.
#[derive(Clone)]
pub struct ClipboardHandle {
    hub: Arc<Hub>,
    state: Arc<Mutex<ClipState>>,
    to_server: bool,
}

impl ClipboardHandle {
    pub fn set_blocking(&self, content: ClipContent) {
        if !self.to_server {
            return;
        }
        let mut st = self.state.lock();
        let Some(_open) = Open::new() else {
            log::warn!("clipboard busy; paste dropped");
            return;
        };
        unsafe {
            let _ = EmptyClipboard();
            match content {
                ClipContent::Text(t) => {
                    let w: Vec<u8> = super::wide(&t)
                        .iter()
                        .flat_map(|u| u.to_le_bytes())
                        .collect();
                    put(CF_UNICODETEXT.0 as u32, &w);
                    *self.hub.last_clipboard.lock() = Some(t);
                }
                ClipContent::Image(png) => {
                    st.last_image = Some(hash(&png));
                    put(png_format(), &png);
                    // Most apps read images as a DIB.
                    if let Some(dib) = png_to_dib(&png) {
                        put(CF_DIB.0 as u32, &dib);
                    }
                }
            }
        }
        st.own_change = unsafe { GetClipboardSequenceNumber() };
    }
}

fn hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

/// A PNG as a bottom-up 32-bit DIB (BITMAPINFOHEADER and BGRA rows).
fn png_to_dib(png: &[u8]) -> Option<Vec<u8>> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        _ => return None,
    };
    let hdr = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        biHeight: h as i32,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: (w * h * 4) as u32,
        ..Default::default()
    };
    let mut out = Vec::with_capacity(40 + w * h * 4);
    out.extend_from_slice(unsafe {
        std::slice::from_raw_parts(&hdr as *const _ as *const u8, std::mem::size_of_val(&hdr))
    });
    for y in (0..h).rev() {
        for x in 0..w {
            let p = &buf[(y * w + x) * ch..][..ch];
            let (r, g, b, a) = match ch {
                4 => (p[0], p[1], p[2], p[3]),
                3 => (p[0], p[1], p[2], 255),
                2 => (p[0], p[0], p[0], p[1]),
                _ => (p[0], p[0], p[0], 255),
            };
            out.extend_from_slice(&[b, g, r, a]);
        }
    }
    Some(out)
}

/// RGBA pixels as PNG.
fn encode_png(w: usize, h: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    enc.write_header().ok()?.write_image_data(rgba).ok()?;
    Some(out)
}

/// A clipboard DIB (24 or 32 bits per pixel, uncompressed or with bit fields) as PNG.
fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    let le32 = |o: usize| Some(u32::from_le_bytes(dib.get(o..o + 4)?.try_into().ok()?));
    let size = le32(0)? as usize;
    let w = le32(4)? as i32;
    let h = le32(8)? as i32;
    let bpp = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let comp = le32(16)?;
    let colors = le32(32)? as usize;
    if w <= 0 || h == 0 || !(bpp == 24 || bpp == 32) || !(comp == 0 || comp == 3) {
        return None;
    }
    let (w, top_down, h) = (w as usize, h < 0, h.unsigned_abs() as usize);
    if w * h > 64 << 20 {
        return None;
    }
    // Bit-field masks follow a plain BITMAPINFOHEADER; palettes are unused at 24/32 bpp.
    let mut off = size + colors * 4;
    if comp == 3 && size == 40 {
        off += 12;
    }
    let stride = (w * bpp as usize / 8).div_ceil(4) * 4;
    let px = dib.get(off..off + stride * h)?;
    let mut rgba = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        let src = &px[(if top_down { y } else { h - 1 - y }) * stride..];
        for x in 0..w {
            let s = &src[x * bpp as usize / 8..];
            let a = if bpp == 32 { s[3] } else { 255 };
            any_alpha |= bpp == 32 && a != 0;
            rgba[(y * w + x) * 4..][..4].copy_from_slice(&[s[2], s[1], s[0], a]);
        }
    }
    // Most 32-bit DIBs leave alpha at zero: they are opaque.
    if bpp == 32 && !any_alpha {
        for p in rgba.chunks_exact_mut(4) {
            p[3] = 255;
        }
    }
    encode_png(w, h, &rgba)
}

pub fn spawn(
    _display: Option<String>,
    hub: Arc<Hub>,
    mode: ClipboardMode,
) -> Result<ClipboardHandle, String> {
    let state = Arc::new(Mutex::new(ClipState::default()));
    let handle = ClipboardHandle {
        hub: hub.clone(),
        state: state.clone(),
        to_server: mode.to_server,
    };
    let h = hub.clone();
    std::thread::Builder::new()
        .name("cursor".into())
        .spawn(move || cursor_loop(h))
        .unwrap();
    if mode.to_client {
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || clipboard_loop(hub, state))
            .unwrap();
    }
    Ok(handle)
}

fn clipboard_loop(hub: Arc<Hub>, state: Arc<Mutex<ClipState>>) {
    let mut seen = unsafe { GetClipboardSequenceNumber() };
    let png = png_format();
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let n = unsafe { GetClipboardSequenceNumber() };
        if n == seen {
            continue;
        }
        seen = n;
        if state.lock().own_change == n {
            continue;
        }
        let Some(open) = Open::new() else {
            continue;
        };
        let has = |f: u32| unsafe { IsClipboardFormatAvailable(f) }.is_ok();
        if has(CF_UNICODETEXT.0 as u32) {
            let Some(raw) = (unsafe { get(CF_UNICODETEXT.0 as u32) }) else {
                continue;
            };
            drop(open);
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let text = super::from_wide(&units);
            if text.is_empty() {
                continue;
            }
            {
                let mut last = hub.last_clipboard.lock();
                if last.as_deref() == Some(text.as_str()) {
                    continue;
                }
                *last = Some(text.clone());
            }
            let msg = serde_json::json!({"t": "clip", "d": text}).to_string();
            hub.broadcast(|| Out::Text(msg.clone()));
            continue;
        }
        let data = if has(png) {
            unsafe { get(png) }
        } else if has(CF_DIB.0 as u32) {
            let dib = unsafe { get(CF_DIB.0 as u32) };
            drop(open);
            dib.and_then(|d| dib_to_png(&d))
        } else {
            None
        };
        let Some(data) = data else {
            continue;
        };
        if data.is_empty() || data.len() > MAX_IMAGE {
            continue;
        }
        let h = hash(&data);
        {
            let mut st = state.lock();
            if st.last_image == Some(h) {
                continue;
            }
            st.last_image = Some(h);
        }
        let mut msg = Vec::with_capacity(data.len() + 1);
        msg.push(0x04);
        msg.extend_from_slice(&data);
        let b = Bytes::from(msg);
        hub.broadcast(|| Out::Bin(b.clone()));
    }
}

/// Rows of `bm` as 32-bit top-down pixels (BGRA), or None.
unsafe fn bitmap_pixels(
    bm: windows::Win32::Graphics::Gdi::HBITMAP,
) -> Option<(usize, usize, Vec<u8>)> {
    let mut b = BITMAP::default();
    if GetObjectW(
        HGDIOBJ(bm.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut b as *mut _ as *mut c_void),
    ) == 0
    {
        return None;
    }
    let (w, h) = (b.bmWidth as usize, b.bmHeight as usize);
    if w == 0 || h == 0 || w > 512 || h > 1024 {
        return None;
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut px = vec![0u8; w * h * 4];
    let dc = GetDC(None);
    let n = GetDIBits(
        dc,
        bm,
        0,
        h as u32,
        Some(px.as_mut_ptr() as *mut c_void),
        &mut info,
        DIB_RGB_COLORS,
    );
    ReleaseDC(None, dc);
    (n > 0).then_some((w, h, px))
}

/// The cursor's image as straight RGBA, with its hotspot.
fn cursor_image(c: HICON) -> Option<(usize, usize, u32, u32, Vec<u8>)> {
    let mut ii = ICONINFO::default();
    unsafe { GetIconInfo(c, &mut ii) }.ok()?;
    let mask = unsafe { bitmap_pixels(ii.hbmMask) };
    let color = (!ii.hbmColor.is_invalid())
        .then(|| unsafe { bitmap_pixels(ii.hbmColor) })
        .flatten();
    unsafe {
        let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
        if !ii.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
        }
    }
    let (hx, hy) = (ii.xHotspot, ii.yHotspot);
    if let Some((w, h, px)) = color {
        // A colour cursor: premultiplied alpha, or (old style) opaque where the AND
        // mask is clear.
        let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
        let mut rgba = vec![0u8; w * h * 4];
        for i in 0..w * h {
            let p = &px[i * 4..][..4];
            let a = if has_alpha {
                p[3]
            } else {
                match &mask {
                    Some((mw, mh, m)) if *mw == w && *mh >= h && m[i * 4] == 0 => 255,
                    Some(_) => 0,
                    None => 255,
                }
            };
            let un = |c: u8| {
                if a == 0 {
                    0
                } else if has_alpha {
                    ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8
                } else {
                    c
                }
            };
            rgba[i * 4..][..4].copy_from_slice(&[un(p[2]), un(p[1]), un(p[0]), a]);
        }
        return Some((w, h, hx, hy, rgba));
    }
    // Monochrome: the mask holds the AND image over the XOR image. Pixels that invert
    // the screen (the text cursor) become black with a white outline, so they show on
    // any background.
    let (w, h2, m) = mask?;
    let h = h2 / 2;
    let and = |x: usize, y: usize| m[(y * w + x) * 4] != 0;
    let xor = |x: usize, y: usize| m[((y + h) * w + x) * 4] != 0;
    let mut rgba = vec![0u8; w * h * 4];
    let mut invert = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let px = &mut rgba[(y * w + x) * 4..][..4];
            match (and(x, y), xor(x, y)) {
                (false, false) => px.copy_from_slice(&[0, 0, 0, 255]),
                (false, true) => px.copy_from_slice(&[255, 255, 255, 255]),
                (true, true) => {
                    px.copy_from_slice(&[0, 0, 0, 255]);
                    invert[y * w + x] = true;
                }
                (true, false) => {}
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            if rgba[(y * w + x) * 4 + 3] != 0 {
                continue;
            }
            let near = (y.saturating_sub(1)..(y + 2).min(h))
                .any(|ny| (x.saturating_sub(1)..(x + 2).min(w)).any(|nx| invert[ny * w + nx]));
            if near {
                rgba[(y * w + x) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    Some((w, h, hx, hy, rgba))
}

/// Polls the cursor and sends its image whenever the shape changes.
fn cursor_loop(hub: Arc<Hub>) {
    super::follow_input_desktop();
    let mut last: Option<(isize, bool)> = None;
    let mut serial = 0u32;
    loop {
        std::thread::sleep(Duration::from_millis(16));
        if hub.clients.lock().is_empty() {
            last = None;
            continue;
        }
        let mut ci = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetCursorInfo(&mut ci) }.is_err() {
            continue;
        }
        let shown = ci.flags.0 & CURSOR_SHOWING.0 != 0 && !ci.hCursor.is_invalid();
        let key = (ci.hCursor.0 as isize, shown);
        if last == Some(key) {
            continue;
        }
        last = Some(key);
        // A hidden cursor is a transparent pixel.
        let (w, h, hx, hy, rgba) = if shown {
            match cursor_image(HICON(ci.hCursor.0)) {
                Some(c) => c,
                None => continue,
            }
        } else {
            (1, 1, 0, 0, vec![0; 4])
        };
        serial = serial.wrapping_add(1);
        let mut msg = Vec::with_capacity(14 + rgba.len());
        msg.push(0x03);
        msg.push(0);
        msg.extend_from_slice(&(w as u16).to_be_bytes());
        msg.extend_from_slice(&(h as u16).to_be_bytes());
        msg.extend_from_slice(&(hx as u16).to_be_bytes());
        msg.extend_from_slice(&(hy as u16).to_be_bytes());
        msg.extend_from_slice(&serial.to_be_bytes());
        msg.extend_from_slice(&rgba);
        let b = Bytes::from(msg);
        *hub.cursor.lock() = Some(b.clone());
        hub.broadcast(|| Out::Bin(b.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_round_trip() {
        let rgba: Vec<u8> = (0..4 * 3 * 2).map(|i| (i * 10) as u8).collect();
        let png = encode_png(3, 2, &rgba).unwrap();
        let dib = png_to_dib(&png).unwrap();
        let back = dib_to_png(&dib).unwrap();
        let mut dec = png::Decoder::new(std::io::Cursor::new(back))
            .read_info()
            .unwrap();
        let mut buf = vec![0; dec.output_buffer_size().unwrap()];
        dec.next_frame(&mut buf).unwrap();
        assert_eq!(buf, rgba);
    }
}
