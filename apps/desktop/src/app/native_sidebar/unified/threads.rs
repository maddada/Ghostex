//! A coordinator's threads under its Unified card: the crew count that folds them, the dots a
//! folded card shows for them, and the one-line thread rows and "N older threads" row drawn under
//! an unfolded card.
//!
//! SEE-ALSO: CDXC:Sidebar 2026-11-09 in packages/gx-core/src/unified_sidebar/tree.rs (which rows
//! nest, fold, list and lift), native_sidebar/threads.rs (the Project sidebar's tree look reused
//! here: indent, tree-line colour, crew badge, older-threads wording).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::{h_flex, v_flex};
use serde_json::Value;

use super::cards::{mark_active, now_ms, status_slot};
use super::model::{UnifiedCard, UnifiedCardLine};
use crate::GhostexGpuiApp;
use crate::app::helpers::chrome_color;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::status;
use crate::app::native_sidebar::threads::{
    THREAD_INDENT, contains_active_marker, coordinator_icon, crew_badge, crew_tint,
};

/// A thread row's height, unscaled.
const THREAD_ROW_HEIGHT: f32 = 26.0;

/// What a click on the crew count does: "Hide threads" when the user unfolded it; "Keep threads
/// open" while a search holds it open (the click saves it unfolded); otherwise what the unfold
/// shows: "Show 3 threads", "Show 2 threads (5 older)", or "Show threads (20 older)" when every
/// thread waits behind the older-threads row.
fn crew_label(card: &UnifiedCard) -> Option<String> {
    let crew = card.crew.as_ref()?;
    let plural = |count: u16| if count == 1 { "thread" } else { "threads" };
    Some(if crew.unfolded {
        "Hide threads".to_string()
    } else if crew.open {
        "Keep threads open".to_string()
    } else if crew.listed == 0 {
        format!("Show threads ({} older)", crew.older)
    } else if crew.older > 0 && !crew.older_shown {
        format!(
            "Show {} {} ({} older)",
            crew.listed,
            plural(crew.listed),
            crew.older
        )
    } else {
        format!("Show {} {}", crew.total, plural(crew.total))
    })
}

/// The overlapping dots of everything a folded card hides, with its own status (gx-core
/// `UnifiedCrew::folded_status`), in its status dot's spot; its own working timer stays to the
/// left so the dots never move.
///
/// CDXC:Coordinators 2026-10-24 SEE-ALSO: the user's decision on gx-core `RowNesting::folded_status` ("show 2 dots (yellow and blue) in this case (they need to be overlapping dots pls)"), which a folded Unified card follows.
pub(super) fn folded_slot(
    card: &UnifiedCard,
    appearance: &SidebarAppearance,
) -> Option<AnyElement> {
    let crew = card.crew.as_ref().filter(|crew| !crew.open)?;
    let folded = crew.folded_status;
    if !folded.any() {
        return None;
    }
    let scale = appearance.scale;
    let timer = (card.status == Some(ghostex_gx_core::unified_sidebar::UnifiedStatus::Working))
        .then(|| card.row.timing.working_started_ms)
        .flatten()
        .map(|started| super::cards::elapsed_label(now_ms() - started));
    Some(
        h_flex()
            .id("unified-card-folded-status")
            .flex_shrink_0()
            .gap(px(4.0 * scale))
            .text_size(px(12.5 * scale))
            .text_color(appearance.muted)
            .children(timer.map(|timer| div().opacity(0.85).child(timer)))
            .child(status::folded_status_indicator(
                folded.working,
                folded.attention,
                folded.background_work,
                folded.question,
                folded.approval,
                scale,
                status::session_row_ring(appearance, false, false, None, false),
            ))
            .into_any_element(),
    )
}

impl GhostexGpuiApp {
    /// The crew count of a row with threads under it: the toggle that folds them. Drawn whenever
    /// the row has threads, grey while none of them works or waits (CDXC:Coordinators 2026-11-09 in
    /// native_sidebar/threads.rs). A coordinator with none nested under it (its threads parked,
    /// snoozed or closed) draws none: what it would count is on screen as cards, and the phone
    /// draws none either.
    pub(super) fn render_unified_crew_toggle(
        &self,
        card: &UnifiedCard,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let crew = card.crew.as_ref()?;
        let scale = appearance.scale;
        let tone = crew.badge.tone;
        let count = if tone == ghostex_gx_core::CoordinatorBadgeTone::Idle {
            crew.total
        } else {
            crew.badge.count
        };
        let label = crew_label(card).unwrap_or_default();
        let key = crew.key.clone();
        let span = crate::app::native_sidebar::tooltips::SidebarTooltipSpan::sidebar(
            self.sidebar_width,
            scale,
        );
        let tooltip = label.clone();
        Some(
            h_flex()
                .id(gpui::SharedString::from(format!(
                    "native-unified-crew-{}",
                    card.session.session_id
                )))
                .role(gpui::Role::Button)
                .aria_label(label)
                .aria_expanded(crew.open)
                .flex_shrink_0()
                .gap(px(4.0 * scale))
                .px(px(3.0 * scale))
                .rounded(px(4.0 * scale))
                .cursor_pointer()
                .hover(|style| style.bg(appearance.session_hover))
                .child(crew_badge(
                    u64::from(count),
                    crew_tint(tone, appearance),
                    appearance,
                ))
                .when(crew.contains_active, |toggle| {
                    toggle.child(contains_active_marker(appearance))
                })
                // A press on the count is not a press on the card: the card would open its session.
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.gx_store_toggle_unified_coordinator(&key, false, cx);
                }))
                .managed_discrete_tooltip_with_placement(
                    span.placement(),
                    appearance.tooltip_delay,
                    move |window, cx| {
                        crate::app::native_sidebar::tooltips::sidebar_tooltip(
                            tooltip.clone(),
                            span,
                            scale,
                            window,
                            cx,
                        )
                    },
                )
                .into_any_element(),
        )
    }

    /// The rows drawn under an unfolded card: its threads, depth first, each coordinator's
    /// "N older threads" row after its own. `None` while nothing is drawn under it.
    pub(super) fn render_unified_thread_lines(
        &self,
        card: &UnifiedCard,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if card.lines.is_empty() {
            return None;
        }
        let scale = appearance.scale;
        let lines: Vec<AnyElement> = card
            .lines
            .iter()
            .map(|line| match line {
                UnifiedCardLine::Thread {
                    card: thread,
                    depth,
                    branch,
                    pull_request,
                    machine,
                } => self.render_unified_thread_row(
                    thread,
                    *depth,
                    branch.as_deref(),
                    *pull_request,
                    machine.as_deref(),
                    hud,
                    appearance,
                    cx,
                ),
                UnifiedCardLine::Older {
                    key,
                    older,
                    shown,
                    depth,
                } => self.render_unified_older_row(key, *older, *shown, *depth, appearance, cx),
            })
            .collect();
        // One tree line down the block, in the Project sidebar's tree colour: the list is not
        // virtualised, so the block draws it whole instead of each row drawing its piece.
        // The inset is padding on a full-width row, so the block never runs past the card's edge.
        Some(
            div()
                .w_full()
                .pl(px((3.0 + 12.0) * scale))
                .pr(px(3.0 * scale))
                .mb(px(4.0 * scale))
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .pl(px(2.0 * scale))
                        .border_l(px(1.0 * scale))
                        .border_color(chrome_color(0x4a4a4a, 0xc8c8c8))
                        .children(lines),
                )
                .into_any_element(),
        )
    }

    /// One thread: its agent's icon (a sub-coordinator's crown), title, what differs from its card
    /// (pin, computer, branch, pull request), its crew count when it has threads of its own, and
    /// its status or folded dots on the right edge, where the card's dot sits.
    #[allow(clippy::too_many_arguments)]
    fn render_unified_thread_row(
        &self,
        thread: &UnifiedCard,
        depth: u8,
        branch: Option<&str>,
        pull_request: Option<u64>,
        machine: Option<&str>,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let session = &thread.session;
        let now = now_ms();
        let (focused, _) = self.gx_store_sidebar_row_focus(
            &session.session_id,
            false,
            session.is_focused,
            session.is_visible,
        );
        let focused = focused
            || thread
                .crew
                .as_ref()
                .is_some_and(|crew| crew.contains_active);
        // No hover strip on a thread row: it would push the crew count left under the pointer, and
        // the row's right-click menu holds the same actions.
        let right = folded_slot(thread, appearance)
            .or_else(|| status_slot(thread, appearance, now))
            .unwrap_or_else(|| {
                div()
                    .flex_shrink_0()
                    .text_size(px(12.5 * scale))
                    .text_color(chrome_color(0xa6a6a6, 0x424242))
                    .child(
                        thread
                            .row
                            .last_interaction_label(now.max(0) as u64)
                            .unwrap_or_default(),
                    )
                    .into_any_element()
            });
        let icon = if thread.crew.is_some() || thread.row.is_coordinator {
            coordinator_icon(appearance, false)
        } else {
            crate::app::native_sidebar::icons::session_icon(session, hud, appearance, false, false)
        };
        let meta = |text: String| {
            div()
                .flex_shrink_0()
                .max_w(px(110.0 * scale))
                .truncate()
                .text_size(px(11.5 * scale))
                .text_color(appearance.muted)
                .child(text)
        };
        let row = h_flex()
            .id(gpui::SharedString::from(format!(
                "native-unified-thread-{}",
                session.session_id
            )))
            .role(gpui::Role::TreeItem)
            .aria_label(session.title().to_owned())
            .aria_selected(focused)
            .w_full()
            .h(px(THREAD_ROW_HEIGHT * scale))
            .pl(px((6.0
                + f32::from(depth.saturating_sub(1)) * THREAD_INDENT)
                * scale))
            .pr(px(8.0 * scale))
            .gap(px(6.0 * scale))
            .rounded(px(5.0 * scale))
            .relative()
            .when(!focused, |row| {
                row.hover(|row| row.bg(appearance.session_hover))
            })
            .child(div().flex_shrink_0().opacity(0.75).child(icon))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if focused {
                        gpui::Hsla::from(chrome_color(0xd8d8d8, 0x292929))
                    } else {
                        appearance.foreground
                    })
                    .child(session.title().to_owned()),
            )
            .when(session.is_pinned, |row| {
                row.child(
                    gpui::svg()
                        .path("titlebar/pin-filled.svg")
                        .size(px(12.0 * scale))
                        .flex_shrink_0()
                        .text_color(appearance.muted),
                )
            })
            .when_some(machine, |row, machine| row.child(meta(machine.to_owned())))
            .when_some(branch, |row, branch| {
                row.child(
                    h_flex()
                        .flex_shrink(1.0)
                        .min_w_0()
                        .max_w(px(110.0 * scale))
                        .gap(px(3.0 * scale))
                        .child(
                            gpui::svg()
                                .path("titlebar/git-branch.svg")
                                .size(px(11.0 * scale))
                                .flex_shrink_0()
                                .text_color(appearance.muted),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.5 * scale))
                                .text_color(appearance.muted)
                                .child(branch.to_owned()),
                        ),
                )
            })
            .when_some(pull_request, |row, number| {
                row.child(meta(format!("#{number}")))
            })
            .children(self.render_unified_crew_toggle(thread, appearance, cx))
            .child(right)
            .when(focused, |row| {
                mark_active(
                    row,
                    appearance.session_selected,
                    crate::app::native_sidebar::decorations::session_outline(appearance),
                )
            });
        div()
            .w_full()
            .pb(px(1.0 * scale))
            .child(self.unified_row_interactions(row, thread, appearance, cx))
            .into_any_element()
    }

    /// The quiet row that lists a coordinator's older threads, or tucks them away again, worded as
    /// the Project sidebar's (`native_sidebar/threads.rs`).
    fn render_unified_older_row(
        &self,
        key: &str,
        older: u16,
        shown: bool,
        depth: u8,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let label = match (shown, older) {
            (true, _) => "Hide older threads".to_string(),
            (false, 1) => "1 older thread".to_string(),
            (false, count) => format!("{count} older threads"),
        };
        let key = key.to_string();
        div()
            .id(gpui::SharedString::from(format!(
                "native-unified-older-threads-{key}"
            )))
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .h(px(22.0 * scale))
            // Lines up with the thread titles: the row's inset, its icon and the gap after it.
            .pl(px((6.0
                + f32::from(depth.saturating_sub(1)) * THREAD_INDENT
                + 13.0
                + 6.0)
                * scale))
            .mb(px(1.0 * scale))
            .flex()
            .items_center()
            .rounded(px(5.0 * scale))
            .text_size(px(11.5 * scale))
            .text_color(appearance.muted)
            .cursor_pointer()
            .hover(|style| style.bg(appearance.session_hover))
            .child(label)
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.gx_store_toggle_unified_coordinator(&key, true, cx);
            }))
            .into_any_element()
    }
}
