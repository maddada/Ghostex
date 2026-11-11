//! Family b's state: everything the transcript projection remembers between frames.
//!
//! **This file belongs to family b (transcript rows).** No other family edits it.
//!
//! Read from, never write to: `ChatState::messages` (family a owns the composed list),
//! `ChatState::session`, `ChatState::pending`.
//!
//! The three caches in `native-presentation.ts` are load bearing and the port keeps them
//! (`docs/2026-09-21/rust-chat/SEAM.md` section 2c):
//!
//!  * **Projected messages are cached by identity.** The TypeScript keys a `WeakMap` by object
//!    identity and falls back to a deep equality check on the id; a Rust core has no object
//!    identity, so [`TranscriptViewState::projected`] keeps the source message the projection was
//!    built from and a value comparison stands in for both. The rule is the same: a recreated
//!    message that is data-identical keeps its projection.
//!  * **Items keep their identity when unchanged**, so a splice carries only the changed window.
//!    In the TypeScript that is `reuseItems` handing back the previous object; here the splice is
//!    computed by comparing item values, which is the same decision without the pointer.
//!  * **Older items ship as plain-text placeholders and backfill in batches of 24.** That one is
//!    observable in the document, so it is reproduced exactly: [`TranscriptViewState::projected`]
//!    decides which rows are whole, [`TranscriptViewState::backfill`] is the queue the next batch
//!    drains, and the batch is taken from the END of the queue, newest first.

use crate::state::ChatRow;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::document::{DeferredWorkRow, RowDetails};
use crate::transcript::deferred_work::{DeferredWalk, DeferredWorkCache};
use crate::transcript::items::TranscriptItems;

/// Items from the tail that are fully projected before the first publish; the rest backfill in
/// batches of [`BACKFILL_BATCH`].
pub const EAGER_TAIL_ITEMS: usize = 12;
/// How many placeholders one backfill pass projects (`rows::advance`), newest first.
///
/// CDXC:SessionChat 2026-11-08 WHY:
/// Every pass rebuilds the transcript list and assembles the document again, which on a long chat
/// costs far more than projecting the rows themselves: at 24 per pass a 19,000-row chat scrolled
/// far up kept the chat worker busy for 15 seconds after the rows arrived (the user's app "gets
/// slower with time ... even pasting an image takes long"). 96 per pass took 8 seconds, and its
/// slowest pass was no longer than at 48.
pub const BACKFILL_BATCH: usize = 96;

/// The conversation the main transcript belongs to.
pub const ROOT_AGENT_PATH: &str = "/root";

/// What the transcript projection remembers between frames.
/// Agent work Quiet: a card the agent waits on, and the gap after it leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum QuietCard {
    #[default]
    None,
    /// A card is up.
    Up,
    /// The card left and nothing has reported since: the newest row it rests on.
    Left(String),
    /// The user stopped the agent: the newest row then, which an unanswered question no longer
    /// holds open (`working::quiet_holds_turn_open`) until a new row arrives.
    Stopped(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptViewState {
    /// Whether the transcript is folded into one row per turn.
    pub summary_mode: bool,
    /// The chat's saved `verbose` record. Verbose is retired: a chat saved on it reads as Detailed
    /// (`transcript::view_level`).
    pub verbose_override: Option<bool>,
    /// The chat's saved `simple` record: `true` is Normal, `false` Detailed, `None` follows the
    /// Default to Detailed view setting (`transcript::view_level`).
    pub simple_override: Option<bool>,
    /// The session's directory, which shortens the paths on file-change cards.
    ///
    /// A late directory throws the cached projections away, because every card's display path
    /// depends on it.
    pub working_directory: Option<String>,
    /// The conversation these messages belong to: `/root` for the session, the child's own path
    /// inside the subagent viewer.
    pub agent_path: String,
    /// The rows a `loadWork` read brought back, keyed by the turn's user-message id.
    pub deferred: BTreeMap<String, Vec<ChatRow>>,
    /// The messages whose full projection has been built, with the source each was built from.
    ///
    /// `NativeChatPresentation.modelsById`. A row not in here, and not inside the eager tail, ships
    /// as a plain-text placeholder. The model rides beside the source so an unchanged message is
    /// never parsed twice: a working session publishes once a second, and re-projecting a long
    /// transcript's markdown on every publish is exactly the cost the TypeScript cache exists to
    /// avoid.
    pub projected: BTreeMap<String, ProjectedMessage>,
    /// The rows queued for the next backfill batch, in the order the last projection met them.
    ///
    /// Messages rather than ids, as `NativeChatPresentation.backfill` holds them: a completed
    /// turn's work rows come from the `loadWork` reads and are in no list an id could be looked up
    /// in, and an id that never resolved kept the zero-delay timer firing on every tick.
    pub backfill: Vec<ChatRow>,
    /// The ids a backfill batch has projected since the queue was last empty.
    ///
    /// CDXC:SessionChat 2026-10-23 WHY: A row a batch has just projected that the next pass still
    /// finds out of date cannot be settled by another batch, and re-queuing it kept the zero-delay
    /// timer re-running the whole projection for as long as the chat was retained (a third of the
    /// chat host thread in a real profile; one cause is in `prune_cache`). Such a row is projected
    /// on the spot instead, so every batch makes progress and the queue always drains.
    pub backfilled: std::collections::BTreeSet<String>,
    /// Bumped by every applied batch, so the next publish differs and the rows ship.
    pub backfill_revision: u64,
    /// The open-row set the renderer last reported, with the detail each row asks for.
    pub open_rows: Vec<OpenRow>,
    /// The rewind sheet, or `None` when it is closed.
    pub rewind: Option<RewindRequest>,
    /// Save-prompt status by message id: `saving`, `saved` or `error`.
    pub saved_prompts: BTreeMap<String, String>,
    /// The last built item list, so a publish that changed nothing ships nothing.
    ///
    /// Shared with the frame parts and the record of what each renderer was sent, which would
    /// otherwise copy every row twice on each rebuild.
    pub items: std::sync::Arc<TranscriptItems>,
    /// The newest finished turn's "Worked for 2m 13s", which the working card above the composer shows
    /// instead of that turn's fold (`extras::working_card`), or `None`.
    pub worked_line: Option<String>,
    /// The ids of the assistant rows that carry a turn's one copy affordance.
    pub final_ids: Vec<String>,
    /// The published pages whose answers arrived (`transcript/page_answers.rs`).
    pub answered_pages: Vec<String>,
    /// The question forms the agent's replies hold (`transcript/question_forms.rs`).
    pub question_forms: crate::state::Stamped<Vec<crate::transcript::question_forms::QuestionForm>>,
    /// The answers that arrived, for pages and forms alike.
    pub page_answers: crate::state::Stamped<Vec<crate::transcript::page_answers::PageAnswers>>,
    /// The forms and question pages, oldest first (`transcript/open_questions.rs`).
    pub askers: crate::state::Stamped<Vec<crate::transcript::open_questions::Asker>>,
    /// The `loadWork` reads in flight or failed, keyed by the turn's user-message id. This is the
    /// document's `deferredWork`, which reports the READ's state, not the rows it brought back.
    pub deferred_work: std::sync::Arc<BTreeMap<String, DeferredWorkRow>>,
    /// Bumped whenever an open row's detail could have changed, so the next publish rebuilds it.
    pub detail_revision: u64,
    /// Details for the rows the renderer currently draws open.
    pub row_details: RowDetails,
    /// The `saveStashedPrompt` calls in flight, by request id, with the message they belong to.
    pub save_prompt_requests: BTreeMap<u64, String>,
    /// The `loadWork` reads in flight, by request id, with the walk that asked for them.
    pub deferred_requests: BTreeMap<u64, DeferredWalk>,
    /// Sections already walked, so reopening one is not several more round trips.
    pub deferred_cache: DeferredWorkCache,
    /// The `readSessionChatImage` reads in flight, by request id, with the path asked for.
    pub image_requests: BTreeMap<u64, String>,
    /// A digest of each picture file's bytes the chat has read, by path: a picture an agent read
    /// that is the same picture as one the user showed at another path stays out of a closed
    /// fold's strip (`transcript/read_images.rs`). Bumped `picture_digests_revision` when a new
    /// pair matches.
    pub picture_digests: std::collections::HashMap<String, u64>,
    pub picture_digests_revision: u64,
    /// The `readVisualPageThumbnail` reads in flight, by request id, with the page asked for
    /// ([`crate::transcript::page_thumbnails`]).
    pub page_thumbnail_requests: BTreeMap<u64, String>,
    /// How many times in a row each page's picture was asked for and not there yet.
    pub page_thumbnail_attempts: BTreeMap<String, u32>,
    /// The "GBot added" cards' live state, by change id ([`crate::transcript::gbot_cards`]).
    pub gbot_cards: BTreeMap<String, crate::transcript::gbot_cards::GbotCardEntry>,
    /// The `/api/gbots` calls those cards made, by request id, with the change asked about.
    pub gbot_card_requests: BTreeMap<u64, String>,
    /// The summary mode the write in flight will adopt once it lands.
    pub pending_summary_mode: Option<bool>,
    /// The verbose override the write in flight will adopt once it lands.
    pub pending_verbose_override: Option<Option<bool>>,
    /// The Simple override the write in flight will adopt once it lands.
    pub pending_simple_override: Option<Option<bool>>,
    /// The five inputs the last projection was built from, so an unchanged turn rebuilds nothing.
    pub projection_inputs: Option<ProjectionInputs>,
    /// The fold memory, `NativeChatPresentation.foldMemory`: the newest message id when the
    /// transcript last settled, which keeps a landed fold folded through a working blip
    /// ([`crate::transcript::turns::sticky_transcript_working`]).
    pub fold_settled_at: Option<String>,
    /// Agent work Quiet: where a card the agent waits on stands
    /// ([`crate::session::working::track_quiet_card`]).
    pub quiet_card: QuietCard,
    /// The projection was rebuilt and the publish that ships it has not run yet.
    pub projection_rebuilt: bool,
    /// Bumped by `ChatCore::republish` when it publishes a rebuilt projection, which is what the
    /// host's incremental channels compare on: `take` tests
    /// `sentTranscriptItems === transcriptItems`, and a rebuilt list is a new array even when
    /// every row in it was reused.
    pub projection_revision: u64,
    /// The turns before the newest accepted prompt (and the older rows of a long running turn),
    /// built once and shared by every rebuild after them (`transcript/settled.rs`), or `None` until
    /// a whole build could settle them.
    pub settled: Option<crate::transcript::settled::SettledPrefix>,
    /// The host's session list, which names the other session on a message between agents.
    pub session_directory: Vec<crate::event::KnownSession>,
    /// Bumped whenever [`Self::session_directory`] changes, so the projection rebuilds.
    pub session_directory_revision: u64,
    /// Settings > Chat > Tool call cards (`chatToolCallCards`): each message's tool calls project
    /// as one card (`transcript/tool_card.rs`). A change throws the cached projections away.
    pub tool_cards: bool,
    /// The Default to Detailed view setting, which a chat with no level of its own follows.
    pub detailed_default: bool,
    /// The chat look settings (`transcript/chat_look.rs`), all Default unless the user picked one.
    /// A change throws the cached projections away.
    pub chat_look: crate::transcript::chat_look::ChatLook,
    /// The host has sent its own chat look ([`crate::ChatSettings::chat_look`]), so a chat read's
    /// `chatLook` no longer applies (`transcript/chat_look.rs`).
    pub chat_look_from_host: bool,
    /// The Quiet View settings the host sent, or the computer's from a chat read for a host that
    /// sends none (the phone; `transcript/quiet_view.rs`); `None` keeps Quiet View off.
    pub quiet_view_settings: Option<crate::transcript::quiet_view::QuietViewSettings>,
    /// The host sent Quiet View settings of its own, which a read's never replace
    /// (`quiet_view::apply_from_read`).
    pub quiet_view_from_host: bool,
    /// This chat's own Quiet View choice from its View menu, or `None` to follow the default.
    pub quiet_view_override: Option<bool>,
    /// The Quiet View choice the write in flight will adopt once it lands.
    pub pending_quiet_view_override: Option<Option<bool>>,
    /// Quiet View is on for this chat now (`quiet_view::sync`).
    pub quiet_view_on: bool,
    /// The chat's view across devices (`transcript/view_sync.rs`).
    pub view_sync: crate::transcript::view_sync::ViewSyncState,
}

/// One message's full projection, with the source it was built from.
///
/// The source is what decides whether the entry still applies (`sameSessionChatMessage`), and what
/// an open row's detail is read from: folding merges a tool-only message INTO the assistant turn
/// above it, so the blocks a row's index points at are the anchor's merged blocks as projected, not
/// the raw message's.
///
/// CDXC:SessionChat 2026-10-24 WHY: Every drawn row held a deep copy of its cached model, so a
/// 3,400-message chat kept each model twice: 75.6 MB in this cache and 76.5 MB more in the row
/// list. A row now shares the cached model and copies it only when it changes it
/// (`Arc::make_mut`: a thinking card, the open tool call, a name on an agent-message card). Rows
/// inside a turn's work fold all carry the fold's two marks (`list_tools_in_work_fold`), so the
/// entry keeps its model the way its rows last drew it (`in_work_fold`) rather than have each of
/// them change a copy.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedMessage {
    pub source: ChatRow,
    /// The row as it arrived, when `source` is the tool calls a background row was turned into
    /// (`background_task_source`): the cache is judged by it, so an unchanged row is not turned
    /// again to check.
    pub arrived: Option<ChatRow>,
    pub model: Arc<Value>,
    /// Set while `model` carries the work-fold marks, with what they replaced, so a row drawn
    /// outside a fold gets the model exactly as projected.
    pub in_work_fold: Option<crate::transcript::tool_rows::WorkFoldMarks>,
    /// The line-break rule the model was projected under; a change of agent family re-projects.
    pub line_breaks: crate::transcript::line_breaks::AgentLineBreaks,
    /// `source` has a call that never answered (`tool_card::has_unanswered_call`), so its tool card
    /// follows whether the session is working.
    pub unanswered: bool,
    /// The tag-shaped words of the reply's prose (`transcript/open_questions.rs`), and those of
    /// them the model links to a question; a question asking one of the others re-projects.
    pub tag_words: Vec<String>,
    pub linked_tags: Vec<String>,
}

/// What `NativeChatPresentation.update` decides on.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectionInputs {
    /// The rows themselves, for a projector whose list has no identity of its own (the subagent
    /// viewer); empty for the session's transcript, whose identity is `composition_identity`.
    pub composed: Vec<ChatRow>,
    /// The composed array's identity, which the TypeScript compares instead of its rows
    /// (`MessagesState::compose_generation`: a new array whenever the memo re-ran).
    pub composition_identity: u64,
    pub working: bool,
    /// [`crate::session::working::hides_running_response`], which a lifecycle or a row can move
    /// while `working` stays the same.
    pub hides_running: bool,
    /// [`crate::session::working::quiet_finished_before`], which a new run's lifecycle can move with
    /// no row changing.
    pub quiet_finished_before: Option<i64>,
    pub summary: bool,
    pub detail_revision: u64,
    pub backfill_revision: u64,
    /// The prompt queue, which is a dependency of the composition memo
    /// (`controller.ts`, the `messages` `useMemo`) even on the turns where no row moves: a queue
    /// answer hands `setQueuePrompts` a new array, the memo re-runs, and `update` therefore ships
    /// the degenerate splice the host compares by identity.
    pub queue: Option<Vec<serde_json::Value>>,
    pub line_breaks: crate::transcript::line_breaks::AgentLineBreaks,
    /// The side question whose card is open above the composer, which the list leaves out.
    pub live_side_question: Option<String>,
    /// The tool approval the agent is blocked on, whose call the list draws open.
    pub approval_wait: Option<crate::transcript::approval_wait::ApprovalWait>,
    /// [`TranscriptViewState::session_directory_revision`]: the names on agent message cards.
    pub session_directory_revision: u64,
    /// `SessionState::agent_message_deliveries`: the statuses on sent agent message cards.
    pub agent_message_deliveries: Option<serde_json::Value>,
    /// The bar above the chat box still shows the newest finished turn's "Worked for"
    /// (`rows::worked_line_shown`), so that turn's own fold leaves its time out.
    pub worked_line_shown: bool,
    /// The forms and pages whose answers are sending or queued, which turn their Q-tags answered
    /// (or skipped) before the transcript carries the answers.
    pub answers_on_the_way: Vec<String>,
}

impl Default for TranscriptViewState {
    fn default() -> Self {
        Self {
            summary_mode: false,
            verbose_override: None,
            simple_override: None,
            working_directory: None,
            agent_path: ROOT_AGENT_PATH.to_string(),
            deferred: BTreeMap::new(),
            projected: BTreeMap::new(),
            backfill: Vec::new(),
            backfilled: std::collections::BTreeSet::new(),
            backfill_revision: 0,
            open_rows: Vec::new(),
            rewind: None,
            saved_prompts: BTreeMap::new(),
            items: Default::default(),
            worked_line: None,
            final_ids: Vec::new(),
            answered_pages: Vec::new(),
            question_forms: Default::default(),
            page_answers: Default::default(),
            askers: Default::default(),
            deferred_work: Default::default(),
            detail_revision: 0,
            row_details: RowDetails::new(),
            save_prompt_requests: BTreeMap::new(),
            deferred_requests: BTreeMap::new(),
            deferred_cache: DeferredWorkCache::default(),
            image_requests: BTreeMap::new(),
            picture_digests: Default::default(),
            picture_digests_revision: 0,
            page_thumbnail_requests: BTreeMap::new(),
            page_thumbnail_attempts: BTreeMap::new(),
            gbot_cards: BTreeMap::new(),
            gbot_card_requests: BTreeMap::new(),
            pending_summary_mode: None,
            pending_verbose_override: None,
            pending_simple_override: None,
            projection_inputs: None,
            fold_settled_at: None,
            quiet_card: QuietCard::None,
            projection_rebuilt: false,
            projection_revision: 0,
            settled: None,
            session_directory: Vec::new(),
            session_directory_revision: 0,
            tool_cards: false,
            detailed_default: false,
            chat_look: Default::default(),
            chat_look_from_host: false,
            quiet_view_settings: None,
            quiet_view_from_host: false,
            quiet_view_override: None,
            pending_quiet_view_override: None,
            quiet_view_on: false,
            view_sync: Default::default(),
        }
    }
}

impl TranscriptViewState {
    /// Throws every cached projection away, which a new working directory or a new conversation
    /// must do because both change what a projected row says.
    pub fn invalidate(&mut self) {
        self.projected.clear();
        self.backfill.clear();
        self.backfilled.clear();
        self.items = Default::default();
        self.projection_inputs = None;
        self.settled = None;
    }

    /// Whether the projection draws tool call cards: the Tool call cards setting, unless Agent work
    /// is Reduced or Quiet, which replace them (`transcript/chat_look.rs`).
    pub fn effective_tool_cards(&self) -> bool {
        self.tool_cards
            && self.chat_look.agent_work == crate::transcript::chat_look::AgentWork::Default
    }

    /// Whether Agent work is Reduced or Quiet, which fold each message's finished turns into one
    /// row (`transcript/grouped_work.rs`); Quiet also moves the replies' calls into it, and the
    /// renderers draw it as the reply's work button.
    pub fn grouped_work(&self) -> bool {
        use crate::transcript::chat_look::AgentWork;
        matches!(
            self.chat_look.agent_work,
            AgentWork::Reduced | AgentWork::Quiet
        )
    }

    /// Whether Agent work is Quiet, which keeps the work out of the chat: the work draws in each reply's hover row, never before it
    /// (`transcript/grouped_work.rs` `hide_work`).
    pub fn hidden_work(&self) -> bool {
        self.effective_chat_look().agent_work == crate::transcript::chat_look::AgentWork::Quiet
    }

    /// The chat look as the chat draws it: under Quiet View, Agent work Quiet draws as Reduced,
    /// since the quiet line already holds the work (`transcript/quiet_view.rs`).
    pub fn effective_chat_look(&self) -> crate::transcript::chat_look::ChatLook {
        use crate::transcript::chat_look::AgentWork;
        let mut look = self.chat_look;
        if self.quiet_view_on && look.agent_work == AgentWork::Quiet {
            look.agent_work = AgentWork::Reduced;
        }
        look
    }

    /// Whether any row is still drawn as a plain-text placeholder, which is what keeps the 0 ms
    /// backfill timer armed.
    pub fn has_pending_backfill(&self) -> bool {
        !self.backfill.is_empty()
    }
}

/// One open row, keyed the way the renderer keys it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRow {
    pub key: String,
    /// `file` for a change card, anything else for a tool row.
    pub kind: String,
    pub message_id: String,
    pub index: usize,
}

/// The rewind confirmation in front of `/api/rewindSessionChat`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RewindRequest {
    pub message_id: String,
    pub prompt: String,
    pub agent: String,
    pub busy: bool,
    pub completed: bool,
    pub synchronization_pending: bool,
    pub error: Option<String>,
    /// The request id in flight, so a late answer to a retired attempt is dropped.
    pub request_id: Option<u64>,
}
