//! The extensions menu's row: icon, title, placement chip and the pin button. Its own file so the
//! native-modal-demo can draw the app's real row.
use gpui::{
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Styled as _, div, img,
    prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::{
    h_flex,
    tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement},
};

use super::*;
use crate::*;

pub(crate) fn titlebar_popup_extension_menu_row(
    extension: GpuiInstalledExtension,
) -> impl IntoElement {
    let pinned = extension.pinned;
    let pin_action = ToggleGpuiExtensionPin {
        extension_id: extension.id.clone(),
        pinned: !pinned,
    };
    let pin_icon = if pinned {
        "titlebar/pin-filled.svg"
    } else {
        "titlebar/pin-outline.svg"
    };
    // User, 2026-11-08: "this pin is fugly ... we should use the other look for the pin we have in the app already (filled / unfilled)". The same outline and filled pin the sidebar's Links list and account usage button draw, in the usage button's colours: the active text colour when pinned, the muted chrome colour when not.
    let pin_icon_color = if pinned {
        titlebar_active_text_color()
    } else {
        titlebar_inactive_text_color()
    };
    let pin_tooltip = if pinned { "Unpin" } else { "Pin" };
    let placement_label = extension.launch_placement_label();

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .min_h(px(TITLEBAR_POPUP_EXTENSION_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .text_color(titlebar_popup_menu_foreground())
        .child(
            h_flex()
                .flex_shrink_0()
                .size(px(20.0))
                .items_center()
                .justify_center()
                .child(img(extension.icon_image).size(px(18.0))),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                .child(extension.title),
        )
        .child(div().min_w_0().flex_1())
        .child(
            div()
                .flex_shrink_0()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(3.0))
                .bg(rgb(0xffffff).opacity(0.07))
                .text_size(px(10.0))
                .text_color(titlebar_inactive_text_color())
                .child(placement_label),
        )
        .child(
            h_flex()
                .id(format!("ghostex-gpui-extension-pin-{}", extension.id))
                .flex_shrink_0()
                .size(px(28.0))
                .items_center()
                .justify_center()
                .rounded(px(3.0))
                .when(pinned, |this| this.bg(titlebar_active_segment_color()))
                .hover(move |this| {
                    if pinned {
                        this.bg(titlebar_active_segment_color())
                    } else {
                        this.bg(titlebar_button_hover_color())
                    }
                })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(pin_action.clone()), cx);
                })
                .child(titlebar_svg_icon(pin_icon, 15.0, pin_icon_color))
                .managed_tooltip_with_placement(
                    ManagedTooltipPlacement::BelowLeft,
                    move |window, cx| titlebar_tooltip(pin_tooltip, window, cx),
                ),
        )
}
