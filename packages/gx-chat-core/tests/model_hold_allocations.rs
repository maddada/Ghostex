//! The held-messages card costs nothing while nothing changes: every event of a retained chat
//! assembles the document again, and with a failed pick holding messages that assembly must share
//! the card it already built instead of building it anew. Counted with a counting allocator, which
//! is why this file is its own test binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ghostex_gx_chat_core::menus::picker::hold_card::model_hold_card;
use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatFrame, ChatSnapshotFrame, ComposerBootRead, Event, StartConfig,
};
use serde_json::json;

struct Counting;

static COUNTING: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Allocations and reallocations while `run` runs.
fn allocations(run: impl FnOnce()) -> usize {
    CALLS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    run();
    COUNTING.store(false, Ordering::Relaxed);
    CALLS.load(Ordering::Relaxed)
}

const NOW_MS: f64 = 1_793_363_400_000.0;

fn held_omp_chat() -> ChatCore {
    let mut core = ChatCore::new();
    let start: StartConfig = serde_json::from_value(json!({
        "clientId": "client-1", "projectId": "billing-service", "sessionId": "s-rate-limit",
    }))
    .expect("the start config parses");
    core.handle(Event::Start(Box::new(start)), ChatContext::at(NOW_MS));
    // Outranks the bundled catalog whatever its date (the later `updatedAt` wins at a boot read).
    let boot: ComposerBootRead = serde_json::from_value(json!({
        "sessionKey": "billing-service:s-rate-limit",
        "clientId": "client-1",
        "modelCatalog": {
            "schemaVersion": 1, "updatedAt": "9999-12-31", "effortLabels": {},
            "agents": { "omp": { "name": "OMP", "efforts": ["low", "high"], "models": [
                { "value": "openrouter/qwen3.8-max", "label": "qwen3.8-max", "efforts": ["low", "high"] },
            ] } },
        },
    }))
    .expect("the boot read parses");
    core.handle(
        Event::ComposerBootRead(Box::new(boot)),
        ChatContext::at(NOW_MS),
    );
    let frame: ChatSnapshotFrame = serde_json::from_value(json!({
        "projectId": "billing-service", "sessionId": "s-rate-limit", "serverId": "server-1",
        "protocolVersion": 1, "epoch": 1, "seq": 1, "status": "ready", "agent": "omp",
        "messages": [], "hasMore": false, "beforeOffset": 0,
        "queue": [
            { "id": "q-1", "text": "Cover the SSO callback.", "startupSend": true, "state": "queued", "createdAt": "", "updatedAt": "" },
            { "id": "q-2", "text": "Keep the window at 15 minutes.", "startupSend": false, "state": "queued", "createdAt": "", "updatedAt": "" },
        ],
        "pendingModelSelection": {
            "id": "sel-1", "model": "openrouter/qwen3.8-max", "effort": "high", "options": {},
            "scope": "default", "state": "failed", "errorMessage": "No API key for openrouter.",
        },
    }))
    .expect("the frame parses");
    core.handle(
        Event::Frame(Box::new(ChatFrame::Snapshot(Box::new(frame)))),
        ChatContext::at(NOW_MS),
    );
    core
}

#[test]
fn an_unchanged_hold_builds_its_card_once() {
    let core = held_omp_chat();
    let state = core.state();
    let menu = state.pickers.model_menu_context.as_ref();
    assert!(menu.is_some(), "the OMP chat has its model menu");
    assert_eq!(
        core.document()
            .model_hold_card
            .as_ref()
            .map(|card| card["heldCount"].clone()),
        Some(json!(2))
    );
    let count = allocations(|| {
        for _ in 0..100 {
            std::hint::black_box(model_hold_card(state, menu));
        }
    });
    println!("100 projections of an unchanged hold: {count} allocations");
    assert_eq!(count, 0);
}
