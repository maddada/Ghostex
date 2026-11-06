//! Whether a finished turn's handover waits on the user (`attention.handoverNeedsYou`).

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// Attention events remembered; one entry per finished turn, so a few hundred cover every session
/// in attention at once.
const MAX_CACHED_EVENTS: usize = 1_024;

#[derive(Default)]
struct HandoverAsks {
    answers: HashMap<String, bool>,
    order: VecDeque<String>,
}

fn cache() -> &'static Mutex<HandoverAsks> {
    static CACHE: OnceLock<Mutex<HandoverAsks>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// Whether the finished turn behind attention event `event_id` of `session` ended with a 🛑
/// Blocked or ❓ Needs you section (gx-chat-core `waits_on_reader`).
///
/// CDXC:Notifications 2026-11-06 WHY:
/// The answer comes from the agent's last message, which lives in its transcript. Every snapshot
/// and delta projects the attention again, so the transcript tail is read once per attention
/// event, the way the Notifications row's body already is, and the answer kept by event id. A miss
/// (no transcript or no assistant text yet) is not kept, so a later projection reads again.
pub(crate) fn handover_needs_you(session: &Value, event_id: &str) -> bool {
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    remembered(&format!("{session_id}\u{1f}{event_id}"), || {
        crate::notification_feed::body::last_assistant_markdown(session)
    })
}

/// The answer kept for `key`, else `read`'s answer, kept when it found a message.
fn remembered(key: &str, read: impl FnOnce() -> Option<String>) -> bool {
    if let Some(answer) = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.answers.get(key).copied())
    {
        return answer;
    }
    let Some(message) = read() else {
        return false;
    };
    let answer = ghostex_gx_chat_core::transcript::marked_sections::waits_on_reader(&message);
    if let Ok(mut cache) = cache().lock() {
        if cache.answers.insert(key.to_string(), answer).is_none() {
            cache.order.push_back(key.to_string());
        }
        while cache.order.len() > MAX_CACHED_EVENTS {
            if let Some(stale) = cache.order.pop_front() {
                cache.answers.remove(&stale);
            }
        }
    }
    answer
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use serde_json::json;

    use super::remembered;

    #[test]
    fn a_finished_attention_publishes_a_handover_that_needs_you() {
        let session = |session_id: &str, source: &str| {
            json!({
                "sessionId": session_id,
                "runtimeSettings": { "agentActivity": {
                    "activity": "attention",
                    "attentionSource": source,
                    "attentionEventId": "attn_handover",
                    "lastChangedAt": "2026-11-06T01:00:00.000Z",
                } },
            })
        };
        let at = "2026-11-06T01:00:05.000Z";
        let asks = session("handover-asks", "turnComplete");
        remembered("handover-asks\u{1f}attn_handover", || {
            Some(
                "✅ **Done:**\n- Merged.\n\n🛑 **Blocked:**\n- The deploy needs your OK."
                    .to_string(),
            )
        });
        let attention = crate::presentation::attention_state(&asks, at, false);
        assert_eq!(attention["kind"], json!("finished"));
        assert_eq!(attention["handoverNeedsYou"], json!(true));
        // Nothing marked (and no transcript here): the field is left out.
        let quiet = session("handover-quiet", "turnComplete");
        remembered("handover-quiet\u{1f}attn_handover", || {
            Some("✅ **Done:**\n- Merged.".to_string())
        });
        let attention = crate::presentation::attention_state(&quiet, at, false);
        assert_eq!(attention["kind"], json!("finished"));
        assert!(attention.get("handoverNeedsYou").is_none());
        // An attention that already waits on the user never reads the handover.
        let waiting = crate::presentation::attention_state(&asks, at, true);
        assert_eq!(waiting["kind"], json!("needsInput"));
        assert!(waiting.get("handoverNeedsYou").is_none());
    }

    #[test]
    fn a_handover_is_read_once_per_event_and_a_miss_reads_again() {
        let reads = Cell::new(0);
        let read = |message: Option<&str>| {
            reads.set(reads.get() + 1);
            message.map(str::to_string)
        };
        let key = "handover-test-session\u{1f}event-1";
        // No message yet: not kept.
        assert!(!remembered(key, || read(None)));
        assert!(remembered(key, || read(Some(
            "✅ **Done:**\n- Built it.\n\n❓ **Needs you:**\n- Approve the merge."
        ))));
        assert!(remembered(key, || read(Some(
            "A later read is never made."
        ))));
        assert_eq!(reads.get(), 2);
        let other = "handover-test-session\u{1f}event-2";
        assert!(!remembered(other, || read(Some(
            "✅ **Done:**\n- Reported to the coordinator."
        ))));
        assert!(!remembered(other, || read(Some(
            "🛑 **Blocked:** never read"
        ))));
        assert_eq!(reads.get(), 3);
    }
}
