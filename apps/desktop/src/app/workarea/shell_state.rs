//! Shell-layout persistence and the project-scoped availability read the rest of this
//! directory starts from. Moved verbatim out of `app/workarea.rs` on 2026-09-20.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// Marks the shell layout dirty; one task saves it at most once a second while it is (gx_store/layout_persist.rs).
    pub(crate) fn persist_shell_layout_state(&self) {
        self.gx_store.layout_persist.mark_dirty();
    }

    /// Synchronous variant for the quit path; see `flush_gpui_workspace_shell_state`. It serializes the current state itself, so it never depends on the dirty mark, and it writes the focus state file a local selection may not have reached yet.
    pub(crate) fn flush_shell_layout_state(&mut self) {
        flush_gpui_workspace_shell_state(self);
        if !self.workspace_window_closing {
            persist_gpui_gxserver_presentation_focus_state(
                &self.presentation_focus_state_path(),
                &self.sidebar_gxserver_presentation_focus_state,
            );
        }
        // The sidebar's own state is written on a debounce, so the last clicks before a quit are
        // still only in memory (gx_store/sidebar_ui.rs).
        self.gx_store_flush_sidebar_ui_write();
        // The workspace session groups document is the fourth key and is written the same way: a
        // rename or a move whose write lost a lock race is still owed, and its debounced push has
        // not gone out either (gx_store/workspace_groups.rs).
        self.gx_store_flush_workspace_groups_write();
        // The project collections document is the fifth key, written the same way and for the same
        // reason; the Spaces document has no stored key at all and answers at once
        // (gx_store/project_docs.rs).
        self.gx_store_flush_project_docs();
        // The last-seen copy of every remote machine is written on a two-second debounce, so the
        // rows of the last seconds of a session are still only in memory, and this app must be the
        // later writer of a key the sidebar page still writes too (gx_store/remote_last_seen.rs).
        self.gx_store_flush_last_seen_writes();
        // This computer's own copy, which the next launch's first frame is drawn from
        // (gx_store/local_last_seen.rs).
        self.gx_store_flush_local_last_seen();
    }

    pub(crate) fn project_scoped_workarea_availability(&self) -> ProjectScopedWorkareaAvailability {
        project_scoped_workarea_availability_from_latest_sidebar_snapshot(
            self.latest_sidebar_project_snapshot.as_ref(),
            ProjectScopedWorkareaAvailability::from_env_bridge(),
        )
    }

    pub(crate) fn refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-24-10:12:
        The real CEF surface map is active app-level ownership, not proof-only evidence. Refresh may prune already-owned Source/Kanban/Automate/Manage CefSurface entities when their safe gate disappears, but it still cannot create a surface, issue or store a URL, synthesize fallback navigation, mount hidden/offscreen views, use WKWebView/WebKit, or persist/log private runtime data.

        CDXC:Workarea 2026-06-28-17:09:
        The old slot, URL-issuance, startup-readiness, and owner-gate proof maps are removed from runtime state. Refresh now only prunes already-owned project workarea CefSurface entities whose current explicit project context can no longer provide a direct runtime URL.

        CDXC:Workarea 2026-06-29-00:15:
        Refresh must also prune already-owned surfaces whose stored runtime URL identity differs from the current direct runtime URL so a valid new project cannot inherit the previous project's slot-owned CEF view.
        */
        self.prune_project_workarea_runtime_cef_surfaces_for_current_gates(cx)
    }
}
