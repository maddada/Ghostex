//! The dark chrome's depth limit and the Full colour end of both appearances, shared by the window
//! (the desktop's `helpers/titlebar/chrome_tints.rs`) and the Settings preview
//! (`settings_modal/tabs/theme/colours.rs`), which cannot reach each other.
//!
//! CDXC:Theming 2026-10-31 DECISION:
//! Issue #357: a flat editor theme such as Monokai Pro Machine (`#273136`) could never colour the
//! sidebar, because the dark chrome keeps only the picked colour's hue on a near-black base and
//! stops at depth 85 (`#00141f` for `#273136`). User (Q837): "Lower the limit and make the far end
//! reach the exact colour you picked." Colourfulness now runs past Vivid to Full: from Vivid
//! (-12 points) to Full (-24) the dark chrome fades from what it draws at Vivid toward the picked
//! colour itself (a preset's own tint for a preset), keeping its red, green and blue, and lands on
//! it at Full. Depth itself still stops at 85 and every setting from Subtle to Vivid resolves as
//! before, so nobody's look changes: a stored or preset value that computes 84 or 82 is still cut
//! to 85, and only contrast points below -12, which nothing could save before, fade. Light mode's
//! scale is unchanged (it stops at Vivid). The one exception to "exact": a picked colour too light
//! for the dark chrome's text is darkened, keeping its hue, until it is as deep as `FULL_COLOUR_MAX_LUMINANCE`.
//!
//! CDXC:Theming 2026-11-01 WHY:
//! Theme presets such as Kanagawa Lotus (`#f2ecbc`) or Gruvbox Material Light (`#fbf1c7`) are
//! warmer than any light chrome Subtle to Vivid can draw, so light mode now has the same Full end:
//! from Vivid to Full the light chrome fades from its Vivid colour toward the picked colour and is
//! that colour at Full. Light mode's depth scale and every setting from Subtle to Vivid are
//! unchanged. Its one exception mirrors the dark one: a picked colour too dark for the light
//! chrome's dark text is lightened toward white, keeping its hue, until it is as light as
//! `LIGHT_FULL_COLOUR_MIN_LUMINANCE`. This supersedes "Light mode's scale is unchanged (it stops
//! at Vivid)" above.

/// The deepest the dark chrome's depth goes; Colourfulness past Vivid fades toward the picked
/// colour instead of lowering it.
pub use crate::data::MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;

/// Vivid: the contrast points where depth stops (presets and Custom both reach 85 here) and the
/// fade toward the picked colour starts.
pub const THEME_CONTRAST_VIVID_POINTS: f64 = -12.0;

/// Full: the far end of Colourfulness, where the dark chrome is the picked colour.
pub const THEME_CONTRAST_FULL_POINTS: f64 = -24.0;

/// The most colourful end of the stored contrast range (`themeSidebarContrast`,
/// `themeWorkAreaContrast`), and the least: Subtle.
pub const THEME_CONTRAST_MAX_POINTS: f64 = 4.0;

/// The lightest a Full colour may be (WCAG relative luminance). `#273136` is 0.029; at 0.04 the
/// sidebar's #d8d8d8 text keeps 8:1 and its grey labels (about #7d828c) keep 3:1.
pub const FULL_COLOUR_MAX_LUMINANCE: f64 = 0.04;

/// The darkest a light-mode Full colour may be (WCAG relative luminance). At 0.62 the light
/// chrome's #262626 text keeps about 10:1 and its grey labels (about #6b7280) about 3:1.
pub const LIGHT_FULL_COLOUR_MIN_LUMINANCE: f64 = 0.62;

/// CDXC:Theming 2026-11-21 DECISION:
/// User (Q4388, theme-editor page): built-in themes and built-in colour squares keep their hue as
/// a subtle tint rather than a saturated surface: OKLCH chroma at most 0.020 in the sidebar and
/// 0.016 (80%) in the work area, and dark lightness at most 23% (0.23). Custom colours, Fine-tune
/// colours and the accent are never capped, so the user can still pick anything.
pub const SUBTLE_SIDEBAR_MAX_CHROMA: f64 = 0.020;
pub const SUBTLE_WORK_AREA_MAX_CHROMA: f64 = 0.016;
pub const SUBTLE_DARK_MAX_LIGHTNESS: f64 = 0.23;

/// Contrast points rounded to the half point Colourfulness moves in, over the whole slider
/// (Subtle to Full). Depth uses the narrower Subtle-to-Vivid clamp on top of this.
pub fn round_theme_contrast_points(points: f64) -> f64 {
    ((points * 2.0).round() / 2.0).clamp(THEME_CONTRAST_FULL_POINTS, THEME_CONTRAST_MAX_POINTS)
}

/// How far past Vivid `points` sit: 0 at Vivid and above, 1 at Full.
pub fn full_colour_fade(points: f64) -> f64 {
    if !points.is_finite() {
        return 0.0;
    }
    let points = round_theme_contrast_points(points);
    ((THEME_CONTRAST_VIVID_POINTS - points)
        / (THEME_CONTRAST_VIVID_POINTS - THEME_CONTRAST_FULL_POINTS))
        .clamp(0.0, 1.0)
}

fn linear(channel: f64) -> f64 {
    let value = channel / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance of an sRGB colour (0-255 channels).
pub fn relative_luminance(rgb: [f64; 3]) -> f64 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// The colour Full paints for a picked colour: the colour itself, or, when it is lighter than
/// `FULL_COLOUR_MAX_LUMINANCE`, the same red, green and blue scaled down until it is that deep.
pub fn full_colour_target(picked: [f64; 3]) -> [f64; 3] {
    if relative_luminance(picked) <= FULL_COLOUR_MAX_LUMINANCE {
        return picked;
    }
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if relative_luminance(picked.map(|channel| channel * middle)) <= FULL_COLOUR_MAX_LUMINANCE {
            low = middle;
        } else {
            high = middle;
        }
    }
    picked.map(|channel| channel * low)
}

/// The dark chrome `fade` of the way from `vivid` (what depth draws at Vivid) to the picked
/// colour's Full target. Unrounded; 0 returns `vivid` unchanged.
pub fn dark_chrome_toward_full(vivid: [f64; 3], picked: [f64; 3], fade: f64) -> [f64; 3] {
    if fade <= 0.0 {
        return vivid;
    }
    let target = full_colour_target(picked);
    let fade = fade.min(1.0);
    [0, 1, 2].map(|index| vivid[index] + (target[index] - vivid[index]) * fade)
}

/// The colour light-mode Full paints for a picked colour: the colour itself, or, when it is darker
/// than `LIGHT_FULL_COLOUR_MIN_LUMINANCE`, the same colour moved toward white just far enough that
/// it still reaches the limit once its channels are rounded to whole numbers, as the chrome is
/// painted (rounding can otherwise take it one step below).
pub fn light_full_colour_target(picked: [f64; 3]) -> [f64; 3] {
    if relative_luminance(picked) >= LIGHT_FULL_COLOUR_MIN_LUMINANCE {
        return picked;
    }
    let toward_white = |amount: f64| picked.map(|channel| channel + (255.0 - channel) * amount);
    let painted = |amount: f64| toward_white(amount).map(|channel| (channel + 0.5).floor());
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if relative_luminance(painted(middle)) >= LIGHT_FULL_COLOUR_MIN_LUMINANCE {
            high = middle;
        } else {
            low = middle;
        }
    }
    toward_white(high)
}

fn delinear(value: f64) -> f64 {
    let value = value.max(0.0);
    if value <= 0.0031308 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// sRGB (0-255 channels) to OKLCH `(L, C, h_radians)`.
pub fn srgb_to_oklch(rgb: [f64; 3]) -> (f64, f64, f64) {
    let [r, g, b] = rgb.map(linear);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let lightness = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    (lightness, a.hypot(b), b.atan2(a))
}

/// OKLCH `(L, C, h_radians)` back to sRGB (0-255 channels, clamped to sRGB gamut).
pub fn oklch_to_srgb(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let a = chroma * hue.cos();
    let b = chroma * hue.sin();
    let l = (lightness + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (lightness - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (lightness - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    [r, g, b].map(|value| delinear(value).clamp(0.0, 1.0) * 255.0)
}

/// Caps a dark built-in surface colour in OKLCH to at most `max_chroma` and `max_lightness`,
/// keeping colours that are already within the bounds unchanged.
pub fn cap_subtle_dark_rgb(rgb: [f64; 3], max_chroma: f64, max_lightness: f64) -> [f64; 3] {
    let (lightness, chroma, hue) = srgb_to_oklch(rgb);
    if chroma <= max_chroma && lightness <= max_lightness {
        return rgb;
    }
    oklch_to_srgb(
        lightness.min(max_lightness),
        chroma.min(max_chroma),
        hue,
    )
}

/// Caps a light built-in surface colour in OKLCH to at most `max_chroma`, keeping colours that are
/// already within the chroma bound unchanged.
pub fn cap_subtle_light_rgb(rgb: [f64; 3], max_chroma: f64) -> [f64; 3] {
    let (lightness, chroma, hue) = srgb_to_oklch(rgb);
    if chroma <= max_chroma {
        return rgb;
    }
    oklch_to_srgb(lightness, max_chroma, hue)
}

/// Built-in dark colour squares' chrome at `fade` (0 at Vivid and above, 1 at Full), with both the
/// Vivid base and the Full target capped to `(max_chroma, max_lightness)` so every step stays a
/// subtle tint and Full reaches the capped tint smoothly.
pub fn dark_chrome_toward_full_subtle(
    vivid: [f64; 3],
    picked: [f64; 3],
    fade: f64,
    max_chroma: f64,
    max_lightness: f64,
) -> [f64; 3] {
    let vivid = cap_subtle_dark_rgb(vivid, max_chroma, max_lightness);
    if fade <= 0.0 {
        return vivid;
    }
    let target = cap_subtle_dark_rgb(full_colour_target(picked), max_chroma, max_lightness);
    let fade = fade.min(1.0);
    let mixed = [0, 1, 2].map(|index| vivid[index] + (target[index] - vivid[index]) * fade);
    cap_subtle_dark_rgb(mixed, max_chroma, max_lightness)
}

/// Built-in light colour squares' chrome at `fade`, with both Vivid and Full capped to `max_chroma`.
pub fn light_chrome_toward_full_subtle(
    vivid: [f64; 3],
    picked: [f64; 3],
    fade: f64,
    max_chroma: f64,
) -> [f64; 3] {
    let vivid = cap_subtle_light_rgb(vivid, max_chroma);
    if fade <= 0.0 {
        return vivid;
    }
    let target = cap_subtle_light_rgb(light_full_colour_target(picked), max_chroma);
    let fade = fade.min(1.0);
    let mixed = [0, 1, 2].map(|index| vivid[index] + (target[index] - vivid[index]) * fade);
    cap_subtle_light_rgb(mixed, max_chroma)
}

/// The light chrome `fade` of the way from `vivid` (what lightness draws at Vivid) to the picked
/// colour's light Full target. Unrounded; 0 returns `vivid` unchanged.
pub fn light_chrome_toward_full(vivid: [f64; 3], picked: [f64; 3], fade: f64) -> [f64; 3] {
    if fade <= 0.0 {
        return vivid;
    }
    let target = light_full_colour_target(picked);
    let fade = fade.min(1.0);
    [0, 1, 2].map(|index| vivid[index] + (target[index] - vivid[index]) * fade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_fade_from_subtle_to_vivid() {
        let mut points = THEME_CONTRAST_MAX_POINTS;
        while points >= THEME_CONTRAST_VIVID_POINTS {
            assert_eq!(full_colour_fade(points), 0.0, "{points}");
            points -= 0.5;
        }
        assert_eq!(full_colour_fade(THEME_CONTRAST_FULL_POINTS), 1.0);
        assert_eq!(full_colour_fade(-100.0), 1.0);
        assert_eq!(full_colour_fade(f64::NAN), 0.0);
    }

    #[test]
    fn a_dark_picked_colour_is_kept_exactly() {
        let monokai = [39.0, 49.0, 54.0];
        assert_eq!(full_colour_target(monokai), monokai);
        assert_eq!(
            dark_chrome_toward_full([0.0, 20.0, 31.0], monokai, 1.0),
            monokai
        );
    }

    #[test]
    fn a_light_picked_colour_keeps_its_hue_at_the_cap() {
        let teal = [47.0, 127.0, 127.0];
        let target = full_colour_target(teal);
        assert!((relative_luminance(target) - FULL_COLOUR_MAX_LUMINANCE).abs() < 1e-6);
        assert!((target[0] / target[1] - teal[0] / teal[1]).abs() < 1e-9);
    }

    #[test]
    fn a_pale_picked_colour_is_kept_exactly_in_light_mode() {
        // The palest and the warmest light theme backgrounds a preset picks (Alucard's and
        // Kanagawa Lotus's picks) are above the limit, so light Full paints them unchanged.
        for picked in [[255.0, 250.0, 228.0], [238.0, 230.0, 166.0]] {
            assert_eq!(light_full_colour_target(picked), picked);
            assert_eq!(
                light_chrome_toward_full([241.0, 241.0, 242.0], picked, 1.0),
                picked
            );
        }
        assert_eq!(
            light_chrome_toward_full([241.0, 241.0, 242.0], [238.0, 230.0, 166.0], 0.0),
            [241.0, 241.0, 242.0]
        );
    }

    #[test]
    fn a_dark_picked_colour_is_lightened_just_to_the_light_limit() {
        let midnight = [31.0, 58.0, 138.0];
        let target = light_full_colour_target(midnight);
        let painted = target.map(|channel| (channel + 0.5).floor());
        let luminance = relative_luminance(painted);
        assert!(luminance >= LIGHT_FULL_COLOUR_MIN_LUMINANCE, "{luminance}");
        // One whole step less would fall below the limit: the step is the least that holds.
        let one_less = painted.map(|channel| channel - 1.0);
        assert!(
            relative_luminance(one_less) < LIGHT_FULL_COLOUR_MIN_LUMINANCE,
            "{luminance}"
        );
        // Moving toward white keeps the channels in their order (the hue).
        assert!(target[2] > target[1] && target[1] > target[0], "{target:?}");
        // Black and white still land inside the range.
        assert!(relative_luminance(light_full_colour_target([0.0; 3])) >= 0.62);
        assert_eq!(light_full_colour_target([255.0; 3]), [255.0; 3]);
    }
}
