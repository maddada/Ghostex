//! How the address bar above a link's page looks (app/custom_links/page.rs): Back, Forward, Reload
//! (Stop while loading), the site's icon and the address field, and Close, in the side panel
//! Browser's toolbar metrics (titlebar/icon_button_and_browser_toolbar.rs). Its own file so the
//! native-modal-demo can draw the app's real bar; what the buttons do is page.rs.
use std::time::Duration;

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use super::*;
use crate::*;

/// What the bar shows of the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CustomLinkBarState {
    pub(crate) can_go_back: bool,
    pub(crate) can_go_forward: bool,
    pub(crate) loading: bool,
}

/// A press on one of the bar's parts.
pub(crate) type CustomLinkBarPress = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

/// What the bar's parts do.
pub(crate) struct CustomLinkBarActions {
    pub(crate) back: CustomLinkBarPress,
    pub(crate) forward: CustomLinkBarPress,
    /// Reload, or Stop while the page loads.
    pub(crate) reload: CustomLinkBarPress,
    pub(crate) close: CustomLinkBarPress,
    /// A press on the address field, which takes the keyboard.
    pub(crate) address: CustomLinkBarPress,
    /// A key in the address field (Escape puts the page's address back).
    pub(crate) address_key: Box<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>,
}

fn bar_button(
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    enabled: bool,
    press: CustomLinkBarPress,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex()
        .flex_shrink_0()
        .h(px(TITLEBAR_CONTROL_HEIGHT))
        .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
        .items_center()
        .justify_center()
        .rounded(px(TITLEBAR_BUTTON_RADIUS))
        .cursor_default()
        .child(titlebar_svg_icon(
            icon,
            14.0,
            if enabled {
                titlebar_icon_color()
            } else {
                browser_toolbar_disabled_icon_color()
            },
        ))
        .when(enabled, |button| {
            button
                .hover(|button| button.bg(titlebar_button_hover_color()))
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    press(event, window, cx);
                })
        })
        .managed_discrete_tooltip_with_placement(
            ManagedTooltipPlacement::Auto,
            Duration::from_millis(300),
            move |window, cx| titlebar_tooltip(label, window, cx),
        )
        .into_any_element()
}

/// The bar: `site_icon` is the page's favicon or the lock or globe, `address` the address field.
pub(crate) fn custom_link_bar(
    state: CustomLinkBarState,
    site_icon: AnyElement,
    address: Option<AnyElement>,
    actions: CustomLinkBarActions,
) -> AnyElement {
    let CustomLinkBarActions {
        back,
        forward,
        reload,
        close,
        address: address_press,
        address_key,
    } = actions;
    let field = h_flex()
        .id("ghostex-gpui-link-address")
        .flex_1()
        .min_w(px(BROWSER_ADDRESS_MINIMUM_WIDTH))
        .h(px(BROWSER_ADDRESS_HEIGHT))
        .items_center()
        .cursor_text()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            address_press(event, window, cx)
        })
        .on_key_down(move |event, window, cx| address_key(event, window, cx))
        .child(site_icon)
        .child(
            div()
                .ml(px(8.0))
                .flex_1()
                .min_w_0()
                .h(px(BROWSER_ADDRESS_HEIGHT))
                .overflow_hidden()
                .children(address),
        );
    h_flex()
        .id(SharedString::from("ghostex-gpui-link-bar"))
        .flex_shrink_0()
        .h(px(BROWSER_TOOLBAR_HEIGHT))
        .w_full()
        .items_center()
        .pl(px(BROWSER_TOOLBAR_LEADING_PADDING))
        .pr(px(WORKAREA_HEADER_EDGE_PADDING + WORKAREA_HEADER_PINNED_GAP))
        .gap(px(BROWSER_TOOLBAR_ITEM_GAP))
        .bg(browser_toolbar_background())
        .border_b_1()
        .border_color(chrome_color(0x252525, 0xd4d4d4))
        .child(bar_button(
            "ghostex-gpui-link-back",
            "Back",
            TITLEBAR_ICON_CHEVRON_LEFT,
            state.can_go_back,
            back,
        ))
        .child(bar_button(
            "ghostex-gpui-link-forward",
            "Forward",
            BROWSER_ICON_CHEVRON_RIGHT,
            state.can_go_forward,
            forward,
        ))
        .child(bar_button(
            "ghostex-gpui-link-reload",
            if state.loading { "Stop" } else { "Reload" },
            if state.loading {
                TITLEBAR_ICON_X
            } else {
                BROWSER_ICON_RELOAD
            },
            true,
            reload,
        ))
        .child(div().w(px(6.0)).flex_shrink_0())
        .child(field)
        .child(bar_button(
            "ghostex-gpui-link-close",
            "Close",
            TITLEBAR_ICON_X,
            true,
            close,
        ))
        .into_any_element()
}
