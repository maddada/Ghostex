//! The one way a sidebar request reaches a remote machine's gxserver.
//!
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! Two callers make these requests: the old runtime, through the `gpuiRemoteGxserverSidebarRequest`
//! bridge message, and the Rust store, which answers a remote row's sleep, wake, close, fork, flags,
//! snooze and Full Reload itself. Everything between "a machine, a path and a body" and the answer
//! is the security boundary `CDXC:RemoteMachines 2026-06-24-16:48` describes (the endpoint
//! allowlist, the size bound, the per-path shaping, the live tunnel's port and token, the timeout
//! bounds) plus the refresh that keeps the old runtime's copy of the machine current. Both callers
//! go through this one function, so a fix to any of those cannot land on one route and miss the
//! other, and the store can never send a machine something the bridge would have refused.
//!
//! SEE-ALSO: apps/desktop/src/app/remote_conn/sidebar_request_and_recent_projects.rs (the bridge),
//! apps/desktop/src/app/gx_store/sidebar_remote.rs (the store),
//! apps/desktop/src/app/helpers/remote/sidebar_bridge.rs (the allowlist and the shaping).

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

/// The sentence a failed remote request answers with. The machine's own body never crosses this
/// boundary, so this is also what a waited caller's failure toast says.
pub(crate) const GPUI_REMOTE_GXSERVER_REQUEST_FAILED: &str = "Remote gxserver request failed.";

/// Who reads the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteSidebarRpcMode {
    /// The caller does: every refusal and every failure is an `Err` it handles itself, which is a
    /// bridge request that carries a request id.
    Awaited,
    /// Nobody does: a refusal or a failure is a toast here, which is what a bridge request with no
    /// request id has always got.
    FireAndForget,
}

impl GhostexGpuiApp {
    /// Sends one allowlisted request to `remote_machine_id`, which the caller has already passed
    /// through `gpui_normalize_remote_machine_id`, and refreshes the old runtime's copy of that
    /// machine once the call has come back when the path is one that changes it.
    ///
    /// The task resolves to the machine's raw answer, which a caller must shape before any of it
    /// reaches a renderer (`gpui_remote_sidebar_response_payload`), or to
    /// [`GPUI_REMOTE_GXSERVER_REQUEST_FAILED`].
    pub(crate) fn start_gpui_remote_sidebar_rpc(
        &mut self,
        remote_machine_id: &str,
        path: &str,
        params: Option<serde_json::Value>,
        timeout: Duration,
        mode: GpuiRemoteSidebarRpcMode,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<serde_json::Value, String>> {
        let refused = || gpui::Task::ready(Err(GPUI_REMOTE_GXSERVER_REQUEST_FAILED.to_string()));
        // A path off the allowlist is silent for a fire-and-forget request, as it always was: the
        // renderer that sent it is the only one that could have been told, and it asked nobody to.
        if !gpui_remote_sidebar_request_path_allowed(path) {
            return refused();
        }
        let Some(params) = params
            .filter(|params| {
                params.is_object()
                    && params.to_string().len() <= GPUI_REMOTE_GXSERVER_PARAMS_MAX_BYTES
            })
            .and_then(|params| gpui_remote_sidebar_request_params(path, params))
        else {
            if mode == GpuiRemoteSidebarRpcMode::FireAndForget {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "GPUI rejected the remote gxserver request.",
                    cx,
                );
            }
            return refused();
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(remote_machine_id) else {
            if mode == GpuiRemoteSidebarRpcMode::FireAndForget {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "Reconnect the remote machine before using its sessions.",
                    cx,
                );
            }
            return refused();
        };
        let remote_machine_id = remote_machine_id.to_string();
        let path = path.to_string();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let refreshes = gpui_remote_sidebar_request_refreshes_presentation(path.as_str());
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(&target, path.as_str(), &params, timeout)
                })
                .await
                .map_err(|_| GPUI_REMOTE_GXSERVER_REQUEST_FAILED.to_string());
            let _ = this.update(cx, |this, cx| {
                if result.is_err() && mode == GpuiRemoteSidebarRpcMode::FireAndForget {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Remote action failed",
                        "The remote gxserver action did not complete.",
                        cx,
                    );
                }
                if refreshes {
                    this.refresh_gpui_remote_gxserver_presentation_in_background(
                        &remote_machine_id,
                    );
                }
            });
            result
        })
    }
}
