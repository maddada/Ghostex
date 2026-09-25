//! Family d's state: the composer, its drafts, the queue controls, suggestions and references.
//!
//! **This file belongs to family d (composer).** No other family edits it. It holds what
//! `packages/shared/session-chat-controller/queue.ts`, `submission.ts`, `native-suggestions.ts`,
//! `native-composer-chrome.ts`, `skills.ts`, `files.ts`, `note.ts`, `draft-handoff.ts` and the
//! plumbing's `native-composer.ts` keep: the draft text the core tracks (the host owns the text
//! field itself), the suggestion popup, the reference pills, the stash, the session note, the
//! attachment count and what is in flight.
//!
//! Read from, never write to: `ChatState::session::queue_prompts` (family a folds it; `None` is
//! the "daemon has no queue" capability probe), `ChatState::session::synced_draft`,
//! `ChatState::session::returned_prompt`, `ChatState::pending::sends` (family a owns the
//! optimistic echoes; a send adds one through family a's helper rather than by pushing here).

use crate::composer::history::ComposerHistory;
use crate::composer::layout::ComposerScrollGesture;
use crate::composer::note::{ComposerChromeState, NoteState};
use crate::composer::queue::{DraftVersion, TransportQueueMethods};
use crate::composer::storage::StoredDraftRecord;
use crate::composer::suggestions::{SuggestionDismissals, SuggestionSources};
use crate::composer::trigger::Skill;
use crate::document::{ComposerOverflow, IncomingDraft};

/// What the composer remembers between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComposerState {
    /// The draft text the host's field holds, mirrored here so every rule reads one value.
    pub text: String,
    /// The caret, in UTF-16 code units, which is what a JavaScript string index is.
    pub caret: usize,
    /// The draft revision the next save carries, `None` before the boot read answers.
    pub version: Option<DraftVersion>,
    /// The stored draft this session booted with, so a handoff can tell a stale transfer apart.
    pub stored_draft: Option<StoredDraftRecord>,
    /// The `/`, `$` and `@` popup's own state.
    pub suggestions: ComposerSuggestionState,
    /// The two catalogs the popup filters.
    pub sources: SuggestionSources,
    /// Up-arrow recall of what this composer sent.
    pub history: ComposerHistory,
    /// The session note sheet.
    pub note: NoteState,
    /// The stash badge and the note dot.
    pub chrome: ComposerChromeState,
    /// Which toolbar controls the renderer could not fit.
    pub overflow: ComposerOverflow,
    /// The box is drawn one line high.
    pub collapsed: bool,
    /// The in-flight wheel gesture over the composer.
    pub scroll: ComposerScrollGesture,
    /// Attachment reads the host has started but not finished.
    pub pending_attachments: u32,
    /// The `importNativeAttachments` reads in flight, so their answers reach this family and the
    /// `finally` that counts them back down runs exactly once each.
    pub attachment_imports: Vec<u64>,
    /// The `setSessionChatDraft` writes `saveDraft` started, so their answers reach
    /// `setSyncedDraft`. The send chain tracks its own through `Submission`.
    pub draft_pushes: Vec<u64>,
    /// How many image references the draft currently holds, which is what the thumbnails draw.
    pub draft_attachment_count: u32,
    /// A draft offered from another client, or `None`.
    pub incoming_draft: Option<IncomingDraft>,
    /// The offer's bookkeeping and the push waiting for typing to pause.
    pub draft_sync: crate::composer::draft_sync::DraftSyncState,
    /// Which queue and draft endpoints this host can actually call.
    pub transport: TransportQueueMethods,
    /// Whether this host can offer the session note, the stash, attachments and the terminal.
    pub actions: ComposerActionAvailability,
    /// A send is in flight and the composer is holding its text.
    pub submitting: Option<Submission>,
    /// The keystroke `sendKey` is waiting on: the request id, the key, and the marker to record
    /// only once the write is accepted.
    pub key_send: Option<(u64, String, String)>,
    /// Draft transfers being received right now, so a repeated `receiveHandoff` is a no-op.
    pub receiving_handoffs: Vec<String>,
    /// Draft transfers this composer has already taken, kept for the same reason across retries.
    pub received_handoffs: Vec<String>,
    /// The acknowledgement call for a transfer, by request id.
    pub handoff_acknowledgement: Option<(u64, String)>,
    /// A queue mutation in flight, and the row a remove took out.
    ///
    /// Every mutation answers with the whole authoritative queue, so the answer replaces the
    /// strip rather than patching it (`queueMutation` in `controller.ts`).
    pub queue_mutation: Option<(u64, Option<String>)>,
    /// Edit on a queued row, between its removal and the text landing in the composer.
    pub queue_edit: Option<crate::composer::queue_edit::QueueEdit>,
    /// The boot read has answered, so the two catalog reads may go out.
    pub boot_read: bool,
    /// The returned prompt whose claim is in flight: its id and the text to restore.
    ///
    /// `composer('claimReturned')` reads the applied-id list, and the restore only happens when
    /// the id was not already on it.
    pub claiming_returned: Option<(String, String)>,
    /// The returned prompt id the controller's `useEffect` last ran for (`[chat.returnedPrompt?.id]`).
    pub returned_effect_id: Option<String>,
    /// `sendBlockedReason(state)`, recomputed once per event by family d's settle.
    ///
    /// Family b's `rewindEnabled` is `sendBlockedReason(...) === null`, and `document::assemble`
    /// runs b before d, so the answer is cached on the state rather than read out of a half-built
    /// document.
    pub send_blocked_reason: Option<String>,
}

/// The `/`, `$` and `@` popup's own state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerSuggestionState {
    /// Which lists Escape closed, per trigger.
    pub dismissed: SuggestionDismissals,
    /// The highlighted row.
    pub index: usize,
    /// Whether the `$` list was active last frame, so a skills read is asked for once.
    pub skill_active: bool,
    /// Whether the `@` list was active last frame, which is what asks for the file catalog.
    pub file_active: bool,
    /// The draft and caret the dismissals were judged against.
    pub text: String,
    pub caret: usize,
}

/// Which composer controls the host can actually serve.
///
/// React gates these by only passing the handler it has; the native renderer needs the same answer
/// so a control that would do nothing stays out of the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComposerActionAvailability {
    pub summary: bool,
    pub note: bool,
    pub stash: bool,
    pub attach: bool,
    pub terminal: bool,
}

impl Default for ComposerActionAvailability {
    fn default() -> Self {
        Self {
            summary: true,
            note: false,
            stash: true,
            attach: true,
            terminal: true,
        }
    }
}

/// A submission the composer is holding text for.
///
/// The TypeScript arm awaits five or six things in a row; the core cannot await, so the same
/// order is a list of phases walked one answer at a time (`crate::composer::send`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submission {
    pub text: String,
    pub version: Option<DraftVersion>,
    pub mode: crate::composer::submission::SubmissionMode,
    /// Set by an interrupt that arrived before the send left, so the recovered draft is not
    /// delivered after it.
    pub cancelled: bool,
    /// The pictures riding with the draft.
    pub image_paths: Vec<String>,
    /// The phases still to run, head first; the head is the one in flight.
    pub phases: Vec<crate::composer::send::SendPhase>,
    /// The gxserver call the head phase is waiting for.
    pub request: Option<u64>,
    /// The stored record or flush the head phase is waiting for.
    pub storage: Option<crate::event::StorageKey>,
    /// Family a's optimistic echo, so a failed call can drop it.
    pub pending_id: Option<String>,
    /// The "Ran /x" marker's command and stamp, so a failed call can drop it.
    pub marker: Option<(String, i64)>,
    /// `chat.availableAgents !== null` when the send left: a draft session resyncs afterwards.
    pub refresh_after_send: bool,
    /// This is a `handoff`: the draft is parked for the terminal rather than delivered.
    pub handoff: bool,
    /// The head delivery phase is waiting for the send gate to clear. A send has already drawn its
    /// echo or marker, so the phase resumes without drawing it again.
    pub awaiting_gate: bool,
}

impl ComposerState {
    /// The skills the `$` list offers, or an empty slice before the read answers.
    pub fn skills(&self) -> &[Skill] {
        self.sources.skills.as_deref().unwrap_or(&[])
    }
}
