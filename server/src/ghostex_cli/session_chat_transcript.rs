//! `read-session-chat --all | --grep | --role | --context | --since | --last | --format text`: a
//! whole thread, or the parts of it that matter, in one command.

use chrono::TimeZone;
use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{self, CliError, CliResult};
use crate::ghostex_cli::{actions, selector};

/// Rows per request while walking a thread. The daemon caps a read at 10,000.
const PAGE_LIMIT: u64 = 1000;
/// A thread longer than this many pages is almost certainly a cursor that stopped moving.
const MAX_PAGES: usize = 2000;
/// A tool call's one-line summary is cut here in text output.
const TOOL_SUMMARY_CHARS: usize = 160;

/// The flags that switch `read-session-chat` from one raw page to this reader.
pub fn wants_transcript_reader(args: &[String]) -> bool {
    args.iter().any(|arg| {
        [
            "--all",
            "--grep",
            "--role",
            "--context",
            "--before",
            "--after",
            "--format",
            "--since",
            "--until",
            "--last",
        ]
        .iter()
        .any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))
    })
}

/*
CDXC:SessionChat 2026-09-24 DECISION:
The user asked that an agent reading another thread (for example "did we already talk about this
there?") do it with one CLI command instead of a hand-written paging loop over raw JSON. `--all`
walks the thread from the newest turn back to the first, `--grep` searches it (and implies
`--all`), `--role` keeps only some speakers, `--context` keeps the rows around each match, and
`--format text` prints it as a readable conversation. Older turns come as the history pager's
turn rows (prompt, final reply, work collapsed) unless `--history-mode detail` asks for every row.
*/
pub fn read_session_chat_transcript(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let options = ReaderOptions::from_flags(flags)?;
    let Some(session_selector) = selector::session_selector_from_args(&parsed.rest, flags) else {
        return Err(CliError::Other(
            "read-session-chat needs a session: a title, session id, global ref, zmx name, agent session id, or the Session ID from Copy Details."
                .to_string(),
        ));
    };
    let mut base = Map::new();
    base.insert("sessionId".to_string(), Value::String(session_selector));
    if let Some(subagent) = &options.subagent {
        base.insert("subagent".to_string(), Value::String(subagent.clone()));
    }
    // Resolve the selector once; every page after the first reuses the resolved ids.
    let base = actions::with_resolved_gxserver_session_params(&Value::Object(base), flags)?;

    let mut first = base.clone();
    first["limit"] = json!(options.limit.unwrap_or(PAGE_LIMIT));
    if options.detail && options.subagent.is_none() {
        // `detail` without a cursor is the daemon's verbatim tail.
        first["historyMode"] = json!("detail");
    }
    let first_page = rpc::call_gxserver_rpc("/api/readSessionChat", &first, flags)?;
    if is_failed_cli_result(&first_page) {
        print_json(&first_page);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }

    let mut pages: Vec<Vec<Value>> = vec![page_messages(&first_page)];
    let mut cursor = first_page.get("beforeOffset").and_then(Value::as_u64);
    let mut has_more = first_page
        .get("hasMore")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if options.all {
        while has_more && pages.len() < MAX_PAGES {
            let Some(before) = cursor else { break };
            let mut request = base.clone();
            request["beforeOffset"] = json!(before);
            request["limit"] = json!(PAGE_LIMIT);
            if options.subagent.is_none() {
                request["historyMode"] = json!(if options.detail { "detail" } else { "turns" });
            }
            let page = rpc::call_gxserver_rpc("/api/readSessionChat", &request, flags)?;
            if is_failed_cli_result(&page) {
                print_json(&page);
                crate::ghostex_cli::set_exit_code(1);
                return Ok(());
            }
            pages.push(page_messages(&page));
            let next = page.get("beforeOffset").and_then(Value::as_u64);
            has_more = page
                .get("hasMore")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if next.is_none() || next == cursor {
                break;
            }
            cursor = next;
        }
    }
    let page_count = pages.len();
    let messages: Vec<Value> = pages.into_iter().rev().flatten().collect();
    let total = messages.len();
    let (selected, match_count) = options.select(&messages);

    if options.text {
        print!(
            "{}",
            render_text(
                &first_page,
                &messages,
                &selected,
                total,
                page_count,
                match_count,
                &options
            )
        );
        return Ok(());
    }
    let mut result = Map::new();
    result.insert("ok".to_string(), Value::Bool(true));
    for key in ["agent", "agentSessionId", "status", "working"] {
        if let Some(value) = first_page.get(key) {
            result.insert(key.to_string(), value.clone());
        }
    }
    result.insert("pages".to_string(), json!(page_count));
    result.insert("totalMessages".to_string(), json!(total));
    if options.grep.is_some() {
        result.insert("matches".to_string(), json!(match_count));
    }
    result.insert("hasMore".to_string(), json!(has_more));
    if let Some(before) = cursor.filter(|_| has_more) {
        result.insert("beforeOffset".to_string(), json!(before));
    }
    result.insert(
        "messages".to_string(),
        Value::Array(
            selected
                .iter()
                .filter_map(|line| match line {
                    Line::Row { index, matched } => {
                        let mut message = messages[*index].clone();
                        if *matched {
                            message["match"] = Value::Bool(true);
                        }
                        Some(message)
                    }
                    Line::Gap(_) => None,
                })
                .collect(),
        ),
    );
    print_json(&Value::Object(result));
    Ok(())
}

struct ReaderOptions {
    all: bool,
    detail: bool,
    text: bool,
    limit: Option<u64>,
    subagent: Option<String>,
    roles: Option<Vec<String>>,
    /// Lowercased alternatives; a row matches when its text contains any of them.
    grep: Option<Vec<String>>,
    /// Rows kept before and after each match (`--context` sets both).
    context_before: usize,
    context_after: usize,
    /// Epoch ms; rows outside `[since, until)` are left out.
    since_ms: Option<i64>,
    until_ms: Option<i64>,
    last: Option<usize>,
}

/// One printed line of the reader: a row (marked when `--grep` matched it) or a run of skipped
/// rows between two context windows.
enum Line {
    Row { index: usize, matched: bool },
    Gap(usize),
}

impl ReaderOptions {
    fn from_flags(flags: &Flags) -> CliResult<Self> {
        let history_mode = flags.text("historyMode");
        if let Some(mode) = history_mode.as_deref() {
            if !matches!(mode, "turns" | "detail") {
                return Err(CliError::Other(
                    "--history-mode must be turns or detail.".to_string(),
                ));
            }
        }
        let format = flags.text("format");
        if let Some(format) = format.as_deref() {
            if !matches!(format, "text" | "json") {
                return Err(CliError::Other(
                    "--format must be text or json.".to_string(),
                ));
            }
        }
        let grep = match flags.text("grep") {
            Some(pattern) => {
                let alternatives: Vec<String> = pattern
                    .split('|')
                    .map(|part| part.trim().to_lowercase())
                    .filter(|part| !part.is_empty())
                    .collect();
                if alternatives.is_empty() {
                    return Err(CliError::Other(
                        "--grep needs text to search for.".to_string(),
                    ));
                }
                Some(alternatives)
            }
            None if flags.contains("grep") => {
                return Err(CliError::Other(
                    "--grep needs text to search for.".to_string(),
                ))
            }
            None => None,
        };
        let roles = match flags.text("role") {
            Some(list) => {
                let roles: Vec<String> = list
                    .split(',')
                    .map(|role| role.trim().to_lowercase())
                    .filter(|role| !role.is_empty())
                    .collect();
                if let Some(unknown) = roles.iter().find(|role| {
                    !matches!(
                        role.as_str(),
                        "user" | "assistant" | "tool" | "system" | "reasoning" | "harness"
                    )
                }) {
                    return Err(CliError::Other(format!(
                        "Unknown --role {unknown}. Use user, assistant, tool, system, reasoning or harness, comma separated."
                    )));
                }
                Some(roles)
            }
            None => None,
        };
        let rows_flag = |name: &str| -> CliResult<Option<usize>> {
            match flags.text(name) {
                Some(value) => value
                    .trim()
                    .parse::<usize>()
                    .map(Some)
                    .map_err(|_| CliError::Other(format!("--{name} takes a number of rows."))),
                None => Ok(None),
            }
        };
        let context = rows_flag("context")?.unwrap_or(0);
        let context_before = rows_flag("before")?.unwrap_or(context);
        let context_after = rows_flag("after")?.unwrap_or(context);
        let limit = match flags.text("limit") {
            Some(value) => Some(
                value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| CliError::Other("--limit takes a number of rows.".to_string()))?,
            ),
            None => None,
        };
        let since_ms = match flags.text("since") {
            Some(value) => Some(parse_local_time("since", &value)?),
            None => None,
        };
        let until_ms = match flags.text("until") {
            Some(value) => Some(parse_local_time("until", &value)?),
            None => None,
        };
        let last = match flags.text("last") {
            Some(value) => Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| CliError::Other("--last takes a number of rows.".to_string()))?,
            ),
            None => None,
        };
        Ok(Self {
            all: flags.truthy("all") || grep.is_some() || since_ms.is_some() || until_ms.is_some(),
            detail: history_mode.as_deref() == Some("detail"),
            text: format.as_deref() == Some("text"),
            limit,
            subagent: flags
                .text("subagent")
                .filter(|value| !value.trim().is_empty()),
            roles,
            grep,
            context_before,
            context_after,
            since_ms,
            until_ms,
            last,
        })
    }

    /// The lines to print, in thread order, and how many rows matched `--grep`.
    fn select(&self, messages: &[Value]) -> (Vec<Line>, usize) {
        let in_window = |message: &Value| {
            if self.since_ms.is_none() && self.until_ms.is_none() {
                return true;
            }
            message
                .get("timestamp")
                .and_then(Value::as_i64)
                .is_some_and(|timestamp| {
                    self.since_ms.is_none_or(|since| timestamp >= since)
                        && self.until_ms.is_none_or(|until| timestamp < until)
                })
        };
        let eligible = |message: &Value| {
            in_window(message)
                && match &self.roles {
                    None => true,
                    Some(roles) => roles.iter().any(|role| role == effective_role(message)),
                }
        };
        let Some(alternatives) = &self.grep else {
            let mut rows: Vec<usize> = (0..messages.len())
                .filter(|index| eligible(&messages[*index]))
                .collect();
            if let Some(last) = self.last {
                rows = rows.split_off(rows.len().saturating_sub(last));
            }
            let lines = rows
                .into_iter()
                .map(|index| Line::Row {
                    index,
                    matched: false,
                })
                .collect();
            return (lines, 0);
        };
        let mut hits: Vec<usize> = (0..messages.len())
            .filter(|index| {
                let message = &messages[*index];
                eligible(message) && {
                    let text = message_text(message, usize::MAX).to_lowercase();
                    alternatives.iter().any(|needle| text.contains(needle))
                }
            })
            .collect();
        let match_count = hits.len();
        if let Some(last) = self.last {
            hits = hits.split_off(hits.len().saturating_sub(last));
        }
        let mut keep = vec![false; messages.len()];
        for hit in &hits {
            let start = hit.saturating_sub(self.context_before);
            let end = (hit + self.context_after).min(messages.len().saturating_sub(1));
            for (index, slot) in keep.iter_mut().enumerate().take(end + 1).skip(start) {
                // Context rows skip harness noise so a match is followed by the real reply.
                if index == *hit
                    || (effective_role(&messages[index]) != "harness"
                        && in_window(&messages[index]))
                {
                    *slot = true;
                }
            }
        }
        let mut lines = Vec::new();
        let mut previous: Option<usize> = None;
        for (index, kept) in keep.iter().enumerate() {
            if !kept {
                continue;
            }
            if let Some(last) = previous.filter(|last| index > last + 1) {
                lines.push(Line::Gap(index - last - 1));
            }
            lines.push(Line::Row {
                index,
                matched: hits.binary_search(&index).is_ok(),
            });
            previous = Some(index);
        }
        (lines, match_count)
    }
}

/// `--since` / `--until`: a local date (`2026-09-21`), a local date and time (`2026-09-21T18:00`,
/// `2026-09-21 18:00`), or an RFC 3339 instant.
fn parse_local_time(flag: &str, value: &str) -> CliResult<i64> {
    let value = value.trim();
    if let Ok(instant) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(instant.timestamp_millis());
    }
    let local = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M"))
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map(|date| date.and_hms_opt(0, 0, 0).unwrap_or_default())
        })
        .map_err(|_| {
            CliError::Other(format!(
                "--{flag} takes a local date or time like 2026-09-21 or 2026-09-21T18:00."
            ))
        })?;
    chrono::Local
        .from_local_datetime(&local)
        .earliest()
        .map(|time| time.timestamp_millis())
        .ok_or_else(|| CliError::Other(format!("--{flag} {value} is not a valid local time.")))
}

fn page_messages(page: &Value) -> Vec<Value> {
    page.get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The row's role, except that a prompt the agent's harness injected (a task notification, a
/// system reminder, a local command's output) reads as `harness`, not as something the user typed.
pub(crate) fn effective_role(message: &Value) -> &str {
    let role = message.get("role").and_then(Value::as_str).unwrap_or("");
    if role == "user"
        && crate::session_chat_decode_claude::is_known_harness_injected_user_turn_text(
            &message_text(message, usize::MAX),
        )
    {
        return "harness";
    }
    role
}

/// The row's readable text: its text blocks, one line per tool call, and a note for collapsed
/// work. Tool results are left out; `--history-mode detail --format json` has them.
pub(crate) fn message_text(message: &Value, tool_summary_chars: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in message
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    let text = text.trim();
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
            }
            Some("tool-call") => {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                let summary = tool_call_summary(block.get("input"));
                let line = if summary.is_empty() {
                    format!("→ {name}")
                } else {
                    format!("→ {name}: {summary}")
                };
                parts.push(truncate_chars(&line, tool_summary_chars));
            }
            Some("image-ref") => parts.push("[image]".to_string()),
            _ => {}
        }
    }
    if let Some(work) = message.get("deferredWork") {
        let count = work
            .get("messageCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let files: Vec<&str> = work
            .get("filePaths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut note = format!("[{count} rows of work collapsed");
        if !files.is_empty() {
            note.push_str(&format!("; files: {}", files.join(", ")));
        }
        note.push(']');
        parts.push(truncate_chars(&note, tool_summary_chars.max(400)));
    }
    parts.join("\n")
}

fn tool_call_summary(input: Option<&Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    let parsed;
    let input = match input.as_str() {
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(value) => {
                parsed = value;
                &parsed
            }
            Err(_) => {
                // Codex's code mode runs shell commands from a JavaScript script; name the commands, not the script's first line.
                let commands =
                    crate::session_transcript_export::javascript_string_fields(text, "cmd");
                if commands.is_empty() {
                    return first_line(text);
                }
                return commands
                    .iter()
                    .map(|command| first_line(command))
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ");
            }
        },
        None => input,
    };
    for key in [
        "description",
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "query",
        "prompt",
    ] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            let line = first_line(value);
            if !line.is_empty() {
                return line;
            }
        }
    }
    String::new()
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn format_timestamp(message: &Value) -> String {
    message
        .get("timestamp")
        .and_then(Value::as_i64)
        .and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single())
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "(no time)".to_string())
}

fn render_text(
    first_page: &Value,
    messages: &[Value],
    selected: &[Line],
    total: usize,
    pages: usize,
    match_count: usize,
    options: &ReaderOptions,
) -> String {
    let agent = first_page
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or("agent");
    let mut header = format!("{agent} thread");
    if let Some(id) = first_page.get("agentSessionId").and_then(Value::as_str) {
        header.push_str(&format!(" {id}"));
    }
    header.push_str(&format!(
        " · {total} rows read in {pages} page(s) · times are local (UTC{})",
        chrono::Local::now().format("%:z")
    ));
    if let Some(last_user) = messages
        .iter()
        .rev()
        .find(|message| effective_role(message) == "user")
    {
        header.push_str(&format!(
            " · last user message {}",
            format_timestamp(last_user)
        ));
    }
    if options.grep.is_some() {
        header.push_str(&format!(" · {match_count} match(es)"));
    }
    if !options.all {
        header.push_str(" · newest page only (add --all for the whole thread)");
    }
    let mut out = format!("{header}\n");
    for line in selected {
        let (index, matched) = match line {
            Line::Row { index, matched } => (*index, *matched),
            Line::Gap(skipped) => {
                out.push_str(&format!("\n   … {skipped} row(s) skipped …\n"));
                continue;
            }
        };
        let message = &messages[index];
        let role = effective_role(message);
        let text = if role == "harness" {
            truncate_chars(&first_line(&message_text(message, usize::MAX)), 120)
        } else {
            message_text(message, TOOL_SUMMARY_CHARS)
        };
        if text.is_empty() {
            continue;
        }
        let queued = if message.get("queued").and_then(Value::as_bool) == Some(true) {
            " (queued)"
        } else {
            ""
        };
        let marker = if matched { " · match" } else { "" };
        out.push_str(&format!(
            "\n── {} · {role}{queued}{marker} ──\n{text}\n",
            format_timestamp(message)
        ));
    }
    out
}
