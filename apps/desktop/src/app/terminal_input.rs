// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: clipboard/paste, IME preedit, text-input handoff, close-confirm dialogs

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::Ordering;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Bounds;
use gpui::ClipboardItem;
use gpui::Entity;
use gpui::Focusable as _;
use gpui::Pixels;
use gpui::Window;
use gpui_component::WindowExt;
use gpui_component::button::ButtonVariant;
use gpui_component::dialog::DialogButtonProps;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn paste_into_focused_terminal_from_clipboard(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUITerminalPaste 2026-06-23-09:59:
        Cmd+V terminal paste is scoped to the shell focus model instead of the body mouse handlers: only the currently focused mounted Agents or command Ghostty surface can receive clipboard bytes. Clipboard contents stay ephemeral, explicit-string-only, and are never logged, persisted, or converted from file paths.

        CDXC:GPUITerminalImagePaste 2026-06-27-10:28:
        Direct GPUI terminal paste uses the same runtime-only previewable-image setting as macOS before targeting the focused mounted Ghostty surface. Disabled keeps explicit-string-only behavior; enabled converts only validated image file references or raw image bytes into Markdown before any terminal insertion.
        */
        let Some(item) = cx.read_from_clipboard() else {
            support_logs::append_temporary(
                support_logs::GpuiSupportLog::TerminalFocus,
                "TEMP.gpui.fluidVoice.clipboardPaste",
                serde_json::json!({
                    "accepted": false,
                    "reason": "clipboardUnavailable",
                }),
            );
            return false;
        };
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        if paste_previewable_images_enabled
            && self.paste_clipboard_image_into_focused_remote_terminal(&item, cx)
        {
            return true;
        }
        let Some(text) = terminal_clipboard_paste_text(
            &item,
            paste_previewable_images_enabled,
            self.focused_terminal_is_factory_droid(),
        ) else {
            support_logs::append_temporary(
                support_logs::GpuiSupportLog::TerminalFocus,
                "TEMP.gpui.fluidVoice.clipboardPaste",
                serde_json::json!({
                    "accepted": false,
                    "reason": "clipboardHadNoAcceptedText",
                }),
            );
            return false;
        };

        let accepted = self.paste_text_into_focused_terminal_surface(&text, cx);
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.clipboardPaste",
            serde_json::json!({
                "accepted": accepted,
                "text": support_logs::temporary_fluid_voice_text_shape(&text),
            }),
        );
        accepted
    }

    pub(crate) fn paste_image_or_send_control_v(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        if enabled && let Some(item) = cx.read_from_clipboard() {
            if self.paste_clipboard_image_into_focused_remote_terminal(&item, cx) {
                return true;
            }
            if let Some(markdown) = terminal_clipboard_previewable_image_markdown_text(&item) {
                let text = if self.focused_terminal_is_factory_droid() {
                    format!("  {markdown}")
                } else {
                    markdown
                };
                return self.paste_text_into_focused_terminal_surface(&text, cx);
            }
        }
        self.send_text_to_focused_terminal_surface("\u{16}", cx)
    }

    /*
    CDXC:GPUITerminalRemoteImagePaste 2026-08-21:
    A remote session's terminal runs on the remote machine, so the local
    "[Image #N](path)" reference a clipboard image normally produces names a
    file the remote agent cannot open. Pasting into a remote terminal therefore
    takes the same route the Attach File or Folder button already takes: stage
    the clipboard payload, upload it over that machine's SSH connection, and
    paste the returned remote path. Ownership of the paste is claimed only once
    a remote image destination is proven (focused remote-attached session, an
    accepted clipboard image, and a reachable remote machine); every other
    clipboard shape falls through to the unchanged local paste path.
    */
    pub(crate) fn paste_clipboard_image_into_focused_remote_terminal(
        &mut self,
        item: &ClipboardItem,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_terminal_shell_session_id() else {
            return false;
        };
        let Some(remote_machine_id) = self.remote_machine_id_for_attached_shell_session(session_id)
        else {
            return false;
        };
        let Some(payload) = terminal_clipboard_image_payload(item) else {
            return false;
        };

        // Engine terminals keep their own runtime identity, so the upload can
        // land in that exact terminal even if focus moved. The ghostty path has
        // no runtime handle here, so it re-checks focus instead.
        let runtime_session_id = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.runtime_session_id);
        let target = GpuiEngineTerminalEventTarget::Agents(session_id);
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) =
            gpui_remote_machine_config_from_settings(settings.object(), remote_machine_id.as_str())
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Image paste unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return true;
        };
        let Some(remote_target) = self.gpui_remote_gxserver_request_target(&remote_machine_id)
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Image paste unavailable",
                "Reconnect the remote machine before pasting an image.",
                cx,
            );
            return true;
        };

        let pad_reference = self.focused_terminal_is_factory_droid();
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Uploading image",
            "Uploading the pasted image to the remote machine.",
            cx,
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_upload_terminal_clipboard_image_to_remote(
                        &config,
                        &remote_target.execution_target,
                        payload,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let destination_is_live = match runtime_session_id {
                    Some(runtime_session_id) => {
                        this.gpui_engine_terminal_target_matches_runtime(target, runtime_session_id)
                    }
                    None => this.focused_terminal_shell_session_id() == Some(session_id),
                };
                if !destination_is_live {
                    return;
                }
                match result {
                    Ok(references) => {
                        let markdown = gpui_terminal_attachment_markdown_text(&references);
                        let text = if pad_reference {
                            format!("  {markdown}")
                        } else {
                            markdown
                        };
                        match runtime_session_id {
                            Some(runtime_session_id) => {
                                this.paste_text_into_gpui_engine_terminal_target(
                                    target,
                                    runtime_session_id,
                                    text.as_str(),
                                    cx,
                                );
                            }
                            None => {
                                this.paste_text_into_focused_terminal_surface(text.as_str(), cx);
                            }
                        }
                    }
                    Err(message) => this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Image upload failed",
                        message.as_str(),
                        cx,
                    ),
                }
            });
        })
        .detach();
        true
    }

    pub(crate) fn focused_terminal_shell_session_id(&self) -> Option<TerminalSessionId> {
        match focused_terminal_text_target(self.active_mode, self.shell_focus) {
            Some(FocusedTerminalTextTarget::Agents) => focused_agents_terminal_surface_mount_slot(
                self.active_mode,
                self.shell_focus,
                &self.agents_workspace,
            )
            .map(|slot| slot.session_id),
            Some(FocusedTerminalTextTarget::ProjectEditorCompanion) => {
                self.project_editor_companion_focused_terminal_session_id()
            }
            _ => None,
        }
    }

    pub(crate) fn focused_terminal_is_factory_droid(&self) -> bool {
        self.focused_terminal_shell_session_id()
            .and_then(|session_id| self.agents_workspace.session(session_id))
            .and_then(|session| session.agent_icon)
            == Some("factory-droid")
    }

    pub(crate) fn remote_machine_id_for_attached_shell_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<String> {
        self.remote_attach_sessions
            .iter()
            .find_map(|(key, mapped_session_id)| {
                (*mapped_session_id == session_id).then(|| key.remote_machine_id.clone())
            })
    }

    pub(crate) fn paste_text_into_focused_terminal_surface(
        &mut self,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // GPUI-engine terminals get real paste semantics (bracketed-paste
        // aware, unsafe bytes stripped) instead of ghostty text insertion.
        if let Some(view) = self.focused_gpui_engine_terminal_view() {
            let paste_protection = shared_settings::shared_sidebar_settings_snapshot()
                .terminal_clipboard_paste_protection();
            if paste_protection
                && view.update(cx, |view, _cx| view.paste_requires_confirmation(text))
            {
                if self.pending_terminal_paste_confirmation.is_none() {
                    self.pending_terminal_paste_confirmation =
                        Some(PendingGpuiTerminalPasteConfirmation {
                            text: text.to_string(),
                            view,
                        });
                    cx.notify();
                }
                return true;
            }
            view.update(cx, |view, cx| view.paste_text(text, cx));
            return true;
        }
        self.send_text_to_focused_terminal_surface(text, cx)
    }

    pub(crate) fn sync_terminal_paste_confirmation_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.terminal_paste_confirmation_dialog_open
            || self.pending_terminal_paste_confirmation.is_none()
        {
            return;
        }
        self.terminal_paste_confirmation_dialog_open = true;
        let entity = cx.entity().clone();
        let ok_entity = entity.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title("Paste potentially unsafe text?")
                .description(
                    "This paste contains a newline or terminal control sequence and may run commands.",
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .ok_text("Paste")
                        .ok_variant(ButtonVariant::Default)
                        .on_ok({
                            let ok_entity = ok_entity.clone();
                            move |_, _, cx| {
                                ok_entity.update(cx, |this, cx| {
                                    if let Some(pending) =
                                        this.pending_terminal_paste_confirmation.take()
                                    {
                                        pending
                                            .view
                                            .update(cx, |view, cx| view.paste_text(&pending.text, cx));
                                    }
                                    this.terminal_paste_confirmation_dialog_open = false;
                                    cx.notify();
                                    true
                                })
                            }
                        })
                        .on_cancel({
                            let entity = entity.clone();
                            move |_, _, cx| {
                                entity.update(cx, |this, cx| {
                                    this.pending_terminal_paste_confirmation = None;
                                    this.terminal_paste_confirmation_dialog_open = false;
                                    cx.notify();
                                    true
                                })
                            }
                        }),
                )
                .overlay_closable(false)
                .close_button(false)
                .keyboard(true)
        });
    }

    /// The GPUI-engine view backing the focused terminal text target, if
    /// the focused slot is engine-claimed.
    pub(crate) fn focused_gpui_engine_terminal_view(
        &self,
    ) -> Option<Entity<terminal_element::TerminalView>> {
        match focused_terminal_text_target(self.active_mode, self.shell_focus)? {
            FocusedTerminalTextTarget::Agents => {
                let slot_id = focused_agents_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    &self.agents_workspace,
                )?;
                self.agents_gpui_engine_terminals
                    .get(&slot_id.session_id)
                    .map(|record| record.view.clone())
            }
            FocusedTerminalTextTarget::Command => {
                let slot_id = focused_command_terminal_surface_mount_slot(
                    self.shell_focus,
                    &self.command_pane,
                )?;
                self.command_gpui_engine_terminals
                    .get(&slot_id.session_id)
                    .map(|record| record.view.clone())
            }
            FocusedTerminalTextTarget::ProjectEditorCompanion => {
                let slot_id = focused_project_editor_companion_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    self.project_editor_companion_focused_terminal_session_id(),
                )?;
                self.agents_gpui_engine_terminals
                    .get(&slot_id.session_id)
                    .map(|record| record.view.clone())
            }
        }
    }

    pub(crate) fn focused_gpui_engine_terminal_action_target(
        &self,
    ) -> Option<(
        GpuiEngineTerminalEventTarget,
        AgentsTerminalRuntimeSessionId,
    )> {
        match self.focused_terminal_text_mount_target()? {
            FocusedTerminalTextMountTarget::Agents(slot_id) => {
                let record = self.agents_gpui_engine_terminals.get(&slot_id.session_id)?;
                Some((
                    GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                    record.runtime_session_id,
                ))
            }
            FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => {
                let record = self.agents_gpui_engine_terminals.get(&slot_id.session_id)?;
                Some((
                    GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                    record.runtime_session_id,
                ))
            }
            FocusedTerminalTextMountTarget::Command(slot_id) => {
                let record = self
                    .command_gpui_engine_terminals
                    .get(&slot_id.session_id)?;
                Some((
                    GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                    record.runtime_session_id,
                ))
            }
        }
    }

    pub(crate) fn run_gpui_terminal_toolbar_hotkey_action(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !matches!(
            action_id,
            "promptEditor"
                | "attachFileOrFolder"
                | "exportTranscript"
                | "sessionNote"
                | "stashPrompt"
                | "stashedPrompts"
                | "toggleAgentActions"
                | "scrollTerminalToTop"
                | "scrollTerminalToBottom"
        ) {
            return false;
        }
        if action_id == "sessionNote" {
            if let Some(session_id) = self.focused_agents_or_companion_shell_session_id() {
                let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                    "openSessionNote",
                    session_id,
                    cx,
                );
            }
            return true;
        }
        if action_id == "promptEditor" {
            if let Some((target, runtime_session_id)) =
                self.focused_gpui_engine_terminal_action_target()
            {
                self.handle_gpui_engine_prompt_editor_shortcut(target, runtime_session_id, cx);
            } else {
                #[cfg(target_os = "macos")]
                if let Some(target) = self.focused_native_terminal_prompt_editor_target() {
                    self.handle_focused_native_terminal_prompt_editor_shortcut(target, cx);
                }
            }
            return true;
        }
        let Some((target, runtime_session_id)) = self.focused_gpui_engine_terminal_action_target()
        else {
            return true;
        };
        match action_id {
            "attachFileOrFolder" => {
                if let Some(attachment_target) =
                    self.gpui_terminal_attachment_target_for_engine_target(target)
                {
                    self.request_gpui_engine_terminal_attachment_paths(
                        attachment_target,
                        runtime_session_id,
                        cx,
                    );
                }
            }
            "stashPrompt" => {
                if let GpuiEngineTerminalEventTarget::Agents(session_id) = target {
                    self.request_gpui_stash_prompt_for_active_input(session_id, cx);
                }
            }
            "stashedPrompts" => {
                if matches!(target, GpuiEngineTerminalEventTarget::Agents(_)) {
                    let _ = self.open_gpui_stashed_prompts_modal_for_focused_agents_session(cx);
                }
            }
            "exportTranscript" => {
                if let GpuiEngineTerminalEventTarget::Agents(session_id) = target
                    && self.focused_agents_or_companion_shell_session_id() == Some(session_id)
                {
                    let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                        "exportTranscript",
                        session_id,
                        cx,
                    );
                }
            }
            "toggleAgentActions" => {
                if let GpuiEngineTerminalEventTarget::Agents(session_id) = target
                    && self.focused_agents_or_companion_shell_session_id() == Some(session_id)
                {
                    self.toggle_terminal_agent_action_bar_menu(session_id, cx);
                }
            }
            "scrollTerminalToTop" | "scrollTerminalToBottom" => {
                let Some(view) = self.gpui_engine_terminal_view_for_target(target) else {
                    return true;
                };
                view.update(cx, |view, cx| {
                    if action_id == "scrollTerminalToTop" {
                        view.scroll_to_top(window, cx);
                    } else {
                        view.scroll_to_bottom(window, cx);
                    }
                });
            }
            _ => {}
        }
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn focused_native_terminal_prompt_editor_target(
        &self,
    ) -> Option<FocusedTerminalTextMountTarget> {
        let target = self.focused_terminal_text_mount_target()?;
        match target {
            FocusedTerminalTextMountTarget::Agents(slot_id)
                if self.agents_terminal_ghostty_surface_matches(slot_id) =>
            {
                Some(target)
            }
            FocusedTerminalTextMountTarget::Command(slot_id)
                if self.command_terminal_ghostty_surface_matches(slot_id) =>
            {
                Some(target)
            }
            FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id)
                if self.project_editor_companion_terminal_ghostty_surface_matches(slot_id) =>
            {
                Some(target)
            }
            _ => None,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_focused_native_terminal_prompt_editor_shortcut(
        &mut self,
        target: FocusedTerminalTextMountTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_context = match target {
            FocusedTerminalTextMountTarget::Agents(slot_id) => self
                .remote_prompt_editor_context_for_shell_session(slot_id.session_id)
                .map(|(key, connection_generation)| {
                    (slot_id.session_id, key, connection_generation)
                }),
            FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => self
                .remote_prompt_editor_context_for_shell_session(slot_id.session_id)
                .map(|(key, connection_generation)| {
                    (slot_id.session_id, key, connection_generation)
                }),
            FocusedTerminalTextMountTarget::Command(_) => None,
        };
        if let Some((shell_session_id, key, connection_generation)) = remote_context {
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.queue_remote_prompt_editor_request(
                        shell_session_id,
                        &key,
                        connection_generation,
                        RemotePromptEditorDeliveryTarget::NativeTerminal(target),
                        window,
                        cx,
                    );
                });
            })
            .detach();
            return;
        }
        let originating_session_id = match target {
            FocusedTerminalTextMountTarget::Agents(slot_id) => self
                .local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (*mapped_session_id == slot_id.session_id)
                        .then(|| format!("{}:{}", key.project_id, key.session_id))
                }),
            FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => self
                .local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (*mapped_session_id == slot_id.session_id)
                        .then(|| format!("{}:{}", key.project_id, key.session_id))
                }),
            FocusedTerminalTextMountTarget::Command(slot_id) => self
                .command_gxserver_session_mappings
                .get(&slot_id.session_id)
                .map(|key| format!("{}:{}", key.project_id, key.session_id)),
        };
        let Some(originating_session_id) = originating_session_id else {
            let _ = self.send_prompt_editor_shortcut_to_native_terminal_target(target);
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
                    let _ = this.send_prompt_editor_shortcut_to_native_terminal_target(target);
                }
            });
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn send_prompt_editor_shortcut_to_native_terminal_target(
        &mut self,
        target: FocusedTerminalTextMountTarget,
    ) -> bool {
        match target {
            FocusedTerminalTextMountTarget::Agents(slot_id) => {
                self.send_text_bytes_to_mounted_agents_terminal_surface(slot_id, b"\x07")
            }
            FocusedTerminalTextMountTarget::Command(slot_id) => {
                if !self
                    .command_pane
                    .is_current_terminal_body_mount_slot(slot_id)
                {
                    return false;
                }
                let runtime_session_id = command_terminal_runtime_session_id(slot_id);
                let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                    return false;
                };
                if surface.mount_slot_id() != slot_id
                    || surface.runtime_session_id() != runtime_session_id
                {
                    return false;
                }
                surface.send_text_bytes(b"\x07");
                true
            }
            FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => {
                if !self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id) {
                    return false;
                }
                let Some(runtime_session_id) = self
                    .agents_terminal_runtime_sessions
                    .runtime_session_id_for_shell_session(slot_id.session_id)
                else {
                    return false;
                };
                let Some(surface) = self
                    .project_editor_companion_terminal_ghostty_surfaces
                    .get_mut(&slot_id)
                else {
                    return false;
                };
                if surface.mount_slot_id() != slot_id
                    || surface.runtime_session_id() != runtime_session_id
                {
                    return false;
                }
                surface.send_text_bytes(b"\x07");
                true
            }
        }
    }

    /// The agent action bar reads chat eligibility straight off the app when it
    /// renders (apps/desktop/src/app/render/terminal_agent_action_bar.rs), so
    /// nothing has to be pushed into the terminal views any more. What still
    /// belongs on this reconcile edge is the automatic Chat handoff, which
    /// fires the moment a session first becomes chat-eligible.
    pub(crate) fn sync_gpui_engine_agents_chat_eligibility(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let chat_view_session_ids = self
            .agents_gpui_engine_terminals
            .keys()
            .copied()
            .filter(|session_id| self.agents_session_chat_eligible(*session_id))
            .collect::<HashSet<_>>();
        self.reconcile_automatic_agents_chat_modes(&chat_view_session_ids, cx);
    }

    pub(crate) fn reconcile_automatic_agents_chat_modes(
        &mut self,
        eligible_session_ids: &HashSet<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The Default Agent View is resolved per session, not once for the whole
        sweep: a per-agent override can put one agent in Chat while the global
        preference keeps every other agent in the terminal. Recording the value
        each session was considered under is also what replaces the old global
        "preference just enabled" flag — a session is swept again exactly when
        its own effective value changes, so a global flip and an override flip
        are the same edge instead of two mechanisms.
        */
        let shared_settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings_object = shared_settings_snapshot.object();
        let mut observed_sessions = HashMap::with_capacity(eligible_session_ids.len());
        let mut newly_eligible = Vec::new();
        for session_id in eligible_session_ids.iter().copied() {
            let effective_interface = gpui_effective_preferred_agent_interface_for_agent_icon(
                settings_object,
                self.agents_workspace
                    .session(session_id)
                    .and_then(|session| session.agent_icon),
            );
            let previous_interface = self
                .agents_chat_auto_switch_observed_sessions
                .get(&session_id)
                .copied();
            if effective_interface == GpuiPreferredAgentInterface::Chat
                && previous_interface != Some(effective_interface)
            {
                newly_eligible.push(session_id);
            }
            observed_sessions.insert(session_id, effective_interface);
        }
        self.agents_chat_auto_switch_observed_sessions = observed_sessions;

        let mut changed = false;
        for session_id in newly_eligible {
            if !self.agents_chat_mode_sessions.insert(session_id) {
                continue;
            }
            changed = true;
            /*
            CDXC:SessionChatDraftHandoff 2026-08-18:
            This is the switch a user never asked for: they started an agent by
            typing into a terminal, and the moment it becomes chat-eligible the
            app moves them to Chat. Anything already typed into the CLI composer
            has to come with them, or it is simply gone from view behind the
            parked terminal.
            */
            self.request_session_chat_draft_transfer(session_id, cx);
        }
        if !changed {
            return;
        }
        if let Some(focused_session_id) = self.focused_agents_or_companion_shell_session_id()
            && eligible_session_ids.contains(&focused_session_id)
        {
            self.pending_session_chat_composer_focus = Some(focused_session_id);
        }
        self.reconcile_agents_pane_surfaces(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn gpui_engine_terminal_view_for_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<Entity<terminal_element::TerminalView>> {
        match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
            GpuiEngineTerminalEventTarget::Command(session_id) => self
                .command_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
        }
    }

    /*
    CDXC:GPUITerminalGpuiEngineFocus 2026-07-04-05:45:
    GPUI-engine terminals are GPUI-owned key surfaces inside a main window
    whose AppKit first responder can be parked on a CEF child view (sidebar/
    browser interaction) or a native Ghostty host view. gpui makes its
    content NSView first responder only once at window creation and never
    reclaims it on click, so focusing the engine element's FocusHandle alone
    leaves hardware key events flowing into Chromium and the terminal dead
    to typing. Every engine-view focus must therefore return first-responder
    ownership to the exact GPUI parent view first — the same handoff the
    GPUI address bar and terminal search input already perform — and then
    focus the element handle for GPUI-side dispatch.
    */
    pub(crate) fn focus_gpui_engine_terminal_view(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        view: &Entity<terminal_element::TerminalView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:SessionChatFocusDiagnostics 2026-08-24: this is the primitive
        // that yanks AppKit first responder off any CEF surface (the chat
        // composer included) onto the GPUI root, so record every execution
        // with the responder it is about to displace.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.engineTerminalViewFocused",
            serde_json::json!({
                "target": format!("{target:?}"),
                "shellFocus": format!("{:?}", self.shell_focus),
                "firstResponderTarget": format!("{:?}", self.first_responder_target),
            }),
        );
        #[cfg(target_os = "macos")]
        self.begin_programmatic_focus();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        cef::focus_gpui_root_view(self.parent_ns_view);
        let focus_handle = view.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
        #[cfg(target_os = "macos")]
        {
            /*
            CDXC:GPUICompositedTerminalKeyboardOwnership 2026-07-30:
            App-level shell focus and GPUI's FocusHandle already identify the
            exact composited terminal synchronously. Do not wait for the
            terminal's next rendered prepaint edge to update native keyboard
            ownership: hidden/remounted views can retain their cached focused
            bit, leaving the router on an old terminal or the generic GPUI
            responder while dictation events arrive. Claim the same exact
            terminal target as part of this canonical focus handoff.
            */
            update_gpui_keyboard_router_composited_terminal_focus(
                self.parent_ns_view,
                target,
                true,
                self.first_responder_target,
            );
            self.end_programmatic_focus();
        }
    }

    /*
    CDXC:GPUITerminalGpuiEngineFocus 2026-07-04-09:10:
    Keyboard tab cycling (focusNextSession/focusPreviousSession, the
    cmd+shift+]/[ aliases, and ctrl-tab) mutates the workspace/command tab
    model and app-level shell focus but historically never touched GPUI
    keyboard focus, so cycling onto an engine-claimed slot after any CEF
    sidebar interaction left the terminal visible yet dead to typing: the
    CEF child NSView kept AppKit first responder and the engine element's
    FocusHandle was never focused. After a successful keyboard switch,
    resolve the newly focused terminal mount slot from the same shell-focus
    truth the input pipeline uses and run the exact click-path engine
    handoff (first responder back to the GPUI parent view, then the element
    handle). Native and placeholder slots resolve no engine record and are
    intentionally left on their existing AppKit surface-focus sync.
    */
    pub(crate) fn focus_gpui_engine_terminal_for_focused_mount_slot(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = self.focused_terminal_text_mount_target() else {
            return;
        };
        let event_target_and_view = match target {
            FocusedTerminalTextMountTarget::Agents(slot_id) => self
                .agents_gpui_engine_terminals
                .get(&slot_id.session_id)
                .map(|record| {
                    (
                        GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                        record.view.clone(),
                    )
                }),
            FocusedTerminalTextMountTarget::Command(slot_id) => self
                .command_gpui_engine_terminals
                .get(&slot_id.session_id)
                .map(|record| {
                    (
                        GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                        record.view.clone(),
                    )
                }),
            FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => self
                .agents_gpui_engine_terminals
                .get(&slot_id.session_id)
                .map(|record| {
                    (
                        GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                        record.view.clone(),
                    )
                }),
        };
        let Some((event_target, view)) = event_target_and_view else {
            return;
        };
        self.focus_gpui_engine_terminal_view(event_target, &view, window, cx);
    }

    /*
    CDXC:GPUITerminalGpuiEngineFocus 2026-07-04-05:45:
    Session create/attach flows request a terminal text focus handoff that
    only the native mount-slot canvas used to drain, so engine-claimed slots
    (which render no native canvas) never received creation-time keyboard
    focus and typing required understanding that a click was still routed to
    the CEF sidebar. Drain the same pending slot from render for engine
    records with the native drain's exact current-slot/focused-target
    checks, focusing the engine element instead of the shared text service.
    */
    pub(crate) fn drain_pending_gpui_engine_terminal_focus(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:SessionChatComposerFocusInvariant 2026-08-24: a slot armed
        // before its session entered chat mode must not execute as a focus
        // grab against the chat composer — the terminal it targets is no
        // longer the pane's keyboard owner. Drop it instead of waiting.
        if let Some(slot_id) = self.pending_agents_terminal_text_focus_slot
            && self.agents_chat_mode_sessions.contains(&slot_id.session_id)
        {
            self.pending_agents_terminal_text_focus_slot = None;
        }
        if let Some(slot_id) = self.pending_project_editor_companion_terminal_text_focus_slot
            && self.agents_chat_mode_sessions.contains(&slot_id.session_id)
        {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
        }
        // A pending slot whose engine record does not exist yet (spawn still
        // in flight, or a native-surface slot) must WAIT without blocking the
        // other two families: an early return here previously starved the
        // command/companion drains for as long as one stale agents pending
        // lingered (CDXC:GPUIWorkspaceSessionFocus 2026-07-11).
        if let Some(slot_id) = self.pending_agents_terminal_text_focus_slot {
            if let Some(view) = self
                .agents_gpui_engine_terminals
                .get(&slot_id.session_id)
                .map(|record| record.view.clone())
            {
                if !self
                    .agents_workspace
                    .is_current_terminal_body_mount_slot(slot_id)
                    || self.focused_terminal_text_mount_target()
                        != Some(FocusedTerminalTextMountTarget::Agents(slot_id))
                {
                    self.pending_agents_terminal_text_focus_slot = None;
                } else {
                    self.pending_agents_terminal_text_focus_slot = None;
                    self.focus_gpui_engine_terminal_view(
                        GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                        &view,
                        window,
                        cx,
                    );
                    // Remove-after-success: the pending record is the app's
                    // only pointer at the draft's durable Saved Prompts row,
                    // so a paste the terminal did not take must leave it in
                    // place for the next focus handoff to retry.
                    if let Some(handoff) = self
                        .pending_session_terminal_composer_insert
                        .get(&slot_id.session_id)
                        .cloned()
                        && view.update(cx, |view, cx| view.paste_text(&handoff.content, cx))
                    {
                        self.pending_session_terminal_composer_insert
                            .remove(&slot_id.session_id);
                        self.release_session_chat_draft_handoff_stash(handoff, cx);
                    }
                    return;
                }
            }
        }

        if let Some(slot_id) = self.pending_command_terminal_text_focus_slot {
            if let Some(view) = self
                .command_gpui_engine_terminals
                .get(&slot_id.session_id)
                .map(|record| record.view.clone())
            {
                if !self
                    .command_pane
                    .is_current_terminal_body_mount_slot(slot_id)
                    || self.focused_terminal_text_mount_target()
                        != Some(FocusedTerminalTextMountTarget::Command(slot_id))
                {
                    self.pending_command_terminal_text_focus_slot = None;
                } else {
                    self.pending_command_terminal_text_focus_slot = None;
                    self.focus_gpui_engine_terminal_view(
                        GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                        &view,
                        window,
                        cx,
                    );
                    return;
                }
            }
        }

        let Some(slot_id) = self.pending_project_editor_companion_terminal_text_focus_slot else {
            return;
        };
        let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        if !self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id) {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
            return;
        }
        if self.focused_terminal_text_mount_target()
            != Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(
                slot_id,
            ))
        {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
            return;
        }
        self.pending_project_editor_companion_terminal_text_focus_slot = None;
        self.focus_gpui_engine_terminal_view(
            GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
            &view,
            window,
            cx,
        );
        if let Some(handoff) = self
            .pending_session_terminal_composer_insert
            .get(&slot_id.session_id)
            .cloned()
            && view.update(cx, |view, cx| view.paste_text(&handoff.content, cx))
        {
            self.pending_session_terminal_composer_insert
                .remove(&slot_id.session_id);
            self.release_session_chat_draft_handoff_stash(handoff, cx);
        }
    }

    /*
    CDXC:SessionChatFocusDiagnostics 2026-08-24:
    Arming a terminal text-focus slot is what the render drain later executes
    as a first-responder grab. When it happens while the same session is in
    chat mode it steals keyboard focus from the chat composer, so each arm
    leaves a breadcrumb naming the family and whether chat mode was active.
    */
    fn log_terminal_text_focus_armed(
        &self,
        family: &str,
        session_label: String,
        in_chat_mode: bool,
    ) {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.terminalTextFocusArmed",
            serde_json::json!({
                "family": family,
                "sessionId": session_label,
                "sessionInChatMode": in_chat_mode,
                "armed": !in_chat_mode,
                "shellFocus": format!("{:?}", self.shell_focus),
                "firstResponderTarget": format!("{:?}", self.first_responder_target),
            }),
        );
    }

    /*
    CDXC:SessionChatComposerFocusInvariant 2026-08-24:
    A session whose pane shows the chat surface has no mounted terminal, so a
    terminal text-focus handoff for it can only execute later as a bare
    first-responder grab that yanks the keyboard out of the chat composer
    mid-typing. The chat-aware wrappers in workspace_events.rs already re-route
    such requests to the composer; enforce the same rule here so no direct
    caller can arm a steal for a chat-mode session. Chat → terminal toggles
    remove the session from agents_chat_mode_sessions before requesting their
    handoff, so the legitimate remount path is unaffected.
    */
    pub(crate) fn request_agents_terminal_text_focus_handoff(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) {
        let in_chat_mode = self.agents_chat_mode_sessions.contains(&slot_id.session_id);
        self.log_terminal_text_focus_armed(
            "agents",
            format!("{:?}", slot_id.session_id),
            in_chat_mode,
        );
        if in_chat_mode {
            return;
        }
        self.pending_agents_terminal_text_focus_slot = Some(slot_id);
    }

    pub(crate) fn request_command_terminal_text_focus_handoff(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) {
        self.log_terminal_text_focus_armed("command", format!("{:?}", slot_id.session_id), false);
        self.pending_command_terminal_text_focus_slot = Some(slot_id);
    }

    pub(crate) fn request_project_editor_companion_terminal_text_focus_handoff(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) {
        let in_chat_mode = self.agents_chat_mode_sessions.contains(&slot_id.session_id);
        self.log_terminal_text_focus_armed(
            "projectEditorCompanion",
            format!("{:?}", slot_id.session_id),
            in_chat_mode,
        );
        if in_chat_mode {
            return;
        }
        self.pending_project_editor_companion_terminal_text_focus_slot = Some(slot_id);
    }

    pub(crate) fn request_focused_command_terminal_text_focus_handoff(&mut self) {
        let Some((group_id, session_id)) = self.command_pane.focused_group_active_session_id()
        else {
            return;
        };
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
    }

    pub(crate) fn request_command_group_terminal_text_focus_handoff(
        &mut self,
        group_id: CommandPaneGroupId,
    ) {
        let Some(session_id) = self
            .command_pane
            .find_leaf(group_id)
            .and_then(|leaf| leaf.tab_group.active_session_id())
        else {
            return;
        };
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
    }

    pub(crate) fn clear_pending_agents_terminal_text_focus_if_focus_moved(&mut self) {
        let Some(slot_id) = self.pending_agents_terminal_text_focus_slot else {
            return;
        };
        if self.focused_terminal_text_mount_target()
            != Some(FocusedTerminalTextMountTarget::Agents(slot_id))
        {
            self.pending_agents_terminal_text_focus_slot = None;
        }
    }

    pub(crate) fn clear_pending_command_terminal_text_focus_if_focus_moved(&mut self) {
        let Some(slot_id) = self.pending_command_terminal_text_focus_slot else {
            return;
        };
        if self.focused_terminal_text_mount_target()
            != Some(FocusedTerminalTextMountTarget::Command(slot_id))
        {
            self.pending_command_terminal_text_focus_slot = None;
        }
    }

    pub(crate) fn clear_pending_project_editor_companion_terminal_text_focus_if_focus_moved(
        &mut self,
    ) {
        let Some(slot_id) = self.pending_project_editor_companion_terminal_text_focus_slot else {
            return;
        };
        if self.focused_terminal_text_mount_target()
            != Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(
                slot_id,
            ))
        {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drain_pending_agents_terminal_text_focus_handoff(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.pending_agents_terminal_text_focus_slot != Some(slot_id) {
            return;
        }
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            self.pending_agents_terminal_text_focus_slot = None;
            return;
        }
        if self.focused_terminal_text_mount_target()
            != Some(FocusedTerminalTextMountTarget::Agents(slot_id))
        {
            self.pending_agents_terminal_text_focus_slot = None;
            return;
        }
        if !self.agents_terminal_ghostty_surface_matches(slot_id) {
            return;
        }
        self.pending_agents_terminal_text_focus_slot = None;
        self.sync_agents_terminal_ghostty_surface_focus_with_appkit_handoff(true);
        /*
        CDXC:SessionChatDraftHandoff 2026-08-24:
        A multi-line draft must reach the agent's composer as ONE paste, not as
        N submitted lines, and it already does: these bytes go to
        `ghostty_surface_text`, whose callback is Ghostty's own paste completion
        (`Surface.completeClipboardPaste`). That encoder reads the live terminal
        state and adds the `ESC[200~`/`ESC[201~` fenceposts itself when
        bracketed-paste mode is on (and rewrites newlines to CR when it is not),
        exactly like the gpui-engine pipeline's `send_paste`. Do NOT pre-frame
        the draft here: the same encoder replaces raw `ESC` bytes with spaces
        before wrapping, so a hand-written fencepost would arrive as literal
        "[200~" text inside the agent's prompt.
        */
        if let Some(handoff) = self
            .pending_session_terminal_composer_insert
            .get(&slot_id.session_id)
            .cloned()
            && self.send_text_bytes_to_mounted_agents_terminal_surface(
                slot_id,
                handoff.content.as_bytes(),
            )
        {
            self.pending_session_terminal_composer_insert
                .remove(&slot_id.session_id);
            self.release_session_chat_draft_handoff_stash(handoff, cx);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn drain_pending_agents_terminal_text_focus_handoff(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
        if self.pending_agents_terminal_text_focus_slot == Some(slot_id) {
            self.pending_agents_terminal_text_focus_slot = None;
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drain_pending_command_terminal_text_focus_handoff(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
        if self.pending_command_terminal_text_focus_slot != Some(slot_id) {
            return;
        }
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            self.pending_command_terminal_text_focus_slot = None;
            return;
        }
        if self.focused_terminal_text_mount_target()
            != Some(FocusedTerminalTextMountTarget::Command(slot_id))
        {
            self.pending_command_terminal_text_focus_slot = None;
            return;
        }
        if !self.command_terminal_ghostty_surface_matches(slot_id) {
            return;
        }
        self.pending_command_terminal_text_focus_slot = None;
        self.sync_command_terminal_ghostty_surface_focus_with_appkit_handoff(true);
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn drain_pending_command_terminal_text_focus_handoff(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
        if self.pending_command_terminal_text_focus_slot == Some(slot_id) {
            self.pending_command_terminal_text_focus_slot = None;
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drain_pending_project_editor_companion_terminal_text_focus_handoff(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.pending_project_editor_companion_terminal_text_focus_slot != Some(slot_id) {
            return;
        }
        if !self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id) {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
            return;
        }
        if self.focused_terminal_text_mount_target()
            != Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(
                slot_id,
            ))
        {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
            return;
        }
        if !self.project_editor_companion_terminal_ghostty_surface_matches(slot_id) {
            return;
        }
        self.pending_project_editor_companion_terminal_text_focus_slot = None;
        self.sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(true);
        if let Some(handoff) = self
            .pending_session_terminal_composer_insert
            .get(&slot_id.session_id)
            .cloned()
            && self.send_text_bytes_to_focused_project_editor_companion_terminal_surface(
                handoff.content.as_bytes(),
            )
        {
            self.pending_session_terminal_composer_insert
                .remove(&slot_id.session_id);
            self.release_session_chat_draft_handoff_stash(handoff, cx);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn drain_pending_project_editor_companion_terminal_text_focus_handoff(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
        if self.pending_project_editor_companion_terminal_text_focus_slot == Some(slot_id) {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
        }
    }

    pub(crate) fn focused_terminal_text_mount_target(
        &self,
    ) -> Option<FocusedTerminalTextMountTarget> {
        match focused_terminal_text_target(self.active_mode, self.shell_focus)? {
            FocusedTerminalTextTarget::Agents => focused_agents_terminal_surface_mount_slot(
                self.active_mode,
                self.shell_focus,
                &self.agents_workspace,
            )
            .map(FocusedTerminalTextMountTarget::Agents),
            FocusedTerminalTextTarget::Command => {
                focused_command_terminal_surface_mount_slot(self.shell_focus, &self.command_pane)
                    .map(FocusedTerminalTextMountTarget::Command)
            }
            FocusedTerminalTextTarget::ProjectEditorCompanion => {
                focused_project_editor_companion_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    self.project_editor_companion_focused_terminal_session_id(),
                )
                .map(FocusedTerminalTextMountTarget::ProjectEditorCompanion)
            }
        }
    }

    pub(crate) fn terminal_text_input_should_track_agents_slot(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.focused_terminal_text_mount_target()
            == Some(FocusedTerminalTextMountTarget::Agents(slot_id))
    }

    pub(crate) fn register_agents_terminal_text_input_handler(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        _bounds: Bounds<Pixels>,
        _view: Entity<Self>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.drain_pending_agents_terminal_text_focus_handoff(slot_id, window, cx);
    }

    pub(crate) fn register_command_terminal_text_input_handler(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        _bounds: Bounds<Pixels>,
        _view: Entity<Self>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.drain_pending_command_terminal_text_focus_handoff(slot_id, window, cx);
    }

    pub(crate) fn register_project_editor_companion_terminal_text_input_handler(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        _bounds: Bounds<Pixels>,
        _view: Entity<Self>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.drain_pending_project_editor_companion_terminal_text_focus_handoff(
            slot_id, window, cx,
        );
    }

    pub(crate) fn terminal_text_service_accepts_text_input(&self, window: &Window) -> bool {
        self.terminal_text_focus_handle.is_focused(window)
            && self.exact_focused_terminal_text_surface_target().is_some()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn exact_focused_terminal_text_surface_target(
        &self,
    ) -> Option<FocusedTerminalTextMountTarget> {
        let target = self.focused_terminal_text_mount_target()?;
        match target {
            FocusedTerminalTextMountTarget::Agents(slot_id) => self
                .agents_terminal_ghostty_surface_matches(slot_id)
                .then_some(target),
            FocusedTerminalTextMountTarget::Command(slot_id) => self
                .command_terminal_ghostty_surface_matches(slot_id)
                .then_some(target),
            FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => self
                .project_editor_companion_terminal_ghostty_surface_matches(slot_id)
                .then_some(target),
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn exact_focused_terminal_text_surface_target(
        &self,
    ) -> Option<FocusedTerminalTextMountTarget> {
        None
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn agents_terminal_ghostty_surface_matches(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        self.agents_terminal_ghostty_surfaces
            .get(&slot_id)
            .is_some_and(|surface| {
                surface.mount_slot_id() == slot_id
                    && surface.runtime_session_id() == runtime_session_id
            })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn command_terminal_ghostty_surface_matches(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        self.command_pane
            .is_current_terminal_body_mount_slot(slot_id)
            && self
                .command_terminal_ghostty_surfaces
                .get(&slot_id)
                .is_some_and(|surface| {
                    surface.mount_slot_id() == slot_id
                        && surface.runtime_session_id()
                            == command_terminal_runtime_session_id(slot_id)
                })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn project_editor_companion_terminal_ghostty_surface_matches(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> bool {
        if !self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id) {
            return false;
        }
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        self.project_editor_companion_terminal_ghostty_surfaces
            .get(&slot_id)
            .is_some_and(|surface| {
                surface.mount_slot_id() == slot_id
                    && surface.runtime_session_id() == runtime_session_id
            })
    }

    pub(crate) fn set_preedit_on_focused_terminal_surface(&mut self, bytes: &[u8]) -> bool {
        let Some(target) = self.exact_focused_terminal_text_surface_target() else {
            return false;
        };
        self.set_preedit_on_terminal_text_target(target, bytes)
    }

    pub(crate) fn set_preedit_on_terminal_text_target(
        &mut self,
        target: FocusedTerminalTextMountTarget,
        bytes: &[u8],
    ) -> bool {
        #[cfg(target_os = "macos")]
        {
            match target {
                FocusedTerminalTextMountTarget::Agents(slot_id) => {
                    self.set_preedit_bytes_on_agents_terminal_surface(slot_id, bytes)
                }
                FocusedTerminalTextMountTarget::Command(slot_id) => {
                    self.set_preedit_bytes_on_command_terminal_surface(slot_id, bytes)
                }
                FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => self
                    .set_preedit_bytes_on_project_editor_companion_terminal_surface(slot_id, bytes),
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (target, bytes);
            false
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_preedit_bytes_on_agents_terminal_surface(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        bytes: &[u8],
    ) -> bool {
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
            return false;
        };
        if surface.mount_slot_id() != slot_id || surface.runtime_session_id() != runtime_session_id
        {
            return false;
        }

        surface.set_preedit_bytes(bytes);
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_preedit_bytes_on_command_terminal_surface(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        bytes: &[u8],
    ) -> bool {
        let runtime_session_id = command_terminal_runtime_session_id(slot_id);
        let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
            return false;
        };
        if surface.mount_slot_id() != slot_id || surface.runtime_session_id() != runtime_session_id
        {
            return false;
        }

        surface.set_preedit_bytes(bytes);
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_preedit_bytes_on_project_editor_companion_terminal_surface(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        bytes: &[u8],
    ) -> bool {
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        let Some(surface) = self
            .project_editor_companion_terminal_ghostty_surfaces
            .get_mut(&slot_id)
        else {
            return false;
        };
        if surface.mount_slot_id() != slot_id || surface.runtime_session_id() != runtime_session_id
        {
            return false;
        }

        surface.set_preedit_bytes(bytes);
        true
    }

    pub(crate) fn clear_focused_terminal_preedit(&mut self) {
        let _ = self.set_preedit_on_focused_terminal_surface(b"");
        self.terminal_text_marked_range = None;
    }

    pub(crate) fn bounds_for_focused_terminal_ime_point(
        &self,
        element_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        /*
        CDXC:GPUITerminalTextInput 2026-06-23-10:45:
        IME candidate-window bounds may use only the current exact Ghostty surface `ime_point` plus the mounted terminal body bounds supplied by GPUI's paint-time input handler. If the focused surface is missing or stale, return None instead of inventing title/path/content/cursor fallbacks.
        */
        #[cfg(target_os = "macos")]
        {
            match self.exact_focused_terminal_text_surface_target()? {
                FocusedTerminalTextMountTarget::Agents(slot_id) => {
                    if !self.agents_terminal_ghostty_surface_matches(slot_id) {
                        return None;
                    }
                    let ime_point = self
                        .agents_terminal_ghostty_surfaces
                        .get(&slot_id)?
                        .ime_point();
                    terminal_ime_bounds_from_ghostty_point(element_bounds, ime_point)
                }
                FocusedTerminalTextMountTarget::Command(slot_id) => {
                    if !self.command_terminal_ghostty_surface_matches(slot_id) {
                        return None;
                    }
                    let ime_point = self
                        .command_terminal_ghostty_surfaces
                        .get(&slot_id)?
                        .ime_point();
                    terminal_ime_bounds_from_ghostty_point(element_bounds, ime_point)
                }
                FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id) => {
                    if !self.project_editor_companion_terminal_ghostty_surface_matches(slot_id) {
                        return None;
                    }
                    let ime_point = self
                        .project_editor_companion_terminal_ghostty_surfaces
                        .get(&slot_id)?
                        .ime_point();
                    terminal_ime_bounds_from_ghostty_point(element_bounds, ime_point)
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = element_bounds;
            None
        }
    }

    pub(crate) fn send_text_to_focused_terminal_surface(
        &mut self,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if text.is_empty() {
            return false;
        }

        if let Some(view) = self.focused_gpui_engine_terminal_view() {
            view.update(cx, |view, cx| view.send_text_input(text, cx));
            return true;
        }

        #[cfg(target_os = "macos")]
        {
            match focused_terminal_text_target(self.active_mode, self.shell_focus) {
                Some(FocusedTerminalTextTarget::Command) => {
                    self.send_text_bytes_to_focused_command_terminal_surface(text.as_bytes())
                }
                Some(FocusedTerminalTextTarget::Agents) => {
                    self.send_text_bytes_to_focused_agents_terminal_surface(text.as_bytes())
                }
                Some(FocusedTerminalTextTarget::ProjectEditorCompanion) => self
                    .send_text_bytes_to_focused_project_editor_companion_terminal_surface(
                        text.as_bytes(),
                    ),
                None => false,
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = text;
            false
        }
    }

    pub(crate) fn send_tab_key_to_gpui_engine_terminal(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        action: ghostty_vt::VtKeyAction,
        shift: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(view) = self.gpui_engine_terminal_view_for_target(target) else {
            return false;
        };
        view.update(cx, |view, cx| view.send_tab_key_action(action, shift, cx))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_window_scoped_ghostex_hotkey(
        &mut self,
        action_id: &str,
        owner: GpuiKeyboardOwner,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIKeyboardRouter 2026-07-24:
        Native pre-dispatch captures the window and exact keyboard owner before
        queuing an app action. Never bounce the selector through a process-global
        callback or another window's current focus; the target app entity and
        Window supplied by update_in are the registration that accepted it.
        */
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.keyboardRouter.hotkeyDispatched",
            serde_json::json!({
                "actionId": action_id,
                "owner": format!("{owner:?}"),
            }),
        );
        self.handle_gpui_app_modal_sidebar_command(
            serde_json::json!({
                "message": {
                    "actionId": action_id,
                    "type": "runGhostexHotkeyAction",
                },
            }),
            window,
            cx,
        );
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_window_scoped_application_keyboard_command(
        &mut self,
        command: GpuiApplicationKeyboardCommand,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            GpuiApplicationKeyboardCommand::Hide => cx.hide(),
            GpuiApplicationKeyboardCommand::HideOthers => cx.hide_other_apps(),
            GpuiApplicationKeyboardCommand::MinimizeWindow => window.minimize_window(),
            GpuiApplicationKeyboardCommand::Quit => {
                GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
                cx.quit();
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn send_text_bytes_to_focused_agents_terminal_surface(
        &mut self,
        bytes: &[u8],
    ) -> bool {
        let Some(slot_id) = focused_agents_terminal_surface_mount_slot(
            self.active_mode,
            self.shell_focus,
            &self.agents_workspace,
        ) else {
            return false;
        };
        self.send_text_bytes_to_mounted_agents_terminal_surface(slot_id, bytes)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn send_text_bytes_to_mounted_agents_terminal_surface(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        bytes: &[u8],
    ) -> bool {
        if bytes.is_empty()
            || !self
                .agents_workspace
                .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
            return false;
        };
        if surface.mount_slot_id() != slot_id || surface.runtime_session_id() != runtime_session_id
        {
            return false;
        }

        surface.send_text_bytes(bytes);
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn send_text_bytes_to_focused_command_terminal_surface(
        &mut self,
        bytes: &[u8],
    ) -> bool {
        let Some(slot_id) =
            focused_command_terminal_surface_mount_slot(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        let runtime_session_id = command_terminal_runtime_session_id(slot_id);
        let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
            return false;
        };
        if surface.mount_slot_id() != slot_id || surface.runtime_session_id() != runtime_session_id
        {
            return false;
        }

        surface.send_text_bytes(bytes);
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn send_text_bytes_to_focused_project_editor_companion_terminal_surface(
        &mut self,
        bytes: &[u8],
    ) -> bool {
        let Some(slot_id) = focused_project_editor_companion_terminal_surface_mount_slot(
            self.active_mode,
            self.shell_focus,
            self.project_editor_companion_focused_terminal_session_id(),
        ) else {
            return false;
        };
        if bytes.is_empty()
            || !self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        let Some(surface) = self
            .project_editor_companion_terminal_ghostty_surfaces
            .get_mut(&slot_id)
        else {
            return false;
        };
        if surface.mount_slot_id() != slot_id || surface.runtime_session_id() != runtime_session_id
        {
            return false;
        }

        surface.send_text_bytes(bytes);
        true
    }

    pub(crate) fn send_return_key_to_mounted_command_terminal_surface(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUICommandDelayedSend 2026-06-25-15:11:
        Delayed Send must submit the staged prompt through Ghostty's key path, matching native `sendTerminalEnter`, rather than writing carriage-return text. Use the exact current mounted command slot and the macOS Return keycode/text tuple; if the surface is missing or stale, no other terminal receives the key.
        */
        if self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
            && let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id)
        {
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_return_key(cx));
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            if !self
                .command_pane
                .is_current_terminal_body_mount_slot(slot_id)
            {
                return false;
            }
            let runtime_session_id = command_terminal_runtime_session_id(slot_id);
            let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
            {
                return false;
            }
            let Ok(return_text) = std::ffi::CString::new(COMMAND_PANE_DELAYED_SEND_RETURN_TEXT)
            else {
                return false;
            };
            surface.send_key(ghostty_kit::ffi::ghostty_input_key_s {
                action: COMMAND_PANE_GHOSTTY_KEY_ACTION_PRESS,
                mods: 0,
                consumed_mods: 0,
                keycode: COMMAND_PANE_DELAYED_SEND_RETURN_KEYCODE,
                text: return_text.as_ptr(),
                unshifted_codepoint: COMMAND_PANE_DELAYED_SEND_RETURN_UNSHIFTED_CODEPOINT,
                composing: false,
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = slot_id;
            false
        }
    }

    pub(crate) fn send_return_key_to_mounted_agents_terminal_surface(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
        Mapped Agents rename submission must use Ghostty's key path for the exact mounted Agents slot, matching native Return delivery. If the slot is stale, hidden, sleeping, missing a runtime id, or owned by a different surface, no other terminal receives a newline or fallback key.
        */
        if self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
            && let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
        {
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_return_key(cx));
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            if !self
                .agents_workspace
                .is_current_terminal_body_mount_slot(slot_id)
            {
                return false;
            }
            let Some(runtime_session_id) = self
                .agents_terminal_runtime_sessions
                .runtime_session_id_for_shell_session(slot_id.session_id)
            else {
                return false;
            };
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
            {
                return false;
            }
            let Ok(return_text) = std::ffi::CString::new(GPUI_TERMINAL_RETURN_TEXT) else {
                return false;
            };
            surface.send_key(ghostty_kit::ffi::ghostty_input_key_s {
                action: COMMAND_PANE_GHOSTTY_KEY_ACTION_PRESS,
                mods: 0,
                consumed_mods: 0,
                keycode: GPUI_TERMINAL_RETURN_KEYCODE,
                text: return_text.as_ptr(),
                unshifted_codepoint: GPUI_TERMINAL_RETURN_UNSHIFTED_CODEPOINT,
                composing: false,
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = slot_id;
            false
        }
    }

    pub(crate) fn send_return_key_to_mounted_project_editor_companion_terminal_surface(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id) {
            return false;
        }
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        if let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
            && record.runtime_session_id == runtime_session_id
        {
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_return_key(cx));
            return true;
        }

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get_mut(&slot_id)
            else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
            {
                return false;
            }
            let Ok(return_text) = std::ffi::CString::new(GPUI_TERMINAL_RETURN_TEXT) else {
                return false;
            };
            surface.send_key(ghostty_kit::ffi::ghostty_input_key_s {
                action: COMMAND_PANE_GHOSTTY_KEY_ACTION_PRESS,
                mods: 0,
                consumed_mods: 0,
                keycode: GPUI_TERMINAL_RETURN_KEYCODE,
                text: return_text.as_ptr(),
                unshifted_codepoint: GPUI_TERMINAL_RETURN_UNSHIFTED_CODEPOINT,
                composing: false,
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn send_return_key_to_parked_agents_terminal_surface(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> bool {
        let Some(owner) = self
            .agents_terminal_parked_runtime_owners
            .get_mut(&runtime_session_id)
        else {
            return false;
        };
        if !owner.matches_identity(
            runtime_session_id,
            owner.shell_session_id,
            owner.mount_slot_id,
        ) {
            return false;
        }
        let Ok(return_text) = std::ffi::CString::new(GPUI_TERMINAL_RETURN_TEXT) else {
            return false;
        };
        owner
            .surface_owner
            .send_key(ghostty_kit::ffi::ghostty_input_key_s {
                action: COMMAND_PANE_GHOSTTY_KEY_ACTION_PRESS,
                mods: 0,
                consumed_mods: 0,
                keycode: GPUI_TERMINAL_RETURN_KEYCODE,
                text: return_text.as_ptr(),
                unshifted_codepoint: GPUI_TERMINAL_RETURN_UNSHIFTED_CODEPOINT,
                composing: false,
            })
    }

    pub(crate) fn send_gpui_command_action_script_to_mounted_terminal(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        execution_text: &str,
        status_file_path: &Path,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        #[cfg(target_os = "windows")]
        {
            let input = if matches!(
                windows_terminal_backend::resolve_current(),
                Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
            ) {
                execution_text.to_string()
            } else {
                gpui_command_action_mounted_terminal_script_text(execution_text, status_file_path)
            };
            let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id) else {
                return false;
            };
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_text_input(&input, cx));
            return self.send_return_key_to_mounted_command_terminal_surface(slot_id, cx);
        }
        #[cfg(not(target_os = "windows"))]
        let Some(source_command) = gpui_command_action_staged_mounted_script_source_command(
            execution_text,
            status_file_path,
        ) else {
            return false;
        };
        #[cfg(not(target_os = "windows"))]
        if let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id) {
            let view = record.view.clone();
            view.update(cx, |view, cx| view.send_text_input(&source_command, cx));
            return self.send_return_key_to_mounted_command_terminal_surface(slot_id, cx);
        }

        #[cfg(target_os = "macos")]
        {
            let runtime_session_id = command_terminal_runtime_session_id(slot_id);
            {
                let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                    return false;
                };
                if surface.mount_slot_id() != slot_id
                    || surface.runtime_session_id() != runtime_session_id
                {
                    return false;
                }
                surface.send_text_bytes(source_command.as_bytes());
            }
            /*
            CDXC:GPUICommandPaneActions 2026-06-27-07:54:
            Mounted reused default Actions mirror native `writeTerminalScript`: stage the private wrapper in a temp script for the exact current command surface, send only the short source command as terminal text, and submit it through the real Return key path so reruns execute immediately without relying on carriage-return text or a deferred launch payload.
            */
            self.send_return_key_to_mounted_command_terminal_surface(slot_id, cx)
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    pub(crate) fn gpui_command_action_mounted_reuse_surface_available(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        /*
        CDXC:GPUICommandPaneActions 2026-06-27-07:54:
        Default Action reuse may bypass launch payloads only when the selected reused tab already owns the exact current mounted command Ghostty surface. Missing, stale, sleeping, or unmounted reused tabs must use the exact-slot launch payload path instead of borrowing another terminal surface.
        */
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        if self
            .command_gpui_engine_terminals
            .contains_key(&slot_id.session_id)
        {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            let runtime_session_id = command_terminal_runtime_session_id(slot_id);
            self.command_terminal_ghostty_surfaces
                .get(&slot_id)
                .is_some_and(|surface| {
                    surface.mount_slot_id() == slot_id
                        && surface.runtime_session_id() == runtime_session_id
                })
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn agents_terminal_close_confirm_slot_is_current(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        if self.agents_gpui_engine_close_confirms.contains(&slot_id)
            && self
                .agents_gpui_engine_terminals
                .contains_key(&slot_id.session_id)
            && self
                .agents_workspace
                .is_current_terminal_body_mount_slot(slot_id)
            && self
                .agents_workspace
                .can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            return true;
        }
        self.agents_terminal_close_confirms
            .exact_current_pending_slot(
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &self.agents_terminal_ghostty_surfaces,
                slot_id,
            )
            .is_some()
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn command_terminal_close_confirm_slot_is_current(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        if self.command_gpui_engine_close_confirms.contains(&slot_id)
            && self
                .command_gpui_engine_terminals
                .contains_key(&slot_id.session_id)
            && self
                .command_pane
                .is_current_terminal_body_mount_slot(slot_id)
        {
            return true;
        }
        self.command_terminal_close_confirms
            .exact_current_pending_slot(
                &self.command_pane,
                &self.command_terminal_ghostty_surfaces,
                slot_id,
            )
            .is_some()
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn terminal_close_confirm_dialog_key_is_current(
        &self,
        key: TerminalCloseConfirmDialogKey,
    ) -> bool {
        match key {
            TerminalCloseConfirmDialogKey::Agents(slot_id) => {
                self.agents_terminal_close_confirm_slot_is_current(slot_id)
            }
            TerminalCloseConfirmDialogKey::Command(slot_id) => {
                self.command_terminal_close_confirm_slot_is_current(slot_id)
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn next_terminal_close_confirm_dialog_key(
        &self,
    ) -> Option<TerminalCloseConfirmDialogKey> {
        if let Some(slot_id) =
            focused_command_terminal_surface_mount_slot(self.shell_focus, &self.command_pane)
                .filter(|slot_id| self.command_terminal_close_confirm_slot_is_current(*slot_id))
        {
            return Some(TerminalCloseConfirmDialogKey::Command(slot_id));
        }
        if let Some(slot_id) = focused_agents_terminal_surface_mount_slot(
            self.active_mode,
            self.shell_focus,
            &self.agents_workspace,
        )
        .filter(|slot_id| self.agents_terminal_close_confirm_slot_is_current(*slot_id))
        {
            return Some(TerminalCloseConfirmDialogKey::Agents(slot_id));
        }
        if let Some(slot_id) = self
            .command_pane
            .rendered_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| self.command_terminal_close_confirm_slot_is_current(*slot_id))
        {
            return Some(TerminalCloseConfirmDialogKey::Command(slot_id));
        }
        self.agents_workspace
            .rendered_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| self.agents_terminal_close_confirm_slot_is_current(*slot_id))
            .map(TerminalCloseConfirmDialogKey::Agents)
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn sync_terminal_close_confirm_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(key) = self.terminal_close_confirm_dialog_key {
            if self.terminal_close_confirm_dialog_key_is_current(key) {
                return;
            }
            self.terminal_close_confirm_dialog_key = None;
            if window.has_active_dialog(cx) {
                window.close_dialog(cx);
            }
        }

        if window.has_active_dialog(cx) {
            return;
        }
        let Some(key) = self.next_terminal_close_confirm_dialog_key() else {
            return;
        };
        self.terminal_close_confirm_dialog_key = Some(key);
        cx.defer_in(window, move |this, window, cx| {
            if this.terminal_close_confirm_dialog_key != Some(key)
                || !this.terminal_close_confirm_dialog_key_is_current(key)
            {
                this.terminal_close_confirm_dialog_key = None;
                return;
            }
            if window.has_active_dialog(cx) {
                this.terminal_close_confirm_dialog_key = None;
                cx.notify();
                return;
            }
            this.open_terminal_close_confirm_dialog(key, window, cx);
        });
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn open_terminal_close_confirm_dialog(
        &mut self,
        key: TerminalCloseConfirmDialogKey,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let signature = terminal_close_confirm_surface_signature(key.family());
        let entity = cx.entity().clone();
        let ok_entity = entity.clone();
        let cancel_entity = entity;

        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title(signature.title)
                .description(signature.message)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text(signature.keep_open_label)
                        .ok_text(signature.confirm_action_label)
                        .ok_variant(ButtonVariant::Default)
                        .on_ok({
                            let ok_entity = ok_entity.clone();
                            move |_, _, cx| {
                                ok_entity.update(cx, |this, cx| {
                                    if !this.terminal_close_confirm_dialog_key_is_current(key) {
                                        this.terminal_close_confirm_dialog_key = None;
                                        return true;
                                    }
                                    let confirmed = match key {
                                        TerminalCloseConfirmDialogKey::Agents(slot_id) => {
                                            this.confirm_pending_agents_terminal_close(slot_id, cx)
                                        }
                                        TerminalCloseConfirmDialogKey::Command(slot_id) => {
                                            this.confirm_pending_command_terminal_close(slot_id, cx)
                                        }
                                    };
                                    if confirmed
                                        && !this.terminal_close_confirm_dialog_key_is_current(key)
                                    {
                                        this.terminal_close_confirm_dialog_key = None;
                                    }
                                    if confirmed {
                                        cx.notify();
                                    }
                                    confirmed
                                })
                            }
                        })
                        .on_cancel({
                            let cancel_entity = cancel_entity.clone();
                            move |_, _, cx| {
                                cancel_entity.update(cx, |this, cx| {
                                    let canceled = match key {
                                        TerminalCloseConfirmDialogKey::Agents(slot_id) => {
                                            this.cancel_pending_agents_terminal_close(slot_id)
                                        }
                                        TerminalCloseConfirmDialogKey::Command(slot_id) => {
                                            this.cancel_pending_command_terminal_close(slot_id)
                                        }
                                    };
                                    this.terminal_close_confirm_dialog_key = None;
                                    if canceled {
                                        cx.notify();
                                    }
                                    true
                                })
                            }
                        }),
                )
                .overlay_closable(false)
                .close_button(false)
                .keyboard(true)
        });
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn confirm_pending_agents_terminal_close(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
        Confirming a pending Agents close validates the exact current pending slot, process-local runtime identity, mounted Ghostty owner, and `needs_confirm_quit` boolean before closing the shell tab through the existing workspace model path. This uses the real GhosttyKit close-confirm query, not a synthetic runtime callback or broad fallback close.

        CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
        Direct user confirmation has the same shell side effects as the callback close path: reconcile process-local runtime ids after model removal, keep Agents focus on the surviving focused pane, scroll its active tab, and persist layout state without touching command/startup maps.

        CDXC:GPUIWorkspaceLifecycle 2026-06-26-07:25:
        If the confirmed Agents tab is mapped to a gxserver workspace session, confirmation commits the local shell close immediately; the fixed sidebar lifecycle bridge then receives best-effort provider cleanup without gating tab removal.
        */
        if self
            .local_workspace_key_for_shell_session(slot_id.session_id)
            .is_some()
        {
            let Some(pending) = self
                .agents_terminal_close_confirms
                .pending_by_slot
                .get(&slot_id)
                .copied()
            else {
                return false;
            };
            let Some(current) = pending_agents_terminal_close_confirm_for_slot(
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &self.agents_terminal_ghostty_surfaces,
                slot_id,
            ) else {
                self.agents_terminal_close_confirms
                    .pending_by_slot
                    .remove(&slot_id);
                return false;
            };
            if pending != current {
                self.agents_terminal_close_confirms
                    .pending_by_slot
                    .remove(&slot_id);
                return false;
            }
            let replacement_key = self
                .agents_workspace
                .selected_session_after_direct_tab_close(slot_id.pane_id, slot_id.session_id)
                .and_then(|replacement_session_id| {
                    self.local_workspace_key_for_shell_session(replacement_session_id)
                });
            let skip_replacement_fallback = replacement_key.is_none();
            let requested = self.request_local_workspace_terminal_lifecycle(
                slot_id.pane_id,
                slot_id.session_id,
                GpuiLocalWorkspaceLifecycleAction::Close,
                GpuiLocalWorkspaceLifecycleMutationKind::DirectClose,
                replacement_key,
                skip_replacement_fallback,
                Some(slot_id),
                cx,
            );
            return requested;
        }
        let confirmed = if self.agents_gpui_engine_close_confirms.remove(&slot_id) {
            self.agents_gpui_engine_terminals
                .remove(&slot_id.session_id);
            let closed = self
                .agents_workspace
                .close_tab(slot_id.pane_id, slot_id.session_id);
            if closed {
                self.forget_local_workspace_mappings_for_shell_session(slot_id.session_id, cx);
            }
            closed
        } else {
            self.agents_terminal_close_confirms.confirm(
                &mut self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &self.agents_terminal_ghostty_surfaces,
                slot_id,
            )
        };
        if confirmed {
            self.agents_terminal_runtime_sessions
                .reconcile_with_workspace(&self.agents_workspace);
            self.set_shell_focus(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
        }
        confirmed
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn cancel_pending_agents_terminal_close(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        if self.agents_gpui_engine_close_confirms.remove(&slot_id) {
            return true;
        }
        self.agents_terminal_close_confirms.cancel(
            &self.agents_workspace,
            &self.agents_terminal_runtime_sessions,
            &mut self.agents_terminal_ghostty_surfaces,
            slot_id,
        )
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn confirm_pending_command_terminal_close(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
        Confirming a pending command close stays command-local and validates the exact current slot, transient runtime identity, mounted Ghostty owner, and `needs_confirm_quit` boolean before removing the command session through `CommandPaneModel::close_session`. It must not route through Agents/startup state or synthesize a confirmed runtime callback.

        CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
        Command confirmation mirrors the confirmed-callback shell side effects: if sessions remain, keep focus on the command pane and scroll the active command tab; if the pane empties, restore the previous non-command focus before persisting layout state.

        CDXC:GPUITerminalCloseConfirm 2026-06-25-21:12:
        Direct user confirmation must also share the command close cleanup side effects from callback and tab-close paths. Prune command-owned Delayed Send and Close After Done timers, refresh the sidebar command projection, and repaint immediately after the confirmed command session leaves the model.

        CDXC:GPUICommandPaneResize 2026-06-27-03:21:
        Direct confirmation can remove the final mounted command tab from the close-confirm surface. Clear command resize hover chrome only after that leaves the command pane empty, matching runtime confirmed-close and process-exit cleanup.
        */
        let confirmed = if self.command_gpui_engine_close_confirms.remove(&slot_id) {
            self.command_gpui_engine_terminals
                .remove(&slot_id.session_id);
            self.command_pane
                .close_session(slot_id.group_id, slot_id.session_id)
        } else {
            self.command_terminal_close_confirms.confirm(
                &mut self.command_pane,
                &self.command_terminal_ghostty_surfaces,
                slot_id,
            )
        };
        if confirmed {
            self.forget_command_gxserver_session_for_closed_tab(slot_id.session_id, cx);
            self.prune_gpui_command_delayed_send_timers_for_command_model();
            self.prune_gpui_command_close_after_done_timers_for_command_model();
            self.clear_command_resize_hover_state_if_command_pane_hidden();
            if self.command_pane.has_sessions() {
                self.set_shell_focus(ShellFocusTarget::CommandPane);
                self.scroll_focused_command_active_tab();
            } else {
                self.restore_previous_non_command_focus_or_default();
            }
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
        }
        confirmed
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    pub(crate) fn cancel_pending_command_terminal_close(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        if self.command_gpui_engine_close_confirms.remove(&slot_id) {
            return true;
        }
        self.command_terminal_close_confirms.cancel(
            &self.command_pane,
            &mut self.command_terminal_ghostty_surfaces,
            slot_id,
        )
    }
}
