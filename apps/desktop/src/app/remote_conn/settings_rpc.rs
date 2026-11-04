//! Settings > Agents and Settings > Integrations with a remote machine picked: their gxserver
//! calls go to that machine's gxserver through its tunnel instead of this computer's.
//!
//! CDXC:RemoteMachines 2026-10-26 WHY:
//! The pages call the same endpoints for every machine (`/api/agentCliMaintenance`, `/api/managedTools`, `/api/machineIntegrations`), so a machine runs its own installer code and the desktop composes no commands for it. Only those paths cross, with only the keys each one reads, so a Settings page cannot reach any other remote endpoint; an older gxserver that does not know a path (404) is named as the machine that needs updating instead of failing quietly.
//! SEE-ALSO: server/src/machine_integrations.rs, apps/desktop/src/app/window/settings_modal/tabs/machine_picker.rs.
use std::time::Duration;

use serde_json::{Map, Value};

use crate::app::helpers::*;
use crate::app::window::settings_modal::model::GxserverRpcReply;
use crate::{GhostexGpuiApp, shared_settings};

/// The keys each remote Settings path may carry, all text but the `fresh` and
/// `includeDesktopControl` switches.
fn allowed_keys(path: &str) -> Option<&'static [&'static str]> {
    Some(match path {
        "/api/agentCliMaintenance" => &["action", "agentId", "operation", "methodId"],
        "/api/managedTools" => &[
            "action",
            "tool",
            "operation",
            "fresh",
            "includeDesktopControl",
        ],
        "/api/machineIntegrations" => &["action", "skillName"],
        _ => return None,
    })
}

/// The params as they cross to the machine, or `None` when the path or a value is not allowed.
pub(crate) fn settings_remote_params(path: &str, params: &Value) -> Option<Value> {
    let keys = allowed_keys(path)?;
    let params = params.as_object()?;
    if Value::Object(params.clone()).to_string().len() > GPUI_REMOTE_GXSERVER_PARAMS_MAX_BYTES {
        return None;
    }
    let mut shaped = Map::new();
    for (key, value) in params {
        if !keys.contains(&key.as_str()) {
            continue;
        }
        let allowed = match value {
            Value::String(text) => text.chars().count() <= 200,
            Value::Bool(_) => matches!(key.as_str(), "fresh" | "includeDesktopControl"),
            _ => false,
        };
        if !allowed {
            return None;
        }
        shaped.insert(key.clone(), value.clone());
    }
    Some(Value::Object(shaped))
}

/// The saved name of a machine, for the sentences below.
fn machine_name(machine_id: &str) -> String {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(Value::as_array)
        .and_then(|machines| {
            machines
                .iter()
                .find(|machine| machine.get("id").and_then(Value::as_str) == Some(machine_id))
        })
        .and_then(|machine| machine.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "The remote machine".to_string())
}

/// What the page shows for a machine's answer: its `result`, its own rejection, or why there is none.
pub(crate) fn settings_remote_answer(
    name: &str,
    status_code: u16,
    body: &str,
) -> Result<Value, String> {
    if (200..300).contains(&status_code) {
        return parse_gpui_gxserver_rpc_result(body)
            .map_err(|_| format!("{name} sent an answer this Ghostex cannot read."));
    }
    // A gxserver from before the endpoint does not know the path.
    if status_code == 404 {
        return Err(format!(
            "{name} runs an older Ghostex that can't do this yet. Update Ghostex on {name} (Settings > Remote), then try again."
        ));
    }
    Err(gpui_gxserver_rejection_message(body)
        .unwrap_or_else(|| format!("{name} did not accept the request.")))
}

impl GhostexGpuiApp {
    pub(crate) fn run_settings_remote_gxserver_rpc(
        &mut self,
        machine_id: String,
        path: String,
        params: Value,
        timeout: Duration,
        reply: GxserverRpcReply,
        cx: &mut gpui::Context<Self>,
    ) {
        let refused = match gpui_normalize_remote_machine_id(&machine_id) {
            None => Err("That machine is not in Settings > Remote.".to_string()),
            Some(machine_id) => {
                let name = machine_name(&machine_id);
                match (
                    settings_remote_params(&path, &params),
                    self.gpui_remote_gxserver_request_target(&machine_id),
                ) {
                    (None, _) => Err(format!("Ghostex can't send that request to {name}.")),
                    (_, None) => Err(format!(
                        "{name} is not connected. Connect to it in Settings > Remote first."
                    )),
                    (Some(params), Some(target)) => Ok((name, params, target)),
                }
            }
        };
        let (name, params, target) = match refused {
            Ok(request) => request,
            Err(message) => {
                cx.defer(move |cx| reply(Err(message), cx));
                return;
            }
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |_, cx| {
            let result = background
                .spawn(async move {
                    match gpui_remote_gxserver_post_typed_operation(&target, &path, &params, timeout)
                    {
                        Ok((status_code, body)) => {
                            settings_remote_answer(&name, status_code, &body)
                        }
                        Err(_) => Err(format!(
                            "{name} did not answer. Check that it is still connected in Settings > Remote."
                        )),
                    }
                })
                .await;
            cx.update(|cx| reply(result, cx));
        })
        .detach();
    }

    /// Hands an open Settings window every saved machine's connection state, for the machine picker.
    pub(crate) fn handle_request_remote_machine_connect_states(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        self.publish_remote_machine_connect_states(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_settings_paths_and_their_keys_cross() {
        assert_eq!(
            settings_remote_params(
                "/api/agentCliMaintenance",
                &json!({ "action": "start", "agentId": "codex", "operation": "update", "methodId": "npm", "command": "rm -rf /" }),
            ),
            Some(
                json!({ "action": "start", "agentId": "codex", "operation": "update", "methodId": "npm" })
            )
        );
        assert_eq!(
            settings_remote_params(
                "/api/managedTools",
                &json!({ "action": "read", "tool": "node", "fresh": true })
            ),
            Some(json!({ "action": "read", "tool": "node", "fresh": true }))
        );
        assert_eq!(
            settings_remote_params("/api/killSession", &json!({ "action": "x" })),
            None
        );
        // A value that is not text (or `fresh`) refuses the whole request.
        assert_eq!(
            settings_remote_params("/api/machineIntegrations", &json!({ "action": ["x"] })),
            None
        );
        assert_eq!(
            settings_remote_params("/api/machineIntegrations", &json!("status")),
            None
        );
    }

    #[test]
    fn an_older_gxserver_names_the_machine_to_update() {
        let error = settings_remote_answer("Kubuntu VM", 404, "").expect_err("404");
        assert!(error.contains("Update Ghostex on Kubuntu VM"));
        let error = settings_remote_answer(
            "Kubuntu VM",
            400,
            &json!({ "ok": false, "error": "badRequest", "message": "Unknown Ghostex agent skill." })
                .to_string(),
        )
        .expect_err("rejected");
        assert_eq!(error, "Unknown Ghostex agent skill.");
        let error = settings_remote_answer("Kubuntu VM", 500, "oops").expect_err("500");
        assert_eq!(error, "Kubuntu VM did not accept the request.");
    }
}
