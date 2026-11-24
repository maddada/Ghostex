//! Rewind for a Native Codex chat: `thread/revert` on the app-server's own thread.

use super::*;

/// How many turns one `thread/turns/list` page asks for while looking for the target.
const TURN_PAGE_SIZE: u32 = 50;
/// Full pages carry every item of a turn (tool output included), so they are asked for in fewer turns.
const FULL_TURN_PAGE_SIZE: u32 = 10;
/// Ceiling on pages walked, so a malformed cursor can never spin the host.
const TURN_PAGE_LIMIT: usize = 400;

/// CDXC:SessionChat 2026-11-24 WHY:
/// Codex 0.172's app-server no longer has `thread/rollback`; it reverts a paginated thread with `thread/revert { threadId, beforeTurnId }`, which writes a replacement rollout under the same thread id (its `session_meta.history_base` points into the old file) and leaves files alone. The chat names a user message (a Codex item id), so the turn is found through `thread/turns/list`, and a message that is not the first of its turn (a steer) is refused like the Terminal rewind refuses it, because Codex can only drop whole turns.
pub(super) fn rewind_codex_thread(
    transport: &CodexTransport,
    state: &Mutex<CodexState>,
    message_id: &str,
) -> Result<Value, String> {
    let thread_id = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.working || !s.pending_requests.is_empty() {
            return Err("ACP_AGENT_BUSY".to_string());
        }
        s.thread_id
            .clone()
            .ok_or_else(|| "Codex thread not initialized.".to_string())?
    };
    // The summary view lists only the first user message of each turn, which is the only kind
    // that can be reverted to; only a miss pays for the full view, to tell a steer apart.
    let before_turn_id = match find_turn_starting_with(transport, &thread_id, message_id, "summary")
    {
        Err(missing) if missing == "REWIND_NOT_FOUND" => {
            find_turn_starting_with(transport, &thread_id, message_id, "full")?
        }
        found => found?,
    };
    let reverted = transport
        .call(
            "thread/revert",
            json!({ "threadId": thread_id, "beforeTurnId": before_turn_id }),
            CODEX_RPC_TIMEOUT,
        )
        .map_err(|err| {
            if err.contains("unknown variant") {
                format!("REWIND_UNSUPPORTED: {err}")
            } else {
                format!("REWIND_FAILED: {err}")
            }
        })?;
    let path = reverted
        .pointer("/thread/path")
        .and_then(Value::as_str)
        .map(str::to_string);
    let child_pid = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if path.is_some() {
            s.thread_path = path.clone();
        }
        // The context meter described the dropped turns; the next turn reports it again.
        s.usage_used_tokens = None;
        s.child_pid
    };
    Ok(json!({
        "threadId": thread_id,
        "transcriptPath": path,
        "beforeTurnId": before_turn_id,
        "childPid": child_pid,
    }))
}

/// The id of the turn whose first user message is `message_id`, newest turns first.
fn find_turn_starting_with(
    transport: &CodexTransport,
    thread_id: &str,
    message_id: &str,
    items_view: &str,
) -> Result<String, String> {
    let mut cursor: Option<String> = None;
    for _ in 0..TURN_PAGE_LIMIT {
        let page = transport.call(
            "thread/turns/list",
            json!({
                "threadId": thread_id,
                "cursor": cursor,
                "limit": if items_view == "full" { FULL_TURN_PAGE_SIZE } else { TURN_PAGE_SIZE },
                "sortDirection": "desc",
                "itemsView": items_view,
            }),
            CODEX_RPC_TIMEOUT,
        )?;
        for turn in page
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let users: Vec<&str> = turn
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("userMessage"))
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .collect();
            let Some(position) = users.iter().position(|id| *id == message_id) else {
                continue;
            };
            let turn_id = turn.get("id").and_then(Value::as_str).unwrap_or_default();
            let root = turn
                .get("rootTurnId")
                .and_then(Value::as_str)
                .unwrap_or(turn_id);
            if position != 0 || root != turn_id || turn_id.is_empty() {
                return Err("REWIND_STEER".to_string());
            }
            return Ok(turn_id.to_string());
        }
        match page.get("nextCursor").and_then(Value::as_str) {
            Some(next) if Some(next) != cursor.as_deref() => cursor = Some(next.to_string()),
            _ => break,
        }
    }
    Err("REWIND_NOT_FOUND".to_string())
}
