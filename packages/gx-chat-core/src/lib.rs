//! Platform-neutral core of the Ghostex chat: the only chat brain, behind every chat renderer.
//!
//! Inputs are [`Event`]s (gxserver chat frames, user [`UserAction`]s, clock ticks, storage
//! answers). Outputs are the [`Document`] the renderer draws plus [`Effect`]s the host performs.
//! Nothing here touches gpui, threads, sockets, the file system, or a clock; the host passes
//! `now_ms` in. That is what lets the desktop app, the mobile app (through UniFFI), and the web
//! build share one chat brain. The crate must keep building for wasm32-unknown-unknown.
//!
//! Design rules:
//!
//! - The document is the contract. Its JSON is what the GPUI renderer
//!   (`apps/desktop/src/app/native_chat/`, also compiled into `apps/gpui-web`) and the phone's
//!   native views (`apps/mobile/app/src/chat/`) read by key, so a key, its spelling or its
//!   absent-versus-null behaviour changes only together with every renderer.
//! - Absent, `null` and a value are three different things on the wire. Use `Tri` where the
//!   producer can leave a key out.
//! - Integers stay integers. A JSON `1` written back as `1.0` is a contract break.
//! - `chatLook` (`transcript/chat_look.rs`) is absent while every chat look setting is Default,
//!   which is today's chat; otherwise it is
//!   `{"messageLayout":"default|left","userStyle":"default|band","agentWork":"default|reduced|quiet","statusInTranscript":bool,"spacing"?:"compact"}`,
//!   and `messageLayout` is the effective layout (Left whenever `userStyle` is Band).
//! - With `agentWork` Reduced or Quiet, the transcript carries `grouped-work` items
//!   (`transcript/grouped_work.rs`) where it carried `completed-work`:
//!   `{"kind":"grouped-work","id":"grouped:<first turn id>","label","stats"?,"filesCard","files","filesLabel","simpleFilesLabel","expandable","work","turns","readImages"?,"workButton","workCarrier"?}`.
//!   The counts, files and work are the merged turns' added up; each `turns` entry is one turn's
//!   tail `{"id","deferred"?,"questions","sentMessages"?,"gbotCards"?,"workflows"?,"artifacts",
//!   "final"?}`, in turn order, shaped like the same-named `completed-work` fields. Opening the line sends one
//!   `loadWork` per distinct `deferred` section, keyed by the section's `startId` (several
//!   wake-up turns can share their prompt's section; its load state is `deferredWork[<startId>]`,
//!   which every turn it holds reads); Find reads the line's fold as `work:<id>`. Under Quiet the
//!   renderers draw no line: `workButton` ("24s · 5 tools · 2 files") is a button in the hover
//!   row of the reply `workCarrier` names (on a row of its own after the tails when it is absent),
//!   and it opens the same `work:<id>` fold right under that row. Every reply's own calls and
//!   edits are moved into `work` (a row with the reply's id and no words), and while the agent works
//!   the running response draws only the rows that need the user, with no calls or edits unless
//!   one waits for an approval and no words when it is kept only for an answered card
//!   (`transcript/grouped_work.rs` `hide_work`); finished rows outside grouped rows keep theirs.
//!   `workCarrier` is a final reply with actions (assistant, `actionContent.copyable`, in
//!   `finalIds`). Find walks the same order.
//! - No callbacks into the core, no generics and no lifetimes at the public boundary, so the same
//!   API works over UniFFI.
//!
//! Each directory below owns one area of the chat (session, transcript, questions, composer,
//! menus, extras); the split is described in `docs/2026-09-21/rust-chat/FAMILIES.md`. Keep a
//! rule in the directory that owns its area and keep the barrels here thin.

mod action;
pub mod composer;
mod core;
mod dispatch;
mod document;
mod effect;
mod event;
pub mod extras;
pub mod jsnum;
pub mod jstime;
pub mod menus;
pub mod query;
pub mod questions;
pub mod read_aloud;
pub mod session;
pub mod state;
pub mod transcript;
mod wire;

pub use crate::action::{ActionKind, UserAction};
pub use crate::core::{ChatCore, SentFrame};
pub use crate::dispatch::{owner, Family};
pub use crate::document::{
    assemble, frame_parts, AccountStatus, AsyncQuestions, ChatPill, ComposerActions,
    ComposerChrome, ComposerOverflow, DeferredWorkRow, Document, Draft, EmptyState, Frame,
    FrameParts, GroupedTurn, HostAction, IncomingDraft, Interaction, ItemsSplice, LoadingNotice,
    MinimapMarker, NewSessionWelcome, Note, ProjectedMessage, QuestionCard, QuestionControls,
    QuestionDraft, Queue, QueueCapabilities, QueuedPrompt, RowDetails, TerminalTail,
    TerminalTailNotice, TranscriptItem, ViewState, WorkTally, WorkingCard, WorkingStrip,
};
pub use crate::effect::{Effect, HostRequest, OpenTarget, RequestKind, StorageWrite};
pub use crate::event::{
    ChatSettings, ComposerBootRead, ConnectionUpdate, Event, KnownSession, Measurement,
    OpenRowDetail, StartConfig, StorageKey, StorageRecord,
};
pub use crate::read_aloud::{ReadAloudDocument, ReadAloudEvent, SegmentKind, SpeechSegment};
pub use crate::state::{
    ChatContext, ChatState, CommandMarker, ComposerState, CoreState, ExtrasState, FormattedTime,
    FormattedTimeStyle, FramePosition, LoadEarlierRequest, MenusState, MessagesState, PendingSend,
    PendingState, QuestionsState, ResyncState, SessionIdentity, SessionState, TerminalStream,
    TranscriptViewState,
};
pub use crate::wire::{
    ChatAppendedFrame, ChatBlock, ChatFrame, ChatFrameBase, ChatMessage, ChatRole, ChatRpcMethod,
    ChatSideState, ChatSnapshotFrame, ChatStateFrame, ChatStatus, ClientMessage,
    ReadSessionChatResult, RpcErrorCode, RpcOutcome, ServerEvent, StreamPosition, TurnLifecycle,
    CHAT_FRAME_TYPES, GXSERVER_EVENTS_STREAM_SESSION_CHAT, GXSERVER_PROTOCOL_VERSION,
};

/// Absent versus `null` versus a value, reused from the wire crate so both sides spell it once.
pub use ghostex_gx_protocol::Tri;

/// The wire types, re-exported so a host needs one dependency.
pub use ghostex_gx_protocol as protocol;

/// The chat's visual blocks (```visual fences): the scene every renderer paints, re-exported so the
/// GPUI renderers, the web build and the phone reach it through the chat core alone.
pub use ghostex_gx_visual as visual;
