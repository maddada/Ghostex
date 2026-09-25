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

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct PickTerminalAttachmentKind {
    pub(crate) target: GpuiTerminalAttachmentTarget,
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) directories_only: bool,
}

impl GhostexGpuiApp {
    pub(crate) fn gpui_terminal_attachment_target_for_engine_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<GpuiTerminalAttachmentTarget> {
        Some(GpuiTerminalAttachmentTarget::Terminal(target))
    }

    pub(crate) fn request_gpui_engine_terminal_attachment_paths(
        &mut self,
        target: GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if cfg!(target_os = "linux") {
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |_, window, cx| {
                    crate::app::context_menu::GpuiContextMenu::new()
                        .menu(
                            "Images or files…",
                            Box::new(PickTerminalAttachmentKind {
                                target: target.clone(),
                                runtime_session_id,
                                directories_only: false,
                            }),
                        )
                        .menu(
                            "Folders…",
                            Box::new(PickTerminalAttachmentKind {
                                target,
                                runtime_session_id,
                                directories_only: true,
                            }),
                        )
                        .show_for_app(cx.entity(), window.mouse_position(), window, cx);
                });
            })
            .detach();
            return;
        }
        self.request_gpui_engine_terminal_attachment_paths_for_kind(
            target,
            runtime_session_id,
            None,
            cx,
        );
    }

    /// CDXC:PlatformSupport 2026-09-23 WHY:
    /// XDG file chooser portals have mutually exclusive file and directory modes. Linux attachment menus choose the mode before opening the portal; passing both flags hides every file in KDE's folder chooser.
    pub(crate) fn attachment_path_prompt_options(
        multiple: bool,
        directories_only: Option<bool>,
    ) -> gpui::PathPromptOptions {
        let directories_only =
            directories_only.or_else(|| cfg!(target_os = "linux").then_some(false));
        gpui::PathPromptOptions {
            files: directories_only != Some(true),
            directories: directories_only != Some(false),
            multiple,
            prompt: Some(
                match directories_only {
                    Some(true) => "Attach Folders",
                    Some(false) => "Attach Images or Files",
                    None => "Attach an Image, File, or Folder",
                }
                .into(),
            ),
        }
    }

    pub(crate) fn request_gpui_engine_terminal_attachment_paths_for_kind(
        &mut self,
        target: GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        directories_only: Option<bool>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((origin_view_id, lease)) = (match target.engine_target() {
            GpuiEngineTerminalEventTarget::Agents(id) => self.agents_gpui_engine_terminals.get(&id),
            GpuiEngineTerminalEventTarget::Command(id) => {
                self.command_gpui_engine_terminals.get(&id)
            }
        })
        .filter(|record| record.runtime_session_id == runtime_session_id)
        .map(|record| (record.view.entity_id(), record.pin_viewer())) else {
            return;
        };
        let receiver = cx.prompt_for_paths(Self::attachment_path_prompt_options(
            false,
            directories_only,
        ));
        cx.spawn(async move |this, cx| {
            let _lease = lease;
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if !this.gpui_terminal_viewer_matches_entity(target.engine_target(), origin_view_id)
                {
                    return;
                }
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

        let Some(remote_machine_id) = self.gpui_terminal_attachment_remote_machine_id(&target)
        else {
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
        self.upload_paths_to_remote_gpui_engine_terminal(
            target,
            runtime_session_id,
            remote_machine_id,
            vec![path],
            cx,
        );
    }

    /// The remote machine whose terminal the target shows, or `None` when the
    /// terminal runs on this computer.
    pub(crate) fn gpui_terminal_attachment_remote_machine_id(
        &self,
        target: &GpuiTerminalAttachmentTarget,
    ) -> Option<String> {
        match target {
            GpuiTerminalAttachmentTarget::Terminal(GpuiEngineTerminalEventTarget::Agents(
                session_id,
            )) => self
                .agents_chat_remote_key_for_session(*session_id)
                .map(|key| key.remote_machine_id),
            GpuiTerminalAttachmentTarget::Terminal(GpuiEngineTerminalEventTarget::Command(
                session_id,
            )) => self
                .command_remote_action_session_for_command_tab(*session_id)
                .map(|reference| reference.remote_machine_id.clone()),
        }
    }

    /// Uploads local items over the remote machine's SSH connection and pastes
    /// the returned remote references into the terminal. Items upload in order
    /// and the first failure cancels the paste, so the agent never receives a
    /// partial list.
    pub(crate) fn upload_paths_to_remote_gpui_engine_terminal(
        &mut self,
        target: GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        remote_machine_id: String,
        paths: Vec<PathBuf>,
        cx: &mut gpui::Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
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
        let Some((origin_view_id, lease)) = (match target.engine_target() {
            GpuiEngineTerminalEventTarget::Agents(id) => self.agents_gpui_engine_terminals.get(&id),
            GpuiEngineTerminalEventTarget::Command(id) => {
                self.command_gpui_engine_terminals.get(&id)
            }
        })
        .map(|record| (record.view.entity_id(), record.pin_viewer())) else {
            return;
        };

        let uploading_message = if paths.len() == 1 {
            "Uploading the selected item to the remote machine.".to_string()
        } else {
            format!("Uploading {} items to the remote machine.", paths.len())
        };
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Uploading attachment",
            uploading_message.as_str(),
            cx,
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let _lease = lease;
            let result = background
                .spawn(async move {
                    paths
                        .iter()
                        .map(|path| {
                            gpui_upload_terminal_attachment_to_remote(
                                &config,
                                &remote_target.execution_target,
                                path.as_path(),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.gpui_terminal_viewer_matches_entity(target.engine_target(), origin_view_id)
                    || !this.gpui_terminal_attachment_target_matches_runtime(
                        &target,
                        runtime_session_id,
                    )
                {
                    return;
                }
                match result {
                    Ok(references) => {
                        let text = gpui_terminal_attachment_markdown_text(&references);
                        if this.paste_text_into_gpui_engine_terminal_target(
                            target.engine_target(),
                            runtime_session_id,
                            text.as_str(),
                            cx,
                        ) {
                            this.dispatch_gpui_workspace_action_toast(
                                "success",
                                "Attachment uploaded",
                                if references.len() == 1 {
                                    "The remote attachment reference was pasted into the terminal."
                                } else {
                                    "The remote attachment references were pasted into the terminal."
                                },
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
                gpui_copy_to_clipboard(ClipboardItem::new_string(path), cx);
                Ok(())
            }
            ManageFilesBridgeSideEffect::RevealInFinder(path) => gpui_reveal_path_in_finder(&path),
            ManageFilesBridgeSideEffect::AddToSessionContext(prompt) => {
                let session_id = self
                    .manage_session_context_target_session_id()
                    .ok_or_else(|| "No active agent session is available.".to_string())?;
                if self
                    .insert_manage_file_context_into_agents_session(session_id, &prompt, true, cx)
                {
                    Ok(())
                } else {
                    Err("No active agent session is available.".to_string())
                }
            }
        }
    }

    pub(crate) fn manage_session_context_target_session_id(&self) -> Option<TerminalSessionId> {
        let mut candidates = Vec::new();
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

    /// Pastes `prompt` into the session's terminal input. With `reveal` the app
    /// switches to the Agents view and focuses the pane first (Add to Session
    /// Context); without it the text lands while the current view stays put.
    pub(crate) fn insert_manage_file_context_into_agents_session(
        &mut self,
        shell_session_id: TerminalSessionId,
        prompt: &str,
        reveal: bool,
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
        let pane_id = self.agents_workspace.pane_id_for_session(shell_session_id);
        {
            let Some(pane_id) = pane_id else {
                return false;
            };
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            if reveal {
                self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
            }
            self.scroll_workspace_pane_active_tab(pane_id);
        }
        self.ensure_agents_gpui_engine_terminal_view(shell_session_id, cx);
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
            let inserted = if let Some(pane_id) = pane_id {
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: shell_session_id,
                };
                if reveal {
                    self.agents_terminal_ghostty_surface_matches(slot_id)
                        && self
                            .send_text_bytes_to_focused_agents_terminal_surface(prompt.as_bytes())
                } else {
                    self.send_text_bytes_to_mounted_agents_terminal_surface(
                        slot_id,
                        prompt.as_bytes(),
                    )
                }
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
        preserve_existing: bool,
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
            if preserve_existing {
                return self.append_recovered_prompt_into_session_chat(
                    shell_session_id,
                    content,
                    cx,
                );
            }
            return self.insert_prompt_into_session_chat(shell_session_id, content, cx);
        }
        // CDXC:Workarea 2026-09-20 WHY:
        // The paste reveals the session in the Agents column, which is on screen beside whatever the
        // view panel shows, so it no longer has to leave the user's view to do it.
        let pane_id = self.agents_workspace.pane_id_for_session(shell_session_id);
        {
            let Some(pane_id) = pane_id else {
                return false;
            };
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
            self.scroll_workspace_pane_active_tab(pane_id);
        }
        self.ensure_agents_gpui_engine_terminal_view(shell_session_id, cx);
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
        false
    }

    /// CDXC:Clipboard 2026-09-25 WHY:
    /// A remote terminal runs on the remote machine, so a dropped item's local path names nothing the agent there can open. Drops on a remote Agents or command terminal therefore upload like the terminal's Attach File or Folder button and paste the remote references; local terminals keep the typed paths.
    pub(crate) fn insert_paths_into_gpui_engine_terminal(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        paths: &[PathBuf],
        cx: &mut gpui::Context<Self>,
    ) {
        let attachment_target = GpuiTerminalAttachmentTarget::Terminal(target);
        if let Some(remote_machine_id) =
            self.gpui_terminal_attachment_remote_machine_id(&attachment_target)
        {
            if self.gpui_terminal_attachment_target_matches_runtime(
                &attachment_target,
                runtime_session_id,
            ) {
                self.upload_paths_to_remote_gpui_engine_terminal(
                    attachment_target,
                    runtime_session_id,
                    remote_machine_id,
                    paths.to_vec(),
                    cx,
                );
            }
            return;
        }
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
        self.agents_workspace_visible() && !self.workspace_tab_drag_active
    }
}
