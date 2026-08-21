/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Chat is a transcript projection, so everything an agent TUI paints ONLY on the
screen is invisible in it: codex's expired-login banner, a workspace-trust
dialog, a usage-limit countdown, a stream error, the CLI having exited back to a
shell. A message typed into one of those screens is silently lost.

This module turns that screen state into one nullable wire field
(`terminalNotice`) carried exactly like `prompt`/`selectedOptions`. It is a PURE
classifier plus a tiny in-memory store: it never spawns a process and never
touches the filesystem. The screen text it classifies is the SAME `zmx history`
capture the model/effort detector already pays for (session_chat_options.rs), so
notices cost zero extra process spawns.

Matching is phrase-based rather than regex-based, for the same reason the option
grammar is hand-written: this crate deliberately carries no regex dependency
(it ships as a static musl binary to remote machines). The screen is folded to
single spaces and joined across lines first, so a wrapped TUI sentence matches
the same literal as an unwrapped one, and a `Gap` stands in for every variable
middle (`·` separators, version numbers, reasons).

Two match windows, per the researched catalog:
  - Banner-class states (an inline error/limit line, a crashed process) must
    appear in the LAST few non-blank lines — an identical line further up is
    scrollback from a problem that is already over.
  - Dialog-class states (trust, login, onboarding, update modal) own the visible
    screen, so they may appear anywhere in it. "The visible screen" is
    approximated by a wider tail window, NOT by the whole 256 KiB scrollback.

Wordings are versioned: remote machines run older agent builds, so every state
keeps a LIST of signatures ordered newest-first and the first match wins.
*/

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::session_chat_options::{
    normalize_spaces, session_chat_option_agent, strip_ansi_sgr, SessionChatOptionAgent,
};

// ---------------------------------------------------------------------------
// Wire contract (mirror of shared/session-chat.ts SessionChatTerminalNotice)
// ---------------------------------------------------------------------------

/// The agent cannot talk to its provider until the user signs in again.
pub const SESSION_CHAT_NOTICE_LOGIN_EXPIRED: &str = "loginExpired";
/// A workspace/directory trust dialog is blocking the composer.
pub const SESSION_CHAT_NOTICE_TRUST_PROMPT: &str = "trustPrompt";
/// A sibling blocking dialog about settings/permissions.
pub const SESSION_CHAT_NOTICE_PERMISSIONS_WARNING: &str = "permissionsWarning";
/// A first-run setup screen is blocking the composer.
pub const SESSION_CHAT_NOTICE_ONBOARDING: &str = "onboarding";
/// Usage/rate/credit limit reported on screen.
pub const SESSION_CHAT_NOTICE_USAGE_LIMIT: &str = "usageLimit";
/// Network/server failure reported on screen.
pub const SESSION_CHAT_NOTICE_STREAM_ERROR: &str = "streamError";
/// A blocking update dialog.
pub const SESSION_CHAT_NOTICE_UPDATE_PROMPT: &str = "updatePrompt";
/// The agent process appears to have exited back to a shell.
pub const SESSION_CHAT_NOTICE_AGENT_EXITED: &str = "agentExited";
/// Input accepted but held client-side until the running turn ends.
pub const SESSION_CHAT_NOTICE_QUEUED_INPUT: &str = "queuedInput";
/// The send watchdog could not prove a message reached the agent.
pub const SESSION_CHAT_NOTICE_DELIVERY_FAILED: &str = "deliveryFailed";
/// Claude Code's resume-usage picker: an on-screen chooser the chat surface can
/// ANSWER, not just point at. Its rows ride the notice as `choices`.
pub use crate::session_chat_resume_prompt::SESSION_CHAT_RESUME_PROMPT_KIND as SESSION_CHAT_NOTICE_RESUME_PROMPT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatTerminalNoticeSeverity {
    Error,
    Warning,
    Info,
}

impl SessionChatTerminalNoticeSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatTerminalNoticeSource {
    /// Classified from the session's terminal screen.
    Screen,
    /// Raised by the send-delivery watchdog.
    Watchdog,
}

impl SessionChatTerminalNoticeSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Watchdog => "watchdog",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatTerminalNoticeActionKind {
    /// Client-side: show the session's terminal instead of the chat.
    SwitchToTerminal,
    /// Verbatim bytes, delivered through the existing approval-answer path.
    SendKeys,
}

impl SessionChatTerminalNoticeActionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SwitchToTerminal => "switchToTerminal",
            Self::SendKeys => "sendKeys",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalNoticeAction {
    pub id: String,
    pub label: String,
    pub kind: SessionChatTerminalNoticeActionKind,
    /// Raw bytes for `SendKeys`; never set for `SwitchToTerminal`.
    pub send: Option<String>,
}

impl SessionChatTerminalNoticeAction {
    pub fn switch_to_terminal(label: &str) -> Self {
        Self {
            id: "switchToTerminal".to_string(),
            label: label.to_string(),
            kind: SessionChatTerminalNoticeActionKind::SwitchToTerminal,
            send: None,
        }
    }

    pub fn send_keys(id: &str, label: &str, send: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: SessionChatTerminalNoticeActionKind::SendKeys,
            send: Some(send.to_string()),
        }
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".to_string(), json!(self.id));
        map.insert("label".to_string(), json!(self.label));
        map.insert("kind".to_string(), json!(self.kind.as_str()));
        if let Some(send) = self.send.as_deref() {
            map.insert("send".to_string(), json!(send));
        }
        Value::Object(map)
    }
}

/*
CDXC:SessionChatTerminalPicker 2026-08-21:
Rows of an on-screen picker the chat surface can answer from here. A notice
that carries them is not just "go look at your terminal": the client renders
the same option rows the AskUserQuestion card uses and sends the pick back
through answerSessionChatPrompt's `terminalChoice` lane, which re-reads the
live screen and walks the highlight onto that row.

`selected` is where the TUI highlight sits AT DETECTION TIME. It is shown as
the TUI's own default, never used to compute keystrokes — the highlight can
move (the user arrows around in the terminal) between a detection and an
answer, so the answer path always re-derives it from a fresh capture.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalNoticeChoice {
    /// 0-based row index, which is what an answer addresses.
    pub index: usize,
    pub label: String,
    pub selected: bool,
}

impl SessionChatTerminalNoticeChoice {
    fn to_value(&self) -> Value {
        json!({
            "index": self.index,
            "label": self.label,
            "selected": self.selected,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalNotice {
    /// Open set: clients must render an unknown kind generically.
    pub kind: String,
    pub severity: SessionChatTerminalNoticeSeverity,
    pub title: String,
    pub detail: Option<String>,
    /// SGR-stripped last visible lines, for the card's collapsible evidence.
    pub screen_tail: Option<String>,
    pub source: SessionChatTerminalNoticeSource,
    /// RFC3339 millis; also the client's dismissal key.
    pub detected_at: String,
    pub actions: Vec<SessionChatTerminalNoticeAction>,
    /// Answerable picker rows, in screen order. Empty for every notice that
    /// only describes a state.
    pub choices: Vec<SessionChatTerminalNoticeChoice>,
}

impl SessionChatTerminalNotice {
    pub fn new(
        kind: &str,
        severity: SessionChatTerminalNoticeSeverity,
        source: SessionChatTerminalNoticeSource,
        title: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.to_string(),
            severity,
            title: title.into(),
            detail: None,
            screen_tail: None,
            source,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            actions: Vec::new(),
            choices: Vec::new(),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        self.detail = (!detail.trim().is_empty()).then_some(detail);
        self
    }

    pub fn with_screen_tail(mut self, screen_tail: Option<String>) -> Self {
        self.screen_tail = screen_tail.filter(|tail| !tail.trim().is_empty());
        self
    }

    pub fn with_actions(mut self, actions: Vec<SessionChatTerminalNoticeAction>) -> Self {
        self.actions = actions;
        self
    }

    pub fn with_choices(mut self, choices: Vec<SessionChatTerminalNoticeChoice>) -> Self {
        self.choices = choices;
        self
    }

    /// True when this notice offers rows the chat surface can answer itself.
    pub fn is_answerable(&self) -> bool {
        !self.choices.is_empty()
    }

    /// True when two detections say the SAME thing. `detectedAt` and the raw
    /// screen tail are ignored on purpose: a re-detect every probe must not
    /// emit a frame (and must not churn the long-poll fingerprint) while the
    /// screen keeps showing the same state.
    pub fn same_notice(&self, other: Option<&SessionChatTerminalNotice>) -> bool {
        other.is_some_and(|other| {
            self.kind == other.kind
                && self.severity == other.severity
                && self.title == other.title
                && self.detail == other.detail
                && self.source == other.source
                && self.actions == other.actions
                // Labels only: the highlight moves whenever the user arrows
                // around in the terminal, and re-minting `detectedAt` for that
                // would resurrect a card they just dismissed.
                && choice_labels(&self.choices) == choice_labels(&other.choices)
        })
    }

    /*
    CDXC:SessionChatTerminalNotices 2026-08-19:
    A notice is an INSTANCE, not a sample. The screen keeps saying the same
    thing for as long as the state lasts, so every probe re-classifies it — but
    the client keys its local dismissal on `kind` + `detectedAt`, and the state
    frames are emitted by two independent publishers (the follower's probe and
    the hook-driven prompt-state path). Minting a fresh timestamp per
    classification therefore both resurrected dismissed cards within seconds and
    made the two publishers flip-flop between timestamps for one unchanged
    state. Whenever a new classification says the same thing as the one it
    replaces, it inherits that instance's `detectedAt`; a genuinely new state
    gets its own.
    */
    pub fn carry_forward_detected_at(&mut self, previous: Option<&SessionChatTerminalNotice>) {
        if self.same_notice(previous) {
            if let Some(previous) = previous {
                self.detected_at = previous.detected_at.clone();
            }
        }
    }

    /// True when a message delivered while this notice is showing would not
    /// reach the model — see `session_chat_notice_kind_blocks_input`. This is
    /// NOT `severity == Error`; the two axes disagree in both directions.
    pub fn blocks_input(&self) -> bool {
        session_chat_notice_kind_blocks_input(&self.kind)
    }

    /// Stable identity for the long-poll fingerprint: kind plus the human text.
    /// Never includes `detectedAt` or the screen tail.
    pub fn identity(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            self.kind,
            self.title,
            self.detail.as_deref().unwrap_or_default()
        )
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("kind".to_string(), json!(self.kind));
        map.insert("severity".to_string(), json!(self.severity.as_str()));
        map.insert("title".to_string(), json!(self.title));
        if let Some(detail) = self.detail.as_deref() {
            map.insert("detail".to_string(), json!(detail));
        }
        if let Some(screen_tail) = self.screen_tail.as_deref() {
            map.insert("screenTail".to_string(), json!(screen_tail));
        }
        map.insert("source".to_string(), json!(self.source.as_str()));
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        if !self.choices.is_empty() {
            map.insert(
                "choices".to_string(),
                Value::Array(
                    self.choices
                        .iter()
                        .map(SessionChatTerminalNoticeChoice::to_value)
                        .collect(),
                ),
            );
        }
        if !self.actions.is_empty() {
            map.insert(
                "actions".to_string(),
                Value::Array(
                    self.actions
                        .iter()
                        .map(SessionChatTerminalNoticeAction::to_value)
                        .collect(),
                ),
            );
        }
        Value::Object(map)
    }
}

fn choice_labels(choices: &[SessionChatTerminalNoticeChoice]) -> Vec<&str> {
    choices.iter().map(|choice| choice.label.as_str()).collect()
}

/// Change test for a notice that can also disappear. Both absent ⇒ unchanged;
/// present→absent ⇒ changed, because clients treat an omitted field on a state
/// frame as "cleared".
pub fn same_session_chat_terminal_notice(
    current: Option<&SessionChatTerminalNotice>,
    published: Option<&SessionChatTerminalNotice>,
) -> bool {
    match current {
        Some(current) => current.same_notice(published),
        None => published.is_none(),
    }
}

// ---------------------------------------------------------------------------
// Screen preparation
// ---------------------------------------------------------------------------

/// Banner-class window: an inline error/limit line only counts while it is
/// still on the live screen.
const NOTICE_BANNER_SCAN_LINES: usize = 15;
/// Dialog-class window: roughly one visible screen, never the whole scrollback.
const NOTICE_DIALOG_SCAN_LINES: usize = 60;
/// Exit signatures must be at the very bottom, right above the shell prompt.
const NOTICE_EXIT_SCAN_LINES: usize = 10;
/// Evidence attached to a notice.
const NOTICE_SCREEN_TAIL_LINES: usize = 12;
const NOTICE_SCREEN_TAIL_MAX_CHARS: usize = 2000;
const NOTICE_EVIDENCE_MAX_CHARS: usize = 240;

/// One prepared capture: parallel display/folded views of the same tail lines,
/// oldest first.
struct NoticeScreen {
    /// ANSI-stripped, whitespace-folded, trailing space removed — what the user
    /// sees, used for `screenTail` and quoted evidence.
    display: Vec<String>,
    /// Additionally space-collapsed and apostrophe-folded — what patterns run
    /// against.
    folded: Vec<String>,
    /// Flattened windows (`folded` joined by one space) for wrap tolerance.
    banner: String,
    dialog: String,
}

/// Claude renders `’`, codex renders `'`; both mean the same word.
fn fold_typographic(line: &str) -> String {
    line.chars()
        .map(|ch| match ch {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            other => other,
        })
        .collect()
}

/// Collapses the space runs that folding NBSP/tabs produces, so one literal
/// matches however the TUI padded the line.
fn collapse_spaces(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut pending_space = false;
    for ch in line.chars() {
        if ch == ' ' {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

impl NoticeScreen {
    fn new(text: &str) -> Self {
        let mut display: Vec<String> = Vec::new();
        let mut folded: Vec<String> = Vec::new();
        for raw in text.lines().rev() {
            let line = normalize_spaces(&strip_ansi_sgr(raw))
                .trim_end()
                .to_string();
            if line.trim().is_empty() {
                continue;
            }
            folded.push(collapse_spaces(&fold_typographic(&line)));
            display.push(line);
            if display.len() >= NOTICE_DIALOG_SCAN_LINES {
                break;
            }
        }
        display.reverse();
        folded.reverse();
        let banner = flatten_tail(&folded, NOTICE_BANNER_SCAN_LINES);
        let dialog = flatten_tail(&folded, NOTICE_DIALOG_SCAN_LINES);
        Self {
            display,
            folded,
            banner,
            dialog,
        }
    }

    fn window(&self, scope: NoticeScope) -> &str {
        match scope {
            NoticeScope::Banner => &self.banner,
            NoticeScope::Dialog => &self.dialog,
            NoticeScope::Exit => &self.banner,
        }
    }

    /// The newest displayed line carrying `needle`, capped. Absent when the
    /// phrase only matched across a wrap.
    fn evidence(&self, needle: &str) -> Option<String> {
        let index = self.folded.iter().rposition(|line| line.contains(needle))?;
        let line = self.display.get(index)?.trim();
        (!line.is_empty()).then(|| cap_chars_from_end(line, NOTICE_EVIDENCE_MAX_CHARS))
    }

    /*
    CDXC:SessionChatTerminalNotices 2026-08-19:
    Agent TUIs frame their footers with full-width rules that carry no
    information but eat the whole card width. A rule row becomes a blank line —
    the break it was drawing is kept, the wall of glyphs is not — and it does
    not spend the content budget, so the tail still carries
    `NOTICE_SCREEN_TAIL_LINES` real lines.
    */
    fn screen_tail(&self) -> Option<String> {
        let mut newest_first: Vec<&str> = Vec::new();
        let mut content_lines = 0usize;
        for line in self.display.iter().rev() {
            if content_lines >= NOTICE_SCREEN_TAIL_LINES {
                break;
            }
            let line = line.trim_end();
            if is_decoration_line(line) {
                newest_first.push("");
                continue;
            }
            content_lines += 1;
            newest_first.push(line);
        }
        let mut tail_lines: Vec<&str> = Vec::new();
        for line in newest_first.into_iter().rev() {
            // Collapse runs of blanks (original or rule-produced) and drop the
            // leading ones outright.
            if line.is_empty() && tail_lines.last().is_none_or(|last| last.is_empty()) {
                continue;
            }
            tail_lines.push(line);
        }
        while tail_lines.last().is_some_and(|line| line.is_empty()) {
            tail_lines.pop();
        }
        let tail = tail_lines.join("\n");
        (!tail.trim().is_empty()).then(|| cap_chars_from_end(&tail, NOTICE_SCREEN_TAIL_MAX_CHARS))
    }

    /// A bare shell prompt as the bottom line is what distinguishes "the agent
    /// exited" from "the agent printed something that looks like an exit".
    fn ends_with_shell_prompt(&self) -> bool {
        let Some(line) = self.folded.last().map(|line| line.trim()) else {
            return false;
        };
        // `›` is codex's own composer marker: an alive TUI, never a shell.
        if line.starts_with('\u{203a}') || line.len() > 200 {
            return false;
        }
        matches!(
            line.chars().last(),
            Some('$') | Some('%') | Some('#') | Some('\u{276f}') | Some('\u{279c}')
        )
    }
}

fn flatten_tail(folded: &[String], lines: usize) -> String {
    let start = folded.len().saturating_sub(lines);
    folded[start..].join(" ")
}

/// The ASCII/typographic rule characters TUIs pad separators with. Box drawing
/// (U+2500–U+257F) and block elements (U+2580–U+259F) are one contiguous range,
/// so they are matched separately.
const NOTICE_DECORATION_CHARS: &[char] = &[
    '-', '=', '_', '*', '~', '#', '+', '.', '\u{00b7}', '\u{2022}', '\u{2014}', '\u{2013}',
];

fn is_decoration_char(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{259f}') || NOTICE_DECORATION_CHARS.contains(&ch)
}

/// A separator row: non-empty, and every non-whitespace character is
/// decoration. `──── session ──` mixes in real text, so it is not one.
fn is_decoration_line(line: &str) -> bool {
    let mut saw_decoration = false;
    for ch in line.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if !is_decoration_char(ch) {
            return false;
        }
        saw_decoration = true;
    }
    saw_decoration
}

/// Keeps the NEWEST characters: the bottom of the screen is the evidence.
fn cap_chars_from_end(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    text.chars().skip(count - max_chars).collect()
}

// ---------------------------------------------------------------------------
// Phrase matcher (no regex dependency — see the module header)
// ---------------------------------------------------------------------------

enum NoticePart {
    /// Literal, matched against the space-collapsed window.
    Text(&'static str),
    /// Up to N arbitrary characters: separators, names, reasons, wrap padding.
    Gap(usize),
    /// One ASCII digit (versions, retry counters).
    Digit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoticeScope {
    /// Last few non-blank lines only.
    Banner,
    /// Anywhere on the visible screen.
    Dialog,
    /// Last few lines AND a shell prompt at the bottom.
    Exit,
}

struct NoticeSignature {
    scope: NoticeScope,
    /// Always starts with a `Text` part: that literal seeds the scan.
    parts: &'static [NoticePart],
    /// Any-of literals that must also be in the same window. Empty ⇒ the
    /// phrase stands alone.
    corroborators: &'static [&'static str],
}

fn matches_from(hay: &str, at: usize, parts: &[NoticePart]) -> bool {
    let Some((part, rest)) = parts.split_first() else {
        return true;
    };
    match part {
        NoticePart::Text(text) => {
            hay[at..].starts_with(text) && matches_from(hay, at + text.len(), rest)
        }
        NoticePart::Digit => {
            hay[at..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit())
                && matches_from(hay, at + 1, rest)
        }
        NoticePart::Gap(max) => {
            let mut cursor = at;
            for _ in 0..=*max {
                if matches_from(hay, cursor, rest) {
                    return true;
                }
                let Some(ch) = hay[cursor..].chars().next() else {
                    return false;
                };
                cursor += ch.len_utf8();
            }
            false
        }
    }
}

fn matches_parts(hay: &str, parts: &[NoticePart]) -> bool {
    let Some(NoticePart::Text(first)) = parts.first() else {
        return false;
    };
    hay.match_indices(first)
        .any(|(index, _)| matches_from(hay, index + first.len(), &parts[1..]))
}

/// The signature's leading literal, used to quote the line it matched on.
fn signature_needle(signature: &NoticeSignature) -> Option<&'static str> {
    match signature.parts.first() {
        Some(NoticePart::Text(text)) => Some(text),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

struct NoticeActionSpec {
    id: &'static str,
    label: &'static str,
    kind: SessionChatTerminalNoticeActionKind,
    send: Option<&'static str>,
}

const OPEN_TERMINAL: NoticeActionSpec = NoticeActionSpec {
    id: "switchToTerminal",
    label: "Open terminal",
    kind: SessionChatTerminalNoticeActionKind::SwitchToTerminal,
    send: None,
};

struct NoticeRule {
    kind: &'static str,
    severity: SessionChatTerminalNoticeSeverity,
    title: &'static str,
    detail: &'static str,
    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    Severity and "blocks input" are DIFFERENT axes and must not be collapsed.
    Severity says how alarming the card looks; this says whether a message
    delivered while the state is up actually reaches the model. A trust dialog
    is only a `Warning` — the user is one keypress from continuing — yet
    anything typed into it is eaten as the ANSWER to it, which is worse for an
    automated sender than a loud `Error` banner the composer still works
    behind.

    True ⇒ a message sent now does not reach the model: it is consumed by an
    on-screen dialog, swallowed by a CLI that cannot talk to its provider, or
    typed at a shell where the agent used to be. Anything that only makes a
    turn *fail loudly* (a transient stream error the CLI retries) stays false:
    holding there would stall the queue on a state that heals itself.
    */
    blocks_input: bool,
    /// Ordered newest-wording-first; the first match wins.
    signatures: &'static [NoticeSignature],
    actions: &'static [NoticeActionSpec],
    /// Append the matched line to `detail` — limits carry their reset time and
    /// stream errors carry the server's own words.
    quote_evidence: bool,
}

// --- codex ------------------------------------------------------------------

const CODEX_RULES: &[NoticeRule] = &[
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Codex login expired",
        detail: "Codex needs a fresh login on this machine. Open the terminal and run /login — the sign-in flow is interactive, so it cannot be answered from chat.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Your access token could not be refreshed"),
                    NoticePart::Gap(160),
                    NoticePart::Text("sign in again"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Login expired"),
                    NoticePart::Gap(40),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Sign in with ChatGPT to use Codex")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Finish signing in via your browser")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Provide your own API key")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_TRUST_PROMPT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Codex is waiting for directory trust",
        detail: "Codex asks whether to trust this folder before it will run anything here. Nothing you send reaches the agent until it is answered.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Dialog,
            parts: &[NoticePart::Text(
                "Do you trust the contents of this directory?",
            )],
            corroborators: &["Yes, continue", "No, quit"],
        }],
        // The trust screen commits on the digit, so "1" is unambiguous here
        // (claude's equivalent dialog is not — see CLAUDE_RULES).
        actions: &[
            NoticeActionSpec {
                id: "trustDirectory",
                label: "Trust and continue",
                kind: SessionChatTerminalNoticeActionKind::SendKeys,
                send: Some("1"),
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_AGENT_EXITED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Codex is no longer running in this terminal",
        detail: "The codex process appears to have exited in this terminal. Messages sent from chat cannot reach it until it is started again.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[NoticePart::Text("To continue this session, run codex resume")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Exit,
                parts: &[
                    NoticePart::Text("thread '"),
                    NoticePart::Gap(80),
                    NoticePart::Text("panicked at"),
                ],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_USAGE_LIMIT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Codex reported a usage limit",
        detail: "Codex says it is out of quota, so new turns will fail until it resets.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("hit your usage limit")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("hit your spend cap")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Your workspace is out of credits")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Quota exceeded. Check your plan")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_STREAM_ERROR,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Codex hit a network or server error",
        detail: "Codex reported a transport failure on screen. The turn may need to be retried.",
        // The only `false` in the catalog. The composer still accepts input and
        // codex retries the transport itself, so a message sent now DOES reach
        // the model once the connection comes back. Holding here would stall a
        // queue on a state that heals without the user, which is the failure
        // mode of over-widening this predicate.
        blocks_input: false,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Reconnecting... waiting for network")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Reconnecting..."),
                    NoticePart::Gap(1),
                    NoticePart::Digit,
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Error while reading the server response")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("exceeded retry limit, last status")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("unexpected status "),
                    NoticePart::Digit,
                    NoticePart::Digit,
                    NoticePart::Digit,
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Connection failed")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text(
                    "We're currently experiencing high demand",
                )],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("Selected model is at capacity")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_UPDATE_PROMPT,
        severity: SessionChatTerminalNoticeSeverity::Info,
        title: "Codex is showing an update prompt",
        detail: "An update dialog is on screen. It blocks the composer until it is answered.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[
                    NoticePart::Text("Update available!"),
                    NoticePart::Gap(1),
                    NoticePart::Digit,
                ],
                // Only the blocking MODAL warrants a notice; the harmless
                // in-history box carries no skip choice.
                corroborators: &["Skip until next version"],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text(
                    "This version will no longer be supported",
                )],
                corroborators: &[],
            },
        ],
        actions: &[
            NoticeActionSpec {
                id: "skipUpdate",
                label: "Skip for now",
                kind: SessionChatTerminalNoticeActionKind::SendKeys,
                send: Some("2"),
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
];

// --- claude / openclaude ----------------------------------------------------

const CLAUDE_RULES: &[NoticeRule] = &[
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Claude Code login expired",
        detail: "Claude Code needs a fresh login on this machine — open the terminal and run /login.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Not logged in"),
                    NoticePart::Gap(24),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Login expired"),
                    NoticePart::Gap(24),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("OAuth token revoked"),
                    NoticePart::Gap(24),
                    NoticePart::Text("/login"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("API Error: 401")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Select login method:")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Claude account with subscription")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Paste code here if prompted")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text(
                    "Run in another terminal: security unlock-keychain",
                )],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_TRUST_PROMPT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Claude Code is waiting for folder trust",
        detail: "Claude Code is showing its workspace-trust dialog and accepts nothing until it is answered. Which option is focused differs between versions, so answer it in the terminal rather than blind-pressing Enter.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Accessing workspace:")],
                corroborators: &["Quick safety check", "Yes, I trust this folder"],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Do you trust the files in this folder?")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[
                    NoticePart::Text("hasn't been trusted yet"),
                    NoticePart::Gap(200),
                    NoticePart::Text("Trusting allows Claude to read and execute files"),
                ],
                corroborators: &[],
            },
        ],
        // Deliberately no sendKeys: the mid-session variant focuses CANCEL, so
        // a blind Enter would decline.
        actions: &[OPEN_TERMINAL],
        quote_evidence: false,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_PERMISSIONS_WARNING,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Claude Code is waiting on a permissions dialog",
        detail: "Claude Code is showing a settings/permissions dialog that blocks its composer. Answer it in the terminal.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text("Managed settings require approval")],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Dialog,
                parts: &[NoticePart::Text(
                    "WARNING: Claude Code running in Bypass Permissions mode",
                )],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: false,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_AGENT_EXITED,
        severity: SessionChatTerminalNoticeSeverity::Error,
        title: "Claude Code stopped with an error",
        detail: "Claude Code printed a fatal error in this terminal. Check it there before sending more messages.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Banner,
            parts: &[
                NoticePart::Text("Sorry, Claude"),
                NoticePart::Gap(6),
                NoticePart::Text("encountered an error"),
            ],
            corroborators: &[],
        }],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_USAGE_LIMIT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Claude Code is waiting to continue",
        detail: "The usage limit has reset and Claude Code is waiting for a keypress before it resumes.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Banner,
            parts: &[
                NoticePart::Text("Usage limit has reset"),
                NoticePart::Gap(24),
                NoticePart::Text("press enter to continue"),
            ],
            corroborators: &[],
        }],
        actions: &[
            NoticeActionSpec {
                id: "continueNow",
                label: "Continue now",
                kind: SessionChatTerminalNoticeActionKind::SendKeys,
                send: Some("\r"),
            },
            OPEN_TERMINAL,
        ],
        quote_evidence: false,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_USAGE_LIMIT,
        severity: SessionChatTerminalNoticeSeverity::Warning,
        title: "Claude Code hit a usage limit",
        detail: "Claude Code reported a usage limit on screen.",
        blocks_input: true,
        signatures: &[
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Usage limit reached"),
                    NoticePart::Gap(60),
                    NoticePart::Text("continuing automatically"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("Automatic continue "),
                    NoticePart::Gap(4),
                    NoticePart::Text("turned off"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("You've hit your"),
                    NoticePart::Gap(40),
                    NoticePart::Text("limit"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[
                    NoticePart::Text("You've reached your"),
                    NoticePart::Gap(40),
                    NoticePart::Text("limit"),
                ],
                corroborators: &[],
            },
            NoticeSignature {
                scope: NoticeScope::Banner,
                parts: &[NoticePart::Text("You're out of usage credits")],
                corroborators: &[],
            },
        ],
        actions: &[OPEN_TERMINAL],
        quote_evidence: true,
    },
    NoticeRule {
        kind: SESSION_CHAT_NOTICE_ONBOARDING,
        severity: SessionChatTerminalNoticeSeverity::Info,
        title: "Claude Code is in first-run setup",
        detail: "Claude Code is showing a first-run setup screen, which blocks its composer until it is finished in the terminal.",
        blocks_input: true,
        signatures: &[NoticeSignature {
            scope: NoticeScope::Dialog,
            parts: &[NoticePart::Text(
                "Choose the text style that looks best with your terminal",
            )],
            corroborators: &[],
        }],
        actions: &[OPEN_TERMINAL],
        quote_evidence: false,
    },
];

fn notice_rules(agent: SessionChatOptionAgent) -> &'static [NoticeRule] {
    match agent {
        SessionChatOptionAgent::Claude => CLAUDE_RULES,
        SessionChatOptionAgent::Codex => CODEX_RULES,
    }
}

/// Every catalog, for the kind-level queries below. Adding an agent's rules
/// here is the only step needed to teach the predicate about it.
const ALL_NOTICE_RULES: &[&[NoticeRule]] = &[CODEX_RULES, CLAUDE_RULES];

/*
CDXC:SessionChatPromptQueue 2026-08-21:
Whether a message delivered right now would actually reach the model, DERIVED
from the catalog above rather than restated as a second list of kind strings
that would drift the first time a rule is added. Automated senders — the chat
prompt queue's scheduler is the first — must gate on this, never on severity:
a `Warning` trust dialog eats what it is sent, while an `Error` stream banner
does not.

Kind-level rather than rule-level because the wire notice only carries its
kind, and because two agents' rules for the same kind describe the same state.
When they ever disagree, ANY blocking rule wins: holding costs a visible failed
row the user can retry, delivering into a dialog costs the turn.

The two watchdog-only kinds have no catalog rule and are answered here:
  - `deliveryFailed` — the watchdog could not prove the LAST message arrived,
    so the terminal has already demonstrated it is not accepting sends.
  - `queuedInput` — the opposite: the CLI accepted the message and is holding
    it client-side. Nothing is lost, so failing a row for it would be a false
    alarm. The scheduler's own idle gate is what keeps it from piling on.
An unknown kind is not blocking: it can only come from a newer peer, and this
predicate runs on notices this daemon classified itself.
*/
pub fn session_chat_notice_kind_blocks_input(kind: &str) -> bool {
    match kind {
        SESSION_CHAT_NOTICE_DELIVERY_FAILED => true,
        SESSION_CHAT_NOTICE_QUEUED_INPUT => false,
        /*
        CDXC:SessionChatTerminalPicker 2026-08-21: the resume-usage picker owns
        the input line, and unlike the dialogs in the catalog it does not merely
        swallow a message — its trailing Enter CONFIRMS a row. A send delivered
        into it silently compacts the conversation the user was continuing, so
        it blocks harder than anything else here. It has no catalog rule because
        its rows are read off the screen rather than declared.
        */
        SESSION_CHAT_NOTICE_RESUME_PROMPT => true,
        _ => ALL_NOTICE_RULES
            .iter()
            .flat_map(|rules| rules.iter())
            .any(|rule| rule.kind == kind && rule.blocks_input),
    }
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

fn signature_matches(screen: &NoticeScreen, signature: &NoticeSignature) -> bool {
    if signature.scope == NoticeScope::Exit && !screen.ends_with_shell_prompt() {
        return false;
    }
    let window = screen.window(signature.scope);
    if signature.scope == NoticeScope::Exit {
        // Exit signatures live in an even tighter window than banners.
        let exit_window = flatten_tail(&screen.folded, NOTICE_EXIT_SCAN_LINES);
        if !matches_parts(&exit_window, signature.parts) {
            return false;
        }
    } else if !matches_parts(window, signature.parts) {
        return false;
    }
    signature.corroborators.is_empty()
        || signature
            .corroborators
            .iter()
            .any(|corroborator| window.contains(corroborator))
}

fn notice_from_rule(
    screen: &NoticeScreen,
    rule: &NoticeRule,
    signature: &NoticeSignature,
) -> SessionChatTerminalNotice {
    let evidence = rule
        .quote_evidence
        .then(|| signature_needle(signature).and_then(|needle| screen.evidence(needle)))
        .flatten();
    let detail = match evidence {
        Some(evidence) => format!("{} Terminal: \"{}\"", rule.detail, evidence),
        None => rule.detail.to_string(),
    };
    SessionChatTerminalNotice::new(
        rule.kind,
        rule.severity,
        SessionChatTerminalNoticeSource::Screen,
        rule.title,
    )
    .with_detail(detail)
    .with_screen_tail(screen.screen_tail())
    .with_actions(
        rule.actions
            .iter()
            .map(|action| SessionChatTerminalNoticeAction {
                id: action.id.to_string(),
                label: action.label.to_string(),
                kind: action.kind,
                send: action.send.map(str::to_string),
            })
            .collect(),
    )
}

/*
CDXC:SessionChatTerminalPicker 2026-08-21:
The picker as a notice. `detail` is Claude's OWN prose (the session's age and
token count, and its usage-limit recommendation) because that is the entire
basis for the choice — restating it in our words would drop the numbers the
user decides on. The switch-to-terminal action stays as the escape hatch for a
row this build cannot drive.
*/
fn notice_from_picker(
    screen: &NoticeScreen,
    picker: crate::session_chat_resume_prompt::SessionChatTerminalPicker,
) -> SessionChatTerminalNotice {
    const PICKER_GUIDANCE: &str =
        "Claude Code accepts no input until this is answered — pick an option to answer it here.";
    let detail = match picker.detail.as_deref() {
        Some(prose) => format!("{prose} {PICKER_GUIDANCE}"),
        None => PICKER_GUIDANCE.to_string(),
    };
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_RESUME_PROMPT,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        "Claude Code is asking how to resume this session",
    )
    .with_detail(detail)
    .with_screen_tail(screen.screen_tail())
    .with_choices(
        picker
            .rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| SessionChatTerminalNoticeChoice {
                index,
                label: row.label,
                selected: row.selected,
            })
            .collect(),
    )
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Pure classifier over ONE terminal capture. Rules are evaluated in the catalog's
precedence order (login > trust > permissions > exited > usage > stream >
update > onboarding), so the most blocking truth wins when a screen shows two.
`None` means "this screen is clean" — which is also what retires a notice, so
the tail windows above must stay tight enough that stale scrollback never keeps
one alive.
*/
pub fn classify_session_chat_terminal_notice(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatTerminalNotice> {
    let agent = session_chat_option_agent(agent)?;
    let screen = NoticeScreen::new(screen_text);
    if screen.folded.is_empty() {
        return None;
    }
    /*
    CDXC:SessionChatTerminalPicker 2026-08-21:
    The resume-usage picker outranks the whole catalog below it. Every rule
    there can only say "answer this in your terminal"; this one carries the
    rows, so the user answers it from the chat surface they are already on.
    Claude Code is the only CLI that paints it.
    */
    if agent == SessionChatOptionAgent::Claude {
        if let Some(picker) =
            crate::session_chat_resume_prompt::detect_session_chat_terminal_picker(screen_text)
        {
            return Some(notice_from_picker(&screen, picker));
        }
    }
    for rule in notice_rules(agent) {
        if let Some(signature) = rule
            .signatures
            .iter()
            .find(|signature| signature_matches(&screen, signature))
        {
            return Some(notice_from_rule(&screen, rule, signature));
        }
    }
    None
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Codex queues input typed while a turn runs CLIENT-SIDE: nothing is written to
the rollout until the turn ends. The send watchdog must consult this before it
declares a message undelivered, which is why the state is exposed as a
predicate instead of as a user-facing notice.
*/
pub fn session_chat_screen_shows_queued_input(agent: Option<&str>, screen_text: &str) -> bool {
    if session_chat_option_agent(agent) != Some(SessionChatOptionAgent::Codex) {
        return false;
    }
    let screen = NoticeScreen::new(screen_text);
    screen.dialog.contains("Queued follow-up inputs")
        || screen.dialog.contains("Queued followup inputs")
}

/// The trimmed screen tail a watchdog notice attaches as evidence.
pub fn session_chat_terminal_screen_tail(screen_text: &str) -> Option<String> {
    NoticeScreen::new(screen_text).screen_tail()
}

// ---------------------------------------------------------------------------
// Watchdog notice store (in memory only — never persisted, never in settings)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Watchdog notices (a send that could not be proven delivered) live here rather
than in the session registry: they describe a moment, not durable state, and a
daemon restart must not resurrect one. Keyed exactly like the send queues so
both halves of the feature address a session the same way.
*/
struct StoredWatchdogNotice {
    notice: SessionChatTerminalNotice,
    stored_at: Instant,
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Retirement backstop. A watchdog notice is normally retired by the next send or
by a later verification, but a session nobody touches again would otherwise keep
one forever — and "your message from an hour ago never arrived" is noise, not
news. Expiry is checked lazily on read (there is no sweeper task), so an expired
entry is indistinguishable from an absent one for every consumer.
*/
const WATCHDOG_NOTICE_MAX_AGE: Duration = Duration::from_secs(600);

fn watchdog_notices() -> &'static Mutex<HashMap<String, StoredWatchdogNotice>> {
    static NOTICES: OnceLock<Mutex<HashMap<String, StoredWatchdogNotice>>> = OnceLock::new();
    NOTICES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn session_chat_notice_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

pub fn set_session_chat_watchdog_notice(
    project_id: &str,
    session_id: &str,
    mut notice: SessionChatTerminalNotice,
) {
    if let Ok(mut notices) = watchdog_notices().lock() {
        let key = session_chat_notice_key(project_id, session_id);
        /*
        CDXC:SessionChatTerminalNotices 2026-08-19:
        Re-publishing the SAME verdict keeps the instance the client already
        knows: its `detectedAt` is that client's dismissal key, and the stored
        age is what expires the instance, so neither may be reset by a repeat.
        */
        let stored_at = match notices.get(&key) {
            Some(stored) if notice.same_notice(Some(&stored.notice)) => {
                notice.carry_forward_detected_at(Some(&stored.notice));
                stored.stored_at
            }
            _ => Instant::now(),
        };
        notices.insert(key, StoredWatchdogNotice { notice, stored_at });
    }
}

pub fn session_chat_watchdog_notice(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let mut notices = watchdog_notices().lock().ok()?;
    let key = session_chat_notice_key(project_id, session_id);
    let stored = notices.get(&key)?;
    if stored.stored_at.elapsed() >= WATCHDOG_NOTICE_MAX_AGE {
        notices.remove(&key);
        return None;
    }
    Some(stored.notice.clone())
}

/// Retires a watchdog notice. Returns the notice that was showing, so the
/// caller knows whether a clearing state frame is owed.
pub fn clear_session_chat_watchdog_notice(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let mut notices = watchdog_notices().lock().ok()?;
    let stored = notices.remove(&session_chat_notice_key(project_id, session_id))?;
    // An entry that had already expired was invisible to every reader, so
    // removing it here is not a change anybody is owed a frame for.
    (stored.stored_at.elapsed() < WATCHDOG_NOTICE_MAX_AGE).then_some(stored.notice)
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Clean-screen retirement. A watchdog verdict about SCREEN state (the login
screen, the trust dialog, the crashed CLI, a queued input) is only true while
that screen is up, so the next capture that succeeds whole and classifies to
nothing is proof the state is over and the card must go. `deliveryFailed` is
deliberately exempt: it describes a message that was lost in the past, not
anything currently painted, so a clean screen says nothing about it and it keeps
its own retirement rules (the next send, a later verification, expiry).

Returns the notice that was retired, so a caller can tell whether a clearing
frame is owed; an already-expired entry was invisible to every reader and
therefore reports nothing.
*/
pub fn retire_session_chat_watchdog_notice_on_clean_screen(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let mut notices = watchdog_notices().lock().ok()?;
    let key = session_chat_notice_key(project_id, session_id);
    if notices
        .get(&key)
        .is_none_or(|stored| stored.notice.kind == SESSION_CHAT_NOTICE_DELIVERY_FAILED)
    {
        return None;
    }
    let stored = notices.remove(&key)?;
    (stored.stored_at.elapsed() < WATCHDOG_NOTICE_MAX_AGE).then_some(stored.notice)
}

/*
A watchdog notice normally wins: it is both fresher and more specific than
whatever the screen classifier read at the same moment.

CDXC:SessionChatTerminalPicker 2026-08-21: an ANSWERABLE screen notice is the
one exception, and it is not a close call. A watchdog notice reports a PAST
event ("your message could not be proven delivered") and its only advice is to
go look at the terminal; an answerable picker is the LIVE state that most
likely caused that event, and it can be resolved from the chat surface in one
click. Letting the past-event card mask it left the user staring at a
delivery-failed banner with the picker sitting unanswered on screen — the exact
dead end this feature exists to remove. `deliveryFailed` in particular is
exempt from clean-screen retirement, so it would have masked the picker for its
full 10-minute lifetime.
*/
pub fn merge_session_chat_terminal_notices(
    watchdog: Option<SessionChatTerminalNotice>,
    screen: Option<SessionChatTerminalNotice>,
) -> Option<SessionChatTerminalNotice> {
    if let Some(screen) = screen {
        if screen.is_answerable() {
            return Some(screen);
        }
        return watchdog.or(Some(screen));
    }
    watchdog
}

/// Store lookup + merge, for the read/frame paths that only hold a screen
/// classification.
pub fn resolve_session_chat_terminal_notice(
    project_id: &str,
    session_id: &str,
    screen: Option<SessionChatTerminalNotice>,
) -> Option<SessionChatTerminalNotice> {
    merge_session_chat_terminal_notices(
        session_chat_watchdog_notice(project_id, session_id),
        screen,
    )
}
