//! The window's colours from the theme settings. The derivation is shared with gxserver in
//! packages/settings-catalog/src/theme_colours.rs (where its decisions are recorded); these keep the
//! names the app and the web build use.
//!
//! CDXC:Theming 2026-11-14 SEE-ALSO: packages/settings-catalog/src/theme_colours.rs.
#![allow(dead_code)]

use super::*;

pub(crate) fn sidebar_titlebar_gradient_stops(background: u32) -> (u32, u32) {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_gradient_stops(background)
}

pub(crate) fn sidebar_titlebar_gradient_stops_toward_full(
    background: u32,
    fade: f64,
) -> (u32, u32) {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_gradient_stops_toward_full(
        background, fade,
    )
}

pub(crate) fn sidebar_titlebar_rgb_channels(color: u32) -> [f32; 3] {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_rgb_channels(color)
}

pub(crate) fn sidebar_titlebar_pack_rgb(channels: [f32; 3]) -> u32 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_pack_rgb(channels)
}

pub(crate) fn sidebar_titlebar_tint_direction(base: [f32; 3]) -> [f32; 3] {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_tint_direction(base)
}

pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_RGB: u32 =
    ghostex_settings_catalog::theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_RGB;

pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB: u32 =
    ghostex_settings_catalog::theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB;

pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;

pub(crate) const MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;

pub(crate) const MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;

pub(crate) const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT;

const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_RGB: u32 =
    ghostex_settings_catalog::theme_colours::CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_RGB;

const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_NEUTRAL_RGB: u32 =
    ghostex_settings_catalog::theme_colours::CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_NEUTRAL_RGB;

pub(crate) const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS: [(u32, u32); 25] =
    ghostex_settings_catalog::theme_colours::CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS;

pub(crate) fn clamp_sidebar_titlebar_background_darkness_percent(value: f64) -> f64 {
    ghostex_settings_catalog::theme_colours::clamp_sidebar_titlebar_background_darkness_percent(
        value,
    )
}

pub(crate) fn sidebar_titlebar_background_darkness_for_color(background: u32) -> f64 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_background_darkness_for_color(
        background,
    )
}

pub(crate) fn sidebar_titlebar_background_for_darkness(darkness_percent: f64, tint: u32) -> u32 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_background_for_darkness(
        darkness_percent,
        tint,
    )
}

pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_RGB: u32 = ghostex_settings_catalog::theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_RGB;

pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT;

pub(crate) const MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT;

pub(crate) const MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT;

const CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_SCALE_REFERENCE_LIGHTNESS_PERCENT: f64 = ghostex_settings_catalog::theme_colours::CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_SCALE_REFERENCE_LIGHTNESS_PERCENT;

const CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_RGB: u32 = ghostex_settings_catalog::theme_colours::CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_RGB;

const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS: [(u32, u32); 25] =
    ghostex_settings_catalog::theme_colours::CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS;

pub(crate) fn clamp_sidebar_titlebar_light_background_lightness_percent(value: f64) -> f64 {
    ghostex_settings_catalog::theme_colours::clamp_sidebar_titlebar_light_background_lightness_percent(value)
}

pub(crate) fn sidebar_titlebar_light_background_for_lightness(
    lightness_percent: f64,
    tint: u32,
) -> u32 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_light_background_for_lightness(
        lightness_percent,
        tint,
    )
}

pub(crate) const DARK_THEME_PRESET_CONTROLS: [(&str, f64, u32); 16] =
    ghostex_settings_catalog::theme_colours::DARK_THEME_PRESET_CONTROLS;

pub(crate) const LIGHT_THEME_PRESET_CONTROLS: [(&str, f64, u32); 16] =
    ghostex_settings_catalog::theme_colours::LIGHT_THEME_PRESET_CONTROLS;

pub(crate) fn custom_dark_chrome_controls(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (f64, u32) {
    ghostex_settings_catalog::theme_colours::custom_dark_chrome_controls(object)
}

fn theme_contrast_points(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> f64 {
    ghostex_settings_catalog::theme_colours::theme_contrast_points(object, key)
}

pub(crate) fn theme_contrast_offset(object: &serde_json::Map<String, serde_json::Value>) -> f64 {
    ghostex_settings_catalog::theme_colours::theme_contrast_offset(object)
}

pub(crate) fn theme_full_colour_fade(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> f64 {
    ghostex_settings_catalog::theme_colours::theme_full_colour_fade(object, key)
}

pub(crate) fn sidebar_titlebar_background_toward_full(
    darkness_percent: f64,
    tint: u32,
    fade: f64,
) -> u32 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_background_toward_full(
        darkness_percent,
        tint,
        fade,
    )
}

pub(crate) fn sidebar_titlebar_light_background_toward_full(
    lightness_percent: f64,
    tint: u32,
    fade: f64,
) -> u32 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_light_background_toward_full(
        lightness_percent,
        tint,
        fade,
    )
}

fn work_area_contrast_delta(object: &serde_json::Map<String, serde_json::Value>) -> f64 {
    ghostex_settings_catalog::theme_colours::work_area_contrast_delta(object)
}

pub(crate) fn dark_chrome_controls(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (f64, u32) {
    ghostex_settings_catalog::theme_colours::dark_chrome_controls(object)
}

fn dark_chrome_raw_controls(object: &serde_json::Map<String, serde_json::Value>) -> (f64, u32) {
    ghostex_settings_catalog::theme_colours::dark_chrome_raw_controls(object)
}

pub(crate) const CUSTOM_LIGHT_THEME_DEPTH_SHIFT: f64 =
    ghostex_settings_catalog::theme_colours::CUSTOM_LIGHT_THEME_DEPTH_SHIFT;

pub(crate) fn light_chrome_controls(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (f64, u32) {
    ghostex_settings_catalog::theme_colours::light_chrome_controls(object)
}

fn light_chrome_raw_controls(object: &serde_json::Map<String, serde_json::Value>) -> (f64, u32) {
    ghostex_settings_catalog::theme_colours::light_chrome_raw_controls(object)
}

pub(crate) fn resolved_custom_sidebar_titlebar_background_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    ghostex_settings_catalog::theme_colours::resolved_custom_sidebar_titlebar_background_for_variant(
        object, light,
    )
}

pub(crate) fn resolved_work_area_chrome_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    ghostex_settings_catalog::theme_colours::resolved_work_area_chrome_for_variant(object, light)
}

pub(crate) fn work_area_background_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    ghostex_settings_catalog::theme_colours::work_area_background_for_variant(object, light)
}

/// CDXC:Theming 2026-09-22 DECISION:
/// User: light mode has its own background contrast and tint, and both appearances pick a preset theme
/// or Custom. This supersedes the 2026-09-14 fixed #f4f4f5 light chrome: that colour is now what the
/// default Light Gray preset resolves to.
/// SEE-ALSO: packages/shared/ghostex-settings/normalize.ts (deleted 2026-10-01) computes the same two effective colours.
pub(crate) fn resolved_custom_sidebar_titlebar_background(
    object: &serde_json::Map<String, serde_json::Value>,
) -> u32 {
    resolved_custom_sidebar_titlebar_background_for_variant(
        object,
        sidebar_uses_light_theme(object),
    )
}

fn sidebar_titlebar_blend_toward_white(color: u32, amount: f32) -> u32 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_blend_toward_white(color, amount)
}

fn sidebar_titlebar_background_is_light(color: u32) -> bool {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_background_is_light(color)
}

pub(crate) fn sidebar_titlebar_menu_background_for_chrome(chrome: u32) -> u32 {
    ghostex_settings_catalog::theme_colours::sidebar_titlebar_menu_background_for_chrome(chrome)
}

pub(crate) fn session_chat_background_for_chrome(chrome: u32) -> u32 {
    ghostex_settings_catalog::theme_colours::session_chat_background_for_chrome(chrome)
}

pub(crate) fn accent_color_for_tint(tint: u32, light: bool) -> u32 {
    ghostex_settings_catalog::theme_colours::accent_color_for_tint(tint, light)
}

/// The Accent theme role when it is set (helpers/theme_roles.rs), laid over the work area so a
/// see-through value still gives one solid accent.
pub(crate) fn theme_accent_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    let derived = ghostex_settings_catalog::theme_colours::theme_accent_for_variant(object, light);
    let Some(role) = crate::app::helpers::theme_role_rgba("accent", light) else {
        return derived;
    };
    let under = sidebar_titlebar_rgb_channels(work_area_background_for_variant(object, light));
    let over = [role.r, role.g, role.b].map(|channel| channel * 255.0);
    sidebar_titlebar_pack_rgb(
        [0, 1, 2].map(|index| under[index] + (over[index] - under[index]) * role.a),
    )
}

#[cfg(test)]
mod full_colour_tests {
    use super::*;
    use crate::app::helpers::gpui_settings_hex_rgb;
    use serde_json::{Map, Value, json};

    fn settings(preset: &str, tint: &str, darkness: f64, points: f64) -> Map<String, Value> {
        let mut object = Map::new();
        object.insert("darkThemePreset".into(), json!(preset));
        object.insert(
            "customSidebarTitlebarBackgroundTintColor".into(),
            json!(tint),
        );
        object.insert(
            "customSidebarTitlebarBackgroundDarknessPercent".into(),
            json!(darkness),
        );
        object.insert("themeSidebarContrast".into(), json!(points));
        object.insert("themeWorkAreaContrast".into(), json!(points));
        object
    }

    /// What the sidebar and work area chrome resolved to before Full existed: depth only.
    fn depth_only(object: &Map<String, Value>) -> (u32, u32) {
        let (darkness, tint) = dark_chrome_controls(object);
        let delta = work_area_contrast_delta(object);
        (
            sidebar_titlebar_background_for_darkness(darkness, tint),
            sidebar_titlebar_background_for_darkness(
                clamp_sidebar_titlebar_background_darkness_percent(darkness + delta),
                tint,
            ),
        )
    }

    fn resolved(object: &Map<String, Value>) -> (u32, u32) {
        (
            resolved_custom_sidebar_titlebar_background_for_variant(object, false),
            resolved_work_area_chrome_for_variant(object, false),
        )
    }

    /// Every Colourfulness position from Subtle (+4) to Vivid (-12), in half points.
    fn subtle_to_vivid() -> impl Iterator<Item = f64> {
        (0..=32).map(|step| 4.0 - f64::from(step) * 0.5)
    }

    #[test]
    fn subtle_to_vivid_is_unchanged_for_every_preset_and_custom_colour() {
        for (preset, _, _) in DARK_THEME_PRESET_CONTROLS {
            for points in subtle_to_vivid() {
                let object = settings(preset, "#808080", 96.0, points);
                assert_eq!(
                    resolved(&object),
                    depth_only(&object),
                    "{preset} at {points}"
                );
            }
        }
        for tint in ["#273136", "#808080", "#ffffff", "#884444", "#1f3a8a"] {
            for points in subtle_to_vivid() {
                let darkness = clamp_sidebar_titlebar_background_darkness_percent(96.0 + points);
                let object = settings("custom", tint, darkness, points);
                assert_eq!(resolved(&object), depth_only(&object), "{tint} at {points}");
            }
        }
    }

    #[test]
    fn vivid_values_below_the_limit_are_still_cut_to_85() {
        // A preset at Vivid computes 94 - 12 = 82, a custom colour 96 - 12 = 84, and a hand-edited
        // file may hold 84 or 82: all still draw what depth 85 draws.
        let at_85 = sidebar_titlebar_background_for_darkness(85.0, 0x2f7f7f);
        assert_eq!(resolved(&settings("teal", "#808080", 96.0, -12.0)).0, at_85);
        for stored in [84.0, 82.0, 0.0] {
            let object = settings("custom", "#273136", stored, -12.0);
            assert_eq!(resolved(&object).0, 0x00141f, "stored {stored}");
        }
        // The values before this change: Monokai's colour at Vivid, the default Graphite at Soft.
        assert_eq!(
            resolved(&settings("custom", "#273136", 85.0, -12.0)).0,
            0x00141f
        );
        assert_eq!(
            resolved(&settings("gray", "#808080", 96.0, 0.0)).0,
            0x0b0b0b
        );
    }

    #[test]
    fn full_is_the_picked_colour_exactly() {
        let object = settings("custom", "#273136", 85.0, -24.0);
        assert_eq!(resolved(&object), (0x273136, 0x273136));
        // The chat and menus keep their small steps toward white from it.
        assert_eq!(session_chat_background_for_chrome(0x273136), 0x293338);
        assert_eq!(
            sidebar_titlebar_menu_background_for_chrome(0x273136),
            0x323b40
        );
    }

    #[test]
    fn every_step_past_vivid_keeps_its_red_and_never_turns_saturated_blue() {
        let mut previous = resolved(&settings("custom", "#273136", 85.0, -12.0)).0;
        for step in 1..=24 {
            let points = -12.0 - f64::from(step) * 0.5;
            let chrome = resolved(&settings("custom", "#273136", 85.0, points)).0;
            let [red, green, blue] = sidebar_titlebar_rgb_channels(chrome);
            assert!(red > 0.0, "{points}: {chrome:06x}");
            // Every channel moves straight from #00141f to #273136, never past it.
            assert!((0.0..=39.0).contains(&red), "{points}: {chrome:06x}");
            assert!((20.0..=49.0).contains(&green), "{points}: {chrome:06x}");
            assert!((31.0..=54.0).contains(&blue), "{points}: {chrome:06x}");
            let [previous_red, ..] = sidebar_titlebar_rgb_channels(previous);
            assert!(red >= previous_red, "{points}: {chrome:06x}");
            previous = chrome;
        }
    }

    #[test]
    fn presets_and_the_neutral_default_reach_their_own_tint_kept_readable() {
        for (preset, _, tint) in DARK_THEME_PRESET_CONTROLS {
            let chrome = resolved(&settings(preset, "#808080", 96.0, -24.0)).0;
            let channels = sidebar_titlebar_rgb_channels(chrome).map(f64::from);
            let luminance = ghostex_settings_catalog::dark_chrome::relative_luminance(channels);
            assert!(
                luminance
                    <= ghostex_settings_catalog::dark_chrome::FULL_COLOUR_MAX_LUMINANCE + 0.002,
                "{preset}: {chrome:06x}"
            );
            let picked = sidebar_titlebar_rgb_channels(tint);
            // Same hue: the channels keep their order.
            for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                if (picked[a] - picked[b]).abs() >= 8.0 {
                    assert_eq!(
                        picked[a] > picked[b],
                        channels[a] > channels[b],
                        "{preset}: {chrome:06x}"
                    );
                }
            }
        }
        assert_eq!(
            resolved(&settings("black", "#808080", 96.0, -24.0)).0,
            0x000000
        );
    }

    #[test]
    fn the_sidebar_gradient_is_unchanged_to_vivid_and_flat_at_full() {
        assert_eq!(
            sidebar_titlebar_gradient_stops_toward_full(0x00141f, 0.0),
            sidebar_titlebar_gradient_stops(0x00141f)
        );
        assert_eq!(
            sidebar_titlebar_gradient_stops(0x00141f),
            (0x001421, 0x001627)
        );
        assert_eq!(
            sidebar_titlebar_gradient_stops_toward_full(0x273136, 1.0),
            (0x273136, 0x273136)
        );
    }

    fn light_settings(preset: &str, tint: &str, lightness: f64, points: f64) -> Map<String, Value> {
        let mut object = Map::new();
        object.insert("lightThemePreset".into(), json!(preset));
        object.insert(
            "customSidebarTitlebarLightBackgroundTintColor".into(),
            json!(tint),
        );
        object.insert(
            "customSidebarTitlebarLightBackgroundLightnessPercent".into(),
            json!(lightness),
        );
        object.insert("themeSidebarContrast".into(), json!(points));
        object.insert("themeWorkAreaContrast".into(), json!(points));
        object
    }

    fn resolved_light(object: &Map<String, Value>) -> (u32, u32) {
        (
            resolved_custom_sidebar_titlebar_background_for_variant(object, true),
            resolved_work_area_chrome_for_variant(object, true),
        )
    }

    #[test]
    fn light_mode_is_unchanged_from_subtle_to_vivid() {
        for (preset, lightness, tint) in LIGHT_THEME_PRESET_CONTROLS {
            for points in subtle_to_vivid() {
                let object = light_settings(preset, "#808080", 96.0, points);
                let before = sidebar_titlebar_light_background_for_lightness(
                    clamp_sidebar_titlebar_light_background_lightness_percent(lightness + points),
                    tint,
                );
                assert_eq!(
                    resolved_light(&object),
                    (before, before),
                    "{preset} at {points}"
                );
            }
        }
        for points in subtle_to_vivid() {
            let saved = clamp_sidebar_titlebar_light_background_lightness_percent(96.0 + points);
            let object = light_settings("custom", "#eee6a6", saved, points);
            let before = sidebar_titlebar_light_background_for_lightness(
                clamp_sidebar_titlebar_light_background_lightness_percent(saved - 1.0),
                0xeee6a6,
            );
            assert_eq!(
                resolved_light(&object),
                (before, before),
                "custom at {points}"
            );
        }
    }

    #[test]
    fn light_full_is_the_picked_colour_or_the_lightest_safe_step_toward_it() {
        // Kanagawa Lotus's pale yellow is light enough to paint as picked, and the work area is a
        // quarter toward white from it: the theme's #f2ecbc.
        let lotus = light_settings("custom", "#eee6a6", 84.0, -24.0);
        assert_eq!(resolved_light(&lotus), (0xeee6a6, 0xeee6a6));
        assert_eq!(work_area_background_for_variant(&lotus, true), 0xf2ecbc);
        // Halfway past Vivid the chrome sits between its Vivid colour and the pick.
        let halfway = resolved_light(&light_settings("custom", "#eee6a6", 84.0, -18.0)).0;
        let vivid = resolved_light(&light_settings("custom", "#eee6a6", 84.0, -12.0)).0;
        assert!(halfway != vivid && halfway != 0xeee6a6, "{halfway:06x}");
        // Graphite's grey is lifted to #cfcfcf (luminance 0.62396), the first painted grey at the limit.
        assert_eq!(
            resolved_light(&light_settings("gray", "#808080", 96.0, -24.0)).0,
            0xcfcfcf
        );
        // The squares' darker tints are lifted only as far as the dark text needs, packed colour
        // included.
        for (preset, _, tint) in LIGHT_THEME_PRESET_CONTROLS {
            let chrome = resolved_light(&light_settings(preset, "#808080", 96.0, -24.0)).0;
            let channels = sidebar_titlebar_rgb_channels(chrome).map(f64::from);
            let luminance = ghostex_settings_catalog::dark_chrome::relative_luminance(channels);
            let limit = ghostex_settings_catalog::dark_chrome::LIGHT_FULL_COLOUR_MIN_LUMINANCE;
            assert!(
                luminance >= limit - 0.002,
                "{preset}: {chrome:06x} {luminance}"
            );
            assert!(
                luminance <= limit + 0.01 || tint == 0xffffff,
                "{preset}: {chrome:06x}"
            );
        }
    }

    #[test]
    fn a_custom_neutral_grey_draws_like_graphite() {
        for grey in ["#777777", "#7f7f7f", "#808080", "#3a3a3a"] {
            assert_eq!(
                resolved(&settings("custom", grey, 96.0, 0.0)).0,
                0x0b0b0b,
                "{grey}"
            );
        }
        // Black and the calibrated colours keep their own base; a coloured custom tint still
        // takes its hue from the calibration colour.
        assert_eq!(
            resolved(&settings("custom", "#000000", 96.0, 0.0)).0,
            0x000000
        );
        assert_eq!(
            resolved(&settings("custom", "#336699", 96.0, 0.0)).0,
            0x0a0b0e
        );
        let coloured = resolved(&settings("custom", "#884455", 96.0, 0.0)).0;
        let [red, green, blue] = sidebar_titlebar_rgb_channels(coloured);
        assert!(red > blue && blue > green, "{coloured:06x}");
    }

    #[test]
    fn a_preset_work_area_starts_from_the_unclamped_sidebar_depth() {
        let mut object = settings("gray", "#808080", 96.0, 0.0);
        let both_soft = work_area_background_for_variant(&object, false);
        assert_eq!(both_soft, 0x0d0d0d);
        // The sidebar at Vivid computes 84 (drawn at 85); the work area at Soft stays at 96.
        object.insert("themeSidebarContrast".into(), json!(-12.0));
        assert_eq!(work_area_background_for_variant(&object, false), both_soft);
        assert_eq!(
            resolved_custom_sidebar_titlebar_background_for_variant(&object, false),
            sidebar_titlebar_background_for_darkness(85.0, 0x808080)
        );
        // A custom light colour saved at 60 draws at 59, cut to 60; its work area 4 points less
        // colourful is 63.
        let mut light = light_settings("custom", "#336699", 60.0, -4.0);
        light.insert("themeWorkAreaContrast".into(), json!(0.0));
        assert_eq!(
            resolved_light(&light),
            (
                sidebar_titlebar_light_background_for_lightness(60.0, 0x336699),
                sidebar_titlebar_light_background_for_lightness(63.0, 0x336699)
            )
        );
    }

    /// The theme's own background each preset's work area paints, from the theme projects;
    /// Solarized Dark keeps red 3, the least the dark work area's step toward white leaves.
    const PRESET_WORK_AREA: [(&str, u32); 20] = [
        ("mocha", 0x1e1e2e),
        ("macchiato", 0x24273a),
        ("frappe", 0x303446),
        ("latte", 0xeff1f5),
        ("dracula", 0x282a36),
        ("alucard", 0xfffbeb),
        ("sol-dark", 0x032b36),
        ("sol-light", 0xfdf6e3),
        ("tn-night", 0x1a1b26),
        ("tn-storm", 0x24283b),
        ("tn-moon", 0x222436),
        ("tn-day", 0xe1e2e7),
        ("nord", 0x2e3440),
        ("kg-wave", 0x1f1f28),
        ("kg-dragon", 0x181616),
        ("kg-lotus", 0xf2ecbc),
        ("ef-dark", 0x2d353b),
        ("ef-light", 0xfdf6e3),
        ("gm-dark", 0x282828),
        ("gm-light", 0xfbf1c7),
    ];

    #[test]
    fn every_theme_preset_paints_its_theme_background() {
        use crate::app::window::settings_modal::tabs::theme::presets::{
            theme_preset_patch, theme_presets,
        };
        let presets = theme_presets();
        assert_eq!(presets.len(), PRESET_WORK_AREA.len());
        for (id, background) in PRESET_WORK_AREA {
            let preset = presets.iter().find(|preset| preset.id == id).unwrap();
            let object = theme_preset_patch(preset, None);
            let light = !preset.dark;
            let pick = gpui_settings_hex_rgb(Some(&json!(preset.tint))).unwrap();
            let sidebar = resolved_custom_sidebar_titlebar_background_for_variant(&object, light);
            // Full paints the picked colour itself in both areas, and the work area's step
            // toward white lands on the theme's background.
            assert_eq!(sidebar, pick, "{id}");
            assert_eq!(
                resolved_work_area_chrome_for_variant(&object, light),
                pick,
                "{id}"
            );
            assert_eq!(
                work_area_background_for_variant(&object, light),
                background,
                "{id}: {:06x}",
                work_area_background_for_variant(&object, light)
            );
            if preset.dark {
                assert_eq!(
                    sidebar_titlebar_gradient_stops_toward_full(sidebar, 1.0),
                    (sidebar, sidebar),
                    "{id}"
                );
            }
        }
    }
}
