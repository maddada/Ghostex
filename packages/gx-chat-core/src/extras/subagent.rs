//! The subagent transcript viewer, ported from
//! `packages/shared/session-chat-controller/native-subagent.ts`: the navigation stack, the page
//! reads with their gap fill and poll, and the projection the native viewer paints.
//!
//! The TypeScript wrote the gap fill as an `await` loop inside one request. The core performs no
//! I/O, so the loop is turned inside out: each page is one [`crate::Effect::SendRpc`] and the walk
//! lives in [`crate::state::SubagentGap`] between the answers. The reads, their order and their
//! stopping conditions are the same.

use serde_json::{json, Map, Value};

use crate::effect::Effect;
use crate::extras::agent_fleet::subagent_model_label;
use crate::extras::subagent_target::{SubagentTarget, ROOT_AGENT_PATH};
use crate::state::{ChatContext, ChatState, SubagentGap, SubagentRequest, SubagentState};
use crate::wire::ChatRpcMethod;

/// Messages per page read, the size React's viewer asked for.
pub const PAGE_LIMIT: u32 = 300;
/// How often an open viewer re-reads its newest page.
pub const POLL_MS: f64 = 2_000.0;

/// `isOpen`: true while the viewer is the pane's modal, which is when nothing behind it takes an
/// event.
pub fn is_open(state: &SubagentState) -> bool {
    !state.stack.is_empty()
}

/// `openSubagent`: push a target and start reading it.
pub fn open(state: &mut SubagentState, target: SubagentTarget, next_request: u64) -> Vec<Effect> {
    state.stack.push(target);
    restart(state, next_request)
}

/// `subagentBack`: drop the deepest target, if there is one under it.
pub fn back(state: &mut SubagentState, next_request: u64) -> Vec<Effect> {
    if state.stack.len() > 1 {
        state.stack.pop();
        return restart(state, next_request);
    }
    Vec::new()
}

/// `subagentClose`: drop the whole stack.
pub fn close(state: &mut SubagentState, next_request: u64) -> Vec<Effect> {
    state.stack.clear();
    restart(state, next_request)
}

/// `restart`: cancel the open target's reads and start the one now on top of the stack.
fn restart(state: &mut SubagentState, next_request: u64) -> Vec<Effect> {
    state.generation += 1;
    state.request = None;
    state.busy = false;
    state.poll_at_ms = None;
    state.error = None;
    state.loading_earlier = false;
    state.working = false;
    state.page = None;
    // `this.presentation = this.projector()`: a restart builds a fresh projector, so the previous
    // target's cached projections, its placeholder queue and its agent path go with it.
    state.view = crate::state::TranscriptViewState::default();
    if state.stack.is_empty() {
        return Vec::new();
    }
    request(state, false, next_request)
}

/// `request`: read the newest page, or the one before the page already shown.
pub fn request(state: &mut SubagentState, earlier: bool, next_request: u64) -> Vec<Effect> {
    let Some(target) = state.stack.last().cloned() else {
        return Vec::new();
    };
    if state.busy || (earlier && !page_has_more(state.page.as_ref())) {
        return Vec::new();
    }
    state.busy = true;
    state.poll_at_ms = None;
    if earlier {
        state.loading_earlier = true;
    }
    let current = state.page.clone();
    let selector = current
        .as_ref()
        .and_then(|page| page.pointer("/subagent/id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(target.selector);
    let mut params = Map::new();
    params.insert("subagent".to_string(), Value::String(selector));
    params.insert("limit".to_string(), Value::from(PAGE_LIMIT));
    if earlier {
        if let Some(before) = current.as_ref().and_then(|page| page.get("beforeOffset")) {
            params.insert("beforeOffset".to_string(), before.clone());
        }
    }
    state.request = Some(SubagentRequest {
        request_id: next_request,
        generation: state.generation,
        earlier,
        base: current,
        gap: None,
    });
    vec![Effect::SendRpc {
        request_id: next_request,
        method: ChatRpcMethod::ReadSessionChat,
        params: Box::new(Value::Object(params)),
    }]
}

/// Takes the answer to one of the viewer's reads, or `None` when the id is somebody else's.
pub fn settle_rpc(
    state: &mut SubagentState,
    request_id: u64,
    outcome: Result<&Value, String>,
    context: &ChatContext,
    next_request: u64,
) -> Option<Vec<Effect>> {
    let pending = state.request.clone()?;
    if pending.request_id != request_id {
        return None;
    }
    state.request = None;
    // A restart retires the read, and its answer is dropped rather than merged.
    if pending.generation != state.generation {
        return Some(Vec::new());
    }
    let effects = match outcome {
        Ok(result) => match pending.gap.clone() {
            Some(gap) => continue_gap(state, &pending, gap, result, next_request),
            None => first_page(state, &pending, result, next_request),
        },
        Err(message) => {
            state.error = Some(message);
            Vec::new()
        }
    };
    if state.request.is_none() {
        state.busy = false;
        state.loading_earlier = false;
        state.poll_at_ms = Some(context.now_ms + POLL_MS);
    }
    Some(effects)
}

/// The answer to the read that started this request.
fn first_page(
    state: &mut SubagentState,
    pending: &SubagentRequest,
    next: &Value,
    next_request: u64,
) -> Vec<Effect> {
    // A daemon predating child reads must never paint the main conversation in the viewer.
    let Some(selector) = next
        .pointer("/subagent/id")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        state.error = Some("This server needs an update to read subagent transcripts.".to_string());
        return Vec::new();
    };
    if !pending.earlier {
        if let Some(current) = pending.base.as_ref() {
            let last_offset = messages(current)
                .last()
                .and_then(|message| message.get("byteOffset"))
                .and_then(Value::as_f64);
            let cursor = before_offset(next);
            let has_more = page_has_more(Some(next));
            if let (Some(last_offset), Some(cursor)) = (last_offset, cursor) {
                if has_more && cursor > last_offset {
                    let gap = SubagentGap {
                        page: next.clone(),
                        cursor,
                        has_more,
                        last_offset,
                        selector,
                    };
                    return read_gap(state, pending, gap, next_request);
                }
            }
        }
    }
    finish(state, pending, next.clone());
    Vec::new()
}

/// The answer to one gap read: prepend it, then either walk again or stop.
fn continue_gap(
    state: &mut SubagentState,
    pending: &SubagentRequest,
    mut gap: SubagentGap,
    read: &Value,
    next_request: u64,
) -> Vec<Effect> {
    let read_before = before_offset(read);
    // A page that does not move the cursor backwards would loop forever, so the walk stops.
    if read_before.is_none_or(|before| before >= gap.cursor) {
        return finish_gap(state, pending, gap);
    }
    let mut list = messages(read);
    list.extend(messages(&gap.page));
    set_messages(&mut gap.page, list);
    gap.cursor = read_before.unwrap_or(gap.cursor);
    gap.has_more = page_has_more(Some(read));
    if gap.has_more && gap.cursor > gap.last_offset {
        return read_gap(state, pending, gap, next_request);
    }
    finish_gap(state, pending, gap)
}

/// One more page of the walk.
fn read_gap(
    state: &mut SubagentState,
    pending: &SubagentRequest,
    gap: SubagentGap,
    next_request: u64,
) -> Vec<Effect> {
    let params = json!({
        "subagent": gap.selector,
        "limit": PAGE_LIMIT,
        "beforeOffset": gap.cursor,
    });
    state.request = Some(SubagentRequest {
        request_id: next_request,
        gap: Some(gap),
        ..pending.clone()
    });
    vec![Effect::SendRpc {
        request_id: next_request,
        method: ChatRpcMethod::ReadSessionChat,
        params: Box::new(params),
    }]
}

/// The walk is over: the assembled window takes the cursor it stopped at.
fn finish_gap(
    state: &mut SubagentState,
    pending: &SubagentRequest,
    gap: SubagentGap,
) -> Vec<Effect> {
    let mut next = gap.page;
    next["beforeOffset"] = Value::from(gap.cursor);
    next["hasMore"] = Value::Bool(gap.has_more);
    finish(state, pending, next);
    Vec::new()
}

/// The merge that ends every read; the viewer's working flag follows the page's lifecycle at once.
fn finish(state: &mut SubagentState, pending: &SubagentRequest, next: Value) {
    state.page = Some(merge_page(pending, next));
    state.error = None;
    state.working = working_lifecycle(state.page.as_ref());
}

/// The poll coming due, which the host drives with a tick.
pub fn settle_tick(
    state: &mut SubagentState,
    context: &ChatContext,
    next_request: u64,
) -> Vec<Effect> {
    match state.poll_at_ms {
        Some(due) if context.now_ms >= due && !state.stack.is_empty() => {
            request(state, false, next_request)
        }
        _ => Vec::new(),
    }
}

/// `projection`: the `subagent` document value, or `null` when the viewer is closed.
pub fn project(chat: &ChatState) -> Value {
    let state = &chat.extras.subagent;
    let Some(target) = state.stack.last() else {
        return Value::Null;
    };
    let page = state.page.as_ref();
    let info = page.and_then(|page| page.get("subagent"));
    let has_target_model = target
        .model
        .as_deref()
        .is_some_and(|model| !model.is_empty());
    let loading_model = page.is_none() && !has_target_model && state.error.is_none();
    let unavailable = state.error.is_some() && page.is_none() && !has_target_model;
    let title = if loading_model {
        "Loading\u{2026}".to_string()
    } else if unavailable {
        "Model unavailable".to_string()
    } else {
        match info {
            Some(info) => subagent_model_label(
                info.get("model").and_then(Value::as_str),
                info.get("effort").and_then(Value::as_str),
            ),
            None => subagent_model_label(target.model.as_deref(), target.effort.as_deref()),
        }
    };
    json!({
        "selector": target.selector,
        // The header's compact model and effort label; agent types stay in the tooltip.
        "title": title,
        "tooltip": info
            .and_then(|info| info.get("agentType"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| target.agent_type.clone())
            .unwrap_or_else(|| target.name.clone()),
        "description": target.task.clone().unwrap_or_else(|| "Subagent transcript".to_string()),
        "canBack": state.stack.len() > 1,
        "error": state.error.clone(),
        "loading": page.is_none() && state.error.is_none(),
        // `page?.messages.length === 0`: optional chaining short-circuits the WHOLE chain, so a
        // viewer with no page yet compares `undefined === 0` and answers false, never null.
        "empty": page.is_some_and(|page| messages(page).is_empty()),
        "hasMore": page_has_more(page) && state.error.is_none(),
        "loadingEarlier": state.loading_earlier,
    })
}

/// A nested selector is read against the transcript being shown, so a row pointing back at it is
/// not a link.
pub fn agent_path(state: &SubagentState) -> String {
    let name = state
        .page
        .as_ref()
        .and_then(|page| page.pointer("/subagent/name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name.starts_with('/') {
        name.to_string()
    } else {
        ROOT_AGENT_PATH.to_string()
    }
}

/// The merge a finished read does against the page the viewer already had.
fn merge_page(pending: &SubagentRequest, next: Value) -> Value {
    let Some(current) = pending.base.as_ref() else {
        return next;
    };
    if pending.earlier {
        let ids: Vec<String> = messages(current)
            .iter()
            .filter_map(|message| message.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        let mut merged = current.clone();
        merged["beforeOffset"] = next.get("beforeOffset").cloned().unwrap_or(Value::Null);
        merged["hasMore"] = next.get("hasMore").cloned().unwrap_or(Value::Bool(false));
        let mut list: Vec<Value> = messages(&next)
            .into_iter()
            .filter(|message| {
                !message
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| ids.iter().any(|known| known == id))
            })
            .collect();
        list.extend(messages(current));
        set_messages(&mut merged, list);
        return merged;
    }
    // A refresh keeps whatever of the shown page is older than the new window, so the list does
    // not jump back to one page while the user is reading further up.
    let next_before = before_offset(&next).unwrap_or(f64::INFINITY);
    let older: Vec<Value> = if page_has_more(Some(&next)) {
        messages(current)
            .into_iter()
            .filter(|message| {
                message
                    .get("byteOffset")
                    .and_then(Value::as_f64)
                    .unwrap_or(f64::INFINITY)
                    < next_before
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut merged = next.clone();
    if !older.is_empty() {
        merged["beforeOffset"] = current.get("beforeOffset").cloned().unwrap_or(Value::Null);
        merged["hasMore"] = current
            .get("hasMore")
            .cloned()
            .unwrap_or(Value::Bool(false));
    }
    let mut list = older;
    list.extend(messages(&next));
    set_messages(&mut merged, list);
    merged
}

/// `page.lifecycle?.state === 'working'`.
fn working_lifecycle(page: Option<&Value>) -> bool {
    page.and_then(|page| page.pointer("/lifecycle/state"))
        .and_then(Value::as_str)
        == Some("working")
}

fn page_has_more(page: Option<&Value>) -> bool {
    page.and_then(|page| page.get("hasMore")) == Some(&Value::Bool(true))
}

fn before_offset(page: &Value) -> Option<f64> {
    page.get("beforeOffset").and_then(Value::as_f64)
}

/// The page's messages, oldest first, as the read returns them.
pub fn messages(page: &Value) -> Vec<Value> {
    page.get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn set_messages(page: &mut Value, messages: Vec<Value>) {
    page["messages"] = Value::Array(messages);
}
