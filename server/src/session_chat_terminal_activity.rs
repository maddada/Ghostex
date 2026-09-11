/*
CDXC:AgentScreenDetection 2026-08-22:
Live work the agent CLI reports ONLY on its terminal screen, before the same
text reaches the transcript. Claude Code replaces a current-status line as it
works:

    ⏺ Removing temporary examples

It also reports a small set of meaningful states under other markers:

    ✻ Waiting for 1 dynamic workflow to finish
    ✻ Cooked for 46s · 1 shell still running

The client keeps each general status change as a transient reasoning row, then
lets the authoritative transcript replace it when JSONL catches up. A running
shell stays as one bottom activity row only while it remains on screen.
Claude's compaction is the structured-progress variant:

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

/// Cursor's tail window. Claude activity scans the whole screen because queued
/// messages can push live status and tool rows away from the bottom.
const CURSOR_ACTIVITY_SCAN_LINES: usize = 15;

/// Rows after the label that may carry the bar. Claude paints it on the very
/// next line; two leaves room for a wrap.
const ACTIVITY_PERCENT_LOOKAHEAD: usize = 2;

/// Activity kind for Claude Code's `/compact` (manual and automatic).
pub const SESSION_CHAT_ACTIVITY_COMPACTING: &str = "compacting";

/// Claude Code's current assistant status, not yet flushed to transcript JSONL.
/// Since 2026-09-11 this is only the allowlisted star-marker rows; a `⏺`
/// prose row is a `agent-stream`.
pub const SESSION_CHAT_ACTIVITY_CLAUDE_STATUS: &str = "claude-status";

/*
CDXC:AgentScreenDetection 2026-09-11 DECISION:
User: while Claude Code streams a long reply, the chat view stayed empty until
the whole message reached the transcript JSONL, although the terminal was
already painting it. Read the reply off the terminal as it comes in (chunks
every second are enough), show it in the chat as the message being written,
and switch to the transcript's row the moment it is saved there.

The `⏺` row and every indented row under it, down to the next marker (the
spinner, a tool gutter, the next bullet, the composer), is the message Claude
is writing. A `⏺` row is therefore published as `agent-stream` with the whole
block as `text`, not as the first-paragraph `claude-status` it used to be. The
first paragraph stays in `label` so an older client still shows something.
The kind is agent-neutral on purpose: the client does the same thing with a
streamed reply whoever wrote it, so a Codex reader only needs its own block
grammar feeding the shared stitching, rendering, and retirement below.

Claude runs as a full-screen TUI here, so a message longer than the grid
scrolls its `⏺` row off the top and the capture only holds the tail. Each
probe therefore carries the painted rows and whether the bullet was on
screen; the detection funnel (`merge_agent_stream`) stitches a headless
sample onto the rows it accumulated from earlier probes by finding the rows
they share. A stream only ever STARTS from a visible bullet: a headless sample
with nothing to stitch onto is not evidence of a message (the top of the grid
can just as well be the tail of the user's prompt or of a thinking block).
*/
pub const SESSION_CHAT_ACTIVITY_AGENT_STREAM: &str = "agent-stream";

/// The row of a Claude Code tool call: the row directly above the `⎿` output
/// gutter, with or without its bullet. The client shows it as a pending tool
/// row at the bottom of the transcript, never as reasoning history — the
/// transcript writes the call itself once its result lands, and the screen
/// paints the description in a different form ("Reading …" for "Read …")
/// than the transcript stores, so the row could never be matched as prose.
pub const SESSION_CHAT_ACTIVITY_CLAUDE_TOOL: &str = "claude-tool";

/// Cursor Agent's live reasoning/composition row before its transcript flushes.
pub const SESSION_CHAT_ACTIVITY_CURSOR_THINKING: &str = "cursor-thinking";

/// A Claude Code background shell or monitor that remains live after the
/// assistant turn.
pub const SESSION_CHAT_ACTIVITY_SHELLS_RUNNING: &str = "shells-running";

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
    /// The tool block Claude painted under a `claude-tool` row (the `⎿` gutter
    /// and its continuation rows), exactly as shown on the terminal.
    pub detail: Option<String>,
    /// `agent-stream` only: the message text painted so far, stitched across
    /// probes when its bullet has scrolled off the grid.
    pub text: Option<String>,
    /// `agent-stream` only, never serialized: the painted rows this value was
    /// rendered from, kept so the next probe can be stitched onto them.
    pub stream: Option<AgentStreamRows>,
}

/// One painted row of a Claude message block, layout kept (see `ScreenRow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStreamRow {
    /// Trimmed row text.
    pub text: String,
    /// Leading spaces on the physical row.
    pub indent: usize,
    /// A blank row separated this row from the previous one.
    pub after_blank: bool,
}

/// The rows of one Claude message block as painted, and whether they start at
/// its `⏺` row or at the top of a grid the bullet has scrolled off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStreamRows {
    pub rows: Vec<AgentStreamRow>,
    pub head_visible: bool,
}

impl SessionChatTerminalActivity {
    fn new(kind: &'static str, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            percent: None,
            elapsed_seconds: None,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            detail: None,
            text: None,
            stream: None,
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
                && self.detail == other.detail
                && self.text == other.text
        })
    }

    /*
    An ongoing run keeps its original `detectedAt`: it anchors the client's
    elapsed clock, so re-minting it every 3s would peg the timer at ~0s
    forever. Same instance-not-sample rule as a terminal notice's timestamp.
    */
    pub fn carry_forward_detected_at(&mut self, previous: Option<&SessionChatTerminalActivity>) {
        if let Some(previous) = previous.filter(|previous| self.same_activity(Some(previous))) {
            // Once a run has an elapsed baseline, keep that first sample with
            // its first timestamp. The client advances it locally; accepting
            // every later CLI clock sample as well would count the same time
            // twice. If elapsed first APPEARS later, that later sample needs
            // its own timestamp and becomes the baseline instead.
            if previous.elapsed_seconds.is_none() && self.elapsed_seconds.is_some() {
                return;
            }
            self.detected_at = previous.detected_at.clone();
            if self.elapsed_seconds.is_some() {
                self.elapsed_seconds = previous.elapsed_seconds;
            }
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
        if let Some(detail) = &self.detail {
            map.insert("detail".to_string(), json!(detail));
        }
        if let Some(text) = &self.text {
            map.insert("text".to_string(), json!(text));
        }
        Value::Object(map)
    }

    /*
    CDXC:AgentScreenDetection 2026-09-02:
    Most terminal activity is stale scrollback once the main turn becomes
    ready: a `⏺` status row stays on the primary screen after Claude stops, so
    the hook-derived working flag is what proves it current. Two kinds are
    proven by the screen itself and must NOT be gated on that flag:

      - a background shell: Claude reports it precisely because that work
        remains live after the assistant turn has finished;
      - compaction: Claude repaints the `Compacting conversation` row in place
        and replaces it with its `Compacted` line when done, so a whole capture
        that still shows the row is proof the compaction is running, and a
        whole capture without it is proof it ended. Hooks are not a start
        authority here — `PreCompact` is deliberately unregistered and a
        `/compact` typed in the terminal is not proven to raise
        `UserPromptSubmit` — so gating on "working" hid a compaction the
        terminal was visibly showing.
    */
    pub fn remains_live_when_ready(&self) -> bool {
        matches!(
            self.kind,
            SESSION_CHAT_ACTIVITY_SHELLS_RUNNING | SESSION_CHAT_ACTIVITY_COMPACTING
        )
    }
}

/// Which activity a client may be told about given the hook-derived working
/// flag: everything while the agent is working, and only the screen-proven
/// kinds (see `remains_live_when_ready`) once it is ready.
pub fn publishable_session_chat_terminal_activity(
    working: bool,
    activity: Option<SessionChatTerminalActivity>,
) -> Option<SessionChatTerminalActivity> {
    activity.filter(|activity| working || activity.remains_live_when_ready())
}

/// CDXC:SessionStatus 2026-09-06 DECISION:
/// User: Claude must be considered working while its footer reports one or more monitors running.
pub(crate) fn is_session_chat_monitor_activity(
    activity: Option<&SessionChatTerminalActivity>,
) -> bool {
    activity.is_some_and(|activity| {
        activity.kind == SESSION_CHAT_ACTIVITY_SHELLS_RUNNING
            && activity
                .label
                .rsplit_once(" · ")
                .is_some_and(|(_, status)| {
                    status.split_once(' ').is_some_and(|(count, suffix)| {
                        count.parse::<u64>().is_ok_and(|count| count > 0)
                            && matches!(suffix, "monitor still running" | "monitors still running")
                    })
                })
    })
}

/// Follower reconciles (1s each) probed back-to-back after the transcript
/// records a command that starts long on-screen work. Claude paints the
/// compaction row within a second or two of the `/compact` record; eight
/// covers a slow first repaint without turning into a polling tier.
pub const SESSION_CHAT_ACTIVITY_COMMAND_PROBE_TICKS: u64 = 8;

/*
CDXC:AgentScreenDetection 2026-09-02:
The transcript is the one place BOTH ways of issuing `/compact` land: Claude
records `<command-name>/compact</command-name>` whether the bytes came from the
chat composer or were typed straight into the terminal. The follower keys its
fast re-probe off that row, so a terminal-typed compaction shows its card as
quickly as a chat-sent one, and the follower stays the single owner of what it
has published. Only Claude's user-role command envelope counts: prose that
mentions the command, tool output, and the `<local-command-stdout>` completion
row do not.
*/
pub fn transcript_message_starts_session_chat_activity(
    agent: Option<&str>,
    message: &crate::session_chat::SessionChatMessage,
) -> bool {
    if message.role != crate::session_chat::SessionChatRole::User {
        return false;
    }
    let text = crate::session_chat_decode_claude::message_text(message);
    let command = claude_command_envelope_name(&text).unwrap_or(text.as_str());
    crate::session_chat_options::is_session_chat_activity_command_text(agent, command)
}

/// `<command-name>/compact</command-name>…` → `/compact`. `None` for any text
/// that does not open with Claude's command envelope.
fn claude_command_envelope_name(text: &str) -> Option<&str> {
    const OPEN: &str = "<command-name>";
    const CLOSE: &str = "</command-name>";
    let rest = text.trim_start().strip_prefix(OPEN)?;
    let end = rest.find(CLOSE)?;
    Some(rest[..end].trim())
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

pub fn is_session_chat_compacting_activity(activity: Option<&SessionChatTerminalActivity>) -> bool {
    activity.is_some_and(|activity| activity.kind == SESSION_CHAT_ACTIVITY_COMPACTING)
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

/// Claude picks a playful action word for this row (`Cooked`, `Crunched`,
/// `Sautéed`, ...), so the stable evidence is the rest of its whole grammar:
///
///     <one alphabetic word> for <duration> · <count> shell(s) still running
///     <one alphabetic word> for <duration> · done <time> · <count> shell(s) still running
///
/// Claude paints the identical grammar for a running Monitor
/// (`… · 1 monitor still running`), so the unit word may also be
/// `monitor`/`monitors`; both share the `shells-running` activity kind.
///
/// The first shape is an advancing clock, so move it into progress metadata.
/// In the second shape Claude has frozen that duration and added a completion
/// timestamp; keep both in the stable label rather than making a finished
/// duration tick forward in the client.
fn running_shells_activity(label: &str) -> Option<SessionChatTerminalActivity> {
    let (action, rest) = label.split_once(" for ")?;
    if action.is_empty() || !action.chars().all(char::is_alphabetic) {
        return None;
    }
    let (timing, shell_status) = rest.rsplit_once(" · ")?;
    let (elapsed, completed_at) = match timing.split_once(" · ") {
        Some((elapsed, completed_at)) if completed_at.trim().starts_with("done ") => {
            (elapsed, Some(completed_at.trim()))
        }
        Some(_) => return None,
        None => (timing, None),
    };
    let elapsed_seconds = parse_elapsed_seconds(elapsed.trim())?;
    let (count, suffix) = shell_status.trim().split_once(' ')?;
    let count = count.parse::<u64>().ok()?;
    let valid_suffix = if count == 1 {
        suffix == "shell still running" || suffix == "monitor still running"
    } else {
        suffix == "shells still running" || suffix == "monitors still running"
    };
    if !valid_suffix {
        return None;
    }

    let activity = if let Some(completed_at) = completed_at {
        SessionChatTerminalActivity::new(
            SESSION_CHAT_ACTIVITY_SHELLS_RUNNING,
            format!(
                "{action} for {} · {completed_at} · {count} {suffix}",
                elapsed.trim()
            ),
        )
    } else {
        let mut activity = SessionChatTerminalActivity::new(
            SESSION_CHAT_ACTIVITY_SHELLS_RUNNING,
            format!("{action} · {count} {suffix}"),
        );
        activity.elapsed_seconds = Some(elapsed_seconds);
        activity
    };
    Some(activity)
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
fn compacting_activity_from_line(line: &str) -> Option<SessionChatTerminalActivity> {
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

    None
}

fn activity_from_line(line: &str) -> Option<SessionChatTerminalActivity> {
    if let Some(activity) = compacting_activity_from_line(line) {
        return Some(activity);
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
    claude_status_from_label(marker, rest.trim())
}

/// The activity a Claude status row carries once its marker is known.
fn claude_status_from_label(marker: char, raw_label: &str) -> Option<SessionChatTerminalActivity> {
    if let Some(activity) = running_shells_activity(raw_label) {
        return Some(activity);
    }
    let (label, elapsed_seconds) = stable_status_label(raw_label);
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

/*
Cursor's working row is a Braille spinner followed by one owned state and an
optional token counter:

    ⠠⠜ Thinking 73 tokens
    ⠋ Composing 1.2K tokens

The spinner is required so assistant prose containing either word cannot be
mistaken for live activity. The token count is intentionally not projected:
it is throughput metadata, not stable reasoning content.
*/
fn cursor_activity_from_line(line: &str) -> Option<SessionChatTerminalActivity> {
    let mut tokens = line.split_whitespace();
    let spinner = tokens.next()?;
    if spinner.is_empty()
        || !spinner
            .chars()
            .all(|ch| ('\u{2800}'..='\u{28ff}').contains(&ch))
    {
        return None;
    }
    let label = tokens.next()?;
    if label != "Thinking" && label != "Composing" {
        return None;
    }
    let remaining: Vec<_> = tokens.collect();
    if !remaining.is_empty()
        && (remaining.len() != 2
            || remaining[1] != "tokens"
            || !remaining[0]
                .trim_end_matches(['K', 'M'])
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == '.'))
    {
        return None;
    }
    Some(SessionChatTerminalActivity::new(
        SESSION_CHAT_ACTIVITY_CURSOR_THINKING,
        label,
    ))
}

/*
CDXC:AgentScreenDetection 2026-09-02:
One physical screen row with its layout kept. `normalized_screen_lines` throws
the indentation away, which is right for every marker scan but wrong for the
`⏺` row: Claude wraps a long status at the terminal width and paints the rest
on rows indented by two spaces, and a scan that reads only the marker row
publishes a label cut mid-sentence. The client showed that cut prefix next to
the transcript's full sentence for the rest of the session, because a prefix
never text-matches the sentence it was cut from. Keeping `indent` and
`after_blank` lets the detector re-join exactly the wrapped rows and nothing
below them.
*/
struct ScreenRow {
    /// Trimmed text, exactly what `normalized_screen_lines` would hold.
    text: String,
    /// Leading spaces on the physical row.
    indent: usize,
    /// A blank row separated this row from the previous non-blank one.
    after_blank: bool,
    /// Column after the row's last character on the physical row, before any
    /// space collapsing, so a right-aligned row can be told from a wrapped one.
    end: usize,
}

fn screen_rows(screen_text: &str) -> Vec<ScreenRow> {
    let mut rows = Vec::new();
    let mut after_blank = false;
    for raw in screen_text.lines() {
        let stripped = crate::session_chat_options::strip_ansi_sgr(raw);
        let line = crate::session_chat_options::normalize_spaces(&stripped);
        let text = line.trim();
        if text.is_empty() {
            after_blank = true;
            continue;
        }
        rows.push(ScreenRow {
            text: text.to_string(),
            indent: line.len() - line.trim_start().len(),
            after_blank,
            end: stripped.trim_end().chars().count(),
        });
        after_blank = false;
    }
    rows
}

/*
CDXC:AgentScreenDetection 2026-09-11 WHY:
Claude Code paints one receipt row per hook under its `✻ Compacted` line
(`PostCompact [<command>] completed successfully: {"continue":true}`), indented
like wrapped prose and with no bullet. A second PostCompact hook whose command
was a 2 KB inline script wrapped over fourteen such rows, and the `⎿ Referenced
file` stack under it made the walk-back in `claude_tool_activity` read the whole
run as a pending tool call, so the chat showed the script as a tool card; once
the marker scrolled off, the same rows were the top of the grid and read as a
headless message tail. Receipts are never a tool or a message, so they are cut
out before any scan. A run is found from its closing row (`] completed
successfully: …`, `] failed …`) and walked back to its `<Event> [` row, because
the opening row may already have scrolled off the grid.
*/
fn strip_claude_hook_receipt_rows(rows: &mut Vec<ScreenRow>) {
    let mut index = 0;
    while index < rows.len() {
        if !is_claude_hook_receipt_end(&rows[index].text) {
            index += 1;
            continue;
        }
        let indent = rows[index].indent;
        let mut start = index;
        while start > 0 && !is_claude_hook_receipt_start(&rows[start].text) {
            let previous = &rows[start - 1];
            if rows[start].after_blank
                || previous.indent != indent
                || previous.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER)
                || is_claude_hook_receipt_end(&previous.text)
                || activity_from_line(&previous.text).is_some()
            {
                break;
            }
            start -= 1;
        }
        rows.drain(start..=index);
        index = start;
    }
}

/// `PostCompact [`, `SessionStart [`, …: a hook event name followed by the
/// bracketed command Claude ran for it.
fn is_claude_hook_receipt_start(text: &str) -> bool {
    let name_len = text
        .bytes()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    name_len >= 3 && text.as_bytes()[0].is_ascii_uppercase() && text[name_len..].starts_with(" [")
}

/// The row that closes a receipt: `…] completed successfully: {…}` or the
/// failure wordings Claude uses in its place.
fn is_claude_hook_receipt_end(text: &str) -> bool {
    let Some(at) = text.rfind("] ") else {
        return false;
    };
    let status = &text[at + 2..];
    status.starts_with("completed successfully")
        || status.starts_with("failed")
        || status.starts_with("timed out")
        || status.starts_with("blocked")
        || status.starts_with("cancelled")
}

/// The grid width as painted: the composer rules span every column.
fn screen_width(rows: &[ScreenRow]) -> usize {
    rows.iter().map(|row| row.end).max().unwrap_or(0)
}

/// Minimum indent of a row that continues the `⏺` line above it. Wrapped prose
/// sits at exactly two spaces; a wrapped list item inside that prose sits
/// deeper, so anything at least this deep still belongs to the status.
const CLAUDE_STATUS_CONTINUATION_INDENT: usize = 2;

/// Deepest indent of a row that can still continue the `⏺` line: wrapped
/// prose sits at two, a wrapped item of a nested list a few deeper. A row that
/// starts far to the right is a second column (a side pane), not a wrap.
const CLAUDE_STATUS_CONTINUATION_MAX_INDENT: usize = 8;

/// Claude's tool-output gutter. It is indented like a continuation row but
/// starts the tool block, so it ends the status text.
const CLAUDE_TOOL_OUTPUT_MARKER: char = '⎿';

/// Rows of a tool block carried as the activity's detail. Claude itself
/// collapses long blocks, so this only bounds a fully expanded one.
const CLAUDE_TOOL_DETAIL_MAX_ROWS: usize = 12;

/// A row that continues the status row above it: indented, physically
/// contiguous, and neither a gutter nor a marker row of its own.
fn is_claude_continuation_row(row: &ScreenRow) -> bool {
    (CLAUDE_STATUS_CONTINUATION_INDENT..=CLAUDE_STATUS_CONTINUATION_MAX_INDENT)
        .contains(&row.indent)
        && !row.after_blank
        && !row.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER)
        && activity_from_line(&row.text).is_none()
}

/*
CDXC:SessionChatTerminalActivity 2026-09-03:
Only rows that are physically contiguous with the `⏺` row are part of it. A
blank row ends the status even though what follows is indented the same way,
because Claude paints everything that belongs to the turn under that bullet
with the same two-space indent:

    ⏺ Found a lead in the daemon log: the visible claim arrives, then a hidden
      claim follows within the same second.

      Running cd /Users/madda/dev/_active/Ghostex; rg -n "zmx_c…
      ⎿  $ cd /Users/madda/dev/_active/Ghostex; rg -n …

      Ran 6 shell commands

The in-flight tool row, the collapsed "Ran 6 shell commands" summary, and a
second paragraph of the message are indistinguishable by layout. Joining past
the blank once swallowed the tool row into the status, which then never
matched the transcript's sentence and produced a fresh near-duplicate for
every tool that followed. Stopping at the blank makes the label the message's
first paragraph: always a prefix of the transcript text, so the client can
retire it by prefix and never needs to guess what the indented rows below were.
*/
fn joined_claude_status_line(rows: &[ScreenRow], index: usize) -> String {
    let mut label = rows[index].text.clone();
    for row in &rows[index + 1..] {
        if !is_claude_continuation_row(row) {
            break;
        }
        label.push(' ');
        label.push_str(&row.text);
    }
    label
}

/*
CDXC:SessionChatTerminalActivity 2026-09-04 WHY:
A tool call is recognised by its `⎿` output gutter, not by its bullet. Claude
paints the in-flight tool row both as `⏺ Dumping other live Claude screens…`
and as `  Running cd …` (same row, bullet absent), so a detector keyed on the
bullet saw the tool appear and vanish with the paint, and the client card
blinked once a second and pushed the transcript up and down with it. The row
directly above the gutter, walked back over its wrapped rows, is the tool
whether the bullet is drawn or not. What sits above a gutter is a tool only
when it is a bullet row or an indented row: the spinner's `⎿ Tip:` and a
prompt's `⎿ Referenced file` hang under marker and prompt rows and are not.
*/
fn claude_tool_activity(rows: &[ScreenRow], gutter: usize) -> Option<SessionChatTerminalActivity> {
    if gutter == 0 || rows[gutter].after_blank {
        return None;
    }
    // CDXC:SessionChat 2026-09-05 WHY:
    // Claude also puts skill-availability notices under multiline user prompts, so their gutter is not evidence that the preceding paragraph is a tool call.
    let mut notice = rows[gutter]
        .text
        .trim_start_matches(CLAUDE_TOOL_OUTPUT_MARKER)
        .split_whitespace();
    if notice
        .next()
        .is_some_and(|count| count.bytes().all(|byte| byte.is_ascii_digit()))
        && matches!(notice.next(), Some("skill" | "skills"))
        && notice.next() == Some("available")
        && notice.next().is_none()
    {
        return None;
    }
    let mut start = gutter - 1;
    while start > 0 && is_claude_continuation_row(&rows[start]) {
        start -= 1;
    }
    let row = &rows[start];
    let bullet = row.text.starts_with('⏺');
    // A stacked gutter (Claude's compaction summary paints one `⎿` row per
    // referenced file, directly under each other) is never a tool row: the
    // row above a gutter that is itself a gutter belongs to whatever sits
    // above the whole stack.
    if row.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER)
        || (!bullet && row.indent < CLAUDE_STATUS_CONTINUATION_INDENT)
    {
        return None;
    }
    let label = joined_claude_status_line(rows, start);
    let raw_label = label
        .strip_prefix('⏺')
        .map_or(label.as_str(), str::trim_start);
    let mut activity = claude_status_from_label('⏺', raw_label)?;
    if activity.kind == SESSION_CHAT_ACTIVITY_CLAUDE_STATUS {
        activity.kind = SESSION_CHAT_ACTIVITY_CLAUDE_TOOL;
        activity.detail = claude_tool_gutter_text(rows, gutter);
    }
    Some(activity)
}

/*
CDXC:SessionChatTerminalActivity 2026-09-04 DECISION:
User: the pending tool card must open to show the actual tool call text the
TUI shows under the row (the `⎿ $ rg -n …` block), in a mono code area; the
text as Claude painted it is enough, nothing is fetched from anywhere else.
The block is the gutter row and the rows indented under it until the next
blank row or marker.
*/
fn claude_tool_gutter_text(rows: &[ScreenRow], gutter: usize) -> Option<String> {
    let gutter_indent = rows[gutter].indent;
    let mut lines = vec![rows[gutter]
        .text
        .trim_start_matches(CLAUDE_TOOL_OUTPUT_MARKER)
        .trim()
        .to_string()];
    for row in &rows[gutter + 1..] {
        if lines.len() >= CLAUDE_TOOL_DETAIL_MAX_ROWS
            || row.after_blank
            || row.indent <= gutter_indent
            || row.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER)
            || activity_from_line(&row.text).is_some()
        {
            break;
        }
        lines.push(row.text.clone());
    }
    let text = lines.join("\n");
    (!text.trim().is_empty()).then_some(text)
}

/// `Some` while the agent is painting a live line this build understands.
///
/// CDXC:AgentScreenDetection 2026-09-11 DECISION:
/// User: detect compaction and Claude's other live activity across the whole terminal screen so long queued messages cannot hide their chat indicators.
/// This extends the compaction-only scan decision from 2026-09-10 to tool progress, workflow waits, and running shell/monitor indicators.
pub fn detect_session_chat_terminal_activity(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatTerminalActivity> {
    let agent = session_chat_option_agent(agent)?;
    if agent == SessionChatOptionAgent::Cursor {
        let lines = crate::session_chat_agent_fleet::normalized_screen_lines(screen_text);
        return lines
            .iter()
            .rev()
            .take(CURSOR_ACTIVITY_SCAN_LINES)
            .find_map(|line| cursor_activity_from_line(line));
    }
    // Claude Code is the only remaining CLI whose compaction paints this row;
    // codex compacts without a progress screen, so it would only false-match.
    if agent != SessionChatOptionAgent::Claude {
        return None;
    }
    let mut rows = screen_rows(screen_text);
    strip_claude_hook_receipt_rows(&mut rows);
    for index in (0..rows.len()).rev() {
        if let Some(mut activity) = compacting_activity_from_line(&rows[index].text) {
            activity.percent = rows[index + 1..]
                .iter()
                .take(ACTIVITY_PERCENT_LOOKAHEAD)
                .find_map(|candidate| parse_percent(&candidate.text));
            return Some(activity);
        }
    }
    /*
    CDXC:AgentScreenDetection 2026-08-23: cut the background-agent block off
    the bottom of the screen before reading anything. Its rows are
    indistinguishable from a status line — `⏺` there is the TUI's selection
    marker, so a selected subagent paints `⏺ general-purpose  Fixing tool-ro…`
    — and the block sits BELOW the statusline, so newest-match-wins would
    prefer it over the real status line and its rows would spend the scan
    window's line budget getting there.
    */
    let lines: Vec<String> = rows.iter().map(|row| row.text.clone()).collect();
    if let Some(start) = crate::session_chat_agent_fleet::agent_fleet_block_start(&lines) {
        rows.truncate(start);
    }
    // Newest match wins: a screen can still hold the tail of a previous run.
    // The whole capture is scanned, not a bottom window: a message Claude is
    // still writing can be far taller than any window, and its `⏺` row is the
    // only thing that says where it starts.
    for index in (0..rows.len()).rev() {
        let line = &rows[index].text;
        if line.starts_with(CLAUDE_TOOL_OUTPUT_MARKER) {
            if let Some(activity) = claude_tool_activity(&rows, index) {
                return Some(activity);
            }
            continue;
        }
        let Some(activity) = activity_from_line(line) else {
            continue;
        };
        if activity.kind == SESSION_CHAT_ACTIVITY_CLAUDE_STATUS && line.starts_with('⏺') {
            return Some(claude_stream_activity(&rows, Some(index)));
        }
        return Some(activity);
    }
    headless_claude_stream_sample(&rows)
}

/// Markers that end a Claude message block whatever their indent: the next
/// bullet, a tool gutter, the working spinner and its allowlisted cousins, the
/// thinking header, the composer prompt and the rules around it.
const CLAUDE_BLOCK_TERMINATORS: &str = "⏺⎿❯∴─╭╰│✳✶✻✽✸✹✺✷✴";

/// A row painted flush against the right edge, deeper than any wrapped prose
/// or nested list sits. Claude Code puts its own notices there (`Update
/// available! Run: …`), and one stitched itself into a streamed story.
fn is_right_aligned_notice(row: &ScreenRow, width: usize) -> bool {
    if row.indent <= CLAUDE_STATUS_CONTINUATION_MAX_INDENT {
        return false;
    }
    // Claude leaves a couple of columns of margin after the notice, and a
    // row that starts past any plausible code indentation is never prose.
    row.indent > AGENT_STREAM_MAX_ROW_INDENT || (width > 0 && row.end + 4 >= width)
}

/// Deepest indent a painted message row can have: wrapped prose sits at two,
/// nested lists and code a few dozen deeper. Anything past this is chrome.
const AGENT_STREAM_MAX_ROW_INDENT: usize = 40;

fn is_claude_block_row(row: &ScreenRow, width: usize) -> bool {
    row.indent >= CLAUDE_STATUS_CONTINUATION_INDENT
        && !is_right_aligned_notice(row, width)
        && !row
            .text
            .chars()
            .next()
            .is_some_and(|marker| CLAUDE_BLOCK_TERMINATORS.contains(marker))
}

fn agent_stream_row(row: &ScreenRow) -> AgentStreamRow {
    AgentStreamRow {
        text: row.text.clone(),
        indent: row.indent,
        after_blank: row.after_blank,
    }
}

/// The message block that starts at the `⏺` row `head` (or at the top of the
/// grid when the bullet has scrolled off), down to the next marker row.
fn claude_stream_activity(rows: &[ScreenRow], head: Option<usize>) -> SessionChatTerminalActivity {
    let mut block = Vec::new();
    let first = match head {
        Some(head) => {
            let text = rows[head]
                .text
                .strip_prefix('⏺')
                .map_or(rows[head].text.as_str(), str::trim_start);
            block.push(AgentStreamRow {
                text: text.to_string(),
                indent: CLAUDE_STATUS_CONTINUATION_INDENT,
                after_blank: false,
            });
            head + 1
        }
        None => 0,
    };
    let width = screen_width(rows);
    for row in &rows[first..] {
        if !is_claude_block_row(row, width) {
            break;
        }
        block.push(agent_stream_row(row));
    }
    if head.is_some() && block.len() == 1 {
        if let Some(tool) = claude_tool_row_without_gutter(&block[0].text) {
            return tool;
        }
    }
    let mut activity = SessionChatTerminalActivity::new(SESSION_CHAT_ACTIVITY_AGENT_STREAM, "");
    activity.stream = Some(AgentStreamRows {
        rows: block,
        head_visible: head.is_some(),
    });
    activity.render_agent_stream();
    activity
}

/*
CDXC:AgentScreenDetection 2026-09-11 WHY:
Claude repaints an in-flight tool row every second with and without its
bullet and with and without its `⎿` gutter, and before the tool's input has
finished streaming it paints a `⏺ Running 1 shell command…` placeholder with
no gutter at all. Read as a message block, those rows became a streaming
bubble that blinked in and out of the chat for the whole tool run. Two shapes
prove a lone bullet row is a tool, gutter or not: the ` · 4s` clock Claude
appends only to running tool rows, and the `Running <n> …` placeholder
grammar. A row with neither still lands as a stream for at most a probe: the
transcript records the tool call the moment the row is painted, and the
client hides a stream while the newest transcript row is a tool call waiting
for its result (session-chat-terminal-stream.ts).
*/
fn claude_tool_row_without_gutter(text: &str) -> Option<SessionChatTerminalActivity> {
    let (stable, elapsed_seconds) = trailing_elapsed_status(text);
    if elapsed_seconds.is_some() && !stable.is_empty() {
        let mut activity =
            SessionChatTerminalActivity::new(SESSION_CHAT_ACTIVITY_CLAUDE_TOOL, stable);
        activity.elapsed_seconds = elapsed_seconds;
        return Some(activity);
    }
    let placeholder = text.strip_suffix('…')?;
    let rest = placeholder.strip_prefix("Running ")?;
    let (count, noun) = rest.split_once(' ')?;
    if count.is_empty()
        || !count.bytes().all(|byte| byte.is_ascii_digit())
        || noun.trim().is_empty()
    {
        return None;
    }
    Some(SessionChatTerminalActivity::new(
        SESSION_CHAT_ACTIVITY_CLAUDE_TOOL,
        text,
    ))
}

/// A grid whose top row is an indented message row with no bullet anywhere:
/// the tail of something that scrolled. Only `merge_agent_stream` can say
/// whether it continues a message already being read.
fn headless_claude_stream_sample(rows: &[ScreenRow]) -> Option<SessionChatTerminalActivity> {
    let first = rows.first()?;
    if !is_claude_block_row(first, screen_width(rows)) {
        return None;
    }
    Some(claude_stream_activity(rows, None))
}

/// Deepest relative indent a rendered stream line keeps (four opens a markdown
/// indented code block).
const AGENT_STREAM_MAX_RENDERED_INDENT: usize = 3;

/// Rows of one accumulated stream after which growth stops; a reply this tall
/// is not something the chat needs a live mirror of.
const AGENT_STREAM_MAX_ROWS: usize = 4_000;

/// Rows from the end of the accumulated stream tried as stitch anchors. The
/// last row is the tip Claude is still appending to, so it rarely matches;
/// the rows above it are complete and stable.
const AGENT_STREAM_ANCHOR_ROWS: usize = 12;

/// Shortest row text that may anchor a stitch on its own; shorter rows (a
/// lone bullet, `and`) repeat too easily to place the sample.
const AGENT_STREAM_ANCHOR_MIN_CHARS: usize = 16;

fn starts_list_item(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if matches!(first, '-' | '•' | '*' | '◦' | '▪' | '☐' | '☒' | '✔' | '✓') {
        return chars.next().is_some_and(char::is_whitespace);
    }
    if first.is_ascii_digit() {
        let rest: String = chars.collect();
        let digits_end = rest
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(rest.len());
        let after = &rest[digits_end..];
        return (after.starts_with(". ") || after.starts_with(") ")) && digits_end <= 2;
    }
    false
}

impl SessionChatTerminalActivity {
    /// `text` and `label` from `stream`: wrapped prose rows re-join into one
    /// paragraph, blank rows separate paragraphs, and rows Claude painted
    /// deeper than the message indent (list nesting, code) or that open a list
    /// item keep their own line with their relative indent.
    fn render_agent_stream(&mut self) {
        let Some(stream) = self.stream.as_ref() else {
            return;
        };
        let mut lines: Vec<String> = Vec::new();
        let mut paragraph = String::new();
        let mut first_paragraph: Option<String> = None;
        let flush =
            |paragraph: &mut String, lines: &mut Vec<String>, first: &mut Option<String>| {
                if paragraph.is_empty() {
                    return;
                }
                if first.is_none() {
                    *first = Some(paragraph.clone());
                }
                lines.push(std::mem::take(paragraph));
            };
        for row in &stream.rows {
            // Capped below four spaces: markdown reads a deeper indent as a
            // code block, and a nested list row or a wrapped tip painted deep
            // opened an empty "code" box under the streamed text.
            let relative = row
                .indent
                .saturating_sub(CLAUDE_STATUS_CONTINUATION_INDENT)
                .min(AGENT_STREAM_MAX_RENDERED_INDENT);
            let own_line = relative > 0 || starts_list_item(&row.text);
            if row.after_blank {
                flush(&mut paragraph, &mut lines, &mut first_paragraph);
                lines.push(String::new());
            }
            if own_line {
                flush(&mut paragraph, &mut lines, &mut first_paragraph);
                if first_paragraph.is_none() {
                    first_paragraph = Some(row.text.clone());
                }
                lines.push(format!("{}{}", " ".repeat(relative), row.text));
                continue;
            }
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(&row.text);
        }
        flush(&mut paragraph, &mut lines, &mut first_paragraph);
        let mut text = lines.join("\n");
        if !stream.head_visible {
            text.insert_str(0, "… ");
        }
        self.label = first_paragraph.unwrap_or_default();
        self.text = Some(text);
    }
}

/// Where `sample` continues `previous`: the index in each of the row the two
/// share, preferring a two-row window and accepting a single row only when it
/// occurs once in the sample.
fn agent_stream_anchor(
    previous: &[AgentStreamRow],
    sample: &[AgentStreamRow],
) -> Option<(usize, usize)> {
    let start = previous.len().saturating_sub(AGENT_STREAM_ANCHOR_ROWS);
    for index in (start..previous.len()).rev() {
        let row = &previous[index].text;
        if row.chars().count() < AGENT_STREAM_ANCHOR_MIN_CHARS {
            continue;
        }
        if index > 0 {
            let above = &previous[index - 1].text;
            if let Some(found) = sample
                .iter()
                .enumerate()
                .skip(1)
                .rev()
                .find(|(position, candidate)| {
                    candidate.text == *row && sample[position - 1].text == *above
                })
                .map(|(position, _)| position)
            {
                return Some((index, found));
            }
        }
        let mut matches = sample
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.text == *row)
            .map(|(position, _)| position);
        if let (Some(found), None) = (matches.next(), matches.next()) {
            return Some((index, found));
        }
    }
    None
}

/// Loose identity of a message's first row across repaints: Claude re-renders
/// markdown as it streams, so decoration may change while the words hold.
fn same_agent_stream_head(previous: &str, sample: &str) -> bool {
    let normalize = |text: &str| -> String {
        text.chars()
            .filter(|ch| !matches!(ch, '*' | '_' | '`' | '#' | '~'))
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let previous = normalize(previous);
    let sample = normalize(sample);
    if previous.is_empty() || sample.is_empty() {
        return previous.is_empty() && sample.is_empty();
    }
    previous.starts_with(&sample)
        || sample.starts_with(&previous)
        || previous.chars().take(20).eq(sample.chars().take(20))
}

/// Folds one `agent-stream` probe into what earlier probes accumulated for the
/// same session. Returns false when the sample cannot be placed: a headless
/// grid with nothing to stitch onto is not a message, and the caller drops it.
///
/// A sample whose bullet is on screen is complete from the message's first
/// row, so it replaces the accumulated rows outright (that is also how
/// Claude's in-place repaints reach the client). It continues the previous
/// stream when the previous rows also started at the bullet and the first rows
/// agree; otherwise it is a new message and keeps its own fresh `detected_at`.
pub fn merge_agent_stream(
    current: &mut SessionChatTerminalActivity,
    previous: Option<&SessionChatTerminalActivity>,
) -> bool {
    let Some(sample) = current.stream.take() else {
        return false;
    };
    let previous = previous.filter(|previous| previous.kind == SESSION_CHAT_ACTIVITY_AGENT_STREAM);
    let previous_rows = previous.and_then(|previous| previous.stream.as_ref());
    let merged = if sample.head_visible {
        let continues = previous_rows.is_some_and(|rows| {
            rows.head_visible
                && match (rows.rows.first(), sample.rows.first()) {
                    (Some(first), Some(head)) => same_agent_stream_head(&first.text, &head.text),
                    (None, _) => true,
                    _ => false,
                }
        });
        if continues {
            current.detected_at = previous.unwrap().detected_at.clone();
        }
        sample
    } else {
        let Some(rows) = previous_rows else {
            return false;
        };
        let Some((keep_through, resume_after)) = agent_stream_anchor(&rows.rows, &sample.rows)
        else {
            return false;
        };
        let mut stitched = rows.rows[..=keep_through].to_vec();
        if stitched.len() < AGENT_STREAM_MAX_ROWS {
            stitched.extend_from_slice(&sample.rows[resume_after + 1..]);
        }
        current.detected_at = previous.unwrap().detected_at.clone();
        AgentStreamRows {
            rows: stitched,
            head_visible: rows.head_visible,
        }
    };
    current.stream = Some(merged);
    current.render_agent_stream();
    true
}
