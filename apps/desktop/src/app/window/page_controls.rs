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
    /// The round buttons' fills (`control_button`): the neutral filled button's.
    pub(crate) round: ButtonFills,
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
        let round = neutral_button_fills(light);
        if light {
            Self {
                round,
                fill: gpui::rgba(0xffffffb8),
                hover: gpui::rgba(0xfffffff2),
                border: gpui::rgba(0x43434324),
                icon: gpui::rgba(0x3f3f46ff),
                text: gpui::rgba(0x18181bff),
            }
        } else {
            Self {
                round,
                fill: gpui::rgba(0x26262699),
                hover: gpui::rgba(0x404040b8),
                border: gpui::rgba(0xe9e9e924),
                icon: gpui::rgba(0xc8cdd5ff),
                text: gpui::rgba(0xfcfcfcff),
            }
        }
    }
}

/// A round button of the floating controls (⋯, Hide, ×); its presses never reach the drag band.
///
/// CDXC:AppModal 2026-11-25 DECISION:
/// User, of the Work item pop-up's round ⋯ and × beside its Done and Reply buttons: "please make the x and 3 dots buttons here match the look of the done button in terms of the bg. also remove the outline around them. do this for all modals similar to this one pls the x and 3 dots button everywhere should look like i explain". The round controls take the neutral filled button's rest, hover and pressed fills (`neutral_button_fills`, what Done wears) and draw no border, in the Work item pop-up and the Live page window alike; they stay round, the same size and icon.
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
        .bg(glass.round.rest)
        .cursor_pointer()
        .hover(move |style| style.bg(glass.round.hover))
        .active(move |style| style.bg(glass.round.pressed))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
        .child(modal_icon(icon, 14.0, glass.icon))
}
