//! Opens Drafts when a new session lands there, and scrolls to it once it is the open session
//! (CDXC:Drafts 2026-11-02 in packages/gx-core/src/sidebar_view/draft_arrivals.rs). Both sidebars:
//! the Project sidebar's Drafts heading of the row's project, and the Unified sidebar's Drafts
//! shelf. The web build compiles this file too.

use ghostex_gx_core::{
    DraftArrivals, DraftRow, SessionKey, open_draft_headings, project_draft_rows, reveal_plan,
};

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::model::NativeSidebarRevealRequest;

/// The Unified sidebar's Drafts shelf, by the key its open or folded state is kept under
/// (`native_sidebar/unified/shelves.rs`).
const UNIFIED_DRAFTS_SHELF_KEY: &str = "Drafts";

/// One tracker per sidebar: each reads its own list, and the one not drawn is reset so switching
/// to it records its drafts before anything counts as new.
#[derive(Default)]
pub(crate) struct DraftArrivalsHost {
    project: DraftArrivals,
    unified: DraftArrivals,
}

impl GhostexGpuiApp {
    /// Runs after every rebuild of the drawn list, once it is ready.
    pub(crate) fn gx_store_open_arrived_drafts(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.gx_store_sidebar_list_ready() {
            return;
        }
        let now_ms = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or_default();
        let focused = self
            .gx_store
            .core
            .focus()
            .focused_session
            .as_ref()
            .map(SessionKey::to_sidebar_session_id);
        match self.native_sidebar.unified.clone() {
            Some(unified) => {
                self.gx_store.draft_arrivals.project.reset();
                let drafts: Vec<DraftRow> = unified
                    .drafts
                    .iter()
                    .map(|card| DraftRow {
                        sidebar_session_id: card.session.session_id.clone(),
                        storage_id: String::new(),
                        created_ms: card.row.timing.created_ms,
                    })
                    .collect();
                let tracker = &mut self.gx_store.draft_arrivals.unified;
                let arrived = !tracker.observe(&drafts, now_ms).is_empty();
                let reveal = tracker.take_focused(focused.as_deref(), now_ms);
                let host = &mut self.gx_store.unified_sidebar;
                if arrived && host.saved.toggled_shelves.remove(UNIFIED_DRAFTS_SHELF_KEY) {
                    host.persist();
                }
                if let Some(sidebar_session_id) = reveal {
                    self.scroll_sidebar_to_arrived_draft(sidebar_session_id);
                }
                if arrived {
                    cx.notify();
                }
            }
            None => {
                self.gx_store.draft_arrivals.unified.reset();
                let inputs = &self.gx_store.sidebar_list.last_inputs;
                let drafts = project_draft_rows(
                    self.gx_store.sidebar_list.view(),
                    inputs.settings.enable_session_parking,
                    now_ms,
                );
                let tracker = &mut self.gx_store.draft_arrivals.project;
                let arrivals = tracker.observe(&drafts, now_ms);
                let reveal = tracker.take_focused(focused.as_deref(), now_ms);
                let mut intents = open_draft_headings(&arrivals, self.gx_store.sidebar_ui.state());
                // The open arrival is put on screen the way a notification click puts its row
                // there: its group, its collection and its Space too, not only the heading.
                if let Some(sidebar_session_id) = &reveal {
                    let store = &self.gx_store;
                    if let Some(plan) = reveal_plan(
                        &store.core,
                        &store.sidebar_list.last_inputs,
                        store.sidebar_list.view(),
                        sidebar_session_id,
                        now_ms,
                    ) {
                        intents.extend(plan.intents(store.sidebar_ui.state()));
                    }
                }
                // Each intent rebuilds the list, which runs this again: the tracker has already
                // recorded these drafts, so that pass finds nothing new.
                for intent in intents {
                    self.gx_store_apply_sidebar_ui_intent(intent, cx);
                }
                if let Some(sidebar_session_id) = reveal {
                    self.scroll_sidebar_to_arrived_draft(sidebar_session_id);
                }
            }
        }
    }

    /// Scrolls the row into view the next time it is drawn, as a previous or next session key
    /// does (`gx_store/session_walk.rs`).
    fn scroll_sidebar_to_arrived_draft(&mut self, sidebar_session_id: String) {
        self.native_sidebar.scroll_animation = None;
        self.native_sidebar.pending_reveal = Some(NativeSidebarRevealRequest {
            session_id: sidebar_session_id,
            // Only reveal requests of the sidebar snapshot are numbered.
            request_id: 0,
        });
    }
}
