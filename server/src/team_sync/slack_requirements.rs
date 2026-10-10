//! The requirements in a new working thread's opening post: a short summary of the source thread,
//! written on the requester's Ghostex when it claims the `slack.request` command.
//!
//! CDXC:TeamSync 2026-10-09 DECISION:
//! User: the working thread's opening post lists requirements summarised from the source thread,
//! not a quote of its first message. Convex has no model, so the requester's Ghostex summarises
//! with a short `claude -p` call on a small model, bounded by a timeout, and Convex edits the post
//! (`chat.update`); when the summary fails the post keeps its quote.

use std::fs;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::logging::{GxserverLogInput, LogLevel};
use crate::platform::process::background_command;
use crate::server::AppState;

use crate::cloud_runner::run_with_stdin;
use super::connections::TeamConnection;
use super::convex_http::ConvexCallKind;
use super::operations::member_call;

const SUMMARY_TIMEOUT: Duration = Duration::from_secs(60);
const SUMMARY_MODEL: &str = "haiku";
const MAX_REQUIREMENTS: usize = 10;
const MAX_REQUIREMENT_CHARS: usize = 300;
/// The thread as Convex sent it is up to 30k characters; the summary needs the gist.
const MAX_TRANSCRIPT_CHARS: usize = 20_000;

fn text<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// Starts the summary in the background when this command opened the ticket's working thread from
/// a Slack thread; the session start does not wait for it.
pub(super) fn summarize_requirements_in_background(
    state: &AppState,
    connection: &TeamConnection,
    command_id: &str,
    payload: &Value,
) {
    let opened = payload.get("openedWorkingThread") == Some(&Value::Bool(true));
    let has_thread = text(payload, "/source/threadTs").is_some();
    let Some(transcript) = text(payload, "/transcript").map(str::to_string) else {
        return;
    };
    if !opened || !has_thread || payload.get("openingPost").is_none() {
        return;
    }
    let state = state.clone();
    let connection = connection.clone();
    let command_id = command_id.to_string();
    let payload = payload.clone();
    std::thread::spawn(move || {
        let outcome = summarize(&state, &payload, &transcript).and_then(|requirements| {
            let mut args = Map::new();
            args.insert("commandId".to_string(), json!(command_id));
            args.insert("requirements".to_string(), json!(requirements));
            member_call(
                &connection,
                ConvexCallKind::Action,
                "slackPost:updateOpeningPost",
                args,
            )
            .map(|_| ())
        });
        if let Err(error) = outcome {
            let _ = state.logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "teamSyncRequirementsSummaryFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error),
                details: Some(json!({ "commandId": command_id })),
            });
        }
    });
}

fn summarize(state: &AppState, payload: &Value, transcript: &str) -> Result<Vec<String>, String> {
    let program = crate::accounts::helpers::executable(&state.paths.home_dir, "claude")
        .ok_or("Claude Code (`claude`) is not installed on this computer.")?;
    // An empty folder of its own, so no project's CLAUDE.md or settings shape the summary.
    let cwd = state.paths.app_data_dir.join("slack-summaries");
    fs::create_dir_all(&cwd)
        .map_err(|error| format!("Could not create {}: {error}", cwd.display()))?;
    let mut command = background_command(program);
    // `--safe-mode` leaves out hooks, plugins and MCP servers (Ghostex's own hooks included),
    // `--no-session-persistence` keeps it out of the user's sessions and prompt history.
    command
        .args([
            "-p",
            "--model",
            SUMMARY_MODEL,
            "--safe-mode",
            "--tools",
            "",
            "--no-session-persistence",
            "--output-format",
            "text",
        ])
        .current_dir(&cwd);
    let answer = run_with_stdin(
        command,
        &summary_prompt(payload, transcript),
        SUMMARY_TIMEOUT,
    )
    .map_err(|error| format!("The requirements summary failed: {error}"))?;
    let requirements = requirement_lines(&answer);
    if requirements.is_empty() {
        return Err("The requirements summary had no bullet points.".to_string());
    }
    Ok(requirements)
}

fn summary_prompt(payload: &Value, transcript: &str) -> String {
    let ticket = match (text(payload, "/ticket/key"), text(payload, "/ticket/title")) {
        (Some(key), Some(title)) => format!("{key} · {title}"),
        (Some(key), None) => key.to_string(),
        _ => "the ticket".to_string(),
    };
    let transcript: String = if transcript.chars().count() > MAX_TRANSCRIPT_CHARS {
        let skip = transcript.chars().count() - MAX_TRANSCRIPT_CHARS;
        format!(
            "…(earlier messages cut)\n{}",
            transcript.chars().skip(skip).collect::<String>()
        )
    } else {
        transcript.to_string()
    };
    format!(
        "Summarise what this Slack thread asks the developer to do, as requirements for the ticket {ticket}.\n\n\
         Answer with 2 to 8 short bullet points, one per line, each starting with \"- \". Only the requirements: what must change or work, and how to tell it is done. No intro, no names, no greetings, no links unless a requirement needs one. Write in the thread's language.\n\n\
         The request: {}\n\nThe thread:\n{transcript}\n",
        text(payload, "/prompt").unwrap_or("(none; the thread says what to do)")
    )
}

/// The `- ` / `• ` / `* ` lines of the answer, without their bullets, clipped.
fn requirement_lines(answer: &str) -> Vec<String> {
    answer
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            ["- ", "• ", "* "]
                .iter()
                .find_map(|bullet| line.strip_prefix(bullet))
                .map(str::trim)
                .filter(|line| !line.is_empty())
        })
        .take(MAX_REQUIREMENTS)
        .map(|line| {
            if line.chars().count() > MAX_REQUIREMENT_CHARS {
                let mut clipped: String = line.chars().take(MAX_REQUIREMENT_CHARS - 1).collect();
                clipped.push('…');
                clipped
            } else {
                line.to_string()
            }
        })
        .collect()
}
