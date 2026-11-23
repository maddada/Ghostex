//! Agents Hub commands: the catalog, file reads and saves, Agent Sync, and revealing or editing Hub files.

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_agents_hub_command(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            "requestAgentsHubCatalog" => {
                /*
                CDXC:AgentLauncher 2026-06-24-12:26:
                Agents Hub catalog requests return metadata-only rows through the existing app-modal sidebarState path. File bodies stay out of the open/catalog message and are read only by requestAgentsHubFileContent after Rust validates the selected file against the generated Hub catalog.
                */
                self.run_gpui_app_modal_sidebar_status_task(gpui_agents_hub_catalog_message, cx);
            }
            "requestAgentsHubFileContent" => {
                let file_path = command
                    .get("filePath")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let request_id = command
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_agents_hub_file_content_message(file_path, request_id),
                    cx,
                );
            }
            "saveAgentsHubFile" => {
                self.handle_gpui_save_agents_hub_file_command(command, cx);
            }
            "requestAgentSyncReport" => {
                /*
                CDXC:AgentSync 2026-09-16 WHY:
                Agent Sync scans, plans, and applies through the shared ghostex-agent-sync crate on the background executor, and the JSON it returns is posted to the Hub through the same sidebarState path as the catalog, so the tab needs no new bridge.
                */
                self.run_gpui_app_modal_sidebar_status_task(gpui_agent_sync_report_message, cx);
            }
            "requestAgentSyncPlan" => {
                let scope = command
                    .get("scope")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("all")
                    .to_string();
                let seed_main_from = command
                    .get("seedMainFrom")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let pointer_policy = command
                    .get("pointerPolicy")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_agent_sync_plan_message(scope, seed_main_from, pointer_policy),
                    cx,
                );
            }
            "applyAgentSyncPlan" => {
                let scope = command
                    .get("scope")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("all")
                    .to_string();
                let groups: Vec<String> = command
                    .get("groups")
                    .and_then(serde_json::Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let seed_main_from = command
                    .get("seedMainFrom")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let pointer_policy = command
                    .get("pointerPolicy")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_agent_sync_apply_message(scope, groups, seed_main_from, pointer_policy)
                    },
                    cx,
                );
            }
            "openAgentsHubPathInFinder" => {
                self.open_gpui_agents_hub_path_in_finder(command, cx);
            }
            "openAgentsHubFileInBuiltInEditor" => {
                self.open_gpui_agents_hub_file_in_built_in_editor(command, window, cx);
            }
            _ => {}
        }
    }
}
