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
        if message.get("type").and_then(serde_json::Value::as_str) != Some("sessionChatHostAction")
        {
            return;
        }
        let Some(action) = message.get("action").and_then(serde_json::Value::as_str) else {
            return;
        };
        if action == "composerReady" {
            if self.agents_chat_mode_sessions.contains(&session_id)
                && self.agents_chat_surfaces.contains_key(&session_id)
            {
                self.session_chat_composer_ready_sessions.insert(session_id);
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
        if action == "draftHandoffToTerminalComplete" {
            if !self.pending_session_chat_draft_handoffs.remove(&session_id) {
                return;
            }
            let content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !content.is_empty() {
                // The terminal view already owns the pane while this
                // asynchronous copy finishes. Keep the cleared composer
                // snapshot until the exact terminal owner has remounted and
                // completed its focus handoff; inserting synchronously here
                // still races that remount.
                self.pending_session_terminal_composer_insert
                    .insert(session_id, content);
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
        handler here, so the bytes ride the bridge and Rust runs the native
        save panel and writes the file itself. The reply carries an error
        string only when the write actually failed: a cancelled panel is a
        completed request, not an error.
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
            self.open_session_chat_file(path, window, cx);
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
        // A file name, never a path: the page supplies it, and the panel's
        // directory is ours to choose.
        let file_name = Path::new(&suggested_name)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "image.png".to_string());
        // Downloads is where a saved picture belongs by default; the panel lets
        // the user go anywhere else from there.
        let home = home_dir();
        let downloads = home.join("Downloads");
        let directory = if downloads.is_dir() { downloads } else { home };
        let receiver = cx.prompt_for_new_path(&directory, Some(file_name.as_str()));
        cx.spawn(async move |this, cx| {
            let destination = match receiver.await {
                Ok(Ok(Some(path))) => path,
                // Cancelled, or the panel could not open: nothing was written
                // and the panel already told the user. Settle the page promise.
                _ => {
                    let _ = this.update(cx, |this, cx| {
                        this.deliver_session_chat_image_save(session_id, &request_id, None, cx);
                    });
                    return;
                }
            };
            let error = match std::fs::write(&destination, &bytes) {
                Ok(()) => None,
                Err(error) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::AppModal,
                        "gpui.sessionChat.imageSaveFailed",
                        serde_json::json!({
                            "error": error.to_string(),
                            "path": destination.display().to_string(),
                            "stage": "write",
                        }),
                    );
                    Some("The image could not be written.")
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.deliver_session_chat_image_save(session_id, &request_id, error, cx);
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
    A file link opens in the workarea that can actually show it. Docs is a
    docs-scoped browser (the docs/ folder, configured extra Docs folders, and
    root Markdown/HTML/Excalidraw artifacts), so a Markdown/HTML/Excalidraw
    file inside that scope goes to Docs and everything else — including a .md
    that lives outside it — goes to Code, which roots on the whole project.
    Paths are resolved against the active project so a relative path an agent
    quoted works the same as an absolute one.
    */
    pub(crate) fn open_session_chat_file(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let trimmed = path.trim();
        if trimmed.is_empty() || trimmed.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS {
            return;
        }
        let Some(project_root) = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.in_memory_project_path.clone())
        else {
            self.report_session_chat_file_open_failure("No project is active in this window.", cx);
            return;
        };
        // One stat of a path the reader just clicked; no directory walk, so it
        // stays on the caller's thread like the other click handlers here.
        let candidate = if Path::new(trimmed).is_absolute() {
            PathBuf::from(trimmed)
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
        let docs_relative_path = file_path
            .strip_prefix(&root)
            .ok()
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .filter(|relative| {
                let docs_folders = gpui_manage_additional_docs_folders_text(
                    &self.sidebar_runtime_settings_snapshot,
                );
                session_chat_file_opens_in_docs(relative, &docs_folders)
            });
        if let Some(relative_path) = docs_relative_path {
            if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage) {
                self.copy_path_for_disabled_project_workarea(trimmed, "Docs", cx);
                return;
            }
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
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
            || !self.titlebar_mode_available(TitlebarMode::Source)
        {
            /*
            CDXC:GPUTitlebarAvailability 2026-08-20:
            Source is also unavailable for remote projects, where switching
            would be refused by set_active_mode and leave this route silently
            dead. Fall back to the same copy-the-path affordance the hidden-tab
            case uses instead of parking on an unreachable workarea.
            */
            self.copy_path_for_disabled_project_workarea(trimmed, "Code", cx);
            return;
        }
        /*
        Code view is an on-demand component, so it is not always installed.
        Switching to Source without it only parks the reader on the component
        installer prompt with the file still unopened, so reveal the file in the
        OS file manager instead and let them open it with what they do have.
        */
        if let Some(reason) = self.embedded_code_editor_unavailable_reason() {
            self.reveal_session_chat_file_in_file_manager(&file_path, reason, cx);
            return;
        }
        self.report_session_chat_file_opening("Code view", &file_path, cx);
        self.pending_source_file_open = Some(PendingSourceFileOpen {
            file_path,
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

    /**
    CDXC:GPUISessionChatLinks 2026-08-23:
    A path that does not resolve here is still the answer to "which file was
    that?", and the reason is usually not that the file is gone: an agent
    quotes partial paths, paths relative to a subdirectory it was working in,
    and paths on a remote checkout, any of which can name a file that is
    sitting right there on disk. So the toast claims nothing about the file. It
    copies the path and points at Code view's file search, which is the tool
    that turns a fragment back into the real file. When Code is not reachable
    at all this defers to the disabled-workarea copy, so the toast never names
    a place the reader cannot go.
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
            "This path did not resolve here. Search for the file in Code view.".to_string()
        };
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-session-chat-unresolved-file-path-copied".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("success")),
                title: "Copied path to clipboard".to_string(),
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
                id: "gpui-session-chat-file-opening".to_string(),
                level: GpuiAppToastLevel::from_raw(None),
                title: format!("Opening file in {destination}"),
                description: gpui_path_file_name_label(file_path),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    /** Chat file link on a machine with no usable Code view: reveal it instead. */
    pub(crate) fn reveal_session_chat_file_in_file_manager(
        &mut self,
        file_path: &Path,
        reason: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Err(message) = gpui_reveal_path_in_finder(file_path) {
            self.report_session_chat_file_open_failure(&message, cx);
            return;
        }
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-session-chat-file-opening".to_string(),
                level: GpuiAppToastLevel::from_raw(None),
                title: format!("Opening file in {GPUI_FILE_MANAGER_NAME}"),
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
    pub(crate) fn schedule_pending_docs_file_open_delivery(&mut self, cx: &mut gpui::Context<Self>) {
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
