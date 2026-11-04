//! The chat's state: one struct, six owners.
//!
//! Each family owns exactly one file here (`docs/2026-09-21/rust-chat/FAMILIES.md`). Adding a
//! family's own module below is the only edit anyone makes outside their own directory.

mod chat;
mod composer;
mod context;
mod extras;
mod menus;
mod messages;
mod pending;
mod pickers;
mod questions;
mod read_aloud;
mod session;
mod transcript_view;

pub use crate::state::chat::{ChatState, CoreState, PublishAwait};
pub use crate::state::composer::{ComposerState, FailedSend, Submission};
pub use crate::state::context::{ChatContext, FormattedTime, FormattedTimeStyle};
pub use crate::state::extras::{
    ExtrasState, PanelsState, SaveMarkdownRequest, SaveMarkdownSheet, SaveMarkdownStage,
    SaveMarkdownState, SearchState, SubagentGap, SubagentRequest, SubagentState, SubagentTarget,
    TerminalTailState, WorkingWordState, LOADING_STAGE_BLANK, LOADING_STAGE_INDICATOR,
    LOADING_STAGE_RETRY,
};
pub use crate::state::menus::{DraftAgentSwitch, MenusState};
pub use crate::state::messages::{
    FramePosition, LoadEarlierRequest, MessagesState, OutstandingRead, ReadKind, ResyncState,
};
pub use crate::state::pending::{CommandMarker, PendingSend, PendingState, TerminalStream};
pub use crate::state::pickers::{
    ContextPreferencesByAgent, ContextState, DraftHandoffModel, ForkBranchesState, PickersState,
};
pub use crate::state::questions::{
    AnswerRequest, AsyncQuestionsState, AsyncSubmit, FormSend, FormState, QuestionsState,
};
pub use crate::state::read_aloud::{Notice, Playing, ReadAloudState};
pub use crate::state::session::{SessionIdentity, SessionState};
pub use crate::state::transcript_view::{
    OpenRow, ProjectedMessage, ProjectionInputs, RewindRequest, TranscriptViewState,
    BACKFILL_BATCH, EAGER_TAIL_ITEMS, ROOT_AGENT_PATH,
};
