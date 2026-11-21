//! The theme roles the window paints with (packages/settings-catalog/src/theme_roles.rs, where the
//! user's decision is): saved here whenever the settings are applied, and read by the palettes,
//! each of which keeps the colour it derives today for a role that is not set.
//!
//! CDXC:Theming 2026-11-14 SEE-ALSO: packages/settings-catalog/src/theme_roles.rs (the roles and their values), server/src/theme/ (`ghostex theme`).

use std::sync::RwLock;

use ghostex_settings_catalog::theme_colours::work_area_background_for_variant;
use ghostex_settings_catalog::theme_roles::{RoleValue, roles_for};
use gpui::{Hsla, Rgba};
use serde_json::{Map, Value};

struct Roles {
    dark: Vec<(&'static str, RoleValue)>,
    light: Vec<(&'static str, RoleValue)>,
    dark_base: u32,
    light_base: u32,
}

static ROLES: RwLock<Roles> = RwLock::new(Roles {
    dark: Vec::new(),
    light: Vec::new(),
    dark_base: 0x0d0d0d,
    light_base: 0xf7f7f7,
});

/// Saves the roles `settings` sets, and the work area base colour of each appearance so relative
/// roles (`base+N%`) follow the picked colour on every palette drawn from now on.
pub(crate) fn apply_theme_roles(settings: &Map<String, Value>) {
    let roles = Roles {
        dark: roles_for(settings, false),
        light: roles_for(settings, true),
        dark_base: work_area_background_for_variant(settings, false),
        light_base: work_area_background_for_variant(settings, true),
    };
    if let Ok(mut saved) = ROLES.write() {
        *saved = roles;
    }
}

/// The colour role `id` sets in one appearance, or `None` while it keeps the theme's own.
pub(crate) fn theme_role_rgba(id: &str, light: bool) -> Option<Rgba> {
    let roles = ROLES.read().ok()?;
    let (list, base) = if light {
        (&roles.light, roles.light_base)
    } else {
        (&roles.dark, roles.dark_base)
    };
    let [r, g, b, a] = list
        .iter()
        .find(|(role, _)| *role == id)?
        .1
        .rgba_on_base(light, base);
    Some(Rgba { r, g, b, a })
}

/// `fallback` unless role `id` is set in this appearance.
pub(crate) fn theme_role_or(id: &str, light: bool, fallback: Hsla) -> Hsla {
    theme_role_rgba(id, light).map_or(fallback, Hsla::from)
}

/// [`theme_role_or`] for the palettes that keep `Rgba`.
pub(crate) fn theme_role_or_rgba(id: &str, light: bool, fallback: Rgba) -> Rgba {
    theme_role_rgba(id, light).unwrap_or(fallback)
}
