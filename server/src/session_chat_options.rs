/*
CDXC:SessionChatDetectedOptions 2026-08-01:
Reads the CURRENT model / reasoning effort from agent-owned structured
transcript metadata and the session's terminal scrollback, plus Claude's
current permission mode from its footer, so the composer's option pills show
evidence instead of a catalog guess.

The agent TUIs render their state into a statusline (Claude Code) or a footer
(Codex). `zmx history` already returns that text (the live screen is part of
the history output), so detection is one bounded process spawn — no new
protocol, no agent cooperation.

Matching is SEGMENT-EXACT and case-sensitive: each scanned line is split on the
statusline delimiters (`|` for Claude's custom statusline, `·` for Codex's
footer), every segment is trimmed, and a segment only counts when the WHOLE
segment matches the grammar. Prose can therefore never false-match (an
assistant sentence mentioning "high" is one long segment), and the Codex
session title — which is the footer's own first `·` segment — is excluded by
the grammar itself.

Terminal evidence wins per option because it can reflect an idle `/model`
change before the next response. The latest Claude assistant / Codex
turn-context record fills any missing value. Nothing matched ⇒ `None` ⇒ the
field is omitted from results/frames. There is deliberately no guessing.
*/

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};

use serde_json::{json, Map, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::DomainRepository;
use crate::events::GxserverEventHub;
use crate::paths::GxserverPaths;
use crate::server::{read_runtime_text, session_observer_key, AppState, SessionChatFollowerEntry};
use crate::session_chat_follower::{is_session_chat_followable_session, session_chat_hook_working};
use crate::storage::open_gxserver_database;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tail window scanned for a statusline/footer. The real dumps put the signal
/// within the last ~6 lines; 15 leaves headroom for an on-screen picker.
pub const SESSION_CHAT_OPTION_SCAN_LINES: usize = 15;

/// Bounded transcript tail used to find the latest structured model record.
/// Two maximum-sized chat records still leave room for the preceding
/// assistant/turn-context metadata row.
const SESSION_CHAT_OPTION_TRANSCRIPT_SCAN_BYTES: u64 = 6 * 1024 * 1024;

/// Detection spawns a process, so every trigger goes through a short cache.
pub const SESSION_CHAT_OPTION_CACHE_TTL: Duration = Duration::from_secs(5);

/// Re-detect schedule after a client dispatches `/model`, `/effort` or `/fast`:
/// the TUI needs a repaint, and Codex needs the user to finish its overlay.
pub const SESSION_CHAT_OPTION_REDETECT_DELAYS_MS: [u64; 2] = [2_000, 6_000];

/// Follower reconciles (1s each) between periodic re-detects.
pub const SESSION_CHAT_OPTION_RECONCILE_INTERVAL_TICKS: u64 = 30;

/*
CDXC:SessionChatTerminalActivity 2026-08-22:
Faster tiers for the same probe, picked by what the LAST one found. A capture is
a direct zmx socket read, so these are priced, not chosen for feel:

  - a live activity ⇒ 1s. Claude replaces its current `⏺` line in place, so
    this is the cadence at which chat can preserve each visible change. The
    direct zmx socket capture makes the followed-session sample inexpensive.
  - working, nothing found yet ⇒ 1s. The next Claude `⏺` line is exactly what
    this probe is waiting to discover; a 15s activity-discovery tier loses most
    short status lines before the first sample. This applies only while a chat
    client follows a session that the agent reports as working.
  - idle ⇒ the original 30s, unchanged.

A user-typed `/compact` does not wait for any of this: it rides the +2s/+6s
post-dispatch redetect (see `is_session_chat_activity_command_text`).
*/
pub const SESSION_CHAT_ACTIVITY_RECONCILE_INTERVAL_TICKS: u64 = 1;
pub const SESSION_CHAT_WORKING_RECONCILE_INTERVAL_TICKS: u64 = 1;

/// A newly followed agent may paint its model/effort footer just after the
/// chat's seed read. Re-detect on each of the first ten 1s reconciles until
/// both values are present instead of leaving a cached startup miss visible.
pub const SESSION_CHAT_OPTION_STARTUP_RECONCILE_TICKS: u64 = 10;

// ---------------------------------------------------------------------------
// Result types (mirror of packages/shared/session-chat.ts SessionChatDetectedOptions)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatDetectedChoice {
    /// Pill value: the catalog id the client keys its state by.
    pub value: String,
    /// Agent-reported label (`Fable 5`, `gpt-5.6-sol`).
    pub label: String,
    /// Which agent-owned surface confirmed this exact value.
    pub source: SessionChatOptionEvidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatOptionEvidence {
    Terminal,
    Transcript,
}

impl SessionChatOptionEvidence {
    fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Transcript => "transcript",
        }
    }
}

/// A detection with no timestamp: the pure parser's output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChatDetectedSelection {
    pub model: Option<SessionChatDetectedChoice>,
    pub effort: Option<SessionChatDetectedChoice>,
    /// Claude's Shift+Tab permission/input mode, available only on screen.
    pub mode: Option<SessionChatDetectedChoice>,
    /// Codex's trailing `fast` modifier. Informational only.
    pub fast: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatDetectedOptions {
    pub selection: SessionChatDetectedSelection,
    /// ISO-8601 millis; the client compares it against its own dispatch time.
    pub detected_at: String,
}

impl SessionChatDetectedOptions {
    pub fn new(selection: SessionChatDetectedSelection) -> Self {
        Self {
            selection,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    }

    /// True when two detections say the same thing (timestamps ignored), so a
    /// periodic re-detect only emits a frame on a REAL change.
    pub fn same_selection(&self, other: Option<&SessionChatDetectedOptions>) -> bool {
        other.is_some_and(|other| other.selection == self.selection)
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        if let Some(model) = self.selection.model.as_ref() {
            map.insert(
                "model".to_string(),
                json!({
                    "value": model.value,
                    "label": model.label,
                    "source": model.source.as_str(),
                }),
            );
        }
        if let Some(effort) = self.selection.effort.as_ref() {
            map.insert(
                "effort".to_string(),
                json!({
                    "value": effort.value,
                    "label": effort.label,
                    "source": effort.source.as_str(),
                }),
            );
        }
        if let Some(mode) = self.selection.mode.as_ref() {
            map.insert(
                "mode".to_string(),
                json!({
                    "value": mode.value,
                    "label": mode.label,
                    "source": mode.source.as_str(),
                }),
            );
        }
        if let Some(fast) = self.selection.fast {
            map.insert("fast".to_string(), json!(fast));
        }
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        Value::Object(map)
    }
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
One `zmx history` capture, two readings. The model/effort grammar and the
terminal-state classifier (session_chat_notice.rs) both want the same screen, so
they are produced together and cached together — a notice must never cost a
second process spawn.
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChatTerminalDetection {
    pub options: Option<SessionChatDetectedOptions>,
    /*
    CDXC:SessionChatComposerReady 2026-08-26: whether the agent CLI's input box
    is on screen and accepting input. Fifth reading of the same capture, for the
    same reason as the second through fourth: it must never cost a spawn.

    Unlike the others this is not an `Option`: absence of a notice means "no
    notice", but absence of composer evidence is itself a verdict the send path
    has to distinguish from "the composer is missing", so the three-way state
    lives inside the value (`Unknown` by `Default`).
    */
    pub composer: crate::session_chat_composer::SessionChatComposerReadiness,
    pub notice: Option<crate::session_chat_notice::SessionChatTerminalNotice>,
    /*
    CDXC:SessionChatTerminalActivity 2026-08-22: live work the CLI reports on
    screen before transcript JSONL catches up (Claude's current `⏺` line and
    compaction). Third reading of the same capture, for the same reason the
    notice is the second one: it must never cost a spawn.
    */
    pub activity: Option<crate::session_chat_terminal_activity::SessionChatTerminalActivity>,
    /*
    CDXC:SessionChatAgentFleet 2026-08-23: the sub-agents the screen is
    painting. Fourth reading of the same capture, same reason as the second and
    third: it must never cost a spawn.
    */
    pub fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    /// True when a usable (non-truncated) screen backed this detection. It is
    /// the ONLY case where `notice: None` means "the screen is clean" — a failed
    /// or capped capture must never retire a notice.
    pub captured: bool,
    /*
    CDXC:SessionChatScreenProbed 2026-08-22:
    True once a capture was ATTEMPTED for this session, whatever came back.

    Deliberately weaker than `captured`, and for a different consumer. `captured`
    answers "can I trust an absence?" — only a whole screen proves a notice is
    gone. `attempted` answers "has the looking happened?", which is what the chat
    composer needs to stop showing a loading skeleton on its model/effort pills.

    They must not be the same bit: a stopped or sleeping session has no screen to
    capture, so `captured` is false forever, and a skeleton keyed on it would
    shimmer for the life of the session. The attempt still happened, and its
    answer — there is nothing to read — is final until the session runs again.
    */
    pub attempted: bool,
}

/// How a consumer wants its detection served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatOptionsReadMode {
    /// Last known value only — never spawns a process (snapshot/replaced frames).
    Cached,
    /// Re-detect, bypassing the TTL (the follower's periodic probe).
    Refresh,
}

/// Lets the follower engine ask for a detection without owning the cache or the
/// domain repository, mirroring `SessionChatStateReader`.
pub type SessionChatOptionsReader = std::sync::Arc<
    dyn Fn(SessionChatOptionsReadMode) -> SessionChatTerminalDetection + Send + Sync,
>;

// ---------------------------------------------------------------------------
// Agent tables
// ---------------------------------------------------------------------------

/// Agents whose statusline grammar is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatOptionAgent {
    Claude,
    Codex,
    Grok,
    Omp,
    Pi,
}

pub fn session_chat_option_agent(agent: Option<&str>) -> Option<SessionChatOptionAgent> {
    match agent.map(str::trim).unwrap_or_default() {
        "claude" | "openclaude" => Some(SessionChatOptionAgent::Claude),
        "codex" => Some(SessionChatOptionAgent::Codex),
        "grok" => Some(SessionChatOptionAgent::Grok),
        "omp" => Some(SessionChatOptionAgent::Omp),
        "pi" => Some(SessionChatOptionAgent::Pi),
        _ => None,
    }
}

/// Slash commands whose dispatch can change what the statusline reports. Mirrors
/// `sessionChatOptionCommandNames` in packages/core-ui/chat/session-chat-session-options.ts.
pub fn is_session_chat_option_command_text(agent: Option<&str>, text: &str) -> bool {
    if session_chat_option_agent(agent).is_none() {
        return false;
    }
    let Some(first) = text.trim_start().split_whitespace().next() else {
        return false;
    };
    matches!(first, "/model" | "/effort" | "/fast")
}

/*
CDXC:SessionChatTerminalActivity 2026-08-22:
Commands that START long on-screen work. The follower would find a compaction
on its own within a probe tier, but the user who just typed `/compact` is
watching for a response RIGHT NOW, and a transcript that sits silent for ten
seconds before admitting anything is happening reads as a dropped message. This
reuses the post-dispatch redetect (+2s/+6s), which is already the mechanism for
"read the screen back after we typed something at it".

Automatic compaction announces itself to nobody, so it is still discovered by
the working-tier probe; that is the case this cannot help with.
*/
pub fn is_session_chat_activity_command_text(agent: Option<&str>, text: &str) -> bool {
    if session_chat_option_agent(agent) != Some(SessionChatOptionAgent::Claude) {
        return false;
    }
    let Some(first) = text.trim_start().split_whitespace().next() else {
        return false;
    };
    first == "/compact"
}

// ---------------------------------------------------------------------------
// Line/segment preparation
// ---------------------------------------------------------------------------

/// Defensive SGR strip: `zmx history` output is already plain text, but a
/// themed statusline could carry colours.
pub(crate) fn strip_ansi_sgr(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        if chars.peek() != Some(&'[') {
            continue;
        }
        chars.next();
        for inner in chars.by_ref() {
            if inner.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

/// Claude renders its thread title inside a long `─` rule, and Codex renders
/// `─ Worked for … ─`. Skipping those lines keeps titles out of the scan.
fn is_divider_line(line: &str) -> bool {
    let mut run = 0usize;
    for ch in line.chars() {
        if ch == '\u{2500}' {
            run += 1;
            if run >= 8 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/*
Claude Code renders its statusline with NON-BREAKING spaces (U+00A0), verified
on a live session: the segment arrives as `Fable\u{a0}5`. Folding every
whitespace character to a plain space is what makes the grammar match what the
user actually sees, instead of silently detecting only the space-free segments.
*/
pub(crate) fn normalize_spaces(line: &str) -> String {
    line.chars()
        .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
        .collect()
}

/// The last `SESSION_CHAT_OPTION_SCAN_LINES` non-blank lines, oldest first.
fn scan_window(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines().rev() {
        let line = normalize_spaces(&strip_ansi_sgr(raw))
            .trim_end()
            .to_string();
        if line.trim().is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= SESSION_CHAT_OPTION_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

/// Trimmed segments of one statusline, split on `|` and `·`.
fn line_segments(line: &str) -> Vec<String> {
    line.split(|ch| ch == '|' || ch == '\u{00b7}')
        .map(|segment| segment.trim().to_string())
        .collect()
}

/// Belt-and-braces companion to the grammar: on a Codex footer (identified by
/// its `Context N% used` segment) the first segment is the session TITLE, so it
/// is never eligible.
fn skips_first_segment(segments: &[String]) -> bool {
    segments.iter().any(|segment| {
        segment.starts_with("Context ")
            && segment.ends_with("% used")
            && segment.len() > "Context % used".len()
    })
}

// ---------------------------------------------------------------------------
// Claude / OpenClaude grammar
//   Ctx Used: 11.0% | 13.5% | $261.54 | Fable 5 | high
// Model family and effort are independent segments, matched independently.
// ---------------------------------------------------------------------------

/// `(family segment prefix, pill value)` — mirrors CLAUDE_MODELS in
/// packages/core-ui/chat/session-chat-session-options.ts.
const CLAUDE_MODEL_FAMILIES: &[(&str, &str)] = &[
    ("Fable", "fable"),
    ("Opus", "opus"),
    ("Sonnet", "sonnet"),
    ("Haiku", "haiku"),
];

/// Rendered lowercase by the TUI; mirrors CLAUDE_EFFORTS.
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// `` or ` 5` or ` 4.5` — the family's optional version suffix.
fn is_model_version_suffix(rest: &str) -> bool {
    if rest.is_empty() {
        return true;
    }
    let Some(version) = rest.strip_prefix(' ') else {
        return false;
    };
    let (major, minor) = match version.split_once('.') {
        Some((major, minor)) => (major, Some(minor)),
        None => (version, None),
    };
    if major.is_empty() || !major.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    match minor {
        None => true,
        Some(minor) => !minor.is_empty() && minor.chars().all(|ch| ch.is_ascii_digit()),
    }
}

fn match_claude_model(segment: &str) -> Option<SessionChatDetectedChoice> {
    CLAUDE_MODEL_FAMILIES
        .iter()
        .find(|(family, _)| {
            segment
                .strip_prefix(*family)
                .is_some_and(is_model_version_suffix)
        })
        .map(|(_, value)| SessionChatDetectedChoice {
            value: (*value).to_string(),
            label: segment.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        })
}

fn match_claude_effort(segment: &str) -> Option<SessionChatDetectedChoice> {
    CLAUDE_EFFORTS
        .contains(&segment)
        .then(|| SessionChatDetectedChoice {
            value: segment.to_string(),
            label: segment.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        })
}

/*
Claude's bottom row is outside the custom statusline:

    ⏵⏵ bypass permissions on (shift+tab to cycle)
    ⏸ plan mode on (shift+tab to cycle)

The leading glyph pair and the complete trailing grammar are required. This
keeps ordinary prose containing "plan mode" or "manual mode" from becoming
agent-owned state merely because it appears near the bottom of the terminal.
*/
fn match_claude_mode(segment: &str) -> Option<SessionChatDetectedChoice> {
    let status = segment
        .strip_prefix("⏵⏵ ")
        .or_else(|| segment.strip_prefix("⏸ "))?;
    let status = status
        .strip_suffix(" (shift+tab to cycle)")
        .unwrap_or(status);
    let (value, label) = match status {
        "auto mode on" => ("auto", "Auto"),
        "bypass permissions on" => ("bypass", "Bypass permissions"),
        "plan mode on" => ("plan", "Plan"),
        "accept edits on" => ("accept-edits", "Accept edits"),
        "manual mode on" => ("manual", "Manual"),
        _ => return None,
    };
    Some(SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Terminal,
    })
}

// ---------------------------------------------------------------------------
// Codex grammar
//   <Title> · gpt-5.6-sol high fast · 225K used · … · Context 26% used · …
// Model + effort (+ the `fast` modifier) live in ONE segment.
// ---------------------------------------------------------------------------

/// Mirrors CODEX_EFFORTS; `max` is Claude-only.
const CODEX_EFFORTS: &[&str] = &["minimal", "low", "medium", "high", "xhigh"];

/// `gpt-` + a digit + id characters, lowercase and case-sensitive so an
/// uppercase "GPT-5.6" in prose or a title cannot match.
fn is_codex_model_id(token: &str) -> bool {
    let Some(rest) = token.strip_prefix("gpt-") else {
        return false;
    };
    rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        && rest
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
}

fn match_codex_segment(segment: &str) -> Option<SessionChatDetectedSelection> {
    let mut tokens = segment.split(' ');
    let model = tokens.next().filter(|token| is_codex_model_id(token))?;
    let mut selection = SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: model.to_string(),
            label: model.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        ..SessionChatDetectedSelection::default()
    };
    let mut next = tokens.next();
    if let Some(effort) = next.filter(|token| CODEX_EFFORTS.contains(token)) {
        selection.effort = Some(SessionChatDetectedChoice {
            value: effort.to_string(),
            label: effort.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        });
        next = tokens.next();
    }
    if next == Some("fast") {
        selection.fast = Some(true);
        next = tokens.next();
    }
    // Anything left over means this was prose that merely started with an id.
    next.is_none().then_some(selection)
}

// ---------------------------------------------------------------------------
// Grok grammar
//   ╰──────────────────────── Grok 4.6 (medium) · always-approve ─╯
// Model and effort share ONE segment, and that segment is drawn INSIDE the
// bottom border of the composer box — so the rule has to come off the line
// before it can be read at all, and before `is_divider_line` would skip it.
// ---------------------------------------------------------------------------

/// The values grok's model catalog offers (`reasoning_efforts` in
/// `~/.grok/models_cache.json`); mirrors GROK_EFFORTS on the client.
const GROK_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh"];

/// Box-drawing runs are chrome, not content: fold them to spaces so the
/// statusline drawn on a border reads like any other line.
fn strip_box_drawing(line: &str) -> String {
    line.chars()
        .map(|ch| {
            if matches!(ch, '\u{2500}'..='\u{257f}') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

/// `Grok 4.6 (medium)`, or `Grok 4.6` on a model with no reasoning effort.
/// Anything else in the parentheses means this was not the statusline.
fn match_grok_segment(segment: &str) -> Option<SessionChatDetectedSelection> {
    let (name, effort) = match segment.split_once('(') {
        None => (segment.trim(), None),
        Some((name, rest)) => (name.trim(), Some(rest.strip_suffix(')')?.trim())),
    };
    if !name
        .strip_prefix("Grok")
        .is_some_and(is_model_version_suffix)
    {
        return None;
    }
    let effort = match effort {
        None => None,
        Some(effort) => {
            Some(
                GROK_EFFORTS
                    .contains(&effort)
                    .then(|| SessionChatDetectedChoice {
                        value: effort.to_string(),
                        label: effort.to_string(),
                        source: SessionChatOptionEvidence::Terminal,
                    })?,
            )
        }
    };
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            // The catalog id for the displayed name (`Grok 4.6` ⇒ `grok-4.6`),
            // which is what grok's own `models_cache.json` keys models by.
            value: name.to_ascii_lowercase().replace(' ', "-"),
            label: name.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort,
        mode: None,
        fast: None,
    })
}

// ---------------------------------------------------------------------------
// Pi grammar
//   0.0%/300k (auto)              claude-fable-5@300k • medium
//
// Pi's statusline is configurable, so require both pieces from the measured
// default layout: a context meter and the model/effort suffix. This keeps a
// prose line that happens to mention `model • medium` from becoming state.
// ---------------------------------------------------------------------------

const PI_FAMILY_EFFORTS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh"];
const OMP_MIN_HEADER_RULE_CHARS: usize = 20;

fn is_pi_context_meter(token: &str) -> bool {
    let Some((used, total)) = token.split_once('/') else {
        return false;
    };
    let Some(used) = used.strip_suffix('%') else {
        return false;
    };
    if used.parse::<f64>().is_err() {
        return false;
    }
    let total = total.strip_suffix(['k', 'K', 'm', 'M']).unwrap_or(total);
    !total.is_empty() && total.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

fn is_pi_family_model_id(token: &str) -> bool {
    !token.is_empty()
        && token.chars().any(|ch| ch.is_ascii_alphanumeric())
        && token.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/' | '@' | '+')
        })
}

fn match_pi_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let bullet = tokens.iter().rposition(|token| *token == "•")?;
    if bullet < 2 || bullet + 2 != tokens.len() {
        return None;
    }
    if !tokens[..bullet - 1]
        .iter()
        .any(|token| is_pi_context_meter(token))
    {
        return None;
    }
    let model = tokens[bullet - 1];
    let effort = tokens[bullet + 1].to_ascii_lowercase();
    if !is_pi_family_model_id(model) || !PI_FAMILY_EFFORTS.contains(&effort.as_str()) {
        return None;
    }
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: model.to_string(),
            label: model.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort: Some(SessionChatDetectedChoice {
            value: effort.clone(),
            label: effort,
            source: SessionChatOptionEvidence::Terminal,
        }),
        mode: None,
        fast: None,
    })
}

// ---------------------------------------------------------------------------
// Omp grammar
//   ╭── π > ⬢ GPT-5.6-Sol · ◒ high > … ▶────────────────────────╮
//
// Both values live on the rounded composer head. Require that complete chrome
// plus Omp's two glyph labels so ordinary terminal output cannot match.
// ---------------------------------------------------------------------------

fn match_omp_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let trimmed = line.trim();
    if !trimmed.starts_with('\u{256d}')
        || !trimmed.ends_with('\u{256e}')
        || trimmed.chars().filter(|ch| *ch == '\u{2500}').count() < OMP_MIN_HEADER_RULE_CHARS
    {
        return None;
    }
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let model_marker = tokens.iter().position(|token| *token == "⬢")?;
    let effort_marker = tokens.iter().position(|token| *token == "◒")?;
    if model_marker >= effort_marker || !tokens[..model_marker].contains(&"π") {
        return None;
    }
    let model = *tokens.get(model_marker + 1)?;
    let effort = tokens.get(effort_marker + 1)?.to_ascii_lowercase();
    if !is_pi_family_model_id(model) || !PI_FAMILY_EFFORTS.contains(&effort.as_str()) {
        return None;
    }
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: model.to_string(),
            label: model.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort: Some(SessionChatDetectedChoice {
            value: effort.clone(),
            label: effort,
            source: SessionChatOptionEvidence::Terminal,
        }),
        mode: None,
        fast: None,
    })
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Scans the tail window bottom-up; the bottom-most match wins. Returns `None`
/// when the window carries no statusline this parser understands.
pub fn detect_session_chat_selection(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatDetectedSelection> {
    let mut found = SessionChatDetectedSelection::default();
    for scanned in scan_window(text).iter().rev() {
        match agent {
            SessionChatOptionAgent::Pi => {
                if let Some(selection) = match_pi_statusline(scanned) {
                    return Some(selection);
                }
                continue;
            }
            SessionChatOptionAgent::Omp => {
                if let Some(selection) = match_omp_statusline(scanned) {
                    return Some(selection);
                }
                continue;
            }
            _ => {}
        }
        // Grok draws its statusline on the composer box's bottom border.
        let unboxed;
        let line = if agent == SessionChatOptionAgent::Grok {
            unboxed = strip_box_drawing(scanned);
            &unboxed
        } else {
            scanned
        };
        if is_divider_line(line) {
            continue;
        }
        let segments = line_segments(line);
        let first_eligible = usize::from(skips_first_segment(&segments));
        for segment in segments.iter().skip(first_eligible) {
            match agent {
                SessionChatOptionAgent::Claude => {
                    if found.model.is_none() {
                        found.model = match_claude_model(segment);
                    }
                    if found.effort.is_none() {
                        found.effort = match_claude_effort(segment);
                    }
                    if found.mode.is_none() {
                        found.mode = match_claude_mode(segment);
                    }
                }
                SessionChatOptionAgent::Codex => {
                    if found.model.is_none() && found.effort.is_none() {
                        if let Some(selection) = match_codex_segment(segment) {
                            found = selection;
                        }
                    }
                }
                SessionChatOptionAgent::Grok => {
                    if found.model.is_none() && found.effort.is_none() {
                        if let Some(selection) = match_grok_segment(segment) {
                            found = selection;
                        }
                    }
                }
                SessionChatOptionAgent::Omp => {
                    unreachable!("Omp is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Pi => unreachable!("Pi is parsed as a complete statusline"),
            }
        }
        if found.model.is_some() && found.effort.is_some() {
            break;
        }
    }
    (found.model.is_some() || found.effort.is_some() || found.mode.is_some()).then_some(found)
}

fn transcript_tail_text(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(SESSION_CHAT_OPTION_TRANSCRIPT_SCAN_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut bytes)?;
    if start > 0 {
        if let Some(first_newline) = bytes.iter().position(|byte| *byte == b'\n') {
            bytes.drain(..=first_newline);
        } else {
            bytes.clear();
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn transcript_text(value: Option<&Value>) -> Option<&str> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn claude_transcript_model_choice(model: &str) -> Option<SessionChatDetectedChoice> {
    let normalized = model.trim().to_ascii_lowercase();
    let tokens: Vec<&str> = normalized.split('-').collect();
    let (family_index, family) =
        tokens
            .iter()
            .enumerate()
            .find_map(|(index, token)| match *token {
                "fable" | "opus" | "sonnet" | "haiku" => Some((index, *token)),
                _ => None,
            })?;
    let title = match family {
        "fable" => "Fable",
        "opus" => "Opus",
        "sonnet" => "Sonnet",
        "haiku" => "Haiku",
        _ => return None,
    };
    let following_version: Vec<&str> = tokens
        .iter()
        .skip(family_index + 1)
        .copied()
        .take_while(|token| {
            token.len() <= 2 && !token.is_empty() && token.chars().all(|ch| ch.is_ascii_digit())
        })
        .take(2)
        .collect();
    let preceding_version: Vec<&str> = tokens
        .iter()
        .take(family_index)
        .rev()
        .copied()
        .take_while(|token| {
            token.len() <= 2 && !token.is_empty() && token.chars().all(|ch| ch.is_ascii_digit())
        })
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let version = if following_version.is_empty() {
        preceding_version
    } else {
        following_version
    };
    let label = if version.is_empty() {
        title.to_string()
    } else {
        format!("{title} {}", version.join("."))
    };
    Some(SessionChatDetectedChoice {
        value: family.to_string(),
        label,
        source: SessionChatOptionEvidence::Transcript,
    })
}

fn transcript_effort_choice(effort: &str) -> Option<SessionChatDetectedChoice> {
    let normalized = effort.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
    )
    .then(|| SessionChatDetectedChoice {
        value: normalized.clone(),
        label: normalized,
        source: SessionChatOptionEvidence::Transcript,
    })
}

fn detect_session_chat_transcript_selection(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatDetectedSelection> {
    for line in text.lines().rev() {
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let selection = match agent {
            SessionChatOptionAgent::Claude
                if transcript_text(record.get("type")) == Some("assistant")
                    && record.get("isSidechain") != Some(&Value::Bool(true)) =>
            {
                let message = record.get("message").and_then(Value::as_object);
                SessionChatDetectedSelection {
                    model: message
                        .and_then(|message| transcript_text(message.get("model")))
                        .and_then(claude_transcript_model_choice),
                    effort: transcript_text(record.get("effort"))
                        .and_then(transcript_effort_choice),
                    mode: None,
                    fast: None,
                }
            }
            SessionChatOptionAgent::Codex
                if transcript_text(record.get("type")) == Some("turn_context") =>
            {
                let payload = record.get("payload").and_then(Value::as_object);
                SessionChatDetectedSelection {
                    model: payload
                        .and_then(|payload| transcript_text(payload.get("model")))
                        .map(|model| SessionChatDetectedChoice {
                            value: model.to_string(),
                            label: model.to_string(),
                            source: SessionChatOptionEvidence::Transcript,
                        }),
                    effort: payload
                        .and_then(|payload| {
                            transcript_text(payload.get("effort"))
                                .or_else(|| transcript_text(payload.get("reasoning_effort")))
                        })
                        .and_then(transcript_effort_choice),
                    mode: None,
                    fast: None,
                }
            }
            _ => continue,
        };
        if selection.model.is_some() || selection.effort.is_some() || selection.mode.is_some() {
            return Some(selection);
        }
    }
    None
}

fn read_session_chat_transcript_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent: SessionChatOptionAgent,
) -> Option<SessionChatDetectedSelection> {
    let session = repository.get_session(project_id, session_id).ok()??;
    let runtime = session.get("runtimeSettings").and_then(Value::as_object);
    let agent_session_id =
        runtime.and_then(|runtime| transcript_text(runtime.get("agentSessionId")));
    let agent_session_path =
        runtime.and_then(|runtime| transcript_text(runtime.get("agentSessionPath")));
    let transcript_agent =
        crate::session_chat::resolve_session_chat_transcript_agent(match agent {
            SessionChatOptionAgent::Claude => Some("claude"),
            SessionChatOptionAgent::Codex => Some("codex"),
            /*
            Grok's statusline is on screen for the whole session and names both
            values, and its update-stream rows carry no effort at all, so there
            is nothing a transcript read could add here.
            */
            SessionChatOptionAgent::Grok => return None,
            SessionChatOptionAgent::Omp => return None,
            SessionChatOptionAgent::Pi => return None,
        })?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        agent_session_id,
        agent_session_path,
    )?;
    let text = transcript_tail_text(&path).ok()?;
    detect_session_chat_transcript_selection(agent, &text)
}

fn merge_session_chat_option_selections(
    transcript: Option<SessionChatDetectedSelection>,
    terminal: Option<SessionChatDetectedSelection>,
) -> Option<SessionChatDetectedSelection> {
    let mut merged = transcript.unwrap_or_default();
    if let Some(terminal) = terminal {
        if terminal.model.is_some() {
            merged.model = terminal.model;
        }
        if terminal.effort.is_some() {
            merged.effort = terminal.effort;
        }
        if terminal.mode.is_some() {
            merged.mode = terminal.mode;
        }
        if terminal.fast.is_some() {
            merged.fast = terminal.fast;
        }
    }
    (merged.model.is_some() || merged.effort.is_some() || merged.mode.is_some()).then_some(merged)
}

/// Full detection for one session: resolve structured transcript metadata,
/// then let any current terminal statusline value win per option. `None` means
/// neither agent-owned source proved a value.
///
/// CDXC:SessionChatTerminalNotices 2026-08-19: the same capture is classified
/// for terminal-state notices, so both readings ride one process spawn.
pub fn detect_session_chat_terminal_state(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent_id: Option<&str>,
) -> SessionChatTerminalDetection {
    /*
    CDXC:SessionChatComposerReady 2026-08-26:
    Two independent reasons to spend a capture on this session now. The
    statusline grammar covers three agents; the composer signature table covers
    nine, so an agent with only the latter (cursor, copilot, opencode, gemini,
    omp) reaches the funnel through this second door and gets every reading the
    capture can support — which for it is composer readiness alone, since the
    notice, activity and fleet classifiers are all keyed on the option agent.
    */
    let agent = session_chat_option_agent(agent_id);
    if agent.is_none()
        && !crate::session_chat_composer::has_session_chat_composer_signature(agent_id)
    {
        return SessionChatTerminalDetection::default();
    }
    let transcript = agent.and_then(|agent| {
        read_session_chat_transcript_selection(repository, project_id, session_id, agent)
    });
    let capture =
        crate::zmx::read_zmx_session_history_capture(repository, project_id, session_id).ok();
    let terminal = agent
        .zip(capture.as_ref())
        .and_then(|(agent, capture)| detect_session_chat_selection(agent, &capture.text));
    // A capped capture lost its tail, so the live screen is not in it.
    let screen = capture.as_ref().filter(|capture| !capture.truncated);
    let notice = screen.and_then(|capture| {
        crate::session_chat_notice::classify_session_chat_terminal_notice(agent_id, &capture.text)
    });
    SessionChatTerminalDetection {
        options: merge_session_chat_option_selections(transcript, terminal)
            .map(SessionChatDetectedOptions::new),
        composer: match screen {
            Some(capture) => crate::session_chat_composer::detect_session_chat_composer_readiness(
                agent_id,
                &capture.text,
                notice.as_ref(),
            ),
            None => crate::session_chat_composer::SessionChatComposerReadiness::default(),
        },
        notice,
        activity: screen.and_then(|capture| {
            crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                agent_id,
                &capture.text,
            )
        }),
        fleet: screen.and_then(|capture| {
            crate::session_chat_agent_fleet::detect_session_chat_agent_fleet(
                agent_id,
                &capture.text,
            )
        }),
        captured: screen.is_some(),
        // We got past the agent check, so a capture was tried. Whether it came
        // back is `captured`'s business, not this field's.
        attempted: true,
    }
}

// ---------------------------------------------------------------------------
// Inline tests — fixtures are VERBATIM `ghostex read-text <id> --lines 15`
// dumps captured from live sessions on 2026-08-01.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// G1ipk — claude, this user's custom statusline, with prose and sub-agent
    /// rows around it that must never match.
    const CLAUDE_CUSTOM_STATUSLINE: &str = concat!(
        "  \u{25fc} Show actual current model+effort in chat pills via zmx scrollback detection\n",
        "  \u{25fc} RN: merge the two top-right more-options menus; rename attach option\n",
        "  \u{25fb} Rebuild, restart, E2E verify, Fable verifier, commit\n",
        "                                current: 2.1.220 \u{b7} latest: 2.1.220 \u{2718} Auto-update failed \u{b7} Run claude doctor\n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} multi-agent-terminal-architecture \u{2500}\u{2500}\n",
        "\u{276f} \n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
        "  Ctx Used: 11.0% | 13.5% | $261.54 | Fable 5 | high\n",
        "  fb7572ef-2965-4e5e-b21e-bb0e3c455b66 | .../Ghostex | xyzt71@gmail.com\n",
        "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
        "\n",
        "  \u{23fa} main\n",
        "  \u{25ef} general-purpose  Diagnose stale chat identity          11m 33s \u{b7} \u{2193} 58.7k tokens\n",
        "  \u{25ef} general-purpose  Design scrollback model detection     11m 15s \u{b7} \u{2193} 58.9k tokens\n",
        "\u{276f} \u{25ef} general-purpose  RN merge header menus              11m 1s \u{b7} \u{2193} 38.1k tokens\n",
    );

    /// G1htq — claude, effort `medium`.
    const CLAUDE_MEDIUM: &str = concat!(
        "                                                                   66936 tokens\n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} sync-local-to-remote-main \u{2500}\u{2500}\n",
        "\u{276f} \n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
        "  Ctx Used: 7.0% | 8.3% | $2.42 | Fable 5 | medium\n",
        "  b6672e82-b770-411b-b7b7-17b0449ad9c5 | .../Ghostex | xyzt71@gmail.com\n",
        "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
    );

    /// G6l3p — claude, with assistant prose above the statusline.
    const CLAUDE_WITH_PROSE: &str = concat!(
        "  The high-effort path is what Opus would pick here, but gpt-5.6 also works.\n",
        "\u{273b} Brewed for 12m 23s\n",
        "                                        new task? /clear to save 120.7k tokens\n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} bg-image-file-picker \u{2500}\u{2500}\n",
        "\u{276f} \n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
        "  Ctx Used: 12.0% | 15.1% | $10.12 | Fable 5 | high\n",
        "  1636419d-1d52-42b4-a546-c4db8fdfcfed | .../Ghostex | xyzt71@gmail.com\n",
        "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
    );

    /// G2a9p — codex, the primary codex footer sample.
    const CODEX_FOOTER: &str = concat!(
        "  - Final verification passed: signed app bundle, versions, APK checksum, release notes.\n",
        "  - Windows packages remain unsigned beta builds and may display SmartScreen warnings.\n",
        "\n",
        "  Summary: Ghostex 6.13.0 is live and verified across GitHub, Sparkle, Homebrew, and gxserver.\n",
        "\n",
        "\u{2500} Worked for 1h 47m 48s \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
        "\n",
        "\n",
        "\u{203a} Summarize recent commits\n",
        "\n",
        "  GPUI MacOS Release \u{b7} gpt-5.6-sol high \u{b7} 19.8M used \u{b7} Ghostex \u{b7} codex/fix-agent-skill-settings-controls \u{b7} Context 29% used \u{b7} weekly 99% left\n",
    );

    /// G8q7x — codex with the trailing `fast` modifier.
    const CODEX_FAST: &str = concat!(
        "\u{203a} Explain this codebase\n",
        "\n",
        "  APK Release Server \u{b7} gpt-5.6-sol high fast \u{b7} 225K used \u{b7} Ghostex \u{b7} main \u{b7} Context 26% used \u{b7} weekly 99% left\n",
    );

    /// G5w59 — codex, effort `xhigh`.
    const CODEX_XHIGH: &str = concat!(
        "\u{203a}  xxxxhjg tersseeeegrssss\n",
        "\n",
        "  Cloud Code Cursor Bug \u{b7} gpt-5.6-sol xhigh \u{b7} 746K used \u{b7} Ghostex \u{b7} codex/fix-agent-skill-settings-controls \u{b7} Context 54% used \u{b7} weekly 96% left\n",
    );

    /// G83ih — codex in a narrow pane: the footer is width-clipped.
    const CODEX_CLIPPED: &str = concat!(
        "\u{203a} Run /review on my current changes\n",
        "\n",
        "  Command Pane Border Fix \u{b7} gpt-5.6-sol high \u{b7} 484K used \u{b7} G\u{2026}\n",
    );

    fn claude(text: &str) -> Option<SessionChatDetectedSelection> {
        detect_session_chat_selection(SessionChatOptionAgent::Claude, text)
    }

    fn codex(text: &str) -> Option<SessionChatDetectedSelection> {
        detect_session_chat_selection(SessionChatOptionAgent::Codex, text)
    }

    fn pair(selection: &SessionChatDetectedSelection) -> (Option<&str>, Option<&str>) {
        (
            selection.model.as_ref().map(|choice| choice.value.as_str()),
            selection
                .effort
                .as_ref()
                .map(|choice| choice.value.as_str()),
        )
    }

    #[test]
    fn detects_claude_custom_statusline_model_and_effort() {
        let selection = claude(CLAUDE_CUSTOM_STATUSLINE).expect("claude statusline detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("high")));
        // The RAW rendered text is preserved so the pill can show the real
        // version instead of the catalog's.
        assert_eq!(selection.model.as_ref().unwrap().label, "Fable 5");
        assert_eq!(selection.effort.as_ref().unwrap().label, "high");
        assert_eq!(selection.fast, None);
    }

    #[test]
    fn detects_claude_medium_effort() {
        let selection = claude(CLAUDE_MEDIUM).expect("claude statusline detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("medium")));
    }

    #[test]
    fn claude_prose_mentioning_models_never_matches() {
        let selection = claude(CLAUDE_WITH_PROSE).expect("claude statusline detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("high")));
    }

    #[test]
    fn claude_thread_title_divider_is_skipped() {
        // A title that IS a model family name still cannot win: the line is a
        // `─` rule, and rules are skipped before segmenting.
        let text = concat!(
            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} Opus 4.8 \u{2500}\u{2500}\n",
            "\u{276f} \n",
        );
        assert_eq!(claude(text), None);
    }

    #[test]
    fn claude_without_a_statusline_detects_nothing() {
        let text = concat!(
            "\u{276f} \n",
            "  Ready. Ask me anything about high availability or sonnet forms.\n",
        );
        assert_eq!(claude(text), None);
    }

    #[test]
    fn claude_version_variants_map_to_the_family_value() {
        for (segment, value) in [
            ("Opus 4.5", "opus"),
            ("Opus", "opus"),
            ("Sonnet 5", "sonnet"),
            ("Haiku", "haiku"),
        ] {
            let text = format!("  Ctx Used: 1.0% | 2.0% | $1.00 | {segment} | max\n");
            let selection = claude(&text).expect("statusline detected");
            assert_eq!(pair(&selection), (Some(value), Some("max")));
            assert_eq!(selection.model.as_ref().unwrap().label, segment);
        }
    }

    #[test]
    fn claude_rejects_lookalike_segments() {
        let text = concat!(
            "  Ctx Used: 1.0% | Opusculum | opus | Fable five | HIGH | Sonnet 5x\n",
            "\u{276f} \n",
        );
        assert_eq!(claude(text), None);
    }

    #[test]
    fn detects_codex_footer_model_and_effort() {
        let selection = codex(CODEX_FOOTER).expect("codex footer detected");
        assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
        assert_eq!(selection.fast, None);
    }

    #[test]
    fn detects_codex_fast_modifier() {
        let selection = codex(CODEX_FAST).expect("codex footer detected");
        assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
        assert_eq!(selection.fast, Some(true));
    }

    #[test]
    fn detects_codex_xhigh_effort() {
        let selection = codex(CODEX_XHIGH).expect("codex footer detected");
        assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("xhigh")));
    }

    #[test]
    fn detects_codex_footer_clipped_after_the_model_segment() {
        let selection = codex(CODEX_CLIPPED).expect("codex footer detected");
        assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
    }

    #[test]
    fn codex_model_id_outside_the_catalog_is_reported_verbatim() {
        let text = "  Some Title \u{b7} gpt-9.1-nova medium \u{b7} 1K used \u{b7} Context 3% used \u{b7} weekly 99% left\n";
        let selection = codex(text).expect("codex footer detected");
        assert_eq!(pair(&selection), (Some("gpt-9.1-nova"), Some("medium")));
        assert_eq!(selection.model.as_ref().unwrap().label, "gpt-9.1-nova");
    }

    #[test]
    fn codex_session_title_is_never_read_as_a_model() {
        // A title that literally spells a model+effort is still segment 0 of a
        // footer line, which the `Context N% used` guard makes ineligible.
        let text = "  gpt-5.5 low \u{b7} gpt-5.6-sol high \u{b7} 19.8M used \u{b7} Ghostex \u{b7} main \u{b7} Context 29% used \u{b7} weekly 99% left\n";
        let selection = codex(text).expect("codex footer detected");
        assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
    }

    #[test]
    fn codex_prose_mentioning_a_model_never_matches() {
        let text = concat!(
            "  I switched the worker to gpt-5.6-sol high because it is faster.\n",
            "\u{203a} \n",
        );
        assert_eq!(codex(text), None);
    }

    #[test]
    fn codex_worked_for_rule_line_is_skipped() {
        let text = concat!(
            "\u{2500} Worked for 2m \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} gpt-5.6-sol high \u{2500}\u{2500}\n",
            "\u{203a} \n",
        );
        assert_eq!(codex(text), None);
    }

    #[test]
    fn bottom_most_statusline_wins() {
        let text = concat!(
            "  Ctx Used: 1.0% | 2.0% | $1.00 | Sonnet 5 | low\n",
            "\u{276f} \n",
            "  Ctx Used: 1.0% | 2.0% | $1.00 | Fable 5 | high\n",
        );
        let selection = claude(text).expect("statusline detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("high")));
    }

    #[test]
    fn only_the_tail_window_is_scanned() {
        let mut text = String::from("  Ctx Used: 1.0% | 2.0% | $1.00 | Fable 5 | high\n");
        for index in 0..SESSION_CHAT_OPTION_SCAN_LINES {
            text.push_str(&format!("  filler line {index}\n"));
        }
        assert_eq!(claude(&text), None);
    }

    /// Captured live from G1ipk on 2026-08-02: Claude Code's statusline is
    /// rendered with NON-BREAKING spaces, which the parser must fold.
    #[test]
    fn claude_statusline_rendered_with_non_breaking_spaces_matches() {
        let text = concat!(
            "  \u{25fb} Rebuild, restart, E2E verify, Fable verifier, commit\n",
            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} multi-agent-terminal-architecture \u{2500}\u{2500}\n",
            "\u{276f}\u{a0}\n",
            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
            "  Ctx\u{a0}Used:\u{a0}13.0%\u{a0}|\u{a0}16.5%\u{a0}|\u{a0}$286.90\u{a0}|\u{a0}Fable\u{a0}5\u{a0}|\u{a0}high\n",
            "  fb7572ef-2965-4e5e-b21e-bb0e3c455b66\u{a0}|\u{a0}.../Ghostex\u{a0}|\u{a0}xyzt71@gmail.com\n",
            "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
        );
        let selection = claude(text).expect("statusline detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("high")));
        assert_eq!(selection.model.as_ref().unwrap().label, "Fable 5");
    }

    #[test]
    fn ansi_colour_codes_are_stripped_before_matching() {
        let text = "  Ctx Used: 1.0% | \u{1b}[32m$1.00\u{1b}[0m | \u{1b}[1mFable 5\u{1b}[0m | \u{1b}[33mhigh\u{1b}[0m\n";
        let selection = claude(text).expect("statusline detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("high")));
    }

    #[test]
    fn agents_without_a_table_detect_nothing() {
        assert_eq!(session_chat_option_agent(Some("cursor")), None);
        assert_eq!(session_chat_option_agent(None), None);
        assert_eq!(
            session_chat_option_agent(Some("openclaude")),
            Some(SessionChatOptionAgent::Claude)
        );
        assert_eq!(
            session_chat_option_agent(Some("grok")),
            Some(SessionChatOptionAgent::Grok)
        );
    }

    #[test]
    fn effort_only_statuslines_are_reported() {
        let text = "  Ctx Used: 1.0% | 2.0% | $1.00 | high\n";
        let selection = claude(text).expect("statusline detected");
        assert_eq!(pair(&selection), (None, Some("high")));
    }

    #[test]
    fn detects_claude_model_and_effort_from_structured_transcript_rows() {
        let text = concat!(
            "{\"type\":\"assistant\",\"effort\":\"high\",\"message\":{\"model\":\"claude-fable-5\",\"content\":[]}}\n",
            "{\"type\":\"user\",\"message\":{\"content\":\"next\"}}\n",
        );
        let selection =
            detect_session_chat_transcript_selection(SessionChatOptionAgent::Claude, text)
                .expect("claude transcript options detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("high")));
        let model = selection.model.as_ref().unwrap();
        assert_eq!(model.label, "Fable 5");
        assert_eq!(model.source, SessionChatOptionEvidence::Transcript);
    }

    #[test]
    fn ignores_claude_sidechain_models_when_resolving_the_main_session() {
        let text = concat!(
            "{\"type\":\"assistant\",\"isSidechain\":false,\"effort\":\"high\",\"message\":{\"model\":\"claude-fable-5\"}}\n",
            "{\"type\":\"assistant\",\"isSidechain\":true,\"effort\":\"low\",\"message\":{\"model\":\"claude-haiku-4-5\"}}\n",
        );
        let selection =
            detect_session_chat_transcript_selection(SessionChatOptionAgent::Claude, text)
                .expect("main claude transcript options detected");
        assert_eq!(pair(&selection), (Some("fable"), Some("high")));
    }

    #[test]
    fn detects_codex_model_and_effort_from_latest_turn_context() {
        let text = concat!(
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.5\",\"effort\":\"medium\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.6-sol\",\"effort\":\"high\"}}\n",
        );
        let selection =
            detect_session_chat_transcript_selection(SessionChatOptionAgent::Codex, text)
                .expect("codex transcript options detected");
        assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
        assert_eq!(
            selection.model.as_ref().unwrap().source,
            SessionChatOptionEvidence::Transcript
        );
    }

    #[test]
    fn terminal_values_override_transcript_values_per_option() {
        let transcript = SessionChatDetectedSelection {
            model: Some(SessionChatDetectedChoice {
                value: "fable".to_string(),
                label: "Fable 5".to_string(),
                source: SessionChatOptionEvidence::Transcript,
            }),
            effort: Some(SessionChatDetectedChoice {
                value: "high".to_string(),
                label: "high".to_string(),
                source: SessionChatOptionEvidence::Transcript,
            }),
            mode: None,
            fast: None,
        };
        let terminal = claude("Ctx Used: 1% | Opus 4.8").unwrap();
        let merged = merge_session_chat_option_selections(Some(transcript), Some(terminal))
            .expect("merged options");
        assert_eq!(pair(&merged), (Some("opus"), Some("high")));
        assert_eq!(
            merged.model.as_ref().unwrap().source,
            SessionChatOptionEvidence::Terminal
        );
        assert_eq!(
            merged.effort.as_ref().unwrap().source,
            SessionChatOptionEvidence::Transcript
        );
    }

    #[test]
    fn option_command_text_is_recognised_per_agent() {
        assert!(is_session_chat_option_command_text(
            Some("claude"),
            "/model opus"
        ));
        assert!(is_session_chat_option_command_text(Some("claude"), "/fast"));
        assert!(is_session_chat_option_command_text(Some("codex"), "/model"));
        assert!(!is_session_chat_option_command_text(
            Some("claude"),
            "please /model opus"
        ));
        // Grok has a `/model` picker of its own, so typing it still earns the
        // post-dispatch screen re-read.
        assert!(is_session_chat_option_command_text(Some("grok"), "/model"));
    }

    #[test]
    fn detected_options_serialize_to_the_shared_contract_shape() {
        let options = SessionChatDetectedOptions {
            selection: SessionChatDetectedSelection {
                model: Some(SessionChatDetectedChoice {
                    value: "fable".to_string(),
                    label: "Fable 5".to_string(),
                    source: SessionChatOptionEvidence::Transcript,
                }),
                effort: Some(SessionChatDetectedChoice {
                    value: "high".to_string(),
                    label: "high".to_string(),
                    source: SessionChatOptionEvidence::Terminal,
                }),
                mode: None,
                fast: Some(true),
            },
            detected_at: "2026-08-01T12:00:00.000Z".to_string(),
        };
        assert_eq!(
            options.to_value(),
            json!({
                "model": { "value": "fable", "label": "Fable 5", "source": "transcript" },
                "effort": { "value": "high", "label": "high", "source": "terminal" },
                "fast": true,
                "detectedAt": "2026-08-01T12:00:00.000Z",
            })
        );
    }

    #[test]
    fn same_selection_ignores_the_timestamp() {
        let selection = SessionChatDetectedSelection {
            model: Some(SessionChatDetectedChoice {
                value: "fable".to_string(),
                label: "Fable 5".to_string(),
                source: SessionChatOptionEvidence::Transcript,
            }),
            ..SessionChatDetectedSelection::default()
        };
        let first = SessionChatDetectedOptions {
            selection: selection.clone(),
            detected_at: "2026-08-01T12:00:00.000Z".to_string(),
        };
        let second = SessionChatDetectedOptions {
            selection,
            detected_at: "2026-08-01T12:00:05.000Z".to_string(),
        };
        assert!(first.same_selection(Some(&second)));
        assert!(!first.same_selection(None));
    }
}

/*
CDXC:SessionChatDetectedOptions 2026-08-01:
Model/effort detection reads structured transcript metadata plus the session's
zmx scrollback. The latter costs one short-lived process, so the combined read
is NEVER done per frame or per long-poll tick.
Every trigger goes through this 5s-TTL per-session cache: chat reads, the
+2s/+6s probes after a dispatched `/model`//`/effort`//`/fast`, and the
follower's ~30s piggyback. A miss is cached too — a session whose agent prints
no statusline must not re-spawn `zmx history` on every read. Detection is
deliberately absent from resolve_session_chat_read_state's fingerprint: hashing
it would make each 500ms long-poll tick spawn a process.

CDXC:SessionChatTerminalNotices 2026-08-19: the SAME capture is classified for
terminal-state notices (login expired, trust dialog, usage limit, a crashed
CLI), so the cache entry carries both readings and neither costs an extra spawn.
*/
pub(crate) struct SessionChatOptionCacheEntry {
    pub(crate) fetched_at: std::time::Instant,
    pub(crate) value: crate::session_chat_options::SessionChatTerminalDetection,
}

#[derive(Clone)]
pub(crate) struct SessionChatOptionDetector {
    cache: Arc<Mutex<HashMap<String, SessionChatOptionCacheEntry>>>,
    paths: GxserverPaths,
    server_id: String,
}

impl SessionChatOptionDetector {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            cache: state.session_chat_option_cache.clone(),
            paths: state.paths.clone(),
            server_id: state.metadata.server_id.clone(),
        }
    }

    /// Last known value with no process spawn. Used by frames that must stay
    /// free (snapshot/replaced).
    pub(crate) fn cached(
        &self,
        project_id: &str,
        session_id: &str,
    ) -> crate::session_chat_options::SessionChatTerminalDetection {
        let key = session_observer_key(project_id, session_id);
        self.cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).map(|entry| entry.value.clone()))
            .unwrap_or_default()
    }

    /// BLOCKING: refreshes through the TTL (`force` bypasses it).
    pub(crate) fn detect_blocking(
        &self,
        project_id: &str,
        session_id: &str,
        agent: Option<&str>,
        force: bool,
    ) -> crate::session_chat_options::SessionChatTerminalDetection {
        // CDXC:SessionChatComposerReady 2026-08-26: same two-door gate the
        // funnel itself uses, restated here so an agent with only a composer
        // signature is not turned away before the cache is even consulted.
        if crate::session_chat_options::session_chat_option_agent(agent).is_none()
            && !crate::session_chat_composer::has_session_chat_composer_signature(agent)
        {
            return crate::session_chat_options::SessionChatTerminalDetection::default();
        }
        let key = session_observer_key(project_id, session_id);
        if !force {
            if let Ok(cache) = self.cache.lock() {
                if let Some(entry) = cache.get(&key) {
                    if entry.fetched_at.elapsed()
                        < crate::session_chat_options::SESSION_CHAT_OPTION_CACHE_TTL
                    {
                        return entry.value.clone();
                    }
                }
            }
        }
        let mut detected = open_gxserver_database(&self.paths)
            .ok()
            .map(|db| {
                let repository = DomainRepository::new(&db, self.server_id.as_str());
                crate::session_chat_options::detect_session_chat_terminal_state(
                    &repository,
                    project_id,
                    session_id,
                    agent,
                )
            })
            .unwrap_or_default();
        /*
        CDXC:SessionChatTerminalNotices 2026-08-19:
        This is the ONE funnel every fresh capture goes through (the follower's
        probe, a read-triggered detect, the post-dispatch redetect), so it owns
        the two rules a single detection cannot state on its own:

        1. A capture that succeeded WHOLE and classified to nothing proves the
           screen is clean, which retires a watchdog verdict about screen state.
           `deliveryFailed` is exempt inside the store — it describes a lost
           message, not the current screen.
        2. A re-classification that says the same thing as the cached one is the
           SAME notice instance and keeps its `detectedAt`; see
           `SessionChatTerminalNotice::carry_forward_detected_at`.

        Neither publishes anything itself: every consumer already re-reads this
        cache (plus the watchdog store) and emits on change.
        */
        /*
        CDXC:SessionChatComposerReady 2026-08-26: only an agent the NOTICE
        catalog covers can prove a screen clean. A composer-only agent always
        classifies to no notice — there are no rules for it — so retiring on
        that absence would clear a watchdog verdict on evidence that was never
        collected.
        */
        if detected.captured
            && detected.notice.is_none()
            && crate::session_chat_options::session_chat_option_agent(agent).is_some()
        {
            crate::session_chat_notice::retire_session_chat_watchdog_notice_on_clean_screen(
                project_id, session_id,
            );
        }
        if let Ok(mut cache) = self.cache.lock() {
            if let Some(notice) = detected.notice.as_mut() {
                notice.carry_forward_detected_at(
                    cache
                        .get(&key)
                        .and_then(|entry| entry.value.notice.as_ref()),
                );
            }
            /*
            CDXC:SessionChatTerminalActivity 2026-08-22: same instance-not-sample
            rule, and load-bearing here — the client anchors its elapsed clock to
            `detectedAt`, so re-minting it on every probe would peg the timer at
            zero for the whole run.
            */
            if let Some(activity) = detected.activity.as_mut() {
                activity.carry_forward_detected_at(
                    cache
                        .get(&key)
                        .and_then(|entry| entry.value.activity.as_ref()),
                );
            }
            /*
            CDXC:SessionChatAgentFleet 2026-08-23: deliberately NOT carried
            forward, unlike the notice and the activity row above. A fleet's
            `detectedAt` is the anchor its per-row clocks count from, so it has
            to stay paired with the seconds it was read beside; giving a fresh
            reading an older anchor would make every client count that interval
            twice. Holding a fleet still is `same_fleet`'s job.
            */
            cache.insert(
                key,
                SessionChatOptionCacheEntry {
                    fetched_at: std::time::Instant::now(),
                    value: detected.clone(),
                },
            );
        }
        detected
    }

    /// Async handlers must not block the executor on a process spawn.
    pub(crate) async fn detect(
        &self,
        project_id: &str,
        session_id: &str,
        agent: Option<&str>,
        force: bool,
    ) -> crate::session_chat_options::SessionChatTerminalDetection {
        let detector = self.clone();
        let project_id = project_id.to_string();
        let session_id = session_id.to_string();
        let agent = agent.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            detector.detect_blocking(&project_id, &session_id, agent.as_deref(), force)
        })
        .await
        .unwrap_or_default()
    }
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
The notice a session should be showing RIGHT NOW, with no detection of its own:
the last classification the shared 5s cache holds, overridden by a watchdog
notice when one is pending. Every path that must stay spawn-free — the 500ms
long-poll fingerprint, prompt-driven state frames — reads it through here.
*/
/*
CDXC:SessionChatTerminalActivity 2026-08-22:
The whole screen-derived half of a session's state, owned, from the shared 5s
cache and the watchdog store. Every spawn-free publisher reads it through here
so the notice and the progress row are always taken from the SAME cache read
and can never be published one frame apart.
*/
#[derive(Default)]
pub(crate) struct CachedSessionChatScreenState {
    pub(crate) notice: Option<crate::session_chat_notice::SessionChatTerminalNotice>,
    pub(crate) activity: Option<crate::session_chat_terminal_activity::SessionChatTerminalActivity>,
    pub(crate) fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    /// CDXC:SessionChatScreenProbed 2026-08-22: whether the cache entry these
    /// came from was backed by a whole capture at all.
    pub(crate) probed: bool,
}

impl CachedSessionChatScreenState {
    pub(crate) fn borrow(&self) -> crate::session_chat::SessionChatScreenState<'_> {
        crate::session_chat::SessionChatScreenState {
            notice: self.notice.as_ref(),
            activity: self.activity.as_ref(),
            fleet: self.fleet.as_ref(),
            probed: self.probed,
        }
    }
}

pub(crate) fn cached_session_chat_screen_state(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> CachedSessionChatScreenState {
    let (screen_notice, activity, fleet, probed) = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .get(&session_observer_key(project_id, session_id))
                .map(|entry| {
                    (
                        entry.value.notice.clone(),
                        entry.value.activity.clone(),
                        entry.value.fleet.clone(),
                        entry.value.attempted,
                    )
                })
        })
        .unwrap_or_default();
    CachedSessionChatScreenState {
        notice: crate::session_chat_notice::resolve_session_chat_terminal_notice(
            project_id,
            session_id,
            screen_notice,
        ),
        activity,
        fleet,
        probed,
    }
}

/*
CDXC:SessionChatComposerReady 2026-08-26:
Last known composer verdict with no process spawn, for the prompt-queue
scheduler. A tick must never trigger a capture (that is the rule the notice
reader next to this one exists to keep), so a session nobody has probed reads
`Unknown` and the queue proceeds exactly as it did before this feature.
*/
pub(crate) fn cached_session_chat_composer_readiness(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> crate::session_chat_composer::SessionChatComposerReadiness {
    state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .get(&session_observer_key(project_id, session_id))
                .map(|entry| entry.value.composer.clone())
        })
        .unwrap_or_default()
}

pub(crate) fn cached_session_chat_terminal_notice(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<crate::session_chat_notice::SessionChatTerminalNotice> {
    let screen = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .get(&session_observer_key(project_id, session_id))
                .and_then(|entry| entry.value.notice.clone())
        });
    crate::session_chat_notice::resolve_session_chat_terminal_notice(project_id, session_id, screen)
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
The send watchdog owns no frames and no database: it mutates the watchdog notice
store and then calls this, which republishes whatever the session should be
showing now — the cached model/effort pills included, so a notice frame can
never blank them.
*/
pub(crate) fn session_chat_terminal_notice_publisher(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> crate::session_chat_watchdog::SessionChatWatchdogPublisher {
    let followers = state.session_chat_followers.clone();
    let event_hub = state.event_hub.clone();
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let option_cache = state.session_chat_option_cache.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    Arc::new(move || {
        let key = session_observer_key(&project_id, &session_id);
        let (options, screen_notice, activity, fleet, captured) = option_cache
            .lock()
            .ok()
            .and_then(|cache| {
                cache.get(&key).map(|entry| {
                    (
                        entry.value.options.clone(),
                        entry.value.notice.clone(),
                        entry.value.activity.clone(),
                        entry.value.fleet.clone(),
                        entry.value.attempted,
                    )
                })
            })
            .unwrap_or_default();
        let notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
            &project_id,
            &session_id,
            screen_notice,
        );
        emit_session_chat_options_state_frame(
            &followers,
            &event_hub,
            &paths,
            &server_id,
            &project_id,
            &session_id,
            options.as_ref(),
            crate::session_chat::SessionChatScreenState {
                notice: notice.as_ref(),
                activity: activity.as_ref(),
                fleet: fleet.as_ref(),
                probed: captured,
            },
        );
    })
}

/// Fresh lifecycle/working truth for the watchdog's timeout decision. Blocking
/// (SQLite), so the watchdog calls it from a blocking task.
pub(crate) fn session_chat_watchdog_state_reader(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> crate::session_chat_watchdog::SessionChatWatchdogStateReader {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    Arc::new(move || {
        let read = || -> Option<crate::session_chat_watchdog::SessionChatWatchdogLiveState> {
            let db = open_gxserver_database(&paths).ok()?;
            let repository = DomainRepository::new(&db, server_id.as_str());
            let session = repository.get_session(&project_id, &session_id).ok()??;
            Some(crate::session_chat_watchdog::SessionChatWatchdogLiveState {
                running: is_session_chat_followable_session(&session),
                working: session_chat_hook_working(&session),
            })
        };
        read().unwrap_or_default()
    })
}

pub(crate) fn forget_session_chat_options(state: &AppState, project_id: &str, session_id: &str) {
    if let Ok(mut cache) = state.session_chat_option_cache.lock() {
        cache.remove(&session_observer_key(project_id, session_id));
    }
}

/*
CDXC:SessionChatDetectedOptions 2026-08-01:
Post-dispatch confirmation. After the chat surface types `/model`, `/effort` or
`/fast`, the pill shows an unconfirmed value; these two probes read the
statusline back (+2s for the TUI repaint, +6s to catch a Codex overlay the user
had to finish) and push the real value through the live follower stream. A
probe that finds the SAME value emits nothing, and with no follower/no
subscribers nothing is emitted at all — the next readSessionChat still carries
it.
*/
pub(crate) fn schedule_session_chat_option_redetect(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    agent: Option<&str>,
) {
    if crate::session_chat_options::session_chat_option_agent(agent).is_none() {
        return;
    }
    let detector = SessionChatOptionDetector::new(state);
    let followers = state.session_chat_followers.clone();
    let event_hub = state.event_hub.clone();
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    let agent = agent.map(str::to_string);
    tokio::spawn(async move {
        let cached = detector.cached(&project_id, &session_id);
        let mut published = cached.options;
        // CDXC:SessionChatTerminalNotices 2026-08-19: this probe re-reads the
        // screen anyway, so a notice that appeared or cleared with the dispatch
        // rides the same frame instead of waiting for the ~30s follower probe.
        let mut published_notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
            &project_id,
            &session_id,
            cached.notice,
        );
        let mut published_activity = cached.activity;
        let mut published_fleet = cached.fleet;
        for delay_ms in crate::session_chat_options::SESSION_CHAT_OPTION_REDETECT_DELAYS_MS {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let detection = detector
                .detect(&project_id, &session_id, agent.as_deref(), true)
                .await;
            let notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
                &project_id,
                &session_id,
                detection.notice,
            );
            let notice_changed = detection.captured
                && !crate::session_chat_notice::same_session_chat_terminal_notice(
                    notice.as_ref(),
                    published_notice.as_ref(),
                );
            let options_changed = detection
                .options
                .as_ref()
                .is_some_and(|detected| !detected.same_selection(published.as_ref()));
            let activity_changed = detection.captured
                && !crate::session_chat_terminal_activity::same_session_chat_terminal_activity(
                    detection.activity.as_ref(),
                    published_activity.as_ref(),
                );
            let fleet_changed = detection.captured
                && !crate::session_chat_agent_fleet::same_session_chat_agent_fleet(
                    detection.fleet.as_ref(),
                    published_fleet.as_ref(),
                );
            if !options_changed && !notice_changed && !activity_changed && !fleet_changed {
                continue;
            }
            if options_changed {
                published = detection.options;
            }
            if notice_changed {
                published_notice = notice;
            }
            if activity_changed {
                published_activity = detection.activity;
            }
            if fleet_changed {
                published_fleet = detection.fleet;
            }
            emit_session_chat_options_state_frame(
                &followers,
                &event_hub,
                &paths,
                &server_id,
                &project_id,
                &session_id,
                published.as_ref(),
                crate::session_chat::SessionChatScreenState {
                    notice: published_notice.as_ref(),
                    activity: published_activity.as_ref(),
                    fleet: published_fleet.as_ref(),
                    probed: detection.attempted,
                },
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_session_chat_options_state_frame(
    followers: &Arc<Mutex<HashMap<String, SessionChatFollowerEntry>>>,
    event_hub: &GxserverEventHub,
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    detected: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
    screen: crate::session_chat::SessionChatScreenState<'_>,
) {
    let stream = {
        let Ok(followers) = followers.lock() else {
            return;
        };
        let Some(entry) = followers.get(&session_observer_key(project_id, session_id)) else {
            return;
        };
        let follower_active =
            entry.subscribers > 0 && entry.task.as_ref().is_some_and(|task| !task.is_finished());
        if !follower_active {
            return;
        }
        entry.stream.clone()
    };
    let Ok(db) = open_gxserver_database(paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, server_id);
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let prompt = crate::agents::session_chat_prompt_setting(&session)
        .as_deref()
        .and_then(crate::session_chat::parse_stored_session_chat_prompt);
    let working = session_chat_hook_working(&session);
    let status = if working {
        crate::session_chat::SessionChatStatus::Working
    } else {
        crate::session_chat::SessionChatStatus::Ready
    };
    let agent_session_id = read_runtime_text(&session, "agentSessionId");
    let queue =
        crate::session_chat_queue::read_session_chat_queue_snapshot(paths, project_id, session_id);
    // Same seq discipline as the prompt frame: take the epoch and the seq and
    // publish as one step, because the follower task publishes into the SAME
    // counter and can start a new generation in between
    // (CDXC:SessionChatFollowerLiveness 2026-08-24).
    stream.emit_sequenced(
        |seq| {
            let (epoch, _) = stream.current();
            crate::session_chat::build_session_chat_prompt_state_frame(
                project_id,
                session_id,
                epoch,
                seq,
                status,
                prompt.as_ref(),
                agent_session_id.as_deref(),
                GXSERVER_PROTOCOL_VERSION,
                server_id,
                working,
                detected,
                screen,
                Some(&queue),
            )
        },
        |frame| event_hub.broadcast(frame),
    );
}
