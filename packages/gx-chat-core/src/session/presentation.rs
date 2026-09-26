//! `transport.presentation`: the small cache the sidebar and the chat share.
//!
//! Ported from `packages/core-ui/chat/session-chat-presentation-cache.ts` and its two writers in
//! `packages/shared/session-chat-controller/controller.ts` (`:227` for the status line's options,
//! `:378` for the agent identity). `native-host.ts:676` built the store with the cache the host
//! passed in and pushed every change back over the bridge; the core pushes it as
//! [`crate::Effect::UpdatePresentation`], which the desktop host dispatches.
//!
//! CDXC:SessionChat 2026-09-14 DECISION:
//! User: returning to a chat should immediately restore its account, context usage and status line,
//! then refresh them in the background; initial waiting belongs only to a chat that has not loaded
//! yet. Keep this small state independent of transcript retention so releasing a large
//! conversation's page does not reset its bottom bar. The core read the cache at boot and never
//! wrote it back until 2026-09-22, so the second visit was as empty as the first.

use serde_json::{Map, Value};

use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};

/// The keys the chat owns. `sessionTitle` and `workingDirectory` belong to the sidebar and are
/// carried through untouched.
const AGENT: &str = "agent";
const AGENT_SESSION_ID: &str = "agentSessionId";
const SESSION_AGENT_ID: &str = "sessionAgentId";
const SELECTED_OPTIONS: &str = "selectedOptions";
const ACCOUNTS: &str = "accounts";
/// The working directory, which family b reads to shorten a file-change card's path.
pub const WORKING_DIRECTORY: &str = "workingDirectory";

/// The store's snapshot and the serialized form its change test compares.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresentationCache {
    /// The whole cache, in the key order `JSON.stringify` would write it.
    pub snapshot: Map<String, Value>,
    /// `JSON.stringify(snapshot)` as of the last change the store reported.
    pub serialized: String,
    /// The account changed during this dispatch, so the cached accounts are no longer this
    /// session's (`controller.ts:380`, `accountChanged ? {accounts: undefined} : {}`).
    pub account_changed: bool,
}

/// `createSessionChatPresentationStore(config.initialPresentation ?? undefined, …)`, plus the
/// restore the `sessionChanged` branch of the subscribe effect does with it.
///
/// The three identity fields and the status line's options come back before anything is read, which
/// is the whole of the decision above: the bottom bar is drawn from the cache on the first frame and
/// only refreshed afterwards.
pub fn seed(state: &mut ChatState, initial: Option<&Value>) {
    let snapshot = match initial {
        Some(Value::Object(entries)) => entries.clone(),
        _ => Map::new(),
    };
    state.session.presentation.serialized =
        serde_json::to_string(&Value::Object(snapshot.clone())).unwrap_or_default();
    state.session.presentation.snapshot = snapshot;
    let cache = &state.session.presentation.snapshot;
    let text = |key: &str| {
        cache
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    state.session.agent = text(AGENT);
    state.session.agent_session_id = text(AGENT_SESSION_ID);
    state.session.session_agent_id = text(SESSION_AGENT_ID);
    state.session.selected_options = cache
        .get(SELECTED_OPTIONS)
        .filter(|options| options.is_object())
        .cloned();
    state.transcript_view.working_directory = text(WORKING_DIRECTORY);
}

/// The identity the cached transcript is only allowed to fold against.
///
/// `matchingIdentity` in `controller.ts:911`: a retained snapshot whose agent, agent session or
/// draft agent contradicts the cache belongs to a conversation this one has moved on from, so its
/// read-only draft-agent fields are left out of the fold.
pub fn identity_matches(
    state: &ChatState,
    cached: &ghostex_gx_protocol::ReadSessionChatResult,
) -> bool {
    let pairs = [
        (state.session.agent.as_deref(), cached.agent.as_deref()),
        (
            state.session.agent_session_id.as_deref(),
            cached.state.agent_session_id.as_deref(),
        ),
        (
            state.session.session_agent_id.as_deref(),
            cached.session_agent_id.as_deref(),
        ),
    ];
    pairs
        .iter()
        .all(|(known, incoming)| match (known, incoming) {
            (Some(known), Some(incoming)) => known == incoming,
            _ => true,
        })
}

/// `transport.presentation.update(...)`, run once per event over the final state.
///
/// The TypeScript wrote from two call sites, one per patch; the core writes from one, over the
/// values those patches leave behind. Two writes in one turn end at the same snapshot, and the
/// store itself only reports a change when the serialized form moved, so the effect a host performs
/// is identical.
pub fn settle(state: &mut ChatState, _context: &ChatContext) -> Vec<Effect> {
    let account_changed = std::mem::take(&mut state.session.presentation.account_changed);
    // `if (controller)`: nothing is restored or written before the boot read has answered.
    if !state.core.controller_started {
        return Vec::new();
    }
    let mut next = state.session.presentation.snapshot.clone();
    // `if (patch.agent !== undefined) next.agent = patch.agent`: the two-state fields are only ever
    // set, never cleared, so an unknown value leaves what the cache had.
    if let Some(agent) = &state.session.agent {
        next.insert(AGENT.to_string(), Value::String(agent.clone()));
    }
    if let Some(agent_session_id) = &state.session.agent_session_id {
        next.insert(
            AGENT_SESSION_ID.to_string(),
            Value::String(agent_session_id.clone()),
        );
    }
    // `sessionAgentId` is three-state: a read that omits it PROMOTES the draft, which is a real
    // `null` in the cache rather than an absent key.
    match &state.session.session_agent_id {
        Some(value) => {
            next.insert(SESSION_AGENT_ID.to_string(), Value::String(value.clone()));
        }
        None if next.contains_key(SESSION_AGENT_ID) => {
            next.insert(SESSION_AGENT_ID.to_string(), Value::Null);
        }
        None => {}
    }
    // `selectedOptions: undefined` on an identity change, which `JSON.stringify` leaves out.
    match &state.session.selected_options {
        Some(options) => {
            next.insert(SELECTED_OPTIONS.to_string(), options.clone());
        }
        None => {
            next.remove(SELECTED_OPTIONS);
        }
    }
    if account_changed {
        next.remove(ACCOUNTS);
    }
    let serialized = serde_json::to_string(&Value::Object(next.clone())).unwrap_or_default();
    if serialized == state.session.presentation.serialized {
        return Vec::new();
    }
    state.session.presentation.snapshot = next;
    state.session.presentation.serialized = serialized;
    vec![Effect::UpdatePresentation {
        state: Box::new(Value::Object(state.session.presentation.snapshot.clone())),
    }]
}
