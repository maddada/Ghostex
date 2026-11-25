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

use super::window_art_style::{
    ArtChoice, ArtKey, ArtStyle, ArtTone, art_choice, art_surface_percents, load_artwork_rgba,
    stylise_artwork,
};

/// How long a newly arrived artwork takes to fade in.
const ART_FADE_IN: Duration = Duration::from_millis(450);

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
    /// The same artwork blurred, grained and at half size, for glass surfaces over it
    /// (`window_art_frosted`).
    frosted: Arc<RenderImage>,
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

/// Reads the art settings. Call after the appearance is known (`CHROME_LIGHT_APPEARANCE`).
/// Returns whether art turned on or off.
pub(crate) fn refresh_window_art(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    let light = crate::CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    let choice = art_choice(object, light);
    let (sidebar, work_area) = art_surface_percents(object, light);
    ART_SIDEBAR_PERCENT.store(sidebar, Ordering::Relaxed);
    ART_WORK_AREA_PERCENT.store(work_area, Ordering::Relaxed);
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

/// Whether the artwork on screen is drawn as ASCII or Dither (`windowArtStyle`), the styles the
/// chat's reading panel and the frosted glass surfaces are made for; a Picture draws the window as
/// it was before them (CDXC:Theming 2026-11-25 DECISION on `stylise_artwork`).
pub(crate) fn window_art_stylised() -> bool {
    window_art_active()
        && WINDOW_ART
            .lock()
            .ok()
            .and_then(|art| *art)
            .is_some_and(|art| art.tone.style != ArtStyle::Picture)
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
        art.map(|art| (art.key(), art.dim)).hash(&mut hasher);
    }
    ART_SIDEBAR_PERCENT
        .load(Ordering::Relaxed)
        .hash(&mut hasher);
    ART_WORK_AREA_PERCENT
        .load(Ordering::Relaxed)
        .hash(&mut hasher);
    hasher.finish()
}

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

/// Decodes `bytes` into the BGRA bitmap GPUI draws, blurred by `blur` points first and drawn
/// in its style (`stylise_artwork`). An artwork is WebP; Your image is any picture format
/// gxserver accepted (server/src/theme/art.rs), scaled down when huge (`load_artwork_rgba`).
#[cfg(not(ghostex_web))]
type DecodedArtwork = (Arc<RenderImage>, Arc<RenderImage>, ArtPreview);

#[cfg(ghostex_web)]
type DecodedArtwork = (Arc<RenderImage>, Arc<RenderImage>);

fn decode_artwork(
    bytes: &[u8],
    blur: u8,
    yours: bool,
    tone: ArtTone,
    light: bool,
) -> Option<DecodedArtwork> {
    let mut rgba = load_artwork_rgba(bytes, blur, yours)?;
    stylise_artwork(&mut rgba, tone, light);
    // The preview (a JPEG, which has no transparency) and the frosted copy start from the artwork
    // laid on the work area colour.
    let mut flat = rgba.clone();
    let base = [
        (tone.base >> 16) as u8,
        (tone.base >> 8) as u8,
        tone.base as u8,
    ];
    for pixel in flat.pixels_mut() {
        let a = f32::from(pixel.0[3]) / 255.0;
        for channel in 0..3 {
            let under = f32::from(base[channel]);
            pixel.0[channel] = (under + (f32::from(pixel.0[channel]) - under) * a).round() as u8;
        }
        pixel.0[3] = 255;
    }
    #[cfg(not(ghostex_web))]
    let preview = encode_art_preview(&flat);
    let frosted = frost_artwork(&flat);
    drop(flat);
    // GPUI RenderImage expects BGRA.
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let image = Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]));
    #[cfg(not(ghostex_web))]
    {
        Some((image, frosted, preview))
    }
    #[cfg(ghostex_web)]
    {
        Some((image, frosted))
    }
}

/// The frosted copy's blur (in pixels of the half-size copy, so about 15 points on screen for a
/// 2560-pixel artwork on a 1920-point window) and its grain (luma steps either way).
const FROST_BLUR: f32 = 10.0;
const FROST_GRAIN: f32 = 6.0;

/// `flat` (the artwork on the work area colour) at half size, blurred and lightly grained, as the
/// BGRA bitmap a glass surface draws behind itself so it reads as frosted glass over the artwork.
fn frost_artwork(flat: &image::RgbaImage) -> Arc<RenderImage> {
    let (width, height) = ((flat.width() / 2).max(1), (flat.height() / 2).max(1));
    let small = image::imageops::resize(flat, width, height, image::imageops::FilterType::Triangle);
    let mut soft = image::imageops::fast_blur(&small, FROST_BLUR);
    for (x, y, pixel) in soft.enumerate_pixels_mut() {
        let hash =
            (x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263)).wrapping_mul(1_274_126_177);
        let grain = (((hash >> 16) & 0xff) as f32 / 255.0 * 2.0 - 1.0) * FROST_GRAIN;
        for channel in 0..3 {
            pixel.0[channel] = (f32::from(pixel.0[channel]) + grain)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        pixel.0[3] = 255;
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(soft)]))
}

/// The frosted copy of the artwork on screen and its dimming, for a glass surface over it (the chat
/// box, native_chat/composer.rs), or `None` without an ASCII or Dither artwork (a Picture keeps the
/// surfaces as they were) or while it loads. Draw it where the
/// artwork itself is drawn (the window, cropped to fill) clipped to the surface, then the dimming
/// in the work area colour, then the surface's own tint.
pub(crate) struct FrostedArt {
    pub(crate) image: Arc<RenderImage>,
    pub(crate) dim: f32,
}

pub(crate) fn window_art_frosted() -> Option<FrostedArt> {
    if !window_art_stylised() {
        return None;
    }
    let choice = (*WINDOW_ART.lock().ok()?)?;
    let decoded = DECODED.lock().ok()?;
    let ready = decoded.ready.as_ref()?;
    (ready.key == choice.key()).then(|| FrostedArt {
        image: ready.frosted.clone(),
        dim: f32::from(choice.dim) / 100.0,
    })
}

/// A dark, blurred glass backdrop for a surface over the artwork, rounded by `radius`: the frosted
/// artwork lined up with the original, its dimming, black smoke (dark mode) and the work area
/// colour over it. `None` without an artwork. Put it first in the surface (`relative`). The floating
/// sidebar card and Settings use it; the chat box no longer does (CDXC:SessionChat 2026-11-25 below).
///
/// CDXC:SessionChat 2026-11-25 DECISION:
/// User, after PR 757: "pls revert the chat box to look similar to the inbox above it: 1- wider pills bar that doesn't end before end of the chat box itself 2- bg color. also please make the scrolling in the chat transcript not bring the chat transcript behind my inbox and chat box (like we used to have before i merged this from jay)". The chat box's frosted glass, its pills tray and its floating over the transcript are gone (native_chat/composer.rs, render.rs); the box has its own solid fill with the pills along its top again, and the transcript ends above the Inbox and the box. This backdrop stays for the sidebar card and Settings.
///
/// CDXC:Workarea 2026-11-25 DECISION:
/// User, of the floating sidebar card showing the artwork straight through it: "add a bg to the sidebar man why transparent", "a drak tint blured", then "keep the same amount of transperancy for both". Over an artwork the card is dark frosted glass exactly like the chat box (the sidebar and the rail paint no tint of their own over it); otherwise it takes the sidebar's own solid colour (`render_sidebar_card` in native_sidebar/host.rs).
pub(crate) fn frosted_glass_backdrop(
    radii: gpui::Corners<gpui::Pixels>,
    light: bool,
) -> Option<AnyElement> {
    let frost = window_art_frosted()?;
    let base = crate::app::helpers::workspace_background_color().opacity(1.0);
    let smoke = gpui::black().opacity(if light { 0.0 } else { 0.6 });
    let tint = base.opacity(0.6);
    Some(
        gpui::canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let whole = gpui::Bounds {
                    origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
                    size: window.viewport_size(),
                };
                let placed = ObjectFit::Cover.get_bounds(whole, frost.image.size(0));
                let _ = window.paint_image(bounds, placed, radii, frost.image.clone(), 0, false);
                for fill in [base.opacity(frost.dim), smoke, tint] {
                    window.paint_quad(gpui::fill(bounds, fill).corner_radii(radii));
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element(),
    )
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

/// The setup window's picture of the backdrop the settings in `object` give the `light` or dark
/// appearance (`art_style_preview` with gxserver's artwork), for its Appearance card's Window art
/// tiles and its Native Chat preview.
#[cfg(not(ghostex_web))]
pub(crate) fn window_art_style_preview(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
    cx: &mut App,
) -> Option<super::window_art_style::ArtStylePreview> {
    super::window_art_style::art_style_preview(object, light, fetch_artwork, cx)
}

/// The decoded artwork for `choice`, or `None` while it is being fetched and decoded (that starts
/// here the first time it is asked for, and every window redraws when it is ready) or when it
/// could not be had.
fn artwork_image(choice: ArtChoice, cx: &mut App) -> Option<Arc<RenderImage>> {
    let key = choice.key();
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
                    .spawn(async move {
                        decode_artwork(&bytes, choice.blur, yours, choice.tone, choice.light)
                    })
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
                    frosted: result.1,
                    preview: result.2,
                };
                #[cfg(ghostex_web)]
                let entry = ReadyArt {
                    key,
                    image: result.0,
                    frosted: result.1,
                };
                decoded.ready.replace(entry)
            };
            if let Some(previous) = previous {
                cx.drop_image(previous.image, None);
                cx.drop_image(previous.frosted, None);
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
    let key = choice.key();
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
/// it (black in dark mode and white in light mode for a Picture, the theme's own colour for ASCII
/// and Dither, so only the characters or dots fade). Put it first in a window's root (`relative`),
/// under everything else.
pub(crate) fn window_art_backdrop(cx: &mut App) -> Option<AnyElement> {
    if !window_art_active() {
        return None;
    }
    let choice = (*WINDOW_ART.lock().ok()?)?;
    let image = artwork_image(choice, cx);
    let base = crate::app::helpers::workspace_background_color().opacity(1.0);
    // A picture dims under black (dark mode) or white (light mode); an ASCII or Dither artwork
    // dims toward the theme's own colour, so the window around the characters or dots stays it.
    let wash = match choice.tone.style {
        ArtStyle::Picture if choice.light => gpui::white(),
        ArtStyle::Picture => gpui::black(),
        ArtStyle::Ascii | ArtStyle::Dither => base,
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
                            "window-art-{}-{}-{}-{}-{:?}",
                            choice.id,
                            choice.picture,
                            choice.blur,
                            choice.tone.monochrome,
                            choice.tone.style
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
