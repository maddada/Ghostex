/*
CDXC:SessionChat 2026-09-10 DECISION:
User: a slash command sent from chat has to appear IN chat, the way PR #121's
`!` local commands do, and it has to still be there after a reload.

The agent CLIs cannot supply that. Claude records a `<command-name>` envelope
for only a handful of its commands (`/model`, `/effort`, `/compact`, `/mcp`,
`/plan`, `/usage`, `/login`); Codex records nothing for any of them; and
`/rename` — the command that prompted this — writes no transcript row at all.
So the durable record is ours, and it is the ONE exception to
session_chat_app_command.rs's "nothing here is ever persisted": that store is a
five-minute acknowledgement of what Ghostex typed, this file is the archive of
what the USER typed.

Each row replays as the envelope the CLI would have written, which is what buys
these rows their whole presentation for free: the client's noise classifier
already renders `<command-name>` as a `Slash command` row and
`<local-command-stdout>` as `Local command output`, already folds `/model` and
`/effort` into their status pills, and already retires the live app-command
acknowledgement when a matching envelope reaches the messages. Nothing new
renders them.

SEE-ALSO: server/src/session_chat_app_command.rs (the live acknowledgement
these retire), server/src/session_chat_read.rs (the merge into a page),
packages/core-ui/chat/session-chat-local-command-transcript.ts (the client's
half of the escaped-markup contract).
*/

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::session_chat::{
    SessionChatBlock, SessionChatMessage, SessionChatRole, SessionChatSource,
};

/// Rows replayed per session. Deep history is the transcript's job; this is the
/// command trail beside it, and a session that ran more than this many local
/// commands does not need its oldest ones back.
const LOCAL_COMMAND_LIMIT: usize = 200;
/// Output is appended as a second record under the same id, so the file holds
/// more lines than rows. Compaction rewrites it once it passes this.
const LOCAL_COMMAND_LINE_LIMIT: usize = 600;
/// Same ceiling `codex_command_output` applies to a screen diff.
const LOCAL_COMMAND_OUTPUT_CHARS: usize = 24_000;

/// The marker attribute PR #121 introduced for Codex's `!` commands: it tells
/// the client the payload is HTML-escaped, so a command or an output carrying
/// `<…>` survives the markup strip instead of being eaten by it.
pub(crate) const ESCAPED_MARKUP_ATTRIBUTE: &str = "data-ghostex-escaped=\"html\"";

#[derive(Clone, Debug)]
pub struct SessionChatLocalCommand {
    /// `<sent_at_ms>-<sequence>`: unique per session and stable once written,
    /// so the output record can find its row and the client can dedupe.
    pub id: String,
    /// Command token including the slash, e.g. `/rename`.
    pub command: String,
    /// Everything after the token, verbatim. Empty for a bare command.
    pub args: String,
    /// What the CLI printed, once the screen diff settled. `None` until then.
    pub output: Option<String>,
    pub sent_at_ms: i64,
}

impl SessionChatLocalCommand {
    fn from_value(value: &Value) -> Option<Self> {
        let record = value.as_object()?;
        let id = record.get("id").and_then(Value::as_str)?.to_string();
        let command = record.get("command").and_then(Value::as_str)?.to_string();
        if id.is_empty() || command.is_empty() {
            return None;
        }
        Some(Self {
            id,
            command,
            args: record
                .get("args")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            output: record
                .get("output")
                .and_then(Value::as_str)
                .map(str::to_string),
            sent_at_ms: record.get("sentAt").and_then(Value::as_i64).unwrap_or(0),
        })
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".to_string(), json!(self.id));
        map.insert("command".to_string(), json!(self.command));
        map.insert("args".to_string(), json!(self.args));
        if let Some(output) = self.output.as_deref() {
            map.insert("output".to_string(), json!(output));
        }
        map.insert("sentAt".to_string(), json!(self.sent_at_ms));
        Value::Object(map)
    }

    /// `/rename pilot game work` — what the user typed, for the client's
    /// dedupe against the live app-command row.
    pub fn text(&self) -> String {
        if self.args.is_empty() {
            self.command.clone()
        } else {
            format!("{} {}", self.command, self.args)
        }
    }
}

/*
The line-leading slash law, matching `classifySessionChatSend` on the client:
untrimmed, so a leading space is prose, and the token has to look like a
command name rather than an absolute path — `/Users/sven/notes.md` pasted as a
whole message is a path, and archiving it as a command would put a permanent
lie in the trail. `/plugin:deploy` and `/ghostex-auto-rename-session` pass.
*/
pub fn parse_session_chat_local_command(text: &str) -> Option<(String, String)> {
    let (token, args) = match text.find(char::is_whitespace) {
        Some(at) => (&text[..at], text[at..].trim()),
        None => (text, ""),
    };
    let name = token.strip_prefix('/')?;
    if name.is_empty() || !name.starts_with(|ch: char| ch.is_ascii_alphanumeric()) {
        return None;
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-' | '.'))
    {
        return None;
    }
    Some((token.to_string(), args.to_string()))
}

/// Commands whose one result row the CLI's own transcript already carries:
/// Claude's `/model`, `/effort` and `/fast` status pills, and the compaction row
/// for `/compact` (Codex's `ContextCompaction` record). Archiving them would add
/// a second, screen-scraped result under the authoritative one.
pub fn transcript_records_command_result(command: &str) -> bool {
    matches!(
        command.to_ascii_lowercase().as_str(),
        "/model" | "/effort" | "/fast" | "/compact"
    )
}

fn session_file(project_id: &str, session_id: &str) -> Option<PathBuf> {
    let project = sanitize_id(project_id)?;
    let session = sanitize_id(session_id)?;
    Some(
        ghostex_paths::GhostexPaths::resolve()
            .gxserver_state_dir()
            .join("session-commands")
            .join(project)
            .join(format!("{session}.jsonl")),
    )
}

fn sanitize_id(id: &str) -> Option<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                    ch
                } else {
                    '_'
                }
            })
            .collect(),
    )
}

fn append(project_id: &str, session_id: &str, row: &SessionChatLocalCommand) -> Option<()> {
    let path = session_file(project_id, session_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok()?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    writeln!(file, "{}", row.to_value()).ok()?;
    Some(())
}

/// Archive one command the user sent from chat. Returns the row id, which the
/// caller carries so the settled output can be attached to the same row.
pub fn record_session_chat_local_command(
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Option<String> {
    let (command, args) = parse_session_chat_local_command(text)?;
    let sent_at_ms = chrono::Utc::now().timestamp_millis();
    let existing = load_session_chat_local_commands(project_id, session_id);
    let sequence = existing
        .iter()
        .filter(|row| row.sent_at_ms == sent_at_ms)
        .count();
    let row = SessionChatLocalCommand {
        id: format!("{sent_at_ms}-{sequence}"),
        command,
        args,
        output: None,
        sent_at_ms,
    };
    append(project_id, session_id, &row)?;
    compact_if_needed(project_id, session_id, existing.len() + 1);
    Some(row.id)
}

/// Attach (or replace) the output of an archived command. Called every time the
/// screen probe refines a result, so the last record for an id wins.
pub fn attach_session_chat_local_command_output(
    project_id: &str,
    session_id: &str,
    id: &str,
    output: &str,
) {
    let rows = load_session_chat_local_commands(project_id, session_id);
    let Some(row) = rows.iter().find(|row| row.id == id) else {
        return;
    };
    let output: String = output.chars().take(LOCAL_COMMAND_OUTPUT_CHARS).collect();
    if row.output.as_deref() == Some(output.as_str()) {
        return;
    }
    let updated = SessionChatLocalCommand {
        output: Some(output),
        ..row.clone()
    };
    append(project_id, session_id, &updated);
}

/// Archived rows, oldest first, one per id with the last record's output.
pub fn load_session_chat_local_commands(
    project_id: &str,
    session_id: &str,
) -> Vec<SessionChatLocalCommand> {
    let Some(path) = session_file(project_id, session_id) else {
        return Vec::new();
    };
    let Ok(file) = fs::File::open(&path) else {
        return Vec::new();
    };
    let mut rows: Vec<SessionChatLocalCommand> = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Some(row) = serde_json::from_str::<Value>(&line)
            .ok()
            .as_ref()
            .and_then(SessionChatLocalCommand::from_value)
        else {
            continue;
        };
        match rows.iter_mut().find(|existing| existing.id == row.id) {
            Some(existing) => *existing = row,
            None => rows.push(row),
        }
    }
    rows.sort_by(|left, right| {
        left.sent_at_ms
            .cmp(&right.sent_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    if rows.len() > LOCAL_COMMAND_LIMIT {
        rows.drain(..rows.len() - LOCAL_COMMAND_LIMIT);
    }
    rows
}

/// Rewrite the file as one line per surviving row once appended updates have
/// made it longer than its rows justify.
fn compact_if_needed(project_id: &str, session_id: &str, row_count: usize) {
    let Some(path) = session_file(project_id, session_id) else {
        return;
    };
    let Ok(file) = fs::File::open(&path) else {
        return;
    };
    let lines = BufReader::new(file).lines().map_while(Result::ok).count();
    if lines <= LOCAL_COMMAND_LINE_LIMIT && row_count <= LOCAL_COMMAND_LIMIT {
        return;
    }
    let rows = load_session_chat_local_commands(project_id, session_id);
    let body = rows
        .iter()
        .map(|row| row.to_value().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let temporary = path.with_extension("jsonl.tmp");
    if fs::write(&temporary, format!("{body}\n")).is_ok() {
        let _ = fs::rename(&temporary, &path);
    }
}

/// One archived command as the transcript rows the CLI would have written.
fn local_command_messages(row: &SessionChatLocalCommand) -> Vec<SessionChatMessage> {
    let mut command = escaped_marker("command-name", &row.command);
    if !row.args.is_empty() {
        // The space is load-bearing: the client reads these rows by stripping
        // their markup, and without it the name and its arguments would be
        // concatenated into one word.
        command.push(' ');
        command.push_str(&escaped_marker("command-args", &row.args));
    }
    let mut messages = vec![message(
        format!("local-command:{}", row.id),
        command,
        row.sent_at_ms,
    )];
    if let Some(output) = row.output.as_deref().filter(|output| !output.is_empty()) {
        messages.push(message(
            format!("local-command:{}:output", row.id),
            escaped_marker("local-command-stdout", output),
            // One millisecond later so the result can never sort above the
            // command that produced it.
            row.sent_at_ms.saturating_add(1),
        ));
    }
    messages
}

fn message(id: String, text: String, timestamp: i64) -> SessionChatMessage {
    SessionChatMessage {
        id,
        role: SessionChatRole::User,
        blocks: vec![SessionChatBlock::Text { text }],
        timestamp: Some(timestamp),
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
    }
}

/*
CDXC:SessionChat 2026-09-10 WHY:
The lines a local command added to the screen, found by anchoring on the tail of
the pre-send capture rather than by requiring the whole historical prefix to
stay byte-identical: Codex repaints old `/status` cells with refreshed
rate-limit timestamps, and Claude repaints its own footer, so a strict prefix
comparison reported the entire screen as new. Shared by both extractors below
so one proven anchor search serves every agent.
*/
pub(crate) fn newly_printed_lines(before: &[String], after: &[String]) -> Option<Vec<String>> {
    let boundary = (before.len().saturating_sub(16)..before.len()).find_map(|start| {
        let anchor = &before[start..];
        if !anchor
            .iter()
            .any(|line| line.chars().any(char::is_alphanumeric))
            || after.len() < anchor.len()
        {
            return None;
        }
        after
            .windows(anchor.len())
            .enumerate()
            .filter(|(_, window)| *window == anchor)
            .min_by_key(|(index, _)| index.abs_diff(start))
            .map(|(index, _)| index + anchor.len())
    })?;
    Some(after[boundary..].to_vec())
}

/// What one local command printed, for the agent whose screen this is. Codex
/// keeps its own extractor: its dialogs own the screen and get their own card.
pub fn session_chat_local_command_output(
    agent: Option<&str>,
    command: &str,
    before: &str,
    after: &str,
) -> Option<String> {
    if agent == Some("codex") {
        return crate::session_chat_codex_dialog::codex_command_output(before, after);
    }
    let before_history = screen_history(before);
    let after_history = screen_history(after);
    let mut cut_off = false;
    /*
    Claude echoes the command it intercepted onto its own composer line and
    hangs the result under it as an indented `⎿` branch, so the diff reads
    [blank, echo, result…] — the opposite of Codex, where a composer line means
    a prompt was submitted and everything after it belongs to the transcript.
    Take what the echo owns: the branch under it, ending at the first line that
    starts a new column, which is where an agent turn the transcript already
    records would begin. With no echo in the diff there is nothing to attribute
    it to, so the whole diff is the result.
    */
    let result: Vec<String> = match newly_printed_lines(&before_history, &after_history) {
        Some(printed) => match printed.iter().rposition(|line| is_composer_line(line)) {
            Some(echo) => result_branch(&printed[echo + 1..]),
            None => printed,
        },
        /*
        CDXC:SessionChat 2026-09-10 DECISION:
        User: output taller than the chat-view grid shows the part still on
        screen rather than nothing, inside the same expandable row.
        Claude draws on the alternate screen, which keeps no scrollback, and
        chat view shrinks that grid to ~24 rows, so a 35-line `/context` pushes
        the pre-send lines the anchor needs off the top. Its own echo still
        attributes the result while it is visible; once even the echo is gone
        and none of the pre-send tail survives anywhere, everything left on the
        grid was printed after the send, so it is this command's output with
        its top cut off, and it says so. Any other anchor miss — a repaint that
        kept the old lines — attributes nothing.
        */
        None => {
            if let Some(echo) = after_history
                .iter()
                .rposition(|line| is_echo_of(line, command))
            {
                result_branch(&after_history[echo + 1..])
            } else if scrolled_past(&before_history, &after_history) {
                cut_off = true;
                after_history
            } else {
                return None;
            }
        }
    };
    let mut lines: Vec<String> = result
        .iter()
        .map(|line| strip_result_gutter(line))
        .filter(|line| !is_rule_line(line))
        .collect();
    // A panel still open when captured (`/status`) ends in its own key hint.
    if lines.last().is_some_and(|line| is_key_hint(line)) {
        lines.pop();
    }
    let output = lines.join("\n");
    let output = output.trim();
    if output.is_empty() {
        return None;
    }
    let output = if cut_off {
        format!("…\n{output}")
    } else {
        output.to_string()
    };
    Some(output.chars().take(LOCAL_COMMAND_OUTPUT_CHARS).collect())
}

fn result_branch(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .take_while(|line| is_result_branch(line))
        .cloned()
        .collect()
}

/// This command's own echo on a composer line, e.g. `❯ /context`.
fn is_echo_of(line: &str, command: &str) -> bool {
    if !is_composer_line(line) {
        return false;
    }
    let cleaned = line
        .trim()
        .trim_matches(['│', '┃', '▌'])
        .trim()
        .trim_start_matches(['❯', '›', '>'])
        .trim();
    !command.trim().is_empty() && cleaned == command.trim()
}

/// None of the last pre-send lines survive anywhere on the new grid, so every
/// line on it was printed after the send.
fn scrolled_past(before: &[String], after: &[String]) -> bool {
    let tail: Vec<&String> = before
        .iter()
        .rev()
        .filter(|line| line.chars().any(char::is_alphanumeric))
        .take(16)
        .collect();
    !after.is_empty() && !tail.is_empty() && !tail.iter().any(|line| after.contains(line))
}

/// The screen as comparable lines, with the composer and everything the CLI
/// paints below it (rules, statusline, hints) removed — that chrome repaints on
/// its own schedule and is not output.
fn screen_history(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| {
            crate::session_chat_options::normalize_spaces(
                &crate::session_chat_options::strip_ansi_sgr(line),
            )
            .trim_end()
            .to_string()
        })
        .collect();
    if let Some(composer) = lines.iter().rposition(|line| is_composer_line(line)) {
        lines.truncate(composer);
    }
    while lines.last().is_some_and(|line| {
        line.trim().is_empty() || is_frame_line(line) || is_right_aligned_notice(line)
    }) {
        lines.pop();
    }
    lines
}

/*
CDXC:SessionChat 2026-09-10 WHY:
Claude paints a right-aligned notice directly above its composer ("● high ·
/effort", "You've used 82% of your weekly limit") and swaps it between any two
captures. Left in the history it became the last line of every anchor, so a
`/rename` whose result was plainly on screen diffed to nothing, and a later
probe archived the notice itself as `/status`'s output. It is chrome: a result
line is indented by a gutter, never pushed to the right edge.
*/
fn is_right_aligned_notice(line: &str) -> bool {
    let indent = line.len() - line.trim_start().len();
    indent >= 40 && !line.trim().is_empty()
}

/// A composer input line: `❯` for Claude and most CLIs, `›` for Codex, and the
/// `>` of a box-drawn composer — matched only inside its box, because a bare
/// `>` at the start of a line is a markdown quote in the agent's own answer.
fn is_composer_line(line: &str) -> bool {
    let trimmed = line.trim();
    let cleaned = trimmed.trim_matches(['│', '┃', '▌']).trim();
    cleaned.starts_with('❯')
        || cleaned.starts_with('›')
        || (trimmed.contains('│') && cleaned.starts_with('>'))
}

/// A rule or box edge the CLI draws around its composer. Claude's top rule
/// carries the session title, so this cannot require the line to be pure
/// box-drawing.
fn is_frame_line(line: &str) -> bool {
    line.trim().starts_with(['─', '━', '╭', '╮', '╰', '╯'])
}

/// A panel's drawn edge (`▔▔▔`, `───`): drawing, not output.
fn is_rule_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| matches!(ch, '▔' | '▁' | '─' | '━' | '═'))
}

fn is_key_hint(line: &str) -> bool {
    line.trim().to_ascii_lowercase().starts_with("esc to ")
}

/// A line hanging off the echoed command: blank, gutter-drawn, or indented.
fn is_result_branch(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with(['⎿', '└', '├']) || line.starts_with([' ', '\t'])
}

/// Claude prints a command's result under a gutter glyph (`⎿`). The glyph is
/// the terminal's tree drawing, not part of the sentence.
fn strip_result_gutter(line: &str) -> String {
    let trimmed = line.trim_start();
    for glyph in ['⎿', '└', '├', '⏺'] {
        if let Some(rest) = trimmed.strip_prefix(glyph) {
            return rest.trim_start().to_string();
        }
    }
    line.to_string()
}

pub(crate) fn escaped_marker(tag: &str, body: &str) -> String {
    let mut escaped = String::with_capacity(body.len());
    for character in body.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(character),
        }
    }
    format!("<{tag} {ESCAPED_MARKUP_ATTRIBUTE}>{escaped}</{tag}>")
}

/*
Which archived rows belong to the page being answered. A row is anchored to the
transcript message it FOLLOWS, never to raw time: pages are contiguous, so
"the last message at or before this row" identifies one page exactly, and a row
after the page's final message belongs to the live tail (there is nothing after
it anywhere) or to the next page up. The oldest page owns anything older than
the transcript itself, which is where a command sent before the CLI first
flushed lands.
*/
pub fn select_session_chat_local_commands(
    rows: Vec<SessionChatLocalCommand>,
    messages: &[SessionChatMessage],
    is_tail_page: bool,
    has_more: bool,
) -> Vec<SessionChatLocalCommand> {
    if rows.is_empty() {
        return Vec::new();
    }
    let stamps: Vec<i64> = messages
        .iter()
        .filter_map(|message| message.timestamp)
        .collect();
    let recorded = recorded_command_texts(messages);
    rows.into_iter()
        .filter(|row| !recorded.contains(&row.text()))
        .filter(|row| {
            let Some(&first) = stamps.first() else {
                // No timestamped transcript row to anchor against: the tail
                // page carries the trail, a pagination page never invents it.
                return is_tail_page;
            };
            if row.sent_at_ms < first {
                return !has_more;
            }
            let last = stamps.last().copied().unwrap_or(first);
            row.sent_at_ms < last || is_tail_page
        })
        .collect()
}

/*
The agent's own record wins. Claude writes a `<command-name>` envelope for the
commands it does record, and that row renders from the transcript; replaying
ours beside it would show the same command twice.
*/
fn recorded_command_texts(messages: &[SessionChatMessage]) -> Vec<String> {
    messages
        .iter()
        .filter(|message| message.role == SessionChatRole::User)
        .filter_map(|message| {
            let text = crate::session_chat_decode_claude::message_text(message);
            let name = between(&text, "<command-name>", "</command-name>")?;
            let args = between(&text, "<command-args>", "</command-args>").unwrap_or_default();
            Some(if args.is_empty() {
                name
            } else {
                format!("{name} {args}")
            })
        })
        .collect()
}

fn between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(text[start..end].trim().to_string())
}

/// Merge the selected rows into a page, keeping it ordered oldest first.
pub fn merge_session_chat_local_commands(
    messages: Vec<SessionChatMessage>,
    rows: &[SessionChatLocalCommand],
) -> Vec<SessionChatMessage> {
    if rows.is_empty() {
        return messages;
    }
    let mut merged = messages;
    for row in rows {
        for synthesized in local_command_messages(row) {
            let at = merged
                .iter()
                .rposition(|message| {
                    message.timestamp.unwrap_or(i64::MIN)
                        <= synthesized.timestamp.unwrap_or(i64::MIN)
                })
                .map(|index| index + 1)
                .unwrap_or(0);
            merged.insert(at, synthesized);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape a Claude screen capture has: scroll-back, then the composer
    /// rule, the input line, another rule, and the status footer.
    fn claude_screen(history: &[&str]) -> String {
        let mut lines: Vec<String> = history.iter().map(|line| line.to_string()).collect();
        lines.push("──────────────────────────────── pilot game work ─".to_string());
        lines.push("❯ ".to_string());
        lines.push("──────────────────────────────────────────────────".to_string());
        lines.push("   📁 Ghostex · 🤖 Opus 5 (1M context)/high · 🧠 21%".to_string());
        lines.push("  ⏵⏵ bypass permissions on (shift+tab to cycle)".to_string());
        lines.join("\n")
    }

    fn row(
        id: &str,
        command: &str,
        args: &str,
        output: Option<&str>,
        sent_at_ms: i64,
    ) -> SessionChatLocalCommand {
        SessionChatLocalCommand {
            id: id.to_string(),
            command: command.to_string(),
            args: args.to_string(),
            output: output.map(str::to_string),
            sent_at_ms,
        }
    }

    fn transcript_row(id: &str, text: &str, timestamp: i64) -> SessionChatMessage {
        SessionChatMessage {
            id: id.to_string(),
            role: SessionChatRole::User,
            blocks: vec![SessionChatBlock::Text {
                text: text.to_string(),
            }],
            timestamp: Some(timestamp),
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            queued: false,
        }
    }

    #[test]
    fn parses_only_line_leading_command_tokens() {
        assert_eq!(
            parse_session_chat_local_command("/rename pilot game work"),
            Some(("/rename".to_string(), "pilot game work".to_string()))
        );
        assert_eq!(
            parse_session_chat_local_command("/context"),
            Some(("/context".to_string(), String::new()))
        );
        assert_eq!(
            parse_session_chat_local_command("/plugin:deploy prod"),
            Some(("/plugin:deploy".to_string(), "prod".to_string()))
        );
        // A pasted path is not a command, and archiving it would put a
        // permanent lie in the trail.
        assert_eq!(
            parse_session_chat_local_command("/Users/sven/code/notes.md"),
            None
        );
        // Untrimmed, exactly like the client's send classification.
        assert_eq!(parse_session_chat_local_command(" /rename x"), None);
        assert_eq!(
            parse_session_chat_local_command("what does /rename do"),
            None
        );
        assert_eq!(parse_session_chat_local_command("! printf hi"), None);
    }

    #[test]
    fn reads_a_claude_command_result_off_the_screen() {
        let before = claude_screen(&["⏺ Earlier answer.", ""]);
        let after = claude_screen(&[
            "⏺ Earlier answer.",
            "",
            "❯ /rename pilot game work",
            "  ⎿ Session renamed to: pilot game work",
            "",
        ]);
        assert_eq!(
            session_chat_local_command_output(Some("claude"), "/rename", &before, &after)
                .as_deref(),
            Some("Session renamed to: pilot game work")
        );
    }

    #[test]
    fn a_command_that_printed_nothing_has_no_output() {
        let before = claude_screen(&["⏺ Earlier answer."]);
        // Only the echo, and a repainted footer.
        let after = claude_screen(&["⏺ Earlier answer.", "❯ /diff"]);
        assert_eq!(
            session_chat_local_command_output(Some("claude"), "/rename", &before, &after),
            None
        );
    }

    #[test]
    fn replays_a_row_as_the_envelope_the_cli_would_have_written() {
        let messages = merge_session_chat_local_commands(
            Vec::new(),
            &[row(
                "1-0",
                "/rename",
                "pilot <game> work",
                Some("Session renamed to: pilot <game> work"),
                1_000,
            )],
        );
        let text = |index: usize| match &messages[index].blocks[0] {
            SessionChatBlock::Text { text } => text.clone(),
            _ => panic!("expected text"),
        };
        assert_eq!(messages.len(), 2);
        assert_eq!(
            text(0),
            "<command-name data-ghostex-escaped=\"html\">/rename</command-name> \
             <command-args data-ghostex-escaped=\"html\">pilot &lt;game&gt; work</command-args>"
        );
        assert_eq!(
            text(1),
            "<local-command-stdout data-ghostex-escaped=\"html\">Session renamed to: pilot &lt;game&gt; work</local-command-stdout>"
        );
        // The result can never sort above the command that produced it.
        assert!(messages[0].timestamp < messages[1].timestamp);
    }

    #[test]
    fn merges_a_row_after_the_message_it_followed() {
        let messages = vec![
            transcript_row("t1", "first prompt", 100),
            transcript_row("t2", "second prompt", 300),
        ];
        let merged =
            merge_session_chat_local_commands(messages, &[row("1-0", "/context", "", None, 200)]);
        let ids: Vec<&str> = merged.iter().map(|message| message.id.as_str()).collect();
        assert_eq!(ids, vec!["t1", "local-command:1-0", "t2"]);
    }

    #[test]
    fn a_page_only_claims_the_rows_anchored_inside_it() {
        let messages = vec![
            transcript_row("t1", "first prompt", 100),
            transcript_row("t2", "second prompt", 300),
        ];
        let inside = row("1-0", "/context", "", None, 200);
        let after_end = row("2-0", "/diff", "", None, 400);
        let before_start = row("0-0", "/rename", "early", None, 50);

        // Live tail: owns everything from its first message onwards.
        let tail = select_session_chat_local_commands(
            vec![inside.clone(), after_end.clone(), before_start.clone()],
            &messages,
            true,
            true,
        );
        assert_eq!(
            tail.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["1-0", "2-0"]
        );

        // A pagination page keeps the row anchored inside it and leaves the one
        // past its end to the page above, so neither is shown twice.
        let older = select_session_chat_local_commands(
            vec![inside.clone(), after_end.clone(), before_start.clone()],
            &messages,
            false,
            true,
        );
        assert_eq!(
            older.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["1-0"]
        );

        // The oldest page owns what predates the transcript itself.
        let oldest =
            select_session_chat_local_commands(vec![before_start.clone()], &messages, false, false);
        assert_eq!(
            oldest.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["0-0"]
        );
    }

    #[test]
    fn the_agents_own_record_wins() {
        let messages = vec![transcript_row(
            "t1",
            "<command-name>/model</command-name>\n<command-message>model</command-message>\n<command-args>opus</command-args>",
            100,
        )];
        let selected = select_session_chat_local_commands(
            vec![
                row("1-0", "/model", "opus", None, 150),
                row("1-1", "/context", "", None, 160),
            ],
            &messages,
            true,
            false,
        );
        assert_eq!(
            selected
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["1-1"]
        );
    }

    #[test]
    fn archives_and_replays_across_a_reload() {
        let project = format!("test-{}", std::process::id());
        let session = "archive-roundtrip";
        let path = session_file(&project, session).expect("path");
        let _ = fs::remove_file(&path);

        let id = record_session_chat_local_command(&project, session, "/rename pilot game work")
            .expect("recorded");
        attach_session_chat_local_command_output(
            &project,
            session,
            &id,
            "Session renamed to: pilot game work",
        );
        // A refinement replaces the output rather than adding a second row.
        attach_session_chat_local_command_output(
            &project,
            session,
            &id,
            "Session renamed to: final",
        );

        let rows = load_session_chat_local_commands(&project, session);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text(), "/rename pilot game work");
        assert_eq!(rows[0].output.as_deref(), Some("Session renamed to: final"));

        let _ = fs::remove_file(&path);
        // The per-project directory is this test's debris too.
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    fn notice(text: &str) -> String {
        format!("{}{text}", " ".repeat(120))
    }

    fn claude_screen_with_notice(history: &[&str], notice_text: &str) -> String {
        let mut lines: Vec<String> = history.iter().map(|line| line.to_string()).collect();
        lines.push(String::new());
        lines.push(notice(notice_text));
        lines.push("──────────────────────────────── probe one ─".to_string());
        lines.push("❯ ".to_string());
        lines.push("──────────────────────────────────────────────────".to_string());
        lines.push("  ⏵⏵ bypass permissions on (shift+tab to cycle)".to_string());
        lines.join("\n")
    }

    /// The shape captured live on 2026-09-10: the notice above the composer
    /// changed between the two captures, which used to break the anchor.
    #[test]
    fn ignores_the_notice_claude_repaints_above_its_composer() {
        let before = claude_screen_with_notice(&["⏺ Earlier answer."], "● high · /effort");
        let after = claude_screen_with_notice(
            &[
                "⏺ Earlier answer.",
                "❯ /rename probe one",
                "  ⎿  Session renamed to: probe one",
            ],
            "You've used 82% of your weekly limit · resets 8pm (Europe/Malta)",
        );
        assert_eq!(
            session_chat_local_command_output(Some("claude"), "/rename", &before, &after)
                .as_deref(),
            Some("Session renamed to: probe one")
        );
    }

    #[test]
    fn a_repainted_notice_alone_is_not_output() {
        let before = claude_screen_with_notice(&["⏺ Earlier answer."], "● high · /effort");
        let after = claude_screen_with_notice(
            &["⏺ Earlier answer."],
            "You've used 82% of your weekly limit",
        );
        assert_eq!(
            session_chat_local_command_output(Some("claude"), "/rename", &before, &after),
            None
        );
    }

    #[test]
    fn an_open_panel_drops_its_drawing_and_key_hint() {
        let before = claude_screen_with_notice(&["⏺ Earlier answer."], "● high · /effort");
        // An open panel replaces the composer, so there is no echo to hang off.
        let after = [
            "⏺ Earlier answer.",
            "▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔",
            "   Settings  Status   Config",
            "",
            "   Version:                 2.1.267",
            "",
            "   Esc to cancel",
        ]
        .join("\n");
        let output = session_chat_local_command_output(Some("claude"), "/rename", &before, &after)
            .expect("panel");
        assert!(
            output.contains("Version:                 2.1.267"),
            "{output}"
        );
        assert!(!output.contains('▔'), "{output}");
        assert!(!output.contains("Esc to cancel"), "{output}");
    }

    #[test]
    fn commands_the_transcript_records_are_not_archived() {
        for command in ["/model", "/Effort", "/fast", "/compact"] {
            assert!(transcript_records_command_result(command), "{command}");
        }
        for command in ["/rename", "/context", "/status"] {
            assert!(!transcript_records_command_result(command), "{command}");
        }
    }

    /// Chat view's 24-row grid: the pre-send lines scrolled off, the echo did not.
    #[test]
    fn a_tall_result_is_attributed_by_its_own_echo() {
        let before = claude_screen_with_notice(&["⏺ Old answer line."], "● high · /effort");
        let after = claude_screen_with_notice(
            &[
                "❯ /context",
                "  ⎿  Context Usage",
                "     ⛁ ⛁ ⛶ ⛶   61.6k/1m tokens (6%)",
                "     Auto-compact window: 1m tokens",
            ],
            "● high · /effort",
        );
        assert_eq!(
            session_chat_local_command_output(Some("claude"), "/context", &before, &after).as_deref(),
            Some("Context Usage\n     ⛁ ⛁ ⛶ ⛶   61.6k/1m tokens (6%)\n     Auto-compact window: 1m tokens")
        );
    }

    /// Even the echo scrolled off: what is left is this command's output, cut off.
    #[test]
    fn a_result_taller_than_the_grid_says_it_was_cut_off() {
        let before = claude_screen_with_notice(&["⏺ Old answer line."], "● high · /effort");
        let after = claude_screen_with_notice(
            &[
                "     Auto-compact window: 1m tokens",
                "     Suggestions",
                "       Read results using 309.7k tokens (31%)",
            ],
            "● high · /effort",
        );
        let output = session_chat_local_command_output(Some("claude"), "/context", &before, &after)
            .expect("cut-off output");
        assert!(output.starts_with("…\n"), "{output}");
        assert!(
            output.contains("Read results using 309.7k tokens"),
            "{output}"
        );
    }

    /// The old lines are still on screen, just repainted: nothing to attribute.
    #[test]
    fn a_repaint_that_kept_the_old_lines_attributes_nothing() {
        let before =
            claude_screen_with_notice(&["⏺ Old answer line.", "  second line"], "● high · /effort");
        let after = claude_screen_with_notice(
            &["⏺ Old answer line.", "  second line changed"],
            "● high · /effort",
        );
        assert_eq!(
            session_chat_local_command_output(Some("claude"), "/context", &before, &after),
            None
        );
    }
}
