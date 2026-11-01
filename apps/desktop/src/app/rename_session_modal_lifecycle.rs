//! Open, close, and bridge plumbing for the native Rename Session dialog.
//! SEE-ALSO: apps/desktop/src/app/window/rename_session_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::window::*;
use crate::*;
use std::rc::Rc;

/// `sidebarAgentIconSupportsSessionHistoryTitleGeneration`: the agents whose
/// transcript gxserver can summarize into a name.
fn gpui_session_agent_icon_supports_history_title_generation(icon: Option<&str>) -> bool {
    matches!(
        icon,
        Some("claude" | "codex" | "cursor-cli" | "antigravity-cli" | "empryo")
    )
}

/// The Settings key that picks the agent Generate name runs (Settings › Agents › Title Generation Agent).
const TITLE_GENERATION_AGENT_KEY: &str = "sessionTitleGenerationAgent";

/// The name of the generator the dialog says it uses: the setting's option label, as Settings shows it.
fn gpui_rename_session_generator_name() -> String {
    use ghostex_settings_catalog::{Platform, modules};
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let agent = snapshot
        .object()
        .get(TITLE_GENERATION_AGENT_KEY)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            ghostex_settings_catalog::default_value(TITLE_GENERATION_AGENT_KEY)
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "codex".to_string());
    if agent == "custom" {
        return "your custom title command".to_string();
    }
    modules::exports(modules::module::SETTINGS, Platform::current())
        .into_iter()
        .find(|(name, _)| *name == "SESSION_TITLE_GENERATION_AGENT_OPTIONS")
        .and_then(|(_, options)| {
            options.to_value().as_array()?.iter().find_map(|option| {
                (option.get("value")?.as_str()? == agent)
                    .then(|| option.get("label")?.as_str().map(str::to_string))
                    .flatten()
            })
        })
        .unwrap_or(agent)
}

/// Asks this computer's gxserver for a name (`/api/previewSessionTitle`) without applying it.
fn gpui_rename_session_generator(sidebar_session_id: String) -> RenameSessionGenerator {
    Rc::new(move |text: String, cx: &mut gpui::App| {
        let session = ghostex_gx_core::SessionKey::parse_sidebar_session_id(&sidebar_session_id);
        let Some(session) = session.filter(|session| session.machine.is_local()) else {
            return gpui::Task::ready(Err(
                "Generate name works for agent sessions on this computer.".to_string(),
            ));
        };
        let params = serde_json::json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "text": text,
        });
        cx.spawn(async move |_cx| {
            let result = crate::app::gx_store::gx_rpc(None, "/api/previewSessionTitle", params)
                .await
                .map_err(|error| error.message)?;
            result
                .get("title")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "The generator returned no name.".to_string())
        })
    })
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the `open` message of the `renameSession`
    /// modal kind: `sessionId` (required), `initialTitle`, `sessionAgentIcon`.
    pub(crate) fn open_gpui_rename_session_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = message
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|session_id| !session_id.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        let initial_title = message
            .get("initialTitle")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let can_generate_from_history = gpui_session_agent_icon_supports_history_title_generation(
            message
                .get("sessionAgentIcon")
                .and_then(serde_json::Value::as_str),
        );
        let config = RenameSessionModalConfig {
            initial_title,
            can_generate_from_history,
            generator_name: gpui_rename_session_generator_name(),
            generate: gpui_rename_session_generator(session_id.clone()),
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_rename_session_modal_command(&session_id, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::RenameSession,
            RENAME_SESSION_MODAL_WIDTH,
            RENAME_SESSION_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiRenameSessionModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Posts the same `renameSession` command the React page did, or opens Settings at the
    /// generator row, then releases the window the dialog already removed.
    fn handle_gpui_rename_session_modal_command(
        &mut self,
        session_id: &str,
        command: RenameSessionModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::RenameSession;
        let mut message = serde_json::Map::new();
        message.insert("sessionId".to_string(), serde_json::json!(session_id));
        message.insert("type".to_string(), serde_json::json!("renameSession"));
        match command {
            RenameSessionModalCommand::Rename { title } => {
                message.insert("title".to_string(), serde_json::json!(title));
                self.handle_gpui_rename_command_session_command(&message, cx);
            }
            RenameSessionModalCommand::OpenGeneratorSettings => {
                // One app modal at a time: free the slot before Settings takes it.
                self.release_native_app_modal_window(kind, cx);
                self.open_app_modal_from_bridge(
                    serde_json::json!({
                        "initialSearchQuery": "Title Generation Agent",
                        "initialTab": "agents",
                        "modal": GpuiAppModalKind::Settings.modal_id(),
                        "type": "open",
                    }),
                    cx,
                );
                return;
            }
            RenameSessionModalCommand::Cancel => {}
        }
        self.release_native_app_modal_window(kind, cx);
    }
}
