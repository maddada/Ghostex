//! What the page does for a chat view's app-shell requests. The chat itself (its socket, storage and timers) is the desktop's Rust chat host in `app/gx_chat/`, run on the page's thread; a view only hands the app its presentation cache and the host actions a shell performs.
use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// A chat view's `broker` request, which is only ever the presentation cache the next view of the session opens from.
    pub(crate) fn web_relay_chat_broker(&mut self, session: &SessionKey, message: &Value) {
        if message["method"] == "presentation" {
            self.chat_presentations
                .insert(session.clone(), message["params"]["state"].clone());
        }
    }

    /// The desktop's `receive_session_chat_host_action` (`app/session_chat/host_actions.rs`) for a page: every action it answers is answered here, by the same store handler where the page has one and by a toast where it needs the desktop (a pane, a native panel, a Code or Docs view, a CEF dialog).
    pub(crate) fn web_chat_host_action(
        &mut self,
        session: &SessionKey,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        match message["type"].as_str() {
            Some("toast") => {
                self.receive_gpui_app_toast_bridge_message(message, cx);
                return;
            }
            // Settings, a diagram, a table or an agent's page: the page's modal door opens what it
            // can and says which it cannot.
            Some("open") => {
                self.open_app_modal_from_bridge(message.clone(), cx);
                return;
            }
            _ => {}
        }
        if self.web_chat_session_control(session, message, cx) {
            return;
        }
        match message["action"]
            .as_str()
            .or_else(|| message["method"].as_str())
        {
            // The composer's terminal button: the same session, as a terminal.
            Some("terminalView" | "switchToTerminal") => self.web_show_terminal(true, cx),
            // Copy Report's last step: Send Bug / Feedback with its agent switch on.
            Some("sendFeedback") => {
                let collect_with_agent = message["collectWithAgent"] == true
                    || message["params"]["collectWithAgent"] == true;
                self.open_gpui_feedback_modal(collect_with_agent, cx);
            }
            // A row of a coordinator's Threads panel: that thread's chat, on the same machine.
            // gxserver resumed it first when it was closed (`openCoordinatorThread`). An agent
            // message card's session-name pill opens the other session's chat the same way.
            Some("openCoordinatorThread" | "openAgentSession") => {
                let text = |key: &str| {
                    message[key]
                        .as_str()
                        .or_else(|| message["params"][key].as_str())
                        .map(str::to_string)
                };
                if let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId"))
                {
                    // A coordinator's worker on another computer names that computer's gxserver;
                    // this build streams only its own computer (and Cloud Boxes), so a worker
                    // elsewhere is not opened as a session of the same ids here.
                    let on_server = match text("serverId").filter(|id| !id.is_empty()) {
                        Some(server_id) => {
                            let machine = self
                                .gx_store
                                .core
                                .presentation()
                                .machine_for_server_session(&server_id, &project_id, &session_id);
                            if machine.is_none() {
                                self.dispatch_gpui_app_modal_toast(
                                    "warning",
                                    "Could not open that worker",
                                    "It runs on another computer this browser can't reach. Open it in the Ghostex app.",
                                    cx,
                                );
                                return;
                            }
                            machine
                        }
                        None => None,
                    };
                    // A Cloud Box start names the box's machine (`cloudbox:<boxId>`) the session is on.
                    let machine = on_server.unwrap_or_else(|| {
                        text("machineId")
                            .map(ghostex_gx_core::MachineId::Remote)
                            .unwrap_or_else(|| session.machine.clone())
                    });
                    self.web_open_session_in_work_area(
                        SessionKey {
                            machine,
                            project_id,
                            session_id,
                        },
                        false,
                        cx,
                    );
                }
            }
            // A side chat opens in the panel beside the chat (`web_host/side_chat.rs`), the
            // desktop's side panel tab; a deleted one's panel closes.
            Some(action @ ("openSideChat" | "sideChatDeleted")) => {
                let text = |key: &str| {
                    message[key]
                        .as_str()
                        .or_else(|| message["params"][key].as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };
                if let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId"))
                {
                    let side_chat = SessionKey {
                        machine: session.machine.clone(),
                        project_id,
                        session_id,
                    };
                    if action == "openSideChat" {
                        self.web_open_side_chat(side_chat, cx);
                    } else {
                        self.web_close_side_chat(Some(&side_chat), cx);
                    }
                }
            }
            // A command in a reply: a new terminal in the session's folder that runs it (or only
            // types it, for Paste), the desktop's `run_session_chat_command_in_terminal`.
            Some(action @ ("runInTerminal" | "pasteInTerminal")) => {
                let command = message["command"]
                    .as_str()
                    .or_else(|| message["params"]["command"].as_str())
                    .unwrap_or_default()
                    .to_string();
                if !command.trim().is_empty() {
                    let cwd = self
                        .gx_store
                        .core
                        .presentation()
                        .session(session)
                        .and_then(|row| row.cwd.clone());
                    self.gx_store_create_command_terminal(
                        ghostex_gx_core::ProjectKey {
                            machine: session.machine.clone(),
                            project_id: session.project_id.clone(),
                        },
                        cwd,
                        command,
                        action == "runInTerminal",
                        cx,
                    );
                }
            }
            // A fork picker branch: that branch's chat, woken first when it is stopped, as the
            // desktop's `select_session_chat_fork_branch` does.
            Some("selectForkBranch") => {
                let text = |key: &str| {
                    message[key]
                        .as_str()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };
                let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId"))
                else {
                    return;
                };
                let branch = SessionKey {
                    machine: session.machine.clone(),
                    project_id,
                    session_id,
                };
                if text("lifecycleState").as_deref() != Some("stopped") {
                    self.web_open_session_in_work_area(branch, false, cx);
                    return;
                }
                let params =
                    json!({ "projectId": branch.project_id, "sessionId": branch.session_id });
                cx.spawn(async move |this, cx| {
                    let woken =
                        crate::app::gx_store::gx_rpc(None, "/api/wakeSession", params).await;
                    let _ = this.update(cx, |this, cx| match woken {
                        Ok(_) => this.web_open_session_in_work_area(branch, false, cx),
                        Err(_) => this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Could not open that branch",
                            "The session could not be resumed. Try again from the sidebar.",
                            cx,
                        ),
                    });
                })
                .detach();
            }
            // A chat's zoom chord while Interface scale keeps the areas together: it steps Scale
            // (the desktop's app/interface_scale_zoom.rs). The page cannot save Settings, so it
            // holds until the page reloads.
            Some("stepInterfaceScale") => {
                use ghostex_settings_catalog::interface_scale::{
                    INTERFACE_SCALE_SETTING, interface_scale_percent,
                };
                if let Some(step) = message["step"]
                    .as_str()
                    .and_then(crate::app::native_chat::zoom::ZoomStep::from_name)
                {
                    let mut settings = crate::shared_settings::shared_sidebar_settings_snapshot()
                        .object()
                        .clone();
                    let next = crate::app::native_chat::zoom::step_zoom_percent(
                        interface_scale_percent(&settings) as f32,
                        step,
                    );
                    settings.insert(INTERFACE_SCALE_SETTING.into(), f64::from(next).into());
                    crate::shared_settings::install(settings);
                    cx.refresh_windows();
                }
            }
            // A chat's zoom chords. The page cannot save Settings, so the zoom changes every chat
            // on this page until it reloads.
            Some("setChatZoomPercent") => {
                if let Some(percent) = message["percent"].as_f64() {
                    let mut settings = crate::shared_settings::shared_sidebar_settings_snapshot()
                        .object()
                        .clone();
                    settings.insert("sessionChatZoomPercent".into(), percent.into());
                    crate::shared_settings::install(settings);
                    cx.refresh_windows();
                }
            }
            // A web link or a page card's Open (the page cannot float an agent's page,
            // `visual_popup.rs`): a new tab of this browser, which runs on the session's own
            // computer (`ghostex web` listens on its loopback only), so a localhost address in it
            // is the session's.
            // On a Ghostex Collab guest's page, a page card's Open reads the page through the
            // share's tunnel instead (collab_guest.rs), since the owner's address is not the guest's.
            Some("openLink") => {
                if let Some(url) = message["url"]
                    .as_str()
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    && !crate::app::collab_guest::open_page_for_guest(url)
                    && let Some(window) = web_sys::window()
                {
                    let _ = window.open_with_url_and_target(url, "_blank");
                }
            }
            // A guest opens the chat's HTML files in its page viewer; any other file stays on the
            // owner's computer.
            Some("openFile") if crate::app::collab_guest::guest().is_some() => {
                let opened = message["path"]
                    .as_str()
                    .is_some_and(crate::app::collab_guest::open_file_for_guest);
                if !opened && let Some(guest) = crate::app::collab_guest::guest() {
                    self.web_chat_needs_app_toast(
                        &format!("This file is on {}\u{2019}s computer", guest.owner),
                        "A shared chat opens its pictures and HTML pages here; other files stay with the chat\u{2019}s owner.",
                        cx,
                    );
                }
            }
            // The model picker's "pick it in the CLI": the terminal, with the desktop's hint.
            Some("agentPickerTerminalView") => {
                self.web_show_terminal(true, cx);
                self.dispatch_gpui_app_modal_toast(
                    "info",
                    "Please pick the model and effort in the CLI then switch back to the chat view",
                    "",
                    cx,
                );
            }
            Some("stashedPrompts") => self.open_app_modal_from_bridge(
                json!({
                    "type": "open",
                    "modal": "stashedPrompts",
                    "projectId": session.project_id,
                    "sessionId": session.to_sidebar_session_id(),
                }),
                cx,
            ),
            // The chat's own notices (a failed copy or read), as `{message, level}`.
            Some("toast") => {
                if let Some(text) = message["message"].as_str() {
                    let level = message["level"].as_str().unwrap_or("info");
                    self.dispatch_gpui_app_modal_toast(level, text, "", cx);
                }
            }
            Some("pickAttachments" | "attachPath") => self.web_chat_needs_app_toast(
                "Attaching files from disk is not available in the browser",
                "Paste an image into the composer, or attach it in the Ghostex app.",
                cx,
            ),
            Some("promptEditor") => self.web_chat_needs_app_toast(
                "The prompt editor is not available in the browser",
                "Open it from the Ghostex app.",
                cx,
            ),
            Some("saveImageStart") => self.web_chat_needs_app_toast(
                "Saving images is not available in the browser",
                "Save it from the Ghostex app.",
                cx,
            ),
            // The chat's Tasks pill: this chat's panel, which lists the tasks.
            Some("showSessionDetails") => {
                self.session_details_choices
                    .set(&session.to_sidebar_session_id(), true);
                cx.notify();
            }
            // The Session details panel's card was dismissed (shell.rs): it goes off for this chat
            // once the press that did it has landed, if the chat is still the one on screen
            // (`SessionDetailsChoices::settle`).
            Some("sessionDetailsDismissed") => {
                let key = session.to_sidebar_session_id();
                self.session_details_dismissed_at = Some(web_time::Instant::now());
                self.session_details_choices
                    .dismissed(&key, web_time::Instant::now());
                cx.notify();
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(crate::app::native_chat::session_details::DISMISS_SETTLE)
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        let shown = this
                            .open_session
                            .as_ref()
                            .filter(|session| !this.web_shows_terminal(session))
                            .map(|session| session.to_sidebar_session_id());
                        this.session_details_choices
                            .settle(shown.as_deref(), web_time::Instant::now());
                        cx.notify();
                    });
                })
                .detach();
            }
            Some("openAgentsHubSkill") => self.web_chat_needs_app_toast(
                "Agents Hub is in the Ghostex app",
                "Open the skill from the Ghostex app's Agents Hub.",
                cx,
            ),
            Some("openSettingsPage") => self.web_chat_needs_app_toast(
                "Settings are in the Ghostex app",
                "Open Settings > Connectors in the Ghostex app.",
                cx,
            ),
            Some("gitAction") => self.web_chat_needs_app_toast(
                "Use the Commit button above",
                "The header's Commit menu commits and opens pull requests here.",
                cx,
            ),
            // Review all and Session details' Changes card open the Changes panel beside the chat
            // on their first file; the page has no VS Code view for `fileChangesOpenTarget`.
            Some("reviewChanges") => self.open_changes_view(
                Some(crate::app::native_changes::ChangesPreset {
                    scope: crate::app::native_changes::ChangesScope::AgainstMain,
                    action: None,
                    reveal: message["paths"]
                        .as_array()
                        .and_then(|paths| paths.first())
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                }),
                cx,
            ),
            Some("openInBrowser") => self.web_chat_needs_app_toast(
                "Pages open in the Ghostex app",
                "The file is on the computer running Ghostex, and its Browser opens it there.",
                cx,
            ),
            Some("openFile" | "locateFile" | "annotateReply") => self.web_chat_needs_app_toast(
                "Files open in the Ghostex app",
                "The browser has no Code or Docs view.",
                cx,
            ),
            // The desktop's own bookkeeping for its panes, support logs and draft handoffs, which
            // the page does not have: nothing to do.
            Some(
                "composerReady"
                | "composerDraftState"
                | "composerRequest"
                | "recordSendFailure"
                | "diagnosticLog"
                | "draftHandoffToChatComplete"
                | "draftHandoffToChatRetry"
                | "draftHandoffToTerminalComplete"
                | "draftHandoffToTerminalFailed",
            ) => {}
            other => log::info!("chat host action not handled on web yet: {other:?}"),
        }
    }

    fn web_chat_needs_app_toast(
        &mut self,
        title: &str,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_app_modal_toast("info", title, description, cx);
    }

    /// A session's own controls in the chat's More actions menu (Rename, Sleep, Delayed actions,
    /// Close After Done, Fork, Full Reload, Handoff / Export, Switch Account) and the model
    /// picker's handoff: the commands the sidebar row's menu sends for the same session, through
    /// the same store handlers, as the desktop's terminal bar does
    /// (`gx_store/terminal_lifecycle/runtime_actions.rs`).
    fn web_chat_session_control(
        &mut self,
        session: &SessionKey,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let row = session.to_sidebar_session_id();
        let command = |message: Value| json!({ "type": "command", "message": message });
        let sidebar_command = match message["action"].as_str() {
            Some("rename") => {
                self.web_open_chat_rename(session, &row, cx);
                return true;
            }
            Some("delayedActions") => {
                self.web_open_chat_delayed_send(session, &row, cx);
                return true;
            }
            // The working card's Links pill: the page's links card, as the sidebar row opens it.
            Some("sessionLinks") => {
                self.open_session_links_card(row.clone(), cx);
                return true;
            }
            Some("sleep") => {
                command(json!({ "type": "setSessionSleeping", "sessionId": row, "sleeping": true }))
            }
            Some("closeAfterDone") => {
                command(json!({ "type": "toggleCloseAfterDone", "sessionId": row }))
            }
            Some("fullReload") => command(json!({ "type": "fullReloadSession", "sessionId": row })),
            Some("fork") => {
                self.gx_store_run_workspace_session_fork(
                    &session.project_id,
                    &session.session_id,
                    cx,
                );
                return true;
            }
            Some("exportTranscript") => {
                self.git_export_session_transcript(&row, None, cx);
                return true;
            }
            Some("handoffToModel") => {
                let text = |key: &str| message[key].as_str().map(str::trim).unwrap_or_default();
                let (provider, model) = (text("provider"), text("model"));
                if matches!(
                    provider,
                    "claude" | "codex" | "cursor" | "grok" | "antigravity" | "opencode"
                ) && !model.is_empty()
                {
                    let effort = text("effort");
                    self.git_export_session_transcript(
                        &row,
                        Some(crate::app::gx_store::git::HandoffTarget {
                            provider: provider.to_string(),
                            model: model.to_string(),
                            effort: (!effort.is_empty()).then(|| effort.to_string()),
                        }),
                        cx,
                    );
                }
                return true;
            }
            Some("switchAccount") => {
                if let Some(agent_id) = message["agentId"]
                    .as_str()
                    .map(str::trim)
                    .filter(|agent_id| !agent_id.is_empty())
                {
                    self.web_switch_session_agent(session, agent_id.to_string(), cx);
                }
                return true;
            }
            _ => return false,
        };
        self.web_run_sidebar_command(sidebar_command, cx);
        true
    }

    /// The rename dialog for the chat's own session, seeded from its presentation rather than its
    /// sidebar row (the desktop's `open_gpui_rename_session_modal_for_agents_tab`), so it opens
    /// while the row is folded away or in another Space. A rename the agent never confirmed is
    /// offered again, as the sidebar's Rename does.
    fn web_open_chat_rename(
        &mut self,
        session: &SessionKey,
        row: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(server) = self
            .gx_store
            .core
            .presentation()
            .loaded(&session.machine)
            .and_then(|loaded| loaded.server_session(&session.project_id, &session.session_id))
        else {
            return;
        };
        let title = server
            .rename_failed_title
            .clone()
            .filter(|attempt| !attempt.trim().is_empty())
            .unwrap_or_else(|| {
                ghostex_gx_core::rename_seed_title(
                    server.primary_title.as_deref(),
                    server.terminal_title.as_deref(),
                    &server.title,
                )
            });
        self.close_app_modal_from_bridge(cx);
        self.open_app_modal_from_bridge(
            json!({
                "type": "open",
                "modal": "renameSession",
                "sessionId": row,
                "initialTitle": title,
            }),
            cx,
        );
    }

    /// Delayed actions for the chat's own session, seeded as the desktop seeds it from a session's
    /// own bar (`open_gpui_delayed_send_modal_for_agents_session`): its title and agent, the
    /// daemon's Close After Done and armed send (gx-core `delayed_send_seed`, which reads the row
    /// when it is drawn and the daemon's state when it is not).
    fn web_open_chat_delayed_send(
        &mut self,
        session: &SessionKey,
        row: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(server) = self
            .gx_store
            .core
            .presentation()
            .loaded(&session.machine)
            .and_then(|loaded| loaded.server_session(&session.project_id, &session.session_id))
        else {
            return;
        };
        let mut open = json!({
            "type": "open",
            "modal": "delayedSend",
            "sessionId": row,
            "title": ghostex_gx_core::rename_seed_title(
                server.primary_title.as_deref(),
                server.terminal_title.as_deref(),
                &server.title,
            ),
            "closeAfterDoneActive": server.close_after_done,
            "supportsSendWhenAgentStops": true,
            "supportsSendWhenAllProjectSessionsStop": true,
        });
        if let Some(agent_icon) = &server.agent_icon {
            open["agentIcon"] = json!(agent_icon);
        }
        let seed = ghostex_gx_core::delayed_send_seed(
            self.gx_store.sidebar_list.view(),
            &self.gx_store.core,
            session,
        );
        if let Some(open) = open.as_object_mut() {
            open.extend(seed);
        }
        self.open_app_modal_from_bridge(open, cx);
    }

    /// The desktop's `gx_store_switch_workspace_session_agent`: the daemon moves the conversation
    /// to the picked agent, and a Full Reload resumes it under that agent.
    fn web_switch_session_agent(
        &mut self,
        session: &SessionKey,
        agent_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let project_id = session.project_id.clone();
        let raw_session_id = session.session_id.clone();
        let row = session.to_sidebar_session_id();
        cx.spawn(async move |this, cx| {
            let result =
                crate::app::gx_store::terminal_lifecycle::session_calls::switch_session_agent(
                    project_id,
                    raw_session_id,
                    agent_id,
                )
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => this.web_run_sidebar_command(
                    json!({
                        "type": "command",
                        "message": { "type": "fullReloadSession", "sessionId": row },
                    }),
                    cx,
                ),
                Err(error) => this.dispatch_gpui_app_modal_toast(
                    "error",
                    "Could not switch account",
                    &error.message,
                    cx,
                ),
            });
        })
        .detach();
    }
}
