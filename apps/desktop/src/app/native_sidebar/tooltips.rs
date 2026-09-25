use gpui::prelude::FluentBuilder as _;
use gpui::{AnyView, App, FontWeight, ParentElement, Styled, Window, div, px};
use gpui_component::{
    tooltip::{ManagedTooltipPlacement, Tooltip},
    v_flex,
};

use super::session_list::SESSION_INSET_X;
use crate::app::helpers::{
    titlebar_popup_menu_background, titlebar_popup_menu_border_color,
    titlebar_popup_menu_foreground, window_glass_active_in,
};

/**
CDXC:Sidebar 2026-09-21 DECISION: A session or project tooltip "needs to not go out of the sidebar. It has to remain inside it with some padding from the right and left", its width matches the width of the actual card, and it sits right below the card rather than floating lower. Cards sit at different insets (collections, the scroll gutter), so a session's span is its card's own painted bounds, recorded while it is hovered; `sidebar_tooltip` drops the stock bubble margins so the two line up.
*/
#[derive(Clone, Copy)]
pub(super) struct SidebarTooltipSpan {
    pub(super) left: f32,
    pub(super) right: f32,
}

impl SidebarTooltipSpan {
    /// The span of a row that fills the sidebar at the standard row inset.
    pub(super) fn sidebar(sidebar_width: f32, scale: f32) -> Self {
        let inset = SESSION_INSET_X * scale;
        Self {
            left: inset,
            right: (sidebar_width - inset).max(inset),
        }
    }

    pub(super) fn placement(self) -> ManagedTooltipPlacement {
        ManagedTooltipPlacement::BelowWithin {
            left: px(self.left),
            right: px(self.right),
        }
    }
}

/**
CDXC:Sidebar 2026-09-21 DECISION: "Please make the tooltip content not have spaces between each of the sections." The shared tooltip text separates its sections with blank lines for the React renderer; here every non-blank line is one tight row.
*/
pub(super) fn sidebar_tooltip(
    text: String,
    span: SidebarTooltipSpan,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    sidebar_tooltip_sized(text, span, true, scale, window, cx)
}

/// CDXC:Spaces 2026-09-21 DECISION: The Space tooltips up top "match the look and appear below, but no need to make the width match anything, give them free width": same bubble, sized to its text and only capped so it stays inside the sidebar.
pub(super) fn sidebar_free_width_tooltip(
    text: String,
    span: SidebarTooltipSpan,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    sidebar_tooltip_sized(text, span, false, scale, window, cx)
}

/**
CDXC:Sidebar 2026-09-23 DECISION: User: the session tooltip, a flat near-black box on the tinted glass sidebar, should "look more fitting". It takes the sidebar menus' shape (8px corners, the menus' soft ink outline and tinted menu colour), and under window glass it is frosted: on macOS it draws in the frosted tooltip host with the frosted menus' fill and corners (2026-09-25, app/window/frosted_host.rs); where it stays in the window it is only slightly see-through, because an in-window tooltip cannot blur the rows beneath it. The first line (the title) reads as the heading; the state and id lines beneath it are smaller, muted and cut short on one line each.
*/
fn sidebar_tooltip_sized(
    text: String,
    span: SidebarTooltipSpan,
    fill_span: bool,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let card_width = (span.right - span.left).max(0.0);
    let content_width = px((card_width - 16.0 * scale - 2.0).max(0.0));
    let menu = titlebar_popup_menu_background();
    // Under glass the tooltip draws in the frosted tooltip host, so it takes the frosted menus' fill
    // and the host's bubble corners (see app/window/frosted_host.rs).
    let frosted = window_glass_active_in(window)
        && crate::app::window::frosted_host::frosted_hosting_active();
    let background = if frosted {
        crate::app::helpers::frosted_menu_fill(menu)
    } else if window_glass_active_in(window) {
        menu.opacity(0.9)
    } else {
        menu
    };
    let radius = if frosted {
        px(gpui_component::tooltip::FROSTED_TOOLTIP_RADIUS)
    } else {
        px(8.0 * scale)
    };
    let secondary = titlebar_popup_menu_foreground().opacity(0.6);
    Tooltip::element(move |_, _| {
        let lines = text.lines().filter(|line| !line.trim().is_empty());
        v_flex()
            .map(|lines| {
                if fill_span {
                    lines.w(content_width)
                } else {
                    lines.max_w(content_width)
                }
            })
            .gap(px(2.0 * scale))
            .children(lines.enumerate().map(|(index, line)| {
                let row = div().min_w_0();
                if index == 0 {
                    row.pb(px(1.0 * scale))
                        .text_size(px(12.5 * scale))
                        .line_height(px(17.0 * scale))
                        .font_weight(FontWeight::MEDIUM)
                        .child(line.to_owned())
                } else {
                    row.truncate()
                        .text_size(px(11.0 * scale))
                        .line_height(px(15.0 * scale))
                        .text_color(secondary)
                        .child(line.to_owned())
                }
            }))
    })
    .when(fill_span, |bubble| bubble.w(px(card_width)))
    .mx_0()
    .mt(px(2.0 * scale))
    .mb_0()
    .py(px(7.0 * scale))
    .px(px(9.0 * scale))
    .rounded(radius)
    .bg(background)
    .border_color(titlebar_popup_menu_border_color())
    .build(window, cx)
}
