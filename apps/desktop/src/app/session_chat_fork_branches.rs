//! The chat branch switcher's `selectForkBranch` host action.
//!
//! CDXC:SessionFork 2026-09-18 SEE-ALSO:
//! GPUI chat's strip is `apps/desktop/src/app/native_chat/fork_branches.rs` and the rows come from
//! `packages/gx-chat-core/src/menus/picker/fork_branches.rs`. This action must keep waking a
//! stopped branch before focusing it.

use crate::*;
use ghostex_gx_core::MachineId;
use std::time::Duration;

impl GhostexGpuiApp {
    /*
    CDXC:SessionFork 2026-09-03:
    A fork's ancestor is usually STOPPED: forking kills the source's provider and Previous Sessions
    hides the row. A stopped row is not in the live presentation, so focusing it alone finds no
    session and the click does nothing. Wake it first: `/api/wakeSession` has no lifecycle guard,
    respawns the provider with the row's saved agent resume command, marks the SAME registry row
    running, and broadcasts its presentation delta before it answers, so the follow-up focus
    resolves. Reviving in place keeps the family edge intact; a Previous-Sessions-style restore
    would create a new row and remove the parent both leaves point at.
    */
    pub(crate) fn select_session_chat_fork_branch(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let field = |name: &str| {
            message
                .get(name)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let (Some(project_id), Some(branch_session_id)) = (field("projectId"), field("sessionId"))
        else {
            return;
        };
        // The family is derived by the daemon that owns this chat, so a remote session's branches
        // live on that same machine and take its tunnel.
        // A Cloud Box start names the box's machine (`cloudbox:<boxId>`) the new session is on.
        // CDXC:RemoteMachines 2026-11-04 WHY: a coordinator's worker on another computer than the
        // coordinator's names that computer's gxserver (`serverId`, server/coordinator_open_http.rs),
        // which may be this computer itself when the coordinator runs elsewhere. Without a machine
        // for it the click must not open a session of the same ids on the coordinator's computer.
        let on_server = match field("serverId") {
            Some(server_id) => {
                match self.machine_for_server_session(&server_id, &project_id, &branch_session_id) {
                    // Its rows are the copy kept from the last run: connect, then open it.
                    Some(MachineId::Remote(machine_id))
                        if self
                            .gx_store
                            .core
                            .presentation()
                            .loaded_live(&MachineId::Remote(machine_id.clone()))
                            .is_none() =>
                    {
                        self.open_worker_once_connected(
                            session_id,
                            machine_id,
                            message.clone(),
                            cx,
                        );
                        return;
                    }
                    Some(machine) => Some(machine),
                    None => {
                        self.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Could not open that worker",
                            "This app isn't connected to the computer it runs on. Connect to it from the sidebar, then try again.",
                            cx,
                        );
                        return;
                    }
                }
            }
            None => None,
        };
        let remote_machine_id = match (
            on_server,
            field("machineId"),
            self.workspace_terminal_key_for_shell_session(session_id),
        ) {
            (Some(MachineId::Local), _, _) => None,
            (Some(MachineId::Remote(machine_id)), _, _) => Some(machine_id),
            (None, Some(machine_id), _) => Some(machine_id),
            (None, None, Some(GpuiWorkspaceTerminalSessionKey::Remote(key))) => {
                Some(key.remote_machine_id)
            }
            _ => None,
        };
        let sidebar_session_id = match &remote_machine_id {
            Some(machine_id) => {
                gpui_remote_scoped_session_id(machine_id, &project_id, &branch_session_id)
            }
            None => gpui_combined_presentation_session_id(&project_id, &branch_session_id),
        };
        if field("lifecycleState").as_deref() != Some("stopped") {
            self.focus_session_chat_fork_branch(&sidebar_session_id, cx);
            return;
        }
        let remote = remote_machine_id
            .as_deref()
            .and_then(|machine_id| self.gpui_remote_gxserver_request_target(machine_id));
        if remote_machine_id.is_some() && remote.is_none() {
            self.report_session_chat_fork_branch_failure(cx);
            return;
        }
        let params = serde_json::json!({
            "projectId": project_id,
            "sessionId": branch_session_id,
        });
        cx.spawn(async move |this, cx| {
            let woken = cx
                .background_executor()
                .spawn(async move {
                    let response = match remote {
                        Some(target) => gpui_remote_gxserver_post_typed_operation(
                            &target,
                            "/api/wakeSession",
                            &params,
                            Duration::from_secs(60),
                        ),
                        None => gxserver_post_typed_operation(
                            "/api/wakeSession",
                            &params,
                            Duration::from_secs(60),
                        ),
                    };
                    matches!(response, Ok((status, _)) if (200..300).contains(&status))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if woken {
                    this.focus_session_chat_fork_branch(&sidebar_session_id, cx);
                } else {
                    this.report_session_chat_fork_branch_failure(cx);
                }
            });
        })
        .detach();
    }

    /// The same route the Find Prompts modal takes: the Rust store owns focus (gx_store/activation_focus.rs) and the reveal
    /// expands and scrolls the containers holding the row.
    fn focus_session_chat_fork_branch(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.dispatch_gpui_command_palette_session_focus(sidebar_session_id, cx) {
            // In the window the focus went to (workspace_windows/session_routing.rs).
            let row_id = sidebar_session_id.to_string();
            self.run_in_session_window(sidebar_session_id, cx, move |app, cx| {
                app.reveal_sidebar_session(&row_id, cx)
            });
        } else {
            self.report_session_chat_fork_branch_failure(cx);
        }
    }

    /// The machine a session of the gxserver `server_id` is on: this computer, a remote machine
    /// the store holds rows of (its live stream named that server, or its last-seen copy holds the
    /// session, `PresentationStore::machine_for_server_session`), else a machine this app
    /// registered under that server id (remote_conn/connected_computers.rs).
    ///
    /// CDXC:RemoteMachines 2026-11-04 WHY:
    /// The first version matched only a live stream's server id. After a restart the app had not connected to Windows again, so its rows were the last-seen copy with no server id, and this app had not registered Windows either: the only registration was the one the Windows app made when it connected to this computer (`ghostex computers` then names it by Windows' own name, M7-DESKTOP). So the lookup found nothing and said "Its computer isn't connected" for a computer that was.
    fn machine_for_server_session(
        &self,
        server_id: &str,
        project_id: &str,
        session_id: &str,
    ) -> Option<MachineId> {
        self.gx_store
            .core
            .presentation()
            .machine_for_server_session(server_id, project_id, session_id)
            .or_else(|| {
                crate::app::remote_conn::connected_computers::gpui_remote_machine_for_server_id(
                    server_id,
                )
                .map(MachineId::Remote)
            })
    }

    /// A worker on a machine this app holds only last run's rows of: connects to it, then opens
    /// the worker (`when_remote_machine_live`).
    fn open_worker_once_connected(
        &mut self,
        session_id: TerminalSessionId,
        machine_id: String,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.when_remote_machine_live(
            machine_id,
            move |this, cx| this.select_session_chat_fork_branch(session_id, &message, cx),
            cx,
        );
    }

    /// A click on a coordinator's thread row nested from another computer whose rows are last
    /// run's copy: connects to that computer, then focuses the row once it is live. `false` when
    /// the row's computer is live (or is this one), and the click goes on as usual.
    ///
    /// CDXC:RemoteMachines 2026-11-04 WHY:
    /// The nested row sits in this computer's live group, so the sidebar's guard for a stale machine's rows (`group.is_stale`) does not stop its click, which then focused a session on a computer the app was not connected to. It now takes the Workers list's path.
    pub(crate) fn open_borrowed_thread_row_when_connected(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = ghostex_gx_core::SessionKey::parse_sidebar_session_id(sidebar_session_id)
        else {
            return false;
        };
        let Some(machine_id) = key.machine.remote_id() else {
            return false;
        };
        if self
            .gx_store
            .core
            .presentation()
            .loaded_live(&key.machine)
            .is_some()
        {
            return false;
        }
        let row = sidebar_session_id.to_string();
        self.when_remote_machine_live(
            machine_id.to_string(),
            move |this, cx| {
                this.dispatch_native_sidebar_ui(
                    serde_json::json!({"type": "selectSession", "sessionId": row, "mode": "focus"}),
                    cx,
                );
                let _ = this.react_to_native_sidebar_session_click(&row, cx);
            },
            cx,
        );
        true
    }

    /// Runs `then` once `machine_id`'s rows come from a live stream, connecting it first (the
    /// machine tab's connect button) unless an attempt is already running; after 30 seconds it
    /// says the computer isn't connected instead.
    fn when_remote_machine_live(
        &mut self,
        machine_id: String,
        then: impl FnOnce(&mut Self, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let connecting = self
            .remote_machine_connect_states
            .get(machine_id.as_str())
            .is_some_and(|state| {
                matches!(
                    state.as_str(),
                    "connecting" | "downloadingRemoteServerPackage" | "installing"
                )
            });
        if !connecting {
            self.remote_reconnect_from_sidebar(&machine_id, cx);
        }
        let machine = MachineId::Remote(machine_id.clone());
        cx.spawn(async move |this, cx| {
            let started = std::time::Instant::now();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let Ok(live) = this.update(cx, |this, _| {
                    this.gx_store
                        .core
                        .presentation()
                        .loaded_live(&machine)
                        .is_some()
                }) else {
                    return;
                };
                if live {
                    let _ = this.update(cx, |this, cx| then(this, cx));
                    return;
                }
                if started.elapsed() >= Duration::from_secs(30) {
                    let _ = this.update(cx, |this, cx| {
                        let label = this
                            .gx_store
                            .remote
                            .tabs()
                            .iter()
                            .find(|tab| tab.machine_id == machine_id)
                            .map_or_else(|| machine_id.clone(), |tab| tab.label.clone());
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            &format!("{label} isn't connected"),
                            &format!(
                                "This worker runs on {label}. Connect to it from the sidebar, then try again."
                            ),
                            cx,
                        );
                    });
                    return;
                }
            }
        })
        .detach();
    }

    fn report_session_chat_fork_branch_failure(&mut self, cx: &mut gpui::Context<Self>) {
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Could not open that branch",
            "The session could not be resumed. Try again from the sidebar.",
            cx,
        );
    }
}
