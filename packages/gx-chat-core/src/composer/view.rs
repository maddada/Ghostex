//! The suggestion inputs read off the whole state, in one place, so the document writer and the
//! action handler cannot disagree about what the popup is showing.
//!
//! This is the `Sources` object `NativeComposerSuggestions` was handed in
//! `packages/shared/session-chat-controller/native-suggestions.ts`, assembled from the composer's
//! own catalogs and the session's agent identity.

use crate::composer::suggestions::{
    composer_suggestions, AvailableAgent, SuggestionMatches, SuggestionSources,
};
use crate::state::ChatState;

/// The catalogs and identity the three filters read.
pub fn suggestion_sources(state: &ChatState) -> SuggestionSources {
    sources_with(state, true, true)
}

/// [`suggestion_sources`] for the chat box's own draft and caret ([`current_matches`]), which is
/// what every document assembly asks for.
///
/// CDXC:SessionChat 2026-11-26 WHY: the `+` list (every reachable session, sorted) and the `#M`
/// list (every loaded message) were built on every assembly of every chat, for a popup that is
/// closed nearly always; on a coordinator chat with thousands of loaded messages that was a steady
/// share of the chat engine's thread. [`composer_suggestions`] reads each list only while the
/// trigger at the caret is its own (`+` or `#M`), so each is built only then and the popup is the
/// same.
pub fn composer_sources(state: &ChatState) -> SuggestionSources {
    use crate::composer::trigger::{detect_composer_trigger, TriggerKind};
    let trigger = detect_composer_trigger(
        &state.composer.suggestions.text,
        Some(state.composer.suggestions.caret),
    )
    .map(|trigger| trigger.kind);
    sources_with(
        state,
        trigger == Some(TriggerKind::Session),
        trigger == Some(TriggerKind::Message),
    )
}

fn sources_with(state: &ChatState, sessions: bool, messages: bool) -> SuggestionSources {
    let mut sources = state.composer.sources.clone();
    sources.agent = state.session.agent.clone();
    sources.non_cli = state.session.non_cli_mode;
    sources.session_agent_id = state.session.session_agent_id.clone();
    sources.available_agents = available_agents(state);
    if sessions {
        sources.sessions = crate::transcript::session_mentions::session_choices(
            &state.transcript_view.session_directory,
            &state.identity.project_id,
            &state.identity.session_id,
        );
    }
    if messages {
        sources.messages =
            crate::transcript::message_numbers::message_choices(&state.messages.list);
    }
    sources.boosts = crate::extras::boosts::picker_choices(&state.extras.boosts);
    sources
}

/// The draft session's agent rows, as far as the `$` heading needs them.
fn available_agents(state: &ChatState) -> Vec<AvailableAgent> {
    state
        .session
        .available_agents
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

/// What the three filters produce for the draft and caret the SUGGESTION CONTROLLER holds.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// `NativeComposerSuggestions.matches()` read `this.text` and `this.caret`, and the only two
/// callers of `update()` were the `composerSelection` arm and `recall`; `editDraft` never touched
/// them (`native-host.ts:821`, `:1200`). Reading the composer's own text here opened the `/`
/// popup on a keystroke the TypeScript left closed, because the renderer sends the caret
/// separately from the draft write.
///
/// `canRequestSkills` is always true here: the TypeScript native host passed the same constant,
/// because the host's own skills read is always available.
pub fn current_matches(state: &ChatState, sources: &SuggestionSources) -> SuggestionMatches {
    composer_suggestions(
        &state.composer.suggestions.text,
        state.composer.suggestions.caret,
        sources,
        state.composer.suggestions.dismissed,
        true,
    )
}
