//! `gx_rpc`: the one way a Rust executor file calls gxserver. The GPUI web build has a file of the
//! same name and signature that performs the call with `fetch`
//! (`apps/gpui-web/src/app/gx_store/rpc.rs`), so an executor that uses only this function (and no
//! CEF, AppKit or desktop window API) can be symlinked into the web build unchanged.
//!
//! CDXC:ServerApi 2026-09-25 WHY:
//! The app runtime port (docs/2026-09-25/app-runtime-port/PLAN.md) moves about 27k lines of
//! QuickJS into Rust that must also run in the web build. The desktop had about 110 blocking
//! `TcpStream` call sites and the web build calls `fetch`, with no shared seam, so each ported
//! executor would have had to be written twice. This is the seam, in the shape
//! `native_chat/rpc.rs` already proved: an async function, same signature on both builds.
//!
//! Unlike `native_chat/rpc.rs`, the desktop body does not block the thread that awaits it: the
//! blocking request runs on a thread of its own and the future only waits for the answer, so a
//! caller may await it from `cx.spawn` on the main thread as well as from the background
//! executor. The future is `Send`.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/rpc_types.rs (the shared error type),
//! docs/2026-09-25/app-runtime-port/LEDGER.md (how families use it).

use std::time::Duration;

use futures::channel::oneshot;
use serde_json::Value;

use super::rpc_types::GxRpcError;
use crate::app::helpers::{
    gpui_remote_gxserver_post_typed_operation, gxserver_post_typed_operation,
};
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// The old QuickJS runtime's `fetch` ran under the same 60-second global timeout.
const GX_RPC_TIMEOUT: Duration = Duration::from_secs(60);

/// Calls `path` (`/api/...`) with `params` on the local gxserver, or on the remote machine whose
/// tunnel `remote` names, and returns the envelope's `result`.
pub(crate) async fn gx_rpc(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: Value,
) -> Result<Value, GxRpcError> {
    gx_rpc_with_timeout(remote, path, params, GX_RPC_TIMEOUT).await
}

/// [`gx_rpc`] with its own time limit, for the calls the old runtime gave a longer one than the
/// default: a commit message an agent writes, a worktree cut on the far side of a tunnel.
pub(crate) async fn gx_rpc_with_timeout(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, GxRpcError> {
    let (sender, receiver) = oneshot::channel();
    let owned_path = path.to_string();
    let spawned = std::thread::Builder::new()
        .name("ghostex-gx-rpc".into())
        .spawn(move || {
            let response = match remote {
                Some(target) => gpui_remote_gxserver_post_typed_operation(
                    &target,
                    &owned_path,
                    &params,
                    timeout,
                ),
                None => gxserver_post_typed_operation(&owned_path, &params, timeout),
            };
            let result = match response {
                Ok((status, body)) => match serde_json::from_str::<Value>(&body) {
                    Ok(envelope) => GxRpcError::from_envelope(&owned_path, Some(status), envelope),
                    Err(_) => Err(GxRpcError::transport(
                        &owned_path,
                        "gxserver returned invalid JSON.",
                    )),
                },
                Err(message) => Err(GxRpcError::transport(&owned_path, message)),
            };
            let _ = sender.send(result);
        });
    if spawned.is_err() {
        return Err(GxRpcError::transport(
            path,
            "Could not start the gxserver request.",
        ));
    }
    receiver.await.unwrap_or_else(|_| {
        Err(GxRpcError::transport(
            path,
            "The gxserver request was dropped.",
        ))
    })
}

/// The typed-operation call the store's sidebar action files make: `gpui_gxserver_rpc_result`
/// (its error text and its result check unchanged) run on the background executor, exactly as each
/// of those files spawned it before.
///
/// CDXC:WebGpui 2026-09-25 WHY:
/// Those files (sleep, wake, close, fork, flags, snooze, the order writes, the client documents'
/// pushes) predate `gx_rpc` and blocked a background thread on the helper inline, which a page
/// cannot do. Moving that one expression behind this name lets the GPUI web build compile the
/// same files with a `fetch` twin of the same signature (`apps/gpui-web/src/app/gx_store/rpc.rs`)
/// while the desktop keeps the call, the error strings and the timeout it had.
pub(crate) fn gxserver_rpc_result_task(
    background: &gpui::BackgroundExecutor,
    path: &str,
    params: Value,
    timeout: Duration,
) -> gpui::Task<Result<Value, String>> {
    let path = path.to_string();
    background.spawn(async move {
        crate::app::helpers::gpui_gxserver_rpc_result(&path, &params, timeout)
    })
}
