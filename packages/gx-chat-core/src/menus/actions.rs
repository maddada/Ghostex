//! Family e's user actions: the option pills, the model menu and picker, accounts, the fork
//! branch picker and the context editor.
//!
//! The model picker, model menu and fork branch kinds go to `picker::handle`, the context kinds to
//! `context::handle`: both live in family e2's subdirectories. Everything else is family e1's.

use serde_json::Value;

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::menus::option_dispatch::{
    descriptor_for, exit_plan_delivery, model_pick_scope, plan_dispatch, queue_session_chat_option,
    QueuedOption,
};
use crate::menus::options::{compute_native_chat_options, picker_supports_session_scope};
use crate::state::{ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// Handles one action family e owns.
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    match action.kind {
        ActionKind::ToggleModelPicker
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
        | ActionKind::SelectForkBranch => crate::menus::picker::handle(state, action, context),

        ActionKind::ContextEdit
        | ActionKind::ContextCancel
        | ActionKind::ContextQuery
        | ActionKind::ContextShown
        | ActionKind::ContextStar
        | ActionKind::ContextReorder
        | ActionKind::ContextReset
        | ActionKind::ContextSave
        | ActionKind::ContextCompact
        | ActionKind::MeasureContextStatus => crate::menus::context::handle(state, action, context),

        ActionKind::Accounts => accounts(state, action),
        ActionKind::SwitchDraftAgent => switch_draft_agent(state, action),
        ActionKind::SelectOption => select_option(state, action, context),

        _ => Vec::new(),
    }
}

/// `case 'accounts'`: the Switch Account panel's own requests (select, refresh, policy, stop
/// recovery, retry). The panel hands the request through unchanged.
fn accounts(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let request = action.params.get("request").cloned().unwrap_or(Value::Null);
    if request.is_null() {
        return Vec::new();
    }
    state.menus.accounts_generation += 1;
    state.menus.accounts_busy = true;
    state.menus.account_error = None;
    // The panel's own request is NOT a poll: `setInterval` keeps its own 30 second schedule and
    // only skips a tick while one read is pending. Clearing the stamp here made the poll due the
    // moment this read answered, so the panel went straight back to busy and stayed there.
    let request_id = state.core.allocate_request_id();
    state.menus.accounts_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AgentAccounts,
        params: Box::new(request),
    }]
}

/// `case 'switchDraftAgent'`: a draft session changes which agent CLI it will start.
fn switch_draft_agent(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(agent_id) = action.params.get("agentId").and_then(Value::as_str) else {
        return Vec::new();
    };
    let text = |key: &str| {
        action
            .params
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    switch_draft_agent_with(state, agent_id, text("model"), text("effort"))
}

/// `switchDraftAgent` for an agent another rule already picked.
pub fn switch_draft_agent_to(state: &mut ChatState, agent_id: &str) -> Vec<Effect> {
    switch_draft_agent_with(state, agent_id, None, None)
}

/// `switchDraftAgent` with the model and effort the launch line should carry
/// (`...(command.model ? { agentModel: command.model } : {})`, and the same for the effort).
pub fn switch_draft_agent_with(
    state: &mut ChatState,
    agent_id: &str,
    model: Option<String>,
    effort: Option<String>,
) -> Vec<Effect> {
    let known =
        crate::menus::option_menus::DraftAgent::list(state.session.available_agents.as_ref())
            .is_some_and(|agents| agents.iter().any(|agent| agent.agent_id == agent_id));
    if !known || state.session.session_agent_id.as_deref() == Some(agent_id) {
        return Vec::new();
    }
    // `await composer('flush')` comes first: the draft outbox is on disk before the agent changes
    // under it. The call itself goes out when the flush answers (`switch_after_flush`), and the
    // arm's `finally { chat.refresh() }` runs when the call answers (`switch_settled`); the
    // closing publish waits for all three, as the TypeScript's does.
    state.menus.draft_agent_switch = Some(crate::state::DraftAgentSwitch {
        agent_id: agent_id.to_string(),
        model,
        effort,
        request_id: None,
    });
    let effects = vec![Effect::FlushStorage {
        store: crate::composer::storage::DRAFTS_STORE.to_string(),
    }];
    state.core.publish_after(&effects);
    effects
}

/// The flush answered: the `switchDraftAgent` call goes out now.
pub fn switch_after_flush(
    state: &mut ChatState,
    key: &crate::event::StorageKey,
    error: Option<&str>,
) -> Vec<Effect> {
    let waiting = state
        .menus
        .draft_agent_switch
        .as_ref()
        .is_some_and(|switch| switch.request_id.is_none());
    if !waiting || key.store != crate::composer::storage::DRAFTS_STORE || !key.suffix.is_empty() {
        return Vec::new();
    }
    if let Some(message) = error {
        // A refused flush throws out of the arm before the call is made.
        state.menus.draft_agent_switch = None;
        state.core.fail(message.to_string(), None);
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    let (agent_id, model, effort) = state
        .menus
        .draft_agent_switch
        .as_ref()
        .map(|switch| {
            (
                switch.agent_id.clone(),
                switch.model.clone(),
                switch.effort.clone(),
            )
        })
        .unwrap_or_default();
    if let Some(switch) = state.menus.draft_agent_switch.as_mut() {
        switch.request_id = Some(request_id);
    }
    let mut params = serde_json::Map::new();
    params.insert("agentId".to_string(), Value::String(agent_id));
    if let Some(model) = model {
        params.insert("agentModel".to_string(), Value::String(model));
    }
    if let Some(effort) = effort {
        params.insert("agentEffort".to_string(), Value::String(effort));
    }
    let effects = vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SwitchDraftAgent,
        params: Box::new(Value::Object(params)),
    }];
    state.core.publish_after(&effects);
    effects
}

/// The call answered, refused or not: `finally { chat.refresh() }` re-reads the session.
pub fn switch_settled(
    state: &mut ChatState,
    context: &crate::state::ChatContext,
    request_id: u64,
) -> Vec<Effect> {
    if state
        .menus
        .draft_agent_switch
        .as_ref()
        .and_then(|switch| switch.request_id)
        != Some(request_id)
    {
        return Vec::new();
    }
    state.menus.draft_agent_switch = None;
    let effects = crate::session::reads::request_resync(state, context);
    // `for (const delay of [2000, 6000]) schedule(() => chat.refresh(), delay)`: two more reads
    // follow, because the daemon settles the switched agent a moment after the call answers.
    for (key, delay) in [
        crate::menus::lifecycle::REFRESH_AFTER_SWITCH_SOON,
        crate::menus::lifecycle::REFRESH_AFTER_SWITCH_LATER,
    ]
    .into_iter()
    .zip(crate::menus::lifecycle::REFRESH_AFTER_SWITCH_DELAYS_MS)
    {
        state.core.timers.arm(key, context.now_ms, delay);
    }
    state.core.publish_after(&effects);
    effects
}

/// `case 'selectOption'`: one row of a pill's menu.
///
/// The decision tree is `option-dispatch.ts`'s `plan_dispatch`; the plan's steps are then walked
/// one answer at a time by `crate::menus::dispatch_run`, because the TypeScript awaited them in
/// order (a `command-confirm-picker` types the command and only then presses Enter).
fn select_option(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    let Some(descriptor_id) = action.params.get("descriptorId").and_then(Value::as_str) else {
        return Vec::new();
    };
    let value = action
        .params
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string);
    let options = compute_native_chat_options(state, context);
    let Some(descriptor) = descriptor_for(
        options.catalog.as_ref(),
        &options.option_descriptors,
        descriptor_id,
    ) else {
        return Vec::new();
    };
    let model_id = options
        .catalog
        .as_ref()
        .map(|catalog| catalog.model.id.clone());
    let scoped = (Some(descriptor.id.as_str()) == model_id.as_deref() || descriptor.id == "effort")
        && options
            .model_provider
            .is_some_and(picker_supports_session_scope);
    // Where the agent tells the two scopes apart, picking the running model again still moves it
    // between them.
    if !scoped {
        if let Some(value) = &value {
            if options
                .state
                .get(&descriptor.id)
                .map(|entry| entry.value.as_str())
                == Some(value.as_str())
            {
                return Vec::new();
            }
        }
    }
    let exit_plan = action
        .params
        .get("exitPlan")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let delivery = if exit_plan {
        exit_plan_delivery(&descriptor)
    } else {
        descriptor.clone()
    };
    let secondary = action
        .params
        .get("secondary")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let queued = queue_session_chat_option(
        &delivery,
        value.as_deref(),
        options.catalog.as_ref(),
        &options.state,
        options.queued_controls,
        options.model_provider.is_some(),
        Some(model_pick_scope(options.model_provider, secondary)),
    );
    if let Some(queued) = queued {
        return queue_effects(state, context, queued);
    }
    if state.menus.option_dispatch_id.is_some() || crate::session::working::is_working(state) {
        return Vec::new();
    }
    let now_ms = context.now_millis();
    let plan = plan_dispatch(
        &delivery,
        value.as_deref(),
        options.catalog.as_ref(),
        &options.state,
        options.can_pick_model,
    );
    if let Some(error) = plan.error {
        state.core.fail(error, None);
        return Vec::new();
    }
    // `beginDispatch` answers the receipt the run completes or rolls back; a model picker's real
    // values are only known once the pick resolves, so it takes the later of the two.
    let mut receipt = None;
    if !plan.optimistic.is_empty() {
        receipt = Some(state.menus.options.begin_dispatch(plan.optimistic, now_ms));
    }
    if !plan.picker_optimistic.is_empty() {
        receipt = Some(
            state
                .menus
                .options
                .begin_dispatch(plan.picker_optimistic, now_ms),
        );
    }
    state.menus.option_dispatch_id = Some(descriptor.id.clone());
    crate::menus::dispatch_run::begin(state, context, plan.steps, receipt)
}

/// A choice the daemon queues rather than the TUI accepting.
///
/// `queueSessionChatOption` hands it to `modelSelection.select` / `.selectOptions`, which is the
/// durable OUTBOX, not the endpoint: the record survives a disconnect and the pills read it as
/// where the session is heading. Delivery is `native-options.ts`'s own `selectSessionChatModel`
/// lane, and it is not wired yet (see the report): nothing here calls the endpoint.
///
/// The intent's id is the host's: `crypto.randomUUID()` in the TypeScript, and
/// `ChatContext::random_id` here, which is the turn's own draw of random bits
/// (`docs/2026-09-21/rust-chat/SEAM.md` section 7.3).
fn queue_effects(
    state: &mut ChatState,
    context: &ChatContext,
    queued: QueuedOption,
) -> Vec<Effect> {
    let (selection, options, scope) = match queued {
        QueuedOption::Swallowed => return Vec::new(),
        QueuedOption::SelectOptions { mode, fast_mode } => {
            let mut options = serde_json::Map::new();
            if let Some(mode) = mode {
                options.insert("mode".to_string(), Value::String(mode));
            }
            if let Some(fast_mode) = fast_mode {
                options.insert("fastMode".to_string(), Value::String(fast_mode));
            }
            (
                state.pickers.model_selection.options_only_selection(),
                options,
                None,
            )
        }
        QueuedOption::SelectModel {
            model,
            effort,
            scope,
        } => (
            crate::menus::picker::ModelPickerSelection { model, effort },
            serde_json::Map::new(),
            scope.and_then(|scope| crate::menus::picker::ModelSelectionScope::from_wire(&scope)),
        ),
    };
    let intent = state.pickers.model_selection.persist(
        selection,
        Some(&options),
        scope,
        context.random_id(0),
    );
    vec![Effect::WriteStorage {
        key: crate::menus::picker::selection::scoped_model_outbox_key(state),
        value: Some(serde_json::to_string(&intent).unwrap_or_else(|_| "null".to_string())),
        durable: true,
    }]
}
