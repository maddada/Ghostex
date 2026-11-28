//! Performing a validated renderer command through the path the same action takes in the app.
//!
//! Every verb goes where the app's own gesture for it goes, so a later port of that gesture moves
//! the CLI with it: `ghostex focus` is the status indicator's focus (`focusSession` in the old
//! runtime, exactly what the runtime's renderer handler called), Full Reload, Restart and Close
//! After Done are the sidebar row's commands, `switch-project` and `focus-group` are the menu bar's
//! project activation, `move-project` is the sidebar's project drag, `run-command` is the titlebar
//! Action runner, and `open` / `edit` are routed by `open_paths.rs`.
//!
//! SEE-ALSO: packages/gx-core/src/renderer_commands/ (validation and target resolution),
//! apps/desktop/src/app/status_activation.rs (`dispatch_gpui_status_activation`,
//! `dispatch_gpui_menu_bar_project_activation`; F3 moves both off the runtime).

use ghostex_gx_core::protocol::RendererCommand;
use ghostex_gx_core::{
    OpenPathsMode, RendererCommandError, RendererSession, RendererVerb, SessionGroupOp,
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
    GpuiTitlebarActionRunMode, GpuiTitlebarActionType, gpui_sidebar_open_browser_url_from_json,
};

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
        self.gx_store_perform_renderer_command_inner(command, false, window, cx)
    }

    pub(crate) fn gx_store_perform_renderer_command_allowing_rename_hop(
        &mut self,
        command: &RendererCommand,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<Value, RendererCommandError> {
        self.gx_store_perform_renderer_command_inner(command, true, window, cx)
    }

    fn gx_store_perform_renderer_command_inner(
        &mut self,
        command: &RendererCommand,
        allow_rename_focus_hop: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<Value, RendererCommandError> {
        let payload = super::project_path::with_registered_project_folder(
            &self.gx_store.core,
            &command.payload,
        );
        let verb = plan_renderer_command(&self.gx_store.core, &command.action, &payload)?;
        let action = command.action.as_str();
        match verb {
            RendererVerb::FocusSession(session) => {
                // The voice agent's follow (server/src/voice_agent/show.rs) leaves the user where
                // they are while they type; a focus they asked for always switches.
                if command.payload.get("unlessTyping") == Some(&Value::Bool(true))
                    && crate::app::voice_agent::typing::user_is_typing()
                {
                    return Ok(json!({
                        "ok": true,
                        "reason": "userTyping",
                        "session": session_json(&session),
                        "shown": false,
                    }));
                }
                // The status item's focus switches the sidebar to the session's computer and
                // reveals its row the way a notification click does, so a session on another
                // computer, in another Space, or hidden by a filter or a closed group is shown.
                if !self.dispatch_gpui_status_activation(&session.sidebar_session_id, cx) {
                    return Err(RendererCommandError::BridgeUnavailable);
                }
                Ok(json!({ "ok": true, "session": session_json(&session), "shown": true }))
            }
            RendererVerb::RenameCommand {
                session,
                title,
                command: rename_command_text,
            } => {
                // CDXC:SwitchRequests 2026-11-23 (Q4705):
                // Do not steal focus to rename an unfocused session unless the user explicitly
                // accepted the switch-request toast.
                #[cfg(not(ghostex_web))]
                if !allow_rename_focus_hop
                    && self.gx_store.core.focus().focused_session.as_ref() != Some(&session.key)
                {
                    let target_title = self
                        .gx_store
                        .core
                        .presentation()
                        .session(&session.key)
                        .map(|row| row.title.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| session.key.session_id.clone());
                    let request = ghostex_gx_core::protocol::SwitchRequest {
                        id: command.command_id.clone(),
                        action: "renameCommand".to_string(),
                        payload: command.payload.clone(),
                        title: format!("Rename {target_title}"),
                        subtitle: (!title.trim().is_empty()).then(|| format!("New title: {title}")),
                        primary_label: "Switch".to_string(),
                        icon: None,
                        requester_title: None,
                        project_id: Some(session.key.project_id.clone()),
                        session_id: Some(session.key.session_id.clone()),
                        external_url: None,
                        external_browser: None,
                        created_at: command.created_at.clone(),
                    };
                    self.show_gpui_switch_request_toast(session.key.machine.clone(), request, cx);
                    return Ok(json!({
                        "accepted": true,
                        "action": "renameCommand",
                        "ok": true,
                        "shown": false,
                        "toastShown": true,
                        "reason": "switchRequestToast",
                        "session": session_json(&session),
                    }));
                }
                let _ = allow_rename_focus_hop;
                // Surfacing the session first is what the old runtime did before it posted the
                // rename; the Rust delivery then focuses and waits for the terminal to mount.
                if !self.dispatch_gpui_status_activation(&session.sidebar_session_id, cx) {
                    return Err(RendererCommandError::BridgeUnavailable);
                }
                let payload = json!({
                    "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION,
                    "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE,
                    "projectId": session.key.project_id,
                    "sessionId": session.key.session_id,
                    "title": title,
                    "command": rename_command_text,
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
            RendererVerb::ReadFocusedSession => {
                // This computer's sessions in `focused`/`onScreen`; a connected machine's go in
                // `focusedElsewhere`/`onScreenElsewhere` with its gxserver's server id, which the
                // voice agent names it by (server/src/voice_agent/screen.rs). A machine whose id
                // is not known yet is left out.
                let focus = self.gx_store.core.focus();
                let local = |key: &ghostex_gx_core::SessionKey| {
                    key.machine.is_local().then(
                        || json!({ "projectId": key.project_id, "sessionId": key.session_id }),
                    )
                };
                let elsewhere = |key: &ghostex_gx_core::SessionKey| {
                    let server_id =
                        crate::app::remote_conn::connected_computers::gpui_remote_machine_server_id(
                            key.machine.remote_id()?,
                        )?;
                    Some(json!({
                        "serverId": server_id,
                        "projectId": key.project_id,
                        "sessionId": key.session_id,
                    }))
                };
                Ok(json!({
                    "focused": focus.focused_session.as_ref().and_then(local),
                    "focusedElsewhere": focus.focused_session.as_ref().and_then(elsewhere),
                    "ok": true,
                    "onScreen": focus.displayed_sessions.iter().filter_map(local).collect::<Vec<_>>(),
                    "onScreenElsewhere": focus.displayed_sessions.iter().filter_map(elsewhere).collect::<Vec<_>>(),
                }))
            }
            RendererVerb::UpdateSettingsPatch { patch, keys } => {
                // CDXC:Settings 2026-09-09 DECISION:
                // User: `ghostex settings set` writes through the running desktop app, never the
                // settings file, so a CLI change takes the exact save and fan-out path a Settings
                // modal save takes (Rust merges the patch onto the stored snapshot and hydrates
                // every surface). The renderer command carries only a flat key/value patch; the
                // CLI validates keys and values against the generated settings catalog before it
                // dispatches.
                // SEE-ALSO: server/src/ghostex_cli/settings.rs, server/src/ghostex_cli/help_guide.md.
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
            // Connect both ways: gxserver paired this computer with the one that asked
            // (server/src/remote_access/connect_back.rs); a page saves no computers.
            #[cfg(not(ghostex_web))]
            RendererVerb::AddRemoteMachine {
                name,
                easy_connect_address,
                ssh_user,
                ssh_port,
            } => {
                let id = self.save_easy_connect_machine(
                    &crate::app::remote_conn::easy_connect_machines::EasyConnectMachine {
                        name,
                        easy_connect_address,
                        ssh_user,
                        ssh_port,
                    },
                    cx,
                );
                Ok(json!({ "ok": true, "remoteMachineId": id }))
            }
            #[cfg(ghostex_web)]
            RendererVerb::AddRemoteMachine { .. } => Err(RendererCommandError::Unsupported),
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
                agent_session,
                background,
                default_browser,
            } => {
                // gxserver marks an agent's open this way only when the user set "Agents open
                // pages in" to My default browser (the toast's External browser switch) and let
                // agents open pages without asking (#762, Q5027).
                if default_browser
                    && !background
                    && let Some(page) = crate::app::helpers::default_browser_page_url(&url)
                {
                    crate::app::helpers::gpui_open_external_url(&page, cx);
                    return Ok(json!({
                        "accepted": true,
                        "action": action,
                        "defaultBrowser": true,
                        "ok": true,
                    }));
                }
                let mut payload = json!({
                    "reuse": reuse,
                    "type": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
                    "url": url,
                    "version": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
                });
                if let Some(project_id) = &project_id {
                    payload["projectId"] = json!(project_id);
                }
                if background {
                    // Shows nothing, so an agent's open gets here with no toast (switch_requests.rs
                    // in gxserver pins it to the agent's own project).
                    let Some(project_id) =
                        project_id.or_else(|| self.agents_workspace_project_id.clone())
                    else {
                        return Err(RendererCommandError::NoMatchingProject);
                    };
                    let message = gpui_sidebar_open_browser_url_from_json(&payload.to_string())
                        .map_err(|_| RendererCommandError::Failed)?;
                    // Only this agent's own earlier page may be reused (#762 review B6).
                    let agent_tab = agent_session
                        .as_deref()
                        .and_then(|session| self.browser_agent_pages.get(session))
                        .filter(|(owner, _)| *owner == project_id)
                        .map(|(_, tab_id)| *tab_id);
                    return Ok(
                        match self.open_browser_url_in_background(
                            &project_id,
                            &message.url,
                            message.reuse,
                            agent_tab,
                            window,
                            cx,
                        ) {
                            Ok(tab_id) => {
                                if let Some(agent_session) = agent_session {
                                    self.browser_agent_pages
                                        .insert(agent_session, (project_id.clone(), tab_id));
                                }
                                json!({
                                    "accepted": true,
                                    "action": action,
                                    "background": true,
                                    "ok": true,
                                    "projectId": project_id,
                                    "shown": false,
                                })
                            }
                            Err(reason) => json!({
                                "accepted": false,
                                "action": action,
                                "background": true,
                                "error": reason,
                                "ok": false,
                            }),
                        },
                    );
                }
                if let Ok(message) = gpui_sidebar_open_browser_url_from_json(&payload.to_string()) {
                    /*
                    CDXC:CefRuntime 2026-11-28 WHY:
                    12.5.0 smoke: on a fresh home `ghostex browser open` put the page in the user's own default browser (Windows) or showed nothing in Ghostex (macOS), and answered ok, because it took the link fallback to the system browser (`open_browser_url_door`). The command asks for Ghostex's Browser, so without the runtime it says so, as `--background` already does, and the page's tab waits in the Browser behind its Install prompt (an accepted agent toast lands there too). The web page has no runtime to install and keeps its own browser's tab.
                    */
                    if cfg!(not(ghostex_web))
                        && !crate::app::helpers::web_runtime::web_runtime_available()
                        && self.renderer_open_remote_machine_id(&message).is_none()
                    {
                        self.open_browser_url_behind_install_prompt(message, window, cx);
                        return Ok(json!({
                            "accepted": false,
                            "action": action,
                            "error": "The Browser's web runtime is not installed. Press Install in Ghostex's Browser, then open the page again.",
                            "ok": false,
                        }));
                    }
                    self.open_browser_url_from_renderer_command(message, window, cx);
                    if let (Some(agent_session), Some(project_id)) = (agent_session, &project_id) {
                        self.note_agent_browser_page(agent_session, project_id);
                    }
                }
                Ok(json!({ "accepted": true, "action": action, "ok": true }))
            }
            RendererVerb::ListBrowserPages {
                project_id,
                agent_session,
            } => Ok(self.list_agent_browser_pages(&project_id, agent_session.as_deref(), cx)),
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
            #[cfg(not(ghostex_web))]
            RendererVerb::ShowDebugModal { name } => {
                match name.as_str() {
                    "rail-intro" => self.open_sidebar_rail_intro_preview(cx),
                    "native-mode-intro" => self.open_native_mode_intro_preview(cx),
                    _ => return Err(RendererCommandError::UnknownDebugModal),
                }
                Ok(json!({ "ok": true, "modal": name }))
            }
            // `ghostex debug show-modal` is for the desktop app on the user's own computer.
            #[cfg(ghostex_web)]
            RendererVerb::ShowDebugModal { .. } => Err(RendererCommandError::Unsupported),
            RendererVerb::OpenExtensionView {
                extension_id,
                project_id,
                mode,
                switch_project,
            } => {
                // The switch toast's Open is the user's yes: it may bring the project on screen.
                let switch_project = switch_project || allow_rename_focus_hop;
                Ok(
                    match self.open_extension_view_in_side_panel(
                        &extension_id,
                        Some(&project_id),
                        mode,
                        switch_project,
                        cx,
                    ) {
                        Ok(answer) => answer,
                        Err(error) => json!({
                            "accepted": false,
                            "action": action,
                            "error": error,
                            "ok": false,
                        }),
                    },
                )
            }
            RendererVerb::ToggleSidebarCollapsed => {
                // With Claude on, showing the sidebar leaves Claude (app/claude_web/takeover.rs).
                self.toggle_sidebar_from_user(cx);
                Ok(json!({
                    "accepted": true,
                    "action": action,
                    "collapsed": self.sidebar_collapsed,
                    "ok": true,
                }))
            }
            RendererVerb::OpenPaths { mode, targets } => {
                self.gx_store_open_cli_paths(mode, targets, cx);
                let mode = match mode {
                    OpenPathsMode::Open => "open",
                    OpenPathsMode::Edit => "edit",
                };
                Ok(json!({ "accepted": true, "action": action, "mode": mode, "ok": true }))
            }
            // Another device updating this computer's Ghostex (os_integration/app_update_remote_control.rs).
            RendererVerb::AppUpdate(op) => Ok(self.answer_app_update_renderer_command(op, cx)),
            // The `ghostex` session group verbs, made on the document this app pushes, like a
            // sidebar gesture (gx-core `workspace_groups/verb_edits.rs`).
            RendererVerb::SessionGroups(request) => {
                if !self.gx_store_restore_workspace_groups(cx) {
                    return Err(RendererCommandError::SessionGroupsLoading);
                }
                let outcome = self
                    .gx_store
                    .workspace_groups
                    .sync
                    .document()
                    .apply_session_group_request(&request)?;
                if let Some(document) = outcome.document {
                    self.gx_store_edit_workspace_groups(document, cx);
                }
                let mut answer = json!({ "ok": true });
                // A delete is the group menu's Close Group (workspace_groups/group_commands.rs):
                // the members close through the store's own close, then the group goes.
                if let Some(sidebar_group_id) = outcome.close_group {
                    if let (Some(project_id), Some(group_id)) =
                        (request.project_id.as_deref(), outcome.group_id.as_deref())
                    {
                        answer["closedSessionIds"] = json!(
                            self.gx_store
                                .workspace_groups
                                .sync
                                .document()
                                .subgroup_members(project_id, group_id)
                        );
                    }
                    let message = json!({ "type": "closeGroup", "groupId": sidebar_group_id });
                    let command = json!({ "type": "command", "message": message });
                    self.gx_store_run_group_command(&command, &message, cx);
                }
                // Only a list prints the groups; an edit answers with the group it touched.
                if request.op == SessionGroupOp::List {
                    answer["groups"] = self
                        .gx_store
                        .workspace_groups
                        .sync
                        .document()
                        .session_groups_json(request.project_id.as_deref());
                }
                if let Some(group_id) = outcome.group_id {
                    answer["groupId"] = json!(group_id);
                }
                if let Some(project_id) = request.project_id {
                    answer["projectId"] = json!(project_id);
                }
                Ok(answer)
            }
        }
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
