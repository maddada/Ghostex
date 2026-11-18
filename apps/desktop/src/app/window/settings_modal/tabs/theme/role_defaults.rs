//! What a theme role paints while it is not set, for its row in Theme colours and Chat colours
//! (roles.rs): the app hands Settings a resolver when it opens it
//! (app/helpers/theme_role_defaults.rs); Settings cannot reach the app's palettes itself, since
//! the `native-modal-demo` binary includes this module alone. Without one (that binary) a row not
//! set shows no colour.

use ghostex_settings_catalog::theme_roles::{RoleValue, appearance_key, parse_role_value};
use serde_json::Value;
use std::sync::OnceLock;

/// Red, green, blue and opacity (0 to 1) role `id` paints in one appearance while it is not set.
pub(crate) type RoleDefaultResolver = fn(&str, bool) -> Option<[f32; 4]>;

static RESOLVER: OnceLock<RoleDefaultResolver> = OnceLock::new();

/// Called by the app before it opens Settings; later calls keep the first.
pub(crate) fn set_role_default_resolver(resolver: RoleDefaultResolver) {
    let _ = RESOLVER.set(resolver);
}

/// What role `id` paints in one appearance while it is not set in `saved` (the `themeRoles` on the
/// page), as a role value: an opacity of the ink when it is a wash of white (dark) or black
/// (light), so the slider keeps it see-through, else a colour with its opacity.
pub(super) fn role_default(saved: &Value, id: &str, light: bool) -> Option<RoleValue> {
    // A role that follows another takes that one's value while it is set on this page.
    let follows = match id {
        "link" => Some("accent"),
        "codeBlock" => Some("control"),
        _ => None,
    };
    if let Some(value) = follows.and_then(|other| {
        saved
            .get(appearance_key(light))?
            .get(other)?
            .as_str()
            .and_then(|raw| parse_role_value(raw).ok())
    }) {
        return Some(value);
    }
    let [r, g, b, a] = RESOLVER.get()?(id, light)?;
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    let rgb = [byte(r), byte(g), byte(b)];
    let ink = if light { 0 } else { 255 };
    let alpha = (a.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
    if alpha < 1.0 && rgb == [ink; 3] {
        Some(RoleValue::Ink(alpha))
    } else {
        Some(RoleValue::Colour { rgb, alpha })
    }
}
