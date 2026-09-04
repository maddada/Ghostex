/*
CDXC:AgentScreenDetection 2026-08-01:
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
session title is excluded by the grammar itself.

CDXC:AgentScreenDetection 2026-09-03 WHY:
Codex's footer is a user-ordered list (`tui.status_line`), so NO segment
position carries meaning. An earlier "the first segment is the title" skip
threw away `gpt-5.6-sol high` on every config that lists
`model-with-reasoning` first, leaving the pills empty until the first turn's
transcript record. The whole-segment grammar is the only guard; a title would
have to be exactly `<model id> <effort>` to be mistaken for one.

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

use serde_json::{Map, Value, json};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::DomainRepository;
use crate::events::GxserverEventHub;
use crate::paths::GxserverPaths;
use crate::server::{AppState, SessionChatFollowerEntry, read_runtime_text, session_observer_key};
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
CDXC:AgentScreenDetection 2026-08-22:
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

A `/compact` does not wait for any of this: the follower probes back-to-back as
soon as the transcript records the command, whether it was sent from the chat
composer or typed straight into the terminal (see
`transcript_message_starts_session_chat_activity`).
*/
pub const SESSION_CHAT_ACTIVITY_RECONCILE_INTERVAL_TICKS: u64 = 1;
pub const SESSION_CHAT_WORKING_RECONCILE_INTERVAL_TICKS: u64 = 1;

/// A newly followed agent may paint its model/effort footer just after the
/// chat's seed read. Re-detect on each of the first ten 1s reconciles until
/// both values are present instead of leaving a cached startup miss visible.
pub const SESSION_CHAT_OPTION_STARTUP_RECONCILE_TICKS: u64 = 10;

/*
CDXC:AgentScreenDetection (settled 2026-08-30):
A drawn screen whose statusline has not painted yet must not settle the probe.
Claude draws its composer chrome (and the permission-mode footer this grammar
reads) immediately, but the user's statusline script runs asynchronously, so
the model segment can trail the rest of the screen by seconds — settling on
that first capture flashes a bare "Model" pill right before the value lands.
So a statusline agent's otherwise-settleable capture that names NO model holds
`attempted` false for this long, counted from the first such capture. Chosen to
fit inside the 10×1s startup reconcile window, so both the value arriving and
the grace expiring land on a fast probe rather than the 30s steady tier. After
it, "this screen names no model" is the settled answer — statuslines are
user-configured and may legitimately be absent.
*/
pub const SESSION_CHAT_OPTION_MODEL_SETTLE_GRACE: Duration = Duration::from_secs(6);

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
    /// CDXC:AgentScreenDetection 2026-09-03 WHY: the JSON Claude Code pipes to its
    /// statusLine command, stored by the Ghostex-installed script.
    Statusline,
}

impl SessionChatOptionEvidence {
    fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Transcript => "transcript",
            Self::Statusline => "statusline",
        }
    }
}

/*
CDXC:AgentScreenDetection 2026-09-03 WHY:
Claude's statusLine payload reports how full the context window is. The chat
composer renders it as a usage ring (the t3code meter): percentage when Claude
reports one, tokens over window size when it reports those. Both are optional
in the payload and both are carried, so the client can show whichever exists.
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChatContextUsage {
    /// `context_window.used_percentage`, rounded — Claude reports an integer.
    pub used_percentage: Option<u32>,
    /// `context_window.total_input_tokens`.
    pub used_tokens: Option<u64>,
    /// `context_window.context_window_size`.
    pub window_size: Option<u64>,
}

impl SessionChatContextUsage {
    fn is_empty(&self) -> bool {
        self.used_percentage.is_none() && self.used_tokens.is_none() && self.window_size.is_none()
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        if let Some(used_percentage) = self.used_percentage {
            map.insert("usedPercentage".to_string(), json!(used_percentage));
        }
        if let Some(used_tokens) = self.used_tokens {
            map.insert("usedTokens".to_string(), json!(used_tokens));
        }
        if let Some(window_size) = self.window_size {
            map.insert("windowSize".to_string(), json!(window_size));
        }
        Value::Object(map)
    }
}

/// A detection with no timestamp: the pure parser's output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChatDetectedSelection {
    pub model: Option<SessionChatDetectedChoice>,
    pub effort: Option<SessionChatDetectedChoice>,
    /// Claude's Shift+Tab permission/input mode, available only on screen.
    pub mode: Option<SessionChatDetectedChoice>,
    /// Cursor's model context-window label, for example `272K` or `1M`.
    pub context_window: Option<String>,
    /// The agent's whole footer — every normalized line from the statusline
    /// that supplied this detection down to the bottom of the screen, newline
    /// joined. The chat surface shows it verbatim as the model pill's tooltip.
    pub terminal_status_line: Option<String>,
    /// Cursor or Codex's terminal-reported Fast modifier, or Claude's
    /// statusline-reported fast mode.
    pub fast: Option<bool>,
    /// Claude's statusline-reported context window usage.
    pub context_usage: Option<SessionChatContextUsage>,
    /// The rest of Claude's statusline payload the chat surface can show
    /// (`claude_statusline_status_value`), camelCase and absent-when-absent.
    pub claude_status: Option<Value>,
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
        if let Some(context_window) = self.selection.context_window.as_ref() {
            map.insert("contextWindow".to_string(), json!(context_window));
        }
        if let Some(terminal_status_line) = self.selection.terminal_status_line.as_ref() {
            map.insert(
                "terminalStatusLine".to_string(),
                json!(terminal_status_line),
            );
        }
        if let Some(fast) = self.selection.fast {
            map.insert("fast".to_string(), json!(fast));
        }
        if let Some(context_usage) = self.selection.context_usage.as_ref() {
            map.insert("contextUsage".to_string(), context_usage.to_value());
        }
        if let Some(claude_status) = self.selection.claude_status.as_ref() {
            map.insert("claudeStatus".to_string(), claude_status.clone());
        }
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        Value::Object(map)
    }
}

/*
CDXC:AgentScreenDetection 2026-08-19:
One `zmx history` capture, two readings. The model/effort grammar and the
terminal-state classifier (session_chat_notice.rs) both want the same screen, so
they are produced together and cached together — a notice must never cost a
second process spawn.
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChatTerminalDetection {
    pub options: Option<SessionChatDetectedOptions>,
    /// Cursor's pending AskQuestion panel. Cursor does not persist this tool
    /// call until it has been answered, so it is read from the same live screen
    /// capture as the model, notice, activity, and composer state.
    pub prompt: Option<crate::session_chat::SessionChatInteractivePrompt>,
    /*
    CDXC:SessionChat 2026-08-26: whether the agent CLI's input box
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
    CDXC:AgentScreenDetection 2026-08-22: live work the CLI reports on
    screen before transcript JSONL catches up (Claude's current `⏺` line and
    compaction). Third reading of the same capture, for the same reason the
    notice is the second one: it must never cost a spawn.
    */
    pub activity: Option<crate::session_chat_terminal_activity::SessionChatTerminalActivity>,
    /*
    CDXC:AgentScreenDetection 2026-08-23: the sub-agents the screen is
    painting. Fourth reading of the same capture, same reason as the second and
    third: it must never cost a spawn.
    */
    pub fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    /*
    CDXC:SessionChat 2026-09-03: Claude's task list, read from its
    on-disk task store rather than the screen. It rides in the same detection
    because the detector is the one periodic reader every publisher already
    consults; unlike the screen readings it needs no capture, so a failed
    capture neither clears it nor makes it stale.
    */
    pub tasks: Option<crate::session_chat_agent_tasks::SessionChatAgentTasks>,
    /// True when a usable (non-truncated) screen backed this detection. It is
    /// the ONLY case where `notice: None` means "the screen is clean" — a failed
    /// or capped capture must never retire a notice.
    pub captured: bool,
    /*
    CDXC:AgentScreenDetection 2026-08-22 (settled 2026-08-30):
    True once detection has a SETTLED answer for this session — not merely once
    a capture was tried. A capture of a still-booting CLI comes back as a blank
    or shell screen that no classifier recognizes; saying "probed" then makes
    the model pill drop its loading skeleton for a bare category label seconds
    before the statusline it will name arrives. So the bit is earned by
    evidence: some classifier recognized the agent's chrome (options, notice,
    activity, fleet, or a Ready composer), or the capture itself failed —
    a stopped or sleeping session has no screen to read, and that answer is
    final until it runs again, so it must never sit under a skeleton.

    Deliberately different from `captured`, and for a different consumer.
    `captured` answers "can I trust an absence?" — only a whole screen proves a
    notice is gone, so a failed capture keeps it false. `attempted` answers
    "has looking produced an answer?", which is what the chat composer needs to
    stop showing a loading skeleton on its model/effort pills.
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

/// CDXC:AgentScreenDetection 2026-09-03 WHY: given the current agent session id, true
/// when the stored statusline payload changed since the previous call.
pub type SessionChatOptionsChangeWatch = std::sync::Arc<dyn Fn(Option<&str>) -> bool + Send + Sync>;

/// A watch over the Claude statusline payload file for one session. The first
/// observation only seeds (the subscribe's seed probe already read the file);
/// every later mtime change — including the file first appearing — fires once.
pub(crate) fn claude_statusline_change_watch(
    hook_state_directory: std::path::PathBuf,
) -> SessionChatOptionsChangeWatch {
    let observed: Mutex<Option<Option<std::time::SystemTime>>> = Mutex::new(None);
    Arc::new(move |agent_session_id: Option<&str>| {
        let modified = agent_session_id
            .and_then(|id| {
                crate::agent_hooks::statusline::claude_statusline_payload_path(
                    &hook_state_directory,
                    id,
                )
            })
            .and_then(|path| {
                std::fs::metadata(path)
                    .and_then(|meta| meta.modified())
                    .ok()
            });
        let Ok(mut observed) = observed.lock() else {
            return false;
        };
        let changed = match *observed {
            Some(previous) => modified.is_some() && previous != modified,
            None => false,
        };
        *observed = Some(modified);
        changed
    })
}

// ---------------------------------------------------------------------------
// Agent tables
// ---------------------------------------------------------------------------

/// Agents whose statusline grammar is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatOptionAgent {
    Antigravity,
    Claude,
    Codex,
    Cursor,
    Grok,
    Hermes,
    Omp,
    Pi,
}

pub fn session_chat_option_agent(agent: Option<&str>) -> Option<SessionChatOptionAgent> {
    match agent.map(str::trim).unwrap_or_default() {
        "antigravity" | "antigravity-cli" | "agy" => Some(SessionChatOptionAgent::Antigravity),
        "claude" | "openclaude" => Some(SessionChatOptionAgent::Claude),
        "codex" => Some(SessionChatOptionAgent::Codex),
        "cursor" => Some(SessionChatOptionAgent::Cursor),
        "grok" => Some(SessionChatOptionAgent::Grok),
        "hermes" | "hermes-agent" => Some(SessionChatOptionAgent::Hermes),
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
CDXC:AgentScreenDetection 2026-08-22:
Commands that START long on-screen work. The follower would find a compaction
on its own within a probe tier, but the user who just typed `/compact` is
watching for a response RIGHT NOW, and a transcript that sits silent for ten
seconds before admitting anything is happening reads as a dropped message.

CDXC:AgentScreenDetection 2026-09-02: the fast look is keyed off the
transcript row Claude records for the command, not off the chat send path — a
`/compact` typed straight into the terminal never went through that path and
used to wait for the idle 30s tier. The send path still treats the command as
Ghostex-typed for draft handling; the screen is re-read by the follower burst
(`transcript_message_starts_session_chat_activity`), so one loop owns what was
published.

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

// ---------------------------------------------------------------------------
// Claude / OpenClaude grammar
//   Ctx Used: 11.0% | 13.5% | $261.54 | Fable 5 | high
// Model family and effort are independent segments, matched independently.
// ---------------------------------------------------------------------------

/// `(family segment prefix, pill value)` — mirrors the Claude models in the
/// published agent model catalog (`agent-model-catalog.json`).
const CLAUDE_MODEL_FAMILIES: &[(&str, &str)] = &[
    ("Fable", "fable"),
    ("Opus", "opus"),
    ("Sonnet", "sonnet"),
    ("Haiku", "haiku"),
];

/// Rendered lowercase by the TUI; mirrors the Claude efforts in the published
/// agent model catalog (`agent-model-catalog.json`).
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultracode"];

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

/// Mirrors the Codex efforts in the published agent model catalog
/// (`agent-model-catalog.json`); `max` and `ultra` sit behind the picker's
/// "More reasoning…" row.
const CODEX_EFFORTS: &[&str] = &["minimal", "low", "medium", "high", "xhigh", "max", "ultra"];

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
// Cursor Agent grammar
//   <Title> · GPT-5.6 Sol 272K Medium · 26K used
//
// Cursor paints the current model, context window, and reasoning effort in one
// segment between the chat title and a strict token-usage segment. Model ids
// are account-dependent, so known labels map to values the client can dispatch
// and unknown labels remain honest readbacks.
// ---------------------------------------------------------------------------

/// `(picker row text, pill value)` — mirrors the Cursor models in the
/// published agent model catalog (`agent-model-catalog.json`).
const CURSOR_MODEL_LABELS: &[(&str, &str)] = &[
    ("Auto", "auto"),
    ("Cursor Grok 4.6", "cursor-grok-4.6"),
    ("Composer 2.5", "composer-2.5"),
    ("Claude Opus 5", "claude-opus-5"),
    ("Claude Opus 4.8", "claude-opus-4-8"),
    ("GPT-5.6 Sol", "gpt-5.6-sol"),
    ("GPT-5.5", "gpt-5.5"),
    ("Claude Fable 5.1", "claude-fable-5-1"),
    ("Claude Fable 5", "claude-fable-5"),
    ("Cursor Grok 4.5", "cursor-grok-4.5"),
    ("Gemini 3.8 Flash", "gemini-3.8-flash"),
    ("Gemini 3.7 Flash", "gemini-3.7-flash"),
    ("GPT-5.6 Terra", "gpt-5.6-terra"),
    ("Claude Sonnet 5", "claude-sonnet-5"),
    ("Claude Sonnet 4.6", "claude-sonnet-4-6"),
    ("Codex 5.3", "gpt-5.3-codex"),
    ("Claude Opus 4.7", "claude-opus-4-7"),
    ("GPT-5.4", "gpt-5.4"),
    ("Claude Opus 4.6", "claude-opus-4-6"),
    ("Claude Opus 4.5", "claude-opus-4-5"),
    ("GPT-5.2", "gpt-5.2"),
    ("GPT-5.6 Luna", "gpt-5.6-luna"),
    ("Gemini 3.6 Flash", "gemini-3.6-flash"),
    ("Gemini 3.1 Pro", "gemini-3.1-pro"),
    ("GPT-5.4 Mini", "gpt-5.4-mini"),
    ("GPT-5.4 Nano", "gpt-5.4-nano"),
    ("Claude Haiku 4.5", "claude-haiku-4-5"),
    ("Claude Sonnet 4.5", "claude-sonnet-4-5"),
    ("GPT-5.1", "gpt-5.1"),
    ("Gemini 3.5 Flash", "gemini-3.5-flash"),
    ("Claude Sonnet 4", "claude-sonnet-4"),
    ("GPT-5 Mini", "gpt-5-mini"),
    ("Kimi K3", "kimi-k3"),
    ("Kimi K2.7 Code", "kimi-k2.7-code"),
    ("GLM 5.2", "glm-5.2"),
    ("Gemini 3 Flash", "gemini-3-flash"),
];

fn is_cursor_usage_segment(segment: &str) -> bool {
    let Some(number) = segment.strip_suffix(" used") else {
        return false;
    };
    let number = number.strip_suffix(['K', 'M']).unwrap_or(number);
    !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

const CURSOR_EFFORT_LABELS: &[(&str, &str)] = &[
    ("Extra High", "xhigh"),
    ("xHigh", "xhigh"),
    ("XHigh", "xhigh"),
    ("Medium", "medium"),
    ("Ultra", "ultra"),
    ("None", "none"),
    ("Med", "medium"),
    ("Low", "low"),
    ("High", "high"),
    ("Max", "max"),
];

fn is_cursor_context_window(token: &str) -> bool {
    let number = token.strip_suffix(['K', 'M']).unwrap_or(token);
    token.len() > number.len()
        && !number.is_empty()
        && number.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

fn split_cursor_model_context_and_effort(
    segment: &str,
) -> (String, Option<String>, Option<SessionChatDetectedChoice>) {
    for (label, value) in CURSOR_EFFORT_LABELS {
        let Some(before_effort) = segment.strip_suffix(label) else {
            continue;
        };
        if !before_effort.ends_with(char::is_whitespace) {
            continue;
        }
        let before_effort = before_effort.trim_end();
        if before_effort.is_empty() {
            continue;
        }
        let (model, context_window) = match before_effort.rsplit_once(' ') {
            Some((model, context_window)) if is_cursor_context_window(context_window) => {
                (model.trim_end(), Some(context_window.to_string()))
            }
            _ => (before_effort, None),
        };
        if model.is_empty() {
            continue;
        }
        return (
            model.to_string(),
            context_window,
            Some(SessionChatDetectedChoice {
                value: (*value).to_string(),
                label: (*label).to_string(),
                source: SessionChatOptionEvidence::Terminal,
            }),
        );
    }
    (segment.to_string(), None, None)
}

fn match_cursor_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let segments = line_segments(line);
    if segments.len() < 3 || !is_cursor_usage_segment(segments.last()?) {
        return None;
    }
    let combined = segments.get(segments.len() - 2)?.trim();
    if combined.is_empty() || combined == "\u{2014}" {
        return None;
    }
    let (combined, fast) = if let Some(without_fast) = combined
        .strip_suffix(" Fast")
        .or_else(|| combined.strip_suffix(" fast"))
        .or_else(|| combined.strip_suffix(" (Fast)"))
        .or_else(|| combined.strip_suffix(" (fast)"))
    {
        (without_fast.trim_end(), Some(true))
    } else {
        (combined, None)
    };
    let (model_label, context_window, effort) = split_cursor_model_context_and_effort(combined);
    let value = CURSOR_MODEL_LABELS
        .iter()
        .find(|(candidate, _)| model_label == *candidate)
        .map_or_else(
            || model_label.to_string(),
            |(_, value)| (*value).to_string(),
        );
    let display_model_label = model_label
        .strip_prefix("Cursor ")
        .or_else(|| model_label.strip_prefix("cursor "))
        .unwrap_or(&model_label)
        .to_string();
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value,
            label: display_model_label,
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort,
        context_window,
        terminal_status_line: Some(line.trim().to_string()),
        fast,
        context_usage: None,
        claude_status: None,
        ..SessionChatDetectedSelection::default()
    })
}

// ---------------------------------------------------------------------------
// Grok grammar
//   ╰──────────────────────── Grok 4.6 (medium) · always-approve ─╯
// Model and effort share ONE segment, and that segment is drawn INSIDE the
// bottom border of the composer box — so the rule has to come off the line
// before it can be read at all, and before `is_divider_line` would skip it.
// ---------------------------------------------------------------------------

/// The values grok's model catalog offers (`reasoning_efforts` in
/// `~/.grok/models_cache.json`); mirrors the Grok efforts in the published
/// agent model catalog (`agent-model-catalog.json`).
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
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
    })
}

// ---------------------------------------------------------------------------
// Antigravity CLI grammar
//   ? for shortcuts                                    Gemini 3.8 Flash · high
//   Gemini 3.8 Flash (High)                       (startup banner, same values)
//
// CDXC:AgentScreenDetection 2026-09-03: the footer's right edge is
// `<model> · <effort>` for the Gemini rows, whose ids are model and effort
// flattened (`gemini-3.8-flash-high`, see `agy models`), and a bare `<model>`
// for the rows without an effort slider. The pill values are the catalog's
// model part, exactly what `antigravityModelCommand` re-flattens when it types
// `/model` (packages/core-ui/chat/session-chat-session-options.ts). Only names
// the catalog knows are accepted, so a prose line that ends in `· high` never
// becomes state.
// ---------------------------------------------------------------------------

const ANTIGRAVITY_EFFORTS: &[&str] = &["low", "medium", "high"];

/// Display names that do not derive their catalog id from the name alone:
/// `agy models` folds the fixed reasoning mode into these ids.
const ANTIGRAVITY_FIXED_MODEL_IDS: &[(&str, &str)] = &[
    ("Claude Sonnet 4.6", "claude-sonnet-4-6"),
    ("Claude Opus 4.6", "claude-opus-4-6-thinking"),
    ("GPT-OSS 120B", "gpt-oss-120b-medium"),
];

/// `Gemini 3.8 Flash` ⇒ `gemini-3.8-flash`; `Gemini 3.1 Pro` ⇒ `gemini-3.1-pro`.
fn antigravity_model_id(name: &str) -> Option<String> {
    if let Some((_, id)) = ANTIGRAVITY_FIXED_MODEL_IDS
        .iter()
        .find(|(display, _)| *display == name)
    {
        return Some((*id).to_string());
    }
    let rest = name.strip_prefix("Gemini")?;
    let (version, tier) = rest.trim_start().split_once(' ')?;
    if !is_model_version_suffix(&format!(" {version}")) || !matches!(tier, "Flash" | "Pro") {
        return None;
    }
    Some(name.to_ascii_lowercase().replace(' ', "-"))
}

/// The model name is right-aligned after the shortcut hint, so it is the text
/// after the last run of two or more spaces (or the whole segment when the
/// footer has nothing on its left).
fn antigravity_trailing_name(segment: &str) -> &str {
    let trimmed = segment.trim();
    match trimmed.rfind("  ") {
        Some(index) => trimmed[index..].trim(),
        None => trimmed,
    }
}

fn match_antigravity_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let segments = line_segments(line);
    let (name, effort) = match segments.as_slice() {
        [.., model, effort]
            if ANTIGRAVITY_EFFORTS.contains(&effort.to_ascii_lowercase().as_str()) =>
        {
            (
                antigravity_trailing_name(model),
                Some(effort.to_ascii_lowercase()),
            )
        }
        [only] => match only
            .trim_end()
            .strip_suffix(')')
            .and_then(|rest| rest.rsplit_once(" ("))
        {
            // Startup banner: `Gemini 3.8 Flash (High)`.
            Some((name, effort))
                if ANTIGRAVITY_EFFORTS.contains(&effort.to_ascii_lowercase().as_str()) =>
            {
                (
                    antigravity_trailing_name(name),
                    Some(effort.to_ascii_lowercase()),
                )
            }
            _ => (antigravity_trailing_name(only), None),
        },
        _ => return None,
    };
    let value = antigravity_model_id(name)?;
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value,
            label: name.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort: effort.map(|effort| SessionChatDetectedChoice {
            label: effort.clone(),
            value: effort,
            source: SessionChatOptionEvidence::Terminal,
        }),
        mode: None,
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
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
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
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
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
    })
}

// ---------------------------------------------------------------------------
// Hermes grammar
//   ⚕ grok-4.6 │ ctx -- │ [░░░░░░░░░░] -- │ 34s │ ⏲ 0s
//
// The model is the first `│` segment: one single-glyph marker, then the id
// (measured 2026-08-29, Hermes Agent v0.20.4). The `⏲ …` timer segment plus
// the segment count keep prose from matching; the context segment cannot
// anchor anything because it changes shape after the first exchange (`ctx --`
// becomes `26.2K/900K`). No reasoning effort is drawn anywhere on screen.
// ---------------------------------------------------------------------------

fn match_hermes_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let segments: Vec<&str> = line.split('\u{2502}').map(str::trim).collect();
    if segments.len() < 4
        || !segments[1..]
            .iter()
            .any(|segment| segment.starts_with('\u{23f2}'))
    {
        return None;
    }
    let mut head = segments[0].split_whitespace();
    let marker = head.next()?;
    let model = head.next()?;
    if head.next().is_some()
        || marker.chars().count() != 1
        || marker
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric())
        || !is_pi_family_model_id(model)
    {
        return None;
    }
    Some(SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: model.to_string(),
            label: model.to_string(),
            source: SessionChatOptionEvidence::Terminal,
        }),
        effort: None,
        mode: None,
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
    })
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Scans the tail window bottom-up; the bottom-most match wins. Returns `None`
/// when the window carries no statusline this parser understands. A detection
/// carries the agent's whole footer (statusline down to the bottom of the
/// screen) as `terminal_status_line`.
pub fn detect_session_chat_selection(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatDetectedSelection> {
    let scanned_lines = scan_window(text);
    let mut found = SessionChatDetectedSelection::default();
    // Index of the topmost line that supplied any value; the footer capture
    // starts there.
    let mut topmost_match: Option<usize> = None;
    for (index, scanned) in scanned_lines.iter().enumerate().rev() {
        match agent {
            SessionChatOptionAgent::Antigravity => {
                if let Some(selection) = match_antigravity_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Cursor => {
                if let Some(selection) = match_cursor_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Pi => {
                if let Some(selection) = match_pi_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Omp => {
                if let Some(selection) = match_omp_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Hermes => {
                if let Some(selection) = match_hermes_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
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
        let matched_before = (
            found.model.is_some(),
            found.effort.is_some(),
            found.mode.is_some(),
        );
        for segment in segments.iter() {
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
                SessionChatOptionAgent::Antigravity => {
                    unreachable!("Antigravity is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Cursor => {
                    unreachable!("Cursor is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Grok => {
                    if found.model.is_none() && found.effort.is_none() {
                        if let Some(selection) = match_grok_segment(segment) {
                            found = selection;
                        }
                    }
                }
                SessionChatOptionAgent::Hermes => {
                    unreachable!("Hermes is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Omp => {
                    unreachable!("Omp is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Pi => unreachable!("Pi is parsed as a complete statusline"),
            }
        }
        if (
            found.model.is_some(),
            found.effort.is_some(),
            found.mode.is_some(),
        ) != matched_before
        {
            topmost_match = Some(index);
        }
        if found.model.is_some() && found.effort.is_some() {
            break;
        }
    }
    if found.model.is_none() && found.effort.is_none() && found.mode.is_none() {
        return None;
    }
    if let Some(top) = topmost_match {
        let footer = scanned_lines[top..]
            .iter()
            .map(|line| {
                // Grok draws its statusline on the composer's border chrome.
                if agent == SessionChatOptionAgent::Grok {
                    strip_box_drawing(line).trim().to_string()
                } else {
                    line.trim().to_string()
                }
            })
            .filter(|line| !line.is_empty() && !is_divider_line(line))
            .collect::<Vec<_>>()
            .join("\n");
        if !footer.is_empty() {
            found.terminal_status_line = Some(footer);
        }
    }
    Some(found)
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

pub(crate) fn claude_transcript_model_choice(model: &str) -> Option<SessionChatDetectedChoice> {
    // `claude-fable-5-1[1m]` — the context-window suffix is not a version token.
    let normalized = model.trim().to_ascii_lowercase();
    let normalized = normalized
        .split_once('[')
        .map_or(normalized.as_str(), |(head, _)| head)
        .to_string();
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

/*
CDXC:AgentScreenDetection 2026-09-03 WHY:
Claude's transcript records its permission mode too: a `permission-mode` row
when the mode is set, and `permissionMode` on every user row. Reading them
gives the mode pill a value before the first screen capture and without any
screen at all (a sleeping session, a capped capture). The footer scrape still
wins when present because it is the live value.
*/
fn claude_transcript_mode_choice(mode: &str) -> Option<SessionChatDetectedChoice> {
    let (value, label) = match mode.trim() {
        "auto" => ("auto", "Auto"),
        "bypassPermissions" => ("bypass", "Bypass permissions"),
        "plan" => ("plan", "Plan"),
        "acceptEdits" => ("accept-edits", "Accept edits"),
        "default" => ("manual", "Manual"),
        _ => return None,
    };
    Some(SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Transcript,
    })
}

fn detect_session_chat_transcript_selection(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatDetectedSelection> {
    if agent == SessionChatOptionAgent::Claude {
        return detect_claude_transcript_selection(text);
    }
    for line in text.lines().rev() {
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let selection = match agent {
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
                    context_window: None,
                    terminal_status_line: None,
                    fast: None,
                    context_usage: None,
                    claude_status: None,
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

/// Newest assistant row for model/effort, newest permission row for mode;
/// the scan stops as soon as all three are known.
fn detect_claude_transcript_selection(text: &str) -> Option<SessionChatDetectedSelection> {
    let mut selection = SessionChatDetectedSelection::default();
    let mut assistant_seen = false;
    for line in text.lines().rev() {
        if assistant_seen && selection.mode.is_some() {
            break;
        }
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("isSidechain") == Some(&Value::Bool(true)) {
            continue;
        }
        match transcript_text(record.get("type")) {
            Some("assistant") if !assistant_seen => {
                let message = record.get("message").and_then(Value::as_object);
                selection.model = message
                    .and_then(|message| transcript_text(message.get("model")))
                    .and_then(claude_transcript_model_choice);
                selection.effort =
                    transcript_text(record.get("effort")).and_then(transcript_effort_choice);
                assistant_seen = true;
            }
            Some("permission-mode") | Some("user") if selection.mode.is_none() => {
                selection.mode = transcript_text(record.get("permissionMode"))
                    .and_then(claude_transcript_mode_choice);
            }
            _ => {}
        }
    }
    (selection.model.is_some() || selection.effort.is_some() || selection.mode.is_some())
        .then_some(selection)
}

/*
CDXC:AgentScreenDetection 2026-09-03 WHY:
The payload the Ghostex statusLine script stored for this Claude session id.
Model and effort are the live session values (Claude re-runs the script on
`/model`, `/effort`, each assistant message, compaction and mode changes),
so they outrank the transcript, which only learns a change on the next turn.
*/
fn read_session_chat_statusline_selection(
    hook_state_directory: &Path,
    agent_session_id: Option<&str>,
) -> Option<SessionChatDetectedSelection> {
    let stored = crate::agent_hooks::statusline::read_claude_statusline_payload(
        hook_state_directory,
        agent_session_id?,
    )?;
    let payload = &stored.payload;
    let choice = |value: &str, label: &str| SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Statusline,
    };
    let model = payload
        .get("model")
        .and_then(|model| transcript_text(model.get("id")))
        .and_then(claude_transcript_model_choice)
        .map(|found| choice(&found.value, &found.label));
    let effort = payload
        .get("effort")
        .and_then(|effort| transcript_text(effort.get("level")))
        .and_then(transcript_effort_choice)
        .map(|found| choice(&found.value, &found.label));
    let fast = payload.get("fast_mode").and_then(Value::as_bool);
    let context_window = payload.get("context_window");
    let context_usage = SessionChatContextUsage {
        used_percentage: context_window
            .and_then(|window| window.get("used_percentage"))
            .and_then(Value::as_f64)
            .map(|value| value.round().clamp(0.0, 100.0) as u32),
        used_tokens: context_window
            .and_then(|window| window.get("total_input_tokens"))
            .and_then(Value::as_u64),
        window_size: context_window
            .and_then(|window| window.get("context_window_size"))
            .and_then(Value::as_u64),
    };
    let selection = SessionChatDetectedSelection {
        model,
        effort,
        mode: None,
        context_window: None,
        terminal_status_line: None,
        fast,
        context_usage: (!context_usage.is_empty()).then_some(context_usage),
        claude_status: claude_statusline_status_value(payload),
    };
    (selection.model.is_some() || selection.effort.is_some()).then_some(selection)
}

/*
CDXC:SessionChatDetectedOptions 2026-09-04 DECISION:
User: the context meter popover gets a "More details" section (cost, rate
limits, prompt cache, last request, lines, thinking, version, ...) with a pen
icon to pick, reorder within groups, and star rows for a text status line under
the chat box. Everything the chat can show is lifted here from the stored
payload, renamed to camelCase and dropped when Claude did not report it, so
the client owns which rows exist and never sees the raw payload.
*/
fn claude_statusline_status_value(payload: &Map<String, Value>) -> Option<Value> {
    fn get<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
        value.and_then(|value| value.get(key))
    }
    fn put(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
        if let Some(value) = value {
            map.insert(key.to_string(), value);
        }
    }
    fn number(value: Option<&Value>) -> Option<Value> {
        value
            .and_then(Value::as_f64)
            .filter(|number| number.is_finite())
            .map(|number| json!(number))
    }
    fn integer(value: Option<&Value>) -> Option<Value> {
        value.and_then(Value::as_i64).map(|number| json!(number))
    }
    fn boolean(value: Option<&Value>) -> Option<Value> {
        value.and_then(Value::as_bool).map(|flag| json!(flag))
    }
    fn text(value: Option<&Value>) -> Option<Value> {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| json!(text))
    }
    fn object(map: Map<String, Value>) -> Option<Value> {
        (!map.is_empty()).then_some(Value::Object(map))
    }

    let mut status = Map::new();

    let cost = payload.get("cost");
    let mut cost_map = Map::new();
    put(
        &mut cost_map,
        "totalUsd",
        number(get(cost, "total_cost_usd")),
    );
    put(
        &mut cost_map,
        "durationMs",
        integer(get(cost, "total_duration_ms")),
    );
    put(
        &mut cost_map,
        "apiDurationMs",
        integer(get(cost, "total_api_duration_ms")),
    );
    put(
        &mut cost_map,
        "linesAdded",
        integer(get(cost, "total_lines_added")),
    );
    put(
        &mut cost_map,
        "linesRemoved",
        integer(get(cost, "total_lines_removed")),
    );
    put(&mut status, "cost", object(cost_map));

    let rate_limits = payload.get("rate_limits");
    let mut limits_map = Map::new();
    for (key, name) in [("five_hour", "fiveHour"), ("seven_day", "sevenDay")] {
        let window = get(rate_limits, key);
        let mut window_map = Map::new();
        put(
            &mut window_map,
            "usedPercentage",
            number(get(window, "used_percentage")),
        );
        put(
            &mut window_map,
            "resetsAt",
            integer(get(window, "resets_at")),
        );
        put(&mut limits_map, name, object(window_map));
    }
    put(&mut status, "rateLimits", object(limits_map));

    let cache = payload.get("prompt_cache");
    let mut cache_map = Map::new();
    put(&mut cache_map, "warm", boolean(get(cache, "warm")));
    put(&mut cache_map, "ttl", text(get(cache, "ttl")));
    put(
        &mut cache_map,
        "expiresAt",
        integer(get(cache, "expires_at")),
    );
    put(&mut cache_map, "hitRatio", number(get(cache, "hit_ratio")));
    put(&mut cache_map, "requests", integer(get(cache, "requests")));
    put(&mut cache_map, "misses", integer(get(cache, "misses")));
    put(
        &mut cache_map,
        "lastMissCause",
        text(get(cache, "last_miss_cause")),
    );
    put(
        &mut cache_map,
        "cacheWriteTokens",
        integer(get(cache, "cache_write_tokens")),
    );
    put(
        &mut cache_map,
        "recacheTokensIfCold",
        integer(get(cache, "recache_tokens_if_cold")),
    );
    put(&mut status, "promptCache", object(cache_map));

    let context_window = payload.get("context_window");
    let usage = get(context_window, "current_usage");
    let mut request_map = Map::new();
    put(
        &mut request_map,
        "inputTokens",
        integer(get(usage, "input_tokens")),
    );
    put(
        &mut request_map,
        "outputTokens",
        integer(get(usage, "output_tokens")),
    );
    put(
        &mut request_map,
        "cacheReadTokens",
        integer(get(usage, "cache_read_input_tokens")),
    );
    put(
        &mut request_map,
        "cacheWriteTokens",
        integer(get(usage, "cache_creation_input_tokens")),
    );
    put(&mut status, "lastRequest", object(request_map));
    put(
        &mut status,
        "totalOutputTokens",
        integer(get(context_window, "total_output_tokens")),
    );
    put(
        &mut status,
        "remainingPercentage",
        number(get(context_window, "remaining_percentage")),
    );
    put(
        &mut status,
        "exceeds200kTokens",
        boolean(payload.get("exceeds_200k_tokens")),
    );

    put(
        &mut status,
        "thinkingEnabled",
        boolean(get(payload.get("thinking"), "enabled")),
    );
    put(
        &mut status,
        "outputStyle",
        text(get(payload.get("output_style"), "name")),
    );
    put(
        &mut status,
        "sessionName",
        text(payload.get("session_name")),
    );
    put(&mut status, "version", text(payload.get("version")));

    let workspace = payload.get("workspace");
    let repo = get(workspace, "repo");
    let mut repo_map = Map::new();
    put(&mut repo_map, "host", text(get(repo, "host")));
    put(&mut repo_map, "owner", text(get(repo, "owner")));
    put(&mut repo_map, "name", text(get(repo, "name")));
    put(&mut status, "repo", object(repo_map));
    let added_dirs: Vec<Value> = get(workspace, "added_dirs")
        .and_then(Value::as_array)
        .map(|dirs| dirs.iter().filter_map(|dir| text(Some(dir))).collect())
        .unwrap_or_default();
    if !added_dirs.is_empty() {
        status.insert("addedDirs".to_string(), Value::Array(added_dirs));
    }
    put(
        &mut status,
        "projectDir",
        text(get(workspace, "project_dir")),
    );
    put(
        &mut status,
        "currentDir",
        text(get(workspace, "current_dir")).or_else(|| text(payload.get("cwd"))),
    );

    let pr = payload.get("pr");
    let mut pr_map = Map::new();
    put(&mut pr_map, "number", integer(get(pr, "number")));
    put(&mut pr_map, "url", text(get(pr, "url")));
    put(&mut pr_map, "reviewState", text(get(pr, "review_state")));
    put(&mut status, "pr", object(pr_map));

    object(status)
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
            // Antigravity's footer names both values for the whole session,
            // and its mirrored step log carries no model field.
            SessionChatOptionAgent::Antigravity => return None,
            SessionChatOptionAgent::Cursor => return None,
            /*
            Grok's statusline is on screen for the whole session and names both
            values, and its update-stream rows carry no effort at all, so there
            is nothing a transcript read could add here.
            */
            SessionChatOptionAgent::Grok => return None,
            /*
            Hermes names the model in a statusline that is on screen for the
            whole session, and its mirrored transcript rows carry no model
            field, so there is nothing a transcript read could add here.
            */
            SessionChatOptionAgent::Hermes => return None,
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

/// Every `Some` in `layer` replaces the value beneath it.
fn overlay_session_chat_option_selection(
    merged: &mut SessionChatDetectedSelection,
    layer: SessionChatDetectedSelection,
) {
    if layer.model.is_some() {
        merged.model = layer.model;
    }
    if layer.effort.is_some() {
        merged.effort = layer.effort;
    }
    if layer.mode.is_some() {
        merged.mode = layer.mode;
    }
    if layer.context_window.is_some() {
        merged.context_window = layer.context_window;
    }
    if layer.terminal_status_line.is_some() {
        merged.terminal_status_line = layer.terminal_status_line;
    }
    if layer.fast.is_some() {
        merged.fast = layer.fast;
    }
    if layer.context_usage.is_some() {
        merged.context_usage = layer.context_usage;
    }
    if layer.claude_status.is_some() {
        merged.claude_status = layer.claude_status;
    }
}

/// Precedence, lowest first: transcript (a turn behind), statusline payload
/// (live, but only what Claude puts in it), terminal screen (live, and the
/// only source for the permission mode footer).
fn merge_session_chat_option_selections(
    transcript: Option<SessionChatDetectedSelection>,
    statusline: Option<SessionChatDetectedSelection>,
    terminal: Option<SessionChatDetectedSelection>,
) -> Option<SessionChatDetectedSelection> {
    let mut merged = transcript.unwrap_or_default();
    if let Some(statusline) = statusline {
        overlay_session_chat_option_selection(&mut merged, statusline);
    }
    if let Some(terminal) = terminal {
        overlay_session_chat_option_selection(&mut merged, terminal);
    }
    (merged.model.is_some() || merged.effort.is_some() || merged.mode.is_some()).then_some(merged)
}

/// Full detection for one session: resolve structured transcript metadata,
/// then let any current terminal statusline value win per option. `None` means
/// neither agent-owned source proved a value.
///
/// CDXC:AgentScreenDetection 2026-08-19: the same capture is classified
/// for terminal-state notices, so both readings ride one process spawn.
pub fn detect_session_chat_terminal_state(
    repository: &DomainRepository<'_>,
    hook_state_directory: &Path,
    project_id: &str,
    session_id: &str,
    agent_id: Option<&str>,
) -> SessionChatTerminalDetection {
    /*
    CDXC:SessionChat 2026-08-26:
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
    // The hooks record Claude's own session id and transcript path on the
    // session row; both the statusline sidecar and the task store hang off them.
    let (claude_session_id, claude_session_path) = (agent == Some(SessionChatOptionAgent::Claude))
        .then(|| {
            repository
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        })
        .flatten()
        .map(|session| {
            let runtime_text = |key: &str| {
                session
                    .get("runtimeSettings")
                    .and_then(|runtime| runtime.get(key))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            };
            (
                runtime_text("agentSessionId"),
                runtime_text("agentSessionPath"),
            )
        })
        .unwrap_or((None, None));
    let statusline = (agent == Some(SessionChatOptionAgent::Claude))
        .then(|| {
            read_session_chat_statusline_selection(
                hook_state_directory,
                claude_session_id.as_deref(),
            )
        })
        .flatten();
    // CDXC:SessionChat 2026-09-03: disk, not screen, so it is read
    // whether or not the capture below succeeds.
    let tasks = crate::session_chat_agent_tasks::read_session_chat_agent_tasks(
        claude_session_id.as_deref(),
        claude_session_path.as_deref(),
    );
    let capture = crate::zmx::read_zmx_session_history_capture(repository, project_id, session_id)
        .ok()
        .map(|mut capture| {
            // One cut for every detector below (see session_chat_screen_pane.rs).
            capture.text = crate::session_chat_screen_pane::strip_side_pane(&capture.text);
            capture
        });
    let terminal = agent
        .zip(capture.as_ref())
        .and_then(|(agent, capture)| detect_session_chat_selection(agent, &capture.text));
    // A capped capture lost its tail, so the live screen is not in it.
    let screen = capture.as_ref().filter(|capture| !capture.truncated);
    let notice = screen.and_then(|capture| {
        crate::session_chat_notice::classify_session_chat_terminal_notice(agent_id, &capture.text)
    });
    let options = merge_session_chat_option_selections(transcript, statusline, terminal)
        .map(SessionChatDetectedOptions::new);
    let composer = match screen {
        Some(capture) => crate::session_chat_composer::detect_session_chat_composer_readiness(
            agent_id,
            &capture.text,
            notice.as_ref(),
        ),
        None => crate::session_chat_composer::SessionChatComposerReadiness::default(),
    };
    let activity = if agent == Some(SessionChatOptionAgent::Cursor)
        && composer.state == crate::session_chat_composer::SessionChatComposerState::Ready
    {
        // Cursor leaves its last Braille working row in scrollback after an
        // answer or interruption. Its live composer is authoritative idle
        // evidence, so that older row must not survive as chat activity.
        None
    } else {
        screen.and_then(|capture| {
            crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                agent_id,
                &capture.text,
            )
        })
    };
    let fleet = screen.and_then(|capture| {
        crate::session_chat_agent_fleet::detect_session_chat_agent_fleet(agent_id, &capture.text)
    });
    let prompt = screen.and_then(|capture| {
        crate::session_chat::detect_cursor_question_prompt(agent_id, &capture.text)
    });
    /*
    CDXC:AgentScreenDetection (settled 2026-08-30): probed only counts once
    the answer is settled. A capture that failed IS settled — a sleeping or
    stopped session has nothing to read, and that stays true until it runs. A
    capture that succeeded settles only when some classifier recognized the
    agent on it; a blank screen behind a CLI that is still booting is "not read
    yet", so the model pill keeps its loading skeleton instead of flashing a
    bare "Model" label until the statusline draws.
    */
    let attempted = capture.is_none()
        || options.is_some()
        || notice.is_some()
        || activity.is_some()
        || fleet.is_some()
        || prompt.is_some()
        || composer.state == crate::session_chat_composer::SessionChatComposerState::Ready;
    SessionChatTerminalDetection {
        options,
        prompt,
        composer,
        notice,
        activity,
        fleet,
        tasks,
        captured: screen.is_some(),
        attempted,
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

    const CURSOR_WITHOUT_CONTEXT: &str = concat!(
        "  \u{2192} Plan, search, build anything\n",
        "  New Agent \u{b7} Cursor Grok 4.6 Medium \u{b7} 0 used\n",
        "  Ghostex \u{b7} main \u{b7} Ctx 0% used \u{b7} +6702 -472\n",
    );

    fn claude(text: &str) -> Option<SessionChatDetectedSelection> {
        detect_session_chat_selection(SessionChatOptionAgent::Claude, text)
    }

    fn codex(text: &str) -> Option<SessionChatDetectedSelection> {
        detect_session_chat_selection(SessionChatOptionAgent::Codex, text)
    }

    fn cursor(text: &str) -> Option<SessionChatDetectedSelection> {
        detect_session_chat_selection(SessionChatOptionAgent::Cursor, text)
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
    fn detects_cursor_effort_without_a_context_token_and_hides_the_brand_prefix() {
        let selection = cursor(CURSOR_WITHOUT_CONTEXT).expect("cursor footer detected");
        assert_eq!(pair(&selection), (Some("cursor-grok-4.6"), Some("medium")));
        assert_eq!(selection.model.as_ref().unwrap().label, "Grok 4.6");
        assert_eq!(selection.effort.as_ref().unwrap().label, "Medium");
        assert_eq!(selection.context_window, None);
        assert_eq!(
            selection.terminal_status_line.as_deref(),
            Some(concat!(
                "New Agent \u{b7} Cursor Grok 4.6 Medium \u{b7} 0 used\n",
                "Ghostex \u{b7} main \u{b7} Ctx 0% used \u{b7} +6702 -472"
            ))
        );
    }

    #[test]
    fn detects_every_cursor_effort_spelling() {
        for (label, value) in [
            ("Low", "low"),
            ("Medium", "medium"),
            ("Med", "medium"),
            ("High", "high"),
            ("xHigh", "xhigh"),
            ("Extra High", "xhigh"),
            ("Max", "max"),
            ("Ultra", "ultra"),
        ] {
            let text = format!("New Agent \u{b7} GPT-5.6 Sol {label} \u{b7} 0 used\n");
            let selection = cursor(&text).expect("cursor footer detected");
            assert_eq!(
                selection
                    .effort
                    .as_ref()
                    .map(|choice| choice.value.as_str()),
                Some(value)
            );
        }
    }

    #[test]
    fn codex_model_id_outside_the_catalog_is_reported_verbatim() {
        let text = "  Some Title \u{b7} gpt-9.1-nova medium \u{b7} 1K used \u{b7} Context 3% used \u{b7} weekly 99% left\n";
        let selection = codex(text).expect("codex footer detected");
        assert_eq!(pair(&selection), (Some("gpt-9.1-nova"), Some("medium")));
        assert_eq!(selection.model.as_ref().unwrap().label, "gpt-9.1-nova");
    }

    /// CDXC:AgentScreenDetection 2026-09-03 WHY: `tui.status_line` is user
    /// ordered, so a footer that lists `model-with-reasoning` first must read
    /// the same as the default order. Live capture from a session whose config
    /// puts the model before the thread title.
    #[test]
    fn codex_model_first_footer_is_read() {
        let text = "  gpt-5.6-sol high \u{b7} Fix status line parsing \u{b7} 89K used \u{b7} Ghostex \u{b7} main \u{b7} Context 34% used \u{b7} weekly 34% left\n";
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
        assert_eq!(
            session_chat_option_agent(Some("cursor")),
            Some(SessionChatOptionAgent::Cursor)
        );
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
            context_window: None,
            terminal_status_line: None,
            fast: None,
            context_usage: None,
            claude_status: None,
        };
        let terminal = claude("Ctx Used: 1% | Opus 4.8").unwrap();
        let merged = merge_session_chat_option_selections(Some(transcript), None, Some(terminal))
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
                context_window: None,
                terminal_status_line: None,
                fast: Some(true),
                context_usage: None,
                claude_status: None,
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
CDXC:AgentScreenDetection 2026-08-01:
Model/effort detection reads structured transcript metadata plus the session's
zmx scrollback. The latter costs one short-lived process, so the combined read
is NEVER done per frame or per long-poll tick.
Every trigger goes through this 5s-TTL per-session cache: chat reads, the
+2s/+6s probes after a dispatched `/model`//`/effort`//`/fast`, and the
follower's ~30s piggyback. A miss is cached too — a session whose agent prints
no statusline must not re-spawn `zmx history` on every read. Detection is
deliberately absent from resolve_session_chat_read_state's fingerprint: hashing
it would make each 500ms long-poll tick spawn a process.

CDXC:AgentScreenDetection 2026-08-19: the SAME capture is classified for
terminal-state notices (login expired, trust dialog, usage limit, a crashed
CLI), so the cache entry carries both readings and neither costs an extra spawn.
*/
pub(crate) struct SessionChatOptionCacheEntry {
    pub(crate) fetched_at: std::time::Instant,
    /*
    The compaction state last projected into presentation. This is deliberately
    not identical to `value.activity`: Claude leaves completed status rows in
    terminal scrollback, while its hook-owned working transition tells chat
    those rows are no longer live. `None` means no trustworthy projection has
    been made yet (for example, a first capture failed while the hook still
    reported working).
    */
    pub(crate) projected_compacting: Option<bool>,
    /// First capture that was settle-eligible except for its missing model —
    /// the anchor `SESSION_CHAT_OPTION_MODEL_SETTLE_GRACE` counts from. Cleared
    /// the moment a model (or a screen-owning notice) shows up.
    pub(crate) model_grace_started: Option<std::time::Instant>,
    /*
    CDXC:AgentScreenDetection 2026-09-03:
    The last screen notice that stopped classifying, with the instant it left.
    Claude Code's Ink redraws and a capture that lands mid-repaint make a banner
    (the usage-limit line) miss a probe every so often, so the cached notice
    flipped to None and the next probe minted a fresh `detectedAt` — which is
    the client's dismissal key, so a card the user had closed popped back up
    every few seconds. A re-detection that says the same thing within
    `SESSION_CHAT_NOTICE_REAPPEAR_GRACE` of the notice leaving is the same
    instance and inherits its `detectedAt`; only a longer absence makes the
    same words a new event.
    */
    pub(crate) retired_notice: Option<(
        crate::session_chat_notice::SessionChatTerminalNotice,
        std::time::Instant,
    )>,
    pub(crate) value: crate::session_chat_options::SessionChatTerminalDetection,
}

/// How long a screen notice that stopped classifying still counts as the same
/// instance when the identical words come back; see
/// `SessionChatOptionCacheEntry::retired_notice`.
pub(crate) const SESSION_CHAT_NOTICE_REAPPEAR_GRACE: std::time::Duration =
    std::time::Duration::from_secs(10 * 60);

/// Where the Ghostex agent hooks (and the Claude statusline script) keep their
/// per-session state — the same resolution the installer bakes into them.
pub(crate) fn session_chat_hook_state_directory(paths: &GxserverPaths) -> std::path::PathBuf {
    crate::agent_hooks::config::HookPaths::from_paths(paths).hook_state_directory
}

#[derive(Clone)]
pub(crate) struct SessionChatOptionDetector {
    cache: Arc<Mutex<HashMap<String, SessionChatOptionCacheEntry>>>,
    compacting_publisher: crate::session_chat_compacting::SessionChatCompactingPublisher,
    paths: GxserverPaths,
    server_id: String,
}

impl SessionChatOptionDetector {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            cache: state.session_chat_option_cache.clone(),
            compacting_publisher:
                crate::session_chat_compacting::SessionChatCompactingPublisher::new(state),
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
        // CDXC:SessionChat 2026-08-26: same two-door gate the
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
                    &session_chat_hook_state_directory(&self.paths),
                    project_id,
                    session_id,
                    agent,
                )
            })
            .unwrap_or_default();
        /*
        CDXC:AgentScreenDetection 2026-08-19:
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
        CDXC:SessionChat 2026-08-26: only an agent the NOTICE
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
        let mut compacting_transition: Option<Option<String>> = None;
        if let Ok(mut cache) = self.cache.lock() {
            let previous_compacting = cache.get(&key).and_then(|entry| entry.projected_compacting);
            // A notice that left the screen recently still counts as the
            // instance to inherit from; see `retired_notice`.
            let recently_retired_notice = cache
                .get(&key)
                .and_then(|entry| entry.retired_notice.clone())
                .filter(|(_, retired_at)| {
                    retired_at.elapsed() < SESSION_CHAT_NOTICE_REAPPEAR_GRACE
                });
            let cached_notice = cache.get(&key).and_then(|entry| entry.value.notice.clone());
            let retired_notice = match detected.notice.as_mut() {
                Some(notice) => {
                    notice.carry_forward_detected_at(
                        cached_notice
                            .as_ref()
                            .or(recently_retired_notice.as_ref().map(|(notice, _)| notice)),
                    );
                    None
                }
                None => cached_notice
                    .map(|notice| (notice, std::time::Instant::now()))
                    .or(recently_retired_notice),
            };
            /*
            CDXC:AgentScreenDetection 2026-08-22: same instance-not-sample
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
            CDXC:AgentScreenDetection 2026-08-23: deliberately NOT carried
            forward, unlike the notice and the activity row above. A fleet's
            `detectedAt` is the anchor its per-row clocks count from, so it has
            to stay paired with the seconds it was read beside; giving a fresh
            reading an older anchor would make every client count that interval
            twice. Holding a fleet still is `same_fleet`'s job.
            */
            /*
            CDXC:AgentScreenDetection (settled 2026-08-30): the model grace.
            The pure detector settles on any recognized chrome, but Claude's
            permission-mode footer and composer paint seconds before the async
            statusline that names the model. For an agent whose grammar CAN
            name a model, an otherwise-settled capture with no model stays
            unsettled for `SESSION_CHAT_OPTION_MODEL_SETTLE_GRACE` from the
            first such capture, so the model pill keeps its skeleton until the
            statusline lands — or until the grace decides no statusline is
            coming. A screen-owning notice (trust dialog, expired login) is
            exempt: the model cannot render behind it, and that state can hold
            indefinitely, so it settles at once. Lives here, not in the pure
            function, because the anchor needs the per-session cache.
            */
            let model_missing = detected
                .options
                .as_ref()
                .map_or(true, |options| options.selection.model.is_none());
            let mut model_grace_started = None;
            if detected.attempted
                && model_missing
                && detected.notice.is_none()
                && crate::session_chat_options::session_chat_option_agent(agent).is_some()
            {
                let started = cache
                    .get(&key)
                    .and_then(|entry| entry.model_grace_started)
                    .unwrap_or_else(std::time::Instant::now);
                if started.elapsed()
                    < crate::session_chat_options::SESSION_CHAT_OPTION_MODEL_SETTLE_GRACE
                {
                    detected.attempted = false;
                }
                model_grace_started = Some(started);
            }
            /*
            CDXC:AgentScreenDetection 2026-09-02:
            The screen owns this marker outright. It used to be forced false
            whenever the hooks said idle, on the theory that a finished
            compaction's row lingers in scrollback like a `⏺` status does — but
            Claude repaints the compacting row in place and replaces it with
            its `Compacted` line, so a whole capture is both the start and the
            end evidence, and the hook gate only hid compactions the hooks
            never learned about (a `/compact` typed in the terminal). Only a
            whole capture may change the verdict; a failed/capped capture
            preserves the last safe state.
            */
            let detected_compacting = if detected.captured {
                Some(
                    crate::session_chat_terminal_activity::is_session_chat_compacting_activity(
                        detected.activity.as_ref(),
                    ),
                )
            } else {
                previous_compacting
            };
            cache.insert(
                key,
                SessionChatOptionCacheEntry {
                    fetched_at: std::time::Instant::now(),
                    projected_compacting: detected_compacting,
                    model_grace_started,
                    retired_notice,
                    value: detected.clone(),
                },
            );
            if detected_compacting != previous_compacting {
                if let Some(detected_compacting) = detected_compacting {
                    compacting_transition = Some(
                        detected_compacting
                            .then(|| {
                                detected
                                    .activity
                                    .as_ref()
                                    .map(|activity| activity.detected_at.clone())
                            })
                            .flatten(),
                    );
                }
            }
        }
        if let Some(detected_at) = compacting_transition {
            self.compacting_publisher
                .publish(project_id, session_id, detected_at.as_deref());
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
CDXC:AgentScreenDetection 2026-08-19:
The notice a session should be showing RIGHT NOW, with no detection of its own:
the last classification the shared 5s cache holds, overridden by a watchdog
notice when one is pending. Every path that must stay spawn-free — the 500ms
long-poll fingerprint, prompt-driven state frames — reads it through here.
*/
/*
CDXC:AgentScreenDetection 2026-08-22:
The whole screen-derived half of a session's state, owned, from the shared 5s
cache and the watchdog store. Every spawn-free publisher reads it through here
so the notice and the progress row are always taken from the SAME cache read
and can never be published one frame apart.
*/
#[derive(Default)]
pub(crate) struct CachedSessionChatScreenState {
    pub(crate) prompt: Option<crate::session_chat::SessionChatInteractivePrompt>,
    pub(crate) notice: Option<crate::session_chat_notice::SessionChatTerminalNotice>,
    pub(crate) activity: Option<crate::session_chat_terminal_activity::SessionChatTerminalActivity>,
    pub(crate) fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    pub(crate) tasks: Option<crate::session_chat_agent_tasks::SessionChatAgentTasks>,
    /// CDXC:AgentScreenDetection 2026-08-22: whether the cache entry these
    /// came from was backed by a whole capture at all.
    pub(crate) probed: bool,
}

impl CachedSessionChatScreenState {
    pub(crate) fn borrow(&self) -> crate::session_chat::SessionChatScreenState<'_> {
        crate::session_chat::SessionChatScreenState {
            prompt: self.prompt.as_ref(),
            notice: self.notice.as_ref(),
            activity: self.activity.as_ref(),
            fleet: self.fleet.as_ref(),
            tasks: self.tasks.as_ref(),
            probed: self.probed,
        }
    }
}

pub(crate) fn cached_session_chat_screen_state(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> CachedSessionChatScreenState {
    let (prompt, screen_notice, activity, fleet, tasks, probed) = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .get(&session_observer_key(project_id, session_id))
                .map(|entry| {
                    (
                        entry.value.prompt.clone(),
                        entry.value.notice.clone(),
                        entry.value.activity.clone(),
                        entry.value.fleet.clone(),
                        entry.value.tasks.clone(),
                        entry.value.attempted,
                    )
                })
        })
        .unwrap_or_default();
    CachedSessionChatScreenState {
        prompt,
        notice: crate::session_chat_notice::resolve_session_chat_terminal_notice(
            project_id,
            session_id,
            screen_notice,
        ),
        activity,
        fleet,
        tasks,
        probed,
    }
}

/*
CDXC:SessionChat 2026-08-26:
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
CDXC:AgentScreenDetection 2026-08-19:
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
        let (options, prompt, screen_notice, activity, fleet, tasks, captured) = option_cache
            .lock()
            .ok()
            .and_then(|cache| {
                cache.get(&key).map(|entry| {
                    (
                        entry.value.options.clone(),
                        entry.value.prompt.clone(),
                        entry.value.notice.clone(),
                        entry.value.activity.clone(),
                        entry.value.fleet.clone(),
                        entry.value.tasks.clone(),
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
                prompt: prompt.as_ref(),
                notice: notice.as_ref(),
                activity: activity.as_ref(),
                fleet: fleet.as_ref(),
                tasks: tasks.as_ref(),
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

/// Drops EVERY reading taken off this session's screen, not just the option
/// pills: one cache entry holds the detected options, the terminal notice, the
/// terminal activity, the fleet state and the composer-readiness verdict, and
/// they are all readings of the same capture, so they all go stale together.
/// Callers that need to invalidate only the readiness (there are none — a
/// screen whose composer verdict is worthless has worthless pills too) would be
/// splitting an entry that is only ever written whole.
pub(crate) fn forget_session_chat_options(state: &AppState, project_id: &str, session_id: &str) {
    if let Ok(mut cache) = state.session_chat_option_cache.lock() {
        cache.remove(&session_observer_key(project_id, session_id));
    }
    // Sleeping/stopped sessions have no live screen. Clear the durable
    // compaction projection with the cache so waking cannot resurrect an old
    // working status from a run that ended while the provider was stopped.
    crate::session_chat_compacting::SessionChatCompactingPublisher::new(state)
        .publish(project_id, session_id, None);
}

/*
CDXC:AgentScreenDetection 2026-08-01:
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
        // CDXC:AgentScreenDetection 2026-08-19: this probe re-reads the
        // screen anyway, so a notice that appeared or cleared with the dispatch
        // rides the same frame instead of waiting for the ~30s follower probe.
        let mut published_notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
            &project_id,
            &session_id,
            cached.notice,
        );
        let mut published_activity = cached.activity;
        let mut published_fleet = cached.fleet;
        let mut published_tasks = cached.tasks;
        let mut published_prompt = cached.prompt;
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
            // Disk-backed, so no capture gate: the store is authoritative on its own.
            let tasks_changed = !crate::session_chat_agent_tasks::same_session_chat_agent_tasks(
                detection.tasks.as_ref(),
                published_tasks.as_ref(),
            );
            let prompt_changed = detection.captured && detection.prompt != published_prompt;
            if !options_changed
                && !notice_changed
                && !activity_changed
                && !fleet_changed
                && !tasks_changed
                && !prompt_changed
            {
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
            if tasks_changed {
                published_tasks = detection.tasks;
            }
            if prompt_changed {
                published_prompt = detection.prompt;
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
                    prompt: published_prompt.as_ref(),
                    notice: published_notice.as_ref(),
                    activity: published_activity.as_ref(),
                    fleet: published_fleet.as_ref(),
                    tasks: published_tasks.as_ref(),
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
    // (CDXC:AgentScreenDetection 2026-08-24).
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
