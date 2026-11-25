//! Rewind for a Native Pi chat: `get_fork_messages` + `fork { entryId }` over Pi RPC.

use super::*;

/// Rewinds the live Pi RPC session to immediately before `message_id` using Pi's `fork` command,
/// which creates a branched session file containing only entries preceding `entryId` and switches
/// the live RPC process onto it. Leaves files in the working tree untouched.
pub(super) fn rewind_pi_session(
    transport: &PiRpcTransport,
    state: &Mutex<PiState>,
    message_id: &str,
) -> Result<Value, String> {
    let entry_id = message_id.trim();
    if entry_id.is_empty() {
        return Err("REWIND_NOT_FOUND".to_string());
    }
    {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.working || !s.pending_requests.is_empty() {
            return Err("ACP_AGENT_BUSY".to_string());
        }
    }

    let forkable = transport
        .call("get_fork_messages", json!({}), PI_RPC_TIMEOUT)
        .map_err(|err| format!("REWIND_FAILED: {err}"))?;
    let is_forkable = forkable
        .get("messages")
        .and_then(Value::as_array)
        .is_some_and(|list| {
            list.iter().any(|item| {
                item.get("entryId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id == entry_id)
            })
        });
    if !is_forkable {
        return Err("REWIND_NOT_FOUND".to_string());
    }

    let forked = transport
        .call("fork", json!({ "entryId": entry_id }), PI_RPC_TIMEOUT)
        .map_err(|err| format!("REWIND_FAILED: {err}"))?;
    if forked.get("cancelled").and_then(Value::as_bool) == Some(true) {
        return Err("REWIND_FAILED: Pi cancelled the rewind.".to_string());
    }

    let fresh_state = transport
        .call("get_state", json!({}), PI_RPC_TIMEOUT)
        .map_err(|err| format!("REWIND_FAILED: {err}"))?;
    let (new_session_id, new_path, child_pid) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.apply_get_state(&fresh_state);
        s.usage_used_tokens = None;
        s.last_error = None;
        (
            s.native_session_id.clone(),
            s.session_file.clone(),
            s.child_pid,
        )
    };
    Ok(json!({
        "sessionId": new_session_id,
        "transcriptPath": new_path,
        "childPid": child_pid,
    }))
}
