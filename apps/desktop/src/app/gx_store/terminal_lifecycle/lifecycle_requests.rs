//! A workspace tab's own Close, Sleep and Wake, after the tab has made its local decision: the
//! daemon half and the sidebar half, performed by the store instead of the old runtime
//! (`terminal-lifecycle-queue.ts`, `applyWorkspaceTerminalLifecycleRequest`).
//!
//! CDXC:Workarea 2026-06-26 WHY:
//! GPUI native workspace lifecycle follows macOS ownership: Rust commits Close locally before this runs and uses it only for asynchronous provider cleanup, while Sleep and Wake still wait for the daemon's answer before the tab moves (`finish_local_workspace_lifecycle_request`). A wake answer lets Rust move the reused tab into Mounting; a sleep answer must prove the provider stopped.
//!
//! CDXC:Workarea 2026-07-10 WHY:
//! Close is local-first and hides the sidebar row even when gxserver is disconnected. The provider transition is best-effort cleanup; unlike Sleep and Wake it is not a prerequisite for acknowledging the user's tab close. A close the daemon never confirms puts the sidebar row back, as a sidebar Close does (CDXC:Sessions 2026-09-20 DECISION in gx-core `sidebar_actions/close.rs`).
//!
//! SEE-ALSO: apps/desktop/src/app/workspace_terminals.rs (`request_local_workspace_terminal_lifecycle`,
//! `request_remote_workspace_terminal_lifecycle`), packages/gx-core/src/sidebar_actions/terminal_lifecycle.rs.

use std::time::Duration;

use ghostex_gx_core::protocol::LifecycleState;
use ghostex_gx_core::{
    Event, FocusOptions, Intent, LIFECYCLE_PATCH_TTL_MS, REMOTE_AWAITED_TIMEOUT_MS, SessionKey,
    SessionPatch, open_remote_session_terminal, provider_transition_committed,
    terminal_lifecycle_fallback_focus,
};
use serde_json::{Value, json};

use super::super::host::now_ms;
use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;
use crate::app::helpers::gpui_remote_project_reference_from_project_id;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// The request the tab built, read back from the message it used to post to the runtime.
struct TabLifecycleRequest {
    action: String,
    project_id: String,
    request_id: Option<u64>,
    session_id: String,
    replacement_project_id: Option<String>,
    replacement_session_id: Option<String>,
    skip_replacement_fallback: bool,
    keep_sidebar_focus: bool,
}

impl TabLifecycleRequest {
    fn read(message: &Value) -> Option<Self> {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let action = text("action")
            .filter(|action| matches!(action.as_str(), "close" | "sleep" | "wake"))?;
        Some(Self {
            action,
            project_id: text("projectId")?,
            request_id: message.get("requestId").and_then(Value::as_u64),
            session_id: text("sessionId")?,
            replacement_project_id: text("replacementProjectId"),
            replacement_session_id: text("replacementSessionId"),
            skip_replacement_fallback: message.get("skipReplacementFallback")
                == Some(&Value::Bool(true)),
            keep_sidebar_focus: message.get("keepSidebarFocus") == Some(&Value::Bool(true)),
        })
    }
}

impl GhostexGpuiApp {
    /// Runs a tab's lifecycle request. Returns whether it was accepted; a Sleep or Wake finishes
    /// later through `finish_local_workspace_lifecycle_request`.
    pub(crate) fn gx_store_run_tab_lifecycle_request(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(request) = TabLifecycleRequest::read(message) else {
            return false;
        };
        if let Some(remote) = gpui_remote_project_reference_from_project_id(&request.project_id) {
            self.gx_store_run_remote_tab_lifecycle(
                request,
                remote.remote_machine_id.to_string(),
                remote.project_id,
                cx,
            );
            return true;
        }
        let session = SessionKey::local(&request.project_id, &request.session_id);
        let explicit_replacement = request.replacement_session_id.as_ref().map(|replacement| {
            SessionKey::local(
                request
                    .replacement_project_id
                    .as_deref()
                    .unwrap_or(&request.project_id),
                replacement,
            )
        });
        // Resolved before anything moves, as the runtime resolved it before the row left its list.
        let replacement = explicit_replacement.or_else(|| {
            (!request.skip_replacement_fallback)
                .then(|| terminal_lifecycle_fallback_focus(&self.gx_store.core, &session))
                .flatten()
        });
        match request.action.as_str() {
            "close" => self.gx_store_close_tab_session(session, replacement, cx),
            "sleep" => self.gx_store_sleep_tab_session(request, session, replacement, cx),
            _ => self.gx_store_wake_tab_session(request, session, cx),
        }
        true
    }

    fn gx_store_close_tab_session(
        &mut self,
        session: SessionKey,
        replacement: Option<SessionKey>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_tab_lifecycle_intent(
            Intent::HideSession {
                session: session.clone(),
            },
            cx,
        );
        if let Some(replacement) = replacement {
            self.gx_store_select_local_workspace_session(
                &replacement,
                None,
                FocusOptions::default(),
                cx,
            );
        }
        let params = json!({
            "action": "close",
            "projectId": session.project_id,
            "reason": "closeTerminal",
            "sessionId": session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/transitionSession", params).await;
            if result.is_err() {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_tab_lifecycle_intent(Intent::UnhideSession { session }, cx);
                });
            }
        })
        .detach();
    }

    fn gx_store_sleep_tab_session(
        &mut self,
        request: TabLifecycleRequest,
        session: SessionKey,
        replacement: Option<SessionKey>,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = json!({
            "action": "sleep",
            "projectId": session.project_id,
            "reason": "sleepSession",
            "sessionId": session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/transitionSession", params).await;
            let _ = this.update(cx, |this, cx| {
                let committed = result
                    .as_ref()
                    .is_ok_and(|result| provider_transition_committed(result, "sleep"));
                if committed {
                    this.gx_store_patch_tab_lifecycle(&session, LifecycleState::Sleeping, cx);
                    if let Some(replacement) = replacement {
                        this.gx_store_select_local_workspace_session(
                            &replacement,
                            None,
                            FocusOptions::default(),
                            cx,
                        );
                    }
                }
                this.gx_store_finish_tab_lifecycle(request.request_id, committed, cx);
            });
        })
        .detach();
    }

    /// A woken tab keeps the focus the selection that asked for the wake already gave the store;
    /// nothing is re-focused from here, which is also what the runtime's `keepSidebarFocus` and
    /// "focus moved elsewhere meanwhile" guards were protecting.
    fn gx_store_wake_tab_session(
        &mut self,
        request: TabLifecycleRequest,
        session: SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = request.keep_sidebar_focus;
        let params = json!({
            "projectId": session.project_id,
            "reason": "gpui-sidebar",
            "sessionId": session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/wakeSession", params).await;
            let _ = this.update(cx, |this, cx| {
                let ok = result.is_ok();
                if ok {
                    this.gx_store_patch_tab_lifecycle(&session, LifecycleState::Running, cx);
                }
                this.gx_store_finish_tab_lifecycle(request.request_id, ok, cx);
            });
        })
        .detach();
    }

    /// A tab of a REMOTE machine. The machine's own presentation stream redraws its rows; the only
    /// local move is the replacement's open, which is a remote session click's open.
    fn gx_store_run_remote_tab_lifecycle(
        &mut self,
        request: TabLifecycleRequest,
        machine_id: String,
        project_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let replacement = request
            .replacement_session_id
            .clone()
            .zip(
                request
                    .replacement_project_id
                    .as_deref()
                    .and_then(gpui_remote_project_reference_from_project_id),
            )
            .filter(|(_, reference)| reference.remote_machine_id.to_string() == machine_id)
            .map(|(session_id, reference)| SessionKey {
                machine: ghostex_gx_core::MachineId::Remote(machine_id.clone()),
                project_id: reference.project_id,
                session_id,
            });
        let params = json!({
            "projectId": project_id,
            "reason": if request.action == "close" { "closeTerminal" } else { "gpui-sidebar" },
            "sessionId": request.session_id,
        });
        let timeout = Duration::from_millis(REMOTE_AWAITED_TIMEOUT_MS);
        if request.action == "close" {
            // CDXC:RemoteMachines 2026-08-08 WHY: a remote direct close focuses its surviving terminal through the same native open as a remote session click; a presentation-only focus update selected the row but never moved keyboard ownership.
            if let Some(replacement) = replacement {
                self.gx_store_open_remote_tab(&replacement, cx);
            }
            let _ = self
                .start_gpui_remote_sidebar_rpc(
                    &machine_id,
                    "/api/killSession",
                    Some(params),
                    timeout,
                    GpuiRemoteSidebarRpcMode::Awaited,
                    cx,
                )
                .detach();
            return;
        }
        let path = if request.action == "wake" {
            "/api/wakeSession"
        } else {
            "/api/sleepSession"
        };
        let task = self.start_gpui_remote_sidebar_rpc(
            &machine_id,
            path,
            Some(params),
            timeout,
            GpuiRemoteSidebarRpcMode::Awaited,
            cx,
        );
        let sleeping = request.action == "sleep";
        cx.spawn(async move |this, cx| {
            let ok = task.await.is_ok();
            let _ = this.update(cx, |this, cx| {
                if ok && sleeping {
                    if let Some(replacement) = replacement {
                        this.gx_store_open_remote_tab(&replacement, cx);
                    }
                }
                this.gx_store_finish_tab_lifecycle(request.request_id, ok, cx);
            });
        })
        .detach();
    }

    fn gx_store_open_remote_tab(&mut self, session: &SessionKey, cx: &mut gpui::Context<Self>) {
        let action =
            open_remote_session_terminal(&session.to_sidebar_session_id(), false, None, false);
        self.receive_sidebar_native_project_path_action_payload(&action.to_string(), cx);
    }

    fn gx_store_patch_tab_lifecycle(
        &mut self,
        session: &SessionKey,
        lifecycle: LifecycleState,
        cx: &mut gpui::Context<Self>,
    ) {
        let now = now_ms();
        self.gx_store_tab_lifecycle_intent(
            Intent::PatchSession {
                session: session.clone(),
                patch: SessionPatch::lifecycle(
                    lifecycle,
                    now.saturating_add(LIFECYCLE_PATCH_TTL_MS),
                ),
            },
            cx,
        );
    }

    fn gx_store_tab_lifecycle_intent(&mut self, intent: Intent, cx: &mut gpui::Context<Self>) {
        let output = self.gx_store.core.handle(Event::Intent(intent), now_ms());
        if !output.changes.is_empty() {
            self.gx_store.sidebar_list.note_changes(&output.changes);
            self.gx_store_update_sidebar_list(cx);
        }
    }

    fn gx_store_finish_tab_lifecycle(
        &mut self,
        request_id: Option<u64>,
        ok: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(request_id) = request_id {
            self.finish_local_workspace_lifecycle_request(request_id, ok, cx);
        }
    }
}
