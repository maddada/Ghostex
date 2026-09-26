//! Live glass: an animated style the window's glass draws in place of a picture or video, painted
//! in the current theme's colours (the GPUI macOS window draws it, `window_live.rs`).

use std::sync::atomic::Ordering;

use crate::app::helpers::*;

/// CDXC:Theming 2026-09-26 DECISION:
/// User, after rendering the glass videos live was proposed instead of downloading mp4 files ("how about if we just render them in the bg behind the app? not as mp4 vids?"): "pls implement the shaders thing"; then "the animations can't jump at all they need to loop and never break", "can just be 2 mins only as long as it loops", and "the brightness of those animations is too high btw we need a way to control dim them and they need to be dimmer by default not so bright by default". Glass shows has a Live choice: one of the app's animated styles (`windowGlassLiveStyleDark` / `windowGlassLiveStyleLight`, "Dark only" shows only the dark one) drawn behind the glass at `windowGlassLiveSpeed` and `windowGlassLiveBrightness` (45% by default, every style evened out to the same brightness), in the current theme's colours and Colourfulness, so switching themes recolours it. It loops every two minutes without a seam and never jumps: speed changes the rate, pauses hold it, and a new style, colours or brightness cross-fade (`window_live.rs` in the GPUI macOS crate). It pauses under the same rules as the video (`windowGlassVideoOnlyOnPower` covers both) and shows a still frame under Reduce Motion. Video stays for the computer's aerials and the user's own files.
static WINDOW_GLASS_LIVE: std::sync::Mutex<WindowGlassLive> =
    std::sync::Mutex::new(WindowGlassLive {
        live: false,
        style_dark: String::new(),
        style_light: String::new(),
        speed: 1.0,
        brightness: 0.45,
        only_on_power: true,
        colors_dark: [[0.0; 3]; 3],
        colors_light: [[0.0; 3]; 3],
    });

struct WindowGlassLive {
    live: bool,
    style_dark: String,
    style_light: String,
    speed: f32,
    brightness: f32,
    only_on_power: bool,
    colors_dark: [[f32; 3]; 3],
    colors_light: [[f32; 3]; 3],
}

/// Reads the live glass settings and the theme colours it paints with. Called from
/// `refresh_window_glass`, which every settings change and appearance switch goes through.
pub(crate) fn refresh_window_glass_live(object: &serde_json::Map<String, serde_json::Value>) {
    let Ok(mut live) = WINDOW_GLASS_LIVE.lock() else {
        return;
    };
    let text = |key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    live.live = object
        .get("windowGlassSource")
        .and_then(serde_json::Value::as_str)
        == Some("live");
    live.style_dark = text("windowGlassLiveStyleDark");
    live.style_light = text("windowGlassLiveStyleLight");
    live.speed = object
        .get("windowGlassLiveSpeed")
        .and_then(serde_json::Value::as_f64)
        .filter(|speed| speed.is_finite())
        .map_or(1.0, |speed| speed.clamp(0.25, 2.0) as f32);
    live.brightness = object
        .get("windowGlassLiveBrightness")
        .and_then(serde_json::Value::as_f64)
        .filter(|brightness| brightness.is_finite())
        .map_or(0.45, |brightness| {
            (brightness.clamp(10.0, 100.0) / 100.0) as f32
        });
    live.only_on_power = object
        .get("windowGlassVideoOnlyOnPower")
        .and_then(serde_json::Value::as_bool)
        != Some(false);
    let colourfulness = theme_contrast_offset(object);
    let (_, dark_tint) = dark_chrome_controls(object);
    let (_, light_tint) = light_chrome_controls(object);
    live.colors_dark = live_background_colors(
        dark_tint,
        accent_color_for_tint(dark_tint, false),
        false,
        colourfulness,
    );
    live.colors_light = live_background_colors(
        light_tint,
        accent_color_for_tint(light_tint, true),
        true,
        colourfulness,
    );
}

/// The live style the main window's glass draws in the current appearance, when Glass shows is
/// Live and a style is chosen for that appearance.
pub(crate) fn window_glass_live() -> Option<gpui::LiveBackground> {
    let live = WINDOW_GLASS_LIVE.lock().ok()?;
    if !live.live {
        return None;
    }
    let light = CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    let style = if light {
        &live.style_light
    } else {
        &live.style_dark
    };
    if style.is_empty() {
        return None;
    }
    Some(gpui::LiveBackground {
        style: style.clone().into(),
        colors: if light {
            live.colors_light
        } else {
            live.colors_dark
        },
        speed: live.speed,
        brightness: live.brightness,
        only_on_power: live.only_on_power,
    })
}

/// Whether Glass shows is Live (with or without a style for the current appearance).
pub(crate) fn window_glass_live_mode() -> bool {
    WINDOW_GLASS_LIVE.lock().is_ok_and(|live| live.live)
}

/// CDXC:Theming 2026-09-26 WHY:
/// The style is seen through the sidebar and work area tints, so its colours are the theme's own hue at a brightness that still reads through them: a deep base, the theme's hue, and a lighter second hue (the theme's accent, or the hue turned a little further when the accent matches it). Colourfulness (the sidebar contrast points, -12 vivid to 4 subtle) sets how saturated they are. A neutral theme (Graphite, Black, White) takes its hue from its accent, and a neutral accent too falls back to a quiet blue.
fn live_background_colors(
    tint: u32,
    accent: u32,
    light: bool,
    contrast_points: f64,
) -> [[f32; 3]; 3] {
    let vivid = ((4.0 - contrast_points) / 16.0).clamp(0.0, 1.0) as f32;
    let saturation = 0.35 + 0.5 * vivid;
    let (tint_hue, tint_saturation, _) = hsl_of(tint);
    let (accent_hue, accent_saturation, _) = hsl_of(accent);
    let (hue, second_hue, saturation) = if tint_saturation >= 0.08 {
        let second = if accent_saturation >= 0.1 && hue_distance(accent_hue, tint_hue) > 0.02 {
            accent_hue
        } else {
            tint_hue + 0.1
        };
        (tint_hue, second, saturation)
    } else if accent_saturation >= 0.08 {
        (accent_hue, accent_hue + 0.08, saturation * 0.65)
    } else {
        (0.6, 0.66, saturation * 0.5)
    };
    if light {
        [
            rgb_of_hsl(hue, saturation * 0.35, 0.93),
            rgb_of_hsl(hue, saturation * 0.8, 0.80),
            rgb_of_hsl(second_hue, saturation * 0.9, 0.70),
        ]
    } else {
        [
            rgb_of_hsl(hue, saturation * 0.5, 0.09),
            rgb_of_hsl(hue, saturation, 0.36),
            rgb_of_hsl(second_hue, saturation, 0.55),
        ]
    }
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let distance = (a - b).rem_euclid(1.0);
    distance.min(1.0 - distance)
}

fn hsl_of(rgb: u32) -> (f32, f32, f32) {
    let red = ((rgb >> 16) & 0xff) as f32 / 255.0;
    let green = ((rgb >> 8) & 0xff) as f32 / 255.0;
    let blue = (rgb & 0xff) as f32 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) / 2.0;
    let chroma = max - min;
    if chroma <= f32::EPSILON {
        return (0.0, 0.0, lightness);
    }
    let saturation = chroma / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == red {
        ((green - blue) / chroma).rem_euclid(6.0)
    } else if max == green {
        (blue - red) / chroma + 2.0
    } else {
        (red - green) / chroma + 4.0
    } / 6.0;
    (hue, saturation, lightness)
}

fn rgb_of_hsl(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let hue = hue.rem_euclid(1.0);
    let saturation = saturation.clamp(0.0, 1.0);
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue * 6.0;
    let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let offset = lightness - chroma / 2.0;
    [red + offset, green + offset, blue + offset]
}
