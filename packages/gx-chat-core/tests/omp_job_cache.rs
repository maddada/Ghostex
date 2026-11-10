//! An unchanged OMP background job delivery keeps its cached projection: looking it up again while
//! the session works allocates nothing and hands back the same model, for a completed job, a
//! cancelled one, and batches of jobs that ended differently. None of their calls is waiting for an
//! answer, so the agent working on something else changes nothing they draw.
//!
//! Its own test binary, because the allocation count needs a global allocator; it counts only on
//! the thread doing the lookup, so the test harness's other threads do not show up in it.

use ghostex_gx_chat_core::protocol::ChatMessage;
use ghostex_gx_chat_core::transcript::presentation::{project_cached, scope, ProjectionCache};
use ghostex_gx_chat_core::{ChatContext, ChatState};
use serde_json::{json, Value};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

/// The rows as the core holds them (`ChatRow`).
fn shared(rows: &[ChatMessage]) -> Vec<ghostex_gx_chat_core::state::ChatRow> {
    rows.iter().cloned().map(std::sync::Arc::new).collect()
}

fn count(bytes: usize) {
    // `try_with`: a thread that is shutting down has no counters left and is not measured.
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        let _ = BYTES.try_with(|total| total.set(total.get() + bytes));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What `run` allocated on this thread: calls and requested bytes.
fn allocations<T>(run: impl FnOnce() -> T) -> (T, usize, usize) {
    ALLOCATIONS.with(|count| count.set(0));
    BYTES.with(|total| total.set(0));
    COUNTING.with(|on| on.set(true));
    let value = run();
    COUNTING.with(|on| on.set(false));
    (value, ALLOCATIONS.with(Cell::get), BYTES.with(Cell::get))
}

/// One delivery of jobs that ended as `statuses`, written the way oh-my-pi 18.13.3 writes it
/// (`prompts/tools/async-result.md`: one job alone, several under `── Job <id>` lines), with the
/// details gxserver passes on. Job `n` ran `n + 1` times 21 seconds.
fn delivery(statuses: &[&str]) -> ChatMessage {
    let ids: Vec<String> = (0..statuses.len()).map(|n| format!("job{n}")).collect();
    let text = if statuses.len() == 1 {
        "<system-notice>\nBackground job job0 has completed. Resume your work using the result below.\n\nrunning 112 tests\n</system-notice>".to_string()
    } else {
        let parts: Vec<String> = ids
            .iter()
            .map(|id| format!("── Job {id} (task {id}) ──\noutput of {id}"))
            .collect();
        format!(
            "<system-notice>\n{} background jobs have completed. Resume your work using the results below.\n\n{}\n</system-notice>",
            statuses.len(),
            parts.join("\n")
        )
    };
    let jobs: Vec<Value> = ids
        .iter()
        .zip(statuses)
        .enumerate()
        .map(|(n, (id, status))| {
            json!({ "jobId": id, "label": format!("task {id}"), "status": status, "durationMs": (n as i64 + 1) * 21_000 })
        })
        .collect();
    serde_json::from_value(json!({
        "id": format!("delivery-{}", statuses.join("-")),
        "role": "system",
        "source": "transcript",
        "timestamp": 1_793_363_400_000_i64,
        "blocks": [{ "type": "text", "text": text }],
        "customMessage": { "customType": "async-result", "details": { "jobs": jobs } },
    }))
    .expect("a chat row")
}

/// How the tool card draws a job that ended so.
fn drawn(status: &str) -> &'static str {
    match status {
        "cancelled" => "stopped",
        "failed" => "failed",
        _ => "done",
    }
}

#[test]
fn an_unchanged_job_delivery_is_looked_up_again_without_allocating() {
    // Every delivery is measured before anything is asserted, so a failure reports them all.
    let deliveries: [&[&str]; 5] = [
        &["completed"],
        &["cancelled"],
        &["completed", "cancelled"],
        &["failed", "cancelled"],
        &["completed", "failed"],
    ];
    let measured: Vec<_> = deliveries
        .into_iter()
        .map(|statuses| {
            let mut state = ChatState::default();
            state.messages.composed = shared(&[delivery(statuses)]);
            let mut scope = scope(&state, &state.transcript_view);
            scope.working = true;
            scope.tool_cards = true;
            let context = ChatContext::default();
            let mut cache = ProjectionCache::new();
            let message = &scope.messages[0];

            let first = project_cached(&mut cache, &scope, &context, message);
            let (second, calls, bytes) =
                allocations(|| project_cached(&mut cache, &scope, &context, message));
            let reused = Arc::ptr_eq(&first, &second);
            println!(
                "{statuses:?}: second lookup allocations={calls} bytes={bytes} model_reused={reused}"
            );
            let entries = second["toolCard"]["entries"]
                .as_array()
                .expect("a tool card");
            let shown: Vec<(&str, &str)> = entries
                .iter()
                .map(|entry| {
                    (
                        entry["status"].as_str().unwrap_or_default(),
                        entry["time"].as_str().unwrap_or_default(),
                    )
                })
                .collect();
            let expected: Vec<(&str, &str)> = statuses
                .iter()
                .zip(["21s", "42s"])
                .map(|(status, time)| (drawn(status), time))
                .collect();
            assert_eq!(shown, expected, "{statuses:?}: the card's entries");
            (statuses, calls, bytes, reused)
        })
        .collect();
    for (statuses, calls, bytes, reused) in measured {
        assert_eq!(
            (calls, bytes),
            (0, 0),
            "{statuses:?}: the second identical lookup allocated {calls} times ({bytes} bytes)"
        );
        assert!(
            reused,
            "{statuses:?}: the second identical lookup projected a new model"
        );
    }
}
