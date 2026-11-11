//! What one event costs on a long coordinator chat with a full Work panel (CDXC:SessionChat
//! 2026-11-11 in `extras/work_cards.rs` on `Turns`, and in `extras/coordinator_board.rs` on
//! `ages_change_at`).
//!
//! The chat engine runs every chat on one thread, and a message sent from the chat box waits for
//! that thread. On 11 Nov 2026 it was busy for 7 to 18 seconds at a time on the user's coordinator
//! chat (231 work items): the Work panel was built again on nearly every event, and each of its
//! rows walked the whole chat.
//!
//! The timing run is ignored by default (run it in release):
//! `cargo test --release -p ghostex-gx-chat-core --test coordinator_chat_event_cost -- --ignored --nocapture`.

use ghostex_gx_chat_core::{assemble_with, AssembleCache, ChatContext, ChatState};
use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// 11 Nov 2026 01:00:00 UTC.
const T0: i64 = 1_794_358_800_000;


/// The rows as the core holds them (`ChatRow`).
fn shared(rows: &[ChatMessage]) -> Vec<ghostex_gx_chat_core::state::ChatRow> {
    rows.iter().cloned().map(std::sync::Arc::new).collect()
}

fn iso(at_ms: i64) -> String {
    let seconds = at_ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    // Civil date from days since 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60,
        at_ms.rem_euclid(1000)
    )
}

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

/// A chat of `turns` turns: the user's message, two replies with words and a tool call between.
fn transcript(turns: usize) -> Vec<ChatMessage> {
    let words = "The thread reported back and its change is on main. ".repeat(12);
    let mut rows = Vec::new();
    for turn in 0..turns {
        let at = T0 - (turns - turn) as i64 * 90_000;
        rows.push(message(json!({
            "id": format!("u{turn}"), "role": "user", "timestamp": at, "source": "transcript",
            "blocks": [{ "type": "text", "text": format!("Message {turn}: {words}") }],
        })));
        rows.push(message(json!({
            "id": format!("a{turn}-1"), "role": "assistant", "timestamp": at + 5_000,
            "source": "transcript",
            "blocks": [{ "type": "text", "text": format!("On it. {words}") }],
        })));
        rows.push(message(json!({
            "id": format!("c{turn}"), "role": "assistant", "timestamp": at + 10_000,
            "source": "transcript",
            "blocks": [{ "type": "tool-call", "name": "Bash", "callId": format!("c{turn}"),
                         "input": { "command": "ghostex coordinator status" } }],
        })));
        rows.push(message(json!({
            "id": format!("c{turn}-result"), "role": "tool", "timestamp": at + 12_000,
            "source": "transcript",
            "blocks": [{ "type": "tool-result", "callId": format!("c{turn}"), "output": "ok" }],
        })));
        rows.push(message(json!({
            "id": format!("a{turn}-2"), "role": "assistant", "timestamp": at + 20_000,
            "source": "transcript",
            "blocks": [{ "type": "text", "text": format!("Done. {words}") }],
        })));
    }
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    rows
}

/// `count` work items as gxserver sends them: a third in progress, a third asking, a third later,
/// each changed at its own second of the last hours.
fn work(count: usize) -> Value {
    let items: Vec<Value> = (0..count)
        .map(|index| {
            let number = index + 1;
            let at = iso(T0 - (index as i64 * 67_000 + 13_000));
            let (status, asks) = match index % 3 {
                0 => ("moving", json!([])),
                1 => (
                    "needs",
                    json!([{ "tag": format!("Q{number}"), "open": true, "askedAt": at,
                             "text": "Which one?", "title": "Which one?",
                             "options": ["A", "B"] }]),
                ),
                _ => ("later", json!([])),
            };
            json!({
                "key": format!("work:{number}"), "ref": format!("W{number}"), "kind": "item",
                "title": format!("Work item {number}"), "status": status, "at": at,
                "now": "A thread is on it", "today": true, "asks": asks, "threads": [],
                "waits": [], "steps": [],
                "owner": { "kind": "coordinator", "label": "Coordinator" },
                "history": [
                    { "kind": "add", "at": at, "text": "added" },
                    { "kind": "now", "at": at, "text": "A thread is on it" },
                ],
            })
        })
        .collect();
    json!({ "threads": [], "total": 0, "work": { "items": items } })
}

fn coordinator_chat(turns: usize, items: usize) -> ChatState {
    let rows = transcript(turns);
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.core.controller_started = true;
    state.messages.composed = shared(&rows);
    state.messages.list = shared(&rows);
    *state.session.coordinator_threads = Some(work(items).into());
    // Quiet View, as a coordinator's chat has it by default.
    ghostex_gx_chat_core::transcript::quiet_view::apply_settings(
        &mut state,
        ghostex_gx_chat_core::transcript::quiet_view::QuietViewSettings {
            coordinators: true,
            chats: false,
            expand_last: false,
        },
    );
    state
}

/// The Quiet View lines a core keeps between documents are the ones a fresh build makes, at
/// another time, on another day and after a row arrived (CDXC:SessionChat 2026-11-11
/// in `transcript/quiet_view.rs` on `document_kept`).
#[test]
fn kept_quiet_view_lines_are_the_lines_a_fresh_build_makes() {
    let mut state = coordinator_chat(60, 12);
    ghostex_gx_chat_core::transcript::rows::refresh(&mut state, &at(T0));
    let mut cache = AssembleCache::default();
    let lines = |state: &ChatState, now: i64, cache: &mut AssembleCache| {
        let kept = assemble_with(state, &at(now), cache).quiet_view;
        let fresh = assemble_with(state, &at(now), &mut AssembleCache::default()).quiet_view;
        assert_eq!(kept, fresh, "kept lines equal a fresh build at {now}");
        kept.expect("a coordinator's chat has Quiet View")
    };
    let first = lines(&state, T0, &mut cache);
    assert!(
        first.on && !first.stretches.is_empty(),
        "the chat folds into lines"
    );
    // The clock alone: the same lines.
    assert_eq!(lines(&state, T0 + 5_000, &mut cache), first);
    // The next day a line's times are written with the date: built again, and still a fresh
    // build's lines (these have no times to differ by).
    lines(&state, T0 + 86_400_000, &mut cache);
    // A new row: the projection hands out a new list and the lines follow it.
    let mut rows = state.messages.composed.clone();
    rows.extend(shared(&transcript(61).split_off(60 * 5)));
    state.messages.composed = rows.clone();
    state.messages.list = rows;
    state.messages.compose_generation += 1;
    ghostex_gx_chat_core::transcript::rows::refresh(&mut state, &at(T0));
    let grown = lines(&state, T0, &mut cache);
    assert_ne!(grown, first);
}

fn at(now_ms: i64) -> ChatContext {
    ChatContext {
        now_ms: now_ms as f64,
        ..Default::default()
    }
}

fn timed(label: &str, runs: u32, mut run: impl FnMut()) -> Duration {
    let started = Instant::now();
    for _ in 0..runs {
        run();
    }
    let each = started.elapsed() / runs;
    println!("{each:>12.2?}  {label}");
    each
}

#[test]
#[ignore = "a timing run: release, --ignored --nocapture"]
fn an_event_on_a_long_coordinator_chat_is_cheap() {
    for (turns, items) in [(400, 60), (1500, 231)] {
        println!("{turns} turns, {items} work items");
        let mut state = coordinator_chat(turns, items);
        ghostex_gx_chat_core::transcript::rows::refresh(&mut state, &at(T0));
        let mut cache = AssembleCache::default();
        timed("first document", 1, || {
            assemble_with(&state, &at(T0), &mut cache);
        });
        // The clock moves a third of a second per event, as a streaming reply's events do.
        let mut now = T0;
        let moving = timed("document, clock 300 ms on", 10, || {
            now += 300;
            assemble_with(&state, &at(now), &mut cache);
        });
        // The same clock again: nothing the document reads moved.
        let still = timed("document, same clock", 10, || {
            assemble_with(&state, &at(now), &mut cache);
        });
        println!("moving {moving:?}, still {still:?}");
        let turns = ghostex_gx_chat_core::extras::work_cards::turns_for_work(&state);
        timed("  turns", 10, || {
            ghostex_gx_chat_core::extras::work_cards::turns_for_work(&state);
        });
        timed("  dock", 10, || {
            ghostex_gx_chat_core::extras::work_cards::dock_with(&state, &at(now), &turns);
        });
        timed("  card tags", 10, || {
            ghostex_gx_chat_core::extras::work_cards::card_tags(&state);
        });
        timed("  board (whole build)", 10, || {
            ghostex_gx_chat_core::extras::coordinator_board::coordinator_board_with(
                &state,
                &at(now),
                &turns,
            );
        });
        timed("  work cards (whole build)", 10, || {
            ghostex_gx_chat_core::extras::work_cards::project_with(&state, &at(now), &turns);
        });
    }
}
