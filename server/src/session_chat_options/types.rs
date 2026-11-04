use super::*;

/// Tail window scanned for a statusline/footer. The real dumps put the signal
/// within the last ~6 lines; 15 leaves headroom for an on-screen picker.
pub const SESSION_CHAT_OPTION_SCAN_LINES: usize = 15;

/// Bounded transcript tail used to find the latest structured model record.
/// Two maximum-sized chat records still leave room for the preceding
/// assistant/turn-context metadata row.
pub(super) const SESSION_CHAT_OPTION_TRANSCRIPT_SCAN_BYTES: u64 = 6 * 1024 * 1024;

/// Detection spawns a process, so every trigger goes through a short cache.
pub const SESSION_CHAT_OPTION_CACHE_TTL: Duration = Duration::from_secs(5);

/// Post-delivery probes at 0ms, 150ms, 2s and 6s; entries are incremental delays.
pub const SESSION_CHAT_OPTION_REDETECT_DELAYS_MS: [u64; 4] = [0, 150, 1_850, 4_000];

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
// Result types (mirror of packages/shared/session-chat-agent-state.ts SessionChatDetectedOptions)
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
    /// The flags the session's agent command was started with (`launch_selection.rs`), the
    /// weakest evidence: it holds only until the agent reports for itself.
    Launch,
}

impl SessionChatOptionEvidence {
    fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Transcript => "transcript",
            Self::Statusline => "statusline",
            Self::Launch => "launch",
        }
    }
}

/*
CDXC:AgentScreenDetection 2026-09-03 WHY:
Claude's statusLine payload reports how full the context window is. The chat
composer renders it as a usage ring: percentage when Claude
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
    pub(crate) fn is_empty(&self) -> bool {
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
    /// Claude's Shift+Tab permission/input mode, or Codex's Plan collaboration
    /// mode (`plan`, absent while Codex is in its default mode); both are
    /// available only on screen.
    pub mode: Option<SessionChatDetectedChoice>,
    /// Codex's permission profile (what `/permissions` sets): `ask-for-approval`,
    /// `approve-for-me`, `full-access`, `read-only`, or a profile from the user's own config
    /// under its own name.
    pub permissions: Option<SessionChatDetectedChoice>,
    /// When the transcript recorded `permissions` (its `turn_context` record's own timestamp),
    /// so a newer turn can replace a reading the screen no longer shows. Not sent to clients.
    pub permissions_recorded_at: Option<String>,
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
    pub codex_status: Option<Value>,
    /// What Cursor handed its statusline command plus the checkout's git state
    /// (`session_chat_cursor_status.rs`), camelCase and absent-when-absent.
    pub cursor_status: Option<Value>,
    /// What the Hermes session's own row in its session store reports
    /// (`session_chat_hermes_status.rs`), camelCase and absent-when-absent.
    pub hermes_status: Option<Value>,
    /// What the Pi session's own transcript reports: model, thinking level, token and cost totals,
    /// context use (`session_chat_pi_status.rs`), camelCase and absent-when-absent.
    pub pi_status: Option<Value>,
    /// The session checkout's repository, branch and folder, sent for the agents that report no
    /// status of their own (`session_chat_cursor_status.rs`).
    pub checkout_status: Option<Value>,
    /// Session-local provider inventory supplied by agents with a model API.
    pub model_catalog: Option<Value>,
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
        if let Some(permissions) = self.selection.permissions.as_ref() {
            map.insert(
                "permissions".to_string(),
                json!({
                    "value": permissions.value,
                    "label": permissions.label,
                    "source": permissions.source.as_str(),
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
        if let Some(status) = self.selection.codex_status.as_ref() {
            map.insert("codexStatus".to_string(), status.clone());
        }
        if let Some(status) = self.selection.cursor_status.as_ref() {
            map.insert("cursorStatus".to_string(), status.clone());
        }
        if let Some(status) = self.selection.hermes_status.as_ref() {
            map.insert("hermesStatus".to_string(), status.clone());
        }
        if let Some(status) = self.selection.pi_status.as_ref() {
            map.insert("piStatus".to_string(), status.clone());
        }
        if let Some(status) = self.selection.checkout_status.as_ref() {
            map.insert("checkoutStatus".to_string(), status.clone());
        }
        if let Some(catalog) = &self.selection.model_catalog {
            map.insert("modelCatalog".into(), catalog.clone());
        }
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        Value::Object(map)
    }
}

/*
CDXC:AgentScreenDetection 2026-08-19:
One `zmx history` capture, two readings. The model/effort grammar and the
terminal-state classifier (session_chat_notice/) both want the same screen, so
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
    /// Child lifecycle evidence, independent of the terminal capture.
    pub fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    /// Whether to publish the fleet state, including empty or explicitly unavailable observations.
    pub fleet_observed: bool,
    /*
    CDXC:SessionChat 2026-09-03: Claude's task list, read from its
    on-disk task store rather than the screen. It rides in the same detection
    because the detector is the one periodic reader every publisher already
    consults; unlike the screen readings it needs no capture, so a failed
    capture neither clears it nor makes it stale.
    */
    pub tasks: Option<crate::session_chat_agent_tasks::SessionChatAgentTasks>,
    /// CDXC:SessionChat 2026-10-22: Claude's Workflow runs for the chat's workflow cards, read
    /// from disk beside the task list (`session_chat_claude_workflow_cards.rs`).
    pub workflows: Option<crate::session_chat_claude_workflow_cards::SessionChatWorkflows>,
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
    payload_change_watch(move |agent_session_id| {
        agent_session_id.and_then(|id| {
            crate::agent_hooks::statusline::claude_statusline_payload_path(
                &hook_state_directory,
                id,
            )
        })
    })
}

/// Fires when Cursor's stored payload for this Ghostex session changes, which
/// it does from the first footer Cursor draws, draft or not.
pub(crate) fn cursor_statusline_change_watch(
    hook_state_directory: std::path::PathBuf,
    project_id: String,
    session_id: String,
) -> SessionChatOptionsChangeWatch {
    payload_change_watch(move |_| {
        crate::agent_hooks::statusline::cursor_statusline_payload_path(
            &hook_state_directory,
            &project_id,
            &session_id,
        )
    })
}

fn payload_change_watch(
    path: impl Fn(Option<&str>) -> Option<std::path::PathBuf> + Send + Sync + 'static,
) -> SessionChatOptionsChangeWatch {
    let observed: Mutex<Option<Option<std::time::SystemTime>>> = Mutex::new(None);
    Arc::new(move |agent_session_id: Option<&str>| {
        let modified = path(agent_session_id).and_then(|path| {
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
    Copilot,
    Cursor,
    Empryo,
    Grok,
    Hermes,
    Omp,
    Pi,
    // CDXC:AgentScreenDetection 2026-10-06 WHY:
    // ZCode has no statusline/options grammar — every options arm below treats
    // it as "parse nothing" — but `session_chat_option_agent` is also the gate
    // of the terminal-notice classifier, and without an arm a dead ZCode's
    // exit screen can never classify, so chat shows Ready over a corpse.
    Zcode,
    /// DeepSeek Harness's terminal app (dsh-tui): its status line names the model and level.
    Dsh,
}

pub fn session_chat_option_agent(agent: Option<&str>) -> Option<SessionChatOptionAgent> {
    match agent.map(str::trim).unwrap_or_default() {
        "antigravity" | "antigravity-cli" | "agy" => Some(SessionChatOptionAgent::Antigravity),
        "claude" | "openclaude" => Some(SessionChatOptionAgent::Claude),
        "codex" => Some(SessionChatOptionAgent::Codex),
        "copilot" => Some(SessionChatOptionAgent::Copilot),
        "cursor" => Some(SessionChatOptionAgent::Cursor),
        "empryo" => Some(SessionChatOptionAgent::Empryo),
        "grok" => Some(SessionChatOptionAgent::Grok),
        "hermes" | "hermes-agent" => Some(SessionChatOptionAgent::Hermes),
        "omp" => Some(SessionChatOptionAgent::Omp),
        "pi" => Some(SessionChatOptionAgent::Pi),
        "zcode" | "zcode-cli" => Some(SessionChatOptionAgent::Zcode),
        "dsh" | "dsh-tui" => Some(SessionChatOptionAgent::Dsh),
        _ => None,
    }
}

/// Slash commands whose dispatch can change what the statusline reports. Mirrors
/// `session_option_command_names` in packages/gx-chat-core/src/menus/option_catalog.rs.
pub fn is_session_chat_option_command_text(agent: Option<&str>, text: &str) -> bool {
    if session_chat_option_agent(agent).is_none() {
        return false;
    }
    let Some(first) = text.split_whitespace().next() else {
        return false;
    };
    matches!(first, "/model" | "/effort" | "/fast" | "/plan")
        // Empryo opens its model panel with `/models`.
        || (first == "/models"
            && session_chat_option_agent(agent) == Some(SessionChatOptionAgent::Empryo))
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
    let Some(first) = text.split_whitespace().next() else {
        return false;
    };
    match session_chat_option_agent(agent) {
        Some(
            SessionChatOptionAgent::Claude
            | SessionChatOptionAgent::Codex
            | SessionChatOptionAgent::Grok,
        ) => first == "/compact",
        Some(SessionChatOptionAgent::Cursor) => matches!(first, "/compact" | "/summarize"),
        _ => false,
    }
}
