//! Opening a session this app has just created: the desktop half of every create, kept apart from
//! the executors so they stay free of workspace APIs.
//!
//! CDXC:FocusRouting 2026-09-25 WHY:
//! The old runtime followed a create with `focusLocalWorkspaceSession`, which set ITS focus copy
//! first and then posted the focus, so the attach completion's check that the runtime's copy names
//! the session passed. The store cannot take the session as a local selection while its row has
//! not arrived: the same race `CDXC:SessionFork 2026-09-24 DECISION` solved for Fork, and solved
//! the same way. The session is recorded as created here for a short while, and the attach
//! completion treats it like a fork's (the newest focus request and the workspace's project
//! decide), including the background attach a launch from another view takes. A create in another
//! project takes that project's workspace first, as a fork and a row click do, and the store's
//! focus holds the new session until its row arrives (focus_publish.rs), so the workspace publish
//! follows the create instead of the session it came from. A create that a Git workflow starts
//! takes this same open (git/prompt_agent.rs).
//!
//! SEE-ALSO: apps/desktop/src/app/workspace_events.rs (`focus_local_workspace_terminal_from_message`
//! and the attach completion), apps/desktop/src/app/gx_store/sidebar_lifecycle.rs
//! (`gx_store_place_local_workspace_session`).

use std::time::Duration;
use web_time::Instant;

use crate::GhostexGpuiApp;
use crate::app::model::{
    GpuiLocalWorkspaceSessionKey, GpuiPreferredAgentInterface,
    GpuiSidebarWorkspaceTerminalFocusMessage,
};
use crate::app::workspace_windows::ProjectWindowRoute;

/// How long a created session's attach is treated as the create's own. An attach plan answers in
/// well under a second; a wake of a slow provider can take several.
const CREATED_ATTACH_WINDOW: Duration = Duration::from_secs(30);

/// Sessions this app created and asked the workspace to open, newest last.
#[derive(Default)]
pub(crate) struct CreatedAttaches {
    entries: Vec<(GpuiLocalWorkspaceSessionKey, Instant)>,
}

impl GhostexGpuiApp {
    /// `focusLocalWorkspaceSession(projectId, sessionId, { keepView, preferredInterface })` for a
    /// session a create just made.
    pub(crate) fn gx_store_focus_created_session(
        &mut self,
        project_id: &str,
        session_id: &str,
        keep_view: bool,
        preferred_interface: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let project_id = project_id.trim();
        let session_id = session_id.trim();
        if project_id.is_empty() || session_id.is_empty() {
            return;
        }
        // A session created in a project of another workspace (a `ghostex` create, the Work page's
        // Start chat) opens in the window that shows that workspace, or switches this one to it
        // (workspace_windows/session_routing.rs).
        let switched = match self.route_local_project_to_its_window(project_id, cx) {
            ProjectWindowRoute::Here => false,
            ProjectWindowRoute::Switched => true,
            ProjectWindowRoute::Other(app) => {
                let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
                let preferred_interface = preferred_interface.map(str::to_string);
                cx.defer(move |cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.gx_store_focus_created_session(
                            &project_id,
                            &session_id,
                            keep_view,
                            preferred_interface.as_deref(),
                            cx,
                        )
                    });
                });
                return;
            }
        };
        let message = GpuiSidebarWorkspaceTerminalFocusMessage {
            force_remount: false,
            placement_target_session_id: None,
            preferred_interface: preferred_interface
                .and_then(GpuiPreferredAgentInterface::from_str)
                .unwrap_or_default(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            startup_restore: false,
            keep_view,
            wake_sleeping: false,
            keep_sleeping: false,
        };
        let key = GpuiLocalWorkspaceSessionKey::from(&message);
        let attaches = &mut self.gx_store.create.created_attaches.entries;
        attaches.retain(|(held, at)| *held != key && at.elapsed() < CREATED_ATTACH_WINDOW);
        attaches.push((key.clone(), Instant::now()));
        self.gx_store.create.counters.created_focuses += 1;
        // A launch staged an instant composer for this project; the created session takes it over,
        // exactly as the runtime's focus post did on arrival.
        self.adopt_agent_launch_placeholder(&message, cx);
        self.gx_store_take_created_session(&key, cx);
        // The store's selection above already gave the session its tab (session_chat_launch.rs).
        // A create that names no view follows its agent's Default Agent View
        // (`CDXC:SessionChat 2026-09-30 DECISION` in session_chat_launch.rs).
        if preferred_interface.is_none() {
            self.arm_local_default_view_chat_launch_intent(key);
        } else if message.preferred_interface == GpuiPreferredAgentInterface::Chat {
            self.arm_created_session_chat_launch_intent(key);
        }
        self.focus_local_workspace_terminal_from_message(&message, cx);
        if switched {
            self.settle_routed_window_workspace(cx);
        }
    }

    /// The created session's project takes the workspace and the store's focus takes the session
    /// (held until its row arrives), so no publish before the attach returns pulls the workspace
    /// back to the project the create came from. Every create opens through this, including the
    /// Windows creates that build their attach plan with the session (workspace_events.rs,
    /// session_chat_launch.rs).
    ///
    /// CDXC:FocusRouting 2026-10-04 WHY:
    /// The Windows project agent and terminal creates swapped the workspace but never told the
    /// store, so its focus stayed on project A's session. While the Chat launch waited for its
    /// attach plan, the next publish of that focus took the workspace back to project A and the new
    /// session in project B was never shown.
    pub(crate) fn gx_store_take_created_session(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        self.swap_agents_workspace_to_project_id(Some(key.project_id.clone()), cx);
        self.gx_store_select_opened_session(
            &ghostex_gx_core::SessionKey::local(key.project_id.as_str(), key.session_id.as_str()),
            cx,
        );
    }

    /// Whether this attach belongs to a session a create here just made.
    pub(crate) fn gx_store_is_created_attach(&self, key: &GpuiLocalWorkspaceSessionKey) -> bool {
        self.gx_store
            .create
            .created_attaches
            .entries
            .iter()
            .any(|(held, at)| held == key && at.elapsed() < CREATED_ATTACH_WINDOW)
    }
}
