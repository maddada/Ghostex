// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: menu-bar, status item and command-palette activations

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn set_gpui_reduce_motion_enabled(
        &mut self,
        reduce_motion_enabled: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gpui_reduce_motion_enabled == reduce_motion_enabled {
            return;
        }
        self.gpui_reduce_motion_enabled = reduce_motion_enabled;
        cx.notify();
    }

    pub(crate) fn dispatch_gpui_status_activation(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:StatusPet 2026-06-26-05:07:
        Visible GPUI status activation returns only a bounded session id to the store's existing focusSession path (gx_store/activation_focus.rs; the sidebar runtime's until 2026-09-25). Rust never wakes, creates, restores, or materializes a session from these clicks. The callback carries no generic event bus, paths, URLs, commands, tokens, titles, or terminal text.
        */
        if !gpui_status_bridge_id_allowed(session_id) {
            return false;
        }
        // Shown on its computer, the way a notification click shows it (gx_store/activation_focus.rs).
        self.gx_store_show_activated_session(session_id.trim(), cx)
    }

    pub(crate) fn dispatch_gpui_menu_bar_project_activation(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:StatusPet 2026-06-26-06:05:
        Menu-bar project rows carry only one bounded project id. Rust does not add a generic bus or derive paths or titles from project-only clicks; the store opens the project (gx_store/project_activation.rs), which lands on its remembered session or creates the default one.
        */
        if !gpui_status_bridge_id_allowed(project_id) {
            return false;
        }
        // A project opens from the store's newest selection (gx_store/burst.rs).
        self.gx_store_flush_local_selection(cx);
        self.gx_store_activate_project(project_id, cx)
    }

    pub(crate) fn dispatch_gpui_menu_bar_session_activation(
        &mut self,
        project_id: &str,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:StatusPet 2026-06-26-06:05:
        Menu-bar session rows route through a fixed first-party sidebar callback carrying bounded project/session ids. The sidebar then reuses normal session-card focus, including the existing WorkspaceTerminalFocus Rust handoff for local materialization, without renderer JSON, paths, commands, stdout/stderr, tokens, terminal content, or fallback attach behavior.
        */
        if !gpui_status_bridge_id_allowed(project_id) || !gpui_status_bridge_id_allowed(session_id)
        {
            return false;
        }
        let (project_id, session_id) = (project_id.trim(), session_id.trim());
        if project_id.is_empty() || session_id.is_empty() {
            return false;
        }
        let focus_id = crate::app::gx_store::menu_bar_session_focus_id(project_id, session_id);
        self.gx_store_show_activated_session(&focus_id, cx)
    }

    pub(crate) fn dispatch_gpui_project_board_conversation_request(
        &mut self,
        request: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // The Kanban page's board request is first-party JSON, but Rust still
        // bounds it and requires the envelope fields before the store
        // (gx_store/create/board.rs) handles it.
        if !request.is_object() {
            return false;
        }
        let request_id_valid = manage_request_string(request, "requestId")
            .map(|value| value.trim().to_string())
            .is_some_and(|value| {
                !value.is_empty() && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            });
        if !request_id_valid {
            return false;
        }
        // macOS `appendProjectBoardDebugLog` parity: the board page's debug
        // breadcrumbs persist to the GPUI project-board support log
        // (scenario-gated) while gx_store/create/board.rs answers the state echo.
        // Details arrive as a JSON string and are parsed at the writer
        // boundary like the Swift writers, then sanitized.
        if request.get("action").and_then(serde_json::Value::as_str) == Some("appendDebugLog") {
            let event = request
                .get("event")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if !event.is_empty() {
                let details = request
                    .get("details")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .unwrap_or(serde_json::Value::Null);
                support_logs::append(support_logs::GpuiSupportLog::ProjectBoard, event, details);
            }
        }
        self.gx_store_run_board_conversation_request(request, cx)
    }

    pub(crate) fn dispatch_gpui_command_palette_session_focus(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // Palette rows carry projected sidebar session ids (combined local or
        // remote-shaped). The string is bounded and its shape checked here, then it
        // takes the reviewed focusSession routing (gx_store/activation_focus.rs).
        if session_id.is_empty()
            || session_id.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            || session_id.chars().any(char::is_control)
        {
            return false;
        }
        let Some(session_id) = crate::app::gx_store::palette_session_focus_id(session_id) else {
            return false;
        };
        self.gx_store_show_activated_session(session_id, cx)
    }

    pub(crate) fn dispatch_gpui_command_palette_run_sidebar_command(
        &mut self,
        command_id: &str,
        run_mode: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.dispatch_gpui_run_sidebar_command_with_scope(command_id, run_mode, None, cx)
    }

    /*
    CDXC:AgentLauncher 2026-08-01-19:00:
    A run-by-id selector cannot tell a Global Action from a Project Action with
    the same id, so the tab strip stamps its scope and the store
    (gx_store/sidebar_command_run.rs) resolves that list exclusively. The Command Palette keeps sending no scope,
    which the store reads as project: unchanged behaviour for every existing
    caller.
    */
    pub(crate) fn dispatch_gpui_run_sidebar_command_with_scope(
        &mut self,
        command_id: &str,
        run_mode: Option<&str>,
        scope: Option<&'static str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // The palette payload is an Action selector only (command id + optional
        // run mode). The store (gx_store/sidebar_command_run.rs) resolves the trusted saved/HUD command
        // and executes it;
        // renderer-supplied command text, URLs, or paths never enter this path.
        let bounded = |value: &str| {
            !value.is_empty()
                && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !value.chars().any(char::is_control)
        };
        if !bounded(command_id) || run_mode.is_some_and(|run_mode| !bounded(run_mode)) {
            return false;
        }
        self.gx_store_run_activated_sidebar_command(command_id, run_mode, scope, cx)
    }

    pub(crate) fn dispatch_gpui_workspace_tab_session_selected(
        &mut self,
        project_id: &str,
        session_id: &str,
        local_was_sleeping: bool,
        local_runtime_missing: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:FocusRouting 2026-06-26-08:01:
        Native GPUI workspace tab selection has already mutated Rust pane state, so the sidebar callback may update only local or machine-scoped remote presentation focus. Use a dedicated first-party callback instead of menu-bar session activation so tab clicks cannot re-enter the WorkspaceTerminalFocus attach path and bounce the selected tab.

        CDXC:FocusRouting 2026-06-27-00:33:
        MacOS reattaches a stale locally sleeping pane tab when gxserver already reports that canonical session running. Send only a true `localWasSleeping` flag for that reconciliation check; ordinary tab selections remain one-way sidebar focus updates.

        CDXC:FocusRouting 2026-07-11:
        Restored-after-restart Running tabs can have no live terminal owner, no parked owner, and no pending attach payload behind them; selecting one shows an empty body. Send only a true `localRuntimeMissing` flag so the store can reconcile through one bounded WorkspaceTerminalFocus when gxserver still reports that canonical session running, reusing the exact gxserver attach pipeline instead of mounting anything from renderer input.

        Sidebar visibility follows the actual rendered workspace rather than a
        click-history set. Carry the bounded gxserver ids for every active
        rendered Agents leaf or companion slot with the selected id so split
        siblings keep the visible tier and hidden tabs lose it immediately.

        CDXC:FocusRouting 2026-09-25 WHY:
        Focus is owned by the Rust store alone (the user decision is recorded in gx_store/local_focus.rs). A local session's selection changes the store at once and its follow-up (the workspace publish, the attention acknowledge, the remembered session, the reconcile the two flags ask for) runs once per burst, about 120 ms after the last selection (gx_store/burst.rs). A remote session's selection is taken by the store at once and published (gx_store_select_remote_session in gx_store/local_focus.rs). This supersedes the 2026-09-19 note that told the sidebar runtime about both; every rule above about what a selection carries still holds.
        */
        if !gpui_status_bridge_id_allowed(project_id) || !gpui_status_bridge_id_allowed(session_id)
        {
            return false;
        }
        // Machine-scoped ids (`remote:<machine>:...`) name a remote session or project.
        if !project_id.starts_with("remote:") && !session_id.starts_with("remote:") {
            let key = GpuiLocalWorkspaceSessionKey {
                project_id: project_id.to_string(),
                session_id: session_id.to_string(),
            };
            self.gx_store_select_local_session(&key, local_was_sleeping, local_runtime_missing, cx);
            return true;
        }
        // The remote selection follows the store's newest local one, whose follow-up runs first.
        self.gx_store_flush_local_selection(cx);
        let _ = (local_was_sleeping, local_runtime_missing);
        if !self.gx_store_select_remote_session(project_id, session_id, cx) {
            return false;
        }
        self.gx_store_note_remote_tab_selection();
        true
    }
}
