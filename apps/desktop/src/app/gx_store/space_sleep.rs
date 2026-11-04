//! A Space icon's Sleep Space inside the app.
//!
//! CDXC:Spaces 2026-09-22 DECISION:
//! User: the Space icon menu shows only Sleep Inactive. Sleep Space still sleeps everything in the
//! Space, sessions and open views alike, all of it staying in place asleep, and Sleep Others does
//! the same outside the Space; Sleep Inactive sleeps only the Space's idle sessions. gx-core names each set
//! (`plan_space_sleep`); this file performs the two halves of the chosen one in a fixed order. The views go first, because they are local and instant: the active
//! project's awake views sleep through the tab strip's own Sleep (`sleep_titlebar_view`), so each
//! tab stays in the strip as a sleeping one, and every other project of the Space keeps only
//! parked browser pages alive, which are dropped the way that project's per-tab sleep drops them,
//! leaving the tabs in its model. The rows then go out as ONE `setSessionsSleeping` through the
//! same dispatcher, so they take the bulk path's pacing and the lifecycle path's declined leg
//! unchanged, and this file owns no second copy of either.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/space_sleep.rs,
//! apps/desktop/src/app/gx_store/sidebar_bulk.rs (`setSessionsSleeping`),
//! apps/desktop/src/app/native_sidebar/selectors.rs (the menu row).

use ghostex_gx_core::{SpaceSleepPlan, SpaceSleepPlans, SpaceSleepScope, plan_space_sleep};
use serde_json::{Value, json};

use super::host::now_ms;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// The Space's three sets, from the store's current inputs. `None` when Spaces are off or the
    /// id names no Space of the selected machine.
    pub(crate) fn gx_store_space_sleep_plans(&self, space_id: &str) -> Option<SpaceSleepPlans> {
        plan_space_sleep(
            &self.gx_store.core,
            &self.gx_store.sidebar_list.last_inputs,
            space_id,
            now_ms(),
        )
    }

    /// Whether one of the sleeps would do anything: an awake row, or a project with a live view.
    /// The menu row is disabled otherwise.
    pub(crate) fn gx_store_space_sleep_has_work(
        &self,
        plans: Option<&SpaceSleepPlans>,
        scope: SpaceSleepScope,
    ) -> bool {
        let Some(plan) = plans.map(|plans| plans.plan(scope)) else {
            return false;
        };
        !plan.session_ids.is_empty()
            || plan
                .project_ids
                .iter()
                .any(|project_id| self.project_has_awake_views(project_id))
    }

    /// The active project's awake modes, or what a project that is not mounted keeps: its live
    /// browser pages and the view pages parked for its return (`holds_parked_project_pages`).
    fn project_has_awake_views(&self, project_id: &str) -> bool {
        if self.agents_workspace_project_id.as_deref() == Some(project_id) {
            self.project_editor_shell
                .lifecycle_modes()
                .into_iter()
                .any(|mode| self.project_editor_shell.is_mode_awake(mode))
        } else {
            self.parked_browser_runtimes_by_project
                .get(project_id)
                .is_some_and(|runtime| !runtime.surface_tab_ids().is_empty())
                || self.holds_parked_project_pages(project_id)
        }
    }

    /// Sleeps one project's views and leaves their tabs in place: the active project's awake
    /// views through the tab strip's own Sleep, a parked project's browser pages the way its
    /// per-tab sleep drops them and its parked view pages (Code, Files, website views, the ones
    /// Keep views loaded holds included). A workspace switch sleeps the projects outside the new
    /// workspace with it too (workspace_windows/window_workspace.rs).
    pub(crate) fn sleep_project_views(&mut self, project_id: &str, cx: &mut gpui::Context<Self>) {
        // A page opened for it in the background waits for a click like the rest instead of
        // loading on the way back in: the later sleep wins.
        self.pending_parked_browser_opens.remove(project_id);
        // The project on screen too: Browser can be asleep on its timer or the awake cap with pages
        // of its tabs still waiting to be made, which the Sleep below then skips.
        self.sleep_parked_browser_project(project_id, cx);
        if self.agents_workspace_project_id.as_deref() == Some(project_id) {
            for mode in self.project_editor_shell.lifecycle_modes() {
                if self.project_editor_shell.is_mode_awake(mode) {
                    self.sleep_titlebar_view(mode, cx);
                }
            }
        } else {
            self.release_parked_project_pages(Some(project_id), None);
            // Its active view is no longer held awake for the way back in (`active_view_awake`).
            if let Some(state) = self.project_view_states_by_project.get_mut(project_id) {
                state.active_view_awake = false;
            }
        }
    }

    /// Answers the Space menu's `sleepSpace`, whose `scope` is `space`, `inactive` or `others`.
    /// Returns `true` when the command is one, whether or not it resolved to any work.
    pub(crate) fn gx_store_run_sidebar_space_sleep(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("sleepSpace") {
            return false;
        }
        let Some(space_id) = command.get("spaceId").and_then(Value::as_str) else {
            return true;
        };
        let scope = command
            .get("scope")
            .and_then(Value::as_str)
            .and_then(SpaceSleepScope::parse)
            .unwrap_or(SpaceSleepScope::Space);
        let Some(plans) = self.gx_store_space_sleep_plans(space_id) else {
            return true;
        };
        let plan: SpaceSleepPlan = plans.plan(scope).clone();
        for project_id in &plan.project_ids {
            self.sleep_project_views(project_id, cx);
        }
        if !plan.session_ids.is_empty() {
            self.dispatch_native_sidebar_ui(
                json!({
                    "type": "command",
                    "message": {
                        "type": "setSessionsSleeping",
                        "sessionIds": plan.session_ids,
                        "sleeping": true,
                    },
                }),
                cx,
            );
        }
        true
    }
}
