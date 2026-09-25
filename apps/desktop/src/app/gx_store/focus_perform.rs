//! Focusing a session or a group, performed by the store: a sidebar row click (after its
//! in-process reaction), the session walk's steps into another project, a held key's landing
//! row, a Space restore, Back and Forward, the status item and the pet, a menu bar session row, a
//! Quick Access or palette jump, a notification, an App Shot and `ghostex focus`.
//!
//! CDXC:FocusRouting 2026-09-25 WHY:
//! Every one of these used to end in the app runtime's `focusSession` or `focusGroup`, which moved
//! its own copy of the focus, published it back, and posted the workspace focus request. The
//! store owns focus now, so the same steps run here in the runtime's order: the attention of the
//! session is acknowledged, the focus moves (and the project with it), the workspace is published
//! (focus_publish.rs), and the focus request opens, wakes or attaches the session. A remote row
//! takes the remote focus planner's open (sidebar_remote_focus.rs), and a group takes the store's
//! project or subgroup intent.
//!
//! CDXC:Navigation 2026-09-11 DECISION:
//! User: landing on another project keeps that project's remembered view (Code, Browser, Kanban, Automate, Docs) instead of switching to Agents; only a session click inside the active project still opens Agents.
//! Whether a focus changes the project is judged against the store's active project before the focus moves it. A row click's own reaction moves the store at once, so the click decides it in its own frame (`sidebar_focus_route.rs`) and passes it in as `keep_view`; a Space restore passes `keep_view` because it may reopen a session inside the project already active.
//! SEE-ALSO: `keep_view` on GpuiSidebarWorkspaceTerminalFocusMessage in apps/desktop/src/app/model/sidebar_bridge_messages.rs.
//!
//! CDXC:FocusRouting 2026-06-26-04:42:
//! Local sidebar clicks must not call gxserver `/api/focusSession`. That endpoint is an external renderer-command route and can bounce focus when another renderer is the first open gxserver subscriber.

use ghostex_gx_core::protocol::LifecycleState;
use ghostex_gx_core::{
    ActiveGroup, Event, Intent, LIFECYCLE_PATCH_TTL_MS, ProjectKey, QUICK_AUTOMATIONS_PROJECT_ID,
    SessionKey, SessionPatch, open_remote_session_terminal, plan_remote_focus,
    project_last_session_storage_key,
};
use serde_json::{Value, json};

use super::host::now_ms;
use super::records_storage::{RecordStore, write_record};
use crate::GhostexGpuiApp;
use crate::app::model::{
    GpuiPendingProjectSwitchPayload, GpuiPreferredAgentInterface, GpuiProjectSwitchRequestKind,
    GpuiSidebarWorkspaceTerminalFocusMessage, GpuiWorkspaceTerminalFocusPlacement,
    gpui_click_to_wake_sleeping_sessions_from_shared_settings,
};
use crate::shared_settings;

/// `projectLastSession` in `packages/client-storage/catalog.ts`: a `disk` store (indexeddb
/// backend, collection bounds, no age limit).
pub(super) const PROJECT_LAST_SESSION_STORE: RecordStore = RecordStore {
    id: "projectLastSession",
    version: 1,
    max_entry_bytes: 2 * 1024 * 1024,
    max_bytes: 16 * 1024 * 1024,
    max_entries: 2_000,
    max_age_ms: None,
};

/// The id of the All Automations overview's row (`GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID`).
const QUICK_AUTOMATIONS_ROW_SESSION_ID: &str = "__quick-automations__";

/// What the caller of a session focus asks for beside the session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowFocusOptions {
    /// Keep the destination project's view. A focus that changes the project keeps it anyway.
    pub(crate) keep_view: bool,
    /// Select a sleeping session without waking it (shows its wake placeholder while Click to Wake
    /// Sleeping Panes is on).
    pub(crate) keep_sleeping: bool,
    /// The view to open the session in; `None` is the agent's Default Agent View.
    pub(crate) preferred_interface: Option<GpuiPreferredAgentInterface>,
}

/// What this app run focused. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusPerformCounters {
    pub(crate) sessions: u64,
    pub(crate) remote_sessions: u64,
    pub(crate) groups: u64,
    pub(crate) wakes: u64,
    /// Ids no focus can take (a browser row, which the sidebar no longer draws, or garbage).
    pub(crate) refused: u64,
}

impl GhostexGpuiApp {
    /// A message the runtime's `handleSidebarMessage` used to answer and the store answers now.
    /// Returns whether it was one of them.
    pub(crate) fn gx_store_perform_focus_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match message.get("type").and_then(Value::as_str) {
            Some("focusSession") => {
                if let Some(session_id) = message.get("sessionId").and_then(Value::as_str) {
                    let options = RowFocusOptions {
                        keep_view: message.get("keepView") == Some(&Value::Bool(true)),
                        ..RowFocusOptions::default()
                    };
                    self.gx_store_focus_session_row(session_id, options, cx);
                }
                true
            }
            Some("focusGroup") => {
                if let Some(group_id) = message.get("groupId").and_then(Value::as_str) {
                    self.gx_store_focus_group_row(group_id, cx);
                }
                true
            }
            Some("openAutomationsPage") => {
                self.gx_store_open_quick_automations(cx);
                true
            }
            Some("runSidebarCommand") => {
                self.gx_store_run_sidebar_command(message, cx);
                true
            }
            _ => false,
        }
    }

    /// `focusSession(sessionId, message, options)`. Returns whether the id named something a
    /// focus can take.
    pub(crate) fn gx_store_focus_session_row(
        &mut self,
        row_id: &str,
        options: RowFocusOptions,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let row_id = row_id.trim();
        if let Some(remote) = SessionKey::parse_remote_scoped_session_id(row_id) {
            return self.gx_store_focus_remote_session(row_id, &remote, options, cx);
        }
        let Some(session) = SessionKey::parse_sidebar_session_id(row_id)
            .filter(|session| session.machine.is_local())
        else {
            self.gx_store.focus_perform.refused += 1;
            return false;
        };
        if session.project_id == QUICK_AUTOMATIONS_PROJECT_ID {
            if session.session_id == QUICK_AUTOMATIONS_ROW_SESSION_ID {
                self.gx_store_open_quick_automations(cx);
            }
            return true;
        }
        self.gx_store_focus_local_session(session, options, cx);
        true
    }

    /// The local branch of `focusSession`.
    fn gx_store_focus_local_session(
        &mut self,
        session: SessionKey,
        options: RowFocusOptions,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.focus_perform.sessions += 1;
        let project = session.project_key();
        // `focusChangesActiveProject`, judged before the focus below moves the project.
        let changes_project = self.gx_store.core.focus().active_project.as_ref() != Some(&project)
            || self.gx_store.focus_publish.quick_automations_since
                == Some(self.gx_store.core.focus().local_stamp);
        self.gx_store_acknowledge_attention(session.clone(), cx);
        let preferred_interface = options
            .preferred_interface
            .or_else(|| self.gx_store_row_preferred_interface(&session));
        let sleeping = self
            .gx_store
            .core
            .presentation()
            .is_sleeping_local_session(&session.project_id, &session.session_id);
        let click_to_wake = gpui_click_to_wake_sleeping_sessions_from_shared_settings(
            &shared_settings::shared_sidebar_settings_snapshot(),
        );
        let keep_sleeping = options.keep_sleeping && click_to_wake && sleeping;
        let wake_sleeping = sleeping && !keep_sleeping;
        if wake_sleeping {
            /*
            CDXC:FocusRouting 2026-09-20 WHY:
            The desktop's attach plan commits `/api/wakeSession` before the workspace materializes the terminal: `wakeSleeping` makes that plan use the Wake intent, which starts the provider, marks the row running and returns the attach metadata in one round trip. The row is patched to running the way the wake used to patch it, so the tab list published below does not show it asleep while the wake runs; a wake that fails leaves the daemon's next row to put it back.
            */
            self.gx_store.focus_perform.wakes += 1;
            let output = self.gx_store.core.handle(
                Event::Intent(Intent::PatchSession {
                    session: session.clone(),
                    patch: SessionPatch::lifecycle(
                        LifecycleState::Running,
                        now_ms().saturating_add(LIFECYCLE_PATCH_TTL_MS),
                    ),
                }),
                now_ms(),
            );
            if !output.changes.is_empty() {
                self.gx_store.sidebar_list.note_changes(&output.changes);
                self.gx_store_update_sidebar_list(cx);
            }
        }
        // A row click's reaction already selected the session with the exact rendered set; the
        // focus is only moved here when nothing has selected it yet.
        if self.gx_store.core.focus().focused_session.as_ref() != Some(&session) || changes_project
        {
            self.gx_store_select_opened_session(&session, cx);
        }
        self.gx_store_persist_remembered_sessions(cx);
        self.gx_store_publish_workspace_focus(cx);
        self.gx_store_request_workspace_focus(
            GpuiSidebarWorkspaceTerminalFocusMessage {
                force_remount: false,
                placement: GpuiWorkspaceTerminalFocusPlacement::Tab,
                placement_target_session_id: None,
                preferred_interface: preferred_interface.unwrap_or_default(),
                project_id: session.project_id,
                session_id: session.session_id,
                startup_restore: false,
                keep_view: options.keep_view || changes_project,
                wake_sleeping,
                keep_sleeping,
            },
            cx,
        );
    }

    /// The remote branch of `focusSession`: the planner's open, whose tab-selected callback moves
    /// the store's focus to the row.
    fn gx_store_focus_remote_session(
        &mut self,
        row_id: &str,
        session: &SessionKey,
        options: RowFocusOptions,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut message = json!({ "type": "focusSession", "sessionId": row_id });
        if options.keep_view {
            message["keepView"] = Value::Bool(true);
        }
        let settings = self.gx_store_preferred_interface_settings();
        let group = self.gx_store_remote_focus_group();
        let Some(mut plan) =
            plan_remote_focus(&self.gx_store.core, &message, &settings, group.as_deref())
        else {
            self.gx_store.focus_perform.refused += 1;
            return false;
        };
        // A caller that names the view (a project opened from the menu bar) overrides the agent's
        // Default Agent View, as `options.preferredInterface ?? ...` did.
        if let Some(interface) = options.preferred_interface {
            let interface = match interface {
                GpuiPreferredAgentInterface::Chat => "chat",
                GpuiPreferredAgentInterface::Terminal => "terminal",
            };
            plan.preferred_interface = Some(interface.to_string());
            plan.native_action =
                open_remote_session_terminal(row_id, plan.keep_view, Some(interface), false);
        }
        self.gx_store.focus_perform.remote_sessions += 1;
        if plan.live {
            self.gx_store_acknowledge_attention(session.clone(), cx);
        }
        self.receive_sidebar_native_project_path_action_payload(
            &plan.native_action.to_string(),
            cx,
        );
        true
    }

    /// `focusGroup(groupId)`: a project's own group or a user-made group becomes the active one; a
    /// remote project's group opens its best attachable session.
    pub(crate) fn gx_store_focus_group_row(
        &mut self,
        group_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(group) = ActiveGroup::parse_sidebar_group_id(group_id.trim()) else {
            self.gx_store.focus_perform.refused += 1;
            return false;
        };
        self.gx_store.focus_perform.groups += 1;
        let intent = match group {
            ActiveGroup::Project(project) if !project.machine.is_local() => {
                self.gx_store_attach_remote_group(&project, cx);
                return true;
            }
            ActiveGroup::Project(project) => Intent::FocusProject { project },
            ActiveGroup::Subgroup { project, group_id } => {
                Intent::FocusSubgroup { project, group_id }
            }
            // No sidebar draws a Chats group, so nothing names one to focus.
            ActiveGroup::Chats(_) => {
                self.gx_store.focus_perform.refused += 1;
                return false;
            }
        };
        self.gx_store_focus_intent(intent, cx);
        self.gx_store_publish_workspace_focus(cx);
        true
    }

    /// A project or group focus intent on the store, with the list following it.
    pub(super) fn gx_store_focus_intent(&mut self, intent: Intent, cx: &mut gpui::Context<Self>) {
        let output = self.gx_store.core.handle(Event::Intent(intent), now_ms());
        self.gx_store.focus_publish.unplaced = None;
        self.gx_store.local_focus.drawn_focus = super::local_focus::DrawnFocus::Store;
        if !output.changes.is_empty() {
            self.gx_store.sidebar_list.note_changes(&output.changes);
        }
        self.gx_store.run_effects(output.effects);
        if self.gx_store.refresh_row_focus_cache() {
            cx.notify();
        }
        self.gx_store_sidebar_focus_moved(cx);
    }

    /// `selectRemoteGroupAttachTarget` then the open: the project's running sessions first, then
    /// those in attention, working, pinned, favorite, and the most recently active; none opens a
    /// toast instead.
    fn gx_store_attach_remote_group(&mut self, project: &ProjectKey, cx: &mut gpui::Context<Self>) {
        let target = self
            .gx_store
            .core
            .presentation()
            .loaded_live(&project.machine)
            .and_then(|loaded| {
                let mut candidates: Vec<_> = loaded
                    .server_sessions()
                    .filter(|session| {
                        session.project_id == project.project_id
                            && matches!(session.kind.as_str(), "terminal" | "agent")
                    })
                    .collect();
                candidates.sort_by(|left, right| {
                    let score = |session: &ghostex_gx_core::protocol::PresentationSession| {
                        let mut value = 0;
                        if session.lifecycle_state == LifecycleState::Running {
                            value += 100;
                        }
                        match session.activity.as_str() {
                            "attention" => value += 40,
                            "working" => value += 30,
                            _ => {}
                        }
                        if session.is_pinned {
                            value += 10;
                        }
                        if session.is_favorite {
                            value += 5;
                        }
                        value
                    };
                    let time = |session: &ghostex_gx_core::protocol::PresentationSession| {
                        parse_iso_ms(
                            session
                                .last_active_at
                                .as_deref()
                                .unwrap_or(session.updated_at.as_str()),
                        )
                        .or_else(|| parse_iso_ms(&session.created_at))
                        .unwrap_or(0)
                    };
                    score(right)
                        .cmp(&score(left))
                        .then_with(|| time(right).cmp(&time(left)))
                });
                candidates.first().map(|session| session.session_id.clone())
            });
        let Some(session_id) = target else {
            self.receive_gpui_app_toast_bridge_message(
                &json!({
                    "description": "This remote project has no attachable sessions.",
                    "level": "info",
                    "title": "Remote attach unavailable",
                    "type": "toast",
                }),
                cx,
            );
            return;
        };
        let scoped = SessionKey::remote(
            project.machine.remote_id().unwrap_or_default(),
            project.project_id.as_str(),
            session_id.as_str(),
        )
        .to_focus_state_session_id();
        self.receive_sidebar_native_project_path_action_payload(
            &open_remote_session_terminal(&scoped, false, None, false).to_string(),
            cx,
        );
    }

    /// The group a remote row's `keepView` is planned from: the one the runtime's `activeGroupId`
    /// held, which the store's focus now is. A remote focus names its project's own group or its
    /// machine's Chats (`setRemotePresentationSessionFocus`), never the user-made group its row
    /// sits in, so that is what a focused remote session answers here.
    pub(super) fn gx_store_remote_focus_group(&self) -> Option<String> {
        let focus = self.gx_store.core.focus();
        match &focus.focused_session {
            Some(session) if !session.machine.is_local() => Some(
                ghostex_gx_core::remote_focus_group(&self.gx_store.core, session),
            ),
            _ => focus
                .active_group
                .as_ref()
                .map(ActiveGroup::to_sidebar_group_id),
        }
    }

    /// `sessionPreferredAgentInterface`: the row's agent's Default Agent View, nothing for a row
    /// with no agent.
    fn gx_store_row_preferred_interface(
        &self,
        session: &SessionKey,
    ) -> Option<GpuiPreferredAgentInterface> {
        let row = self.gx_store.core.presentation().session(session)?;
        let settings = self.gx_store_preferred_interface_settings();
        settings
            .resolve(row.agent_id.as_deref())
            .and_then(GpuiPreferredAgentInterface::from_str)
    }

    /// The workspace focus request, as the bridge's `WorkspaceTerminalFocus` entry performed it: a
    /// launch placeholder the session takes over, then the project switch coalescer, then the
    /// open.
    pub(crate) fn gx_store_request_workspace_focus(
        &mut self,
        message: GpuiSidebarWorkspaceTerminalFocusMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        self.adopt_agent_launch_placeholder(&message, cx);
        /*
        CDXC:Navigation 2026-07-29:
        The focus state goes out before this imperative focus request, so when that state is collapsed into the trailing switch this request must ride with it. Running it now would attach the session into the outgoing project's workspace, which the trailing swap would then tear down.
        */
        if self.project_switch_request_is_coalesced(
            Some(message.project_id.as_str()),
            GpuiProjectSwitchRequestKind::WorkspaceTerminalFocus,
        ) {
            self.enqueue_coalesced_project_switch_request(
                Some(message.project_id.clone()),
                GpuiPendingProjectSwitchPayload::WorkspaceTerminalFocus(message),
                cx,
            );
            return;
        }
        self.focus_local_workspace_terminal_from_message(&message, cx);
    }

    /// Writes the remembered session of every project a selection moved to since the last write.
    ///
    /// CDXC:Projects 2026-09-05 DECISION:
    /// User: remember each project's last selected agent or terminal across restarts, including sleeping sessions and closed projects.
    /// Native tab/pane focus and sidebar focus both write this selection; agent activity timestamps do not represent the user's selection. The store keeps the newest per project in memory (`RememberProjectSession`) and this is its one writer; a held key writes once, at the end of the burst.
    pub(crate) fn gx_store_persist_remembered_sessions(&mut self, cx: &mut gpui::Context<Self>) {
        let remembered = std::mem::take(&mut self.gx_store.local_focus.pending_remembered);
        if remembered.is_empty() {
            return;
        }
        let rows: Vec<(String, String)> = remembered
            .iter()
            .map(|session| {
                (
                    project_last_session_storage_key(&session.project_key()),
                    session.to_sidebar_session_id(),
                )
            })
            .collect();
        cx.background_executor()
            .spawn(async move {
                let now = now_ms() as i64;
                for (key, raw) in rows {
                    let _ = write_record(PROJECT_LAST_SESSION_STORE, &key, &raw, now);
                }
            })
            .detach();
    }

    /// The session a project remembers, read back from client storage.
    pub(super) fn gx_store_read_remembered_session(project: &ProjectKey) -> Option<String> {
        match super::records_storage::read_record_raw(
            PROJECT_LAST_SESSION_STORE,
            &project_last_session_storage_key(project),
            now_ms() as i64,
        ) {
            Ok(super::records_storage::RecordRead::Payload(raw)) => Some(raw),
            _ => None,
        }
    }
}

/// `Date.parse` for the daemon's ISO timestamps, in milliseconds; `None` for anything else.
fn parse_iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}
