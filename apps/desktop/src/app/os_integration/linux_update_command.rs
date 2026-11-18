//! Linux package installs: the update dialog shows the command gxserver worked out for this
//! computer (`linuxUpdate` in `/api/appUpdateStatus`, server/src/app_update/linux_install.rs), with
//! Copy command and the release page.
//! SEE-ALSO: server/src/app_update/linux_install.rs, apps/desktop/src/app/window/update_available_modal.rs.
#![cfg(target_os = "linux")]

use std::time::Duration;

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
}
