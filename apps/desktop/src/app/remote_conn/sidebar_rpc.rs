//! The one way a sidebar request reaches a remote machine's gxserver.
//!
//! CDXC:RemoteMachines 2026-09-25 WHY:
//! The Rust store's remote row actions (sleep, wake, close, fork, flags, snooze, Full Reload) and
//! its remote project and account calls go through here. Everything between "a machine, a path
//! and a body" and the answer is the security boundary `CDXC:RemoteMachines 2026-06-24-16:48`
//! describes (the endpoint allowlist, the size bound, the per-path shaping, the live tunnel's port
//! and token, the timeout bounds) plus the presentation refresh, so a fix to any of those lands on
//! every caller. It replaces the 2026-09-21 note that the old runtime's
//! `gpuiRemoteGxserverSidebarRequest` bridge message shared this function: the runtime is gone.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_remote.rs (the store),
//! apps/desktop/src/app/helpers/remote/sidebar_bridge/ (the allowlist and the shaping).

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

/// The sentence a refused or unanswered remote request answers with. A request the machine
/// rejected answers with its gxserver's own `message` instead (`gpui_gxserver_rejection_message`);
/// no transport error, raw body, host, port or token crosses this boundary.
pub(crate) const GPUI_REMOTE_GXSERVER_REQUEST_FAILED: &str = "Remote gxserver request failed.";
pub(crate) const GPUI_REMOTE_HOOKS_UPDATE_REQUIRED: &str = "Remote Ghostex needs an update.";
pub(crate) const GPUI_REMOTE_HOOKS_TIMED_OUT: &str = "The remote computer did not answer in time.";

/// What a waited caller's failure toast says: the machine's own reason when it rejected the
/// request, else the caller's `fallback`, which names what did not happen.
///
/// CDXC:RemoteMachines 2026-10-01 WHY: A remote agent create the machine refused toasted only "The remote gxserver could not create that agent session." while the machine's gxserver had said exactly why ("no launch command for agent …"). Only that structured, bounded `message` crosses, as Add Project's rejections do.
pub(crate) fn gpui_remote_sidebar_rpc_failure_reason(error: &str, fallback: &str) -> String {
    if error == GPUI_REMOTE_GXSERVER_REQUEST_FAILED {
        fallback.to_string()
    } else {
        error.to_string()
    }
}

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
    /// through `gpui_normalize_remote_machine_id`, and refreshes that machine's presentation once
    /// the call has come back when the path is one that changes it.
    ///
    /// The task resolves to the machine's raw answer, which a caller must shape before any of it
    /// reaches a renderer (`gpui_remote_sidebar_response_payload`), or to the machine's rejection
    /// message, or to [`GPUI_REMOTE_GXSERVER_REQUEST_FAILED`].
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
                    let failed = || GPUI_REMOTE_GXSERVER_REQUEST_FAILED.to_string();
                    let (status_code, body) = gpui_remote_gxserver_post_typed_operation(
                        &target,
                        path.as_str(),
                        &params,
                        timeout,
                    )
                    .map_err(|error| {
                        if path == "/api/installAgentHooks"
                            && error.contains("did not answer in time")
                        {
                            GPUI_REMOTE_HOOKS_TIMED_OUT.to_string()
                        } else {
                            failed()
                        }
                    })?;
                    if !(200..300).contains(&status_code) {
                        if matches!(
                            path.as_str(),
                            "/api/readAgentHookStatus" | "/api/installAgentHooks"
                        ) && matches!(status_code, 404 | 426)
                        {
                            return Err(GPUI_REMOTE_HOOKS_UPDATE_REQUIRED.to_string());
                        }
                        return Err(gpui_gxserver_rejection_message(&body).unwrap_or_else(failed));
                    }
                    parse_gpui_gxserver_rpc_result(&body).map_err(|_| failed())
                })
                .await;
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
