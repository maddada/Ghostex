use std::sync::Arc;

use gpui::{AnyElement, Bounds, IntoElement, Styled, canvas, point, px, size};

use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarGroup, NativeSidebarSession},
    session_hover::SessionHoverProbes,
};
use crate::GhostexGpuiApp;

pub(super) const SESSION_HEIGHT: f32 = 34.0;
/// A work-mode card with links: line 1 exactly as tall as every other card, then the chips.
pub(super) const WORK_SESSION_HEIGHT: f32 = 52.0;
pub(super) const SESSION_SPACING: f32 = 1.0;
/// Horizontal gap between the list edge and a session card's rounded body.
pub(super) const SESSION_INSET_X: f32 = 3.0;

/// A card's height before scaling: taller only when it draws a second line, its links or a Work
/// mode card's Clean up / Keep offer (`native_session_has_second_line`, link_chips.rs).
pub(super) fn session_card_height(_session: &NativeSidebarSession, second_line: bool) -> f32 {
    if second_line {
        WORK_SESSION_HEIGHT
    } else {
        SESSION_HEIGHT
    }
}

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-17 WHY:
    /// Spinner frames rebuilt and laid out every expanded session, including offscreen rows, starving scroll input.
    /// Keep the full list height in the existing scroll container but construct rows only inside its paint mask.
    /// Resolve reveal requests from logical row bounds so offscreen sessions remain reachable.
    ///
    /// CDXC:WorkMode 2026-10-09 WHY:
    /// A work-mode card with links is taller than the others, so each row's top is a running sum
    /// of the heights above it and the visible range is found by searching those tops; every other
    /// list is unchanged, since all its rows are still `SESSION_HEIGHT`.
    pub(super) fn render_native_session_list(
        &self,
        group: &NativeSidebarGroup,
        sessions: Vec<Arc<NativeSidebarSession>>,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let snapshot = self.native_sidebar.snapshot.clone();
        let group_id = group.group_id.clone();
        let appearance = appearance.clone();
        let scale = appearance.scale;
        let card_heights: Vec<f32> = sessions
            .iter()
            .map(|session| {
                session_card_height(session, self.native_session_has_second_line(session))
            })
            .collect();
        // Each row's top in unscaled pixels, plus the list's total height as the last entry.
        let mut tops: Vec<f32> = Vec::with_capacity(sessions.len() + 1);
        let mut y = 0.0;
        for height in &card_heights {
            tops.push(y);
            y += height + SESSION_SPACING;
        }
        tops.push(y);
        let height = px(y * scale);
        let view = cx.entity();
        let hover_view = view.clone();
        canvas(
            move |bounds, window, cx| {
                let top_of = |index: usize| px(tops[index] * scale);
                let card_of = |index: usize| px(card_heights[index] * scale);
                let mut rows = 'rows: {
                    let Some(snapshot) = snapshot else {
                        break 'rows Vec::new();
                    };
                    let Some(group) = snapshot
                        .groups
                        .iter()
                        .find(|group| group.group_id == group_id)
                    else {
                        break 'rows Vec::new();
                    };
                    let mask = window.content_mask().bounds.intersect(&bounds);
                    view.update(cx, |app, cx| {
                        if let Some(request) = &app.native_sidebar.pending_reveal
                            && let Some(index) = sessions
                                .iter()
                                .position(|session| session.session_id == request.session_id)
                        {
                            let id = request.session_id.clone();
                            app.reveal_native_session_bounds(
                                &id,
                                Bounds::new(
                                    bounds.origin + point(px(0.0), top_of(index)),
                                    size(bounds.size.width, card_of(index)),
                                ),
                                scale,
                                window,
                                cx,
                            );
                        }
                        let built = if mask.size.width <= px(0.0) || mask.size.height <= px(0.0) {
                            0..0
                        } else {
                            let mask_top = f32::from(mask.top() - bounds.top()) / scale;
                            let mask_bottom = f32::from(mask.bottom() - bounds.top()) / scale;
                            let row_tops = &tops[..sessions.len()];
                            // The last row starting at or above the mask's top, and every row that
                            // starts above its bottom.
                            let first = row_tops
                                .partition_point(|top| *top <= mask_top)
                                .saturating_sub(1);
                            let end = row_tops.partition_point(|top| *top < mask_bottom);
                            first..end
                        };
                        // The rows left unbuilt are still in the sidebar's focus order: Tab and
                        // Shift+Tab scroll to them (focus_order.rs).
                        for index in (0..sessions.len()).filter(|index| !built.contains(index)) {
                            let session = &sessions[index];
                            let first = super::focus_order::session_stop_key(&session.session_id);
                            // A coordinator's fold button is the row's last stop (decorations.rs).
                            let last = super::decorations::coordinator_fold_button(session)
                                .map(|_| {
                                    super::focus_order::coordinator_fold_stop_key(
                                        &session.session_id,
                                    )
                                })
                                .unwrap_or_else(|| first.clone());
                            app.native_sidebar.focus_stops.record_unbuilt_row(
                                super::focus_order::UnbuiltRow {
                                    first,
                                    last,
                                    bounds: Bounds::new(
                                        bounds.origin + point(px(0.0), top_of(index)),
                                        size(bounds.size.width, card_of(index)),
                                    ),
                                },
                            );
                        }
                        built
                            .map(|index| {
                                (
                                    index,
                                    app.render_native_sidebar_session(
                                        group,
                                        &sessions[index],
                                        index
                                            .checked_sub(1)
                                            .map_or(SESSION_HEIGHT, |above| card_heights[above]),
                                        &snapshot.hud,
                                        &appearance,
                                        cx,
                                    ),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                };
                let inset = px(SESSION_INSET_X * scale);
                let hover = SessionHoverProbes::insert(
                    sessions,
                    rows.iter().map(|(index, _)| {
                        (
                            *index,
                            Bounds::new(
                                bounds.origin + point(inset, top_of(*index)),
                                size(bounds.size.width - inset * 2.0, card_of(*index)),
                            ),
                        )
                    }),
                    window,
                );
                for (index, row) in &mut rows {
                    row.layout_as_root(
                        size(
                            bounds.size.width,
                            card_of(*index) + px(SESSION_SPACING * scale),
                        )
                        .into(),
                        window,
                        cx,
                    );
                    row.prepaint_at(bounds.origin + point(px(0.0), top_of(*index)), window, cx);
                }
                (rows, hover)
            },
            move |_, (rows, hover), window, cx| {
                for (_, mut row) in rows {
                    row.paint(window, cx);
                }
                hover.track(hover_view, window, cx);
            },
        )
        .w_full()
        .h(height)
        .flex_shrink_0()
        .into_any_element()
    }
}
