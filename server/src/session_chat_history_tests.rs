use super::*;

fn row(id: &str, role: &str, offset: u64, custom_type: Option<&str>) -> SessionChatMessage {
    let mut value = json!({
        "id": id, "role": role, "source": "transcript", "timestamp": offset as i64,
        "byteOffset": offset, "blocks": [{ "type": "text", "text": format!("{id} text") }],
    });
    if let Some(custom_type) = custom_type {
        value["customMessage"] = json!({ "customType": custom_type });
    }
    serde_json::from_value(value).expect("row")
}

fn ids(rows: &[Value]) -> Vec<&str> {
    rows.iter().map(|row| row["id"].as_str().unwrap()).collect()
}

#[test]
fn a_collapsed_turn_keeps_the_reply_before_an_omp_wake_up() {
    let messages = [
        row("u1", "user", 10, None),
        row("a1", "assistant", 20, None),
        row("w1", "system", 30, Some("irc:incoming")),
        row("a2", "assistant", 40, None),
        row("u2", "user", 50, None),
        row("a3", "assistant", 60, None),
    ];
    let collapsed = collapse_snapshot_messages(&messages, 0);
    assert_eq!(ids(&collapsed), ["u1", "a1", "w1", "a2", "u2", "a3"]);
    // A passive OMP notice, or one the agent did not answer, keeps only the last reply.
    let mut passive = messages.clone();
    passive[2] = row("w1", "system", 30, Some("background-tan-dispatch"));
    assert_eq!(
        ids(&collapse_snapshot_messages(&passive, 0)),
        ["u1", "w1", "a2", "u2", "a3"]
    );
}

/// A Pi transcript file holding `records`, read back by the real tail and history readers.
fn transcript_file(records: &[Value]) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "gxserver-omp-wake-history-{}-{}.jsonl",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let mut lines = vec![json!({"type": "session", "version": 3, "id": "s1",
        "timestamp": "2026-10-30T10:00:00.000Z", "cwd": "/tmp"})
    .to_string()];
    lines.extend(records.iter().map(Value::to_string));
    std::fs::write(&path, lines.join("\n") + "\n").expect("write transcript");
    path
}

/// The records of `rows` (id, what it is), each the child of the one before, a second apart:
/// `user`, `reply`, `reasoning`, `call` (a `read` call), `result` (the result of `c<n>`'s call
/// for `t<n>`), `failed-result`, `write` and `write-result`, `two-calls` (`c2` and `c3` in one
/// row), `mixed` (reply text and a call in one message), `ask` (a question), `send` (a message
/// to another agent), `bash` (a shell command's output, which names no call), `irc` (an OMP IRC
/// message), `async` (an OMP background result the chat draws as a collapsed marker),
/// `passive` (OMP's `/tan` dispatch notice), `status` ("Context compacted"), and the rows the
/// chat never shows: `hidden-user` (a harness reminder), `hidden-system` (a compaction whose
/// summary is one) and `hidden-async` (an OMP background result that is one).
fn records(rows: &[(&str, &str)]) -> Vec<Value> {
    let mut parent: Option<&str> = None;
    rows.iter()
        .enumerate()
        .map(|(at, (id, kind))| {
            let timestamp = format!("2026-10-30T10:{:02}:{:02}.000Z", at / 60, at % 60);
            let message = match *kind {
                "user" => json!({"role": "user", "content": [{"type": "text", "text": format!("{id} prompt")}]}),
                "reply" => json!({"role": "assistant", "content": [{"type": "text", "text": format!("{id} reply")}]}),
                "call" => json!({"role": "assistant", "content": [{"type": "toolCall", "id": format!("{id}-call"), "name": "read", "arguments": {"path": format!("{id}.txt")}}]}),
                "result" => json!({"role": "toolResult", "toolCallId": format!("c{}-call", &id[1..]), "toolName": "read", "content": [{"type": "text", "text": format!("{id} contents")}], "isError": false}),
                "write" => json!({"role": "assistant", "content": [{"type": "toolCall", "id": format!("{id}-call"), "name": "write", "arguments": {"path": format!("{id}.txt"), "content": "updated text"}}]}),
                "write-result" => json!({"role": "toolResult", "toolCallId": format!("c{}-call", &id[1..]), "toolName": "write", "content": [{"type": "text", "text": "Successfully wrote the file"}], "isError": false}),
                "failed-result" => json!({"role": "toolResult", "toolCallId": format!("c{}-call", &id[1..]), "toolName": "read", "content": [{"type": "text", "text": "error: failed to read file"}], "isError": true}),
                "two-calls" => json!({"role": "assistant", "content": [
                    {"type": "toolCall", "id": "c2-call", "name": "read", "arguments": {"path": "c2.txt"}},
                    {"type": "toolCall", "id": "c3-call", "name": "read", "arguments": {"path": "c3.txt"}}]}),
                "hidden-user" => json!({"role": "user", "content": [{"type": "text", "text": "<system-reminder>Context only</system-reminder>"}]}),
                "reasoning" => json!({"role": "assistant", "content": [{"type": "thinking", "thinking": format!("{id} thinking")}]}),
                "mixed" => json!({"role": "assistant", "content": [
                    {"type": "text", "text": format!("{id} note")},
                    {"type": "toolCall", "id": format!("{id}-call"), "name": "read", "arguments": {"path": format!("{id}.txt")}}]}),
                "ask" => json!({"role": "assistant", "content": [{"type": "toolCall", "id": format!("{id}-call"), "name": "ask",
                    "arguments": {"questions": [{"id": "q", "question": "Continue?", "options": ["Yes", "No"]}]}}]}),
                "send" => json!({"role": "assistant", "content": [{"type": "toolCall", "id": format!("{id}-call"), "name": "SendMessage",
                    "arguments": {"to": "Peer", "message": "Peer update"}}]}),
                "bash" => json!({"role": "bashExecution", "command": "ls", "output": "files"}),
                _ => Value::Null,
            };
            let record = match *kind {
                "irc" | "passive" => json!({"type": "custom_message", "id": id, "parentId": parent,
                    "timestamp": timestamp,
                    "customType": if *kind == "irc" { "irc:incoming" } else { "background-tan-dispatch" },
                    "content": "<irc>\nNew work\n</irc>", "display": true,
                    "details": {"from": "Peer", "message": "New work"}, "attribution": "agent"}),
                "async" | "hidden-async" => json!({"type": "custom_message", "id": id, "parentId": parent,
                    "timestamp": timestamp, "customType": "async-result",
                    "content": if *kind == "async" {
                        "<task-notification><status>completed</status><summary>Background job done</summary></task-notification>"
                    } else {
                        "<system-reminder>Context only</system-reminder>"
                    },
                    "display": true, "details": {}, "attribution": "agent"}),
                "hidden-system" | "status" => json!({"type": "compaction", "id": id, "parentId": parent,
                    "timestamp": timestamp,
                    "summary": if *kind == "status" { "Context compacted" } else { "<system-reminder>Context only</system-reminder>" },
                    "firstKeptEntryId": "u1", "tokensBefore": 100, "fromExtension": true}),
                _ => json!({"type": "message", "id": id, "parentId": parent, "timestamp": timestamp, "message": message}),
            };
            parent = Some(id);
            record
        })
        .collect()
}

fn chat_state(rows: &[Value]) -> ghostex_gx_chat_core::ChatState {
    let messages: Vec<ChatMessage> = rows
        .iter()
        .map(|row| serde_json::from_value(row.clone()).expect("a chat row"))
        .collect();
    let mut state = ghostex_gx_chat_core::ChatState::default();
    state.session.agent = Some("omp".into());
    state.transcript_view.tool_cards = true;
    state.messages.list = messages.clone();
    state.messages.composed = messages;
    state.messages.compose_generation += 1;
    state
}

/// The chat's items as the rows refresh builds them for the renderers.
fn items(state: &mut ghostex_gx_chat_core::ChatState) -> Vec<Value> {
    ghostex_gx_chat_core::transcript::rows::refresh(
        state,
        &ghostex_gx_chat_core::ChatContext::default(),
    );
    state
        .transcript_view
        .items
        .iter()
        .map(|item| serde_json::to_value(item).unwrap())
        .collect()
}

/// Each item as `message:<id>` or
/// `work:<opener>[<work ids>] final:<id> <label> <files label> <stats>`.
fn shape(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item["kind"].as_str() {
            Some("completed-work") => format!(
                "work:{}{:?} final:{} {} / {} {}",
                item["id"].as_str().unwrap(),
                ids(item["work"].as_array().unwrap()),
                item["final"]["id"].as_str().unwrap_or("-"),
                item["label"],
                item["filesLabel"],
                item["stats"]
            ),
            _ => format!("message:{}", item["message"]["id"].as_str().unwrap_or("?")),
        })
        .collect()
}

/// Every tool call the items draw, by call id.
fn calls(items: &[Value]) -> Vec<String> {
    fn scan(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                if fields.get("type").and_then(Value::as_str) == Some("tool-call") {
                    if let Some(id) = fields.get("callId").and_then(Value::as_str) {
                        out.push(id.to_string());
                    }
                }
                for (key, value) in fields {
                    if key != "deferredWork" && key != "deferred" {
                        scan(value, out);
                    }
                }
            }
            Value::Array(values) => values.iter().for_each(|value| scan(value, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    items.iter().for_each(|item| scan(item, &mut out));
    out.sort();
    out.dedup();
    out
}

struct Reopened {
    /// The chat's items while it holds every row.
    full: Vec<Value>,
    /// The snapshot gxserver collapses the same rows into.
    snapshot: Vec<Value>,
    /// The chat reopened from the snapshot, with the chosen folds opened.
    expanded: Vec<Value>,
    /// The most detail pages one fold's walk read.
    pages: usize,
}

/// Reopens the chat from the snapshot gxserver makes of a transcript file, then opens each
/// fold `open` picks the way a renderer does: the fold's `deferred` read through the history
/// detail reader by the chat core's own walk, under the fold's id.
fn reopen(rows: &[(&str, &str)], open: impl Fn(&str) -> bool) -> Reopened {
    use ghostex_gx_chat_core::transcript::deferred_work::{DeferredWalk, WalkStep};
    let agent = SessionChatTranscriptAgent::Pi;
    let path = transcript_file(&records(rows));
    let SessionChatTailPage::Page {
        messages,
        before_offset,
        ..
    } = read_session_chat_tail_page_stitched(agent, &path, 2000, None)
        .expect("tail")
        .page
    else {
        panic!("a transcript page");
    };
    let full_rows: Vec<Value> = messages.iter().map(|message| json!(message)).collect();
    let full = items(&mut chat_state(&full_rows));
    let snapshot = collapse_snapshot_messages(&messages, before_offset);
    let mut reopened = chat_state(&snapshot);
    let folds: Vec<(String, Value)> = items(&mut reopened)
        .iter()
        .filter(|item| item["kind"] == "completed-work" && item["deferred"].is_object())
        .map(|item| {
            (
                item["id"].as_str().unwrap().to_string(),
                item["deferred"].clone(),
            )
        })
        .filter(|(id, _)| open(id))
        .collect();
    let mut pages = 0;
    for (id, work) in &folds {
        let mut walk = DeferredWalk::begin(id.clone(), Some(work)).expect("a walk");
        let mut read = 0;
        let loaded = loop {
            read += 1;
            assert!(read < 50, "{id}: the walk does not end");
            let page = read_history(agent, &path, walk.cursor as u64, 200, true, false, "p", "s")
                .expect("a detail page");
            match walk.advance(&page) {
                WalkStep::ReadPage { .. } => continue,
                WalkStep::Done { messages } => break messages,
                WalkStep::Failed { message } => panic!("{id}: {message}"),
            }
        };
        pages = pages.max(read);
        reopened.transcript_view.deferred.insert(id.clone(), loaded);
        reopened.transcript_view.detail_revision += 1;
    }
    let expanded = items(&mut reopened);
    let _ = std::fs::remove_file(path);
    Reopened {
        full,
        snapshot,
        expanded,
        pages,
    }
}

/// The `deferredWork` the snapshot gives, by the row that carries it: (row, start, end).
fn ranges(snapshot: &[Value]) -> Vec<(String, String, String)> {
    snapshot
        .iter()
        .filter(|row| row["deferredWork"].is_object())
        .map(|row| {
            let work = &row["deferredWork"];
            (
                row["id"].as_str().unwrap().to_string(),
                work["startId"].as_str().unwrap().to_string(),
                work["endId"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

const STANDARD: [(&str, &str); 10] = [
    ("u1", "user"),
    ("c1", "call"),
    ("t1", "result"),
    ("a1", "reply"),
    ("w1", "irc"),
    ("c2", "call"),
    ("t2", "result"),
    ("a2", "reply"),
    ("u2", "user"),
    ("a3", "reply"),
];

#[test]
fn every_reopened_fold_holds_its_own_work_around_an_omp_wake_up() {
    let reopened = reopen(&STANDARD, |_| true);
    println!(
        "full {:?}\nreopened and expanded {:?}\nranges {:?}",
        shape(&reopened.full),
        shape(&reopened.expanded),
        ranges(&reopened.snapshot)
    );
    // One range for the prompt, as every chat core reads it; the wake-up's fold cuts its part.
    assert_eq!(
        ranges(&reopened.snapshot),
        [("u1".to_string(), "u1".to_string(), "a2".to_string())]
    );
    assert_eq!(shape(&reopened.expanded), shape(&reopened.full));
    assert!(shape(&reopened.full)
        .iter()
        .any(|item| item.starts_with("work:w1[\"w1\", \"c2\"] final:a2")));

    // The older history page carries the same rows and range as the snapshot.
    let path = transcript_file(&records(&STANDARD));
    let SessionChatTailPage::Page { messages, .. } =
        read_session_chat_tail_page_stitched(SessionChatTranscriptAgent::Pi, &path, 200, None)
            .expect("tail")
            .page
    else {
        panic!("a transcript page");
    };
    let u2 = messages.iter().find(|message| message.id == "u2").unwrap();
    let page = read_history(
        SessionChatTranscriptAgent::Pi,
        &path,
        encode_stitched_cursor(0, u2.byte_offset.unwrap()),
        200,
        false,
        false,
        "p",
        "s",
    )
    .expect("a history page");
    let _ = std::fs::remove_file(path);
    let older = page["messages"].as_array().unwrap();
    assert_eq!(ids(older), ["u1", "a1", "w1", "a2"]);
    assert_eq!(older[..], reopened.snapshot[..4]);
}

#[test]
fn a_wake_up_fold_loads_its_work_through_the_prompt_range() {
    let reopened = reopen(&STANDARD, |id| id == "w1");
    let fold = |items: &[Value]| {
        shape(items)
            .into_iter()
            .find(|item| item.starts_with("work:w1"))
    };
    println!(
        "reopened, only the wake-up opened {:?}",
        shape(&reopened.expanded)
    );
    assert_eq!(fold(&reopened.expanded), fold(&reopened.full));
    assert!(shape(&reopened.expanded)
        .iter()
        .any(|item| item.starts_with("work:u1[] final:a1")));
}

#[test]
fn reopened_folds_match_the_full_list_for_omp_delivery_sequences() {
    let pairs: Vec<(String, String)> = (0..125)
        .map(|n| (format!("c{n}"), format!("t{n}")))
        .collect();
    let mut long = vec![("u1", "user"), ("a1", "reply"), ("w1", "irc")];
    for (call, result) in &pairs {
        long.push((call.as_str(), "call"));
        long.push((result.as_str(), "result"));
    }
    long.extend([("a2", "reply"), ("u2", "user"), ("a3", "reply")]);
    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        (
            "delivery while the agent works",
            vec![
                ("u1", "user"),
                ("c1", "call"),
                ("w1", "irc"),
                ("t1", "result"),
                ("a1", "reply"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "two wake-ups",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c1", "call"),
                ("t1", "result"),
                ("a2", "reply"),
                ("w2", "irc"),
                ("c2", "call"),
                ("t2", "result"),
                ("a3", "reply"),
                ("u2", "user"),
                ("a4", "reply"),
            ],
        ),
        (
            "consecutive deliveries",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("w2", "irc"),
                ("c1", "call"),
                ("t1", "result"),
                ("a2", "reply"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "passive notice before the delivery",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("p1", "passive"),
                ("w1", "irc"),
                ("c1", "call"),
                ("t1", "result"),
                ("a2", "reply"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "hidden system row before the delivery",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("h1", "hidden-system"),
                ("w1", "irc"),
                ("c1", "call"),
                ("t1", "result"),
                ("a2", "reply"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "hidden user row before the delivery",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("h1", "hidden-user"),
                ("w1", "irc"),
                ("c1", "call"),
                ("t1", "result"),
                ("a2", "reply"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "edits in the prompt's turn and the wake-up's",
            vec![
                ("u1", "user"),
                ("c1", "write"),
                ("t1", "write-result"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "write"),
                ("t2", "write-result"),
                ("a2", "reply"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a question after a delivery the agent did not wake from",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("r1", "reasoning"),
                ("w1", "irc"),
                ("c2", "ask"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a collapsed background result just before a delivery",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("j1", "async"),
                ("w1", "irc"),
                ("c1", "call"),
                ("t1", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a background result the chat never shows before another",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("x1", "hidden-async"),
                ("j1", "async"),
                ("r1", "reasoning"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a delivery mid-work after a wake-up the chat collapses",
            vec![
                ("u1", "user"),
                ("a1", "reply"),
                ("j1", "async"),
                ("r1", "reasoning"),
                ("w1", "irc"),
                ("a2", "reply"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        ("two detail pages", long),
    ];
    for (case, rows) in &cases {
        let reopened = reopen(rows, |_| true);
        println!(
            "{case}: full {:?}\n  reopened and expanded {:?}\n  ranges {:?}, detail pages {}",
            shape(&reopened.full),
            shape(&reopened.expanded),
            ranges(&reopened.snapshot),
            reopened.pages
        );
        assert_eq!(shape(&reopened.expanded), shape(&reopened.full), "{case}");
        assert_eq!(calls(&reopened.expanded), calls(&reopened.full), "{case}");
        // Only a prompt the chat shows owns a range.
        assert!(
            ranges(&reopened.snapshot)
                .iter()
                .all(|(row, ..)| row.starts_with('u')),
            "{case}"
        );
        if *case == "two detail pages" {
            assert!(
                reopened.pages >= 2,
                "the walk read {} pages",
                reopened.pages
            );
        }
    }
}

/// How the chat draws the call row `id` made (`<id>-call`): its tool card entry and its plain
/// tool row, in whichever row the chat folded it into (its own, a fold's work or a fold's final
/// reply).
fn call_status(items: &[Value], id: &str) -> Option<(Value, Value)> {
    let call = format!("{id}-call");
    items
        .iter()
        .flat_map(|item| {
            [&item["message"], &item["final"]]
                .into_iter()
                .chain(item["work"].as_array().into_iter().flatten())
        })
        .find_map(|row| {
            let at = row["blocks"]
                .as_array()?
                .iter()
                .filter(|block| block["type"] == "tool-call")
                .position(|block| block["callId"] == call.as_str())?;
            let entry = row["toolCard"]["entries"]
                .as_array()
                .and_then(|entries| {
                    entries
                        .iter()
                        .find(|entry| entry["kind"] == "tool" && entry["index"] == at)
                })
                .map_or(Value::Null, |entry| entry["status"].clone());
            Some((entry, row["tools"][at]["status"].clone()))
        })
}

/// A delivery the agent answered only with the result of a call made before its reply: the
/// prompt's fold holds that call alone, as in the full list, and every call stays reachable.
#[test]
fn a_wake_up_answered_only_by_an_earlier_result_leaves_the_prompts_fold_as_in_the_full_list() {
    let reopened = reopen(
        &[
            ("u1", "user"),
            ("c1", "call"),
            ("a1", "reply"),
            ("w1", "irc"),
            ("t1", "result"),
            ("u2", "user"),
            ("a3", "reply"),
        ],
        |_| true,
    );
    let prompt = |items: &[Value]| {
        shape(items)
            .into_iter()
            .find(|item| item.starts_with("work:u1"))
    };
    println!(
        "full {:?}\nreopened and expanded {:?}",
        shape(&reopened.full),
        shape(&reopened.expanded)
    );
    assert_eq!(prompt(&reopened.expanded), prompt(&reopened.full));
    assert_eq!(calls(&reopened.expanded), calls(&reopened.full));
}

/// A call kept in view (after a wake-up the agent answered only with work, a reply's own call,
/// a question, a message to another agent) reads as it ends in the full list, before any fold
/// loads and after every fold loaded, whatever rows sit between it and its results.
#[test]
fn a_kept_call_shows_how_it_ended_before_its_fold_loads() {
    let cases: [(&str, &str, &[(&str, &str)]); 14] = [
        (
            "a question before the final reply, answered after it",
            "c2",
            &[
                ("u1", "user"),
                ("c2", "ask"),
                ("a2", "reply"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a hidden row between the call and its result",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("h1", "hidden-user"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "the same, with work before the delivery folded away",
            "c2",
            &[
                ("u1", "user"),
                ("c1", "call"),
                ("t1", "result"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("h1", "hidden-user"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "two calls in separate rows before their results",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("c3", "call"),
                ("t2", "result"),
                ("t3", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "two calls in one row, results out of order",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c23", "two-calls"),
                ("t3", "result"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a failed result",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("t2", "failed-result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a status the chat shows between the call and its result",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("s1", "status"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a reasoning row between the call and its result",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("r1", "reasoning"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "two calls in separate rows that never answered",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("c3", "call"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a reply with a call in the same message, and its result",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "mixed"),
                ("t2", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a question after another call, before both results",
            "c3",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("c3", "ask"),
                ("t2", "result"),
                ("t3", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a message to another agent after another call, before both results",
            "c3",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("c3", "send"),
                ("t2", "result"),
                ("t3", "result"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a shell command's output, which names no call",
            "c2",
            &[
                ("u1", "user"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("b1", "bash"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
        (
            "a call that never answered",
            "c2",
            &[
                ("u1", "user"),
                ("c1", "call"),
                ("t1", "result"),
                ("a1", "reply"),
                ("w1", "irc"),
                ("c2", "call"),
                ("u2", "user"),
                ("a3", "reply"),
            ],
        ),
    ];
    for (case, call, rows) in cases {
        let unopened = reopen(rows, |_| false);
        let full = call_status(&unopened.full, call);
        let kept = call_status(&unopened.expanded, call);
        println!("{case}: full {full:?}, kept before loading {kept:?}");
        assert!(full.is_some(), "{case}");
        assert_eq!(kept, full, "{case}");
        let opened = reopen(rows, |_| true);
        assert_eq!(call_status(&opened.expanded, call), full, "{case}, loaded");
        assert_eq!(calls(&opened.expanded), calls(&opened.full), "{case}");
    }
}

#[test]
fn a_wake_up_the_agent_answered_only_with_work_still_loads_that_work_into_its_own_fold() {
    let rows = [
        ("u1", "user"),
        ("c1", "call"),
        ("t1", "result"),
        ("a1", "reply"),
        ("w1", "irc"),
        ("c2", "call"),
        ("t2", "result"),
        ("r1", "reasoning"),
        ("c3", "call"),
        ("t3", "result"),
        ("u2", "user"),
        ("a3", "reply"),
    ];
    // Before its fold loads, the call kept in view to show the agent worked after the
    // delivery shows how it ended, as its recorded result says.
    let unopened = reopen(&rows, |_| false);
    let fold = unopened
        .expanded
        .iter()
        .find(|item| item["id"] == "w1")
        .expect("the wake-up's fold");
    let call = fold["work"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "c2")
        .expect("the call kept in view");
    println!(
        "kept call before loading: card {} rows {}",
        call["toolCard"], call["tools"]
    );
    assert_eq!(call["toolCard"]["entries"][0]["status"], "done");
    assert_eq!(call["tools"][0]["status"], "done");

    let reopened = reopen(&rows, |_| true);
    let (full, expanded) = (shape(&reopened.full), shape(&reopened.expanded));
    println!("full {full:?}\nreopened and expanded {expanded:?}");
    // The full list draws a turn without a reply unfolded, as it does a prompt's; reopened, its
    // work is folded under the wake-up and loads there, never into the turn before it. It
    // ran from the delivery (second 4) to its last result (second 9), past the run of work
    // kept in view, which the reasoning row ends.
    assert_eq!(&full[..2], &expanded[..2]);
    assert!(full[2..].starts_with(&["message:w1".to_string(), "message:c2".to_string()]));
    assert!(
        expanded[2].starts_with("work:w1[\"w1\", \"c2\""),
        "{expanded:?}"
    );
    assert!(
        expanded[2].contains("final:- \"Worked for 5s\""),
        "{expanded:?}"
    );
    assert_eq!(calls(&reopened.expanded), calls(&reopened.full));
}

/// A transcript row without its optional byte offset replacing the turns after a settled
/// prefix that ends in a wake-up's turn: the retained prefix draws what a fresh build draws.
#[test]
fn a_row_without_an_offset_after_a_wake_up_turn_draws_the_same_from_a_retained_prefix() {
    use ghostex_gx_chat_core::transcript::presentation::build;
    let reopened = reopen(
        &[
            ("u1", "user"),
            ("a1", "reply"),
            ("w1", "irc"),
            ("c1", "call"),
            ("t1", "result"),
            ("a2", "reply"),
            ("w2", "irc"),
            ("c2", "call"),
            ("t2", "result"),
            ("a3", "reply"),
            ("u2", "user"),
            ("a4", "reply"),
        ],
        |_| false,
    );
    let mut state = chat_state(&reopened.snapshot);
    items(&mut state);
    let at = state
        .messages
        .composed
        .iter()
        .position(|message| message.id == "u2")
        .unwrap();
    let later = state.messages.composed.last().unwrap().timestamp.unwrap() + 1000;
    state.messages.composed.truncate(at);
    state.messages.composed.push(
        serde_json::from_value(json!({"id": "offsetless", "role": "user",
            "source": "transcript", "timestamp": later,
            "blocks": [{"type": "text", "text": "later arrived prompt"}]}))
        .unwrap(),
    );
    state.messages.list = state.messages.composed.clone();
    state.messages.compose_generation += 1;
    let retained = items(&mut state);
    let projection = build(&state, &ghostex_gx_chat_core::ChatContext::default());
    let mut fresh: Vec<Value> = projection
        .items
        .iter()
        .map(|item| serde_json::to_value(item).unwrap())
        .collect();
    // The rows refresh moves the newest run's Worked time to the working card (`rows.rs`).
    if let Some((id, _)) = projection.worked_turn {
        if let Some(item) = fresh
            .iter_mut()
            .rev()
            .find(|item| item["kind"] == "completed-work" && item["id"] == id.as_str())
        {
            item["label"] = json!("Previous turn");
        }
    }
    println!("retained {:?}\nfresh {:?}", shape(&retained), shape(&fresh));
    assert_eq!(retained, fresh);
}
