//! Creating registered agents terminals and placeholder tabs, splitting, merging and rotating agents panes.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn add_agents_registered_terminal_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.create_registered_agents_terminal(
            pane_id,
            AgentsWorkspaceNewTerminalPlacement::Tab,
            cx,
        );
    }

    pub(crate) fn create_registered_agents_terminal(
        &mut self,
        requested_pane_id: WorkspacePaneId,
        placement: AgentsWorkspaceNewTerminalPlacement,
        cx: &mut gpui::Context<Self>,
    ) {
        self.create_registered_agents_terminal_with_launch(requested_pane_id, placement, None, cx);
    }

    pub(crate) fn create_registered_agents_extension_terminal(
        &mut self,
        requested_pane_id: WorkspacePaneId,
        placement: AgentsWorkspaceNewTerminalPlacement,
        title: String,
        working_directory: Option<String>,
        startup_text: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.create_registered_agents_terminal_with_launch(
            requested_pane_id,
            placement,
            Some(AgentsWorkspaceTerminalLaunch {
                title,
                working_directory,
                startup_text,
            }),
            cx,
        );
    }

    fn create_registered_agents_terminal_with_launch(
        &mut self,
        requested_pane_id: WorkspacePaneId,
        placement: AgentsWorkspaceNewTerminalPlacement,
        launch: Option<AgentsWorkspaceTerminalLaunch>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-07-24:
        Every Agents-workspace quick-create surface (Cmd+T, tab-strip "+", split
        right/below, full-width bottom row) must create a real gxserver session
        and attach to it like sidebar-created sessions do. Local Mounting
        placeholders with raw shells never registered with the daemon, so those
        terminals had no sidebar listing, vanished on project switch, and could
        not recover an attach payload after being moved between panes.
        */
        let Some(project_id) = self.gpui_app_modal_active_project_id() else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Terminal unavailable",
                "Select a project before creating a terminal.",
                cx,
            );
            return;
        };
        if let Some(remote_project) =
            gpui_remote_project_reference_from_project_id(project_id.as_str())
        {
            if launch.is_some() {
                self.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Extension unavailable",
                    "Terminal extensions currently run only for local projects.",
                    cx,
                );
                return;
            }
            let Some(target) =
                self.gpui_remote_gxserver_request_target(remote_project.remote_machine_id.as_str())
            else {
                self.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Terminal unavailable",
                    "Reconnect the remote machine before creating a terminal.",
                    cx,
                );
                return;
            };
            let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
            let Some(config) = gpui_remote_machine_config_from_settings(
                settings_snapshot.object(),
                remote_project.remote_machine_id.as_str(),
            ) else {
                self.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Terminal unavailable",
                    "The saved remote machine is missing required SSH settings.",
                    cx,
                );
                return;
            };
            let active_project_id = project_id;
            let remote_machine_id = remote_project.remote_machine_id.clone();
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let result = background
                    .spawn(async move {
                        gpui_create_remote_project_workspace_terminal(
                            &config,
                            &target,
                            &remote_project,
                        )
                    })
                    .await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok((reference, plan)) => {
                        if this.gpui_app_modal_active_project_id().as_deref()
                            != Some(active_project_id.as_str())
                        {
                            return;
                        }
                        let key = GpuiRemoteAttachSessionKey::from(&reference);
                        this.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
                        this.open_gpui_remote_attach_terminal(
                            reference,
                            plan,
                            Some(requested_pane_id),
                            placement,
                            GpuiRemoteAttachOpenIntent::CreatedByThisAction,
                            cx,
                        );
                        this.refresh_gpui_remote_gxserver_presentation_in_background(
                            &remote_machine_id,
                        );
                    }
                    Err(message) => this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Terminal unavailable",
                        message.as_str(),
                        cx,
                    ),
                });
            })
            .detach();
            return;
        }
        /*
        CDXC:Workarea 2026-11-09 DECISION:
        User: a Cmd+D split from a session that belongs to a sidebar folder must create the new session inside that same folder. The Agents workspace mirrors the active sidebar group, and selecting a session outside it switched the view to the project's own group, so the split collapsed into a lone tab. The new session joins the active user-made group before it opens, as a create from that folder's own row does.
        */
        let active_subgroup = match self.gx_store.core.focus().active_group.as_ref() {
            Some(ghostex_gx_core::ActiveGroup::Subgroup { project, group_id })
                if project.to_workspace_project_id() == project_id =>
            {
                Some((project.clone(), group_id.clone()))
            }
            _ => None,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    if let Some(launch) = launch.as_ref() {
                        gpui_create_local_project_workspace_terminal_with_launch(
                            project_id.as_str(),
                            launch,
                        )
                    } else {
                        gpui_create_local_project_workspace_terminal(project_id.as_str())
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok((key, plan)) => {
                    if let Some((project, group_id)) = &active_subgroup
                        && project.project_id == key.project_id
                    {
                        this.gx_store_place_created_session_in_subgroup(
                            project,
                            &key.session_id,
                            group_id,
                            cx,
                        );
                    }
                    #[cfg(target_os = "windows")]
                    {
                        /*
                        The daemon operation has already committed both the row
                        and provider. A later project-focus change cannot turn a
                        success into an orphan: use the pane captured at click
                        time. If that exact pane was deleted, compensate only
                        while no presentation reconciliation has mapped the key.
                        */
                        let cleanup_key = key.clone();
                        let captured_pane_is_valid = this
                            .agents_workspace
                            .pane_can_accept_workspace_action(requested_pane_id);
                        let already_mapped =
                            this.local_workspace_session_mappings.contains_key(&key);
                        let materialized = if captured_pane_is_valid || already_mapped {
                            match placement {
                                AgentsWorkspaceNewTerminalPlacement::Tab => this
                                    .open_gpui_local_workspace_terminal(
                                        key,
                                        plan,
                                        requested_pane_id,
                                        true,
                                        cx,
                                    ),
                                AgentsWorkspaceNewTerminalPlacement::SplitRight
                                | AgentsWorkspaceNewTerminalPlacement::SplitBelow
                                | AgentsWorkspaceNewTerminalPlacement::BottomRow => this
                                    .open_gpui_local_workspace_terminal_in_new_leaf(
                                        key,
                                        plan,
                                        requested_pane_id,
                                        placement,
                                        cx,
                                    ),
                            }
                        } else {
                            false
                        };
                        if !materialized {
                            this.compensate_unmaterialized_created_workspace_terminal(&cleanup_key);
                        }
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        if this.gpui_app_modal_active_project_id().as_deref()
                            == Some(key.project_id.as_str())
                        {
                            match placement {
                                AgentsWorkspaceNewTerminalPlacement::Tab => {
                                    let _ = this.open_gpui_local_workspace_terminal(
                                        key,
                                        plan,
                                        requested_pane_id,
                                        true,
                                        cx,
                                    );
                                }
                                AgentsWorkspaceNewTerminalPlacement::SplitRight
                                | AgentsWorkspaceNewTerminalPlacement::SplitBelow
                                | AgentsWorkspaceNewTerminalPlacement::BottomRow => {
                                    let _ = this.open_gpui_local_workspace_terminal_in_new_leaf(
                                        key,
                                        plan,
                                        requested_pane_id,
                                        placement,
                                        cx,
                                    );
                                }
                            }
                        }
                    }
                }
                Err(message) => this.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Terminal unavailable",
                    message.as_str(),
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(crate) fn add_terminal_placeholder_tab_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusMode 2026-06-22-23:33:
        Cmd+T follows the shell surface that owns keyboard focus. Command-pane focus adds a command-only placeholder to the focused command group and never creates an Agents workspace tab; Agents-pane focus in Agents mode creates a selected Mounting terminal session in that focused Agents pane because no real process has started yet. Source, Kanban, Automate, and Manage main-surface focus remains out of scope for terminal creation.

        CDXC:FocusMode 2026-06-26-06:47:
        Command-pane Cmd+T requires an expanded visible command pane with a live focused source tab before allocating a placeholder. Collapsed or stale command focus must no-op like native `commandsPanel.isVisible` gating instead of expanding the hidden strip or using model-level stale-focus recovery.

        CDXC:Workarea 2026-09-20 WHY:
        Cmd+T from a focused Kanban, Automate or Docs surface adds its tab to the Agents column beside
        the view, which is where the companion's tab used to go. This supersedes the 2026-07-29 rules
        that restored and retargeted a companion first; a focused Browser main pane still owns the
        chord as New Browser Tab, and Source CEF focus still propagates it to code-server.
        */
        match self.shell_focus {
            ShellFocusTarget::CommandPane => {
                if focused_command_pane_create_split_hotkey_source(
                    self.shell_focus,
                    &self.command_pane,
                )
                .is_none()
                {
                    return;
                }
                let session_id = self.command_pane.add_session_to_focused_group();
                self.start_command_terminal_gxserver_attach_for_slot(
                    CommandTerminalBodyMountSlotId {
                        group_id: self.command_pane.focused_group,
                        session_id,
                    },
                    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                    None,
                    None,
                    None,
                    cx,
                );
                self.focus_command_pane(cx);
                self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
                    group_id: self.command_pane.focused_group,
                    session_id,
                });
                self.scroll_focused_command_active_tab();
                self.persist_shell_layout_state();
                cx.notify();
            }
            ShellFocusTarget::AgentsPane(pane_id) => {
                self.add_agents_registered_terminal_tab(pane_id, cx);
            }
            ShellFocusTarget::BrowserSurface | ShellFocusTarget::BrowserPane(_)
                if self.active_mode == TitlebarMode::Browser =>
            {
                self.add_browser_tab(window, cx);
            }
            ShellFocusTarget::ProjectEditorSurface(mode)
                if self.active_mode == mode
                    && matches!(
                        mode,
                        TitlebarMode::Kanban | TitlebarMode::Automate | TitlebarMode::Manage
                    ) =>
            {
                // A view with no tabs of its own adds one to the Agents column beside it, which is
                // where the companion's tab used to go.
                let pane_id = self.agents_workspace.focused_pane;
                self.focus_agents_pane(pane_id, cx);
                self.add_agents_registered_terminal_tab(pane_id, cx);
            }
            ShellFocusTarget::BrowserSurface
            | ShellFocusTarget::BrowserPane(_)
            | ShellFocusTarget::ProjectEditorSurface(_) => {}
        }
    }

    pub(crate) fn split_focused_terminal_from_hotkey(
        &mut self,
        direction: FocusedTerminalSplitDirection,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusMode 2026-06-22-23:33:
        Cmd+D/Cmd+Shift+D use live shell focus instead of remembered workspace focus. Agents-pane focus in Agents mode reuses the right/below mounting split helpers because a new terminal runtime has not launched yet; expanded command-pane focus follows native by coercing both directions to a command-only horizontal split, focuses the command pane, persists, and scrolls active command tabs.

        CDXC:FocusMode 2026-06-26-06:47:
        Command-pane split hotkeys may allocate only from an already-expanded visible command pane. Stale or collapsed command focus must no-op at the command branch while Agents-pane focus keeps its existing placeholder split behavior.
        */
        match self.shell_focus {
            ShellFocusTarget::CommandPane => {
                self.split_command_placeholder_terminal_from_hotkey(direction, cx);
            }
            ShellFocusTarget::AgentsPane(pane_id) => match direction {
                FocusedTerminalSplitDirection::Right => {
                    self.split_agents_registered_terminal_right(pane_id, cx);
                }
                FocusedTerminalSplitDirection::Down => {
                    self.split_agents_registered_terminal_below(pane_id, cx);
                }
            },
            ShellFocusTarget::BrowserSurface
            | ShellFocusTarget::BrowserPane(_)
            | ShellFocusTarget::ProjectEditorSurface(_) => {}
        }
    }

    pub(crate) fn split_command_placeholder_terminal_from_hotkey(
        &mut self,
        direction: FocusedTerminalSplitDirection,
        cx: &mut gpui::Context<Self>,
    ) {
        if focused_command_pane_create_split_hotkey_source(self.shell_focus, &self.command_pane)
            .is_none()
        {
            return;
        }

        let Some((group_id, session_id)) = self
            .command_pane
            .split_session_adjacent_to_focused_group(direction)
        else {
            return;
        };

        self.start_command_terminal_gxserver_attach_for_slot(
            CommandTerminalBodyMountSlotId {
                group_id,
                session_id,
            },
            COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
            None,
            None,
            None,
            cx,
        );
        self.focus_command_pane(cx);
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
        self.command_drop_feedback = None;
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }

    /// CDXC:Workarea 2026-09-27 DECISION:
    /// User: keep the Split Right shortcut (Option+Shift+D) after removing Split Right from the chat's More actions menu and the sidebar session menu. This is now the only Split Right; splitting the lone tab of the focused pane is a no-op inside the model.
    pub(crate) fn split_existing_agents_session_right(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(source_pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
            return;
        };
        if self.agents_workspace.split_tab_to_pane(
            source_pane_id,
            self.agents_workspace.focused_pane,
            session_id,
            WorkspaceDropZone::Right,
        ) {
            self.focus_agents_pane(self.agents_workspace.focused_pane, cx);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn split_agents_registered_terminal_right(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.create_registered_agents_terminal(
            pane_id,
            AgentsWorkspaceNewTerminalPlacement::SplitRight,
            cx,
        );
    }

    pub(crate) fn split_agents_registered_terminal_below(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.create_registered_agents_terminal(
            pane_id,
            AgentsWorkspaceNewTerminalPlacement::SplitBelow,
            cx,
        );
    }

    pub(crate) fn append_agents_registered_terminal_bottom_row(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.create_registered_agents_terminal(
            pane_id,
            AgentsWorkspaceNewTerminalPlacement::BottomRow,
            cx,
        );
    }

    pub(crate) fn merge_all_agents_tabs_for_pane(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_workspace.merge_all_tabs_into_pane(pane_id) {
            self.focus_shell_target(
                ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
                cx,
            );
            self.workspace_drop_feedback = None;
            self.workspace_split_drag = None;
            self.workspace_split_layout_metrics.clear();
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn merge_all_agents_tabs_from_hotkey(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:CommandPane 2026-06-22-13:17:
        Ctrl+Shift+M is scoped to an active Agents pane focus. Command-pane, Browser, Source, Kanban, Manage, and project-editor focus no-op so their tabs, placeholders, and command sessions cannot be folded into the Agents workspace merge path.
        */
        let ShellFocusTarget::AgentsPane(pane_id) = self.shell_focus else {
            return;
        };
        self.merge_all_agents_tabs_for_pane(pane_id, cx);
    }

    pub(crate) fn rotate_agents_panes_from_hotkey(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:FocusMode 2026-06-26-06:56:
        Command-palette `rotatePanesClockwise` uses the same focused-pane policy as native `handleNativeTerminalTitleBarAction`: command focus default-returns, Browser/project-editor focus no-ops, and only active Agents pane focus may rotate the Agents workspace. Successful rotation restores shell focus to the focused Agents pane, clears stale workspace drag/resize state, persists shell layout, and notifies.
        */
        let Some(pane_id) = apply_rotate_agents_panes_hotkey_model(
            self.active_mode,
            self.shell_focus,
            &mut self.agents_workspace,
        ) else {
            return;
        };
        self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        self.workspace_drop_feedback = None;
        self.workspace_split_drag = None;
        self.workspace_split_layout_metrics.clear();
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn rotate_agents_panes_for_pane(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pane_id) = self.agents_workspace.resolve_action_pane_id(pane_id) else {
            return;
        };
        self.agents_workspace.focus_pane(pane_id);
        if self.agents_workspace.rotate_panes_clockwise() {
            self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
            self.workspace_drop_feedback = None;
            self.workspace_split_drag = None;
            self.workspace_split_layout_metrics.clear();
            self.scroll_workspace_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }
}
