use super::*;
use std::sync::Arc;
#[cfg(not(target_family = "wasm"))]
use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    sync::OnceLock,
};

// Explicit native renderer policy, enforced by the parser before it accepts
// an image. This is a resource budget, not a queried GPU texture limit.
#[cfg(not(target_family = "wasm"))]
const MAX_IMAGE_DIMENSION: u32 = 4096;
#[cfg(not(target_family = "wasm"))]
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
#[cfg(not(target_family = "wasm"))]
const MAX_IMAGE_PIXELS: u64 = (MAX_IMAGE_BYTES / 4) as u64;
// A 16-bit RGBA PNG needs twice the final RGBA8 pixel storage during decode.
#[cfg(not(target_family = "wasm"))]
const MAX_PNG_DECODE_BYTES: u64 = (MAX_IMAGE_BYTES * 2) as u64;

/// Owned pixels from one image generation. The parser can replace or delete
/// its storage without invalidating a frame already handed to the renderer.
#[derive(Debug)]
pub struct VtImage {
    pub id: u32,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Resolved geometry in viewport cells and device pixels; no terminal pins
/// or borrowed pixel pointers cross the snapshot boundary.
#[derive(Clone, Debug)]
pub struct VtImagePlacement {
    pub image: Arc<VtImage>,
    pub col: i32,
    pub row: i32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub width: u32,
    pub height: u32,
    pub source_x: f64,
    pub source_y: f64,
    pub source_width: f64,
    pub source_height: f64,
    pub z: i32,
}

/// The iterator is owned; its borrowed entries are consumed only while the
/// caller holds exclusive access to VtTerminal. Image generations are unique
/// across terminals and primary/alternate screens, including same-size edits.
#[cfg(not(target_family = "wasm"))]
pub(crate) struct VtImageState {
    iterator: ffi::GhosttyKittyGraphicsPlacementIterator,
    images: HashMap<u32, Arc<VtImage>>,
    virtual_buffer: Vec<ffi::GhosttyKittyGraphicsVirtualPlacement>,
}

#[cfg(not(target_family = "wasm"))]
impl VtImageState {
    pub(crate) fn configure(terminal: ffi::GhosttyTerminal) -> Result<(), VtError> {
        check(unsafe {
            ffi::ghostty_terminal_set(
                terminal,
                ffi::GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MAX_DIMENSION,
                std::ptr::from_ref(&MAX_IMAGE_DIMENSION).cast(),
            )
        })?;
        check(unsafe {
            ffi::ghostty_terminal_set(
                terminal,
                ffi::GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MAX_PIXELS,
                std::ptr::from_ref(&MAX_IMAGE_PIXELS).cast(),
            )
        })
    }

    pub(crate) fn new() -> Result<Self, VtError> {
        static DECODER: OnceLock<Result<(), VtError>> = OnceLock::new();
        (*DECODER.get_or_init(|| {
            check(unsafe {
                ffi::ghostty_sys_set(ffi::GHOSTTY_SYS_OPT_DECODE_PNG, decode_png as *const c_void)
            })
        }))?;
        let mut iterator = std::ptr::null_mut();
        check(unsafe {
            ffi::ghostty_kitty_graphics_placement_iterator_new(std::ptr::null(), &mut iterator)
        })?;
        Ok(Self {
            iterator,
            images: HashMap::new(),
            virtual_buffer: Vec::new(),
        })
    }

    pub(crate) fn snapshot(
        &mut self,
        terminal: ffi::GhosttyTerminal,
    ) -> Result<Vec<VtImagePlacement>, VtError> {
        let mut graphics = std::ptr::null_mut();
        check(unsafe {
            ffi::ghostty_terminal_get(
                terminal,
                ffi::GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS,
                (&mut graphics as *mut ffi::GhosttyKittyGraphics).cast(),
            )
        })?;
        let mut placements = Vec::new();
        let mut used = HashSet::new();
        let mut has_virtual = false;
        check(unsafe {
            ffi::ghostty_kitty_graphics_get(
                graphics,
                ffi::GHOSTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
                (&mut self.iterator as *mut ffi::GhosttyKittyGraphicsPlacementIterator).cast(),
            )
        })?;
        while unsafe { ffi::ghostty_kitty_graphics_placement_next(self.iterator) } {
            let mut id = 0u32;
            let mut virtual_ = false;
            let mut z = 0i32;
            let mut offset_x = 0u32;
            let mut offset_y = 0u32;
            for (key, out) in [
                (
                    ffi::GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
                    (&mut id as *mut u32).cast(),
                ),
                (
                    ffi::GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL,
                    (&mut virtual_ as *mut bool).cast(),
                ),
                (
                    ffi::GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET,
                    (&mut offset_x as *mut u32).cast(),
                ),
                (
                    ffi::GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET,
                    (&mut offset_y as *mut u32).cast(),
                ),
                (
                    ffi::GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z,
                    (&mut z as *mut i32).cast(),
                ),
            ] {
                check(unsafe {
                    ffi::ghostty_kitty_graphics_placement_get(self.iterator, key, out)
                })?;
            }
            if virtual_ {
                has_virtual = true;
                continue;
            }
            let handle = unsafe { ffi::ghostty_kitty_graphics_image(graphics, id) };
            if handle.is_null() {
                continue;
            }
            let mut info = ffi::GhosttyKittyGraphicsPlacementRenderInfo::default();
            check(unsafe {
                ffi::ghostty_kitty_graphics_placement_render_info(
                    self.iterator,
                    handle,
                    terminal,
                    &mut info,
                )
            })?;
            if !info.viewport_visible || info.pixel_width == 0 || info.pixel_height == 0 {
                continue;
            }
            if let Some(image) = self.image(graphics, id)? {
                used.insert(id);
                placements.push(VtImagePlacement {
                    image,
                    col: info.viewport_col,
                    row: info.viewport_row,
                    offset_x,
                    offset_y,
                    width: info.pixel_width,
                    height: info.pixel_height,
                    source_x: info.source_x.into(),
                    source_y: info.source_y.into(),
                    source_width: info.source_width.into(),
                    source_height: info.source_height.into(),
                    z,
                });
            }
        }
        if has_virtual {
            let mut len = 0;
            loop {
                let result = unsafe {
                    ffi::ghostty_kitty_graphics_virtual_placements(
                        terminal,
                        self.virtual_buffer.as_mut_ptr(),
                        self.virtual_buffer.len(),
                        &mut len,
                    )
                };
                if result != ffi::GHOSTTY_OUT_OF_SPACE {
                    check(result)?;
                    break;
                }
                self.virtual_buffer
                    .resize(len, ffi::GhosttyKittyGraphicsVirtualPlacement::default());
            }
            for index in 0..len {
                let p = self.virtual_buffer[index];
                if let Some(image) = self.image(graphics, p.image_id)? {
                    used.insert(p.image_id);
                    placements.push(VtImagePlacement {
                        image,
                        col: p.viewport_col.into(),
                        row: p.viewport_row.into(),
                        offset_x: p.offset_x,
                        offset_y: p.offset_y,
                        width: p.dest_width,
                        height: p.dest_height,
                        source_x: p.source_x,
                        source_y: p.source_y,
                        source_width: p.source_width,
                        source_height: p.source_height,
                        z: -1,
                    });
                }
            }
        }
        self.images.retain(|id, _| used.contains(id));
        // Match Ghostty's tie-breaker for equal z values.
        placements.sort_by_key(|p| (p.z, p.image.id));
        Ok(placements)
    }

    fn image(
        &mut self,
        graphics: ffi::GhosttyKittyGraphics,
        id: u32,
    ) -> Result<Option<Arc<VtImage>>, VtError> {
        let handle = unsafe { ffi::ghostty_kitty_graphics_image(graphics, id) };
        if handle.is_null() {
            return Ok(None);
        }
        let get =
            |key, out| check(unsafe { ffi::ghostty_kitty_graphics_image_get(handle, key, out) });
        let mut generation = 0u64;
        get(
            ffi::GHOSTTY_KITTY_IMAGE_DATA_GENERATION,
            (&mut generation as *mut u64).cast(),
        )?;
        if let Some(image) = self
            .images
            .get(&id)
            .filter(|image| image.generation == generation)
        {
            return Ok(Some(Arc::clone(image)));
        }
        let mut width = 0u32;
        let mut height = 0u32;
        let mut format = 0i32;
        let mut len = 0usize;
        let mut data: *const u8 = std::ptr::null();
        for (key, out) in [
            (
                ffi::GHOSTTY_KITTY_IMAGE_DATA_WIDTH,
                (&mut width as *mut u32).cast(),
            ),
            (
                ffi::GHOSTTY_KITTY_IMAGE_DATA_HEIGHT,
                (&mut height as *mut u32).cast(),
            ),
            (
                ffi::GHOSTTY_KITTY_IMAGE_DATA_FORMAT,
                (&mut format as *mut i32).cast(),
            ),
            (
                ffi::GHOSTTY_KITTY_IMAGE_DATA_DATA_PTR,
                (&mut data as *mut *const u8).cast(),
            ),
            (
                ffi::GHOSTTY_KITTY_IMAGE_DATA_DATA_LEN,
                (&mut len as *mut usize).cast(),
            ),
        ] {
            let result = unsafe { ffi::ghostty_kitty_graphics_image_get(handle, key, out) };
            // Async image loading exposes metadata before pixels are ready.
            // That image must not prevent the rest of the terminal painting.
            if result == ffi::GHOSTTY_NO_VALUE {
                return Ok(None);
            }
            check(result)?;
        }
        if width == 0
            || height == 0
            || width > MAX_IMAGE_DIMENSION
            || height > MAX_IMAGE_DIMENSION
            || u64::from(width) * u64::from(height) * 4 > MAX_IMAGE_BYTES as u64
            || data.is_null()
        {
            eprintln!("ghostex: invalid retained Kitty image {id} dimensions: {width}x{height}");
            return Ok(None);
        }
        let channels = match format {
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_RGB => 3,
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_RGBA => 4,
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA => 2,
            ffi::GHOSTTY_KITTY_IMAGE_FORMAT_GRAY => 1,
            _ => return Ok(None),
        };
        if len != width as usize * height as usize * channels {
            return Ok(None);
        }
        let bytes = unsafe { std::slice::from_raw_parts(data, len) };
        let rgba = if channels == 4 {
            bytes.to_vec()
        } else {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for p in bytes.chunks_exact(channels) {
                match format {
                    ffi::GHOSTTY_KITTY_IMAGE_FORMAT_RGB => {
                        rgba.extend_from_slice(&[p[0], p[1], p[2], 255])
                    }
                    ffi::GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA => {
                        rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]])
                    }
                    ffi::GHOSTTY_KITTY_IMAGE_FORMAT_GRAY => {
                        rgba.extend_from_slice(&[p[0], p[0], p[0], 255])
                    }
                    _ => unreachable!(),
                }
            }
            rgba
        };
        let image = Arc::new(VtImage {
            id,
            generation,
            width,
            height,
            rgba,
        });
        self.images.insert(id, Arc::clone(&image));
        Ok(Some(image))
    }
}

#[cfg(not(target_family = "wasm"))]
impl Drop for VtImageState {
    fn drop(&mut self) {
        unsafe { ffi::ghostty_kitty_graphics_placement_iterator_free(self.iterator) };
    }
}

/// Process-global decoder, registered once before image-capable terminals
/// feed input. The output uses Ghostty's allocator so the core does not
/// free a Rust allocation through a different runtime, including on Windows.
#[cfg(not(target_family = "wasm"))]
unsafe extern "C" fn decode_png(
    _userdata: *mut c_void,
    allocator: *const c_void,
    data: *const u8,
    len: usize,
    out: *mut ffi::GhosttySysImage,
) -> bool {
    if data.is_null() || out.is_null() || len > MAX_IMAGE_BYTES {
        return false;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_PNG_DECODE_BYTES);
    reader.limits(limits);
    let Ok(decoder) = reader.into_decoder() else {
        return false;
    };
    let (width, height) = image::ImageDecoder::dimensions(&decoder);
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return false;
    }
    let Ok(decoded) = image::DynamicImage::from_decoder(decoder) else {
        return false;
    };
    let rgba = decoded.into_rgba8();
    let pixels = unsafe { ffi::ghostty_alloc(allocator, rgba.len()) };
    if pixels.is_null() {
        return false;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(rgba.as_ptr(), pixels, rgba.len());
        *out = ffi::GhosttySysImage {
            width: rgba.width(),
            height: rgba.height(),
            data: pixels,
            data_len: rgba.len(),
        };
    }
    true
}

// Ghostty force-disables Kitty graphics on freestanding targets because
// image storage requires OS timestamps. Keep the shared browser renderer
// compiling without references to C symbols its wasm archive does not export.
#[cfg(target_family = "wasm")]
pub(crate) struct VtImageState;

#[cfg(target_family = "wasm")]
impl VtImageState {
    pub(crate) fn configure(_terminal: ffi::GhosttyTerminal) -> Result<(), VtError> {
        Ok(())
    }

    pub(crate) fn new() -> Result<Self, VtError> {
        Ok(Self)
    }

    pub(crate) fn snapshot(
        &mut self,
        _terminal: ffi::GhosttyTerminal,
    ) -> Result<Vec<VtImagePlacement>, VtError> {
        Ok(Vec::new())
    }
}
