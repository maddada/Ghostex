//! The `coordinatorThreads` chat field: a coordinator's threads as its chat's Threads panel reads
//! them.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! Claude puts an Overview of the threads beside the conversation; Ghostex puts a Threads panel above the coordinator's composer, next to the Subagents and Tasks panels, so it also reaches the web build and the phone (which has no sidebar tree). The supervisor refreshes this cache every tick and republishes the coordinator's chat state when it changed; the frame builders only read the cache, because they run under the stream's emit lock and must not touch the database. Absent on a frame means unchanged (like `appCommands`), so a builder that leaves it out can never blank the panel.
//! SEE-ALSO: server/src/server/coordinator_runtime.rs (refresh and republish), server/src/session_chat_follower/frames.rs and session_chat_read.rs (carry it), packages/gx-chat-core/src/extras/coordinator_threads.rs (the panel).
//!
//! CDXC:Coordinators 2026-10-07 WHY:
//! A frame carries only a summary: the threads in flight (working, or waiting on an answer or the user's approval) and the `recent` ones (active in the last two hours, which "Show recent" adds; the user's 2026-10-11 decision in the core), the total and a `revision` of the full list. With ~95 threads the whole list was ~30KB on every chat state frame. The full list, resolved threads included, is read once when the user opens the panel's "N more" row (`/api/readCoordinatorThreads`), and read again when the revision moves while it stays open, so every thread stays reachable. The two-hour boundary is decided here on every tick, so a thread ageing out changes the summary and republishes it.
//! SEE-ALSO: packages/gx-chat-core/src/extras/coordinator_threads.rs (the panel and the read), packages/gx-core/src/sidebar_view/threads.rs (`RECENT_THREAD_MS`, the sidebar's same two hours).

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::brief::report_headline;
use super::endpoint::session_title_of;
use super::live::{thread_live, ThreadLive};
use super::records::{list_coordinators, SessionKey, ThreadRecord};
use super::remote_threads::{list_remote_threads, RemoteThreadRecord};
use super::state::{
    classify_thread_session, thread_needs_user_approval, waiting_prompt, ThreadProgress,
    ThreadState,
};
use crate::domain::DomainRepository;
use crate::presentation::{effective_lifecycle_state, now_iso};

/// How long after its last activity a thread that neither works nor waits is still listed.
const RECENT_THREAD_MS: i64 = 2 * 60 * 60 * 1000;

fn is_recent(active_at: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    chrono::DateTime::parse_from_rfc3339(active_at)
        .is_ok_and(|at| now.signed_duration_since(at).num_milliseconds() < RECENT_THREAD_MS)
}

/// One coordinator's cached panel: what frames carry, and what "N more" reads.
#[derive(PartialEq)]
struct CachedPanel {
    summary: Value,
    /// `chat_frame_dedupe::digest` of `summary`, worked out once per change.
    summary_digest: u64,
    /// Every thread, in the panel's order.
    threads: Vec<Value>,
}

/// Every thread row of one coordinator's panel as last built, resolved ones included, for the
/// board's full read (board.rs).
pub fn cached_thread_rows(key: &SessionKey) -> Vec<Value> {
    panels()
        .lock()
        .ok()
        .and_then(|panels| panels.get(key).map(|panel| panel.threads.clone()))
        .unwrap_or_default()
}

fn panels() -> &'static Mutex<HashMap<SessionKey, CachedPanel>> {
    static PANELS: OnceLock<Mutex<HashMap<SessionKey, CachedPanel>>> = OnceLock::new();
    PANELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Working first, then waiting on the user (an answer or an approval), then the rest; each newest
/// first. The first two are the panel's in-flight rows, always in the summary.
fn panel_rank(row: &Value) -> u8 {
    match (row["state"].as_str(), row["needsApproval"] == true) {
        (Some("working"), _) => 0,
        (Some("waiting"), _) | (_, true) => 1,
        _ => 2,
    }
}

fn cached_panel(
    mut threads: Vec<Value>,
    board: Value,
    work: Value,
    approvals: Value,
    intro_hidden: bool,
    previous: Option<&CachedPanel>,
) -> CachedPanel {
    threads.sort_by(|left, right| {
        panel_rank(left).cmp(&panel_rank(right)).then_with(|| {
            right["activeAt"]
                .as_str()
                .unwrap_or_default()
                .cmp(left["activeAt"].as_str().unwrap_or_default())
        })
    });
    // The summary is a function of the sorted list alone, so an unchanged list keeps it without
    // serialising every thread again for the revision.
    if let Some(previous) = previous.filter(|previous| {
        previous.threads == threads
            && previous.summary["board"] == board
            && previous.summary["work"] == work
            && previous.summary["approvals"] == approvals
            && previous.summary["introHidden"] == intro_hidden
    }) {
        return CachedPanel {
            summary: previous.summary.clone(),
            summary_digest: previous.summary_digest,
            threads,
        };
    }
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(&threads)
        .unwrap_or_default()
        .hash(&mut hasher);
    let summary = json!({
        "threads": threads
            .iter()
            .filter(|row| panel_rank(row) < 2 || row["recent"] == true)
            .collect::<Vec<_>>(),
        "total": threads.len(),
        "revision": format!("{:016x}", hasher.finish()),
        // The coordinator's board, capped per group (board.rs); it rides on this field so every
        // client that carries the Workers list carries it too.
        "board": board,
        // The coordinator's work items (work.rs): what the Work panel lists and the chat's item
        // cards are drawn from. `board` stays for clients from before them.
        "work": work,
        // The thread approval cards its chat relays, open and recently ended (relay.rs).
        "approvals": approvals,
        // "Don't show again" on the coordinator tips, for every coordinator (intro.rs).
        "introHidden": intro_hidden,
    });
    let summary_digest = crate::server::chat_frame_dedupe::digest(&summary);
    CachedPanel {
        summary,
        summary_digest,
        threads,
    }
}

/// The dedupe digest of a coordinator's thread list as a chat frame carries it, when it is the
/// panel's current summary (server/chat_frame_dedupe.rs `field_digest`). `None` while a refresh
/// holds the panels, so an event never waits on a rebuild.
pub(crate) fn coordinator_threads_digest(key: &SessionKey, value: &Value) -> Option<u64> {
    let panels = panels().try_lock().ok()?;
    let panel = panels.get(key)?;
    (panel.summary == *value).then_some(panel.summary_digest)
}

/// `/api/readCoordinatorThreads`: every thread of the coordinator, for the panel's "N more".
pub fn read_coordinator_threads(key: &SessionKey) -> Value {
    let panels = panels().lock().ok();
    match panels.as_ref().and_then(|panels| panels.get(key)) {
        Some(panel) => json!({
            "threads": panel.threads,
            "total": panel.threads.len(),
            "revision": panel.summary["revision"],
        }),
        None => json!({ "threads": [], "total": 0, "revision": "" }),
    }
}

/// Adds `coordinatorThreads` to a chat frame or read result of a coordinator session.
pub fn insert_coordinator_threads(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    let value = panels().lock().ok().and_then(|panels| {
        panels
            .get(&(project_id.to_string(), session_id.to_string()))
            .map(|panel| panel.summary.clone())
    });
    if let Some(value) = value {
        frame.insert("coordinatorThreads".to_string(), value);
    }
}

/// One thread's panel row, kept while nothing it was built from has changed.
struct CachedRow {
    /// Shared with the kept threads (thread_reads.rs), so its text is in memory once.
    thread: std::sync::Arc<ThreadRecord>,
    /// A hash of the session row's stored columns, `None` when the row is gone.
    session: Option<u64>,
    row: Value,
    /// When the row's `recent` mark runs out, while it has one.
    recent_until: Option<chrono::DateTime<chrono::Utc>>,
    /// The supervisor's view of the thread it was built with (live.rs).
    live: Option<ThreadLive>,
    /// When a resolved thread's row next reads its session row again (`RESOLVED_RECHECK`).
    recheck_at: std::time::Instant,
    /// Changes exactly when `row` does (`row_generation`).
    generation: u64,
}

/// CDXC:Coordinators 2026-11-08 WHY:
/// The supervisor still read and hashed every thread's session row every two seconds to learn a
/// cached row was unchanged: on the profiled Mac about 900 of the 1,000 threads were resolved, and
/// that read was a third of the panels' cost (gxserver idled at 20% of a core). A resolved
/// thread's row is in the board's done list and the "N more" read, and its session changes only
/// when someone renames, retags or reopens it, so it reads its session row again every 30 to 40
/// seconds (spread over the threads) instead of every tick. A change to its thread record (a
/// reopen), its live state or its `recent` mark still rebuilds it on the next tick.
const RESOLVED_RECHECK: std::time::Duration = std::time::Duration::from_secs(30);

fn resolved_recheck_at(key: &SessionKey) -> std::time::Instant {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    std::time::Instant::now()
        + RESOLVED_RECHECK
        + std::time::Duration::from_millis(hasher.finish() % 10_000)
}

/// What one coordinator's panel was last built from.
struct PanelInputs {
    /// Each local thread's row, by key and generation, in the panel's input order.
    row_ids: Vec<(SessionKey, u64)>,
    /// The threads on other computers, built fresh every tick (`build_remote_row`).
    remote_rows: Vec<Value>,
    items: Vec<super::work::WorkItem>,
    custom_tags: Value,
    intro_hidden: bool,
    /// The coordinator's idle floor (turns.rs): a new turn moves items in or out of In progress.
    idle_floor: chrono::DateTime<chrono::Utc>,
    built_at: std::time::Instant,
}

/// CDXC:Coordinators 2026-11-08 WHY:
/// Every tick sorted, summarised and rebuilt the board of every coordinator's panel, though
/// between two ticks nearly every panel is built from exactly the same rows. A panel whose rows,
/// board items, tag catalog and intro choice equal what it was last built from is kept as it is.
/// The board's groups move with the day and the relayed approval cards end after two hours
/// (board.rs, relay.rs), so a kept panel is still rebuilt at least every minute.
///
/// CDXC:Coordinators 2026-11-12 WHY:
/// A panel with any thread on another computer used to be rebuilt every tick for its rows' clock-dependent `recent` mark. On the profiled Mac that was the coordinator holding 1,142 of the 1,258 threads and 350 work items (two remote threads, both resolved), so its board, work list and panel were rebuilt every two seconds: about half of gxserver's idle CPU. The remote rows are built every tick and compared like the local ones, so a mark that runs out still rebuilds the panel on that tick. The local rows are compared by key and generation (`row_generation`) instead of being cloned and compared in full every tick.
const PANEL_REBUILD_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

fn panel_inputs() -> &'static Mutex<HashMap<SessionKey, PanelInputs>> {
    static INPUTS: OnceLock<Mutex<HashMap<SessionKey, PanelInputs>>> = OnceLock::new();
    INPUTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The last row built for each thread whose row is not cached (an open running thread, or one
/// whose session row could not be read), with its generation, so an unchanged rebuild keeps it.
fn fresh_rows() -> &'static Mutex<HashMap<SessionKey, (u64, Value)>> {
    static FRESH: OnceLock<Mutex<HashMap<SessionKey, (u64, Value)>>> = OnceLock::new();
    FRESH.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The generation of a row just built: the previous one's when the row is unchanged, a new one
/// otherwise. A panel is kept when its rows' keys and generations are what it was built from.
fn row_generation(previous: Option<(u64, &Value)>, row: &Value) -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    match previous {
        Some((generation, previous)) if previous == row => generation,
        _ => NEXT.fetch_add(1, Ordering::Relaxed),
    }
}

fn rows() -> &'static Mutex<HashMap<SessionKey, CachedRow>> {
    static ROWS: OnceLock<Mutex<HashMap<SessionKey, CachedRow>>> = OnceLock::new();
    ROWS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The thread's session row as stored: a hash of its columns, and the row itself when `needs_row`
/// says the hash does not match what is cached.
fn read_session_row(
    statement: &mut rusqlite::CachedStatement<'_>,
    key: &SessionKey,
    needs_row: impl Fn(u64) -> bool,
) -> rusqlite::Result<Option<(u64, Option<crate::domain::SessionRow>)>> {
    statement
        .query_row([&key.0, &key.1], |row| {
            let mut hasher = DefaultHasher::new();
            for index in 0..row.as_ref().column_count() {
                match row.get_ref(index)? {
                    ValueRef::Null => 0_u8.hash(&mut hasher),
                    ValueRef::Integer(value) => (1_u8, value).hash(&mut hasher),
                    ValueRef::Real(value) => (2_u8, value.to_bits()).hash(&mut hasher),
                    ValueRef::Text(bytes) => (3_u8, bytes).hash(&mut hasher),
                    ValueRef::Blob(bytes) => (4_u8, bytes).hash(&mut hasher),
                }
            }
            let hash = hasher.finish();
            let stored = if needs_row(hash) {
                Some(crate::domain::session_row_from_sql(row)?)
            } else {
                None
            };
            Ok((hash, stored))
        })
        .optional()
}

/// CDXC:Coordinators 2026-10-23 WHY:
/// Every two seconds the supervisor rebuilt every thread's row (about 300 on a long-lived install), reading and parsing each session's JSON columns and working out its title, although nearly all of them are resolved or closed threads that cannot change by themselves. A row that depends on neither the clock nor the supervisor's in-memory screen readings (a resolved thread, a session that is gone or not running, with no rename waiting on its agent) is now kept until its thread record or any stored column of its session row changes, or its two-hour `recent` mark runs out. Rows of open running threads are rebuilt every tick as before, so nothing shows later than it did.
fn row_is_stable(thread: &ThreadRecord, session: Option<&Value>) -> bool {
    let Some(session) = session else {
        return true;
    };
    // `project_session_title` shows a waiting rename for eight seconds from its request.
    if crate::presentation::util::read_runtime_text(session, "pendingAgentTitleRequestStatus")
        .as_deref()
        == Some("pending")
    {
        return false;
    }
    thread.is_resolved() || effective_lifecycle_state(session) != "running"
}

/// A working thread's line, from its brief: the brief's opening heading when it has one (a brief's
/// heading names the work, unlike a report's), else its first sentence after the repo line.
fn task_line(task: &str) -> String {
    let first = task.lines().map(str::trim).find(|line| !line.is_empty());
    if let Some(heading) = first.and_then(|line| line.strip_prefix('#')) {
        let heading = heading.trim_start_matches('#').trim();
        if !heading.is_empty() {
            return report_headline(heading, 140);
        }
    }
    report_headline(task_without_repo_line(task), 140)
}

/// A brief without its opening "Repo: <path> (branch)." line, which says where, not what: a
/// working thread's line in Workers and on the board names the work instead.
fn task_without_repo_line(task: &str) -> &str {
    let trimmed = task.trim_start();
    if !trimmed.starts_with("Repo:") {
        return task;
    }
    // The line ends at its first sentence (". ") or its newline, whichever comes first.
    let end = [
        trimmed.find(". ").map(|at| at + 2),
        trimmed.find('\n').map(|at| at + 1),
    ]
    .into_iter()
    .flatten()
    .min();
    match end.map(|at| &trimmed[at..]) {
        Some(rest) if !rest.trim().is_empty() => rest,
        _ => task,
    }
}

/// The most links a board card shows on its second line.
const BOARD_CARD_LINKS: usize = 3;

/// The work a thread is linked to, for its board card's second line: its open PRs, then its open
/// issues, then its open Linear tickets, as session links last read them, each with what opens it.
/// Merged, closed and finished links and links a person removed are left out, and only the first
/// few are kept.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User, of the first board: "no need to show first line or the name of the thread, no one cares
/// about that. please make this more focused on the branch/pr/issue/linear so i can click on those
/// if i wanna see". A card's second line is these links (session links, the same the sidebar's link
/// chips read), each opening its page.
fn work_links_of(session: &Value) -> Vec<Value> {
    use crate::session_links::{SessionLinkKind, SessionLinkRef};
    let links = crate::session_links::stored_session_links(session);
    let mut out: Vec<(u8, Value)> = links
        .items
        .iter()
        .filter(|item| !links.is_removed(&item.link.key()))
        .filter_map(|item| {
            let read = links
                .read
                .get(&item.link.key())
                .cloned()
                .unwrap_or(Value::Null);
            let state = read["state"].as_str().unwrap_or_default().to_string();
            // Merged, closed or finished work is not where the thread is now.
            if read["finished"] == true || matches!(state.as_str(), "merged" | "closed") {
                return None;
            }
            let url = read["url"]
                .as_str()
                .map(str::to_string)
                .or_else(|| item.url.clone())
                .or_else(|| item.link.github_url());
            let (kind, label, rank) = match &item.link {
                SessionLinkRef::Github {
                    kind: SessionLinkKind::PullRequest,
                    number,
                    ..
                } => (
                    "pr",
                    format!("#{number}"),
                    if matches!(state.as_str(), "open" | "draft" | "") {
                        0
                    } else {
                        3
                    },
                ),
                SessionLinkRef::Github { number, .. } => ("issue", format!("#{number}"), 1),
                SessionLinkRef::Linear { identifier } => ("linear", identifier.clone(), 2),
                SessionLinkRef::Command { .. } => return None,
            };
            let mut link = json!({ "kind": kind, "label": label, "state": state });
            for field in ["checks", "title", "stateName"] {
                if let Some(value) = read.get(field).and_then(Value::as_str) {
                    link[field] = json!(value);
                }
            }
            if let Some(url) = url {
                link["url"] = json!(url);
            }
            Some((rank, link))
        })
        .collect();
    out.sort_by_key(|(rank, _)| *rank);
    out.into_iter()
        .take(BOARD_CARD_LINKS)
        .map(|(_, link)| link)
        .collect()
}

/// How recently a PR linked by someone else (a person, a mention) counts as the thread's work.
const BOARD_PR_RECENT_HOURS: i64 = 24;

/// The PR the board shows for a thread: an open (or draft) PR it is working on, as last read
/// (session links), the newest linked first. Its own count whenever they were linked: one it
/// opened or reported, or its branch's. One someone else linked counts only for a day after it
/// was linked. A merged or closed PR is never current work, so it shows no tag.
///
/// CDXC:Coordinators 2026-11-05 WHY:
/// The Incoming Issues Manager's row carried "PR #249 merged", a link from long ago that says
/// nothing about what it does now. The coordinator asked to count only open PRs linked recently or
/// by the thread itself.
fn pull_request_of(session: &Value) -> Option<Value> {
    use crate::session_links::{SessionLinkKind, SessionLinkRef, SessionLinkSource};
    let links = crate::session_links::stored_session_links(session);
    let now = chrono::Utc::now();
    let recent = |added_at: Option<&str>| {
        added_at
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .is_some_and(|at| now.signed_duration_since(at).num_hours() < BOARD_PR_RECENT_HOURS)
    };
    links
        .items
        .iter()
        .filter_map(|item| {
            let SessionLinkRef::Github {
                kind: SessionLinkKind::PullRequest,
                number,
                ..
            } = &item.link
            else {
                return None;
            };
            let key = item.link.key();
            if links.is_removed(&key) {
                return None;
            }
            let own = matches!(
                item.source,
                SessionLinkSource::Branch
                    | SessionLinkSource::AgentOpened
                    | SessionLinkSource::AgentReported
            );
            if !own && !recent(item.added_at.as_deref()) {
                return None;
            }
            let read = links.read.get(&key).cloned().unwrap_or(Value::Null);
            let state = read["state"].as_str().unwrap_or("open").to_string();
            if !matches!(state.as_str(), "open" | "draft") {
                return None;
            }
            let rank = 0;
            let mut pr = json!({ "number": number, "state": state });
            for field in ["checks", "review", "url"] {
                if let Some(value) = read.get(field).and_then(Value::as_str) {
                    pr[field] = json!(value);
                }
            }
            if pr.get("url").is_none() {
                if let Some(url) = item.link.github_url() {
                    pr["url"] = json!(url);
                }
            }
            Some((rank, item.added_at.clone().unwrap_or_default(), pr))
        })
        .min_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)))
        .map(|(_, _, pr)| pr)
}

fn build_row(
    server_id: &str,
    thread: &ThreadRecord,
    session: Option<&Value>,
    live: Option<&ThreadLive>,
    generated_at: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Value {
    let state = classify_thread_session(session, ThreadProgress::of(thread), generated_at, false);
    // The supervisor's transcript reading wins over hooks that say it stopped (live.rs).
    let state = if state == ThreadState::Finished && live.is_some_and(|live| live.working) {
        ThreadState::Working
    } else {
        state
    };
    let last_report = thread
        .last_report
        .as_deref()
        .filter(|report| !report.trim().is_empty())
        .map(|report| report_headline(report, 140));
    let detail = match state {
        ThreadState::Waiting => session
            .and_then(waiting_prompt)
            .map(|prompt| report_headline(&prompt.summary, 140))
            .unwrap_or_else(|| "Waiting for an answer".to_string()),
        // What it says in the turn it is on beats its last report, and that beats the brief it
        // started from: a long-lived thread's brief is about its first task, its last report about
        // its latest one, and neither about what it is doing now.
        ThreadState::Working => live
            .and_then(|live| live.activity.clone())
            .or(last_report)
            .unwrap_or_else(|| task_line(&thread.task)),
        // A thread with no report yet says what it was given.
        _ => last_report.unwrap_or_else(|| task_line(&thread.task)),
    };
    // The session's own activity clock, as the sidebar's relative time reads it; a thread whose
    // session is gone falls back to its record's last change.
    let active_at = session
        .and_then(|session| session.get("lastActiveAt"))
        .and_then(Value::as_str)
        .unwrap_or(thread.updated_at.as_str())
        .to_string();
    let mut row = json!({
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "title": session.map(session_title_of).unwrap_or_else(|| report_headline(&thread.task, 60)),
        "state": state.as_str(),
        "detail": detail,
        "activeAt": active_at,
        // What a reference to the thread in the coordinator's chat names (the + mention's form).
        "globalRef": crate::ids::create_global_session_ref(server_id, &thread.project_id, &thread.session_id),
    });
    if is_recent(&active_at, now) {
        row["recent"] = json!(true);
    }
    // Its tag as its sidebar row shows it (a legacy favorite reads as the `favorite` tag), for its
    // board card (board.rs).
    if let Some(session) = session {
        let tag = session
            .get("sessionTag")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !tag.is_empty() {
            row["sessionTag"] = json!(tag);
        } else if session.get("isFavorite").and_then(Value::as_bool) == Some(true) {
            row["sessionTag"] = json!("favorite");
        }
    }
    // A working thread that has gone quiet says so where its detail shows (the Threads panel and
    // the board's In progress row), and a stuck one carries the board's red "Not moving" chip; see
    // coordinators/stall.rs.
    if let Some(detail) =
        crate::session_self_heal::attention_detail(&thread.project_id, &thread.session_id)
    {
        // Its terminal ended and Ghostex could not restart it (session_self_heal.rs).
        row["detail"] = json!(detail);
        row["stuck"] = json!(true);
    } else if state == ThreadState::Working {
        if let Some(quiet) = super::stall::session_quiet(&thread.project_id, &thread.session_id)
            .filter(|quiet| quiet.stalled)
        {
            row["detail"] = json!(super::stall::quiet_detail(&quiet, now.timestamp_millis()));
            row["stalled"] = json!(true);
            if quiet.stuck {
                row["stuck"] = json!(true);
            }
            if let Some(since) = super::stall::iso_from_ms(quiet.quiet_since) {
                row["quietSince"] = json!(since);
            }
        }
    } else if super::stall::session_stuck_since(&thread.project_id, &thread.session_id).is_some() {
        // An idle thread its background job still waits for, even after its nudge (idle_wait.rs).
        row["stuck"] = json!(true);
    }
    if state == ThreadState::Waiting && session.is_some_and(thread_needs_user_approval) {
        row["needsApproval"] = json!(true);
        // The card the coordinator's chat relays; `track_approvals` takes it off the row.
        if let Some(card) = session.and_then(super::relay::relay_card) {
            row["relay"] = card;
        }
    }
    if let Some(resolved_at) = &thread.resolved_at {
        row["resolvedAt"] = json!(resolved_at);
    }
    // What an idle or sleeping thread still owes, and since when (idle_wait.rs): the board puts it
    // under In progress with that, not under To do as parked.
    if let Some((reason, since)) = live.and_then(|live| live.unfinished) {
        if matches!(state, ThreadState::Finished | ThreadState::Sleeping) {
            row["unfinished"] = json!(reason.as_str());
            if let Some(since) = super::stall::iso_from_ms(since) {
                row["unfinishedSince"] = json!(since);
            }
        }
    }
    if let Some(started) = live
        .and_then(|live| live.turn_started_ms)
        .filter(|_| state == ThreadState::Working)
        .and_then(super::stall::iso_from_ms)
    {
        row["turnStartedAt"] = json!(started);
    }
    // A report with an Untested section says the work was not tried for real.
    if thread
        .last_report
        .as_deref()
        .is_some_and(|report| report.contains('\u{1f9ea}'))
    {
        row["untested"] = json!(true);
    }
    if let Some(pull_request) = session.and_then(pull_request_of) {
        row["pullRequest"] = pull_request;
    }
    if let Some(session) = session {
        let links = work_links_of(session);
        if !links.is_empty() {
            row["links"] = Value::Array(links);
        }
        if let Some(agent) = session.get("agentId").and_then(Value::as_str) {
            row["agent"] = json!(agent);
        }
    }
    if let Some(session) = session {
        if let Some(lifecycle) = session.get("lifecycleState") {
            row["lifecycleState"] = lifecycle.clone();
        }
        if let Some(marker) = crate::worktree_sessions::read_worktree_session_marker(session) {
            row["branch"] = json!(marker.branch);
        }
    }
    row
}

/// A thread on another computer this one is connected to, from what the remote supervisor last saw
/// of it (server/coordinator_remote_runtime.rs). `serverId` tells the panel's click to open it on
/// that computer, and `computer` names it on the row.
///
/// CDXC:RemoteMachines 2026-11-04 WHY:
/// User, of a thread this Mac's coordinator started on their Windows computer: "why dont i see that thread in the workers under you??". The Workers list read only `coordinator_threads`, the links of threads on this computer, so a coordinator's threads on other computers (`coordinator_remote_threads`, linked by `start-thread --computer`) were missing from it. They are built here from the link alone: the other computer may run an older gxserver and knows nothing of the coordinator. A thread whose computer is not connected reads asleep, as the sidebar draws it.
fn build_remote_row(thread: &RemoteThreadRecord, now: chrono::DateTime<chrono::Utc>) -> Value {
    let computer = crate::connected_computers::find(&thread.server_id)
        .map(|computer| computer.name)
        .unwrap_or_default();
    let unreachable = thread.state() == "unreachable";
    let state = if unreachable {
        "sleeping"
    } else {
        thread.state()
    };
    let detail = match state {
        _ if unreachable => "Its computer isn't connected".to_string(),
        "working" => report_headline(&thread.task, 140),
        "waiting" => "Waiting for an answer".to_string(),
        _ => thread
            .last_report
            .as_deref()
            .map(|report| report_headline(report, 140))
            .unwrap_or_default(),
    };
    let title = Some(thread.title.trim())
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| report_headline(&thread.task, 60));
    let mut row = json!({
        "serverId": thread.server_id,
        "computer": computer,
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "globalRef": format!("{}:{}:{}", thread.server_id, thread.project_id, thread.session_id),
        "title": title,
        "state": state,
        "detail": detail,
        "activeAt": thread.updated_at,
    });
    if is_recent(&thread.updated_at, now) {
        row["recent"] = json!(true);
    }
    if let Some(resolved_at) = &thread.resolved_at {
        row["resolvedAt"] = json!(resolved_at);
    }
    if let Some(lifecycle) = match state {
        "closed" => Some("stopped"),
        "sleeping" => Some("sleeping"),
        "done" => None,
        _ => Some("running"),
    } {
        row["lifecycleState"] = json!(lifecycle);
    }
    row
}

/// When `is_recent` turns false for this row, if it is true now.
fn recent_until(row: &Value) -> Option<chrono::DateTime<chrono::Utc>> {
    if row["recent"] != true {
        return None;
    }
    let active_at = chrono::DateTime::parse_from_rfc3339(row["activeAt"].as_str()?).ok()?;
    Some(active_at.with_timezone(&chrono::Utc) + chrono::Duration::milliseconds(RECENT_THREAD_MS))
}

fn session_exists(db: &Connection, key: &SessionKey) -> bool {
    db.prepare_cached("SELECT 1 FROM sessions WHERE projectId = ?1 AND sessionId = ?2")
        .and_then(|mut statement| statement.exists([&key.0, &key.1]))
        .unwrap_or(false)
}

/// Rebuilds every coordinator's panel and returns the coordinators whose panel changed.
pub fn refresh_coordinator_panels(
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Vec<SessionKey> {
    let Ok(coordinators) = list_coordinators(db) else {
        return Vec::new();
    };
    // The open threads read now, the resolved ones from memory unless one changed (thread_reads.rs).
    let all_threads = super::read_all_threads(db).ok();
    let threads = all_threads
        .as_ref()
        .map(super::AllThreads::ordered_shared)
        .unwrap_or_default();
    let remote_threads = list_remote_threads(db).unwrap_or_default();
    let mut board_items = super::work::list_all_work_items(db);
    let intro_hidden = super::intro::intro_hidden(db);
    let generated_at = now_iso();
    let now = chrono::Utc::now();
    let mut session_rows = db
        .prepare_cached("SELECT * FROM sessions WHERE projectId = ?1 AND sessionId = ?2")
        .ok();
    let mut previous_rows = rows()
        .lock()
        .map(|mut rows| std::mem::take(&mut *rows))
        .unwrap_or_default();
    let mut previous_fresh = fresh_rows()
        .lock()
        .map(|mut fresh| std::mem::take(&mut *fresh))
        .unwrap_or_default();
    let mut next_rows: HashMap<SessionKey, CachedRow> = HashMap::new();
    let mut next_fresh: HashMap<SessionKey, (u64, Value)> = HashMap::new();
    // Each coordinator's threads in `threads` order, grouped once instead of scanning every thread
    // for every coordinator.
    let mut threads_of: HashMap<SessionKey, Vec<&Arc<ThreadRecord>>> = HashMap::new();
    for thread in threads.iter().copied() {
        threads_of.entry(thread.coordinator_key()).or_default().push(thread);
    }
    let mut remote_of: HashMap<SessionKey, Vec<&RemoteThreadRecord>> = HashMap::new();
    for thread in &remote_threads {
        remote_of.entry(thread.coordinator_key()).or_default().push(thread);
    }
    let mut built: Vec<(
        SessionKey,
        Vec<(SessionKey, u64)>,
        Vec<Value>,
        chrono::DateTime<chrono::Utc>,
    )> = Vec::new();
    let mut turn_starts = super::turns::read_turn_starts(db);
    for coordinator in coordinators {
        let key = coordinator.key();
        if !session_exists(db, &key) {
            continue;
        }
        // The coordinator's own turns, for which of its items it is on (turns.rs).
        if let Some(started) = repository
            .get_session(&key.0, &key.1)
            .ok()
            .flatten()
            .and_then(|session| {
                crate::presentation::session_effective_working_started_at(&session, &generated_at)
            })
        {
            super::turns::note_turn_start(db, &mut turn_starts, &key, &started);
        }
        let idle_floor = super::turns::idle_floor(&turn_starts, &key, now);
        let mut row_ids: Vec<(SessionKey, u64)> = Vec::new();
        for thread in threads_of.get(&key).into_iter().flatten().copied() {
            let thread_key = thread.key();
            let live = thread_live(&thread.project_id, &thread.session_id);
            let cached = previous_rows.remove(&thread_key).filter(|cached| {
                cached.thread == *thread
                    && cached.live == live
                    && cached.recent_until.is_none_or(|until| now < until)
            });
            let cached = match cached {
                Some(cached)
                    if thread.is_resolved() && std::time::Instant::now() < cached.recheck_at =>
                {
                    row_ids.push((thread_key.clone(), cached.generation));
                    next_rows.insert(thread_key, cached);
                    continue;
                }
                other => other,
            };
            // A read error leaves the row uncached and builds it from `None`, as before.
            let stored = session_rows.as_mut().and_then(|statement| {
                read_session_row(statement, &thread_key, |hash| {
                    cached
                        .as_ref()
                        .is_none_or(|cached| cached.session != Some(hash))
                })
                .ok()
            });
            let fingerprint = stored
                .as_ref()
                .map(|row| row.as_ref().map(|(hash, _)| *hash));
            let previous = match (fingerprint, cached) {
                (Some(fingerprint), Some(cached)) if cached.session == fingerprint => {
                    row_ids.push((thread_key.clone(), cached.generation));
                    let recheck_at = resolved_recheck_at(&thread_key);
                    next_rows.insert(
                        thread_key,
                        CachedRow {
                            recheck_at,
                            ..cached
                        },
                    );
                    continue;
                }
                (_, cached) => cached.map(|cached| (cached.generation, cached.row)),
            };
            let previous = previous.or_else(|| previous_fresh.remove(&thread_key));
            let session = stored
                .flatten()
                .and_then(|(_, row)| row)
                .and_then(|row| crate::domain::session_from_row(&repository.server_id, row).ok());
            let row = build_row(
                repository.server_id.as_str(),
                thread,
                session.as_ref(),
                live.as_ref(),
                &generated_at,
                now,
            );
            let generation = row_generation(
                previous.as_ref().map(|(generation, row)| (*generation, row)),
                &row,
            );
            row_ids.push((thread_key.clone(), generation));
            match fingerprint {
                Some(fingerprint) if row_is_stable(thread, session.as_ref()) => {
                    next_rows.insert(
                        thread_key.clone(),
                        CachedRow {
                            thread: Arc::clone(thread),
                            session: fingerprint,
                            recent_until: recent_until(&row),
                            live: live.clone(),
                            recheck_at: resolved_recheck_at(&thread_key),
                            row,
                            generation,
                        },
                    );
                }
                _ => {
                    next_fresh.insert(thread_key, (generation, row));
                }
            }
        }
        let remote_rows = remote_of
            .get(&key)
            .into_iter()
            .flatten()
            .map(|thread| build_remote_row(thread, now))
            .collect::<Vec<_>>();
        built.push((key, row_ids, remote_rows, idle_floor));
    }
    let Ok(mut panels) = panels().lock() else {
        return Vec::new();
    };
    // The sidebar's tag catalog, for the board's tag chips and its Tags menu.
    let custom_tags =
        crate::custom_session_tags::read_custom_session_tags(db).unwrap_or(Value::Null);
    let mut inputs = panel_inputs()
        .lock()
        .map(|mut inputs| std::mem::take(&mut *inputs))
        .unwrap_or_default();
    let mut next_inputs: HashMap<SessionKey, PanelInputs> = HashMap::new();
    let mut next: HashMap<SessionKey, CachedPanel> = HashMap::new();
    // Panels built from the same inputs as last time: kept, and unchanged by definition.
    let mut kept: Vec<SessionKey> = Vec::new();
    for (key, row_ids, remote_rows, idle_floor) in built {
        let items = board_items.remove(&key).unwrap_or_default();
        if let Some(last) = inputs.remove(&key).filter(|last| {
            panels.contains_key(&key)
                && last.built_at.elapsed() < PANEL_REBUILD_EVERY
                && last.intro_hidden == intro_hidden
                && last.custom_tags == custom_tags
                && last.items == items
                && last.idle_floor == idle_floor
                && last.row_ids == row_ids
                && last.remote_rows == remote_rows
        }) {
            next_inputs.insert(key.clone(), last);
            kept.push(key);
            continue;
        }
        let mut panel_rows = row_ids
            .iter()
            .filter_map(|(thread_key, _)| {
                next_rows
                    .get(thread_key)
                    .map(|cached| cached.row.clone())
                    .or_else(|| next_fresh.get(thread_key).map(|(_, row)| row.clone()))
            })
            .collect::<Vec<_>>();
        panel_rows.extend(remote_rows.iter().cloned());
        let approvals = super::relay::track_approvals(&mut panel_rows);
        let mut board =
            super::board::board_value(&panel_rows, &items, Some(super::board::SUMMARY_ROWS), now);
        super::board::add_tag_looks(&mut board, &custom_tags);
        let mut work = super::work::work_value(&panel_rows, &items, false, now, Some(idle_floor));
        super::work::add_work_tag_looks(&mut work, &custom_tags);
        let panel = cached_panel(
            panel_rows,
            board,
            work,
            approvals,
            intro_hidden,
            panels.get(&key),
        );
        next.insert(key.clone(), panel);
        next_inputs.insert(
            key,
            PanelInputs {
                row_ids,
                remote_rows,
                items,
                custom_tags: custom_tags.clone(),
                intro_hidden,
                idle_floor,
                built_at: std::time::Instant::now(),
            },
        );
    }
    if let Ok(mut rows) = rows().lock() {
        *rows = next_rows;
    }
    if let Ok(mut fresh) = fresh_rows().lock() {
        *fresh = next_fresh;
    }
    let mut changed = next
        .iter()
        .filter(|(key, panel)| {
            panels.get(*key).map(|cached| &cached.summary) != Some(&panel.summary)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    // A coordinator that went away keeps its last panel on any open chat; nothing to republish.
    changed.sort();
    for key in kept {
        if let Some(panel) = panels.remove(&key) {
            next.insert(key, panel);
        }
    }
    *panels = next;
    if let Ok(mut inputs) = panel_inputs().lock() {
        *inputs = next_inputs;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote_thread(last_state: Option<&str>) -> RemoteThreadRecord {
        RemoteThreadRecord {
            server_id: "S4k".to_string(),
            project_id: "P7".to_string(),
            session_id: "G2".to_string(),
            coordinator_project_id: "P1".to_string(),
            coordinator_session_id: "G1".to_string(),
            task: "Find the CPU churn".to_string(),
            title: "Windows CPU churn".to_string(),
            resolved_at: None,
            observed_working: true,
            reported_at: None,
            reported_prompt_key: None,
            last_report: None,
            last_state: last_state.map(str::to_string),
            reported_message_id: None,
            checked_activity_ms: None,
            updated_at: "2026-11-04T17:00:00.000Z".to_string(),
        }
    }

    /// A working thread's line names its work, not the repo its brief opens with.
    #[test]
    fn a_working_line_skips_the_repo_sentence() {
        assert_eq!(
            task_without_repo_line("Repo: /x (main). Page kit and drafts: fix it"),
            "Page kit and drafts: fix it"
        );
        assert_eq!(
            task_without_repo_line("Repo: /x (main).\nFix the pill"),
            "Fix the pill"
        );
        assert_eq!(task_without_repo_line("Repo: /x"), "Repo: /x");
        assert_eq!(task_without_repo_line("Fix the pill"), "Fix the pill");
        assert_eq!(
            task_line("# A page lost its text\n\nRepo: /x (main). More"),
            "A page lost its text"
        );
        assert_eq!(task_line("Repo: /x (main). Fix the pill"), "Fix the pill");
    }

    /// A coordinator's thread on another computer is a Workers row that names that computer's
    /// server, so the click opens it there, and is in flight while it works.
    #[test]
    fn a_thread_on_another_computer_is_a_workers_row() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-04T17:30:00.000Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let row = build_remote_row(&remote_thread(Some("working")), now);
        assert_eq!(row["serverId"], "S4k");
        assert_eq!(row["projectId"], "P7");
        assert_eq!(row["sessionId"], "G2");
        assert_eq!(row["title"], "Windows CPU churn");
        assert_eq!(row["state"], "working");
        assert_eq!(row["detail"], "Find the CPU churn");
        assert_eq!(row["recent"], true);
        assert_eq!(panel_rank(&row), 0);
        let panel = cached_panel(vec![row], Value::Null, Value::Null, json!([]), false, None);
        assert_eq!(panel.summary["total"], 1);
        assert_eq!(panel.summary["threads"][0]["serverId"], "S4k");

        // Its computer is not connected: asleep, and the row says why.
        let away = build_remote_row(&remote_thread(Some("unreachable")), now);
        assert_eq!(away["state"], "sleeping");
        assert_eq!(away["detail"], "Its computer isn't connected");
        assert_eq!(away["lifecycleState"], "sleeping");
    }
}
