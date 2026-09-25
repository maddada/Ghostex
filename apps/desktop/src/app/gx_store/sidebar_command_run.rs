//! Running a saved Action: a project row's Actions button, a Global Action, and a Quick Access or
//! command palette run (`runSidebarCommand`), performed by the store.
//!
//! CDXC:CommandPane 2026-09-25 WHY:
//! The app runtime resolved the Action from its copy of the HUD, activated the clicked project
//! through its own focus and posted the command-action bridge. The HUD (gx_store/hud/) and the
//! focus are the store's now, so gx-core plans the run (`plan_sidebar_command_run`) and the host
//! performs it in the runtime's order: the project first, then the Action through the same entry
//! the bridge reached, after the project's context has been applied.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_command_run.rs.

use ghostex_gx_core::{Intent, SidebarCommandRun, plan_sidebar_command_run};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// `runSidebarCommand(commandId, message, scope)`.
    pub(crate) fn gx_store_run_sidebar_command(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let plan = plan_sidebar_command_run(
            self.gx_store.runtime_facts.hud.as_deref(),
            message,
            self.gx_store.core.focus().active_project.as_ref(),
        );
        match plan {
            SidebarCommandRun::Unsupported => {}
            SidebarCommandRun::OpenSettings => {
                self.open_app_modal_from_bridge(json!({ "modal": "settings", "type": "open" }), cx);
            }
            SidebarCommandRun::Run {
                focus_project,
                action,
            } => {
                if let Some(project) = focus_project {
                    self.gx_store_focus_intent(Intent::FocusProject { project }, cx);
                    self.gx_store_publish_workspace_focus(cx);
                }
                let payload = action.to_string();
                // Behind the project's context, which the publish above hands to the same window:
                // the Action runs in the project it names.
                self.gx_store_with_main_window(cx, move |app, window, cx| {
                    app.receive_sidebar_command_action_payload(&payload, window, cx);
                });
            }
        }
    }
}
