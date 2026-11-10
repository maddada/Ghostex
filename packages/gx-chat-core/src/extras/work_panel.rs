//! The Work panel: a coordinator's work items (`coordinatorThreads.work`, server/src/coordinators/
//! work.rs) laid out for the panel the Board pill opened, which it replaces. One row per line of
//! work, grouped by who has the ball in the six statuses, Needs you first; what the user is asked
//! is a row of its own under Needs you (Blocked when it holds the work up), naming the work it
//! belongs to, with its answer buttons or Done; threads on no item keep a row of their own.
//!
//! The rows are written in the shape gxserver sends the board's rows, so `coordinator_board`
//! words, caps, folds, counts and menus them exactly as it did the board, and every renderer draws
//! them with the board's own panel, columns, drag and menus.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User, on the work items design page (Q2092 "B", Q2093 six statuses, Q2096 "replace"): the Board
//! pill becomes Work, "one row per line of work, grouped by who has the ball", Needs you first.
//! The groups are Needs you, Blocked, Moving, Waiting, Later and Finished today; Waiting, Later and
//! Finished stay folded. Needs you lists what the user can do, each naming the work it belongs to
//! and whether it holds it up; a row in Moving says who is on it, its steps and how many things
//! it asks of the user. Chores never show (Q2094). Rows keep the board's group ids (`hold` is
//! Waiting, `done` Finished) so the renderers' glyphs and colours carry over.
//! SEE-ALSO: packages/gx-chat-core/src/extras/coordinator_board.rs (the wording, folds and menus),
//! packages/gx-chat-core/src/extras/work_cards.rs (the same items as cards in the chat).

use serde_json::{json, Map, Value};

use crate::extras::work_cards::{owner_words, waits_line};

/// The groups, in the order the panel lists them: the board's ids, the Work panel's titles.
pub const WORK_GROUPS: [(&str, &str); 6] = [
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    // CDXC:Coordinators 2026-11-10 DECISION:
    // User: "Please don't say Moving say In Progress. Moving isn't clear to me" (and on 2026-11-07, of
    // the board: "rename to in progress please"). Wherever the user reads it, the `moving` status is
    // "In progress"; the stored value stays `moving` and `--status` takes `moving` or `in-progress`.
    ("moving", "In progress"),
    ("hold", "Waiting"),
    ("later", "Later"),
    ("done", "Finished today"),
];

/// Groups folded until the reader opens them.
pub fn folded_by_default(group: &str) -> bool {
    matches!(group, "hold" | "later" | "done")
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The board group a status is listed under.
fn group_of(status: &str) -> &'static str {
    match status {
        "needs" => "needs",
        "blocked" => "blocked",
        "moving" => "moving",
        "waiting" => "hold",
        "finished" => "done",
        _ => "later",
    }
}

/// An item's own chips after the board's: who has the ball, its steps, what it asks of the user.
fn item_chips(item: &Value, status: &str) -> Vec<Value> {
    let mut chips = Vec::new();
    let owner = owner_words(item);
    let waiting_says = status == "waiting" && !waits_line(item).is_empty();
    // Only a thread is worth naming: "You" and "Coordinator" are what the group already says
    // (Q2167).
    if !owner.is_empty()
        && !waiting_says
        && !matches!(status, "needs" | "blocked")
        && text(&item["owner"], "kind") == "thread"
    {
        chips.push(json!({ "kind": "owner", "label": owner, "tone": "" }));
    }
    let total = item["steps"]["total"].as_u64().unwrap_or(0);
    if total > 0 {
        chips.push(json!({
            "kind": "steps",
            "label": format!("{} of {total} steps", item["steps"]["done"].as_u64().unwrap_or(0)),
            "tone": "",
        }));
    }
    let open = item["asks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|ask| ask["open"] == true)
        .count();
    if open > 0 {
        chips.push(
            json!({ "kind": "question", "label": format!("{open} for you"), "tone": "question" }),
        );
    }
    chips
}

/// An item's row, in the shape of a board item row.
fn item_row(item: &Value, group: &str, status: &str) -> Value {
    let reference = text(item, "ref");
    let mut line = text(item, "now").to_string();
    if status == "waiting" {
        let waits = waits_line(item);
        if !waits.is_empty() {
            line = waits;
        }
    }
    let mut row = json!({
        "key": text(item, "key"),
        "kind": "item",
        "ref": reference,
        "group": group,
        "title": text(item, "title"),
        "detail": line,
        // A row with nothing to say yet says who has it, never the board's "No links yet".
        "note": if line.is_empty() {
            match status {
                "later" => "Nobody is on it yet".to_string(),
                "finished" => "Finished".to_string(),
                _ => owner_words(item),
            }
        } else {
            line.clone()
        },
        "at": text(item, "at"),
        "workChips": item_chips(item, status),
    });
    for key in ["reason", "release", "link", "tag", "tagLook", "links", "pr"] {
        if let Some(value) = item.get(key).filter(|value| !value.is_null()) {
            row[key] = value.clone();
        }
    }
    if group != "later" {
        if let Some(row) = row.as_object_mut() {
            row.remove("reason");
        }
    }
    if item["untested"] == true {
        row["untested"] = json!(true);
    }
    // A thread on it waiting on the user, or not moving: the board's chips that open it.
    if let Some(thread) = item["threads"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|thread| thread["state"] == "waiting" || thread["stuck"] == true)
    {
        for key in ["projectId", "sessionId", "serverId"] {
            row[key] = thread.get(key).cloned().unwrap_or(json!(""));
        }
        if thread["stuck"] == true {
            row["stuck"] = json!(true);
        } else {
            row["waiting"] = json!(if thread["needsApproval"] == true {
                "approval"
            } else {
                "question"
            });
            row["needsApproval"] = json!(thread["needsApproval"] == true);
        }
    }
    // No links of its own: its first thread's.
    if row.get("links").is_none() {
        if let Some(links) = item["threads"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|thread| thread.get("links").filter(|links| links.is_array()))
        {
            row["links"] = links.clone();
        }
    }
    row
}

/// An open ask's row: what the user is asked, under the work it belongs to, with its answers (or
/// Done for a task). Its key is its chat card's, so an answer given in either place locks both.
fn ask_row(item: &Value, ask: &Value, now_ms: i64) -> Value {
    let reference = text(item, "ref");
    let tag = text(ask, "tag");
    let holds = ask["holds"] == true;
    let answers = match ask["answers"].as_array() {
        Some(answers) if !answers.is_empty() => Value::Array(answers.clone()),
        _ => json!([{ "label": "Done", "recommended": false }]),
    };
    let mut detail = text(item, "title").to_string();
    if holds {
        detail.push_str(" \u{b7} Holds it up");
    }
    let steps = text(ask, "steps");
    // Snoozed: under Later, quietly saying until when (work.rs `snooze`).
    let snoozed = if snoozed_at(ask, now_ms) {
        text(ask, "snoozedUntilLabel")
    } else {
        ""
    };
    if !snoozed.is_empty() {
        detail = format!("{detail} \u{b7} Snoozed until {snoozed}");
    }
    json!({
        "key": format!("work-ask:{reference}:{tag}"),
        "kind": "item",
        "ref": reference,
        "ask": tag,
        "askRow": true,
        "group": if !snoozed.is_empty() { "later" } else if holds { "blocked" } else { "needs" },
        "title": text(ask, "text"),
        "detail": detail,
        "note": if steps.is_empty() { detail.clone() } else { format!("{detail} \u{b7} Steps: {steps}") },
        "at": text(ask, "askedAt"),
        "answers": answers,
        "answered": text(ask, "answered"),
    })
}

/// A thread on no item, in the shape of a board thread row.
fn thread_row(item: &Value, group: &str) -> Value {
    let thread = item["threads"].get(0).cloned().unwrap_or(Value::Null);
    let mut row = json!({
        "key": text(item, "key"),
        "kind": "thread",
        "ref": text(item, "ref"),
        "group": group,
        "title": text(item, "title"),
        "detail": text(item, "now"),
        "at": text(item, "at"),
        "projectId": text(&thread, "projectId"),
        "sessionId": text(&thread, "sessionId"),
    });
    for key in [
        "serverId",
        "globalRef",
        "agent",
        "branch",
        "computer",
        "unfinished",
    ] {
        let value = text(&thread, key);
        if !value.is_empty() {
            row[key] = json!(value);
        }
    }
    for key in ["links", "pr"] {
        if let Some(value) = thread.get(key).filter(|value| !value.is_null()) {
            row[key] = value.clone();
        }
    }
    for key in ["stuck", "needsApproval", "untested"] {
        if thread[key] == true {
            row[key] = json!(true);
        }
    }
    for key in ["flag", "flagTone", "reason", "waiting", "tag", "tagLook"] {
        if let Some(value) = item.get(key).filter(|value| !value.is_null()) {
            row[key] = value.clone();
        }
    }
    row
}

/// The work as a board: `{groups: [{id, total, rows}], items, tagOptions}`, the shape gxserver
/// sends the board in.
pub fn work_board(work: &Value) -> Value {
    work_board_at(work, 0)
}

/// Whether an ask is still snoozed at `now_ms` (0 when the caller has no clock: snoozed while
/// gxserver says so). A snooze that has ended brings the ask back to Needs you at once, as the
/// dock does.
fn snoozed_at(ask: &Value, now_ms: i64) -> bool {
    ask.get("snoozedUntil").is_some_and(|until| {
        crate::extras::work_cards::at_ms(until.as_str().unwrap_or_default())
            .is_some_and(|until| until > now_ms)
    })
}

/// [`work_board`] read at `now_ms`, so snoozes that have ended are back where they belong.
pub fn work_board_at(work: &Value, now_ms: i64) -> Value {
    let mut groups: Map<String, Value> = WORK_GROUPS
        .iter()
        .map(|(id, _)| (id.to_string(), json!([])))
        .collect();
    let mut push = |group: &str, row: Value| {
        if let Some(rows) = groups.get_mut(group).and_then(Value::as_array_mut) {
            rows.push(row);
        }
    };
    let mut mentions = Map::new();
    for item in work["items"].as_array().into_iter().flatten() {
        let status = text(item, "status");
        if status == "dropped" {
            continue;
        }
        let group = group_of(status);
        if item["kind"] == "thread" {
            push(group, thread_row(item, group));
            continue;
        }
        let reference = text(item, "ref");
        mentions.insert(reference.to_string(), json!(text(item, "title")));
        mentions.insert(reference.replacen('W', "B", 1), json!(text(item, "title")));
        let open_asks: Vec<&Value> = item["asks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|ask| ask["open"] == true)
            .collect();
        for ask in &open_asks {
            push(
                if snoozed_at(ask, now_ms) {
                    "later"
                } else if ask["holds"] == true {
                    "blocked"
                } else {
                    "needs"
                },
                ask_row(item, ask, now_ms),
            );
        }
        // Finished before today: off the panel, though a task it left for the user stays.
        if status == "finished" && item["today"] == false {
            continue;
        }
        // An item that is with the user only for its asks shows as them, not twice.
        let only_asking = matches!(status, "needs" | "blocked")
            && !open_asks.is_empty()
            && !item["threads"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|thread| thread["state"] == "waiting" || thread["stuck"] == true);
        if !only_asking {
            push(group, item_row(item, group, status));
        }
    }
    let groups: Vec<Value> = WORK_GROUPS
        .iter()
        .map(|(id, _)| {
            let mut rows = groups.remove(*id).unwrap_or(json!([]));
            if let Some(rows) = rows.as_array_mut() {
                // Newest first, as the board lists them.
                rows.sort_by(|left, right| text(right, "at").cmp(text(left, "at")));
            }
            let total = rows.as_array().map_or(0, Vec::len);
            json!({ "id": id, "total": total, "rows": rows })
        })
        .collect();
    let mut board = json!({ "groups": groups, "items": mentions });
    if let Some(options) = work.get("tagOptions") {
        board["tagOptions"] = options.clone();
    }
    board
}
