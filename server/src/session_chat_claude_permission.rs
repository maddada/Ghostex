//! What a Claude Code tool permission prompt asks, read off its panel so the chat's approval card
//! can say it instead of a generic "asking for permission".
//!
//! CDXC:AgentScreenDetection 2026-10-22 WHY:
//! The permission prompt was read as its Yes/No rows and the few lines straight above them, so the chat said only "Claude Code is asking for permission to proceed": the tool, the subagent asking, the description, the command and the reason Claude flagged it never reached the card (seen on a background subagent's `rm -rf "$P"/*`, whose call is not in the lead transcript either, so nothing opened beside the card). Claude Code 2.1.301 draws every tool's prompt as one panel under a `─` rule, measured on Bash (lead and subagent, with and without a warning, wrapped), Write, Edit, WebFetch and an MCP tool:
//!
//! ```text
//! ────────────────────────────
//!  Bash command · from the general-purpose agent      (tool kind, then the subagent when one asks)
//!  Tip: auto mode handles these prompts for you …     (left out)
//!  Delete all audit scratch files and downloads       (the description; a path for Edit/Write)
//! ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
//!  │ P=…; /bin/rm -rf "$P"/* …                        (the command, diff or input; `│` when it wraps)
//! ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
//!  │ Dangerous rm operation on possibly-empty …        (why Claude asks; an MCP tool's note has a label line)
//!
//!  Do you want to proceed?
//!  ❯ 1. Yes
//! ```
//!
//! SEE-ALSO: server/src/session_chat_resume_prompt.rs (the rows and liveness), packages/gx-chat-core/src/questions/permission_card.rs (the card it becomes).

use serde_json::{json, Value};

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// Content lines carried to the card; a long Write's file stays in the terminal.
const MAX_CONTENT_LINES: usize = 80;

/// One Claude Code permission prompt, in Claude's own words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudePermissionRequest {
    /// The panel's first line, verbatim: "Bash command · from the general-purpose agent".
    pub source: String,
    /// The subagent asking ("general-purpose"), or `None` when the lead agent asks.
    pub agent: Option<String>,
    /// What the tool wants to do: Bash's description, the file of an Edit or Write, WebFetch's
    /// "Claude wants to fetch content from …", an MCP tool's name.
    pub subject: Option<String>,
    /// The command, diff or input between the panel's dashed rules, as drawn.
    pub content: Option<String>,
    /// Content lines left out past `MAX_CONTENT_LINES`.
    pub content_hidden_lines: usize,
    /// A label over the note ("About the Claude Docs — Guide Tool:"): the note informs rather than warns.
    pub note_label: Option<String>,
    /// Why Claude asks, or what the tool is.
    pub note: Option<String>,
    /// "Do you want to proceed?", "Do you want to make this edit to new.txt?".
    pub question: String,
}

impl ClaudePermissionRequest {
    pub fn to_value(&self) -> Value {
        json!({
            "source": self.source,
            "agent": self.agent,
            "subject": self.subject,
            "content": self.content,
            "contentHiddenLines": self.content_hidden_lines,
            "noteLabel": self.note_label,
            "note": self.note,
            "question": self.question,
        })
    }
}

fn is_rule(line: &str, fill: char) -> bool {
    let line = line.trim();
    line.chars().count() >= 20 && line.chars().all(|c| c == fill)
}

/// A line inside the panel without its one-column indent and the `│` gutter Claude draws beside
/// a block that wraps.
fn ungutter(line: &str) -> &str {
    let line = line.strip_prefix(' ').unwrap_or(line);
    match line.strip_prefix('│') {
        Some(rest) => rest.strip_prefix(' ').unwrap_or(rest),
        None => line,
    }
}

/// The lines with blank ones at both ends dropped and their shared indent removed.
fn block(lines: &[&str]) -> Vec<String> {
    let first = lines.iter().position(|line| !line.trim().is_empty());
    let last = lines.iter().rposition(|line| !line.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return Vec::new();
    };
    let lines = &lines[first..=last];
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|c| *c == ' ').count())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.chars().skip(indent).collect::<String>())
        .collect()
}

/// The permission prompt on `screen`, when its heading is painted. Liveness (the Yes and No rows,
/// no composer after them) is `detect_session_chat_terminal_picker`'s to prove.
pub fn detect_claude_permission_request(screen: &str) -> Option<ClaudePermissionRequest> {
    let screen = crate::session_chat_resume_prompt::strip_claude_side_panel(screen);
    let lines: Vec<String> = screen
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect();
    let heading = lines
        .iter()
        .rposition(|line| crate::session_chat_resume_prompt::is_permission_prompt_heading(line))?;
    // Upwards from the heading: the note, its dashed rule, the content, the dashed rule over it,
    // then the header up to the panel's `─` rule.
    let mut dashed = Vec::new();
    let mut top = None;
    for index in (0..heading).rev() {
        if is_rule(&lines[index], '─') {
            top = Some(index);
            break;
        }
        if dashed.len() < 2 && is_rule(&lines[index], '╌') {
            dashed.push(index);
        }
    }
    let top = top?;
    let (header_end, content, note) = match dashed.as_slice() {
        [below, above] => (
            *above,
            &lines[above + 1..*below],
            &lines[below + 1..heading],
        ),
        [only] => (*only, &lines[only + 1..heading], &lines[heading..heading]),
        _ => (heading, &lines[heading..heading], &lines[heading..heading]),
    };
    let mut header = lines[top + 1..header_end]
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty());
    let source = header.next()?.to_string();
    let agent = source
        .split_once(" · from the ")
        .map(|(_, agent)| agent.trim_end_matches(" agent").trim().to_string())
        .filter(|agent| !agent.is_empty());
    let subject = header
        .filter(|line| !line.starts_with("Tip:"))
        .collect::<Vec<_>>()
        .join(" ");
    let content = block(
        &content
            .iter()
            .map(|line| ungutter(line))
            .collect::<Vec<_>>(),
    );
    let content_hidden_lines = content.len().saturating_sub(MAX_CONTENT_LINES);
    let content = content
        .into_iter()
        .take(MAX_CONTENT_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let mut note_label = None;
    let mut note_text: Vec<&str> = Vec::new();
    for line in note.iter().filter(|line| !line.trim().is_empty()) {
        let text = ungutter(line).trim();
        if note_label.is_none() && note_text.is_empty() && !line.trim_start().starts_with('│') {
            note_label = Some(text.to_string());
        } else {
            note_text.push(text);
        }
    }
    let note = note_text.join(" ");
    Some(ClaudePermissionRequest {
        source,
        agent,
        subject: (!subject.is_empty()).then_some(subject),
        content: (!content.trim().is_empty()).then_some(content),
        content_hidden_lines,
        note_label,
        note: (!note.is_empty()).then_some(note),
        question: lines[heading].trim().to_string(),
    })
}

/// The words Claude Code paints under an unattended session's prompt, before its time.
const AUTO_BLOCK_LEAD: &str = "Claude Code will automatically block this request in ";
/// The words that end it, after its time.
const AUTO_BLOCK_TAIL: &str = "unattended session";
/// The most screen rows one countdown wraps to on a narrow terminal.
const AUTO_BLOCK_MAX_ROWS: usize = 4;

/// The screen with Claude Code's auto-block countdown taken out, and the seconds it had left.
///
/// CDXC:SessionChat 2026-11-08 WHY:
/// User: "wtf is this pls fix i can't click yes". Claude Code 2.1.319 paints "⚠ Claude Code will automatically block this request in 1:45, to avoid holding up an unattended session" under a prompt nobody is watching and repaints the time every second (`autoDenyWindow` / `denialLimitFallback`; "about 3 minutes" or "about 40 seconds" in a narrow layout). It was read as part of the prompt's warning and detail, so every second the prompt counted as a new one: its `detectedAt` changed, the card's answer was refused with "The prompt changed before your answer arrived", a coordinator's relayed card got a new key, and an answered card came back. The countdown is no part of what the prompt asks, so it is cut out before the prompt is read, and only its deadline travels (`autoBlockAt`, which a repeat does not change: `carry_forward_detected_at`). The same command with the same rows is the same prompt for as long as it waits; anything else on it still makes a new one.
pub fn split_claude_auto_block(screen: &str) -> (std::borrow::Cow<'_, str>, Option<u64>) {
    if !screen.contains(AUTO_BLOCK_LEAD.trim_end()) {
        return (std::borrow::Cow::Borrowed(screen), None);
    }
    let lines: Vec<&str> = screen.lines().collect();
    let clean = |line: &str| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string();
    let Some(start) = lines
        .iter()
        .rposition(|line| clean(line).contains(AUTO_BLOCK_LEAD.trim_end()))
    else {
        return (std::borrow::Cow::Borrowed(screen), None);
    };
    let mut end = start;
    let mut sentence = clean(lines[start]);
    while !sentence.contains(AUTO_BLOCK_TAIL)
        && end + 1 < lines.len()
        && end + 1 - start < AUTO_BLOCK_MAX_ROWS
    {
        end += 1;
        sentence.push(' ');
        sentence.push_str(&clean(lines[end]));
    }
    if !sentence.contains(AUTO_BLOCK_TAIL) {
        end = start;
    }
    let seconds = sentence
        .split_once(AUTO_BLOCK_LEAD.trim_end())
        .map(|(_, rest)| rest.trim_start())
        .and_then(|rest| rest.split(',').next())
        .and_then(auto_block_seconds);
    let kept = lines[..start]
        .iter()
        .chain(lines[end + 1..].iter())
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    (std::borrow::Cow::Owned(kept), seconds)
}

/// "1:45", "about 3 minutes", "about 1 second" as seconds.
fn auto_block_seconds(time: &str) -> Option<u64> {
    let time = time.trim();
    if let Some((minutes, seconds)) = time.split_once(':') {
        return Some(
            minutes.trim().parse::<u64>().ok()? * 60 + seconds.trim().parse::<u64>().ok()?,
        );
    }
    let mut words = time.trim_start_matches("about").split_whitespace();
    let count: u64 = words.next()?.parse().ok()?;
    let unit = words.next()?;
    Some(if unit.starts_with("minute") {
        count * 60
    } else {
        count
    })
}
