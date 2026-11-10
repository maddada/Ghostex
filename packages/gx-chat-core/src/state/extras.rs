//! Family f's state: the minimap, transcript search, the subagent viewer, the agent fleet and task
//! panels, the working strip, the terminal tail, the loading stage and the Save to Markdown sheet.
//!
//! **This file belongs to family f (extras).** No other family edits it.
//!
//! Read from, never write to: `ChatState::session::agent_fleet`,
//! `ChatState::session::agent_tasks`, `ChatState::session::terminal_activity` (family a folds all
//! three; an omission on a frame that can carry them means CLEARED),
//! `ChatState::session::working_started_at_ms` and the derived working flags.

use serde_json::Value;

use crate::extras::minimap_rail::MinimapMarkerRow;
pub use crate::extras::subagent_target::SubagentTarget;
use crate::extras::transcript_search::{SearchFolds, TranscriptMatch};

/// What the panels, the minimap, search, the tail, the subagent viewer and the save sheet remember
/// between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtrasState {
    /// The working strip's stint word and which stint it belongs to.
    pub working_word: WorkingWordState,
    /// The two folds above the composer.
    pub panels: PanelsState,
    /// The loading run the two stage one-shots were armed for.
    ///
    /// `trackTranscriptLoading` calls `setTimeout` ONCE per run, and the indicator's delay is 0 ms.
    /// Re-arming it on every settle put a wake that had already come due into every frame's
    /// `nextWakeMs`, where the TypeScript's one-shot had been deleted by the tick that ran it.
    pub loading_timers_armed_at_ms: Option<f64>,
    /// The minimap rail as it was last projected.
    pub minimap: Vec<MinimapMarkerRow>,
    /// Cmd+F over the transcript.
    pub search: SearchState,
    /// The session's terminal screen, read only when the user asks for it.
    pub terminal_tail: TerminalTailState,
    /// The subagent transcript viewer.
    pub subagent: SubagentState,
    /// The Save to Markdown sheet.
    pub save_markdown: SaveMarkdownState,
    /// The Session details panel (`extras/session_details/`).
    pub session_details: crate::extras::session_details::SessionDetailsState,
    /// A coordinator's relayed thread approval cards: answers in flight and refusals.
    pub relayed_approvals: crate::extras::relayed_approvals::RelayedApprovalsState,
    /// When the current transcript read started, which is what the loading stage counts from.
    pub loading_started_at_ms: Option<f64>,
    /// The stage the empty region is showing while a read runs: `blank`, `indicator` or `retry`.
    pub loading_stage: String,
    /// The working strip's activity clock, `const [now, setNow] = useState(() => Date.now())` in
    /// `activity.ts`: latched, and moved only by its effect and its one-second interval.
    pub activity_now_ms: Option<f64>,
    /// The `[activity?.detectedAt, hasClock]` the activity clock's effect last ran for.
    pub activity_clock_deps: Option<(Option<String>, bool)>,
}

/// The stint word the pinned strip shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkingWordState {
    /// The word itself, empty until the first draw.
    pub word: String,
    /// The `working` flag the last draw was made against, so a stint is only re-worded once.
    pub last_working: Option<bool>,
}

/// The agent fleet strip's and the task panel's folds.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelsState {
    /// The fleet card's override, or `None` to leave the default ("expanded unless Simple mode")
    /// to the renderer, which is the only side that knows the mode.
    pub fleet_open: Option<bool>,
    /// "I want the plan out of the way" is a preference, not a per-session view state, so this is
    /// restored from client storage and written back on every change.
    pub tasks_collapsed: bool,
    /// How many "Show more" presses the pill's done list has had, which a fresh plan or the
    /// popover closing resets.
    pub tasks_done_step: u32,
    /// The task list length the fold was last reset against; `-1` before the first projection.
    pub task_signature: i64,
    /// How far the list is unfolded: 0 in flight only, 1 with the recent ones ("Show recent"),
    /// 2+ with 30 more rows per step ("N more"). Back to 0 when its menu closes or a chat starts.
    pub threads_step: u32,
    /// The full thread list "N more" read: the revision it answers and its rows (`Null` when the
    /// read failed, so it is not retried until the list changes). Dropped when the list folds.
    pub threads_all: Option<(String, Value)>,
    /// The `readCoordinatorThreads` in flight and the revision it was asked for.
    pub threads_all_request: Option<(u64, String)>,
    /// The `openCoordinatorThread` in flight for a clicked thread row; a newer click replaces it.
    pub threads_open_request: Option<u64>,
    /// The side chat call in flight (`extras::side_chats`): Bring back or Delete, and the side
    /// chat it names.
    pub side_chat_request: Option<(u64, crate::extras::side_chats::SideChatCall)>,
    /// The secret request answers in flight (`extras::secret_requests`), by RPC id, and the
    /// last refusal of each card by its id. Never the typed value.
    pub secret_request_calls: std::collections::BTreeMap<u64, String>,
    pub secret_request_errors: std::collections::BTreeMap<String, String>,
    /// The grouped background-task row is unfolded to its tasks; folded by default, per chat.
    pub background_tasks_open: bool,
    /// The coordinator board's groups the reader folded or unfolded against their default (Done and
    /// Ideas start folded), per chat (`extras::coordinator_board`).
    pub board_toggled: std::collections::BTreeSet<String>,
    /// The finished-work cards the reader dismissed from the dock above the chat box, by card key
    /// (`extras::work_cards::dock`).
    pub work_dock_dismissed: std::collections::BTreeSet<String>,
    /// Work asks snoozed here, by card key, until when (epoch ms): shown at once, before gxserver's
    /// `snoozedUntil` comes back.
    pub work_snoozed: std::collections::BTreeMap<String, i64>,
    /// The board's groups showing every row instead of the first few.
    pub board_expanded: std::collections::BTreeSet<String>,
    /// The board changes in flight (a row moved to Removed or restored), whose refusal is toasted.
    pub board_change_requests: std::collections::BTreeSet<u64>,
    /// The answer this client sent for each board card that asks something, by card key.
    pub board_answers: std::collections::BTreeMap<String, String>,
    /// Board answers being recorded with gxserver (op `answer`), by RPC id: the card's key, the
    /// answer, and the message that goes once it is recorded.
    pub board_answer_requests: std::collections::BTreeMap<u64, (String, String, String)>,
    /// The row a board pill in the chat asked the board to show (`board:B4`), and how many times
    /// one was clicked, so the renderers open the board and scroll to it once per click.
    pub board_focus: Option<(String, u64)>,
    /// "Don't show again" was clicked on the coordinator tips here: the pill goes before gxserver
    /// confirms (`extras::coordinator_intro`), and comes back if it refuses.
    pub intro_hidden: bool,
    /// That call to gxserver while it is in flight.
    pub intro_hide_request: Option<u64>,
    /// The Autopilot pill's save in flight, and what it showed before it flipped
    /// (`extras::autopilot`).
    pub autopilot_request: Option<(u64, Option<serde_json::Value>)>,
    /// The owner's answers to guest messages in flight, by RPC id, with the message's id
    /// (`extras::collab`).
    pub collab_requests: Vec<(u64, String)>,
    /// The last typing signal this chat box sent, whether it said typing, and its call.
    pub collab_typing_at: f64,
    pub collab_typing_on: bool,
    pub collab_typing_request: Option<u64>,
}

impl Default for PanelsState {
    fn default() -> Self {
        Self {
            fleet_open: None,
            tasks_collapsed: false,
            tasks_done_step: 0,
            task_signature: -1,
            threads_step: 0,
            threads_all: None,
            threads_all_request: None,
            threads_open_request: None,
            side_chat_request: None,
            secret_request_calls: Default::default(),
            secret_request_errors: Default::default(),
            background_tasks_open: false,
            board_toggled: Default::default(),
            work_dock_dismissed: Default::default(),
            work_snoozed: Default::default(),
            board_expanded: Default::default(),
            board_change_requests: Default::default(),
            board_answers: Default::default(),
            board_answer_requests: Default::default(),
            board_focus: None,
            intro_hidden: false,
            intro_hide_request: None,
            autopilot_request: None,
            collab_requests: Vec::new(),
            collab_typing_at: 0.0,
            collab_typing_on: false,
            collab_typing_request: None,
        }
    }
}

/// Transcript search: what is typed, which occurrence is selected, and which rows carry a match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchState {
    pub open: bool,
    pub query: String,
    pub active_index: usize,
    /// Identity of the selected occurrence, so a transcript refresh keeps it instead of snapping
    /// back to the first result.
    pub active_key: Option<String>,
    /// Bumped on every explicit move, so the renderer scrolls exactly once per move.
    pub revision: u64,
    pub matches: Vec<TranscriptMatch>,
    /// Which folds the renderer has open, so only what the reader can see is searched.
    pub folds: SearchFolds,
}

/// The terminal tail reader's two independent reads: the hover verdict and the expanded sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalTailState {
    /// The newest hover read, or `None` before the first one answered.
    pub tail: Option<Value>,
    /// Whether a turn was running and a question or approval was open when `tail` answered
    /// (`extras/terminal_tail.rs` `expire`).
    pub tail_measured_during: Option<(bool, bool)>,
    /// The request id of the hover read in flight, so an older answer is dropped.
    pub hover_request: Option<u64>,
    /// When a "not ready" verdict reads the screen again by itself (`extras/terminal_tail.rs`
    /// `recheck_due`), so it clears once the input box reads again; `None` otherwise.
    pub recheck_at_ms: Option<f64>,
    /// The hover read in flight is a recheck, so its answer schedules the slower follow-up.
    pub rechecking: bool,
    pub notice_request: Option<u64>,
    /// Copy Report's read in flight (extras/report.rs).
    pub report_request: Option<u64>,
    pub notice_open: bool,
    pub notice_loading: bool,
    pub notice_error: Option<String>,
    pub notice_tail: Option<Value>,
}

/// The subagent transcript viewer's navigation stack and its current page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubagentState {
    /// The opened targets, deepest last. Empty means the viewer is closed.
    pub stack: Vec<SubagentTarget>,
    /// The page the viewer is showing, or `None` while the first read is in flight.
    pub page: Option<Value>,
    pub error: Option<String>,
    pub loading_earlier: bool,
    /// A read is in flight, which is what stops a second one starting.
    pub busy: bool,
    /// The read this viewer is waiting for; anything else is a cancelled target's answer.
    pub request: Option<SubagentRequest>,
    /// Bumped by every restart, so an in-flight read for a retired target is dropped.
    pub generation: u64,
    /// When the newest page may be re-read.
    pub poll_at_ms: Option<f64>,
    /// Whether the open child's lifecycle says it is working. The viewer's own projection keeps
    /// a landed fold sticky through a blip (`view.fold_settled_at`).
    pub working: bool,
    /// The viewer's OWN transcript projection.
    ///
    /// `native-subagent.ts:48` holds a second `NativeChatPresentation`, built fresh by every
    /// `restart()`, so the child transcript has its own `agentPath`, its own projection cache and
    /// its own placeholder queue while the session's list behind the modal keeps every row it had.
    /// The same struct carries both here; the fields the viewer never uses (the rewind sheet, the
    /// saved prompts, the deferred-work reads) stay at their defaults, because a child transcript
    /// has none of them.
    pub view: crate::state::TranscriptViewState,
}

/// One read the viewer is waiting on.
#[derive(Clone, Debug, PartialEq)]
pub struct SubagentRequest {
    pub request_id: u64,
    pub generation: u64,
    /// The read is a Load earlier, not a refresh of the newest page.
    pub earlier: bool,
    /// The page the read started from, so a late answer merges against the right one.
    pub base: Option<Value>,
    /// The window being assembled, once a refresh has started filling the gap between the newest
    /// page and the one already shown.
    pub gap: Option<SubagentGap>,
}

/// A refresh walking backwards until it reaches the page the viewer already has, so a burst of new
/// messages cannot leave a hole in the middle of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct SubagentGap {
    /// The new window, with every page read so far already prepended.
    pub page: Value,
    /// The offset the next gap read starts from.
    pub cursor: f64,
    /// Whether anything older than `cursor` is left.
    pub has_more: bool,
    /// The offset of the newest message the viewer already has, which is where the walk stops.
    pub last_offset: f64,
    /// The child the gap reads are addressed to.
    pub selector: String,
}

/// The Save to Markdown sheet's own lifecycle, which stops a stale listing replacing a typed name
/// or a later sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveMarkdownState {
    /// The open sheet, or `None` when it is closed.
    pub sheet: Option<SaveMarkdownSheet>,
    /// The Docs listing, or `None` until it answers. Submit waits for it.
    pub paths: Option<Vec<String>>,
    /// Bumped by every open and close, so an older listing or save is ignored.
    pub generation: u64,
    /// The session title the suggestion is built from.
    pub title: String,
    /// The Markdown the renderer handed over, written back byte for byte.
    pub markdown: String,
    /// The Docs listing in flight, as `(request id, generation)`.
    pub list_request: Option<(u64, u64)>,
    /// The Docs correlation ids this sheet has handed out. Monotonic, never reused, because the
    /// core may not call `crypto.randomUUID()`.
    pub docs_request_seq: u64,
    /// The Docs save in flight, and then the path read that follows it.
    pub save_request: Option<SaveMarkdownRequest>,
}

/// One Docs call the sheet is waiting on.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveMarkdownRequest {
    pub request_id: u64,
    pub generation: u64,
    /// The Docs `requestId` the answer must echo.
    pub correlation: String,
    /// The path the save was asked for, which the follow-up read resolves to an absolute one.
    pub path: String,
    /// The save itself, or the absolute-path read that follows it.
    pub stage: SaveMarkdownStage,
}

/// Which half of the two-call save is in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveMarkdownStage {
    /// `action: "save"`.
    Save,
    /// `action: "copyFullPath"`.
    ResolvePath,
}

/// The sheet's fields, which are what the document publishes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveMarkdownSheet {
    pub folder: String,
    pub file_name: String,
    /// The name is still the one this sheet suggested, so a fresh listing may replace it.
    pub suggested: bool,
    pub loading: bool,
    pub saving: bool,
    pub folder_error: Option<String>,
    pub file_name_error: Option<String>,
    pub listing_error: Option<String>,
}

/// `blank`, `indicator` or `retry` while a transcript read is running.
///
/// CDXC:SessionChat 2026-09-18 WHY:
/// React advanced the empty region through the shared loading stages on timers of its own. The
/// native chat renders whatever the document says, so the same stages are computed here.
pub const LOADING_STAGE_BLANK: &str = "blank";
pub const LOADING_STAGE_INDICATOR: &str = "indicator";
pub const LOADING_STAGE_RETRY: &str = "retry";
