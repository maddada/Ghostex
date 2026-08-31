// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: sidebar/app-modal/session-chat CEF bridge handlers and chat host actions

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use gpui::ClipboardItem;
use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

/*
CDXC:SessionChatDraftHandoff 2026-08-24:
A draft moving from the chat composer to the terminal is never held in memory
alone. The page saves it to Saved Prompts BEFORE it clears the composer, and
this record carries that row's id next to the text, so the row is deleted only
after a terminal confirms the paste. Every other outcome — a failed paste, a
chat surface torn down mid-move, a session that never remounts — drops this
record and leaves the row standing in Saved Prompts, where the user can get the
text back by hand. Losing the text is the one outcome this shape forbids.
*/
#[derive(Clone, Debug)]
pub(crate) struct GpuiSessionChatDraftHandoff {
    /// Exact composer text the terminal must receive.
    pub(crate) content: String,
    /// The Saved Prompts row holding the durable copy, when this handoff
    /// created it. `None` means the save matched a prompt the user had already
    /// saved by hand, which must stay in Saved Prompts.
    pub(crate) stashed_prompt_id: Option<String>,
}

/// Next free path in Downloads for `<session>-1.png` without overwriting.
fn session_chat_image_downloads_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("session");
    let extension = path.extension().and_then(|ext| ext.to_str());
    let (base, mut next) = match stem.rsplit_once('-') {
        Some((base, suffix))
            if !base.is_empty()
                && !suffix.is_empty()
                && suffix.chars().all(|ch| ch.is_ascii_digit()) =>
        {
            (
                base.to_string(),
                suffix.parse::<u32>().unwrap_or(1).saturating_add(1),
            )
        }
        _ => (stem.to_string(), 2),
    };
    loop {
        let numbered = match extension {
            Some(ext) => format!("{base}-{next}.{ext}"),
            None => format!("{base}-{next}"),
        };
        let path = directory.join(&numbered);
        if !path.exists() {
            return path;
        }
        if next == u32::MAX {
            return directory.join(format!("{base}-{next}-{}", next));
        }
        next += 1;
    }
}

impl GhostexGpuiApp {
    pub(crate) fn sidebar_bridge_event_handler(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> cef::SidebarBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();

        Rc::new(move |event: cef::SidebarBridgeEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, window, cx| {
                        this.receive_sidebar_bridge_event(event, window, cx);
                    });
                })
                .detach();
        })
    }

    /*
    CDXC:GPUITutorialVideoFullscreen 2026-08-18:
    The tutorial video should play fullscreen inside its own modal window. The
    page is a third-party document, so the app cannot call `requestFullscreen()`
    for it (Chromium requires a transient user activation that app-owned
    JavaScript never has); the host sends the player's own "f" shortcut as real
    input instead. The press waits a beat after main-frame load-end because the
    YouTube player installs its keyboard shortcuts after the page loads, and it
    is sent once because "f" toggles.
    */
    pub(crate) fn tutorial_video_page_load_end_handler(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> cef::PageLoadEndHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        let foreground = cx.foreground_executor().clone();

        Rc::new(move || {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            let background = background.clone();
            foreground
                .spawn(async move {
                    background
                        .timer(GPUI_TUTORIAL_VIDEO_FULLSCREEN_KEY_DELAY)
                        .await;
                    let _ = app.update(&mut async_cx, |this, cx| {
                        this.send_gpui_tutorial_video_fullscreen_key(cx);
                    });
                })
                .detach();
        })
    }

    pub(crate) fn send_gpui_tutorial_video_fullscreen_key(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(handle) = self.app_modal_window else {
            return;
        };
        let _ = handle.update(cx, |host, _modal_window, cx| {
            host.send_tutorial_video_fullscreen_key(cx);
        });
    }

    pub(crate) fn app_modal_host_bridge_event_handler(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> cef::AppModalHostBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();

        Rc::new(move |event: cef::AppModalHostBridgeEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, window, cx| {
                        this.receive_app_modal_host_bridge_event(event, window, cx);
                    });
                })
                .detach();
        })
    }

    /*
    CDXC:GPUISessionChatHostActions 2026-07-31:
    The chat CEF surface renders its own top-right [Terminal View][Agent
    Actions] cluster because a gpui-drawn overlay cannot paint above the
    native CEF view. Button clicks arrive over the already-installed
    app-modal-host bridge shim as `sessionChatHostAction` messages; this
    handler routes them into the same agent-action dispatch the terminal
    overlay uses.
    */
    pub(crate) fn session_chat_host_bridge_event_handler(
        &self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> cef::AppModalHostBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();

        Rc::new(move |event: cef::AppModalHostBridgeEvent| {
            let cef::AppModalHostBridgeEvent::Message(payload) = event else {
                return;
            };
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, window, cx| {
                        this.receive_session_chat_host_action(session_id, &payload, window, cx);
                    });
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
        if message.get("type").and_then(serde_json::Value::as_str)
            == Some("sessionChatExtensionBridgeRequest")
        {
            self.handle_chat_bar_extension_bridge_request(session_id, &message, cx);
            return;
        }
        if message.get("type").and_then(serde_json::Value::as_str) != Some("sessionChatHostAction")
        {
            return;
        }
        let Some(action) = message.get("action").and_then(serde_json::Value::as_str) else {
            return;
        };
        /*
        CDXC:SessionChatFocusDiagnostics 2026-08-24:
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
            CDXC:SessionChatLoadingDiagnostics 2026-08-28:
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
                    "details": details,
                }),
            );
            return;
        }
        if action == "composerReady" {
            if self.agents_chat_mode_sessions.contains(&session_id)
                && self.agents_chat_surfaces.contains_key(&session_id)
            {
                self.session_chat_composer_ready_sessions.insert(session_id);
                self.flush_pending_chat_bar_extension_toggles(session_id, cx);
            }
            self.complete_session_chat_composer_focus_handoff(session_id, window, cx);
            /*
            CDXC:SessionChatDraftHandoff 2026-08-18:
            A transferred draft also has to reach a chat surface the user is
            not looking at — the automatic switch runs over every newly
            eligible session, not just the focused one, and the focus handoff
            above deliberately does nothing for the rest.
            */
            if let Some(content) = self
                .pending_session_chat_composer_insert
                .remove(&session_id)
            {
                self.insert_prompt_into_session_chat(session_id, &content, cx);
            }
            return;
        }
        /*
        CDXC:GPUISessionChatSurfaceEviction 2026-08-24:
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
            if !self.pending_session_chat_draft_handoffs.remove(&session_id) {
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
                CDXC:SessionChatDraftHandoff 2026-08-24:
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
                        content,
                        stashed_prompt_id,
                    },
                );
                if !self.deliver_pending_session_terminal_composer_insert(session_id, cx) {
                    self.schedule_pending_session_terminal_composer_insert_delivery(session_id, cx);
                }
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
        CDXC:GPUISessionChatContextMenuPaste 2026-08-28:
        Chromium rejects navigator.clipboard.read() in a windowed CEF chat
        page when GPUI owns the surrounding window focus, even though Monaco
        is accepting routed keyboard input. Execute CEF's own Paste command
        for the exact visible, focused chat surface instead. This produces the
        normal paste event, so text and image clipboard payloads keep using
        the same Monaco/composer handlers as Cmd+V.
        */
        if action == "pasteIntoComposer" {
            if !self.agents_chat_mode_sessions.contains(&session_id)
                || self.focused_agents_or_companion_shell_session_id() != Some(session_id)
            {
                return;
            }
            let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
                return;
            };
            let focus_handle = surface.read(cx).focus_handle.clone();
            focus_handle.focus(window, cx);
            surface.update(cx, |surface, _| {
                surface.focus();
                let _ = surface.paste();
            });
            return;
        }
        /*
        CDXC:GPUISessionChatAttachPicker 2026-08-02:
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
            self.request_session_chat_attachment_picks(session_id, request_id, cx);
            return;
        }
        /*
        CDXC:GPUISessionChatImageSave 2026-08-19:
        "Save image" in the chat's image overlay. A CEF page has no download
        handler here, so the bytes ride the bridge and Rust writes them into
        Downloads. The reply carries an error string only when the write
        actually failed.
        */
        if action == "saveImage" {
            let request_id = message
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let suggested_name = message
                .get("suggestedName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("image.png")
                .to_string();
            let base64_data = message
                .get("base64Data")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.request_session_chat_image_save(
                session_id,
                request_id,
                suggested_name,
                base64_data,
                cx,
            );
            return;
        }
        /*
        CDXC:GPUISessionChatLinks 2026-08-03:
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
            self.open_session_chat_link(url, external, window, cx);
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
            self.open_session_chat_file(path, line, column, window, cx);
            return;
        }
        // The chat surface is only interactive as a rendered pane's active
        // session; focus that pane so the focused-session guard and the
        // "for focused session" modal openers resolve to this session.
        if self.focused_agents_or_companion_shell_session_id() != Some(session_id) {
            if self.active_mode.is_project_editor_mode() {
                // The chat surface is showing in the companion side pane;
                // focus its slot so focused-session guards resolve here.
                self.focus_project_editor_companion_terminal_session(
                    self.active_mode,
                    session_id,
                    window,
                    cx,
                );
            } else if let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) {
                self.focus_agents_pane(pane_id, cx);
            }
        }
        // Prompt Editor and Attach File or Folder are separate TerminalView
        // events, not TerminalAgentActionRequest variants; both take explicit
        // targets and write to the warm PTY, so they work while the terminal
        // body is parked under the chat surface.
        if action == "promptEditor" || action == "attachPath" {
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
            _ => return,
        };
        self.handle_gpui_engine_terminal_agent_action(
            GpuiEngineTerminalEventTarget::Agents(session_id),
            request,
            cx,
        );
    }

    pub(crate) fn request_session_chat_attachment_picks(
        &mut self,
        session_id: TerminalSessionId,
        request_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Attach an Image, File, or Folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                _ => Vec::new(),
            };
            let _ = this.update(cx, |this, cx| {
                this.deliver_session_chat_attachment_picks(session_id, &request_id, picked, cx);
            });
        })
        .detach();
    }

    pub(crate) fn complete_session_chat_composer_focus_handoff(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.pending_session_chat_composer_focus != Some(session_id) {
            return;
        }
        self.pending_session_chat_composer_focus = None;
        if !self.agents_chat_mode_sessions.contains(&session_id)
            || self.focused_agents_or_companion_shell_session_id() != Some(session_id)
        {
            return;
        }
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return;
        };
        let focus_handle = surface.read(cx).focus_handle.clone();
        focus_handle.focus(window, cx);
        surface.update(cx, |surface, _| {
            surface.focus();
            surface.execute_app_owned_script(
                "(function(){var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatFocusComposerRequested==='function'){ns.onSessionChatFocusComposerRequested();}})(); undefined;",
            );
        });
        if let Some(content) = self
            .pending_session_chat_composer_insert
            .remove(&session_id)
        {
            let _ = self.insert_prompt_into_session_chat(session_id, &content, cx);
        }
    }

    pub(crate) fn drain_pending_session_chat_composer_focus_handoff(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.pending_session_chat_composer_focus else {
            return;
        };
        if self
            .session_chat_composer_ready_sessions
            .contains(&session_id)
        {
            self.complete_session_chat_composer_focus_handoff(session_id, window, cx);
        }
    }

    pub(crate) fn request_session_chat_image_save(
        &mut self,
        session_id: TerminalSessionId,
        request_id: String,
        suggested_name: String,
        base64_data: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let bytes = match BASE64_STANDARD.decode(base64_data.as_bytes()) {
            Ok(bytes) => bytes,
            Err(error) => {
                support_logs::append(
                    support_logs::GpuiSupportLog::AppModal,
                    "gpui.sessionChat.imageSaveFailed",
                    serde_json::json!({ "error": error.to_string(), "stage": "decode" }),
                );
                self.deliver_session_chat_image_save(
                    session_id,
                    &request_id,
                    Some("The image bytes could not be read."),
                    cx,
                );
                return;
            }
        };
        // A file name, never a path: the page supplies `<session>-1.png`.
        let file_name = Path::new(&suggested_name)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "session-1.png".to_string());
        let downloads = home_dir().join("Downloads");
        cx.spawn(async move |this, cx| {
            let write_result = (|| -> Result<PathBuf, String> {
                fs::create_dir_all(&downloads).map_err(|error| error.to_string())?;
                let destination = session_chat_image_downloads_path(&downloads, &file_name);
                fs::write(&destination, &bytes).map_err(|error| error.to_string())?;
                Ok(destination)
            })();
            let _ = this.update(cx, |this, cx| match write_result {
                Ok(destination) => {
                    let saved_name = destination
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| file_name.clone());
                    this.dispatch_gpui_workspace_action_toast(
                        "success",
                        "Saved to Downloads",
                        &saved_name,
                        cx,
                    );
                    this.deliver_session_chat_image_save(session_id, &request_id, None, cx);
                }
                Err(error) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::AppModal,
                        "gpui.sessionChat.imageSaveFailed",
                        serde_json::json!({
                            "error": error,
                            "stage": "write",
                        }),
                    );
                    this.deliver_session_chat_image_save(
                        session_id,
                        &request_id,
                        Some("The image could not be written."),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn deliver_session_chat_image_save(
        &mut self,
        session_id: TerminalSessionId,
        request_id: &str,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return;
        };
        let payload = serde_json::json!({
            "error": error,
            "requestId": request_id,
        });
        let literal = payload
            .to_string()
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        let script = format!(
            "(function(){{var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatImageSaved==='function'){{ns.onSessionChatImageSaved({literal});}}}})(); undefined;"
        );
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn deliver_session_chat_attachment_picks(
        &mut self,
        session_id: TerminalSessionId,
        request_id: &str,
        paths: Vec<PathBuf>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return;
        };
        let payload = serde_json::json!({
            "requestId": request_id,
            "paths": paths
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
        });
        // JSON is a valid JS literal except for U+2028/U+2029; escape them so
        // a pathological file name cannot break the generated script.
        let literal = payload
            .to_string()
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        let script = format!(
            "(function(){{var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatAttachmentsPicked==='function'){{ns.onSessionChatAttachmentsPicked({literal});}}}})(); undefined;"
        );
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn request_session_chat_stash_prompt(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return;
        };
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(
                "(function(){var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatStashPromptRequested==='function'){ns.onSessionChatStashPromptRequested();}})(); undefined;",
            );
        });
    }

    pub(crate) fn request_session_chat_handoff_to_terminal(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        // This is the Chat View button's directional command, not the shared
        // toggle hotkey. Its CEF bridge message crosses an async queue, so it
        // may arrive after a newer native Chat View click has already switched
        // the session back from terminal. Ignore that stale request instead of
        // toggling from whichever state happens to be current when it lands.
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return;
        }
        /*
        CDXC:SessionChatViewSwitch 2026-08-21:
        A view switch is unconditional UI state, not the success result of a
        draft-copy handshake. Ask a ready chat composer to copy its draft in
        the background, retain that hidden CEF surface until it answers, and
        show the terminal immediately. If the bridge is not ready, the draft
        remains in the chat composer's per-session storage for the return trip.
        */
        if self
            .session_chat_composer_ready_sessions
            .contains(&session_id)
            && self.pending_session_chat_draft_handoffs.insert(session_id)
            && let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned()
        {
            surface.update(cx, |surface, _| {
                surface.execute_app_owned_script(
                    "(function(){var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatHandoffToTerminalRequested==='function'){ns.onSessionChatHandoffToTerminalRequested();}})(); undefined;",
                );
            });
        }
        self.toggle_agents_session_chat_mode(session_id, cx);
    }

    /*
    CDXC:SessionChatDraftHandoff 2026-08-24:
    The single point where a handed-off draft stops being recoverable, reached
    only from a terminal drain that has confirmed the text reached the pty.
    Nothing else may delete the row: an unconfirmed paste keeps the pending
    record so the next focus handoff retries it, and a dropped record leaves
    the row in Saved Prompts on purpose.
    */
    pub(crate) fn release_session_chat_draft_handoff_stash(
        &self,
        handoff: GpuiSessionChatDraftHandoff,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(prompt_id) = handoff.stashed_prompt_id else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = gpui_gxserver_rpc_result(
                    "/api/deleteStashedPrompt",
                    &serde_json::json!({ "promptId": prompt_id }),
                    std::time::Duration::from_secs(5),
                );
            })
            .detach();
    }

    /*
    CDXC:SessionChatDraftHandoff 2026-08-24:
    Delivery of a handed-off draft, decoupled from the focus-handoff drains:
    those run once per remount and always before the chat page's async
    save-then-clear answers, so a record parked after the drain used to wait,
    invisible, for a second view switch. This follows the pane's CURRENT
    owner: a session already back in chat mode gets the draft returned to the
    composer instead of a paste into a terminal the user is no longer looking
    at. Returns true when nothing is pending any more (delivered, or moved
    back to chat); false means the record stayed parked and a retry may help.
    The remove-after-success shape is load-bearing — the record points at the
    draft's durable Saved Prompts row, and only a confirmed terminal paste may
    delete that row.
    */
    pub(crate) fn deliver_pending_session_terminal_composer_insert(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(handoff) = self
            .pending_session_terminal_composer_insert
            .get(&session_id)
            .cloned()
        else {
            return true;
        };
        if self.agents_chat_mode_sessions.contains(&session_id) {
            // The draft goes back where the user is. The Saved Prompts row
            // stays: reaching the composer is not a confirmed terminal paste.
            self.pending_session_terminal_composer_insert
                .remove(&session_id);
            self.deliver_session_chat_composer_insert(session_id, handoff.content, cx);
            return true;
        }
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.clone())
        {
            if view.update(cx, |view, cx| view.paste_text(&handoff.content, cx)) {
                self.pending_session_terminal_composer_insert
                    .remove(&session_id);
                self.release_session_chat_draft_handoff_stash(handoff, cx);
                return true;
            }
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            let slot_ids = self
                .agents_terminal_ghostty_surfaces
                .keys()
                .copied()
                .filter(|slot_id| slot_id.session_id == session_id)
                .collect::<Vec<_>>();
            for slot_id in slot_ids {
                if self.send_text_bytes_to_mounted_agents_terminal_surface(
                    slot_id,
                    handoff.content.as_bytes(),
                ) {
                    self.pending_session_terminal_composer_insert
                        .remove(&session_id);
                    self.release_session_chat_draft_handoff_stash(handoff, cx);
                    return true;
                }
            }
            if self.project_editor_companion_focused_terminal_session_id() == Some(session_id)
                && self.send_text_bytes_to_focused_project_editor_companion_terminal_surface(
                    handoff.content.as_bytes(),
                )
            {
                self.pending_session_terminal_composer_insert
                    .remove(&session_id);
                self.release_session_chat_draft_handoff_stash(handoff, cx);
                return true;
            }
        }
        false
    }

    /*
    A terminal surface can still be remounting when the draft handoff answer
    arrives; surface reconciliation is event-driven, so poll briefly instead
    of waiting for the next unrelated event. Running out of attempts leaves
    the record parked (and the draft in Saved Prompts) for the next focus
    handoff or return to chat — never dropped.
    */
    pub(crate) fn schedule_pending_session_terminal_composer_insert_delivery(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            for _ in 0..PENDING_TERMINAL_COMPOSER_INSERT_MAX_ATTEMPTS {
                cx.background_executor()
                    .timer(PENDING_TERMINAL_COMPOSER_INSERT_RETRY_INTERVAL)
                    .await;
                match this.update(cx, |this, cx| {
                    this.deliver_pending_session_terminal_composer_insert(session_id, cx)
                }) {
                    Ok(false) => {}
                    // Delivered, moved back to chat, or the app is gone.
                    _ => return,
                }
            }
        })
        .detach();
    }

    pub(crate) fn insert_prompt_into_session_chat(
        &mut self,
        session_id: TerminalSessionId,
        content: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return false;
        };
        let literal = serde_json::json!({ "content": content })
            .to_string()
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        let script = format!(
            "(function(){{var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatInsertPromptRequested==='function'){{ns.onSessionChatInsertPromptRequested({literal});}}}})(); undefined;"
        );
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
        true
    }

    /*
    CDXC:GPUISessionChatLinks 2026-08-03:
    A web link in the conversation opens where the reader already is: the
    integrated Browser workarea, through the same renderer-open path as
    `ghostex browser open` (same-origin reuse, so re-clicking a dev-server URL
    does not multiply tabs). Shift+click is the explicit escape hatch to the OS
    browser and takes the http/https-only external opener.

    CDXC:GPUISessionChatLinks 2026-08-18:
    Chat web links answer to the same "Open links in embedded browser" setting
    as Command-clicked terminal links, so a single switch decides where every
    agent-sent web link lands. With that setting off, an ordinary click leaves
    for the system default browser exactly like Shift+click already does.
    */
    pub(crate) fn open_session_chat_link(
        &mut self,
        url: &str,
        external: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if url.chars().count() > GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS {
            return;
        }
        let open_in_app =
            shared_settings::shared_sidebar_settings_snapshot().web_links_open_in_app();
        if external || !open_in_app {
            if let Some(url) = normalize_address(url) {
                let _ = gpui_open_external_http_url(&url);
            }
            return;
        }
        self.open_browser_url_from_renderer_command(
            GpuiSidebarOpenBrowserUrlMessage {
                url: url.to_string(),
                reuse: GpuiBrowserRendererOpenReuse::Similar,
                from_quick_header: false,
                project_id: None,
            },
            window,
            cx,
        );
    }

    /*
    CDXC:GPUISessionChatLinks 2026-08-03:
    Markdown and HTML file links follow their independent Docs/Code settings,
    falling back to the other available workarea. Excalidraw prefers Docs and
    every other file prefers Code. Absolute paths and home-relative paths stay
    machine paths, while ordinary relative paths resolve against the active
    project.
    */
    pub(crate) fn open_session_chat_file(
        &mut self,
        path: &str,
        line: Option<u32>,
        column: Option<u32>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let trimmed = path.trim();
        if trimmed.is_empty() || trimmed.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS {
            return;
        }
        let Some(snapshot) = self.latest_sidebar_project_snapshot.as_ref() else {
            self.report_session_chat_file_open_failure("No project is active in this window.", cx);
            return;
        };
        let Some(project_root) = snapshot.in_memory_project_path.clone() else {
            self.report_session_chat_file_open_failure("No project is active in this window.", cx);
            return;
        };
        let active_project_id = snapshot
            .active_project_id
            .as_ref()
            .map(|project_id| project_id.0.clone());
        // One stat of a path the reader just clicked; no directory walk, so it
        // stays on the caller's thread like the other click handlers here.
        let candidate = if Path::new(trimmed).is_absolute() {
            PathBuf::from(trimmed)
        } else if trimmed == "~" {
            home_dir()
        } else if let Some(relative) = trimmed
            .strip_prefix("~/")
            .or_else(|| trimmed.strip_prefix("~\\"))
        {
            home_dir().join(relative)
        } else {
            project_root.join(trimmed)
        };
        let (Ok(file_path), Ok(root)) = (
            fs::canonicalize(&candidate),
            fs::canonicalize(&project_root),
        ) else {
            self.copy_unresolved_session_chat_file_path(trimmed, cx);
            return;
        };
        if !fs::metadata(&file_path).is_ok_and(|metadata| metadata.is_file()) {
            self.report_session_chat_file_open_failure("That path is not a file.", cx);
            return;
        }
        let project_relative_path = file_path
            .strip_prefix(&root)
            .ok()
            .map(|relative| relative.to_string_lossy().replace('\\', "/"));
        let resolved_path = file_path.to_string_lossy().into_owned();
        let extension = file_path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let document_preferred_view = match extension.as_deref() {
            Some("md" | "markdown" | "mdown" | "mkdn") => Some(settings.markdown_file_open_view()),
            Some("htm" | "html") => Some(settings.html_file_open_view()),
            Some("excalidraw") => Some(shared_settings::SharedChatFileOpenView::Docs),
            _ => None,
        };
        let docs_available = !gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage)
            && self.titlebar_mode_available(TitlebarMode::Manage);
        let code_unavailable_reason =
            if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
                || !self.titlebar_mode_available(TitlebarMode::Source)
            {
                Some("Code view is not available for this project.")
            } else {
                self.embedded_code_editor_unavailable_reason()
            };
        let code_available = code_unavailable_reason.is_none();
        let destination = match document_preferred_view {
            Some(shared_settings::SharedChatFileOpenView::Docs) if docs_available => {
                Some(shared_settings::SharedChatFileOpenView::Docs)
            }
            Some(shared_settings::SharedChatFileOpenView::Docs) if code_available => {
                Some(shared_settings::SharedChatFileOpenView::Code)
            }
            Some(shared_settings::SharedChatFileOpenView::Code) if code_available => {
                Some(shared_settings::SharedChatFileOpenView::Code)
            }
            Some(shared_settings::SharedChatFileOpenView::Code) if docs_available => {
                Some(shared_settings::SharedChatFileOpenView::Docs)
            }
            Some(_) => None,
            None if code_available => Some(shared_settings::SharedChatFileOpenView::Code),
            None => None,
        };

        if destination.is_none() {
            if document_preferred_view.is_some() {
                self.copy_path_for_unavailable_project_workarea(
                    &resolved_path,
                    "Docs and Code",
                    "Docs and Code views are not available for this project.",
                    cx,
                );
            } else {
                self.copy_path_for_unavailable_project_workarea(
                    &resolved_path,
                    "Code",
                    code_unavailable_reason
                        .unwrap_or("Code view is not available for this project."),
                    cx,
                );
            }
            return;
        }

        if destination == Some(shared_settings::SharedChatFileOpenView::Docs) {
            let docs_folders =
                gpui_manage_additional_docs_folders_text(&self.sidebar_runtime_settings_snapshot);
            let normal_docs_path = project_relative_path.clone().filter(|relative| {
                manage_path_is_in_docs_scan_root(relative, &docs_folders)
                    || manage_is_root_artifact_file_relative_path(relative)
            });
            let relative_path = if let Some(relative_path) = normal_docs_path {
                if let Ok(mut authorization) = self.session_chat_docs_file_authorization.lock() {
                    *authorization = None;
                }
                relative_path
            } else {
                let Some(project_id) = active_project_id else {
                    self.copy_path_for_unavailable_project_workarea(
                        &resolved_path,
                        "Docs",
                        "No active project can authorize this file for Docs.",
                        cx,
                    );
                    return;
                };
                let Some(parent) = file_path.parent().map(Path::to_path_buf) else {
                    self.report_session_chat_file_open_failure(
                        "That document has no containing folder.",
                        cx,
                    );
                    return;
                };
                let Some(file_name) = file_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                else {
                    self.report_session_chat_file_open_failure("That path is not a file.", cx);
                    return;
                };
                let Ok(mut authorization) = self.session_chat_docs_file_authorization.lock() else {
                    self.report_session_chat_file_open_failure(
                        "Docs could not authorize that file.",
                        cx,
                    );
                    return;
                };
                *authorization = Some(GpuiSessionChatDocsFileAuthorization {
                    file_name: file_name.clone(),
                    project_id,
                    root: parent,
                });
                format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{file_name}")
            };
            self.report_session_chat_file_opening("Docs view", &file_path, cx);
            self.pending_docs_file_open = Some(relative_path);
            self.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
            self.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
            self.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
            if !self.deliver_pending_docs_file_open(cx) {
                self.schedule_pending_docs_file_open_delivery(cx);
            }
            return;
        }
        if let Ok(mut authorization) = self.session_chat_docs_file_authorization.lock() {
            *authorization = None;
        }
        self.report_session_chat_file_opening("Code view", &file_path, cx);
        self.pending_source_file_open = Some(PendingSourceFileOpen {
            column,
            file_path,
            line,
            origin: PendingSourceFileOpenOrigin::SessionChat,
            project_path: root,
        });
        self.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Source, cx);
        self.focus_project_editor_surface(TitlebarMode::Source, window, cx);
    }

    pub(crate) fn copy_path_for_disabled_project_workarea(
        &mut self,
        path: &str,
        plugin_name: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: format!(
                    "gpui-disabled-{}-file-path-copied",
                    plugin_name.to_ascii_lowercase()
                ),
                level: GpuiAppToastLevel::from_raw(Some("success")),
                title: "Copied to Clipboard!".to_string(),
                description: Some(format!("({plugin_name} plugin is disabled)")),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    pub(crate) fn copy_path_for_unavailable_project_workarea(
        &mut self,
        path: &str,
        workarea_name: &str,
        reason: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: format!(
                    "gpui-unavailable-{}-file-path-copied",
                    workarea_name.to_ascii_lowercase()
                ),
                level: GpuiAppToastLevel::from_raw(Some("success")),
                title: "Copied path to clipboard".to_string(),
                description: Some(reason.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    /**
    CDXC:GPUISessionChatLinks 2026-08-23:
    A path that does not resolve here is still the answer to "which file was
    that?": an agent quotes partial paths, paths relative to a subdirectory it
    was working in, and paths on a remote checkout, any of which can name a
    file that is sitting right there on disk. The toast names that (missing
    file or incomplete path), copies the path, and points at Code view's file
    search, which is the tool that turns a fragment back into the real file.
    When Code is not reachable at all this defers to the disabled-workarea
    copy, so the toast never names a place the reader cannot go.
    */
    pub(crate) fn copy_unresolved_session_chat_file_path(
        &mut self,
        path: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
            || !self.titlebar_mode_available(TitlebarMode::Source)
        {
            self.copy_path_for_disabled_project_workarea(path, "Code", cx);
            return;
        }
        // Naming Code view is only useful advice when Code view can actually
        // search: with the component uninstalled the tab is a prompt to install
        // it, so the toast stops at the copy.
        let description = if self.embedded_code_editor_unavailable_reason().is_some() {
            "This path did not resolve in the active project.".to_string()
        } else {
            "Copied the path. Try searching for it in the code view".to_string()
        };
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-session-chat-unresolved-file-path-copied".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("success")),
                title: "Couldn't open this file (missing file/incomplete path)".to_string(),
                description: Some(description),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    /** Main-window toast for a chat file link that cannot be opened. */
    pub(crate) fn report_session_chat_file_open_failure(
        &mut self,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-session-chat-file-open-failed".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("warning")),
                title: "Could not open the file".to_string(),
                description: Some(description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    /**
    Main-window toast naming where a clicked chat file link is going. The
    workarea switch takes the reader off the chat pane and Code/Docs can take a
    moment to show the file, so the toast is what tells them the click landed.
    */
    pub(crate) fn report_session_chat_file_opening(
        &mut self,
        destination: &str,
        file_path: &Path,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID.to_string(),
                level: GpuiAppToastLevel::from_raw(None),
                title: format!("Opening file in {destination}"),
                description: Some(file_path.to_string_lossy().into_owned()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    /**
    `None` when the bundled code-server component can actually open a file right
    now, otherwise the reason to show the reader. This is the same availability
    read the Source workarea launches on, so a `None` here means the workarea
    switch ends on the file rather than on the component installer prompt.
    */
    pub(crate) fn embedded_code_editor_unavailable_reason(&self) -> Option<&'static str> {
        let Some(target) = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot))
        else {
            return Some("Code view is not available for this project.");
        };
        match source_code_server_runtime_availability(&target) {
            SourceCodeServerRuntimeAvailability::Available => None,
            SourceCodeServerRuntimeAvailability::InstallRequired => {
                Some("Code view is not installed on this machine.")
            }
            SourceCodeServerRuntimeAvailability::Failed(_) => {
                Some("Code view is unavailable on this machine.")
            }
        }
    }

    /**
    Hands the pending Docs path to the Manage page. The surface may not exist
    yet (the mode was just switched) and the page may still be loading, so the
    request stays pending until the script lands and the injected script itself
    waits for the page's own open hook, the same retry shape the Manage bridge
    shim uses.
    */
    pub(crate) fn deliver_pending_docs_file_open(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(relative_path) = self.pending_docs_file_open.clone() else {
            return false;
        };
        let Some(surface) = self
            .project_workarea_runtime_cef_surfaces
            .get(&ProjectWorkareaCefSurfaceSlotKey::Manage)
            .map(|owned_surface| owned_surface.surface.clone())
        else {
            return false;
        };
        // JSON is a valid JS literal except for U+2028/U+2029; escape them so
        // a pathological file name cannot break the generated script.
        let literal = serde_json::Value::String(relative_path)
            .to_string()
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        let script = format!(
            "(function(){{var p={literal};var a=0;var send=function(){{var open=window.ghostexOpenDocsFile;if(typeof open==='function'){{open(p);return;}}if(++a<250){{setTimeout(send,20);}}}};send();}})(); undefined;"
        );
        let dispatched = surface.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        if dispatched {
            self.pending_docs_file_open = None;
        }
        dispatched
    }

    /**
    Docs may need a moment before it can take the request: the mode switch
    creates the surface, and CEF has no main frame to run the script in until
    the page commits. Surface reconciliation is event-driven, so poll briefly
    instead of waiting for the next unrelated event, and drop the request if
    Docs never comes up.
    */
    pub(crate) fn schedule_pending_docs_file_open_delivery(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            for _ in 0..PENDING_DOCS_FILE_OPEN_MAX_ATTEMPTS {
                cx.background_executor()
                    .timer(PENDING_DOCS_FILE_OPEN_RETRY_INTERVAL)
                    .await;
                match this.update(cx, |this, cx| this.deliver_pending_docs_file_open(cx)) {
                    Ok(false) => {}
                    // Delivered, or the app is gone.
                    _ => return,
                }
            }
            let _ = this.update(cx, |this, _| {
                this.pending_docs_file_open = None;
            });
        })
        .detach();
    }
}
