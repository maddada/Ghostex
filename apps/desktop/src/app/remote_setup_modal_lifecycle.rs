//! Open, gxserver, and Settings handoff plumbing for the native Remote Setup dialog.
//! SEE-ALSO: apps/desktop/src/app/window/remote_setup_modal.rs (the window entity and its decision record), packages/core-ui/remote-setup-modal/connect-section.tsx (deleted 2026-10-01) (the Easy Connect flow mirrored by `gpui_remote_setup_connect_easy_connect`), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;
use std::time::Duration;

/// Connect does everything the Easy Connect card promises, the way the React
/// page did it: read SSH access, enable it when it is off (one admin prompt;
/// a cancelled prompt is not an error), then turn Easy Connect on.
fn gpui_remote_setup_connect_easy_connect() -> Result<(), String> {
    let status = gpui_gxserver_rpc_result(
        "/api/remoteAccessStatus",
        &serde_json::json!({}),
        Duration::from_secs(15),
    )?;
    if status
        .get("ssh")
        .and_then(|ssh| ssh.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        // The admin password prompt stays open until the user answers it.
        let enabled = gpui_gxserver_rpc_result(
            "/api/enableSshAccess",
            &serde_json::json!({}),
            Duration::from_secs(300),
        )?;
        if enabled.get("outcome").and_then(serde_json::Value::as_str) == Some("failed") {
            return Err(enabled
                .get("message")
                .and_then(serde_json::Value::as_str)
                .filter(|message| !message.is_empty())
                .unwrap_or("SSH access could not be turned on.")
                .to_string());
        }
    }
    gpui_gxserver_rpc_result(
        "/api/updateTailcatState",
        &serde_json::json!({ "kind": "setEnabled", "enabled": true }),
        Duration::from_secs(60),
    )?;
    Ok(())
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the `remoteSetup` open message (sidebar
    /// menu > Mobile & Remote). The React page read `remoteTailscaleEnabled`
    /// from the hydrated settings and the gxserver bootstrap from the page;
    /// here both come from the shared settings and the local auth token.
    pub(crate) fn open_gpui_remote_setup_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if message.get("modal").and_then(serde_json::Value::as_str)
            != Some(GpuiAppModalKind::RemoteSetup.modal_id())
        {
            return;
        }
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let tailscale_enabled = settings
            .object()
            .get("remoteTailscaleEnabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let config = RemoteSetupModalConfig {
            palette: self.gpui_native_dialog_palette(),
            tailscale_enabled,
            gxserver_available: read_gpui_gxserver_auth_token().is_ok(),
            android_open: false,
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_remote_setup_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::RemoteSetup,
            REMOTE_SETUP_MODAL_WIDTH,
            REMOTE_SETUP_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiRemoteSetupModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    fn handle_gpui_remote_setup_modal_command(
        &mut self,
        command: RemoteSetupModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::RemoteSetup;
        match command {
            RemoteSetupModalCommand::OpenExternalUrl(url) => {
                self.receive_gpui_titlebar_resources_open_external_url_message(
                    &serde_json::json!({
                        "type": "openExternalUrl",
                        "url": url,
                    }),
                );
            }
            RemoteSetupModalCommand::AndroidLinkCopied => {
                gpui_copy_feedback(cx);
            }
            RemoteSetupModalCommand::Connect => {
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move { gpui_remote_setup_connect_easy_connect() })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        app.update_native_app_modal(
                            kind,
                            cx,
                            |modal: &mut GpuiRemoteSetupModalWindow, window, cx| {
                                modal.connect_finished(result, window, cx);
                            },
                        );
                    });
                })
                .detach();
            }
            RemoteSetupModalCommand::OpenRemoteSettings(section) => {
                self.release_native_app_modal_window(kind, cx);
                self.open_gpui_settings_remote_section(section, cx);
            }
            RemoteSetupModalCommand::Close => {
                self.release_native_app_modal_window(kind, cx);
            }
        }
    }

    /// Settings on the Remote tab, focused on one of its sections: the
    /// `openAppModal({ modal: 'settings', initialTab: 'remote', initialRemoteSection })`
    /// the React page posted.
    fn open_gpui_settings_remote_section(
        &mut self,
        section: RemoteSettingsSection,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialRemoteSection": section.open_message_value(),
            "initialTab": "remote",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }
}
