//! A coordinator's threads under its Unified card, drawn with the Project sidebar's own rows: its
//! session row (tree line, status, hover buttons, badges) for each thread and its "N older
//! threads" row, plus the crew count a card shows and the dots a folded card shows for them.
//!
//! SEE-ALSO: CDXC:Sidebar 2026-11-11 in packages/gx-core/src/unified_sidebar/tree.rs (which rows
//! nest, fold, list and lift), gx-core `lay_out_tree` (the tree layout both sidebars share),
//! native_sidebar/sessions.rs and threads.rs (the rows drawn here).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::Value;

use super::cards::now_ms;
use super::model::{UnifiedCard, UnifiedCardLine};
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::focus_order::SidebarStopPlace;
use crate::app::native_sidebar::model::NativeSidebarSession;
use crate::app::native_sidebar::session_list::{SESSION_HEIGHT, session_card_height};
use crate::app::native_sidebar::status;
use crate::app::native_sidebar::threads::{
    TreeAnchor, coordinator_fold_state, crew_badge, crew_tint, unified_clock_owns_fold,
    unified_fold_key,
};

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

/// The thread's row with its time labels read now from its session, as a card reads its own: the
/// row was built with the list, and the Project list's relabel pass refreshes only its own rows.
fn with_live_labels(thread: &UnifiedCard) -> std::sync::Arc<NativeSidebarSession> {
    let now = now_ms().max(0) as u64;
    let labels = [
        ("timerLabel", thread.row.timer_label(now)),
        (
            "lastInteractionLabel",
            thread.row.last_interaction_label(now),
        ),
    ];
    let stale = labels.iter().any(|(key, label)| {
        thread.session.details.get(*key).and_then(Value::as_str) != label.as_deref()
    });
    if !stale {
        return thread.session.clone();
    }
    let mut session = (*thread.session).clone();
    for (key, label) in labels {
        match label {
            Some(label) => {
                session.details.insert(key.to_string(), Value::from(label));
            }
            None => {
                session.details.remove(key);
            }
        }
    }
    std::sync::Arc::new(session)
}

impl GhostexGpuiApp {
    /// The crew count of a Unified row with threads under it: always drawn on a card, grey while
    /// none works or waits, as information (CDXC:Coordinators 2026-11-11 WHY in
    /// native_sidebar/threads.rs). While a pending clock owns the row's hat, it is the fold toggle
    /// instead, so the row stays unfoldable. `None` on a row with no threads nested under it.
    pub(crate) fn render_unified_crew_count(
        &self,
        session: &NativeSidebarSession,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let key = unified_fold_key(session)?.to_owned();
        let collapsed = coordinator_fold_state(session)?;
        let threads = &session.details["coordinatorThreads"];
        let count = threads["count"].as_u64().unwrap_or(0);
        let tone = match threads["tone"].as_str() {
            Some("approval") => ghostex_gx_core::CoordinatorBadgeTone::Approval,
            Some("waiting") => ghostex_gx_core::CoordinatorBadgeTone::Waiting,
            Some("working") => ghostex_gx_core::CoordinatorBadgeTone::Working,
            _ => ghostex_gx_core::CoordinatorBadgeTone::Idle,
        };
        let badge = crew_badge(count, crew_tint(tone, appearance), appearance);
        if !unified_clock_owns_fold(session) {
            return Some(badge.into_any_element());
        }
        let scale = appearance.scale;
        let label = if collapsed {
            "Show threads"
        } else {
            "Hide threads"
        };
        let span = crate::app::native_sidebar::tooltips::SidebarTooltipSpan::sidebar(
            self.sidebar_width,
            scale,
        );
        Some(
            h_flex()
                .id(gpui::SharedString::from(format!(
                    "native-unified-crew-{}",
                    session.session_id
                )))
                .role(gpui::Role::Button)
                .aria_label(label)
                .aria_expanded(!collapsed)
                .flex_shrink_0()
                .px(px(3.0 * scale))
                .rounded(px(4.0 * scale))
                .cursor_pointer()
                .hover(|style| style.bg(appearance.session_hover))
                .child(badge)
                // A press on the count is not a press on the row: the row would open its session.
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.gx_store_toggle_unified_coordinator(&key, false, cx);
                }))
                // After the mouse-down above, so a press moves focus here before it stops.
                .map(|toggle| {
                    self.sidebar_focus_stop(
                        toggle,
                        format!("unified-crew:{}", session.session_id),
                        SidebarStopPlace::List,
                        appearance,
                        cx,
                    )
                })
                .managed_discrete_tooltip_with_placement(
                    span.placement(),
                    appearance.tooltip_delay,
                    move |window, cx| {
                        crate::app::native_sidebar::tooltips::sidebar_tooltip(
                            label.to_string(),
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

    /// The rows drawn under an unfolded card, depth first, as the Project sidebar draws a
    /// coordinator's: its session row for each thread and each coordinator's "N older threads" row
    /// after its own threads. `first` is where the card's hat sits, which the first thread's tree
    /// line rises to. `None` while nothing is drawn under it.
    pub(super) fn render_unified_thread_lines(
        &self,
        card: &UnifiedCard,
        first: TreeAnchor,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if card.lines.is_empty() {
            return None;
        }
        let mut anchor = first;
        let mut rows = Vec::with_capacity(card.lines.len());
        for line in &card.lines {
            match line {
                UnifiedCardLine::Thread {
                    card: thread,
                    group,
                } => {
                    let session = &with_live_labels(thread);
                    let row = self.render_native_sidebar_session(
                        group,
                        session,
                        anchor,
                        crate::app::native_sidebar::sessions::SessionRowPlace::UnifiedThread,
                        hud,
                        appearance,
                        cx,
                    );
                    // The Project list tracks hover with its own probes; here the row does.
                    let hover_id = session.session_id.clone();
                    // A row with links opens the floating links card on hover, as a card does.
                    let opens_links_card = self.native_session_links_row(session).hover_card;
                    rows.push(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "native-unified-thread-{}",
                                session.session_id
                            )))
                            .w_full()
                            .on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
                                let current = app.native_sidebar.hovered_session.as_deref()
                                    == Some(hover_id.as_str());
                                if *hovered && !current {
                                    app.native_sidebar.hovered_session = Some(hover_id.clone());
                                    if opens_links_card {
                                        app.open_session_links_card_after_delay(
                                            hover_id.clone(),
                                            cx,
                                        );
                                    }
                                    cx.notify();
                                } else if !*hovered && current {
                                    app.native_sidebar.hovered_session = None;
                                    cx.notify();
                                }
                            }))
                            .child(row)
                            .into_any_element(),
                    );
                    anchor = TreeAnchor::below_row(session_card_height(
                        session,
                        self.native_session_has_second_line(session),
                    ));
                }
                UnifiedCardLine::Older(older) => {
                    rows.push(self.render_older_threads_row(older.clone(), appearance, cx));
                    // As the Project list starts the rows after an older-threads row.
                    anchor = TreeAnchor::below_row(SESSION_HEIGHT);
                }
            }
        }
        Some(div().w_full().children(rows).into_any_element())
    }
}
