//! What this Ghostex does with a command from its member's queue.

use serde_json::{json, Value};

use crate::server::AppState;

use super::connections::TeamConnection;

pub(crate) enum CommandOutcome {
    Done(Value),
    Failed(String),
}

/// Whether this Ghostex runs commands of this type.
///
/// CDXC:TeamSync 2026-10-09 WHY:
/// A type this build does not know stays `pending` in the team's queue instead of failing, so a
/// newer Ghostex of the same member (or this one after an update) still picks it up.
pub(crate) fn handles_command_type(command_type: &str) -> bool {
    matches!(command_type, "ping" | "slack.request")
}

/// Runs one claimed command. Blocking; the runtime calls it from `spawn_blocking`.
///
/// CDXC:TeamSync 2026-10-25 WHY:
/// A `slack.request` types a prompt into this member's agent and can start work on their computer, so it runs only when the team's Slack flow queued it (`source` "slack", set by Convex, never by the caller). Teams still on older Convex functions let any teammate queue one through `commands:enqueue` (stored with `source` "ghostex"), so this check is what protects them until the owner redeploys (security audit 2026-10-25).
pub(crate) fn run_team_command(
    state: &AppState,
    connection: &TeamConnection,
    command: &Value,
) -> CommandOutcome {
    match command.get("type").and_then(Value::as_str).unwrap_or("") {
        "ping" => CommandOutcome::Done(json!({
            "pong": true,
            "workspaceId": connection.workspace_id,
            "serverId": state.metadata.server_id,
            "version": state.version,
            "at": chrono::Utc::now().to_rfc3339(),
        })),
        "slack.request" if command.get("source").and_then(Value::as_str) != Some("slack") => {
            CommandOutcome::Failed(
                "Slack requests run only when they come from the team's Slack flow; this one was queued by a teammate, so it was not run.".to_string(),
            )
        }
        "slack.request" => super::slack_request::run_slack_request(state, connection, command),
        other => CommandOutcome::Failed(format!("This Ghostex cannot run \"{other}\" commands.")),
    }
}
