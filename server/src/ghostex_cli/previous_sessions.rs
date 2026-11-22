//! `ghostex previous-sessions` and `ghostex restore-previous-session`: the closed sessions a client
//! can bring back, and bringing one back, over the CLI for clients that reach the computer only
//! through SSH (Ghostex mobile).
//!
//! CDXC:Sessions 2026-10-01 WHY:
//! The phone's session search lists closed sessions like the desktop's Sessions search and restores
//! one when tapped, but the phone has no HTTP path to gxserver. Both verbs are thin doors onto the
//! daemon's own endpoints: `/api/listPreviousSessions` (the list the desktop pages through) and
//! `/api/restorePreviousSession` (the one restore every client uses), so a client never rebuilds
//! restore parameters itself.
//!
//! SEE-ALSO: apps/mobile/app/src/sessions/sessionSearch.ts (the phone's caller and matching rules),
//! server/src/previous_session_restore.rs.

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/// The page size the desktop's Sessions search asks for (`SESSIONS_PAGE_SIZE` in gx-core).
const DEFAULT_LIMIT: u64 = 80;
const MAX_LIMIT: u64 = 200;
/// The desktop's restore waits as long for gxserver to create the new session.
const RESTORE_TIMEOUT_MS: u64 = 30_000;

/// The row fields a client draws, matches and restores by; paths, commands and transcripts stay on
/// the computer.
const ROW_FIELDS: [&str; 19] = [
    "projectId",
    "sessionId",
    "projectTitle",
    "displayTitle",
    "primaryTitle",
    "terminalTitle",
    "title",
    "agentId",
    "agentIcon",
    "agentName",
    "agentSessionId",
    "sessionTag",
    "isFavorite",
    "closedAt",
    "lastActiveAt",
    "isRestorable",
    "restoreUnavailableReason",
    "externalSession",
    "forkBranchCount",
];

fn list_usage() -> &'static str {
    "Usage: ghostex previous-sessions [--query <text>] [--limit <n>] [--cursor <cursor>] [--project-id <id>] [--json]"
}

fn restore_usage() -> &'static str {
    "Usage: ghostex restore-previous-session --project-id <id> --session-id <id> [--json]"
}

fn wants_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "-h" || arg == "--help")
}

fn non_empty_text(flags: &Flags, key: &str) -> Option<String> {
    flags
        .string_value(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `ghostex previous-sessions`: one page of closed sessions, newest closed first.
pub fn previous_sessions_command(args: &[String]) -> CliResult<()> {
    if wants_help(args) {
        println!("{}", list_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let limit = flags
        .number("limit")
        .filter(|limit| limit.is_finite() && *limit >= 1.0)
        .map_or(DEFAULT_LIMIT, |limit| (limit as u64).min(MAX_LIMIT));
    let mut params = Map::new();
    params.insert("limit".to_string(), json!(limit));
    for (flag, param) in [
        ("query", "query"),
        ("cursor", "cursor"),
        ("projectId", "projectId"),
    ] {
        if let Some(value) = non_empty_text(flags, flag) {
            params.insert(param.to_string(), Value::String(value));
        }
    }
    let result = call_gxserver_rpc("/api/listPreviousSessions", &Value::Object(params), flags)?;
    if is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    let rows: Vec<Value> = result
        .get("results")
        .and_then(Value::as_array)
        .map(|results| results.iter().filter_map(compact_row).collect())
        .unwrap_or_default();
    let mut answer = json!({ "ok": true, "results": rows });
    if let Some(cursor) = result.get("cursor").filter(|cursor| cursor.is_string()) {
        answer["cursor"] = cursor.clone();
    }
    print_json(&answer);
    Ok(())
}

fn compact_row(row: &Value) -> Option<Value> {
    let row = row.as_object()?;
    row.get("sessionId")?.as_str()?;
    row.get("projectId")?.as_str()?;
    let compact: Map<String, Value> = ROW_FIELDS
        .iter()
        .filter_map(|key| {
            row.get(*key)
                .filter(|value| !value.is_null())
                .map(|value| ((*key).to_string(), value.clone()))
        })
        .collect();
    Some(Value::Object(compact))
}

/// `ghostex restore-previous-session`: bring a closed session back as a new session and print it;
/// its agent resumes when the session is opened.
pub fn restore_previous_session_command(args: &[String]) -> CliResult<()> {
    if wants_help(args) {
        println!("{}", restore_usage());
        return Ok(());
    }
    let mut flags = parse_args(args).flags;
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeoutMs", &RESTORE_TIMEOUT_MS.to_string());
    }
    let flags = &flags;
    let (Some(project_id), Some(session_id)) = (
        non_empty_text(flags, "projectId"),
        non_empty_text(flags, "sessionId"),
    ) else {
        return Err(CliError::Other(restore_usage().to_string()));
    };
    let result = call_gxserver_rpc(
        "/api/restorePreviousSession",
        &json!({
            "projectId": project_id,
            "reason": "restorePreviousSession",
            "sessionId": session_id,
        }),
        flags,
    )?;
    if is_failed_cli_result(&result) {
        crate::ghostex_cli::set_exit_code(1);
    }
    print_json(&result);
    Ok(())
}
