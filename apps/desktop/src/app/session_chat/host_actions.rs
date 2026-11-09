//! Account switch progress, the app modal host bridge handler, and `receive_session_chat_host_action`, the switch over every chat host action.

use crate::app::helpers::web_bridge_types::{
    AppModalHostBridgeEvent, AppModalHostBridgeEventHandler,
};
use std::rc::Rc;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

use super::drafts_and_attachments::GpuiSessionChatDraftHandoff;

impl GhostexGpuiApp {
    pub(crate) fn session_account_switch_progress(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&SessionAccountSwitchProgress> {
        self.account_switch_progress
            .get(&self.workspace_terminal_key_for_shell_session(session_id)?)
    }

    /// CDXC:Drafts 2026-09-09 WHY:
    /// Rendering a draft's chat child is not enough to keep it visible: workspace reconciliation also controls the native page's visibility. Both paths use this same placeholder gate.
    pub(crate) fn session_account_switch_placeholder_progress(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&SessionAccountSwitchProgress> {
        let progress = self.session_account_switch_progress(session_id)?;
        let key = self.workspace_terminal_key_for_shell_session(session_id)?;
        let is_draft = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .as_ref()
            .is_some_and(|sessions| {
                sessions
                    .iter()
                    .any(|session| session.key == key && session.is_draft)
            });
        (!is_draft).then_some(progress)
    }

    /// CDXC:AgentProviders 2026-09-09 DECISION:
    /// During account switching, prompted sessions show "Switching Claude account to" (or Codex) with the selected email below it while retaining their page. Drafts keep their existing chat visible throughout the background switch, superseding the earlier placeholder rule for drafts only.
    pub(crate) fn set_session_account_switch_progress(
        &mut self,
        key: GpuiWorkspaceTerminalSessionKey,
        progress: &serde_json::Value,
        page_generation: Option<u64>,
        cx: &mut gpui::Context<Self>,
    ) {
        if progress.is_null() {
            if self
                .account_switch_progress
                .get(&key)
                .is_some_and(|progress| progress.page_generation == page_generation)
            {
                self.account_switch_progress.remove(&key);
            }
        } else {
            let provider = match progress["provider"].as_str() {
                Some("claude") => "Claude",
                Some("codex") => "Codex",
                _ => return,
            };
            let Some(email) = progress["email"].as_str() else {
                return;
            };
            self.account_switch_progress.insert(
                key,
                SessionAccountSwitchProgress {
                    title: format!("Switching {provider} account to"),
                    email: email.to_string(),
                    provider: if provider == "Claude" {
                        "claude"
                    } else {
                        "codex"
                    },
                    indicator: progress["indicator"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    page_generation,
                },
            );
        }
        self.reconcile_agents_chat_surfaces(cx);
        cx.notify();
    }

    pub(crate) fn app_modal_host_bridge_event_handler(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AppModalHostBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();

        Rc::new(move |event: AppModalHostBridgeEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    // `update_in` needs an active window, and a message that arrives without one is
                    // dropped. That was invisible until the sidebar page started handing the
                    // workspace session groups document over this bridge, where a dropped message
                    // is a rename that never reaches disk, so the drop is counted
                    // (gx_store/workspace_groups.rs reports it as `hostMessagesDropped`).
                    if app
                        .update_in(&mut async_cx, |this, window, cx| {
                            this.receive_app_modal_host_bridge_event(event, window, cx);
                        })
                        .is_err()
                    {
                        crate::app::gx_store::note_native_host_message_dropped();
                    }
                })
                .detach();
        })
    }

    pub(crate) fn receive_session_chat_host_action(
        &mut self,
        session_id: TerminalSessionId,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        use terminal_element::TerminalAgentActionRequest;

        let Ok(message) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        /*
        Composer option-pill failures post the shared app-toast request over
        this same shim. The chat surface's handler otherwise ignores anything
        that is not a sessionChatHostAction, so without this arm the error
        would vanish instead of appearing in the workspace toast window.
        */
        if message.get("type").and_then(serde_json::Value::as_str) == Some("toast") {
            self.receive_gpui_app_toast_bridge_message(&message, cx);
            return;
        }
        // CDXC:Settings 2026-09-06 WHY: The chat bridge previously dropped Settings opens as unknown chat actions, so the account settings shortcut did nothing. Route this modal through the existing native modal owner.
        // CDXC:SessionChat 2026-09-06 WHY:
        // Transcript diagrams use the shared modal launcher; their expand requests must reach the native diagram window instead of being discarded as unknown chat actions.
        if message.get("type").and_then(serde_json::Value::as_str) == Some("open")
            && message.get("modal").and_then(serde_json::Value::as_str) == Some("visualPage")
        {
            self.open_session_chat_visual_page(session_id, &message, window, cx);
            return;
        }
        if message.get("type").and_then(serde_json::Value::as_str) == Some("open")
            && matches!(
                message.get("modal").and_then(serde_json::Value::as_str),
                Some("settings" | "mermaidDiagram" | "markdownTable")
            )
        {
            self.receive_app_modal_host_bridge_event(
                AppModalHostBridgeEvent::Message(payload.to_string()),
                window,
                cx,
            );
            return;
        }
        if message.get("type").and_then(serde_json::Value::as_str) != Some("sessionChatHostAction")
        {
            return;
        }
        let Some(action) = message.get("action").and_then(serde_json::Value::as_str) else {
            return;
        };
        if self.handle_session_details_host_action(session_id, action, &message, window, cx) {
            return;
        }
        // The coordinator board's view switch (native_chat/coordinator_board.rs): the Board pill
        // opens that view until it is switched again, on every chat and every computer.
        if action == "setCoordinatorBoardView" {
            if let Some(view) = message
                .get("view")
                .and_then(serde_json::Value::as_str)
                .filter(|view| matches!(*view, "list" | "columns"))
            {
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &serde_json::json!({
                        "patch": { "coordinatorBoardView": view },
                        "source": "chat:coordinatorBoard",
                        "type": "updateSettingsPatch",
                    }),
                    cx,
                );
            }
            return;
        }
        // The offline-computer card (remote_conn/chat_computers.rs): a message to the chat's
        // computer got no answer, or the card's Retry.
        if matches!(action, "checkComputer" | "reconnectComputer") {
            if let Some(machine_id) = message
                .get("machineId")
                .and_then(serde_json::Value::as_str)
                .filter(|machine_id| *machine_id != crate::app::gx_chat::LOCAL_MACHINE_ID)
                .map(str::to_string)
            {
                if action == "checkComputer" {
                    self.check_session_chat_computer(machine_id, cx);
                } else {
                    self.retry_session_chat_computer(&machine_id, cx);
                }
            }
            return;
        }
        // A page card on screen whose page has no picture yet (visual_page_background_pictures.rs).
        if action == "pictureUnopenedPage" {
            self.picture_unopened_page(session_id, &message, cx);
            return;
        }
        // The working card's Links pill (native_chat/links_pill.rs) opens the session's links list.
        if action == "sessionLinks" {
            if let (Some(key), Some(anchor)) = (
                self.workspace_terminal_session_key(session_id),
                session_links_anchor(&message),
            ) {
                self.open_session_links_list(key, anchor, window, cx);
            }
            return;
        }
        // Copy Report's last step: Send Bug / Feedback with its agent switch on (gx-chat-core
        // `extras/report.rs`).
        if action == "sendFeedback" {
            let collect_with_agent = message
                .get("collectWithAgent")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            self.open_gpui_feedback_modal(collect_with_agent, cx);
            return;
        }
        if action == "recordSendFailure" {
            if let Some(details) = message.get("details") {
                support_logs::append_session_chat_send_failure(details.clone());
            }
            return;
        }
        // A chat's zoom chord while Interface scale keeps the areas together: it steps Scale
        // (app/interface_scale_zoom.rs).
        if action == "stepInterfaceScale" {
            if let Some(step) = message
                .get("step")
                .and_then(serde_json::Value::as_str)
                .and_then(crate::app::native_chat::zoom::ZoomStep::from_name)
            {
                self.step_interface_scale(step, cx);
            }
            return;
        }
        // A chat's zoom chords, already debounced by the chat (native_chat/zoom.rs).
        if action == "setChatZoomPercent" {
            if let Some(percent) = message.get("percent").and_then(serde_json::Value::as_f64) {
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &serde_json::json!({
                        "patch": { "sessionChatZoomPercent": percent },
                        "source": "chat:zoom",
                    }),
                    cx,
                );
            }
            return;
        }
        /*
        CDXC:Diagnostics 2026-08-24:
        Typing-focus-loss repro breadcrumbs from the chat page (composer
        mount/unmount, focus enter/leave, prompt-kind flips). The page cannot
        write disk logs itself, so they land in the same terminal-focus log as
        the native first-responder transitions they must be correlated with,
        behind the same native.terminal.focus scenario gate.
        */
        if action == "diagnosticLog" {
            let event = message
                .get("event")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("sessionChat.unknown");
            let details = message
                .get("details")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            if event.starts_with("sessionChat.draft.") {
                support_logs::append_for_scenario(
                    support_logs::GpuiSupportLog::SessionChat,
                    "gpui.sessionChat.viewState",
                    event,
                    serde_json::json!({ "sessionId": format!("{session_id:?}"), "details": details }),
                );
                return;
            }
            support_logs::append_for_scenario(
                support_logs::GpuiSupportLog::TerminalFocus,
                "native.terminal.focus",
                event,
                serde_json::json!({
                    "sessionId": format!("{session_id:?}"),
                    "details": details.clone(),
                }),
            );
            /*
            CDXC:Diagnostics 2026-08-28:
            The same page breadcrumbs, duplicated into a dedicated chat log
            behind their own scenario, so the "Loading conversation…" flash can
            be reproduced without turning on the whole focus firehose. Each
            append gates independently; with only one scenario enabled only
            that log is written.
            */
            support_logs::append_for_scenario(
                support_logs::GpuiSupportLog::SessionChat,
                "gpui.sessionChat.viewState",
                event,
                serde_json::json!({
                    "sessionId": format!("{session_id:?}"),
                    "projectId": self.agents_workspace_project_id,
                    "mappedSessionId": self.workspace_terminal_key_for_shell_session(session_id).map(|key| match key {
                        GpuiWorkspaceTerminalSessionKey::Local(key) => key.session_id,
                        GpuiWorkspaceTerminalSessionKey::Remote(key) => key.session_id,
                    }),
                    "pageGeneration": self.agents_chat_page_states.get(&session_id).map(|state| state.generation),
                    "details": details,
                }),
            );
            return;
        }
        if action == "draftHandoffToChatComplete" {
            if self
                .pending_session_chat_received_drafts
                .get(&session_id)
                .and_then(|value| value.get("handoffId"))
                == message.get("handoffId")
            {
                self.pending_session_chat_received_drafts
                    .remove(&session_id);
            }
            return;
        }
        if action == "draftHandoffToChatRetry" {
            return;
        }
        if action == "composerReady" {
            self.reconcile_agents_chat_surfaces(cx);
            self.deliver_pending_session_chat_received_draft(session_id, cx);
            // CDXC:Drafts 2026-10-25 WHY:
            // Recorded whether or not the session is in Chat yet: a prewarmed view reports ready
            // once, before its session switches to Chat, and never again, so requiring Chat mode
            // here left a working chat box marked not ready and its inserts waiting for a click.
            if self.native_chat_views.contains_key(&session_id) {
                self.session_chat_composer_ready_sessions.insert(session_id);
            }
            self.drain_pending_keyboard_handoff(window, cx);
            self.sync_session_chat_pane_focus(window, cx);
            /*
            CDXC:Drafts 2026-08-18:
            A transferred draft also has to reach a chat surface the user is
            not looking at — the automatic switch runs over every newly
            eligible session, not just the focused one, and the focus handoff
            above deliberately does nothing for the rest.
            */
            self.deliver_held_session_chat_composer_text(session_id, cx);
            return;
        }
        /*
        CDXC:SessionChat 2026-08-24:
        Whether this page's composer currently holds anything unsent. Posted on
        composer mount, on every empty↔non-empty transition, and re-asserted on
        composer blur — never per keystroke, and never with the draft itself,
        only the boolean. The RAM eviction pass requires an explicit `true`
        before destroying a page, so a lost report can only make eviction more
        conservative, never destroy text the user typed. A malformed message
        parses as non-empty for the same reason.
        */
        if action == "composerDraftState" {
            let empty = message
                .get("empty")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            self.session_chat_composer_empty_reports
                .insert(session_id, empty);
            return;
        }
        if action == "draftHandoffToTerminalComplete" {
            if !self
                .pending_session_chat_draft_handoffs
                .contains(&session_id)
            {
                return;
            }
            let content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let stashed_prompt_id = message
                .get("stashedPromptId")
                .and_then(serde_json::Value::as_str)
                .filter(|prompt_id| !prompt_id.is_empty())
                .map(str::to_string);
            if !content.is_empty() {
                /*
                CDXC:Drafts 2026-08-24:
                This answer always arrives AFTER the terminal's remount focus
                drain: the page does a gxserver round trip (save to Saved
                Prompts, clear the composer) before posting it, while the
                drain runs on the very next frame of the view switch. So the
                record cannot wait for the drain that already ran — deliver it
                now, and retry briefly while the terminal surface finishes
                coming up. A record the retries never place stays parked, with
                the Saved Prompts row named above holding the same text, until
                the next focus handoff or a return to chat picks it up.
                */
                self.pending_session_terminal_composer_insert.insert(
                    session_id,
                    GpuiSessionChatDraftHandoff {
                        handoff_id: message
                            .get("handoffId")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                format!(
                                    "draft-{}-{}",
                                    std::process::id(),
                                    gpui_remote_install_unique_id()
                                )
                            }),
                        draft_version: message.get("draftVersion").cloned(),
                        content,
                        stashed_prompt_id,
                    },
                );
                if !self.deliver_pending_session_terminal_composer_insert(session_id, cx) {
                    self.schedule_pending_session_terminal_composer_insert_delivery(session_id, cx);
                }
            }
            if message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .is_empty()
            {
                self.pending_session_chat_draft_handoffs.remove(&session_id);
            }
            self.reconcile_agents_chat_surfaces(cx);
            return;
        }
        if action == "draftHandoffToTerminalFailed" {
            if self.pending_session_chat_draft_handoffs.remove(&session_id) {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Draft handoff failed",
                    "The draft stayed in chat. Try switching again.",
                    cx,
                );
                self.reconcile_agents_chat_surfaces(cx);
            }
            return;
        }
        /*
        CDXC:SessionChat 2026-08-02:
        The chat composer's attach button opens the same native open panel the
        terminal's Attach File or Folder action uses (files AND folders — a
        browser file input cannot offer folders or absolute paths). The answer
        rides back into the chat page through the app-owned script boundary as
        the fixed onSessionChatAttachmentsPicked callback with {requestId,
        paths}; cancel answers with empty paths so the page promise settles.
        */
        if action == "pickAttachments" {
            let request_id = message
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let directories_only =
                match message.get("selection").and_then(serde_json::Value::as_str) {
                    Some("files") => Some(false),
                    Some("folders") => Some(true),
                    _ => None,
                };
            self.request_session_chat_attachment_picks(
                session_id,
                request_id,
                directories_only,
                cx,
            );
            return;
        }
        if self.receive_session_chat_image_save_action(session_id, action, &message, cx) {
            return;
        }
        /*
        CDXC:SessionChat 2026-08-03:
        Conversation links open in the app's own surfaces: a web URL goes to
        the integrated Browser while "Open links in embedded browser" is on
        (Shift+click, or that setting off, asks for the system default browser
        instead), and a file path goes to Docs or Code. Both leave the chat
        pane behind by design, so neither needs the focused-session routing
        below.
        */
        if action == "openLink" {
            let Some(url) = message.get("url").and_then(serde_json::Value::as_str) else {
                return;
            };
            let external = message
                .get("external")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let force_embedded = message
                .get("forceEmbedded")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            // CDXC:SessionChat 2026-10-26 WHY: An agent that writes its page's address as a plain link (`…/visual/<id>`) instead of the card sent it to the side panel's Browser, past Settings > Chat > Open Ghostex pages in. Such a link opens where that setting says, as the card does; Shift+click still sends it to the system browser.
            if !external
                && !force_embedded
                && crate::app::window::visual_page_modal::page_id(url).is_some()
            {
                self.open_session_chat_visual_page(
                    session_id,
                    &serde_json::json!({ "type": "open", "modal": "visualPage", "url": url }),
                    window,
                    cx,
                );
                return;
            }
            self.open_session_chat_link(
                url,
                Some(session_id),
                external,
                force_embedded,
                window,
                cx,
            );
            return;
        }
        // A "GBot added" card's Open settings (native_chat/gbot_card.rs): the bot's GBot Settings
        // window, on the chat's own computer.
        if action == "openGbotSettings" {
            if let Some(bot_id) = message
                .get("botId")
                .and_then(serde_json::Value::as_str)
                .filter(|bot_id| !bot_id.is_empty())
            {
                let machine_id = message
                    .get("machineId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
                    .to_string();
                self.open_gpui_gbot_settings_modal(bot_id.to_string(), machine_id, cx);
            }
            return;
        }
        // A "Routine set" card's Edit (native_chat/gbot_card.rs): the Automate view with the
        // routine selected when the GBot's project is the active one, else its GBot Settings on the
        // Routines tab (CDXC:GBots 2026-11-09 in gx-chat-core transcript/routine_cards.rs).
        if action == "editGbotRoutine" {
            let field = |key: &str| {
                message
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let (bot_id, routine_id, project_id) =
                (field("botId"), field("routineId"), field("projectId"));
            let machine_id = Some(field("machineId"))
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| ghostex_gx_core::LOCAL_MACHINE_ID.to_string());
            let local = machine_id == ghostex_gx_core::LOCAL_MACHINE_ID;
            if local
                && !project_id.is_empty()
                && self.open_native_automate_for_routine(&project_id, &routine_id, window, cx)
            {
                return;
            }
            if !bot_id.is_empty() {
                self.open_gpui_gbot_settings_modal_on(
                    bot_id,
                    machine_id,
                    crate::app::window::GbotSettingsTab::Routines,
                    cx,
                );
            }
            return;
        }
        // A command in a reply goes to a terminal of the Commands panel, run or only typed
        // (`run_in_terminal.rs`).
        if action == "runInTerminal" || action == "pasteInTerminal" {
            if let Some(command) = message
                .get("command")
                .and_then(serde_json::Value::as_str)
                .filter(|command| !command.trim().is_empty())
            {
                self.run_session_chat_command_in_terminal(
                    session_id,
                    command,
                    action == "runInTerminal",
                    window,
                    cx,
                );
            }
            return;
        }
        // A file change card's Review all, and Session details' Changes card: where Settings > General
        // > File opening sends file changes (native_changes_host.rs).
        if action == "reviewChanges" {
            let paths: Vec<String> = message
                .get("paths")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect();
            self.open_file_changes(&paths, window, cx);
            return;
        }
        if action == "locateFile" {
            let Some(path) = message.get("path").and_then(serde_json::Value::as_str) else {
                return;
            };
            self.locate_session_chat_file(session_id, path, cx);
            return;
        }
        if action == "openInBrowser" {
            let Some(path) = message.get("path").and_then(serde_json::Value::as_str) else {
                return;
            };
            self.open_session_chat_file_in_browser(session_id, path, window, cx);
            return;
        }
        if action == "annotateReply" {
            let Some(markdown) = message
                .get("markdown")
                .and_then(serde_json::Value::as_str)
                .filter(|markdown| !markdown.trim().is_empty())
            else {
                return;
            };
            self.open_session_chat_reply_in_docs_review(session_id, markdown, window, cx);
            return;
        }
        if action == "openFile" {
            let Some(path) = message.get("path").and_then(serde_json::Value::as_str) else {
                return;
            };
            let line = message
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value > 0);
            let column = line.and_then(|_| {
                message
                    .get("column")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|value| *value > 0)
            });
            let view = match message.get("view").and_then(serde_json::Value::as_str) {
                Some("code") => Some(shared_settings::SharedChatFileOpenView::Code),
                Some("docs") => Some(shared_settings::SharedChatFileOpenView::Docs),
                Some(_) => return,
                None => None,
            };
            self.open_session_chat_file_for_session(
                session_id, path, line, column, view, window, cx,
            );
            return;
        }
        // The chat surface is only interactive as a rendered pane's active
        // session; focus that pane so the focused-session guard and the
        // "for focused session" modal openers resolve to this session.
        if self.focused_agents_or_companion_shell_session_id() != Some(session_id)
            && let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id)
        {
            self.focus_agents_pane(pane_id, cx);
        }
        // Prompt Editor and Attach File or Folder need terminal input even
        // while Chat is visible, so create their viewer on demand.
        if action == "promptEditor" || action == "attachPath" {
            self.ensure_agents_gpui_engine_terminal_view(session_id, cx);
            let Some(runtime_session_id) = self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.runtime_session_id)
            else {
                return;
            };
            let target = GpuiEngineTerminalEventTarget::Agents(session_id);
            if action == "promptEditor" {
                self.handle_gpui_engine_prompt_editor_shortcut(target, runtime_session_id, cx);
            } else if let Some(attachment_target) =
                self.gpui_terminal_attachment_target_for_engine_target(target)
            {
                self.request_gpui_engine_terminal_attachment_paths(
                    attachment_target,
                    runtime_session_id,
                    cx,
                );
            }
            return;
        }
        if action == "terminalView" {
            self.handoff_agents_session_chat_mode(session_id, cx);
            return;
        }
        // The core's own switch (a Codex side chat, a model pick the CLI must finish) carries no
        // chat draft, so it is a plain view switch like the agent picker's, never a handoff.
        if action == "switchToTerminal" {
            if self.agents_chat_mode_sessions.contains(&session_id) {
                self.toggle_agents_session_chat_mode(session_id, cx);
            }
            return;
        }
        if action == "agentPickerTerminalView" {
            // Model selection deliberately uses a plain view switch: `/model`
            // must reach an empty CLI composer, while the chat draft remains
            // persisted for the return trip.
            self.toggle_agents_session_chat_mode(session_id, cx);
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Please pick the model and effort in the CLI then switch back to the chat view",
                "",
                cx,
            );
            return;
        }
        /*
        CDXC:AgentProviders 2026-09-03:
        Switch Account carries the picked agent id, so it is not a plain
        `TerminalAgentActionRequest`; it takes the same sidebar-runtime route
        the terminal bar's submenu takes, and the runtime does the daemon call
        plus the Full reload.
        */
        if action == "switchAccount" {
            let Some(agent_id) = message
                .get("agentId")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|agent_id| !agent_id.is_empty())
            else {
                return;
            };
            let _ = self.dispatch_gpui_workspace_terminal_switch_account(session_id, agent_id, cx);
            return;
        }
        if action == "handoffToModel" {
            let text = |key: &str| {
                message
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
            };
            let (Some(provider), Some(model)) = (
                text("provider").filter(|provider| !provider.is_empty()),
                text("model").filter(|model| !model.is_empty()),
            ) else {
                return;
            };
            let _ = self.dispatch_gpui_workspace_terminal_handoff_to_model(
                session_id,
                provider,
                model,
                text("effort").unwrap_or_default(),
                cx,
            );
            return;
        }
        // CDXC:Coordinators 2026-10-08 WHY: a row of a coordinator's Threads panel opens that thread the way a fork branch opens, by focusing and revealing it in the sidebar; gxserver already resumed it when it was closed (server/src/server/coordinator_open_http.rs), so the action carries no lifecycleState and nothing is woken here. Supersedes the 2026-09-30 note that this host woke it.
        // A session name's pill on an agent message card focuses the other session the same way.
        if action == "selectForkBranch"
            || action == "openCoordinatorThread"
            || action == "openAgentSession"
        {
            self.select_session_chat_fork_branch(session_id, &message, cx);
            return;
        }
        // A coordinator's View playbook: the page its sidebar menu row opens
        // (gx_store/coordinator_actions.rs).
        if action == "viewPlaybook" {
            let session = match self.workspace_terminal_key_for_shell_session(session_id) {
                Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                    ghostex_gx_core::SessionKey::local(key.project_id, key.session_id)
                }
                Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => {
                    ghostex_gx_core::SessionKey::remote(
                        key.remote_machine_id,
                        key.project_id,
                        key.session_id,
                    )
                }
                None => return,
            };
            self.gx_store_run_coordinator_action(
                &serde_json::json!({
                    "type": "coordinatorSessionAction",
                    "sessionId": session.to_sidebar_session_id(),
                    "action": "viewPlaybook",
                }),
                cx,
            );
            return;
        }
        // A side chat opens as a tab of the side panel (side_chat_panel/), and a deleted one's
        // tab closes.
        if action == "openSideChat" {
            self.open_side_chat_from_host_action(session_id, &message, window, cx);
            return;
        }
        if action == "sideChatDeleted" {
            self.close_deleted_side_chat_tab(session_id, &message, window, cx);
            return;
        }
        let request = match action {
            "rename" => TerminalAgentActionRequest::Rename,
            "sleep" => TerminalAgentActionRequest::Sleep,
            "delayedActions" => TerminalAgentActionRequest::DelayedActions,
            "closeAfterDone" => TerminalAgentActionRequest::CloseAfterDone,
            "fork" => TerminalAgentActionRequest::Fork,
            "fullReload" => TerminalAgentActionRequest::FullReload,
            "exportTranscript" => TerminalAgentActionRequest::ExportTranscript,
            "stashPrompt" => TerminalAgentActionRequest::StashPrompt,
            "stashedPrompts" => TerminalAgentActionRequest::StashedPrompts,
            "sessionNote" => TerminalAgentActionRequest::SessionNote,
            _ => return,
        };
        self.handle_gpui_engine_terminal_agent_action(
            GpuiEngineTerminalEventTarget::Agents(session_id),
            request,
            cx,
        );
    }
}

/// The anchor a `sessionLinks` host action carries, in window coordinates.
fn session_links_anchor(message: &serde_json::Value) -> Option<gpui::Bounds<gpui::Pixels>> {
    let anchor = &message["anchor"];
    let number = |key: &str| anchor[key].as_f64().map(|value| gpui::px(value as f32));
    Some(gpui::Bounds::new(
        gpui::point(number("x")?, number("y")?),
        gpui::size(number("width")?, number("height")?),
    ))
}
