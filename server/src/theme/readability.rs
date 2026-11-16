//! What a look change can make hard to read, said in the command's output and on the "Theme
//! changed" card. The change is saved either way.
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3117, agent theming page): when an agent picks colours that are hard to read, "warn on unreadable colours (don't refuse)": the change applies, and `ghostex theme set` and its card say what is hard to read and why.
//!
//! CDXC:Theming 2026-11-14 WHY:
//! The look settings cannot make text unreadable: the window's colours keep the sidebar's and work area's text readable at every colour and Colourfulness (packages/settings-catalog/src/dark_chrome.rs). What can is the glass, where a low tint leaves text over whatever the desktop shows, the active pane outline, whose colour is free, and the theme colours (`themeRoles`), each checked against the work area the window really paints (packages/settings-catalog/src/theme_colours.rs, shared with the desktop): text at 4.5:1, grey text and marks (the accent, the focus ring, success, warning and danger) at 3:1, the text on a fill (cards, controls, menus, selected and hovered rows, your messages, the chat box) at 4.5:1, and borders and dividers only once they all but vanish (1.05:1), since toning them down is the point.

use ghostex_settings_catalog::dark_chrome::relative_luminance;
use ghostex_settings_catalog::theme_colours::{
    resolved_custom_sidebar_titlebar_background_for_variant, work_area_background_for_variant,
};
use ghostex_settings_catalog::theme_roles::{parse_role_value, saved_role, RoleValue};
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
    pub(crate) keys: Vec<String>,
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

fn channels(rgb: u32) -> [f64; 3] {
    [
        f64::from((rgb >> 16) & 0xff),
        f64::from((rgb >> 8) & 0xff),
        f64::from(rgb & 0xff),
    ]
}

/// The colour of the work area (chat, terminals) in one appearance, as the window paints it.
fn work_area(settings: &Map<String, Value>, light: bool) -> [f64; 3] {
    channels(work_area_background_for_variant(settings, light))
}

/// The colour of the sidebar in one appearance.
fn sidebar(settings: &Map<String, Value>, light: bool) -> [f64; 3] {
    channels(resolved_custom_sidebar_titlebar_background_for_variant(
        settings, light,
    ))
}

/// A role's colour laid over `under`.
fn over(value: RoleValue, light: bool, under: [f64; 3]) -> [f64; 3] {
    let [r, g, b, a] = value.rgba(light);
    let a = f64::from(a);
    let top = [r, g, b].map(|channel| f64::from(channel) * 255.0);
    [0, 1, 2].map(|index| under[index] + (top[index] - under[index]) * a)
}

/// A role's value in one appearance, when it is set.
fn role(settings: &Map<String, Value>, light: bool, id: &str) -> Option<RoleValue> {
    saved_role(settings, light, id).and_then(|raw| parse_role_value(raw).ok())
}

/// The chat's message text when the Text role is not set (native_chat/chat_text_look.rs).
fn default_text(light: bool) -> [f64; 3] {
    if light {
        [0x4d as f64, 0x4d as f64, 0x50 as f64]
    } else {
        [0xb4 as f64, 0xb8 as f64, 0xc0 as f64]
    }
}

/// The settings that move the work area, for a role warning's keys.
const BACKGROUND_KEYS: [&str; 7] = [
    "sidebarTheme",
    "darkThemePreset",
    "lightThemePreset",
    "customSidebarTitlebarBackgroundTintColor",
    "customSidebarTitlebarLightBackgroundTintColor",
    "themeSidebarContrast",
    "themeWorkAreaContrast",
];

/// What the theme colours of one appearance make hard to read.
fn role_warnings(settings: &Map<String, Value>, light: bool, mode: &str, found: &mut Vec<Warning>) {
    let page = work_area(settings, light);
    let side = sidebar(settings, light);
    let text_on = |fill: [f64; 3]| {
        role(settings, light, "text").map_or(default_text(light), |text| over(text, light, fill))
    };
    let mut push = |id: &str, text: String, extra: Vec<String>| {
        let mut keys = vec![role_key(light, id)];
        keys.extend(extra);
        keys.extend(BACKGROUND_KEYS.iter().map(|key| (*key).to_string()));
        found.push(Warning { text, keys });
    };
    // Text and grey text, on the chat and on the sidebar. When the text itself is hard to read,
    // the fills below are not checked against it again.
    let mut text_fails = false;
    for (id, name, needed) in [("text", "Text", 4.5), ("mutedText", "Grey text", 3.0)] {
        let Some(value) = role(settings, light, id) else {
            continue;
        };
        let ratio =
            contrast(over(value, light, page), page).min(contrast(over(value, light, side), side));
        if ratio < needed {
            text_fails |= id == "text";
            push(
                id,
                format!(
                    "{name} in {mode} reads at {ratio:.1}:1, where {needed}:1 is easy to read."
                ),
                Vec::new(),
            );
        }
    }
    // Marks drawn on the chat.
    for (id, name) in [
        ("accent", "The accent"),
        ("focusRing", "The focus ring"),
        ("success", "Success"),
        ("warning", "Warning"),
        ("danger", "Danger"),
    ] {
        let Some(value) = role(settings, light, id) else {
            continue;
        };
        let ratio = contrast(over(value, light, page), page);
        if ratio < 3.0 {
            push(
                id,
                format!("{name} in {mode} is hard to see: {ratio:.1}:1, where 3:1 reads clearly."),
                Vec::new(),
            );
        }
    }
    // Lines: only once they all but vanish.
    for (id, name) in [("border", "Borders"), ("divider", "Dividers")] {
        let Some(value) = role(settings, light, id) else {
            continue;
        };
        let ratio = contrast(over(value, light, page), page);
        if ratio < 1.05 {
            push(
                id,
                format!("{name} in {mode} are almost invisible ({ratio:.2}:1)."),
                Vec::new(),
            );
        }
    }
    // Fills: the text drawn on them.
    for (id, name) in [
        ("surface", "cards"),
        ("control", "controls"),
        ("menu", "menus"),
        ("selection", "the selected row"),
        ("hover", "a hovered row"),
        ("userMessage", "your messages"),
        ("chatBox", "the chat box"),
    ] {
        let Some(value) = role(settings, light, id).filter(|_| !text_fails) else {
            continue;
        };
        let fill = over(value, light, page);
        let ratio = contrast(text_on(fill), fill);
        if ratio < 4.5 {
            push(
                id,
                format!(
                    "Text on {name} in {mode} reads at {ratio:.1}:1, where 4.5:1 is easy to read."
                ),
                vec![role_key(light, "text")],
            );
        }
    }
}

/// How a role is named in a change (`themeRoles.<dark|light>.<role>`, look.rs).
fn role_key(light: bool, id: &str) -> String {
    format!("themeRoles.{}.{id}", if light { "light" } else { "dark" })
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|key| (*key).to_string()).collect()
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
                keys: keys(&[key, "windowGlass", "sidebarTheme"]),
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
                        keys: keys(&[
                            "workspaceActivePaneBorderColor",
                            "showActivePaneOutline",
                            "sidebarTheme",
                            "darkThemePreset",
                            "lightThemePreset",
                            "customSidebarTitlebarBackgroundTintColor",
                            "customSidebarTitlebarLightBackgroundTintColor",
                            "themeWorkAreaContrast",
                        ]),
                    });
                }
            }
        }
    }
    for (light, mode) in appearances(settings) {
        role_warnings(settings, light, mode, &mut found);
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
