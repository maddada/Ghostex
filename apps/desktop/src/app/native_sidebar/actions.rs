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
        // A row's context menu is built for the row the user opened, and since M4c the store
        // builds it: the panel is filled in this frame instead of after a round trip through the
        // old runtime (gx_store/sidebar_menus.rs).
        if self.gx_store_answer_session_menu(&command, cx) {
            return;
        }
        // An action the store owns is performed here and goes no further: the old runtime resolved
        // the same ids and came straight back over the fixed native bridge, so sending it on would
        // run the action twice (gx_store/sidebar_actions.rs).
        if self.gx_store_run_sidebar_action(&command, cx) {
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
        if self.gx_store_run_sidebar_fork(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_flags(&command, cx) {
            return;
        }
        if self.gx_store_run_sidebar_modal(&command, cx) {
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
        let Some(service) = self.sidebar.clone() else {
            return;
        };
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
        self.stage_agent_launch_placeholder(&command, cx);
        // A command that moves the sidebar's own state (collapse, Space, filters, hidden items,
        // selection) moves the Rust state here, before it is sent on: the list is rebuilt from it
        // in the same frame, and the old projection keeps its own copy for the menus it owns until
        // M4c (gx_store/sidebar_ui_commands.rs).
        self.gx_store_note_sidebar_command(&command, cx);
        // A sidebar command can change focus in the runtime, so it must not be handled while the runtime still holds an older focus stamp than the store (gx_store/burst.rs).
        self.gx_store_flush_old_runtime_tell(cx);
        let script = format!("window.ghostexGpui.onNativeSidebarCommand({command}); undefined;");
        service.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }
}
