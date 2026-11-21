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
//! The artwork is decoded (and blurred, when `windowArtBlur` asks for it) once, off the main thread, and only the artwork in use is kept: switching artwork frees the previous bitmap. Until the first decode finishes the window shows the theme's own colour. Art is dark mode only (the art themes are dark themes); light mode keeps its own look.
//! SEE-ALSO: app/helpers/window_glass.rs (`refresh_window_glass`, `window_glass_background_appearance`), packages/settings-catalog/src/data/theme_gallery.rs (the art looks), apps/gpui-web/src/app/helpers/web.rs.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use gpui::{
    AnyElement, App, IntoElement, ObjectFit, ParentElement as _, RenderImage, Styled as _,
    StyledImage as _, div, img,
};

macro_rules! artwork {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../../desktop/assets/themes/art/",
            $name,
            ".webp"
        ))
    };
}

/// The artworks the art themes paint, by `windowArt` value (`WINDOW_ART_OPTIONS`).
fn artwork_bytes(id: &str) -> Option<(&'static str, &'static [u8])> {
    Some(match id {
        "moonwater" => ("moonwater", artwork!("moonwater")),
        "glasshouse" => ("glasshouse", artwork!("glasshouse")),
        "red-earth" => ("red-earth", artwork!("red-earth")),
        "deep-current" => ("deep-current", artwork!("deep-current")),
        "night-garden" => ("night-garden", artwork!("night-garden")),
        "nocturne-canyon" => ("nocturne-canyon", artwork!("nocturne-canyon")),
        _ => return None,
    })
}

/// The artwork the settings ask for, whatever the appearance: its id, dim and blur.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ArtChoice {
    id: &'static str,
    dim: u8,
    blur: u8,
}

static WINDOW_ART: Mutex<Option<ArtChoice>> = Mutex::new(None);
/// An artwork is chosen and the app shows dark mode.
static WINDOW_ART_ACTIVE: AtomicBool = AtomicBool::new(false);
/// The sidebar's and the work area's tint over the artwork, percent (the dark glass tints).
static ART_SIDEBAR_PERCENT: AtomicU8 = AtomicU8::new(88);
static ART_WORK_AREA_PERCENT: AtomicU8 = AtomicU8::new(88);

/// The decoded artwork in use (by id and blur), and the one being decoded.
struct Decoded {
    ready: Option<((&'static str, u8), Arc<RenderImage>)>,
    pending: Option<(&'static str, u8)>,
}

static DECODED: Mutex<Decoded> = Mutex::new(Decoded {
    ready: None,
    pending: None,
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
    let choice = object
        .get("windowArt")
        .and_then(serde_json::Value::as_str)
        .and_then(artwork_bytes)
        .map(|(id, _)| {
            let percent = |key: &str, max: f64| {
                read_number(object, key)
                    .unwrap_or_else(|| catalog_number(key))
                    .clamp(0.0, max)
                    .round() as u8
            };
            ArtChoice {
                id,
                dim: percent("windowArtDim", ghostex_settings_catalog::MAX_WINDOW_ART_DIM),
                blur: percent(
                    "windowArtBlur",
                    ghostex_settings_catalog::MAX_WINDOW_ART_BLUR,
                ),
            }
        });
    let tint = |key: &str| {
        read_number(object, key)
            .unwrap_or_else(|| catalog_number(key))
            .clamp(0.0, 100.0)
            .round() as u8
    };
    ART_SIDEBAR_PERCENT.store(tint("windowGlassSidebarOpacityDark"), Ordering::Relaxed);
    ART_WORK_AREA_PERCENT.store(tint("windowGlassWorkAreaTintDark"), Ordering::Relaxed);
    if let Ok(mut art) = WINDOW_ART.lock() {
        *art = choice;
    }
    let light = crate::CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    let active = choice.is_some() && !light;
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
        art.map(|art| (art.id, art.dim, art.blur)).hash(&mut hasher);
    }
    ART_SIDEBAR_PERCENT
        .load(Ordering::Relaxed)
        .hash(&mut hasher);
    ART_WORK_AREA_PERCENT
        .load(Ordering::Relaxed)
        .hash(&mut hasher);
    hasher.finish()
}

/// Decodes `bytes` into the BGRA bitmap GPUI draws, blurred by `blur` points first.
fn decode_artwork(bytes: &'static [u8], blur: u8) -> Option<Arc<RenderImage>> {
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP).ok()?;
    let mut rgba = decoded.into_rgba8();
    if blur > 0 {
        // The artwork is about 1.5 times the size it covers on a common window, so a point of
        // blur is about a pixel and a half of it.
        rgba = image::imageops::fast_blur(&rgba, f32::from(blur) * 1.5);
    }
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}

/// The decoded artwork for `choice`, or `None` while it is being decoded (the decode starts here
/// the first time it is asked for, and every window redraws when it is ready).
fn artwork_image(choice: ArtChoice, cx: &mut App) -> Option<Arc<RenderImage>> {
    let key = (choice.id, choice.blur);
    let mut decoded = DECODED.lock().ok()?;
    if let Some((ready_key, image)) = &decoded.ready
        && *ready_key == key
    {
        return Some(image.clone());
    }
    if decoded.pending == Some(key) {
        return None;
    }
    decoded.pending = Some(key);
    drop(decoded);
    let (_, bytes) = artwork_bytes(choice.id)?;
    let task = cx
        .background_executor()
        .spawn(async move { decode_artwork(bytes, choice.blur) });
    cx.spawn(async move |cx| {
        let image = task.await;
        let _ = cx.update(|cx| {
            let previous = {
                let Ok(mut decoded) = DECODED.lock() else {
                    return;
                };
                if decoded.pending != Some(key) {
                    // Another artwork was asked for meanwhile; its own decode will land.
                    return;
                }
                // A bundled artwork that fails to decode stays pending, so it is not tried again
                // every frame; the window keeps the theme's own colour.
                let Some(image) = image else {
                    return;
                };
                decoded.pending = None;
                decoded.ready.replace((key, image))
            };
            if let Some((_, previous)) = previous {
                cx.drop_image(previous, None);
            }
            cx.refresh_windows();
        });
    })
    .detach();
    None
}

/// The window's bottom layer while an art theme is on: the theme's own colour, the artwork
/// covering the window (centred, cropped to fill), and the dimming over it. Put it first in a
/// window's root (`relative`), under everything else.
pub(crate) fn window_art_backdrop(cx: &mut App) -> Option<AnyElement> {
    if !window_art_active() {
        return None;
    }
    let choice = (*WINDOW_ART.lock().ok()?)?;
    let image = artwork_image(choice, cx);
    let base = crate::app::helpers::workspace_background_color().opacity(1.0);
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden()
            .bg(base)
            .children(image.map(|image| {
                img(image)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(gpui::black().opacity(f32::from(choice.dim) / 100.0)),
            )
            .into_any_element(),
    )
}
