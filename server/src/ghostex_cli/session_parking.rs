use serde_json::{json, Value};

use super::args::Flags;
use super::output::is_failed_cli_result;
use super::rpc::{self, CliResult};

/// Mobile parks through SSH, preserving the computer's Sleep session when parking preference.
pub(super) fn set_parked(params: &Value, flags: &Flags) -> CliResult<Value> {
    let parked = params.get("parked") == Some(&Value::Bool(true));
    let settings = super::settings::read_settings_file()?;
    let should_sleep =
        parked && settings.get("sleepSessionWhenParking") == Some(&Value::Bool(true));
    let mut update = params.as_object().cloned().unwrap_or_default();
    update.remove("parked");
    update.insert("isParked".to_string(), json!(parked));
    let result = rpc::call_gxserver_rpc("/api/updateSession", &Value::Object(update), flags)?;
    if should_sleep && !is_failed_cli_result(&result) {
        return rpc::call_gxserver_rpc(
            "/api/sleepSession",
            &json!({
                "projectId": params.get("projectId"),
                "sessionId": params.get("sessionId"),
            }),
            flags,
        );
    }
    Ok(result)
}
