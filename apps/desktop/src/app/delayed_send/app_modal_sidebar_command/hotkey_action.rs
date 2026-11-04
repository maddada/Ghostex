//! The command palette's `runGhostexHotkeyAction` bridge and the Setup (onboarding) row.

use gpui::Window;

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_hotkey_action_command(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            // CDXC:Onboarding 2026-09-15 DECISION:
            // The Tips dropdown's "Setup" button opens the Onboarding modal, the same one the automatic
            // first run opens (modals/modal_window.rs); the older setup modal was deleted on 2026-09-27. Quick Access's
            // Setup Ghostex row reaches this arm, so it must open the same modal as the native Tips header
            // action in titlebar/settings_and_action_state.rs.
            "openWorkspaceWelcome" => {
                self.open_gpui_app_modal_from_titlebar(GpuiAppModalKind::Onboarding, window, cx);
            }
            "runGhostexHotkeyAction" => {
                let Some(action_id) = command.get("actionId").and_then(serde_json::Value::as_str)
                else {
                    return;
                };
                // The New Thread picker is a native GPUI window, not an app-modal page.
                if action_id == "openNewThreadPalette" {
                    self.toggle_gpui_new_thread_picker(cx);
                    return;
                }
                if action_id == "createAgentSession" {
                    self.start_new_agent_session(cx);
                    return;
                }
                // focus/terminal_focus_hotkeys.rs
                if action_id == "unfocusTerminal" {
                    self.unfocus_terminal(window, cx);
                    return;
                }
                if action_id == "focusLastTerminal" {
                    self.focus_last_terminal(window, cx);
                    return;
                }
                if gpui_focused_chat_hotkey_action_id(action_id) {
                    self.run_focused_chat_hotkey(action_id, window, cx);
                    return;
                }
                if action_id == "readAloudStop" {
                    crate::app::gx_chat::read_aloud_stop();
                    return;
                }
                /*
                CDXC:FocusMode 2026-06-25-15:01:
                The shared command palette posts focused-session commands as `runGhostexHotkeyAction`. Handle command-pane Sleep/Wake/Close focused-session ids directly in GPUI before modal routing so command-palette rows operate on the shell-focused command tab instead of no-oping or trying to open another modal.

                CDXC:DelayedSend 2026-06-27-06:37:
                The shared Delayed Send row is also a focused-pane action, but native command terminals consume it through the command-panel titlebar default no-op. GPUI must consume the id before generic modal routing without opening the focused command-pane timer modal.

                CDXC:Sessions 2026-06-25-15:24:
                The shared Close After Done row is also a focused command-terminal action. In GPUI command panes it toggles the focused mounted command tab's terminal-scoped watcher before modal routing, matching native command-palette behavior without applying the timer to Agents, Browser, or project-editor focus.

                CDXC:CommandPane 2026-06-25-16:33:
                Rename Active Session is also a focused command-terminal action. When the command pane owns shell focus, open the shared Rename Session modal for the active command tab instead of falling through to unrelated app-modal commands.

                CDXC:CommandPalette 2026-06-25-17:32:
                The shared command palette sends focused-pane split/open/merge actions through the same `runGhostexHotkeyAction` bridge as focused-session actions. Route the supported GPUI pane actions to the existing shell hotkey helpers before modal routing so command-pane focus can create command splits and Browser opens without requiring a separate keybinding event.

                CDXC:CommandPalette 2026-06-26-07:24:
                Command-palette Create Session is ordinary focused hotkey behavior in GPUI. Dispatch it to the same Cmd+T helper before app-modal routing so command-pane focus and Agents-pane focus keep their existing source gates and placeholder semantics.

                CDXC:CommandPalette 2026-06-26-07:24:
                Shared workarea switch rows also arrive as hotkey actions. Route them before app-modal fallback through `switch_workarea_from_hotkey` so command-palette selection uses the same titlebar availability checks, no-wake lifecycle, focus target, Browser visibility, and persistence behavior as Option+1..5.

                CDXC:CommandPalette 2026-06-26-07:36:
                Command-palette focus-navigation rows are shell navigation, not app-modal commands. Route tab cycling and directional focus through the same GPUI keyboard helpers as direct hotkeys so command-pane, Agents, Browser, and project-editor focus keep their existing source gates and layout semantics.

                CDXC:CommandPalette 2026-06-26-10:04:
                Shared previous/next group focus is render-order navigation, not spatial arrow focus. Dispatch `focusPreviousGroup` and `focusNextGroup` directly through the existing render-order workspace traversal only from Agents-pane or command-pane focus so GPUI moves like native focusAdjacentGroup without adding numbered group slots, project jumps, or fallback guessing.

                CDXC:CommandPalette 2026-06-26-10:04:
                Command-palette Start Action 1-5 rows are positional titlebar Actions hotkeys. Dispatch them through the existing titlebar action index runner so GPUI executes the configured project action without adding renderer payloads containing command text, URLs, paths, or session data.

                CDXC:Sidebar 2026-06-26-10:04:
                `toggleSidebarCollapsed` is shell chrome, not a modal command. Route it before app-modal fallback so the command-palette row and Cmd+B hide or restore the GPUI sidebar and divider while preserving the expanded sidebar width.

                CDXC:CommandPalette 2026-09-21 WHY:
                Numbered session-slot rows (`focusSessionSlot1..9`) resolve against the drawn row order: the store resolves and focuses the Nth drawn row when its list is drawn (gx_store/sidebar_session_slot.rs), and with the switch off they are delegated to SidebarApp as nativeHotkey messages. Previous/Next Session walk the native sidebar's rows in Rust (gx_store/session_walk.rs), which supersedes their delegation of 2026-06-26-23:20. Previous/Next Tab in Pane stays on GPUI tab-cycle routing, and jump-to-project ids must not enter this bounce path because SidebarApp forwards those back to native.

                CDXC:Hotkeys 2026-09-21 WHY:
                Project jump rows resolve against the drawn project order. With the store's list drawn the store resolves and performs the whole jump (gx_store/sidebar_slot_jump.rs); with the switch off they still go to SidebarApp as the dedicated `gpuiProjectSlotHotkey` host message, never `nativeHotkey`, which SidebarApp would forward back to GPUI. Supersedes the 2026-06-26-23:42 note that SidebarApp always resolved them.
                */
                if self.run_gpui_terminal_toolbar_hotkey_action(action_id, window, cx) {
                    return;
                }
                if action_id == "openModelPicker" {
                    self.request_focused_session_model_picker(window, cx);
                    return;
                }
                if action_id == "toggleChatView" {
                    /*
                    CDXC:SessionChat 2026-07-31:
                    Chat View toggling must work while the terminal is hidden
                    behind the chat surface, so it resolves the focused Agents
                    session directly instead of requiring a focused terminal
                    view like the other toolbar actions.
                    */
                    gpui_component::Root::hide_tooltip(window, cx);
                    self.toggle_agents_session_chat_mode_for_focused_session(cx);
                    return;
                }
                if let Some(mode) = gpui_command_palette_switch_workarea_hotkey_mode(action_id) {
                    self.switch_workarea_from_hotkey(mode, window, cx);
                    return;
                }
                if let Some(index) = gpui_titlebar_view_hotkey_index(action_id) {
                    /*
                    CDXC:Hotkeys 2026-09-20 DECISION:
                    User (screen 07): "⌥1–9 jumps to a view ... Follows the order of the tabs in this
                    panel." So the numbers walk the open tab strip, and a number past the last tab
                    falls through to the view in that position in Settings' own order, which is what
                    opens a view that is not open yet. This supersedes the 2026-09-09 rule that they
                    followed the titlebar's displayed view list, because that list is gone.
                    */
                    let tabs = self.strip_view_tabs();
                    if let Some(mode) = tabs.get(index).copied() {
                        self.switch_workarea_from_hotkey(mode, window, cx);
                        return;
                    }
                    if let Some(item) = self.titlebar_mode_switcher_items().get(index) {
                        self.switch_workarea_from_hotkey(item.mode, window, cx);
                    }
                    return;
                }
                if let Some(action_index) = gpui_command_palette_action_slot_index(action_id) {
                    self.run_configured_gpui_titlebar_action_index(action_index, window, cx);
                    return;
                }
                if let Some(direction) =
                    navigation_history::navigation_history_hotkey_direction(action_id)
                {
                    /*
                    CDXC:Navigation 2026-08-19:
                    Back/Forward is shell navigation, not an app-modal command,
                    and it is owned by the navigation history controller
                    (navigation_history/controller.rs; the sidebar runtime until
                    2026-09-25): the keypress takes the exact same route as a
                    click on the titlebar arrows, unless a focused Browser pane
                    takes it (navigate_focused_browser_history).
                    */
                    if self.navigate_focused_browser_history(direction == "back", cx) {
                        return;
                    }
                    self.request_navigation_history_navigation(direction, cx);
                    return;
                }
                if let Some(command) =
                    notification_feed::notification_feed_hotkey_command(action_id)
                {
                    if command == "open" {
                        self.toggle_gpui_titlebar_notifications_popup(window, cx);
                    } else {
                        self.request_notification_feed_command(
                            command,
                            None,
                            ghostex_gx_core::NotificationFeedTab::All,
                            cx,
                        );
                    }
                    return;
                }
                if action_id == "toggleSidebarCollapsed" {
                    // With Claude on, showing the sidebar leaves Claude (app/claude_web/takeover.rs).
                    self.toggle_sidebar_from_user(cx);
                    return;
                }
                if action_id == "toggleViewPanel" {
                    self.toggle_view_panel(window, cx);
                    return;
                }
                // Toggle Voice Agent does what the header's Voice button does.
                if action_id == "toggleVoiceAgent" {
                    self.toggle_voice_agent_conversation(cx);
                    return;
                }
                // The headset arrow's "This session" row (CDXC:VoiceMode in voice_agent/header_button.rs).
                // Hold to Talk: its key coming up ends the hold (voice_agent/hold_to_talk.rs).
                if action_id == "holdToTalk" {
                    self.hold_to_talk_hotkey_pressed(cx);
                    return;
                }
                if action_id == "voiceCallAboutSession" {
                    self.voice_call_about_this_session(window, cx);
                    return;
                }
                if action_id == "expandViewPanel" {
                    self.toggle_view_panel_maximized(cx);
                    return;
                }
                if action_id == "expandViewPanelFully" {
                    self.toggle_view_panel_fully_expanded(cx);
                    return;
                }
                if action_id == "openExtensions" {
                    self.open_gpui_settings_extensions_page(Some(window), cx);
                    return;
                }
                if action_id == "openGhostexHelp" {
                    self.show_gpui_titlebar_help_menu(window, cx);
                    return;
                }
                if action_id == "openFileInFiles" {
                    self.native_docs_open_file_prompt(window, cx);
                    return;
                }
                if let Some(tab_cycle_action) =
                    gpui_command_palette_tab_cycle_hotkey_action(action_id)
                {
                    self.cycle_focused_tab(tab_cycle_action.reverse(), window, cx);
                    return;
                }
                if let Some(direction) =
                    gpui_command_palette_adjacent_group_focus_direction(action_id)
                {
                    if gpui_command_palette_adjacent_group_focus_source_allowed(self.shell_focus)
                        && self.focus_workspace_direction_by_render_order(direction, window, cx)
                    {
                        cx.notify();
                    }
                    return;
                }
                if let Some(direction) =
                    WorkspaceFocusDirection::from_command_palette_directional_focus_action_id(
                        action_id,
                    )
                {
                    self.focus_workspace_direction(direction, window, cx);
                    return;
                }
                match gpui_focused_pane_hotkey_action(action_id) {
                    Some(GpuiFocusedPaneHotkeyAction::CreateSession) => {
                        self.add_terminal_placeholder_tab_from_hotkey(window, cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::OpenCommandsPanel) => {
                        self.open_command_pane_from_command_palette(window, cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::OpenBrowserPane) => {
                        self.add_browser_tab_from_hotkey(window, cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::SplitSessionRight) => {
                        if let Some(session_id) = self.focused_agents_workspace_shell_session_id() {
                            self.split_existing_agents_session_right(session_id, cx);
                        }
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::SplitRight) => {
                        self.split_focused_terminal_from_hotkey(
                            FocusedTerminalSplitDirection::Right,
                            cx,
                        );
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::SplitDown) => {
                        self.split_focused_terminal_from_hotkey(
                            FocusedTerminalSplitDirection::Down,
                            cx,
                        );
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::MergeAllTabs) => {
                        self.merge_all_agents_tabs_from_hotkey(cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::RotatePanesClockwise) => {
                        self.rotate_agents_panes_from_hotkey(cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(runtime_action)) => {
                        match runtime_action {
                            GpuiFocusedPaneRuntimeAction::ForkSession => {
                                if let Some(shell_session_id) =
                                    self.focused_agents_workspace_shell_session_id()
                                {
                                    let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                                        "forkSession",
                                        shell_session_id,
                                        cx,
                                    );
                                }
                            }
                            GpuiFocusedPaneRuntimeAction::ReloadSession => {
                                if let Some(shell_session_id) =
                                    self.focused_agents_workspace_shell_session_id()
                                {
                                    let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                                        "fullReloadSession",
                                        shell_session_id,
                                        cx,
                                    );
                                } else {
                                    let _ = self.reload_focused_view(window, cx);
                                }
                            }
                            GpuiFocusedPaneRuntimeAction::PopOutPane => {}
                        }
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::CommandSession(command_action)) => {
                        match command_action {
                            GpuiCommandPaneFocusedSessionHotkeyAction::Rename => {
                                if !self.open_gpui_rename_session_modal_for_focused_command_pane(cx)
                                    && !self.rename_in_focused_view(window, cx)
                                {
                                    let _ = self
                                        .open_gpui_rename_session_modal_for_focused_agents_session(
                                            cx,
                                        );
                                }
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::DelayedSend => {
                                if !self.open_gpui_delayed_send_modal_for_focused_command_pane(cx) {
                                    let _ = self
                                        .open_gpui_delayed_send_modal_for_focused_agents_session(
                                            cx,
                                        );
                                }
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::CloseAfterDone => {
                                if !self
                                    .toggle_gpui_command_close_after_done_for_focused_command_pane(
                                        cx,
                                    )
                                {
                                    let _ = self
                                        .toggle_gpui_close_after_done_for_focused_agents_session(
                                            cx,
                                        );
                                }
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::Sleep => {
                                if !self.sleep_focused_command_pane_session(cx) {
                                    let _ = self.sleep_focused_view(cx);
                                }
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::Wake => {
                                self.wake_focused_command_pane_session(cx);
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::Close => {
                                if focused_command_pane_close_target(
                                    self.shell_focus,
                                    &self.command_pane,
                                )
                                .is_some()
                                    || self.focused_view_mode().is_some()
                                {
                                    self.close_focused_surface(window, cx);
                                }
                            }
                        }
                        return;
                    }
                    None => {}
                }
                if let Some(reverse) = gpui_sidebar_session_walk_hotkey_reverse(action_id) {
                    self.walk_native_sidebar_sessions(reverse, cx);
                    return;
                }
                if let Some(slot_number) =
                    gpui_command_palette_session_slot_hotkey_number(action_id)
                {
                    // The store resolves the Nth drawn row and focuses it as a click would
                    // (gx_store/sidebar_session_slot.rs). Nothing else is told: the page that used
                    // to answer `nativeHotkey` is deleted, and the message had no listener left.
                    self.gx_store_run_session_slot_hotkey(slot_number, cx);
                    return;
                }
                if let Some(slot_number) =
                    gpui_command_palette_project_slot_hotkey_number(action_id)
                {
                    // The store plans and performs the whole jump (gx_store/sidebar_slot_jump.rs).
                    // Nothing else is told, for the same reason as the session slot above.
                    self.gx_store_run_project_slot_hotkey(slot_number, cx);
                    return;
                }
                if let Some(position) = gpui_space_slot_hotkey_number(action_id) {
                    self.go_to_native_space(position, cx);
                    return;
                }
                let Some(modal) = gpui_app_modal_kind_for_hotkey_action_id(action_id) else {
                    return;
                };
                let sidebar_state_message =
                    self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
                let mut open_message = modal.open_message();
                if modal.requires_sidebar_state() {
                    open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
                }
                self.open_gpui_app_modal_window(
                    modal,
                    open_message,
                    sidebar_state_message,
                    None,
                    cx,
                );
            }
            _ => {}
        }
    }
}
