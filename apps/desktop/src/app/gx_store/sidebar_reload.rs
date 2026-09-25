//! Full Reload and Split Right, performed by the store.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! Neither of these calls the daemon itself. Full Reload is the sleep and the wake the store
//! already owns, run ONE AT A TIME because the TypeScript awaited the first before it started the
//! second and because a wake that overtakes its own sleep reloads nothing; Split Right is a
//! selection that carries where the pane goes, and for a sleeping row it is the same wake with a
//! placement on it. So this file waits and it selects, and every decision it acts on is gx-core's.
//!
//! **The counters that prove this path fires in the app** are `reloads`, `reloadLegs`, `remounts`,
//! `splits`, `splitsWoken` and `splitsPlaced` on `gxStore.sidebarLifecycle`. A run where the user
//! used Advanced > Full Reload or Advanced > Split Right and any of them is zero means the command
//! never reached here, which is the failure the envelope bug of piece 3d was.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/reload.rs,
//! packages/gx-core/src/sidebar_actions/split.rs.

use ghostex_gx_core::{
    FocusOptions, SplitAction, owns_reload_message, owns_reload_set_message,
    owns_remote_session_message, owns_split_message, plan_full_reload, plan_reload_set,
    plan_split_right, reload_continues_after,
};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::model::GpuiLocalWorkspaceSessionKey;

impl GhostexGpuiApp {
    /// Answers `fullReloadSession` and `restartSession` when the store owns the row.
    pub(crate) fn gx_store_run_sidebar_reload(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(message) = wrapped_message(command) else {
            return false;
        };
        if !owns_reload_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        match self.gx_store_start_reload(message, cx) {
            Some(task) => {
                task.detach();
                true
            }
            None => false,
        }
    }

    /// The same Full Reload as a task that resolves to whether it came home without a failed call,
    /// which is what a set reload waits on before it moves to the next row. `None` means the
    /// single-session planner does not own the row.
    fn gx_store_start_reload(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Task<bool>> {
        // A remote row, a browser row and an id that does not parse are refused inside the
        // planner, each one for a reason written down there.
        let plan = plan_full_reload(&self.gx_store.core, message)?;
        self.gx_store.sidebar_lifecycle.reloads += 1;
        self.gx_store.sidebar_lifecycle.reload_legs += plan.legs.len() as u64;
        self.gx_store
            .diagnostics
            .sidebar_reload_ran(&plan, self.gx_store.sidebar_lifecycle);
        let legs = plan.legs;
        Some(cx.spawn(async move |this, cx| {
            for leg in legs {
                // Each leg goes through the single-session path, which owns the call, the declined
                // leg, the replacement focus and the echo guard. The wait is for the ANSWER and not
                // for a timer: the provider has to be dead before it is asked to come back, and a
                // sleep whose call failed stops the reload, as the TypeScript's first `await`
                // rejecting did.
                let started = this.update(cx, |this, cx| this.gx_store_start_lifecycle(&leg, cx));
                let Ok(Some(task)) = started else {
                    return false;
                };
                if !reload_continues_after(task.await) {
                    let _ = this.update(cx, |this, _| {
                        this.gx_store.sidebar_lifecycle.reloads_stopped += 1;
                    });
                    return false;
                }
            }
            true
        }))
    }

    /// Answers `fullReloadProjectZmxSessions` and `fullReloadGroup`: a Full Reload of each row of
    /// the set, ONE AT A TIME, stopping at the first that fails (gx-core `reload_set.rs`).
    ///
    /// Both are wrapped gxserver messages (`{ type: 'command', message }`), posted by the project
    /// menu and the user-made group menu.
    pub(crate) fn gx_store_run_sidebar_reload_set(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(message) = wrapped_message(command) else {
            return false;
        };
        if !owns_reload_set_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        // A user-made group's members are the workspace session groups document, so its stored key
        // has to be in hand first, for the reason every edit of it waits: an answer computed against
        // a document this app has not read would reload the wrong rows, or none.
        if message.get("type").and_then(Value::as_str) == Some("fullReloadGroup")
            && !self.gx_store_restore_workspace_groups(cx)
        {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        let plan = {
            let store = &self.gx_store;
            plan_reload_set(&store.core, store.workspace_groups.sync.document(), message)
        };
        let Some(plan) = plan else {
            return false;
        };
        self.gx_store.sidebar_lifecycle.reload_sets += 1;
        self.gx_store.sidebar_lifecycle.reload_set_rows += plan.messages.len() as u64;
        cx.spawn(async move |this, cx| {
            let mut index = 0;
            while let Some(message) = plan.messages.get(index) {
                let started = this.update(cx, |this, cx| {
                    // The route the dispatcher takes for this row: a remote row down its machine's
                    // tunnel, a local one through the single-session reload, and anything neither
                    // owns to the old runtime, which is the one row nothing here can wait for.
                    if owns_remote_session_message(message) {
                        if let Some(task) = this.gx_store_start_remote(message, cx) {
                            return Some(task);
                        }
                    } else if let Some(task) = this.gx_store_start_reload(message, cx) {
                        return Some(task);
                    }
                    this.dispatch_native_sidebar_command(message.clone(), cx);
                    None
                });
                let completed = match started {
                    Err(_) => return,
                    Ok(Some(task)) => task.await,
                    Ok(None) => true,
                };
                match plan.step_after(index, completed) {
                    Some(next) => index = next,
                    None => {
                        if !completed {
                            let _ = this.update(cx, |this, _| {
                                this.gx_store.sidebar_lifecycle.reload_sets_stopped += 1;
                            });
                        }
                        return;
                    }
                }
            }
        })
        .detach();
        true
    }

    /// Answers `splitSessionRight` when the store owns the row.
    pub(crate) fn gx_store_run_sidebar_split(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(message) = wrapped_message(command) else {
            return false;
        };
        if !owns_split_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        let Some(plan) = plan_split_right(&self.gx_store.core, message) else {
            return false;
        };
        self.gx_store.sidebar_lifecycle.splits += 1;
        if plan.acknowledge_attention {
            // The old runtime owns the attention timers and the minimum visible window, so the
            // acknowledgement is queued for the next tell exactly as a local selection's is, rather
            // than reimplemented here (gx_store/burst.rs).
            self.gx_store_queue_attention_acknowledge(
                GpuiLocalWorkspaceSessionKey {
                    project_id: plan.session.project_id.clone(),
                    session_id: plan.session.session_id.clone(),
                },
                cx,
            );
        }
        self.gx_store
            .diagnostics
            .sidebar_split_ran(&plan, self.gx_store.sidebar_lifecycle);
        match &plan.action {
            // The Quick Automations row, where the TypeScript returned before it did anything.
            SplitAction::Nothing => {}
            SplitAction::Wake(wake) => {
                self.gx_store.sidebar_lifecycle.splits_woken += 1;
                let wake = wake.clone();
                if let Some(task) = self.gx_store_start_lifecycle(&wake, cx) {
                    task.detach();
                }
            }
            SplitAction::Focus => {
                self.gx_store.sidebar_lifecycle.splits_placed += 1;
                let session = plan.session.clone();
                self.gx_store_select_local_workspace_session(
                    &session,
                    None,
                    FocusOptions {
                        force_remount: false,
                        split_right: true,
                    },
                    cx,
                );
            }
        }
        true
    }
}

/// The inner message of a gxserver command envelope. Both of these payloads are wrapped ones, not
/// renderer commands: they reach the runtime through `controller.ts:131`, unlike `sessionAction`
/// and `batch`, which arrive at the top level.
fn wrapped_message(command: &Value) -> Option<&Value> {
    if command.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }
    command.get("message")
}
