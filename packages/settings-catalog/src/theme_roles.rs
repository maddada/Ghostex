//! Theme roles: named colours of the app's look (borders, dividers, grey text, the accent, …) that
//! override what the window's palettes derive, set per appearance in one setting, `themeRoles`:
//! `{ "dark": { "border": "4%", … }, "light": { … } }`. A role that is not set keeps today's colour
//! exactly.
//!
//! A role's value is the ink at an opacity (`"6%"`: white in dark mode, black in light mode, laid
//! over whatever is under it), or a colour (`"#7dd3fc"`, or `"#7dd3fc80"` with its opacity).
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3113 B and Q3195, agent theming page): after the look settings, agents restyle Ghostex through about sixteen named theme roles, "border, hairline, muted text, accent, surfaces, menu", that override what the palettes derive, per appearance, in one synced setting, so "the white borders feel too bright, tone them down" works. Borders, dividers and grey text are washes of the ink by default, so on window glass they stay see-through and on a solid fill they read the same: a role set as an opacity keeps that, and one set as a colour paints that colour on both.
//! SEE-ALSO: apps/desktop/src/app/helpers/theme_roles.rs (the desktop applies them), server/src/theme/ (`ghostex theme`), skills/ghostex-help/SKILL.md.

use serde_json::{Map, Value};

/// The setting that holds every role.
pub const THEME_ROLES_SETTING: &str = "themeRoles";

/// One theme role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeRole {
    pub id: &'static str,
    pub title: &'static str,
    pub about: &'static str,
}

const fn role(id: &'static str, title: &'static str, about: &'static str) -> ThemeRole {
    ThemeRole { id, title, about }
}

/// Every role, in the order `ghostex theme read` and Settings list them.
pub const THEME_ROLES: &[ThemeRole] = &[
    role(
        "border",
        "Borders",
        "Outlines of cards, boxes, inputs, the chat box and your messages.",
    ),
    role(
        "divider",
        "Dividers",
        "Lines between areas: the sidebar's edge, under the header, between sections and rows.",
    ),
    role(
        "text",
        "Text",
        "Main text: messages, titles and the sidebar's session names.",
    ),
    role(
        "mutedText",
        "Grey text",
        "Secondary text: times, hints, labels and descriptions.",
    ),
    role(
        "accent",
        "Accent",
        "Links, switches and sliders that are on, and other marks of the theme colour.",
    ),
    role("surface", "Cards", "The fill of cards and panels."),
    role(
        "control",
        "Controls",
        "The fill of inputs, buttons and other controls.",
    ),
    role("menu", "Menus", "The fill of menus and popovers."),
    role(
        "selection",
        "Selected row",
        "The fill of the selected session or list row.",
    ),
    role(
        "hover",
        "Hover",
        "The fill of a row or button under the pointer.",
    ),
    role(
        "focusRing",
        "Focus ring",
        "The ring around the input you are typing in.",
    ),
    role(
        "userMessage",
        "Your messages",
        "The fill of your messages in chat.",
    ),
    role("chatBox", "Chat box", "The fill of the box you type in."),
    role(
        "success",
        "Success",
        "Finished and passing marks, such as a tool call's check.",
    ),
    role(
        "warning",
        "Warning",
        "Warnings, such as what a theme change made hard to read.",
    ),
    role(
        "danger",
        "Danger",
        "Errors, and buttons that remove or delete.",
    ),
];

pub fn theme_role(id: &str) -> Option<&'static ThemeRole> {
    THEME_ROLES.iter().find(|role| role.id == id)
}

/// A role's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoleValue {
    /// The ink (white in dark mode, black in light mode) at this opacity, 0 to 1.
    Ink(f32),
    /// A colour, with its opacity, 0 to 1.
    Colour { rgb: [u8; 3], alpha: f32 },
}

impl RoleValue {
    /// Red, green, blue and opacity, each 0 to 1, in one appearance.
    pub fn rgba(self, light: bool) -> [f32; 4] {
        match self {
            RoleValue::Ink(alpha) => {
                let ink = if light { 0.0 } else { 1.0 };
                [ink, ink, ink, alpha]
            }
            RoleValue::Colour { rgb, alpha } => [
                f32::from(rgb[0]) / 255.0,
                f32::from(rgb[1]) / 255.0,
                f32::from(rgb[2]) / 255.0,
                alpha,
            ],
        }
    }
}

/// Reads `"6%"`, `"#rrggbb"`, `"#rgb"` or `"#rrggbbaa"`.
pub fn parse_role_value(raw: &str) -> Result<RoleValue, String> {
    let text = raw.trim();
    if let Some(percent) = text.strip_suffix('%') {
        let number: f32 = percent
            .trim()
            .parse()
            .map_err(|_| format!("\"{text}\" is not a percentage."))?;
        if !(0.0..=100.0).contains(&number) {
            return Err(format!("\"{text}\" must be from 0% to 100%."));
        }
        return Ok(RoleValue::Ink(number / 100.0));
    }
    let digits = text.strip_prefix('#').ok_or_else(|| {
        format!("\"{text}\" is not a role value: give an opacity such as 6%, or a colour such as #7dd3fc.")
    })?;
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|digit| [digit, digit]).collect(),
        6 | 8 => digits.to_string(),
        _ => {
            return Err(format!(
                "\"{text}\" is not a colour: use #rrggbb or #rrggbbaa."
            ))
        }
    };
    let byte = |index: usize| {
        u8::from_str_radix(&expanded[index..index + 2], 16)
            .map_err(|_| format!("\"{text}\" is not a colour: use #rrggbb or #rrggbbaa."))
    };
    let rgb = [byte(0)?, byte(2)?, byte(4)?];
    let alpha = if expanded.len() == 8 {
        f32::from(byte(6)?) / 255.0
    } else {
        1.0
    };
    Ok(RoleValue::Colour { rgb, alpha })
}

/// The value in the words `ghostex theme` and Settings show it in: `6%` or `#7dd3fc`.
pub fn format_role_value(value: RoleValue) -> String {
    match value {
        RoleValue::Ink(alpha) => {
            let percent = (alpha * 1000.0).round() / 10.0;
            if percent.fract() == 0.0 {
                format!("{}%", percent as i32)
            } else {
                format!("{percent}%")
            }
        }
        RoleValue::Colour { rgb, alpha } if alpha >= 1.0 => {
            format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
        }
        RoleValue::Colour { rgb, alpha } => format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            rgb[0],
            rgb[1],
            rgb[2],
            (alpha * 255.0).round() as u8
        ),
    }
}

/// The appearance key a role is saved under.
pub fn appearance_key(light: bool) -> &'static str {
    if light {
        "light"
    } else {
        "dark"
    }
}

/// The raw value of `role` in one appearance, as saved.
pub fn saved_role<'a>(
    settings: &'a Map<String, Value>,
    light: bool,
    role: &str,
) -> Option<&'a str> {
    settings
        .get(THEME_ROLES_SETTING)?
        .get(appearance_key(light))?
        .get(role)?
        .as_str()
}

/// The role overrides of one appearance, by role id; roles not set (or unreadable) are absent.
pub fn roles_for(settings: &Map<String, Value>, light: bool) -> Vec<(&'static str, RoleValue)> {
    THEME_ROLES
        .iter()
        .filter_map(|role| {
            let raw = saved_role(settings, light, role.id)?;
            parse_role_value(raw).ok().map(|value| (role.id, value))
        })
        .collect()
}

/// Whether `value` is a `themeRoles` value a settings save may carry: `dark` and `light` objects
/// (either may be absent) whose keys are roles and whose values read as role values.
pub fn theme_roles_value_allowed(value: &Value) -> bool {
    let Some(appearances) = value.as_object() else {
        return false;
    };
    appearances.iter().all(|(appearance, roles)| {
        matches!(appearance.as_str(), "dark" | "light")
            && roles.as_object().is_some_and(|roles| {
                roles.iter().all(|(role, raw)| {
                    theme_role(role).is_some()
                        && raw
                            .as_str()
                            .is_some_and(|raw| parse_role_value(raw).is_ok())
                })
            })
    })
}
