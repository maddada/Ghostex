//! Preview fixtures for Settings > Grok Bot (tabs/grok_bot.rs): a scripted `/api/grokbot`.
//!
//! States: `grokbot` (mid-checklist: the app running and the bot's first brief seen, so step 3,
//! the webhook form, is next), `grokbot-start` (nothing done yet: no app, no brief),
//! `grokbot-connected` (the Connected view, 14 notes sent).
use serde_json::{Value, json};

pub(super) fn owns(state: &str) -> bool {
    state.starts_with("grokbot")
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    owns(state).then(|| json!({ "initialTab": "grokBot" }))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// The scripted daemon for these states; `None` for paths it does not know.
pub(super) fn rpc(state: &str, path: &str, params: &Value) -> Option<Result<Value, String>> {
    if path != "/api/grokbot" {
        return None;
    }
    let now = now_ms();
    Some(Ok(match params["op"].as_str().unwrap_or("status") {
        "setup" if state == "grokbot-connected" => json!({
            "enabled": true, "connected": true,
            "url": "https://api2.cursor.sh/automations/webhook/aut_7Qm2xLr9Vt4Kp8Hn3k3f9",
            "connectedAt": now - 86_400_000, "lastSentAt": now - 180_000, "sent": 14, "queued": 0,
            "app": "running", "briefSeenAt": now - 86_400_000, "testArrived": true
        }),
        "setup" if state == "grokbot-start" => json!({
            "enabled": true, "connected": false, "queued": 0, "app": "missing"
        }),
        "setup" => json!({
            "enabled": true, "connected": false, "queued": 0,
            "app": "running", "briefSeenAt": now - 120_000
        }),
        "instructions" => json!({ "text": "You are Ghostex, my remote control for Ghostex…" }),
        "test" => json!({ "sent": true }),
        "connect" => {
            json!({ "connected": true, "tested": true, "url": params["url"], "sent": 1, "lastSentAt": now })
        }
        _ => json!({}),
    }))
}
