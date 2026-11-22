//! The short session rows `ghostex sessions --json` prints unless `--full` asks for the inventory.

use serde_json::{json, Map, Value};

use crate::ghostex_cli::agents;
use crate::ghostex_cli::args::Flags;

/// CDXC:Cli 2026-09-26 DECISION:
/// The user asked for `ghostex sessions --json` to default to the same short facts as the sidebar's Copy Details (agent, title, Global Ref, agent session id, zmx name, project), and for `--json --full` to print the whole inventory. `projectId` and `status` ride along because this answer is live, unlike a copied block: `--project-id` takes the one, and the other says whether a session has to be woken before a send.
pub(crate) fn brief_session_list(result: &Value, flags: &Flags) -> Value {
    let mut sessions = result
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    agents::resolve_names(&mut sessions, flags);
    let rows: Vec<Value> = sessions.iter().map(brief_session).collect();
    json!({"ok": true, "sessions": rows})
}

fn brief_session(session: &Value) -> Value {
    let mut row = Map::new();
    for key in [
        "agentName",
        "title",
        "globalRef",
        "agentSessionId",
        "zmxName",
        "projectName",
        "projectPath",
        "projectId",
        "status",
    ] {
        if let Some(value) = session.get(key).filter(|value| !is_blank(value)) {
            row.insert(key.to_string(), value.clone());
        }
    }
    if !row.contains_key("agentName") {
        if let Some(agent_id) = session.get("agentId").filter(|value| !is_blank(value)) {
            row.insert("agentName".to_string(), agent_id.clone());
        }
    }
    Value::Object(row)
}

fn is_blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}
