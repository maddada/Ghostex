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
pub const WORK_GROUPS: [(&str, &str); 7] = [
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    // CDXC:Coordinators 2026-11-10 DECISION:
    // User: "Please don't say Moving say In Progress. Moving isn't clear to me" (and on 2026-11-07, of
    // the board: "rename to in progress please"). Wherever the user reads it, the `moving` status is
    // "In progress"; the stored value stays `moving` and `--status` takes `moving` or `in-progress`.
    ("moving", "In progress"),
    // CDXC:Coordinators 2026-11-11 DECISION:
    // User: "the user should have accurate statuses in the work board/list always!" An item the
    // coordinator calls In progress that nobody is on (no thread working, waiting on the user or
    // owing work, and not changed by the coordinator in its current or last turn) is gxserver's
    // `idle` (server/src/coordinators/turns.rs): listed here under In progress, folded, with its
    // last line, and left out of every "in progress" count. A core from before reads `idle` as
    // Later.
    ("idle", "Not being worked on"),
    ("hold", "Waiting"),
    ("later", "Later"),
    ("done", "Finished today"),
];

/// Groups folded until the reader opens them.
pub fn folded_by_default(group: &str) -> bool {
    matches!(group, "idle" | "hold" | "later" | "done")
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
        "idle" => "idle",
        "waiting" => "hold",
        "finished" => "done",
        _ => "later",
    }
}

/// An item's own chips after the board's: usage-limit reset pill (when paused on a limit), who
/// has the ball, its steps, what it asks of the user (its open asks not snoozed at `now_ms`: a
/// snoozed one waits in Later, so it is not "for you").
fn item_chips(item: &Value, status: &str, now_ms: i64) -> Vec<Value> {
    let mut chips = Vec::new();
    if !matches!(status, "finished" | "dropped") {
        if let Some(thread) = crate::extras::work_cards::usage_limit_thread(item) {
            if status != "needs" || !crate::extras::work_cards::thread_wait_needs_user(thread) {
                let (label, tone, tip) =
                    crate::extras::work_cards::usage_limit_pill(thread, now_ms);
                chips.push(json!({
                    "kind": "usageLimit",
                    "label": label,
                    "tone": tone,
                    "title": tip,
                    "command": {
                        "type": "openCoordinatorThread",
                        "serverId": text(thread, "serverId"),
                        "projectId": text(thread, "projectId"),
                        "sessionId": text(thread, "sessionId"),
                    },
                }));
            }
        }
    }
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
        .filter(|ask| ask["open"] == true && !snoozed_at(ask, now_ms))
        .count();
    if open > 0 {
        chips.push(
            json!({ "kind": "question", "label": format!("{open} for you"), "tone": "question" }),
        );
    }
    chips
}

/// An item's row, in the shape of a board item row.
fn item_row(item: &Value, group: &str, status: &str, now_ms: i64) -> Value {
    let reference = text(item, "ref");
    let waiting_thread = crate::extras::work_cards::waiting_user_thread(item);
    let mut title = text(item, "title").to_string();
    let mut line = text(item, "now").to_string();
    if status == "waiting" {
        let waits = waits_line(item);
        if !waits.is_empty() {
            line = waits;
        }
    }
    let thread_wait = status == "needs"
        && waiting_thread.is_some()
        && !item["asks"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|ask| ask["open"] == true);
    let mut open_label = String::new();
    let mut wait_kind_str = String::new();
    if let Some(thread) = waiting_thread.filter(|_| thread_wait) {
        let (wait_title, wait_subtitle, wait_kind, action_label) =
            crate::extras::work_cards::thread_wait_copy(Some(item), thread, now_ms);
        title = wait_title;
        line = wait_subtitle;
        wait_kind_str = wait_kind.to_string();
        open_label = action_label.to_string();
    }
    let mut row = json!({
        "key": text(item, "key"),
        "kind": "item",
        "ref": reference,
        "group": group,
        "title": title,
        "detail": line,
        // A row with nothing to say yet says who has it, never the board's "No links yet".
        "note": if line.is_empty() {
            match status {
                "later" => "Nobody is on it yet".to_string(),
                "idle" => "Nobody is on it".to_string(),
                "finished" => "Finished".to_string(),
                _ => owner_words(item),
            }
        } else {
            line.clone()
        },
        "at": text(item, "at"),
        "workChips": item_chips(item, status, now_ms),
    });
    if thread_wait {
        row["threadWait"] = json!(true);
        row["waitKind"] = json!(wait_kind_str);
        row["openLabel"] = json!(open_label);
    }
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
    // Under Needs you with no open question: Done, its one answer ("W7: Done"), given until the
    // coordinator next changes the item (`saidDone`); `answerKey` locks it here and on its dock
    // row alike (work_cards.rs `done_answer_key`). Thread-wait rows never get a bare Done button.
    if let Some(answer_key) = crate::extras::work_cards::done_answer_key(item) {
        row["answers"] = json!([{ "label": "Done", "recommended": false }]);
        row["answerKey"] = json!(answer_key);
        if item["saidDone"] == true {
            row["answered"] = json!("Done");
        }
    }
    // A thread on it genuinely waiting on the user: the board's chip that opens it. Usage-limit
    // pauses and quiet threads do not get approval/question chips here (Q4476-Q4478).
    if let Some(thread) = waiting_thread {
        for key in ["projectId", "sessionId", "serverId"] {
            row[key] = thread.get(key).cloned().unwrap_or(json!(""));
        }
        let kind = if thread["needsApproval"] == true || text(thread, "waitKind") == "approval" {
            "approval"
        } else {
            match text(thread, "waitKind") {
                "signIn" => "signIn",
                "usageLimit" => "usageLimit",
                _ => "question",
            }
        };
        row["waiting"] = json!(kind);
        row["needsApproval"] = json!(kind == "approval");
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
fn ask_row(items: &[&Value], item: &Value, ask: &Value, now_ms: i64) -> Value {
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
        // Its title's links (`#562`, a Markdown link) as runs the renderers draw, or null.
        "titleRuns": crate::questions::form_links::word_runs(text(ask, "text")),
        // Its longer explanation under the short title (`item ask --detail`).
        "body": crate::transcript::native_markdown::ask_markdown(text(ask, "detail")),
        "detail": detail,
        "note": if steps.is_empty() { detail.clone() } else { format!("{detail} \u{b7} Steps: {steps}") },
        "at": text(ask, "askedAt"),
        "answers": answers,
        "answered": text(ask, "answered"),
        // The item pills that lead the opened card's answers (work_cards.rs `item_pills`), and
        // what the opened card's line says in place of the item's name they carry: only a snooze.
        "itemPills": crate::extras::work_cards::item_pills(items, item, tag),
        "openLine": if snoozed.is_empty() { String::new() } else { format!("Snoozed until {snoozed}") },
    })
}

/// A thread on no item, in the shape of a board thread row.
fn thread_row(item: &Value, group: &str, now_ms: i64) -> Value {
    let thread = item["threads"].get(0).cloned().unwrap_or(Value::Null);
    let mut title = text(item, "title").to_string();
    let mut detail = text(item, "now").to_string();
    let thread_wait = group == "needs"
        && crate::extras::work_cards::waiting_user_thread(item).is_some();
    let mut open_label = String::new();
    let mut wait_kind_str = String::new();
    if thread_wait {
        let (wait_title, wait_subtitle, wait_kind, action_label) =
            crate::extras::work_cards::thread_wait_copy(Some(item), &thread, now_ms);
        title = wait_title;
        detail = wait_subtitle;
        wait_kind_str = wait_kind.to_string();
        open_label = action_label.to_string();
    }
    let mut row = json!({
        "key": text(item, "key"),
        "kind": "thread",
        "ref": text(item, "ref"),
        "group": group,
        "title": title,
        "detail": detail,
        "at": text(item, "at"),
        "projectId": text(&thread, "projectId"),
        "sessionId": text(&thread, "sessionId"),
    });
    if thread_wait {
        row["threadWait"] = json!(true);
        row["waitKind"] = json!(wait_kind_str);
        row["openLabel"] = json!(open_label);
    } else if !matches!(group, "done" | "needs") && text(&thread, "waitKind") == "usageLimit" {
        let (label, tone, tip) = crate::extras::work_cards::usage_limit_pill(&thread, now_ms);
        row["workChips"] = json!([{
            "kind": "usageLimit",
            "label": label,
            "tone": tone,
            "title": tip,
            "command": {
                "type": "openCoordinatorThread",
                "serverId": text(&thread, "serverId"),
                "projectId": text(&thread, "projectId"),
                "sessionId": text(&thread, "sessionId"),
            },
        }]);
    }
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
    let all_items: Vec<&Value> = work["items"].as_array().into_iter().flatten().collect();
    for item in work["items"].as_array().into_iter().flatten() {
        let status = text(item, "status");
        if status == "dropped" {
            continue;
        }
        let group = group_of(status);
        if item["kind"] == "thread" {
            push(group, thread_row(item, group, now_ms));
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
                ask_row(&all_items, item, ask, now_ms),
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
                .any(crate::extras::work_cards::thread_wait_needs_user);
        if !only_asking {
            push(group, item_row(item, group, status, now_ms));
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
