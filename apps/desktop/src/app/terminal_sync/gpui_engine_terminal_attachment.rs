// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: GPUI-engine terminal file attachment targets/paths, manage-files bridge side effects, stashed-prompt/path insertion, and first-prompt/search-total sync.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use gpui::ClipboardItem;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn gpui_terminal_attachment_target_for_engine_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<GpuiTerminalAttachmentTarget> {
        let GpuiEngineTerminalEventTarget::Agents(session_id) = target else {
            return Some(GpuiTerminalAttachmentTarget::Terminal(target));
        };
        let Some(slot_id) = self
            .current_project_editor_companion_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| slot_id.session_id == session_id)
        else {
            if self.active_mode.is_project_editor_mode() {
                return None;
            }
            return Some(GpuiTerminalAttachmentTarget::Terminal(target));
        };
        let session_key = self.project_editor_companion_terminal_key_for_slot(slot_id)?;
        Some(GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
            slot_id,
            session_key,
        })
    }

    pub(crate) fn request_gpui_engine_terminal_attachment_paths(
        &mut self,
        target: GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Attach File or Folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.attach_selected_path_to_gpui_engine_terminal(
                    target,
                    runtime_session_id,
                    path,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn attach_selected_path_to_gpui_engine_terminal(
        &mut self,
        target: GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        path: PathBuf,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.gpui_terminal_attachment_target_matches_runtime(&target, runtime_session_id) {
            return;
        }

        let remote_machine_id = match &target {
            GpuiTerminalAttachmentTarget::Terminal(GpuiEngineTerminalEventTarget::Agents(
                session_id,
            )) => self
                .remote_attach_sessions
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (mapped_session_id == session_id).then(|| key.remote_machine_id.clone())
                }),
            GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
                session_key: GpuiWorkspaceTerminalSessionKey::Remote(remote_key),
                ..
            } => Some(remote_key.remote_machine_id.clone()),
            GpuiTerminalAttachmentTarget::Terminal(GpuiEngineTerminalEventTarget::Command(_))
            | GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
                session_key: GpuiWorkspaceTerminalSessionKey::Local(_),
                ..
            } => None,
        };
        let Some(remote_machine_id) = remote_machine_id else {
            match gpui_local_terminal_attachment_reference(path.as_path()) {
                Ok(reference) => {
                    let text = gpui_terminal_attachment_markdown_text(&[reference]);
                    let _ = self.paste_text_into_gpui_engine_terminal_target(
                        target.engine_target(),
                        runtime_session_id,
                        text.as_str(),
                        cx,
                    );
                }
                Err(message) => self.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Attachment unavailable",
                    message.as_str(),
                    cx,
                ),
            }
            return;
        };

        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) =
            gpui_remote_machine_config_from_settings(settings.object(), remote_machine_id.as_str())
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Attachment unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return;
        };
        let Some(remote_target) = self.gpui_remote_gxserver_request_target(&remote_machine_id)
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Attachment unavailable",
                "Reconnect the remote machine before attaching a file or folder.",
                cx,
            );
            return;
        };

        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Uploading attachment",
            "Uploading the selected item to the remote machine.",
            cx,
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_upload_terminal_attachment_to_remote(
                        &config,
                        &remote_target.execution_target,
                        path.as_path(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this
                    .gpui_terminal_attachment_target_matches_runtime(&target, runtime_session_id)
                {
                    return;
                }
                match result {
                    Ok(reference) => {
                        let text = gpui_terminal_attachment_markdown_text(&[reference]);
                        if this.paste_text_into_gpui_engine_terminal_target(
                            target.engine_target(),
                            runtime_session_id,
                            text.as_str(),
                            cx,
                        ) {
                            this.dispatch_gpui_workspace_action_toast(
                                "success",
                                "Attachment uploaded",
                                "The remote attachment reference was pasted into the terminal.",
                                cx,
                            );
                        }
                    }
                    Err(message) => this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Attachment upload failed",
                        message.as_str(),
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    pub(crate) fn gpui_terminal_attachment_target_matches_runtime(
        &self,
        target: &GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> bool {
        if !self
            .gpui_engine_terminal_target_matches_runtime(target.engine_target(), runtime_session_id)
        {
            return false;
        }
        match target {
            GpuiTerminalAttachmentTarget::Terminal(_) => true,
            GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
                slot_id,
                session_key,
            } => {
                self.project_editor_companion_terminal_key_for_slot(*slot_id)
                    .as_ref()
                    == Some(session_key)
            }
        }
    }

    pub(crate) fn gpui_engine_terminal_target_matches_runtime(
        &self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> bool {
        match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .is_some_and(|record| record.runtime_session_id == runtime_session_id),
            GpuiEngineTerminalEventTarget::Command(session_id) => self
                .command_gpui_engine_terminals
                .get(&session_id)
                .is_some_and(|record| record.runtime_session_id == runtime_session_id),
        }
    }

    pub(crate) fn paste_text_into_gpui_engine_terminal_target(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if text.is_empty() {
            return false;
        }
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
            return false;
        };
        view.update(cx, |view, cx| view.paste_text(text, cx));
        true
    }

    pub(crate) fn perform_manage_files_bridge_side_effect(
        &mut self,
        side_effect: ManageFilesBridgeSideEffect,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        match side_effect {
            ManageFilesBridgeSideEffect::CopyFullPath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path));
                Ok(())
            }
            ManageFilesBridgeSideEffect::RevealInFinder(path) => gpui_reveal_path_in_finder(&path),
            ManageFilesBridgeSideEffect::AddToSessionContext(prompt) => {
                let session_id = self
                    .manage_session_context_target_session_id()
                    .ok_or_else(|| "No active agent session is available.".to_string())?;
                if self.insert_manage_file_context_into_agents_session(session_id, &prompt, cx) {
                    Ok(())
                } else {
                    Err("No active agent session is available.".to_string())
                }
            }
        }
    }

    pub(crate) fn manage_session_context_target_session_id(&self) -> Option<TerminalSessionId> {
        let mut candidates = Vec::new();
        if let Some(key) = self.project_editor_companion_active_terminal_key()
            && let Some(session_id) = self.shell_session_for_workspace_terminal_key(&key)
        {
            candidates.push(session_id);
        }
        if let Some(session_id) = self.focused_agents_or_companion_shell_session_id() {
            candidates.push(session_id);
        }
        if let (Some(active_project_id), Some(latest_key)) = (
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.active_project_id.as_ref())
                .map(|project_id| project_id.0.as_str()),
            self.local_workspace_latest_focus_key.as_ref(),
        ) && latest_key.project_id == active_project_id
            && let Some(session_id) = self.local_workspace_session_mappings.get(latest_key)
        {
            candidates.push(*session_id);
        }
        let mut seen = HashSet::new();
        candidates.into_iter().find(|session_id| {
            seen.insert(*session_id)
                && self
                    .agents_workspace
                    .session(*session_id)
                    .is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Running
                            && session.agent_icon.is_some()
                    })
        })
    }

    pub(crate) fn insert_manage_file_context_into_agents_session(
        &mut self,
        shell_session_id: TerminalSessionId,
        prompt: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if prompt.is_empty()
            || !self
                .agents_workspace
                .session(shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Running
                        && session.agent_icon.is_some()
                })
        {
            return false;
        }
        let companion_focused = matches!(
            self.focused_terminal_text_mount_target(),
            Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id))
                if slot_id.session_id == shell_session_id
        );
        let pane_id = self.agents_workspace.pane_id_for_session(shell_session_id);
        if !companion_focused {
            let Some(pane_id) = pane_id else {
                return false;
            };
            self.active_mode = TitlebarMode::Agents;
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.scroll_workspace_pane_active_tab(pane_id);
        }
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .map(|record| record.view.clone())
        {
            view.update(cx, |view, cx| view.paste_text(prompt, cx));
            cx.notify();
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            let inserted = if companion_focused {
                self.send_text_bytes_to_focused_project_editor_companion_terminal_surface(
                    prompt.as_bytes(),
                )
            } else if let Some(pane_id) = pane_id {
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: shell_session_id,
                };
                self.agents_terminal_ghostty_surface_matches(slot_id)
                    && self.send_text_bytes_to_focused_agents_terminal_surface(prompt.as_bytes())
            } else {
                false
            };
            if inserted {
                cx.notify();
                return true;
            }
        }
        false
    }

    /// Inserts a stashed prompt back into the mapped Agents input surface for a
    /// combined presentation session id ("P…:G…"). Chat sessions receive the
    /// prompt through their bounded composer callback; terminal sessions use
    /// their native paste semantics so multiline prompts do not submit.
    pub(crate) fn insert_stashed_prompt_into_agents_session(
        &mut self,
        combined_session_id: &str,
        content: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if content.is_empty() {
            return false;
        }
        let Some(shell_session_id) =
            self.local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped)| {
                    (gpui_combined_presentation_session_id(&key.project_id, &key.session_id)
                        == combined_session_id)
                        .then_some(*mapped)
                })
        else {
            return false;
        };
        if self.agents_chat_mode_sessions.contains(&shell_session_id) {
            return self.insert_prompt_into_session_chat(shell_session_id, content, cx);
        }
        // A session focused in a project-editor companion pane receives the
        // paste in place; switching the app into the Agents view just to
        // reveal a tab the user is already looking at would lose their editor
        // context.
        let companion_focused = matches!(
            self.focused_terminal_text_mount_target(),
            Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id))
                if slot_id.session_id == shell_session_id
        );
        let pane_id = self.agents_workspace.pane_id_for_session(shell_session_id);
        if !companion_focused {
            let Some(pane_id) = pane_id else {
                return false;
            };
            self.active_mode = TitlebarMode::Agents;
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.scroll_workspace_pane_active_tab(pane_id);
        }
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .map(|record| record.view.clone())
        {
            view.update(cx, |view, cx| view.paste_text(content, cx));
            cx.notify();
            return true;
        }
        #[cfg(target_os = "macos")]
        if !companion_focused {
            if let Some(pane_id) = pane_id {
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: shell_session_id,
                };
                if self.agents_terminal_ghostty_surface_matches(slot_id)
                    && self.send_text_bytes_to_focused_agents_terminal_surface(content.as_bytes())
                {
                    cx.notify();
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn insert_paths_into_gpui_engine_terminal(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        paths: &[PathBuf],
        cx: &mut gpui::Context<Self>,
    ) {
        let mut next_image_number = 1usize;
        let text = paths
            .iter()
            .map(|path| {
                if is_project_board_image_file_path(path) {
                    let markdown =
                        terminal_clipboard_markdown_image_reference(path, next_image_number);
                    next_image_number += 1;
                    markdown
                } else {
                    path.to_string_lossy().into_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            return;
        }
        let view = match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
            GpuiEngineTerminalEventTarget::Command(session_id) => self
                .command_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
        };
        if let Some(view) = view {
            view.update(cx, |view, cx| view.send_text_input(&text, cx));
        }
    }

    /// True while gxserver reports a first-prompt title job in flight for the
    /// mapped workspace session. Drives both terminal input suppression and
    /// the blocking "Generating title" pane overlay so they can never disagree.
    pub(crate) fn agents_session_is_generating_first_prompt_title(
        &self,
        shell_session_id: TerminalSessionId,
    ) -> bool {
        self.agents_workspace
            .session(shell_session_id)
            .is_some_and(|session| session.is_generating_first_prompt_title)
    }

    pub(crate) fn sync_gpui_engine_first_prompt_input_suppression(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let suppression_by_session = self
            .agents_gpui_engine_terminals
            .keys()
            .copied()
            .map(|shell_session_id| {
                let suppress =
                    self.agents_session_is_generating_first_prompt_title(shell_session_id);
                (shell_session_id, suppress)
            })
            .collect::<Vec<_>>();

        for (shell_session_id, suppress) in suppression_by_session {
            if let Some(record) = self.agents_gpui_engine_terminals.get(&shell_session_id) {
                record.view.update(cx, |view, cx| {
                    view.set_input_suppressed(suppress, cx);
                });
            }
        }
    }

    /// Mirror each open GPUI-engine find's totals into the shared search
    /// state so the search bar count label matches the native path.
    pub(crate) fn sync_gpui_engine_search_totals(&mut self, cx: &mut gpui::Context<Self>) {
        fn mirror_totals<'a>(
            records: impl Iterator<Item = &'a terminal_gpui_engine::GpuiEngineTerminalRecord>,
            osc_states: &mut HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
            cx: &gpui::App,
        ) -> bool {
            let mut changed = false;
            for record in records {
                let Some((total, selected)) = record.view.read(cx).search_totals() else {
                    continue;
                };
                let Some(search) = osc_states
                    .get_mut(&record.runtime_session_id)
                    .and_then(|state| state.search.as_mut())
                else {
                    continue;
                };
                let total = Some(total as u64);
                let selected = Some(selected as u64);
                if search.total != total || search.selected != selected {
                    search.total = total;
                    search.selected = selected;
                    changed = true;
                }
            }
            changed
        }
        let mut changed = mirror_totals(
            self.agents_gpui_engine_terminals.values(),
            &mut self.agents_terminal_runtime_osc_states,
            cx,
        );
        changed |= mirror_totals(
            self.command_gpui_engine_terminals.values(),
            &mut self.command_terminal_runtime_osc_states,
            cx,
        );
        if changed {
            cx.notify();
        }
    }

    /// The GPUI-engine record backing a runtime session id, if any
    /// (Agents and command maps share the runtime-id namespace).
    pub(crate) fn gpui_engine_record_for_runtime_session_id(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Option<&terminal_gpui_engine::GpuiEngineTerminalRecord> {
        self.agents_gpui_engine_terminals
            .values()
            .chain(self.command_gpui_engine_terminals.values())
            .find(|record| record.runtime_session_id == runtime_session_id)
    }

    pub(crate) fn agents_terminal_native_views_may_be_visible(&self) -> bool {
        /*
        CDXC:Workarea 2026-07-03:
        Workspace/Agents tab drags treat mounted Agents terminals like a mode switch away from Agents: Running host reconciliation, parked-owner reattach, and ready-startup handoff promotion all wait until the drag ends. This hides the native Ghostty child views for the whole drag so the GPUI drag ghost and pane-body drop-edge bands stay visible, while parked owners keep every runtime surface alive for hide/show-only restore on drop or cancel. Startup candidates, launch plans, and hidden startup hosts intentionally keep running during a drag; only promotion to a visible Running host is deferred.
        */
        self.active_mode == TitlebarMode::Agents && !self.workspace_tab_drag_active
    }
}
