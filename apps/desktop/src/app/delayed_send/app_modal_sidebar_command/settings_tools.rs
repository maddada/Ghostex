//! Settings tool pages: daemon sessions, the ghostex CLI and bundled skills, cua-driver, Spaceo, managed tools, Agentbox, agent hooks, OS integration, plugins and the Ghostex folder.

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_settings_tools_command(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            "refreshDaemonSessions" => {
                self.refresh_gpui_daemon_sessions_state_in_background(None, cx);
            }
            "killDaemonSession" => {
                let project_id = command
                    .get("workspaceId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let session_id = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let active_project_id = self.gpui_daemon_sessions_active_project_id();
                let focused_session_id = self.gpui_daemon_sessions_focused_session_id();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_close_daemon_session_and_refresh_state(
                            project_id,
                            session_id,
                            active_project_id.as_deref(),
                            focused_session_id.as_deref(),
                        )
                    },
                    cx,
                );
            }
            "killTerminalDaemon" => {
                let dispatched = self.dispatch_gpui_workspace_sleep_all_daemon_sessions(cx);
                self.refresh_gpui_daemon_sessions_state_in_background(
                    (!dispatched).then(|| {
                        "The sidebar runtime is not ready to stop local terminal sessions. The list was refreshed without changing daemon state.".to_string()
                    }),
                    cx,
                );
            }
            "requestGhostexCliStatus" => {
                self.run_gpui_app_modal_and_titlebar_status_task(
                    || gpui_ghostex_cli_status_message(None),
                    cx,
                );
            }
            "installGhostexCli" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallGhostexCli,
                    cx,
                );
            }
            "installBrowserControl" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallBrowserControl,
                    cx,
                );
            }
            "installBrowserUseSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallBrowserUseSkill,
                    cx,
                );
            }
            "installComputerUseSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallComputerUseSkill,
                    cx,
                );
            }
            "installCliSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallCliSkill,
                    cx,
                );
            }
            "installAgentsOrchestrationSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallAgentsOrchestrationSkill,
                    cx,
                );
            }
            "installManageBeadsSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallManageBeadsSkill,
                    cx,
                );
            }
            "installGenerateTitleSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallGenerateTitleSkill,
                    cx,
                );
            }
            "installMoveCodexSessionSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallMoveCodexSessionSkill,
                    cx,
                );
            }
            "installHelpSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallHelpSkill,
                    cx,
                );
            }
            "installVisualsSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallVisualsSkill,
                    cx,
                );
            }
            "installSecretsSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallSecretsSkill,
                    cx,
                );
            }
            "installCuaDriverSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallCuaDriverSkill,
                    cx,
                );
            }
            "installCuaDriver" => {
                self.handle_gpui_cua_driver_install_or_update(window, cx);
            }
            "reinstallCuaDriver" => {
                self.handle_gpui_cua_driver_reinstall(window, cx);
            }
            "uninstallCuaDriver" => {
                self.handle_gpui_cua_driver_uninstall(window, cx);
            }
            "checkCuaDriverUpdate" => {
                self.check_gpui_cua_driver_update(cx);
            }
            "installSpaceoSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallSpaceoSkill,
                    cx,
                );
            }
            "installSpaceo" => {
                self.handle_gpui_spaceo_install_or_update(window, cx);
            }
            "reinstallSpaceo" => {
                self.handle_gpui_spaceo_reinstall(window, cx);
            }
            "uninstallSpaceo" => {
                self.handle_gpui_spaceo_uninstall(window, cx);
            }
            "checkSpaceoUpdate" => {
                self.check_gpui_spaceo_update(cx);
            }
            "requestToolPermissionStatus" => {
                self.refresh_gpui_tool_permission_status(cx);
            }
            "grantToolPermission" => {
                let text = |key: &str| {
                    command
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                self.grant_gpui_tool_permission(&text("tool"), &text("permission"), cx);
            }
            "restartSpaceo" => {
                self.restart_gpui_spaceo(cx);
            }
            "restartCuaDriver" => {
                self.restart_gpui_cua_driver(cx);
            }
            "runManagedToolTerminalCommand" => {
                if let Some(tool_id) = command.get("toolId").and_then(serde_json::Value::as_str) {
                    self.run_managed_tool_terminal_command(tool_id.to_string(), window, cx);
                }
            }
            "runAgentboxTerminalCommand" => {
                self.run_agentbox_terminal_command(command, window, cx);
            }
            "setUpAgentboxWithAgent" => {
                self.run_agentbox_setup_chat(cx);
            }
            "uninstallBundledAgentSkills" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::UninstallBundledAgentSkills,
                    cx,
                );
            }
            "uninstallBundledAgentSkill" => {
                if let Some(skill_name) = command
                    .get("skillId")
                    .and_then(serde_json::Value::as_str)
                    .and_then(gpui_bundled_agent_skill_name)
                {
                    self.run_gpui_ghostex_cli_settings_action(
                        GpuiGhostexCliSettingsAction::UninstallBundledAgentSkill(skill_name),
                        cx,
                    );
                }
            }
            "requestAgentHookStatus" => {
                let agent_ids = gpui_settings_command_ordered_agent_ids(command);
                self.run_gpui_progressive_agent_hook_status_task(agent_ids, cx);
            }
            "installAgentHooks" => {
                let agent_ids = gpui_settings_command_agent_ids(command);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_agent_hook_status_message(
                            "/api/installAgentHooks",
                            agent_ids,
                            "Agent hook install failed.",
                        )
                    },
                    cx,
                );
            }
            "uninstallAgentHooks" => {
                let agent_ids = gpui_settings_command_agent_ids(command);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_agent_hook_status_message(
                            "/api/uninstallAgentHooks",
                            agent_ids,
                            "Agent hook uninstall failed.",
                        )
                    },
                    cx,
                );
            }
            "requestOSIntegrationStatus" => {
                self.run_gpui_app_modal_sidebar_status_task(gpui_os_integration_status_message, cx);
            }
            "requestPluginSettingsStatus" => {
                self.request_plugin_settings_status(cx);
            }
            "reinstallPlugin" => {
                if let Some(plugin_id) = command.get("pluginId").and_then(serde_json::Value::as_str)
                {
                    self.reinstall_plugin_from_settings(plugin_id, cx);
                }
            }
            "uninstallPlugin" => {
                if let Some(plugin_id) = command.get("pluginId").and_then(serde_json::Value::as_str)
                {
                    self.uninstall_plugin_from_settings(plugin_id, cx);
                }
            }
            "cancelPluginChange" => {
                if command.get("pluginId").and_then(serde_json::Value::as_str) == Some("cef") {
                    self.cancel_web_runtime_pending_change(cx);
                }
            }
            "setOSIntegrationDefaults" => {
                let target = command
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_set_os_integration_defaults_status_message(target.as_deref()),
                    cx,
                );
            }
            "requestGhostexFolderStats" => {
                self.run_gpui_app_modal_sidebar_status_task(gpui_ghostex_folder_stats_message, cx);
            }
            "openGhostexFolder" => {
                self.open_gpui_ghostex_folder(cx);
            }
            _ => {}
        }
    }
}
