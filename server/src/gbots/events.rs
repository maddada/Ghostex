//! Follows the sidecar's live event stream (`/api/events`, server-sent events): a bot's state
//! (its conversations and whether each is working) and every message written or changed.
//! Reconnects while the sidecar runs; each connect starts with a full resync, so nothing missed
//! in a gap is lost.

use std::io::{BufRead as _, BufReader};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::server::AppState;

pub(crate) fn start_event_reader(state: &AppState, port: u16, generation: u64) {
    let state = Arc::new(state.clone());
    let _ = std::thread::Builder::new()
        .name("gbots-events".to_string())
        .spawn(move || {
            let _runtime = super::sidecar::enter_runtime();
            while super::sidecar::GENERATION.load(Ordering::SeqCst) == generation {
                super::projects::full_sync(&state);
                if let Err(error) = read_stream(&state, port, generation) {
                    let _ = error;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
}

fn read_stream(state: &AppState, port: u16, generation: u64) -> Result<(), String> {
    let response = crate::http_agent::agent()
        .get(&format!("http://127.0.0.1:{port}/api/events?screens=off"))
        .header("accept", "text/event-stream")
        .config()
        .timeout_global(None)
        .timeout_recv_body(Some(Duration::from_secs(90)))
        .build()
        .call()
        .map_err(|error| error.to_string())?;
    let reader = BufReader::new(response.into_body().into_reader());
    let mut data = String::new();
    for line in reader.lines() {
        if super::sidecar::GENERATION.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        let line = line.map_err(|error| error.to_string())?;
        if let Some(rest) = line.strip_prefix("data:") {
            data.push_str(rest.trim_start());
            continue;
        }
        if line.is_empty() && !data.is_empty() {
            if let Ok(event) = serde_json::from_str::<Value>(&data) {
                handle_event(state, &event);
            }
            data.clear();
        }
    }
    Ok(())
}

fn handle_event(state: &AppState, event: &Value) {
    match event.get("kind").and_then(Value::as_str) {
        Some("bot") => {
            if let Some(bot) = event.get("bot") {
                super::projects::note_bot(state, bot);
            }
        }
        Some("message") | Some("message.patch") => {
            if let Some(thread_id) = event.get("threadId").and_then(Value::as_str) {
                super::mirror::sync_thread(thread_id);
            }
        }
        _ => {}
    }
}
