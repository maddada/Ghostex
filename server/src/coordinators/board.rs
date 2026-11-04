//! A coordinator's board: its lines of work grouped by who has the ball, for the chat's Board pill
//! and `ghostex coordinator board`.
//!
//! Five groups, in the order every client shows them: Later (nobody is on it yet: parked, up next or
//! held for a release), Needs you, Blocked, Moving, Done (today only), then Ideas, folded. Threads
//! land in a group from their own state and their PR's checks and review; the coordinator only
//! places what is not a thread (an item, `B7`) and moves a thread whose state cannot tell where it
//! stands (a pin: Blocked on a CI token, Needs you for a test only the user can do).
//!
//! The board stays light: only open work and today's Done. A thread nobody has touched for
//! `IDLE_THREAD_DAYS` leaves it unless the coordinator pinned it, Done items drop off after the day
//! they were done, and the chat's copy carries at most `SUMMARY_ROWS` rows per group (the CLI reads
//! them all).
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User, after the design pages (issues-1027/coordinator-board/board-explained.html, Q1243 "Yes, build it"): groups say who has the ball and tags say what a row is; five groups plus Ideas; "shift Later to be most left one"; and "keep this board light please", read as only current lines of work, old Done dropping off, a capped number of rows per group with "N more", nothing the user cleans up by hand. Rows sort themselves from thread, question and PR/CI state; the coordinator adds what is not a thread with `ghostex coordinator board add/move/note/done/drop`. The chat's questions join Needs you in the chat core (packages/gx-chat-core/src/extras/coordinator_board.rs), which also caps, folds and counts.
//! SEE-ALSO: server/src/coordinators/panel.rs (the thread rows and the cache this is built beside), server/src/ghostex_cli/coordinator/board.rs (the CLI), server/src/storage/migrations/0055_coordinator_board.sql (the items).

use chrono::{DateTime, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Map, Value};

use super::records::{now_iso, sql_error, SessionKey};
use crate::domain::DomainStateError;

/// The groups, in the order every client shows them.
pub const BOARD_GROUPS: [&str; 6] = ["later", "needs", "blocked", "moving", "done", "ideas"];

/// The rows per group the chat's copy carries; the CLI reads every row.
pub const SUMMARY_ROWS: usize = 12;

/// An unpinned thread that has not moved for this long leaves the board (it stays in Workers).
const IDLE_THREAD_DAYS: i64 = 3;

/// Done items older than this are deleted.
const PRUNE_DONE_DAYS: i64 = 14;

/// One row of `coordinator_board_items`: an item, or a pin when it names a thread.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BoardItem {
    pub number: i64,
    pub title: String,
    pub group: String,
    pub reason: String,
    pub note: String,
    pub link: String,
    pub release: String,
    pub untested: bool,
    pub thread_server_id: String,
    pub thread_project_id: String,
    pub thread_session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub done_at: Option<String>,
}

impl BoardItem {
    pub fn is_pin(&self) -> bool {
        !self.thread_session_id.is_empty()
    }

    fn pins(&self, server_id: &str, project_id: &str, session_id: &str) -> bool {
        self.is_pin()
            && self.thread_server_id == server_id
            && self.thread_project_id == project_id
            && self.thread_session_id == session_id
    }
}

/// The group a word names (`needs-you`, `up-next`, `parked`…) and the Later reason it implies.
pub fn parse_group(word: &str) -> Option<(&'static str, &'static str)> {
    let word = word.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    Some(match word.as_str() {
        "later" => ("later", ""),
        "parked" | "park" => ("later", "parked"),
        "next" | "up-next" | "upnext" => ("later", "next"),
        "held" | "hold" | "held-for-release" => ("later", "held"),
        "needs" | "needs-you" | "you" => ("needs", ""),
        "blocked" | "block" => ("blocked", ""),
        "moving" | "in-progress" | "working" => ("moving", ""),
        "done" => ("done", ""),
        "ideas" | "idea" => ("ideas", ""),
        _ => return None,
    })
}

fn item_from_row(row: &Row<'_>) -> rusqlite::Result<BoardItem> {
    Ok(BoardItem {
        number: row.get("itemNumber")?,
        title: row.get("title")?,
        group: row.get("boardGroup")?,
        reason: row.get("reason")?,
        note: row.get("note")?,
        link: row.get("link")?,
        release: row.get("release")?,
        untested: row.get::<_, i64>("untested")? != 0,
        thread_server_id: row.get("threadServerId")?,
        thread_project_id: row.get("threadProjectId")?,
        thread_session_id: row.get("threadSessionId")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
        done_at: row.get("doneAt")?,
    })
}

/// Every item and pin of one coordinator, oldest first.
pub fn list_board_items(
    db: &Connection,
    key: &SessionKey,
) -> Result<Vec<BoardItem>, DomainStateError> {
    let mut statement = db
        .prepare_cached(
            "SELECT * FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 ORDER BY itemNumber",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([&key.0, &key.1], item_from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

/// Every coordinator's items, by coordinator, for the supervisor's tick. A database without the
/// table (a test fixture) reads as no items.
pub fn list_all_board_items(
    db: &Connection,
) -> std::collections::HashMap<SessionKey, Vec<BoardItem>> {
    let mut items: std::collections::HashMap<SessionKey, Vec<BoardItem>> = Default::default();
    let Ok(mut statement) =
        db.prepare_cached("SELECT * FROM coordinator_board_items ORDER BY itemNumber")
    else {
        return items;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            (
                row.get::<_, String>("coordinatorProjectId")?,
                row.get::<_, String>("coordinatorSessionId")?,
            ),
            item_from_row(row)?,
        ))
    }) else {
        return items;
    };
    for (key, item) in rows.flatten() {
        items.entry(key).or_default().push(item);
    }
    items
}

fn read_item(
    db: &Connection,
    key: &SessionKey,
    number: i64,
) -> Result<Option<BoardItem>, DomainStateError> {
    db.query_row(
        "SELECT * FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber = ?3",
        params![key.0, key.1, number],
        item_from_row,
    )
    .optional()
    .map_err(sql_error)
}

fn write_item(db: &Connection, key: &SessionKey, item: &BoardItem) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO coordinator_board_items (
          coordinatorProjectId, coordinatorSessionId, itemNumber, title, boardGroup, reason, note,
          link, release, untested, threadServerId, threadProjectId, threadSessionId, createdAt,
          updatedAt, doneAt
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT (coordinatorProjectId, coordinatorSessionId, itemNumber) DO UPDATE SET
          title = excluded.title, boardGroup = excluded.boardGroup, reason = excluded.reason,
          note = excluded.note, link = excluded.link, release = excluded.release,
          untested = excluded.untested, updatedAt = excluded.updatedAt, doneAt = excluded.doneAt
        "#,
        params![
            key.0,
            key.1,
            item.number,
            item.title,
            item.group,
            item.reason,
            item.note,
            item.link,
            item.release,
            i64::from(item.untested),
            item.thread_server_id,
            item.thread_project_id,
            item.thread_session_id,
            item.created_at,
            item.updated_at,
            item.done_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The next B<number>: one past the highest ever used, so a dropped item's number is not reused
/// while its row is still around to be confused with it. A thread's pin takes the next number
/// below zero instead, so pins never use up or answer to an item's B<number>.
fn next_number(db: &Connection, key: &SessionKey, pin: bool) -> Result<i64, DomainStateError> {
    let sql = if pin {
        "SELECT COALESCE(MIN(itemNumber), 0) - 1 FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber < 0"
    } else {
        "SELECT COALESCE(MAX(itemNumber), 0) + 1 FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber > 0"
    };
    db.query_row(sql, params![key.0, key.1], |row| row.get(0))
        .map_err(sql_error)
}

fn prune_done(db: &Connection, key: &SessionKey) -> Result<(), DomainStateError> {
    let cutoff = (Utc::now() - chrono::Duration::days(PRUNE_DONE_DAYS))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    db.execute(
        "DELETE FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND doneAt IS NOT NULL AND doneAt < ?3",
        params![key.0, key.1, cutoff],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The day `at` (an ISO time) falls on here, or `None` when it does not parse.
fn local_day(at: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&Local).date_naive())
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A GitHub PR's number from its URL.
fn pull_request_number(url: &str) -> Option<u64> {
    let (_, rest) = url.split_once("/pull/")?;
    rest.split(['/', '?', '#']).next()?.parse().ok()
}

/// What the board says about a PR: its number, state, checks and review, from a thread row's
/// `pullRequest` or the cached read of an item's link.
fn pr_tag(pr: &Value) -> Option<Value> {
    let number = pr.get("number").and_then(Value::as_u64)?;
    let mut tag = json!({
        "number": number,
        "state": text(pr, "state"),
    });
    for key in ["checks", "review", "url"] {
        let value = text(pr, key);
        if !value.is_empty() {
            tag[key] = json!(value);
        }
    }
    Some(tag)
}

fn item_pr(link: &str) -> Option<Value> {
    let number = pull_request_number(link)?;
    let info = crate::work_mode::cached_work_pull_request("", link);
    let mut pr = json!({ "number": number, "url": link });
    if let Some(info) = info {
        pr["state"] = json!(info.state.as_wire());
        if let Some(checks) = info.checks {
            pr["checks"] = json!(checks);
        }
        if let Some(review) = info.review {
            pr["review"] = json!(review);
        }
    }
    pr_tag(&pr)
}

/// Where an open PR puts a thread that is not working or waiting: failing checks block it, running
/// checks keep it moving, and green checks or a review to give put it with the user.
fn pr_group(pr: &Value) -> Option<(&'static str, String)> {
    let state = text(pr, "state");
    if !matches!(state.as_str(), "open" | "draft" | "") {
        return None;
    }
    let number = pr["number"].as_u64()?;
    Some(
        match (text(pr, "checks").as_str(), text(pr, "review").as_str()) {
            ("failing", _) => ("blocked", format!("CI failing on PR #{number}")),
            ("pending", _) => ("moving", format!("CI running on PR #{number}")),
            (_, "changesRequested") => ("blocked", format!("Changes asked on PR #{number}")),
            _ if state == "draft" => return None,
            (_, "approved") => ("needs", format!("PR #{number} approved, ready to merge")),
            _ => ("needs", format!("PR #{number} waiting on your review")),
        },
    )
}

/// A thread's board row from its Workers row (panel.rs), or `None` when it is not on the board.
fn thread_row(
    row: &Value,
    pin: Option<&BoardItem>,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Option<Value> {
    let state = text(row, "state");
    let resolved_at = text(row, "resolvedAt");
    let active_at = text(row, "activeAt");
    let pr = row.get("pullRequest").and_then(pr_tag);
    let mut detail = text(row, "detail");
    let (group, reason, at) = match state.as_str() {
        "done" => {
            if local_day(&resolved_at)? != today {
                return None;
            }
            ("done", String::new(), resolved_at.clone())
        }
        // What the thread is doing now wins over any pin.
        "working" => ("moving", String::new(), active_at.clone()),
        "waiting" => ("needs", String::new(), active_at.clone()),
        _ => {
            if let Some(pin) = pin.filter(|pin| pin.group != "done") {
                (
                    pin.group.as_str(),
                    pin.reason.clone(),
                    pin.updated_at.clone(),
                )
            } else if let Some((group, words)) = pr.as_ref().and_then(pr_group) {
                detail = words;
                (group, String::new(), active_at.clone())
            } else {
                let idle = DateTime::parse_from_rfc3339(&active_at)
                    .is_ok_and(|at| now.signed_duration_since(at).num_days() >= IDLE_THREAD_DAYS);
                if idle {
                    return None;
                }
                ("later", "parked".to_string(), active_at.clone())
            }
        }
    };
    if let Some(note) = pin
        .map(|pin| pin.note.trim())
        .filter(|note| !note.is_empty())
    {
        detail = note.to_string();
    }
    let mut out = json!({
        "key": format!("thread:{}:{}:{}", text(row, "serverId"), text(row, "projectId"), text(row, "sessionId")),
        "kind": "thread",
        "ref": text(row, "sessionId"),
        "group": group,
        "title": text(row, "title"),
        "detail": detail,
        "at": at,
        "projectId": row["projectId"],
        "sessionId": row["sessionId"],
    });
    for key in ["serverId", "computer"] {
        let value = text(row, key);
        if !value.is_empty() {
            out[key] = json!(value);
        }
    }
    if !reason.is_empty() {
        out["reason"] = json!(reason);
    }
    if let Some(pin) = pin {
        out["pinned"] = json!(true);
        if !pin.release.is_empty() {
            out["release"] = json!(pin.release);
        }
    }
    if row["untested"] == true || pin.is_some_and(|pin| pin.untested) {
        out["untested"] = json!(true);
    }
    if row["needsApproval"] == true {
        out["needsApproval"] = json!(true);
    }
    if let Some(pr) = pr {
        out["pr"] = pr;
    }
    Some(out)
}

/// An item's board row, or `None` once its Done day has passed.
fn item_row(item: &BoardItem, today: NaiveDate) -> Option<Value> {
    if item.group == "dropped" {
        return None;
    }
    let (group, at) = match item.done_at.as_deref() {
        Some(done_at) => {
            if local_day(done_at)? != today {
                return None;
            }
            ("done", done_at.to_string())
        }
        None => (item.group.as_str(), item.updated_at.clone()),
    };
    let mut out = json!({
        "key": format!("item:{}", item.number),
        "kind": "item",
        "ref": format!("B{}", item.number),
        "group": group,
        "title": item.title,
        "detail": item.note,
        "at": at,
    });
    if !item.reason.is_empty() && group == "later" {
        out["reason"] = json!(item.reason);
    }
    if !item.release.is_empty() {
        out["release"] = json!(item.release);
    }
    if !item.link.is_empty() {
        out["link"] = json!(item.link);
        if let Some(pr) = item_pr(&item.link) {
            out["pr"] = pr;
        }
    }
    if item.untested {
        out["untested"] = json!(true);
    }
    Some(out)
}

/// Later's reasons in the order they are listed: up next, held, parked, anything else.
fn reason_rank(row: &Value) -> u8 {
    match row["reason"].as_str() {
        Some("next") => 0,
        Some("held") => 1,
        Some("parked") => 2,
        _ => 3,
    }
}

/// The board: every group in order with its total and its rows (newest first; Later by reason),
/// at most `cap` rows each when given. `threads` are the Workers rows, resolved ones included.
pub fn board_value(
    threads: &[Value],
    items: &[BoardItem],
    cap: Option<usize>,
    now: DateTime<Utc>,
) -> Value {
    let today = now.with_timezone(&Local).date_naive();
    let mut rows: Vec<Value> = threads
        .iter()
        .filter_map(|row| {
            let pin = items.iter().find(|item| {
                item.pins(
                    &text(row, "serverId"),
                    &text(row, "projectId"),
                    &text(row, "sessionId"),
                )
            });
            thread_row(row, pin, today, now)
        })
        .collect();
    rows.extend(
        items
            .iter()
            .filter(|item| !item.is_pin())
            .filter_map(|item| item_row(item, today)),
    );
    let groups: Vec<Value> = BOARD_GROUPS
        .iter()
        .map(|group| {
            let mut listed: Vec<&Value> =
                rows.iter().filter(|row| row["group"] == *group).collect();
            listed.sort_by(|left, right| {
                let by_reason = if *group == "later" {
                    reason_rank(left).cmp(&reason_rank(right))
                } else {
                    std::cmp::Ordering::Equal
                };
                by_reason.then_with(|| text(right, "at").cmp(&text(left, "at")))
            });
            let total = listed.len();
            let shown: Vec<Value> = listed
                .into_iter()
                .take(cap.unwrap_or(usize::MAX))
                .cloned()
                .collect();
            json!({ "id": group, "total": total, "rows": shown })
        })
        .collect();
    json!({ "groups": groups })
}

fn param_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn bad(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message.into())
}

/// The item a `B7` (or `7`) names; only items have positive numbers.
fn item_number(params: &Map<String, Value>) -> Option<i64> {
    let item = param_text(params, "item")?;
    item.trim_start_matches(['B', 'b'])
        .parse()
        .ok()
        .filter(|number: &i64| *number > 0)
}

/// The thread a request names: its server (empty for this computer), project and session.
fn thread_target(params: &Map<String, Value>) -> Option<(String, String, String)> {
    Some((
        param_text(params, "threadServerId").unwrap_or_default(),
        param_text(params, "threadProjectId")?,
        param_text(params, "threadSessionId")?,
    ))
}

/// Applies the fields a request sets (`title`, `note`, `link`, `release`, `untested`, and the
/// group) to `item`.
fn apply_fields(item: &mut BoardItem, params: &Map<String, Value>) -> Result<(), DomainStateError> {
    if let Some(group) = param_text(params, "group") {
        let (group, reason) = parse_group(&group).ok_or_else(|| {
            bad(format!(
                "Unknown group {group}. Use later, parked, next, held, needs-you, blocked, moving, done or ideas."
            ))
        })?;
        item.group = group.to_string();
        if !reason.is_empty() || group != "later" {
            item.reason = reason.to_string();
        }
        item.done_at = (group == "done").then(now_iso);
    }
    if let Some(reason) = param_text(params, "reason") {
        item.reason = match parse_group(&reason) {
            Some(("later", reason)) if !reason.is_empty() => reason.to_string(),
            _ => return Err(bad("A reason is parked, next or held.")),
        };
    }
    for (key, field) in [
        ("title", &mut item.title),
        ("note", &mut item.note),
        ("link", &mut item.link),
        ("release", &mut item.release),
    ] {
        if let Some(value) = params.get(key).and_then(Value::as_str) {
            *field = value.trim().to_string();
        }
    }
    if let Some(untested) = params.get("untested").and_then(Value::as_bool) {
        item.untested = untested;
    }
    if item.release.is_empty() {
        return Ok(());
    }
    // A release says the item is held for it.
    if item.group == "later" && item.reason.is_empty() {
        item.reason = "held".to_string();
    }
    Ok(())
}

/// `/api/coordinatorBoard`: `op` is `read` (the whole board, every row), `add`, `move`, `note`,
/// `done` or `drop`. Returns the answer and whether the board changed.
pub fn handle_board_request(
    db: &Connection,
    key: &SessionKey,
    threads: &[Value],
    params: &Map<String, Value>,
) -> Result<(Value, bool), DomainStateError> {
    let op = param_text(params, "op").unwrap_or_else(|| "read".to_string());
    let items = list_board_items(db, key)?;
    let now = now_iso();
    let pin_of = |target: &(String, String, String)| {
        items
            .iter()
            .find(|item| item.pins(&target.0, &target.1, &target.2))
            .cloned()
    };
    let answer = |item: &BoardItem| {
        json!({
            "item": if item.is_pin() { item.thread_session_id.clone() } else { format!("B{}", item.number) },
            "group": if item.done_at.is_some() { "done" } else { item.group.as_str() },
            "reason": item.reason,
        })
    };
    match op.as_str() {
        "read" => {
            let cap =
                (params.get("all").and_then(Value::as_bool) != Some(true)).then_some(SUMMARY_ROWS);
            Ok((board_value(threads, &items, cap, Utc::now()), false))
        }
        "add" => {
            let title = param_text(params, "title").ok_or_else(|| bad("An item needs a title."))?;
            let mut item = BoardItem {
                number: next_number(db, key, false)?,
                title,
                group: "later".to_string(),
                created_at: now.clone(),
                updated_at: now.clone(),
                ..BoardItem::default()
            };
            apply_fields(&mut item, params)?;
            write_item(db, key, &item)?;
            prune_done(db, key)?;
            Ok((answer(&item), true))
        }
        "move" | "note" | "done" => {
            let mut item = if let Some(number) = item_number(params) {
                read_item(db, key, number)?
                    .filter(|item| !item.is_pin() && item.group != "dropped")
                    .ok_or_else(|| bad(format!("This board has no item B{number}.")))?
            } else if let Some(target) = thread_target(params) {
                // A thread's first move makes its pin.
                pin_of(&target).unwrap_or_else(|| BoardItem {
                    number: 0,
                    group: "later".to_string(),
                    thread_server_id: target.0,
                    thread_project_id: target.1,
                    thread_session_id: target.2,
                    created_at: now.clone(),
                    ..BoardItem::default()
                })
            } else {
                return Err(bad("Name an item (B7) or a thread."));
            };
            if item.number == 0 {
                item.number = next_number(db, key, true)?;
            }
            let mut fields = params.clone();
            match op.as_str() {
                "move" if param_text(params, "group").is_none() => {
                    return Err(bad("Say which group to move it to."));
                }
                "note" => {
                    fields.remove("group");
                }
                "done" => {
                    fields.insert("group".to_string(), json!("done"));
                }
                _ => {}
            }
            apply_fields(&mut item, &fields)?;
            if op == "move" && item.group != "done" {
                item.done_at = None;
            }
            item.updated_at = now;
            // A thread's Done is its resolve (the caller resolves it); its pin goes.
            if item.is_pin() && item.done_at.is_some() {
                db.execute(
                    "DELETE FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber = ?3",
                    params![key.0, key.1, item.number],
                )
                .map_err(sql_error)?;
            } else {
                write_item(db, key, &item)?;
            }
            prune_done(db, key)?;
            Ok((answer(&item), true))
        }
        "drop" => {
            let removed = if let Some(number) = item_number(params) {
                // A dropped item stays as a tombstone, pruned with old Done rows, so its number is
                // not handed out again meanwhile.
                db.execute(
                    "UPDATE coordinator_board_items SET boardGroup = 'dropped', doneAt = ?4, updatedAt = ?4 WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber = ?3 AND boardGroup != 'dropped'",
                    params![key.0, key.1, number, now],
                )
                .map_err(sql_error)?
            } else if let Some(pin) = thread_target(params).and_then(|target| pin_of(&target)) {
                // Dropping a thread's pin hands it back to its own state.
                db.execute(
                    "DELETE FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber = ?3",
                    params![key.0, key.1, pin.number],
                )
                .map_err(sql_error)?
            } else {
                0
            };
            if removed == 0 {
                return Err(bad("Nothing on this board by that name."));
            }
            Ok((json!({ "dropped": true }), true))
        }
        other => Err(bad(format!(
            "Unknown board request {other}. Use read, add, move, note, done or drop."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0055_coordinator_board.sql"
        ))
        .unwrap();
        db
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn ago(hours: i64) -> String {
        (now() - chrono::Duration::hours(hours))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    fn thread(id: &str, state: &str, active_hours_ago: i64) -> Value {
        json!({"projectId": "P1", "sessionId": id, "title": id, "state": state, "detail": "d", "activeAt": ago(active_hours_ago)})
    }

    fn group_of<'a>(board: &'a Value, id: &str) -> &'a Value {
        board["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["id"] == id)
            .unwrap()
    }

    fn refs(board: &Value, id: &str) -> Vec<String> {
        group_of(board, id)["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| text(row, "ref"))
            .collect()
    }

    /// Threads land by their own state; an idle unpinned one leaves the board; an old Done leaves
    /// it; the groups come in the board's order with Later first.
    #[test]
    fn threads_sort_themselves_and_the_board_stays_light() {
        let mut resolved_today = thread("Gdone", "done", 1);
        resolved_today["resolvedAt"] = json!(now().to_rfc3339());
        let mut resolved_long_ago = thread("Gold", "done", 200);
        resolved_long_ago["resolvedAt"] = json!(ago(24 * 9));
        let threads = vec![
            thread("Gwork", "working", 0),
            thread("Gask", "waiting", 0),
            thread("Gsleep", "sleeping", 5),
            thread("Gidle", "sleeping", 24 * 5),
            resolved_today,
            resolved_long_ago,
        ];
        let board = board_value(&threads, &[], None, now());
        let ids: Vec<_> = board["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| text(group, "id"))
            .collect();
        assert_eq!(ids, BOARD_GROUPS);
        assert_eq!(refs(&board, "moving"), vec!["Gwork"]);
        assert_eq!(refs(&board, "needs"), vec!["Gask"]);
        assert_eq!(refs(&board, "later"), vec!["Gsleep"]);
        assert_eq!(group_of(&board, "later")["rows"][0]["reason"], "parked");
        assert_eq!(refs(&board, "done"), vec!["Gdone"]);
    }

    /// A finished thread's open PR places it: failing checks block it, running checks move it,
    /// green checks put it with the user.
    #[test]
    fn a_threads_pr_places_it() {
        let with_pr = |id: &str, checks: &str| {
            let mut row = thread(id, "finished", 1);
            row["pullRequest"] = json!({"number": 12, "state": "open", "checks": checks});
            row
        };
        let board = board_value(
            &[
                with_pr("Gred", "failing"),
                with_pr("Gamber", "pending"),
                with_pr("Ggreen", "passing"),
            ],
            &[],
            None,
            now(),
        );
        assert_eq!(refs(&board, "blocked"), vec!["Gred"]);
        assert_eq!(
            group_of(&board, "blocked")["rows"][0]["detail"],
            "CI failing on PR #12"
        );
        assert_eq!(refs(&board, "moving"), vec!["Gamber"]);
        assert_eq!(refs(&board, "needs"), vec!["Ggreen"]);
    }

    /// The coordinator's items and pins: an item lands where it was put, a release holds it, a
    /// pin moves a sleeping thread but not a working one, Done drops a pin, and `drop` removes it.
    #[test]
    fn items_and_pins_through_the_endpoint() {
        let db = db();
        let key = ("P1".to_string(), "Gcoord".to_string());
        let request = |params: Value| {
            handle_board_request(&db, &key, &[], params.as_object().unwrap()).unwrap()
        };
        let (added, changed) =
            request(json!({"op": "add", "title": "Help text", "release": "full"}));
        assert!(changed);
        assert_eq!(added["item"], "B1");
        assert_eq!(added["group"], "later");
        assert_eq!(added["reason"], "held");
        request(json!({"op": "add", "title": "Muse support", "group": "ideas"}));
        let (moved, _) = request(
            json!({"op": "move", "threadProjectId": "P1", "threadSessionId": "Gsleep", "group": "blocked", "note": "VOID_TOKEN expired"}),
        );
        assert_eq!(moved["item"], "Gsleep");
        request(
            json!({"op": "move", "threadProjectId": "P1", "threadSessionId": "Gwork", "group": "blocked"}),
        );

        let items = list_board_items(&db, &key).unwrap();
        let threads = vec![
            thread("Gsleep", "sleeping", 2),
            thread("Gwork", "working", 0),
        ];
        let board = board_value(&threads, &items, None, now());
        assert_eq!(refs(&board, "later"), vec!["B1"]);
        assert_eq!(refs(&board, "ideas"), vec!["B2"]);
        assert_eq!(refs(&board, "blocked"), vec!["Gsleep"]);
        assert_eq!(
            group_of(&board, "blocked")["rows"][0]["detail"],
            "VOID_TOKEN expired"
        );
        assert_eq!(
            refs(&board, "moving"),
            vec!["Gwork"],
            "a working thread ignores its pin"
        );

        request(json!({"op": "done", "item": "B1"}));
        request(json!({"op": "done", "threadProjectId": "P1", "threadSessionId": "Gsleep"}));
        request(json!({"op": "drop", "item": "B2"}));
        let items = list_board_items(&db, &key).unwrap();
        let board = board_value(&threads, &items, None, now());
        assert_eq!(refs(&board, "done"), vec!["B1"]);
        assert_eq!(refs(&board, "ideas"), Vec::<String>::new());
        assert_eq!(
            refs(&board, "later"),
            vec!["Gsleep"],
            "its pin went with its Done"
        );
        // Numbers are never reused, and pins never take one.
        let (next, _) = request(json!({"op": "add", "title": "Next"}));
        assert_eq!(next["item"], "B3");
        assert!(handle_board_request(&db, &key, &[], json!({"op": "drop", "item": "B4"}).as_object().unwrap()).is_err());
    }

    /// The chat's copy carries at most `SUMMARY_ROWS` per group, with the true total.
    #[test]
    fn the_chats_copy_is_capped() {
        let threads: Vec<Value> = (0..20)
            .map(|index| thread(&format!("G{index}"), "working", 0))
            .collect();
        let board = board_value(&threads, &[], Some(SUMMARY_ROWS), now());
        let moving = group_of(&board, "moving");
        assert_eq!(moving["total"], 20);
        assert_eq!(moving["rows"].as_array().unwrap().len(), SUMMARY_ROWS);
    }
}
