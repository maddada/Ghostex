//! Pasting text and images into the focused terminal, the paste confirmation dialog, terminal toolbar hotkeys and prompt editor shortcuts.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::ClipboardItem;
use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn paste_into_focused_terminal_from_clipboard(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Clipboard 2026-06-23-09:59:
        Cmd+V terminal paste is scoped to the shell focus model instead of the body mouse handlers: only the currently focused mounted Agents or command Ghostty surface can receive clipboard bytes. Clipboard contents stay ephemeral, explicit-string-only, and are never logged, persisted, or converted from file paths.

        CDXC:Clipboard 2026-06-27-10:28:
        Direct GPUI terminal paste uses the same runtime-only previewable-image setting as macOS before targeting the focused mounted Ghostty surface. Disabled keeps explicit-string-only behavior; enabled converts only validated image file references or raw image bytes into Markdown before any terminal insertion.
        */
        if self.focused_terminal_is_still_mounting() {
            self.report_terminal_paste_failure(TerminalPasteFailure::TerminalStarting, cx);
            return true;
        }
        let Some(target) = self.terminal_paste_target() else {
            return false;
        };
        let read = gpui_read_clipboard(cx, "terminalPaste");
        cx.spawn(async move |this, cx| {
            let item = read.await;
            this.update(cx, |this, cx| {
                if this.terminal_paste_target() != Some(target) {
                    this.report_terminal_paste_failure(TerminalPasteFailure::FocusMoved, cx);
                    return;
                }
                let Some(item) = item else {
                    this.report_terminal_paste_failure(
                        TerminalPasteFailure::ClipboardUnreadable,
                        cx,
                    );
                    return;
                };
                this.paste_clipboard_item_into_focused_terminal(&item, cx);
            })
            .ok();
        })
        .detach();
        true
    }

    /// The focused pane holds a terminal that is still being created, so there is nothing to paste
    /// into yet. Cmd+V used to fall through to no handler there and do nothing.
    fn focused_terminal_is_still_mounting(&self) -> bool {
        matches!(
            self.shell_keyboard_owner(),
            ShellKeyboardOwner::TerminalMounting
        )
    }

    /*
    CDXC:Clipboard 2026-11-17 WHY:
    User (Discord feedback, and the owner had seen it too): pasting into a terminal sometimes pasted nothing. Every step of a terminal paste had a way to stop without a word: the terminal still starting, focus moving before the clipboard read finished, an empty or unreadable clipboard (Universal Clipboard and apps that hand over their text late can fail to deliver it), an image or file with no text while images-as-links is off, an image that could not be saved, a terminal whose connection had dropped refusing the bytes, and a paste confirmation that could not open. Each now shows a short card saying what happened and what to do, and writes the reason (never the clipboard contents) to the terminal focus log, so the next report names its cause.
    */
    pub(crate) fn report_terminal_paste_failure(
        &mut self,
        failure: TerminalPasteFailure,
        cx: &mut gpui::Context<Self>,
    ) {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalPaste.notDelivered",
            serde_json::json!({
                "reason": format!("{failure:?}"),
                "shellFocus": format!("{:?}", self.shell_focus),
                "firstResponderTarget": format!("{:?}", self.first_responder_target),
            }),
        );
        let (title, description) = failure.card_text();
        self.dispatch_gpui_workspace_action_toast("warning", title, description, cx);
    }

    /// The terminal a paste is meant for, taken when the key is pressed. The clipboard read
    /// finishes later (see `gpui_read_clipboard`), and the paste lands only if that terminal still
    /// has the keyboard; `None` when no terminal has it.
    ///
    /// CDXC:Clipboard 2026-10-12 WHY: the terminal is taken from `shell_keyboard_owner`, not from shell focus alone. Shell focus keeps naming the Agents pane while a chat-mode session's composer owns its keys, so claiming the paste from shell focus swallowed Cmd+V before the composer's own `Paste` could run, and aimed it at the parked terminal behind the chat.
    fn terminal_paste_target(&self) -> Option<TerminalPasteTarget> {
        let view = match self.shell_keyboard_owner() {
            ShellKeyboardOwner::EngineTerminal { view, .. } => Some(view.entity_id()),
            _ => return None,
        };
        Some(TerminalPasteTarget {
            view,
            text_target: focused_terminal_text_target(self.active_mode, self.shell_focus),
            session_id: self.focused_terminal_shell_session_id(),
        })
    }

    fn paste_clipboard_item_into_focused_terminal(
        &mut self,
        item: &ClipboardItem,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        if paste_previewable_images_enabled
            && self.paste_clipboard_image_into_focused_remote_terminal(item, cx)
        {
            return true;
        }
        let Some(text) = terminal_clipboard_paste_text(
            item,
            paste_previewable_images_enabled,
            self.focused_terminal_is_factory_droid(),
        ) else {
            self.report_terminal_paste_failure(
                terminal_clipboard_without_text_failure(item, paste_previewable_images_enabled),
                cx,
            );
            return false;
        };

        self.paste_text_into_focused_terminal_surface(&text, cx)
    }

    pub(crate) fn paste_image_or_send_control_v(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        if !enabled {
            return self.send_text_to_focused_terminal_surface("\u{16}", cx);
        }
        if self.focused_terminal_is_still_mounting() {
            self.report_terminal_paste_failure(TerminalPasteFailure::TerminalStarting, cx);
            return true;
        }
        let Some(target) = self.terminal_paste_target() else {
            return false;
        };
        let read = gpui_read_clipboard(cx, "terminalControlV");
        cx.spawn(async move |this, cx| {
            let item = read.await;
            this.update(cx, |this, cx| {
                if this.terminal_paste_target() != Some(target) {
                    this.report_terminal_paste_failure(TerminalPasteFailure::FocusMoved, cx);
                    return;
                }
                if let Some(item) = item
                    && this.paste_previewable_clipboard_image(&item, cx)
                {
                    return;
                }
                this.send_text_to_focused_terminal_surface("\u{16}", cx);
            })
            .ok();
        })
        .detach();
        true
    }

    fn paste_previewable_clipboard_image(
        &mut self,
        item: &ClipboardItem,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.paste_clipboard_image_into_focused_remote_terminal(item, cx) {
            return true;
        }
        let Some(markdown) = terminal_clipboard_previewable_image_markdown_text(item) else {
            return false;
        };
        let text = if self.focused_terminal_is_factory_droid() {
            format!("  {markdown}")
        } else {
            markdown
        };
        self.paste_text_into_focused_terminal_surface(&text, cx)
    }

    /*
    CDXC:Clipboard 2026-08-21:
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
        let Some(remote_machine_id) = self
            .agents_chat_remote_key_for_session(session_id)
            .map(|key| key.remote_machine_id)
        else {
            return false;
        };
        let Some(payload) = terminal_clipboard_image_payload(item) else {
            return false;
        };

        // Runtime IDs are project-local, so async uploads also retain the
        // originating viewer identity before they can paste their result.
        let runtime_session_id = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.runtime_session_id);
        let originating_view_id = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.entity_id());
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
        let viewer_lease = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.pin_viewer());
        cx.spawn(async move |this, cx| {
            let _viewer_lease = viewer_lease;
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
                            && originating_view_id.is_some_and(|view_id| {
                                this.gpui_terminal_viewer_matches_entity(target, view_id)
                            })
                    }
                    None => this.focused_terminal_shell_session_id() == Some(session_id),
                };
                if !destination_is_live {
                    this.report_terminal_paste_failure(TerminalPasteFailure::FocusMoved, cx);
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
                                if !this.paste_text_into_gpui_engine_terminal_target(
                                    target,
                                    runtime_session_id,
                                    text.as_str(),
                                    cx,
                                ) {
                                    this.report_terminal_paste_failure(
                                        TerminalPasteFailure::TerminalRefusedInput,
                                        cx,
                                    );
                                }
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
            _ => None,
        }
    }

    pub(crate) fn focused_terminal_is_factory_droid(&self) -> bool {
        self.focused_terminal_shell_session_id()
            .and_then(|session_id| self.agents_workspace.session(session_id))
            .and_then(|session| session.agent_icon)
            == Some("factory-droid")
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
                // Only a dialog that is really on screen keeps an earlier paste waiting. A pending
                // paste whose dialog is gone would otherwise swallow every later paste that needs
                // confirming, without a word.
                let dialog_on_screen = self.terminal_paste_confirmation_dialog_open
                    && self.native_app_modal_kind() == Some(GpuiAppModalKind::TerminalPasteConfirm);
                if !dialog_on_screen {
                    self.pending_terminal_paste_confirmation =
                        Some(PendingGpuiTerminalPasteConfirmation {
                            text: text.to_string(),
                            view,
                        });
                    self.open_terminal_paste_confirmation_modal(cx);
                    cx.notify();
                }
                return true;
            }
            if !view.update(cx, |view, cx| view.paste_text(text, cx)) {
                self.report_terminal_paste_failure(TerminalPasteFailure::TerminalRefusedInput, cx);
            }
            return true;
        }
        if self.send_text_to_focused_terminal_surface(text, cx) {
            return true;
        }
        self.report_terminal_paste_failure(TerminalPasteFailure::FocusMoved, cx);
        false
    }

    /// Asks before a paste that could run commands, in a native app-modal window
    /// (window/terminal_paste_confirm_modal.rs) so CEF views cannot cover it.
    fn open_terminal_paste_confirmation_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let palette = self.gpui_native_dialog_palette();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_terminal_paste_confirmation_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::TerminalPasteConfirm,
            TERMINAL_PASTE_CONFIRM_MODAL_WIDTH,
            TERMINAL_PASTE_CONFIRM_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiTerminalPasteConfirmModalWindow::new(palette, host, window, cx))
            },
            cx,
        );
        if self.native_app_modal_kind() == Some(GpuiAppModalKind::TerminalPasteConfirm) {
            self.terminal_paste_confirmation_dialog_open = true;
        } else {
            self.pending_terminal_paste_confirmation = None;
            self.terminal_paste_confirmation_dialog_open = false;
            self.report_terminal_paste_failure(TerminalPasteFailure::ConfirmationUnavailable, cx);
        }
    }

    fn handle_terminal_paste_confirmation_modal_command(
        &mut self,
        command: TerminalPasteConfirmModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let pending = self.pending_terminal_paste_confirmation.take();
        self.terminal_paste_confirmation_dialog_open = false;
        self.release_native_app_modal_window(GpuiAppModalKind::TerminalPasteConfirm, cx);
        if let (TerminalPasteConfirmModalCommand::Paste, Some(pending)) = (command, pending)
            && !pending
                .view
                .update(cx, |view, cx| view.paste_text(&pending.text, cx))
        {
            self.report_terminal_paste_failure(TerminalPasteFailure::TerminalRefusedInput, cx);
        }
        cx.notify();
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
            // A chat box maximizes or restores instead (CDXC:Hotkeys 2026-10-25 DECISION in native_chat/maximized.rs).
            if self.run_focused_chat_hotkey("toggleChatComposerMaximized", window, cx) {
                return true;
            }
            if let Some((target, runtime_session_id)) =
                self.focused_gpui_engine_terminal_action_target()
            {
                self.handle_gpui_engine_prompt_editor_shortcut(target, runtime_session_id, cx);
            }
            return true;
        }
        // A chat box stashes its own draft: no terminal is the target while it shows
        // (CDXC:Hotkeys 2026-10-31, the chat view's Alt+S did nothing). Saved Prompts opens
        // for the chat on screen the same way (GitHub #756: it did nothing in a Native chat),
        // and so do Export Transcript, Attach, Agent Actions (the chat box's More actions) and
        // Scroll to Top / Bottom (the oldest loaded message and the newest one).
        if matches!(
            action_id,
            "stashPrompt"
                | "stashedPrompts"
                | "exportTranscript"
                | "attachFileOrFolder"
                | "toggleAgentActions"
                | "scrollTerminalToTop"
                | "scrollTerminalToBottom"
        ) && self.run_focused_chat_hotkey(action_id, window, cx)
        {
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
}

/// Why a terminal paste did not reach the terminal (CDXC:Clipboard 2026-11-17 above).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalPasteFailure {
    /// The focused pane's terminal is still being created.
    TerminalStarting,
    /// The terminal that had the keyboard when Paste was pressed lost it before the paste landed.
    FocusMoved,
    /// The clipboard read gave nothing back: empty, or the app that copied never handed it over.
    ClipboardUnreadable,
    /// The clipboard holds something, but no text a terminal can take.
    NoText,
    /// An image or image file with no text while Paste images as preview links is off.
    ImageNeedsSetting,
    /// Paste images as preview links is on, but the image could not be read or saved.
    ImageNotSaved,
    /// The terminal refused the bytes: its connection to the session has ended or is being replaced.
    TerminalRefusedInput,
    /// The paste needed confirming and the confirmation window could not open.
    ConfirmationUnavailable,
}

impl TerminalPasteFailure {
    fn card_text(self) -> (&'static str, &'static str) {
        match self {
            Self::TerminalStarting => (
                "Nothing pasted",
                "The terminal is still starting. Paste again in a moment.",
            ),
            Self::FocusMoved => (
                "Nothing pasted",
                "The terminal lost focus before the paste finished. Click the terminal and paste again.",
            ),
            Self::ClipboardUnreadable => (
                "Nothing pasted",
                "The clipboard is empty or could not be read. Copy the text again, then paste.",
            ),
            Self::NoText => (
                "Nothing pasted",
                "The clipboard has no text a terminal can take. Copy the text again, then paste.",
            ),
            Self::ImageNeedsSetting => (
                "Image not pasted",
                "Terminals take images only with Paste images as preview links on. Turn it on in Settings, Terminal.",
            ),
            Self::ImageNotSaved => (
                "Image not pasted",
                "Ghostex could not save the image from the clipboard. Copy it again, then paste.",
            ),
            Self::TerminalRefusedInput => (
                "Nothing pasted",
                "The terminal is not connected right now. Wait for it to reconnect, then paste again.",
            ),
            Self::ConfirmationUnavailable => (
                "Nothing pasted",
                "The paste confirmation could not open. Paste again.",
            ),
        }
    }
}

/// Why a clipboard that `terminal_clipboard_paste_text` turned down gave a terminal nothing.
fn terminal_clipboard_without_text_failure(
    item: &ClipboardItem,
    paste_previewable_images_enabled: bool,
) -> TerminalPasteFailure {
    let holds_image = item.entries().iter().any(|entry| match entry {
        gpui::ClipboardEntry::Image(_) => true,
        gpui::ClipboardEntry::ExternalPaths(paths) => paths
            .paths()
            .iter()
            .any(|path| is_project_board_image_file_path(path)),
        gpui::ClipboardEntry::String(_) => false,
    });
    match (holds_image, paste_previewable_images_enabled) {
        (true, false) => TerminalPasteFailure::ImageNeedsSetting,
        (true, true) => TerminalPasteFailure::ImageNotSaved,
        (false, _) if item.entries().is_empty() => TerminalPasteFailure::ClipboardUnreadable,
        (false, _) => TerminalPasteFailure::NoText,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct TerminalPasteTarget {
    view: Option<gpui::EntityId>,
    text_target: Option<FocusedTerminalTextTarget>,
    session_id: Option<TerminalSessionId>,
}
