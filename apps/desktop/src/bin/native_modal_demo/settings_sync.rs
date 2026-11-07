//! Preview of the Sync and Autopilot Settings pages against a real gxserver (a throwaway one from
//! `cargo xtask test-server`), so the pages show live computers and notes, or Autopilot's log.
//!
//! States: `sync`, `autopilot`, `collab`. Environment: `GHOSTEX_NATIVE_MODAL_DEMO_GXSERVER_URL` (`http://127.0.0.1:<port>`)
//! and `GHOSTEX_NATIVE_MODAL_DEMO_GXSERVER_TOKEN_FILE` (that server's `auth/token`). Without them
//! the page shows the service as unreachable.
use serde_json::{Value, json};

pub(super) fn owns(state: &str) -> bool {
    state == "sync" || state == "autopilot" || state == "collab"
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    owns(state).then(|| json!({ "initialTab": state }))
}

pub(super) fn gxserver_rpc_available() -> bool {
    !super::super::env("GHOSTEX_NATIVE_MODAL_DEMO_GXSERVER_URL").is_empty()
}

/// Forwards the page's call to the configured gxserver.
pub(super) fn rpc(path: &str, params: &Value) -> Result<Value, String> {
    let base = super::super::env("GHOSTEX_NATIVE_MODAL_DEMO_GXSERVER_URL");
    let token = std::fs::read_to_string(super::super::env(
        "GHOSTEX_NATIVE_MODAL_DEMO_GXSERVER_TOKEN_FILE",
    ))
    .map_err(|error| error.to_string())?;
    let body = json!({ "protocolVersion": 1, "params": params });
    let output = std::process::Command::new("curl")
        .args(["-s", "-X", "POST", &format!("{base}{path}")])
        .args(["-H", &format!("authorization: Bearer {}", token.trim())])
        .args(["-H", "content-type: application/json"])
        .args(["-H", "x-gxserver-protocol-version: 1"])
        .args(["-d", &body.to_string()])
        .output()
        .map_err(|error| error.to_string())?;
    let answer: Value =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    answer.get("result").cloned().ok_or_else(|| {
        answer
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("No answer")
            .to_string()
    })
}
