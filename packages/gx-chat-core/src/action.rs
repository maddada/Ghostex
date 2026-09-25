//! Everything the user can do in the chat, as the renderer reports it.
//!
//! The renderer sends `{"type": "<kind>", ...}`; the brain dispatches on `type` alone and reads
//! the rest per kind. The kinds and their payloads are the ones `action` in
//! `packages/shared/session-chat-controller/native-host.ts` received (plus the four sub-controllers
//! it asked first), and the senders are the `json!({"type": ...})` sites under
//! `apps/desktop/src/app/native_chat/`.
//!
//! The payload stays a free-form map on purpose: each port family types its own actions as it
//! ports their handlers, and until then an unmodelled field must survive a round trip.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One user action: its kind plus whatever fields that kind carries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserAction {
    #[serde(rename = "type")]
    pub kind: ActionKind,
    /// The kind's own fields, flattened beside `type` the way the renderer sends them.
    #[serde(flatten)]
    pub params: Map<String, Value>,
}

impl UserAction {
    /// An action with no fields.
    pub fn new(kind: ActionKind) -> Self {
        Self {
            kind,
            params: Map::new(),
        }
    }

    /// A field of the action's payload.
    pub fn param(&self, name: &str) -> Option<&Value> {
        self.params.get(name)
    }
}

macro_rules! action_kinds {
    ($($variant:ident => $wire:literal),+ $(,)?) => {
        /// The kind of a user action. Open: an unknown kind is kept verbatim, so a renderer newer
        /// than the core cannot lose an action on the way in or out.
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum ActionKind {
            $($variant,)+
            /// A kind this build does not know; kept verbatim.
            Other(String),
        }

        impl ActionKind {
            /// The wire spelling.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value.as_str(),
                }
            }

            /// Maps a wire spelling to a kind; never fails.
            pub fn from_wire(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    other => Self::Other(other.to_string()),
                }
            }

            /// Every kind this build knows, in the order the receiver handles them.
            pub const KNOWN: &'static [&'static str] = &[$($wire,)+];
        }
    };
}

action_kinds! {
    // View state the sub-controllers answer before the main switch.
    RowDetails => "rowDetails",
    ComposerScroll => "composerScroll",
    ComposerExpand => "composerExpand",
    ToggleAgentFleet => "toggleAgentFleet",
    ToggleAgentTasks => "toggleAgentTasks",
    ToggleAgentTasksCompleted => "toggleAgentTasksCompleted",
    SearchOpen => "searchOpen",
    SearchClose => "searchClose",
    SearchQuery => "searchQuery",
    SearchNext => "searchNext",
    SearchPrevious => "searchPrevious",
    TerminalTailHover => "terminalTailHover",
    TerminalTailToggle => "terminalTailToggle",
    OpenSubagent => "openSubagent",
    SubagentBack => "subagentBack",
    SubagentClose => "subagentClose",
    SubagentRetry => "subagentRetry",
    SubagentLoadEarlier => "subagentLoadEarlier",

    // Composer text, suggestions and references.
    CompleteComposerCommand => "completeComposerCommand",
    ComposerSelection => "composerSelection",
    SuggestionKey => "suggestionKey",
    SuggestionPick => "suggestionPick",
    SuggestionHighlight => "suggestionHighlight",
    SuggestionRetry => "suggestionRetry",
    SuggestionDismiss => "suggestionDismiss",
    MeasureComposer => "measureComposer",
    AppendToDraft => "appendToDraft",
    EditDraft => "editDraft",
    SaveDraft => "saveDraft",
    RecallHistory => "recallHistory",
    OpenComposerReference => "openComposerReference",
    OpenMarkdownLink => "openMarkdownLink",
    RefreshComposerChrome => "refreshComposerChrome",
    Stash => "stash",
    RestoreReturned => "restoreReturned",
    ApplyReturned => "applyReturned",
    RestoreSubmission => "restoreSubmission",

    // Attachments.
    AttachmentsStarted => "attachmentsStarted",
    AttachmentsFinished => "attachmentsFinished",
    AttachPaths => "attachPaths",
    InsertAttachments => "insertAttachments",
    RemoveAttachment => "removeAttachment",
    LoadImage => "loadImage",

    // Sending.
    Send => "send",
    Queue => "queue",
    Compact => "compact",
    SendKey => "sendKey",
    Interrupt => "interrupt",
    Handoff => "handoff",
    ReceiveHandoff => "receiveHandoff",
    DismissIncomingDraft => "dismissIncomingDraft",
    UseIncomingDraft => "useIncomingDraft",

    // The queue.
    RetryQueue => "retryQueue",
    RemoveQueue => "removeQueue",
    SendQueue => "sendQueue",
    ReorderQueue => "reorderQueue",
    MoveQueue => "moveQueue",

    // Questions, approvals and notices.
    Answer => "answer",
    QuestionText => "questionText",
    QuestionBack => "questionBack",
    QuestionOption => "questionOption",
    QuestionNext => "questionNext",
    QuestionCancel => "questionCancel",
    AsyncQuestionToggle => "asyncQuestionToggle",
    AsyncQuestionNavigate => "asyncQuestionNavigate",
    AsyncQuestionText => "asyncQuestionText",
    AsyncQuestionImagesPending => "asyncQuestionImagesPending",
    AsyncQuestionOption => "asyncQuestionOption",
    AsyncQuestionSend => "asyncQuestionSend",
    AsyncQuestionSkip => "asyncQuestionSkip",
    DismissNotice => "dismissNotice",
    NoticePrimary => "noticePrimary",
    NoticeSecondary => "noticeSecondary",

    // Options, models and accounts.
    SelectOption => "selectOption",
    ToggleModelPicker => "toggleModelPicker",
    ModelPickerMeasure => "modelPickerMeasure",
    ModelPickerPane => "modelPickerPane",
    ModelPickerKey => "modelPickerKey",
    ModelPickerKeyUp => "modelPickerKeyUp",
    ModelPickerBlur => "modelPickerBlur",
    ModelPickerControl => "modelPickerControl",
    ModelPickerScroll => "modelPickerScroll",
    ModelPickerModel => "modelPickerModel",
    ModelPickerEffort => "modelPickerEffort",
    ModelPickerCancel => "modelPickerCancel",
    ModelMenuView => "modelMenuView",
    ModelMenuFavorite => "modelMenuFavorite",
    ModelMenuPick => "modelMenuPick",
    ModelMenuTrait => "modelMenuTrait",
    Accounts => "accounts",
    SwitchDraftAgent => "switchDraftAgent",
    SelectForkBranch => "selectForkBranch",

    // Context.
    ContextEdit => "contextEdit",
    ContextCancel => "contextCancel",
    ContextQuery => "contextQuery",
    ContextShown => "contextShown",
    ContextStar => "contextStar",
    ContextReorder => "contextReorder",
    ContextReset => "contextReset",
    ContextSave => "contextSave",
    ContextCompact => "contextCompact",
    MeasureContextStatus => "measureContextStatus",

    // The session note and Save to Markdown.
    ToggleNote => "toggleNote",
    EditNote => "editNote",
    ClearNote => "clearNote",
    SaveNote => "saveNote",
    MarkdownSaveOpen => "markdownSaveOpen",
    MarkdownSaveFolder => "markdownSaveFolder",
    MarkdownSaveName => "markdownSaveName",
    MarkdownSaveCancel => "markdownSaveCancel",
    MarkdownSaveSubmit => "markdownSaveSubmit",

    // Per-message actions.
    RewindOpen => "rewindOpen",
    RewindCancel => "rewindCancel",
    RewindSubmit => "rewindSubmit",
    SavePrompt => "savePrompt",

    // Modes and loading.
    ToggleSummary => "toggleSummary",
    SetVerbose => "setVerbose",
    Retry => "retry",
    Refresh => "refresh",
    LoadEarlier => "loadEarlier",
    LoadWork => "loadWork",
}

impl Serialize for ActionKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ActionKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_wire(&String::deserialize(deserializer)?))
    }
}
