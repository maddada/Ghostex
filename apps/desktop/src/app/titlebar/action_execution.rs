// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds titlebar action link/browser-url opening and command-action terminal launching.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn open_gpui_titlebar_action_links(
        &mut self,
        links: &[GpuiTitlebarActionLink],
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:ProjectActions 2026-07-31-12:00:
        Terminal Actions open their saved links right after the command-pane
        launch. Integrated links reuse the same Browser tab path as renderer
        `openBrowserUrl` commands (same-origin reuse, otherwise a new loaded
        tab) so re-running an Action does not multiply tabs; external links go
        through the http/https-only OS open helper after the same toolbar
        normalization as typed addresses.
        */
        for link in links {
            match link.target {
                GpuiTitlebarActionLinkTarget::Integrated => {
                    self.open_browser_url_from_renderer_command(
                        GpuiSidebarOpenBrowserUrlMessage {
                            url: link.url.clone(),
                            reuse: GpuiBrowserRendererOpenReuse::Similar,
                            from_quick_header: false,
                            project_id: None,
                        },
                        window,
                        cx,
                    );
                }
                GpuiTitlebarActionLinkTarget::External => {
                    if let Some(url) = normalize_address(&link.url) {
                        let _ = gpui_open_external_http_url(&url);
                    }
                }
            }
        }
    }

    pub(crate) fn open_gpui_browser_action_url(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Browser Actions must enter the existing GPUI Browser tab/CEF path: switch to Browser, wake the Browser shell, load the saved URL into the active Browser tab, and let Browser surface machinery own navigation. Do not call OS open, shell commands, external browsers, persistent logs, or duplicate CEF surfaces from the titlebar action.
        */
        /*
        CDXC:DisabledPluginRouting 2026-08-23:
        Running a saved Browser Action is a deliberate click, so a Browser
        turned off in Settings → Customize owes the user the URL and a reason
        instead of an Action that appears to do nothing.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            self.copy_path_for_disabled_project_workarea(&url, "Browser", cx);
            return;
        }
        self.active_mode = TitlebarMode::Browser;
        self.set_shell_focus(ShellFocusTarget::BrowserPane(
            self.browser_tabs.focused_pane,
        ));
        self.set_browser_address_input_value_unchecked(
            self.browser_tabs.focused_pane,
            url.clone(),
            window,
            cx,
        );
        self.commit_browser_address(url, cx);
    }

    pub(crate) fn open_gpui_debug_command_action_terminal(
        &mut self,
        title: String,
        command: String,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandPane 2026-06-25-10:29:
        `runMode:"debug"` must match macOS Debug Action behavior: create a normal visible Agents workspace terminal titled `Debug: <Action>` and send the saved command as visible initial input with the Atuin-ignore prefix. Do not reuse command-pane tabs, post command-button run state, write command status files, or hide the wrapper process for debug runs.
        */
        let working_directory = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.in_memory_project_path.as_ref())
            .and_then(|path| path.to_str())
            .map(str::to_string);
        let payload = AgentsTerminalStartupExplicitLaunchPayload {
            working_directory,
            command: None,
            env_vars: Vec::new(),
            initial_input: Some(gpui_debug_command_action_initial_input(&command)),
            wait_after_command: false,
        };
        if payload.to_ghostty_launch_payload().is_err() {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Action unavailable",
                "GPUI could not prepare the debug Action terminal.",
                cx,
            );
            return;
        }

        let requested_pane_id = self.agents_workspace.focused_pane;
        let Some(session_id) = self
            .agents_workspace
            .add_mounting_session_to_pane(requested_pane_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Action unavailable",
                "GPUI could not create a debug Action terminal.",
                cx,
            );
            return;
        };
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.title = format!("Debug: {title}");
        }
        let pane_id = self.agents_workspace.focused_pane;
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .ensure_runtime_session_id(session_id);
        self.agents_terminal_startup_launch_payload_source
            .insert_explicit_payload_for_startup_key(
                runtime_session_id,
                session_id,
                AgentsTerminalStartupBodySlotId {
                    pane_id,
                    session_id,
                },
                payload,
            );
        self.active_mode = TitlebarMode::Agents;
        self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn open_gpui_command_action_terminal(
        &mut self,
        command_id: String,
        title: String,
        command: String,
        play_completion_sound: bool,
        close_terminal_on_exit: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Terminal Actions must create a real command-pane terminal startup path, not run a process from the titlebar. Insert command text only as an explicit launch payload for the newly selected command-pane body slot, use the active project snapshot path only when the sidebar supplied it, and keep run-state success/error feedback tied to the command Action lifecycle rather than titlebar-side command execution.

        CDXC:GPUICommandPane 2026-06-24-23:17:
        Sidebar/titlebar terminal Actions should mirror macOS command-pane startup by using Ghostty's launch command field for the wrapped zsh action process instead of pasting command text as visible initial input. The payload remains process-local and exact-slot keyed; command text is not logged, persisted, inferred from labels, or stored in shell-state JSON.

        CDXC:GPUICommandPane 2026-06-24-23:36:
        Re-running an action should reuse a matching idle command-pane tab instead of multiplying tabs. New or inactive reused tabs receive the wrapped command through the launch-payload boundary; an already mounted reused tab receives the same wrapper through the exact mounted command surface, with status reset driven by the session-state file.

        CDXC:GPUICommandPane 2026-06-24-23:49:
        GPUI command Actions now mirror macOS sidebar button feedback: post `running` for the selected run id immediately, then let the status-file poller post success/error and play the configured action completion sound when the wrapped command exits. The feedback path carries only command id, run id, state, exit code, and sound preference.

        CDXC:GPUICommandPane 2026-06-25-11:47:
        Command Actions open the hidden command pane through the same default-height rule as macOS sidebar Actions. Reset height only when the pane was hidden before selecting or creating the Action-owned tab; visible panes keep their live resize while the run metadata and launch payload update.

        CDXC:GPUICommandPaneActions 2026-06-26-04:59:
        Command-pane Action runs ignore the saved/requested close-on-exit flag at run start so the selected Action tab remains reusable after completion, matching native `runNativeSidebarCommand`. The parsed flag must not enter launch payloads, status files, shell-state JSON, logs, command text, cwd/env, terminal output, or project paths.

        CDXC:GPUICommandPaneActions 2026-06-27-01:45:
        Default terminal Actions select and reveal their command tab but keep the current shell first responder, matching native `focusAfterCreate: false`. Only explicit command-pane focus routes and Debug Actions may transfer typing focus.

        CDXC:GPUICommandPaneActions 2026-06-27-02:05:
        After Action run-start metadata is installed and sidebar run-state feedback is posted, GPUI must immediately refresh the cached sanitized `commandPaneSessions` bridge like native `runNativeSidebarCommand.publish()`. The bridge may carry only session ids, active/focus booleans, sanitized titles, semantic statuses, sleeping/timer fields, and sanitized action command ids; command text, cwd/env, run ids, status-file paths, terminal output, persisted shell data, and project paths must stay out.

        CDXC:GPUICommandPaneActions 2026-06-27-07:54:
        Default terminal Action execution is mutually exclusive like native: mounted idle reuse writes the staged wrapper to the exact current command surface and submits Return without enqueueing startup data, while created or unmounted Action tabs receive an exact-slot launch payload for first mount. Do not use a launch payload as fallback for a mounted reused shell.
        */
        self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
        let mut selection = self
            .command_pane
            .select_or_create_action_session(command_id.clone(), title.clone());
        if matches!(
            selection.kind,
            CommandPaneActionSessionSelectionKind::ReusedActive
        ) {
            /*
            CDXC:GPUICommandPaneActions 2026-08-27:
            Re-running an Action whose owner tab is still working restarts it:
            close that tab through the direct tab-close path (which kills the
            gxserver zmx session and the running command with it), then launch
            a fresh run in a replacement tab, replacing the old select-only
            no-op. Spam protection lives at the click source via the titlebar
            quick-action cooldown, not here.
            */
            if self.close_command_pane_tab(selection.group_id, selection.session_id, cx) {
                self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
                selection = self
                    .command_pane
                    .select_or_create_action_session(command_id.clone(), title.clone());
            }
        }
        let group_id = selection.group_id;
        let session_id = selection.session_id;
        if matches!(
            selection.kind,
            CommandPaneActionSessionSelectionKind::ReusedActive
        ) {
            /*
            CDXC:GPUICommandPaneActions 2026-08-09:
            The still-running owner tab could not be closed (or a second live
            owner claimed the same command id). Never write a second command
            into a process that is still running: select and reveal only.
            */
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            self.scroll_command_group_active_tab(group_id);
            self.scroll_focused_command_active_tab();
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }
        let slot_id = CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        };
        let run_id = create_gpui_command_action_run_id();
        let status_file_path = gpui_command_action_status_file_path(session_id);
        let delayed_send_cleared = self.clear_gpui_command_delayed_send_timer(session_id);
        let action_started = self.command_pane.mark_action_session_run_started(
            session_id,
            command_id.clone(),
            title,
            run_id.clone(),
            status_file_path.clone(),
            play_completion_sound,
            close_terminal_on_exit,
        );
        if delayed_send_cleared || action_started {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        self.refresh_gpui_command_close_after_done_timer_for_session(session_id, cx);
        self.dispatch_gpui_sidebar_command_run_state(
            &command_id,
            &run_id,
            GpuiSidebarCommandRunState::Running,
            cx,
        );
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        let execution_text = gpui_command_action_execution_text_for_current_backend(
            &command,
            &run_id,
            &status_file_path,
        );
        let mounted_reuse_surface_available = matches!(
            selection.kind,
            CommandPaneActionSessionSelectionKind::Reused
        ) && self
            .gpui_command_action_mounted_reuse_surface_available(slot_id);
        let wrote_to_mounted_reuse = mounted_reuse_surface_available
            && self.send_gpui_command_action_script_to_mounted_terminal(
                slot_id,
                &execution_text,
                &status_file_path,
                cx,
            );
        if gpui_command_action_should_insert_launch_payload(
            selection.kind,
            mounted_reuse_surface_available,
            wrote_to_mounted_reuse,
        ) {
            let action_title = self
                .command_pane
                .session(session_id)
                .map(|session| session.title.clone())
                .unwrap_or_else(|| COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string());
            let startup_text = gpui_command_action_startup_text(&execution_text, &status_file_path);
            self.start_command_terminal_gxserver_attach_for_slot(
                slot_id,
                action_title.clone(),
                Some(startup_text),
                Some(command_id.clone()),
                Some(action_title),
                cx,
            );
        }
        if gpui_command_pane_default_action_should_focus_command_pane() {
            self.focus_command_pane();
            self.request_command_terminal_text_focus_handoff(slot_id);
        }
        self.begin_titlebar_quick_action_button_cooldown(cx);
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        cx.notify();
    }
}
