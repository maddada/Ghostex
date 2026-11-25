//! Opens Drafts when a new session lands there, and scrolls to it once it is the open session
//! (CDXC:Drafts 2026-11-02 in packages/gx-core/src/sidebar_view/draft_arrivals.rs). Both sidebars:
//! the Project sidebar's Drafts heading of the row's project, and the Unified sidebar's Drafts
//! shelf. The web build compiles this file too. A new session that is not a draft (a terminal, a
//! fork) is put on screen once it is the open session, in the Project sidebar (CDXC:Sidebar
//! 2026-11-14 in the same gx-core file).

use ghostex_gx_core::{
    DraftArrivals, DraftRow, Event, Intent, LOCAL_MACHINE_ID, MachineId, SessionKey,
    open_draft_headings, project_draft_rows, project_session_rows, reveal_plan,
};
use serde_json::{Value, json};

use super::host::now_ms;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::model::NativeSidebarRevealRequest;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// The Unified sidebar's Drafts shelf, by the key its open or folded state is kept under
/// (`native_sidebar/unified/shelves.rs`).
const UNIFIED_DRAFTS_SHELF_KEY: &str = "Drafts";

/// A draft created by the user's new-session action that has not been typed in or modified yet.
/// When the user opens something else while it is still untouched, it is removed automatically.
#[derive(Clone, Debug)]
struct TrackedUntouchedDraft {
    session: SessionKey,
    /// Whatever session was selected before this draft opened, so a transient focus state before
    /// the brand-new draft arrives does not discard it prematurely.
    opened_from: Option<SessionKey>,
    /// True once this draft has become the selected session at least once.
    has_been_focused: bool,
}

/// One tracker per sidebar: each reads its own list, and the one not drawn is reset so switching
/// to it records its drafts before anything counts as new.
#[derive(Default)]
pub(crate) struct DraftArrivalsHost {
    project: DraftArrivals,
    unified: DraftArrivals,
    /// Every row of the Project sidebar, whatever its heading: the new one that becomes the open
    /// session is revealed.
    project_created: DraftArrivals,
    /// Blank drafts opened by the user that are auto-removed on leave if still untouched.
    untouched_drafts: Vec<TrackedUntouchedDraft>,
}

/// Whether `row` in the presentation store still looks like an untouched draft (no text, no pin,
/// park, favorite, tag, note, queue, stash, question, delayed send, Close After Done, coordinator,
/// or user/generated rename).
fn presentation_draft_is_untouched(row: &ghostex_gx_protocol::PresentationSession) -> bool {
    row.is_draft
        && !row.has_composer_draft
        && !row.is_pinned
        && !row.is_parked
        && !row.is_favorite
        && !row.close_after_done
        && row.session_tag.is_none()
        && row.session_note.is_none()
        && row.delayed_send_deadline_at.is_none()
        && row.coordinator_role.is_none()
        && row.queued_prompt_count.unwrap_or(0) == 0
        && row.stashed_prompt_count.unwrap_or(0) == 0
        && row.pending_question_count == 0
        && !matches!(
            row.title_source,
            Some(
                ghostex_gx_protocol::TitleSource::User
                    | ghostex_gx_protocol::TitleSource::Generated
            )
        )
}

impl GhostexGpuiApp {
    /// Records a newly created blank chat draft so opening something else removes it if it is
    /// still untouched (no text typed, nothing changed).
    pub(crate) fn gx_store_track_untouched_draft(&mut self, session: SessionKey) {
        let opened_from = self.gx_store_current_selected_session();
        let has_been_focused = opened_from.as_ref() == Some(&session);
        let list = &mut self.gx_store.draft_arrivals.untouched_drafts;
        list.retain(|entry| entry.session != session);
        list.push(TrackedUntouchedDraft {
            session,
            opened_from,
            has_been_focused,
        });
    }

    /// Whether `session` is currently tracked as an untouched draft.
    #[cfg_attr(not(ghostex_web), allow(dead_code))]
    pub(crate) fn gx_store_is_tracked_untouched_draft(&self, session: &SessionKey) -> bool {
        self.gx_store
            .draft_arrivals
            .untouched_drafts
            .iter()
            .any(|entry| &entry.session == session)
    }

    /// The session the user currently has open, including a newly created session whose row has
    /// not yet arrived in the store. `None` while Quick Automations is open.
    fn gx_store_current_selected_session(&self) -> Option<SessionKey> {
        #[cfg(not(ghostex_web))]
        {
            let focus = self.gx_store.core.focus();
            if self
                .gx_store
                .focus_publish
                .quick_automations_open(focus.local_stamp)
            {
                return None;
            }
            match self.gx_store.focus_publish.unplaced_for(focus.local_stamp) {
                Some(held) => Some(held.session.clone()),
                None => focus.focused_session.clone(),
            }
        }
        #[cfg(ghostex_web)]
        {
            self.open_session
                .clone()
                .or_else(|| self.gx_store.core.focus().focused_session.clone())
        }
    }

    /// Whether `session` is still displayed in any visible pane (e.g. side-by-side split).
    fn gx_store_draft_is_visible(&self, session: &SessionKey) -> bool {
        #[cfg(not(ghostex_web))]
        {
            self.gx_store
                .core
                .focus()
                .visible_sessions
                .contains(session)
        }
        #[cfg(ghostex_web)]
        {
            self.open_session.as_ref() == Some(session)
        }
    }

    /// Checks the open `NativeChatView` for `session` (when mounted) to see whether the user has
    /// typed text, attached anything, sent/queued a message, or changed any setup option.
    pub(crate) fn gx_store_draft_chat_view_untouched(
        &self,
        session: &SessionKey,
        cx: &gpui::Context<Self>,
    ) -> bool {
        #[cfg(not(ghostex_web))]
        {
            let machine_id = match &session.machine {
                MachineId::Local => LOCAL_MACHINE_ID,
                MachineId::Remote(id) => id.as_str(),
            };
            self.native_chat_views.values().all(|view| {
                let chat = view.read(cx);
                if chat.config.machine_id == machine_id
                    && chat.config.project_id == session.project_id
                    && chat.config.session_id == session.session_id
                {
                    chat.is_untouched_draft()
                } else {
                    true
                }
            })
        }
        #[cfg(ghostex_web)]
        {
            self.native_chats
                .get(session)
                .is_none_or(|(_, view)| view.read(cx).is_untouched_draft())
        }
    }

    /// Closes an untouched draft's tab/chat view quietly without disturbing the newly opened
    /// session's focus.
    fn gx_store_close_untouched_draft_surface(
        &mut self,
        session: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(not(ghostex_web))]
        {
            let machine_id = match &session.machine {
                MachineId::Local => LOCAL_MACHINE_ID,
                MachineId::Remote(id) => id.as_str(),
            };
            let shell_ids: Vec<crate::TerminalSessionId> = self
                .native_chat_views
                .iter()
                .filter_map(|(id, view)| {
                    let chat = view.read(cx);
                    (chat.config.machine_id == machine_id
                        && chat.config.project_id == session.project_id
                        && chat.config.session_id == session.session_id)
                        .then_some(*id)
                })
                .collect();
            if session.machine.is_local() {
                let local_key = crate::app::model::GpuiLocalWorkspaceSessionKey {
                    project_id: session.project_id.clone(),
                    session_id: session.session_id.clone(),
                };
                self.local_workspace_session_mappings.remove(&local_key);
            }
            let mut closed_any = false;
            for shell_id in shell_ids {
                self.agents_chat_mode_sessions.remove(&shell_id);
                self.remove_agents_chat_surface_for_session(shell_id, cx);
                if let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_id)
                    && self.agents_workspace.can_close_tab(pane_id, shell_id)
                {
                    self.agents_workspace.close_tab(pane_id, shell_id);
                    closed_any = true;
                }
            }
            if closed_any {
                self.persist_shell_layout_state();
            }
        }
        #[cfg(ghostex_web)]
        {
            let _ = cx;
            self.native_chats.remove(session);
            self.chat_presentations.remove(session);
        }
    }

    /// Runs whenever focus moves to another session (or Quick Automations): any tracked draft that
    /// is no longer focused or visible is checked, kept if the user typed or changed anything, and
    /// quietly removed otherwise.
    pub(crate) fn gx_store_prune_untouched_drafts(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.draft_arrivals.untouched_drafts.is_empty() {
            return;
        }
        let selected = self.gx_store_current_selected_session();
        let tracked = std::mem::take(&mut self.gx_store.draft_arrivals.untouched_drafts);
        let mut keep = Vec::with_capacity(tracked.len());
        let mut to_remove = Vec::new();
        for mut entry in tracked {
            if selected.as_ref() == Some(&entry.session) {
                entry.has_been_focused = true;
                keep.push(entry);
                continue;
            }
            if self.gx_store_draft_is_visible(&entry.session) {
                keep.push(entry);
                continue;
            }
            // Wait until either the draft has been focused at least once, or the user moved to a
            // third session different from the one they started from.
            if !entry.has_been_focused && selected == entry.opened_from && selected.is_some() {
                keep.push(entry);
                continue;
            }
            let row_untouched = self
                .gx_store
                .core
                .presentation()
                .session(&entry.session)
                .as_deref()
                .is_none_or(presentation_draft_is_untouched);
            let view_untouched = self.gx_store_draft_chat_view_untouched(&entry.session, cx);
            if row_untouched && view_untouched {
                to_remove.push(entry.session);
            }
            // Touched drafts are dropped from `untouched_drafts` and kept forever.
        }
        self.gx_store.draft_arrivals.untouched_drafts = keep;
        if to_remove.is_empty() {
            return;
        }
        let now = now_ms();
        let mut sidebar_changed = false;
        for session in to_remove {
            self.gx_store_close_untouched_draft_surface(&session, cx);
            let output = self.gx_store.core.handle(
                Event::Intent(Intent::HideSession {
                    session: session.clone(),
                }),
                now,
            );
            if !output.changes.is_empty() {
                sidebar_changed = true;
                self.gx_store.sidebar_list.note_changes(&output.changes);
            }
            self.gx_store_close_untouched_draft_on_server(session, cx);
        }
        if sidebar_changed {
            self.gx_store_sidebar_state_changed(cx);
        }
    }

    /// Quietly closes an untouched draft on gxserver via `/api/draftWorktree { finish, moved }`,
    /// which only closes a session that is still a draft on the server. If the server declines or
    /// the call fails, the row is unhidden again.
    fn gx_store_close_untouched_draft_on_server(
        &mut self,
        session: SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "finish": true,
            "moved": true,
        });
        let unhide = |this: &mut Self, session: SessionKey, cx: &mut gpui::Context<Self>| {
            let output = this
                .gx_store
                .core
                .handle(Event::Intent(Intent::UnhideSession { session }), now_ms());
            if !output.changes.is_empty() {
                this.gx_store.sidebar_list.note_changes(&output.changes);
                this.gx_store_sidebar_state_changed(cx);
            }
        };
        fn was_closed<E>(result: &Result<Value, E>) -> bool {
            result
                .as_ref()
                .is_ok_and(|value| value.get("closed").and_then(Value::as_bool) == Some(true))
        }
        match &session.machine {
            MachineId::Local => {
                cx.spawn(async move |this, cx| {
                    let result = super::rpc::gx_rpc(None, "/api/draftWorktree", params).await;
                    if !was_closed(&result) {
                        let _ = this.update(cx, |this, cx| unhide(this, session, cx));
                    }
                })
                .detach();
            }
            MachineId::Remote(machine_id) => {
                let task = self.start_gpui_remote_sidebar_rpc(
                    machine_id,
                    "/api/draftWorktree",
                    Some(params),
                    std::time::Duration::from_secs(15),
                    GpuiRemoteSidebarRpcMode::Awaited,
                    cx,
                );
                cx.spawn(async move |this, cx| {
                    let result = task.await;
                    if !was_closed(&result) {
                        let _ = this.update(cx, |this, cx| unhide(this, session, cx));
                    }
                })
                .detach();
            }
        }
    }

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
                self.gx_store.draft_arrivals.project_created.reset();
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
                let rows = project_session_rows(self.gx_store.sidebar_list.view());
                let arrivals = self
                    .gx_store
                    .draft_arrivals
                    .project
                    .observe(&drafts, now_ms);
                // Drafts open for any arrival; the reveal is for the new row this client opened,
                // whichever heading it is under.
                let created = &mut self.gx_store.draft_arrivals.project_created;
                created.observe(&rows, now_ms);
                let reveal = created.take_focused(focused.as_deref(), now_ms);
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
                        // Both read the state before either applies, so the Drafts heading both
                        // want open is toggled once, not opened and shut again.
                        for intent in plan.intents(store.sidebar_ui.state()) {
                            if !intents.contains(&intent) {
                                intents.push(intent);
                            }
                        }
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
