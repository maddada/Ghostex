//! What a look change can make hard to read, said in the command's output and on the "Theme
//! changed" card. The change is saved either way.
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3117, agent theming page): when an agent picks colours that are hard to read, "warn on unreadable colours (don't refuse)": the change applies, and `ghostex theme set` and its card say what is hard to read and why.
//!
//! CDXC:Theming 2026-11-14 WHY:
//! The colours themselves cannot make text unreadable: the window's colours keep the sidebar's and work area's text readable at every colour and Colourfulness (packages/settings-catalog/src/dark_chrome.rs caps how light a dark chrome and how dark a light chrome may get). What an agent can make hard to read is the glass, where a low tint leaves text over whatever the desktop shows, and the active pane outline, whose colour is free. The work area the outline sits on is estimated from the same inputs, since its exact colour is the desktop's (`helpers/titlebar/chrome_tints.rs`): the Colourfulness fade toward the picked colour moves it far more than the depth does.

use ghostex_settings_catalog::dark_chrome::{
    dark_chrome_toward_full, full_colour_fade, light_chrome_toward_full, relative_luminance,
};
use serde_json::{Map, Value};

use super::look::current;

/// Below this tint (how much of the desktop an area hides), text on the glass sits mostly on the
/// desktop. The defaults are 94 (dark) and 97 (light).
const LOW_GLASS_TINT: f64 = 70.0;

/// The contrast an outline needs against what it sits on to read clearly (WCAG's 3:1 for parts of
/// the interface).
const OUTLINE_MIN_CONTRAST: f64 = 3.0;

/// One thing a look makes hard to read: the sentence, and the settings it comes from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Warning {
    pub(crate) text: String,
    pub(crate) keys: Vec<&'static str>,
}

fn number(settings: &Map<String, Value>, key: &str) -> Option<f64> {
    current(settings, key).as_f64()
}

fn text(settings: &Map<String, Value>, key: &str) -> String {
    current(settings, key)
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// The appearances the app shows with this Appearance setting: `(light, word)`.
fn appearances(settings: &Map<String, Value>) -> Vec<(bool, &'static str)> {
    match text(settings, "sidebarTheme").as_str() {
        "plain-light" => vec![(true, "light mode")],
        "system" => vec![(false, "dark mode"), (true, "light mode")],
        _ => vec![(false, "dark mode")],
    }
}

fn parse_hex(value: &str) -> Option<[f64; 3]> {
    let digits = value.trim().trim_start_matches('#');
    if digits.len() != 6 {
        return None;
    }
    let rgb = u32::from_str_radix(digits, 16).ok()?;
    Some([
        f64::from((rgb >> 16) & 0xff),
        f64::from((rgb >> 8) & 0xff),
        f64::from(rgb & 0xff),
    ])
}

/// The colour an appearance's theme square or custom colour picks.
fn picked_colour(settings: &Map<String, Value>, light: bool) -> Option<[f64; 3]> {
    let (preset_key, custom_key, table) = if light {
        (
            "lightThemePreset",
            "customSidebarTitlebarLightBackgroundTintColor",
            "LIGHT_THEME_PRESET_CONTROLS",
        )
    } else {
        (
            "darkThemePreset",
            "customSidebarTitlebarBackgroundTintColor",
            "DARK_THEME_PRESET_CONTROLS",
        )
    };
    let preset = text(settings, preset_key);
    if preset == "custom" {
        return parse_hex(&text(settings, custom_key));
    }
    let controls = ghostex_settings_catalog::modules::exports(
        ghostex_settings_catalog::modules::module::SETTINGS,
        ghostex_settings_catalog::Platform::current(),
    )
    .into_iter()
    .find(|(name, _)| *name == table)?
    .1
    .to_value();
    parse_hex(controls.get(&preset)?.get("tintColor")?.as_str()?)
}

/// About the colour of the work area (chat, terminals) in one appearance.
fn work_area(settings: &Map<String, Value>, light: bool) -> [f64; 3] {
    // The neutral work areas: #0d0d0d dark, #f7f7f7 light. A colour square from Subtle to Vivid
    // only tints them a few steps.
    let neutral = if light { [247.0; 3] } else { [13.0; 3] };
    let fade = number(settings, "themeWorkAreaContrast").map_or(0.0, full_colour_fade);
    let Some(picked) = picked_colour(settings, light).filter(|_| fade > 0.0) else {
        return neutral;
    };
    // The chrome fades toward the picked colour, then the work area steps toward white off it
    // (1% dark, 25% light).
    let (chrome, step) = if light {
        (
            light_chrome_toward_full([241.0, 241.0, 242.0], picked, fade),
            0.25,
        )
    } else {
        (dark_chrome_toward_full([14.0; 3], picked, fade), 0.01)
    };
    chrome.map(|channel| channel + (255.0 - channel) * step)
}

fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (relative_luminance(a), relative_luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn glass_on(settings: &Map<String, Value>, light: bool) -> bool {
    match text(settings, "windowGlass").as_str() {
        "frosted" => true,
        "auto" => !light,
        _ => false,
    }
}

/// Everything `settings` makes hard to read, in the order Settings shows the settings.
pub(crate) fn warnings(settings: &Map<String, Value>) -> Vec<Warning> {
    let mut found = Vec::new();
    let same_glass = current(settings, "windowGlassSameForLightAndDark") != Value::Bool(false);
    // The tints each shown appearance uses, with the appearances that use it: light mode uses the
    // dark-mode tints while Same for light and dark is on, and then one sentence names both.
    let mut tints: Vec<(&'static str, &str, Vec<&str>)> = Vec::new();
    for (light, mode) in appearances(settings) {
        if !glass_on(settings, light) {
            continue;
        }
        let keys: [(&'static str, &str); 2] = if light && !same_glass {
            [
                ("windowGlassSidebarOpacityLight", "sidebar"),
                ("windowGlassWorkAreaTintLight", "work area"),
            ]
        } else {
            [
                ("windowGlassSidebarOpacityDark", "sidebar"),
                ("windowGlassWorkAreaTintDark", "work area"),
            ]
        };
        for (key, area) in keys {
            match tints.iter_mut().find(|(known, ..)| *known == key) {
                Some((.., modes)) => modes.push(mode),
                None => tints.push((key, area, vec![mode])),
            }
        }
    }
    for (key, area, modes) in tints {
        let Some(tint) = number(settings, key) else {
            continue;
        };
        if tint < LOW_GLASS_TINT {
            let modes = modes.join(" and ").replace(" mode and ", " and ");
            found.push(Warning {
                text: format!(
                    "The {area} tint in {modes} is {}%, so its text sits mostly on your desktop and may be hard to read over a bright or busy background. {}% or more keeps it readable.",
                    crate::ghostex_cli::settings::format_number(tint),
                    LOW_GLASS_TINT as i64
                ),
                keys: vec![key, "windowGlass", "sidebarTheme"],
            });
        }
    }
    if current(settings, "showActivePaneOutline") == Value::Bool(true) {
        let colour = text(settings, "workspaceActivePaneBorderColor");
        if let Some(outline) = parse_hex(&colour) {
            for (light, mode) in appearances(settings) {
                let ratio = contrast(outline, work_area(settings, light));
                if ratio < OUTLINE_MIN_CONTRAST {
                    found.push(Warning {
                        text: format!(
                            "The active pane outline ({}) is hard to see on the work area in {mode}: {ratio:.1}:1, where 3:1 reads clearly.",
                            colour.trim()
                        ),
                        keys: vec![
                            "workspaceActivePaneBorderColor",
                            "showActivePaneOutline",
                            "sidebarTheme",
                            "darkThemePreset",
                            "lightThemePreset",
                            "customSidebarTitlebarBackgroundTintColor",
                            "customSidebarTitlebarLightBackgroundTintColor",
                            "themeWorkAreaContrast",
                        ],
                    });
                }
            }
        }
    }
    found
}

/// What a change made hard to read: the warnings of the look after it (`after`) that come from a
/// setting it changed and that the look before it (`before`) did not have.
pub(crate) fn warnings_from(
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    changed: &[String],
) -> Vec<Warning> {
    let earlier = warnings(before);
    warnings(after)
        .into_iter()
        .filter(|warning| {
            warning
                .keys
                .iter()
                .any(|key| changed.iter().any(|changed| changed == key))
                && !earlier.contains(warning)
        })
        .collect()
}
