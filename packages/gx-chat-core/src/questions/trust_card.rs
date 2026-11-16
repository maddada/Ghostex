//! The folder-trust card: an agent's own "do you trust this folder?" question as two buttons.
//!
//! CDXC:AgentProviders 2026-11-14 DECISION:
//! User: "can we make it so we show the interface to trust a folder when the agent shows it to us? should be easy to trust folders too." Ghostex no longer trusts folders for agents by default, so the agent's question is a card in the chat with the folder (gxserver writes it into the card's words, session_chat_trust_memory.rs `word_trust_notice`) and two buttons, Trust folder and Don't trust, that answer the agent's own prompt the way its rows do. One click trusts; Cmd+Enter (Ctrl+Enter) is Trust folder and Escape is Don't trust. Terminal view stays, and the terminal keeps the agent's own prompt. Trust and Remember is no longer on the card (the brief asked for exactly these two buttons); folders remembered before keep being answered by gxserver.
//! SEE-ALSO: apps/desktop/src/app/native_chat/notice.rs and apps/mobile/app/src/chat/native/cards/NoticeCard.tsx (both draw a `primary` action filled and the rest beside it).

use serde_json::{json, Map, Value};

/// A row that grants trust: "Yes, I trust this folder", Codex's "Yes, continue", Pi's "Trust".
fn grants(label: &str) -> bool {
    let label = label.trim();
    label.starts_with("Yes") || label == "Trust" || label.starts_with("Trust this")
}

/// A row that refuses: "No, exit", "No, quit", Pi's "Do not trust" (not its session-only one).
fn refuses(label: &str) -> bool {
    let label = label.trim();
    label.starts_with("No") || label == "Do not trust"
}

fn label(row: &Value) -> &str {
    row.get("label").and_then(Value::as_str).unwrap_or_default()
}

fn button(id: &str, words: &str, answer: Value, primary: bool) -> Value {
    let mut button = json!({ "id": id, "label": words, "kind": id, "answer": answer });
    if primary {
        button["primary"] = json!(true);
    }
    button
}

/// Rewrites a decorated `trustPrompt` notice into the two-button card. A notice it cannot answer
/// both ways from (no Yes and No row, no trust key) is left as it is.
pub fn decorate(object: &mut Map<String, Value>) {
    if object.get("kind").and_then(Value::as_str) != Some("trustPrompt") {
        return;
    }
    let actions: Vec<Value> = object
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let terminal: Vec<Value> = actions
        .iter()
        .filter(|action| action.get("kind").and_then(Value::as_str) == Some("switchToTerminal"))
        .cloned()
        .collect();
    let choices: Vec<Value> = object
        .get("choices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let row_answer = |test: fn(&str) -> bool| {
        choices
            .iter()
            .find(|row| test(label(row)))
            .and_then(|row| row.get("answer"))
            .filter(|answer| answer.is_object())
            .cloned()
    };
    let key_answer = |id: &str| {
        actions
            .iter()
            .find(|action| action.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|action| action.get("answer"))
            .filter(|answer| answer.is_object())
            .cloned()
    };
    let (trust, dont) = match (row_answer(grants), row_answer(refuses)) {
        (Some(trust), Some(dont)) => (trust, Some(dont)),
        _ => match key_answer("trustDirectory") {
            Some(trust) => (trust, key_answer("dontTrustDirectory")),
            None => return,
        },
    };
    let mut buttons = vec![button("trustFolder", "Trust folder", trust, true)];
    if let Some(dont) = dont {
        buttons.push(button("dontTrustFolder", "Don't trust", dont, false));
    }
    buttons.extend(terminal);
    object.insert("actions".to_string(), Value::Array(buttons));
    // The rows are the buttons now. An empty list (not none) keeps the card answered, not closed:
    // the renderers offer Dismiss only on a card without rows.
    object.insert("choices".to_string(), json!([]));
    object.insert("collapsedChoices".to_string(), json!([]));
    object.remove("dialog");
    object.remove("numbered");
    object.remove("more");
}
