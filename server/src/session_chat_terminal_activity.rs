/*
CDXC:SessionChatTerminalActivity 2026-08-22:
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

/// The `⏺` row of a Claude Code tool call: the row has the `⎿` output gutter
/// directly under it. Shown as live work only, never as chat history — the
/// transcript writes the call itself once its result lands, and the screen
/// paints the description in a different form ("Reading …" for "Read …")
/// than the transcript stores, so the row could never be matched against it.
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
        Value::Object(map)
    }

    /*
    CDXC:SessionChatCompactingStatus 2026-09-02:
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

/// Follower reconciles (1s each) probed back-to-back after the transcript
/// records a command that starts long on-screen work. Claude paints the
/// compaction row within a second or two of the `/compact` record; eight
/// covers a slow first repaint without turning into a polling tier.
pub const SESSION_CHAT_ACTIVITY_COMMAND_PROBE_TICKS: u64 = 8;

/*
CDXC:SessionChatCompactingStatus 2026-09-02:
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
    let raw_label = rest.trim();
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
CDXC:SessionChatTerminalActivity 2026-09-02:
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
}

fn screen_rows(screen_text: &str) -> Vec<ScreenRow> {
    let mut rows = Vec::new();
    let mut after_blank = false;
    for raw in screen_text.lines() {
        let line = crate::session_chat_options::normalize_spaces(
            &crate::session_chat_options::strip_ansi_sgr(raw),
        );
        let text = line.trim();
        if text.is_empty() {
            after_blank = true;
            continue;
        }
        rows.push(ScreenRow {
            text: text.to_string(),
            indent: line.len() - line.trim_start().len(),
            after_blank,
        });
        after_blank = false;
    }
    rows
}

/// Minimum indent of a row that continues the `⏺` line above it. Wrapped prose
/// sits at exactly two spaces; a wrapped list item inside that prose sits
/// deeper, so anything at least this deep still belongs to the status.
const CLAUDE_STATUS_CONTINUATION_INDENT: usize = 2;

/// Claude's tool-output gutter. It is indented like a continuation row but
/// starts the tool block, so it ends the status text.
const CLAUDE_TOOL_OUTPUT_MARKER: char = '⎿';

/// A `⏺` row read across its wrapped extent.
struct ClaudeStatusExtent {
    /// The row's text with its wrapped continuation rows joined back on.
    label: String,
    /// The `⎿` gutter sits directly under the extent: this is a tool call.
    opens_tool_block: bool,
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

The in-flight tool row (whose own bullet blinks on and off), the collapsed
"Ran 6 shell commands" summary, and a second paragraph of the message are
indistinguishable by layout. Joining past the blank once swallowed the tool row
into the status, which then never matched the transcript's sentence and
produced a fresh near-duplicate for every tool that followed. Stopping at the
blank makes the label the message's first paragraph: always a prefix of the
transcript text, so the client can retire it by prefix and never needs to
guess what the indented rows below were.
*/
fn claude_status_extent(rows: &[ScreenRow], index: usize) -> ClaudeStatusExtent {
    let mut label = rows[index].text.clone();
    let mut next = index + 1;
    while let Some(row) = rows.get(next) {
        if row.after_blank
            || row.indent < CLAUDE_STATUS_CONTINUATION_INDENT
            || row.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER)
            || activity_from_line(&row.text).is_some()
        {
            break;
        }
        label.push(' ');
        label.push_str(&row.text);
        next += 1;
    }
    let opens_tool_block = rows
        .get(next)
        .is_some_and(|row| !row.after_blank && row.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER));
    ClaudeStatusExtent {
        label,
        opens_tool_block,
    }
}

/// `Some` while the agent is painting a live line this build understands.
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
            .take(ACTIVITY_SCAN_LINES)
            .find_map(|line| cursor_activity_from_line(line));
    }
    // Claude Code is the only remaining CLI whose compaction paints this row;
    // codex compacts without a progress screen, so it would only false-match.
    if agent != SessionChatOptionAgent::Claude {
        return None;
    }
    let mut rows = screen_rows(screen_text);
    /*
    CDXC:SessionChatAgentFleet 2026-08-23: cut the background-agent block off
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
    let window_start = rows.len().saturating_sub(ACTIVITY_SCAN_LINES);
    // Newest match wins: a screen can still hold the tail of a previous run.
    for index in (window_start..rows.len()).rev() {
        let line = &rows[index].text;
        let Some(mut activity) = activity_from_line(line) else {
            continue;
        };
        if activity.kind == SESSION_CHAT_ACTIVITY_COMPACTING {
            activity.percent = rows[index + 1..]
                .iter()
                .take(ACTIVITY_PERCENT_LOOKAHEAD)
                .find_map(|candidate| parse_percent(&candidate.text));
        } else if activity.kind == SESSION_CHAT_ACTIVITY_CLAUDE_STATUS && line.starts_with('⏺') {
            // Re-read the status from its whole wrapped extent, so the clock
            // and parenthetical metadata are stripped from the real end of
            // the text rather than from the end of its first row.
            let extent = claude_status_extent(&rows, index);
            if let Some(joined) = activity_from_line(&extent.label) {
                activity = joined;
            }
            if extent.opens_tool_block {
                activity.kind = SESSION_CHAT_ACTIVITY_CLAUDE_TOOL;
            }
        }
        return Some(activity);
    }
    None
}
