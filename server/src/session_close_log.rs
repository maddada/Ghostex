//! One gxserver log line for every session close, saying where the close came from.
//!
//! CDXC:Sessions 2026-11-02 DECISION:
//! User: log where every session close came from (the key or Cmd+W, a tab's ×, the tab menu, a
//! sidebar row or its menu, a multi-selection, Close Inactive or Close Group, Running Sessions, the
//! chat's Close, `ghostex kill`, the phone, a coordinator's close, and the automatic closes such as
//! Handoff, Close After Done and cleanup), one line per close in the gxserver log with the session
//! ref and the source, so the next unexplained close is answered at once. The Main Coordinator
//! closed on 2026-11-02 with nothing in the log to say which door did it. Each client passes
//! `closeSource` with its close request (and the CLI `closedByRef`, the session that ran it);
//! the line is written whatever the diagnostics switches say, because it is rare and holds ids only.
//! SEE-ALSO: server/src/server/zmx_http.rs (writes it for `/api/killSession` and a
//! `/api/transitionSession` close), packages/gx-core/src/sidebar_actions/close.rs (the desktop and
//! web build's sources), server/src/ghostex_cli/wait/lifecycle.rs (`ghostex kill --close-source`),
//! apps/mobile/app/src/commands/ghostexCli.ts (the phone's).

use serde_json::{json, Map, Value};

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};

/// The close request's field naming the door it came through, such as `desktop.tabShortcut`.
pub const CLOSE_SOURCE_PARAM: &str = "closeSource";
/// The session that asked for the close, when a session did (`ghostex kill` run by an agent).
pub const CLOSED_BY_PARAM: &str = "closedByRef";

/// A source or ref as the line keeps it: letters, digits and `.`, `_`, `:`, `-`, at most `max`
/// characters, so a caller cannot write anything else into the log.
fn token(value: Option<&Value>, max: usize) -> Option<String> {
    let text: String = value?
        .as_str()?
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
        .take(max)
        .collect();
    (!text.is_empty()).then_some(text)
}

/// Where a close came from: its `closeSource`, else the older `reason` its caller already sent,
/// else `unspecified` (a client that predates the field).
pub fn close_source(params: &Map<String, Value>) -> String {
    token(params.get(CLOSE_SOURCE_PARAM), 64)
        .or_else(|| token(params.get("reason"), 64))
        .unwrap_or_else(|| "unspecified".to_string())
}

/// The line's details: the session, the source, who asked, the endpoint and what came of it.
pub fn close_log_details(
    server_id: &str,
    project_id: &str,
    session_id: &str,
    params: &Map<String, Value>,
    endpoint: &str,
    outcome: &str,
) -> Value {
    let mut details = json!({
        "sessionRef": format!("{server_id}:{project_id}:{session_id}"),
        "closeSource": close_source(params),
        "endpoint": endpoint.trim_start_matches("/api/"),
        "outcome": outcome,
    });
    if let Some(reason) = token(params.get("reason"), 64) {
        details["reason"] = json!(reason);
    }
    if let Some(closed_by) = token(params.get(CLOSED_BY_PARAM), 128) {
        details[CLOSED_BY_PARAM] = json!(closed_by);
    }
    details
}

/// Writes the line. `outcome` is the session's state after the close (`stopped`, or `sleeping` for
/// a Claude cloud session a close keeps asleep) or `failed`.
pub fn log_session_close(
    logger: &GxserverLogger,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    params: &Map<String, Value>,
    endpoint: &str,
    outcome: &str,
) {
    let _ = logger.log_record(GxserverLogInput {
        level: LogLevel::Info,
        event: "sessionClosed".to_string(),
        server_id: Some(server_id.to_string()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(close_log_details(
            server_id, project_id, session_id, params, endpoint, outcome,
        )),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("object")
    }

    #[test]
    fn the_line_names_the_session_the_source_and_who_asked() {
        let details = close_log_details(
            "S90",
            "P3lv0",
            "G1pif",
            &params(json!({
                "closeSource": "desktop.tabShortcut",
                "reason": "closeTerminal",
                "closedByRef": "S90:P3lv0:G5tuk",
            })),
            "/api/transitionSession",
            "stopped",
        );
        assert_eq!(
            details,
            json!({
                "sessionRef": "S90:P3lv0:G1pif",
                "closeSource": "desktop.tabShortcut",
                "endpoint": "transitionSession",
                "outcome": "stopped",
                "reason": "closeTerminal",
                "closedByRef": "S90:P3lv0:G5tuk",
            })
        );
    }

    #[test]
    fn an_older_client_falls_back_to_its_reason_or_unspecified() {
        assert_eq!(
            close_source(&params(json!({ "reason": "gpui-sidebar" }))),
            "gpui-sidebar"
        );
        assert_eq!(close_source(&params(json!({}))), "unspecified");
        // Only plain tokens reach the log.
        assert_eq!(
            close_source(&params(json!({ "closeSource": "phone.menu; rm -rf /" }))),
            "phone.menurm-rf"
        );
    }
}
