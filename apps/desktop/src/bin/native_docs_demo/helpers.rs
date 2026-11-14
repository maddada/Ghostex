#[path = "../../app/helpers/manage_docs_resources.rs"]
pub(crate) mod manage_docs_resources;
#[path = "../../app/helpers/markdown_table_csv.rs"]
pub(crate) mod markdown_table_csv;

/// The titlebar tooltip (`helpers/titlebar/tooltips_and_popup_geometry.rs`) without its sizes.
pub(crate) fn titlebar_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    gpui_component::tooltip::Tooltip::new(text).build(window, cx)
}

/// The app's spinner rate (app/helpers/indicator_animation.rs).
pub(crate) const INDICATOR_MAX_FPS: f32 = 30.0;

/// The app's theme roles (app/helpers/theme_roles.rs): a preview sets none, so every palette
/// keeps the colour it derives.
pub(crate) fn theme_role_rgba(_id: &str, _light: bool) -> Option<gpui::Rgba> {
    None
}
pub(crate) fn theme_role_or(_id: &str, _light: bool, fallback: gpui::Hsla) -> gpui::Hsla {
    fallback
}
pub(crate) fn theme_role_or_rgba(_id: &str, _light: bool, fallback: gpui::Rgba) -> gpui::Rgba {
    fallback
}
