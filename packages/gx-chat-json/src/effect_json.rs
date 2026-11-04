//! The JSON a phone host receives for each [`Effect`].
//!
//! One object tagged by `type`, camelCase, in the order the core returned them. The TypeScript
//! mirror is `ChatCoreEffect` in `apps/mobile/app/src/chat/rust/core-types.ts`; keep the two
//! in lockstep. Routing (which effects the host performs itself, which it feeds back as an action)
//! is the host's, as it is on desktop (`apps/desktop/src/app/gx_chat/effects.rs`).

use ghostex_gx_chat_core::{Effect, OpenTarget};
use serde_json::{json, Value};

/// The effects of one turn as a JSON array.
pub(crate) fn encode_all(effects: Vec<Effect>) -> String {
    let list: Vec<Value> = effects.into_iter().map(encode).collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
}

fn encode(effect: Effect) -> Value {
    match effect {
        Effect::SendRpc {
            request_id,
            method,
            params,
        } => json!({
            "type": "sendRpc",
            "requestId": request_id,
            "method": method.as_str(),
            "params": *params,
        }),
        Effect::Subscribe { limit, catalog } => {
            json!({ "type": "subscribe", "limit": limit, "catalog": catalog })
        }
        Effect::Unsubscribe => json!({ "type": "unsubscribe" }),
        Effect::Reconnect => json!({ "type": "reconnect" }),
        Effect::ReadStorage { key } => json!({ "type": "readStorage", "key": key }),
        Effect::ReadStorageBatch { keys } => json!({ "type": "readStorageBatch", "keys": keys }),
        Effect::ReadRetainedSnapshot => json!({ "type": "readRetainedSnapshot" }),
        Effect::WriteRetainedSnapshot { value } => {
            json!({ "type": "writeRetainedSnapshot", "value": value })
        }
        Effect::UpdatePresentation { state } => {
            json!({ "type": "updatePresentation", "state": *state })
        }
        Effect::ReadComposerBoot { request_id } => {
            json!({ "type": "readComposerBoot", "requestId": request_id })
        }
        Effect::WriteStorage {
            key,
            value,
            durable,
        } => json!({ "type": "writeStorage", "key": key, "value": value, "durable": durable }),
        Effect::WriteStorageBatch { writes } => {
            json!({ "type": "writeStorageBatch", "writes": writes })
        }
        Effect::FlushStorage { store } => json!({ "type": "flushStorage", "store": store }),
        Effect::RecordDeliveries { deliveries } => {
            json!({ "type": "recordDeliveries", "deliveries": deliveries })
        }
        Effect::SetTimer { delay_ms } => json!({ "type": "setTimer", "delayMs": delay_ms }),
        Effect::SetComposerText {
            content,
            caret,
            from_history,
        } => json!({
            "type": "setComposerText",
            "content": content,
            "caret": caret,
            "fromHistory": from_history,
        }),
        Effect::ClearComposerIfUnchanged { text } => {
            json!({ "type": "clearComposerIfUnchanged", "text": text })
        }
        Effect::Open(OpenTarget::Url { url, external }) => {
            let mut target = json!({ "kind": "url", "url": url });
            if external {
                target["external"] = Value::Bool(true);
            }
            json!({ "type": "open", "target": target })
        }
        Effect::Open(OpenTarget::File { path, line, column }) => {
            let mut target = json!({ "kind": "file", "path": path });
            if let Some(line) = line {
                target["line"] = Value::from(line);
            }
            if let Some(column) = column {
                target["column"] = Value::from(column);
            }
            json!({ "type": "open", "target": target })
        }
        Effect::Copy { text } => json!({ "type": "copy", "text": text }),
        Effect::Toast { level, message } => {
            json!({ "type": "toast", "level": level, "message": message })
        }
        Effect::RestoreReturnedPrompt { text } => {
            json!({ "type": "restoreReturnedPrompt", "text": text })
        }
        Effect::MarkdownSaved { path } => json!({ "type": "markdownSaved", "path": path }),
        Effect::HostAction { action, params } => {
            json!({ "type": "hostAction", "action": action, "params": *params })
        }
        Effect::Speak {
            id,
            message_id,
            segments,
        } => json!({
            "type": "speak",
            "id": id,
            "messageId": message_id,
            "segments": segments,
        }),
        Effect::StopSpeaking { id } => json!({ "type": "stopSpeaking", "id": id }),
        Effect::SetReadAloudAuto { on } => json!({ "type": "setReadAloudAuto", "on": on }),
        // `Effect` is `#[non_exhaustive]`: a core newer than this binding surfaces as a named
        // unknown the host can count, never as a silent drop.
        other => json!({ "type": "unknown", "name": variant_name(&other) }),
    }
}

/// The Rust variant name of an effect, never its payload (which can hold chat text).
fn variant_name(effect: &Effect) -> String {
    let debug = format!("{effect:?}");
    debug
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or_default()
        .to_string()
}
