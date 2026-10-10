//! `handle_gpui_app_modal_sidebar_command`: the switch over every `sidebarCommand` message the app's own modal windows send, routing each command type to its per-family handler.

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_app_modal_sidebar_command(
        &mut self,
        message: serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(command) = message
            .get("message")
            .and_then(serde_json::Value::as_object)
        else {
            return;
        };
        let Some(command_type) = command.get("type").and_then(serde_json::Value::as_str) else {
            return;
        };

        match command_type {
            "updateSettings"
            | "updateSettingsPatch"
            | "openExternalUrl"
            | "listAppIcons"
            | "setAppIcon"
            | "pickAppIconFile"
            | "pickTerminalBackgroundImageFile"
            | "pickWindowGlassImageFile"
            | "pickWindowGlassVideoFile"
            | "pickFirstLaunchProjectFolder"
            | "firstLaunchCreateProjectSession"
            | "revealAppIconsFolder"
            | "openGhosttySettingsDocs"
            | "openAccessibilityPreferences"
            | "openScreenRecordingPreferences"
            | "openMacOSNotificationSettings"
            | "requestMacOSNotificationPermission"
            | "playCompletionSoundPreview"
            | "testAgentTaskCompletion"
            | "applyRecommendedGhosttySettings"
            | "resetGhosttySettingsToDefault"
            | "openGhosttyConfigFile"
            | "runPortlessSettingsAdminAction"
            | "runPortlessSetupPromptAdminAction"
            | "setPortlessEnabled"
            | "saveSidebarAgent"
            | "deleteSidebarAgent"
            | "syncSidebarAgentOrder"
            | "setSidebarAgentsEnabled"
            | "saveSidebarCommand"
            | "deleteSidebarCommand"
            | "syncSidebarCommandOrder"
            | "saveGlobalSidebarCommand"
            | "deleteGlobalSidebarCommand"
            | "syncGlobalSidebarCommandOrder"
            | "setProjectWorktreeCommand"
            | "setProjectBeadsDisplayKey"
            | "setProjectBeadsDirectory"
            | "setProjectDocsDirectory" => {
                self.handle_gpui_app_modal_settings_command(command_type, command, cx);
            }
            "refreshDaemonSessions"
            | "killDaemonSession"
            | "killTerminalDaemon"
            | "requestGhostexCliStatus"
            | "installGhostexCli"
            | "installBrowserControl"
            | "installBrowserUseSkill"
            | "installComputerUseSkill"
            | "installCliSkill"
            | "installAgentsOrchestrationSkill"
            | "installManageBeadsSkill"
            | "installGenerateTitleSkill"
            | "installMoveCodexSessionSkill"
            | "installHelpSkill"
            | "installVisualsSkill"
            | "installCuaDriverSkill"
            | "installCuaDriver"
            | "reinstallCuaDriver"
            | "uninstallCuaDriver"
            | "checkCuaDriverUpdate"
            | "installSpaceoSkill"
            | "installSpaceo"
            | "reinstallSpaceo"
            | "uninstallSpaceo"
            | "checkSpaceoUpdate"
            | "runManagedToolTerminalCommand"
            | "runAgentboxTerminalCommand"
            | "setUpAgentboxWithAgent"
            | "uninstallBundledAgentSkills"
            | "uninstallBundledAgentSkill"
            | "requestAgentHookStatus"
            | "installAgentHooks"
            | "uninstallAgentHooks"
            | "requestOSIntegrationStatus"
            | "requestPluginSettingsStatus"
            | "reinstallPlugin"
            | "uninstallPlugin"
            | "setOSIntegrationDefaults"
            | "requestGhostexFolderStats"
            | "openGhostexFolder" => {
                self.handle_gpui_app_modal_settings_tools_command(
                    command_type,
                    command,
                    window,
                    cx,
                );
            }
            "saveRemoteMachinePassword"
            | "reconnectRemoteMachine"
            | "probeRemoteGxserverInstall"
            | "addProjectDialogRequest"
            | "pickReplacementProjectFolder"
            | "setSessionNote"
            | "sidebarSpaceEditorResult"
            | "updateCustomSessionTags"
            | "confirmAgentHookLaunch"
            | "removeProject"
            | "requestProjectWorktrees"
            | "createProjectWorktree"
            | "confirmDeleteWorktree"
            | "confirmRenameWorktree"
            | "commitWorktreeBeforeDelete"
            | "confirmSidebarGitCommit"
            | "confirmSidebarGitDirectMerge"
            | "runSidebarGitMultipleCommits"
            | "openSidebarGitChangedFileDiff"
            | "openSidebarGitChangedFile"
            | "cancelSidebarGitCommit"
            | "revealExportedTranscript"
            | "cancelExportSessionTranscript"
            | "startExportedTranscriptConversation"
            | "runExportSessionTranscript"
            | "savePinnedPrompt"
            | "renameSession"
            | "scheduleDelayedSend"
            | "postponeDelayedSend"
            | "cancelDelayedSend"
            | "toggleCloseAfterDone" => {
                self.handle_gpui_app_modal_dialog_command(command_type, command, cx);
            }
            "openWorkspaceWelcome" | "runGhostexHotkeyAction" => {
                self.handle_gpui_app_modal_hotkey_action_command(command_type, command, window, cx);
            }
            "requestAgentsHubCatalog"
            | "requestAgentsHubFileContent"
            | "saveAgentsHubFile"
            | "requestAgentSyncReport"
            | "requestAgentSyncPlan"
            | "applyAgentSyncPlan"
            | "openAgentsHubPathInFinder"
            | "openAgentsHubFileInBuiltInEditor" => {
                self.handle_gpui_app_modal_agents_hub_command(command_type, command, window, cx);
            }
            "requestPreviousSessions"
            | "requestSessionTranscriptSizes"
            | "restorePreviousSession"
            | "deletePreviousSession" => {
                self.handle_gpui_app_modal_previous_sessions_command(command_type, command, cx);
            }
            "requestStashedPrompts"
            | "saveStashedPrompt"
            | "saveStashedPromptTag"
            | "deleteStashedPromptTag"
            | "setStashedPromptTags"
            | "deleteStashedPrompt"
            | "insertStashedPrompt"
            | "jumpToStashedPromptSession" => {
                self.handle_gpui_app_modal_saved_prompts_command(command_type, command, cx);
            }
            "requestRecentProjects"
            | "restoreRecentProject"
            | "closeProjectFromProjects"
            | "focusRecentProject"
            | "removeRecentProject"
            | "copyRecentProjectPath"
            | "openRecentProjectInFinder"
            | "openRecentProjectTerminal"
            | "focusSession"
            | "runSidebarCommand" => {
                self.handle_gpui_app_modal_projects_command(command_type, command, cx);
            }
            command_type
                if crate::app::quick_access::commands::QUICK_ACCESS_COMMAND_ROW_TYPES
                    .contains(&command_type) =>
            {
                self.run_quick_access_command_row(command_type, command, window, cx);
            }
            "searchPreviousSessionsByText" => {
                self.handle_gpui_app_modal_previous_sessions_command(command_type, command, cx);
            }
            // The Workspaces settings page's "Sign out of all sites" (workspace_browser.rs).
            "clearWorkspaceBrowserSignins" => {
                if let Some(workspace_id) = command
                    .get("workspaceId")
                    .and_then(serde_json::Value::as_str)
                {
                    match crate::app::workspace_browser::clear_workspace_browser_signins(
                        workspace_id,
                    ) {
                        Ok(()) => self.dispatch_gpui_workspace_action_toast(
                            "success",
                            "Signed out",
                            "This workspace's Browser is signed out of every site.",
                            cx,
                        ),
                        Err(error) => self.dispatch_gpui_workspace_action_toast(
                            "error",
                            "Couldn't sign out",
                            &error,
                            cx,
                        ),
                    }
                }
            }
            // The Workspaces settings page's "Forget all answers" (browser_site_requests.rs).
            "forgetWorkspaceBrowserSiteAnswers" => {
                let workspace_id = command
                    .get("workspaceId")
                    .and_then(serde_json::Value::as_str);
                let profile =
                    crate::app::workspace_browser::workspace_browser_profile(workspace_id);
                use crate::app::browser_site_requests::{
                    ForgetSiteAnswers, browser_site_answers_forget_pending,
                    forget_browser_site_answers,
                };
                match forget_browser_site_answers(&profile) {
                    ForgetSiteAnswers::Forgotten(0) => self.dispatch_gpui_workspace_action_toast(
                        "success",
                        "Nothing to forget",
                        "No site in this workspace's Browser has an answer kept.",
                        cx,
                    ),
                    ForgetSiteAnswers::Forgotten(count) => {
                        self.toast_forgot_site_answers(count, cx)
                    }
                    // The context's start forgets them before anything else uses it.
                    ForgetSiteAnswers::Pending(count) => {
                        let started =
                            crate::app::workspace_browser::start_workspace_browser_context(
                                &profile, cx,
                            );
                        cx.spawn(async move |this, cx| {
                            let forgotten =
                                started.await && !browser_site_answers_forget_pending(&profile);
                            let _ = this.update(cx, |app, cx| {
                                if forgotten {
                                    app.toast_forgot_site_answers(count, cx);
                                } else {
                                    app.dispatch_gpui_workspace_action_toast(
                                        "info",
                                        "Forget all answers",
                                        "Will apply when this workspace's browser starts.",
                                        cx,
                                    );
                                }
                            });
                        })
                        .detach();
                    }
                }
            }
            "postponePortlessSetupPrompt" | "cancelPortlessSetupPrompt" => {
                self.handle_gpui_app_modal_settings_command(command_type, command, cx);
            }
            command_type if gpui_app_modal_unsupported_settings_command_noop(command_type) => {}
            command_type if self.gx_store_run_app_modal_create_command(command_type, cx) => {}
            _ => {}
        }
    }
}
