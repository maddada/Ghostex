//! `ghostex agents wait-reply <session-ref> [--contains <text>] [--timeout <secs>]
//! [--after <message-id>|now]`: block until the agent's next reply finishes and print it.
//!
//! CDXC:AgentMessaging 2026-11-20 DECISION:
//! User, after the 11.12.0 smoke run (smoke-improvements page): every smoke step waited for a
//! reply with a hand-written polling loop over `read-session-chat`; "a built-in wait would make
//! each check one line and give a clean timeout message for the report". It rides the chat's own
//! long poll (`readSessionChat` with `waitMs` + `fingerprint`, the phone's and `help ask`'s way),
//! so gxserver holds each request until the chat changes instead of the CLI polling, and works
//! wherever `read-session-chat` does: Claude, Codex and the other chat agents, and sessions on a
//! connected computer (`/api/readSessionChat` is a peer endpoint).

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::arguments::Arguments;
use super::identity::{self, text};
use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::rpc::{CliError, CliResult, call_gxserver_rpc};
use crate::ghostex_cli::selector;

const DEFAULT_TIMEOUT_SECONDS: f64 = 600.0;
/// Rows read per request: enough to hold a long turn's final reply and the row it follows.
const READ_LIMIT: u64 = 200;
/// Rows per page while looking for an `--after` message id further back.
const ANCHOR_PAGE_LIMIT: u64 = 1000;
const ANCHOR_MAX_PAGES: usize = 50;
/// How long one read waits for the chat to change before the next.
const LONG_POLL_WAIT_MS: u64 = 20_000;
/// How much longer than its `waitMs` one request may take.
const LONG_POLL_MARGIN_MS: u64 = 15_000;
/// A finished reply is read once more after this long, so a turn that only paused between two
/// steps (working flips back on, or another row lands) is not taken for its end.
const SETTLE_WAIT_MS: u64 = 1_500;

/// Where the reply must come after: a row of the chat, or the start of an empty one.
#[derive(Clone)]
struct Anchor {
    id: Option<String>,
    byte_offset: Option<u64>,
    timestamp: Option<i64>,
}

impl Anchor {
    fn start() -> Self {
        Self {
            id: None,
            byte_offset: None,
            timestamp: None,
        }
    }

    fn at(row: &Value) -> Self {
        Self {
            id: Some(text(row, "id").to_string()).filter(|id| !id.is_empty()),
            byte_offset: row.get("byteOffset").and_then(Value::as_u64),
            timestamp: row.get("timestamp").and_then(Value::as_i64),
        }
    }

    /// The index of the first row after the anchor in `rows` (the newest page of the chat).
    fn first_after(&self, rows: &[Value]) -> usize {
        let Some(id) = &self.id else {
            return 0;
        };
        if let Some(index) = rows.iter().position(|row| text(row, "id") == id) {
            return index + 1;
        }
        // The anchor row is not on this page: older than all of it, or a row that was replaced.
        rows.iter()
            .position(|row| {
                match (
                    self.byte_offset,
                    row.get("byteOffset").and_then(Value::as_u64),
                ) {
                    (Some(anchor), Some(offset)) => offset > anchor,
                    _ => match (self.timestamp, row.get("timestamp").and_then(Value::as_i64)) {
                        (Some(anchor), Some(timestamp)) => timestamp > anchor,
                        _ => true,
                    },
                }
            })
            .unwrap_or(rows.len())
    }
}

/// A turn that ended with a reply.
struct Reply {
    text: String,
    message_id: String,
    timestamp: Option<i64>,
}

pub(super) fn wait_reply(args: &Arguments) -> CliResult<Value> {
    let reference = &args.positional[0];
    let timeout_seconds = match &args.timeout {
        Some(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
            .ok_or_else(|| {
                CliError::Other("--timeout takes a number of seconds above 0.".into())
            })?,
        None => DEFAULT_TIMEOUT_SECONDS,
    };
    let contains = match &args.contains {
        Some(needle) if needle.trim().is_empty() => {
            return Err(CliError::Other("--contains needs text to look for.".into()));
        }
        Some(needle) => Some(needle.to_lowercase()),
        None => None,
    };
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_seconds);
    let flags = identity::inventory_flags(&args.flags, reference)?;
    let session = selector::resolve_live_or_closed_session(reference, &flags)?;
    if !identity::is_agent(&session) {
        return Err(CliError::Other(
            "That session runs no agent, so it has no replies to wait for. Run ghostex agents list --all.".into(),
        ));
    }
    let global_ref = text(&session, "globalRef").to_string();
    let base = json!({
        "globalRef": session["globalRef"], "projectId": session["projectId"], "sessionId": session["sessionId"],
    });

    let first = read(&base, None, 0, &flags)?;
    let mut anchor = match args.after.as_deref().map(str::trim) {
        None | Some("now") => rows(&first)
            .iter()
            .rev()
            .find(|row| text(row, "source") == "transcript" && !text(row, "id").is_empty())
            .map(Anchor::at)
            .unwrap_or_else(Anchor::start),
        Some(id) => find_anchor(&base, &first, id, &flags)?.ok_or_else(|| {
            CliError::Other(format!(
                "{global_ref}'s chat has no message {id}. Take an id from ghostex read-session-chat {global_ref} --last 1 --json, or use --after now."
            ))
        })?,
    };

    let mut chat = first;
    let mut fingerprint: Option<String> = None;
    let mut working = chat["working"] == json!(true);
    loop {
        if let Some(reply) = finished_reply(&chat, &anchor) {
            let matched = contains
                .as_ref()
                .is_none_or(|needle| reply.text.to_lowercase().contains(needle));
            // Read once more: a turn between two steps can look finished for a moment.
            let settled = read(
                &base,
                current_fingerprint(&chat).as_deref(),
                SETTLE_WAIT_MS,
                &flags,
            )
            .ok();
            let still = settled
                .as_ref()
                .and_then(|settled| finished_reply(settled, &anchor))
                .is_some_and(|again| again.message_id == reply.message_id);
            if still {
                if matched {
                    return Ok(json!({
                        "ok": true,
                        "session": global_ref,
                        "agent": chat.get("agent").cloned().unwrap_or(Value::Null),
                        "messageId": reply.message_id,
                        "timestamp": reply.timestamp,
                        "reply": reply.text,
                    }));
                }
                // Not the reply asked for: wait for one after it.
                let rows = rows(&chat);
                if let Some(last) = rows
                    .iter()
                    .rev()
                    .find(|row| text(row, "id") == reply.message_id)
                {
                    anchor = Anchor::at(last);
                }
            }
            if let Some(settled) = settled {
                fingerprint = current_fingerprint(&settled);
                working = settled["working"] == json!(true);
                chat = settled;
                continue;
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let wanted = match &args.contains {
                Some(needle) => format!(" containing \"{needle}\""),
                None => String::new(),
            };
            let state = if working { " It is still working." } else { "" };
            return Err(CliError::Other(format!(
                "Timed out: no reply{wanted} from {global_ref} within {} seconds.{state}",
                format_seconds(timeout_seconds)
            )));
        }
        let wait_ms = (remaining.as_millis() as u64).clamp(1, LONG_POLL_WAIT_MS);
        match read(&base, fingerprint.as_deref(), wait_ms, &flags) {
            Ok(next) => {
                let next_fingerprint = current_fingerprint(&next);
                if next_fingerprint.is_none() {
                    // An older gxserver without the long poll answers at once: pace the reads.
                    std::thread::sleep(Duration::from_secs(1).min(remaining));
                }
                fingerprint = next_fingerprint;
                working = next["working"] == json!(true);
                chat = next;
            }
            // gxserver restarting or the connection to the other computer dropping: try again
            // until the deadline. An answer that refuses the read ends the wait.
            Err(CliError::Rpc { message, .. }) => return Err(CliError::Other(message)),
            Err(_) => {
                std::thread::sleep(Duration::from_secs(2).min(remaining));
                fingerprint = None;
            }
        }
    }
}

/// The reply that ended the chat's last turn, when that turn has ended and its reply came after
/// `anchor`: the assistant's text after the turn's last tool call (its last text when the turn
/// was all tool calls between texts).
fn finished_reply(chat: &Value, anchor: &Anchor) -> Option<Reply> {
    if chat["working"] == json!(true) {
        return None;
    }
    let rows = rows(chat);
    let after = anchor.first_after(rows);
    // The last row that is part of the conversation (not reasoning, system notes or the like).
    let last = rows
        .iter()
        .rposition(|row| matches!(text(row, "role"), "user" | "assistant" | "tool"))?;
    if last < after || text(&rows[last], "role") != "assistant" || row_text(&rows[last]).is_empty()
    {
        return None;
    }
    let turn_start = rows[..=last]
        .iter()
        .rposition(|row| text(row, "role") == "user")
        .map_or(0, |index| index + 1);
    let mut first_text = last;
    while first_text > turn_start {
        let row = &rows[first_text - 1];
        match text(row, "role") {
            "assistant" if !has_tool_call(row) => {
                if !row_text(row).is_empty() {
                    first_text -= 1;
                } else {
                    break;
                }
            }
            "user" | "assistant" | "tool" => break,
            // Reasoning and system rows between two texts belong to the reply's span.
            _ => first_text -= 1,
        }
    }
    let text = rows[first_text..=last]
        .iter()
        .filter(|row| text(row, "role") == "assistant")
        .map(row_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(Reply {
        text,
        message_id: text_of(&rows[last], "id"),
        timestamp: rows[last].get("timestamp").and_then(Value::as_i64),
    })
}

/// Pages back from the newest rows until the row with `id` shows up.
fn find_anchor(base: &Value, first: &Value, id: &str, flags: &Flags) -> CliResult<Option<Anchor>> {
    let mut page = first.clone();
    for _ in 0..ANCHOR_MAX_PAGES {
        if let Some(row) = rows(&page).iter().find(|row| text(row, "id") == id) {
            return Ok(Some(Anchor::at(row)));
        }
        let more = page.get("hasMore").and_then(Value::as_bool) == Some(true);
        let Some(before) = page
            .get("beforeOffset")
            .and_then(Value::as_u64)
            .filter(|_| more)
        else {
            return Ok(None);
        };
        let mut request = base.clone();
        request["beforeOffset"] = json!(before);
        request["limit"] = json!(ANCHOR_PAGE_LIMIT);
        request["historyMode"] = json!("detail");
        let next = call_gxserver_rpc("/api/readSessionChat", &request, flags)?;
        if next.get("beforeOffset").and_then(Value::as_u64) == Some(before) {
            page = next;
            if let Some(row) = rows(&page).iter().find(|row| text(row, "id") == id) {
                return Ok(Some(Anchor::at(row)));
            }
            return Ok(None);
        }
        page = next;
    }
    Ok(None)
}

/// One read of the newest rows; with a fingerprint, gxserver holds it up to `wait_ms` until the
/// chat changes.
fn read(base: &Value, fingerprint: Option<&str>, wait_ms: u64, flags: &Flags) -> CliResult<Value> {
    let mut params = base.clone();
    params["limit"] = json!(READ_LIMIT);
    let mut flags = flags.clone();
    if let Some(fingerprint) = fingerprint.filter(|_| wait_ms > 0) {
        params["fingerprint"] = json!(fingerprint);
        params["waitMs"] = json!(wait_ms);
        flags.insert_text("timeout", &(wait_ms + LONG_POLL_MARGIN_MS).to_string());
    }
    let chat = call_gxserver_rpc("/api/readSessionChat", &params, &flags)?;
    if chat.get("ok") == Some(&json!(false)) {
        return Err(CliError::Rpc {
            message: text(&chat, "error").to_string(),
            response: chat,
        });
    }
    Ok(chat)
}

fn current_fingerprint(chat: &Value) -> Option<String> {
    chat.get("fingerprint")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn rows(chat: &Value) -> &[Value] {
    chat.get("messages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn has_tool_call(row: &Value) -> bool {
    row.get("blocks")
        .and_then(Value::as_array)
        .is_some_and(|blocks| {
            blocks
                .iter()
                .any(|block| block.get("type").and_then(Value::as_str) == Some("tool-call"))
        })
}

/// A row's text blocks, without its tool calls.
fn row_text(row: &Value) -> String {
    row.get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_of(value: &Value, key: &str) -> String {
    text(value, key).to_string()
}

fn format_seconds(seconds: f64) -> String {
    if seconds.fract() == 0.0 {
        format!("{}", seconds as u64)
    } else {
        format!("{seconds}")
    }
}
