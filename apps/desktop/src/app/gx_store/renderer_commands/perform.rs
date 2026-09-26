//! Performing a validated renderer command through the path the same action takes in the app.
//!
//! Every verb goes where the app's own gesture for it goes, so a later port of that gesture moves
//! the CLI with it: `ghostex focus` is the status indicator's focus (`focusSession` in the old
//! runtime, exactly what the runtime's renderer handler called), Full Reload, Restart and Close
//! After Done are the sidebar row's commands, `switch-project` and `focus-group` are the menu bar's
//! project activation, `move-project` is the sidebar's project drag, `run-command` is the titlebar
//! Action runner, and `open` is the operating system's open-file request.
//!
//! SEE-ALSO: packages/gx-core/src/renderer_commands/ (validation and target resolution),
//! apps/desktop/src/app/status_pet.rs (`dispatch_gpui_status_pet_activation`,
//! `dispatch_gpui_menu_bar_project_activation`; F3 moves both off the runtime).

use std::path::PathBuf;

use ghostex_gx_core::protocol::RendererCommand;
use ghostex_gx_core::{
    OpenPathTarget, OpenPathsMode, RendererCommandError, RendererSession, RendererVerb,
    plan_project_step, plan_renderer_command,
};
use gpui::Window;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::consts::{
    GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE, GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
    GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE,
    GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION,
};
use crate::app::helpers::{
    GpuiTitlebarActionRunMode, GpuiTitlebarActionType, gpui_os_integration_project_root_for_path,
    gpui_sidebar_open_browser_url_from_json,
};
use crate::app::model::{PendingSourceFileOpen, PendingSourceFileOpenOrigin};

/// `{ ghostexId, projectId, sessionId }`, the session block of the old runtime's answers.
fn session_json(session: &RendererSession) -> Value {
    json!({
        "ghostexId": session.sidebar_session_id,
        "projectId": session.key.project_id,
        "sessionId": session.key.session_id,
    })
}

impl GhostexGpuiApp {
    pub(super) fn gx_store_perform_renderer_command(
        &mut self,
        command: &RendererCommand,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<Value, RendererCommandError> {
        let verb = plan_renderer_command(&self.gx_store.core, &command.action, &command.payload)?;
        let action = command.action.as_str();
        match verb {
            RendererVerb::FocusSession(session) => {
                if !self.dispatch_gpui_status_pet_activation(&session.sidebar_session_id, cx) {
                    return Err(RendererCommandError::BridgeUnavailable);
                }
                Ok(json!({ "ok": true, "session": session_json(&session) }))
            }
            RendererVerb::RenameCommand {
                session,
                title,
                command,
            } => {
                // Surfacing the session first is what the old runtime did before it posted the
                // rename; the Rust delivery then focuses and waits for the terminal to mount.
                if !self.dispatch_gpui_status_pet_activation(&session.sidebar_session_id, cx) {
                    return Err(RendererCommandError::BridgeUnavailable);
                }
                let payload = json!({
                    "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION,
                    "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE,
                    "projectId": session.key.project_id,
                    "sessionId": session.key.session_id,
                    "title": title,
                    "command": command,
                });
                self.receive_sidebar_workspace_terminal_rename_command_payload(
                    &payload.to_string(),
                    cx,
                );
                Ok(json!({
                    "accepted": true,
                    "action": "renameCommand",
                    "ok": true,
                    "session": session_json(&session),
                }))
            }
            RendererVerb::RunCommand { command_id } => {
                self.gx_store_run_renderer_action_button(&command_id, window, cx)?;
                Ok(json!({ "accepted": true, "action": action, "ok": true }))
            }
            RendererVerb::ReadResourcesSnapshot => {
                Ok(self.gpui_native_resources_snapshot_export(cx))
            }
            RendererVerb::UpdateSettingsPatch { patch, keys } => {
                // CDXC:Settings 2026-09-09 DECISION:
                // User: `ghostex settings set` writes through the running desktop app, never the
                // settings file, so a CLI change takes the exact save and fan-out path a Settings
                // modal save takes (Rust merges the patch onto the stored snapshot and hydrates
                // every surface). The renderer command carries only a flat key/value patch; the
                // CLI validates keys and values against the generated settings catalog before it
                // dispatches.
                // SEE-ALSO: server/src/ghostex_cli/settings.rs, skills/ghostex-help.
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &json!({
                        "patch": Value::Object(patch),
                        "source": "cli:settings",
                        "type": "updateSettingsPatch",
                    }),
                    cx,
                );
                Ok(json!({ "accepted": true, "keys": keys, "ok": true }))
            }
            RendererVerb::OpenSettings { tab, search_query } => {
                // `ghostex settings open [<key>]` lands on the Settings modal with the tab and
                // search prefilled, the same open message the titlebar Tips rows use, so settings
                // an agent may not write are one command away for the user.
                let mut message = json!({ "initialTab": tab, "modal": "settings", "type": "open" });
                if let Some(query) = &search_query {
                    message["initialSearchQuery"] = json!(query);
                }
                self.open_app_modal_from_bridge(message, cx);
                Ok(json!({ "ok": true, "searchQuery": search_query, "tab": tab }))
            }
            RendererVerb::OpenBrowser {
                project_id,
                reuse,
                url,
            } => {
                let mut payload = json!({
                    "reuse": reuse,
                    "type": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
                    "url": url,
                    "version": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
                });
                if let Some(project_id) = project_id {
                    payload["projectId"] = json!(project_id);
                }
                if let Ok(message) = gpui_sidebar_open_browser_url_from_json(&payload.to_string()) {
                    self.open_browser_url_from_renderer_command(message, window, cx);
                }
                Ok(json!({ "accepted": true, "action": action, "ok": true }))
            }
            RendererVerb::ReloadSession {
                session,
                message_type,
            } => {
                self.dispatch_native_sidebar_command(
                    json!({ "type": message_type, "sessionId": session.sidebar_session_id }),
                    cx,
                );
                Ok(json!({
                    "accepted": true,
                    "action": action,
                    "ok": true,
                    "session": session_json(&session),
                }))
            }
            RendererVerb::ToggleCloseAfterDone(session) => {
                self.dispatch_native_sidebar_command(
                    json!({ "type": "toggleCloseAfterDone", "sessionId": session.sidebar_session_id }),
                    cx,
                );
                Ok(json!({
                    "accepted": true,
                    "action": action,
                    "ok": true,
                    "session": session_json(&session),
                }))
            }
            RendererVerb::ActivateProject { project_id } => {
                if !self.dispatch_gpui_menu_bar_project_activation(&project_id, cx) {
                    return Err(RendererCommandError::BridgeUnavailable);
                }
                Ok(
                    json!({ "accepted": true, "action": action, "ok": true, "projectId": project_id }),
                )
            }
            RendererVerb::MoveProject {
                project_id,
                direction,
            } => {
                let step = plan_project_step(
                    &self.gx_store.core,
                    &self.gx_store.sidebar_list.last_inputs,
                    &project_id,
                    direction,
                )?;
                let moved = match step {
                    Some(step) => self.gx_store_run_project_move(&step.to_command(), cx),
                    None => false,
                };
                Ok(json!({ "accepted": true, "action": action, "moved": moved, "ok": true }))
            }
            RendererVerb::ToggleSidebarCollapsed => {
                self.toggle_gpui_sidebar_collapsed(cx);
                Ok(json!({
                    "accepted": true,
                    "action": action,
                    "collapsed": self.sidebar_collapsed,
                    "ok": true,
                }))
            }
            RendererVerb::OpenPaths { mode, targets } => {
                self.gx_store_open_renderer_paths(targets, cx);
                let mode = match mode {
                    OpenPathsMode::Open => "open",
                    OpenPathsMode::Edit => "edit",
                };
                Ok(json!({ "accepted": true, "action": action, "mode": mode, "ok": true }))
            }
        }
    }

    /// `ghostex open` / `edit` / `ghostex <path>`: each path's project is added and opened in the
    /// Code view, the way the operating system's open-file request does it
    /// (`open_gpui_os_integration_paths`), and the first FILE among them is opened in the editor
    /// at its line and column once Code is ready for that project, the way a chat file link is.
    fn gx_store_open_renderer_paths(
        &mut self,
        targets: Vec<OpenPathTarget>,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let paths: Vec<PathBuf> = targets
                .iter()
                .map(|target| PathBuf::from(&target.path))
                .collect();
            let file = background
                .spawn(async move {
                    targets.into_iter().find_map(|target| {
                        let file_path = PathBuf::from(&target.path);
                        if !file_path.is_file() {
                            return None;
                        }
                        let project_path = gpui_os_integration_project_root_for_path(&file_path)?;
                        Some(PendingSourceFileOpen {
                            column: target.column,
                            file_path,
                            line: target.line,
                            origin: PendingSourceFileOpenOrigin::SessionChat,
                            project_path,
                            remote_target: None,
                            remote_working_directory: None,
                        })
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some(file) = file {
                    this.pending_source_file_open = Some(file);
                }
                this.open_gpui_os_integration_paths(paths, window, cx);
            });
        })
        .detach();
    }

    /// `runGxserverRendererCommandButton`: the active project's Action with this id, launched the
    /// way the sidebar's `runSidebarCommand` launched it. The payload is a selector only; command
    /// text, URL, links and the completion sound come from the project's own Action.
    ///
    /// CDXC:CefRuntime 2026-06-27-05:51:
    /// gxserver `runCommand` and `clickButton(kind:"command")` must launch the same trusted
    /// project Action button as native. Treat renderer payloads as selectors only; command text,
    /// URLs, close-on-exit normalization, completion-sound preference, cwd/env, paths, output, and
    /// logs must come from the live HUD command and fixed Rust command-action bridge.
    fn gx_store_run_renderer_action_button(
        &mut self,
        command_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), RendererCommandError> {
        let mut action = self
            .visible_gpui_titlebar_actions()
            .into_iter()
            .find(|action| action.command_id == command_id)
            .filter(|action| action.is_configured())
            .ok_or(RendererCommandError::Unsupported)?;
        // What the sidebar bridge payload carried: no icon, no run mode, close-on-exit forced off,
        // and a completion sound only for a terminal Action.
        action.icon = None;
        action.run_mode = GpuiTitlebarActionRunMode::Default;
        action.close_terminal_on_exit = false;
        if action.action_type == GpuiTitlebarActionType::Browser {
            action.play_completion_sound = false;
            action.links.clear();
        }
        self.run_gpui_titlebar_action(action, window, cx);
        Ok(())
    }
}
