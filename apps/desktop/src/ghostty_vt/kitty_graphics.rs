//! Kitty graphics: the images a program draws inline in the terminal.
//!
//! libghostty-vt parses the protocol (transmit, place, delete, Unicode
//! placeholders), stores the images, answers the program's queries and
//! positions every placement in the render state. This file turns it on,
//! decodes PNG for it, and copies what is on screen into owned values the
//! renderer draws.

use super::*;
use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    fmt,
    sync::{Arc, Once},
};

/// CDXC:Terminal 2026-11-10 WHY: Ghostty's own `image-storage-limit` default (320 MB per screen). libghostty-vt's 10 MB library default rejects one large chart, and an agent CLI that prints diagrams (#536) fills it quickly.
pub const KITTY_IMAGE_STORAGE_LIMIT_BYTES: u64 = 320 * 1000 * 1000;

/// The longest side of a picture's texture. GPUI's Metal atlas tops out at 16384 px per side and a texture that size is 1 GB; a larger picture is scaled down once, and it still fills the cells the program asked for.
const KITTY_IMAGE_MAX_TEXTURE_SIDE: u32 = 4096;

/// Where a picture is drawn relative to the terminal's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VtImageLayer {
    /// Under the cell background colours (Kitty z below -1,073,741,824).
    BelowBackground,
    /// Over the cell backgrounds, under the text (negative z).
    BelowText,
    /// Over the text (z 0 and up).
    AboveText,
}

/// A picture's pixels, laid out for GPUI's sprite atlas: BGRA, 8 bits per channel, rows top to bottom. `width`/`height` are the texture's; `image_width`/`image_height` are the picture's own, which placements' source rectangles are measured in (they differ only when a picture was scaled down to fit a texture).
pub struct VtImagePixels {
    pub image_id: u32,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub bgra: Vec<u8>,
}

impl fmt::Debug for VtImagePixels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VtImagePixels")
            .field("image_id", &self.image_id)
            .field("generation", &self.generation)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// One rectangle of one picture to draw this frame. Positions are in cells of the viewport (the top-left cell can be above or left of it, so the renderer clips), offsets and sizes in the terminal's pixels (the cell size passed to `resize`).
#[derive(Clone, Debug)]
pub struct VtImage {
    pub layer: VtImageLayer,
    pub z: i32,
    pub viewport_col: i32,
    pub viewport_row: i32,
    pub cell_offset_x: u32,
    pub cell_offset_y: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub pixels: Arc<VtImagePixels>,
}

/// The pixels of the pictures on screen, kept between snapshots so each picture is read out of the terminal once per change.
#[derive(Default)]
pub(crate) struct VtImagePixelCache {
    entries: HashMap<u32, Arc<VtImagePixels>>,
}

impl VtTerminal {
    /// Turn the Kitty graphics protocol on with this storage limit, or off with zero. Off means the terminal ignores the protocol and answers no queries, so programs fall back to text; [`VtTerminal::new`] starts with it off.
    pub fn set_kitty_image_storage_limit(&mut self, bytes: u64) -> Result<(), VtError> {
        if bytes > 0 {
            install_png_decoder();
        }
        check(unsafe {
            ffi::ghostty_terminal_set(
                self.raw,
                ffi::GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT,
                std::ptr::from_ref(&bytes).cast(),
            )
        })
    }

    fn kitty_graphics(&mut self) -> Option<ffi::GhosttyKittyGraphics> {
        let mut graphics: ffi::GhosttyKittyGraphics = std::ptr::null_mut();
        let result = unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS,
                (&raw mut graphics).cast::<c_void>(),
            )
        };
        (result == ffi::GHOSTTY_SUCCESS && !graphics.is_null()).then_some(graphics)
    }

    /// Copy one stored picture's pixels out of the terminal (borrowed memory, so it happens under the caller's terminal lock). `None` when the picture is gone or its data has not all arrived.
    fn kitty_image_pixels(&mut self, image_id: u32) -> Option<VtImagePixels> {
        let graphics = self.kitty_graphics()?;
        let image = unsafe { ffi::ghostty_kitty_graphics_image(graphics, image_id) };
        if image.is_null() {
            return None;
        }
        let read_u32 = |data| {
            let mut value = 0_u32;
            let result = unsafe {
                ffi::ghostty_kitty_graphics_image_get(image, data, (&raw mut value).cast())
            };
            (result == ffi::GHOSTTY_SUCCESS).then_some(value)
        };
        let width = read_u32(ffi::GHOSTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH)?;
        let height = read_u32(ffi::GHOSTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT)?;
        let mut format: ffi::GhosttyKittyImageFormat = ffi::GHOSTTY_KITTY_IMAGE_FORMAT_RGBA;
        let mut generation = 0_u64;
        let mut pixels = ffi::GhosttyString {
            ptr: std::ptr::null(),
            len: 0,
        };
        let ok = unsafe {
            ffi::ghostty_kitty_graphics_image_get(
                image,
                ffi::GHOSTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT,
                (&raw mut format).cast(),
            ) == ffi::GHOSTTY_SUCCESS
                && ffi::ghostty_kitty_graphics_image_get(
                    image,
                    ffi::GHOSTTY_KITTY_GRAPHICS_IMAGE_DATA_GENERATION,
                    (&raw mut generation).cast(),
                ) == ffi::GHOSTTY_SUCCESS
                && ffi::ghostty_kitty_graphics_image_get(
                    image,
                    ffi::GHOSTTY_KITTY_GRAPHICS_IMAGE_DATA_PIXELS,
                    (&raw mut pixels).cast(),
                ) == ffi::GHOSTTY_SUCCESS
        };
        if !ok || pixels.ptr.is_null() || width == 0 || height == 0 {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(pixels.ptr, pixels.len) };
        let channels: usize = match format {
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_RGBA => 4,
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_RGB => 3,
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA => 2,
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_GRAY => 1,
            _ => return None,
        };
        let pixel_count = width as usize * height as usize;
        if bytes.len() < pixel_count * channels {
            return None;
        }
        let mut bgra = Vec::with_capacity(pixel_count * 4);
        for pixel in bytes[..pixel_count * channels].chunks_exact(channels) {
            let (r, g, b, a) = match channels {
                4 => (pixel[0], pixel[1], pixel[2], pixel[3]),
                3 => (pixel[0], pixel[1], pixel[2], u8::MAX),
                2 => (pixel[0], pixel[0], pixel[0], pixel[1]),
                _ => (pixel[0], pixel[0], pixel[0], u8::MAX),
            };
            bgra.extend_from_slice(&[b, g, r, a]);
        }
        let (texture_width, texture_height, bgra) = fit_texture(width, height, bgra);
        Some(VtImagePixels {
            image_id,
            generation,
            width: texture_width,
            height: texture_height,
            image_width: width,
            image_height: height,
            bgra,
        })
    }
}

/// Scale a picture down so its longer side fits [`KITTY_IMAGE_MAX_TEXTURE_SIDE`].
fn fit_texture(width: u32, height: u32, bgra: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let longest = width.max(height);
    if longest <= KITTY_IMAGE_MAX_TEXTURE_SIDE {
        return (width, height, bgra);
    }
    let scale = f64::from(KITTY_IMAGE_MAX_TEXTURE_SIDE) / f64::from(longest);
    let new_width = ((f64::from(width) * scale).round() as u32).max(1);
    let new_height = ((f64::from(height) * scale).round() as u32).max(1);
    let Some(buffer) = image::RgbaImage::from_raw(width, height, bgra) else {
        return (0, 0, Vec::new());
    };
    // The filter treats the four channels alike, so BGRA scales as RGBA does.
    let scaled = image::imageops::resize(
        &buffer,
        new_width,
        new_height,
        image::imageops::FilterType::Triangle,
    );
    (new_width, new_height, scaled.into_raw())
}

impl VtRenderState {
    /// The pictures on screen as of the last [`update`](Self::update), with their pixels. Call it right after the update while still holding the terminal (pixels are read from it, and only for pictures that are new or changed since the last call). A picture whose pixels can't be read yet is left out of this frame.
    pub fn images(&mut self, terminal: &mut VtTerminal) -> Result<Vec<VtImage>, VtError> {
        if self.image_iter.is_null() {
            check(unsafe {
                ffi::ghostty_render_state_image_iterator_new(
                    std::ptr::null(),
                    &raw mut self.image_iter,
                )
            })?;
        }
        let mut image_iter = self.image_iter;
        check(unsafe {
            ffi::ghostty_render_state_get(
                self.raw,
                ffi::GHOSTTY_RENDER_STATE_DATA_IMAGE_ITERATOR,
                (&raw mut image_iter).cast::<c_void>(),
            )
        })?;

        let mut images = Vec::new();
        let mut seen: HashSet<u32> = HashSet::new();
        while unsafe { ffi::ghostty_render_state_image_iterator_next(image_iter) } {
            let read = |data: ffi::GhosttyRenderStateImageData, out: *mut c_void| {
                check(unsafe { ffi::ghostty_render_state_image_get(image_iter, data, out) })
            };
            let mut protocol: ffi::GhosttyImageProtocol = -1;
            read(
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_PROTOCOL,
                (&raw mut protocol).cast(),
            )?;
            if protocol != ffi::GHOSTTY_IMAGE_PROTOCOL_KITTY {
                continue;
            }
            let mut image_id = 0_u32;
            let mut generation = 0_u64;
            let mut layer: ffi::GhosttyImageLayer = ffi::GHOSTTY_IMAGE_LAYER_ABOVE_TEXT;
            let mut z = 0_i32;
            let mut viewport_col = 0_i32;
            let mut viewport_row = 0_i32;
            let mut values = [0_u32; 8];
            read(
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_IMAGE_ID,
                (&raw mut image_id).cast(),
            )?;
            read(
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_IMAGE_GENERATION,
                (&raw mut generation).cast(),
            )?;
            read(
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_LAYER,
                (&raw mut layer).cast(),
            )?;
            read(ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_Z, (&raw mut z).cast())?;
            read(
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_COL,
                (&raw mut viewport_col).cast(),
            )?;
            read(
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_ROW,
                (&raw mut viewport_row).cast(),
            )?;
            for (slot, data) in values.iter_mut().zip([
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_X,
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_Y,
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_WIDTH,
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_HEIGHT,
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_X,
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_Y,
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_WIDTH,
                ffi::GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_HEIGHT,
            ]) {
                read(data, std::ptr::from_mut(slot).cast())?;
            }
            let [
                cell_offset_x,
                cell_offset_y,
                pixel_width,
                pixel_height,
                source_x,
                source_y,
                source_width,
                source_height,
            ] = values;
            if pixel_width == 0 || pixel_height == 0 || source_width == 0 || source_height == 0 {
                continue;
            }

            let cached = self
                .image_pixels
                .entries
                .get(&image_id)
                .filter(|pixels| pixels.generation == generation)
                .cloned();
            let pixels = match cached {
                Some(pixels) => pixels,
                None => {
                    let Some(pixels) = terminal.kitty_image_pixels(image_id) else {
                        continue;
                    };
                    let pixels = Arc::new(pixels);
                    self.image_pixels
                        .entries
                        .insert(image_id, Arc::clone(&pixels));
                    pixels
                }
            };
            seen.insert(image_id);
            images.push(VtImage {
                layer: match layer {
                    ffi::GHOSTTY_IMAGE_LAYER_BELOW_BACKGROUND => VtImageLayer::BelowBackground,
                    ffi::GHOSTTY_IMAGE_LAYER_BELOW_TEXT => VtImageLayer::BelowText,
                    _ => VtImageLayer::AboveText,
                },
                z,
                viewport_col,
                viewport_row,
                cell_offset_x,
                cell_offset_y,
                pixel_width,
                pixel_height,
                source_x,
                source_y,
                source_width,
                source_height,
                pixels,
            });
        }
        // Pictures that left the screen drop their pixels; scrolling back to one reads it again.
        self.image_pixels
            .entries
            .retain(|image_id, _| seen.contains(image_id));
        images.sort_by_key(|image| (image.layer, image.z));
        Ok(images)
    }
}

/// libghostty-vt has no PNG decoder of its own and rejects PNG pictures (`f=100`, what most tools send) until the host installs one. Installed once per process, before the first terminal turns graphics on.
fn install_png_decoder() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let decode: ffi::GhosttySysDecodePngFn = decode_png;
        unsafe {
            ffi::ghostty_sys_set(ffi::GHOSTTY_SYS_OPT_DECODE_PNG, decode as *const c_void);
        }
    });
}

/// Same bound as the protocol's largest picture side (10,000 px) times four bytes; the library's own allocator limit still applies on top.
const PNG_DECODE_MAX_BYTES: u64 = 10_000 * 10_000 * 4;

unsafe extern "C" fn decode_png(
    _userdata: *mut c_void,
    allocator: *const c_void,
    data: *const u8,
    data_len: usize,
    out: *mut ffi::GhosttySysImage,
) -> bool {
    if data.is_null() || out.is_null() {
        return false;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, data_len) };
    let decoded = std::panic::catch_unwind(|| {
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(PNG_DECODE_MAX_BYTES);
        reader.limits(limits);
        reader.decode().ok().map(image::DynamicImage::into_rgba8)
    });
    let Ok(Some(rgba)) = decoded else {
        return false;
    };
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();
    if pixels.is_empty() {
        return false;
    }
    let buffer = unsafe { ffi::ghostty_alloc(allocator, pixels.len()) };
    if buffer.is_null() {
        return false;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), buffer, pixels.len());
        *out = ffi::GhosttySysImage {
            width,
            height,
            data: buffer,
            data_len: pixels.len(),
        };
    }
    true
}
