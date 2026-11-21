//! Theme roles: named colours of the app's look (borders, dividers, grey text, the accent, …) that
//! override what the window's palettes derive, set per appearance in one setting, `themeRoles`:
//! `{ "dark": { "border": "4%", … }, "light": { … } }`. A role that is not set keeps today's colour
//! exactly.
//!
//! A role's value is the ink at an opacity (`"6%"`: white in dark mode, black in light mode, laid
//! over whatever is under it), a colour (`"#7dd3fc"`, or `"#7dd3fc80"` with its opacity), or a step
//! off the appearance's work area base colour (`"base+4%"`, `"base-15%"`, or `"base+4%/80%"` with
//! its opacity: positive steps toward the ink, negative toward the opposite pole, so built-in
//! theme fills follow whatever window colour the user picks).
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3113 B and Q3195, agent theming page): after the look settings, agents restyle Ghostex through about sixteen named theme roles, "border, hairline, muted text, accent, surfaces, menu", that override what the palettes derive, per appearance, in one synced setting, so "the white borders feel too bright, tone them down" works. Borders, dividers and grey text are washes of the ink by default, so on window glass they stay see-through and on a solid fill they read the same: a role set as an opacity keeps that, and one set as a colour paints that colour on both.
//!
//! CDXC:Theming 2026-11-21 DECISION:
//! User (Q4390 Option B, theme-editor page): theme surface fills (`userMessage`, `chatBox`, `surface`, `control`, `menu`, `codeBlock`, `selection`) are saved relative to the window colour (`base+N%`, `base-N%`, with optional `/A%` opacity) so picking any Dark/Light colour square or Custom colour shifts every fill naturally while still allowing any custom hex colour override.
//! SEE-ALSO: apps/desktop/src/app/helpers/theme_roles.rs (the desktop applies them), server/src/theme/ (`ghostex theme`), server/src/ghostex_cli/help_guide.md.

use crate::json::ToJson;
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
///
/// CDXC:Theming 2026-11-18 DECISION:
/// User: "i think we're missing some sliders in interface to be able to adjust things manually? Ex i see chat bubble is this color but i can't change it in ui at all", then on the theme-colour controls page: Q3980 "all" (a Chat colours group led by Your messages with a See-through slider and a live preview, and every Theme colours row a colour editor: swatch, picker, opacity slider, Reset, the text box kept), Q3981 "both" (More theme colours opens itself while one of its roles is set, and the example themes name the one applied and how many colours it set) and Q3982 "links, dots and code fill". Links take the Links role, else the Accent role, else Ghostex's blue; the chat's pill dots take Warning (working), Accent (questions) and Danger (approvals) while those are set; code blocks take the new Code blocks role over the Controls fill.
/// SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/theme/roles.rs and chat_colours.rs (the Settings controls), apps/desktop/src/app/native_chat/markdown_links.rs, chat_pills.rs and markdown_style.rs (the chat).
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
    role(
        "link",
        "Links",
        "Links in chat. Empty follows the Accent, or Ghostex's own blue when the Accent is not set.",
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
        "codeBlock",
        "Code blocks",
        "The fill behind code blocks in chat. Empty keeps the Controls fill.",
    ),
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
    /// Relative to the appearance's work area base colour: `step` (-1 to 1, positive toward the
    /// ink: white in dark mode, black in light mode; negative toward the opposite pole), with its
    /// opacity `alpha` (0 to 1).
    Base { step: f32, alpha: f32 },
}

impl RoleValue {
    /// Red, green, blue and opacity, each 0 to 1, in one appearance over its neutral fallback base.
    pub fn rgba(self, light: bool) -> [f32; 4] {
        self.rgba_on_base(light, if light { 0xf7f7f7 } else { 0x0d0d0d })
    }

    /// Red, green, blue and opacity, each 0 to 1, in one appearance with work area base `base_rgb`.
    pub fn rgba_on_base(self, light: bool, base_rgb: u32) -> [f32; 4] {
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
            RoleValue::Base { step, alpha } => {
                let base = [
                    f32::from(((base_rgb >> 16) & 0xff) as u8) / 255.0,
                    f32::from(((base_rgb >> 8) & 0xff) as u8) / 255.0,
                    f32::from((base_rgb & 0xff) as u8) / 255.0,
                ];
                let (target, amount) = if step >= 0.0 {
                    (if light { 0.0 } else { 1.0 }, step.clamp(0.0, 1.0))
                } else {
                    (if light { 1.0 } else { 0.0 }, (-step).clamp(0.0, 1.0))
                };
                [
                    (base[0] + (target - base[0]) * amount).clamp(0.0, 1.0),
                    (base[1] + (target - base[1]) * amount).clamp(0.0, 1.0),
                    (base[2] + (target - base[2]) * amount).clamp(0.0, 1.0),
                    alpha.clamp(0.0, 1.0),
                ]
            }
        }
    }
}

fn parse_percent(raw: &str, label: &str) -> Result<f32, String> {
    let percent = raw
        .trim()
        .strip_suffix('%')
        .ok_or_else(|| format!("\"{label}\" is not a percentage."))?;
    let number: f32 = percent
        .trim()
        .parse()
        .map_err(|_| format!("\"{label}\" is not a percentage."))?;
    if !number.is_finite() || !(0.0..=100.0).contains(&number) {
        return Err(format!("\"{label}\" must be from 0% to 100%."));
    }
    Ok(number / 100.0)
}

fn parse_base_value(text: &str) -> Result<RoleValue, String> {
    let rest = text[4..].trim();
    let (step_part, alpha_part) = match rest.split_once('/') {
        Some((step, alpha)) => (step.trim(), Some(alpha.trim())),
        None => (rest, None),
    };
    let step = if step_part.is_empty() {
        0.0
    } else if let Some(pos) = step_part.strip_prefix('+') {
        parse_percent(pos, text)?
    } else if let Some(neg) = step_part.strip_prefix('-') {
        -parse_percent(neg, text)?
    } else {
        return Err(format!(
            "\"{text}\" is not a relative colour: use base+4%, base-15% or base+4%/80%."
        ));
    };
    let alpha = match alpha_part {
        Some(raw) => parse_percent(raw, text)?,
        None => 1.0,
    };
    Ok(RoleValue::Base { step, alpha })
}

/// Reads `"6%"`, `"base+4%"` (or `"base-15%"`, `"base+4%/80%"`), `"#rrggbb"`, `"#rgb"` or `"#rrggbbaa"`.
pub fn parse_role_value(raw: &str) -> Result<RoleValue, String> {
    let text = raw.trim();
    if text.len() >= 4 && text[..4].eq_ignore_ascii_case("base") {
        return parse_base_value(text);
    }
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
        format!("\"{text}\" is not a role value: give an opacity such as 6%, a base step such as base+4%, or a colour such as #7dd3fc.")
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

fn format_percent(fraction: f32) -> String {
    let percent = (fraction * 1000.0).round() / 10.0;
    if percent.fract() == 0.0 {
        format!("{}%", percent as i32)
    } else {
        format!("{percent}%")
    }
}

/// The value in the words `ghostex theme` and Settings show it in: `6%`, `base+4%`, `base+4%/80%`
/// or `#7dd3fc`.
pub fn format_role_value(value: RoleValue) -> String {
    match value {
        RoleValue::Ink(alpha) => format_percent(alpha),
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
        RoleValue::Base { step, alpha } => {
            let sign = if step < 0.0 { "-" } else { "+" };
            let magnitude = format_percent(step.abs());
            if alpha >= 1.0 {
                format!("base{sign}{magnitude}")
            } else {
                format!("base{sign}{magnitude}/{}", format_percent(alpha))
            }
        }
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

/// The raw value of `role` in one appearance, as saved, or as the catalog's default `themeRoles`
/// sets it when the settings hold no `themeRoles` at all.
pub fn saved_role<'a>(
    settings: &'a Map<String, Value>,
    light: bool,
    role: &str,
) -> Option<&'a str> {
    match settings.get(THEME_ROLES_SETTING) {
        Some(saved) => saved.get(appearance_key(light))?.get(role)?.as_str(),
        None => crate::default_value(THEME_ROLES_SETTING)?
            .get(appearance_key(light))?
            .get(role)?
            .as_str(),
    }
}

/// Every role as saved, or the catalog's default `themeRoles` when the settings hold none, for a
/// change to start from.
pub fn saved_roles(settings: &Map<String, Value>) -> Map<String, Value> {
    match settings.get(THEME_ROLES_SETTING) {
        Some(saved) => saved.as_object().cloned().unwrap_or_default(),
        None => crate::default_value(THEME_ROLES_SETTING)
            .and_then(|roles| roles.to_json().to_value().as_object().cloned())
            .unwrap_or_default(),
    }
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
