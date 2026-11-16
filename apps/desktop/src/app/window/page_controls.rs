//! The page window's round glass controls (window/visual_page_modal.rs), shared with the dock ask
//! panel (native_chat/dock_ask_modal.rs) so the two floating panels wear the same buttons.

use super::native_modal_kit::*;
use super::page_band_geometry::CONTROL_SIZE;
use gpui::{
    InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div,
    px,
};

/// The glass of the floating controls in one theme (`ControlGlass::for_theme`).
#[derive(Clone, Copy)]
pub(crate) struct ControlGlass {
    pub(crate) fill: gpui::Rgba,
    pub(crate) hover: gpui::Rgba,
    pub(crate) border: gpui::Rgba,
    pub(crate) icon: gpui::Rgba,
    pub(crate) text: gpui::Rgba,
}

impl ControlGlass {
    /// The page's own glass buttons (server/src/visual_pages/theme.rs `--gx-raised`,
    /// `--gx-card-border`, `--gx-text`, `--gx-fg`), so they read as part of the page.
    pub(crate) fn for_theme(light: bool) -> Self {
        if light {
            Self {
                fill: gpui::rgba(0xffffffb8),
                hover: gpui::rgba(0xfffffff2),
                border: gpui::rgba(0x43434324),
                icon: gpui::rgba(0x3f3f46ff),
                text: gpui::rgba(0x18181bff),
            }
        } else {
            Self {
                fill: gpui::rgba(0x26262699),
                hover: gpui::rgba(0x404040b8),
                border: gpui::rgba(0xe9e9e924),
                icon: gpui::rgba(0xc8cdd5ff),
                text: gpui::rgba(0xfcfcfcff),
            }
        }
    }
}

/// A round glass button of the floating controls; its presses never reach the drag band.
pub(crate) fn control_button(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    glass: ControlGlass,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CONTROL_SIZE))
        .rounded_full()
        .bg(glass.fill)
        .border_1()
        .border_color(glass.border)
        .cursor_pointer()
        .hover(move |style| style.bg(glass.hover))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
        .child(modal_icon(icon, 14.0, glass.icon))
}
