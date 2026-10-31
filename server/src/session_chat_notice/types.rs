use super::*;

// ---------------------------------------------------------------------------
// Wire contract (mirror of packages/shared/session-chat-agent-state.ts SessionChatTerminalNotice)
// ---------------------------------------------------------------------------

/// The agent cannot talk to its provider until the user signs in again.
pub const SESSION_CHAT_NOTICE_LOGIN_EXPIRED: &str = "loginExpired";
/// The title of the `loginExpired` notice for a Claude login whose organization disabled
/// subscription access: the account itself is unusable, so the fix is another account.
pub const CLAUDE_ACCOUNT_DISABLED_TITLE: &str =
    "This Claude account's subscription access is disabled";
/// A workspace/directory trust dialog is blocking the composer.
pub const SESSION_CHAT_NOTICE_TRUST_PROMPT: &str = "trustPrompt";
/// A sibling blocking dialog about settings/permissions.
pub const SESSION_CHAT_NOTICE_PERMISSIONS_WARNING: &str = "permissionsWarning";
/// Usage/rate/credit limit reported on screen.
pub const SESSION_CHAT_NOTICE_USAGE_LIMIT: &str = "usageLimit";
/// Network/server failure reported on screen.
pub const SESSION_CHAT_NOTICE_STREAM_ERROR: &str = "streamError";
/// A blocking update dialog.
pub const SESSION_CHAT_NOTICE_UPDATE_PROMPT: &str = "updatePrompt";
/// The agent process appears to have exited back to a shell.
pub const SESSION_CHAT_NOTICE_AGENT_EXITED: &str = "agentExited";
/// The agent reported an error; this alone does not establish that it exited.
pub const SESSION_CHAT_NOTICE_AGENT_ERROR: &str = "agentError";
/// Input accepted but held client-side until the running turn ends.
pub const SESSION_CHAT_NOTICE_QUEUED_INPUT: &str = "queuedInput";
/*
The send watchdog could not prove a message reached the agent. ONE kind covers
every watchdog verdict about a lost send — including the affirmative one built
by `session_chat_delivery_mismatch_notice` — because the kind is what carries
this state's rules: it blocks the prompt queue, it is exempt from clean-screen
retirement (it describes a past event, not anything painted right now), and
clients already render it. Splitting the affirmative case into a second kind
would silently opt it out of all three.
*/
pub const SESSION_CHAT_NOTICE_DELIVERY_FAILED: &str = "deliveryFailed";
/// The id of the Fix it action on a delivery card the send recovery gave up on.
pub const SESSION_CHAT_NOTICE_ACTION_FIX_SEND: &str = "fixSend";
/*
CDXC:AgentScreenDetection 2026-08-28:
The agent's API refused to answer the last message (Claude Code's safeguards
refusal row). Detected from the session TRANSCRIPT, not the screen — the
follower spots the recorded refusal row, which is authoritative where a screen
capture is a guess. A separate kind from `deliveryFailed` on purpose: the
message DID reach the agent and the composer works fine, so this must not
block the prompt queue — but like `deliveryFailed` it describes a past event,
so a clean screen must not retire it.
*/
pub const SESSION_CHAT_NOTICE_API_REFUSAL: &str = "apiRefusal";
/// A Codex decision surface has replaced the ordinary composer. The concrete
/// title/detail come from the source-derived screen classifier.
pub const SESSION_CHAT_NOTICE_CLAUDE_INPUT_BLOCKED: &str = "claudeInputBlocked";
pub const SESSION_CHAT_NOTICE_CODEX_INPUT_BLOCKED: &str = "codexInputBlocked";
/// A Cursor model/effort picker owns terminal input while retaining the normal
/// composer frame around its filter field.
pub const SESSION_CHAT_NOTICE_CURSOR_INPUT_BLOCKED: &str = "cursorInputBlocked";
/// A Grok Build card, picker, modal, authentication flow, or special editor
/// owns terminal input instead of the ordinary composer.
pub const SESSION_CHAT_NOTICE_GROK_INPUT_BLOCKED: &str = "grokInputBlocked";
/// A Hermes prompt_toolkit state, Ink overlay, setup, or authentication flow
/// owns terminal input instead of the ordinary composer.
pub const SESSION_CHAT_NOTICE_HERMES_INPUT_BLOCKED: &str = "hermesInputBlocked";
/// An OMP approval, prompt, selector, authentication flow, focused panel, or
/// modal owns terminal input instead of the ordinary composer.
pub const SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED: &str = "ompInputBlocked";
/// A Pi focused selector, prompt, authentication flow, or modal has replaced
/// its ordinary prompt editor.
pub const SESSION_CHAT_NOTICE_PI_INPUT_BLOCKED: &str = "piInputBlocked";
/// An Empryo panel (`/router`, `/models`, `/settings`, the Ctrl+K palette, …) has the
/// keyboard instead of its input box.
pub const SESSION_CHAT_NOTICE_EMPRYO_INPUT_BLOCKED: &str = "empryoInputBlocked";
/// Empryo has no model it can use or lost an account's sign-in; informational, since the row
/// that says so stays on screen after it is fixed.
pub const SESSION_CHAT_NOTICE_EMPRYO_SIGN_IN: &str = "empryoSignIn";
/// Claude Code's tool permission prompt ("Do you want to proceed?" over
/// Yes/No rows), read off the screen: answerable the same way the resume
/// picker is, and the only card for it when the hook-derived approval card
/// never arrived or was retired early.
pub use crate::session_chat_resume_prompt::SESSION_CHAT_PERMISSION_PROMPT_KIND as SESSION_CHAT_NOTICE_PERMISSION_PROMPT;
/// Claude Code's resume-usage picker: an on-screen chooser the chat surface can
/// ANSWER, not just point at. Its rows ride the notice as `choices`.
pub use crate::session_chat_resume_prompt::SESSION_CHAT_RESUME_PROMPT_KIND as SESSION_CHAT_NOTICE_RESUME_PROMPT;
/// Claude Code's safeguards "Session paused" chooser (switch to the fallback
/// model vs edit the prompt): answerable the same way the resume picker is.
pub use crate::session_chat_resume_prompt::SESSION_CHAT_SESSION_PAUSED_PROMPT_KIND as SESSION_CHAT_NOTICE_SESSION_PAUSED_PROMPT;
/// Claude Code's model/effort switch confirmation ("Switch model?" / "Change
/// effort level?"): answerable the same way the resume picker is.
pub use crate::session_chat_resume_prompt::SESSION_CHAT_SWITCH_CONFIRM_PROMPT_KIND as SESSION_CHAT_NOTICE_SWITCH_CONFIRM_PROMPT;

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
    RecoverCodexConversation,
    /// Server-side: remember the session's folders as trusted, then accept
    /// the prompt on screen (session_chat_trust_memory.rs).
    TrustAndRemember,
    /// Server-side: sleep and wake a session whose agent exited to the shell
    /// (session_chat_agent_restart.rs).
    RestartAgent,
}

impl SessionChatTerminalNoticeActionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SwitchToTerminal => "switchToTerminal",
            Self::SendKeys => "sendKeys",
            Self::RecoverCodexConversation => "recoverCodexConversation",
            Self::TrustAndRemember => "trustAndRemember",
            Self::RestartAgent => "restartAgent",
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

    /// Fix it: the send recovery gxserver already tried on its own (session_chat_queue_runtime/send_heal.rs),
    /// run again because the user asked. It rides the `restartAgent` answer every client already sends.
    pub fn fix_send() -> Self {
        Self {
            id: SESSION_CHAT_NOTICE_ACTION_FIX_SEND.to_string(),
            label: "Fix it".to_string(),
            kind: SessionChatTerminalNoticeActionKind::RestartAgent,
            send: None,
        }
    }

    /// Retry: the failed message goes back to the queue with nothing else changed.
    pub fn retry_send() -> Self {
        Self {
            id: crate::session_chat_send_failure::ACTION_RETRY.to_string(),
            label: "Retry".to_string(),
            kind: SessionChatTerminalNoticeActionKind::RestartAgent,
            send: None,
        }
    }

    /// Start fresh: the agent starts on a new conversation in this session, for a failed message
    /// whose agent is gone and has no conversation to resume.
    pub fn start_fresh_send() -> Self {
        Self {
            id: crate::session_chat_send_failure::ACTION_START_FRESH.to_string(),
            label: "Start fresh".to_string(),
            kind: SessionChatTerminalNoticeActionKind::RestartAgent,
            send: None,
        }
    }

    pub fn trust_and_remember() -> Self {
        Self {
            id: "trustAndRemember".to_string(),
            label: "Trust and Remember".to_string(),
            kind: SessionChatTerminalNoticeActionKind::TrustAndRemember,
            send: None,
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
CDXC:SessionChat 2026-08-21:
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
    /// The few lines that say why, drawn on the card itself (`errorLines`): what a failed start printed, without the shell prompt it left.
    pub error_lines: Option<String>,
    /// The command an approval card asks to run, drawn under its title while the card is folded (`approvalCommand`, read by the chat core's `collapsedDetail`).
    pub approval_command: Option<String>,
    /// What a Claude Code permission prompt asks (`permissionRequest`), which the chat core turns into the approval card's title, command and warning.
    pub permission_request: Option<crate::session_chat_claude_permission::ClaudePermissionRequest>,
    /// The whole plan Claude's "Ready to code?" asks to approve, as Markdown from its plan file (`plan`); the screen shows only a scrolled window of it (session_chat_claude_plan.rs).
    pub plan: Option<String>,
    pub source: SessionChatTerminalNoticeSource,
    /// RFC3339 millis; also the client's dismissal key.
    pub detected_at: String,
    pub actions: Vec<SessionChatTerminalNoticeAction>,
    /// Answerable picker rows, in screen order. Empty for every notice that
    /// only describes a state.
    pub choices: Vec<SessionChatTerminalNoticeChoice>,
    pub dialog: Option<crate::session_chat_terminal_dialog::TerminalDialog>,
    pub conversation_lock: Option<crate::session_chat_codex_lock::ConversationLock>,
    /// A trust prompt on a remembered folder: the detection funnel answers it
    /// itself and clients never see it (session_chat_trust_memory.rs).
    pub auto_trust: bool,
    /// A usage-limit notice while account recovery waits with every account full: what the card says about it (accounts/limit_wait.rs). Attached by `resolve_session_chat_terminal_notice`, never classified from the screen.
    pub account_wait: Option<Value>,
    /// The failed queue row this card stands for (session_chat_send_failure.rs).
    pub send_failure_prompt_id: Option<String>,
    /// Server-side delivery policy for this particular detected state.
    blocks_input: bool,
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
            error_lines: None,
            approval_command: None,
            permission_request: None,
            plan: None,
            source,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            actions: Vec::new(),
            choices: Vec::new(),
            dialog: None,
            conversation_lock: None,
            auto_trust: false,
            account_wait: None,
            send_failure_prompt_id: None,
            blocks_input: session_chat_notice_kind_blocks_input(kind),
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

    pub fn with_error_lines(mut self, error_lines: Option<String>) -> Self {
        self.error_lines = error_lines.filter(|lines| !lines.trim().is_empty());
        self
    }

    pub fn with_approval_command(mut self, command: Option<String>) -> Self {
        self.approval_command = command.filter(|command| !command.trim().is_empty());
        self
    }

    pub fn with_permission_request(
        mut self,
        request: Option<crate::session_chat_claude_permission::ClaudePermissionRequest>,
    ) -> Self {
        self.permission_request = request;
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

    pub(crate) fn with_input_blocking(mut self, blocks_input: bool) -> Self {
        self.blocks_input = blocks_input;
        self
    }

    /// CDXC:SessionChat 2026-10-11 WHY: the chat cannot read these screens row by row (other agents' pickers and dialogs, and Codex's own screens its dialog reader does not know), so while one owns the input the chat passes keys through to the terminal and shows the screen region (`screenTail`) live; see the DECISION in session_chat_resume_picker.rs.
    pub fn passes_keys_through(&self) -> bool {
        self.dialog.is_none()
            && self.choices.is_empty()
            && self.conversation_lock.is_none()
            && matches!(
                self.kind.as_str(),
                SESSION_CHAT_NOTICE_CODEX_INPUT_BLOCKED
                    | SESSION_CHAT_NOTICE_CURSOR_INPUT_BLOCKED
                    | SESSION_CHAT_NOTICE_GROK_INPUT_BLOCKED
                    | SESSION_CHAT_NOTICE_HERMES_INPUT_BLOCKED
                    | SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED
                    | SESSION_CHAT_NOTICE_PI_INPUT_BLOCKED
                    | SESSION_CHAT_NOTICE_EMPRYO_INPUT_BLOCKED
            )
    }

    /// True when this notice offers rows the chat surface can answer itself.
    pub fn is_answerable(&self) -> bool {
        !self.choices.is_empty() || self.dialog.is_some()
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
                && self.error_lines == other.error_lines
                && self.approval_command == other.approval_command
                && self.permission_request == other.permission_request
                && self.plan == other.plan
                && self.source == other.source
                && self.blocks_input == other.blocks_input
                && self.actions == other.actions
                && self.dialog == other.dialog
                && self.conversation_lock == other.conversation_lock
                && self.auto_trust == other.auto_trust
                // Labels only: the highlight moves whenever the user arrows
                // around in the terminal, and re-minting `detectedAt` for that
                // would resurrect a card they just dismissed.
                && choice_labels(&self.choices) == choice_labels(&other.choices)
        })
    }

    /// `same_notice`, plus the screen a pass-through card shows: a moved highlight there is news
    /// for the clients, though it is still the same instance.
    pub fn same_frame(&self, other: Option<&SessionChatTerminalNotice>) -> bool {
        self.same_notice(other)
            && other.is_some_and(|other| self.account_wait == other.account_wait)
            && (!self.passes_keys_through()
                || other.is_some_and(|other| self.screen_tail == other.screen_tail))
    }

    /*
    CDXC:AgentScreenDetection 2026-08-19:
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

    /// CDXC:AgentScreenDetection 2026-09-05 DECISION:
    /// User: Claude and Codex quota and login-error banners, and Claude's generic error banner, must show clear errors without blocking a usable composer.
    /// Actual dialogs and exited agents still block; leave Claude's automatic-continue wait-screen policy unchanged.
    /// This extends the earlier Claude quota-warning decision; automatic delivery separately holds on unresolved quota, authentication, and agent-error evidence so it cannot consume queued prompts in failed attempts.
    pub fn blocks_input(&self) -> bool {
        self.blocks_input
    }

    /// The session's login can never answer again (its organization disabled Claude
    /// subscription access); only another account fixes it. See `accounts/disabled.rs`.
    pub fn account_disabled(&self) -> bool {
        self.kind == SESSION_CHAT_NOTICE_LOGIN_EXPIRED
            && self.title == CLAUDE_ACCOUNT_DISABLED_TITLE
    }

    pub fn blocks_queued_delivery(&self) -> bool {
        self.blocks_input()
            || matches!(
                self.kind.as_str(),
                SESSION_CHAT_NOTICE_USAGE_LIMIT
                    | SESSION_CHAT_NOTICE_LOGIN_EXPIRED
                    | SESSION_CHAT_NOTICE_AGENT_ERROR
            )
    }

    /// Stable identity for the long-poll fingerprint: kind plus the human text.
    /// Never includes `detectedAt` or the screen tail.
    pub fn identity(&self) -> String {
        let mut identity = format!(
            "{}\u{1f}{}\u{1f}{}",
            self.kind,
            self.title,
            self.dialog
                .as_ref()
                .map(|dialog| dialog.id.as_str())
                .unwrap_or_else(|| self.detail.as_deref().unwrap_or_default())
        );
        if let Some(lock) = &self.conversation_lock {
            identity.push('\u{1f}');
            identity.push_str(&json!(lock).to_string());
        }
        if self.passes_keys_through() {
            identity.push('\u{1f}');
            identity.push_str(self.screen_tail.as_deref().unwrap_or_default());
        }
        // Two prompts with the same rows and no warning differ only in what they ask.
        if let Some(request) = self.permission_request.as_ref() {
            identity.push('\u{1f}');
            identity.push_str(&request.to_value().to_string());
        }
        // The dialog id is the plain panel's hash, which a side answer read whole does not change.
        if let Some(side_question) = self
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.side_question.as_ref())
        {
            identity.push('\u{1f}');
            identity.push_str(&crate::session_chat_claude_panel::side_answer_fingerprint(
                side_question,
            ));
        }
        identity
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("kind".to_string(), json!(self.kind));
        if let Some(lock) = &self.conversation_lock {
            map.insert("conversationLock".to_string(), json!(lock));
        }
        map.insert("severity".to_string(), json!(self.severity.as_str()));
        if let Some(dialog) = self.dialog.as_ref() {
            map.insert("dialog".to_string(), json!(dialog));
        }
        map.insert("title".to_string(), json!(self.title));
        if let Some(detail) = self.detail.as_deref() {
            map.insert("detail".to_string(), json!(detail));
        }
        if let Some(screen_tail) = self.screen_tail.as_deref() {
            map.insert("screenTail".to_string(), json!(screen_tail));
        }
        if let Some(error_lines) = self.error_lines.as_deref() {
            map.insert("errorLines".to_string(), json!(error_lines));
        }
        if let Some(command) = self.approval_command.as_deref() {
            map.insert("approvalCommand".to_string(), json!(command));
        }
        if let Some(request) = self.permission_request.as_ref() {
            map.insert("permissionRequest".to_string(), request.to_value());
        }
        if let Some(plan) = self.plan.as_deref() {
            map.insert("plan".to_string(), json!(plan));
        }
        if self.passes_keys_through() {
            map.insert("passthrough".to_string(), json!(true));
        }
        map.insert("source".to_string(), json!(self.source.as_str()));
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        if let Some(wait) = self.account_wait.as_ref() {
            map.insert("accountWait".to_string(), wait.clone());
        }
        if let Some(prompt_id) = self.send_failure_prompt_id.as_deref() {
            map.insert("sendFailure".to_string(), json!({ "promptId": prompt_id }));
        }
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
        Some(current) => current.same_frame(published),
        None => published.is_none(),
    }
}
