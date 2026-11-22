//! Art themes: an artwork painted behind the sidebar and the work area of an opaque window
//! (`windowArt`, `windowArtDim`, `windowArtBlur`). The window never lets the desktop through; the
//! surfaces over the artwork take the glass tints, so every view that already knows how to sit on
//! glass (`window_glass_active`) sits on the artwork the same way. The desktop and the GPUI web
//! build both compile this file (apps/gpui-web/src/app/helpers/window_art.rs is a link to it).
//!
//! CDXC:Theming 2026-11-21 WHY:
//! The picture backdrop Glass already has (Behind the glass > Picture) is drawn by each platform's window backend behind a see-through window, so it needs window glass, never reaches the web build, and lets the desktop show through wherever a surface is clear. Art is painted by GPUI itself as the window's bottom layer instead: one decoded bitmap per artwork, drawn as a single image quad and a dimming quad each frame, the same on macOS, Windows, Linux and in the browser.
//!
//! CDXC:Theming 2026-11-21 WHY:
//! No artwork ships inside the app (CDXC:Theming 2026-11-21 DECISION on `ARTWORKS` in packages/settings-catalog/src/data/themes.rs). gxserver downloads and keeps it (server/src/theme/art.rs); the desktop reads the file gxserver names, the web build gets its bytes in the answer. It is decoded (and blurred, when `windowArtBlur` asks) once, off the main thread, and only the artwork in use is kept: switching artwork frees the previous bitmap. Until it is here the window shows the theme's own colour, solid, and the artwork fades in when it arrives (at once with Reduce Motion); when it can't be downloaded (offline) the window stays solid and the next change of artwork or appearance tries again. Dark mode paints `windowArt`, light mode `windowArtLight`.
//! SEE-ALSO: app/helpers/window_glass.rs (`refresh_window_glass`, `window_glass_background_appearance`), packages/settings-catalog/src/data/themes.rs (the artworks and the themes' Art backgrounds), apps/gpui-web/src/app/helpers/web.rs.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, App, IntoElement, ObjectFit, ParentElement as _,
    RenderImage, Styled as _, StyledImage as _, div, img,
};

/// How long a newly arrived artwork takes to fade in.
const ART_FADE_IN: Duration = Duration::from_millis(450);

/// The artwork the settings ask for in the current appearance: its id, dim and blur, and whether
/// it is painted under light mode (the dimming is then a white wash). `picture` tells Your image's
/// pictures apart (a hash of the kept file's path; 0 for the artworks).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ArtChoice {
    id: &'static str,
    picture: u64,
    dim: u8,
    blur: u8,
    light: bool,
}

/// A decoded artwork's identity: the artwork (or Your image's picture) and its blur.
type ArtKey = (&'static str, u64, u8);

/// A small JPEG of the active artwork (toned and blurred as the main window draws it), for the
/// floating Live page window to soften further (`visual_page_modal/glass.rs`).
#[cfg(not(ghostex_web))]
pub(crate) struct WindowArtSnapshot {
    pub(crate) jpeg: Vec<u8>,
    pub(crate) size: gpui::Size<gpui::Pixels>,
    pub(crate) dim: f32,
    pub(crate) light: bool,
}

#[cfg(not(ghostex_web))]
type ArtPreview = Option<(Vec<u8>, (u32, u32))>;

#[cfg(not(ghostex_web))]
const ART_PREVIEW_MAX_WIDTH: u32 = 640;

static WINDOW_ART: Mutex<Option<ArtChoice>> = Mutex::new(None);
/// An artwork is chosen for the appearance the app shows.
static WINDOW_ART_ACTIVE: AtomicBool = AtomicBool::new(false);
/// The sidebar's and the work area's tint over the artwork, percent (the dark glass tints).
static ART_SIDEBAR_PERCENT: AtomicU8 = AtomicU8::new(88);
static ART_WORK_AREA_PERCENT: AtomicU8 = AtomicU8::new(88);

/// The decoded artwork in use (by id and blur), the one being fetched and decoded, and the last
/// one that could not be had (not tried again until another is asked for).
struct ReadyArt {
    key: ArtKey,
    image: Arc<RenderImage>,
    #[cfg(not(ghostex_web))]
    preview: ArtPreview,
}

struct Decoded {
    ready: Option<ReadyArt>,
    pending: Option<ArtKey>,
    failed: Option<ArtKey>,
}

static DECODED: Mutex<Decoded> = Mutex::new(Decoded {
    ready: None,
    pending: None,
    failed: None,
});

fn read_number(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<f64> {
    object
        .get(key)
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .filter(|value| value.is_finite())
}

fn catalog_number(key: &str) -> f64 {
    ghostex_settings_catalog::default_value(key)
        .and_then(ghostex_settings_catalog::J::as_f64)
        .unwrap_or(0.0)
}

/// Reads the art settings. Call after the appearance is known (`CHROME_LIGHT_APPEARANCE`).
/// Returns whether art turned on or off.
pub(crate) fn refresh_window_art(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    let light = crate::CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    let art_key = if light { "windowArtLight" } else { "windowArt" };
    let percent = |key: &str, max: f64| {
        read_number(object, key)
            .unwrap_or_else(|| catalog_number(key))
            .clamp(0.0, max)
            .round() as u8
    };
    let chosen = object
        .get(art_key)
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            ghostex_settings_catalog::default_value(art_key)
                .and_then(ghostex_settings_catalog::J::as_str)
        })
        .unwrap_or_default();
    // Your image (CDXC:Theming 2026-11-22 on `YOUR_IMAGE_ART_ID` in the settings catalog's
    // data/themes.rs): the picture gxserver keeps, named by its path; none chosen paints nothing.
    let your_picture = (chosen == ghostex_settings_catalog::YOUR_IMAGE_ART_ID)
        .then(|| {
            object
                .get(ghostex_settings_catalog::your_image_key(light))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
        })
        .flatten()
        .map(|path| {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            path.hash(&mut hasher);
            hasher.finish().max(1)
        });
    let id_and_picture = match your_picture {
        Some(picture) => Some((ghostex_settings_catalog::YOUR_IMAGE_ART_ID, picture)),
        None => ghostex_settings_catalog::artwork_by_id(chosen).map(|artwork| (artwork.id, 0)),
    };
    let choice = id_and_picture.map(|(id, picture)| ArtChoice {
        id,
        picture,
        dim: percent("windowArtDim", ghostex_settings_catalog::MAX_WINDOW_ART_DIM),
        blur: percent(
            "windowArtBlur",
            ghostex_settings_catalog::MAX_WINDOW_ART_BLUR,
        ),
        light,
    });
    let (sidebar, work_area) = if light {
        (
            ghostex_settings_catalog::window_glass_light_key(
                object,
                "windowGlassSidebarOpacityLight",
            ),
            ghostex_settings_catalog::window_glass_light_key(
                object,
                "windowGlassWorkAreaTintLight",
            ),
        )
    } else {
        (
            "windowGlassSidebarOpacityDark",
            "windowGlassWorkAreaTintDark",
        )
    };
    ART_SIDEBAR_PERCENT.store(percent(sidebar, 100.0), Ordering::Relaxed);
    ART_WORK_AREA_PERCENT.store(percent(work_area, 100.0), Ordering::Relaxed);
    if let Ok(mut art) = WINDOW_ART.lock()
        && *art != choice
    {
        *art = choice;
        // A change of artwork or appearance tries an artwork that could not be had again.
        if let Ok(mut decoded) = DECODED.lock() {
            decoded.failed = None;
        }
    }
    let active = choice.is_some();
    WINDOW_ART_ACTIVE.swap(active, Ordering::Relaxed) != active
}

/// Whether the window paints an artwork behind its surfaces now.
pub(crate) fn window_art_active() -> bool {
    WINDOW_ART_ACTIVE.load(Ordering::Relaxed)
}

/// How much of the artwork the sidebar's tint covers.
pub(crate) fn window_art_sidebar_alpha() -> f32 {
    f32::from(ART_SIDEBAR_PERCENT.load(Ordering::Relaxed)) / 100.0
}

/// How much of the artwork the work area's tint covers.
pub(crate) fn window_art_work_area_alpha() -> f32 {
    f32::from(ART_WORK_AREA_PERCENT.load(Ordering::Relaxed)) / 100.0
}

/// Changes whenever what the backdrop draws changes, for views that redraw on their own key.
pub(crate) fn window_art_redraw_key() -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    window_art_active().hash(&mut hasher);
    if let Ok(art) = WINDOW_ART.lock() {
        art.map(|art| (art.id, art.picture, art.dim, art.blur))
            .hash(&mut hasher);
    }
    ART_SIDEBAR_PERCENT
        .load(Ordering::Relaxed)
        .hash(&mut hasher);
    ART_WORK_AREA_PERCENT
        .load(Ordering::Relaxed)
        .hash(&mut hasher);
    hasher.finish()
}

/// How much of its colour saturation a built-in artwork keeps behind the window
/// (CDXC:Theming 2026-11-21, Q4388 subtle rule: Art backgrounds dimmed to 45% and shown at 70%
/// of their colour).
const ART_COLOUR_SATURATION: f32 = 0.70;

/// The longest side Your image is decoded to: a picture larger than any window is scaled down
/// first, so a huge photo costs no more memory than an artwork.
const YOUR_IMAGE_MAX_SIDE: u32 = 3840;

/// Resizes a toned RGBA artwork to at most [`ART_PREVIEW_MAX_WIDTH`] pixels wide and encodes it as
/// a JPEG for the floating Live page window (`window_art_snapshot`).
#[cfg(not(ghostex_web))]
fn encode_art_preview(rgba: &image::RgbaImage) -> ArtPreview {
    let (width, height) = (rgba.width(), rgba.height());
    if width == 0 || height == 0 {
        return None;
    }
    let out_width = width.min(ART_PREVIEW_MAX_WIDTH);
    let out_height = ((height as f32 * out_width as f32 / width as f32).round() as u32).max(1);
    let small = if out_width == width && out_height == height {
        rgba.clone()
    } else {
        image::imageops::resize(
            rgba,
            out_width,
            out_height,
            image::imageops::FilterType::Triangle,
        )
    };
    let rgb = image::DynamicImage::ImageRgba8(small).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&rgb)
        .ok()?;
    Some((out, (width, height)))
}

/// Decodes `bytes` into the BGRA bitmap GPUI draws, blurred by `blur` points first and toned to
/// [`ART_COLOUR_SATURATION`] of its colour. An artwork is WebP; Your image is any picture format
/// gxserver accepted (server/src/theme/art.rs), scaled to [`YOUR_IMAGE_MAX_SIDE`].
#[cfg(not(ghostex_web))]
type DecodedArtwork = (Arc<RenderImage>, ArtPreview);

#[cfg(ghostex_web)]
type DecodedArtwork = Arc<RenderImage>;

fn decode_artwork(bytes: &[u8], blur: u8, yours: bool) -> Option<DecodedArtwork> {
    let decoded = if yours {
        let picture = image::load_from_memory(bytes).ok()?;
        if picture.width().max(picture.height()) > YOUR_IMAGE_MAX_SIDE {
            picture.resize(
                YOUR_IMAGE_MAX_SIDE,
                YOUR_IMAGE_MAX_SIDE,
                image::imageops::FilterType::Triangle,
            )
        } else {
            picture
        }
    } else {
        image::load_from_memory_with_format(bytes, image::ImageFormat::WebP).ok()?
    };
    let mut rgba = decoded.into_rgba8();
    if blur > 0 {
        // The artwork is about 1.5 times the size it covers on a common window, so a point of
        // blur is about a pixel and a half of it.
        rgba = image::imageops::fast_blur(&rgba, f32::from(blur) * 1.5);
    }
    for pixel in rgba.pixels_mut() {
        let r = f32::from(pixel.0[0]);
        let g = f32::from(pixel.0[1]);
        let b = f32::from(pixel.0[2]);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let tone = |channel: f32| {
            (luma + (channel - luma) * ART_COLOUR_SATURATION)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        pixel.0[0] = tone(r);
        pixel.0[1] = tone(g);
        pixel.0[2] = tone(b);
    }
    #[cfg(not(ghostex_web))]
    let preview = encode_art_preview(&rgba);
    // GPUI RenderImage expects BGRA.
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let image = Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]));
    #[cfg(not(ghostex_web))]
    {
        Some((image, preview))
    }
    #[cfg(ghostex_web)]
    {
        Some(image)
    }
}

/// The artwork's bytes from gxserver, which downloads it the first time (server/src/theme/art.rs):
/// on the desktop the file it keeps, in the browser the bytes in its answer. Your image is the
/// picture gxserver keeps for the appearance `light` names.
async fn fetch_artwork(id: &'static str, light: bool) -> Option<Vec<u8>> {
    let inline = cfg!(ghostex_web);
    let answer = crate::app::gx_store::gx_rpc(
        None,
        "/api/theme",
        serde_json::json!({ "op": "art", "id": id, "light": light, "inline": inline }),
    )
    .await
    .ok()?;
    if inline {
        use base64::Engine as _;
        return base64::engine::general_purpose::STANDARD
            .decode(answer.get("bytesBase64")?.as_str()?)
            .ok();
    }
    #[cfg(not(ghostex_web))]
    {
        return std::fs::read(answer.get("path")?.as_str()?).ok();
    }
    #[allow(unreachable_code)]
    None
}

/// The decoded artwork for `choice`, or `None` while it is being fetched and decoded (that starts
/// here the first time it is asked for, and every window redraws when it is ready) or when it
/// could not be had.
fn artwork_image(choice: ArtChoice, cx: &mut App) -> Option<Arc<RenderImage>> {
    let key = (choice.id, choice.picture, choice.blur);
    let mut decoded = DECODED.lock().ok()?;
    if let Some(ready) = &decoded.ready
        && ready.key == key
    {
        return Some(ready.image.clone());
    }
    if decoded.pending == Some(key) || decoded.failed == Some(key) {
        return None;
    }
    decoded.pending = Some(key);
    decoded.failed = None;
    drop(decoded);
    let background = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let yours = choice.id == ghostex_settings_catalog::YOUR_IMAGE_ART_ID;
        let result = match fetch_artwork(choice.id, choice.light).await {
            Some(bytes) => {
                background
                    .spawn(async move { decode_artwork(&bytes, choice.blur, yours) })
                    .await
            }
            None => None,
        };
        let _ = cx.update(|cx| {
            let previous = {
                let Ok(mut decoded) = DECODED.lock() else {
                    return;
                };
                if decoded.pending != Some(key) {
                    // Another artwork was asked for meanwhile; its own fetch will land.
                    return;
                }
                decoded.pending = None;
                let Some(result) = result else {
                    // Offline or refused: the window stays solid in the theme's colour.
                    decoded.failed = Some(key);
                    return;
                };
                #[cfg(not(ghostex_web))]
                let entry = ReadyArt {
                    key,
                    image: result.0,
                    preview: result.1,
                };
                #[cfg(ghostex_web)]
                let entry = ReadyArt { key, image: result };
                decoded.ready.replace(entry)
            };
            if let Some(previous) = previous {
                cx.drop_image(previous.image, None);
            }
            cx.refresh_windows();
        });
    })
    .detach();
    None
}

/// The active artwork's JPEG preview, aspect size, dimming and appearance, for the floating Live
/// page window (`visual_page_modal/glass.rs`). Starts fetching the artwork if it is not decoded
/// yet, and returns `None` when no artwork is active or while it is still loading.
#[cfg(not(ghostex_web))]
pub(crate) fn window_art_snapshot(cx: &mut App) -> Option<WindowArtSnapshot> {
    if !window_art_active() {
        return None;
    }
    let choice = (*WINDOW_ART.lock().ok()?)?;
    let _ = artwork_image(choice, cx)?;
    let key = (choice.id, choice.picture, choice.blur);
    let decoded = DECODED.lock().ok()?;
    let ready = decoded.ready.as_ref()?;
    if ready.key != key {
        return None;
    }
    let (jpeg, (width, height)) = ready.preview.as_ref()?;
    Some(WindowArtSnapshot {
        jpeg: jpeg.clone(),
        size: gpui::size(gpui::px(*width as f32), gpui::px(*height as f32)),
        dim: f32::from(choice.dim) / 100.0,
        light: choice.light,
    })
}

/// The window's bottom layer while an artwork shows: the theme's own colour, the artwork covering
/// the window (centred, cropped to fill) fading in the first time it is drawn, and the dimming over
/// it (black in dark mode, white in light mode). Put it first in a window's root (`relative`),
/// under everything else.
pub(crate) fn window_art_backdrop(cx: &mut App) -> Option<AnyElement> {
    if !window_art_active() {
        return None;
    }
    let choice = (*WINDOW_ART.lock().ok()?)?;
    let image = artwork_image(choice, cx);
    let base = crate::app::helpers::workspace_background_color().opacity(1.0);
    let wash = if choice.light {
        gpui::white()
    } else {
        gpui::black()
    };
    let still = crate::app::helpers::gpui_macos_reduce_motion_enabled();
    let dim = f32::from(choice.dim) / 100.0;
    let picture = image.map(|image| {
        let picture = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                img(image)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(wash.opacity(dim)),
            );
        if still {
            picture.into_any_element()
        } else {
            // Keyed by the artwork, so it fades in once when it arrives, not on every redraw.
            picture
                .with_animation(
                    gpui::ElementId::Name(
                        format!(
                            "window-art-{}-{}-{}",
                            choice.id, choice.picture, choice.blur
                        )
                        .into(),
                    ),
                    Animation::new(ART_FADE_IN),
                    |picture, delta| picture.opacity(delta),
                )
                .into_any_element()
        }
    });
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden()
            .bg(base)
            .children(picture)
            .into_any_element(),
    )
}
