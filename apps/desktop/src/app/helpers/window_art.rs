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

/// The artwork the settings ask for in the current appearance: its id, dim and blur, whether it
/// is painted under light mode, and the colours its dots take (`tone`). `picture` tells Your
/// image's pictures apart (a hash of the kept file's path; 0 for the artworks).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ArtChoice {
    id: &'static str,
    picture: u64,
    dim: u8,
    blur: u8,
    light: bool,
    tone: ArtTone,
}

/// How the artwork is drawn (`windowArtStyle`): as the picture itself (the default), as ASCII
/// characters over a soft blur of it, or as halftone dots.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ArtStyle {
    Picture,
    Ascii,
    Dither,
}

/// How the artwork is drawn: its `style`; in `ink` (the theme's text colour; greys for a picture)
/// while `monochrome` (`windowArtMonochrome`, Black and white) is on, else in the artwork's own
/// colours; on `base` (the work area colour).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ArtTone {
    ink: u32,
    base: u32,
    monochrome: bool,
    style: ArtStyle,
}

/// A decoded artwork's identity: the artwork (or Your image's picture), its blur, its tone and the
/// appearance (light mode dithers the dark parts of the picture).
type ArtKey = (&'static str, u64, u8, ArtTone, bool);

impl ArtChoice {
    fn key(&self) -> ArtKey {
        (self.id, self.picture, self.blur, self.tone, self.light)
    }
}

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
        tone: art_tone(object, light),
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

/// How the artwork is drawn: the theme's text colour (the Text theme role, else the chat's own text
/// grey), its work area colour, whether Black and white (`windowArtMonochrome`, off by default)
/// draws in that text colour, and the style (`windowArtStyle`, Picture by default).
fn art_tone(object: &serde_json::Map<String, serde_json::Value>, light: bool) -> ArtTone {
    let monochrome = object
        .get("windowArtMonochrome")
        .and_then(serde_json::Value::as_bool)
        .or_else(
            || match ghostex_settings_catalog::default_value("windowArtMonochrome") {
                Some(ghostex_settings_catalog::J::Bool(on)) => Some(*on),
                _ => None,
            },
        )
        .unwrap_or(true);
    let base =
        ghostex_settings_catalog::theme_colours::work_area_background_for_variant(object, light);
    let ink = ghostex_settings_catalog::theme_roles::roles_for(object, light)
        .into_iter()
        .find(|(role, _)| *role == "text")
        .map(|(_, value)| {
            let [r, g, b, a] = value.rgba_on_base(light, base);
            let under =
                [base >> 16 & 0xff, base >> 8 & 0xff, base & 0xff].map(|c| c as f32 / 255.0);
            let channel = |c: f32, under: f32| ((under + (c - under) * a) * 255.0).round() as u32;
            channel(r, under[0]) << 16 | channel(g, under[1]) << 8 | channel(b, under[2])
        })
        .unwrap_or(if light { 0x262626 } else { 0xe6e6e6 });
    let style = match object
        .get("windowArtStyle")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            ghostex_settings_catalog::default_value("windowArtStyle")
                .and_then(ghostex_settings_catalog::J::as_str)
        }) {
        Some("ascii") => ArtStyle::Ascii,
        Some("dither") => ArtStyle::Dither,
        _ => ArtStyle::Picture,
    };
    ArtTone {
        ink,
        base,
        monochrome,
        style,
    }
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

/// How much of its colour saturation a built-in artwork keeps behind the window in the Picture
/// style (CDXC:Theming 2026-11-21, Q4388 subtle rule: Art backgrounds dimmed to 45% and shown at
/// 70% of their colour).
const ART_COLOUR_SATURATION: f32 = 0.70;

/// The halftone grid: one dot per `HALFTONE_CELL` artwork pixels (6 points on a 1920-point window
/// for a 2560-pixel artwork); a full dot's radius is `HALFTONE_RADIUS` of the cell, so the brightest
/// dots just touch.
const HALFTONE_CELL: u32 = 8;
const HALFTONE_RADIUS: f32 = 0.62;
const HALFTONE_ALPHA: f32 = 0.8;

/// ASCII: the soft copy behind the characters (its blur, in artwork pixels, and its strength in
/// mono and colour), the characters' own strength, and their glow (blur and strength).
const ASCII_BACKDROP_BLUR: f32 = 8.0;
const ASCII_BACKDROP_ALPHA_MONO: f32 = 0.24;
const ASCII_BACKDROP_ALPHA_COLOUR: f32 = 0.22;
const ASCII_GLOW_BLUR: f32 = 2.0;
const ASCII_GLOW_ALPHA: f32 = 0.5;

/// Light mode draws dark ink on a pale window, which reads stronger: every layer is weaker.
const LIGHT_STRENGTH: f32 = 0.7;

/// How bright a coloured character or dot is drawn (its luma, 0 to 255) when Black and white is
/// off: each keeps its cell's hue and saturation at one brightness, so dark artwork shows colour.
const COLOUR_LUMA_DARK: f32 = 200.0;
const COLOUR_LUMA_LIGHT: f32 = 90.0;

/// How much of the artwork row `y` draws: all of it along the bottom (the lowest 8% of the
/// height), fading out to nothing by 70% up, so the window's top (its header and title) sits on
/// the theme's colour alone.
///
/// CDXC:Theming 2026-11-25 DECISION:
/// User, after trying art full along the top and bottom edges with a faint middle: "this bar is bad on the top also the fade is looking really bad what u can do is add back the top fade like the previous one and also the fede in the middle make it a little reansparent". The artwork rises from the bottom and fades out toward the top; the chat's reading column keeps its soft panel, lighter (`art_reading_panel` in native_chat/render.rs).
fn rise_fade(y: u32, height: u32) -> f32 {
    let height_up = 1.0 - y as f32 / (height.max(2) - 1) as f32;
    (1.0 - (height_up - 0.08) / 0.62).clamp(0.0, 1.0)
}

fn luma([r, g, b]: [f32; 3]) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// `colour` with its hue and saturation at luma `target`.
fn at_luma(colour: [f32; 3], target: f32) -> [f32; 3] {
    let lifted = colour.map(|channel| channel * target / luma(colour).max(1.0));
    let top = lifted.iter().copied().fold(0.0f32, f32::max);
    let fit = if top > 255.0 { 255.0 / top } else { 1.0 };
    lifted.map(|channel| channel * fit)
}

/// Lays `colour` at `alpha` over a straight-alpha pixel.
fn paint_over(pixel: &mut image::Rgba<u8>, colour: [f32; 3], alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let under = f32::from(pixel.0[3]) / 255.0;
    let out = alpha + under * (1.0 - alpha);
    for channel in 0..3 {
        let mixed =
            (colour[channel] * alpha + f32::from(pixel.0[channel]) * under * (1.0 - alpha)) / out;
        pixel.0[channel] = mixed.round().clamp(0.0, 255.0) as u8;
    }
    pixel.0[3] = (out * 255.0).round() as u8;
}

/// The average colour of each `cell_width` by `cell_height` cell, row by row.
fn cell_colours(
    rgba: &image::RgbaImage,
    cell_width: u32,
    cell_height: u32,
) -> (usize, usize, Vec<[f32; 3]>) {
    let columns = rgba.width().div_ceil(cell_width) as usize;
    let rows = rgba.height().div_ceil(cell_height) as usize;
    let mut sums = vec![([0.0f32; 3], 0u32); columns * rows];
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let cell = &mut sums[(y / cell_height) as usize * columns + (x / cell_width) as usize];
        for channel in 0..3 {
            cell.0[channel] += f32::from(pixel.0[channel]);
        }
        cell.1 += 1;
    }
    let colours = sums
        .iter()
        .map(|(sum, count)| sum.map(|channel| channel / (*count).max(1) as f32))
        .collect();
    (columns, rows, colours)
}

/// The picture's own dark and light ends (its 2nd and 98.5th luma percentiles), so dark artwork
/// still spans the whole ramp.
fn luma_range(rgba: &image::RgbaImage) -> (f32, f32) {
    let mut sample: Vec<f32> = rgba
        .pixels()
        .step_by(7)
        .map(|pixel| luma([0, 1, 2].map(|channel| f32::from(pixel.0[channel]))))
        .collect();
    if sample.is_empty() {
        return (0.0, 255.0);
    }
    sample.sort_by(f32::total_cmp);
    let at = |share: f32| sample[((sample.len() - 1) as f32 * share) as usize];
    let (low, high) = (at(0.02), at(0.985));
    (low, (high - low).max(1.0))
}

/// Redraws `rgba` behind the window, on a clear ground the theme's work area colour shows through:
/// ASCII characters (`ASCII_RAMP`, by each cell's brightness) with a faint glow over a soft blurred
/// copy of the picture, or halftone dots sized by each cell's brightness. Brightness is stretched
/// to the picture's own range; the detail rises from the bottom and fades out toward the top (`rise_fade`). In
/// light mode the dark parts of the picture are drawn. With Black and white on everything is the
/// theme's text colour (`tone.ink`), else each cell's own colour at one brightness.
///
/// CDXC:Theming 2026-11-25 DECISION:
/// User: "change the artwork into black and white detail reading from bottom to top ... clean and minimal", "the bg should be changed to the b/w of that perticular theme and also ditering + fade and also add a option to remvoe that b/w", "the black and whte there is dither but colorfull threr is no dither why?", then, of the first dithers (an ordered Bayer pattern that read as a mesh, then error-diffused dots that read as random): "it's just some random drop. What we can do instead is have some ASCII effect and a little blur effect" (pointing at ascii-magic.com) "add a variant in the settings ... toggle between ASCII and Dither and the fixed dither as well". Every artwork, and Your image, can be drawn in ASCII (ascii-magic's Standard ramp, a soft blurred copy behind, a faint glow) or Dither (an even halftone grid of round dots) (`windowArtStyle`). Black and white (`windowArtMonochrome`) draws them in the theme's text colour; off in the artwork's own colours. Dimming washes them toward the work area colour, so the window around them stays exactly the theme's colour.
/// CDXC:Theming 2026-11-25 DECISION:
/// User, when PR 757 was merged: "please let's not change the default for people who install the app newly, i like my current default theme. i'll tell u if i wanna add one of those asciis as default for new users", and existing users keep their artwork as pictures. Picture (`ART_COLOUR_SATURATION`, the 2026-11-21 rule, under a black or white dimming wash) is the default style and Black and white is off by default; ASCII and Dither are choices in Settings > Themes. Black and white on a Picture draws it in greys.
fn stylise_artwork(rgba: &mut image::RgbaImage, tone: ArtTone, light: bool) {
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return;
    }
    if tone.style == ArtStyle::Picture {
        let saturation = if tone.monochrome {
            0.0
        } else {
            ART_COLOUR_SATURATION
        };
        for pixel in rgba.pixels_mut() {
            let r = f32::from(pixel.0[0]);
            let g = f32::from(pixel.0[1]);
            let b = f32::from(pixel.0[2]);
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let tone = |channel: f32| {
                (luma + (channel - luma) * saturation)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            pixel.0[0] = tone(r);
            pixel.0[1] = tone(g);
            pixel.0[2] = tone(b);
        }
        return;
    }
    let strength = if light { LIGHT_STRENGTH } else { 1.0 };
    let ink = [
        (tone.ink >> 16) as u8,
        (tone.ink >> 8) as u8,
        tone.ink as u8,
    ]
    .map(f32::from);
    let target = if light {
        COLOUR_LUMA_LIGHT
    } else {
        COLOUR_LUMA_DARK
    };
    let colour_of = |colour: [f32; 3]| {
        if tone.monochrome {
            ink
        } else {
            at_luma(colour, target)
        }
    };
    let (low, range) = luma_range(rgba);
    let level_of = |colour: [f32; 3]| {
        let level = ((luma(colour) - low) / range).clamp(0.0, 1.0);
        if light { 1.0 - level } else { level }
    };
    let mut out = image::RgbaImage::new(width, height);
    if tone.style == ArtStyle::Ascii {
        let soft = image::imageops::fast_blur(&*rgba, ASCII_BACKDROP_BLUR);
        let backdrop_alpha = if tone.monochrome {
            ASCII_BACKDROP_ALPHA_MONO
        } else {
            ASCII_BACKDROP_ALPHA_COLOUR
        } * strength;
        for (x, y, pixel) in out.enumerate_pixels_mut() {
            let colour = [0, 1, 2].map(|channel| f32::from(soft.get_pixel(x, y).0[channel]));
            let alpha = backdrop_alpha * level_of(colour) * rise_fade(y, height);
            if alpha > 0.0 {
                let [r, g, b] = colour_of(colour);
                pixel.0 = [r as u8, g as u8, b as u8, (alpha * 255.0).round() as u8];
            }
        }
        drop(soft);
        let (columns, rows, colours) = cell_colours(
            rgba,
            ascii_glyphs::ASCII_GLYPH_WIDTH,
            ascii_glyphs::ASCII_GLYPH_HEIGHT,
        );
        let glyph_count = ascii_glyphs::ASCII_GLYPHS.len();
        let mut mask = image::GrayImage::new(width, height);
        let mut glyph_colours = vec![[0.0f32; 3]; columns * rows];
        for row in 0..rows {
            let middle = (row as u32 * ascii_glyphs::ASCII_GLYPH_HEIGHT
                + ascii_glyphs::ASCII_GLYPH_HEIGHT / 2)
                .min(height - 1);
            let fade = rise_fade(middle, height);
            for column in 0..columns {
                let index = row * columns + column;
                let level = level_of(colours[index]).powf(0.75) * fade;
                let step = (level * glyph_count as f32).round() as usize;
                if step == 0 {
                    continue;
                }
                glyph_colours[index] = colour_of(colours[index]);
                let glyph = &ascii_glyphs::ASCII_GLYPHS[step.min(glyph_count) - 1];
                for gy in 0..ascii_glyphs::ASCII_GLYPH_HEIGHT {
                    let y = row as u32 * ascii_glyphs::ASCII_GLYPH_HEIGHT + gy;
                    if y >= height {
                        break;
                    }
                    for gx in 0..ascii_glyphs::ASCII_GLYPH_WIDTH {
                        let x = column as u32 * ascii_glyphs::ASCII_GLYPH_WIDTH + gx;
                        if x >= width {
                            break;
                        }
                        let coverage = glyph[(gy * ascii_glyphs::ASCII_GLYPH_WIDTH + gx) as usize];
                        mask.put_pixel(x, y, image::Luma([coverage]));
                    }
                }
            }
        }
        let glow = image::imageops::fast_blur(&mask, ASCII_GLOW_BLUR);
        for (x, y, pixel) in out.enumerate_pixels_mut() {
            let index = (y / ascii_glyphs::ASCII_GLYPH_HEIGHT) as usize * columns
                + (x / ascii_glyphs::ASCII_GLYPH_WIDTH) as usize;
            let colour = glyph_colours.get(index).copied().unwrap_or(ink);
            let colour = if colour == [0.0; 3] { ink } else { colour };
            let halo = f32::from(glow.get_pixel(x, y).0[0]) / 255.0;
            paint_over(pixel, colour, halo * ASCII_GLOW_ALPHA * strength);
            let coverage = f32::from(mask.get_pixel(x, y).0[0]) / 255.0;
            paint_over(pixel, colour, coverage * strength);
        }
    } else {
        let (columns, rows, colours) = cell_colours(rgba, HALFTONE_CELL, HALFTONE_CELL);
        let cell = HALFTONE_CELL as f32;
        for row in 0..rows {
            let middle = (row as u32 * HALFTONE_CELL + HALFTONE_CELL / 2).min(height - 1);
            let fade = rise_fade(middle, height);
            for column in 0..columns {
                let index = row * columns + column;
                let level = level_of(colours[index]).powf(1.2) * fade;
                let radius = level.sqrt() * cell * HALFTONE_RADIUS;
                if radius < 0.35 {
                    continue;
                }
                let colour = colour_of(colours[index]);
                let (left, top) = (column as u32 * HALFTONE_CELL, row as u32 * HALFTONE_CELL);
                let centre = (left as f32 + cell / 2.0, top as f32 + cell / 2.0);
                for y in top..(top + HALFTONE_CELL).min(height) {
                    for x in left..(left + HALFTONE_CELL).min(width) {
                        let distance = (x as f32 + 0.5 - centre.0).hypot(y as f32 + 0.5 - centre.1);
                        let coverage = (radius - distance + 0.5).clamp(0.0, 1.0);
                        if coverage > 0.0 {
                            paint_over(
                                out.get_pixel_mut(x, y),
                                colour,
                                coverage * HALFTONE_ALPHA * strength,
                            );
                        }
                    }
                }
            }
        }
    }
    // A clear pixel takes the colour of what is drawn nearest, so scaling never blends toward black.
    for pixel in out.pixels_mut() {
        if pixel.0[3] == 0 {
            let [r, g, b] = ink.map(|channel| channel as u8);
            pixel.0 = [r, g, b, 0];
        }
    }
    *rgba = out;
}

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

/// Decodes `bytes` into the BGRA bitmap GPUI draws, blurred by `blur` points first and drawn
/// in its style (`stylise_artwork`). An artwork is WebP; Your image is any picture format
/// gxserver accepted (server/src/theme/art.rs), scaled to [`YOUR_IMAGE_MAX_SIDE`].
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
/// colour over it, the way the chat box draws its glass (`composer_frost_layer` in
/// native_chat/composer.rs). `None` without an artwork. Put it first in the surface (`relative`).
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

/// The ASCII characters, darkest last, as alpha stamps (`stylise_artwork`).
mod ascii_glyphs {
    // Generated by rendering JetBrains Mono Regular 2.304 (apps/gpui-web/fonts/JetBrainsMono-Regular.ttf,
    // SIL Open Font License 1.1, see THIRD_PARTY_NOTICES.md) with FreeType at 4x (52 pixels, the
    // baseline under row 11) and averaging down; 9 by 16 alpha, 0 to 255.
    // CDXC:Theming 2026-11-25 DECISION: User, when PR 757 was merged: regenerate the glyphs (first rendered from Consolas, which Ghostex may not ship) from the OFL JetBrains Mono the repo already bundles, at the same size and with the same characters.
    /// The characters, lightest first (ascii-magic's Standard ramp reversed), for reference.
    #[allow(dead_code)]
    pub(super) const ASCII_RAMP: &str = ".:;-=+xX80S#@";
    pub(super) const ASCII_GLYPH_WIDTH: u32 = 9;
    pub(super) const ASCII_GLYPH_HEIGHT: u32 = 16;
    pub(super) const ASCII_GLYPHS: [[u8; 144]; 13] = [
        // '.'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 84,
            26, 0, 0, 0, 0, 0, 0, 93, 255, 169, 0, 0, 0, 0, 0, 0, 25, 149, 65, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
        // ':'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 150, 69, 0, 0, 0, 0, 0, 0, 99, 255, 175, 0, 0, 0, 0,
            0, 0, 8, 87, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 8, 86, 30, 0, 0, 0, 0, 0, 0, 98, 255, 175, 0, 0, 0, 0, 0, 0, 28, 150, 69, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ],
        // ';'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 152, 70, 0, 0, 0, 0, 0, 0, 104, 255, 180, 0, 0, 0, 0,
            0, 0, 9, 89, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 57, 32, 0, 0, 0, 0, 0, 0, 6, 250, 93, 0, 0, 0, 0, 0, 0, 47, 255, 35, 0, 0, 0, 0,
            0, 0, 93, 232, 0, 0, 0, 0, 0, 0, 0, 99, 136, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // '-'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 31,
            73, 73, 73, 53, 0, 0, 0, 0, 82, 191, 191, 191, 140, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0,
        ],
        // '='
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 37, 255, 255, 255, 255, 255, 114, 0,
            0, 1, 9, 9, 9, 9, 9, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 200, 200, 200, 200, 200,
            89, 0, 0, 9, 64, 64, 64, 64, 64, 28, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // '+'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 176, 43, 0, 0, 0, 0, 0, 0, 0, 235, 57, 0, 0, 0, 0, 30,
            73, 73, 241, 113, 73, 51, 0, 0, 78, 191, 191, 250, 206, 191, 135, 0, 0, 0, 0, 0, 235,
            57, 0, 0, 0, 0, 0, 0, 0, 235, 57, 0, 0, 0, 0, 0, 0, 0, 59, 14, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // 'x'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 35, 127, 13, 0, 0, 100, 75, 0, 0, 2, 191, 147, 0, 68, 241, 32,
            0, 0, 0, 31, 239, 65, 221, 98, 0, 0, 0, 0, 0, 96, 244, 178, 0, 0, 0, 0, 0, 0, 117, 237,
            191, 2, 0, 0, 0, 0, 50, 241, 47, 208, 120, 0, 0, 0, 11, 216, 117, 0, 46, 244, 52, 0, 0,
            54, 118, 3, 0, 0, 83, 92, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // 'X'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 80, 241, 20, 0, 0, 169, 155,
            0, 0, 0, 194, 138, 0, 50, 242, 25, 0, 0, 0, 56, 245, 25, 183, 125, 0, 0, 0, 0, 0, 170,
            192, 228, 10, 0, 0, 0, 0, 0, 42, 255, 115, 0, 0, 0, 0, 0, 0, 115, 238, 200, 1, 0, 0, 0,
            0, 21, 238, 60, 228, 89, 0, 0, 0, 0, 151, 167, 0, 100, 224, 8, 0, 0, 45, 246, 35, 0, 3,
            213, 120, 0, 0, 75, 90, 0, 0, 0, 57, 113, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // '8'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 27, 9, 0, 0, 0, 0, 0, 73, 230, 240, 246, 132, 0,
            0, 0, 7, 238, 106, 0, 42, 242, 66, 0, 0, 28, 255, 17, 0, 0, 195, 104, 0, 0, 1, 202,
            126, 7, 62, 235, 34, 0, 0, 0, 47, 232, 255, 250, 104, 0, 0, 0, 19, 233, 106, 16, 60,
            231, 75, 0, 0, 90, 211, 0, 0, 0, 135, 166, 0, 0, 77, 231, 7, 0, 0, 162, 153, 0, 0, 6,
            205, 195, 111, 156, 242, 45, 0, 0, 0, 12, 111, 155, 133, 36, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // '0'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 26, 9, 0, 0, 0, 0, 0, 74, 231, 240, 246, 133, 0,
            0, 0, 10, 239, 98, 0, 40, 237, 71, 0, 0, 49, 242, 0, 0, 0, 166, 126, 0, 0, 54, 235, 0,
            0, 0, 158, 130, 0, 0, 54, 235, 16, 238, 80, 158, 130, 0, 0, 54, 235, 2, 110, 28, 158,
            130, 0, 0, 54, 236, 0, 0, 0, 159, 130, 0, 0, 33, 251, 21, 0, 0, 196, 110, 0, 0, 0, 179,
            205, 112, 165, 233, 23, 0, 0, 0, 7, 107, 155, 130, 29, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // 'S'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 28, 8, 0, 0, 0, 0, 0, 68, 230, 239, 244, 116, 0,
            0, 0, 2, 230, 104, 0, 50, 248, 47, 0, 0, 15, 255, 24, 0, 0, 82, 34, 0, 0, 0, 212, 159,
            29, 0, 0, 0, 0, 0, 0, 41, 199, 254, 211, 91, 0, 0, 0, 0, 0, 0, 28, 109, 246, 66, 0, 0,
            8, 22, 0, 0, 0, 154, 146, 0, 0, 60, 241, 10, 0, 0, 168, 137, 0, 0, 4, 202, 197, 112,
            152, 241, 40, 0, 0, 0, 11, 110, 154, 134, 37, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // '#'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 133, 99, 0, 135, 97, 0,
            0, 0, 0, 180, 52, 0, 182, 50, 0, 0, 22, 64, 229, 72, 64, 231, 71, 6, 0, 43, 143, 228,
            128, 145, 226, 128, 12, 0, 0, 65, 167, 0, 68, 164, 0, 0, 0, 0, 110, 122, 0, 113, 119,
            0, 0, 0, 152, 235, 205, 191, 236, 205, 123, 0, 0, 0, 205, 27, 0, 207, 25, 0, 0, 0, 4,
            228, 0, 5, 227, 0, 0, 0, 0, 16, 100, 0, 17, 99, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        // '@'
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 24, 15, 0, 0, 0, 0, 0, 81, 226, 242, 247, 188,
            24, 0, 0, 43, 240, 74, 1, 8, 152, 176, 0, 0, 136, 145, 0, 0, 0, 22, 248, 1, 0, 166,
            100, 0, 88, 184, 126, 255, 8, 0, 170, 96, 12, 244, 41, 61, 255, 8, 0, 170, 96, 32, 233,
            0, 3, 255, 8, 0, 170, 96, 32, 233, 0, 3, 255, 6, 0, 170, 96, 8, 235, 41, 63, 216, 0, 0,
            160, 108, 0, 61, 176, 170, 44, 0, 0, 105, 190, 1, 0, 0, 0, 0, 0, 0, 9, 208, 178, 86,
            73, 30, 0, 0, 0, 0, 13, 123, 182, 191, 78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
        ],
    ];
}
