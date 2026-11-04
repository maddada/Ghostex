//! App modal pickers and first-launch actions: the app icon reset, background and glass images, first-launch project folder and session, worktree images.

#[cfg(target_os = "windows")]
use std::path::Path;

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_set_app_icon_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(source_id) = message.get("sourceId").and_then(serde_json::Value::as_str) else {
            return;
        };
        let current_source_id = app_icon::source_id_from_settings(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            app_icon::select_state(source_id, &current_source_id),
            cx,
        );
    }

    pub(crate) fn handle_gpui_pick_terminal_background_image_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        self.pick_image_for_app_modal(
            |path| {
                serde_json::json!({
                    "path": path,
                    "type": "terminalBackgroundImageFilePicked",
                })
            },
            cx,
        );
    }

    /// Settings -> Window glass -> Custom image: the Choose button of the dark or light picture.
    pub(crate) fn handle_gpui_pick_window_glass_image_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let appearance = if message["appearance"] == "light" {
            "light"
        } else {
            "dark"
        };
        self.pick_image_for_app_modal(
            move |path| {
                serde_json::json!({
                    "appearance": appearance,
                    "path": path,
                    "type": "windowGlassImageFilePicked",
                })
            },
            cx,
        );
    }

    /// Settings -> Window glass -> Live: "Choose a file…" for the dark or light mode's own video. The dialog
    /// cannot filter by type, so a picked file that is not a .mov, .mp4 or .m4v comes back as an
    /// error for the row to show.
    pub(crate) fn handle_gpui_pick_window_glass_video_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let appearance = if message["appearance"] == "light" {
            "light"
        } else {
            "dark"
        };
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Video".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let message = if crate::app::helpers::window_glass_video::is_video_file(&path) {
                serde_json::json!({
                    "appearance": appearance,
                    "path": path.to_string_lossy(),
                    "type": "windowGlassVideoFilePicked",
                })
            } else {
                serde_json::json!({
                    "appearance": appearance,
                    "error": "Choose a .mov, .mp4 or .m4v video.",
                    "type": "windowGlassVideoFilePicked",
                })
            };
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(message, cx);
            });
        })
        .detach();
    }

    /// A native image dialog whose picked absolute path is posted back to the open app-modal
    /// window as the message `reply` builds.
    fn pick_image_for_app_modal(
        &mut self,
        reply: impl FnOnce(String) -> serde_json::Value + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Image".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let message = reply(path.to_string_lossy().into_owned());
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(message, cx);
            });
        })
        .detach();
    }

    /*
    CDXC:Onboarding 2026-08-24:
    The onboarding Get Started page's Browse button. Same round trip as the
    terminal background image picker: native dialog host-side, picked absolute
    path posted back to the open app-modal window, where the first-launch page
    drops it into the project-folder input like a typed path.
    */
    pub(crate) fn handle_gpui_pick_first_launch_project_folder_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose Project Folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                // Cancelled in the dialog.
                Ok(Ok(None)) => return,
                Ok(Err(error)) => {
                    let reason = error.to_string();
                    let _ = this.update(cx, |this, cx| {
                        this.first_launch_folder_dialog_unavailable(&reason, cx)
                    });
                    return;
                }
                Err(_) => {
                    let _ = this.update(cx, |this, cx| {
                        this.first_launch_folder_dialog_unavailable(
                            "the folder dialog closed without an answer",
                            cx,
                        )
                    });
                    return;
                }
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            #[cfg(target_os = "windows")]
            let picked_path =
                match windows_terminal_backend::wsl_path_for_windows_path(path.as_path()) {
                    Ok(path) => path,
                    Err(message) => {
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_gpui_app_modal_toast(
                                "warning",
                                "Could not use that project folder",
                                &message,
                                cx,
                            );
                        });
                        return;
                    }
                };
            #[cfg(not(target_os = "windows"))]
            let picked_path = path.to_string_lossy().to_string();
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "path": picked_path,
                        "type": "firstLaunchProjectFolderPicked",
                    }),
                    cx,
                );
            });
        })
        .detach();
    }

    /// The system folder dialog could not open (on Linux: no xdg-desktop-portal file chooser).
    /// The reason is logged, and the open onboarding chooses the folder itself
    /// (window/onboarding/folder_chooser.rs) instead of the click doing nothing.
    fn first_launch_folder_dialog_unavailable(
        &mut self,
        reason: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        eprintln!("[ghostex] first-launch folder dialog unavailable: {reason}");
        support_logs::append(
            support_logs::GpuiSupportLog::AppModal,
            "gpui.onboarding.folderDialogFailed",
            serde_json::json!({ "error": reason }),
        );
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "reason": reason,
                "type": "firstLaunchProjectFolderDialogUnavailable",
            }),
            cx,
        );
    }

    /*
    CDXC:Onboarding 2026-08-24:
    Onboarding Finish crosses from the app-modal window into the sidebar
    runtime over the existing workspaceFolderPicked chain, which already owns
    project registration and focus. `firstLaunchAgentId` additionally asks the
    runtime to start the first session ('terminal' means a plain shell). Only
    the two bounded strings cross the boundary.
    */
    pub(crate) fn handle_gpui_first_launch_create_project_session_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(path) = command
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string)
        else {
            self.dispatch_gpui_first_launch_create_project_session_result(
                &request_id,
                false,
                Some("Choose a project folder before finishing setup."),
                cx,
            );
            return;
        };
        let Some(agent_id) = command
            .get("agentId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|agent_id| {
                !agent_id.is_empty()
                    && agent_id.len() <= 64
                    && agent_id.chars().all(|character| {
                        character.is_ascii_alphanumeric() || character == '-' || character == '_'
                    })
            })
            .map(str::to_string)
        else {
            self.dispatch_gpui_first_launch_create_project_session_result(
                &request_id,
                false,
                Some("Choose an available agent before finishing setup."),
                cx,
            );
            return;
        };
        #[cfg(target_os = "windows")]
        let project_path = if gpui_add_project_dialog_is_windows_absolute_path(&path) {
            match windows_terminal_backend::wsl_path_for_windows_path(Path::new(&path)) {
                Ok(path) => path,
                Err(message) => {
                    self.dispatch_gpui_first_launch_create_project_session_result(
                        &request_id,
                        false,
                        Some(&message),
                        cx,
                    );
                    return;
                }
            }
        } else {
            path
        };
        #[cfg(not(target_os = "windows"))]
        let project_path = path;
        let dispatched = self.dispatch_gpui_workspace_folder_picked_message(
            serde_json::json!({
                "firstLaunchAgentId": agent_id,
                "path": project_path,
                "requestId": request_id,
                "type": "workspaceFolderPicked",
            }),
            cx,
        );
        if !dispatched {
            self.dispatch_gpui_first_launch_create_project_session_result(
                &request_id,
                false,
                Some("The project sidebar is not available."),
                cx,
            );
        }
    }

    pub(crate) fn dispatch_gpui_first_launch_create_project_session_result(
        &mut self,
        request_id: &str,
        ok: bool,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut result = serde_json::json!({
            "ok": ok,
            "requestId": request_id,
            "type": "firstLaunchCreateProjectSessionResult",
        });
        if let Some(error) = error {
            result["error"] = serde_json::json!(error);
        }
        self.dispatch_open_gpui_app_modal_message(result, cx);
    }

    pub(crate) fn handle_gpui_pick_worktree_images_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach Images".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let paths: Vec<String> = paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
            if paths.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "paths": paths,
                        "type": "worktreeImageFilesPicked",
                    }),
                    cx,
                );
            });
        })
        .detach();
    }
}
