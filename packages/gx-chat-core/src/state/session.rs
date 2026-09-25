//! The session facts the wire carries: what the agent is, what it is doing, and the side state
//! that rides along with every frame.
//!
//! Owned by family a. Other families read it and must not write it.
//!
//! The absent-versus-null rules are the whole point of this struct, and they differ per field.
//! `packages/gx-protocol/src/chat.rs` documents them on the wire; the field docs below say what
//! this side does with each one, matching `foldSessionChatState` in
//! `apps/desktop/sidebar/session-chat-runtime/fold.ts` and `applyAuthoritative` in
//! `packages/shared/session-chat-controller/controller.ts`.

use ghostex_gx_protocol::{ChatStatus, Tri, TurnLifecycle};
use serde_json::Value;

/// Everything the session is, as folded from reads and frames.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionState {
    /// What the host opened this chat with, kept until the boot read answers and the controller
    /// can be built from it.
    pub boot_config: Option<crate::event::StartConfig>,
    /// The boot read in flight, so a late answer to a retired one is dropped.
    pub boot_read_request: Option<u64>,
    /// The small cache the sidebar and the chat share, which is what a session switch restores its
    /// account, context usage and status line from.
    pub presentation: crate::session::presentation::PresentationCache,
    /// The transcript status the server reported, before the local working derivation.
    pub server_status: ChatStatus,
    /// The failure copy, or `None`. Set only by a read or a frame that says `error`.
    pub error: Option<String>,
    /// The live turn's lifecycle; last one wins.
    pub lifecycle: Option<TurnLifecycle>,
    /// The blocking question or approval. Cleared by any frame that can carry it and does not.
    pub prompt: Option<Value>,
    /// The async questions the user has already retired. Absent means unchanged.
    pub retired_async_question_ids: Vec<String>,
    /// Questions older than this stamp no longer count. Absent means unchanged, `null` means none.
    pub async_questions_since: Option<i64>,
    /// The transcript family, for example `claude` or `codex`.
    pub agent: Option<String>,
    pub agent_session_id: Option<String>,
    /// The session's own launch agent id, carried by reads alone.
    pub session_agent_id: Option<String>,
    /// A draft session's agent choices, carried by reads alone; an omission promotes the draft.
    pub available_agents: Option<Value>,
    /// The same-family accounts a prompted session can resume under, reads alone.
    pub switchable_agents: Option<Value>,
    /// Model and effort read off the agent's screen. Absent means unchanged; merged by evidence.
    pub selected_options: Option<Value>,
    /// How many times `setSelectedOptions` was handed a NEW object, which is the identity family
    /// e1's detection effect depends on. Equal values with different identities still count.
    pub selected_options_generation: u64,
    /// The blocking or failed terminal state. Cleared on omission.
    pub terminal_notice: Option<Value>,
    /// Live on-screen progress such as compaction. Cleared on omission.
    pub terminal_activity: Option<Value>,
    /// Subagents on the agent's screen. Cleared on omission, never gated on `working`.
    pub agent_fleet: Option<Value>,
    /// The agent's task list. Cleared on omission, never gated on `working`.
    pub agent_tasks: Option<Value>,
    /// Commands Ghostex typed into the agent itself. Unchanged on omission, never cleared.
    pub app_commands: Vec<Value>,
    /// The prompt the agent handed back after an Escape. Unchanged on omission.
    pub returned_prompt: Option<Value>,
    /// Latched once gxserver has read this session's screen; an omission never unsets it.
    pub screen_probed: bool,
    /// Live work as the chat channel itself reports it.
    pub server_working: bool,
    /// The session activity gxserver presents, which is what the sidebar spinner reads.
    pub session_activity_working: bool,
    /// The host's own live-work signal, merged with the server's.
    ///
    /// It was `options.working` in `controller.ts`, which only a Storybook story ever passed: the
    /// product's own caller (`native-host.ts`) left it out, so it was inert there and has no
    /// writer here on purpose. Kept because it is the seam a host WOULD push a live-work signal
    /// through, and the five readers that merge it are the shipped rule.
    pub external_working: bool,
    /// The local Stop suppression: the user pressed Escape and the spinner is held down.
    pub interrupted: bool,
    /// When the current working run began, so a previous turn's terminal lifecycle cannot settle
    /// the new one instantly.
    pub working_started_at_ms: Option<f64>,
    /// The account switch gxserver reports. Three-state on the wire.
    pub account_switch: Tri<Value>,
    /// A model selection waiting on the agent. Three-state on the wire.
    pub pending_model_selection: Tri<Value>,
    /// Ghostex's own prompt queue. `None` means no frame has carried a `queue` field yet, which is
    /// the "daemon does not support it" state and hides every queue control; an empty list means
    /// supported and empty.
    pub queue_prompts: Option<Vec<Value>>,
    /// The latest synced composer draft. Unchanged on omission; a clear is an explicit empty
    /// `content`.
    pub synced_draft: Option<Value>,
    /// Bumped on every assignment of `synced_draft`, which is every `setSyncedDraft` with a
    /// merged (and therefore new) object: the identity the `useEffect` on `syncedDraft` re-runs
    /// on.
    pub synced_draft_revision: u64,
    /// The revision the delivered-draft receipts were last handed to the host for.
    pub deliveries_recorded_revision: u64,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            boot_config: None,
            presentation: Default::default(),
            boot_read_request: None,
            server_status: ChatStatus::Loading,
            error: None,
            lifecycle: None,
            prompt: None,
            retired_async_question_ids: Vec::new(),
            async_questions_since: None,
            agent: None,
            agent_session_id: None,
            session_agent_id: None,
            available_agents: None,
            switchable_agents: None,
            selected_options: None,
            selected_options_generation: 0,
            terminal_notice: None,
            terminal_activity: None,
            agent_fleet: None,
            agent_tasks: None,
            app_commands: Vec::new(),
            returned_prompt: None,
            screen_probed: false,
            server_working: false,
            session_activity_working: false,
            external_working: false,
            interrupted: false,
            working_started_at_ms: None,
            account_switch: Tri::Absent,
            pending_model_selection: Tri::Absent,
            queue_prompts: None,
            synced_draft: None,
            synced_draft_revision: 0,
            deliveries_recorded_revision: 0,
        }
    }
}

/// Who this chat is, fixed for the life of the core.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionIdentity {
    /// This client's opaque id, echoed back as a draft's `originClientId`. The host supplies it,
    /// because the core generates no random values.
    pub client_id: String,
    pub project_id: String,
    pub session_id: String,
    /// `<projectId>:<sessionId>`, with a `remote-<machineId>:` prefix off the local machine.
    ///
    /// The storage key for every per-session record, and the identity a stored option state is
    /// scoped by. The host builds it (it is the one that knows the machine), and it arrives with
    /// the boot read rather than being rebuilt here.
    pub session_key: String,
}
