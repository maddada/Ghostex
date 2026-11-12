//! Long tool outputs cut for the phone's link, with what the cards read from them kept as facts.
//!
//! CDXC:SessionChat 2026-11-12 DECISION:
//! The user chose "Head and tail plus counts" for long tool outputs on the phone (Q2729): a long output costs the phone its whole text in every snapshot, frame and read although an open row shows only its first [`MAX_TOOL_RESULT_CHARS`] characters, and the user wanted every card label to stay exactly as today, with Copy fetching the full text in one round trip when asked. gxserver (which links this crate) cuts an output only for a client that asked (`toolOutputCut`): it keeps the first [`MAX_TOOL_RESULT_CHARS`] UTF-16 units, so an open row clips to the very same text, then [`SENTINEL`], the [`CutFacts`] this crate's own label code works out from the full output, [`SENTINEL`] again and the last [`TAIL_BYTES`]. The labels read the facts ([`crate::transcript::tool_card::tool_result`], [`crate::transcript::tool_card::failure_reason`]); an opened row asks gxserver for the full text (`readSessionChatToolOutput`), which its Copy then carries. An output a reader needs whole stays whole: JSON (gbot cards, MCP counts), one naming a subagent (`agentId`), and anything under [`CUT_ABOVE_UTF16`].
//! SEE-ALSO: server/src/session_chat_tool_outputs.rs (the cutting side), apps/mobile/app/src/chat/rust/machine-link.ts.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::jsstr::{js_trim, utf16_len, utf16_take};
use crate::transcript::tool_rows::MAX_TOOL_RESULT_CHARS;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// Between the kept head, the facts and the kept tail. No agent prints it.
pub const SENTINEL: &str = "\u{0}\u{1}gx-tool-output-cut\u{1}\u{0}";
/// How much of the output's end is kept.
pub const TAIL_BYTES: usize = 2048;
/// Outputs at most this long (UTF-16 units) are sent whole: cutting them would save little.
pub const CUT_ABOVE_UTF16: usize = MAX_TOOL_RESULT_CHARS + 4096;

/// What the cards read from a full output, worked out by the same functions they call.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutFacts {
    /// UTF-16 length of the full output.
    pub total: usize,
    pub command_ok: String,
    pub command_failed: String,
    pub read_lines: Option<usize>,
    pub search_matches: String,
    pub search_files: String,
    pub listed: usize,
    pub urls: usize,
    pub failure_reason: String,
}

/// Whether gxserver may cut `output`: long enough to be worth it, and not one a reader needs whole.
pub fn may_cut(output: &str) -> bool {
    if utf16_len(output) <= CUT_ABOVE_UTF16 {
        return false;
    }
    let trimmed = js_trim(output);
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return false;
    }
    !(output.contains("agentId") || output.contains("agent_id") || output.contains(SENTINEL))
}

/// `output` cut as described above, or `None` when it stays whole.
pub fn cut(output: &str) -> Option<String> {
    if !may_cut(output) {
        return None;
    }
    let facts = crate::transcript::tool_card::cut_facts(output, utf16_len(output));
    let head = utf16_take(output, MAX_TOOL_RESULT_CHARS);
    let mut tail_start = output.len().saturating_sub(TAIL_BYTES).max(head.len());
    while !output.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let facts = serde_json::to_string(&facts).ok()?;
    Some(format!(
        "{head}{SENTINEL}{facts}{SENTINEL}{}",
        &output[tail_start..]
    ))
}

/// The facts of a cut output, or `None` for a whole one.
pub fn facts(output: &str) -> Option<CutFacts> {
    let (_, rest) = output.split_once(SENTINEL)?;
    let (facts, _) = rest.split_once(SENTINEL)?;
    serde_json::from_str(facts).ok()
}

/// Whether `output` was cut.
pub fn is_cut(output: &str) -> bool {
    output.contains(SENTINEL)
}

/// Every long tool output in a chat frame or read result, cut in place. `keep` hears each
/// `(callId, full output)` it cut, so the full text can be answered later. A row of the terminal
/// tool gxserver paints (`foreign::is_terminal_tool_message`) is left whole.
pub fn cut_messages(value: &mut Value, keep: &mut dyn FnMut(&str, &str)) -> usize {
    let mut saved = 0;
    let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut) else {
        return 0;
    };
    for message in messages {
        let terminal = message
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with(crate::transcript::foreign::TERMINAL_TOOL_ID_PREFIX));
        if terminal {
            continue;
        }
        let Some(blocks) = message.get_mut("blocks").and_then(Value::as_array_mut) else {
            continue;
        };
        for block in blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool-result") {
                continue;
            }
            let Some(call_id) = block
                .get("callId")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let Some(output) = block.get("output").and_then(Value::as_str) else {
                continue;
            };
            let Some(short) = cut(output) else {
                continue;
            };
            saved += output.len().saturating_sub(short.len());
            keep(&call_id, output);
            block["output"] = Value::String(short);
        }
    }
    saved
}

/// `full` as the core would hold it: what the core put in front of the cut text it holds (the
/// "Exit code 1" line `command_exits` adds to a failed command) goes in front of the full text too.
pub fn as_held(held_cut: &str, full: &str) -> String {
    let start = utf16_take(full, 64);
    match held_cut.find(start) {
        Some(at) if at > 0 => format!("{}{full}", &held_cut[..at]),
        _ => full.to_string(),
    }
}

/// The full text of a cut output once it has been read, else `None`.
pub fn full_output<'a>(
    state_full: &'a HashMap<String, String>,
    call_id: Option<&str>,
) -> Option<&'a str> {
    call_id
        .and_then(|id| state_full.get(id))
        .map(String::as_str)
}

/// The rows the renderer holds open whose cut output has not been asked for yet: one
/// `readSessionChatToolOutput` each.
pub fn request_open_outputs(state: &mut ChatState) -> Vec<Effect> {
    let wanted: Vec<String> = state
        .transcript_view
        .open_rows
        .iter()
        .filter(|open| open.kind == "tool")
        .filter_map(|open| {
            crate::transcript::presentation::open_tool_call_id(state, &open.message_id, open.index)
        })
        .collect();
    let mut effects = Vec::new();
    for call_id in wanted {
        let view = &state.transcript_view;
        if view.full_tool_outputs.contains_key(&call_id)
            || view
                .tool_output_requests
                .values()
                .any(|asked| asked == &call_id)
        {
            continue;
        }
        let request_id = state.core.allocate_request_id();
        state
            .transcript_view
            .tool_output_requests
            .insert(request_id, call_id.clone());
        let mut params = Map::new();
        params.insert("projectId".into(), json!(state.identity.project_id));
        params.insert("sessionId".into(), json!(state.identity.session_id));
        params.insert("callId".into(), json!(call_id));
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::ReadSessionChatToolOutput,
            params: Box::new(Value::Object(params)),
        });
    }
    effects
}

/// A `readSessionChatToolOutput` answer: true when this request was one, so the caller stops.
/// A refusal forgets the request, so opening the row again asks again.
pub fn settle(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
    context: &ChatContext,
) -> bool {
    let Some(call_id) = state
        .transcript_view
        .tool_output_requests
        .remove(&request_id)
    else {
        return false;
    };
    if let RpcOutcome::Ok { result } = outcome {
        if let Some(output) = result.get("output").and_then(Value::as_str) {
            state
                .transcript_view
                .full_tool_outputs
                .insert(call_id, output.to_string());
            state.transcript_view.row_details =
                crate::transcript::rows::row_details(state, context);
            state.core.request_publish();
        }
    }
    true
}
