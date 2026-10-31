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
    AccountStatus, AsyncQuestions, ChatPill, ComposerActions, ComposerChrome, ComposerOverflow,
    DeferredWorkRow, Draft, EmptyState, HostAction, IncomingDraft, Interaction, InterruptToast,
    LoadingNotice, NewSessionWelcome, Note, QuestionCard, Queue, TerminalTail, ViewState,
    WorkingCard, WorkingStrip,
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
    /// A draft's Run on row (`menus/run_location.rs`), absent when the row is hidden. Family e.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub run_location: Tri<Value>,
    /// The session's own launch agent id, never the transcript family.
    pub session_agent_id: Tri<String>,
    pub error: Tri<String>,
    pub has_more: bool,
    /// Byte cursor after the most recently accepted history window.
    pub earlier_page_cursor: i64,
    pub loading_earlier: bool,
    /// The ids of the turns whose reply is final, so the renderer can mark them.
    pub final_ids: Vec<String>,
    /// The published pages (`ghostex show`) whose answers arrived, so their cards read Answered.
    pub answered_pages: Vec<String>,
    /// The question pages not answered whose questions were withdrawn, some or all, by page id,
    /// so their cards say so (`transcript::open_questions::withdrawn_pages`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub withdrawn_pages: BTreeMap<String, crate::transcript::open_questions::WithdrawnPage>,
    /// The question pages and forms put off "For later" that still wait for answers, so their
    /// cards say so and offer "Back to unanswered" (`questions::later`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub later_questions: Vec<String>,
    /// Per completed-work row: the read is in flight, or it failed and offers a retry.
    pub deferred_work: BTreeMap<String, DeferredWorkRow>,
    /// Claude Code workflow runs drawn as cards, by Workflow call id
    /// (`transcript/workflow_card.rs`); a tool row's `workflow` names its card. Family b.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workflow_cards: BTreeMap<String, Value>,

    // ---- composer ---------------------------------------------------------
    pub queue: Queue,
    /// The queued messages Back to composer is offered on, oldest first; the last is the one the
    /// composer's Up key takes back. Family d.
    pub queue_take_back_ids: Vec<String>,
    /// The message a Back to composer is taking back right now, drawn busy. Family d.
    pub queue_take_back_busy: Option<String>,
    /// The key Back to composer's tooltip names.
    pub queue_take_back_hotkey: String,
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
    /// The Catch me up button and card. Family d.
    #[serde(default)]
    pub catch_up: crate::composer::catch_up::CatchUp,
    /// The Files and links card. Family d.
    #[serde(default)]
    pub files_links: crate::composer::files_links::FilesLinks,
    pub interaction: Interaction,
    /// The toast in the scroll-to-bottom pill's place: "Press Escape again to interrupt" while a
    /// first Escape waits, then the red "Agent was interrupted". Family d.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt_toast: Option<InterruptToast>,
    pub host_actions: Vec<HostAction>,
    /// The Side chat prefix (`/btw `) when the agent takes side questions, else null. Family d.
    pub side_chat: Option<String>,
    /// The More actions menu's Skills submenu: the Ghostex skills the agent has installed. Family d.
    #[serde(default)]
    pub ghostex_skills: Vec<crate::composer::ghostex_skills::GhostexSkill>,

    // ---- questions and notices -------------------------------------------
    pub question_card: QuestionCard,
    pub async_questions: AsyncQuestions,
    /// The question forms (```questions blocks) by id, with the reader's picks or the sent answers
    /// (`questions/forms.rs`); the transcript's Markdown names each by id where it is drawn.
    pub question_forms: BTreeMap<String, Value>,
    /// The form or chart a Resources row opened in the floating modal, or `null`
    /// (`questions/resource_modal.rs`); a form's picks are in `questionForms` under its id.
    #[serde(default)]
    pub resource_modal: Value,
    /// The Previous question pill and the jumps to a question (`questions/open_questions.rs`).
    #[serde(default)]
    pub open_questions: crate::questions::open_questions::OpenQuestions,
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

    // ---- panels and extras ------------------------------------------------
    pub working_strip: WorkingStrip,
    pub terminal_tail: TerminalTail,
    /// The agent fleet strip, or `null`. Family f.
    pub agent_fleet_strip: Tri<Value>,
    /// The task panel, or `null`. Family f.
    pub agent_tasks_panel: Tri<Value>,
    /// A coordinator's Threads panel, or `null`. Family f.
    pub coordinator_threads_panel: Tri<Value>,
    /// The background tasks the agent left running (`extras::background_tasks`), which the
    /// Background pill's menu lists, or `null`. Family f.
    pub background_tasks_panel: Tri<Value>,
    /// The pills on the working card, each opening the menu of one of the panels above. Family f.
    pub chat_pills: Vec<ChatPill>,
    /// The working card right above the composer. Family f.
    pub working_card: WorkingCard,
    /// The subagent viewer, or `null` when closed. Family f.
    pub subagent: Tri<Value>,
    /// Transcript search, or `null` when closed. Family f.
    pub transcript_search: Tri<Value>,
    /// The fork branch picker, or `null`. Family e.
    pub fork_branches: Tri<Value>,
    /// The Save to Markdown sheet, or `null` when closed. Family f.
    pub save_markdown: Tri<Value>,
    /// The Session details panel (`extras/session_details/document.rs`), or `null` while it does
    /// not apply to this chat. Family f.
    pub session_details: Value,
    /// The card of the failed API call nothing has answered yet, with gxserver's retry, or
    /// `null` (`transcript/api_errors.rs`). Family b.
    pub api_error_card: Value,
    /// The rewind confirmation, or `null` when closed. Family b.
    pub rewind: Tri<Value>,
    /// Whether this host can rewind at all.
    pub rewind_available: bool,
    /// Whether rewinding is allowed right now, the same gate the composer sends under.
    pub rewind_enabled: bool,
    /// Which message ids have a saved prompt, so the rail can mark them. Family b.
    pub saved_prompts: Map<String, Value>,
    /// How strongly marked sections' headings take their kind's colour, 0 to 100 percent
    /// (`transcript/marked_sections.rs`); absent at 60%, the default. Family b.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marked_heading_strength: Option<u8>,

    // ---- modes and loading -----------------------------------------------
    pub summary_mode: bool,
    /// The user's verbose override, or `null` to follow the setting.
    pub verbose_override: Option<bool>,
    /// The user's Simple override for this chat, or `null` to follow the setting.
    pub simple_override: Option<bool>,
    pub empty_state: EmptyState,
    pub new_session_welcome: Option<NewSessionWelcome>,
    /// `blank`, `indicator` or `retry` while the transcript loads; `null` otherwise.
    pub loading_stage: Option<String>,
    /// What the loading hold says, with its Try now button; `null` while it stays blank.
    pub loading_notice: Option<LoadingNotice>,
    /// The loading hold draws the transcript skeleton rows (`visual/transcript-skeleton.json`):
    /// Ghostex's service is not answering and the core is retrying it.
    pub transcript_skeleton: bool,
    pub skills_loading: bool,
    pub files_loading: bool,

    // ---- read aloud ---------------------------------------------------------
    /// The Read aloud controls' state (`crate::read_aloud`).
    #[serde(default)]
    pub read_aloud: crate::read_aloud::ReadAloudDocument,

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
