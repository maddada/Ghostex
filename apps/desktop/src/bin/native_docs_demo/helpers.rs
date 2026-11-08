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
