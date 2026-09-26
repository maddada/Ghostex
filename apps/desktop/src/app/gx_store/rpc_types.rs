//! The error type `gx_rpc` returns, shared by the desktop and the GPUI web build (symlinked into
//! `apps/gpui-web/src/app/gx_store/`). Only the transport differs between the two builds, which is
//! `rpc.rs`; everything a caller matches on lives here, so an executor file compiles unchanged on
//! both.

use serde_json::Value;

/// Why one gxserver call failed.
///
/// `code` is the daemon's error code (`composerNotReady`, `sendCancelled`, ...) when the daemon
/// answered with `{ok:false, error}`; `None` for a transport failure (not reachable, timed out,
/// not JSON, an HTTP error with no envelope). `message` is the daemon's own text, or a fixed
/// sentence for a transport failure. `path` is the endpoint that was called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GxRpcError {
    pub(crate) code: Option<String>,
    pub(crate) message: String,
    pub(crate) path: String,
}

impl GxRpcError {
    pub(crate) fn transport(path: &str, message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            path: path.to_string(),
        }
    }

    /// Reads a `{ok, result, error, message}` envelope: the result when `ok` is true and the HTTP
    /// status (when there is one) is a success, the daemon's error otherwise.
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // the web transport reads its own envelope
    pub(crate) fn from_envelope(
        path: &str,
        status: Option<u16>,
        mut envelope: Value,
    ) -> Result<Value, Self> {
        if envelope["ok"] == false {
            return Err(Self {
                code: envelope["error"].as_str().map(str::to_string),
                message: envelope["message"]
                    .as_str()
                    .or_else(|| envelope["error"].as_str())
                    .unwrap_or("gxserver rejected the request.")
                    .to_string(),
                path: path.to_string(),
            });
        }
        if status.is_some_and(|status| !(200..300).contains(&status)) || envelope["ok"] != true {
            let status = status.map_or_else(|| "no".to_string(), |status| status.to_string());
            return Err(Self::transport(
                path,
                format!("gxserver request failed with HTTP {status}."),
            ));
        }
        Ok(envelope["result"].take())
    }
}

impl std::fmt::Display for GxRpcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
