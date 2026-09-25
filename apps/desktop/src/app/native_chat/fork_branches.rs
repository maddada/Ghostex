//! The chat's fork branch switcher, GPUI's port of the deleted React
//! `packages/core-ui/chat/session-chat-fork-branch-switcher.tsx`.
//!
//! CDXC:SessionFork 2026-09-18 SEE-ALSO:
//! The button renders what the chat core projects
//! (`packages/gx-chat-core/src/menus/picker/fork_branches.rs`, the port of the deleted
//! `native-fork-branches.ts` and `fork-branches.ts`) from its copy and row rules, so the renderer
//! does not decide what a row says. The pick travels back as the
//! `selectForkBranch` host action, handled in
//! `apps/desktop/src/app/session_chat_fork_branches.rs`.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, px, svg,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use std::{cell::Cell, rc::Rc};

/// The button's toggle key in `menu_toggle.rs`.
pub(super) const FORK_BRANCHES_TRIGGER: &str = "chat-fork-branches";

/// The button's corner radius before the chat's zoom, shared with its frosted window.
pub(super) const BADGE_RADIUS: f32 = 6.0;

/// CDXC:SessionFork 2026-09-23 DECISION:
/// User: "please make the tooltip for this one appear to the left not to the right (below it) / and show have max width for it's tool tip 220px". The switcher's tooltip opens under the button with its right edge on the button's right edge, so it grows leftward into the pane, and wraps at 220px. React's switcher places it the same way.
pub(super) const TOOLTIP_PLACEMENT: ManagedTooltipPlacement = ManagedTooltipPlacement::BelowLeft;

/// CDXC:SessionFork 2026-09-25 DECISION:
/// User: "make this button very low opacity when not hovered (20%)". The switcher rests at 20% opacity and comes back to full while the pointer is over it or its menu is open. Under window glass its frosted window fades as a whole, so the blur behind the badge fades with it.
pub(super) const RESTING_OPACITY: f32 = 0.2;

/// The switcher's tooltip bubble: the family summary, wrapped at 220px.
pub(super) fn fork_branches_tooltip(
    text: String,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    gpui_component::tooltip::Tooltip::new(text)
        .max_w(px(220.0))
        .build(window, cx)
}

/// The lifecycle dot's tint, the tones of `sessionChatForkBranchTone` in React's colours
/// (`bg-emerald-500`, `bg-muted-foreground/60`, `bg-muted-foreground/35`).
pub(super) fn branch_dot_color(tone: &str, appearance: &ChatAppearance) -> Hsla {
    match tone {
        "running" => gpui::rgb(0x10b981).into(),
        "sleeping" => appearance.muted.opacity(0.6),
        _ => appearance.muted.opacity(0.35),
    }
}

impl NativeChatView {
    /// The fork switcher, or nothing at all when this session has no family: an unforked
    /// conversation shows no button.
    ///
    /// CDXC:SessionFork 2026-09-21 DECISION:
    /// User: a forked session shows a small button in the top right of the chat view, not a bar of
    /// its own. It is placed absolutely against the region the conversation occupies, so it costs
    /// the transcript no row and never pushes the rows down, and it starts below whatever chrome is
    /// above that region (the error banner, the "load earlier turns" row, the search bar) instead
    /// of over it. It carries the chat's own surface and a hairline because it floats over text.
    /// This supersedes the thin right-aligned strip the switcher used to own above the transcript.
    ///
    /// CDXC:SessionFork 2026-09-23 DECISION:
    /// User: "please make this also show like we show the scroll to bottom since it's rare" ("This conversation has X branches that share earlier history"). Under window glass the button wears the composer's wash and border over a real blur of what is behind it, drawn in its own small blurred window like the scroll-to-bottom pill (frosted_overlay_window.rs); this lays out an invisible button of the same size where the in-window one would be. Outside glass the in-pane button is unchanged.
    pub(super) fn render_fork_branch_badge(
        &mut self,
        p: &ChatAppearance,
        glass: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use super::frosted_overlay_window::FrostedOverlay;
        let branches = &self.snapshot["forkBranches"];
        let Some(count) = branches["count"].as_u64() else {
            self.hide_frosted_overlay(FrostedOverlay::ForkBranches, cx);
            return None;
        };
        let tooltip = branches["tooltip"].as_str().unwrap_or_default().to_owned();
        let s = p.scale;
        if glass {
            let shown = !self.frosted_overlay_covered(FrostedOverlay::ForkBranches);
            let report = self.frosted_overlay_reporter(FrostedOverlay::ForkBranches, shown, cx);
            return Some(
                div()
                    .absolute()
                    .top(px(6.0 * s))
                    .right(px(10.0 * s))
                    .h(px(24.0 * s))
                    .px(px(6.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .text_size(px(11.0 * s))
                    .text_color(gpui::transparent_black())
                    .whitespace_nowrap()
                    .child(div().size(px(14.0 * s)).flex_shrink_0())
                    .child(count.to_string())
                    .child(report)
                    .into_any_element(),
            );
        }
        let bounds = Rc::new(Cell::new(gpui::Bounds::default()));
        let measured = bounds.clone();
        let label = tooltip.clone();
        let menu_open = self.chat_menu_is_open(FORK_BRANCHES_TRIGGER);
        Some(
            div()
                .id("chat-fork-branches")
                .absolute()
                // Clear of the transcript's own 5px scrollbar column at the pane's right edge.
                .top(px(6.0 * s))
                .right(px(10.0 * s))
                .role(gpui::Role::Button)
                .aria_label(label)
                .chat_cursor_pointer()
                .h(px(24.0 * s))
                .px(px(6.0 * s))
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .rounded(px(BADGE_RADIUS * s))
                .border_1()
                .border_color(p.control_border)
                .bg(p.background)
                .text_size(px(11.0 * s))
                .text_color(p.muted)
                .when(menu_open, |this| this.bg(p.border))
                .when(!menu_open, |this| this.opacity(RESTING_OPACITY))
                .hover(|style| style.bg(p.border).opacity(1.0))
                .managed_tooltip_with_placement(TOOLTIP_PLACEMENT, move |window, cx| {
                    fork_branches_tooltip(tooltip.clone(), window, cx)
                })
                .child(
                    svg()
                        .path("titlebar/git-branch.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(count.to_string())
                .on_click(cx.listener(move |chat, _, window, cx| {
                    chat.open_fork_branches_menu(bounds.get(), window, cx);
                }))
                .child(
                    canvas(move |rect, _, _| measured.set(rect), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .into_any_element(),
        )
    }

    /// Opens the family menu under `trigger` (in the chat window's coordinates), or shuts it when
    /// the same button's menu is up.
    pub(super) fn open_fork_branches_menu(
        &mut self,
        trigger: gpui::Bounds<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.snapshot["forkBranches"]["menu"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if rows.is_empty() || self.chat_menu_toggled_shut(FORK_BRANCHES_TRIGGER, cx) {
            return;
        }
        self.show_chat_menu(rows, trigger, 288.0, window, cx);
    }
}
