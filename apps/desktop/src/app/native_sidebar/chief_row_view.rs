//! What Chief's row looks like, without what it does: the sidebar (chief.rs) adds its clicks and
//! menus, and the `native-modal-demo` preview (`GHOSTEX_NATIVE_MODAL_DEMO=chief-row`) draws the
//! same pixels offscreen for screenshots. Only gpui and gpui-component, so the preview can include
//! it by path.
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User: "make the chief row have an icon to its left and make it match the placement of the project header and match its style and not have border around it (should be exactly like the project header basically). Also when i have chiefs on multiple machines like we agreed we need to make the icon to the left of the chief turn to chevron just like projects have multiple sessions below them." The row is a project header's twin (project_header.rs: its box, height, insets, gutter, weight, hover and selected fill, no border), and other computers' Chiefs fold under it like a project's sessions. This supersedes the outlined row with a "+N" chip.
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1882): line "Chief" up with project names. The flower sits in the project-icon column and "Chief" where project names start; the chevron gutter holds the project's fold chevron only while other computers have a Chief, and stays empty with one. Supersedes the flower in the gutter.
//! CDXC:Chief 2026-11-08 DECISION:
//! User: "We need a dimmed chevron here please so there's not just empty space on the left needlessly". With one Chief the gutter shows the project's chevron dimmed (`DIMMED_CHEVRON`), pointing right and doing nothing; it is the normal chevron once other computers' Chiefs can unfold under it. Supersedes the empty gutter with one Chief.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use gpui_component::{h_flex, v_flex};

/// Chief's name on its row and in its menu ("Main Hub", Q1931; server/src/chief/mod.rs `CHIEF_TITLE`).
pub(crate) const CHIEF_LABEL: &str = "Main Hub";
/// The Ghostex flower in one colour (Q1735).
pub(crate) const FLOWER_ICON: &str = "titlebar/ghostex-flower.svg";
/// How much of the muted colour the chevron keeps while nothing can unfold under it.
const DIMMED_CHEVRON: f32 = 0.35;
/// The project header's fold chevron (`COMMAND_ICON_CHEVRON_RIGHT`).
const CHEVRON_ICON: &str = "titlebar/chevron-right.svg";
/// The project header's chevron gutter and how far its box reaches left into it
/// (project_header.rs `PROJECT_HEADER_CHEVRON_GUTTER`, `PROJECT_HEADER_CHEVRON_INSET`).
const GUTTER: f32 = 18.0;
const INSET: f32 = GUTTER - 3.0;

/// The sidebar colours the row is drawn in (appearance.rs).
#[derive(Clone, Copy)]
pub(crate) struct ChiefRowColors {
    /// The flower, the fold chevron and the first-open hint (`muted`).
    pub(crate) muted: Hsla,
    /// The hover fill of a project header and a session card (`session_hover`).
    pub(crate) hover: Hsla,
    /// The fill while its chat is open (`selected`).
    pub(crate) selected_fill: Hsla,
}

/// Whether other computers' Chiefs fold under this computer's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChiefFold {
    /// Only this computer has one: the gutter shows the chevron dimmed, and it does nothing.
    Single,
    /// Others do and are folded away: the project's chevron, pointing right.
    Folded,
    /// Others do and show under it: the chevron, pointing down.
    Unfolded,
}

fn flower(size: f32, color: Hsla) -> impl IntoElement {
    svg()
        .size(px(size))
        .flex_shrink_0()
        .path(FLOWER_ICON)
        .text_color(color)
}

/// CDXC:Chief 2026-11-08 DECISION:
/// User: "please don't show the chief of staff thing at the top in the gbots or hermes bots' views". The row shows in the sessions sidebar only: never in Bots mode (GBots or Hermes), nor while the extension is off.
pub(crate) fn chief_row_shown(enabled: bool, bots_mode: bool) -> bool {
    enabled && !bots_mode
}

/// The block the rows sit in: the list column's and a project block's left and right padding
/// (render.rs: the column's `pl 6 / pr 2`, each project block's `pl 18 / mr 5`, its `mb 10`), so
/// the header lines up with the project headers under it.
pub(crate) fn chief_block(scale: f32) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .pl(px(24.0 * scale))
        .pr(px(7.0 * scale))
        .pb(px(10.0 * scale))
}

/// The selected computer's Chief, drawn as a project header: the fold chevron in the gutter
/// (dimmed with no other computers' Chiefs), the flower in the project-icon column, "Chief", "Pick an agent"
/// before its first open, its status dot. `outline` is drawn over
/// it while its chat is open (the sidebar's selected outline).
pub(crate) fn chief_header_row(
    label: &str,
    scale: f32,
    colors: ChiefRowColors,
    fold: ChiefFold,
    focused: bool,
    needs_agent: bool,
    outline: Option<AnyElement>,
    status: Option<AnyElement>,
) -> Stateful<Div> {
    let chevron = svg()
        .path(CHEVRON_ICON)
        .size(px(16.0 * scale))
        .text_color(if fold == ChiefFold::Single {
            colors.muted.opacity(DIMMED_CHEVRON)
        } else {
            colors.muted
        })
        .with_transformation(gpui::Transformation::rotate(gpui::percentage(
            if fold == ChiefFold::Unfolded {
                0.25
            } else {
                0.0
            },
        )));
    h_flex()
        .id(if label == CHIEF_LABEL {
            gpui::SharedString::from("native-sidebar-chief")
        } else {
            gpui::SharedString::from(format!("native-sidebar-chief-{label}"))
        })
        .role(gpui::Role::TreeItem)
        .aria_label(label.to_string())
        .when(fold != ChiefFold::Single, |row| {
            row.aria_expanded(fold == ChiefFold::Unfolded)
        })
        .relative()
        .h(px(30.0 * scale))
        .ml(px(-INSET * scale))
        .mr(px(3.0 * scale))
        .pl(px((5.0 + GUTTER) * scale))
        .pr(px(6.0 * scale))
        .gap(px(10.0 * scale))
        .rounded(px(5.0 * scale))
        .cursor_default()
        .hover(|row| row.bg(colors.hover))
        .when(focused, |row| {
            row.bg(colors.selected_fill).children(outline)
        })
        .child(
            div()
                .absolute()
                .left_0()
                .top(px(7.0 * scale))
                .child(chevron),
        )
        .child(flower(16.0 * scale, colors.muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .font_weight(gpui::FontWeight::LIGHT)
                .child(div().min_w_0().truncate().child(label.to_string())),
        )
        .when(needs_agent, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.0 * scale))
                    .text_color(colors.muted)
                    .child("Pick an agent"),
            )
        })
        .children(status)
}

/// The width of the Main Hub tooltip's text column.
pub(crate) const MAIN_HUB_TOOLTIP_WIDTH: f32 = 264.0;

/// What the Main Hub card says under its title.
pub(crate) fn main_hub_tooltip_lines() -> [&'static str; 2] {
    [
        "The home for chats that don't belong to a project: quick questions, terminals and agents started in your home folder.",
        "Its agent sees and steers every Ghostex agent on this computer, and talks to the Main Hubs on your other computers.",
    ]
}

/// The Main Hub card's content, without its frame: the flower and the title, two plain lines,
/// and what a click and a right-click do. tooltips.rs `main_hub_card` frames it; the desktop shows
/// it in its own window beside the sidebar (window/main_hub_card.rs), the web build in its page
/// (web_host/main_hub_card.rs), and the `native-modal-demo` preview draws it for screenshots.
///
/// CDXC:Chief 2026-11-08 DECISION:
/// User (Q1931): "when we hover on it we need to show a tool tip that's beautiful and explains what is this please". A card, not a one-liner: a short title, then plain lines saying it is the home for chats that belong to no project and the agent that sees and steers every Ghostex agent on this computer (and the other computers' Main Hubs). On the phone the same words open its long-press menu.
/// CDXC:Chief 2026-11-08 DECISION:
/// User (Q1991): "move the hover card OUTSIDE the sidebar, like the hover menu that shows a session's links (it floats beside the sidebar instead of over the rows); keep its content". It opens beside the sidebar, level with the row, after the tooltip delay, and closes when the pointer leaves the row. Supersedes the tooltip drawn over the rows.
pub(crate) fn main_hub_tooltip_content(scale: f32, foreground: Hsla, secondary: Hsla) -> Div {
    let lines = main_hub_tooltip_lines();
    let hint = |key: &'static str, does: &'static str| {
        h_flex()
            .gap(px(4.0 * scale))
            .child(
                div()
                    .px(px(4.0 * scale))
                    .rounded(px(3.0 * scale))
                    .border_1()
                    .border_color(secondary.opacity(0.45))
                    .text_size(px(10.0 * scale))
                    .line_height(px(14.0 * scale))
                    .child(key),
            )
            .child(does)
    };
    v_flex()
        .w(px(MAIN_HUB_TOOLTIP_WIDTH * scale))
        .gap(px(5.0 * scale))
        .child(
            h_flex()
                .gap(px(7.0 * scale))
                .pb(px(1.0 * scale))
                .child(flower(15.0 * scale, foreground))
                .child(
                    div()
                        .text_size(px(13.0 * scale))
                        .line_height(px(18.0 * scale))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(foreground)
                        .child(CHIEF_LABEL),
                ),
        )
        .children(lines.into_iter().map(|line| {
            div()
                .text_size(px(11.5 * scale))
                .line_height(px(16.0 * scale))
                .text_color(foreground.opacity(0.82))
                .child(line)
        }))
        .child(
            h_flex()
                .pt(px(3.0 * scale))
                .gap(px(10.0 * scale))
                .text_size(px(10.5 * scale))
                .text_color(secondary)
                .child(hint("Click", "open the chat"))
                .child(hint("Right-click", "more")),
        )
}

/// Another computer's Chief, unfolded under this computer's like a session under its project: the
/// flower and the computer's name.
pub(crate) fn chief_other_row(
    id: impl Into<gpui::SharedString>,
    scale: f32,
    colors: ChiefRowColors,
    machine_label: &str,
    focused: bool,
    outline: Option<AnyElement>,
    status: Option<AnyElement>,
) -> Stateful<Div> {
    h_flex()
        .id(id.into())
        .role(gpui::Role::TreeItem)
        .aria_label(format!("{CHIEF_LABEL} on {machine_label}"))
        .relative()
        .h(px(30.0 * scale))
        .mr(px(3.0 * scale))
        .pl(px(8.0 * scale))
        .pr(px(6.0 * scale))
        .gap(px(8.0 * scale))
        .rounded(px(5.0 * scale))
        .cursor_default()
        .hover(|row| row.bg(colors.hover))
        .when(focused, |row| {
            row.bg(colors.selected_fill).children(outline)
        })
        .child(flower(14.0 * scale, colors.muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(gpui::FontWeight::LIGHT)
                .child(machine_label.to_string()),
        )
        .children(status)
}
