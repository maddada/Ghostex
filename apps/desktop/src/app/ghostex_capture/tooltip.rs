//! The tooltip every Ghostex Capture window shows.

use gpui::{AnyView, App, Styled, Window};
use gpui_component::text::Text;
use gpui_component::tooltip::Tooltip;

/// A solid bubble in Ghostex Capture's own colours.
///
/// CDXC:GhostexCapture 2026-09-30 DECISION:
/// User: "make the continue your draft and other tooltips in the whole floating capture feature not
/// transparent please even if transparency enabled in the app". The theme's popover colour is
/// see-through under app transparency, so these bubbles set their own opaque fill.
///
/// CDXC:Theming 2026-11-20: the fill follows the theme (`SolidPalette`, still opaque): the work
/// area's colour lifted toward white in dark mode under glass and in light mode, with dark text in
/// light mode; dark with glass off keeps #1f1f1f.
pub(crate) fn solid_tooltip(label: impl Into<Text>, window: &mut Window, cx: &mut App) -> AnyView {
    let c = super::glass::SolidPalette::resolve();
    Tooltip::new(label)
        .bg(c.tooltip)
        .border_color(c.edge)
        .text_color(c.strong)
        .build(window, cx)
}
