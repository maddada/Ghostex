//! What each theme role paints while it is not set: the colour Settings shows on a role's row so
//! the user starts from it (settings_modal/tabs/theme/role_defaults.rs). Each role takes the
//! colour of the place it is most seen, worked out as that place works it out but with no role laid
//! over it.
//!
//! CDXC:Theming 2026-11-18 SEE-ALSO: settings_modal/tabs/theme/roles.rs (the decision), helpers/theme_roles.rs (the roles applied).

use crate::app::native_chat::appearance::ChatAppearance;
use gpui::{Hsla, Rgba, rgb};

/// Red, green, blue and opacity (0 to 1) role `id` paints in one appearance while it is not set.
/// `None` for a role this does not know.
pub(crate) fn theme_role_default_rgba(id: &str, light: bool) -> Option<[f32; 4]> {
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let settings = snapshot.object();
    let chat = ChatAppearance::without_theme_roles(light);
    let glass = super::window_glass_active();
    let ink = |alpha: f32| Hsla::from(rgb(if light { 0x000000 } else { 0xffffff })).opacity(alpha);
    let white = |alpha: f32| Hsla::from(rgb(0xffffff)).opacity(alpha);
    let colour: Hsla = match id {
        "border" => chat.border,
        // The sidebar's edge and the lines between panes (`glass_divider`).
        "divider" if glass => ink(0.08),
        "divider" => rgb(if light { 0xd4d4d4 } else { 0x252525 }).into(),
        "text" => chat.prose,
        "mutedText" => chat.muted,
        "accent" => {
            rgb(ghostex_settings_catalog::theme_colours::theme_accent_for_variant(settings, light))
                .into()
        }
        "link" => ChatAppearance::own_link(light)?,
        "surface" => chat.card_panel,
        // Code blocks keep the Controls fill (markdown_style.rs).
        "control" | "codeBlock" => chat.input,
        "menu" => chat.menu_opaque(),
        // The sidebar's selected session (native_sidebar/appearance.rs).
        "selection" if glass && light => white(0.55),
        "selection" if light => rgb(0xe7e7e7).into(),
        "selection" => white(0.12),
        // The sidebar's session under the pointer.
        "hover" if glass => ink(if light { 0.12 } else { 0.16 }),
        "hover" if light == super::chrome_uses_light_appearance() => {
            let foreground = super::titlebar_active_text_color();
            super::titlebar_background()
                .blend(rgb(0).opacity(0.04).into())
                .blend(foreground.opacity(if light { 0.12 } else { 0.22 }))
        }
        "hover" => ink(if light { 0.12 } else { 0.22 }),
        "focusRing" => chat.ring,
        "userMessage" => chat.user_message,
        "chatBox" => chat.composer_background,
        "success" => ChatAppearance::own_success(light),
        // The chat's working dot (chat_pills.rs).
        "warning" => rgb(crate::app::native_sidebar::status::WORKING_COLOR).into(),
        "danger" => ChatAppearance::own_error(light),
        _ => return None,
    };
    let Rgba { r, g, b, a } = colour.into();
    Some([r, g, b, a])
}
