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
    let mut sources = state.composer.sources.clone();
    sources.agent = state.session.agent.clone();
    sources.session_agent_id = state.session.session_agent_id.clone();
    sources.available_agents = available_agents(state);
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
