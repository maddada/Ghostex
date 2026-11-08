//! What Chief's row looks like, without what it does: the sidebar (chief.rs) adds its clicks and
//! menus, and the `native-modal-demo` preview (`GHOSTEX_NATIVE_MODAL_DEMO=chief-row`) draws the
//! same pixels offscreen for screenshots. Only gpui and gpui-component, so the preview can include
//! it by path.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use gpui_component::h_flex;

/// Chief's name on its row and in its menu (Q1746).
pub(crate) const CHIEF_LABEL: &str = "Chief";
/// The Ghostex flower in one colour (Q1735).
pub(crate) const FLOWER_ICON: &str = "titlebar/ghostex-flower.svg";

/// The sidebar colours the row is drawn in.
#[derive(Clone, Copy)]
pub(crate) struct ChiefRowColors {
    pub(crate) foreground: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) icon: Hsla,
    pub(crate) label: Hsla,
    pub(crate) hover: Hsla,
    pub(crate) selected_fill: Hsla,
    pub(crate) selected_outline: Hsla,
}

fn flower(size: f32, color: Hsla) -> impl IntoElement {
    svg()
        .size(px(size))
        .flex_shrink_0()
        .path(FLOWER_ICON)
        .text_color(color)
}

/// This computer's Chief: the flower, "Chief", "Pick an agent" before its first open, the "+N"
/// chip of the other computers' Chiefs, and its status dot. Outlined like a Space tile, filled
/// like a selected one while its chat is open.
pub(crate) fn chief_main_row(
    scale: f32,
    colors: ChiefRowColors,
    focused: bool,
    needs_agent: bool,
    others_chip: Option<AnyElement>,
    status: Option<AnyElement>,
) -> Stateful<Div> {
    h_flex()
        .id("native-sidebar-chief")
        .role(gpui::Role::Button)
        .aria_label(CHIEF_LABEL)
        .h(px(30.0 * scale))
        .px(px(8.0 * scale))
        .gap(px(7.0 * scale))
        .rounded(px(6.0 * scale))
        .border_1()
        .map(|row| {
            if focused {
                row.bg(colors.selected_fill)
                    .border_color(colors.selected_outline)
            } else {
                row.border_color(colors.muted.opacity(0.25))
                    .hover(|row| row.bg(colors.hover))
            }
        })
        .cursor_pointer()
        .child(flower(16.0 * scale, colors.icon))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(13.0 * scale))
                .text_color(colors.foreground)
                .child(CHIEF_LABEL),
        )
        .when(needs_agent, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5 * scale))
                    .text_color(colors.muted)
                    .child("Pick an agent"),
            )
        })
        .children(others_chip)
        .children(status)
}

/// The "+N" chip that unfolds the other computers' Chiefs (computers combined, Q1747).
pub(crate) fn chief_others_chip(
    scale: f32,
    colors: ChiefRowColors,
    count: usize,
    expanded: bool,
) -> Stateful<Div> {
    div()
        .id("native-sidebar-chief-others")
        .role(gpui::Role::Button)
        .aria_label(if expanded {
            "Fold the other computers' Chiefs"
        } else {
            "Show the other computers' Chiefs"
        })
        .flex_shrink_0()
        .px(px(5.0 * scale))
        .rounded(px(4.0 * scale))
        .text_size(px(11.5 * scale))
        .text_color(colors.label)
        .when(expanded, |chip| chip.bg(colors.hover))
        .hover(|chip| chip.bg(colors.hover))
        .child(format!("+{count}"))
}

/// Another computer's Chief, unfolded under this computer's: the flower and the computer's name.
pub(crate) fn chief_other_row(
    id: impl Into<gpui::SharedString>,
    scale: f32,
    colors: ChiefRowColors,
    machine_label: &str,
    focused: bool,
    status: Option<AnyElement>,
) -> Stateful<Div> {
    h_flex()
        .id(id.into())
        .role(gpui::Role::Button)
        .aria_label(format!("{CHIEF_LABEL} on {machine_label}"))
        .h(px(26.0 * scale))
        .pl(px(22.0 * scale))
        .pr(px(8.0 * scale))
        .gap(px(6.0 * scale))
        .rounded(px(6.0 * scale))
        .cursor_pointer()
        .map(|row| {
            if focused {
                row.bg(colors.selected_fill)
            } else {
                row.hover(|row| row.bg(colors.hover))
            }
        })
        .child(flower(13.0 * scale, colors.icon))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.5 * scale))
                .text_color(colors.label)
                .child(machine_label.to_string()),
        )
        .children(status)
}
