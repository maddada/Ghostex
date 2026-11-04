//! The "what to draw" document and the frame that carries it.

mod assemble;
mod composer;
mod frame;
mod panels;
mod question;
mod queue;
mod snapshot;
mod transcript;
mod view;

pub use crate::document::assemble::{assemble, frame_parts, frame_parts_with_items, FrameParts};
pub use crate::document::composer::{
    ComposerActions, ComposerChrome, ComposerOverflow, Draft, IncomingDraft, Interaction,
    InterruptToast, Note,
};
pub use crate::document::frame::{Frame, ItemsSplice, MinimapMarker, RowDetails};
pub use crate::document::panels::{
    AccountStatus, ChatPill, DeferredWorkRow, HostAction, SavedDraftPill, TerminalTail,
    TerminalTailNotice, ViewChip, WorkingCard, WorkingStrip,
};
pub use crate::document::question::{
    AsyncQuestions, PendingAsyncQuestion, QuestionCard, QuestionControls, QuestionDraft,
};
pub use crate::document::queue::{Queue, QueueCapabilities, QueuedPrompt};
pub use crate::document::snapshot::Document;
pub use crate::document::transcript::{ProjectedMessage, TranscriptItem};
pub use crate::document::view::{EmptyState, LoadingNotice, NewSessionWelcome, ViewState};
