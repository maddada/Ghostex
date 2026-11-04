//! The dark chrome's depth limit and its Full colour end, shared by the window (the desktop's
//! `helpers/titlebar/chrome_tints.rs`) and the Settings preview (`settings_modal/tabs/theme/colours.rs`),
//! which cannot reach each other.
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
}
