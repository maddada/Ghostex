//! Open and sidebar bridge plumbing for the native New Space / Edit Space dialog.
//! SEE-ALSO: apps/desktop/src/app/window/space_editor_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/sidebar_dispatch/runtime_settings_and_host_messages.rs (`forward_gpui_sidebar_space_editor_result_to_sidebar`, the bridge arm this calls).
use crate::app::window::*;
use crate::*;

/// The open message's routing fields, kept by the host closure so the result
/// carries the same ids the React page posted.
struct GpuiSpaceEditorContext {
    mode: SpaceEditorMode,
    space_id: Option<String>,
    member_collection_id: Option<String>,
    member_project_id: Option<String>,
    remote_machine_id: Option<String>,
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `sidebarSpaceEditor` modal kind. Edit mode has to name a Space; create
    /// mode must not, or Save would patch whichever Space id was left on the
    /// message (the React host throws on the same payloads).
    pub(crate) fn open_gpui_space_editor_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let non_empty = |key: &str| text(key).filter(|value| !value.trim().is_empty());
        let mode = if message.get("mode").and_then(serde_json::Value::as_str) == Some("edit") {
            SpaceEditorMode::Edit
        } else {
            SpaceEditorMode::Create
        };
        let space_id = non_empty("spaceId");
        if mode == SpaceEditorMode::Edit && space_id.is_none() {
            return;
        }
        let context = GpuiSpaceEditorContext {
            mode,
            space_id: (mode == SpaceEditorMode::Edit)
                .then_some(space_id)
                .flatten(),
            member_collection_id: (mode == SpaceEditorMode::Create)
                .then(|| non_empty("memberCollectionId"))
                .flatten(),
            member_project_id: (mode == SpaceEditorMode::Create)
                .then(|| non_empty("memberProjectId"))
                .flatten(),
            remote_machine_id: non_empty("remoteMachineId"),
        };
        let config = SpaceEditorModalConfig {
            mode,
            initial_name: text("spaceName"),
            initial_icon: text("spaceIcon"),
            initial_color: text("spaceColor"),
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_space_editor_modal_command(&context, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::SidebarSpaceEditor,
            SPACE_EDITOR_MODAL_WIDTH,
            SPACE_EDITOR_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiSpaceEditorModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Posts the same `sidebarSpaceEditorResult` the React page did and lets
    /// the sidebar apply it to the current Space document.
    fn handle_gpui_space_editor_modal_command(
        &mut self,
        context: &GpuiSpaceEditorContext,
        command: SpaceEditorModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::SidebarSpaceEditor;
        let mut message = serde_json::Map::new();
        message.insert(
            "type".to_string(),
            serde_json::json!("sidebarSpaceEditorResult"),
        );
        if let Some(remote_machine_id) = &context.remote_machine_id {
            message.insert(
                "remoteMachineId".to_string(),
                serde_json::json!(remote_machine_id),
            );
        }
        match command {
            SpaceEditorModalCommand::Submit { name, icon, color } => {
                message.insert("color".to_string(), serde_json::json!(color));
                message.insert("icon".to_string(), serde_json::json!(icon));
                if let Some(member_collection_id) = &context.member_collection_id {
                    message.insert(
                        "memberCollectionId".to_string(),
                        serde_json::json!(member_collection_id),
                    );
                }
                if let Some(member_project_id) = &context.member_project_id {
                    message.insert(
                        "memberProjectId".to_string(),
                        serde_json::json!(member_project_id),
                    );
                }
                message.insert(
                    "mode".to_string(),
                    serde_json::json!(match context.mode {
                        SpaceEditorMode::Create => "create",
                        SpaceEditorMode::Edit => "edit",
                    }),
                );
                message.insert("name".to_string(), serde_json::json!(name));
                if context.mode == SpaceEditorMode::Edit
                    && let Some(space_id) = &context.space_id
                {
                    message.insert("spaceId".to_string(), serde_json::json!(space_id));
                }
                self.forward_gpui_sidebar_space_editor_result_to_sidebar(&message, cx);
            }
            SpaceEditorModalCommand::Delete => {
                if let Some(space_id) = &context.space_id {
                    message.insert("mode".to_string(), serde_json::json!("delete"));
                    message.insert("spaceId".to_string(), serde_json::json!(space_id));
                    self.forward_gpui_sidebar_space_editor_result_to_sidebar(&message, cx);
                }
            }
            SpaceEditorModalCommand::Cancel => {}
        }
        self.release_native_app_modal_window(kind, cx);
    }
}
