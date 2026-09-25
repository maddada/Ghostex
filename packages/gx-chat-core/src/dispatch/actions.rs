//! Which family handles which user action.
//!
//! One arm per kind, so the owner of an action is a fact you can grep rather than a guess. The
//! table follows `docs/2026-09-21/rust-chat/SEAM.md` section 4: an action belongs to the family
//! that owns the TypeScript file handling it today.
//!
//! Adding a kind means adding it to [`crate::ActionKind`] and to the arm of its owner here. Nobody
//! edits another family's arm.

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::{composer, extras, menus, questions, session, transcript};

/// The family that owns an action kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Family a: state, wire fold, merge, pagination, pending, persistence.
    Session,
    /// Family b: transcript rows.
    Transcript,
    /// Family c: questions, approvals, notices.
    Questions,
    /// Family d: composer.
    Composer,
    /// Family e: menus, pickers, options, accounts, context.
    Menus,
    /// Family f: minimap, search, subagents, panels, terminal tail, Save to Markdown.
    Extras,
}

/// Who handles this kind.
///
/// An unknown kind has no owner: the renderer is newer than this build, and the TypeScript threw
/// on one (`native-host.ts:1610`) while this drops it. That difference is deliberate; a renderer
/// must never be able to crash the brain.
pub fn owner(kind: &ActionKind) -> Option<Family> {
    Some(match kind {
        ActionKind::Retry | ActionKind::Refresh | ActionKind::LoadEarlier => Family::Session,

        ActionKind::RowDetails
        | ActionKind::ToggleSummary
        | ActionKind::SetVerbose
        | ActionKind::LoadWork
        | ActionKind::RewindOpen
        | ActionKind::RewindCancel
        | ActionKind::RewindSubmit
        | ActionKind::SavePrompt
        | ActionKind::OpenMarkdownLink
        | ActionKind::LoadImage => Family::Transcript,

        ActionKind::Answer
        | ActionKind::QuestionText
        | ActionKind::QuestionBack
        | ActionKind::QuestionOption
        | ActionKind::QuestionNext
        | ActionKind::QuestionCancel
        | ActionKind::AsyncQuestionToggle
        | ActionKind::AsyncQuestionNavigate
        | ActionKind::AsyncQuestionText
        | ActionKind::AsyncQuestionImagesPending
        | ActionKind::AsyncQuestionOption
        | ActionKind::AsyncQuestionSend
        | ActionKind::AsyncQuestionSkip
        | ActionKind::DismissNotice
        | ActionKind::NoticePrimary
        | ActionKind::NoticeSecondary => Family::Questions,

        ActionKind::ComposerScroll
        | ActionKind::ComposerExpand
        | ActionKind::CompleteComposerCommand
        | ActionKind::ComposerSelection
        | ActionKind::SuggestionKey
        | ActionKind::SuggestionPick
        | ActionKind::SuggestionHighlight
        | ActionKind::SuggestionRetry
        | ActionKind::SuggestionDismiss
        | ActionKind::MeasureComposer
        | ActionKind::AppendToDraft
        | ActionKind::EditDraft
        | ActionKind::SaveDraft
        | ActionKind::RecallHistory
        | ActionKind::OpenComposerReference
        | ActionKind::RefreshComposerChrome
        | ActionKind::Stash
        | ActionKind::RestoreReturned
        | ActionKind::ApplyReturned
        | ActionKind::RestoreSubmission
        | ActionKind::AttachmentsStarted
        | ActionKind::AttachmentsFinished
        | ActionKind::AttachPaths
        | ActionKind::InsertAttachments
        | ActionKind::RemoveAttachment
        | ActionKind::Send
        | ActionKind::Queue
        | ActionKind::Compact
        | ActionKind::SendKey
        | ActionKind::Interrupt
        | ActionKind::Handoff
        | ActionKind::ReceiveHandoff
        | ActionKind::DismissIncomingDraft
        | ActionKind::UseIncomingDraft
        | ActionKind::RetryQueue
        | ActionKind::RemoveQueue
        | ActionKind::SendQueue
        | ActionKind::ReorderQueue
        | ActionKind::MoveQueue
        | ActionKind::ToggleNote
        | ActionKind::EditNote
        | ActionKind::ClearNote
        | ActionKind::SaveNote => Family::Composer,

        ActionKind::SelectOption
        | ActionKind::ToggleModelPicker
        | ActionKind::ModelPickerMeasure
        | ActionKind::ModelPickerPane
        | ActionKind::ModelPickerKey
        | ActionKind::ModelPickerKeyUp
        | ActionKind::ModelPickerBlur
        | ActionKind::ModelPickerControl
        | ActionKind::ModelPickerScroll
        | ActionKind::ModelPickerModel
        | ActionKind::ModelPickerEffort
        | ActionKind::ModelPickerCancel
        | ActionKind::ModelMenuView
        | ActionKind::ModelMenuFavorite
        | ActionKind::ModelMenuPick
        | ActionKind::ModelMenuTrait
        | ActionKind::Accounts
        | ActionKind::SwitchDraftAgent
        | ActionKind::SelectForkBranch
        | ActionKind::ContextEdit
        | ActionKind::ContextCancel
        | ActionKind::ContextQuery
        | ActionKind::ContextShown
        | ActionKind::ContextStar
        | ActionKind::ContextReorder
        | ActionKind::ContextReset
        | ActionKind::ContextSave
        | ActionKind::ContextCompact
        | ActionKind::MeasureContextStatus => Family::Menus,

        ActionKind::ToggleAgentFleet
        | ActionKind::ToggleAgentTasks
        | ActionKind::ToggleAgentTasksCompleted
        | ActionKind::SearchOpen
        | ActionKind::SearchClose
        | ActionKind::SearchQuery
        | ActionKind::SearchNext
        | ActionKind::SearchPrevious
        | ActionKind::TerminalTailHover
        | ActionKind::TerminalTailToggle
        | ActionKind::OpenSubagent
        | ActionKind::SubagentBack
        | ActionKind::SubagentClose
        | ActionKind::SubagentRetry
        | ActionKind::SubagentLoadEarlier
        | ActionKind::MarkdownSaveOpen
        | ActionKind::MarkdownSaveFolder
        | ActionKind::MarkdownSaveName
        | ActionKind::MarkdownSaveCancel
        | ActionKind::MarkdownSaveSubmit => Family::Extras,

        ActionKind::Other(_) => return None,
    })
}

/// Routes one action to its owner.
///
/// Every arm of `action` in `packages/shared/session-chat-controller/native-host.ts` ended in a
/// publish, whatever it changed (native-host.ts:1637), so one is asked for here rather than in six
/// family directories. The arm was `async`, so that publish ran after its last `await`: an action
/// that asked the host for something publishes when the answer lands, which is what
/// [`crate::state::CoreState::publish_after`] records. The three arms that return early
/// (`composerScroll`/`composerExpand`, `rowDetails`, and the sub-controller commands) never reach
/// the closing publish; only the first of them is conditional, and its condition is a state change
/// the core's own republish rule already catches.
pub fn dispatch(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    // `if (!controller) { if (command.type === 'retry') start(bootConfig); return; }`
    // (`native-host.ts`, the top of `action`): until the boot read answers there is no controller,
    // so every gesture is dropped. The renderer's first `measureContextStatus` lands in that
    // window on every real chat, which is why the TypeScript kept its `[0]` and the core must too.
    // Every `action` call is its own promise, so what it awaits is a chain of its own. A dispatch
    // nested inside another (the controller's own `restoreReturned` effect) hands the outer chain
    // back when it returns.
    state.core.quiet_action = renders_nothing(&action.kind);
    let outer_chain = state.core.publish_chain;
    state.core.begin_publish_chain();
    let effects = dispatch_action(state, action, context);
    if outer_chain.is_some() {
        state.core.publish_chain = outer_chain;
    }
    effects
}

fn dispatch_action(
    state: &mut ChatState,
    action: &UserAction,
    context: &ChatContext,
) -> Vec<Effect> {
    if !state.core.controller_started {
        return if matches!(action.kind, ActionKind::Retry) {
            crate::session::events::restart_boot(state)
        } else {
            Vec::new()
        };
    }
    // `const clearedError = operationError !== undefined`, read just above the clear because two
    // arms publish only when it was true.
    state.core.cleared_error = state.core.operation_error.is_some();
    state.core.skip_closing_publish = false;
    state.core.effects_not_awaited = false;
    if clears_error(&action.kind) {
        state.core.clear_error();
    }
    let effects = match owner(&action.kind) {
        Some(Family::Session) => session::handle(state, action, context),
        Some(Family::Transcript) => transcript::handle(state, action, context),
        Some(Family::Questions) => questions::handle(state, action, context),
        Some(Family::Composer) => composer::handle(state, action, context),
        Some(Family::Menus) => menus::handle(state, action, context),
        Some(Family::Extras) => extras::handle(state, action, context),
        None => return Vec::new(),
    };
    if std::mem::take(&mut state.core.skip_closing_publish) {
        return effects;
    }
    // The four sub-controllers and the open-row list publish ON THE SPOT and return
    // (native-host.ts:786, :795); they never await, so their publish is not the closing one.
    if publishes_at_once(&action.kind) {
        state.core.request_publish();
        return effects;
    }
    // An arm that did not await what it raised publishes on its own turn; the answer publishes
    // again when it lands, through the owner's own `changed()`.
    let awaited =
        !std::mem::take(&mut state.core.effects_not_awaited) && state.core.publish_after(&effects);
    if publishes_on_return(&action.kind) && !awaited {
        state.core.request_publish();
    }
    effects
}

/// Whether this action clears the refusal the composer is showing.
///
/// `native-host.ts` cleared `operationError` just before its switch, so every gesture that can
/// itself refuse starts from a clean line. Two groups do not reach it: the five kinds listed
/// there, which are measurements and selection changes rather than gestures, and the arms that
/// return earlier (the composer wheel, the open-row list, and the panel, search, terminal-tail and
/// subagent sub-controllers, which are pure view state and never clear a send error).
fn clears_error(kind: &ActionKind) -> bool {
    !matches!(
        kind,
        ActionKind::RestoreSubmission
            | ActionKind::ComposerSelection
            | ActionKind::SuggestionHighlight
            | ActionKind::MeasureComposer
            | ActionKind::MeasureContextStatus
            | ActionKind::ComposerScroll
            | ActionKind::ComposerExpand
            | ActionKind::RowDetails
            | ActionKind::ToggleAgentFleet
            | ActionKind::ToggleAgentTasks
            | ActionKind::ToggleAgentTasksCompleted
            | ActionKind::SearchOpen
            | ActionKind::SearchClose
            | ActionKind::SearchQuery
            | ActionKind::SearchNext
            | ActionKind::SearchPrevious
            | ActionKind::TerminalTailHover
            | ActionKind::TerminalTailToggle
            | ActionKind::OpenSubagent
            | ActionKind::SubagentBack
            | ActionKind::SubagentClose
            | ActionKind::SubagentRetry
            | ActionKind::SubagentLoadEarlier
            | ActionKind::Other(_)
    )
}

/// Whether this kind publishes immediately and returns, rather than at the end of an async arm.
///
/// `rowDetails` and the four sub-controller commands (panels, transcript search, the terminal tail
/// and the subagent viewer) are handled before the switch and end in `publish(chat); return;`
/// (native-host.ts:783-796). An effect one of them asks for is answered later and publishes again
/// through its own `changed()` callback, not by ending this arm.
/// The kinds whose arm touched only `native-host.ts`'s module variables and published without a
/// `useState` setter, so the controller did not re-render: the at-once publishers, the composer
/// collapse, and the renderer's measurements and caret reports.
fn renders_nothing(kind: &ActionKind) -> bool {
    publishes_at_once(kind)
        || matches!(
            kind,
            ActionKind::ComposerScroll
                | ActionKind::ComposerExpand
                | ActionKind::MeasureComposer
                | ActionKind::MeasureContextStatus
                | ActionKind::ComposerSelection
        )
}

fn publishes_at_once(kind: &ActionKind) -> bool {
    matches!(
        kind,
        ActionKind::RowDetails
            | ActionKind::ToggleAgentFleet
            | ActionKind::ToggleAgentTasks
            | ActionKind::ToggleAgentTasksCompleted
            | ActionKind::SearchOpen
            | ActionKind::SearchClose
            | ActionKind::SearchQuery
            | ActionKind::SearchNext
            | ActionKind::SearchPrevious
            | ActionKind::TerminalTailHover
            | ActionKind::TerminalTailToggle
            | ActionKind::OpenSubagent
            | ActionKind::SubagentBack
            | ActionKind::SubagentClose
            | ActionKind::SubagentRetry
            | ActionKind::SubagentLoadEarlier
    )
}

/// Whether this kind reaches the closing `publish(controller.current())`.
///
/// `composerScroll` and `composerExpand` return before it and publish only when the collapse
/// actually moved, which the republish rule sees on its own.
fn publishes_on_return(kind: &ActionKind) -> bool {
    !matches!(
        kind,
        ActionKind::ComposerScroll | ActionKind::ComposerExpand | ActionKind::Other(_)
    )
}
