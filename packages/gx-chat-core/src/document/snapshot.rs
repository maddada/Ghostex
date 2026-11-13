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
    LoadingNotice, NewSessionWelcome, Note, QuestionCard, Queue, TerminalTail, ViewChip, ViewState,
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
    /// The session's Cloud Box is paused or stopped; sending a message wakes it
    /// (`session/cloud_box.rs`). The composer's placeholder already says so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cloud_box_asleep: bool,
    /// The card above the chat box while the session's computer is offline, or a message to it
    /// got no answer (`session/computer.rs`): "Windows PC is offline · last seen 2h ago" and Retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_notice: Option<crate::session::computer::ComputerNotice>,
    /// The project's GitHub issues address (`https://github.com/<owner>/<repo>/issues/`), which a
    /// `ghostex-issue:<n>` link in the Markdown points at with its number appended; absent for a
    /// project without a GitHub remote, whose links draw as plain `#<n>` (`transcript/issue_links.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_link_base: Option<String>,
    pub agent: Tri<String>,
    pub agent_session_id: Tri<String>,
    /// A draft session's agent choices. Family e.
    pub available_agents: Tri<Value>,
    /// The same-family accounts a prompted session can resume under. Family e.
    pub switchable_agents: Tri<Value>,
    /// A draft's Run on row (`menus/run_location.rs`), absent when the row is hidden. Family e.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub run_location: Tri<Value>,
    /// A draft's worktree tray (`menus/draft_worktree.rs`), absent when it is hidden. Family e.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub worktree_tray: Tri<Value>,
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
    pub deferred_work: Shared<BTreeMap<String, DeferredWorkRow>>,
    /// Claude Code workflow runs drawn as cards, by Workflow call id
    /// (`transcript/workflow_card.rs`); a tool row's `workflow` names its card. Family b.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workflow_cards: BTreeMap<String, Value>,
    /// What can change on each "GBot added" card, by change id (`transcript/gbot_cards.rs`); a
    /// card with no entry here asks for one with `gbotCard` `op: load`. Family b.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gbot_card_states: BTreeMap<String, Value>,

    // ---- composer ---------------------------------------------------------
    pub queue: Queue,
    /// The queued messages Back to composer is offered on, oldest first; the last is the one the
    /// composer's Up key takes back. Family d.
    pub queue_take_back_ids: Vec<String>,
    /// The message a Back to composer is taking back right now, drawn busy. Family d.
    pub queue_take_back_busy: Option<String>,
    /// The key Back to composer's tooltip names.
    pub queue_take_back_hotkey: String,
    /// Your messages still waiting for pictures on their way into the chat box: loading tiles in
    /// the bubble, and after a moment the spinner and Back to composer beside it
    /// (`composer/attaching.rs`). Family d.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attaching_messages: Vec<crate::composer::attaching::AttachingMessage>,
    pub draft: Draft,
    pub composer_placeholder: String,
    /// Why sending is blocked, or `null` when it is not.
    pub send_blocked_reason: Option<String>,
    /// Why a message sent now would wait in the chat instead of going at once (an account or mode
    /// switch, a dialog in the agent's terminal), or `null`. A send is still taken: it shows as
    /// waiting with these words and goes once the wait clears (composer/policy.rs `hold_words`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_hold_reason: Option<String>,
    /// The words the message that has not gone yet shows under its bubble, or `null` when none
    /// waits: the send gate's (`composer::send::held_echo`), or a send waiting to go again after
    /// a failure time can fix (`composer/delivery.rs`). The host logs when it starts and ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_held: Option<String>,
    /// How many submissions from the chat box the core has taken in (`ComposerState`).
    #[serde(default)]
    pub submissions_received: u64,
    /// The `sendRequestId`s of the chat box's sends this core has taken in, newest last
    /// (`composer/delivery.rs` `take`): the host stops sending an action again once its id is
    /// here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends_taken: Vec<String>,
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
    /// The Saved draft pill for `incoming_draft`, pinned at the left of the pill strip, or `null`.
    /// Family d.
    #[serde(default)]
    pub saved_draft_pill: Option<super::panels::SavedDraftPill>,
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
    pub question_forms: SharedForms,
    /// The form or chart a Resources row opened in the floating modal, or `null`
    /// (`questions/resource_modal.rs`); a form's picks are in `questionForms` under its id.
    #[serde(default)]
    pub resource_modal: Value,
    /// The Previous question pill and the jumps to a question (`questions/open_questions.rs`).
    #[serde(default)]
    pub open_questions: crate::questions::open_questions::OpenQuestions,
    pub notice_visible: bool,
    /// The failed queue row the visible card stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_failure_card_prompt_id: Option<String>,
    /// The card under each waiting or failed message, by its `startupDelivery.promptId`:
    /// `{ promptId, level, icon, title, sentence, buttons, menu }`, where `level` is `working`,
    /// `needs` or `failed` and each button or menu row is `{ id, label, icon?, primary?, command }`
    /// (a core action) or `{ id, label, icon?, host: "terminalView" }` (`composer/delivery_card.rs`).
    #[serde(default)]
    pub delivery_cards: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub notice_error: Tri<String>,
    /// The approval answer the drawn notice or approval card waits on: `{ label, card, sending }`
    /// (`card` is `notice` or `prompt`), drawn dimmed with a spinner; `null` otherwise
    /// (`questions/pending_approval.rs`).
    #[serde(default)]
    pub approval_pending: Value,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub operation_error: Tri<String>,
    /// The refusal's own code, so the composer can draw `composerNotReady` instead of an error
    /// line.
    pub operation_error_code: Option<String>,
    /// With an `unreachable` refusal of a send or queue: the mode (`send`, `queue`, `compact`) the
    /// error line's Retry submits the chat box with. The text went back to the box, and submitting
    /// it again reuses the failed call's `sendRequestId`, so a send gxserver did take is answered
    /// from its ledger instead of typed twice (`composer::send::submission_send_request_id`).
    /// Absent when Retry only reconnects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_error_resend: Option<String>,
    /// How the refusal is drawn: `{tone, title, chip, detail}` (`session::error_card`). Absent with
    /// no refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_error_card: Option<Value>,

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
    pub model_menu: Tri<SharedValue>,
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
    /// The card above the composer while a failed model change holds messages, absent otherwise
    /// (`menus/picker/hold_card.rs`). `body` is the plain failure sentence; `detail` retains the
    /// diagnostic for the renderer's Details disclosure. Family e.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hold_card: Option<std::sync::Arc<Value>>,

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
    /// A coordinator's board (`extras::coordinator_board`), or `null`. Family f.
    pub coordinator_board: Tri<SharedValue>,
    /// A coordinator's "How to use coordinators" tips (`extras::coordinator_intro`), or `null` where
    /// its pill does not show. Family f.
    pub coordinator_intro: Tri<Value>,
    /// The Side chats pill's menu (`extras::side_chats::project`), or `null`. Family f.
    pub side_chats_panel: Tri<Value>,
    /// A side chat's own bar (`{ parentSessionId, parentTitle }`), or `null`. Family f.
    pub side_chat_of: Tri<Value>,
    /// The private secret request cards above the composer (`extras::secret_requests::project`),
    /// an array, empty when none. Family f.
    pub secret_requests: Tri<Value>,
    /// Ghostex Collab (`extras::collab::project`): who watches and types, the owner's "Message
    /// from" cards, a guest's waiting rows and chat box note; `null` while not shared. Family f.
    pub collab: Tri<Value>,
    /// A coordinator's relayed thread approval cards (`extras::relayed_approvals::project`), an
    /// array, empty when none: drawn under each thread's report, in the Approvals pill's menu and
    /// on the board's Needs you row. Family f.
    pub relayed_approvals: Tri<Value>,
    /// A thread's "Approved from <coordinator>" lines (`extras::relayed_approvals::audit_lines`),
    /// an array, empty when none. Family f.
    pub relayed_answer_lines: Tri<Value>,
    /// A coordinator's work item cards (`extras::work_cards::project`), an array, empty when none:
    /// drawn under the reply each sits under (`anchorId`), the item's newest card in full and its
    /// earlier ones folded to one line. Family f.
    pub work_cards: Tri<SharedValue>,
    /// The question tags work items ask (`extras::work_cards::card_tags`): a one-click question or
    /// form asking only these is drawn once, as the ask card. Family f.
    pub work_ask_tags: Tri<SharedValue>,
    /// The dock above a coordinator's chat box (`extras::work_cards::dock`): `{cards, line}`, or
    /// `null` for a chat with no work. Family f.
    pub work_dock: Tri<Value>,
    /// The background tasks the agent left running (`extras::background_tasks`), which the
    /// Background pill's menu lists, or `null`. Family f.
    pub background_tasks_panel: Tri<Value>,
    /// The pills on the working card, each opening the menu of one of the panels above. Family f.
    pub chat_pills: Vec<ChatPill>,
    /// The view modes that are on, as status chips at the left of the chat box's pill strip.
    /// Family f.
    #[serde(default)]
    pub view_chips: Vec<ViewChip>,
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
    /// Whether a prompt's and a reply's menu offer Fork from here (#362): the agent's
    /// conversation can be copied up to one message (`agent_supports_fork_from_message`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fork_from_message_available: bool,
    /// Which message ids have a saved prompt, so the rail can mark them. Family b.
    pub saved_prompts: Map<String, Value>,
    /// How strongly marked sections' headings take their kind's colour, 0 to 100 percent
    /// (`transcript/marked_sections.rs`); absent at 60%, the default. Family b.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marked_heading_strength: Option<u8>,
    /// How far the chat's body text is drawn from its standard colour toward full white or black,
    /// 0 to 100 percent (`transcript/text_strength.rs`); absent at 0%, the default. Family b.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_text_strength: Option<u8>,
    /// The chat look settings (`transcript/chat_look.rs`), absent while all five are Default;
    /// `messageLayout` is the effective layout and `spacing` shows only at Compact. Family b.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_look: Option<crate::transcript::chat_look::ChatLook>,
    /// Quiet View (`transcript/quiet_view.rs`): the chat's choice, its default and the quiet
    /// lines; absent where the host sends no Quiet View settings (the phone). Family b.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_view: Option<crate::transcript::quiet_view::QuietViewDocument>,

    // ---- modes and loading -----------------------------------------------
    pub summary_mode: bool,
    /// The chat's one view level, `summary`, `normal` or `detailed` (`transcript::view_level`);
    /// the renderers draw from it rather than from the records below.
    #[serde(default)]
    pub view_level: String,
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

/// The document's `questionForms`, shared rather than copied: the core keeps the projection it
/// built and hands the same map to every document assembled while its inputs stay the same
/// (`document::AssembleCache`), so an unchanged map is neither copied, compared nor dropped value
/// by value. Serialized as the bare map.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SharedForms(pub std::sync::Arc<BTreeMap<String, Value>>);

impl PartialEq for SharedForms {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl std::ops::Deref for SharedForms {
    type Target = BTreeMap<String, Value>;

    fn deref(&self) -> &BTreeMap<String, Value> {
        &self.0
    }
}

impl From<BTreeMap<String, Value>> for SharedForms {
    fn from(forms: BTreeMap<String, Value>) -> Self {
        Self(std::sync::Arc::new(forms))
    }
}

/// A document part the state already holds behind an `Arc`, handed to every document as the same
/// allocation rather than copied, compared or dropped part by part. Serialized as the value. The
/// wire type's own (`ghostex_gx_protocol::shared::Shared`), so a frame's side data and the
/// document share one allocation.
pub use ghostex_gx_protocol::shared::Shared;

/// A document value kept by the core across documents (`document::AssembleCache`): the same value
/// is shared rather than copied, compared or dropped while its inputs hold. Serialized as the value.
pub type SharedValue = Shared<Value>;
