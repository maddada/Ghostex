//! The lanes' own view entity. The app embeds it as a cached view, so the cards are only
//! rebuilt when something they show changed, not on every app render (terminal output,
//! chat streaming, indicator animation). The rest of the board is drawn in the app's render.
//!
//! CDXC:ProjectBoard 2026-09-23 WHY:
//! The board used to render inline in the app's render, which re-filtered every ticket and rebuilt
//! every card on each app frame and made the whole app visibly slower while Kanban was on screen.
//! Board state still lives on the app (the Beads bridge, conversation routing and toasts are app
//! methods), so this view's render borrows the app, which GPUI allows because a child view renders
//! after the parent's render lease ends. Every board change must go through
//! `native_kanban_notify`; a plain `cx.notify()` on the app does not redraw the cached board.

use gpui::{
    AnyElement, App, AppContext as _, Context, IntoElement, Render, Styled as _, WeakEntity,
    Window, div,
};

use super::derived::KanbanDerived;
use super::filters::{filter_tickets, sort_tickets, tag_options};
use super::state::NativeKanbanState;
use crate::GhostexGpuiApp;

pub(crate) struct NativeKanbanView {
    app: WeakEntity<GhostexGpuiApp>,
}

impl Render for NativeKanbanView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app
            .update(cx, |app, cx| app.render_native_kanban_lanes(window, cx))
            .unwrap_or_else(|_| div().size_full().into_any_element())
    }
}

impl NativeKanbanState {
    /// The toolbar and lanes read tickets through this cache; anything that changes the tickets,
    /// columns, search or filters calls `invalidate_derived`.
    pub(crate) fn invalidate_derived(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Filtered, sorted ticket indices per lane and the tag options, rebuilt only when the
    /// revision moved.
    pub(crate) fn derived(&mut self) -> &KanbanDerived {
        if self
            .derived
            .as_ref()
            .is_none_or(|derived| derived.revision != self.revision)
        {
            let tags = tag_options(&self.tickets);
            let active_tag = self.view.active_tag(&tags).to_string();
            let filtered =
                filter_tickets(&self.tickets, &self.search_query, &self.view, &active_tag);
            let lanes = self
                .columns
                .iter()
                .map(|column| {
                    let mut indices = filtered
                        .iter()
                        .copied()
                        .filter(|index| self.tickets[*index].board_status == column.key)
                        .collect::<Vec<_>>();
                    sort_tickets(&mut indices, &self.tickets, self.view.sort, &column.key);
                    indices
                })
                .collect();
            let filter_count = self.view.active_count(&tags);
            let nothing_matches = !self.tickets.is_empty() && filtered.is_empty();
            self.derived = Some(KanbanDerived {
                revision: self.revision,
                lanes,
                filter_count,
                nothing_matches,
            });
        }
        self.derived.as_ref().expect("derived was just filled")
    }
}

impl GhostexGpuiApp {
    pub(crate) fn native_kanban_view_entity(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<NativeKanbanView> {
        if let Some(view) = self.native_kanban.board_view.clone() {
            return view;
        }
        crate::app::native_chat::fonts::register(cx);
        let app = cx.weak_entity();
        let view = cx.new(|_| NativeKanbanView { app });
        self.native_kanban.board_view = Some(view.clone());
        view
    }

    /// Redraws the board: the cached lanes and the app, which draws the rest. Every board state
    /// change calls this instead of `cx.notify()`.
    pub(crate) fn native_kanban_notify(&mut self, cx: &mut Context<Self>) {
        self.native_kanban.window_redraws = self.native_kanban.window_redraws.wrapping_add(1);
        if let Some(view) = self.native_kanban.board_view.as_ref() {
            App::notify(cx, view.entity_id());
        }
        cx.notify();
    }

    /// Something outside the board that it draws with changed: settings, theme, the system
    /// appearance. Drops the cached palette and redraws.
    pub(crate) fn native_kanban_notify_appearance(&mut self, cx: &mut Context<Self>) {
        self.native_kanban.palette = None;
        self.native_kanban_notify(cx);
    }

    /// The lanes, each given its tickets from the cached indices. Lanes outside `in_view` draw
    /// only their headers.
    pub(crate) fn native_kanban_lane_elements(
        &mut self,
        in_view: std::ops::Range<usize>,
        p: &super::palette::KanbanPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let lanes = self.native_kanban.derived().lanes.clone();
        let state = &mut self.native_kanban;
        let loading = state.loading_first();
        let measure_key = {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            state.revision.hash(&mut hasher);
            state.card_view.hash(&mut hasher);
            p.font.hash(&mut hasher);
            for link in &state.conversation.links {
                link.bead_id.hash(&mut hasher);
                (link.openable || link.resumable, link.live).hash(&mut hasher);
            }
            hasher.finish()
        };
        let columns = state.columns.clone();
        state
            .lane_lists
            .retain(|key, _| columns.iter().any(|column| column.key == *key));
        columns
            .iter()
            .zip(lanes)
            .enumerate()
            .map(|(position, (column, indices))| {
                if loading {
                    return self.render_native_kanban_lane(
                        column,
                        &[],
                        Some(position),
                        true,
                        measure_key,
                        p,
                        cx,
                    );
                }
                self.render_native_kanban_lane(
                    column,
                    &indices,
                    None,
                    in_view.contains(&position),
                    measure_key,
                    p,
                    cx,
                )
            })
            .collect()
    }
}
