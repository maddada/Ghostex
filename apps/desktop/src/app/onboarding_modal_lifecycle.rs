//! Open, first-run gating, commands and answers for the native Onboarding.
//!
//! CDXC:Onboarding 2026-09-28 WHY:
//! The React onboarding posted bridge messages (`updateSettings`, `installAgentHooks`,
//! `pickFirstLaunchProjectFolder`, `firstLaunchCreateProjectSession`, `completeFirstLaunchSetup`,
//! ...) through the modal host and read `agentHookStatus` / `ghostexCliStatus` /
//! `settingsActionStatus` back as `sidebarState` JSON; its agent CLI rows called gxserver's
//! `/api/agentCliMaintenance` over HTTP. The native window sends `OnboardingCommand`s that call the
//! same Rust functions directly, and the same status payloads are handed to it as they are
//! produced, so first run no longer needs CEF at all.
//! SEE-ALSO: apps/desktop/src/app/window/onboarding/ (the window), apps/desktop/src/app/modals/modal_window.rs (`open_gpui_first_launch_setup_with_sidebar_state`, `complete_first_launch_setup`), apps/desktop/src/app/os_integration/first_run_onboarding.rs (the first-run pass), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::helpers::*;
use crate::app::window::onboarding::model::{
    COLOURFULNESS_LAST_POSITION, colourfulness_points, js_number,
};
use crate::app::window::onboarding::{
    AgentCliRequest, CatalogAgent, FinishTarget, GpuiOnboardingWindow, InitialPanel,
    ONBOARDING_MODAL_HEIGHT, ONBOARDING_MODAL_WIDTH, OnboardingCommand, OnboardingConfig,
    OnboardingHost, OnboardingSettings, ThemeSwatchPreset, ThemeTable,
};
use crate::*;
use std::collections::HashSet;
use std::time::Duration;

const KIND: GpuiAppModalKind = GpuiAppModalKind::Onboarding;

/// The settings the onboarding shows, read the way `normalizeghostexSettings` reads them.
pub(crate) fn gpui_onboarding_settings() -> OnboardingSettings {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let object = snapshot.object();
    let mut settings = OnboardingSettings::from_object(object);
    // A settings file saved before the preset dropdowns existed migrates to Custom when its
    // custom chrome was tuned (normalize.ts).
    let dark_preset_known = object
        .get("darkThemePreset")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|preset| {
            preset == "custom"
                || DARK_THEME_PRESET_CONTROLS
                    .iter()
                    .any(|(key, _, _)| *key == preset)
        });
    if !dark_preset_known
        && custom_dark_chrome_controls(object)
            != (
                DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
                DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB,
            )
    {
        settings.dark_theme_preset = "custom".to_string();
    }
    settings
}

/// The look card's colour squares and Colourfulness patches, from the same theme math the window
/// paints its chrome with (`presetChromeAtStep`, `colourfulnessPatch` in theme-simple-controls.tsx (deleted 2026-10-01)).
fn gpui_onboarding_theme_table() -> ThemeTable {
    let points: Vec<f64> = (0..=COLOURFULNESS_LAST_POSITION)
        .map(colourfulness_points)
        .collect();
    let dark = DARK_THEME_PRESET_CONTROLS
        .iter()
        .map(|(value, darkness, tint)| ThemeSwatchPreset {
            value: value.to_string(),
            chrome: points
                .iter()
                .map(|points| {
                    sidebar_titlebar_background_for_darkness(
                        clamp_sidebar_titlebar_background_darkness_percent(darkness + points),
                        *tint,
                    )
                })
                .collect(),
            accent: accent_color_for_tint(*tint, false),
        })
        .collect();
    let light = LIGHT_THEME_PRESET_CONTROLS
        .iter()
        .map(|(value, lightness, tint)| ThemeSwatchPreset {
            value: value.to_string(),
            chrome: points
                .iter()
                .map(|points| {
                    sidebar_titlebar_light_background_for_lightness(
                        clamp_sidebar_titlebar_light_background_lightness_percent(
                            lightness + points,
                        ),
                        *tint,
                    )
                })
                .collect(),
            accent: accent_color_for_tint(*tint, true),
        })
        .collect();
    let colourfulness_patches = points
        .iter()
        .map(|points| {
            let mut patch = serde_json::Map::new();
            patch.insert("themeSidebarContrast".into(), js_number(*points));
            patch.insert("themeWorkAreaContrast".into(), js_number(*points));
            patch.insert(
                "customSidebarTitlebarBackgroundDarknessPercent".into(),
                js_number(clamp_sidebar_titlebar_background_darkness_percent(
                    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT + points,
                )),
            );
            patch.insert(
                "customSidebarTitlebarLightBackgroundLightnessPercent".into(),
                js_number(clamp_sidebar_titlebar_light_background_lightness_percent(
                    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT + points,
                )),
            );
            patch
        })
        .collect();
    ThemeTable {
        dark,
        light,
        colourfulness_patches,
    }
}

fn open_message_has_projects(message: &serde_json::Value) -> bool {
    message
        .get("latestSidebarStateMessage")
        .and_then(|state| state.get("hud"))
        .and_then(|hud| hud.get("projectSettingsProjects"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|projects| !projects.is_empty())
}

impl GhostexGpuiApp {
    /// Opens the onboarding for the `onboarding` modal's open message (`firstRun` marks the
    /// automatic first-run open).
    pub(crate) fn open_gpui_onboarding_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let first_run = message.get("firstRun").and_then(serde_json::Value::as_bool) == Some(true);
        // Only the automatic first run carries `introVideo`; Tips > Setup never opens on the video.
        let intro_video = first_run
            && message
                .get("introVideo")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
        let has_projects = open_message_has_projects(message);
        let catalog = GPUI_DEFAULT_SIDEBAR_AGENTS
            .iter()
            .map(|agent| CatalogAgent {
                agent_id: agent.agent_id.to_string(),
                name: agent.name.to_string(),
                icon: agent.icon.to_string(),
            })
            .collect();
        let config = OnboardingConfig {
            first_run,
            has_projects,
            settings: gpui_onboarding_settings(),
            catalog,
            theme: gpui_onboarding_theme_table(),
            system_light: gpui_system_uses_light_appearance(),
            cli_available: true,
            initial_panel: InitialPanel::Panel(1),
            picked_folder: None,
            intro_video,
        };
        let host: OnboardingHost = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_onboarding_command(command, cx);
        });
        let main_window_native_view = self.parent_ns_view;
        let close_host = host.clone();
        self.open_native_app_modal(
            KIND,
            ONBOARDING_MODAL_WIDTH,
            ONBOARDING_MODAL_HEIGHT,
            move |window, cx| {
                /*
                CDXC:Onboarding 2026-09-15 DECISION:
                User: "the modal must stay on top of the main ghostex app and centered on top of it".
                The onboarding window is a child of the main window so it never drops behind the workspace and follows it when it moves.
                */
                attach_gpui_app_modal_window_to_main_window(window, main_window_native_view);
                let view = cx.new(|cx| GpuiOnboardingWindow::new(config, host, window, cx));
                let guard = view.clone();
                // First-launch setup is required until the sidebar has a project: native close
                // controls do nothing until then. Afterwards a close counts as finishing setup.
                window.on_window_should_close(cx, move |_window, cx| {
                    let can_close = guard.read(cx).can_close();
                    if can_close {
                        close_host(OnboardingCommand::Finish(FinishTarget::None), cx);
                    }
                    false
                });
                view
            },
            cx,
        );
    }

    fn update_gpui_onboarding(
        &mut self,
        cx: &mut gpui::Context<Self>,
        update: impl FnOnce(&mut GpuiOnboardingWindow, &mut gpui::Context<GpuiOnboardingWindow>),
    ) -> bool {
        self.update_native_app_modal(KIND, cx, |view: &mut GpuiOnboardingWindow, _window, cx| {
            update(view, cx)
        })
        .is_some()
    }

    fn handle_gpui_onboarding_command(
        &mut self,
        command: OnboardingCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            OnboardingCommand::UpdateSettings(patch) => {
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &serde_json::json!({
                        "patch": patch,
                        "source": "firstLaunch:preferences",
                    }),
                    cx,
                );
                let settings = gpui_onboarding_settings();
                self.update_gpui_onboarding(cx, |view, cx| view.receive_settings(settings, cx));
            }
            OnboardingCommand::RescanAgents => {
                self.run_gpui_progressive_agent_hook_status_task(None, cx);
            }
            OnboardingCommand::RequestCliStatus => {
                self.run_gpui_app_modal_and_titlebar_status_task(
                    || gpui_ghostex_cli_status_message(None),
                    cx,
                );
            }
            OnboardingCommand::InstallAgentHooks(agent_ids) => {
                let agent_ids: HashSet<String> = agent_ids.into_iter().collect();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_agent_hook_status_message(
                            "/api/installAgentHooks",
                            Some(agent_ids),
                            "Agent hook install failed.",
                        )
                    },
                    cx,
                );
            }
            OnboardingCommand::InstallComputerUse { install_driver } => {
                // Cua Driver first when it is missing, then the Computer Use skill.
                if install_driver {
                    self.defer_in_main_window(cx, |app, window, cx| {
                        app.handle_gpui_cua_driver_install_or_update(window, cx);
                    });
                }
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallComputerUseSkill,
                    cx,
                );
            }
            OnboardingCommand::OpenAccessibilityPreferences => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_ACCESSIBILITY_PREFERENCES_URL,
                    "openAccessibilityPreferences",
                    cx,
                );
            }
            OnboardingCommand::OpenScreenRecordingPreferences => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL,
                    "openScreenRecordingPreferences",
                    cx,
                );
            }
            OnboardingCommand::InstallBrowserSkill => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallBrowserUseSkill,
                    cx,
                );
            }
            OnboardingCommand::UninstallBrowserSkill => {
                if let Some(skill_name) = gpui_bundled_agent_skill_name("browserUse") {
                    self.run_gpui_ghostex_cli_settings_action(
                        GpuiGhostexCliSettingsAction::UninstallBundledAgentSkill(skill_name),
                        cx,
                    );
                }
            }
            OnboardingCommand::OpenExternalUrl(url) => {
                let _ = gpui_open_external_http_url(url.trim());
            }
            OnboardingCommand::IntroVideoSeen => {
                self.persist_gpui_first_run_onboarding_marker(
                    GpuiFirstRunOnboardingMarker::IntroVideoSeen,
                    cx,
                );
            }
            OnboardingCommand::PickProjectFolder => {
                self.handle_gpui_pick_first_launch_project_folder_message(cx);
            }
            OnboardingCommand::FinishFirstLaunch {
                request_id,
                agent_id,
                path,
            } => {
                let command = serde_json::json!({
                    "agentId": agent_id,
                    "path": path,
                    "requestId": request_id,
                    "type": "firstLaunchCreateProjectSession",
                });
                if let Some(command) = command.as_object() {
                    self.handle_gpui_first_launch_create_project_session_message(command, cx);
                }
            }
            OnboardingCommand::Finish(target) => {
                self.complete_first_launch_setup();
                let modal = self
                    .native_app_modal
                    .as_ref()
                    .filter(|modal| modal.kind == KIND)
                    .map(|modal| (modal.window, modal.view.clone()));
                if let Some((window, view)) = modal
                    && let Ok(view) = view.downcast::<GpuiOnboardingWindow>()
                {
                    let _ = window.update(cx, |_root, window, cx| {
                        view.update(cx, |view, cx| view.release_images(window, cx));
                    });
                }
                self.close_native_app_modal_from_bridge(cx);
                match target {
                    FinishTarget::None => {}
                    FinishTarget::Settings(tab) => {
                        let mut message =
                            serde_json::json!({ "modal": "settings", "type": "open" });
                        if let Some(tab) = tab {
                            message["initialTab"] = serde_json::json!(tab);
                        }
                        self.open_app_modal_from_bridge(message, cx);
                    }
                    FinishTarget::RemoteSettings => {
                        self.open_app_modal_from_bridge(
                            serde_json::json!({
                                "initialRemoteSection": "easyConnect",
                                "initialTab": "remote",
                                "modal": "settings",
                                "type": "open",
                            }),
                            cx,
                        );
                    }
                }
            }
            OnboardingCommand::AgentCli {
                request_id,
                agent_id,
                request,
            } => {
                let mut params = serde_json::json!({ "agentId": agent_id });
                match request {
                    AgentCliRequest::Read => params["action"] = serde_json::json!("read"),
                    AgentCliRequest::Start { method_id } => {
                        params["action"] = serde_json::json!("start");
                        params["operation"] = serde_json::json!("install");
                        params["methodId"] = serde_json::json!(method_id);
                    }
                    AgentCliRequest::AddToPath => params["action"] = serde_json::json!("addToPath"),
                }
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_gxserver_rpc_result(
                                "/api/agentCliMaintenance",
                                &params,
                                Duration::from_secs(30),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        app.update_gpui_onboarding(cx, |view, cx| {
                            view.receive_agent_cli_result(request_id, result, cx)
                        });
                    });
                })
                .detach();
            }
        }
    }

    /// Host messages for the open onboarding (`firstLaunchProjectFolderPicked`,
    /// `firstLaunchCreateProjectSessionResult`). Returns false for any other message.
    pub(crate) fn receive_gpui_onboarding_modal_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match message.get("type").and_then(serde_json::Value::as_str) {
            Some("firstLaunchProjectFolderPicked") => {
                let Some(path) = message
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return false;
                };
                self.update_gpui_onboarding(cx, |view, cx| view.receive_picked_folder(path, cx))
            }
            Some("firstLaunchCreateProjectSessionResult") => {
                let Some(request_id) = message
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return false;
                };
                let ok = message.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
                let error = message
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.update_gpui_onboarding(cx, |view, cx| {
                    if ok {
                        // The project now exists, so the window may close.
                        view.set_has_projects(true, cx);
                    }
                    view.receive_finish_result(&request_id, ok, error, cx);
                })
            }
            _ => false,
        }
    }

    /// Status payloads the modal host received as `sidebarState` (`agentHookStatus`,
    /// `ghostexCliStatus`, `settingsActionStatus`), for the open onboarding.
    pub(crate) fn receive_gpui_onboarding_status_payload(
        &mut self,
        payload: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        match payload.get("type").and_then(serde_json::Value::as_str) {
            Some("agentHookStatus") => {
                self.update_gpui_onboarding(cx, |view, cx| {
                    view.receive_agent_hook_status(payload, cx)
                });
            }
            Some("ghostexCliStatus") => {
                self.update_gpui_onboarding(cx, |view, cx| view.receive_cli_status(payload, cx));
            }
            Some("settingsActionStatus") => {
                self.update_gpui_onboarding(cx, |view, cx| {
                    view.receive_settings_action_status(&payload, cx)
                });
            }
            _ => {}
        }
    }
}
