// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: stash-prompt requests, session-chat handoff, and the Monaco prompt-editor shortcut/attach/deliver/report pipeline.

use std::{fs, path::PathBuf};

use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Routes Stash Prompt to the input surface the user can currently see.
    /// Chat owns its React draft; terminal mode owns the agent CLI composer.
    pub(crate) fn request_gpui_stash_prompt_for_active_input(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_chat_mode_sessions.contains(&shell_session_id) {
            self.request_session_chat_stash_prompt(shell_session_id, cx);
        } else {
            self.request_gpui_stash_current_prompt(shell_session_id, cx);
        }
    }

    /// Terminal-mode stash reuses the Ctrl+G contract headlessly: write a
    /// one-shot marker, then send BEL. The agent CLI writes its composer to
    /// the editor file; `ghostex prompt-editor` saves it and clears the input.
    pub(crate) fn request_gpui_stash_current_prompt(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self.local_workspace_key_for_shell_session(shell_session_id) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Stash Prompt unavailable",
                "This terminal is not attached to a gxserver session.",
                cx,
            );
            return;
        };
        if !gpui_write_prompt_stash_request_marker(&key.project_id, &key.session_id, "1\n") {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Stash Prompt failed",
                "Could not write the stash request marker.",
                cx,
            );
            return;
        }
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .map(|record| record.view.clone())
        {
            view.update(cx, |view, cx| view.send_text_input("\u{7}", cx));
            return;
        }
        #[cfg(target_os = "macos")]
        if self.send_text_bytes_to_focused_agents_terminal_surface(b"\x07") {
            return;
        }
        let _ = gpui_remove_prompt_stash_request_marker(&key.project_id, &key.session_id);
    }

    pub(crate) fn request_terminal_handoff_to_session_chat(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        // This is the Terminal View button's directional command. Native
        // terminal events and CEF host messages travel through separate
        // queues, so a rapid click can otherwise arrive after Chat View is
        // already visible and act like a second toggle. A stale request to
        // show the view that is already showing is a no-op.
        if self.agents_chat_mode_sessions.contains(&shell_session_id) {
            return;
        }
        if !self.agents_session_chat_eligible(shell_session_id) {
            return;
        }
        if !self.show_agents_session_chat_mode(shell_session_id, cx) {
            return;
        }
        /*
        CDXC:SessionChat 2026-08-21:
        Show Chat before asking the daemon to copy the terminal draft. Agent
        startup prompts, permission prompts, shell state, and older CLIs may
        not answer their prompt-editor command; none of those terminal states
        may veto the view switch. The daemon handshake is already loss-safe:
        success clears and delivers the captured draft, while failure leaves it
        in the parked terminal.
        */
        self.request_session_chat_draft_transfer(shell_session_id, cx);
    }

    /// The terminal "Prompts" overlay action opens the stashed-prompts recall
    /// modal scoped to the focused mapped gxserver session, so its default
    /// view is "this project and its worktrees" and a selected prompt can be
    /// inserted back into this exact terminal. Unmapped local placeholder tabs
    /// still open the modal in all-projects browse mode.
    pub(crate) fn open_gpui_stashed_prompts_modal_for_focused_agents_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(shell_session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        let key = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()));
        let modal = GpuiAppModalKind::StashedPrompts;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "modal": modal.modal_id(),
            "type": "open",
        });
        if let Some(key) = key {
            open_message["projectId"] = serde_json::Value::String(key.project_id.clone());
            open_message["sessionId"] = serde_json::Value::String(
                gpui_combined_presentation_session_id(&key.project_id, &key.session_id),
            );
        }
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn handle_gpui_engine_prompt_editor_shortcut(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let GpuiEngineTerminalEventTarget::Agents(shell_session_id) = target
            && self.agents_session_chat_transcript_agent(shell_session_id) == Some("grok")
        {
            self.send_grok_prompt_editor_command_to_gpui_engine_terminal(
                shell_session_id,
                runtime_session_id,
                cx,
            );
            return;
        }
        let remote_context = match target {
            GpuiEngineTerminalEventTarget::Agents(shell_session_id) => self
                .remote_prompt_editor_context_for_shell_session(shell_session_id)
                .map(|(key, connection_generation)| (shell_session_id, key, connection_generation)),
            GpuiEngineTerminalEventTarget::Command(_) => None,
        };
        if let Some((shell_session_id, key, connection_generation)) = remote_context {
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.queue_remote_prompt_editor_request(
                        shell_session_id,
                        &key,
                        connection_generation,
                        RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                            target,
                            runtime_session_id,
                        },
                        window,
                        cx,
                    );
                });
            })
            .detach();
            return;
        }
        self.warn_if_monaco_prompt_editor_helper_is_missing(cx);
        let Some(originating_session_id) =
            self.prompt_editor_originating_session_id_for_engine_target(target)
        else {
            self.send_prompt_editor_shortcut_to_gpui_engine_terminal(
                target,
                runtime_session_id,
                cx,
            );
            return;
        };

        cx.spawn(async move |this, cx| {
            let fronted = cx
                .background_executor()
                .spawn(
                    async move { gpui_ghostex_editor_daemon_front(Some(&originating_session_id)) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if fronted {
                    if !this.prompt_editor_daemon_open {
                        this.prompt_editor_daemon_open = true;
                        cx.notify();
                    }
                } else {
                    this.send_prompt_editor_shortcut_to_gpui_engine_terminal(
                        target,
                        runtime_session_id,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /*
    The app is the only place that knows both that the user asked for the Monaco
    prompt editor and that no GhostexEditor daemon is installed to serve it. The
    terminal-side CLI only sees the negotiated `editor` capability, so it opens
    the machine editor (vi, for anyone with no $EDITOR) without a word. Say so
    where the user is looking instead of degrading silently.
    */
    pub(crate) fn warn_if_monaco_prompt_editor_helper_is_missing(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if !gpui_prompt_editor_backend_setting_is_monaco()
            || gpui_resolved_ghostex_editor_executable().is_some()
        {
            return;
        }
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                this.upsert_gpui_app_toast(
                    GpuiAppToast {
                        id: GPUI_MISSING_MONACO_PROMPT_EDITOR_TOAST_ID.to_string(),
                        level: GpuiAppToastLevel::Warning,
                        title: "Monaco prompt editor unavailable".to_string(),
                        description: Some(
                            "The Ghostex Editor helper is missing from this build, so Ctrl+G opens the machine editor instead. Set GHOSTEX_EDITOR_APP or reinstall Ghostex."
                                .to_string(),
                        ),
                        loading: false,
                        persistent: false,
                        duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                        epoch: 0,
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn remote_prompt_editor_context_for_shell_session(
        &self,
        shell_session_id: TerminalSessionId,
    ) -> Option<(GpuiRemoteAttachSessionKey, u64)> {
        let GpuiWorkspaceTerminalSessionKey::Remote(key) =
            self.workspace_terminal_key_for_shell_session(shell_session_id)?
        else {
            return None;
        };
        let connection_generation = self
            .remote_gxserver_connect_generations
            .get(key.remote_machine_id.as_str())
            .copied()?;
        Some((key, connection_generation))
    }

    pub(crate) fn queue_remote_prompt_editor_request(
        &mut self,
        shell_session_id: TerminalSessionId,
        key: &GpuiRemoteAttachSessionKey,
        connection_generation: u64,
        delivery_target: RemotePromptEditorDeliveryTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let context_is_current = self
            .remote_prompt_editor_context_for_shell_session(shell_session_id)
            .is_some_and(|(current_key, current_generation)| {
                current_key == *key && current_generation == connection_generation
            });
        let scoped_project_id =
            gpui_remote_scoped_project_id(key.remote_machine_id.as_str(), key.project_id.as_str());
        let source_target = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .filter(|snapshot| {
                snapshot.active_project_id.as_ref().map(|id| id.0.as_str())
                    == Some(scoped_project_id.as_str())
            })
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot));
        let source_target_is_current = source_target.as_ref().is_some_and(|target| {
            matches!(
                &target.endpoint,
                SourceCodeServerRuntimeEndpoint::Remote {
                    connection_generation: target_generation,
                    remote_machine_id,
                    ..
                } if remote_machine_id == &key.remote_machine_id
                    && *target_generation == connection_generation
            )
        });
        if !context_is_current || !source_target_is_current {
            self.report_remote_prompt_editor_failure(
                "The remote project or session changed before its editor could open.",
                cx,
            );
            return false;
        }
        if !self.set_active_mode(TitlebarMode::Source, window, cx) {
            self.report_remote_prompt_editor_failure(
                "Code view is unavailable for this remote project.",
                cx,
            );
            return false;
        }
        self.focus_project_editor_surface(TitlebarMode::Source, window, cx);
        let Some(source_target) = source_target else {
            return false;
        };
        let runtime_is_owned_for_request = matches!(
            self.source_code_server_runtime.state,
            SourceCodeServerRuntimeLaunchState::Launching
                | SourceCodeServerRuntimeLaunchState::Ready
        ) && self.source_code_server_runtime.target.as_ref()
            == Some(&source_target);
        if !runtime_is_owned_for_request {
            self.report_remote_prompt_editor_failure(
                "Code view is unavailable for this remote project.",
                cx,
            );
            return false;
        }
        let source_runtime_generation = self.source_code_server_runtime.generation;
        self.source_code_server_runtime
            .queue_remote_prompt_editor_request(PendingRemotePromptEditorRequest {
                shell_session_id,
                remote_key: key.clone(),
                connection_generation,
                source_target,
                source_runtime_generation,
                delivery_target,
            });
        self.deliver_pending_remote_prompt_editor_request_if_ready(cx);
        true
    }

    pub(crate) fn deliver_pending_remote_prompt_editor_request_if_ready(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(request) = self
            .source_code_server_runtime
            .pending_remote_prompt_editor_request
            .clone()
        else {
            return false;
        };
        if !self
            .source_code_server_runtime
            .owns_ready_remote_prompt_editor_ipc(&request)
        {
            let still_waiting_for_owned_runtime = self.source_code_server_runtime.state
                == SourceCodeServerRuntimeLaunchState::Launching
                && self.source_code_server_runtime.generation == request.source_runtime_generation
                && self.source_code_server_runtime.target.as_ref() == Some(&request.source_target);
            if !still_waiting_for_owned_runtime {
                self.source_code_server_runtime
                    .pending_remote_prompt_editor_request = None;
            }
            return false;
        }

        let authoritative_session_is_current = self
            .remote_prompt_editor_context_for_shell_session(request.shell_session_id)
            .is_some_and(|(current_key, current_generation)| {
                current_key == request.remote_key
                    && current_generation == request.connection_generation
            });
        let authoritative_target_is_current = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot))
            .is_some_and(|target| target == request.source_target);
        let delivery_target_is_current = match request.delivery_target {
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(
                FocusedTerminalTextMountTarget::Agents(slot_id),
            ) => slot_id.session_id == request.shell_session_id,
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(
                FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id),
            ) => slot_id.session_id == request.shell_session_id,
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(
                FocusedTerminalTextMountTarget::Command(_),
            ) => false,
            RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                target: GpuiEngineTerminalEventTarget::Agents(shell_session_id),
                runtime_session_id,
            } => {
                shell_session_id == request.shell_session_id
                    && self
                        .agents_gpui_engine_terminals
                        .get(&shell_session_id)
                        .is_some_and(|record| record.runtime_session_id == runtime_session_id)
            }
            RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                target: GpuiEngineTerminalEventTarget::Command(_),
                ..
            } => false,
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeView(native_view) => {
                self.agents_terminal_session_id_containing_responder(
                    native_view as *mut std::ffi::c_void,
                )
                .or_else(|| {
                    self.project_editor_companion_terminal_session_id_containing_responder(
                        native_view as *mut std::ffi::c_void,
                    )
                }) == Some(request.shell_session_id)
            }
        };
        if !authoritative_session_is_current
            || !authoritative_target_is_current
            || !delivery_target_is_current
        {
            self.source_code_server_runtime
                .pending_remote_prompt_editor_request = None;
            return false;
        }

        self.source_code_server_runtime
            .pending_remote_prompt_editor_request = None;
        match request.delivery_target {
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(target) => {
                self.send_prompt_editor_shortcut_to_native_terminal_target(target)
            }
            RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                target,
                runtime_session_id,
            } => {
                self.send_prompt_editor_shortcut_to_gpui_engine_terminal(
                    target,
                    runtime_session_id,
                    cx,
                );
                true
            }
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeView(native_view) => {
                terminal_ghostty_surface::send_native_prompt_editor_shortcut_for_view(
                    native_view as *mut std::ffi::c_void,
                )
            }
        }
    }

    pub(crate) fn report_remote_prompt_editor_failure(
        &mut self,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-remote-prompt-editor-failed".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("warning")),
                title: "Remote prompt editor unavailable".to_string(),
                description: Some(description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    pub(crate) fn prompt_editor_originating_session_id_for_engine_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<String> {
        let key = match target {
            GpuiEngineTerminalEventTarget::Agents(shell_session_id) => self
                .local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (*mapped_session_id == shell_session_id).then_some(key)
                }),
            GpuiEngineTerminalEventTarget::Command(session_id) => {
                self.command_gxserver_session_mappings.get(&session_id)
            }
        }?;
        Some(format!("{}:{}", key.project_id, key.session_id))
    }

    pub(crate) fn send_prompt_editor_shortcut_to_gpui_engine_terminal(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let record = match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => {
                self.agents_gpui_engine_terminals.get(&session_id)
            }
            GpuiEngineTerminalEventTarget::Command(session_id) => {
                self.command_gpui_engine_terminals.get(&session_id)
            }
        };
        let Some(view) = record
            .filter(|record| record.runtime_session_id == runtime_session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        view.update(cx, |view, cx| view.send_text_input("\u{7}", cx));
    }

    /// Grok Build owns Ctrl+G for Tasks. Its prompt editor is a command-palette
    /// action, so invoke the same Ctrl+P, `editor`, Enter sequence a user types.
    pub(crate) fn send_grok_prompt_editor_command_to_gpui_engine_terminal(
        &mut self,
        shell_session_id: TerminalSessionId,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .filter(|record| record.runtime_session_id == runtime_session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        view.update(cx, |view, cx| view.send_text_input("\u{10}editor\r", cx));
    }

    pub(crate) fn open_gpui_engine_terminal_action_url(
        &mut self,
        value: &str,
        working_directory: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > 2048 {
            return;
        }
        let open_value = gpui_terminal_markdown_image_reference_path(trimmed).unwrap_or(trimmed);
        if gpui_terminal_link_is_web_url(open_value) {
            if !shared_settings::shared_sidebar_settings_snapshot().web_links_open_in_app() {
                let _ = gpui_open_terminal_action_url(open_value);
                return;
            }
            /*
            CDXC:Extensions 2026-08-23:
            Answer the Customize refusal here rather than letting the open take
            the renderer round trip only to be refused on the way back: the
            reader cmd+clicked, so the copied link and its toast should land on
            the same interaction.
            */
            if !self.titlebar_mode_available(TitlebarMode::Browser) {
                self.copy_path_for_disabled_project_workarea(open_value, "Browser", cx);
                return;
            }
            let Some(sidebar) = self.sidebar.clone() else {
                return;
            };
            let payload = serde_json::json!({
                "reuse": "similar",
                "type": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
                "url": open_value,
                "version": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
            });
            let script = format!(
                "(function(){{const post=window.ghostexGpui?.postOpenBrowserUrl;if(typeof post==='function'){{post(JSON.stringify({payload}));}}}})(); undefined;"
            );
            sidebar.update(cx, |surface, _| {
                surface.execute_app_owned_script(&script);
            });
            return;
        }
        let Some(file_link_path) = gpui_terminal_file_link_path(open_value) else {
            let _ = gpui_open_terminal_action_url(open_value);
            return;
        };
        let file_path = if file_link_path.is_absolute() {
            file_link_path
        } else if let Some(working_directory) = working_directory
            .map(str::trim)
            .filter(|working_directory| !working_directory.is_empty())
        {
            PathBuf::from(working_directory).join(file_link_path)
        } else {
            file_link_path
        };
        if gpui_terminal_file_opens_with_os_default(&file_path)
            && fs::metadata(&file_path).is_ok_and(|metadata| metadata.is_file())
        {
            if let Err(message) = gpui_open_path(&file_path) {
                self.report_session_chat_file_open_failure(&message, cx);
            }
            return;
        }
        let file_path = file_path.to_string_lossy().to_string();
        cx.spawn(async move |this, cx| {
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_session_chat_file(&file_path, None, None, window, cx);
            });
        })
        .detach();
    }
}
