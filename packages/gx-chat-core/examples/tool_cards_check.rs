//! Prints a transcript's items as the chat core draws them, in Quiet View, to check how a saved
//! session's tool calls group into cards: each item's kind and id, the calls it holds and the label
//! its folded card shows (`simpleToolLabel`), and each Quiet View line's span and words.
//!
//! ```text
//! cargo run --example tool_cards_check -- <read-session-chat --json output> [--from <id prefix>] [--working]
//! ```
//!
//! The file is what `ghostex read-session-chat <session> --history-mode detail --json` prints:
//! its `messages` are fed to the core as one snapshot frame.

use ghostex_gx_chat_core::transcript::quiet_view::{apply_settings, QuietViewSettings};
use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatFrame, ChatSnapshotFrame, Event, StartConfig,
};
use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().expect("a read-session-chat --json file");
    let from = args
        .iter()
        .position(|arg| arg == "--from")
        .and_then(|at| args.get(at + 1))
        .cloned();
    let working = args.iter().any(|arg| arg == "--working");
    let read: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("the file reads"))
        .expect("the file is JSON");
    let now = read["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .and_then(|message| message["timestamp"].as_f64())
        .unwrap_or(0.0)
        + 60_000.0;
    let context = ChatContext::at(now);
    let mut core = ChatCore::new();
    let config: StartConfig = serde_json::from_value(json!({
        "clientId": "client-1",
        "projectId": "project-1",
        "sessionId": "session-1",
    }))
    .expect("the start config parses");
    core.handle(Event::Start(Box::new(config)), context.clone());
    core.handle(
        Event::RetainedSnapshotLoaded { value: None },
        context.clone(),
    );
    core.handle(Event::ComposerBootRead(Box::default()), context.clone());
    core.state_mut().session.agent = Some("claude".to_string());
    apply_settings(core.state_mut(), QuietViewSettings::default());
    let frame: ChatSnapshotFrame = serde_json::from_value(json!({
        "type": "sessionChatSnapshot",
        "projectId": "project-1",
        "sessionId": "session-1",
        "serverId": "server-1",
        "protocolVersion": 1,
        "epoch": 1,
        "seq": 1,
        "status": "ready",
        "messages": read["messages"],
        "hasMore": false,
        "beforeOffset": 0,
        // `--working`: the agent still runs its newest turn, as when the screenshot was taken.
        "working": working,
        "lifecycle": if working {
            json!({"state": "working", "turnId": "check", "timestamp": now as i64 - 60_000})
        } else {
            Value::Null
        },
    }))
    .expect("the frame parses");
    core.handle(
        Event::Frame(Box::new(ChatFrame::Snapshot(Box::new(frame)))),
        context.clone(),
    );
    let frame = serde_json::to_value(core.frame(0)).expect("the frame is JSON");
    let items = frame["itemsSplice"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let start = from
        .as_deref()
        .and_then(|prefix| {
            items
                .iter()
                .position(|item| item_id(item).starts_with(prefix))
        })
        .unwrap_or(0);
    for (index, item) in items.iter().enumerate().skip(start) {
        println!("{index:>3}  {}", describe(item));
    }
    if let Some(lines) = frame["snapshot"]["quietView"]["stretches"].as_array() {
        println!("\nQuiet View lines:");
        for line in lines {
            if line["end"].as_u64().unwrap_or(0) as usize <= start {
                continue;
            }
            println!(
                "  rows {}..{}  \"{}\"  open: \"{}\"",
                line["start"],
                line["end"],
                text(line, "text"),
                text(line, "openText")
            );
            // A finished turn's card inside the opened line, titled by its calls.
            for turn in line["turns"].as_array().into_iter().flatten() {
                if !text(turn, "title").is_empty() {
                    println!("      turn card \"{}\"", text(turn, "title"));
                }
            }
        }
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn item_id(item: &Value) -> &str {
    item["message"]["id"]
        .as_str()
        .or_else(|| item["id"].as_str())
        .unwrap_or_default()
}

fn describe(item: &Value) -> String {
    let kind = text(item, "kind");
    let id: String = item_id(item).chars().take(8).collect();
    let message = &item["message"];
    let tools = message["tools"].as_array().map(Vec::len).unwrap_or(0);
    let words: String = message["body"]
        .as_str()
        .or_else(|| message["markdown"].as_str())
        .unwrap_or_default()
        .chars()
        .take(40)
        .collect::<String>()
        .replace('\n', " ");
    let mut parts = vec![format!("{kind:<16}"), id, text(message, "role").to_string()];
    if tools > 0 {
        parts.push(format!(
            "{tools} call(s), card \"{}\"",
            text(message, "simpleToolLabel")
        ));
    }
    if !words.is_empty() {
        parts.push(format!("\"{words}\""));
    }
    if kind == "completed-work" {
        parts.push(format!("\"{}\"", text(item, "label")));
    }
    parts.join("  ")
}
