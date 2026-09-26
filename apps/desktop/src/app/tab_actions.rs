// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: agents/command tab creation, splitting, context menus, close/sleep actions

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::context_menu::GpuiContextMenu;
use gpui::Bounds;
use gpui::Keystroke;
use gpui::Pixels;
use gpui::Window;

use crate::app::actions::*;
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
            support_logs::append_temporary(
                support_logs::GpuiSupportLog::TerminalFocus,
                "TEMP.remoteNewTerminal.requestReceived",
                serde_json::json!({ "source": "agentsWorkspace" }),
            );
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

    /// CDXC:Workarea 2026-09-05 SEE-ALSO:
    /// Chat and the focused-session shortcut use the same split_tab_to_pane operation as sidebar Split Right in workspace_events.rs, including its lone-tab no-op.
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

    pub(crate) fn open_browser_pane_in_external_browser(&self, pane_id: BrowserPaneId) {
        let Some(tab) = self.browser_tabs.active_tab_for_pane(pane_id) else {
            return;
        };
        let _ = gpui_open_external_http_url(&tab.url);
    }

    pub(crate) fn show_browser_tab_context_menu(
        &self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:ContextMenus 2026-06-22-11:27:
        Individual Browser tabs need an owned GPUI popup window at the right-click position. The menu is scoped to the clicked Browser pane and tab ids, contains only tab-level Select Tab and Close Tab commands, and relies on the existing Browser selection/close helpers so address sync, CEF visibility, split/reorder state, favicon runtime state, history, and last-tab address-only placeholder behavior stay unchanged.
        */
        let tab_exists = self
            .browser_tabs
            .find_leaf(pane_id)
            .is_some_and(|leaf| leaf.tab_group.has_tab(tab_id));
        if !tab_exists {
            return;
        }

        /*
        CDXC:Browser 2026-09-21 DECISION:
        User: a browser tab's right-click menu has Sleep, which sleeps that tab, and Sleep Browser,
        which sleeps every browser tab. Sleep Browser is the Browser view's own Sleep, which the
        view's tab used to carry before Browser stopped having a view tab. Each row is offered only
        while there is a page for it to drop. Supersedes the 2026-09-20 note that offered Sleep Tab
        alone.
        */
        let mut menu = GpuiContextMenu::new().menu(
            "Select Tab",
            Box::new(SelectBrowserTabInPane {
                pane_id: pane_id.0,
                tab_id: tab_id.0,
            }),
        );
        menu = menu.menu(
            if self.view_strip_tab_pinned(ViewStripTabKey::Browser(tab_id)) {
                "Unpin Tab"
            } else {
                "Pin Tab"
            },
            Box::new(ToggleGpuiViewStripTabPinned {
                mode_index: TitlebarMode::Browser.switcher_index(),
                browser_tab_id: Some(tab_id.0),
            }),
        );
        if self.browser_surfaces.contains_key(&tab_id) {
            menu = menu.menu(
                "Sleep",
                Box::new(SleepBrowserTabInPane {
                    pane_id: pane_id.0,
                    tab_id: tab_id.0,
                }),
            );
        }
        if !self.browser_surfaces.is_empty() {
            menu = menu.menu(
                "Sleep Browser",
                Box::new(SleepGpuiTitlebarView {
                    mode_index: TitlebarMode::Browser.switcher_index(),
                }),
            );
        }
        menu.menu(
            "Close Tab",
            Box::new(CloseBrowserTabInPane {
                pane_id: pane_id.0,
                tab_id: tab_id.0,
            }),
        )
        .show(position, window, cx);
    }

    pub(crate) fn show_command_tab_context_menu(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        _expand_pane: bool,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:ContextMenus 2026-06-22-11:31:
        Individual command-pane tabs, including collapsed-strip tabs, need an owned GPUI popup window at the right-click position. The menu is scoped to the clicked command group id and session id and contains macOS-style Close Left/Right/Others commands. Tab selection and collapsed-strip expansion stay on left-click activation, not right-click menu rows.

            CDXC:CommandPane 2026-06-25-11:20:
            Scoped command tab rows carry only group id, session id, and a fixed scope enum; they do not carry command text, paths, terminal output, or cross-pane identifiers.

            CDXC:ContextMenus 2026-06-25-14:13:
            Native command tab right-click menus filter out command-panel controls such as Pin/Unpin, Minimize, and Expand, and retain only per-session tab actions that are actually present in the native action payload before scoped Sleep/Close rows.

            CDXC:ContextMenus 2026-06-27-01:49:
            Native command-panel tabs receive only fixed panel action payloads, and Swift keeps primary tab context rows only when per-session actions are present in that payload. GPUI command-tab right-click menus therefore omit Rename Session, Delayed Send, and Close After Done rows here while preserving focused Rename/Close After Done dispatch and explicit Delayed Send modal/session-id routes.

            CDXC:ContextMenus 2026-06-25-14:19:
            Native tab context menus do not add a direct Close Tab row; direct close is hover/middle-click chrome, while right-click close commands start at Close Right, Close Left, and Close Other Tabs.

            CDXC:ContextMenus 2026-06-25-14:22:
            Native tab context menus do not add Select Tab or Expand Commands Panel rows. Opening a context menu must not select the clicked tab or expand a hidden command panel; those remain left-click tab activation behavior.

            CDXC:SessionSleep 2026-06-25-14:27:
            Native command-tab context menus offer Sleep scopes before Close scopes. GPUI resolves those rows against the clicked command group and marks command sessions sleeping without removing their tabs, content-derived titles, or group layout.

            CDXC:ContextMenus 2026-06-25-14:42:
            Native AppKit command-tab menus leave Sleep Right/Left/Others and Close Right/Left/Others enabled even when the clicked tab has no targets in that scope. Keep GPUI rows action-backed and let the scope resolver no-op on empty target lists instead of disabling rows.

            CDXC:ContextMenus 2026-06-27-01:49:
            Command-tab context menus have no primary per-session action block under native command-panel payloads. Do not add placeholder Fork, Reload, Pop Out, Rename, Delayed Send, or Close After Done rows to fill that gap.

            CDXC:FocusMode 2026-06-25-21:40:
            Command-tab Focus is now action-backed only for split command-pane groups with more than one visible awake owner. Place it before Sleep when eligible, matching native's Focus placement without adding fake Fork, Reload, or Pop Out behavior.

            CDXC:ContextMenus 2026-06-27-01:55:
            Native inserts the separator before Sleep only when `primaryTabContextMenuActions()` is non-empty. Command-panel payloads produce no primary actions, so GPUI must not insert an extra separator between eligible Focus and Sleep.
            */
        let tab_exists = self
            .command_pane
            .find_leaf(group_id)
            .is_some_and(|leaf| leaf.tab_group.has_session(session_id));
        if !tab_exists {
            return;
        }
        let clicked_tab_is_sleeping = self
            .command_pane
            .session(session_id)
            .is_some_and(|session| session.is_sleeping);
        let mut menu = GpuiContextMenu::new();

        if self
            .command_pane
            .tab_context_focus_row_index(group_id, session_id)
            .is_some()
        {
            menu = menu.menu(
                command_pane_tab_context_focus_label(),
                Box::new(FocusCommandPaneTab {
                    group_id: group_id.0,
                    session_id: session_id.0,
                }),
            );
        }

        for scope in command_pane_tab_context_sleep_order(clicked_tab_is_sleeping) {
            menu = menu.menu(
                command_pane_tab_context_sleep_scope_label(scope),
                Box::new(SleepCommandPaneTabsByScope {
                    group_id: group_id.0,
                    session_id: session_id.0,
                    scope: scope.action_value(),
                }),
            );
        }
        menu = menu.separator();

        for scope in command_pane_tab_context_scoped_close_order() {
            menu = menu.menu(
                command_pane_tab_context_close_scope_label(scope),
                Box::new(CloseCommandPaneTabsByScope {
                    group_id: group_id.0,
                    session_id: session_id.0,
                    scope: scope.action_value(),
                }),
            );
        }

        menu.show(position, window, cx);
    }

    pub(crate) fn select_command_pane_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        expand_pane: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-25-12:10:
        Collapsed-strip command-tab selection must match native hidden-open behavior: select the clicked command tab, restore the last pinned/floating mode, and reset height from the current Workspace default only while hidden. Expanded titlebar tab selection stays a pure tab focus change.

        CDXC:SessionSleep 2026-06-25-14:46:
        Native command-tab clicks wake sleeping command sessions immediately only when click-to-wake placeholders are disabled. With the default click-to-wake setting, tab selection stays layout-only and the sleeping body click performs wake.

        CDXC:Notifications 2026-06-25-19:58:
        Direct command-tab activation should also acknowledge an Attention command session like native tab/titlebar focus. Clear only the selected command session's Attention state; Working, Delayed Send, sleeping placeholders, and Agents activity keep their existing semantics.
        */
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let click_to_wake_enabled =
            command_pane_click_to_wake_sleeping_sessions_from_shared_settings(&settings_snapshot);
        let wake_on_tab_selection = command_pane_sleeping_tab_selection_wake_target(
            &self.command_pane,
            group_id,
            session_id,
            click_to_wake_enabled,
        )
        .is_some();
        let selected = if expand_pane {
            self.command_pane.select_session_in_group_for_hidden_open(
                group_id,
                session_id,
                command_pane_content_height(window),
                command_pane_default_height_px_from_shared_settings(&settings_snapshot),
            )
        } else {
            self.command_pane
                .select_session_in_group(group_id, session_id)
        };
        if !selected {
            return false;
        }
        self.command_pane
            .acknowledge_attention_for_session_activation(session_id);
        let woke_sleeping_session =
            wake_on_tab_selection && self.command_pane.set_session_sleeping(session_id, false);
        if woke_sleeping_session {
            let title = self
                .command_pane
                .session(session_id)
                .map(|session| session.title.clone())
                .unwrap_or_else(|| COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string());
            self.start_command_terminal_gxserver_attach_for_slot(
                CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                },
                title,
                None,
                None,
                None,
                cx,
            );
            self.refresh_gpui_command_close_after_done_timer_for_session(session_id, cx);
        }
        self.focus_command_pane(cx);
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        if woke_sleeping_session {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn wake_command_pane_session(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:SessionSleep 2026-06-25-14:27:
        With the default click-to-wake setting, selecting a sleeping command tab only makes that tab active; activating the sleeping command body wakes it. This mirrors native placeholder behavior and prevents right-click tab menus from recreating a command terminal surface early.
        */
        if !self.command_pane.set_session_sleeping(session_id, false) {
            return false;
        }
        let title = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
            .unwrap_or_else(|| COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string());
        self.start_command_terminal_gxserver_attach_for_slot(
            CommandTerminalBodyMountSlotId {
                group_id,
                session_id,
            },
            title,
            None,
            None,
            None,
            cx,
        );
        self.refresh_gpui_command_close_after_done_timer_for_session(session_id, cx);
        self.command_pane.focus_group(group_id);
        self.focus_command_pane(cx);
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn toggle_command_pane_focus_mode_for_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .command_pane
            .toggle_focus_mode_for_tab(group_id, session_id)
        {
            return false;
        }
        self.focus_command_pane(cx);
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn wake_focused_sleeping_command_placeholder_from_keystroke(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((group_id, session_id)) = focused_sleeping_command_placeholder_wake_target(
            self.shell_focus,
            &self.command_pane,
            keystroke,
        ) else {
            return false;
        };
        self.wake_command_pane_session(group_id, session_id, cx)
    }

    pub(crate) fn wake_focused_sleeping_agents_placeholder_from_keystroke(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((pane_id, session_id)) = focused_sleeping_agents_placeholder_wake_target(
            self.active_mode,
            self.shell_focus,
            &self.agents_workspace,
            keystroke,
        ) else {
            return false;
        };
        self.activate_agents_terminal_placeholder(pane_id, session_id, cx);
        true
    }

    pub(crate) fn sleep_focused_command_pane_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((group_id, session_id)) =
            focused_command_pane_sleep_target(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        self.sleep_command_pane_tabs_for_scope(
            group_id,
            session_id,
            CommandPaneTabSleepScope::Sleep,
            CommandPaneScopedTabMutationFocusPolicy::FocusCommandPane,
            cx,
        )
    }

    pub(crate) fn wake_focused_command_pane_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((group_id, session_id)) =
            focused_command_pane_wake_target(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        self.wake_command_pane_session(group_id, session_id, cx)
    }

    pub(crate) fn close_command_pane_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-07-10:
        macOS command-tab close parity is immediate: native closeTerminal
        removes the tab, tears down the surface, and kills the gxserver zmx
        session without a confirm prompt or surface close-request round trip.
        Close mutates the command model first; the render/bounds-driven host
        reconciliation then drops the stale engine record or Ghostty surface
        (CDXC:Terminal 2026-06-23-05:21 cleanup path).
        */
        if !self
            .command_pane
            .close_session_from_direct_tab_close(group_id, session_id)
        {
            return false;
        }
        self.forget_command_gxserver_session_for_closed_tab(session_id, cx);
        self.clear_command_resize_hover_state_if_command_pane_hidden();
        self.clear_gpui_command_delayed_send_timer(session_id);
        self.clear_gpui_command_close_after_done_timer(session_id);
        if self.command_pane.has_sessions() {
            self.focus_command_pane(cx);
        } else {
            self.restore_previous_non_command_focus_or_default(cx);
        }
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    /// CDXC:CommandPane 2026-09-23 DECISION:
    /// User: "make right middle clicking on the command pane tabs bar in an empty area just close all the terminals there". A middle-click on empty tab-bar space closes every tab that bar shows: one command group's tabs, or on the collapsed strip every Commands pane tab it lists (never the Terminal view's), each through the same close as the tab's own middle-click.
    pub(crate) fn close_all_command_pane_tabs_in_bar(
        &mut self,
        group_id: Option<CommandPaneGroupId>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let tabs = match group_id {
            Some(group_id) => self
                .command_pane
                .find_leaf(group_id)
                .map(|leaf| {
                    leaf.tab_group
                        .tabs
                        .iter()
                        .map(|tab| (group_id, tab.session_id))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            None => self.command_pane.panel_flat_tab_ids(),
        };
        let mut closed = false;
        for (group_id, session_id) in tabs {
            closed |= self.close_command_pane_tab(group_id, session_id, cx);
        }
        closed
    }

    pub(crate) fn close_command_pane_tabs_for_scope(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        scope: CommandPaneTabCloseScope,
        focus_policy: CommandPaneScopedTabMutationFocusPolicy,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-25-11:20:
        Bulk command-tab closes must reuse the same close ownership as single command tabs. Every resolved sibling tab is removed from the command model immediately (macOS command close parity, no close-request deferral); the target list is resolved before mutation so Close Left/Right/Others cannot drift while tabs are removed.

        CDXC:ContextMenus 2026-06-25-18:38:
        Scoped Close menu rows are lifecycle requests, not native tab-context primary actions. Do not transfer shell focus just because a GPUI popup menu scoped close removed sibling tabs; direct tab close and focused-session close still use the focus-restoring single-tab close path.
        */
        if scope == CommandPaneTabCloseScope::Close {
            return self.close_command_pane_tab(group_id, session_id, cx);
        }

        let session_ids = self
            .command_pane
            .tab_session_ids_for_close_scope(group_id, session_id, scope);
        if session_ids.is_empty() {
            return false;
        }

        let mut model_changed = false;
        for close_session_id in session_ids {
            if self.command_pane.close_session(group_id, close_session_id) {
                self.forget_command_gxserver_session_for_closed_tab(close_session_id, cx);
                self.clear_gpui_command_delayed_send_timer(close_session_id);
                self.clear_gpui_command_close_after_done_timer(close_session_id);
                model_changed = true;
            }
        }

        if model_changed {
            self.clear_command_resize_hover_state_if_command_pane_hidden();
            if focus_policy == CommandPaneScopedTabMutationFocusPolicy::FocusCommandPane {
                if self.command_pane.has_sessions() {
                    self.focus_command_pane(cx);
                } else {
                    self.restore_previous_non_command_focus_or_default(cx);
                }
            }
            self.scroll_command_group_active_tab(group_id);
            self.scroll_focused_command_active_tab();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
        }
        model_changed
    }

    pub(crate) fn sleep_command_pane_tabs_for_scope(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        scope: CommandPaneTabSleepScope,
        focus_policy: CommandPaneScopedTabMutationFocusPolicy,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:SessionSleep 2026-06-25-14:27:
        Sleeping command tabs is a lifecycle mutation, not a close. Mark the resolved clicked-group command sessions sleeping so tabs and layout remain intact, the body mount-slot list drops sleeping active sessions, and persistence records only safe enum/boolean state without command text, output, paths, process ids, status-file paths, or terminal content.

        CDXC:DelayedSend 2026-06-25-15:46:
        Scoped command-tab Sleep must not cancel Delayed Send or Close After Done. Preserve native's parked-session contract: Delayed Send remains session-owned and submits only if the tab is awake by the deadline, while Close After Done keeps only its armed intent until wake/Done refresh restarts countdown evaluation.

        CDXC:Sessions 2026-06-27-01:37:
        Sleeping a command tab preserves the Close After Done armed flag but clears any active runtime deadline immediately. The three-minute Done watcher must not keep counting down while the tab is sleeping; wake/Done refresh starts a fresh countdown.

        CDXC:ContextMenus 2026-06-25-18:38:
        Scoped Sleep menu rows dispatch through `paneTabSleepRequested` in native without first focusing the clicked terminal. Preserve GPUI shell focus for GPUI popup menu scoped sleep while focused command Sleep keeps command-pane focus ownership.
        */
        let session_ids = self
            .command_pane
            .tab_session_ids_for_sleep_scope(group_id, session_id, scope);
        if session_ids.is_empty() {
            return false;
        }

        let mut model_changed = false;
        for sleep_session_id in session_ids {
            if self
                .command_pane
                .set_session_sleeping(sleep_session_id, true)
            {
                model_changed = true;
            }
        }
        let close_after_done_timer_changed =
            self.prune_gpui_command_close_after_done_timers_for_command_model();

        if model_changed {
            if focus_policy == CommandPaneScopedTabMutationFocusPolicy::FocusCommandPane {
                self.focus_command_pane(cx);
            }
            self.scroll_command_group_active_tab(group_id);
            self.scroll_focused_command_active_tab();
            self.persist_shell_layout_state();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        if model_changed || close_after_done_timer_changed {
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
        }
        model_changed || close_after_done_timer_changed
    }

    pub(crate) fn sleep_command_pane_tabs_for_scope_from_action(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        scope_value: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scope) = CommandPaneTabSleepScope::from_action_value(scope_value) else {
            return;
        };
        self.sleep_command_pane_tabs_for_scope(
            group_id,
            session_id,
            scope,
            command_pane_tab_context_scoped_lifecycle_focus_policy(),
            cx,
        );
    }

    pub(crate) fn close_command_pane_tabs_for_scope_from_action(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        scope_value: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scope) = CommandPaneTabCloseScope::from_action_value(scope_value) else {
            return;
        };
        self.close_command_pane_tabs_for_scope(
            group_id,
            session_id,
            scope,
            command_pane_tab_context_scoped_lifecycle_focus_policy(),
            cx,
        );
    }

    pub(crate) fn focus_command_pane(&mut self, cx: &mut gpui::Context<Self>) {
        if self.command_pane.has_sessions() {
            self.remember_current_non_command_focus();
            self.focus_shell_target(ShellFocusTarget::CommandPane, cx);
            self.persist_shell_layout_state();
        }
    }

    pub(crate) fn command_pane_directional_focus_session_for_app_route(
        command_pane: &mut CommandPaneModel,
        target_group_id: Option<CommandPaneGroupId>,
    ) -> Option<CommandSessionId> {
        /*
        CDXC:FocusRouting 2026-06-25-23:35:
        Cmd-Opt directional focus into command panes must use a live expanded command-panel route. Specific command-group targets validate and focus that group; generic command-pane targets require the current focused group to still resolve, so stale shell focus never falls back to another command session.
        */
        if !command_pane.any_dock_visible() || !command_pane.has_sessions() {
            return None;
        }

        if let Some(group_id) = target_group_id {
            if !command_pane.group_dock_visible(group_id) {
                return None;
            }
            let active_session_id = command_pane
                .find_leaf(group_id)
                .and_then(|leaf| leaf.tab_group.active_session_id())?;
            if command_pane.session(active_session_id).is_none()
                || !command_pane.focus_group(group_id)
            {
                return None;
            }
        }

        if !command_pane.focused_group_dock_visible() {
            return None;
        }
        let (_group_id, session_id) = command_pane.focused_group_active_session_id()?;
        command_pane.session(session_id).map(|_| session_id)
    }

    pub(crate) fn focus_command_pane_directional_target(
        &mut self,
        target_group_id: Option<CommandPaneGroupId>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = Self::command_pane_directional_focus_session_for_app_route(
            &mut self.command_pane,
            target_group_id,
        ) else {
            return false;
        };

        self.focus_command_pane(cx);
        if self.shell_focus != ShellFocusTarget::CommandPane {
            return false;
        }
        self.request_focused_command_terminal_text_focus_handoff();

        /*
        CDXC:FocusRouting 2026-06-25-23:55:
        Cmd-Opt spatial and render-order focus into a live expanded command pane must reveal the focused active command tab in both the target command group and collapsed strip, matching other command activation paths. Collapsed, stale, or orphan command targets return before scrolling, persistence, sidebar refresh, or Attention acknowledgement.
        */
        self.scroll_focused_command_active_tab();

        let attention_acknowledged = self
            .command_pane
            .acknowledge_attention_for_session_activation(session_id);
        if attention_acknowledged {
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        }
        true
    }

    pub(crate) fn focus_project_editor_surface(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.active_mode == mode {
            if mode == TitlebarMode::Terminal {
                if self.seed_terminal_view_for_open(cx) {
                    self.focus_command_pane(cx);
                    self.request_focused_command_terminal_text_focus_handoff();
                }
                return;
            }
            self.mark_project_editor_mode_awake(mode, cx);
            let focus = match mode {
                TitlebarMode::Agents => {
                    ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane)
                }
                TitlebarMode::Browser => {
                    ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane)
                }
                TitlebarMode::Terminal => ShellFocusTarget::CommandPane,
                TitlebarMode::Source
                | TitlebarMode::Kanban
                | TitlebarMode::Automate
                | TitlebarMode::Manage
                | TitlebarMode::Extension(_) => ShellFocusTarget::ProjectEditorSurface(mode),
            };
            self.focus_shell_target(focus, cx);
            if mode == TitlebarMode::Browser {
                self.sync_active_browser_tab_to_surface(window, cx);
            } else {
                self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                self.update_active_mode_cef_child_visibility(cx);
            }
            self.persist_shell_layout_state();
        }
    }

    pub(crate) fn focus_project_editor_surface_for_keyboard(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.active_mode != mode {
            return false;
        }

        if mode == TitlebarMode::Terminal {
            self.focus_project_editor_surface(mode, window, cx);
            self.drain_pending_keyboard_handoff(window, cx);
            return self.shell_focus == ShellFocusTarget::CommandPane;
        }

        if !mode.is_project_editor_mode() {
            return false;
        }
        if self.project_editor_shell.is_mode_awake(mode) {
            /*
            CDXC:FocusRouting 2026-07-29-05:03:
            Left/right focus must transfer real keyboard ownership to the view, not only update shell border state. Browser focuses the current page surface; Source, Kanban, Automate, and Docs focus their exact project-workarea CEF surface after the ordinary shell/lifecycle transition.
            */
            if mode == TitlebarMode::Browser {
                let pane_id = self.browser_tabs.focused_pane;
                return self.focus_browser_content_for_pane(pane_id, window, cx);
            }

            self.focus_project_editor_surface(mode, window, cx);
            self.drain_pending_keyboard_handoff(window, cx);
            return self.shell_focus == ShellFocusTarget::ProjectEditorSurface(mode);
        }

        /*
        CDXC:FocusRouting 2026-06-22-09:44:
        Directional keyboard focus onto a selected sleeping project-editor main surface only updates shell focus and Browser visibility. It must not mark the lifecycle awake, refresh recency, create or sync a Browser CEF surface, or bypass the explicit click-to-wake body activation path.
        */
        let focus = match mode {
            TitlebarMode::Browser => ShellFocusTarget::BrowserSurface,
            TitlebarMode::Source
            | TitlebarMode::Kanban
            | TitlebarMode::Automate
            | TitlebarMode::Manage
            | TitlebarMode::Extension(_) => ShellFocusTarget::ProjectEditorSurface(mode),
            TitlebarMode::Agents | TitlebarMode::Terminal => return false,
        };
        self.focus_shell_target(focus, cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        self.shell_focus == focus
    }

    pub(crate) fn focused_command_pane_tab_cycle_target(
        shell_focus: ShellFocusTarget,
        command_pane: &CommandPaneModel,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:CommandPane 2026-06-25-23:20:
        Ctrl-Tab and Ctrl-Shift-Tab over command focus are live command-panel routes. Cycle only while the command pane is expanded and the stored focused command group still resolves, so collapsed command strips and stale command focus no-op instead of mutating hidden or fallback tabs.

        CDXC:CommandPane 2026-06-25-23:20:
        Keyboard cycling shares direct command-tab activation semantics: after a successful cycle, acknowledge only the selected Attention command session through the existing command attention path.
        */
        if shell_focus != ShellFocusTarget::CommandPane
            || !command_pane.focused_group_dock_visible()
        {
            return None;
        }
        command_pane.focused_group_active_session_id()
    }

    pub(crate) fn cycle_focused_command_pane_tab_for_app_route(
        shell_focus: ShellFocusTarget,
        command_pane: &mut CommandPaneModel,
        reverse: bool,
    ) -> Option<bool> {
        Self::focused_command_pane_tab_cycle_target(shell_focus, command_pane)?;
        if !command_pane.cycle_active_session(reverse) {
            return None;
        }

        let attention_acknowledged =
            if let Some((_group_id, session_id)) = command_pane.focused_group_active_session_id() {
                command_pane.acknowledge_attention_for_session_activation(session_id)
            } else {
                false
            };
        Some(attention_acknowledged)
    }

    pub(crate) fn cycle_focused_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let step_started = Instant::now();
        let changed = if self.shell_focus == ShellFocusTarget::CommandPane {
            if let Some(attention_acknowledged) = Self::cycle_focused_command_pane_tab_for_app_route(
                self.shell_focus,
                &mut self.command_pane,
                reverse,
            ) {
                self.scroll_focused_command_active_tab();
                if attention_acknowledged {
                    self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                }
                true
            } else {
                false
            }
        } else if matches!(self.shell_focus, ShellFocusTarget::AgentsPane(_))
            || self.open_view_mode().is_none()
        {
            let pane_id = match self.shell_focus {
                ShellFocusTarget::AgentsPane(pane_id) => pane_id,
                _ => self.agents_workspace.focused_pane,
            };
            // CDXC:FocusRouting 2026-09-19 WHY: the pane used to be focused before the step, which announced the tab being left as the selected session and left the tab the step landed on to whatever focus event came next. The step comes first, so the one selection this records (store focus, sidebar highlight, attention, the coalesced tell) is the tab now in front.
            let cycled = self.agents_workspace.cycle_tab_in_pane(pane_id, reverse);
            self.focus_agents_pane(pane_id, cx);
            cycled
        } else if self.active_mode == TitlebarMode::Browser
            && matches!(
                self.shell_focus,
                ShellFocusTarget::BrowserSurface | ShellFocusTarget::BrowserPane(_)
            )
        {
            /*
            CDXC:FocusRouting 2026-06-22-09:18:
            Ctrl-Tab in Browser mode is pane-local while split panes are shell-owned placeholders. Cycle only the focused Browser pane's loaded and address-only tab ids, then reuse Browser tab selection sync so the mode wakes, the shared address toolbar follows the selected tab, the focused loaded tab materializes if needed, already-created active loaded surfaces in other rendered Browser leaves stay visible, shell focus remains Browser, and shell state persists.
            */
            if self
                .browser_tabs
                .cycle_tab_in_focused_pane(reverse)
                .is_some()
            {
                self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
                self.focus_shell_target(
                    ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
                    cx,
                );
                if self.gx_store_key_is_held() {
                    // CDXC:Browser 2026-09-19 WHY: a held Previous/Next Tab in Pane now repeats (helpers/os_cli/keyboard_router.rs). Selecting a restored tab creates its CEF surface, which loads the page and starts a renderer process, so a hold over twenty restored tabs would start twenty. A held step moves only the selection (tab strip, address bar, and the surface of a tab that already has one); the surface of the tab the key is released on is created when the selection settles (gx_store/burst.rs).
                    let pane_id = self.browser_tabs.focused_pane;
                    let address_value = self.browser_tabs.address_value_for_pane(pane_id);
                    self.browser_url = address_value.clone();
                    self.set_browser_address_input_value(pane_id, address_value, window, cx);
                    self.update_active_mode_cef_child_visibility(cx);
                    self.gx_store_defer_browser_surface(cx);
                } else {
                    self.sync_active_browser_tab_to_surface(window, cx);
                }
                self.scroll_focused_browser_pane_active_tab();
                true
            } else {
                false
            }
        } else {
            false
        };

        if changed {
            // CDXC:FocusRouting 2026-07-04 WHY: keyboard tab cycling must end in the same
            // focus state as clicking the terminal body; before the unified handoff it only
            // moved the model and left the CEF sidebar as first responder.
            self.drain_pending_keyboard_handoff(window, cx);
            self.scroll_all_active_tab_strips();
            self.persist_shell_layout_state();
            cx.notify();
            self.gx_store_log_tab_step(step_started, reverse);
        }
    }

    pub(crate) fn close_focused_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-06-22-06:02:
        Cmd-W is surface-aware in the GPUI placeholder shell. Command focus closes the active command placeholder, Browser surface focus closes the active browser tab, Agents mode closes the active workspace tab, and Source/Kanban/Automate/Docs never close the project-editor surface itself.

        CDXC:Terminal 2026-06-26-23:59:
        Cmd-W in Agents delegates to the same close helper as pane-tab close. Mapped workspace sessions bypass Ghostty close-confirm and go through the store's lifecycle (formerly SidebarApp's), while unmapped exact mounted Running surfaces can still request `ghostty_surface_request_close` before shell removal.

        CDXC:Terminal 2026-06-23-05:21:
        Cmd-W with command-pane focus must match command tab close parity: an exact current mounted command surface gets a Ghostty close request and stays in the command model until a confirmed close callback is consumed. Non-mounted command placeholders continue to close through the existing command shell model.

        CDXC:CommandPane 2026-06-25-17:37:
        Cmd-W over command-pane focus must use the same clicked command-tab close path as hover, middle-click, scoped menus, and Close After Done. That shared helper owns mounted close requests, timer cleanup, final-panel focus restore, shell persistence, and sidebar refresh.

        CDXC:FocusMode 2026-06-27-02:58:
        Keep the executable Cmd-W route aligned with the pure focused-close decision helper so native parity stays testable without a GPUI window: command focus wins first, BrowserSurface or exact BrowserPane focus closes Browser tabs, and main project-editor surface focus no-ops.

        CDXC:Workarea 2026-09-20 WHY:
        An Agents pane owns Cmd-W whatever the view panel shows, because it is on screen either way. This supersedes the 2026-07-29 rule that gave the chord to a focused companion session.
        */
        match focused_surface_close_decision(self.shell_focus, self.active_mode, &self.command_pane)
        {
            FocusedSurfaceCloseDecision::CloseCommandTab {
                group_id,
                session_id,
            } => {
                self.close_command_pane_tab(group_id, session_id, cx);
            }
            FocusedSurfaceCloseDecision::InterceptNoOp | FocusedSurfaceCloseDecision::NoOp => {}
            FocusedSurfaceCloseDecision::CloseAgentsActiveTab => {
                let pane_id = self.agents_workspace.focused_pane;
                if let Some(session_id) = self
                    .agents_workspace
                    .find_leaf(pane_id)
                    .and_then(|leaf| leaf.tab_group.active_session_id())
                {
                    self.close_agents_tab(pane_id, session_id, cx);
                }
            }
            FocusedSurfaceCloseDecision::CloseBrowserActiveTab => {
                if let Some(tab_id) = self.browser_tabs.active_tab().map(|tab| tab.id) {
                    let changed = self.close_browser_tab_model(tab_id, window, cx);
                    if changed {
                        self.persist_shell_layout_state();
                        cx.notify();
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_agents_focus_mode(&mut self, cx: &mut gpui::Context<Self>) {
        self.toggle_agents_focus_mode_for_pane(self.agents_workspace.focused_pane, cx);
    }

    pub(crate) fn toggle_agents_focus_mode_for_pane(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pane_id) = self.agents_workspace.resolve_action_pane_id(pane_id) else {
            return;
        };
        self.agents_workspace.focus_pane(pane_id);

        if self.agents_workspace.toggle_focus_mode() {
            self.focus_shell_target(
                ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
                cx,
            );
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    /// CDXC:FocusRouting 2026-09-20 WHY:
    /// One candidate set covers the whole workarea now that the Agents column and an open view are on
    /// screen together, so Cmd+Alt+Left and Right cross the split divider by geometry like any other
    /// pane boundary. This supersedes the 2026-07-29 rule that Left from a view restored a hidden
    /// companion: the sessions column is always there to move into.
    pub(crate) fn focus_workspace_direction(
        &mut self,
        direction: WorkspaceFocusDirection,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-07-05:
        Keyboard directional focus and placeholder body activation are separate intents. Cmd-Alt focus may select a sleeping project-editor main placeholder without waking Source, Browser, Kanban, Automate, or Docs, while explicit body activation remains the path that wakes the selected surface.
        */
        let changed = match self.focus_workspace_direction_spatial(direction, window, cx) {
            SpatialFocusOutcome::Focused => true,
            SpatialFocusOutcome::NoTarget => false,
            SpatialFocusOutcome::BoundsUnavailable => {
                self.focus_workspace_direction_by_render_order(direction, window, cx)
            }
        };

        if changed {
            cx.notify();
        }
    }

    pub(crate) fn focus_workspace_direction_spatial(
        &mut self,
        direction: WorkspaceFocusDirection,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> SpatialFocusOutcome {
        if !self.spatial_focus_bounds_ready() {
            return SpatialFocusOutcome::BoundsUnavailable;
        }

        let Some((current_target, current_bounds)) = self.current_spatial_focus_bounds() else {
            return SpatialFocusOutcome::BoundsUnavailable;
        };
        let candidates = self.spatial_focus_candidates();
        let Some(target) =
            nearest_spatial_focus_target(current_bounds, current_target, direction, &candidates)
        else {
            return SpatialFocusOutcome::NoTarget;
        };

        if self.focus_spatial_target(target, window, cx) {
            SpatialFocusOutcome::Focused
        } else {
            SpatialFocusOutcome::NoTarget
        }
    }

    /// Every pane the pointer-free directional focus can land on has to have reported bounds for this
    /// frame, or the caller falls back to render order.
    pub(crate) fn spatial_focus_bounds_ready(&self) -> bool {
        let pane_ids = self.agents_workspace.rendered_leaf_order();
        if pane_ids.is_empty()
            || !pane_ids
                .iter()
                .all(|pane_id| self.workspace_leaf_layout_bounds.contains_key(pane_id))
        {
            return false;
        }

        self.command_spatial_focus_bounds_ready()
    }

    /// CDXC:FocusRouting 2026-09-23 DECISION:
    /// User: Option+Cmd+Arrows must not go to the view panel, only between the Commands pane and the agent sessions in their split panes, so the sessions can be walked by keyboard. The view panel is not a candidate; a focus that is already inside it still moves out of it to the nearest pane in that direction.
    pub(crate) fn spatial_focus_candidates(&self) -> Vec<FocusCandidate> {
        let mut candidates = Vec::new();
        for pane_id in self.agents_workspace.rendered_leaf_order() {
            if let Some(bounds) = self.workspace_leaf_layout_bounds.get(&pane_id).copied() {
                candidates.push(FocusCandidate {
                    target: SpatialFocusTarget::AgentsPane(pane_id),
                    bounds,
                    order: candidates.len(),
                });
            }
        }

        self.append_command_spatial_focus_candidates(&mut candidates);

        candidates
    }

    /// The command groups spatial focus can move between: those in a dock that is on screen.
    pub(crate) fn visible_command_focus_group_ids(&self) -> Vec<CommandPaneGroupId> {
        self.command_pane
            .group_order()
            .into_iter()
            .filter(|group_id| self.command_pane.group_dock_visible(*group_id))
            .collect()
    }

    pub(crate) fn command_spatial_focus_bounds_ready(&self) -> bool {
        if !self.command_pane.any_dock_visible() || !self.command_pane.has_sessions() {
            return true;
        }

        let command_group_ids = self.visible_command_focus_group_ids();
        if command_group_ids.is_empty() {
            return self.command_pane_layout_bounds.is_some();
        }

        command_group_ids
            .iter()
            .all(|group_id| self.command_group_layout_bounds.contains_key(group_id))
            || self.command_pane_layout_bounds.is_some()
    }

    pub(crate) fn append_command_spatial_focus_candidates(
        &self,
        candidates: &mut Vec<FocusCandidate>,
    ) {
        if self.command_pane.any_dock_visible() && self.command_pane.has_sessions() {
            let command_group_ids = self.visible_command_focus_group_ids();
            let command_group_candidates: Option<Vec<_>> = command_group_ids
                .iter()
                .map(|group_id| {
                    self.command_group_layout_bounds
                        .get(group_id)
                        .copied()
                        .map(|bounds| (*group_id, bounds))
                })
                .collect();

            if let Some(command_group_candidates) = command_group_candidates {
                for (group_id, bounds) in command_group_candidates {
                    candidates.push(FocusCandidate {
                        target: SpatialFocusTarget::CommandPaneGroup(group_id),
                        bounds,
                        order: candidates.len(),
                    });
                }
            } else if let Some(bounds) = self.command_pane_layout_bounds {
                candidates.push(FocusCandidate {
                    target: SpatialFocusTarget::CommandPane,
                    bounds,
                    order: candidates.len(),
                });
            }
        }
    }

    pub(crate) fn current_spatial_focus_bounds(
        &self,
    ) -> Option<(SpatialFocusTarget, Bounds<Pixels>)> {
        match self.shell_focus {
            ShellFocusTarget::AgentsPane(pane_id) => self
                .workspace_leaf_layout_bounds
                .get(&pane_id)
                .copied()
                .map(|bounds| (SpatialFocusTarget::AgentsPane(pane_id), bounds)),
            ShellFocusTarget::CommandPane if self.command_pane.focused_group_dock_visible() => {
                self.current_command_spatial_focus_bounds()
            }
            ShellFocusTarget::ProjectEditorSurface(mode) if self.active_mode == mode => self
                .project_editor_surface_bounds_for_mode(mode)
                .map(|bounds| (SpatialFocusTarget::ProjectEditorSurface(mode), bounds)),
            ShellFocusTarget::BrowserPane(pane_id) if self.active_mode == TitlebarMode::Browser => {
                self.browser_leaf_layout_bounds
                    .get(&pane_id)
                    .copied()
                    .map(|bounds| (SpatialFocusTarget::BrowserPane(pane_id), bounds))
                    .or_else(|| self.browser_surface_spatial_focus_bounds())
            }
            ShellFocusTarget::BrowserSurface if self.active_mode == TitlebarMode::Browser => {
                self.browser_surface_spatial_focus_bounds()
            }
            _ => None,
        }
    }

    /// The Browser view's own bounds: its focused leaf while it is awake, else the surface slot.
    fn browser_surface_spatial_focus_bounds(&self) -> Option<(SpatialFocusTarget, Bounds<Pixels>)> {
        let mode = TitlebarMode::Browser;
        if self.project_editor_shell.is_mode_awake(mode) {
            let pane_id = self.browser_tabs.focused_pane;
            if let Some(bounds) = self.browser_leaf_layout_bounds.get(&pane_id).copied() {
                return Some((SpatialFocusTarget::BrowserPane(pane_id), bounds));
            }
        }
        self.project_editor_surface_bounds_for_mode(mode)
            .map(|bounds| (SpatialFocusTarget::ProjectEditorSurface(mode), bounds))
    }

    pub(crate) fn current_command_spatial_focus_bounds(
        &self,
    ) -> Option<(SpatialFocusTarget, Bounds<Pixels>)> {
        if !self.command_pane.focused_group_dock_visible() {
            return None;
        }

        let group_id = self.command_pane.focused_group;
        self.command_group_layout_bounds
            .get(&group_id)
            .copied()
            .map(|bounds| (SpatialFocusTarget::CommandPaneGroup(group_id), bounds))
            .or_else(|| {
                self.command_pane_layout_bounds
                    .map(|bounds| (SpatialFocusTarget::CommandPane, bounds))
            })
    }

    pub(crate) fn focus_spatial_target(
        &mut self,
        target: SpatialFocusTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match target {
            SpatialFocusTarget::AgentsPane(pane_id) => {
                self.focus_agents_pane(pane_id, cx);
                true
            }
            SpatialFocusTarget::BrowserPane(pane_id) => {
                self.focus_browser_pane(pane_id, window, cx)
            }
            SpatialFocusTarget::ProjectEditorSurface(mode) => {
                self.focus_project_editor_surface_for_keyboard(mode, window, cx)
            }
            SpatialFocusTarget::CommandPane => self.focus_command_pane_directional_target(None, cx),
            SpatialFocusTarget::CommandPaneGroup(group_id) => {
                self.focus_command_pane_directional_target(Some(group_id), cx)
            }
        }
    }

    pub(crate) fn focus_workspace_direction_by_render_order(
        &mut self,
        direction: WorkspaceFocusDirection,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let targets = self.workspace_render_order_focus_targets();
        if targets.is_empty() {
            return false;
        }

        let current_target = match self.shell_focus {
            ShellFocusTarget::CommandPane => {
                let focused_group_target =
                    SpatialFocusTarget::CommandPaneGroup(self.command_pane.focused_group);
                if targets.contains(&focused_group_target) {
                    Some(focused_group_target)
                } else if targets.contains(&SpatialFocusTarget::CommandPane) {
                    Some(SpatialFocusTarget::CommandPane)
                } else {
                    None
                }
            }
            ShellFocusTarget::AgentsPane(pane_id) => Some(SpatialFocusTarget::AgentsPane(pane_id)),
            // The view panel is not in the order (see `spatial_focus_candidates`): leave it for the
            // focused agent pane.
            _ => {
                self.focus_agents_pane(self.agents_workspace.focused_pane, cx);
                return true;
            }
        };

        let Some(current_target) = current_target else {
            self.focus_agents_pane(self.agents_workspace.focused_pane, cx);
            return true;
        };
        render_order_focus_target(&targets, Some(current_target), direction)
            .map(|target| self.focus_spatial_target(target, window, cx))
            .unwrap_or(false)
    }

    pub(crate) fn workspace_render_order_focus_targets(&self) -> Vec<SpatialFocusTarget> {
        workspace_render_order_focus_targets(
            self.agents_workspace.rendered_leaf_order(),
            self.command_pane.any_dock_visible(),
            self.command_pane.has_sessions(),
            self.visible_command_focus_group_ids(),
        )
    }

    pub(crate) fn project_editor_surface_bounds_for_mode(
        &self,
        mode: TitlebarMode,
    ) -> Option<Bounds<Pixels>> {
        self.project_editor_surface_layout_bounds
            .filter(|focus_bounds| focus_bounds.mode == mode)
            .map(|focus_bounds| focus_bounds.bounds)
    }

    pub(crate) fn prepare_focus_bounds_for_render(
        &mut self,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Terminal 2026-06-22-20:29:
        Agents terminal mount-slot bounds are App-owned runtime geometry only. Clear them with the other per-render focus/layout bounds so future libghostty native views attach to the current body rectangle below the tab bar without persisting, logging, or retaining stale pane/session geometry.

        CDXC:Terminal 2026-06-22-21:27:
        This per-render bounds clear is a pre-layout measurement reset, not a terminal slot removal. Host sync must preserve the focused running Agents slot while awaiting the body canvas record, and only the runtime bounds map should be empty during that interval.
        */
        self.workspace_leaf_layout_bounds.clear();
        self.browser_leaf_layout_bounds.clear();
        self.command_group_layout_bounds.clear();
        self.command_terminal_mount_slot_bounds.clear();
        self.command_pane_layout_bounds = None;
        self.project_editor_surface_layout_bounds = None;
        self.agents_terminal_mount_slot_bounds.clear();
        self.agents_terminal_startup_body_slot_geometries.clear();
        self.agents_terminal_parked_owner_body_slot_geometries
            .clear();
        // Prune (do not clear) the persistent zmx-refresh bounds maps: a slot
        // that stops being rendered loses its entry here, so its next body
        // record counts as a fresh surfacing and triggers the conditional
        // refresh; a continuously rendered slot keeps its entry, so per-frame
        // records stay refresh-free (CDXC:Zmx 2026-07-11).
        if self.agents_workspace_visible() {
            let rendered = self.agents_workspace.rendered_terminal_body_mount_slots();
            self.agents_terminal_zmx_refresh_recorded_bounds
                .retain(|slot_id, _| rendered.contains(slot_id));
        } else {
            self.agents_terminal_zmx_refresh_recorded_bounds.clear();
        }
        {
            let rendered = self.command_pane.rendered_terminal_body_mount_slots();
            self.command_terminal_zmx_refresh_recorded_bounds
                .retain(|slot_id, _| rendered.contains(slot_id));
        }
        self.sync_agents_terminal_surface_host(scale_factor, cx);
        self.sync_command_terminal_surface_host(scale_factor, cx);
        self.sync_gpui_engine_agents_chat_eligibility(cx);
    }
}
