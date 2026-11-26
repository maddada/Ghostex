//! What the Executor routes do: read and change the computer's Executor CLI (install it, sign it
//! in or out), and remove what the old built-in Executor left. Blocking (they run the CLI), so the
//! routes run these on `spawn_blocking`.
//!
//! CDXC:Connectors 2026-11-27 DECISION:
//! User: "implement executor cli support instead of the current one", and on the Executor CLI page: one CLI sign-in for the computer, agents through Ghostex's relay with its cards, the user's own CLI (no copy Ghostex downloads), the old built-in Executor's apps listed once and then cleaned up, and no Executor tools until the CLI is signed in. Replaces the 2026-10-15 decision to download a pinned Executor and run one per workspace.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

use super::api::failed;
use super::executor;

/// `/api/installExecutorCli`: runs Executor's official installer in the background
/// (`readConnectors` shows its progress).
pub(crate) fn install_executor_cli(
    _state: &AppState,
    _params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    executor::install().map_err(failed)
}

/// `/api/signInExecutorCli`: starts `executor login` (to `host`, a self-hosted Executor, when
/// given) and returns the page to open and its code.
pub(crate) fn sign_in_executor_cli(
    _state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let host = params.get("host").and_then(Value::as_str);
    executor::sign_in(host).map_err(failed)
}

/// `/api/cancelExecutorCliSignIn`: stops a waiting `executor login`.
pub(crate) fn cancel_executor_cli_sign_in(
    _state: &AppState,
    _params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    executor::cancel_sign_in();
    Ok(json!({ "cancelled": true }))
}

/// `/api/signOutExecutorCli`: `executor logout`.
pub(crate) fn sign_out_executor_cli(
    _state: &AppState,
    _params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    executor::sign_out().map_err(failed)?;
    Ok(json!({ "signedIn": false }))
}

/// `/api/removeOldExecutor`: deletes what the old built-in Executor left now.
pub(crate) fn remove_old_executor(
    state: &AppState,
    _params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    executor::remove_old_executor(&state.paths);
    Ok(json!({ "removed": true }))
}
