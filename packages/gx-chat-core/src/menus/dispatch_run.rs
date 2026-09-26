//! One option dispatch, walked a step at a time.
//!
//! Port of `dispatchSessionChatOption`'s `run` (`option-dispatch.ts`) and of the
//! `onDispatchCommand` / `onDispatchKey` callbacks `native-host.ts` handed it, which were
//! `option-command.ts`'s `sendSessionChatOptionAware`. Those steps are `await`ed ONE AT A TIME
//! (a `command-confirm-picker` types the command and then presses Enter; a cyclic mode change
//! presses Shift+Tab as many times as the cycle is long), so the plan is a queue here rather than
//! a list of effects raised at once.
//!
//! The receipt is the pill's optimistic value: it is completed when the last step lands and rolled
//! back when any of them refuses, which is the whole reason a failed dispatch does not leave the
//! control showing a setting the agent never took.

use crate::composer::send::{send_to_agent, undo_agent_send, AgentSend};
use crate::effect::Effect;
use crate::menus::option_dispatch::DispatchStep;
use crate::menus::option_store::DispatchReceipt;
use crate::state::{ChatContext, ChatState};
use crate::wire::RpcOutcome;

/// The dispatch in flight: the steps left, and what the head one is waiting for.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionDispatchRun {
    /// The steps still to run, head first.
    pub steps: Vec<DispatchStep>,
    /// The gxserver call the head step is waiting for.
    pub request: Option<u64>,
    /// The echo or marker a `Command` step left, so a refusal can take it back.
    pub sent: Option<AgentSend>,
    /// The optimistic pill value, completed at the end and rolled back on a refusal.
    pub receipt: Option<DispatchReceipt>,
    /// The cycle turned the send hold on and owes it an off.
    pub switching: bool,
    /// `chat.availableAgents !== null`: a draft session resyncs after a typed command.
    pub refresh_after_command: bool,
}

/// Starts a plan's steps. The caller has already applied the optimistic values.
pub fn begin(
    state: &mut ChatState,
    context: &ChatContext,
    steps: Vec<DispatchStep>,
    receipt: Option<DispatchReceipt>,
) -> Vec<Effect> {
    state.menus.dispatch_run = Some(OptionDispatchRun {
        steps,
        request: None,
        sent: None,
        receipt,
        switching: false,
        refresh_after_command: state.session.available_agents.is_some(),
    });
    advance(state, context)
}

/// Runs the head step, or ends the dispatch when the queue has run out.
fn advance(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    loop {
        let Some(run) = state.menus.dispatch_run.as_mut() else {
            return Vec::new();
        };
        let Some(step) = (!run.steps.is_empty()).then(|| run.steps.remove(0)) else {
            return finish(state, context);
        };
        match step {
            // Neither of these waits for anything: the TypeScript called them synchronously
            // between two awaits, so the loop runs straight on to the next step.
            DispatchStep::Switching(switching) => {
                run.switching = switching;
                state.menus.option_switching = switching;
            }
            DispatchStep::SwitchToTerminal => {
                return vec![Effect::HostAction {
                    action: "switchToTerminal".to_string(),
                    params: Box::new(serde_json::json!({})),
                }]
                .into_iter()
                .chain(advance(state, context))
                .collect();
            }
            DispatchStep::PickModel { model, effort } => {
                let request_id = state.core.allocate_request_id();
                wait(state, request_id);
                return vec![Effect::SendRpc {
                    request_id,
                    method: crate::wire::ChatRpcMethod::SelectSessionChatModel,
                    params: Box::new(serde_json::json!({ "model": model, "effort": effort })),
                }];
            }
            DispatchStep::Command { text } => {
                let (sent, effects) = send_to_agent(state, context, &text, None, &[]);
                if let Some(run) = state.menus.dispatch_run.as_mut() {
                    run.request = Some(sent.request_id);
                    run.sent = Some(sent);
                }
                return effects;
            }
            DispatchStep::Key { key, marker } => {
                let (request_id, effects) = crate::composer::send::send_key(state, &key, &marker);
                match request_id {
                    Some(request_id) => {
                        wait(state, request_id);
                        return effects;
                    }
                    // No `sendKey` on this transport: the step is a no-op and the plan carries on,
                    // which is what `await chat.sendKey?.(…)` does with an absent method.
                    None => continue,
                }
            }
        }
    }
}

/// The head step's answer.
///
/// Answers `None` when the id is not this run's, so the same event can still reach family d's own
/// keystroke marker, which watches the very same call.
pub fn settle(
    state: &mut ChatState,
    context: &ChatContext,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    if state.menus.dispatch_run.as_ref()?.request != Some(request_id) {
        return None;
    }
    let mut effects = Vec::new();
    match outcome {
        RpcOutcome::Ok { .. } => {
            let refresh = {
                let run = state.menus.dispatch_run.as_mut()?;
                run.request = None;
                let typed = run.sent.take().is_some();
                typed && run.refresh_after_command
            };
            // `if (chat.availableAgents) chat.refresh()`: a draft session's identity only settles
            // once the daemon has seen the command.
            if refresh {
                effects.extend(crate::session::reads::request_resync(state, context));
            }
            effects.extend(advance(state, context));
        }
        RpcOutcome::Err { message, code, .. } => {
            if let Some(sent) = state
                .menus
                .dispatch_run
                .as_mut()
                .and_then(|run| run.sent.take())
            {
                undo_agent_send(state, &sent);
            }
            state.core.fail(message.clone(), code.clone());
            effects.extend(rollback(state, context));
        }
    }
    state.core.publish_after(&effects);
    Some(effects)
}

/// Every step landed: the pill keeps its new value.
fn finish(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(run) = state.menus.dispatch_run.take() else {
        return Vec::new();
    };
    if run.switching {
        state.menus.option_switching = false;
    }
    if let Some(receipt) = run.receipt {
        state.menus.options.complete(receipt, context.now_millis());
    }
    state.menus.option_dispatch_id = None;
    Vec::new()
}

/// A step refused: the pill goes back to what the agent last confirmed.
fn rollback(state: &mut ChatState, _context: &ChatContext) -> Vec<Effect> {
    let Some(run) = state.menus.dispatch_run.take() else {
        return Vec::new();
    };
    if run.switching {
        state.menus.option_switching = false;
    }
    if let Some(receipt) = run.receipt {
        state.menus.options.rollback(receipt);
    }
    state.menus.option_dispatch_id = None;
    Vec::new()
}

fn wait(state: &mut ChatState, request_id: u64) {
    if let Some(run) = state.menus.dispatch_run.as_mut() {
        run.request = Some(request_id);
        run.sent = None;
    }
}
