//! The view document: one value that says everything the chat renderer draws.
//!
//! This is the `snapshot` field of a [`crate::Frame`], in the shape `publish` in
//! `packages/shared/session-chat-controller/native-host.ts` built it, and read key by key by
//! `apps/desktop/src/app/native_chat/` and the phone's native views. The JSON is their contract, so
//! every field below carries the TypeScript spelling and the TypeScript absent-versus-null
//! behaviour:
//!
//! - `Tri::Absent` is a key `JSON.stringify` left out because the value was `undefined`.
//! - `Tri::Null` and `Option::None` are a key present with `null`.
//! - Integer-valued fields are signed or unsigned integers, never `f64`, because JSON writes `1`
//!   where an `f64` would write `1.0`.
//!
//! Fields typed [`serde_json::Value`] are the subtrees a later port family owns; each one is
//! listed in `docs/2026-09-21/rust-chat/SEAM.md` with its owner.

use ghostex_gx_protocol::Tri;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::document::{
    AccountStatus, AsyncQuestions, ComposerActions, ComposerChrome, ComposerOverflow,
    DeferredWorkRow, Draft, EmptyState, HostAction, IncomingDraft, Interaction, NewSessionWelcome,
    Note, QuestionCard, Queue, TerminalTail, ViewState, WorkingStrip,
};

/// Everything the chat renderer draws, in one value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    // ---- transcript state -------------------------------------------------
    pub view: ViewState,
    /// `loading`, `ready`, `working`, `error`, `notFound`, `starting`.
    pub status: String,
    /// The live turn's lifecycle, or `null`. Family a.
    pub lifecycle: Tri<Value>,
    /// The agent's blocking question or approval request, or `null`. Family c.
    pub prompt: Tri<Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub retired_async_question_ids: Tri<Vec<String>>,
    /// The turn is live, after the lifecycle settle folded in.
    pub working: bool,
    /// Whether the transcript keeps the newest turn open: the live signal until the turn lifecycle
    /// ends the run (`sessionChatTranscriptWorking`). The projection applies the sticky fold on top.
    pub transcript_working: bool,
    /// The session activity gxserver presents, the same source the sidebar spinner uses.
    pub session_working: bool,
    /// Model and effort read off the agent's own terminal. Family e.
    pub selected_options: Tri<Value>,
    /// The blocking or failed terminal state, projected with its choices and answers. Family c.
    pub terminal_notice: Tri<Value>,
    /// The prompt the agent handed back after an Escape. Family d.
    pub returned_prompt: Tri<Value>,
    /// Live on-screen progress such as compaction. Family f.
    pub terminal_activity: Tri<Value>,
    /// Subagents the agent is running. Family f.
    pub agent_fleet: Tri<Value>,
    /// The agent's task list. Family f.
    pub agent_tasks: Tri<Value>,
    /// Latched once gxserver has read this session's screen.
    pub screen_probed: bool,
    pub agent: Tri<String>,
    pub agent_session_id: Tri<String>,
    /// A draft session's agent choices. Family e.
    pub available_agents: Tri<Value>,
    /// The same-family accounts a prompted session can resume under. Family e.
    pub switchable_agents: Tri<Value>,
    /// The session's own launch agent id, never the transcript family.
    pub session_agent_id: Tri<String>,
    pub error: Tri<String>,
    pub has_more: bool,
    /// Byte cursor after the most recently accepted history window.
    pub earlier_page_cursor: i64,
    pub loading_earlier: bool,
    /// The ids of the turns whose reply is final, so the renderer can mark them.
    pub final_ids: Vec<String>,
    /// Per completed-work row: the read is in flight, or it failed and offers a retry.
    pub deferred_work: BTreeMap<String, DeferredWorkRow>,

    // ---- composer ---------------------------------------------------------
    pub queue: Queue,
    pub draft: Draft,
    pub composer_placeholder: String,
    /// Why sending is blocked, or `null` when it is not.
    pub send_blocked_reason: Option<String>,
    pub composer_collapse_eligible: bool,
    pub composer_collapsed: bool,
    pub composer_overflow: ComposerOverflow,
    pub composer_actions: ComposerActions,
    pub composer_chrome: ComposerChrome,
    /// The slash command the composer would complete to, or `null`. Family d.
    pub composer_command: Option<String>,
    /// The `@` and `$` popup, or `null` when neither is open. Family d.
    pub suggestions: Tri<Value>,
    pub history_active: bool,
    pub pending_attachments: u32,
    pub incoming_draft: Option<IncomingDraft>,
    pub note: Note,
    pub interaction: Interaction,
    pub host_actions: Vec<HostAction>,

    // ---- questions and notices -------------------------------------------
    pub question_card: QuestionCard,
    pub async_questions: AsyncQuestions,
    pub notice_visible: bool,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub notice_error: Tri<String>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub operation_error: Tri<String>,
    /// The refusal's own code, so the composer can draw `composerNotReady` instead of an error
    /// line.
    pub operation_error_code: Option<String>,

    // ---- options, model picker, accounts ----------------------------------
    /// The pill labels the composer shows. Family e.
    pub option_labels: Tri<Value>,
    /// The menus behind those pills. Family e.
    pub option_menus: Tri<Value>,
    /// The session's option state and catalog. Family e.
    pub session_options: Tri<Value>,
    /// The model menu's inputs. Family e.
    pub model_menu_context: Tri<Value>,
    /// The projected model menu, or `null` when closed. Family e.
    pub model_menu: Tri<Value>,
    /// The full model picker window, or `null` when closed. Family e.
    pub model_picker: Tri<Value>,
    /// The provider the model pills belong to. Absent, not null, before the agent is known:
    /// `publish` spreads `computeNativeChatOptions(...)` in, and that object simply has no
    /// `modelProvider` key until there is one, so `JSON.stringify` leaves it out. Family e.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub model_provider: Tri<String>,
    /// The queued model selection and its outbox. Family e.
    pub model_selection: Tri<Value>,
    /// The id of the option dispatch in flight, or `null`.
    pub option_dispatch_id: Option<String>,
    /// The accounts read, absent until the first read answers. Family e.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub accounts: Tri<Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub account_error: Tri<String>,
    pub account_status: AccountStatus,
    /// The accounts panel, or `null` when the family manages no accounts. Family e.
    pub account_panel: Tri<Value>,
    /// The account switch card, or `null`. Family e.
    pub account_switch_card: Tri<Value>,
    /// The switch gxserver reports, or `null`. Family e.
    pub account_switch: Tri<Value>,
    /// A model selection waiting on the agent, or `null`. Family e.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub pending_model_selection: Tri<Value>,

    // ---- context ----------------------------------------------------------
    /// The context meter under the composer. Family e.
    pub context_meter: Tri<Value>,
    /// The context rows editor, or `null` when closed. Family e.
    pub context_editor: Tri<Value>,
    /// Where the status line wraps, measured by the renderer and folded back in.
    pub context_status_rows: Vec<u32>,

    // ---- panels and extras ------------------------------------------------
    pub working_strip: WorkingStrip,
    pub terminal_tail: TerminalTail,
    /// The agent fleet strip, or `null`. Family f.
    pub agent_fleet_strip: Tri<Value>,
    /// The task panel, or `null`. Family f.
    pub agent_tasks_panel: Tri<Value>,
    /// The subagent viewer, or `null` when closed. Family f.
    pub subagent: Tri<Value>,
    /// Transcript search, or `null` when closed. Family f.
    pub transcript_search: Tri<Value>,
    /// The fork branch picker, or `null`. Family e.
    pub fork_branches: Tri<Value>,
    /// The Save to Markdown sheet, or `null` when closed. Family f.
    pub save_markdown: Tri<Value>,
    /// The rewind confirmation, or `null` when closed. Family b.
    pub rewind: Tri<Value>,
    /// Whether this host can rewind at all.
    pub rewind_available: bool,
    /// Whether rewinding is allowed right now, the same gate the composer sends under.
    pub rewind_enabled: bool,
    /// Which message ids have a saved prompt, so the rail can mark them. Family b.
    pub saved_prompts: Map<String, Value>,

    // ---- modes and loading -----------------------------------------------
    pub summary_mode: bool,
    /// The user's verbose override, or `null` to follow the setting.
    pub verbose_override: Option<bool>,
    pub empty_state: EmptyState,
    pub new_session_welcome: Option<NewSessionWelcome>,
    /// `blank`, `indicator` or `retry` while the transcript loads; `null` otherwise.
    pub loading_stage: Option<String>,
    pub skills_loading: bool,
    pub files_loading: bool,

    /// Keys this build does not model yet, carried through untouched.
    ///
    /// The producer spreads whole computation results into the document, so a new key can appear
    /// without any signal here. Keeping them means a frame still round-trips while a port family
    /// is still typing its subtree.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The document's keys whose value can change without a publish.
///
/// `native-host.ts` published for two reasons, and only two. The REACTIVE one was the chat
/// computation re-running: `new ChatComputation(..., publish)` called `publish` whenever the state
/// it held changed, and that state was everything `...viewState` spread into the snapshot. The
/// IMPERATIVE one was an explicit `publish(controller.current())`, which the arms of `action` and a
/// handful of sub-controller callbacks made by hand.
///
/// The keys below were built inside `publish` from module variables rather than from the
/// computation's state, so moving one is not itself a reason to ship a document: it rides on the
/// next publish somebody asks for. The core's own change test therefore ignores them, and the
/// family that owns one asks for a publish where the TypeScript called `changed()`.
///
/// Adding a key here without wiring its owner's publish LOSES a document.
impl Document {
    /// This document with the imperative keys blanked, which is what the change test compares.
    pub fn reactive(&self) -> Self {
        let mut copy = self.clone();
        copy.async_questions = Default::default();
        copy
    }
}
