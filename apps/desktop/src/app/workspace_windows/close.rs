//! Closing a workspace window while another stays open. Closing the last one is a quit and keeps
//! the quit path (main.rs).
//!
//! CDXC:AppWindows 2026-10-01 WHY:
//! A closed window is gone for good (its slot is forgotten), so what only it held goes with it: its Commands panel and Terminal view shells are closed in gxserver, and a remote Action tab's session is closed on its machine, rather than left running where no window shows them; its Delayed Sends are cancelled. The window asks first when it holds any of them, and a remote close goes down another window's tunnel to that machine when there is one, else down this window's own, which is why the window waits for those closes (a few seconds at most) before its tunnels go. Agent sessions are untouched: they live in gxserver and every window's sidebar lists them. A hand-started Keep Awake moves to the window that takes over. Every window it opened over itself closes with it (its dialogs and popups, the chats' maximized composer, image viewer and other pop-ups, the Docs panels, and on macOS anything else AppKit has attached to it), and the Linux caption X asks the same question as the window manager's close. Its terminals detach their zmx clients with terminal sync stopped, so the detach is not read as the sessions exiting (`GPUI_APP_QUIT_IN_PROGRESS` does the same for a quit).

use std::time::Duration;

use gpui::{AnyWindowHandle, App, Context, Task, WeakEntity, Window};

use super::registry::{
    hand_over_app_keep_awake, note_workspace_window_closing, other_workspace_window_apps,
    several_workspace_windows_open,
};
use crate::app::helpers::*;
use crate::*;

/// The longest a closing window waits for its remote Action sessions to close on their machines.
const REMOTE_ACTION_CLOSE_WAIT: Duration = Duration::from_secs(8);

pub(super) fn install_workspace_window_close_handler(
    app: WeakEntity<GhostexGpuiApp>,
    window: &mut Window,
    cx: &mut App,
) {
    window.on_window_should_close(cx, move |window, cx| {
        app.update(cx, |app, cx| app.workspace_window_should_close(window, cx))
            .unwrap_or(true)
    });
}

impl GhostexGpuiApp {
    /// The user asked to close this window (its close button, Cmd+W on the window, or the Linux
    /// caption X). Returns whether it closes now; `false` while the confirmation is up or its
    /// remote closes run, after which it closes itself.
    pub(crate) fn workspace_window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !several_workspace_windows_open() {
            return true;
        }
        if self.workspace_window_closing {
            // Already closing: its remote Action sessions are still being closed.
            return false;
        }
        let command_terminals = self.workspace_window_command_terminal_keys(window).len()
            + self.workspace_window_remote_action_sessions(window).len();
        let delayed_sends = self.agents_delayed_send_timers.len()
            + self.agents_send_when_stopped_watchers.len()
            + self.command_delayed_send_timers.len();
        if command_terminals == 0 && delayed_sends == 0 {
            self.prepare_workspace_window_close(window, cx);
            return true;
        }
        let mut losses = Vec::new();
        if command_terminals > 0 {
            losses.push(if command_terminals == 1 {
                "its Commands terminal will close".to_string()
            } else {
                format!("its {command_terminals} Commands terminals will close")
            });
        }
        if delayed_sends > 0 {
            losses.push(if delayed_sends == 1 {
                "its Delayed Send will be cancelled".to_string()
            } else {
                format!("its {delayed_sends} Delayed Sends will be cancelled")
            });
        }
        let detail = format!(
            "If you close this window, {}. Agent sessions keep running and stay in the sidebar of every window.",
            losses.join(" and ")
        );
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            "Close this window?",
            Some(&detail),
            &["Cancel", "Close Window"],
            cx,
        );
        let handle = gpui::Window::window_handle(window);
        cx.spawn(async move |this, cx| {
            if answer.await != Ok(1) {
                return;
            }
            let remote_closes = handle
                .update(cx, |_, window, cx| {
                    this.update(cx, |app, cx| {
                        // Another window may have closed while the question was up.
                        if !several_workspace_windows_open() {
                            return Vec::new();
                        }
                        app.workspace_window_closing = true;
                        note_workspace_window_closing(
                            gpui::Window::window_handle(window).window_id(),
                        );
                        app.close_workspace_window_remote_actions(window, cx)
                    })
                    .unwrap_or_default()
                })
                .unwrap_or_default();
            if !remote_closes.is_empty() {
                let closes = futures::future::join_all(remote_closes);
                let timeout = cx.background_executor().timer(REMOTE_ACTION_CLOSE_WAIT);
                futures::future::select(Box::pin(closes), Box::pin(timeout)).await;
            }
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |app, cx| {
                    if several_workspace_windows_open() {
                        app.prepare_workspace_window_close(window, cx);
                    }
                });
                window.remove_window();
            });
        })
        .detach();
        false
    }

    /// Closes the windows this one owns, closes its Commands shells, lets go of the app-wide work,
    /// its remote tunnels and its gxserver socket, and stops its terminal sync and its saves.
    fn prepare_workspace_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for key in self.workspace_window_command_terminal_keys(window) {
            self.close_command_gxserver_session_in_background(key, cx);
        }
        self.workspace_window_closing = true;
        let handle = gpui::Window::window_handle(window);
        note_workspace_window_closing(handle.window_id());
        self.close_gpui_titlebar_popup(None, window, cx);
        self.close_floating_reveal(cx);
        self.close_workspace_window_chat_and_docs_windows(cx);
        if self.is_lead_window() {
            self.release_ghostex_capture(cx);
            hand_over_app_keep_awake(self.keep_awake_runtime.take());
        }
        for handle in [
            self.app_modal_window.map(Into::into),
            self.app_toast_window.map(Into::into),
            self.titlebar_popup_window.map(Into::into),
            self.plugins_modal_window.map(Into::into),
            self.new_thread_picker_window.map(Into::into),
            self.native_app_modal
                .as_ref()
                .map(|modal| modal.window.into()),
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            self.cef_component_window.map(Into::into),
        ]
        .into_iter()
        .flatten()
        {
            close_owned_window(handle, cx);
        }
        crate::app::window::frosted_host::hide_frosted_hosts_over(handle, cx);
        #[cfg(target_os = "macos")]
        close_windows_attached_to(handle, cx);
        self.stop_all_gpui_remote_gxserver_connections();
        self.source_code_server_runtime.stop();
        self.gx_store_disconnect_for_window_close();
    }

    /// The chats' own windows (a maximized composer, the image viewer, a table preview, the rewind,
    /// Save as Markdown and context editor dialogs, menus and pills) and the Docs view's drawer,
    /// notes composer, selection toolbar and format bar, which would otherwise outlive this window.
    fn close_workspace_window_chat_and_docs_windows(&mut self, cx: &mut Context<Self>) {
        let chats = self
            .native_chat_views
            .values()
            .chain(
                self.parked_agents_chat_runtimes_by_project
                    .values()
                    .flat_map(|parked| parked.native_views.values()),
            )
            .cloned()
            .collect::<Vec<_>>();
        for chat in chats {
            chat.update(cx, |chat, cx| chat.dismiss_windows_for_hidden_pane(cx));
        }
        self.native_docs_hide_toolbar_host(cx);
        self.native_docs_place_composer_window(None, cx);
        self.native_docs_place_format_bar_window(None, cx);
        if let Some(drawer) = self.native_docs.drawer.take() {
            crate::app::helpers::set_docs_drawer_glass_window(None);
            close_owned_window(drawer.window.into(), cx);
        }
        if let Some(board) = self.native_kanban_take_detached_window() {
            close_owned_window(board, cx);
        }
    }

    /// Starts closing this window's remote Action sessions on their machines. Returns the closes
    /// that go down this window's own tunnel, which the window waits for before it closes; the
    /// ones sent down another window's tunnel run on their own.
    fn close_workspace_window_remote_actions(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<Task<()>> {
        let references = self.workspace_window_remote_action_sessions(window);
        self.command_remote_action_sessions.clear();
        let others = other_workspace_window_apps(cx.entity_id());
        let mut own_closes = Vec::new();
        for reference in references {
            let machine = reference.remote_machine_id.as_str();
            let other_target = others
                .iter()
                .find_map(|other| other.read(cx).gpui_remote_gxserver_request_target(machine));
            let (target, own) = match other_target {
                Some(target) => (target, false),
                None => match self.gpui_remote_gxserver_request_target(machine) {
                    Some(target) => (target, true),
                    // Not connected anywhere: the session stays in that machine's Running
                    // Sessions, as a closed tab's does (command_pane_remote_action.rs).
                    None => continue,
                },
            };
            let close = cx.background_executor().spawn(async move {
                gpui_close_remote_command_action_session(&target, &reference)
            });
            if own {
                own_closes.push(close);
            } else {
                close.detach();
            }
        }
        own_closes
    }

    /// The gxserver sessions behind this window's Commands panel and Terminal view tabs: the live
    /// pane's and the ones parked for its other projects.
    fn workspace_window_command_terminal_keys(
        &self,
        window: &Window,
    ) -> Vec<GpuiLocalWorkspaceSessionKey> {
        let mut keys = self
            .command_gxserver_session_mappings
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for model in self.parked_workspace_window_command_panes(window) {
            keys.extend(command_gxserver_session_mappings_from_command_model(&model).into_values());
        }
        keys.sort_by(|left, right| {
            (&left.project_id, &left.session_id).cmp(&(&right.project_id, &right.session_id))
        });
        keys.dedup();
        keys
    }

    /// The remote sessions behind this window's remote Action tabs, live and parked.
    fn workspace_window_remote_action_sessions(
        &self,
        window: &Window,
    ) -> Vec<GpuiRemoteAttachSessionReference> {
        let mut references = self
            .command_remote_action_sessions
            .values()
            .cloned()
            .chain(
                command_remote_action_sessions_from_command_model(&self.command_pane).into_values(),
            )
            .collect::<Vec<_>>();
        for model in self.parked_workspace_window_command_panes(window) {
            references
                .extend(command_remote_action_sessions_from_command_model(&model).into_values());
        }
        let mut unique = Vec::new();
        for reference in references {
            if !unique.contains(&reference) {
                unique.push(reference);
            }
        }
        unique
    }

    /// The Commands panes this window keeps parked for its other projects.
    fn parked_workspace_window_command_panes(&self, window: &Window) -> Vec<CommandPaneModel> {
        let content_height = command_pane_content_height(window);
        let default_height = command_pane_default_height_px_from_shared_settings(
            &shared_settings::shared_sidebar_settings_snapshot(),
        );
        self.parked_command_panes_by_project
            .values()
            .filter_map(|pane| {
                command_pane_model_from_shell_state_with_default_height_px(
                    pane,
                    content_height,
                    default_height,
                )
            })
            .collect()
    }
}

fn close_owned_window(handle: AnyWindowHandle, cx: &mut App) {
    let _ = handle.update(cx, |_, window, _| window.remove_window());
}

/// Closes every window AppKit has attached to `parent` (directly or through another child) that
/// the explicit list above did not name: a safety net for a window something opened over this one.
#[cfg(target_os = "macos")]
fn close_windows_attached_to(parent: AnyWindowHandle, cx: &mut App) {
    unsafe extern "C" {
        fn GhostexGpuiWindowIsAttachedTo(
            child: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
        ) -> bool;
    }
    let Some(parent_view) = parent
        .update(cx, |_, window, _| {
            crate::app::native_chat::child_window::window_native_view(window)
        })
        .ok()
        .flatten()
    else {
        return;
    };
    for handle in cx.windows() {
        if handle == parent {
            continue;
        }
        let attached = handle
            .update(cx, |_, window, _| {
                crate::app::native_chat::child_window::window_native_view(window).is_some_and(
                    |child| unsafe { GhostexGpuiWindowIsAttachedTo(child, parent_view) },
                )
            })
            .unwrap_or(false);
        if attached {
            close_owned_window(handle, cx);
        }
    }
}
