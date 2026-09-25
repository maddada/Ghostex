use serde_json::{Value, json};

use crate::GhostexGpuiApp;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct NativeSidebarAction {
    pub(crate) command: Value,
}

impl GhostexGpuiApp {
    pub(crate) fn handle_native_sidebar_action(
        &mut self,
        action: &NativeSidebarAction,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if action
            .command
            .get("type")
            .and_then(serde_json::Value::as_str)
            == Some("renameCollection")
        {
            if let Some(id) = action.command["collectionId"].as_str() {
                self.begin_native_collection_rename(id, window, cx);
            }
        } else if action.command["type"] == "openNotifications" {
            // The sidebar menu carries Notifications only while the bell is out of the row, so the
            // dropdown hangs from the menu button instead of the bell's last painted spot.
            if let Some(bounds) = self.native_sidebar.more_button_bounds.get() {
                self.titlebar_notification_bell_bounds.set(Some(bounds));
            }
            self.toggle_gpui_titlebar_notifications_popup(window, cx);
        } else if action.command["type"] == "selectSpace" {
            // The More menu's overflowing Spaces switch with the same slide-and-fade as the row.
            if let Some(id) = action.command["spaceId"].as_str() {
                self.select_native_space(id, cx);
            }
        } else if action.command["type"] == "renameGroup" {
            if let Some(id) = action.command["groupId"].as_str() {
                self.begin_native_sidebar_rename("group", id, window, cx);
            }
        } else if action.command["type"] == "confirmCloseGroup" {
            if let Some(id) = action.command["groupId"].as_str() {
                let id = id.to_owned();
                if let Some(group) =
                    self.native_sidebar.snapshot.as_ref().and_then(|snapshot| {
                        snapshot.groups.iter().find(|group| group.group_id == id)
                    })
                {
                    let detail = format!(
                        "This will close all {} sessions in {}.",
                        group.sessions.len(),
                        group.title
                    );
                    let answer = window.prompt(
                        gpui::PromptLevel::Warning,
                        "Close group?",
                        Some(&detail),
                        &["Cancel", "Close Group"],
                        cx,
                    );
                    cx.spawn(async move |app, cx| {
                        if answer.await == Ok(1) {
                            let _ = app.update(cx, |app, cx| {
                                app.dispatch_native_sidebar_command(
                                    json!({"type": "closeGroup", "groupId": id}),
                                    cx,
                                )
                            });
                        }
                    })
                    .detach();
                }
            }
        } else if action
            .command
            .get("type")
            .and_then(serde_json::Value::as_str)
            == Some("showMenu")
        {
            let coordinate = |key| {
                gpui::px(
                    action
                        .command
                        .get(key)
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0) as f32,
                )
            };
            let position = gpui::point(coordinate("x"), coordinate("y"));
            let scale = action
                .command
                .get("scale")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0) as f32;
            Self::show_native_sidebar_menu(&action.command["items"], position, scale, window, cx);
        } else {
            self.dispatch_native_sidebar_ui(action.command.clone(), cx);
        }
    }

    pub(crate) fn dispatch_native_sidebar_command(
        &mut self,
        message: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_native_sidebar_ui(json!({"type": "command", "message": message}), cx);
    }

    pub(crate) fn dispatch_native_sidebar_ui(
        &mut self,
        command: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:Sidebar 2026-09-21 WHY:
        // The list is not ready yet (the launch window: the sidebar's own state has not been read
        // back; until 2026-09-25 also while the runtime had not posted the HUD), so the renderer is drawing the loading
        // skeleton and every id in this command names a row of a list nobody has seen. Dropped
        // here, once and counted, rather than let through: the planners below would each decline
        // it and the fall-through would then hand a command nobody can perform to the end of the
        // dispatch (gx_store/sidebar_runtime_route.rs).
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store_drop_sidebar_command_before_ready(&command);
            return;
        }
        // A row's context menu is built for the row the user opened, and since M4c the store
        // builds it: the panel is filled in this frame instead of after a round trip through the
        // old runtime (gx_store/sidebar_menus.rs).
        if self.gx_store_answer_session_menu(&command, cx) {
            return;
        }
        // The launcher's account pages and a row's Switch Account flyout call the daemon from here
        // and fill the panel the menu already opened (gx_store/sidebar_accounts.rs).
        if self.gx_store_run_sidebar_accounts(&command, cx) {
            return;
        }
        // Git, worktree and Handoff / Export menu items (gx_store/git/actions.rs).
        if self.gx_store_run_sidebar_git(&command, cx) { return; }
        // A row on a REMOTE machine: its sleep, wake, close, fork, flags, snooze and Full Reload
        // are calls down that machine's tunnel, sent through the same function the old runtime's
        // bridge message reaches, and nothing local moves (gx_store/sidebar_remote.rs).
        if self.gx_store_run_sidebar_remote(&command, cx) {
            return;
        }
        // An action the store owns is performed here and goes no further: the old runtime resolved
        // the same ids and came straight back over the fixed native bridge, so sending it on would
        // run the action twice (gx_store/sidebar_actions.rs).
        if self.gx_store_run_sidebar_action(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_more_menu(&command, cx) {
            return;
        }
        // Sleep and wake call the daemon from here, so the command must not also reach the old
        // runtime: it would make the same call a second time (gx_store/sidebar_lifecycle.rs).
        if self.gx_store_run_sidebar_lifecycle(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_close(&command, cx) {
            return;
        }
        if self.gx_store_run_close_after_done(&command, cx) {
            return;
        }
        if self.gx_store_run_session_edit_command(&command, cx) {
            return;
        }
        if self.gx_store_run_group_sleep(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_fork(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_flags(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_modal(&command, cx) {
            return;
        }
        // The More menu's rows, a machine's Configure, the Space editor and a project header's Add
        // Worktree and History open an app modal and nothing else. Each is a RENDERER command at
        // the TOP level, which is the envelope piece 3d got wrong (gx_store/sidebar_open.rs).
        if self.gx_store_run_sidebar_open(&command, cx) {
            return;
        }
        // Snooze reads the clock and the local calendar here and posts the same two commands the
        // renderer posts; the call it sends back arrives at the arm below (gx_store/sidebar_snooze.rs).
        if self.gx_store_run_sidebar_snooze_action(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_snooze(&command, cx) {
            return;
        }
        // Full Reload is the sleep and the wake in order, and Split Right is a selection that
        // carries where the pane goes; both end in the lifecycle path above
        // (gx_store/sidebar_reload.rs).
        if self.gx_store_run_sidebar_reload(&command, cx) {
            return;
        }
        // A project's Full Reload and a user-made group's are that reload over a set, one row at a
        // time (gx_store/sidebar_reload.rs).
        if self.gx_store_run_sidebar_reload_set(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_split(&command, cx) {
            return;
        }
        // A drag writes an order rather than calling the daemon in the moment: the drop decides the
        // set and the order, and the message it posts edits the workspace session groups document
        // or sends the project's manual order (gx_store/sidebar_drag.rs). `moveSession` is a
        // RENDERER command and arrives at the top level; `createGroupFromSession` is wrapped.
        if self.gx_store_run_sidebar_session_move(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_order_write(&command, cx) {
            return;
        }
        // A PROJECT drag is one gesture across three documents: the project order, the collection
        // the row landed in, and a Space it was dropped on. All three are answered here, because
        // `moveGroup` joins the target's collection in the same gesture and a port that wrote only
        // the order would reorder the row and drop it out of its folder
        // (gx_store/project_docs.rs). Every payload in the family is a RENDERER command and
        // arrives at the top level, the membership menus included.
        if self.gx_store_run_project_move(&command, cx) {
            return;
        }
        // A Project Group's Rename, colour and Ungroup write the same collections document without
        // being moves, so they are answered beside them and go no further: the old runtime would
        // otherwise write and push the document a second time (gx_store/collection_menu.rs).
        if self.gx_store_run_collection_menu_edit(&command, cx) {
            return;
        }
        // A Space icon's Sleep Space names the Space's projects and awake rows, sleeps the views
        // those projects have open, and hands the rows to the bulk path below as one
        // `setSessionsSleeping` (gx_store/space_sleep.rs).
        if self.gx_store_run_sidebar_space_sleep(&command, cx) {
            return;
        }
        // The bulk menu, a collection's lifecycle items and a project's Sleep, Wake and Close
        // resolve their set here and fan out into the per-session actions above, paced when the
        // action is a sleep (gx_store/sidebar_bulk.rs).
        if self.gx_store_run_sidebar_batch(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_bulk(&command, cx) {
            return;
        }
        // Two of the menus' inputs live in client storage and are written by the handler this
        // command is on its way to; the cached copy is dropped so the redraw that follows reads
        // the new value instead of waiting out its second.
        self.gx_store_note_menu_host_write(&command);
        if command["type"] == "selectSession" && command["mode"] == "focus" {
            if let Some(session_id) = command["sessionId"].as_str() {
                crate::app::native_chat::diagnostics::focus_requested(session_id);
            }
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::SidebarRefresh,
                "gpui.sidebar.focusRequested",
                json!({"sessionId": command["sessionId"], "epochMs": crate::support_logs::temporary_epoch_ms()}),
            );
        }
        // A click on a row of a REMOTE machine, and its Split Right: the store acknowledges the
        // attention, performs the same `openRemoteSessionTerminal` the old runtime posted, and the
        // open's own tab selection moves the remote focus marks, so the command goes no further
        // (gx_store/sidebar_remote_focus.rs), for a machine this run has not streamed too.
        if let Some(plan) = self.gx_store_plan_remote_row_focus(&command) {
            self.gx_store_focus_remote_row(&command, &plan, cx);
            return;
        }
        // A click on a row of THIS computer: the page's half of it (the multi-selection cleared, an
        // open app modal closed) and the focus the runtime's `focusSession` used to make are both
        // performed by the store, so the five senders that post this command share ONE route with
        // no page in it (gx_store/sidebar_focus_route.rs).
        if self.gx_store_focus_local_row(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_create(&command, cx) {
            return;
        }
        if self.gx_store_claim_focus_command(&command, cx) {
            return;
        }
        self.stage_agent_launch_placeholder(&command, cx);
        // The Space a `selectSpace` is LEAVING, read before the intent below moves it: the restore
        // that follows only runs when the selection really changed (gx_store/space_switch.rs).
        let space_switch = self.gx_store_space_switch_before(&command);
        // A command that moves the sidebar's own state (collapse, Space, filters, hidden items,
        // selection) moves the Rust state here, and that IS its whole answer: nothing else has an
        // arm for any of them (gx_store/sidebar_ui_commands.rs).
        let ui_only = self.gx_store_note_sidebar_command(&command, cx);
        // Close Project is focus-moving work the page used to do on the message's way past: the
        // store names the session the close focuses, from the list it draws
        // (gx_store/sidebar_close_project.rs).
        let command = self.gx_store_add_close_project_successor(command);
        // What is left has no owner, or was the sidebar's own state (gx_store/sidebar_runtime_route.rs).
        self.gx_store_note_unanswered_sidebar_command(&command, ui_only);
        // The Space the switch landed on reopens the session it was last left on, from the list the
        // intent above has just rebuilt. It posts the same `focusSession` the page posted, after
        // the page has been told, so the order of the two messages is the one the runtime used
        // to see (gx_store/space_switch.rs).
        if let Some(before) = space_switch {
            self.gx_store_restore_space_switch_focus(before, cx);
        }
    }
}
