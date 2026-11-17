//! Linux package installs: Update opens a terminal that runs the command gxserver worked out for
//! this computer (`linuxUpdate` in `/api/appUpdateStatus`, server/src/app_update/linux_install.rs),
//! from Settings > Updates and from the update dialog.
//!
//! CDXC:Updates 2026-11-18 WHY:
//! The terminal is a Ghostex command-pane tab (the one Settings > Integrations > Tools already
//! uses for installs that ask for a password), not the desktop's own terminal app: it works on
//! every distribution and desktop without `xdg-terminal-exec`, and it runs in a zmx session, so
//! closing or restarting Ghostex while pacman, apt or dnf replaces it leaves the install running.
//! The command is read from gxserver here, never taken from a page, so nothing else can make the
//! app run text of its choosing.
//! SEE-ALSO: server/src/app_update/linux_install.rs, apps/desktop/src/app/os_integration/managed_tool_terminal.rs.
#![cfg(target_os = "linux")]

use std::time::Duration;

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

/// gxserver's `linuxUpdate` for this computer: `{ kind, command, releaseUrl }`. Blocking.
fn read_linux_update_plan() -> Result<serde_json::Value, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/appUpdateStatus",
        &serde_json::json!({}),
        Duration::from_secs(15),
    )?;
    Ok(result["status"]["linuxUpdate"].clone())
}

fn plan_command(plan: &serde_json::Value) -> Option<String> {
    plan["command"]
        .as_str()
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(str::to_string)
}

impl GhostexGpuiApp {
    /// The update dialog for a deb, rpm, AUR or tarball install: the command comes from gxserver
    /// first, so the dialog opens with it (or, without one, with its download page button).
    pub(crate) fn open_linux_package_update_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let plan = background
                .spawn(async move { read_linux_update_plan() })
                .await;
            let command = plan.ok().as_ref().and_then(plan_command);
            let _ = this.update(cx, |this, cx| {
                let Some(update) = this.linux_update.as_ref() else {
                    return;
                };
                let open_message = serde_json::json!({
                    "modal": GpuiAppModalKind::UpdateAvailable.modal_id(),
                    "notesMarkdown": update.notes_markdown(),
                    "portable": false,
                    "state": "notify",
                    "type": "open",
                    "version": update.version(),
                    "linuxCommand": command,
                });
                this.open_gpui_app_modal_window(
                    GpuiAppModalKind::UpdateAvailable,
                    open_message,
                    serde_json::Value::Null,
                    None,
                    cx,
                );
            });
        })
        .detach();
    }

    /// The update dialog's Update: the dialog has no workspace window, so the terminal opens in
    /// the main one.
    pub(crate) fn run_linux_update_command_in_main_window(&mut self, cx: &mut gpui::Context<Self>) {
        self.defer_in_main_window(cx, |app, window, cx| {
            app.run_linux_update_command(window, cx)
        });
    }

    /// Settings > Updates' and the dialog's Update on Linux: a command-pane tab that runs the
    /// update command, where the user types their password (the AppImage: its own dialog).
    pub(crate) fn run_linux_update_command(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // The AppImage downloads and swaps in the update itself, from its own dialog.
        if self.linux_appimage_install.is_some() {
            self.open_linux_update_modal(Some(window), cx);
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let plan = background
                .spawn(async move { read_linux_update_plan() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let command = match plan {
                    Ok(plan) => plan_command(&plan),
                    Err(error) => {
                        this.dispatch_gpui_app_modal_toast(
                            "error",
                            "Couldn't start the update",
                            &error,
                            cx,
                        );
                        return;
                    }
                };
                let Some(command) = command else {
                    this.dispatch_gpui_app_modal_toast(
                        "info",
                        "No update command here",
                        "Download the new version from the release page.",
                        cx,
                    );
                    this.open_linux_update_download_page(cx);
                    return;
                };
                this.open_gpui_command_action_terminal(
                    "ghostex.gpui.updateGhostex".to_string(),
                    "Update Ghostex".to_string(),
                    command,
                    false,
                    false,
                    window,
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    "info",
                    "Updating Ghostex",
                    "Type your password in the terminal tab. Restart Ghostex when it finishes.",
                    cx,
                );
            });
        })
        .detach();
    }
}
