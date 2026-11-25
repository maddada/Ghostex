//! The artwork's own drawing, free of the app's state: which artwork and style the settings ask
//! for (`art_choice`), how it is drawn (`stylise_artwork`: Picture, ASCII or Dither, in colour or
//! Black and white), and small finished pictures of each style for the setup window's Appearance
//! card (`art_style_preview`). app/helpers/window_art.rs paints the window from it; the GPUI web
//! build links this file as it links that one, and the native-modal-demo includes it to draw the
//! setup window with real artwork.
//! SEE-ALSO: app/helpers/window_art.rs, app/window/onboarding/get_started.rs.

// The web build has no setup window, so it never asks for a style preview.
#![cfg_attr(ghostex_web, allow(dead_code))]

use std::future::Future;
use std::sync::{Arc, Mutex};

use gpui::{App, RenderImage};

/// The artwork the settings ask for in the current appearance: its id, dim and blur, whether it
/// is painted under light mode, and the colours its dots take (`tone`). `picture` tells Your
/// image's pictures apart (a hash of the kept file's path; 0 for the artworks).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ArtChoice {
    pub(crate) id: &'static str,
    pub(crate) picture: u64,
    pub(crate) dim: u8,
    pub(crate) blur: u8,
    pub(crate) light: bool,
    pub(crate) tone: ArtTone,
}

/// How the artwork is drawn (`windowArtStyle`): as the picture itself (the default), as ASCII
/// characters over a soft blur of it, or as halftone dots.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ArtStyle {
    Picture,
    Ascii,
    Dither,
}

/// How the artwork is drawn: its `style`; in `ink` (the theme's text colour; greys for a picture)
/// while `monochrome` (`windowArtMonochrome`, Black and white) is on, else in the artwork's own
/// colours; on `base` (the work area colour).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ArtTone {
    pub(crate) ink: u32,
    pub(crate) base: u32,
    pub(crate) monochrome: bool,
    pub(crate) style: ArtStyle,
}

/// A decoded artwork's identity: the artwork (or Your image's picture), its blur, its tone and the
/// appearance (light mode dithers the dark parts of the picture).
pub(crate) type ArtKey = (&'static str, u64, u8, ArtTone, bool);

impl ArtChoice {
    pub(crate) fn key(&self) -> ArtKey {
        (self.id, self.picture, self.blur, self.tone, self.light)
    }
}

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

/// The artwork the settings ask for in the `light` or dark appearance, or `None` when it paints
/// nothing (no artwork, or Your image with no picture kept).
pub(crate) fn art_choice(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> Option<ArtChoice> {
    let art_key = if light { "windowArtLight" } else { "windowArt" };
    let percent = |key: &str, max: f64| art_percent(object, key, max);
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
    id_and_picture.map(|(id, picture)| ArtChoice {
        id,
        picture,
        dim: percent("windowArtDim", ghostex_settings_catalog::MAX_WINDOW_ART_DIM),
        blur: percent(
            "windowArtBlur",
            ghostex_settings_catalog::MAX_WINDOW_ART_BLUR,
        ),
        light,
        tone: art_tone(object, light),
    })
}

fn art_percent(object: &serde_json::Map<String, serde_json::Value>, key: &str, max: f64) -> u8 {
    read_number(object, key)
        .unwrap_or_else(|| catalog_number(key))
        .clamp(0.0, max)
        .round() as u8
}

/// How much of the artwork the sidebar's and the work area's tints cover, percent, in the `light`
/// or dark appearance (the glass tints).
pub(crate) fn art_surface_percents(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> (u8, u8) {
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
    (
        art_percent(object, sidebar, 100.0),
        art_percent(object, work_area, 100.0),
    )
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
pub(crate) fn stylise_artwork(rgba: &mut image::RgbaImage, tone: ArtTone, light: bool) {
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
pub(crate) const YOUR_IMAGE_MAX_SIDE: u32 = 3840;

/// The artwork's pixels from its file's `bytes` (a WebP artwork, or Your image in any format
/// gxserver accepted, scaled to [`YOUR_IMAGE_MAX_SIDE`]), blurred by `blur` points.
pub(crate) fn load_artwork_rgba(bytes: &[u8], blur: u8, yours: bool) -> Option<image::RgbaImage> {
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
    Some(rgba)
}

/// A small finished picture of the window's backdrop in one art style: the artwork drawn as the
/// window draws it (`stylise_artwork`, full size first so ASCII characters and Dither dots keep
/// their real size), laid on the work area colour under its dimming, then scaled down. `base` is
/// the work area colour (0xRRGGBB); `sidebar` and `work_area` are how much of it the sidebar and
/// the work area lay over the artwork, so a preview drawn with them shows what the window shows.
#[derive(Clone)]
pub(crate) struct ArtStylePreview {
    pub(crate) image: Arc<RenderImage>,
    pub(crate) base: u32,
    pub(crate) sidebar: f32,
    pub(crate) work_area: f32,
}

/// How wide a style preview is kept, in pixels: enough for a close-up of its corner to still show
/// ASCII characters and Dither dots.
const STYLE_PREVIEW_WIDTH: u32 = 1280;
/// How many style previews are kept: the three styles in one appearance, and three more as the
/// pick changes (about 3.5 MB each).
const STYLE_PREVIEW_KEEP: usize = 6;

enum PreviewSlot {
    Pending,
    Failed,
    Ready(Arc<RenderImage>),
}

/// The style previews asked for, oldest first, by artwork, tone, appearance and dimming.
static STYLE_PREVIEWS: Mutex<Vec<((ArtKey, u8), PreviewSlot)>> = Mutex::new(Vec::new());

/// The artwork `bytes` drawn as `choice` asks, flattened as the window's backdrop draws it (the
/// work area colour, the artwork, then the dimming: black or white over a Picture, the work area
/// colour over ASCII and Dither) and scaled to [`STYLE_PREVIEW_WIDTH`], as GPUI's BGRA.
fn style_preview_image(bytes: &[u8], choice: ArtChoice) -> Option<Arc<RenderImage>> {
    let yours = choice.id == ghostex_settings_catalog::YOUR_IMAGE_ART_ID;
    let mut rgba = load_artwork_rgba(bytes, choice.blur, yours)?;
    stylise_artwork(&mut rgba, choice.tone, choice.light);
    let channels =
        |colour: u32| [colour >> 16 & 0xff, colour >> 8 & 0xff, colour & 0xff].map(|c| c as f32);
    let base = channels(choice.tone.base);
    let wash = match choice.tone.style {
        ArtStyle::Picture if choice.light => [255.0; 3],
        ArtStyle::Picture => [0.0; 3],
        ArtStyle::Ascii | ArtStyle::Dither => base,
    };
    let dim = f32::from(choice.dim) / 100.0;
    for pixel in rgba.pixels_mut() {
        let a = f32::from(pixel.0[3]) / 255.0;
        for channel in 0..3 {
            let laid = base[channel] + (f32::from(pixel.0[channel]) - base[channel]) * a;
            let washed = laid + (wash[channel] - laid) * dim;
            pixel.0[channel] = washed.round().clamp(0.0, 255.0) as u8;
        }
        pixel.0[3] = 255;
    }
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let out_width = width.min(STYLE_PREVIEW_WIDTH);
    let out_height = ((height as f32 * out_width as f32 / width as f32).round() as u32).max(1);
    let mut small = image::imageops::resize(
        &rgba,
        out_width,
        out_height,
        image::imageops::FilterType::Triangle,
    );
    drop(rgba);
    for pixel in small.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(small)])))
}

/// The backdrop the settings in `object` give the `light` or dark appearance, as a small picture,
/// or `None` when they paint no artwork, while it is being made (every window redraws when it is
/// ready) or when the artwork could not be had. `fetch` gives the artwork's bytes (gxserver's file
/// in the app). The setup window's Appearance card asks for each style this way, so its pictures
/// show exactly what the window will draw, including how faint ASCII and Dither are in light mode.
pub(crate) fn art_style_preview<F>(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
    fetch: impl FnOnce(&'static str, bool) -> F,
    cx: &mut App,
) -> Option<ArtStylePreview>
where
    F: Future<Output = Option<Vec<u8>>> + 'static,
{
    let choice = art_choice(object, light)?;
    let (sidebar, work_area) = art_surface_percents(object, light);
    let key = (choice.key(), choice.dim);
    let ready = |image: Arc<RenderImage>| ArtStylePreview {
        image,
        base: choice.tone.base,
        sidebar: f32::from(sidebar) / 100.0,
        work_area: f32::from(work_area) / 100.0,
    };
    let evicted = {
        let mut previews = STYLE_PREVIEWS.lock().ok()?;
        if let Some((_, slot)) = previews.iter().find(|(asked, _)| *asked == key) {
            return match slot {
                PreviewSlot::Ready(image) => Some(ready(image.clone())),
                PreviewSlot::Pending | PreviewSlot::Failed => None,
            };
        }
        previews.push((key, PreviewSlot::Pending));
        let over = previews.len().saturating_sub(STYLE_PREVIEW_KEEP);
        previews.drain(..over).collect::<Vec<_>>()
    };
    for (_, slot) in evicted {
        if let PreviewSlot::Ready(image) = slot {
            cx.drop_image(image, None);
        }
    }
    let bytes = fetch(choice.id, light);
    let background = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let image = match bytes.await {
            Some(bytes) => {
                background
                    .spawn(async move { style_preview_image(&bytes, choice) })
                    .await
            }
            None => None,
        };
        let _ = cx.update(|cx| {
            if let Ok(mut previews) = STYLE_PREVIEWS.lock()
                && let Some((_, slot)) = previews.iter_mut().find(|(asked, _)| *asked == key)
            {
                *slot = match image {
                    Some(image) => PreviewSlot::Ready(image),
                    None => PreviewSlot::Failed,
                };
            }
            cx.refresh_windows();
        });
    })
    .detach();
    None
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
