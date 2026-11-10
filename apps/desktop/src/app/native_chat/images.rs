//! Pictures shared in the conversation.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! The core classifies each block with `image_source` in
//! packages/gx-chat-core/src/transcript/images.rs. A machine path is bytes only
//! `readSessionChatImage` can serve, because the picture lives on the session's machine, so it is
//! read once here and shared by the thumbnail and the full-size viewer. A picture that cannot be
//! read renders the named chip it would otherwise have been, never a broken image well.

use super::file_actions::{FileMenuExt as _, image_file_path};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use base64::Engine as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ImageFormat, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderImage, StatefulInteractiveElement as _, Styled as _, StyledImage as _, div, img, px, svg,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use web_time::Instant;

/// The measurements React drew a transcript picture with, read from `image-visual.json`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChatImageVisual {
    /// React's `.ghostex-chat-inline-image`: a 3rem square.
    pub(super) thumbnail_size: f32,
    pub(super) thumbnail_radius: f32,
    pub(super) border_width: f32,
    /// `gap-1.5` between the user's own pictures, `gap-2` between an agent's.
    pub(super) user_row_gap: f32,
    pub(super) assistant_row_gap: f32,
    pub(super) row_padding_y: f32,
    /// The air React leaves around a picture written into prose.
    pub(super) inline_margin_x: f32,
    pub(super) inline_margin_y: f32,
    pub(super) loading_icon_size: f32,
    /// The filmstrip an agent's pictures on lines of their own sit in: each picture this tall at
    /// its own shape, no wider than the cap, and a lone picture larger.
    pub(super) strip_height: f32,
    pub(super) strip_max_width: f32,
    pub(super) single_height: f32,
    pub(super) single_max_width: f32,
    pub(super) strip_gap: f32,
    pub(super) strip_radius: f32,
    /// Between a picture and the file name under it.
    pub(super) caption_gap: f32,
    pub(super) caption_size: f32,
    /// The narrowest a tile gets, so a tall, thin picture's name still reads.
    pub(super) caption_min_width: f32,
    /// Room under the strip for its sideways scrollbar.
    pub(super) strip_padding_bottom: f32,
}

/// CDXC:SessionChat 2026-09-19 SEE-ALSO: The thumbnail's size, radius, hairline and row gaps come
/// from packages/gx-chat-core/visual/image-visual.json, which mirrored the React chat's CSS in
/// packages/core-ui/styles/chat.css until 2026-09-25 and is the only source now.
pub(super) static VISUAL: LazyLock<ChatImageVisual> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/image-visual.json"
    ))
    .expect("shared transcript image appearance")
});

/// Device pixels per point a thumbnail is rasterized at.
///
/// A screenshot painted straight into a 48pt tile leaves the GPU to shrink it by twenty times or
/// more with a single sample per pixel, which is the mush a full-size picture used to show here.
/// Two per point is 1:1 on every Retina display and a clean halving on a 1x one, and the browser
/// does the same thing for React's `<img>`.
const THUMBNAIL_PIXEL_RATIO: f32 = 2.0;

/// A picture this big is left to GPUI rather than decoded twice: the resize would cost more than
/// the sampling it saves.
const THUMBNAIL_MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;

/// CDXC:SessionChat 2026-10-23 WHY:
/// Every picture a chat ever showed used to stay in memory until its chat view was dropped (its file bytes, every shrunk copy, GPUI's full-size decoded copy of each one opened in the preview, and every painted Mermaid diagram, up to 32 MB each), and up to 24 chat views stay warm (`session_chat_prewarm.rs`). The cache now keeps this many bytes per chat (file bytes, the shrunk copies' pixels and the painted diagrams, `mermaid.rs`) and lets the pictures drawn longest ago go first, so scrolling back to one reads and shrinks (or paints) it again. A tile is drawn from its shrunk copy without the file bytes, so those go first; a diagram let go keeps its drawing and is only painted again.
const CHAT_IMAGE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
/// What a chat whose pane has been off screen for `HIDDEN_TRIM_DELAY` keeps: the pictures it drew
/// last, so coming back to it shows them at once.
#[cfg_attr(target_family = "wasm", allow(dead_code))] // the web page never hides a chat pane
const HIDDEN_CHAT_IMAGE_BUDGET_BYTES: usize = 8 * 1024 * 1024;
/// A picture drawn this recently is never let go: it is on screen, or was a moment ago, and a
/// cached transcript can paint it again without asking the cache.
const RECENTLY_DRAWN: std::time::Duration = std::time::Duration::from_secs(30);
/// Clicking through sessions and back keeps every picture; a chat left for longer is trimmed. At
/// least `RECENTLY_DRAWN`, so the trim spares only what was drawn after the pane went away.
#[cfg_attr(target_family = "wasm", allow(dead_code))] // the web page never hides a chat pane
const HIDDEN_TRIM_DELAY: std::time::Duration = RECENTLY_DRAWN;

enum ChatImageEntry {
    Loading,
    Ready(Arc<gpui::Image>),
    Failed(ChatImageFailure),
}

impl ChatImageEntry {
    fn bytes(&self) -> usize {
        match self {
            Self::Ready(picture) => picture.bytes.len(),
            Self::Loading | Self::Failed(_) => 0,
        }
    }
}

/// The pixels a shrunk copy holds in memory (and again in the window's texture atlas).
pub(super) fn rendered_bytes(picture: &RenderImage) -> usize {
    (0..picture.frame_count())
        .map(|frame| picture.as_bytes(frame).map_or(0, <[u8]>::len))
        .sum()
}

/// One cached value and when a row last drew it.
struct Kept<T> {
    value: T,
    drawn_at: Instant,
}

impl<T> Kept<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            drawn_at: Instant::now(),
        }
    }
}

/// The value under `key`, read by a row being drawn, which makes it the most recently drawn.
fn drawn<K: std::hash::Hash + Eq, T, R>(
    map: &RefCell<HashMap<K, Kept<T>>>,
    key: &K,
    read: impl FnOnce(&T) -> R,
) -> Option<R> {
    let mut map = map.borrow_mut();
    let kept = map.get_mut(key)?;
    kept.drawn_at = Instant::now();
    Some(read(&kept.value))
}

/// The cache's key for a picture the chat reads or decodes itself.
///
/// A `data:` address is the whole picture in base64, so the cache keeps its hash rather than a
/// second copy of it, and hashes it in place rather than cloning it on every draw.
fn source_id(transport: &str, source: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::hash::DefaultHasher::new();
    transport.hash(&mut hasher);
    source.hash(&mut hasher);
    hasher.finish()
}

/// `source_id` for a projected image value, or `None` for one GPUI fetches itself (`url`) or one
/// with nothing to read.
fn image_source_id(image: &Value) -> Option<u64> {
    let transport = image["transport"].as_str().unwrap_or("none");
    let source = match transport {
        "data" => image["url"].as_str(),
        "read" => image["path"].as_str(),
        _ => None,
    }
    .filter(|source| !source.is_empty())?;
    Some(source_id(transport, source))
}

/// What a trim took out of the cache: GPUI still holds a decoded copy of each picture and an atlas
/// tile of each shrunk copy until they are released (`release_chat_pictures`).
#[derive(Default)]
struct ReleasedPictures {
    pictures: Vec<Arc<gpui::Image>>,
    rendered: Vec<Arc<RenderImage>>,
}

/// Frees what GPUI keeps of pictures the chat has let go of, once the current update is over so
/// no window is mid-draw: the decoded copy in the asset cache and the tiles in every window's
/// atlas.
fn release_chat_pictures(released: ReleasedPictures, cx: &mut gpui::App) {
    if released.pictures.is_empty() && released.rendered.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for picture in released.pictures {
            picture.remove_asset(cx);
        }
        for rendered in released.rendered {
            cx.drop_image(rendered, None);
        }
    });
}

/// Why a picture cannot be shown: one of the core's `image_read_failure_reason` words, or
/// `unsupported`, `damaged` and `unavailable`, which only a renderer can tell.
#[derive(Clone, Debug)]
pub(super) struct ChatImageFailure {
    pub(super) reason: String,
    /// The server's own sentence, shown when the reason alone says too little.
    pub(super) error: String,
}

impl ChatImageFailure {
    fn new(reason: &str) -> Self {
        Self {
            reason: reason.to_string(),
            error: String::new(),
        }
    }

    /// The file is there and only its bytes could not be shown, so opening it elsewhere can help.
    pub(super) fn file_exists(&self) -> bool {
        matches!(
            self.reason.as_str(),
            "tooLarge" | "notImage" | "unsupported" | "damaged"
        )
    }
}

/// One picture shrunk to the tile it is painted in.
enum ChatImageThumbnail {
    Loading,
    Ready(Arc<RenderImage>),
    /// Bytes no decoder here reads (an SVG, which GPUI rasterizes at the tile's own size anyway):
    /// the picture itself is painted instead.
    Whole,
}

/// Bytes for the transcript's pictures, read once per location and shared by every thumbnail and the viewer.
///
/// A picture written into prose is asked for from the Markdown renderer, which runs with the view
/// borrowed shared, so the map is behind a cell rather than reachable only from `&mut self`.
#[derive(Default)]
pub(crate) struct ChatImageCache {
    /// Each picture's bytes, keyed by `source_id`.
    entries: RefCell<HashMap<u64, Kept<ChatImageEntry>>>,
    /// Cover-cropped copies keyed by the picture's `source_id` and the device pixels it is painted
    /// at, so a zoom change rasterizes once more, scrolling never resizes anything, and a tile is
    /// drawn from its copy even after the picture's bytes have been let go.
    thumbnails: RefCell<HashMap<(u64, u32), Kept<ChatImageThumbnail>>>,
    /// Filmstrip copies at the picture's own shape, keyed like `thumbnails` by the strip's height
    /// and widest frame.
    strip: RefCell<HashMap<(u64, u32, u32), Kept<ChatStripPicture>>>,
    /// The trim a pane that went off screen waits for (`schedule_hidden_image_trim`); the web
    /// page never hides a chat pane.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    hidden_trim: Option<gpui::Task<()>>,
    /// When the transcript's rows were last drawn. The transcript is a cached view
    /// (transcript_host.rs) that can show the same rows for minutes without drawing them again,
    /// so every picture drawn since then is still on screen however long ago that was.
    rows_drawn_at: std::cell::Cell<Option<Instant>>,
}

impl ChatImageCache {
    /// The transcript is drawing its rows now.
    pub(super) fn note_transcript_rows_drawn(&self) {
        self.rows_drawn_at.set(Some(Instant::now()));
    }

    /// Lets the pictures drawn longest ago go until the cache and the painted Mermaid diagrams
    /// (`diagrams`) hold at most `budget` bytes, never one drawn within `RECENTLY_DRAWN`, one the
    /// transcript's rows still show (when `on_screen`), or one of `pinned` (the preview's and the
    /// composer's pictures).
    fn trim(
        &self,
        budget: usize,
        on_screen: bool,
        pinned: &[u64],
        diagrams: &super::mermaid::ChatMermaidCache,
    ) -> ReleasedPictures {
        let rows_drawn_at = self.rows_drawn_at.get().filter(|_| on_screen);
        enum Slot {
            Entry(u64),
            Thumbnail((u64, u32)),
            Strip((u64, u32, u32)),
            Diagram((String, bool)),
        }
        let mut entries = self.entries.borrow_mut();
        let mut thumbnails = self.thumbnails.borrow_mut();
        let mut strip = self.strip.borrow_mut();
        let mut total = 0;
        let mut candidates = Vec::new();
        for (key, kept) in entries.iter() {
            let bytes = kept.value.bytes();
            total += bytes;
            if bytes > 0 && !pinned.contains(key) {
                candidates.push((kept.drawn_at, Slot::Entry(*key), bytes));
            }
        }
        for (key, kept) in thumbnails.iter() {
            if let ChatImageThumbnail::Ready(shrunk) = &kept.value {
                let bytes = rendered_bytes(shrunk);
                total += bytes;
                if !pinned.contains(&key.0) {
                    candidates.push((kept.drawn_at, Slot::Thumbnail(*key), bytes));
                }
            }
        }
        for (key, kept) in strip.iter() {
            if let ChatStripPicture::Ready(shrunk, _) = &kept.value {
                let bytes = rendered_bytes(shrunk);
                total += bytes;
                if !pinned.contains(&key.0) {
                    candidates.push((kept.drawn_at, Slot::Strip(*key), bytes));
                }
            }
        }
        for (drawn_at, key, bytes) in diagrams.painted() {
            total += bytes;
            candidates.push((drawn_at, Slot::Diagram(key), bytes));
        }
        let mut released = ReleasedPictures::default();
        if total <= budget {
            return released;
        }
        candidates.sort_by_key(|(drawn_at, ..)| *drawn_at);
        for (drawn_at, slot, bytes) in candidates {
            // Both rules spare everything drawn after the first picture they spare.
            if total <= budget
                || drawn_at.elapsed() < RECENTLY_DRAWN
                || rows_drawn_at.is_some_and(|rows| drawn_at >= rows)
            {
                break;
            }
            total -= bytes;
            match slot {
                Slot::Entry(key) => {
                    if let Some(Kept {
                        value: ChatImageEntry::Ready(picture),
                        ..
                    }) = entries.remove(&key)
                    {
                        released.pictures.push(picture);
                    }
                }
                Slot::Thumbnail(key) => {
                    if let Some(Kept {
                        value: ChatImageThumbnail::Ready(shrunk),
                        ..
                    }) = thumbnails.remove(&key)
                    {
                        released.rendered.push(shrunk);
                    }
                }
                Slot::Strip(key) => {
                    if let Some(Kept {
                        value: ChatStripPicture::Ready(shrunk, _),
                        ..
                    }) = strip.remove(&key)
                    {
                        released.rendered.push(shrunk);
                    }
                }
                Slot::Diagram(key) => released.rendered.extend(diagrams.take_painted(&key)),
            }
        }
        released
    }

    /// Everything the cache holds, for a chat view that is going away.
    fn take_all(&self) -> ReleasedPictures {
        let mut released = ReleasedPictures::default();
        for (_, kept) in self.entries.borrow_mut().drain() {
            if let ChatImageEntry::Ready(picture) = kept.value {
                released.pictures.push(picture);
            }
        }
        for (_, kept) in self.thumbnails.borrow_mut().drain() {
            if let ChatImageThumbnail::Ready(shrunk) = kept.value {
                released.rendered.push(shrunk);
            }
        }
        for (_, kept) in self.strip.borrow_mut().drain() {
            if let ChatStripPicture::Ready(shrunk, _) = kept.value {
                released.rendered.push(shrunk);
            }
        }
        released
    }

    /// Whether a picture is painted from its own bytes somewhere in the transcript, which is what
    /// GPUI's decoded copy of it is for.
    fn painted_whole(&self, id: u64, picture: &gpui::Image) -> bool {
        picture.format == ImageFormat::Svg
            || self
                .thumbnails
                .borrow()
                .iter()
                .any(|(key, kept)| key.0 == id && matches!(kept.value, ChatImageThumbnail::Whole))
            || self
                .strip
                .borrow()
                .iter()
                .any(|(key, kept)| key.0 == id && matches!(kept.value, ChatStripPicture::Whole(_)))
    }
}

/// One picture shrunk to a filmstrip's height, never cropped.
enum ChatStripPicture {
    Loading,
    Ready(Arc<RenderImage>, (u32, u32)),
    /// Painted whole: an SVG, or a picture too large to shrink; its size when the header says it.
    Whole(Option<(u32, u32)>),
}

/// What a renderer can do with one picture right now.
pub(super) enum ChatImageSource {
    /// An http(s) address GPUI fetches itself.
    Uri(String),
    Bytes(Arc<gpui::Image>),
    Loading,
    /// No transport, unreadable bytes, or a format GPUI cannot decode: the named chip stands in.
    Unavailable(ChatImageFailure),
}

/// What one transcript tile paints: the same picture, already shrunk to the size it is drawn at.
pub(super) enum ChatImageTile {
    Uri(String),
    Thumbnail(Arc<RenderImage>),
    Whole(Arc<gpui::Image>),
    Loading,
    Unavailable(ChatImageFailure),
}

/// A label that is a whole path, cut to its file name: a stand-in names the picture, and the
/// preview's card is where the folder is shown.
fn file_name_of(label: &str) -> String {
    label
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(label)
        .to_string()
}

fn image_format(media_type: &str) -> Option<ImageFormat> {
    match media_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/webp" => Some(ImageFormat::Webp),
        "image/gif" => Some(ImageFormat::Gif),
        "image/svg+xml" => Some(ImageFormat::Svg),
        "image/bmp" => Some(ImageFormat::Bmp),
        "image/tiff" => Some(ImageFormat::Tiff),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some(ImageFormat::Ico),
        _ => None,
    }
}

fn decode_data_url(url: &str) -> Result<gpui::Image, ChatImageFailure> {
    let damaged = || ChatImageFailure::new("damaged");
    let (meta, payload) = url
        .strip_prefix("data:")
        .and_then(|url| url.split_once(','))
        .ok_or_else(damaged)?;
    if !meta.contains(";base64") {
        return Err(ChatImageFailure {
            reason: "unsupported".to_string(),
            error: format!("data:{meta}"),
        });
    }
    decoded_image(meta.split(';').next().unwrap_or_default(), payload.trim())
}

/**
 * The picture behind base64 bytes, refused when GPUI has no decoder for its type or its header
 * does not read as a picture.
 *
 * CDXC:SessionChat 2026-10-01 WHY:
 * The server recognises a file by its first bytes, so a truncated or corrupt PNG arrives as
 * `loaded` and GPUI then paints nothing at all; checking the header here lets the viewer say the
 * file looks damaged instead of showing an empty stage.
 */
fn decoded_image(media_type: &str, base64_data: &str) -> Result<gpui::Image, ChatImageFailure> {
    // The type that arrived goes with the refusal, so the viewer can name it (unavailable.rs).
    let format = image_format(media_type).ok_or_else(|| ChatImageFailure {
        reason: "unsupported".to_string(),
        error: media_type.trim().to_string(),
    })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|_| ChatImageFailure::new("damaged"))?;
    if format != ImageFormat::Svg {
        // Only a header that fails to parse counts: a type this build has no header reader for
        // (`Unsupported`) is still left to GPUI's own decoders.
        let header = image::ImageReader::new(std::io::Cursor::new(bytes.as_slice()))
            .with_guessed_format()
            .map_err(|_| ChatImageFailure::new("damaged"))?
            .into_dimensions();
        match header {
            Ok((width, height)) if width > 0 && height > 0 => {}
            Err(image::ImageError::Unsupported(_)) => {}
            _ => return Err(ChatImageFailure::new("damaged")),
        }
    }
    Ok(gpui::Image::from_bytes(format, bytes))
}

/// The 3rem square React draws for a picture (`.ghostex-chat-inline-image`), or nothing when its
/// bytes are not there and the caller has to fall back to the picture's name.
fn thumbnail(source: &ChatImageTile, p: &ChatAppearance) -> Option<AnyElement> {
    thumbnail_at(source, VISUAL.thumbnail_size, p)
}

/// `thumbnail` as a `side`-point square (unscaled), for the read-picture tiles beside a tool row.
fn thumbnail_at(source: &ChatImageTile, side: f32, p: &ChatAppearance) -> Option<AnyElement> {
    let s = p.scale;
    let size = px(side * s);
    let radius = px(VISUAL.thumbnail_radius * s);
    let hairline = px(VISUAL.border_width * s);
    // `ObjectFit::Cover` paints the scaled picture through the bounds it was given instead of
    // cropping to them, so the tile itself has to be the clipping well React's `overflow-hidden`
    // rounded box is; without it a wide picture bleeds over its neighbours and the pane's edge.
    let framed = |picture: gpui::Img| {
        div()
            .size(size)
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(radius)
            .border(hairline)
            .border_color(p.border)
            .child(
                picture
                    .size_full()
                    .rounded(radius)
                    .object_fit(gpui::ObjectFit::Cover),
            )
            .into_any_element()
    };
    match source {
        ChatImageTile::Uri(url) => Some(framed(img(url.clone()))),
        ChatImageTile::Thumbnail(picture) => Some(framed(img(picture.clone()))),
        ChatImageTile::Whole(bytes) => Some(framed(img(bytes.clone()))),
        // React's `.ghostex-chat-inline-image-pending`: the same square with a dashed hairline and
        // a turning loader, so a picture being read looks like one arriving rather than one lost.
        ChatImageTile::Loading => Some(
            div()
                .size(size)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius)
                .border(hairline)
                .border_dashed()
                .border_color(p.border)
                .child(
                    svg()
                        .path("titlebar/loader2.svg")
                        .size(px(VISUAL.loading_icon_size.min(side * 0.6) * s))
                        .text_color(p.muted)
                        .with_throttled_animation(
                            "chat-image-spinner",
                            std::time::Duration::from_millis(900),
                            |svg, delta| {
                                svg.with_transformation(gpui::Transformation::rotate(
                                    gpui::radians(delta * std::f32::consts::TAU),
                                ))
                            },
                        ),
                )
                .into_any_element(),
        ),
        ChatImageTile::Unavailable(_) => None,
    }
}

/// The device pixels one tile is painted with at the transcript's current zoom.
fn thumbnail_pixels(p: &ChatAppearance) -> u32 {
    (VISUAL.thumbnail_size * p.scale * THUMBNAIL_PIXEL_RATIO)
        .ceil()
        .max(1.0) as u32
}

/// A picture's pixels the way GPUI's own decoder shows them (the preview), turned by the file's
/// EXIF orientation, so a shrunk copy of a phone photo stands the same way up as the photo.
fn decode_upright(bytes: &[u8]) -> Option<image::DynamicImage> {
    use image::ImageDecoder as _;
    let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).ok()?;
    decoded.apply_orientation(orientation);
    Some(decoded)
}

/// Shrinks one picture to the square it is painted in, off the UI thread.
///
/// The crop is centred, which is what React's `object-fit: cover` does, and the sampling averages
/// every source pixel that lands in a tile pixel instead of picking one of them, which is the
/// difference between a readable screenshot and noise.
fn build_thumbnail(picture: &gpui::Image, pixels: u32) -> Option<RenderImage> {
    if picture.format == ImageFormat::Svg {
        return None;
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(picture.bytes.as_slice()))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > THUMBNAIL_MAX_SOURCE_PIXELS
    {
        return None;
    }
    let decoded = decode_upright(&picture.bytes)?;
    let (width, height) = (decoded.width(), decoded.height());
    let side = width.min(height);
    let square = decoded
        .crop_imm((width - side) / 2, (height - side) / 2, side, side)
        .into_rgba8();
    let mut data = if side >= pixels {
        image::imageops::thumbnail(&square, pixels, pixels)
    } else {
        image::imageops::resize(
            &square,
            pixels,
            pixels,
            image::imageops::FilterType::Triangle,
        )
    };
    // gpui uploads sprite atlas tiles as BGRA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(RenderImage::new(vec![image::Frame::new(data)]))
}

/// Shrinks one picture to `height` device pixels at its own shape, and no wider than `max_width`
/// (the widest frame the strip paints, which a wider picture is fitted into), off the UI thread,
/// with the source's size for the caption. `None` paints the original, as `build_thumbnail` does.
fn build_strip_picture(
    picture: &gpui::Image,
    height: u32,
    max_width: u32,
) -> (Option<RenderImage>, Option<(u32, u32)>) {
    if picture.format == ImageFormat::Svg {
        return (None, None);
    }
    let Some(reader) = image::ImageReader::new(std::io::Cursor::new(picture.bytes.as_slice()))
        .with_guessed_format()
        .ok()
    else {
        return (None, None);
    };
    let Ok((width, source_height)) = reader.into_dimensions() else {
        return (None, None);
    };
    let size = Some((width, source_height));
    if width == 0
        || source_height == 0
        || u64::from(width) * u64::from(source_height) > THUMBNAIL_MAX_SOURCE_PIXELS
    {
        return (None, size);
    }
    let Some(decoded) = decode_upright(&picture.bytes) else {
        return (None, size);
    };
    // The upright shape, which is the one the frame and its caption describe.
    let (width, source_height) = (decoded.width(), decoded.height());
    let size = Some((width, source_height));
    let decoded = decoded.into_rgba8();
    let mut target_height = height.min(source_height).max(1);
    let mut target_width =
        ((u64::from(width) * u64::from(target_height)) / u64::from(source_height)).max(1) as u32;
    if target_width > max_width.max(1) {
        target_width = max_width.max(1);
        target_height =
            ((u64::from(source_height) * u64::from(target_width)) / u64::from(width)).max(1) as u32;
    }
    let mut data = if target_height < source_height {
        image::imageops::thumbnail(&decoded, target_width, target_height)
    } else {
        decoded
    };
    // gpui uploads sprite atlas tiles as BGRA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    (Some(RenderImage::new(vec![image::Frame::new(data)])), size)
}

fn decode_read_result(result: &Value) -> Result<gpui::Image, ChatImageFailure> {
    decoded_image(
        result["mediaType"].as_str().unwrap_or_default(),
        result["base64Data"].as_str().unwrap_or_default(),
    )
}

impl NativeChatView {
    /// Resolves one projected image source, starting the read the first time it is asked for.
    pub(super) fn chat_image(&self, image: &Value, cx: &Context<Self>) -> ChatImageSource {
        let transport = image["transport"].as_str().unwrap_or("none");
        if transport == "url" {
            let url = text(image, "url");
            return if url.is_empty() {
                ChatImageSource::Unavailable(ChatImageFailure::new("unavailable"))
            } else {
                ChatImageSource::Uri(url)
            };
        }
        let Some(id) = image_source_id(image) else {
            return ChatImageSource::Unavailable(ChatImageFailure::new("unavailable"));
        };
        let known = drawn(&self.images.entries, &id, |entry| match entry {
            ChatImageEntry::Ready(image) => ChatImageSource::Bytes(image.clone()),
            ChatImageEntry::Failed(failure) => ChatImageSource::Unavailable(failure.clone()),
            ChatImageEntry::Loading => ChatImageSource::Loading,
        });
        if let Some(known) = known {
            return known;
        }
        self.images
            .entries
            .borrow_mut()
            .insert(id, Kept::new(ChatImageEntry::Loading));
        if transport == "data" {
            let url = image["url"].as_str().unwrap_or_default().to_string();
            let task = cx
                .background_executor()
                .spawn(async move { decode_data_url(&url) });
            cx.spawn(async move |this, cx| {
                let loaded = task.await;
                let _ = this.update(cx, |this, cx| this.store_chat_image(id, loaded, cx));
            })
            .detach();
        } else {
            // The path is on the session's machine, so the read goes through the shared transport.
            // Asked for on the next turn of the loop, because this runs while the row is rendering.
            let path = image["path"].as_str().unwrap_or_default().to_string();
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.invoke(json!({"type": "loadImage", "path": path}), cx)
                });
            })
            .detach();
        }
        ChatImageSource::Loading
    }

    /// The same picture as `chat_image`, shrunk once to the square the transcript paints it in.
    ///
    /// The viewer still opens the original; only the tile trades it for a copy the size of the
    /// tile, which is what keeps a transcript of screenshots readable and cheap to scroll.
    pub(super) fn chat_image_tile(
        &self,
        image: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> ChatImageTile {
        // A shrunk copy is drawn without the picture's bytes, which the cache may have let go.
        let key = image_source_id(image).map(|id| (id, thumbnail_pixels(p)));
        let shrunk = key.and_then(|key| {
            drawn(&self.images.thumbnails, &key, |thumbnail| match thumbnail {
                ChatImageThumbnail::Ready(thumbnail) => {
                    Some(ChatImageTile::Thumbnail(thumbnail.clone()))
                }
                ChatImageThumbnail::Loading => Some(ChatImageTile::Loading),
                ChatImageThumbnail::Whole => None,
            })
        });
        if let Some(Some(tile)) = shrunk {
            return tile;
        }
        let picture = match self.chat_image(image, cx) {
            // An address GPUI fetches and scales itself, as the browser does for React.
            ChatImageSource::Uri(url) => return ChatImageTile::Uri(url),
            ChatImageSource::Loading => return ChatImageTile::Loading,
            ChatImageSource::Unavailable(failure) => return ChatImageTile::Unavailable(failure),
            ChatImageSource::Bytes(picture) => picture,
        };
        // Drawn from its own shapes at whatever size it is given, so there is nothing to shrink
        // and nothing to wait for; or too large to shrink (`shrunk` said so).
        let Some(key) = key.filter(|_| picture.format != ImageFormat::Svg && shrunk.is_none())
        else {
            return ChatImageTile::Whole(picture);
        };
        self.images
            .thumbnails
            .borrow_mut()
            .insert(key, Kept::new(ChatImageThumbnail::Loading));
        let source = picture.clone();
        let task = cx
            .background_executor()
            .spawn(async move { build_thumbnail(&source, key.1).map(Arc::new) });
        cx.spawn(async move |this, cx| {
            let thumbnail = task.await;
            let _ = this.update(cx, |this, cx| {
                let ready = thumbnail.is_some();
                this.images.thumbnails.borrow_mut().insert(
                    key,
                    Kept::new(match thumbnail {
                        Some(thumbnail) => ChatImageThumbnail::Ready(thumbnail),
                        None => ChatImageThumbnail::Whole,
                    }),
                );
                if ready {
                    this.trim_chat_images(cx);
                }
                cx.notify();
            });
        })
        .detach();
        ChatImageTile::Loading
    }

    /// Keeps the cache within its budget after it grew, sparing what is on screen.
    ///
    /// Every caller notifies the chat afterwards, so a surface showing a picture that was let go
    /// is drawn again (and asks for it again) rather than replayed from before.
    pub(super) fn trim_chat_images(&mut self, cx: &mut Context<Self>) {
        let pinned = self.pinned_pictures();
        let released = self
            .images
            .trim(CHAT_IMAGE_BUDGET_BYTES, true, &pinned, &self.mermaid);
        release_chat_pictures(released, cx);
    }

    /// The pictures of the open preview (or the one owed to this pane when it is shown again) and
    /// the composer's attachment tiles, which are drawn whenever the chat is.
    fn pinned_pictures(&self) -> Vec<u64> {
        let viewer = self
            .image_viewer
            .request
            .iter()
            .flat_map(|request| request.images.iter().filter_map(image_source_id));
        let composer = self
            .composer_references
            .iter()
            .filter(|reference| reference.kind == "image" && !reference.path.is_empty())
            .map(|reference| source_id("read", &reference.path));
        viewer.chain(composer).collect()
    }

    /// The pane went off screen: once it has stayed off screen for `HIDDEN_TRIM_DELAY`, the cache
    /// keeps only what it drew last, up to `HIDDEN_CHAT_IMAGE_BUDGET_BYTES`.
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // the web page never hides a chat pane
    pub(super) fn schedule_hidden_image_trim(&mut self, cx: &mut Context<Self>) {
        self.images.hidden_trim = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HIDDEN_TRIM_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.images.hidden_trim = None;
                if this.pane_hidden {
                    // Still sparing what was drawn lately: a view can be drawn while its pane
                    // counts as hidden (the floating sessions panel).
                    let pinned = this.pinned_pictures();
                    let released = this.images.trim(
                        HIDDEN_CHAT_IMAGE_BUDGET_BYTES,
                        false,
                        &pinned,
                        &this.mermaid,
                    );
                    if !released.pictures.is_empty() || !released.rendered.is_empty() {
                        release_chat_pictures(released, cx);
                        cx.notify();
                    }
                }
            });
        }));
    }

    #[cfg_attr(target_family = "wasm", allow(dead_code))] // the web page never hides a chat pane
    pub(super) fn cancel_hidden_image_trim(&mut self) {
        self.images.hidden_trim = None;
    }

    /// Lets GPUI drop its full-size decoded copy of each picture a closed preview showed, unless
    /// the transcript still paints that picture from its own bytes.
    pub(super) fn release_viewed_pictures(&self, images: &[Value], cx: &mut Context<Self>) {
        let entries = self.images.entries.borrow();
        let pictures = images
            .iter()
            .filter_map(image_source_id)
            .filter_map(|id| match entries.get(&id).map(|kept| &kept.value) {
                Some(ChatImageEntry::Ready(picture)) if !self.images.painted_whole(id, picture) => {
                    Some(picture.clone())
                }
                _ => None,
            })
            .collect();
        release_chat_pictures(
            ReleasedPictures {
                pictures,
                rendered: Vec::new(),
            },
            cx,
        );
    }

    /// Registers the release of everything GPUI keeps of this chat's pictures when the view goes
    /// away: a pooled-out chat view otherwise left its tiles in the window's atlas for good.
    pub(super) fn release_chat_images_with_view(cx: &mut Context<Self>) {
        cx.on_release(|chat, cx| {
            let mut released = chat.images.take_all();
            released.rendered.extend(chat.mermaid.take_rasters());
            released
                .pictures
                .extend(chat.visual.page_thumbnails.take_pictures());
            release_chat_pictures(released, cx);
        })
        .detach();
    }

    /// Page-card pictures a newer one replaced (`page_card.rs`).
    pub(super) fn release_page_pictures(
        &self,
        pictures: Vec<Arc<gpui::Image>>,
        cx: &mut Context<Self>,
    ) {
        release_chat_pictures(
            ReleasedPictures {
                pictures,
                rendered: Vec::new(),
            },
            cx,
        );
    }

    /// The transport answered a `loadImage` ask (the core's `chatImage` request).
    pub(super) fn receive_chat_image(&mut self, request: &Value, cx: &mut Context<Self>) {
        let path = request["params"]["path"].as_str().unwrap_or_default();
        if path.is_empty() {
            return;
        }
        let path = source_id("read", path);
        if request["method"] != "loaded" {
            let params = &request["params"];
            let reason = text(params, "reason");
            let failure = ChatImageFailure {
                reason: if reason.is_empty() {
                    "unreadable".to_string()
                } else {
                    reason
                },
                error: text(params, "error"),
            };
            self.store_chat_image(path, Err(failure), cx);
            return;
        }
        let params = request["params"].clone();
        let task = cx
            .background_executor()
            .spawn(async move { decode_read_result(&params) });
        cx.spawn(async move |this, cx| {
            let loaded = task.await;
            let _ = this.update(cx, |this, cx| this.store_chat_image(path, loaded, cx));
        })
        .detach();
    }

    /// A picture this view just saved to `path` (a composer paste), kept with the bytes it already
    /// holds so its tile does not read the file back from the session's machine.
    pub(super) fn seed_chat_image(
        &mut self,
        path: &str,
        image: gpui::Image,
        cx: &mut Context<Self>,
    ) {
        let key = source_id("read", path);
        let known = matches!(
            self.images
                .entries
                .borrow()
                .get(&key)
                .map(|kept| &kept.value),
            Some(ChatImageEntry::Ready(_))
        );
        if !known {
            self.store_chat_image(key, Ok(image), cx);
        }
    }

    fn store_chat_image(
        &mut self,
        key: u64,
        loaded: Result<gpui::Image, ChatImageFailure>,
        cx: &mut Context<Self>,
    ) {
        let ready = loaded.is_ok();
        self.images.entries.borrow_mut().insert(
            key,
            Kept::new(match loaded {
                Ok(image) => ChatImageEntry::Ready(Arc::new(image)),
                Err(failure) => ChatImageEntry::Failed(failure),
            }),
        );
        if ready {
            self.trim_chat_images(cx);
        }
        self.list.remeasure();
        cx.notify();
    }

    /// The user turn's own pictures, right-aligned above the bubble (React: `UserImageThumbnails`).
    pub(super) fn user_image_thumbnails(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.image_row(message, true, p, cx)
    }

    /// Pictures an agent shared, left-aligned above its prose (React: `ImageAttachments`).
    pub(super) fn assistant_image_attachments(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.image_row(message, false, p, cx)
    }

    fn image_row(
        &mut self,
        message: &Value,
        user: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let images = message["images"].as_array()?.clone();
        if images.is_empty() {
            return None;
        }
        let id = text(message, "id");
        // React: `gap-2` between an agent's pictures (`ImageAttachments`), `gap-1.5` between the
        // user's own (`UserImageThumbnails`), both with `py-1`.
        let mut row = div()
            .flex()
            .flex_wrap()
            .min_w_0()
            .gap(px(if user {
                VISUAL.user_row_gap
            } else {
                VISUAL.assistant_row_gap
            } * p.scale))
            .py(px(VISUAL.row_padding_y * p.scale))
            .when(user && !p.left_aligned(), |row| row.justify_end());
        for index in 0..images.len() {
            row = row.child(self.image_tile(&id, &images, index, user, p, cx));
        }
        Some(row.into_any_element())
    }

    /// Your message's text with a loading tile after it for each of its pictures still being saved
    /// (composer_attach.rs), where the picture shows once it is in: a reference at the caret reads
    /// as a thumbnail beside the words.
    pub(super) fn with_loading_pictures(
        &self,
        id: &str,
        prompt: AnyElement,
        has_text: bool,
        p: &ChatAppearance,
    ) -> AnyElement {
        let loading = self.attaching_pictures(id);
        if loading == 0 {
            return prompt;
        }
        div()
            .flex()
            .items_center()
            .gap(px(10.0 * p.scale))
            .min_w_0()
            .when(has_text, |row| row.child(div().min_w_0().child(prompt)))
            .children((0..loading).map(|index| loading_tile(id, index, p)))
            .into_any_element()
    }

    /// How many of message `id`'s pictures are still on their way (the chat core's
    /// `attachingMessages`, packages/gx-chat-core/src/composer/attaching.rs).
    pub(super) fn attaching_pictures(&self, id: &str) -> usize {
        self.attaching_message(id)
            .and_then(|entry| entry["pictures"].as_u64())
            .unwrap_or(0) as usize
    }

    /// Message `id` waits for its pictures, and has waited long enough to say so.
    pub(super) fn attaching_status(&self, id: &str) -> bool {
        self.attaching_message(id)
            .is_some_and(|entry| entry["status"] == true)
    }

    fn attaching_message(&self, id: &str) -> Option<&Value> {
        self.snapshot["attachingMessages"]
            .as_array()?
            .iter()
            .find(|entry| entry["id"] == id)
    }

    pub(super) fn image_tile(
        &mut self,
        message_id: &str,
        images: &[Value],
        index: usize,
        user: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.image_tile_at(
            message_id,
            images,
            index,
            user,
            VISUAL.thumbnail_size,
            p,
            cx,
        )
    }

    /// `image_tile` as a `side`-point square (unscaled): the read-picture tiles use a row-sized one.
    pub(super) fn image_tile_at(
        &mut self,
        message_id: &str,
        images: &[Value],
        index: usize,
        user: bool,
        side: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let image = &images[index];
        let label = text(image, "label");
        let alt = text(image, "alt");
        let source = self.chat_image_tile(image, p, cx);
        let open = image.clone();
        let tile = div()
            .id(gpui::SharedString::from(format!(
                "chat-image:{message_id}:{index}"
            )))
            .role(gpui::Role::Button)
            .aria_label(if alt.is_empty() {
                format!("View {label}")
            } else {
                format!("View {alt}")
            })
            .flex_shrink_0()
            .chat_cursor_pointer()
            // The picture opens alone: a tile on a tool row does not also open or close the row.
            .on_click(cx.listener(move |chat, _, _, cx| {
                cx.stop_propagation();
                chat.open_transcript_picture(&open, cx)
            }))
            .file_menu(image_file_path(image), cx);
        let failure = match &source {
            ChatImageTile::Unavailable(failure) => Some(failure.clone()),
            _ => None,
        };
        if let Some(picture) = thumbnail_at(&source, side, p) {
            return tile.child(picture).into_any_element();
        }
        // A file that has since gone, or bytes that would not decode: still a tile that opens the
        // preview, where the card says why (image_viewer/unavailable.rs).
        let title = failure
            .as_ref()
            .map(|failure| super::image_viewer::failure_copy(failure).0)
            .unwrap_or("Image unavailable");
        let named = if label.is_empty() {
            format!("Image #{}", index + 1)
        } else {
            file_name_of(&label)
        };
        let tooltip = gpui::SharedString::from(format!("{title}: {named}"));
        let tile = tile.tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        });
        if user {
            /*
            CDXC:SessionChat 2026-10-01 DECISION:
            User: the chat's own thumbnail for a picture that cannot be shown is a small
            missing-image placeholder rather than a broken preview, consistent with the preview's
            card. The user's row draws the thumbnail's square with a dashed hairline and a crossed
            photo; the agent's row keeps its named chip with the same glyph.
            */
            let size = px(side * s);
            return tile
                .child(
                    div()
                        .size(size)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(VISUAL.thumbnail_radius * s))
                        .border(px(VISUAL.border_width * s))
                        .border_dashed()
                        .border_color(p.border)
                        .bg(p.input.opacity(0.5))
                        .child(
                            svg()
                                .path("titlebar/photo-off.svg")
                                .size(px(18.0f32.min(side * 0.55) * s))
                                .text_color(p.muted),
                        ),
                )
                .into_any_element();
        }
        tile.child(missing_chip(&named, p)).into_any_element()
    }

    /// A picture written into prose, at the position its author wrote it.
    ///
    /// React drew the same 3rem thumbnail for a Markdown image and for a link that names a picture
    /// (`SessionChatInlineImage`); the shared projection marks both, so a `data:` URL and a machine
    /// path load through this transport instead of the blank band a `TextView` leaves behind. When
    /// the bytes cannot be read the picture's own words stand in, exactly as React's fallback did.
    pub(super) fn inline_image(
        &self,
        id: &str,
        image: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let label = text(image, "label");
        let alt = text(image, "alt");
        let source = self.chat_image_tile(image, p, cx);
        let open = image.clone();
        match thumbnail(&source, p) {
            // React leaves 4px either side and 2px above and below a picture written into prose
            // (`.ghostex-chat-markdown .ghostex-chat-inline-image-frame`), so the words beside it
            // keep their air whether or not the line wraps.
            Some(picture) => div()
                .id(gpui::SharedString::from(format!("chat-inline-image:{id}")))
                .role(gpui::Role::Button)
                .aria_label(if alt.is_empty() {
                    format!("View {label}")
                } else {
                    format!("View {alt}")
                })
                .flex_shrink_0()
                .mx(px(VISUAL.inline_margin_x * p.scale))
                .my(px(VISUAL.inline_margin_y * p.scale))
                .chat_cursor_pointer()
                // Opening the viewer consumes the press: React's picture is a
                // button, which its row's click handler skips over.
                .on_click(cx.listener(move |chat, _, _, cx| {
                    cx.stop_propagation();
                    chat.open_transcript_picture(&open, cx);
                }))
                .file_menu(image_file_path(image), cx)
                .child(picture)
                .into_any_element(),
            // The picture's words with the crossed photo before them, still opening the preview
            // so its card can say why the picture is not there.
            None => {
                let title = match &source {
                    ChatImageTile::Unavailable(failure) => {
                        super::image_viewer::failure_copy(failure).0
                    }
                    _ => "Image unavailable",
                };
                let named = if label.is_empty() {
                    "Image".to_owned()
                } else {
                    file_name_of(&label)
                };
                let tooltip = gpui::SharedString::from(format!("{title}: {named}"));
                div()
                    .id(gpui::SharedString::from(format!("chat-inline-image:{id}")))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{title}: {named}"))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(4.0 * p.scale))
                    .mx(px(VISUAL.inline_margin_x * p.scale))
                    .px(px(6.0 * p.scale))
                    .rounded(px(6.0 * p.scale))
                    .border(px(VISUAL.border_width * p.scale))
                    .border_dashed()
                    .border_color(p.border)
                    .text_color(p.muted)
                    .chat_cursor_pointer()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        cx.stop_propagation();
                        chat.open_transcript_picture(&open, cx);
                    }))
                    .file_menu(image_file_path(image), cx)
                    .child(
                        svg()
                            .path("titlebar/photo-off.svg")
                            .size(px(13.0 * p.scale))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(named)
                    .into_any_element()
            }
        }
    }

    /// The picture a filmstrip tile paints, shrunk once to the strip's height at its own shape, with
    /// the source's size when it is known.
    fn chat_strip_picture(
        &self,
        image: &Value,
        height: f32,
        max_width: f32,
        cx: &Context<Self>,
    ) -> (ChatImageTile, Option<(u32, u32)>) {
        let device = |logical: f32| (logical * THUMBNAIL_PIXEL_RATIO).ceil().max(1.0) as u32;
        // A shrunk copy is drawn without the picture's bytes, which the cache may have let go.
        let key = image_source_id(image).map(|id| (id, device(height), device(max_width)));
        let whole_size = match key.and_then(|key| {
            drawn(&self.images.strip, &key, |picture| match picture {
                ChatStripPicture::Ready(shrunk, size) => {
                    Err((ChatImageTile::Thumbnail(shrunk.clone()), Some(*size)))
                }
                ChatStripPicture::Loading => Err((ChatImageTile::Loading, None)),
                ChatStripPicture::Whole(size) => Ok(*size),
            })
        }) {
            Some(Err(drawn)) => return drawn,
            Some(Ok(size)) => Some(size),
            None => None,
        };
        let picture = match self.chat_image(image, cx) {
            ChatImageSource::Uri(url) => return (ChatImageTile::Uri(url), None),
            ChatImageSource::Loading => return (ChatImageTile::Loading, None),
            ChatImageSource::Unavailable(failure) => {
                return (ChatImageTile::Unavailable(failure), None);
            }
            ChatImageSource::Bytes(picture) => picture,
        };
        if let Some(size) = whole_size {
            return (ChatImageTile::Whole(picture), size);
        }
        let Some(key) = key else {
            return (ChatImageTile::Whole(picture), None);
        };
        self.images
            .strip
            .borrow_mut()
            .insert(key, Kept::new(ChatStripPicture::Loading));
        let source = picture.clone();
        let task = cx
            .background_executor()
            .spawn(async move { build_strip_picture(&source, key.1, key.2) });
        cx.spawn(async move |this, cx| {
            let (shrunk, size) = task.await;
            let _ = this.update(cx, |this, cx| {
                let ready = shrunk.is_some() && size.is_some();
                this.images.strip.borrow_mut().insert(
                    key,
                    Kept::new(match (shrunk, size) {
                        (Some(shrunk), Some(size)) => {
                            ChatStripPicture::Ready(Arc::new(shrunk), size)
                        }
                        (_, size) => ChatStripPicture::Whole(size),
                    }),
                );
                if ready {
                    this.trim_chat_images(cx);
                }
                this.list.remeasure();
                cx.notify();
            });
        })
        .detach();
        (ChatImageTile::Loading, None)
    }

    /// Pictures an agent wrote on lines of their own: one row at each picture's own shape with its
    /// file name under it, scrolling sideways when the row is wider than the transcript.
    ///
    /// The rule that groups the lines is `parse` in rich_markdown.rs; this only draws the strip.
    pub(super) fn picture_strip(
        &self,
        id: &str,
        strip: &super::rich_markdown::PictureLines,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let gutter = super::markdown_style::LIST_GUTTER * s;
        let single = strip.images.len() == 1;
        let (height, max_width) = if single {
            (VISUAL.single_height, VISUAL.single_max_width)
        } else {
            (VISUAL.strip_height, VISUAL.strip_max_width)
        };
        let row =
            div()
                .id(gpui::SharedString::from(format!("chat-image-strip:{id}")))
                .flex()
                .items_start()
                .gap(px(VISUAL.strip_gap * s))
                .min_w_0()
                .flex_1()
                .pb(px(VISUAL.strip_padding_bottom * s))
                .overflow_x_scroll()
                // A wheel turned over the strip still scrolls the transcript; only a sideways swipe
                // moves the strip.
                .restrict_scroll_to_axis()
                .children((0..strip.images.len()).map(|index| {
                    self.strip_tile(id, &strip.images, index, height, max_width, p, cx)
                }));
        div()
            .flex()
            .items_start()
            .w_full()
            .min_w_0()
            .pl(px(gutter * strip.depth as f32))
            .when_some(strip.marker.clone(), |line, marker| {
                // The list item the pictures open, its marker in the gutter the list's own uses.
                line.child(
                    div()
                        .flex()
                        .justify_end()
                        .flex_shrink_0()
                        .min_w(px(gutter))
                        .pr(px(4.0 * s))
                        .text_size(px(14.0 * s))
                        .text_color(p.muted)
                        .child(marker),
                )
            })
            .child(row)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn strip_tile(
        &self,
        id: &str,
        images: &[Value],
        index: usize,
        height: f32,
        max_width: f32,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let image = &images[index];
        let label = text(image, "label");
        let alt = text(image, "alt");
        let named = if label.is_empty() {
            format!("Image #{}", index + 1)
        } else {
            file_name_of(&label)
        };
        let (source, size) = self.chat_strip_picture(image, height * s, max_width * s, cx);
        let open = image.clone();
        let tile = div()
            .id(gpui::SharedString::from(format!(
                "chat-strip-image:{id}:{index}"
            )))
            .role(gpui::Role::Button)
            .aria_label(if alt.is_empty() {
                format!("View {named}")
            } else {
                format!("View {alt}")
            })
            .flex_shrink_0()
            .chat_cursor_pointer()
            .on_click(cx.listener(move |chat, _, _, cx| {
                cx.stop_propagation();
                chat.open_transcript_picture(&open, cx);
            }))
            .file_menu(image_file_path(image), cx);
        if let ChatImageTile::Unavailable(failure) = &source {
            // The same named chip the agent's own attachments draw for a picture that is gone.
            let tooltip = gpui::SharedString::from(format!(
                "{}: {named}",
                super::image_viewer::failure_copy(failure).0
            ));
            return tile
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(missing_chip(&named, p))
                .into_any_element();
        }
        // The frame is the picture's own shape: as wide as the strip's height allows, and a picture
        // wider than the cap gets shorter rather than black bars. An unknown shape (still loading,
        // or fetched by GPUI) takes a screenshot's 16:10.
        let aspect = size
            .filter(|(width, height)| *width > 0 && *height > 0)
            .map_or(1.6, |(width, height)| width as f32 / height as f32);
        let width = (height * aspect).min(max_width);
        let frame_height = width / aspect;
        let frame = div()
            .w(px(width * s))
            .h(px(frame_height * s))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .rounded(px(VISUAL.strip_radius * s))
            .border(px(VISUAL.border_width * s))
            .border_color(p.border)
            .bg(gpui::black())
            .group_hover("chat-strip-tile", |frame| {
                frame.border_color(gpui::white().opacity(0.28))
            });
        let picture = |picture: gpui::Img| {
            picture
                .size_full()
                .object_fit(gpui::ObjectFit::Contain)
                .into_any_element()
        };
        let frame = match source {
            ChatImageTile::Uri(url) => frame.child(picture(img(url))),
            ChatImageTile::Thumbnail(shrunk) => frame.child(picture(img(shrunk))),
            ChatImageTile::Whole(bytes) => frame.child(picture(img(bytes))),
            _ => frame.border_dashed().bg(gpui::transparent_black()).child(
                svg()
                    .path("titlebar/loader2.svg")
                    .size(px(VISUAL.loading_icon_size * s))
                    .text_color(p.muted)
                    .with_throttled_animation(
                        "chat-image-spinner",
                        std::time::Duration::from_millis(900),
                        |svg, delta| {
                            svg.with_transformation(gpui::Transformation::rotate(gpui::radians(
                                delta * std::f32::consts::TAU,
                            )))
                        },
                    ),
            ),
        };
        tile.group("chat-strip-tile")
            // A narrow picture still leaves its name room to be read.
            .w(px(width.max(VISUAL.caption_min_width) * s))
            .flex()
            .flex_col()
            .gap(px(VISUAL.caption_gap * s))
            .child(frame)
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.0 * s))
                    .min_w_0()
                    .text_size(px(VISUAL.caption_size * s))
                    .line_height(px(VISUAL.caption_size * 1.3 * s))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(p.card_muted)
                            .child(named),
                    )
                    .when_some(size, |caption, (width, height)| {
                        caption.child(
                            div()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .text_color(p.muted)
                                .child(format!("{width}×{height}")),
                        )
                    }),
            )
            .into_any_element()
    }
}

/// The card an agent's picture becomes when its bytes cannot be shown: the crossed photo in a
/// small well and the file's name.
///
/// React's `Attachment size='xs'`: a 12px-radius card with a 4px inset, a 28px rounded media well
/// for the icon, and a medium-weight title in the card's own colour. Its `min-w-40` is what keeps
/// a short file name from shrinking the card to a pill.
fn missing_chip(named: &str, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex()
        .items_center()
        .gap(px(6.0 * s))
        .min_w(px(160.0 * s))
        .max_w(px(220.0 * s))
        .p(px(4.0 * s))
        .rounded(px(12.0 * s))
        .border(px(s))
        .border_color(p.border)
        .bg(p.card_background)
        .text_size(px(12.0 * s))
        .text_color(p.foreground)
        .child(
            div()
                .flex_shrink_0()
                .size(px(28.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0 * s))
                .bg(p.input)
                .child(
                    svg()
                        .path("titlebar/photo-off.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                ),
        )
        .child(
            div()
                .min_w_0()
                .px(px(6.0 * s))
                .py(px(4.0 * s))
                .truncate()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(named.to_owned()),
        )
        .into_any_element()
}

/// A picture of your message still being saved: the chat box's uploading tile, at the size the
/// picture will take.
fn loading_tile(message_id: &str, index: usize, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let glyph = gpui::svg()
        .path("titlebar/loader2.svg")
        .size(px(16.0 * s))
        .text_color(p.muted);
    let glyph = if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
        glyph.into_any_element()
    } else {
        glyph
            .with_throttled_animation(
                gpui::SharedString::from(format!("chat-image-loading:{message_id}:{index}")),
                std::time::Duration::from_millis(900),
                |svg, delta| {
                    svg.with_transformation(gpui::Transformation::rotate(gpui::radians(
                        delta * std::f32::consts::TAU,
                    )))
                },
            )
            .into_any_element()
    };
    div()
        .id(gpui::SharedString::from(format!(
            "chat-image-loading:{message_id}:{index}"
        )))
        .role(gpui::Role::Status)
        .aria_label("Attaching the picture")
        .flex_shrink_0()
        .size(px(VISUAL.thumbnail_size * s))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(VISUAL.thumbnail_radius * s))
        .border(px(VISUAL.border_width * s))
        .border_dashed()
        .border_color(p.input_border)
        .child(glyph)
        .into_any_element()
}
