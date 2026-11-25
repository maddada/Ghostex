//! The page window's floating controls (window/visual_page_modal.rs), shared with the dock ask
//! panel (native_chat/dock_ask_modal.rs) so the two floating panels wear the same buttons.

use super::native_modal_kit::*;
use super::page_band_geometry::CONTROL_SIZE;
use gpui::{
    InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div,
    px,
};

/// The look of the floating controls in one theme (`ControlGlass::for_theme`).
#[derive(Clone, Copy)]
pub(crate) struct ControlGlass {
    /// The controls' fills (`control_fill`): the neutral filled button's.
    pub(crate) fills: ButtonFills,
    pub(crate) icon: gpui::Rgba,
    pub(crate) text: gpui::Rgba,
}

impl ControlGlass {
    /// The neutral filled button's fills (what Done wears), with the page's own ink
    /// (server/src/visual_pages/theme.rs `--gx-text`, `--gx-fg`) for icons and words.
    pub(crate) fn for_theme(light: bool) -> Self {
        let fills = neutral_button_fills(light);
        if light {
            Self {
                fills,
                icon: gpui::rgba(0x3f3f46ff),
                text: gpui::rgba(0x18181bff),
            }
        } else {
            Self {
                fills,
                icon: gpui::rgba(0xc8cdd5ff),
                text: gpui::rgba(0xfcfcfcff),
            }
        }
    }
}

/// Paints a floating control (the round ⋯, Hide and ×, the page window's Refresh) with the
/// neutral filled button's rest, hover and pressed fills, and no border.
pub(crate) fn control_fill(
    button: gpui::Stateful<gpui::Div>,
    glass: ControlGlass,
) -> gpui::Stateful<gpui::Div> {
    button
        .bg(glass.fills.rest)
        .hover(move |style| style.bg(glass.fills.hover))
        .active(move |style| style.bg(glass.fills.pressed))
}

/// A round button of the floating controls (⋯, Hide, ×); its presses never reach the drag band.
///
/// CDXC:AppModal 2026-11-25 DECISION:
/// User, of the Work item pop-up's round ⋯ and × beside its Done and Reply buttons: "please make the x and 3 dots buttons here match the look of the done button in terms of the bg. also remove the outline around them. do this for all modals similar to this one pls the x and 3 dots button everywhere should look like i explain". The round controls take the neutral filled button's rest, hover and pressed fills (`neutral_button_fills`, what Done wears) and draw no border, in the Work item pop-up and the Live page window alike; they stay round, the same size and icon. User, after: the Live page window's Refresh in the same row takes the same fill with no outline (`control_fill`).
pub(crate) fn control_button(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    glass: ControlGlass,
) -> gpui::Stateful<gpui::Div> {
    control_fill(div().id(id), glass)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CONTROL_SIZE))
        .rounded_full()
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
        .child(modal_icon(icon, 14.0, glass.icon))
}
