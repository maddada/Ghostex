//! A coordinator's board: its lines of work grouped by who has the ball, for the chat's Board pill
//! and `ghostex coordinator board`.
//!
//! Five groups and the Backlog (stored as `ideas`), in the order every client shows them: Backlog
//! (folded; the user, later: "rename Ideas to Backlog in the board please"), Later (nobody is on
//! it yet: parked, up next or held for a release), Needs you, Blocked, In progress, Done (today only).
//! The user, of the first build: "please move ideas to the top above later". Threads
//! land in a group from their own state and their PR's checks and review; the coordinator only
//! places what is not a thread (an item, `B7`) and moves a thread whose state cannot tell where it
//! stands (a pin: Blocked on a CI token, Needs you for a test only the user can do).
//!
//! The board stays light: only open work and today's Done. A finished thread with nothing expected
//! of it shows under Done as idle on the day it last moved, one that still owes work leaves after
//! `IDLE_THREAD_DAYS` unless the coordinator pinned it, Done items drop off after the day they
//! were done, and the chat's copy carries at most `SUMMARY_ROWS` rows per group (the CLI reads
//! them all).
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User, after the design pages (issues-1027/coordinator-board/board-explained.html, Q1243 "Yes, build it"): groups say who has the ball and tags say what a row is; five groups plus Ideas; "shift Later to be most left one"; and "keep this board light please", read as only current lines of work, old Done dropping off, a capped number of rows per group with "N more", nothing the user cleans up by hand. Rows sort themselves from thread, question and PR/CI state; the coordinator adds what is not a thread with `ghostex coordinator board add/move/note/done/drop`. The chat's questions join Needs you in the chat core (packages/gx-chat-core/src/extras/coordinator_board.rs), which also caps, folds and counts.
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User (Q2096 "replace"): work items replace the board (work.rs). The board's own rows moved into
//! work items once (migration 0074) and nothing writes `coordinator_board_items` any more. This
//! module draws the board from the work items for clients from before the Work panel (11.2.1
//! phones talking to a newer gxserver keep their Board pill and its actions), and turns
//! `/api/coordinatorBoard` and `ghostex coordinator board` into item changes, so running
//! coordinators that still use the board's verbs keep working.
//! SEE-ALSO: server/src/coordinators/work.rs (the items), server/src/coordinators/panel.rs (the thread rows and the cache this is built beside), server/src/ghostex_cli/coordinator/board.rs (the CLI alias), server/src/storage/migrations/0055_coordinator_board.sql (the old items).

use chrono::{DateTime, Local, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use super::records::{now_iso, sql_error, SessionKey};
use super::work::{
    delete_work_item, derive, handle_work_request, item_number_of, list_work_items, parse_answers,
    parse_status, write_work_item, WorkAsk, WorkContext, WorkEvent, WorkItem, WorkThread,
};
use crate::domain::DomainStateError;

/// The groups, in the order every client shows them. `later` is the group the clients call "To do",
/// `hold` is On hold.
///
/// CDXC:Coordinators 2026-11-06 DECISION:
/// User, of a card under Blocked that waited for a certificate their bot was signing up for:
/// "please modify the coordinator playbook so this is clear. blocked means requires immediate
/// attention by the user to unblock it, it doesn't mean waiting on something. waiting on something
/// can be a new one called On hold for example". Blocked is only what needs the user now to unblock
/// it (a stuck thread only they can heal, an expired token, a release step only they can fix); work
/// waiting on something outside (a sign-up, a release, another thread, a vendor, someone else's CI)
/// is On hold, after Blocked and before In progress, in a calm colour and with no pill of its own. A
/// finished thread's failing checks or requested changes are To do (work for the coordinator, not
/// the user), and a thread still not moving after Ghostex's nudge is Blocked. Cards already under
/// Blocked stay there.
///
/// CDXC:Coordinators 2026-11-07 DECISION:
/// User, with a screenshot of the board's side list showing "Moving 3": "rename to in progress
/// please". The group stored as `moving` is shown as In progress everywhere the user reads it (the
/// board's side list and Columns overlay, the Board pill's tooltip, the phone's sheet,
/// `ghostex coordinator board`, the help and the playbook). The key stays `moving` so stored
/// boards keep working, and `in-progress` is accepted by `parse_group` besides `moving`. The
/// Unified sidebar's Moving shelf is a different thing and keeps its name.
pub const BOARD_GROUPS: [&str; 8] = [
    "ideas", "later", "needs", "blocked", "hold", "moving", "done", "removed",
];

/// The rows per group the chat's copy carries; the CLI reads every row.
pub const SUMMARY_ROWS: usize = 12;

/// An unpinned thread that has not moved for this long leaves the board (it stays in Workers).
const IDLE_THREAD_DAYS: i64 = 3;

/// How long a removed row stays under Removed, where it can be restored, before it is deleted.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User: "allow also clicking x twice on any item to remove it from the list to a bottom area called
/// "Removed" so it can be restored from there", and "of course we dont keep things forever there tho".
/// A week covers coming back after a weekend to undo a removal, and it is longer than
/// `IDLE_THREAD_DAYS`, so a removed thread that stays idle is already off the board by the time its
/// removal is deleted and never reappears by itself. Removing only hides: a thread is not closed or
/// resolved, and one that works or waits on the user again after it was removed comes back.
const REMOVED_KEEP_DAYS: i64 = 7;

/// Still listed under Removed: removed less than `REMOVED_KEEP_DAYS` ago.
fn removed_recently(removed_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(removed_at)
        .is_ok_and(|at| now.signed_duration_since(at).num_days() < REMOVED_KEEP_DAYS)
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

/// The GitHub PRs and issues work items link to (`item add --link`), by when an item last showed
/// one, so the links pass reads their state as it reads a thread's links
/// (server/session_links_sync.rs `refresh_link_caches`). A link no item has shown for an hour is
/// forgotten.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User, of the Work panel's chips "#536" and "#538" grey beside a green "#526": why. A thread's
/// link (`ghostex link add`) was read for its state; an item's own link never was, so it stayed
/// grey (an issue) or would show green whatever its state (a PR). A GitHub PR or issue chip looks
/// the same wherever its link came from: its real state's colour once read, neutral until then.
fn item_github_links_seen(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>> {
    static SEEN: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
    > = std::sync::OnceLock::new();
    SEEN.get_or_init(Default::default)
}

/// Notes that an item shows `link`, when it is a GitHub PR or issue.
pub(crate) fn note_item_github_link(link: &str) {
    if crate::work_mode::repo_and_number_of_url(link).is_none() {
        return;
    }
    if let Ok(mut seen) = item_github_links_seen().lock() {
        seen.insert(link.to_string(), std::time::Instant::now());
    }
}

/// The GitHub PR and issue links items showed in the last hour.
pub(crate) fn item_github_links() -> Vec<String> {
    let Ok(mut seen) = item_github_links_seen().lock() else {
        return Vec::new();
    };
    seen.retain(|_, at| at.elapsed() < std::time::Duration::from_secs(3600));
    seen.keys().cloned().collect()
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

pub(crate) fn item_pr(link: &str) -> Option<Value> {
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

/// Where an open PR puts a thread that is not working or waiting: failing checks or requested
/// changes are To do (work for the coordinator, not the user: the 2026-11-06 meaning of Blocked on
/// `BOARD_GROUPS`), running checks keep it moving, and green checks or a review to give put it with
/// the user.
fn pr_group(pr: &Value) -> Option<(&'static str, String)> {
    let state = text(pr, "state");
    if !matches!(state.as_str(), "open" | "draft" | "") {
        return None;
    }
    let number = pr["number"].as_u64()?;
    Some(
        match (text(pr, "checks").as_str(), text(pr, "review").as_str()) {
            ("failing", _) => ("later", format!("CI failing on PR #{number}")),
            ("pending", _) => ("moving", format!("CI running on PR #{number}")),
            (_, "changesRequested") => ("later", format!("Changes asked on PR #{number}")),
            _ if state == "draft" => return None,
            (_, "approved") => ("needs", format!("PR #{number} approved, ready to merge")),
            _ => ("needs", format!("PR #{number} waiting on your review")),
        },
    )
}

/// What a thread that ended its turn still owes (`unfinished` on its Workers row, idle_wait.rs),
/// as the chip its card and the CLI show, and the chip's tint.
fn unfinished_flag(unfinished: &str, state: &str) -> Option<(&'static str, &'static str)> {
    Some(match unfinished {
        "jobRunning" => ("Waiting on its background job", "running"),
        "waitsOnJob" | "queued" => ("Job ended, result not read", "bad"),
        "carriesOn" => ("Stopped mid-task", "bad"),
        "cutOff" if state == "sleeping" => ("Slept mid-task", "bad"),
        "cutOff" => ("Turn cut off", "bad"),
        _ => return None,
    })
}

/// A thread's board row from its Workers row (panel.rs), or `None` when it is not on the board.
///
/// CDXC:Coordinators 2026-11-06 DECISION:
/// User: "why is board not up to date?? what can we do to avoid this please?" (four threads under
/// To do as "parked": two running, one put to sleep mid-command, one idle for two hours with
/// nothing pending). A thread lands from what it is doing now, as the supervisor reads it
/// (live.rs): working, by its transcript too, is Moving with the line it is on; one whose turn
/// ended owing work (a background job's result, a task it stopped in the middle of, a turn cut off
/// by a sleep) is Moving with a chip saying what it owes and since when, and turns "Not moving"
/// once Ghostex's nudge did not move it; one that finished with nothing expected of it is quietly
/// under Done as "idle" for the day it last moved, then leaves the board (it stays in Workers).
/// "parked" is only ever the coordinator's own word, a pin (`board move <thread> parked`).
pub(crate) fn thread_row(
    row: &Value,
    pin: Option<&Pin>,
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
        "working" => (
            "moving",
            String::new(),
            Some(text(row, "turnStartedAt"))
                .filter(|at| !at.is_empty())
                .unwrap_or_else(|| active_at.clone()),
        ),
        "waiting" => ("needs", String::new(), active_at.clone()),
        _ => {
            let unfinished = text(row, "unfinished");
            if let Some(pin) = pin.filter(|pin| pin.moves_thread()) {
                (
                    pin.group.as_str(),
                    pin.reason.clone(),
                    pin.updated_at.clone(),
                )
            } else if unfinished_flag(&unfinished, &state).is_some() {
                let idle = DateTime::parse_from_rfc3339(&active_at)
                    .is_ok_and(|at| now.signed_duration_since(at).num_days() >= IDLE_THREAD_DAYS);
                if idle {
                    return None;
                }
                let since = text(row, "unfinishedSince");
                (
                    "moving",
                    String::new(),
                    if since.is_empty() {
                        active_at.clone()
                    } else {
                        since
                    },
                )
            } else if let Some((group, words)) = pr.as_ref().and_then(pr_group) {
                detail = words;
                (group, String::new(), active_at.clone())
            } else {
                // Finished with nothing expected of it: Done as idle for the day it last moved.
                if local_day(&active_at)? != today {
                    return None;
                }
                ("done", "idle".to_string(), active_at.clone())
            }
        }
    };
    // A thread still not moving after Ghostex's nudge (or whose terminal Ghostex could not
    // restart) waits on the user to heal it: Blocked, the 2026-11-06 meaning on `BOARD_GROUPS`.
    let group = if row["stuck"] == true && group == "moving" {
        "blocked"
    } else {
        group
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
    for key in ["serverId", "computer", "branch", "agent", "globalRef"] {
        let value = text(row, key);
        if !value.is_empty() {
            out[key] = json!(value);
        }
    }
    if let Some(links) = row.get("links").filter(|links| links.is_array()) {
        out["links"] = links.clone();
    }
    if !reason.is_empty() {
        out["reason"] = json!(reason);
    }
    if let Some((flag, tone)) = unfinished_flag(&text(row, "unfinished"), &state) {
        out["unfinished"] = row["unfinished"].clone();
        out["flag"] = json!(flag);
        out["flagTone"] = json!(tone);
    }
    if let Some(pin) = pin.filter(|pin| pin.moves_thread()) {
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
    // A thread not moving even after its nudge (working without output, or idle although its
    // background job waits): the board's red "Not moving" chip.
    if row["stuck"] == true {
        out["stuck"] = json!(true);
    }
    // What the user can do about a thread that waits on them.
    if state == "waiting" {
        out["waiting"] = json!(if row["needsApproval"] == true {
            "approval"
        } else {
            "question"
        });
    }
    // The coordinator's own line for a thread it moved, which its card shows when it has no links.
    if let Some(note) = pin
        .filter(|pin| pin.moves_thread())
        .map(|pin| pin.note.trim())
        .filter(|note| !note.is_empty())
    {
        out["note"] = json!(note);
    }
    if let Some(pr) = pr {
        out["pr"] = pr;
    }
    // The session's own tag, the one its sidebar row shows (panel.rs `build_row`).
    let tag = text(row, "sessionTag");
    if !tag.is_empty() {
        out["tag"] = json!(tag);
    }
    Some(out)
}
/// The sidebar's session tags as the board offers them (`tagOptions`, the menu's Tags list), and
/// each row's `tag` turned into its look (`tagLook`: label, glyph, colour), from this computer's
/// custom tag catalog. A row whose custom tag is gone from the catalog shows none.
pub fn add_tag_looks(board: &mut Value, custom_tags: &Value) {
    let custom: Option<ghostex_gx_protocol::CustomSessionTagsState> =
        serde_json::from_value(custom_tags.clone()).ok();
    for group in board["groups"].as_array_mut().into_iter().flatten() {
        for row in group["rows"].as_array_mut().into_iter().flatten() {
            let Some(tag) = row.get("tag").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            match ghostex_gx_core::tag_choice(&tag, custom.as_ref()) {
                Some(look) => row["tagLook"] = json!(look),
                None => {
                    if let Some(row) = row.as_object_mut() {
                        row.remove("tag");
                    }
                }
            }
        }
    }
    board["tagOptions"] = json!(ghostex_gx_core::tag_choices(custom.as_ref()));
}

/// An item's link that is not a PR: a GitHub issue, a Linear ticket, or any other URL or file,
/// named by what it points at.
pub(crate) fn item_link(link: &str) -> Value {
    if let Some((_, rest)) = link.split_once("/issues/") {
        let number = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let mut out = json!({ "kind": "issue", "label": format!("#{number}"), "url": link });
        // Its state once read, as a thread's issue link shows it (`note_item_github_link`).
        if let Some(info) = crate::work_mode::cached_work_github_issue_url(link) {
            if let Some(state) = info.state.as_deref() {
                out["state"] = json!(state);
            }
        }
        return out;
    }
    if link.contains("linear.app/") {
        let identifier = link
            .split("/issue/")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("Linear");
        return json!({ "kind": "linear", "label": identifier.to_ascii_uppercase(), "url": link });
    }
    let name = link
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(link);
    json!({ "kind": "link", "label": name, "url": link })
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

/// A thread's place on the board when an item exists only for it (`for_thread`, a board pin): the
/// group the coordinator put it in and its line.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pin {
    group: String,
    reason: String,
    note: String,
    release: String,
    untested: bool,
    updated_at: String,
}

impl Pin {
    /// A pin that moves its thread (one that only hides it has no group).
    fn moves_thread(&self) -> bool {
        !self.group.is_empty() && self.group != "done"
    }

    fn of(item: &WorkItem) -> Self {
        Self {
            // A removed thread's item only hides it.
            group: if item.dropped_at.is_some() {
                String::new()
            } else {
                board_group(&item.status, &item.reason).to_string()
            },
            reason: item.reason.clone(),
            note: item.now.clone(),
            release: item.release.clone(),
            untested: item.untested,
            updated_at: item.updated_at.clone(),
        }
    }
}

/// The board group an item's status is shown under.
fn board_group(status: &str, reason: &str) -> &'static str {
    match status {
        "needs" => "needs",
        "blocked" => "blocked",
        "moving" => "moving",
        "waiting" => "hold",
        "finished" => "done",
        "dropped" => "removed",
        _ if reason == "backlog" => "ideas",
        _ => "later",
    }
}

/// An item's board row, or `None` once its Finished day has passed or its removal is old.
fn item_row(
    item: &WorkItem,
    context: &WorkContext<'_>,
    threads: &[Value],
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Option<Value> {
    let (status, _) = derive(item, context);
    let group = board_group(status, &item.reason);
    let at = match group {
        "removed" => {
            let removed_at = item.dropped_at.clone().unwrap_or_default();
            if !removed_recently(&removed_at, now) {
                return None;
            }
            removed_at
        }
        "done" => {
            let done_at = item
                .finished_at
                .clone()
                .unwrap_or_else(|| item.updated_at.clone());
            if local_day(&done_at)? != today {
                return None;
            }
            done_at
        }
        _ => item.updated_at.clone(),
    };
    let mut out = json!({
        "key": format!("item:{}", item.number),
        "kind": "item",
        "ref": format!("B{}", item.number),
        "group": group,
        "title": item.title,
        "detail": item.now,
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
            out["links"] = json!([{
                "kind": "pr",
                "label": format!("#{}", pr["number"]),
                "state": pr["state"],
                "checks": pr.get("checks").cloned().unwrap_or(Value::Null),
                "url": item.link,
            }]);
            out["pr"] = pr;
        } else {
            out["links"] = json!([item_link(&item.link)]);
        }
    } else if let Some(row) = item.threads.iter().find_map(|thread| {
        threads.iter().find(|row| {
            row["projectId"] == thread.project_id.as_str()
                && row["sessionId"] == thread.session_id.as_str()
        })
    }) {
        // Its thread's links and PR, which its own row used to carry.
        if let Some(links) = row.get("links").filter(|links| links.is_array()) {
            out["links"] = links.clone();
        }
        if let Some(pr) = row.get("pullRequest").and_then(pr_tag) {
            out["pr"] = pr;
        }
    }
    if item.untested {
        out["untested"] = json!(true);
    }
    if !item.now.trim().is_empty() {
        out["note"] = json!(item.now.trim());
    }
    // The ask with one-click answers, the open one first.
    if let Some(ask) = item
        .asks
        .iter()
        .filter(|ask| !ask.answers.is_empty())
        .min_by_key(|ask| !ask.is_open())
    {
        out["answers"] = Value::Array(parse_answers(&ask.answers));
        if !ask.answered.is_empty() {
            out["answered"] = json!(ask.answered);
        }
    }
    if !item.tag.is_empty() {
        out["tag"] = json!(item.tag);
    }
    Some(out)
}

/// The board, drawn from the work items for clients from before the Work panel (11.2.1 phones and
/// desktops): every group in order with its total and its rows (newest first; Later by reason), at
/// most `cap` rows each when given. Items keep the B<number> those clients name them by (the same
/// number as W<number>); a thread on an item shows only through its item, an item for one thread
/// (a pin) shows as that thread's row, and a thread on no item shows by its own state. `threads`
/// are the Workers rows, resolved ones included.
pub fn board_value(
    threads: &[Value],
    items: &[WorkItem],
    cap: Option<usize>,
    now: DateTime<Utc>,
) -> Value {
    let today = now.with_timezone(&Local).date_naive();
    let context = WorkContext::new(threads, items);
    let mut rows: Vec<Value> = threads
        .iter()
        .filter_map(|row| {
            let (project, session) = (text(row, "projectId"), text(row, "sessionId"));
            let holder = items
                .iter()
                .find(|item| item.has_thread(&project, &session));
            let live = matches!(text(row, "state").as_str(), "working" | "waiting");
            let pin = match holder {
                Some(item) if item.for_thread && item.finished_at.is_none() => Some(Pin::of(item)),
                Some(item) if item.for_thread => None,
                // On a real item: its item stands for it, unless the item was dropped and the
                // thread works or waits again.
                Some(item) => {
                    let moved_since = item
                        .dropped_at
                        .as_deref()
                        .is_some_and(|at| text(row, "activeAt").as_str() > at);
                    if item.dropped_at.is_none() || !(live || moved_since) {
                        return None;
                    }
                    None
                }
                None => None,
            };
            let mut out = thread_row(row, pin.as_ref(), today, now)?;
            // Removed and not moved since: under Removed until restored. A thread that works or
            // waits again, or was active after it was removed, is back on the board.
            if let Some(removed_at) = holder
                .filter(|item| item.for_thread)
                .and_then(|item| item.dropped_at.as_deref())
            {
                let moved_since = text(row, "activeAt").as_str() > removed_at;
                if !live && !moved_since {
                    if !removed_recently(removed_at, now) {
                        return None;
                    }
                    out["group"] = json!("removed");
                    out["at"] = json!(removed_at);
                }
            }
            Some(out)
        })
        .collect();
    rows.extend(
        items
            .iter()
            .filter(|item| !item.for_thread)
            .filter_map(|item| item_row(item, &context, threads, today, now)),
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
    // Every item on the board by its ref, whatever the cap leaves out of its group, so a chat can
    // tell a `[title](board:B4)` mention of a card still on it from one that left it. The item's
    // W<number> names it too.
    //
    // CDXC:Coordinators 2026-11-08 DECISION:
    // User, of `[Deploy guest page 0.1.2 to share.ghostex.dev](board:B43)` in a message: "please
    // color this different so it's clear that it's a mention for a board item". A board mention is
    // its own tinted chip with the card's current title, dimmed when the card has left the board;
    // the capped groups cannot say that (a busy Done holds more than `SUMMARY_ROWS`), this can.
    let mut mentions = Map::new();
    for row in rows.iter().filter(|row| row["kind"] == "item") {
        let reference = text(row, "ref");
        let title = json!(text(row, "title"));
        mentions.insert(reference.replacen('B', "W", 1), title.clone());
        mentions.insert(reference, title);
    }
    json!({ "groups": groups, "items": mentions })
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

/// The thread a request names: its server (empty for this computer), project and session.
fn thread_target(params: &Map<String, Value>) -> Option<WorkThread> {
    Some(WorkThread {
        server_id: param_text(params, "threadServerId").unwrap_or_default(),
        project_id: param_text(params, "threadProjectId")?,
        session_id: param_text(params, "threadSessionId")?,
    })
}

/// The status and Later kind a board group word names (`needs-you`, `on-hold`, `parked`…).
fn group_status(word: &str) -> Result<(&'static str, &'static str), DomainStateError> {
    parse_status(word).ok_or_else(|| {
        bad(format!(
            "Unknown group {word}. Use todo (or later), parked, next, held, needs-you, blocked, on-hold, in-progress (or moving), done or backlog."
        ))
    })
}

/// `/api/coordinatorBoard`, kept for clients and coordinators from before work items: `op` is
/// `read` (the whole board, every row), `add`, `move`, `note`, `done`, `drop` (to Removed),
/// `restore`, `answer` (records a card's answer once) or `tag` (the user's tag on a card). Each
/// changes the work items (work.rs): an item is the work item with the same number, and a
/// thread's move or removal is an item for that thread. Returns the answer and whether anything
/// changed.
pub fn handle_board_request(
    db: &Connection,
    key: &SessionKey,
    threads: &[Value],
    params: &Map<String, Value>,
) -> Result<(Value, bool), DomainStateError> {
    let op = param_text(params, "op").unwrap_or_else(|| "read".to_string());
    if op == "read" {
        let items = list_work_items(db, key)?;
        let cap =
            (params.get("all").and_then(Value::as_bool) != Some(true)).then_some(SUMMARY_ROWS);
        return Ok((board_value(threads, &items, cap, Utc::now()), false));
    }
    let work = |fields: Value| -> Result<Value, DomainStateError> {
        let mut fields = fields.as_object().cloned().unwrap_or_default();
        fields.retain(|_, value| !value.is_null());
        handle_work_request(db, key, threads, &fields, 0, &Default::default())
            .map(|(answer, _)| answer)
    };
    // What the board's callers read back: the item (B7) or thread, its group and Later kind.
    let board_answer = |work: &Value| {
        let status = work["status"].as_str().unwrap_or_default();
        let reason = text(work, "reason");
        json!({
            "item": text(work, "ref").replacen('W', "B", 1),
            "group": board_group(status, &reason),
            "reason": reason,
            "work": work,
        })
    };
    let item = param_text(params, "item").and_then(|item| item_number_of(&item));
    if op == "tag" && item.is_none() {
        return tag_thread(db, params);
    }
    if op == "add" {
        let (status, reason) = match param_text(params, "group") {
            Some(word) => group_status(&word)?,
            None => ("later", ""),
        };
        let added = work(json!({
            "op": "add",
            "title": params.get("title"),
            "status": if reason.is_empty() { status.to_string() } else { reason.to_string() },
            "note": params.get("note"),
            "link": params.get("link"),
            "release": params.get("release"),
            "untested": params.get("untested"),
        }))?;
        // One-click answers make the item's ask, worded as its title.
        if let Some(answers) = param_text(params, "answers") {
            let number = added["number"].as_i64().unwrap_or_default();
            if let Some(mut item) = list_work_items(db, key)?
                .into_iter()
                .find(|item| item.number == number)
            {
                item.asks.push(WorkAsk {
                    text: item.title.clone(),
                    answers,
                    asked_at: now_iso(),
                    ..WorkAsk::default()
                });
                write_work_item(db, key, &item)?;
            }
        }
        return Ok((board_answer(&added), true));
    }
    if let Some(number) = item {
        let name = format!("W{number}");
        let answer = match op.as_str() {
            "move" => {
                let word = param_text(params, "group")
                    .ok_or_else(|| bad("Say which group to move it to."))?;
                let (status, reason) = group_status(&word)?;
                if status == "finished" {
                    work(json!({ "op": "done", "item": name, "text": params.get("note") }))?
                } else {
                    work(json!({
                        "op": "edit",
                        "item": name,
                        "status": if reason.is_empty() { status } else { reason },
                        "note": params.get("note"),
                    }))?
                }
            }
            "note" => work(json!({
                "op": "edit",
                "item": name,
                "note": params.get("note"),
                "untested": params.get("untested"),
                "title": params.get("title"),
                "link": params.get("link"),
                "release": params.get("release"),
            }))?,
            "done" => work(json!({ "op": "done", "item": name }))?,
            "drop" | "restore" | "tag" => work(json!({
                "op": op,
                "item": name,
                "tag": params.get("tag"),
            }))?,
            // A work ask snoozed or cancelled from its card (work.rs).
            "snooze" | "cancel" | "reply" => {
                let changed = work(json!({
                    "op": op,
                    "item": name,
                    "ask": params.get("ask"),
                    "hours": params.get("hours"),
                    "answer": params.get("answer"),
                }))?;
                return Ok((json!({ "item": format!("B{number}"), "work": changed }), true));
            }
            "answer" => {
                let answered = work(json!({
                    "op": "answer",
                    "item": name,
                    "answer": params.get("answer"),
                    "ask": params.get("ask"),
                }))?;
                return Ok((
                    json!({ "item": format!("B{number}"), "answered": params.get("answer"), "work": answered }),
                    true,
                ));
            }
            other => {
                return Err(bad(format!(
                    "Unknown board request {other}. Use read, add, move, note, done, drop, restore or tag."
                )))
            }
        };
        let mut out = board_answer(&answer);
        if op == "drop" {
            out["group"] = json!("removed");
        }
        if op == "tag" {
            out["tag"] = json!(param_text(params, "tag").unwrap_or_default());
        }
        return Ok((out, true));
    }
    let target = thread_target(params).ok_or_else(|| bad("Name an item (B7) or a thread."))?;
    let items = list_work_items(db, key)?;
    let holder = items
        .iter()
        .find(|item| item.has_thread(&target.project_id, &target.session_id))
        .cloned();
    let now = now_iso();
    let thread_answer = |group: &str, reason: &str| json!({ "item": target.session_id, "group": group, "reason": reason });
    match op.as_str() {
        "move" | "note" => {
            let word = param_text(params, "group");
            if op == "move" && word.is_none() {
                return Err(bad("Say which group to move it to."));
            }
            // `move <thread> auto` hands a thread back to its own state.
            if word.as_deref().is_some_and(|word| {
                matches!(word.to_ascii_lowercase().as_str(), "auto" | "own" | "unpin")
            }) {
                if let Some(item) = holder.filter(|item| item.for_thread) {
                    delete_work_item(db, key, item.number)?;
                }
                return Ok((thread_answer("", ""), true));
            }
            let mut item = match holder {
                Some(item) => item,
                None => new_thread_item(db, key, threads, &target, &now)?,
            };
            if let Some(word) = word {
                let (status, reason) = group_status(&word)?;
                if status == "finished" {
                    item.finished_at = Some(now.clone());
                }
                item.status = status.to_string();
                item.reason = reason.to_string();
            }
            if let Some(note) = params.get("note").and_then(Value::as_str) {
                item.now = note.trim().to_string();
            }
            if let Some(untested) = params.get("untested").and_then(Value::as_bool) {
                item.untested = untested;
            }
            item.dropped_at = None;
            let heading = super::work::status_heading(&item.status).to_string();
            item.updated_at = now.clone();
            item.history.push(WorkEvent {
                at: now.clone(),
                kind: "status".to_string(),
                text: heading,
                status: item.status.clone(),
            });
            write_work_item(db, key, &item)?;
            Ok((
                thread_answer(board_group(&item.status, &item.reason), &item.reason),
                true,
            ))
        }
        // A thread's Done is its resolve (the caller resolves it); its own item goes.
        "done" => {
            if let Some(item) = holder.filter(|item| item.for_thread) {
                delete_work_item(db, key, item.number)?;
            }
            Ok((thread_answer("done", ""), true))
        }
        "drop" => {
            let mut item = match holder {
                Some(item) => item,
                None => new_thread_item(db, key, threads, &target, &now)?,
            };
            if item.dropped_at.is_some() {
                return Err(bad("That is already under Removed."));
            }
            item.dropped_at = Some(now.clone());
            item.updated_at = now;
            write_work_item(db, key, &item)?;
            Ok((thread_answer("removed", ""), true))
        }
        "restore" => {
            let item = holder
                .filter(|item| item.dropped_at.is_some())
                .ok_or_else(|| bad("That thread is not under Removed."))?;
            if item.for_thread {
                // Its item only placed or hid it: the thread is back on its own state.
                delete_work_item(db, key, item.number)?;
                return Ok((thread_answer("", ""), true));
            }
            let restored = work(json!({ "op": "restore", "item": item.reference() }))?;
            Ok((board_answer(&restored), true))
        }
        other => Err(bad(format!(
            "Unknown board request {other}. Use read, add, move, note, done, drop, restore or tag."
        ))),
    }
}

/// An item that exists only for one thread (a board pin), titled as the thread is.
fn new_thread_item(
    db: &Connection,
    key: &SessionKey,
    threads: &[Value],
    target: &WorkThread,
    now: &str,
) -> Result<WorkItem, DomainStateError> {
    let title = threads
        .iter()
        .find(|row| {
            row["projectId"] == target.project_id.as_str()
                && row["sessionId"] == target.session_id.as_str()
        })
        .map(|row| text(row, "title"))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| target.session_id.clone());
    Ok(WorkItem {
        number: super::work::next_item_number(db, key)?,
        title,
        status: "later".to_string(),
        for_thread: true,
        threads: vec![target.clone()],
        created_at: now.to_string(),
        updated_at: now.to_string(),
        ..WorkItem::default()
    })
}

/// The user's tag on a thread's card: its session's tag, where its sidebar row shows it too.
fn tag_thread(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<(Value, bool), DomainStateError> {
    let tag = param_text(params, "tag").unwrap_or_default();
    if !tag.is_empty()
        && ghostex_gx_core::tag_choice(&tag, None).is_none()
        && !crate::custom_session_tags::is_custom_session_tag_id(&tag)
    {
        return Err(bad(format!("Unknown tag {tag}.")));
    }
    let target = thread_target(params).ok_or_else(|| bad("Name an item (B7) or a thread."))?;
    if !target.server_id.is_empty() {
        return Err(bad(
            "Tag a thread on another computer from its own sidebar.",
        ));
    }
    let changed = db
        .execute(
            "UPDATE sessions SET sessionTag = ?3, isFavorite = ?4, updatedAt = ?5 WHERE projectId = ?1 AND sessionId = ?2",
            params![
                target.project_id,
                target.session_id,
                (!tag.is_empty()).then_some(tag.as_str()),
                i64::from(tag == "favorite"),
                now_iso(),
            ],
        )
        .map_err(sql_error)?;
    if changed == 0 {
        return Err(bad("That thread's session is gone."));
    }
    Ok((json!({ "item": target.session_id, "tag": tag }), true))
}
#[cfg(test)]
mod tests {
    use super::super::work::{has_question_items, settle_answered_items};
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0055_coordinator_board.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0056_coordinator_board_removed.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0057_coordinator_board_answers.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0058_coordinator_board_answered.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0060_coordinator_board_tag.sql"
        ))
        .unwrap();
        // What the move into work items reads titles from.
        db.execute_batch(
            "CREATE TABLE sessions (projectId TEXT, sessionId TEXT, title TEXT);
             CREATE TABLE coordinator_remote_threads (serverId TEXT, projectId TEXT, sessionId TEXT, title TEXT);",
        )
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0074_coordinator_work_items.sql"
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

    /// Threads land by their own state; a finished one nothing is expected of is never To do (Done
    /// as idle on the day it moved, else off the board); an old Done leaves it; the groups come in
    /// the board's order with Later first.
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
        assert_eq!(refs(&board, "later"), Vec::<String>::new());
        let done = refs(&board, "done");
        assert!(done.contains(&"Gdone".to_string()));
        assert!(!done.contains(&"Gidle".to_string()) && !done.contains(&"Gold".to_string()));
    }

    /// A finished thread's open PR places it: failing checks are To do (work for the coordinator,
    /// not Blocked), running checks move it, green checks put it with the user; a stuck thread is
    /// Blocked; an item goes On hold.
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
        assert_eq!(refs(&board, "later"), vec!["Gred"]);
        assert_eq!(
            group_of(&board, "later")["rows"][0]["detail"],
            "CI failing on PR #12"
        );
        assert_eq!(refs(&board, "moving"), vec!["Gamber"]);
        assert_eq!(refs(&board, "needs"), vec!["Ggreen"]);
        let mut stuck = thread("Gstuck", "working", 0);
        stuck["stuck"] = json!(true);
        let board = board_value(&[stuck], &[], None, now());
        assert_eq!(refs(&board, "blocked"), vec!["Gstuck"]);
        assert_eq!(parse_status("on-hold"), Some(("waiting", "")));
        assert_eq!(parse_status("on hold"), Some(("waiting", "")));
        let ids: Vec<&str> = board["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["id"].as_str().unwrap())
            .collect();
        assert_eq!(&ids[3..6], &["blocked", "hold", "moving"]);
    }

    /// The coordinator's items and pins, now work items: an item lands where it was put, a release
    /// holds it, a pin (an item for one thread) moves a sleeping thread but not a working one, Done
    /// drops a pin, and `drop` removes it.
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
        request(json!({"op": "add", "title": "Muse support", "group": "backlog"}));
        assert_eq!(
            parse_status("ideas"),
            Some(("later", "backlog")),
            "the old word still works"
        );
        assert_eq!(parse_status("todo"), Some(("later", "")));
        let (moved, _) = request(
            json!({"op": "move", "threadProjectId": "P1", "threadSessionId": "Gsleep", "group": "blocked", "note": "VOID_TOKEN expired"}),
        );
        assert_eq!(moved["item"], "Gsleep");
        request(
            json!({"op": "move", "threadProjectId": "P1", "threadSessionId": "Gwork", "group": "blocked"}),
        );

        let items = list_work_items(&db, &key).unwrap();
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
        let items = list_work_items(&db, &key).unwrap();
        let board = board_value(&threads, &items, None, now());
        assert!(refs(&board, "done").contains(&"B1".to_string()));
        assert_eq!(refs(&board, "ideas"), Vec::<String>::new());
        assert_eq!(
            refs(&board, "removed"),
            vec!["B2"],
            "a dropped item waits under Removed"
        );
        assert!(
            !refs(&board, "later").contains(&"Gsleep".to_string()),
            "its pin went with its Done"
        );
        // Numbers are never reused; a pin is an item now and takes one.
        let (next, _) = request(json!({"op": "add", "title": "Next"}));
        assert_eq!(next["item"], "B5");
        assert!(handle_board_request(
            &db,
            &key,
            &[],
            json!({"op": "drop", "item": "B9"}).as_object().unwrap()
        )
        .is_err());
    }

    /// A card's answer is recorded once: a second answer, the same or another, is refused, and the
    /// board carries the one given.
    #[test]
    fn an_answer_is_recorded_once() {
        let db = db();
        let key = ("P1".to_string(), "Gcoord".to_string());
        let request =
            |params: Value| handle_board_request(&db, &key, &[], params.as_object().unwrap());
        request(json!({"op": "add", "title": "Delete the branch?", "group": "needs-you", "answers": "Yes*,No"})).unwrap();
        assert!(
            request(json!({"op": "answer", "item": "B1", "answer": "Maybe"})).is_err(),
            "not offered"
        );
        request(json!({"op": "answer", "item": "B1", "answer": "Yes"})).unwrap();
        assert!(
            request(json!({"op": "answer", "item": "B1", "answer": "Yes"})).is_err(),
            "twice"
        );
        assert!(
            request(json!({"op": "answer", "item": "B1", "answer": "No"})).is_err(),
            "changed"
        );
        let board = board_value(&[], &list_work_items(&db, &key).unwrap(), None, now());
        // Answered, it leaves Needs you for Done and shows its answer there.
        assert_eq!(group_of(&board, "needs")["total"], 0);
        assert_eq!(group_of(&board, "done")["rows"][0]["answered"], "Yes");
    }

    /// A Needs you item that names a question's tag leaves for Done once the coordinator's chat
    /// settles that question (answered by a one-click link, a form, a page or in plain words, or
    /// withdrawn); a task for the user that names no question stays until it is done.
    #[test]
    fn an_item_naming_a_question_leaves_needs_you_once_it_is_answered() {
        let db = db();
        let key = ("P1".to_string(), "Gcoord".to_string());
        let request =
            |params: Value| handle_board_request(&db, &key, &[], params.as_object().unwrap());
        request(json!({"op": "add", "title": "Q12: Reformat My Passport to a journaled filesystem?", "group": "needs-you"})).unwrap();
        request(json!({"op": "add", "title": "Collect 20 real audios", "group": "needs-you", "note": "Answer Q13 and Q14 in chat"})).unwrap();
        request(json!({"op": "add", "title": "Export your 7 custom preset prompts", "group": "needs-you", "note": "Copy each prompt from its gear icon"})).unwrap();
        request(json!({"op": "add", "title": "Q99 names a tag never asked", "group": "needs-you"}))
            .unwrap();
        let needs = |db: &Connection| {
            let board = board_value(&[], &list_work_items(db, &key).unwrap(), None, now());
            group_of(&board, "needs")["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["ref"].as_str().unwrap().to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        };
        assert!(has_question_items(&db, &key));
        // Q13 answered, Q14 still waiting: B2 waits for both.
        let state = |tag: &str| match tag {
            "Q12" | "Q13" => Some(true),
            "Q14" => Some(false),
            _ => None,
        };
        assert!(settle_answered_items(&db, &key, state).unwrap());
        assert_eq!(needs(&db), vec!["B2", "B3", "B4"]);
        assert!(
            !settle_answered_items(&db, &key, state).unwrap(),
            "nothing new"
        );
        let done = board_value(&[], &list_work_items(&db, &key).unwrap(), None, now());
        assert_eq!(group_of(&done, "done")["rows"][0]["ref"], "B1");
        assert!(settle_answered_items(&db, &key, |tag| (tag != "Q99").then_some(true)).unwrap());
        // The user's task and the item whose tag was never asked stay until done.
        assert_eq!(needs(&db), vec!["B3", "B4"]);
        assert!(has_question_items(&db, &key), "B4 still names Q99");
        request(json!({"op": "done", "item": "B3"})).unwrap();
        assert_eq!(needs(&db), vec!["B4"]);
    }

    /// The two-click × and Restore: a removed item or thread waits under Removed, Restore puts it
    /// back where it was, and a thread that works again comes back by itself.
    #[test]
    fn removed_rows_wait_and_restore() {
        let db = db();
        let key = ("P1".to_string(), "Gcoord".to_string());
        let request =
            |params: Value| handle_board_request(&db, &key, &[], params.as_object().unwrap());
        request(json!({"op": "add", "title": "Help text", "release": "full"})).unwrap();
        request(json!({"op": "drop", "item": "B1"})).unwrap();
        request(json!({"op": "drop", "threadProjectId": "P1", "threadSessionId": "Gsleep"}))
            .unwrap();
        request(json!({"op": "drop", "threadProjectId": "P1", "threadSessionId": "Gwork"}))
            .unwrap();
        assert!(
            request(json!({"op": "drop", "item": "B1"})).is_err(),
            "already removed"
        );
        // Idle since earlier today (two hours ago, or local midnight if sooner), before it was
        // removed: an idle thread from yesterday is off the board, so a fixed two hours failed
        // between midnight and 2 am.
        let since_midnight = i64::from(chrono::Timelike::num_seconds_from_midnight(
            &Local::now().time(),
        ));
        let mut sleeping = thread("Gsleep", "sleeping", 0);
        sleeping["activeAt"] = json!(
            (now() - chrono::Duration::seconds(since_midnight.min(7200)))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        );
        let threads = vec![sleeping, thread("Gwork", "working", 0)];
        let board = board_value(&threads, &list_work_items(&db, &key).unwrap(), None, now());
        assert_eq!(refs(&board, "removed"), vec!["Gsleep", "B1"]);
        assert_eq!(
            refs(&board, "moving"),
            vec!["Gwork"],
            "working again: back on the board"
        );
        assert_eq!(refs(&board, "later"), Vec::<String>::new());

        request(json!({"op": "restore", "item": "B1"})).unwrap();
        request(json!({"op": "restore", "threadProjectId": "P1", "threadSessionId": "Gsleep"}))
            .unwrap();
        let board = board_value(&threads, &list_work_items(&db, &key).unwrap(), None, now());
        assert_eq!(refs(&board, "removed"), Vec::<String>::new());
        assert_eq!(refs(&board, "later"), vec!["B1"]);
        assert_eq!(group_of(&board, "later")["rows"][0]["reason"], "held");
        // Restoring a thread that only hid it leaves no item behind.
        assert!(list_work_items(&db, &key)
            .unwrap()
            .iter()
            .all(|item| !item.has_thread("P1", "Gsleep")));
    }

    /// An item keeps the user's tag; a thread's card shows its session's tag; the board offers the
    /// sidebar's tag list and resolves each row's tag to its look.
    #[test]
    fn cards_carry_the_sidebars_tags() {
        let db = db();
        let key = ("P1".to_string(), "Gcoord".to_string());
        let request =
            |params: Value| handle_board_request(&db, &key, &[], params.as_object().unwrap());
        request(json!({"op": "add", "title": "Help text"})).unwrap();
        request(json!({"op": "tag", "item": "B1", "tag": "bug"})).unwrap();
        assert!(request(json!({"op": "tag", "item": "B1", "tag": "nonsense"})).is_err());
        let mut tagged = thread("Gtag", "working", 0);
        tagged["sessionTag"] = json!("research");
        let mut board = board_value(&[tagged], &list_work_items(&db, &key).unwrap(), None, now());
        add_tag_looks(&mut board, &Value::Null);
        let later = &group_of(&board, "later")["rows"][0];
        assert_eq!(later["tag"], "bug");
        assert_eq!(later["tagLook"]["label"], "Bug");
        assert_eq!(later["tagLook"]["icon"], "bug");
        let moving = &group_of(&board, "moving")["rows"][0];
        assert_eq!(moving["tagLook"]["label"], "Research");
        assert_eq!(board["tagOptions"][0]["value"], "favorite");
        request(json!({"op": "tag", "item": "B1", "tag": ""})).unwrap();
        assert_eq!(list_work_items(&db, &key).unwrap()[0].tag, "");
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
