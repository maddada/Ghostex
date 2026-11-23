use super::*;

// ---------------------------------------------------------------------------
// Tool classification
// ---------------------------------------------------------------------------

const MCP_TOOL_PREFIX: &str = "mcp__";

const TERMINAL_TOOL_NAMES: &[&str] = &[
    "bash",
    "bashexecution",
    "bashoutput",
    "exec",
    "exec_command",
    "execute_command",
    "killshell",
    "kill_shell",
    "local_shell",
    "local_shell_call",
    "run_command",
    "run_terminal_cmd",
    "send_input",
    "shell",
    "shell_command",
    "terminal",
    "write_stdin",
];

const PATCH_TOOL_NAMES: &[&str] = &[
    "apply_patch",
    "create_file",
    "edit",
    "edit_file",
    "multiedit",
    "multi_edit",
    "notebookedit",
    "notebook_edit",
    "str_replace",
    "str_replace_based_edit_tool",
    "str_replace_editor",
    "update_file",
    "write",
    "write_file",
];

const WEB_SEARCH_TOOL_NAMES: &[&str] = &["browser_search", "web_search", "websearch"];

/// Name-only classification, shared by all four agents. Content-based
/// overrides (a shell command that is really an `apply_patch` heredoc) are
/// applied by the per-agent parsers on top of this.
pub(super) fn classify_tool(name: &str) -> TranscriptExportSection {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.starts_with(MCP_TOOL_PREFIX) || normalized == "mcp" {
        return TranscriptExportSection::McpCall;
    }
    if TERMINAL_TOOL_NAMES.contains(&normalized.as_str()) {
        return TranscriptExportSection::TerminalCmd;
    }
    if PATCH_TOOL_NAMES.contains(&normalized.as_str()) {
        return TranscriptExportSection::Patch;
    }
    if WEB_SEARCH_TOOL_NAMES.contains(&normalized.as_str()) {
        return TranscriptExportSection::WebSearch;
    }
    TranscriptExportSection::OtherTool
}

const TOOL_USE_ERROR_OPEN_TAG: &str = "<tool_use_error>";
const TOOL_USE_ERROR_CLOSE_TAG: &str = "</tool_use_error>";

pub(super) fn patch_output_failed(text: &str) -> bool {
    let normalized = text.trim().to_ascii_lowercase();
    normalized.starts_with(TOOL_USE_ERROR_OPEN_TAG)
        || normalized.starts_with("error")
        || normalized.contains("failed to apply")
        || normalized.contains("patch failed")
        || normalized.contains("could not apply")
        || normalized.contains("no such file")
}

/*
A failed patch renders as exactly ONE line (plan Q4), and a raw provider error
is neither one line nor readable: Claude wraps tool errors in
`<tool_use_error>…</tool_use_error>` and follows the sentence that names the
failure ("String to replace not found in file.") with the entire rejected
payload. The wrapper is stripped, only the first meaningful line is kept, and
the result is clamped — the next agent needs to know an edit did not land, not
to re-read the edit.
*/
pub(super) fn short_failure_reason(text: &str) -> String {
    let unwrapped = strip_tool_use_error_wrapper(text);
    let reason = unwrapped
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(one_line)
        .unwrap_or_default();
    if reason.is_empty() {
        return "the agent reported an error".to_string();
    }
    if reason.chars().count() <= PATCH_FAILURE_REASON_MAX_CHARS {
        return reason;
    }
    let clipped: String = reason
        .chars()
        .take(PATCH_FAILURE_REASON_MAX_CHARS)
        .collect();
    format!("{clipped}…")
}

fn strip_tool_use_error_wrapper(text: &str) -> String {
    let trimmed = text.trim();
    let Some(body) = trimmed.strip_prefix(TOOL_USE_ERROR_OPEN_TAG) else {
        return trimmed.to_string();
    };
    body.strip_suffix(TOOL_USE_ERROR_CLOSE_TAG)
        .unwrap_or(body)
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Patch parsing
// ---------------------------------------------------------------------------

const PATCH_ENVELOPE_START: &str = "*** Begin Patch";
const PATCH_ENVELOPE_END: &str = "*** End Patch";

/*
The `*** Begin Patch` envelope reaches us in two spellings: with real newlines
(Codex `function_call` arguments, Pi `apply_patch` arguments) and with escaped
`\n`, because current Codex sends patches inside a freeform JavaScript `exec`
snippet where the whole envelope is one string literal.

Which spelling is in play is decided by the text as a whole, never per
character: a real-newline patch whose body happens to contain the two
characters `\` `n` (any source file that writes a newline escape does) must not
be split there, and the escaped form has no real newlines to be confused by.
*/
fn split_patch_lines(text: &str) -> Vec<String> {
    if text.contains('\n') {
        return text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' => match characters.peek() {
                Some('n') => {
                    characters.next();
                    lines.push(std::mem::take(&mut current));
                }
                Some('\\') => {
                    characters.next();
                    current.push('\\');
                }
                _ => current.push('\\'),
            },
            other => current.push(other),
        }
    }
    lines.push(current);
    lines
}

pub(super) fn contains_patch_envelope(text: &str) -> bool {
    text.contains(PATCH_ENVELOPE_START)
}

/// Reads an `*** Begin Patch` envelope into one summary per touched file.
pub(super) fn parse_patch_envelope(text: &str) -> Vec<PatchFileChange> {
    let Some(start) = text.find(PATCH_ENVELOPE_START) else {
        return Vec::new();
    };
    let body = &text[start..];
    let body = match body.find(PATCH_ENVELOPE_END) {
        Some(end) => &body[..end],
        None => body,
    };
    let mut changes: Vec<PatchFileChange> = Vec::new();
    for line in split_patch_lines(body) {
        let trimmed = line.trim_end();
        if let Some(path) = trimmed.strip_prefix("*** Add File:") {
            changes.push(new_patch_change(path, PatchChangeKind::Added));
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("*** Update File:") {
            changes.push(new_patch_change(path, PatchChangeKind::Updated));
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("*** Delete File:") {
            changes.push(new_patch_change(path, PatchChangeKind::Deleted));
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("*** Move to:") {
            if let Some(change) = changes.last_mut() {
                change.path = format!("{} → {}", change.path, path.trim());
            }
            continue;
        }
        let Some(change) = changes.last_mut() else {
            continue;
        };
        if trimmed.starts_with("@@") {
            if let Some((first, last)) = unified_hunk_range(trimmed) {
                change.start_line = Some(change.start_line.map_or(first, |value| value.min(first)));
                change.end_line = Some(change.end_line.map_or(last, |value| value.max(last)));
            }
            continue;
        }
        if line.starts_with('+') {
            change.added += 1;
        } else if line.starts_with('-') {
            change.removed += 1;
        }
    }
    changes
}

pub(super) fn new_patch_change(path: &str, kind: PatchChangeKind) -> PatchFileChange {
    PatchFileChange {
        path: path.trim().to_string(),
        kind,
        added: 0,
        removed: 0,
        start_line: None,
        end_line: None,
    }
}

/// `@@ -12,7 +40,9 @@` → the new-side range `40..=48`. Codex's own hunk
/// headers carry a context line instead of numbers, which is why the patch
/// summary falls back to `+added/-removed` counts.
fn unified_hunk_range(header: &str) -> Option<(usize, usize)> {
    let after_plus = header.split('+').nth(1)?;
    let numbers = after_plus
        .split_whitespace()
        .next()?
        .trim_end_matches("@@")
        .trim_end_matches(',');
    let mut parts = numbers.split(',');
    let start: usize = parts.next()?.parse().ok()?;
    let count: usize = parts.next().map_or(Some(1), |value| value.parse().ok())?;
    if count == 0 {
        return Some((start, start));
    }
    Some((start, start + count - 1))
}

pub(super) fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.lines().count()
}

// ---------------------------------------------------------------------------
// Harness plumbing (mirrors the chat-view noise filter)
// ---------------------------------------------------------------------------

/*
Claude writes slash commands, their output and every harness injection as
ordinary `user` records whose body is an XML envelope
(`<command-name>/model</command-name>`, `<local-command-stdout>…`). Those are UI
bookkeeping, not the user's voice, so they are filed under the event/system
buckets instead of being quoted verbatim as `👤 User` blocks.

The tag vocabulary is a hand-kept mirror of `KNOWN_HARNESS_TAG_NAMES` /
`HARNESS_INJECTED_TURN_PREFIXES` in `session_chat.rs`. It is copied rather than
called because that module's filter takes an already-normalized
`SessionChatMessage`, which is exactly the representation export does not build.
Only a LEADING tag counts, same as chat view: a real prompt that happens to
carry an appended `<system-reminder>` is still the user speaking.
*/
const HARNESS_COMMAND_TAG_NAMES: &[&str] = &[
    "bash-input",
    "bash-stderr",
    "bash-stdout",
    "command-args",
    "command-message",
    "command-name",
    "local-command-caveat",
    "local-command-stderr",
    "local-command-stdout",
];

const HARNESS_SYSTEM_TAG_NAMES: &[&str] = &[
    "agent-message",
    "cross-session-message",
    "fork-boilerplate",
    "mcp-polling-update",
    "mcp-resource-update",
    "system-reminder",
    "task-notification",
    "teammate-message",
    "user-memory-input",
    "user-prompt-submit-hook",
];

const HARNESS_INJECTED_TURN_PREFIXES: &[&str] = &[
    "<channel source=",
    "<system>authentic event nonces for this delivery:",
    "[request interrupted",
    "a message arrived from ",
    "another claude session sent a message",
    "no response requested.",
    "caveat: the messages below were generated by the user while running local commands",
    "this session is being continued from a previous conversation",
];

fn leading_tag_name(normalized: &str) -> Option<&str> {
    let rest = normalized.strip_prefix('<')?;
    let first = rest.chars().next()?;
    if !first.is_ascii_lowercase() {
        return None;
    }
    let end = rest
        .find(|character: char| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        })
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    match rest[end..].chars().next() {
        None => Some(&rest[..end]),
        Some(character) if character.is_whitespace() || character == '>' => Some(&rest[..end]),
        Some(_) => None,
    }
}

/// The section a user-role record belongs to when its body is harness plumbing.
/// `None` means the record really is the user speaking.
pub(super) fn harness_user_turn_section(text: &str) -> Option<TranscriptExportSection> {
    if crate::session_chat_decode_claude::is_claude_repl_event_text(text) {
        return Some(TranscriptExportSection::SystemMessage);
    }
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    if let Some(tag) = leading_tag_name(&normalized) {
        if HARNESS_COMMAND_TAG_NAMES.contains(&tag) {
            return Some(TranscriptExportSection::SessionEvent);
        }
        if HARNESS_SYSTEM_TAG_NAMES.contains(&tag) {
            return Some(TranscriptExportSection::SystemMessage);
        }
    }
    HARNESS_INJECTED_TURN_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
        .then_some(TranscriptExportSection::SystemMessage)
}
