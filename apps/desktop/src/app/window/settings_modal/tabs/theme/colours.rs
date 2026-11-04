//! The theme colour math the Theme page draws its colour squares and its Colourfulness preview
//! with, and the simple controls' settings mappings (packages/core-ui/settings-modal/ (deleted 2026-10-01)
//! theme-simple-controls.tsx (deleted 2026-10-01), packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01)).
//!
//! CDXC:Theming 2026-09-28 SEE-ALSO:
//! This is the third copy of the chrome scale: packages/shared/ghostex-settings/titlebar-color.ts (deleted 2026-10-01) owns it and apps/desktop/src/app/helpers/titlebar/ paints the app with it. The Settings view cannot reach the app helpers (the preview binary includes it alone), so it ports the functions and reads every table (presets, calibrated tints) from the generated catalog; keep the functions in lockstep with titlebar-color.ts.
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::store::SettingsValues;
use ghostex_settings_catalog::dark_chrome::{
    MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT, THEME_CONTRAST_FULL_POINTS,
    THEME_CONTRAST_VIVID_POINTS, dark_chrome_toward_full, full_colour_fade,
};
use serde_json::{Map, Value, json};

const TITLEBAR_MODULE: &str = module::TITLEBAR_COLOR;

pub(super) const MIN_DARKNESS: f64 = MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;
pub(super) const MAX_DARKNESS: f64 = 100.0;
const DARKNESS_SCALE_REFERENCE: f64 = 95.0;
pub(super) const MIN_LIGHTNESS: f64 = 60.0;
pub(super) const MAX_LIGHTNESS: f64 = 100.0;
const LIGHTNESS_SCALE_REFERENCE: f64 = 95.0;
const DEFAULT_TINT: &str = "#808080";
const DEFAULT_DARK_BACKGROUND: &str = "#0b0b0b";
/// Where depth stops: Vivid. Colourfulness goes on to Full, fading toward the picked colour.
const CONTRAST_MIN_POINTS: f64 = THEME_CONTRAST_VIVID_POINTS;
const CONTRAST_MAX_POINTS: f64 = 4.0;

/// `COLOURFULNESS_CHOICES`: the named points Subtle +4, Soft 0 (the shipped default), Balanced -4, Rich -8, Vivid -12, Full -24.
///
/// CDXC:Theming 2026-10-31 DECISION:
/// Issue #357, user (Q837): "Lower the limit and make the far end reach the exact colour you picked." Colourfulness runs on past Vivid to Full: in dark mode the sidebar and work area fade from their Vivid colour toward the picked colour (a preset's own tint) and are that colour at Full; light mode stays at Vivid past it. The rule and the depth limit live once in packages/settings-catalog/src/dark_chrome.rs.
///
/// CDXC:Theming 2026-10-10 DECISION:
/// User asked to make the Colourfulness sliders more granular and picked 33 positions: half-point steps from Subtle (+4) to Vivid (-12), with the five names still sitting exactly at their points and the label beside the slider showing the nearest name. This supersedes the five steps of 2026-09-25 (User: "You can switch it between more colorful and less colorful"); Colourfulness still sets the sidebar and work area contrast together, and More colour options can set the two areas apart.
pub(super) const COLOURFULNESS: [(&str, f64); 6] = [
    ("Subtle", 4.0),
    ("Soft", 0.0),
    ("Balanced", -4.0),
    ("Rich", -8.0),
    ("Vivid", -12.0),
    ("Full", THEME_CONTRAST_FULL_POINTS),
];

/// The contrast points between two neighbouring Colourfulness positions.
const COLOURFULNESS_STEP_POINTS: f64 = 0.5;
/// The last Colourfulness position: 0 is Subtle (+4), 32 is Vivid (-12), 56 is Full (-24).
pub(super) const COLOURFULNESS_LAST_POSITION: usize = 56;

/// The contrast points one Colourfulness position sets.
pub(super) fn colourfulness_points(position: usize) -> f64 {
    CONTRAST_MAX_POINTS
        - position.min(COLOURFULNESS_LAST_POSITION) as f64 * COLOURFULNESS_STEP_POINTS
}

/// The name of the named point nearest to `points` (Subtle, Soft, Balanced, Rich, Vivid or Full).
pub(super) fn colourfulness_name(points: f64) -> &'static str {
    let mut best = 0;
    for (index, (_, value)) in COLOURFULNESS.iter().enumerate() {
        if (value - points).abs() < (COLOURFULNESS[best].1 - points).abs() {
            best = index;
        }
    }
    COLOURFULNESS[best].0
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Rgb(pub(super) [f64; 3]);

impl Rgb {
    pub(super) fn gpui(self) -> gpui::Rgba {
        gpui::Rgba {
            r: (channel(self.0[0]) / 255.0) as f32,
            g: (channel(self.0[1]) / 255.0) as f32,
            b: (channel(self.0[2]) / 255.0) as f32,
            a: 1.0,
        }
    }

    fn hex(self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            channel(self.0[0]) as u8,
            channel(self.0[1]) as u8,
            channel(self.0[2]) as u8
        )
    }
}

/// `clampColorChannel`.
fn channel(value: f64) -> f64 {
    value.round().clamp(0.0, 255.0)
}

/// `normalizeSidebarTitlebarHexColor`.
fn normalize_hex(value: &str, fallback: &str) -> String {
    let normalized = value.trim().to_lowercase();
    let valid = normalized.len() == 7
        && normalized.starts_with('#')
        && normalized[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if valid {
        normalized
    } else {
        fallback.to_string()
    }
}

/// `parseSidebarTitlebarHexColor`.
fn parse_hex(value: &str) -> Rgb {
    let hex = normalize_hex(value, DEFAULT_DARK_BACKGROUND);
    let part = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16)
            .map(f64::from)
            .unwrap_or(0.0)
    };
    Rgb([part(1..3), part(3..5), part(5..7)])
}

fn is_neutral(color: Rgb) -> bool {
    let max = color.0.iter().cloned().fold(f64::MIN, f64::max);
    let min = color.0.iter().cloned().fold(f64::MAX, f64::min);
    max - min < 1.0
}

/// `normalizedSidebarTitlebarTintDirection`.
fn tint_direction(color: Rgb) -> [f64; 3] {
    let average = (color.0[0] + color.0[1] + color.0[2]) / 3.0;
    let direction = [
        color.0[0] - average,
        color.0[1] - average,
        color.0[2] - average,
    ];
    let magnitude = direction
        .iter()
        .map(|value| value.abs())
        .fold(0.0, f64::max);
    if magnitude < 0.5 {
        return [0.0; 3];
    }
    direction.map(|value| value / magnitude)
}

fn calibrated(table: &str, tint: &str) -> Option<Rgb> {
    settings_catalog()
        .module_value(TITLEBAR_MODULE, table)
        .and_then(|table| table.get(tint))
        .and_then(Value::as_str)
        .map(parse_hex)
}

fn calibration(name: &str, fallback: &str) -> Rgb {
    let value = settings_catalog().text(TITLEBAR_MODULE, name);
    parse_hex(if value.is_empty() { fallback } else { &value })
}

/// `clampSidebarTitlebarBackgroundDarknessPercent`.
pub(super) fn clamp_darkness(value: f64) -> f64 {
    if !value.is_finite() {
        return 96.0;
    }
    round_half(value).clamp(MIN_DARKNESS, MAX_DARKNESS)
}

/// `CUSTOM_LIGHT_THEME_DEPTH_SHIFT` in helpers/titlebar/chrome_tints.rs: a custom light colour is
/// drawn one point deeper than its saved depth, at the tinted presets' depth (this page is also
/// compiled into the modal demo, which has no app helpers).
const CUSTOM_LIGHT_THEME_DEPTH_SHIFT: f64 = -1.0;

/// `clampSidebarTitlebarLightBackgroundLightnessPercent`.
pub(super) fn clamp_lightness(value: f64) -> f64 {
    if !value.is_finite() {
        return 96.0;
    }
    round_half(value).clamp(MIN_LIGHTNESS, MAX_LIGHTNESS)
}

/// Rounds to the nearest half, the finest Colourfulness step.
fn round_half(value: f64) -> f64 {
    (value * 2.0).round() / 2.0
}

fn clamp_contrast(value: f64) -> f64 {
    round_half(value).clamp(CONTRAST_MIN_POINTS, CONTRAST_MAX_POINTS)
}

/// `getSidebarTitlebarBackgroundForDarkness`.
pub(super) fn dark_chrome(darkness: f64, tint: &str) -> Rgb {
    let darkness = clamp_darkness(darkness);
    let tint = normalize_hex(tint, DEFAULT_TINT);
    let base =
        calibrated("CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS", &tint).unwrap_or_else(|| {
            let color = parse_hex(&tint);
            let calibration = calibration(
                "CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_COLOR",
                "#040607",
            );
            if is_neutral(color) {
                return calibration;
            }
            let direction = tint_direction(color);
            Rgb([
                calibration.0[0] + direction[0] * 4.0,
                calibration.0[1] + direction[1] * 4.0,
                calibration.0[2] + direction[2] * 4.0,
            ])
        });
    if darkness == MAX_DARKNESS {
        return Rgb([0.0; 3]);
    }
    let scale = (MAX_DARKNESS - darkness) / (MAX_DARKNESS - DARKNESS_SCALE_REFERENCE);
    Rgb(base.0.map(|value| channel(value * scale)))
}

/// `dark_chrome` faded `fade` of the way toward the tint itself (`sidebar_titlebar_background_toward_full`).
pub(super) fn dark_chrome_full(darkness: f64, tint: &str, fade: f64) -> Rgb {
    let vivid = dark_chrome(darkness, tint);
    if fade <= 0.0 {
        return vivid;
    }
    let picked = parse_hex(&normalize_hex(tint, DEFAULT_TINT));
    Rgb(dark_chrome_toward_full(vivid.0, picked.0, fade).map(channel))
}

/// `getSidebarTitlebarLightBackgroundForLightness`.
pub(super) fn light_chrome(lightness: f64, tint: &str) -> Rgb {
    let lightness = clamp_lightness(lightness);
    if lightness == MAX_LIGHTNESS {
        return Rgb([255.0; 3]);
    }
    let tint = normalize_hex(tint, DEFAULT_TINT);
    let base =
        calibrated("CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS", &tint).unwrap_or_else(|| {
            let color = parse_hex(&tint);
            let calibration = calibration(
                "CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_COLOR",
                "#f1f1f2",
            );
            if is_neutral(color) {
                return calibration;
            }
            let direction = tint_direction(color);
            Rgb([
                calibration.0[0] + direction[0] * 6.0,
                calibration.0[1] + direction[1] * 6.0,
                calibration.0[2] + direction[2] * 6.0,
            ])
        });
    let scale = (MAX_LIGHTNESS - lightness) / (MAX_LIGHTNESS - LIGHTNESS_SCALE_REFERENCE);
    Rgb(base.0.map(|value| channel(255.0 - (255.0 - value) * scale)))
}

/// `accentColorForTint`: the tint's hue at a fixed lightness (0.75 dark, 0.38 light), saturation
/// held in 0.55-0.9; a neutral tint paints `#86d3f8` (dark) or `#262626` (light).
pub(super) fn accent_for_tint(tint: &str, light: bool) -> Rgb {
    let color = parse_hex(&normalize_hex(tint, DEFAULT_TINT));
    if is_neutral(color) {
        return parse_hex(if light { "#262626" } else { "#86d3f8" });
    }
    let [red, green, blue] = color.0.map(|value| value / 255.0);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let chroma = max - min;
    let tint_lightness = (max + min) / 2.0;
    let tint_saturation = chroma / (1.0 - (2.0 * tint_lightness - 1.0).abs());
    let hue = if max == red {
        (((green - blue) / chroma) % 6.0 + 6.0) % 6.0
    } else if max == green {
        (blue - red) / chroma + 2.0
    } else {
        (red - green) / chroma + 4.0
    };
    let lightness = if light { 0.38 } else { 0.75 };
    let saturation = tint_saturation.clamp(0.55, 0.9);
    let accent_chroma = (1.0 - (2.0 * lightness - 1.0_f64).abs()) * saturation;
    let secondary = accent_chroma * (1.0 - ((hue % 2.0) - 1.0).abs());
    let offset = lightness - accent_chroma / 2.0;
    let [r, g, b] = if hue < 1.0 {
        [accent_chroma, secondary, 0.0]
    } else if hue < 2.0 {
        [secondary, accent_chroma, 0.0]
    } else if hue < 3.0 {
        [0.0, accent_chroma, secondary]
    } else if hue < 4.0 {
        [0.0, secondary, accent_chroma]
    } else if hue < 5.0 {
        [secondary, 0.0, accent_chroma]
    } else {
        [accent_chroma, 0.0, secondary]
    };
    Rgb([
        channel((r + offset) * 255.0),
        channel((g + offset) * 255.0),
        channel((b + offset) * 255.0),
    ])
}

fn luminance(color: Rgb) -> f64 {
    (0.2126 * color.0[0] + 0.7152 * color.0[1] + 0.0722 * color.0[2]) / 255.0
}

/// `getSidebarTitlebarForegroundForBackground`.
pub(super) fn foreground_for(background: Rgb) -> Rgb {
    if luminance(background) > 0.54 {
        parse_hex("#262626")
    } else {
        parse_hex("#d8d8d8")
    }
}

/// `getSessionChatBackgroundForChrome`: 1% toward white on dark chrome, 25% on light.
fn chat_background_for_chrome(chrome: Rgb) -> Rgb {
    let amount = if luminance(chrome) > 0.54 { 0.25 } else { 0.01 };
    Rgb(chrome
        .0
        .map(|value| channel(value + (255.0 - value) * amount)))
}

/// One preset's controls (`DARK_THEME_PRESET_CONTROLS[preset]` / `LIGHT_THEME_PRESET_CONTROLS`).
pub(super) fn preset_controls(dark: bool, preset: &str) -> Option<(f64, String)> {
    let table = if dark {
        "DARK_THEME_PRESET_CONTROLS"
    } else {
        "LIGHT_THEME_PRESET_CONTROLS"
    };
    let entry = settings_catalog()
        .module_value(module::SETTINGS, table)?
        .get(preset)?;
    let percent = entry
        .get(if dark {
            "darknessPercent"
        } else {
            "lightnessPercent"
        })?
        .as_f64()?;
    let tint = entry.get("tintColor")?.as_str()?.to_string();
    Some((percent, tint))
}

/// The theme colours offered in each appearance, in order, as `(value, label)`, without Custom.
pub(super) fn preset_options(dark: bool) -> Vec<(String, String)> {
    let name = if dark {
        "DARK_THEME_PRESET_OPTIONS"
    } else {
        "LIGHT_THEME_PRESET_OPTIONS"
    };
    settings_catalog()
        .options(module::SETTINGS, name)
        .into_iter()
        .filter(|option| option.value != "custom")
        .map(|option| (option.value, option.label))
        .collect()
}

/// `themePresetLabel`.
pub(super) fn preset_label(dark: bool, preset: &str) -> String {
    let name = if dark {
        "DARK_THEME_PRESET_OPTIONS"
    } else {
        "LIGHT_THEME_PRESET_OPTIONS"
    };
    settings_catalog()
        .options(module::SETTINGS, name)
        .into_iter()
        .find(|option| option.value == preset)
        .map(|option| option.label)
        .unwrap_or_else(|| preset.to_string())
}

/// `presetChromeAtStep`: a preset's chrome at some Colourfulness contrast points.
fn preset_chrome_at_points(dark: bool, preset: &str, points: f64) -> Rgb {
    let (percent, tint) = preset_controls(dark, preset).unwrap_or((96.0, DEFAULT_TINT.to_string()));
    if dark {
        dark_chrome_full(
            clamp_darkness(percent + clamp_contrast(points)),
            &tint,
            full_colour_fade(points),
        )
    } else {
        light_chrome(clamp_lightness(percent + clamp_contrast(points)), &tint)
    }
}

fn mix(from: Rgb, to: Rgb, amount: f64) -> Rgb {
    Rgb([0, 1, 2].map(|index| (from.0[index] + (to.0[index] - from.0[index]) * amount).round()))
}

/// The gradient stops of one colour square (`swatchStyle`): top, base, accent, glow and rim alpha.
pub(super) struct SwatchPaint {
    pub(super) top: Rgb,
    pub(super) base: Rgb,
    pub(super) accent: Rgb,
    pub(super) glow: f64,
    pub(super) rim: f64,
}

/// `NEUTRAL_SWATCH_ACCENT`: themes without a hue take a grey sheen.
fn neutral_accent(preset: &str, dark: bool) -> Option<Rgb> {
    let hex = match preset {
        "gray" => {
            if dark {
                "#a3a3a8"
            } else {
                "#6b6b70"
            }
        }
        "black" | "white" => {
            if dark {
                "#4a4a4f"
            } else {
                "#c8c8cc"
            }
        }
        _ => return None,
    };
    Some(parse_hex(hex))
}

/// `swatchStyle(scheme, preset, tint, step)`.
///
/// CDXC:Theming 2026-10-16 DECISION:
/// User: make the colour squares "smaller and simpler" and keep today's ring. Supersedes the 2026-09-25 gradient squares: each square is 20px and one flat colour, the average of this paint without its sheen (swatches.rs), so it keeps the hue the gradient gave it and still follows Colourfulness.
///
/// `position` is a Colourfulness position; `step` runs 0 (Subtle) to 4 (Vivid) through the in-between positions, so the five named points paint exactly as the five steps did, and stays 4 past Vivid, where only the chrome fades toward Full.
pub(super) fn swatch_paint(dark: bool, preset: &str, position: usize) -> SwatchPaint {
    let points = colourfulness_points(position);
    let step = ((CONTRAST_MAX_POINTS - points) / 4.0).min(4.0);
    let tint = preset_controls(dark, preset)
        .map(|(_, tint)| tint)
        .unwrap_or_else(|| DEFAULT_TINT.to_string());
    let accent = neutral_accent(preset, dark).unwrap_or_else(|| accent_for_tint(&tint, !dark));
    let top_chrome = preset_chrome_at_points(
        dark,
        preset,
        // Past Vivid the top follows the square's own colour toward Full.
        if points < CONTRAST_MIN_POINTS {
            points
        } else {
            (points - 8.0).max(CONTRAST_MIN_POINTS)
        },
    );
    let top = if dark {
        mix(top_chrome, accent, 0.34 + step * 0.03)
    } else {
        top_chrome
    };
    let base = if dark {
        mix(preset_chrome_at_points(dark, preset, points), accent, 0.1)
    } else {
        preset_chrome_at_points(dark, preset, points)
    };
    // `alphaHex`: the alpha as a byte, as the CSS colour carried it.
    let alpha = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() / 255.0;
    SwatchPaint {
        top,
        base,
        accent,
        glow: alpha(0.26 + step * 0.07),
        rim: alpha(0.1 + step * 0.03),
    }
}

/// `readThemeContrastPoints`: a saved contrast, or the retired five-step `themeContrast`.
pub(super) fn contrast_points(values: &SettingsValues, key: &str) -> f64 {
    if let Some(points) = values
        .map()
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
    {
        return clamp_contrast(points);
    }
    if values.map().get(key).is_none()
        && let Some(legacy) = values
            .map()
            .get("themeContrast")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
    {
        return match legacy.round().clamp(-2.0, 2.0) as i64 {
            -2 => -8.0,
            -1 => -4.0,
            1 => 2.0,
            2 => 4.0,
            _ => 0.0,
        };
    }
    values.f64(key)
}

/// `resolveDarkChromeControls` / `resolveLightChromeControls`: the preset shifted by the sidebar
/// contrast, or the Custom controls.
fn chrome_controls(values: &SettingsValues, dark: bool) -> (f64, String) {
    let preset_key = if dark {
        "darkThemePreset"
    } else {
        "lightThemePreset"
    };
    let preset = values.string(preset_key);
    if preset != "custom"
        && let Some((percent, tint)) = preset_controls(dark, &preset)
    {
        let points = contrast_points(values, "themeSidebarContrast");
        let percent = if dark {
            clamp_darkness(percent + points)
        } else {
            clamp_lightness(percent + points)
        };
        return (percent, tint);
    }
    if dark {
        (
            clamp_darkness(values.f64("customSidebarTitlebarBackgroundDarknessPercent")),
            normalize_hex(
                &values.string("customSidebarTitlebarBackgroundTintColor"),
                DEFAULT_TINT,
            ),
        )
    } else {
        (
            clamp_lightness(
                values.f64("customSidebarTitlebarLightBackgroundLightnessPercent")
                    + CUSTOM_LIGHT_THEME_DEPTH_SHIFT,
            ),
            normalize_hex(
                &values.string("customSidebarTitlebarLightBackgroundTintColor"),
                DEFAULT_TINT,
            ),
        )
    }
}

/// `ColourfulnessPreview`: the sidebar chrome and the work area colour of one appearance, and the
/// foreground its divider is drawn with.
pub(super) fn preview_colours(values: &SettingsValues, dark: bool) -> (Rgb, Rgb, Rgb) {
    let (percent, tint) = chrome_controls(values, dark);
    let chrome = if dark {
        dark_chrome_full(
            percent,
            &tint,
            full_colour_fade(values.f64("themeSidebarContrast")),
        )
    } else {
        light_chrome(percent, &tint)
    };
    // `getWorkAreaBackgroundForSettings`.
    let delta = contrast_points(values, "themeWorkAreaContrast")
        - contrast_points(values, "themeSidebarContrast");
    let work_chrome = if dark {
        dark_chrome_full(
            clamp_darkness(percent + delta),
            &tint,
            full_colour_fade(values.f64("themeWorkAreaContrast")),
        )
    } else {
        light_chrome(clamp_lightness(percent + delta), &tint)
    };
    (
        chrome,
        chat_background_for_chrome(work_chrome),
        foreground_for(chrome),
    )
}

/// `colourfulnessStepForPoints`: the Colourfulness position that sets exactly `points`, if any.
pub(super) fn colourfulness_step_for_points(points: f64) -> Option<usize> {
    let position = (CONTRAST_MAX_POINTS - points) / COLOURFULNESS_STEP_POINTS;
    (position.fract() == 0.0 && (0.0..=COLOURFULNESS_LAST_POSITION as f64).contains(&position))
        .then_some(position as usize)
}

/// `colourfulnessDisplayStep`: the Colourfulness position nearest to `points`.
pub(super) fn colourfulness_display_step(points: f64) -> usize {
    if !points.is_finite() {
        return colourfulness_display_step(0.0);
    }
    ((CONTRAST_MAX_POINTS - points) / COLOURFULNESS_STEP_POINTS)
        .round()
        .clamp(0.0, COLOURFULNESS_LAST_POSITION as f64) as usize
}

/// `colourfulnessStepIndex`: the one position both areas share, or `None` once they are apart.
pub(super) fn colourfulness_step_index(values: &SettingsValues) -> Option<usize> {
    let sidebar = values.f64("themeSidebarContrast");
    if sidebar != values.f64("themeWorkAreaContrast") {
        return None;
    }
    colourfulness_step_for_points(sidebar)
}

/// A whole number is written as a JSON integer, the way `JSON.stringify` writes it.
pub(super) fn js_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

/// `colourfulnessPatch(step)`: both contrasts, and the Custom colours' contrast moved in step.
pub(super) fn colourfulness_patch(position: usize) -> Map<String, Value> {
    let points = colourfulness_points(position);
    let catalog = settings_catalog();
    let dark_default = catalog.number(
        module::SETTINGS,
        "DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT",
    );
    let light_default = catalog.number(
        module::SETTINGS,
        "DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT",
    );
    let mut patch = Map::new();
    patch.insert("themeSidebarContrast".into(), js_number(points));
    patch.insert("themeWorkAreaContrast".into(), js_number(points));
    patch.insert(
        "customSidebarTitlebarBackgroundDarknessPercent".into(),
        js_number(clamp_darkness(dark_default + clamp_contrast(points))),
    );
    patch.insert(
        "customSidebarTitlebarLightBackgroundLightnessPercent".into(),
        js_number(clamp_lightness(light_default + clamp_contrast(points))),
    );
    patch
}

/// `TRANSPARENCY_STRENGTH_*` and the anchor the strength-to-tint curve is drawn from: strength 20 is
/// 88/93, and the shipped default is strength 10 (94 dark and 97 light for both areas in the settings catalog).
pub(super) const STRENGTH_MIN: f64 = 0.0;
pub(super) const STRENGTH_MAX: f64 = 100.0;
pub(super) const STRENGTH_STEP: f64 = 5.0;
const STRENGTH_DEFAULT: f64 = 20.0;
const SIDEBAR_AT_DEFAULT_DARK: f64 = 88.0;
const SIDEBAR_AT_DEFAULT_LIGHT: f64 = 93.0;

fn strength_sidebar_tint(strength: f64, at_default: f64) -> f64 {
    if strength <= STRENGTH_DEFAULT {
        return 100.0 - ((100.0 - at_default) * strength) / STRENGTH_DEFAULT;
    }
    (at_default * (STRENGTH_MAX - strength)) / (STRENGTH_MAX - STRENGTH_DEFAULT)
}

/// How much more solid the work area is than the sidebar (work area tint minus sidebar tint) in
/// dark and in light mode: 0 unless the tints were set apart by hand.
pub(super) fn strength_gaps(values: &SettingsValues) -> (f64, f64) {
    (
        values.f64("windowGlassWorkAreaTintDark") - values.f64("windowGlassSidebarOpacityDark"),
        values.f64("windowGlassWorkAreaTintLight") - values.f64("windowGlassSidebarOpacityLight"),
    )
}

/// `transparencyStrengthPatch`: the four glass tints one strength sets, keeping each appearance's
/// work-area gap (`strength_gaps`).
///
/// CDXC:Theming 2026-10-23 DECISION:
/// User: "add transparency strength selection" to the setup and the Theme page, then "make it a slider with more options" and "Full range"; then "pls make the sidebar and main area default to having the same strength for the transparency by default" and "but look if the user makes one darker than other and we just adjust the strength then we need to keep the difference between the 2 ok?". Strength is one 0-100 slider in steps of 5 (0 fully solid, 100 fully clear) that sets the sidebar's tint from its curve and gives the work area the same tint plus the difference the user set between them by hand under More transparency options, in dark and light mode separately; out of the box there is no difference, so both match. Each tint is clamped to 0-100, so near the ends the difference narrows as far as it has to (and stays narrowed). This supersedes the 2026-09-27 rule that kept the work area up to 7 points more see-through than the sidebar ("it needs to make the sidebar darker than main", "make it 7 point difference").
pub(super) fn strength_patch(strength: f64, gaps: (f64, f64)) -> Map<String, Value> {
    let value = strength.clamp(STRENGTH_MIN, STRENGTH_MAX);
    let tints = |at_default: f64, gap: f64| {
        let sidebar = strength_sidebar_tint(value, at_default).round();
        (sidebar, (sidebar + gap).round().clamp(0.0, 100.0))
    };
    let (dark_sidebar, dark_work) = tints(SIDEBAR_AT_DEFAULT_DARK, gaps.0);
    let (light_sidebar, light_work) = tints(SIDEBAR_AT_DEFAULT_LIGHT, gaps.1);
    let mut patch = Map::new();
    patch.insert(
        "windowGlassSidebarOpacityDark".into(),
        js_number(dark_sidebar),
    );
    patch.insert("windowGlassWorkAreaTintDark".into(), js_number(dark_work));
    patch.insert(
        "windowGlassSidebarOpacityLight".into(),
        js_number(light_sidebar),
    );
    patch.insert("windowGlassWorkAreaTintLight".into(), js_number(light_work));
    patch
}

/// `transparencyStrengthFromSettings`: the exact strength the tints came from (or `None` once a
/// sidebar tint was tuned off the curve by hand; a hand-set work-area gap still counts as exact) and
/// where the slider sits.
pub(super) fn strength_from_settings(values: &SettingsValues) -> (Option<f64>, f64) {
    let sidebar = values.f64("windowGlassSidebarOpacityDark");
    let raw = if sidebar >= SIDEBAR_AT_DEFAULT_DARK {
        ((100.0 - sidebar) * STRENGTH_DEFAULT) / (100.0 - SIDEBAR_AT_DEFAULT_DARK)
    } else {
        STRENGTH_MAX - (sidebar * (STRENGTH_MAX - STRENGTH_DEFAULT)) / SIDEBAR_AT_DEFAULT_DARK
    };
    let nearest = ((raw / STRENGTH_STEP).round() * STRENGTH_STEP).clamp(STRENGTH_MIN, STRENGTH_MAX);
    let patch = strength_patch(nearest, strength_gaps(values));
    let matches = |key: &str| patch.get(key).and_then(Value::as_f64) == Some(values.f64(key));
    let exact = [
        "windowGlassSidebarOpacityDark",
        "windowGlassWorkAreaTintDark",
        "windowGlassSidebarOpacityLight",
        "windowGlassWorkAreaTintLight",
    ]
    .iter()
    .all(|key| matches(key))
    .then_some(nearest);
    (exact, nearest)
}

/// `LIGHT_PRESET_FOR_DARK` / `DARK_PRESET_FOR_LIGHT`: the matching colour in the other appearance
/// (Black and White swap; every other colour keeps its id).
pub(super) fn paired_preset(from_dark: bool, preset: &str) -> Option<String> {
    Some(match (from_dark, preset) {
        (_, "custom") => return None,
        (true, "black") => "white".to_string(),
        (false, "white") => "black".to_string(),
        (_, other) => other.to_string(),
    })
}

/// `#rrggbb` of a colour (for the swatch cache key).
pub(super) fn hex_of(color: Rgb) -> String {
    color.hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINT_KEYS: [&str; 4] = [
        "windowGlassSidebarOpacityDark",
        "windowGlassWorkAreaTintDark",
        "windowGlassSidebarOpacityLight",
        "windowGlassWorkAreaTintLight",
    ];

    fn tints(patch: &Map<String, Value>) -> [f64; 4] {
        TINT_KEYS.map(|key| patch[key].as_f64().unwrap())
    }

    fn values_of(tints: [f64; 4]) -> SettingsValues {
        let mut map = Map::new();
        for (key, tint) in TINT_KEYS.iter().zip(tints) {
            map.insert((*key).into(), js_number(tint));
        }
        SettingsValues::from_map(map)
    }

    #[test]
    fn equal_tints_stay_equal_at_every_strength() {
        let mut strength = STRENGTH_MIN;
        while strength <= STRENGTH_MAX {
            let [dark_sidebar, dark_work, light_sidebar, light_work] =
                tints(&strength_patch(strength, (0.0, 0.0)));
            assert_eq!(dark_sidebar, dark_work);
            assert_eq!(light_sidebar, light_work);
            strength += STRENGTH_STEP;
        }
        assert_eq!(tints(&strength_patch(STRENGTH_MIN, (0.0, 0.0))), [100.0; 4]);
        assert_eq!(tints(&strength_patch(STRENGTH_MAX, (0.0, 0.0))), [0.0; 4]);
    }

    #[test]
    fn a_hand_set_gap_moves_with_the_slider() {
        // Dark: work area 5 more see-through; light: work area 2 more solid.
        let values = values_of([94.0, 89.0, 92.0, 94.0]);
        assert_eq!(strength_gaps(&values), (-5.0, 2.0));
        assert_eq!(
            tints(&strength_patch(40.0, strength_gaps(&values))),
            [66.0, 61.0, 70.0, 72.0]
        );
        // The slider reads the sidebar's stop, and a hand-set gap on a stop still counts as exact.
        let on_stop = values_of([94.0, 89.0, 97.0, 99.0]);
        assert_eq!(strength_from_settings(&on_stop), (Some(10.0), 10.0));
    }

    #[test]
    fn the_ends_clamp_each_tint() {
        let gaps = (-5.0, 3.0);
        assert_eq!(
            tints(&strength_patch(STRENGTH_MIN, gaps)),
            [100.0, 95.0, 100.0, 100.0]
        );
        assert_eq!(
            tints(&strength_patch(STRENGTH_MAX, gaps)),
            [0.0, 0.0, 0.0, 3.0]
        );
    }

    fn dark_values(preset: &str, tint: &str, position: usize) -> SettingsValues {
        let mut map = colourfulness_patch(position);
        map.insert("darkThemePreset".into(), json!(preset));
        map.insert(
            "customSidebarTitlebarBackgroundTintColor".into(),
            json!(tint),
        );
        SettingsValues::from_map(map)
    }

    /// The preview draws what the window draws (the same values `full_colour_tests` in
    /// helpers/titlebar/chrome_tints.rs pins for the window).
    #[test]
    fn the_preview_matches_the_window_from_soft_to_full() {
        let sidebar = |preset: &str, tint: &str, position: usize| {
            preview_colours(&dark_values(preset, tint, position), true)
                .0
                .hex()
        };
        assert_eq!(sidebar("gray", "#808080", 8), "#0b0b0b");
        assert_eq!(sidebar("custom", "#273136", 32), "#00141f");
        assert_eq!(
            sidebar("custom", "#273136", COLOURFULNESS_LAST_POSITION),
            "#273136"
        );
        assert_eq!(
            sidebar("black", "#808080", COLOURFULNESS_LAST_POSITION),
            "#000000"
        );
        // The work area is the same chrome, one step toward white.
        let full = preview_colours(
            &dark_values("custom", "#273136", COLOURFULNESS_LAST_POSITION),
            true,
        );
        assert_eq!(full.1.hex(), "#293338");
    }

    #[test]
    fn past_vivid_only_dark_mode_moves() {
        let vivid = colourfulness_patch(32);
        let full = colourfulness_patch(COLOURFULNESS_LAST_POSITION);
        assert_eq!(full["themeSidebarContrast"], json!(-24));
        for key in [
            "customSidebarTitlebarBackgroundDarknessPercent",
            "customSidebarTitlebarLightBackgroundLightnessPercent",
        ] {
            assert_eq!(full[key], vivid[key], "{key}");
        }
        let light = |position| {
            preview_colours(&dark_values("custom", "#273136", position), false)
                .0
                .hex()
        };
        assert_eq!(light(32), light(COLOURFULNESS_LAST_POSITION));
        assert_eq!(colourfulness_name(colourfulness_points(32)), "Vivid");
        assert_eq!(
            colourfulness_name(colourfulness_points(COLOURFULNESS_LAST_POSITION)),
            "Full"
        );
    }

    #[test]
    fn catalog_defaults_are_strength_ten() {
        let values = SettingsValues::from_map(Map::new());
        let patch = strength_patch(10.0, strength_gaps(&values));
        for key in TINT_KEYS {
            let default = ghostex_settings_catalog::default_value(key)
                .and_then(ghostex_settings_catalog::J::as_f64);
            assert_eq!(patch[key].as_f64(), default, "{key}");
        }
        assert_eq!(strength_from_settings(&values), (Some(10.0), 10.0));
    }
}
