//! A coordinator's merged-worktrees banner, after its threads: "N merged worktrees" with Clean up
//! all and Keep, instead of a Clean up / Keep line on each thread, and a quiet line about the
//! worktrees gxserver cleaned up by itself, with Undo.
//!
//! CDXC:WorkMode 2026-11-20 SEE-ALSO: the user's decision in server/src/server/work_cleanup_threads.rs
//! (Q4286); gx-core `RowNesting::cleanup_offers` (what the banner counts), apps/mobile/app
//! src/components/sessions/CleanupBanner.tsx (the phone's).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::{h_flex, v_flex};
use serde_json::json;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The work chips' merged purple (work_chips.rs).
const MERGED_PURPLE: u32 = 0xa371f7;
/// One line of the banner, unscaled.
pub(super) const BANNER_LINE_HEIGHT: f32 = 18.0;
/// The space above and below its lines, unscaled.
pub(super) const BANNER_PADDING_Y: f32 = 3.0;

/// What a coordinator's banner says; `None` from [`CleanupBanner::new`] when it says nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CleanupBanner {
    /// Threads whose merged-PR offer it answers.
    pub(crate) offers: u16,
    /// Threads cleaned up by themselves that it reports, and how many Undo can put back.
    pub(crate) auto_cleaned: u16,
    pub(crate) auto_cleaned_undo: u16,
    /// Of the offers, the ones the pass that cleans up by itself kept, and every reason why
    /// (gx-core `CleanupKept::bit`).
    pub(crate) kept: u16,
    pub(crate) kept_reasons: u8,
}

impl CleanupBanner {
    pub(crate) fn new(
        offers: u16,
        auto_cleaned: u16,
        auto_cleaned_undo: u16,
        kept: u16,
        kept_reasons: u8,
    ) -> Option<Self> {
        (offers > 0 || auto_cleaned > 0).then_some(Self {
            offers,
            auto_cleaned,
            auto_cleaned_undo,
            kept,
            kept_reasons,
        })
    }

    /// Its height, unscaled.
    pub(super) fn height(&self) -> f32 {
        let lines = if self.offers > 0 { 2.0 } else { 0.0 }
            + if self.offers > 0 && self.kept > 0 {
                1.0
            } else {
                0.0
            }
            + if self.auto_cleaned > 0 { 1.0 } else { 0.0 };
        lines * BANNER_LINE_HEIGHT + 2.0 * BANNER_PADDING_Y
    }
}

fn worktrees(count: u16) -> String {
    if count == 1 {
        "1 merged worktree".to_string()
    } else {
        format!("{count} merged worktrees")
    }
}

impl GhostexGpuiApp {
    /// The banner under `coordinator`'s threads; `left` lines it up with the thread titles and
    /// `passing` draws the outer tree lines through it (threads.rs).
    pub(super) fn render_cleanup_banner(
        &self,
        coordinator: &str,
        banner: CleanupBanner,
        left: f32,
        passing: Vec<AnyElement>,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let span = self.native_sidebar_tooltip_span(scale);
        let tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let purple: Hsla = rgb(MERGED_PURPLE).into();
        let chip = |key: &str,
                    icon: &'static str,
                    color: Hsla,
                    label: Option<&str>,
                    answer: &'static str,
                    tooltip: String,
                    cx: &mut gpui::Context<Self>| {
            let command = json!({
                "type": "answerWorkCleanupThreads",
                "sessionId": coordinator,
                "answer": answer,
            });
            let foreground = appearance.foreground;
            let tip = tooltip.clone();
            div()
                .id(gpui::SharedString::from(format!(
                    "native-sidebar-cleanup-{key}-{coordinator}"
                )))
                .role(gpui::Role::Button)
                .aria_label(tooltip)
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(3.0 * scale))
                .cursor_pointer()
                .hover(move |style| {
                    style
                        .text_color(foreground)
                        .text_decoration_1()
                        .text_decoration_color(foreground)
                })
                .child(titlebar_svg_icon(icon, 12.0 * scale, color))
                .when_some(label, |chip, label| chip.child(label.to_string()))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.dispatch_native_sidebar_ui(command.clone(), cx);
                }))
                .when(tooltips, |chip| {
                    chip.managed_discrete_tooltip_with_placement(
                        span.placement(),
                        appearance.tooltip_delay,
                        move |window, cx| {
                            super::tooltips::sidebar_tooltip(tip.clone(), span, scale, window, cx)
                        },
                    )
                })
                .into_any_element()
        };
        let line = || {
            h_flex()
                .h(px(BANNER_LINE_HEIGHT * scale))
                .min_w_0()
                .gap(px(10.0 * scale))
                .items_center()
        };
        let label = |text: String| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text)
        };
        let mut lines: Vec<AnyElement> = Vec::new();
        if banner.offers > 0 {
            let count = banner.offers;
            lines.push(
                line()
                    .gap(px(4.0 * scale))
                    .child(div().flex_shrink_0().child(titlebar_svg_icon(
                        "titlebar/git-merge.svg",
                        12.0 * scale,
                        purple,
                    )))
                    .child(label(worktrees(count)))
                    .into_any_element(),
            );
            // What the pass that cleans up by itself kept, and why (CDXC:WorkMode 2026-11-20 in
            // server/src/worktree_sessions.rs).
            if banner.kept > 0 {
                lines.push(
                    line()
                        .child(label(format!(
                            "{} kept: {}",
                            banner.kept,
                            ghostex_gx_core::CleanupKept::all_in(banner.kept_reasons)
                                .into_iter()
                                .map(ghostex_gx_core::CleanupKept::words)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )))
                        .into_any_element(),
                );
            }
            lines.push(
                line()
                    .child(chip(
                        "all",
                        "titlebar/archive.svg",
                        purple,
                        Some("Clean up all"),
                        "cleanUp",
                        if count == 1 {
                            "Remove the worktree of this thread and put it to sleep. A worktree with uncommitted changes is kept.".to_string()
                        } else {
                            format!(
                                "Remove the worktrees of these {count} threads and put them to sleep. A worktree with uncommitted changes is kept."
                            )
                        },
                        cx,
                    ))
                    .child(chip(
                        "keep",
                        "titlebar/x.svg",
                        appearance.muted,
                        Some("Keep"),
                        "keep",
                        if count == 1 {
                            "Keep this thread and its worktree. Ghostex won't ask again for this PR."
                        } else {
                            "Keep these threads and their worktrees. Ghostex won't ask again for these PRs."
                        }
                        .to_string(),
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        if banner.auto_cleaned > 0 {
            let count = banner.auto_cleaned;
            lines.push(
                line()
                    .child(label(if count == 1 {
                        "Cleaned up 1 worktree".to_string()
                    } else {
                        format!("Cleaned up {count} worktrees")
                    }))
                    .when(banner.auto_cleaned_undo > 0, |line| {
                        line.child(chip(
                            "undo",
                            "titlebar/arrow-back-up.svg",
                            appearance.muted,
                            Some("Undo"),
                            "undo",
                            "Put the worktrees back on their branches and keep them.".to_string(),
                            cx,
                        ))
                    })
                    .child(chip(
                        "dismiss",
                        "titlebar/x.svg",
                        appearance.muted,
                        None,
                        "dismiss",
                        "Hide this line. The threads stay asleep under the coordinator."
                            .to_string(),
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        v_flex()
            .id(gpui::SharedString::from(format!(
                "native-sidebar-cleanup-banner-{coordinator}"
            )))
            .relative()
            .mx(px(super::session_list::SESSION_INSET_X * scale))
            .mb(px(super::session_list::SESSION_SPACING * scale))
            .py(px(BANNER_PADDING_Y * scale))
            .pl(px(left * scale))
            .pr(px(6.0 * scale))
            .min_w_0()
            .text_size(px(11.5 * scale))
            .text_color(appearance.muted)
            .children(passing)
            .children(lines)
            .into_any_element()
    }
}
