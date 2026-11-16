//! The theme roles the window paints with (packages/settings-catalog/src/theme_roles.rs, where the
//! user's decision is): saved here whenever the settings are applied, and read by the palettes,
//! each of which keeps the colour it derives today for a role that is not set.
//!
//! CDXC:Theming 2026-11-14 SEE-ALSO: packages/settings-catalog/src/theme_roles.rs (the roles and their values), server/src/theme/ (`ghostex theme`).

use std::sync::RwLock;

use ghostex_settings_catalog::theme_roles::{RoleValue, roles_for};
use gpui::{Hsla, Rgba};
use serde_json::{Map, Value};

#[derive(Default)]
struct Roles {
    dark: Vec<(&'static str, RoleValue)>,
    light: Vec<(&'static str, RoleValue)>,
}

static ROLES: RwLock<Roles> = RwLock::new(Roles {
    dark: Vec::new(),
    light: Vec::new(),
});

/// Saves the roles `settings` sets, for every palette drawn from now on.
pub(crate) fn apply_theme_roles(settings: &Map<String, Value>) {
    let roles = Roles {
        dark: roles_for(settings, false),
        light: roles_for(settings, true),
    };
    if let Ok(mut saved) = ROLES.write() {
        *saved = roles;
    }
}

/// The colour role `id` sets in one appearance, or `None` while it keeps the theme's own.
pub(crate) fn theme_role_rgba(id: &str, light: bool) -> Option<Rgba> {
    let roles = ROLES.read().ok()?;
    let list = if light { &roles.light } else { &roles.dark };
    let [r, g, b, a] = list.iter().find(|(role, _)| *role == id)?.1.rgba(light);
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
