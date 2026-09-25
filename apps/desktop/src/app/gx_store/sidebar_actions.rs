//! Running a sidebar action in Rust instead of sending it through the old runtime.
//!
//! CDXC:ContextMenus 2026-09-20 WHY:
//! Every menu row already carries the payload the old runtime answered, and since M4c the store
//! builds the row. What was left was the answer itself: a copy action reached the clipboard by
//! leaving Rust for QuickJS, having an id resolved there, and coming straight back over the fixed
//! native bridge. The store holds everything that resolution reads, so the round trip bought
//! nothing and cost a frame. The rule this file follows is that the CALL must be identical, not
//! merely the outcome: gx-core decides which call to make and the host only performs it.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/ (what a payload does).

use web_time::Instant;

use ghostex_gx_core::{
    ActionEffect, READ_ONLY_MESSAGE_TYPES, SidebarActionPlan, plan_read_only_action,
};
use gpui::ClipboardItem;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_copy_to_clipboard;

/// What this app run did with the actions the store owns. Memory only; the record lines are built
/// from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarActionCounters {
    /// Payloads the store answered.
    pub(crate) handled: u64,
    /// Of those, the ones whose answer is deliberately nothing
    /// (`handleUnsupportedSidebarMessage`).
    pub(crate) nothing: u64,
    pub(crate) copy_text: u64,
    pub(crate) native_project_path: u64,
    pub(crate) toast: u64,
    /// Payloads the store owns but did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
}

impl GhostexGpuiApp {
    /// Answers a sidebar command in Rust when the store owns it. Returns whether it did, in which
    /// case the command must NOT also be sent to the old runtime.
    ///
    /// Only a `{ type: 'command', message }` payload can be owned here: every other command type
    /// is either the sidebar's own state (sidebar_ui_commands.rs) or still the old runtime's.
    pub(crate) fn gx_store_run_sidebar_action(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message") else {
            return false;
        };
        // Only while the list is ready: in the launch window the ids in this payload name rows of
        // a list nobody has drawn. Most commands never get here (the dispatch drops them at the
        // door), but the planners have callers of their own, so the gate stays. The decline is
        // counted by name rather than by planning and throwing the plan away, so a run in the
        // launch window costs this comparison and nothing else.
        if !self.gx_store_sidebar_list_ready() {
            if message
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| READ_ONLY_MESSAGE_TYPES.contains(&kind))
            {
                self.gx_store.sidebar_actions.declined_source += 1;
            }
            return false;
        }
        let started = Instant::now();
        let plan = {
            let store = &self.gx_store;
            plan_read_only_action(&store.core, &store.sidebar_list.last_inputs, message)
        };
        let Some(plan) = plan else {
            return false;
        };
        let plan_us = started.elapsed().as_micros() as u64;
        self.run_sidebar_action_plan(&plan, cx);
        let kind = message
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.gx_store.diagnostics.sidebar_action_ran(
            &kind,
            &plan,
            plan_us,
            self.gx_store.sidebar_actions,
        );
        true
    }

    fn run_sidebar_action_plan(&mut self, plan: &SidebarActionPlan, cx: &mut gpui::Context<Self>) {
        self.gx_store.sidebar_actions.handled += 1;
        if plan.effects.is_empty() {
            self.gx_store.sidebar_actions.nothing += 1;
        }
        for effect in &plan.effects {
            match effect {
                ActionEffect::CopyText { text } => {
                    self.gx_store.sidebar_actions.copy_text += 1;
                    self.gpui_copy_session_details_text(text, cx);
                }
                ActionEffect::NativeProjectPathAction { payload } => {
                    self.gx_store.sidebar_actions.native_project_path += 1;
                    // The same entry point the bridge calls, with the same payload the old runtime
                    // would have posted, so the whole remote-revalidation contract this action has
                    // is the one that already exists and there is no second copy of it.
                    self.receive_sidebar_native_project_path_action_payload(
                        &payload.to_string(),
                        cx,
                    );
                }
                ActionEffect::Toast {
                    level,
                    title,
                    description,
                } => {
                    self.gx_store.sidebar_actions.toast += 1;
                    // `createAppToastRequest` trims the title and drops a description that repeats
                    // it; `gpui_app_toast_from_bridge_message` does both again on this side, so the
                    // request is built from the plan's own strings.
                    let mut request =
                        json!({ "level": level.as_str(), "title": title, "type": "toast" });
                    if let Some(description) = description {
                        request["description"] = Value::String(description.clone());
                    }
                    self.receive_gpui_app_toast_bridge_message(&request, cx);
                }
                // The open family has its own host (gx_store/sidebar_open.rs) and its own
                // dispatcher arm; a read-only payload that planned one would be a planner that
                // had drifted, so it is asserted rather than performed here.
                other => debug_assert!(false, "unexpected read-only effect: {other:?}"),
            }
        }
    }

    /// The one clipboard write for a session's copied details: the sidebar's copy actions and
    /// Quick Access's copy rows (app/quick_access/host.rs) both come here, so they stay one
    /// function. The app modal host's `copySessionDetails` arm that also did was deleted with the
    /// QuickJS runtime, its last sender.
    pub(crate) fn gpui_copy_session_details_text(
        &mut self,
        details_text: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if details_text.trim().is_empty() {
            return;
        }
        gpui_copy_to_clipboard(ClipboardItem::new_string(details_text.to_string()), cx);
    }
}
