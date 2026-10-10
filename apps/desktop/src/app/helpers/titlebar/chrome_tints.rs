use super::*;
use crate::app::helpers::*;

/// Rust port of `getSidebarTitlebarGradientColors` /
/// `normalizedSidebarTitlebarTintDirection` in packages/shared/ghostex-settings.ts (deleted 2026-10-01): the
/// tint direction is the background's per-channel deviation from its average,
/// normalized by its largest channel magnitude (neutral grays stay neutral),
/// and the two stops sit at +2 and +10 of that direction. Rounding matches JS
/// `Math.round` (half toward positive infinity) so both sides emit identical
/// hex stops.
pub(crate) fn sidebar_titlebar_gradient_stops(background: u32) -> (u32, u32) {
    let base = sidebar_titlebar_rgb_channels(background);
    let direction = sidebar_titlebar_tint_direction(base);
    let stop = |amount: f32| -> u32 {
        sidebar_titlebar_pack_rgb([
            base[0] + direction[0] * amount,
            base[1] + direction[1] * amount,
            base[2] + direction[2] * amount,
        ])
    };
    (stop(2.0), stop(10.0))
}

pub(crate) fn sidebar_titlebar_rgb_channels(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
        (color & 0xff) as f32,
    ]
}

/// JS `Math.round` (half toward positive infinity) + 0-255 clamp per channel,
/// so Rust emits the identical hex the shared TS pipeline computes.
pub(crate) fn sidebar_titlebar_pack_rgb(channels: [f32; 3]) -> u32 {
    channels.iter().fold(0u32, |rgb, value| {
        (rgb << 8) | ((value + 0.5).floor().clamp(0.0, 255.0) as u32)
    })
}

pub(crate) fn sidebar_titlebar_tint_direction(base: [f32; 3]) -> [f32; 3] {
    let average = (base[0] + base[1] + base[2]) / 3.0;
    let mut direction = [base[0] - average, base[1] - average, base[2] - average];
    let magnitude = direction
        .iter()
        .map(|channel| channel.abs())
        .fold(0.0f32, f32::max);
    if magnitude < 0.5 {
        return [0.0, 0.0, 0.0];
    }
    for channel in &mut direction {
        *channel /= magnitude;
    }
    direction
}

/// CDXC:Theming 2026-09-08 SEE-ALSO:
/// packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01) owns the matching neutral #808080 tint at 96 contrast default.
pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_RGB: u32 = 0x0b0b0b;
pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB: u32 = 0x808080;
pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = 96.0;
pub(crate) const MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = 85.0;
pub(crate) const MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = 100.0;
pub(crate) const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT: f64 = 95.0;
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_RGB: u32 = 0x040607;

/// Mirror of `CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS` in
/// packages/shared/ghostex-settings.ts (deleted 2026-10-01). Keep both tables in sync.
pub(crate) const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS: [(u32, u32); 25] = [
    (0x000000, 0x000000),
    (0xffffff, 0x0e0e0e),
    (0x808080, 0x0e0e0e),
    (0x88d7ff, 0x0a0f12),
    (0x4f6672, 0x0c0e10),
    (0x884444, 0x0d0005),
    (0x8a5330, 0x100502),
    (0x8a6a2f, 0x110a02),
    (0x657a3f, 0x0c1005),
    (0x3f7a5f, 0x031006),
    (0x2f7d66, 0x03100c),
    (0x287c7f, 0x031011),
    (0x336699, 0x0c0e11),
    (0x4f5f96, 0x080912),
    (0x6c4f8f, 0x0a0611),
    (0x854f7a, 0x100611),
    (0x8a4f5f, 0x100409),
    // 2026-09-25 preset colours (Slate, Midnight, Indigo, Teal, Forest, Olive, Amber, Rose).
    (0x4a6a8a, 0x070d14),
    (0x1f3a8a, 0x02061a),
    (0x4b4fa6, 0x08081c),
    (0x2f7f7f, 0x021213),
    (0x2e6a3a, 0x031205),
    (0x6b6b35, 0x0e0f03),
    (0x8a6a2a, 0x130c02),
    (0x8a4a5c, 0x12040b),
];

pub(crate) fn clamp_sidebar_titlebar_background_darkness_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;
    }
    ((value * 2.0 + 0.5).floor() / 2.0).clamp(
        MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
        MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    )
}

pub(crate) fn sidebar_titlebar_background_darkness_for_color(background: u32) -> f64 {
    let [red, green, blue] = sidebar_titlebar_rgb_channels(background);
    let luminance = f64::from(0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255.0;
    clamp_sidebar_titlebar_background_darkness_percent((1.0 - luminance) * 100.0)
}

/// Rust port of `getSidebarTitlebarBackgroundForDarkness` in
/// packages/shared/ghostex-settings.ts (deleted 2026-10-01): resolve the calibrated dark background for the
/// selected tint (falling back to the neutral default for same-channel tints,
/// or default-base + tint-direction * 4 for uncalibrated tints), then scale it
/// with the Background Contrast slider.
pub(crate) fn sidebar_titlebar_background_for_darkness(darkness_percent: f64, tint: u32) -> u32 {
    let darkness = clamp_sidebar_titlebar_background_darkness_percent(darkness_percent);
    let default_dark_tint_background = CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS
        .iter()
        .find(|(key, _)| *key == tint)
        .map(|(_, value)| sidebar_titlebar_rgb_channels(*value))
        .unwrap_or_else(|| {
            let color = sidebar_titlebar_rgb_channels(tint);
            let spread = color.iter().fold(0.0f32, |max, value| max.max(*value))
                - color.iter().fold(255.0f32, |min, value| min.min(*value));
            if spread < 1.0 {
                return sidebar_titlebar_rgb_channels(
                    CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_RGB,
                );
            }
            let direction = sidebar_titlebar_tint_direction(color);
            let base =
                sidebar_titlebar_rgb_channels(CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_RGB);
            [
                base[0] + direction[0] * 4.0,
                base[1] + direction[1] * 4.0,
                base[2] + direction[2] * 4.0,
            ]
        });
    if darkness == MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT {
        return 0x000000;
    }
    let scale = ((MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT - darkness)
        / (MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT
            - CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT))
        as f32;
    sidebar_titlebar_pack_rgb([
        default_dark_tint_background[0] * scale,
        default_dark_tint_background[1] * scale,
        default_dark_tint_background[2] * scale,
    ])
}

/// CDXC:Theming 2026-09-22 SEE-ALSO:
/// Rust port of the light chrome scale in packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01)
/// (`getSidebarTitlebarLightBackgroundForLightness`): 100 is white, the neutral tint at 96 is the
/// #f4f4f5 light chrome, and the calibrated pale tint table mirrors `CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS`.
pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_RGB: u32 = 0x808080;
pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT: f64 = 96.0;
pub(crate) const MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT: f64 = 60.0;
pub(crate) const MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT: f64 = 100.0;
const CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_SCALE_REFERENCE_LIGHTNESS_PERCENT: f64 = 95.0;
const CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_RGB: u32 = 0xf1f1f2;
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS: [(u32, u32); 25] = [
    (0x000000, 0xf1f1f2),
    (0xffffff, 0xf1f1f2),
    (0x808080, 0xf1f1f2),
    (0x88d7ff, 0xedf4fa),
    (0x4f6672, 0xeef1f3),
    (0x884444, 0xf7ecec),
    (0x8a5330, 0xf8f0e9),
    (0x8a6a2f, 0xf7f3e8),
    (0x657a3f, 0xf1f5ea),
    (0x3f7a5f, 0xecf4ee),
    (0x2f7d66, 0xeaf4f0),
    (0x287c7f, 0xeaf4f4),
    (0x336699, 0xecf1f7),
    (0x4f5f96, 0xeff0f7),
    (0x6c4f8f, 0xf2edf7),
    (0x854f7a, 0xf7ecf3),
    (0x8a4f5f, 0xf7ecef),
    // 2026-09-25 preset colours (Slate, Midnight, Indigo, Teal, Forest, Olive, Amber, Rose).
    (0x4a6a8a, 0xebf1f8),
    (0x1f3a8a, 0xedeff8),
    (0x4b4fa6, 0xeeeef8),
    (0x2f7f7f, 0xebf4f5),
    (0x2e6a3a, 0xedf7f0),
    (0x6b6b35, 0xf4f4ec),
    (0x8a6a2a, 0xf6f2ec),
    (0x8a4a5c, 0xf7edf0),
];

pub(crate) fn clamp_sidebar_titlebar_light_background_lightness_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT;
    }
    ((value * 2.0 + 0.5).floor() / 2.0).clamp(
        MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
        MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
    )
}

pub(crate) fn sidebar_titlebar_light_background_for_lightness(
    lightness_percent: f64,
    tint: u32,
) -> u32 {
    let lightness = clamp_sidebar_titlebar_light_background_lightness_percent(lightness_percent);
    if lightness == MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT {
        return 0xffffff;
    }
    let calibrated = CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS
        .iter()
        .find(|(key, _)| *key == tint)
        .map(|(_, value)| sidebar_titlebar_rgb_channels(*value))
        .unwrap_or_else(|| {
            let color = sidebar_titlebar_rgb_channels(tint);
            let base = sidebar_titlebar_rgb_channels(
                CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_RGB,
            );
            let spread = color.iter().fold(0.0f32, |max, value| max.max(*value))
                - color.iter().fold(255.0f32, |min, value| min.min(*value));
            if spread < 1.0 {
                return base;
            }
            let direction = sidebar_titlebar_tint_direction(color);
            [
                base[0] + direction[0] * 6.0,
                base[1] + direction[1] * 6.0,
                base[2] + direction[2] * 6.0,
            ]
        });
    let scale = ((MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT - lightness)
        / (MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT
            - CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_SCALE_REFERENCE_LIGHTNESS_PERCENT))
        as f32;
    sidebar_titlebar_pack_rgb([
        255.0 - (255.0 - calibrated[0]) * scale,
        255.0 - (255.0 - calibrated[1]) * scale,
        255.0 - (255.0 - calibrated[2]) * scale,
    ])
}

/// CDXC:Theming 2026-09-25 SEE-ALSO:
/// Mirror of `DARK_THEME_PRESET_CONTROLS` / `LIGHT_THEME_PRESET_CONTROLS` in
/// packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01): each of the sixteen presets per appearance is
/// a (contrast, tint) pair fed through the same scale as the custom controls, and the new tints
/// have matching entries in the tint tables above. Keep the tables in sync entry for entry.
pub(crate) const DARK_THEME_PRESET_CONTROLS: [(&str, f64, u32); 16] = [
    (
        "gray",
        DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
        DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB,
    ),
    ("black", 100.0, 0x000000),
    ("slate", 94.0, 0x4a6a8a),
    ("midnight", 94.0, 0x1f3a8a),
    ("blue", 96.0, 0x336699),
    ("indigo", 94.0, 0x4b4fa6),
    ("teal", 94.0, 0x2f7f7f),
    ("green", 96.0, 0x3f7a5f),
    ("forest", 94.0, 0x2e6a3a),
    ("olive", 94.0, 0x6b6b35),
    ("amber", 94.0, 0x8a6a2a),
    ("orange", 96.0, 0x8a5330),
    ("red", 96.0, 0x884444),
    ("rose", 94.0, 0x8a4a5c),
    ("pink", 96.0, 0x854f7a),
    ("purple", 96.0, 0x6c4f8f),
];
pub(crate) const LIGHT_THEME_PRESET_CONTROLS: [(&str, f64, u32); 16] = [
    (
        "gray",
        DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
        DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_RGB,
    ),
    ("white", 100.0, 0xffffff),
    ("slate", 95.0, 0x4a6a8a),
    ("midnight", 94.0, 0x1f3a8a),
    ("blue", 95.0, 0x336699),
    ("indigo", 95.0, 0x4b4fa6),
    ("teal", 95.0, 0x2f7f7f),
    ("green", 95.0, 0x3f7a5f),
    ("forest", 94.0, 0x2e6a3a),
    ("olive", 95.0, 0x6b6b35),
    ("amber", 95.0, 0x8a6a2a),
    ("orange", 95.0, 0x8a5330),
    ("red", 95.0, 0x884444),
    ("rose", 95.0, 0x8a4a5c),
    ("pink", 95.0, 0x854f7a),
    ("purple", 95.0, 0x6c4f8f),
];

/// The saved custom dark controls: the darkness slider (seeded from a valid legacy saved
/// background color when the slider key is missing) plus the tint choice. The stored
/// `customSidebarTitlebarBackgroundColor` hex itself is never the applied color.
pub(crate) fn custom_dark_chrome_controls(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (f64, u32) {
    let legacy_background =
        gpui_settings_hex_rgb(object.get("customSidebarTitlebarBackgroundColor"));
    let darkness_fallback = legacy_background
        .map(sidebar_titlebar_background_darkness_for_color)
        .unwrap_or(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT);
    let darkness = clamp_sidebar_titlebar_background_darkness_percent(
        object
            .get("customSidebarTitlebarBackgroundDarknessPercent")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(darkness_fallback),
    );
    let tint = gpui_settings_hex_rgb(object.get("customSidebarTitlebarBackgroundTintColor"))
        .unwrap_or(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB);
    (darkness, tint)
}

/// CDXC:Theming 2026-09-23 SEE-ALSO:
/// Mirror of `readThemeContrastPoints` / `SIDEBAR_CONTRAST_KEY` / `WORK_AREA_CONTRAST_KEY` in
/// packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01): `themeSidebarContrast` and
/// `themeWorkAreaContrast` are contrast points (-12 to 4, in half points) added to a preset theme's contrast. When
/// either is missing, the retired five-step `themeContrast` (-2 to 2) carries over as -8, -4, 0, 2 or
/// 4 points for both.
fn theme_contrast_points(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> f64 {
    if let Some(points) = object
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
    {
        return ((points * 2.0).round() / 2.0).clamp(-12.0, 4.0);
    }
    let step = object
        .get("themeContrast")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .map_or(0, |value| (value.round() as i64).clamp(-2, 2));
    match step {
        -2 => -8.0,
        -1 => -4.0,
        1 => 2.0,
        2 => 4.0,
        _ => 0.0,
    }
}

pub(crate) fn theme_contrast_offset(object: &serde_json::Map<String, serde_json::Value>) -> f64 {
    theme_contrast_points(object, "themeSidebarContrast")
}

/// How far the work area's contrast sits from the sidebar's, applied on top of whatever the
/// sidebar resolved to (a preset shifted by the sidebar contrast, or a Custom theme's own slider).
fn work_area_contrast_delta(object: &serde_json::Map<String, serde_json::Value>) -> f64 {
    theme_contrast_points(object, "themeWorkAreaContrast") - theme_contrast_offset(object)
}

/// Mirror of `resolveDarkChromeControls` plus the preset migration in
/// packages/shared/ghostex-settings/normalize.ts (deleted 2026-10-01): a missing or unknown preset with non-default
/// custom values means the user tuned them before the dropdown existed, so they stay in force.
pub(crate) fn dark_chrome_controls(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (f64, u32) {
    let custom = custom_dark_chrome_controls(object);
    let preset = object
        .get("darkThemePreset")
        .and_then(serde_json::Value::as_str);
    if preset == Some("custom") {
        return custom;
    }
    if let Some((_, darkness, tint)) = preset.and_then(|name| {
        DARK_THEME_PRESET_CONTROLS
            .iter()
            .find(|(key, _, _)| *key == name)
    }) {
        return (
            clamp_sidebar_titlebar_background_darkness_percent(
                *darkness + theme_contrast_offset(object),
            ),
            *tint,
        );
    }
    if custom
        != (
            DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
            DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB,
        )
    {
        return custom;
    }
    let (_, darkness, tint) = DARK_THEME_PRESET_CONTROLS[0];
    (
        clamp_sidebar_titlebar_background_darkness_percent(
            darkness + theme_contrast_offset(object),
        ),
        tint,
    )
}

/// Mirror of `resolveLightChromeControls` in packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01).
pub(crate) fn light_chrome_controls(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (f64, u32) {
    let preset = object
        .get("lightThemePreset")
        .and_then(serde_json::Value::as_str);
    if preset == Some("custom") {
        let lightness = clamp_sidebar_titlebar_light_background_lightness_percent(
            object
                .get("customSidebarTitlebarLightBackgroundLightnessPercent")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT),
        );
        let tint =
            gpui_settings_hex_rgb(object.get("customSidebarTitlebarLightBackgroundTintColor"))
                .unwrap_or(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_RGB);
        return (lightness, tint);
    }
    let (_, lightness, tint) = preset
        .and_then(|name| {
            LIGHT_THEME_PRESET_CONTROLS
                .iter()
                .find(|(key, _, _)| *key == name)
        })
        .copied()
        .unwrap_or(LIGHT_THEME_PRESET_CONTROLS[0]);
    (
        clamp_sidebar_titlebar_light_background_lightness_percent(
            lightness + theme_contrast_offset(object),
        ),
        tint,
    )
}

/// The chrome background one appearance resolves to from the saved settings, whichever
/// appearance the app is currently on. Chat uses this with its own theme's variant.
pub(crate) fn resolved_custom_sidebar_titlebar_background_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    if light {
        let (lightness, tint) = light_chrome_controls(object);
        sidebar_titlebar_light_background_for_lightness(lightness, tint)
    } else {
        let (darkness, tint) = dark_chrome_controls(object);
        sidebar_titlebar_background_for_darkness(darkness, tint)
    }
}

/// The chrome the work area's colour is derived from: the sidebar's chrome with the work area
/// contrast in place of the sidebar contrast.
pub(crate) fn resolved_work_area_chrome_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    let delta = work_area_contrast_delta(object);
    if light {
        let (lightness, tint) = light_chrome_controls(object);
        sidebar_titlebar_light_background_for_lightness(
            clamp_sidebar_titlebar_light_background_lightness_percent(lightness + delta),
            tint,
        )
    } else {
        let (darkness, tint) = dark_chrome_controls(object);
        sidebar_titlebar_background_for_darkness(
            clamp_sidebar_titlebar_background_darkness_percent(darkness + delta),
            tint,
        )
    }
}

/// CDXC:Theming 2026-09-23 DECISION:
/// User: "make the contrast show as a slider in the advanced and make sidebar and main contrast different please". The work area's own colour (chat, terminals, the workspace and its glass tint, web pages' content colour) comes from the theme at the work area contrast, while the sidebar keeps the sidebar contrast.
/// SEE-ALSO: `getWorkAreaBackgroundForSettings` in packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01).
pub(crate) fn work_area_background_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    session_chat_background_for_chrome(resolved_work_area_chrome_for_variant(object, light))
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
    let channels = sidebar_titlebar_rgb_channels(color);
    sidebar_titlebar_pack_rgb([
        channels[0] + (255.0 - channels[0]) * amount,
        channels[1] + (255.0 - channels[1]) * amount,
        channels[2] + (255.0 - channels[2]) * amount,
    ])
}

/// Mirror of the luminance split in `getSidebarTitlebarForegroundForBackground`.
fn sidebar_titlebar_background_is_light(color: u32) -> bool {
    let [red, green, blue] = sidebar_titlebar_rgb_channels(color);
    f64::from(0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255.0 > 0.54
}

/// CDXC:Theming 2026-09-22 DECISION:
/// User: the theme also colours the sidebar's dropdown menus and the chat view background. Mirror of
/// `getSidebarTitlebarMenuBackgroundForChrome` / `getSessionChatBackgroundForChrome` in
/// packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01): a fixed step toward white off the resolved chrome
/// (menu 5% on dark chrome, 70% on light; chat 1% on dark and 25% on light, the same two-tone split
/// in both appearances), so the neutral defaults land on the previous #0d0d0d dark chat and #f7f7f7.
pub(crate) fn sidebar_titlebar_menu_background_for_chrome(chrome: u32) -> u32 {
    sidebar_titlebar_blend_toward_white(
        chrome,
        if sidebar_titlebar_background_is_light(chrome) {
            0.7
        } else {
            0.05
        },
    )
}

pub(crate) fn session_chat_background_for_chrome(chrome: u32) -> u32 {
    sidebar_titlebar_blend_toward_white(
        chrome,
        if sidebar_titlebar_background_is_light(chrome) {
            0.25
        } else {
            0.01
        },
    )
}

/// Rust port of `getAccentColorForBackgroundTint` / `getLightAccentColorForBackgroundTint` in
/// packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01): the tint's hue at a fixed lightness (0.75 for the
/// dark appearance, 0.38 for the light one) with saturation held in 0.55-0.9, and a fixed colour for a
/// neutral tint (#86d3f8 dark, #262626 light). Keep both in lockstep.
pub(crate) fn accent_color_for_tint(tint: u32, light: bool) -> u32 {
    let [red, green, blue] = sidebar_titlebar_rgb_channels(tint).map(|channel| channel / 255.0);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    if (max - min) * 255.0 < 1.0 {
        return if light { 0x262626 } else { 0x86d3f8 };
    }
    let chroma = max - min;
    let tint_lightness = (max + min) / 2.0;
    let tint_saturation = chroma / (1.0 - (2.0 * tint_lightness - 1.0).abs());
    let hue_sextant = if max == red {
        (((green - blue) / chroma) % 6.0 + 6.0) % 6.0
    } else if max == green {
        (blue - red) / chroma + 2.0
    } else {
        (red - green) / chroma + 4.0
    };
    let lightness: f32 = if light { 0.38 } else { 0.75 };
    let saturation = tint_saturation.clamp(0.55, 0.9);
    let accent_chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let secondary = accent_chroma * (1.0 - ((hue_sextant % 2.0) - 1.0).abs());
    let offset = lightness - accent_chroma / 2.0;
    let [r, g, b] = if hue_sextant < 1.0 {
        [accent_chroma, secondary, 0.0]
    } else if hue_sextant < 2.0 {
        [secondary, accent_chroma, 0.0]
    } else if hue_sextant < 3.0 {
        [0.0, accent_chroma, secondary]
    } else if hue_sextant < 4.0 {
        [0.0, secondary, accent_chroma]
    } else if hue_sextant < 5.0 {
        [secondary, 0.0, accent_chroma]
    } else {
        [accent_chroma, 0.0, secondary]
    };
    sidebar_titlebar_pack_rgb([
        (r + offset) * 255.0,
        (g + offset) * 255.0,
        (b + offset) * 255.0,
    ])
}

/// The accent for one appearance, from the tint that appearance's chrome actually paints.
pub(crate) fn theme_accent_for_variant(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    let tint = if light {
        light_chrome_controls(object).1
    } else {
        dark_chrome_controls(object).1
    };
    accent_color_for_tint(tint, light)
}
