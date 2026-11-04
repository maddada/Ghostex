//! Open, close, and reconnect plumbing for the native Install remote gxserver prompt.
//! SEE-ALSO: apps/desktop/src/app/window/remote_gxserver_install_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), `open_gpui_remote_gxserver_install_modal` in apps/desktop/src/app/remote_conn/project_browse_and_add.rs (builds the `open` message this opener reads), and the `reconnectRemoteMachine` arm in apps/desktop/src/app/delayed_send/app_modal_sidebar_command/dialogs.rs (the React host's route, mirrored here).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the native dialog for the `open` message of the
    /// `remoteGxserverInstall` modal kind. Named `_native_modal` because
    /// `open_gpui_remote_gxserver_install_modal(remote_machine_id, cx)` in
    /// remote_conn/project_browse_and_add.rs is the existing opener that builds
    /// that message. Refuses payloads the React host would have refused:
    /// `remoteMachineId` and `remoteMachineName` must be non-empty.
    pub(crate) fn open_gpui_remote_gxserver_install_native_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        let (Some(remote_machine_id), Some(machine_name)) =
            (text("remoteMachineId"), text("remoteMachineName"))
        else {
            return;
        };
        let config = RemoteGxserverInstallModalConfig {
            machine_name,
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_remote_gxserver_install_modal_command(&remote_machine_id, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::RemoteGxserverInstall,
            REMOTE_GXSERVER_INSTALL_MODAL_WIDTH,
            REMOTE_GXSERVER_INSTALL_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiRemoteGxserverInstallModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    fn handle_gpui_remote_gxserver_install_modal_command(
        &mut self,
        remote_machine_id: &str,
        command: RemoteGxserverInstallModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        if let RemoteGxserverInstallModalCommand::Approve = command {
            let mut message = serde_json::Map::new();
            message.insert("installApproved".to_string(), serde_json::json!(true));
            message.insert(
                "remoteMachineId".to_string(),
                serde_json::json!(remote_machine_id),
            );
            message.insert(
                "type".to_string(),
                serde_json::json!("reconnectRemoteMachine"),
            );
            self.handle_gpui_reconnect_remote_machine_message(&message, cx);
        }
        self.release_native_app_modal_window(GpuiAppModalKind::RemoteGxserverInstall, cx);
    }
}
