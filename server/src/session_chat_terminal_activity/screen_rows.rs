use super::*;

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
pub(super) struct ScreenRow {
    /// Trimmed text, exactly what `normalized_screen_lines` would hold.
    pub(super) text: String,
    /// Leading spaces on the physical row.
    pub(super) indent: usize,
    /// A blank row separated this row from the previous non-blank one.
    pub(super) after_blank: bool,
    /// Column after the row's last character on the physical row, before any
    /// space collapsing, so a right-aligned row can be told from a wrapped one.
    pub(super) end: usize,
    /// Every character of the row is painted bold (known only from a VT capture).
    pub(super) bold: bool,
}

fn screen_rows(screen_text: &str, styled_text: Option<&str>) -> Vec<ScreenRow> {
    let mut styled_lines = styled_text.map(str::lines);
    let mut bold_rows = crate::session_chat_screen_styles::BoldRows::default();
    let mut rows = Vec::new();
    let mut after_blank = false;
    for raw in screen_text.lines() {
        let styled = styled_lines.as_mut().and_then(Iterator::next);
        let painted = crate::session_chat_options::strip_ansi_sgr(raw);
        // Claude Code's updater notices are chrome, never activity (session_chat_update_hint.rs);
        // a row that held only one is skipped without counting as a blank row.
        let stripped = crate::session_chat_update_hint::without_claude_update_notice(&painted);
        if stripped.trim().is_empty() && !painted.trim().is_empty() {
            // The capture's bold state still runs through the row.
            if let Some(styled) = styled {
                bold_rows.next_row(styled, 0, 0);
            }
            continue;
        }
        let line = crate::session_chat_options::normalize_spaces(stripped);
        let text = line.trim();
        let indent = line.len() - line.trim_start().len();
        let end = stripped.trim_end().chars().count();
        let bold = styled.is_some_and(|styled| bold_rows.next_row(styled, indent, end));
        if text.is_empty() {
            after_blank = true;
            continue;
        }
        rows.push(ScreenRow {
            text: text.to_string(),
            indent,
            after_blank,
            end,
            bold,
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
pub(super) fn screen_width(rows: &[ScreenRow]) -> usize {
    rows.iter().map(|row| row.end).max().unwrap_or(0)
}

/// Minimum indent of a row that continues the `⏺` line above it. Wrapped prose
/// sits at exactly two spaces; a wrapped list item inside that prose sits
/// deeper, so anything at least this deep still belongs to the status.
pub(super) const CLAUDE_STATUS_CONTINUATION_INDENT: usize = 2;

/// Deepest indent of a row that can still continue the `⏺` line: wrapped
/// prose sits at two, a wrapped item of a nested list a few deeper. A row that
/// starts far to the right is a second column (a side pane), not a wrap.
pub(super) const CLAUDE_STATUS_CONTINUATION_MAX_INDENT: usize = 8;

/// Claude's tool-output gutter. It is indented like a continuation row but
/// starts the tool block, so it ends the status text.
pub(super) const CLAUDE_TOOL_OUTPUT_MARKER: char = '⎿';

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
/// CDXC:AgentScreenDetection 2026-09-16 WHY:
/// Claude's Bash results can include file diffs followed by another output gutter, even for changes made by another session.
/// A clipped or blank-separated diff tail used to become the shared chat card's heading; a heading cannot sit deeper than the output gutter it owns.
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
        || row.indent > rows[gutter].indent
    {
        return None;
    }
    if !bullet && row.after_blank && !claude_block_owned_by_bullet(rows, start) {
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
CDXC:AgentScreenDetection 2026-09-20 WHY:
Claude paints every row of a turn at the same two-space indent, so a paragraph
that opens after a blank row carries no evidence of its own about whose rows
they are; only the un-indented marker above the whole block says that. A
message from `ghostex agents send` arrives as a header paragraph, a blank row
and the body, and the notice Claude hangs under the submitted prompt (any `⎿`
row) made the walk-back above stop on the body and publish the whole message as
a pending tool call, so the chat showed it as a tool card until the transcript
replaced it with the sender's message. The 2026-09-05 guard covered one wording
of the same shape ("N skills available"); this covers the shape itself. A
blank-separated paragraph is a tool row only while the `⏺` that owns it is
still on the grid: a `>` prompt row, a spinner, a rule or a scrolled-off owner
is not evidence of a tool, and Claude repaints an in-flight tool row every
second anyway.
*/
fn claude_block_owned_by_bullet(rows: &[ScreenRow], start: usize) -> bool {
    let mut index = start;
    while index > 0 {
        index -= 1;
        let row = &rows[index];
        if row.indent < CLAUDE_STATUS_CONTINUATION_INDENT
            || row.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER)
        {
            return row.text.starts_with('⏺');
        }
    }
    false
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
    let mut lines = vec![
        rows[gutter]
            .text
            .trim_start_matches(CLAUDE_TOOL_OUTPUT_MARKER)
            .trim()
            .to_string(),
    ];
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

/// [`detect_session_chat_terminal_activity_styled`] without the VT capture.
pub fn detect_session_chat_terminal_activity(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatTerminalActivity> {
    detect_session_chat_terminal_activity_styled(agent, screen_text, None)
}

/// `Some` while the agent is painting a live line this build understands. `styled_text` is the VT
/// capture `screen_text` was read from, row for row; with it, a streamed Claude message keeps its
/// bold titles.
///
/// CDXC:AgentScreenDetection 2026-09-11 DECISION:
/// User: detect compaction and Claude's other live activity across the whole terminal screen so long queued messages cannot hide their chat indicators.
/// This extends the compaction-only scan decision from 2026-09-10 to tool progress, workflow waits, and running shell/monitor indicators.
pub fn detect_session_chat_terminal_activity_styled(
    agent: Option<&str>,
    screen_text: &str,
    styled_text: Option<&str>,
) -> Option<SessionChatTerminalActivity> {
    let agent = session_chat_option_agent(agent)?;
    if agent == SessionChatOptionAgent::Grok {
        return grok_compacting_activity(screen_text);
    }
    if agent == SessionChatOptionAgent::Hermes {
        return hermes_compacting_activity(screen_text);
    }
    if agent == SessionChatOptionAgent::Cursor {
        let lines = crate::session_chat_agent_fleet::normalized_screen_lines(screen_text);
        let composer = lines.iter().rposition(|line| line.starts_with('→'));
        let summarizing_live =
            composer.is_some_and(|index| lines[index].ends_with("ctrl+c to stop"));
        let status_lines = composer.map_or(lines.as_slice(), |index| &lines[..index]);
        return status_lines
            .iter()
            .rev()
            .take(CURSOR_ACTIVITY_SCAN_LINES)
            .find_map(|line| cursor_activity_from_line(line))
            .filter(|activity| {
                activity.kind != SESSION_CHAT_ACTIVITY_COMPACTING || summarizing_live
            });
    }
    if agent == SessionChatOptionAgent::Codex {
        return screen_text.lines().rev().find_map(|line| {
            codex_compacting_activity_from_line(&crate::session_chat_options::strip_ansi_sgr(line))
        });
    }
    if agent != SessionChatOptionAgent::Claude {
        return None;
    }
    let mut rows = screen_rows(screen_text, styled_text);
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
    if let Some(activity) = claude_shell_command_activity(&rows) {
        return Some(activity);
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
