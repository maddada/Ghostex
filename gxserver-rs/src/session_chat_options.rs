/*
CDXC:SessionChatDetectedOptions 2026-08-01:
Reads the CURRENT model / reasoning effort from agent-owned structured
transcript metadata and the session's terminal scrollback, so the composer's
option pills show evidence instead of a catalog guess.

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

use crate::domain::DomainRepository;

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

/// A newly followed agent may paint its model/effort footer just after the
/// chat's seed read. Re-detect on each of the first ten 1s reconciles until
/// both values are present instead of leaving a cached startup miss visible.
pub const SESSION_CHAT_OPTION_STARTUP_RECONCILE_TICKS: u64 = 10;

// ---------------------------------------------------------------------------
// Result types (mirror of shared/session-chat.ts SessionChatDetectedOptions)
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
    pub notice: Option<crate::session_chat_notice::SessionChatTerminalNotice>,
    /// True when a usable (non-truncated) screen backed this detection. It is
    /// the ONLY case where `notice: None` means "the screen is clean" — a failed
    /// or capped capture must never retire a notice.
    pub captured: bool,
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

/// Agents whose statusline grammar is known. Grok has no pill catalog on the
/// client and no captured sample, so it detects nothing (structural no-op).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatOptionAgent {
    Claude,
    Codex,
}

pub fn session_chat_option_agent(agent: Option<&str>) -> Option<SessionChatOptionAgent> {
    match agent.map(str::trim).unwrap_or_default() {
        "claude" | "openclaude" => Some(SessionChatOptionAgent::Claude),
        "codex" => Some(SessionChatOptionAgent::Codex),
        _ => None,
    }
}

/// Slash commands whose dispatch can change what the statusline reports. Mirrors
/// `sessionChatOptionCommandNames` in sidebar/chat/session-chat-session-options.ts.
pub fn is_session_chat_option_command_text(agent: Option<&str>, text: &str) -> bool {
    if session_chat_option_agent(agent).is_none() {
        return false;
    }
    let Some(first) = text.trim_start().split_whitespace().next() else {
        return false;
    };
    matches!(first, "/model" | "/effort" | "/fast")
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
/// sidebar/chat/session-chat-session-options.ts.
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
// Detection
// ---------------------------------------------------------------------------

/// Scans the tail window bottom-up; the bottom-most match wins. Returns `None`
/// when the window carries no statusline this parser understands.
pub fn detect_session_chat_selection(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatDetectedSelection> {
    let mut found = SessionChatDetectedSelection::default();
    for line in scan_window(text).iter().rev() {
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
                }
                SessionChatOptionAgent::Codex => {
                    if found.model.is_none() && found.effort.is_none() {
                        if let Some(selection) = match_codex_segment(segment) {
                            found = selection;
                        }
                    }
                }
            }
        }
        if found.model.is_some() && found.effort.is_some() {
            break;
        }
    }
    (found.model.is_some() || found.effort.is_some()).then_some(found)
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
                    fast: None,
                }
            }
            _ => continue,
        };
        if selection.model.is_some() || selection.effort.is_some() {
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
        if terminal.fast.is_some() {
            merged.fast = terminal.fast;
        }
    }
    (merged.model.is_some() || merged.effort.is_some()).then_some(merged)
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
    let Some(agent) = session_chat_option_agent(agent_id) else {
        return SessionChatTerminalDetection::default();
    };
    let transcript =
        read_session_chat_transcript_selection(repository, project_id, session_id, agent);
    let capture =
        crate::zmx::read_zmx_session_history_capture(repository, project_id, session_id).ok();
    let terminal = capture
        .as_ref()
        .and_then(|capture| detect_session_chat_selection(agent, &capture.text));
    // A capped capture lost its tail, so the live screen is not in it.
    let screen = capture.as_ref().filter(|capture| !capture.truncated);
    SessionChatTerminalDetection {
        options: merge_session_chat_option_selections(transcript, terminal)
            .map(SessionChatDetectedOptions::new),
        notice: screen.and_then(|capture| {
            crate::session_chat_notice::classify_session_chat_terminal_notice(
                agent_id,
                &capture.text,
            )
        }),
        captured: screen.is_some(),
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
        assert_eq!(session_chat_option_agent(Some("grok")), None);
        assert_eq!(session_chat_option_agent(Some("cursor")), None);
        assert_eq!(session_chat_option_agent(None), None);
        assert_eq!(
            session_chat_option_agent(Some("openclaude")),
            Some(SessionChatOptionAgent::Claude)
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
        assert!(!is_session_chat_option_command_text(
            Some("grok"),
            "/model opus"
        ));
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
