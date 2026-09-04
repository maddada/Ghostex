/*
CDXC:SessionChat 2026-08-26:
POSITIVE evidence that an agent CLI's input box is on screen and accepting
input, as opposed to the negative evidence session_chat_notice.rs collects.

The two are not the same question and cannot be answered by one detector. A
notice rule says "this specific screen — a trust dialog, an expired login, a
usage limit — is known to eat input". Silence from that catalog is not proof
that a composer exists: a CLI that is still booting, painting an auth screen we
have no rule for, or sitting on a dialog shipped after our last catalog update
looks exactly like a healthy idle agent to it. Every one of those swallowed a
chat message, and the user only learned about it from a delivery-failed banner
minutes later.

So this module asks the opposite question and requires an answer: WHERE is the
composer? Each supported CLI paints its input box with chrome that is stable
across versions and survives the zmx plain capture verbatim, and that chrome is
what a signature matches. Three outcomes, and the middle one is the whole point:

  - `Ready`    — the composer's own chrome is on screen.
  - `NotReady` — the agent HAS a known signature and it is absent, so something
                 else owns the screen. Refuse the send.
  - `Unknown`  — no signature for this agent, or the screen could not be read.
                 FAIL OPEN: every caller proceeds, because a detector that
                 cannot see must never be the thing that blocks a message.

Signatures were measured on 2026-08-26 against live CLIs, in three states each
(just launched, trust accepted, one message exchanged). Two rules came out of
that measurement and are load-bearing:

  - Scan the WHOLE visible capture, not the bottom rows. opencode paints its
    composer MID-SCREEN on a fresh start and only migrates downward after the
    first reply, and Claude/pi/omp all sit above a user-customizable statusline
    whose height this daemon cannot know. Nothing here may index from the
    bottom row.
  - A marker glyph alone is never a signature. codex's trust dialog draws its
    selected option with the same `›` its composer uses (`› 1. Yes, continue`),
    and copilot's draws `❯ 1. Yes` with the same `❯`. Every marker match is
    therefore filtered against the numbered-option shape, and most are further
    required to sit inside the frame the composer draws around itself.

A blocking notice always wins over a matched signature: cursor keeps its
answered trust dialog scrolled above a live composer, and grok paints its
composer while the start menu is still up, so "the box exists" and "the box is
what the next keystroke reaches" are different claims.
*/

use std::time::{Duration, Instant};

use crate::agents::identity::normalize_agent_id;
use crate::domain::DomainRepository;
use crate::paths::GxserverPaths;
use crate::session_chat_notice::SessionChatTerminalNotice;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::storage::open_gxserver_database;

/// Non-blank lines kept from the bottom of a capture. Wide enough to hold
/// opencode's mid-screen composer plus the banner above it on an 80x24 pane,
/// and to hold gemini's full-height dialogs without the composer scan running
/// off the top of them.
const SESSION_CHAT_COMPOSER_SCAN_LINES: usize = 120;

/// Screen tail attached to a readiness verdict, so a client can show WHAT is in
/// the terminal instead of asking for a second capture.
pub const SESSION_CHAT_COMPOSER_TAIL_LINES: usize = 30;

/// Shortest line that can be a full-width rule or block frame. Below this a run
/// of `─` is a inline separator inside prose, not composer chrome.
const SESSION_CHAT_COMPOSER_MIN_RULE_CHARS: usize = 20;

/// Poll cadence for every composer wait. Matches the paste-verification poll:
/// fast enough that a composer that paints in 300ms is not made to wait a
/// second, slow enough that a stuck CLI costs a handful of socket reads.
pub const SESSION_CHAT_COMPOSER_POLL_MS: u64 = 150;

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionChatComposerState {
    /// The composer's chrome is on screen.
    Ready,
    /// This agent has a signature and it is absent, or a blocking notice owns
    /// the screen.
    NotReady,
    /// Nothing can be concluded — no signature for this agent, or no readable
    /// screen. Callers FAIL OPEN on this.
    #[default]
    Unknown,
}

impl SessionChatComposerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotReady => "notReady",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChatComposerReadiness {
    pub state: SessionChatComposerState,
    /// User-facing sentence, set only for `NotReady`.
    pub reason: Option<String>,
    /// Newest `SESSION_CHAT_COMPOSER_TAIL_LINES` non-blank screen lines,
    /// ANSI-stripped, oldest first — the evidence behind the verdict.
    pub screen_tail: Vec<String>,
    /// This exact blocking screen is navigation chrome that Escape safely
    /// closes, rather than a question or decision the user must answer.
    dismiss_with_escape: bool,
}

impl SessionChatComposerReadiness {
    pub fn is_not_ready(&self) -> bool {
        self.state == SessionChatComposerState::NotReady
    }

    pub fn should_dismiss_with_escape(&self) -> bool {
        self.dismiss_with_escape
    }

    fn unknown(screen_tail: Vec<String>) -> Self {
        Self {
            state: SessionChatComposerState::Unknown,
            reason: None,
            screen_tail,
            dismiss_with_escape: false,
        }
    }

    fn ready(screen_tail: Vec<String>) -> Self {
        Self {
            state: SessionChatComposerState::Ready,
            reason: None,
            screen_tail,
            dismiss_with_escape: false,
        }
    }

    fn not_ready(reason: String, screen_tail: Vec<String>) -> Self {
        Self {
            state: SessionChatComposerState::NotReady,
            reason: Some(reason),
            screen_tail,
            dismiss_with_escape: false,
        }
    }

    fn not_ready_dismiss_with_escape(reason: String, screen_tail: Vec<String>) -> Self {
        Self {
            state: SessionChatComposerState::NotReady,
            reason: Some(reason),
            screen_tail,
            dismiss_with_escape: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Signature table
// ---------------------------------------------------------------------------

/// The chrome one agent CLI draws around its input box. Every variant is a
/// SHAPE, never a phrase: placeholder copy ("Ask Codex to do anything") is
/// localized and rewritten between releases, while the frame is what the TUI's
/// layout code draws and does not move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComposerSignature {
    /// A marker line sandwiched between two full-width `─` rules.
    ///
    ///     ────────────────────────────── conversation-summary ─
    ///     ❯
    ///     ──────────────────────────────────────────────────────
    ///
    /// The TOP rule may carry a right-aligned session title, and BELOW the
    /// bottom rule sits a statusline of user-chosen height — which is why the
    /// composer is found by the sandwich and never by counting rows up from the
    /// bottom.
    RuleSandwich { marker: char },
    /// hermes's rule sandwich. Same frame, but the marker line carries the
    /// active profile name when one is selected: `hermes -p harry` prompts
    /// with `harry ❯`, the default profile with a bare `❯` (its
    /// `_get_tui_prompt_symbols` prepends any non-default profile). The
    /// 2026-08-29 measurement ran the default profile and missed this, so
    /// every profiled session read NotReady with its composer on screen.
    /// At most ONE leading word is accepted: profile names are single CLI
    /// tokens, and prose that happens to contain the marker has more.
    ProfiledRuleSandwich { marker: char },
    /// An empty input row between two full-width `─` rules. Blank rows are
    /// removed before matching, so the two rules are adjacent in `lines`.
    EmptyRuleSandwich,
    /// A bare marker line with no frame at all: `› Ask Codex to do anything`.
    /// The weakest signature here, so it is the one that most needs the
    /// numbered-option filter — codex's own trust dialog draws `› 1. Yes,
    /// continue` with this exact marker.
    BareMarker { marker: char },
    /// A marker line INSIDE a `│ … │` box: `│ ❯                          │`.
    BoxedMarker { marker: char },
    /// A marker line between half-block rules, `▄▄▄▄…` above and `▀▀▀▀…` below.
    HalfBlockFrame { marker: char },
    /// opencode's heavy left bar (`┃` rows) closed by a `╹▀▀▀▀…` foot. It has
    /// no right or top border and no marker glyph, so the foot plus at least
    /// one bar row above it IS the signature.
    HeavyBarFoot,
    /// A rounded box at the very bottom of the screen, at most
    /// `ROUNDED_FOOT_MAX_BOX_LINES` tall, whose borders carry the statusline:
    ///
    ///     ╭── π  > ⬢ GPT-5.6-Sol · … ▶────────────╮
    ///     ╰─                                    ─╯
    ///
    /// Height is the discriminator: omp's dialogs are full-screen boxes.
    TrailingRoundedFoot,
    /// A THREE-line rounded box holding a marker line: `╭─╮ / │ > … │ / ╰─╯`.
    /// gemini draws dialogs and the composer with the same rounded chrome, so
    /// only the composer's exact height admits a match.
    ShortRoundedBoxMarker { marker: char },
}

/// Tallest rounded box `TrailingRoundedFoot` will accept as a composer. Two
/// lines is what an empty omp composer draws; the slack covers one wrapped
/// input row before the shape stops being distinguishable from a dialog.
const ROUNDED_FOOT_MAX_BOX_LINES: usize = 4;

/// How far above a `╹▀▀▀` foot a `┃` bar row may sit and still belong to it.
const HEAVY_BAR_LOOKBACK_LINES: usize = 8;

/*
The table. Keys are `normalize_agent_id` output, so every alias the rest of the
daemon accepts (`claude code`, `cursor-agent`, `openai codex`, `π`) arrives here
already folded.

Agents deliberately absent, each for a stated reason rather than because nobody
got to them:

  - **amp, droid, kiro, codebuddy, qoder, rovodev** and every
    custom agent were not measured. An unmeasured guess would be the same
    failure as pi's, so they read Unknown until someone captures them.
*/
/// Return the measured composer chrome signature for a normalized agent id.
fn composer_signature(agent: &str) -> Option<ComposerSignature> {
    Some(match agent {
        // `❯` between two full-width rules, statusline below.
        "claude" | "openclaude" => ComposerSignature::RuleSandwich { marker: '\u{276f}' },
        // Identical shape to claude's, measured independently.
        "copilot" => ComposerSignature::RuleSandwich { marker: '\u{276f}' },
        // `>` between two full-width rules, statusline below (`? for
        // shortcuts` idle, `esc to cancel` while working). Measured
        // 2026-09-02, Antigravity CLI 1.1.24.
        "antigravity" => ComposerSignature::RuleSandwich { marker: '>' },
        // `❯` (or `<profile> ❯`) between two full-width rules, statusline
        // above the top rule (measured 2026-08-29, Hermes Agent v0.20.4;
        // profile prefix confirmed against v0.20.5 source on 2026-08-30).
        "hermes-agent" => ComposerSignature::ProfiledRuleSandwich { marker: '\u{276f}' },
        // `› ` with no frame.
        "codex" => ComposerSignature::BareMarker { marker: '\u{203a}' },
        // Empty row bounded by two full-width rules, statusline below.
        "pi" => ComposerSignature::EmptyRuleSandwich,
        // `│ ❯ … │`, model/mode drawn into the bottom border.
        "grok" => ComposerSignature::BoxedMarker { marker: '\u{276f}' },
        // `▄▄▄▄` / `→ placeholder` / `▀▀▀▀`.
        "cursor" => ComposerSignature::HalfBlockFrame { marker: '\u{2192}' },
        // `┃` rows over a `╹▀▀▀▀` foot; mid-screen when fresh.
        "opencode" => ComposerSignature::HeavyBarFoot,
        // Two-line rounded box at the bottom with the statusline in its border.
        "omp" => ComposerSignature::TrailingRoundedFoot,
        // Three-line rounded box with a `>` input marker.
        "gemini" => ComposerSignature::ShortRoundedBoxMarker { marker: '>' },
        _ => return None,
    })
}

/// Whether this agent is one the composer table can say anything about. The
/// screen-capture funnel uses it to decide that a capture is worth taking for
/// an agent whose statusline grammar it does not know.
pub fn has_session_chat_composer_signature(agent_id: Option<&str>) -> bool {
    normalize_agent_id(agent_id)
        .as_deref()
        .and_then(composer_signature)
        .is_some()
}

/// The agent id composer detection keys off. Broader than
/// `session_chat_agent_for_session`, which answers "whose transcript format is
/// this?" and so knows only the four agents with a transcript decoder.
pub fn session_chat_composer_agent_id(session: &serde_json::Value) -> Option<String> {
    let launch_icon_agent = || {
        normalize_agent_id(
            session
                .get("launchSettings")
                .and_then(serde_json::Value::as_object)
                .and_then(|settings| settings.get("icon"))
                .and_then(serde_json::Value::as_str),
        )
    };
    match normalize_agent_id(crate::server::first_prompt_agent_name(session).as_deref()) {
        // A `custom-…` id names a sidebar agent CONFIGURATION, not the CLI in
        // the terminal; the CLI family it runs is declared by its icon — the
        // same contract available_draft_agents and session_chat_agent_for_session
        // read. Without this, every terminal classifier (composer signature,
        // statusline options, notices, activity) sees an unknown agent and the
        // chat's option pills never leave their loading skeleton.
        Some(agent_id) if agent_id.starts_with("custom-") => launch_icon_agent().or(Some(agent_id)),
        Some(agent_id) => Some(agent_id),
        None => launch_icon_agent(),
    }
}

// ---------------------------------------------------------------------------
// Line preparation
// ---------------------------------------------------------------------------

/// Non-blank capture lines, ANSI-stripped and whitespace-folded, OLDEST FIRST.
///
/// Blank rows are dropped rather than kept, which is what makes the adjacency
/// tests below survive a TUI that pads its frame with an empty row: the pieces
/// of one frame stay neighbours either way, and no signature here is defined by
/// the blank between two parts.
fn composer_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines().rev() {
        let line = normalize_spaces(&strip_ansi_sgr(raw))
            .trim_end()
            .to_string();
        if line.trim().is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= SESSION_CHAT_COMPOSER_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

fn composer_screen_tail(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .skip(lines.len().saturating_sub(SESSION_CHAT_COMPOSER_TAIL_LINES))
        .map(|line| line.trim_end().to_string())
        .collect()
}

/*
Claude Code's `/config` screen owns the keyboard until Escape closes it. Its
full-width upper-eighth-block rule scales with the terminal width, while the
tab row below it is stable:

    ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔
       Settings  Status  Config  Usage  Stats

Match the shape rather than a fixed rule length. Requiring both adjacent rows
keeps ordinary transcript prose that names the tabs from becoming an input-
blocking screen.
*/
pub fn is_claude_code_settings_screen(agent_id: Option<&str>, screen_text: &str) -> bool {
    let Some(agent) = normalize_agent_id(agent_id) else {
        return false;
    };
    if !matches!(agent.as_str(), "claude" | "openclaude") {
        return false;
    }
    let lines = composer_lines(screen_text);
    lines.windows(2).any(|pair| {
        let rule = pair[0].trim();
        rule.chars().count() >= SESSION_CHAT_COMPOSER_MIN_RULE_CHARS
            && rule.chars().all(|character| character == '\u{2594}')
            && pair[1]
                .split_whitespace()
                .eq(["Settings", "Status", "Config", "Usage", "Stats"])
    })
}

/*
CDXC:SessionChat 2026-09-04 WHY:
The text Claude Code's input box holds, read off the same rule sandwich the
readiness signature matches: the lowest full-width rule is the composer's
foot, the titled rule above it is its head, and the rows between them are the
input, the first one behind the `❯` marker. A wrapped draft continues on the
following rows, so they are joined with single spaces. `None` for an empty box
(a lone marker, or Claude's grey placeholder is not distinguishable from text
here and is left to the caller's comparison) and for any screen without the
sandwich. Used by the returned-prompt detector, which compares this against
the message it just sent.
*/
pub fn claude_composer_input_text(screen_text: &str) -> Option<String> {
    const CLAUDE_COMPOSER_MAX_ROWS: usize = 40;
    let lines = composer_lines(screen_text);
    let foot = (0..lines.len())
        .rev()
        .find(|&index| is_horizontal_rule(&lines[index]))?;
    let head = (foot.saturating_sub(CLAUDE_COMPOSER_MAX_ROWS)..foot)
        .rev()
        .find(|&index| is_titled_horizontal_rule(&lines[index]))?;
    let inner = &lines[head + 1..foot];
    let first = inner.first()?;
    if !is_marker_line(first, '\u{276f}') {
        return None;
    }
    let mut text = first
        .trim()
        .trim_start_matches('\u{276f}')
        .trim()
        .to_string();
    for line in &inner[1..] {
        text.push(' ');
        text.push_str(line.trim());
    }
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Display rows for terminal-preview clients, OLDEST FIRST.
///
/// Readiness matching needs whitespace-folded non-blank lines, but a visual
/// preview must retain indentation, blank rows, and full-width frame rules.
/// Omit only empty padding below the last painted row, then bound the physical
/// rows to the same small tail size used by the endpoint today.
fn composer_terminal_preview_tail(text: &str) -> Vec<String> {
    let lines: Vec<String> = text
        .lines()
        .map(|raw| strip_ansi_sgr(raw).trim_end().to_string())
        .collect();
    let Some(end) = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map(|index| index + 1)
    else {
        return Vec::new();
    };
    lines[end.saturating_sub(SESSION_CHAT_COMPOSER_TAIL_LINES)..end].to_vec()
}

/// A run of one frame character wide enough to be composer chrome. `ratio_num`
/// / `ratio_den` is the share of non-space characters the run must own: rules
/// that carry a title (`──── conversation-summary ─`) are still rules, half
/// block frames never carry text at all.
fn is_frame_rule(line: &str, frame: char, ratio_num: usize, ratio_den: usize) -> bool {
    let mut frame_chars = 0usize;
    let mut solid_chars = 0usize;
    for ch in line.chars() {
        if ch.is_whitespace() {
            continue;
        }
        solid_chars += 1;
        if ch == frame {
            frame_chars += 1;
        }
    }
    solid_chars >= SESSION_CHAT_COMPOSER_MIN_RULE_CHARS
        && frame_chars * ratio_den >= solid_chars * ratio_num
}

/// `────────── title ─` counts; `│ … │` and `╭ … ╮` do not, because a vertical
/// border means this row belongs to a box rather than being a rule.
fn is_horizontal_rule(line: &str) -> bool {
    !line
        .chars()
        .any(|ch| matches!(ch, '\u{2502}' | '\u{2503}' | '\u{256d}' | '\u{2570}'))
        && is_frame_rule(line, '\u{2500}', 3, 5)
}

/*
The TOP rule of a sandwich, which may carry a right-aligned session title:

    ───────────────────────────────── posthog-privacy-tracking-setup ─

A ratio test cannot recognize this row: the title's length is fixed while the
`─` fill shrinks with the pane, so on a narrow pane the dashes fall under any
proportion threshold and the composer reads as absent (seen live on 2026-08-28
with the title above in a ~66-column pane). So this is a SHAPE test instead —
a leading `─` run, a trailing `─`, no box verticals — and the discrimination
against transcript prose stays where it belongs, in the sandwich adjacency the
caller also requires. The leading-run floor is deliberately small and FIXED:
the fill is `pane width − title − 2`, so any width-scaled minimum recreates the
ratio failure at some width/title pair (a 30-char title in a 40-column pane
leaves 8 dashes).
*/
const TITLED_RULE_MIN_LEAD: usize = 4;

fn is_titled_horizontal_rule(line: &str) -> bool {
    if line
        .chars()
        .any(|ch| matches!(ch, '\u{2502}' | '\u{2503}' | '\u{256d}' | '\u{2570}'))
    {
        return false;
    }
    let trimmed = line.trim();
    trimmed.ends_with('\u{2500}')
        && trimmed.chars().take_while(|&ch| ch == '\u{2500}').count() >= TITLED_RULE_MIN_LEAD
}

/*
The filter that keeps a dialog's SELECTED row from reading as a composer. Both
codex and copilot draw the highlighted option with the very glyph their composer
uses:

    › 1. Yes, continue          ❯ 1. Yes
      2. No, quit                 2. Yes, and remember this folder

Anything of the form `<digit>. ` or `<digit>) ` after the marker is that row.
A composer holding user text that happens to start with a digit and a period is
the false negative this accepts; it costs one extra readiness poll, while the
false positive costs a message delivered into a trust dialog.
*/
fn is_numbered_option(rest: &str) -> bool {
    let mut chars = rest.trim_start().chars();
    let mut digits = 0usize;
    for ch in chars.by_ref() {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        return digits > 0 && matches!(ch, '.' | ')');
    }
    false
}

/// True when `line` is an input row for `marker`: it starts with the marker and
/// what follows is not a numbered dialog option.
fn is_marker_line(line: &str, marker: char) -> bool {
    let trimmed = line.trim();
    let Some(rest) = trimmed.strip_prefix(marker) else {
        return false;
    };
    !is_numbered_option(rest)
}

/// The marker test that admits hermes's profile prefix: `harry ❯ hi` matches,
/// `❯ hi` matches, and anything with two or more words before the marker is
/// prose quoting the glyph rather than a prompt.
fn is_profiled_marker_line(line: &str, marker: char) -> bool {
    if is_marker_line(line, marker) {
        return true;
    }
    let trimmed = line.trim();
    let mut words = trimmed.splitn(2, char::is_whitespace);
    let Some(_profile) = words.next().filter(|word| !word.is_empty()) else {
        return false;
    };
    words
        .next()
        .map(str::trim_start)
        .is_some_and(|tail| is_marker_line(tail, marker))
}

/// The same test for a row drawn inside a `│ … │` box: the marker follows the
/// left border rather than the start of the line.
fn is_boxed_marker_line(line: &str, marker: char) -> bool {
    let trimmed = line.trim();
    let Some(inner) = trimmed.strip_prefix('\u{2502}') else {
        return false;
    };
    is_marker_line(inner, marker)
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/*
Every matcher scans BOTTOM-UP. On a screen that shows the composer twice — the
codex transcript echoes each submitted prompt with the same `›` the composer
draws — the lowest match is the live one. Both would answer "ready", so this is
about picking the honest witness rather than about correctness of the verdict.
*/
/// Whether the captured non-blank lines contain the requested composer shape.
fn signature_matches(signature: ComposerSignature, lines: &[String]) -> bool {
    match signature {
        // In both sandwiches the BOTTOM rule stays strict (it is always solid
        // and spans the pane) and only the top rule is allowed to be the
        // titled kind.
        ComposerSignature::RuleSandwich { marker } => {
            (0..lines.len().saturating_sub(2)).rev().any(|index| {
                is_horizontal_rule(&lines[index + 2])
                    && is_marker_line(&lines[index + 1], marker)
                    && is_titled_horizontal_rule(&lines[index])
            })
        }
        ComposerSignature::ProfiledRuleSandwich { marker } => {
            (0..lines.len().saturating_sub(2)).rev().any(|index| {
                is_horizontal_rule(&lines[index + 2])
                    && is_profiled_marker_line(&lines[index + 1], marker)
                    && is_titled_horizontal_rule(&lines[index])
            })
        }
        ComposerSignature::EmptyRuleSandwich => {
            (0..lines.len().saturating_sub(1)).rev().any(|index| {
                is_horizontal_rule(&lines[index + 1]) && is_titled_horizontal_rule(&lines[index])
            })
        }
        ComposerSignature::BareMarker { marker } => lines.iter().rev().any(|line| {
            // A `›` inside a box belongs to that box's content, not to a
            // frameless composer.
            !line.contains('\u{2502}') && is_marker_line(line, marker)
        }),
        ComposerSignature::BoxedMarker { marker } => lines
            .iter()
            .rev()
            .any(|line| is_boxed_marker_line(line, marker)),
        ComposerSignature::HalfBlockFrame { marker } => {
            (0..lines.len().saturating_sub(2)).rev().any(|index| {
                is_frame_rule(&lines[index], '\u{2584}', 9, 10)
                    && is_marker_line(&lines[index + 1], marker)
                    && is_frame_rule(&lines[index + 2], '\u{2580}', 9, 10)
            })
        }
        ComposerSignature::HeavyBarFoot => (0..lines.len()).rev().any(|index| {
            is_heavy_bar_foot(&lines[index])
                && lines[index.saturating_sub(HEAVY_BAR_LOOKBACK_LINES)..index]
                    .iter()
                    .any(|line| line.trim_start().starts_with('\u{2503}'))
        }),
        ComposerSignature::TrailingRoundedFoot => {
            let Some(foot) = lines.len().checked_sub(1) else {
                return false;
            };
            if !lines[foot].trim_start().starts_with('\u{2570}') {
                return false;
            }
            let head = foot.saturating_sub(ROUNDED_FOOT_MAX_BOX_LINES - 1);
            lines[head..foot]
                .iter()
                .any(|line| line.trim_start().starts_with('\u{256d}'))
        }
        ComposerSignature::ShortRoundedBoxMarker { marker } => {
            (0..lines.len().saturating_sub(2)).rev().any(|index| {
                lines[index].trim_start().starts_with('\u{256d}')
                    && is_boxed_marker_line(&lines[index + 1], marker)
                    && lines[index + 2].trim_start().starts_with('\u{2570}')
            })
        }
    }
}

/// `╹▀▀▀▀▀▀…` — opencode's composer foot: one corner glyph followed by a run of
/// upper half blocks.
fn is_heavy_bar_foot(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(rest) = trimmed.strip_prefix('\u{2579}') else {
        return false;
    };
    is_frame_rule(rest, '\u{2580}', 9, 10)
}

// ---------------------------------------------------------------------------
// Public detection
// ---------------------------------------------------------------------------

/// Signature-only readiness for one capture. `agent_id` is any spelling the
/// daemon uses; it is normalized here.
pub fn detect_session_chat_composer_ready(
    agent_id: Option<&str>,
    screen_text: &str,
) -> SessionChatComposerReadiness {
    let lines = composer_lines(screen_text);
    let screen_tail = composer_screen_tail(&lines);
    if lines.is_empty() {
        return SessionChatComposerReadiness::unknown(screen_tail);
    }
    let Some(agent) = normalize_agent_id(agent_id) else {
        return SessionChatComposerReadiness::unknown(screen_tail);
    };
    let Some(signature) = composer_signature(&agent) else {
        return SessionChatComposerReadiness::unknown(screen_tail);
    };
    if is_claude_code_settings_screen(agent_id, screen_text) {
        return SessionChatComposerReadiness::not_ready_dismiss_with_escape(
            "Claude Code settings are open instead of the input box.".to_string(),
            screen_tail,
        );
    }
    if signature_matches(signature, &lines) {
        SessionChatComposerReadiness::ready(screen_tail)
    } else {
        SessionChatComposerReadiness::not_ready(
            format!("The {agent} input box is not on screen yet."),
            screen_tail,
        )
    }
}

/// The full verdict: signature plus the notice detector's veto.
///
/// A blocking notice outranks a matched signature because the two answer
/// different questions. cursor keeps its ANSWERED trust dialog scrolled above a
/// live composer, and grok paints its composer while the start-menu panel is
/// still up; in both the box exists and is still not what the next keystroke
/// reaches.
pub fn detect_session_chat_composer_readiness(
    agent_id: Option<&str>,
    screen_text: &str,
    notice: Option<&SessionChatTerminalNotice>,
) -> SessionChatComposerReadiness {
    let readiness = detect_session_chat_composer_ready(agent_id, screen_text);
    match notice.filter(|notice| notice.blocks_input()) {
        Some(notice) => SessionChatComposerReadiness::not_ready(
            format!("{}. Clear it in the terminal before sending.", notice.title),
            readiness.screen_tail,
        ),
        None => readiness,
    }
}

// ---------------------------------------------------------------------------
// Waiting
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionChatComposerWaitPolicy {
    /// One settle before the first capture, so a composer the CLI has already
    /// painted costs no poll at all.
    pub settle_ms: u64,
    /// Hard ceiling on the whole wait.
    pub timeout_ms: u64,
    /*
    How long an UNKNOWN verdict is allowed to hold the caller.

    Zero is the send path: a screen it cannot read must not delay a message by
    even one poll. The launch paths pass the blind delay they used to sleep
    unconditionally, so an agent with no signature keeps exactly the behaviour
    it had, while a signed one is released the moment its composer appears.
    */
    pub unknown_hold_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionChatComposerWait {
    Ready,
    /// Nothing could be concluded within `unknown_hold_ms`. Callers proceed.
    Unknown,
    /// The signature stayed absent (or a blocking notice stayed up) until the
    /// deadline.
    NotReady(SessionChatComposerReadiness),
    /// The caller's generation was superseded mid-wait.
    Cancelled,
}

/// Polls the session's screen until its composer appears, the deadline passes,
/// or `cancelled` says the work was superseded.
pub async fn wait_for_session_chat_composer(
    zmx_name: &str,
    agent_id: Option<&str>,
    policy: SessionChatComposerWaitPolicy,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> SessionChatComposerWait {
    /*
    An agent with no signature can only ever answer Unknown, so polling it would
    be a socket read every 150ms to re-learn that. Serve the hold as a plain
    sleep instead — which for the send path (`unknown_hold_ms: 0`) is no wait at
    all, and for the provider-startup paths is exactly the blind delay they used
    to take unconditionally.
    */
    if !has_session_chat_composer_signature(agent_id) {
        tokio::time::sleep(Duration::from_millis(
            policy.settle_ms.saturating_add(policy.unknown_hold_ms),
        ))
        .await;
        return SessionChatComposerWait::Unknown;
    }
    let started = Instant::now();
    tokio::time::sleep(Duration::from_millis(policy.settle_ms)).await;
    let mut last_not_ready: Option<SessionChatComposerReadiness> = None;
    loop {
        if cancelled() {
            return SessionChatComposerWait::Cancelled;
        }
        match crate::session_chat_send::capture_session_terminal_text(zmx_name).await {
            Some(screen) => {
                let readiness = detect_session_chat_composer_ready(agent_id, &screen);
                match readiness.state {
                    SessionChatComposerState::Ready => return SessionChatComposerWait::Ready,
                    SessionChatComposerState::NotReady => last_not_ready = Some(readiness),
                    SessionChatComposerState::Unknown => {
                        if started.elapsed() >= Duration::from_millis(policy.unknown_hold_ms) {
                            return SessionChatComposerWait::Unknown;
                        }
                    }
                }
            }
            None => {
                // Unreadable screen is an Unknown like any other: it must not
                // be mistaken for "the composer is absent".
                if started.elapsed() >= Duration::from_millis(policy.unknown_hold_ms) {
                    return SessionChatComposerWait::Unknown;
                }
            }
        }
        if started.elapsed() >= Duration::from_millis(policy.timeout_ms) {
            return match last_not_ready {
                Some(readiness) => SessionChatComposerWait::NotReady(readiness),
                None => SessionChatComposerWait::Unknown,
            };
        }
        tokio::time::sleep(Duration::from_millis(SESSION_CHAT_COMPOSER_POLL_MS)).await;
    }
}

/// The same wait for a caller that holds ids rather than a resolved zmx name —
/// the provider-startup paths, which used to sleep a flat four seconds here.
/// A session whose zmx name cannot be resolved is Unknown, and still costs the
/// `unknown_hold_ms` the blind sleep used to cost.
pub async fn wait_for_session_chat_composer_by_ids(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    policy: SessionChatComposerWaitPolicy,
) -> SessionChatComposerWait {
    let Some((zmx_name, agent)) =
        resolve_session_chat_composer_target(paths, server_id, project_id, session_id).await
    else {
        tokio::time::sleep(Duration::from_millis(policy.unknown_hold_ms)).await;
        return SessionChatComposerWait::Unknown;
    };
    wait_for_session_chat_composer(&zmx_name, agent.as_deref(), policy, &|| false).await
}

// ---------------------------------------------------------------------------
// /api/readSessionTerminalTail
// ---------------------------------------------------------------------------

/*
CDXC:SessionChat 2026-08-26:
"What is actually in the terminal", for a client that just got `composerNotReady`
back from a send.

It is a separate endpoint rather than a payload on the error because
`DomainStateError` is `{ code: &'static str, message: String }` and nothing
else, built with struct literals at 169 sites across this crate; widening it to
carry evidence would touch every one of them to serve one caller. `rpc_error`
has the same fixed shape on the wire.

It is also not `/api/readSessionText`, which is the whole-history consumer: that
call serializes the entire scrollback (measured at 686 KB on a working session)
and caps it, where this one wants the last thirty lines and the verdict that
goes with them.
*/
pub fn read_session_terminal_tail(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent_id: Option<&str>,
) -> Result<serde_json::Value, crate::domain::DomainStateError> {
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| {
            crate::domain::DomainStateError::not_found(format!(
                "Session {session_id} does not exist."
            ))
        })?;
    let zmx_name = crate::zmx::provider_zmx_session_name(&session)?;
    let agent = agent_id
        .map(str::to_string)
        .or_else(|| session_chat_composer_agent_id(&session));
    // A capture that lost its tail cannot answer either question, so it reports
    // as unreadable rather than as an empty screen.
    let capture = crate::zmx::read_zmx_session_screen_capture(&zmx_name)
        .ok()
        .filter(|capture| !capture.truncated);
    let readiness = match capture.as_ref() {
        Some(capture) => detect_session_chat_composer_readiness(
            agent.as_deref(),
            &capture.text,
            crate::session_chat_notice::classify_session_chat_terminal_notice(
                agent.as_deref(),
                &capture.text,
            )
            .as_ref(),
        ),
        None => SessionChatComposerReadiness::default(),
    };
    let lines = capture
        .as_ref()
        .map(|capture| composer_terminal_preview_tail(&capture.text))
        .unwrap_or_default();
    Ok(serde_json::json!({
        "agentId": agent,
        "captured": capture.is_some(),
        "composerState": readiness.state.as_str(),
        "lines": lines,
        "reason": readiness.reason,
        "sessionId": session_id,
        "projectId": project_id,
    }))
}

async fn resolve_session_chat_composer_target(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> Option<(String, Option<String>)> {
    let paths = paths.clone();
    let server_id = server_id.to_string();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).ok()?;
        let repository = DomainRepository::new(&db, server_id.as_str());
        let session = repository.get_session(&project_id, &session_id).ok()??;
        let zmx_name = crate::zmx::provider_zmx_session_name(&session).ok()?;
        Some((zmx_name, session_chat_composer_agent_id(&session)))
    })
    .await
    .ok()
    .flatten()
}
