//! `gx_rpc` for the GPUI web build: the desktop's signature (`apps/desktop/src/app/gx_store/rpc.rs`),
//! performed with `fetch` against the daemon `ghostex web` handed the page.
//!
//! CDXC:WebGpui 2026-09-25 WHY:
//! Executor files ported out of the desktop's QuickJS runtime call gxserver only through this
//! function, so they are symlinked here unchanged (docs/2026-09-25/app-runtime-port/PLAN.md). The
//! fetch runs as a page-local task and only its plain JSON result crosses back through a channel,
//! because a JavaScript promise is not `Send` and callers await this inside executors that demand
//! a `Send` future (the reason `native_chat/rpc.rs` gives). The web build has no remote machines
//! yet, so a remote target is refused with a message rather than sent to the local daemon.
use futures::channel::oneshot;
use serde_json::Value;

use super::rpc_types::GxRpcError;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// The desktop's signature; `fetch` here has no time limit of its own, so the limit is not used.
#[allow(dead_code)]
pub(crate) async fn gx_rpc_with_timeout(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: Value,
    _timeout: std::time::Duration,
) -> Result<Value, GxRpcError> {
    gx_rpc(remote, path, params).await
}

#[allow(dead_code)] // the first callers arrive with the runtime port's family commits
pub(crate) async fn gx_rpc(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: Value,
) -> Result<Value, GxRpcError> {
    if remote.is_some() {
        return Err(GxRpcError::transport(
            path,
            "Remote machines are not available in the web build yet.",
        ));
    }
    let (sender, receiver) = oneshot::channel();
    let owned_path = path.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        let result = match super::web_transport::current_endpoint() {
            Some(endpoint) => {
                match super::web_transport::rpc_envelope(&endpoint, &owned_path, params).await {
                    Ok(result) => Ok(result),
                    Err(error) => Err(GxRpcError {
                        code: error["code"].as_str().map(str::to_string),
                        message: error["message"]
                            .as_str()
                            .unwrap_or("gxserver rejected the request.")
                            .to_string(),
                        path: owned_path,
                    }),
                }
            }
            None => Err(GxRpcError::transport(
                &owned_path,
                "Ghostex is not connected yet.",
            )),
        };
        let _ = sender.send(result);
    });
    receiver
        .await
        .unwrap_or_else(|_| Err(GxRpcError::transport(path, "The request was dropped.")))
}

/// The desktop's `gxserver_rpc_result_task` (the sidebar action files' call): the same call with `fetch`, its failure as the message a toast shows.
pub(crate) fn gxserver_rpc_result_task(
    background: &gpui::BackgroundExecutor,
    path: &str,
    params: Value,
    _timeout: std::time::Duration,
) -> gpui::Task<Result<Value, String>> {
    let path = path.to_string();
    background.spawn(async move {
        gx_rpc(None, &path, params)
            .await
            .map_err(|error| error.message)
    })
}
