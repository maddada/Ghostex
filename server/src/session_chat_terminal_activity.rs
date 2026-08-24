/*
CDXC:SessionChatTerminalActivity 2026-08-22:
Live work the agent CLI reports ONLY on its terminal screen, before the same
text reaches the transcript. Claude Code replaces a current-status line as it
works:

    ⏺ Removing temporary examples

It also reports a small set of meaningful states under other markers:

    ✻ Waiting for 1 dynamic workflow to finish

The client keeps each changed value as a transient reasoning row, then lets the
authoritative transcript replace it when JSONL catches up. Claude's compaction
is the structured-progress variant:

    ❯ /compact

    ✶ Compacting conversation… (1m 1s)
      ████████████████████░░░░░░░░░░░░░░░░░░░░ 49%
    Tip: Use /btw to ask a quick side question without interrupting Claude's…

For a minute or more the chat surface could say nothing better than "the agent
is working", because a transcript projection cannot see a progress bar. Worse,
compaction is the one operation whose whole point is that the conversation the
user is reading is about to be REPLACED — so a bare typing indicator is not
just uninformative, it hides the single most consequential thing happening.

This is deliberately NOT a terminal notice: nothing is wrong, nothing is
blocked, and there is nothing to answer. Both variants render in the transcript
where the work is.

Parsing is narrow and evidence-only. `⏺` owns Claude's general status rows;
other star markers are accepted only for explicitly understood states. The
percentage and elapsed clock are read off the screen or omitted; neither is
ever estimated.
*/

use crate::session_chat_options::{session_chat_option_agent, SessionChatOptionAgent};

use serde_json::{json, Map, Value};

/// Tail window scanned for a progress line. The spinner row and its bar sit at
/// the very bottom of a working screen, above at most a tip line and the
/// statusline; 15 matches the notice banner scope.
const ACTIVITY_SCAN_LINES: usize = 15;

/// Rows after the label that may carry the bar. Claude paints it on the very
/// next line; two leaves room for a wrap.
const ACTIVITY_PERCENT_LOOKAHEAD: usize = 2;

/// Activity kind for Claude Code's `/compact` (manual and automatic).
pub const SESSION_CHAT_ACTIVITY_COMPACTING: &str = "compacting";

/// Claude Code's current assistant status, not yet flushed to transcript JSONL.
pub const SESSION_CHAT_ACTIVITY_CLAUDE_STATUS: &str = "claude-status";

/// Star frames Claude may use for allowlisted non-general status rows. Merely
/// having one of these markers is not sufficient evidence: custom working
/// spinner text uses the same frames and must never become chat history.
const CLAUDE_SPECIAL_STATUS_MARKERS: &str = "✳✶✻✽✸✹✺✷✴";

/// The phrase Claude paints while compacting. Matched case-sensitively on the
/// space-collapsed line, so prose that merely mentions compaction cannot hit
/// it — the label only counts when it OWNS a line (see `activity_from_line`).
const COMPACTING_LABEL: &str = "Compacting conversation";

/// What the client shows. `kind` is an open set, so a client that has never
/// heard of one still renders `label` plus whatever progress came with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalActivity {
    pub kind: &'static str,
    /// Agent-facing wording, without the spinner glyph or the clock.
    pub label: String,
    /// 0-100, only when the screen actually painted a percentage.
    pub percent: Option<u8>,
    /// Seconds the CLI reports it has been running, only when it painted them.
    pub elapsed_seconds: Option<u64>,
    /// RFC3339 millis. The client interpolates its own clock from this, so a
    /// 3s probe cadence still reads as a smoothly ticking timer.
    pub detected_at: String,
}

impl SessionChatTerminalActivity {
    fn new(kind: &'static str, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            percent: None,
            elapsed_seconds: None,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    }

    /*
    Two samples of the SAME run, ignoring the numbers. Progress changing is not
    a new activity — if it were, the client's `detectedAt`-anchored clock would
    restart from zero on every probe and the timer would never advance past the
    poll interval.
    */
    pub fn same_activity(&self, other: Option<&SessionChatTerminalActivity>) -> bool {
        other.is_some_and(|other| self.kind == other.kind && self.label == other.label)
    }

    /// True when a re-detect says the same thing INCLUDING its numbers, i.e.
    /// there is nothing new to publish.
    pub fn unchanged(&self, other: Option<&SessionChatTerminalActivity>) -> bool {
        other.is_some_and(|other| {
            self.same_activity(Some(other))
                && self.percent == other.percent
                && self.elapsed_seconds == other.elapsed_seconds
        })
    }

    /*
    An ongoing run keeps its original `detectedAt`: it anchors the client's
    elapsed clock, so re-minting it every 3s would peg the timer at ~0s
    forever. Same instance-not-sample rule as a terminal notice's timestamp.
    */
    pub fn carry_forward_detected_at(&mut self, previous: Option<&SessionChatTerminalActivity>) {
        if let Some(previous) = previous.filter(|previous| self.same_activity(Some(previous))) {
            self.detected_at = previous.detected_at.clone();
        }
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("kind".to_string(), json!(self.kind));
        map.insert("label".to_string(), json!(self.label));
        if let Some(percent) = self.percent {
            map.insert("percent".to_string(), json!(percent));
        }
        if let Some(elapsed_seconds) = self.elapsed_seconds {
            map.insert("elapsedSeconds".to_string(), json!(elapsed_seconds));
        }
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        Value::Object(map)
    }
}

/// Change test for a value that can also disappear; an omitted field on a frame
/// means CLEARED, so present→absent is a change clients must be told about.
pub fn same_session_chat_terminal_activity(
    current: Option<&SessionChatTerminalActivity>,
    published: Option<&SessionChatTerminalActivity>,
) -> bool {
    match (current, published) {
        (None, None) => true,
        (Some(current), published) => current.unchanged(published),
        (None, Some(_)) => false,
    }
}

/// `1h 2m 3s` / `1m 1s` / `45s` → seconds. `None` unless EVERY token parsed,
/// so a half-read clock is dropped rather than shown wrong.
pub(crate) fn parse_elapsed_seconds(text: &str) -> Option<u64> {
    let mut total: u64 = 0;
    let mut matched = false;
    for token in text.split_whitespace() {
        let (digits, unit) = token.split_at(token.find(|ch: char| !ch.is_ascii_digit())?);
        let value: u64 = digits.parse().ok()?;
        total += match unit {
            "h" => value * 3_600,
            "m" => value * 60,
            "s" => value,
            _ => return None,
        };
        matched = true;
    }
    matched.then_some(total)
}

/// The `(1m 1s)` a spinner line trails, if it has one.
fn trailing_parenthetical(line: &str) -> Option<&str> {
    let close = line.rfind(')')?;
    let open = line[..close].rfind('(')?;
    Some(line[open + 1..close].trim())
}

/// Claude appends ` · 22s` to a running tool row and repaints only the clock.
/// Keep that clock as progress metadata so one tool run does not become a new
/// transient chat message every second.
fn trailing_elapsed_status(label: &str) -> (&str, Option<u64>) {
    let Some((stable_label, elapsed)) = label.rsplit_once(" · ") else {
        return (label, None);
    };
    let Some(elapsed_seconds) = parse_elapsed_seconds(elapsed.trim()) else {
        return (label, None);
    };
    (stable_label.trim_end(), Some(elapsed_seconds))
}

/// Claude's animated working line can repaint the same wording as:
///
///     label… (1m 5s · thinking with medium effort)
///     label… (1m 11s · ↓ 4.5k tokens)
///
/// The parenthetical is sample metadata, not a new status. Separating its
/// clock keeps one stable label, which lets the client's exact-text
/// deduplication retain one row instead of one row per screen probe.
fn trailing_parenthetical_status(label: &str) -> (&str, Option<u64>) {
    let Some(without_close) = label.strip_suffix(')') else {
        return (label, None);
    };
    let Some(open) = without_close.rfind(" (") else {
        return (label, None);
    };
    let metadata = &without_close[open + 2..];
    let elapsed = metadata
        .split_once(" · ")
        .map_or(metadata, |(elapsed, _)| elapsed);
    let Some(elapsed_seconds) = parse_elapsed_seconds(elapsed.trim()) else {
        return (label, None);
    };
    (without_close[..open].trim_end(), Some(elapsed_seconds))
}

fn stable_status_label(label: &str) -> (&str, Option<u64>) {
    let (stable, elapsed_seconds) = trailing_elapsed_status(label);
    if elapsed_seconds.is_some() {
        return (stable, elapsed_seconds);
    }
    trailing_parenthetical_status(label)
}

/// The one currently understood status that Claude paints with a non-`⏺`
/// marker. Match the whole grammar so arbitrary/custom spinner text cannot be
/// admitted merely because it happens to use the same animated glyph.
fn is_dynamic_workflow_wait_label(label: &str) -> bool {
    let Some(rest) = label.strip_prefix("Waiting for ") else {
        return false;
    };
    let Some((count, suffix)) = rest.split_once(' ') else {
        return false;
    };
    let Ok(count) = count.parse::<u64>() else {
        return false;
    };
    (count == 1 && suffix == "dynamic workflow to finish")
        || (count > 1 && suffix == "dynamic workflows to finish")
}

/// `49%` anywhere on the line (the bar glyphs around it are ignored).
fn parse_percent(line: &str) -> Option<u8> {
    for token in line.split_whitespace() {
        let Some(digits) = token.strip_suffix('%') else {
            continue;
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if let Ok(percent) = digits.parse::<u8>() {
            if percent <= 100 {
                return Some(percent);
            }
        }
    }
    None
}

/*
A label only counts when the line is the CLI's own status row rather than prose
that mentions it. Compaction requires decoration-only text before its phrase;
general status requires `⏺`, and another star marker requires an allowlisted
whole label. An assistant sentence, a tip, and custom spinner wording cannot
satisfy those shapes.
*/
fn activity_from_line(line: &str) -> Option<SessionChatTerminalActivity> {
    if let Some(at) = line.find(COMPACTING_LABEL) {
        if !line[..at]
            .chars()
            .any(|ch| ch.is_alphabetic() || ch.is_ascii_digit())
        {
            let mut activity = SessionChatTerminalActivity::new(
                SESSION_CHAT_ACTIVITY_COMPACTING,
                "Compacting conversation",
            );
            activity.elapsed_seconds = trailing_parenthetical(line).and_then(parse_elapsed_seconds);
            return Some(activity);
        }
    }

    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if marker != '⏺' && !CLAUDE_SPECIAL_STATUS_MARKERS.contains(marker) {
        return None;
    }
    let rest = &trimmed[marker.len_utf8()..];
    if !rest.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    let (label, elapsed_seconds) = stable_status_label(rest.trim());
    if label.is_empty() {
        return None;
    }
    if marker != '⏺' && !is_dynamic_workflow_wait_label(label) {
        return None;
    }
    let mut activity = SessionChatTerminalActivity::new(SESSION_CHAT_ACTIVITY_CLAUDE_STATUS, label);
    activity.elapsed_seconds = elapsed_seconds;
    Some(activity)
}

/// `Some` while the agent is painting a live line this build understands.
pub fn detect_session_chat_terminal_activity(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatTerminalActivity> {
    // Claude Code is the only CLI whose compaction paints this row; codex
    // compacts without a progress screen, so it would only ever false-match.
    if session_chat_option_agent(agent) != Some(SessionChatOptionAgent::Claude) {
        return None;
    }
    let mut lines = crate::session_chat_agent_fleet::normalized_screen_lines(screen_text);
    /*
    CDXC:SessionChatAgentFleet 2026-08-23: cut the background-agent block off
    the bottom of the screen before reading anything. Its rows are
    indistinguishable from a status line — `⏺` there is the TUI's selection
    marker, so a selected subagent paints `⏺ general-purpose  Fixing tool-ro…`
    — and the block sits BELOW the statusline, so newest-match-wins would
    prefer it over the real status line and its rows would spend the scan
    window's line budget getting there.
    */
    if let Some(start) = crate::session_chat_agent_fleet::agent_fleet_block_start(&lines) {
        lines.truncate(start);
    }
    let window = &lines[lines.len().saturating_sub(ACTIVITY_SCAN_LINES)..];
    // Newest match wins: a screen can still hold the tail of a previous run.
    for (index, line) in window.iter().enumerate().rev() {
        let Some(mut activity) = activity_from_line(line) else {
            continue;
        };
        if activity.kind == SESSION_CHAT_ACTIVITY_COMPACTING {
            activity.percent = window
                .iter()
                .skip(index + 1)
                .take(ACTIVITY_PERCENT_LOOKAHEAD)
                .find_map(|candidate| parse_percent(candidate));
        }
        return Some(activity);
    }
    None
}
