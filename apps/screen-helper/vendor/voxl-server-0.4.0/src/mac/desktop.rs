//! Cursor shape tracking and two-way clipboard sync (text and PNG images) on macOS.
//! Clients draw the cursor themselves from the images sent here, as on Linux.

use super::ffi::*;
use crate::hub::{Hub, Out};
use bytes::Bytes;
use objc2::rc::Retained;
#[allow(deprecated)]
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSCursor, NSPasteboard, NSPasteboardTypePNG,
    NSPasteboardTypeString, NSPasteboardTypeTIFF,
};
use objc2_foundation::{NSData, NSDictionary, NSRect, NSSize, NSString};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

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
    /// Pasteboard change count after our own last write (not echoed back to clients).
    own_change: isize,
    last_image: Option<u64>,
}

/// Sets the Mac's clipboard; returns once it holds the new contents, so a paste
/// shortcut injected right afterwards pastes them.
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
        let pb = NSPasteboard::generalPasteboard();
        let mut st = self.state.lock();
        pb.clearContents();
        match content {
            ClipContent::Text(t) => {
                unsafe { pb.setString_forType(&NSString::from_str(&t), NSPasteboardTypeString) };
                *self.hub.last_clipboard.lock() = Some(t);
            }
            ClipContent::Image(png) => {
                st.last_image = Some(hash(&png));
                let data = NSData::with_bytes(&png);
                unsafe { pb.setData_forType(Some(&data), NSPasteboardTypePNG) };
                // Many Mac apps only read TIFF images from the pasteboard.
                if let Some(tiff) = NSBitmapImageRep::imageRepWithData(&data)
                    .and_then(|rep| rep.TIFFRepresentation())
                {
                    unsafe { pb.setData_forType(Some(&tiff), NSPasteboardTypeTIFF) };
                }
            }
        }
        st.own_change = pb.changeCount();
    }
}

fn hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
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
    if !super::show_cursor() {
        let h = hub.clone();
        std::thread::Builder::new()
            .name("cursor".into())
            .spawn(move || cursor_loop(h))
            .unwrap();
    }
    if mode.to_client {
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || clipboard_loop(hub, state))
            .unwrap();
    }
    Ok(handle)
}

/// Whether the system cursor can be read; if not, capture draws it into the video.
#[allow(deprecated)]
pub fn cursor_readable() -> bool {
    NSCursor::currentSystemCursor().is_some()
}

fn clipboard_loop(hub: Arc<Hub>, state: Arc<Mutex<ClipState>>) {
    let pb = NSPasteboard::generalPasteboard();
    let mut seen = pb.changeCount();
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let n = pb.changeCount();
        if n == seen {
            continue;
        }
        seen = n;
        if state.lock().own_change == n {
            continue;
        }
        objc2::rc::autoreleasepool(|_| unsafe {
            if let Some(s) = pb.stringForType(NSPasteboardTypeString) {
                let text = s.to_string();
                if text.is_empty() {
                    return;
                }
                {
                    let mut last = hub.last_clipboard.lock();
                    if last.as_deref() == Some(text.as_str()) {
                        return;
                    }
                    *last = Some(text.clone());
                }
                let msg = serde_json::json!({"t": "clip", "d": text}).to_string();
                hub.broadcast(|| Out::Text(msg.clone()));
                return;
            }
            let png = pb.dataForType(NSPasteboardTypePNG).or_else(|| {
                let tiff = pb.dataForType(NSPasteboardTypeTIFF)?;
                NSBitmapImageRep::imageRepWithData(&tiff)?.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            });
            let Some(png) = png else {
                return;
            };
            let data = png.to_vec();
            if data.is_empty() || data.len() > MAX_IMAGE {
                return;
            }
            let h = hash(&data);
            {
                let mut st = state.lock();
                if st.last_image == Some(h) {
                    return;
                }
                st.last_image = Some(h);
            }
            let mut msg = Vec::with_capacity(data.len() + 1);
            msg.push(0x04);
            msg.extend_from_slice(&data);
            let b = Bytes::from(msg);
            hub.broadcast(|| Out::Bin(b.clone()));
        });
    }
}

/// Polls the system cursor and sends its image whenever the shape changes.
#[allow(deprecated)]
fn cursor_loop(hub: Arc<Hub>) {
    let mut last: Option<u64> = None;
    let mut serial = 0u32;
    loop {
        std::thread::sleep(Duration::from_millis(33));
        if hub.clients.lock().is_empty() {
            continue;
        }
        let scale = super::GEOMETRY.lock().map(|g| g.scale()).unwrap_or(1.0);
        objc2::rc::autoreleasepool(|_| {
            let Some(cursor) = NSCursor::currentSystemCursor() else {
                return;
            };
            let image = cursor.image();
            let hot = cursor.hotSpot();
            let Some((w, h, rgba)) = render(&image, scale) else {
                return;
            };
            let key = hash(&rgba) ^ ((hot.x * scale) as u64) << 32 ^ (hot.y * scale) as u64;
            if last == Some(key) {
                return;
            }
            last = Some(key);
            serial = serial.wrapping_add(1);
            let mut msg = Vec::with_capacity(14 + rgba.len());
            msg.push(0x03);
            msg.push(0);
            msg.extend_from_slice(&(w as u16).to_be_bytes());
            msg.extend_from_slice(&(h as u16).to_be_bytes());
            msg.extend_from_slice(&((hot.x * scale).round() as u16).to_be_bytes());
            msg.extend_from_slice(&((hot.y * scale).round() as u16).to_be_bytes());
            msg.extend_from_slice(&serial.to_be_bytes());
            msg.extend_from_slice(&rgba);
            let b = Bytes::from(msg);
            *hub.cursor.lock() = Some(b.clone());
            hub.broadcast(|| Out::Bin(b.clone()));
        });
    }
}

/// The image at `scale` pixels per point, as straight (not premultiplied) RGBA.
fn render(image: &objc2_app_kit::NSImage, scale: f64) -> Option<(usize, usize, Vec<u8>)> {
    let size: NSSize = image.size();
    let (w, h) = (
        (size.width * scale).round().max(1.0) as usize,
        (size.height * scale).round().max(1.0) as usize,
    );
    if w > 256 || h > 256 {
        return None;
    }
    let mut rect = NSRect::new(
        objc2_foundation::NSPoint::new(0.0, 0.0),
        NSSize::new(w as f64, h as f64),
    );
    let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
    let mut px = vec![0u8; w * h * 4];
    unsafe {
        let cs = CGColorSpaceCreateDeviceRGB();
        let ctx = CGBitmapContextCreate(
            px.as_mut_ptr() as *mut _,
            w,
            h,
            8,
            w * 4,
            cs,
            kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big,
        );
        CGColorSpaceRelease(cs);
        if ctx.is_null() {
            return None;
        }
        CGContextDrawImage(
            ctx,
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: w as f64,
                    height: h as f64,
                },
            },
            Retained::as_ptr(&cg) as CGImageRef,
        );
        CGContextRelease(ctx);
    }
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some((w, h, px))
}
