//! Work mode's session-link commands: the Link to submenu's Unlink and Back to automatic rows, the
//! picker's pick, and a merged-PR card's Clean up / Keep chips.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/link_menu.rs (the rows), apps/desktop/src/app/
//! work_link_picker_modal_lifecycle.rs (the picker), apps/desktop/src/app/native_sidebar/
//! work_chips.rs (the chips), server/src/server/route_http/work_links.rs (`/api/answerWorkCleanup`).

use ghostex_gx_core::SessionKey;
use serde_json::{Map, Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Answers the wrapped `setSessionWorkLinks` (Unlink, Back to automatic) and the top-level
    /// `answerWorkCleanup` (a card's Clean up / Keep chip). Both name the row by its sidebar id.
    pub(crate) fn gx_store_run_work_links(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match command.get("type").and_then(Value::as_str) {
            Some("command")
                if command["message"].get("type").and_then(Value::as_str)
                    == Some("setSessionWorkLinks") =>
            {
                let message = &command["message"];
                if let Some(session) = local_session(message.get("sessionId")) {
                    self.gx_store_set_session_work_links(
                        &session.project_id,
                        &session.session_id,
                        message.get("links").cloned().unwrap_or(Value::Null),
                        cx,
                    );
                }
                true
            }
            // CDXC:WorkMode 2026-11-18 WHY:
            // User: "clicking on keep doesn't do anything here". The card was another machine's
            // session (m7-arch, drawn in the merged sidebar), and the answer was sent only for this
            // computer's sessions, so both chips were dropped without a word and the offer stayed.
            // It goes to the machine that owns the session, like its links (`session_links_machine`);
            // that gxserver allows `/api/answerWorkCleanup` from a remote client (server/src/protocol.rs).
            Some("answerWorkCleanup") => {
                let answer = command.get("answer").and_then(Value::as_str);
                if let (Some(session), Some(answer @ ("cleanUp" | "keep"))) = (
                    command
                        .get("sessionId")
                        .and_then(Value::as_str)
                        .and_then(SessionKey::parse_sidebar_session_id),
                    answer,
                ) {
                    self.gx_store_answer_work_cleanup(session, answer, cx);
                }
                true
            }
            // A coordinator's merged-worktrees banner: Clean up all, Keep, Undo and its dismiss
            // (native_sidebar/threads.rs), sent to the coordinator's computer, which answers for
            // its threads there (server/src/server/work_cleanup_threads.rs).
            Some("answerWorkCleanupThreads") => {
                let answer = command.get("answer").and_then(Value::as_str);
                if let (Some(session), Some(answer @ ("cleanUp" | "keep" | "undo" | "dismiss"))) = (
                    command
                        .get("sessionId")
                        .and_then(Value::as_str)
                        .and_then(SessionKey::parse_sidebar_session_id),
                    answer,
                ) {
                    self.gx_store_answer_work_cleanup_threads(session, answer, cx);
                }
                true
            }
            _ => false,
        }
    }

    fn gx_store_answer_work_cleanup_threads(
        &mut self,
        coordinator: SessionKey,
        answer: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = json!({
            "projectId": coordinator.project_id,
            "sessionId": coordinator.session_id,
            "answer": answer,
        });
        let title = match answer {
            "cleanUp" => "Couldn't clean up the worktrees",
            "undo" => "Couldn't put the worktrees back",
            _ => "Couldn't save your answer",
        };
        let Some(remote) = self.session_links_machine(&coordinator) else {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                title,
                "This coordinator's computer is not connected.",
                cx,
            );
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(remote, "/api/answerWorkCleanupThreads", params).await;
            let _ = this.update(cx, |this, cx| {
                let threads = match result {
                    Ok(result) => result
                        .get("threads")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    Err(error) => {
                        this.dispatch_gpui_workspace_action_toast("error", title, &error.message, cx);
                        return;
                    }
                };
                let failed: Vec<&str> = threads
                    .iter()
                    .filter_map(|thread| thread.get("error").and_then(Value::as_str))
                    .collect();
                let kept_dirty = threads
                    .iter()
                    .filter(|thread| {
                        thread.pointer("/result/keptDirtyWorktree").and_then(Value::as_bool)
                            == Some(true)
                    })
                    .count();
                if let Some(first) = failed.first() {
                    this.dispatch_gpui_workspace_action_toast("error", title, first, cx);
                } else if kept_dirty > 0 {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        if kept_dirty == 1 {
                            "1 worktree kept"
                        } else {
                            "Some worktrees kept"
                        },
                        "A worktree with uncommitted changes was not removed; its thread is asleep.",
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// `/api/setSessionWorkLinks` with `links` merged into the request; the session's card picks
    /// the change up from the presentation delta gxserver sends.
    pub(crate) fn gx_store_set_session_work_links(
        &mut self,
        project_id: &str,
        session_id: &str,
        links: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut params = links.as_object().cloned().unwrap_or_else(Map::new);
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/setSessionWorkLinks", Value::Object(params)).await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't change the session's links",
                        &error.message,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    fn gx_store_answer_work_cleanup(
        &mut self,
        session: SessionKey,
        answer: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "answer": answer,
        });
        let clean_up = answer == "cleanUp";
        let Some(remote) = self.session_links_machine(&session) else {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                if clean_up {
                    "Couldn't clean up the session"
                } else {
                    "Couldn't save your answer"
                },
                "This session's machine is not connected.",
                cx,
            );
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(remote, "/api/answerWorkCleanup", params).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(answer) if clean_up => {
                    if answer.get("keptDirtyWorktree").and_then(Value::as_bool) == Some(true) {
                        this.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Session parked, worktree kept",
                            "The worktree has uncommitted changes, so it was not removed.",
                            cx,
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => this.dispatch_gpui_workspace_action_toast(
                    "error",
                    if clean_up {
                        "Couldn't clean up the session"
                    } else {
                        "Couldn't save your answer"
                    },
                    &error.message,
                    cx,
                ),
            });
        })
        .detach();
    }
}

/// This computer's session behind a sidebar row id; another machine's row has no Link to menu.
fn local_session(sidebar_session_id: Option<&Value>) -> Option<SessionKey> {
    sidebar_session_id
        .and_then(Value::as_str)
        .and_then(SessionKey::parse_sidebar_session_id)
        .filter(|session| session.machine.is_local())
}
