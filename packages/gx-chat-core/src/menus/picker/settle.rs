//! What family e2 has to do before a document can be assembled.
//!
//! [`crate::menus::picker::document`] and [`crate::menus::context::document`] are pure, the way
//! `crate::document::assemble` needs them to be, but four things are carried rather than derived:
//! the open picker's two animations and its key highlights run on deadlines, the fork branch
//! family is read once and kept, the starred model list and the context preferences arrive from
//! storage, and the agent model catalog is pushed in. All of that is the mutating half of the
//! TypeScript's `publish` and of its timer queue, so it runs here, once per event.

use serde_json::Value;

use crate::effect::Effect;
use crate::event::Event;
use crate::menus::catalog::parse_agent_model_catalog;
use crate::menus::context::preferences::{context_preferences_key, parse_preferences};
use crate::menus::context::status::ContextDetailsAgent;
use crate::menus::picker::favorites::{parse_model_favorites, MODEL_FAVORITES_STORE};
use crate::menus::picker::fork_branches::ForkBranch;
use crate::menus::picker::model_picker::{
    ModelPickerRequest, ModelPickerSelection, ModelSelectionScope,
};
use crate::menus::picker::selection::{model_selection_unchanged, MODEL_OUTBOX_RETRY_MS};
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The picker's own deadline key in `state.core.timers`.
pub const MODEL_PICKER_TIMER: &str = "menus.picker.animation";
/// The model outbox's retry key.
pub const MODEL_OUTBOX_TIMER: &str = "menus.picker.outbox";

/// Settles family e2's carried state for this event and returns whatever it has to ask the host
/// for.
///
/// Called from family e's own uniform hook (`crate::menus::settle`), with ids from the core's one
/// allocator.
pub fn settle(
    state: &mut ChatState,
    event: &Event,
    context: &ChatContext,
    mut next_request_id: impl FnMut() -> u64,
) -> Vec<Effect> {
    let mut effects = Vec::new();
    // To fold into family e1: `currentAgentModelCatalog()` starts from the snapshot bundled with
    // the build (`agent-model-catalog-state.ts:37`), so every pill, menu and context row already
    // has a full lineup before any push arrives. `MenusState::model_catalog` starts empty
    // instead, which draws every agent as "outside the catalog". Seeding it here is the smallest
    // fix that makes the port comparable; it belongs in family e1's own boot, beside the
    // `newer()` rule that lets a pushed catalog replace it.
    if state.menus.model_catalog.agents.is_empty() {
        if let Some(bundled) = bundled_agent_model_catalog() {
            state.menus.model_catalog = bundled;
        }
    }
    // Family e1's `observe` has already rebuilt the option store this turn, so the scoped key is
    // current before the outbox is read against it.
    adopt_scoped_outbox(state);
    // CDXC:SessionChat 2026-09-22 WHY:
    // `PickersState::model_menu_context` and `catalogs` had NO writer, so every arm that reads
    // them (`toggleModelPicker`, the model menu's pick and its scope line, the picker's own
    // settle) bailed on `None` and the model picker could not be opened at all. The TypeScript
    // reads `chat.modelProvider` and `chat.sessionOptions` off `controller.current()`, a fresh
    // computation, so recomputing them once per event here is the same read.
    let inputs = crate::menus::picker::inputs::menu_inputs(state, context);
    state.pickers.model_menu_context = inputs.menu;
    state.pickers.catalogs = inputs.catalogs;
    match event {
        // The boot read's own copies (`native-host.ts:647`): `adoptAgentModelCatalog(
        // result.modelCatalog)` then `adoptNativeContextPreferences(result.contextPreferences)`,
        // both before the controller starts. The catalog the host has cached is what every pill
        // and menu draws from until a push arrives, and it can be newer than the bundled one.
        Event::ComposerBootRead(read) => {
            // `adoptAgentModelCatalog` replaces the lineup outright: the host's copy is the
            // service's, and only a gxserver push is compared by `updatedAt` (in the socket,
            // before it ever reaches the brain).
            if let Some(parsed) = parse_agent_model_catalog(&read.model_catalog) {
                state.menus.model_catalog = parsed;
                state.menus.model_catalog_generation =
                    state.menus.model_catalog_generation.wrapping_add(1);
            }
            for agent in ContextDetailsAgent::ALL {
                let value = read.context_preferences.get(agent.as_str());
                *state.pickers.context.preferences.get_mut(agent) =
                    crate::menus::context::preferences::normalize_preferences(value, agent);
            }
        }
        // To fold into family e1: `menus.model_catalog` is e1's field and this adoption belongs
        // in an e1 settle. Nothing routed `ModelCatalogChanged` anywhere, so the option catalog
        // stayed empty and every menu, pill and context row drew as "no catalog"; adopting it
        // here is what made the replay comparable at all.
        Event::ModelCatalogChanged { catalog } => {
            // The broker's `catalog` message is `adoptAgentModelCatalog(message.catalog)` too.
            if let Some(parsed) = parse_agent_model_catalog(catalog) {
                state.menus.model_catalog = parsed;
                // `adoptAgentModelCatalog` parses into a fresh object and `replaceCatalog` swaps
                // it in unconditionally, so the option store rebuilds even on an identical push.
                state.menus.model_catalog_generation =
                    state.menus.model_catalog_generation.wrapping_add(1);
            }
        }
        Event::ContextPreferencesChanged {
            provider,
            preferences,
        } => {
            let agent = ContextDetailsAgent::from_icon(Some(provider.as_str()));
            *state.pickers.context.preferences.get_mut(agent) =
                crate::menus::context::preferences::normalize_preferences(Some(preferences), agent);
        }
        Event::StorageLoaded { key, value } => {
            if key.store == MODEL_FAVORITES_STORE {
                state.pickers.model_favorites = parse_model_favorites(value.as_deref());
                state.pickers.model_favorites_loaded = true;
            }
            for agent in ContextDetailsAgent::ALL {
                if *key == context_preferences_key(agent) {
                    *state.pickers.context.preferences.get_mut(agent) =
                        parse_preferences(value.as_deref(), agent);
                }
            }
        }
        Event::StorageWritten { key, error } => {
            // `contextSave` closes the dialog when the write landed and keeps it open with the
            // message when it did not, which is the TypeScript's try/catch/finally exactly.
            if let Some(editor) = state.pickers.context.editor.as_mut() {
                if *key == context_preferences_key(editor.agent) && editor.saving {
                    editor.saving = false;
                    match error {
                        None => {
                            let agent = editor.agent;
                            let draft = editor.draft.clone();
                            *state.pickers.context.preferences.get_mut(agent) = draft;
                            state.pickers.context.editor = None;
                        }
                        Some(message) => editor.error = Some(message.clone()),
                    }
                }
            }
        }
        Event::RpcSettled {
            request_id,
            outcome,
        } => {
            if let Some(round) = settle_outbox_answer(state, context, *request_id, outcome) {
                effects.extend(round);
            } else {
                settle_fork_branches(state, *request_id, outcome);
            }
        }
        _ => {}
    }

    // `pendingModelSelection` is family a's fold; the outbox reads it on every frame.
    state
        .pickers
        .model_selection
        .adopt_pending(&state.session.pending_model_selection);
    settle_desired_receipt(state, context);

    // `forkBranches.ensure()` at the top of `publish`: once per chat, never again.
    if !state.pickers.fork_branches.asked {
        state.pickers.fork_branches.asked = true;
        state.pickers.fork_branches.request_id = Some(next_request_id());
        effects.push(Effect::SendRpc {
            request_id: state.pickers.fork_branches.request_id.unwrap_or_default(),
            method: ChatRpcMethod::SessionForkBranches,
            params: Box::new(Value::Object(Default::default())),
        });
    }

    effects.extend(settle_picker_timers(state, context));
    // A due retry clears the wait, and the delivery below picks the intent up again.
    if state.core.timer_fired(MODEL_OUTBOX_TIMER) {
        state.pickers.model_selection.retry_at_ms = None;
    }
    effects.extend(settle_outbox_delivery(state, context));
    effects.extend(arm_timers(state, context));
    effects
}

/// The one read of the branch family. An empty answer, an error, or a daemon that predates the
/// route all leave the strip unrendered rather than showing an empty menu.
fn settle_fork_branches(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) {
    if state.pickers.fork_branches.request_id != Some(request_id) {
        return;
    }
    state.pickers.fork_branches.request_id = None;
    let RpcOutcome::Ok { result } = outcome else {
        return;
    };
    let branches: Vec<ForkBranch> = result
        .get("branches")
        .and_then(|branches| serde_json::from_value(branches.clone()).ok())
        .unwrap_or_default();
    if branches.is_empty() {
        return;
    }
    state.pickers.fork_branches.branches = branches;
    // `this.republish()`: unconditional, even for a family of one that draws no strip.
    state.core.request_publish();
}

/// Runs the open picker's deadlines, and applies the choice when its close animation ends.
fn settle_picker_timers(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(picker) = state.pickers.model_picker.as_mut() else {
        return Vec::new();
    };
    let Some(outcome) = picker.expire(context.now_ms) else {
        return Vec::new();
    };
    state.pickers.model_picker = None;
    let Some(selection) = outcome.selection else {
        return Vec::new();
    };
    // The session may have changed under the picker while it was closing.
    let session_key = state
        .pickers
        .model_menu_context
        .as_ref()
        .and_then(|menu| menu.session_key.clone())
        .unwrap_or_default();
    if session_key != outcome.session_key {
        return Vec::new();
    }
    queue_model_selection(
        state,
        selection,
        Some(&outcome.request),
        outcome.scope,
        context.random_id(0),
    )
}

/// Puts a model and effort choice into the durable outbox, unless it would change nothing. Shared
/// by the quick picker's close and the model menu's keyboard pick.
pub(crate) fn queue_model_selection(
    state: &mut ChatState,
    selection: ModelPickerSelection,
    request: Option<&ModelPickerRequest>,
    scope: ModelSelectionScope,
    id: String,
) -> Vec<Effect> {
    let current_model = state
        .pickers
        .model_menu_context
        .as_ref()
        .and_then(|menu| menu.model_value.clone());
    let current_effort = state
        .pickers
        .model_menu_context
        .as_ref()
        .and_then(|menu| menu.effort_value.clone());
    if model_selection_unchanged(
        &selection,
        state.pickers.desired_selection(),
        current_model.as_deref(),
        current_effort.as_deref(),
        request,
        Some(scope),
    ) {
        return Vec::new();
    }
    // `modelSelection.select(selection, undefined, scope)`: the picker's choice goes into the
    // durable outbox, the same lane a queued option pick uses. The intent's id is the host's, so
    // the core takes it from the turn's own draw rather than inventing one.
    //
    // CDXC:SessionChat 2026-09-22 WHY:
    // This was an `Effect::HostAction { action: "selectModel" }` no host performed, so a pick made
    // in the model picker reached neither the outbox nor gxserver. Found by the desktop host agent
    // on 2026-09-22.
    let intent = state
        .pickers
        .model_selection
        .persist(selection, None, Some(scope), id);
    remember_outbox(state, Some(&intent));
    vec![Effect::WriteStorage {
        key: crate::menus::picker::selection::scoped_model_outbox_key(state),
        value: Some(crate::menus::picker::selection::serialize_model_selection_intent(&intent)),
        durable: true,
    }]
}

/// `computeModelSelectionOutbox`'s delivery `useEffect`.
///
/// While an intent sits in the outbox and the daemon can queue a model at all, it is delivered
/// with `selectSessionChatModel { defer: true }`. A refusal waits [`MODEL_OUTBOX_RETRY_MS`] and is
/// tried again; an acceptance deletes the record, which is `persistence.acknowledge`.
fn settle_outbox_delivery(state: &mut ChatState, _context: &ChatContext) -> Vec<Effect> {
    let selection = &state.pickers.model_selection;
    if selection.delivering || selection.retry_at_ms.is_some() {
        return Vec::new();
    }
    let Some(intent) = selection.outbox.clone() else {
        return Vec::new();
    };
    // `canQueue`: the daemon carries `pendingModelSelection` and the agent has a quick picker.
    if state.session.pending_model_selection.is_absent() {
        return Vec::new();
    }
    // `select({ ...params, defer: true })`: the WHOLE intent, `options` nested and `id` included.
    // gxserver reads `params.options` (`session_chat_model_selection::read_options`); spreading
    // `mode` and `fastMode` to the top level made an options-only pick (Mode, Fast) read as a pick
    // with no model and no options, which gxserver refuses as `invalidParams`, so the pick was
    // retried every five seconds and never reached the agent.
    let mut params = match serde_json::to_value(&intent) {
        Ok(Value::Object(params)) => params,
        _ => serde_json::Map::new(),
    };
    params.insert("defer".to_string(), Value::Bool(true));
    let request_id = state.core.allocate_request_id();
    state.pickers.model_selection.delivering = true;
    state.pickers.model_selection_request = Some((request_id, intent.id.clone()));
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SelectSessionChatModel,
        params: Box::new(Value::Object(params)),
    }]
}

/// The delivery answered: accepted deletes the record, refused waits and tries again.
fn settle_outbox_answer(
    state: &mut ChatState,
    context: &ChatContext,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let (pending_id, intent_id) = state.pickers.model_selection_request.clone()?;
    if pending_id != request_id {
        return None;
    }
    state.pickers.model_selection_request = None;
    let intent = state.pickers.model_selection.outbox.clone();
    let requested_options = intent
        .as_ref()
        .map(|intent| intent.options.clone())
        .unwrap_or_default();
    let requested_scope = intent.as_ref().and_then(|intent| intent.scope);
    let accepted = match outcome {
        RpcOutcome::Ok { result } => {
            crate::menus::picker::selection::accept_queued_model_selection(
                result,
                &requested_options,
                requested_scope,
            )
        }
        RpcOutcome::Err { message, .. } => Err(message.clone()),
    };
    match accepted {
        Ok(_) => {
            if state.pickers.model_selection.acknowledge(&intent_id) {
                remember_outbox(state, None);
                return Some(vec![Effect::WriteStorage {
                    key: crate::menus::picker::selection::scoped_model_outbox_key(state),
                    value: None,
                    durable: true,
                }]);
            }
            Some(Vec::new())
        }
        Err(_) => {
            state.pickers.model_selection.defer(context.now_ms);
            Some(Vec::new())
        }
    }
}

/// `computeModelSelectionOutbox`'s second `useEffect`: the pills show where the session is HEADING.
///
/// While a selection is on its way (in the outbox, or pending on the server) the option controls
/// read it rather than the agent's last confirmed values, so a pick does not snap back for the
/// round trip. The receipt completes when the selection is acknowledged, replaced by another, or
/// cleared.
fn settle_desired_receipt(state: &mut ChatState, context: &ChatContext) {
    let desired = state.pickers.model_selection.desired().cloned();
    let current = state
        .pickers
        .model_selection
        .dispatch_receipt
        .as_ref()
        .map(|(id, _)| id.clone());
    match desired {
        Some(desired) if current.as_deref() != Some(desired.id.as_str()) => {
            if let Some((_, receipt)) = state.pickers.model_selection.dispatch_receipt.take() {
                state.menus.options.complete(receipt, context.now_millis());
            }
            let mut values: Vec<(String, String)> = Vec::new();
            if !desired.model.is_empty() {
                let model_id = crate::menus::option_catalog::session_option_catalog(
                    &state.menus.model_catalog,
                    state.session.agent.as_deref(),
                )
                .map(|catalog| catalog.model.id.clone())
                .unwrap_or_else(|| "model".to_string());
                values.push((model_id, desired.model.clone()));
            }
            if !desired.effort.is_empty() {
                values.push(("effort".to_string(), desired.effort.clone()));
            }
            for (key, value) in &desired.options {
                if let Some(value) = value.as_str() {
                    values.push((key.clone(), value.to_string()));
                }
            }
            let receipt = state
                .menus
                .options
                .begin_dispatch(values, context.now_millis());
            state.pickers.model_selection.dispatch_receipt = Some((desired.id, receipt));
        }
        None => {
            if let Some((_, receipt)) = state.pickers.model_selection.dispatch_receipt.take() {
                // CDXC:SessionChat 2026-09-25 WHY:
                // A selection gxserver marks `failed` (a model the CLI does not list for this
                // account) never reached the agent, and the terminal's status line does not
                // change, so completing the receipt left the pill on the refused model for good.
                // Rolling back returns it to what the agent last showed; the menu carries the
                // reason as its "Not applied" row.
                if state.pickers.model_selection.selection_error.is_some() {
                    state.menus.options.rollback(receipt);
                } else {
                    state.menus.options.complete(receipt, context.now_millis());
                }
            }
        }
        _ => {}
    }
}

/// Arms the one timer the picker's animations and the outbox retry share.
fn arm_timers(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    match state
        .pickers
        .model_picker
        .as_ref()
        .and_then(|picker| picker.next_deadline())
    {
        Some(deadline) => state.core.timers.arm(
            MODEL_PICKER_TIMER,
            context.now_ms,
            deadline - context.now_ms,
        ),
        None => {
            state.core.timers.cancel(MODEL_PICKER_TIMER);
        }
    }
    match state.pickers.model_selection.retry_at_ms {
        Some(deadline) => state.core.timers.arm(
            MODEL_OUTBOX_TIMER,
            context.now_ms,
            deadline - context.now_ms,
        ),
        None => {
            state.core.timers.cancel(MODEL_OUTBOX_TIMER);
        }
    }
    Vec::new()
}

/// The selection an outbox retry would deliver again, after [`MODEL_OUTBOX_RETRY_MS`].
pub fn outbox_retry_selection(state: &ChatState) -> Option<ModelPickerSelection> {
    let _ = MODEL_OUTBOX_RETRY_MS;
    state
        .pickers
        .model_selection
        .outbox
        .as_ref()
        .map(|intent| intent.selection())
}

/// The `agent-model-catalog.json` snapshot bundled with the build.
///
/// SEAM.md section 4: the JSON tables are already platform neutral and must not be ported; the
/// crate includes the same file the TypeScript imports.
fn bundled_agent_model_catalog() -> Option<crate::menus::catalog::AgentModelCatalog> {
    const BUNDLED: &str = include_str!("../../../../../agent-model-catalog.json");
    parse_agent_model_catalog(&serde_json::from_str::<Value>(BUNDLED).ok()?)
}

/// The half of the composer boot read that is family e2's: the context preferences for both
/// agents, and the model-selection outbox records for this session's keys.
pub fn adopt_boot_read(state: &mut ChatState, read: &crate::event::ComposerBootRead) {
    for agent in ContextDetailsAgent::ALL {
        let stored = read.context_preferences.get(agent.as_str());
        if stored.is_some() {
            *state.pickers.context.preferences.get_mut(agent) =
                crate::menus::context::preferences::normalize_preferences(stored, agent);
        }
    }
    // `seed.modelOutboxes` is kept whole, because the key the outbox is read under is the SCOPED
    // option key and a draft session's latches to `<sessionKey>#<agentId>` a frame or two later.
    // `adopt_scoped_outbox` re-reads it whenever that key moves, which is
    // `useEffect(..., [key, persistence])` in `computeModelSelectionOutbox`.
    state.pickers.model_outboxes = read.model_outboxes.clone();
    state.pickers.model_outbox_key = None;
}

/// `seed.modelOutboxes[key] = value`: the in-memory map the scoped read is answered from, kept in
/// step with every durable write so a key that moves away and back reads what was stored.
fn remember_outbox(
    state: &mut ChatState,
    intent: Option<&crate::menus::picker::selection::ModelSelectionIntent>,
) {
    let key = crate::menus::picker::selection::scoped_outbox_key(state);
    let value = match intent {
        Some(intent) => serde_json::to_value(intent).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    };
    match state.pickers.model_outboxes.as_object_mut() {
        Some(object) => {
            object.insert(key, value);
        }
        None => {
            let mut object = serde_json::Map::new();
            object.insert(key, value);
            state.pickers.model_outboxes = serde_json::Value::Object(object);
        }
    }
}

/// `useEffect(() => { latest.current = persistence.read(key); setOutbox(...); receipt.current =
/// null; }, [key, persistence])`.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// The outbox record is keyed by the SCOPED option key (`<sessionKey>#<draftAgentId>` while a
/// draft session still offers an agent switcher), not by the session key: `native-options.ts`
/// passes `sessionOptions.sessionKey` into `computeModelSelectionOutbox`, and that is
/// `session-options.ts`'s latching `storageKey`. Reading and writing the plain key made a draft
/// session's queued model land in a record neither brain reads back. Found by the desktop host
/// agent on 2026-09-22.
pub fn adopt_scoped_outbox(state: &mut ChatState) {
    if !state.menus.options_seeded {
        return;
    }
    let key = crate::menus::picker::selection::scoped_outbox_key(state);
    if state.pickers.model_outbox_key.as_deref() == Some(key.as_str()) {
        return;
    }
    state.pickers.model_outbox_key = Some(key.clone());
    state.pickers.model_selection.outbox = state
        .pickers
        .model_outboxes
        .get(&key)
        .and_then(|record| record.get("selection").or(Some(record)))
        .filter(|record| record.is_object())
        .and_then(|record| serde_json::from_value(record.clone()).ok());
    // `receipt.current = null`: the pills stop showing the previous key's intent as dispatched.
    state.pickers.model_selection.dispatch_receipt = None;
}
