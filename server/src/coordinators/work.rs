//! A coordinator's work items: one item per line of work, named the way the user would name it,
//! with where it stands in one line, its status, who has the ball, the threads working on it, what
//! it asks of the user, what it waits on, its steps and its history. The coordinator keeps them with
//! `ghostex coordinator item …` (server/src/ghostex_cli/coordinator/item.rs) as it works; the chat
//! draws a card for each item that changed in a turn and the Work panel lists them all, both from
//! this record (`work` in `coordinatorThreads`, panel.rs).
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User, on the work items design page (work-items-view/page.html), wanting the replies to stop
//! "writing bunch of text at the bottom under Done / Needs you / etc which are jumbling up all the
//! tasks together" and "some sort of better view for understanding the work at high level (like
//! the board but even better)": Q2092 "B. Work items, with live cards under the reply"; Q2093 six
//! statuses: Needs you, Blocked (kept apart from Needs you: what needs the user now to unblock
//! work), Moving, Waiting, Later and Finished; Q2094 chores (rebuilds, test runs) hidden, never
//! items or steps the user sees; Q2095 an item's earlier card folds to one quiet line; Q2096 work
//! items replace the board and its open items move over once (migration 0074). A thread, a
//! question or a release step is never an item of its own: threads attach to an item, asks and
//! steps live inside it. Ghostex works out the status the user sees from the coordinator's word
//! and what it can see itself: an open ask that holds the work up is Blocked (a thread that is not
//! moving is not: the coordinator heals it, 2026-11-20), a thread waiting on the user is Needs you,
//! a working thread is Moving, and a wait on another item or an ask clears itself when that
//! finishes or is answered. Who has the ball (the owner) follows from the status.
//! SEE-ALSO: server/src/coordinators/board.rs (the old board, now drawn from these items for older
//! clients), server/src/storage/migrations/0074_coordinator_work_items.sql.

use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::records::{now_iso, sql_error, SessionKey};
use crate::domain::DomainStateError;

/// The statuses, in the order the Work panel lists them: Needs you first. `idle` (Not being worked
/// on) is never stored: it is what an In progress item nobody is on is shown as (`derive`).
pub const WORK_STATUSES: [&str; 7] = [
    "needs", "blocked", "moving", "idle", "waiting", "later", "finished",
];

/// How many history events an item keeps.
const HISTORY_KEEP: usize = 40;
/// How many of them the chat's copy carries.
const PANEL_HISTORY: usize = 8;
/// Finished items older than this are deleted.
const PRUNE_FINISHED_DAYS: i64 = 14;
/// Dropped items stay this long, so `item restore` can bring one back.
const DROPPED_KEEP_DAYS: i64 = 7;
/// Finished and dropped items stay in the chat's copy this long, so a card in a recent turn can
/// still be drawn; the panel shows only today's under Finished.
const PANEL_RECENT_DAYS: i64 = 3;

/// A Waiting item the coordinator has not rechecked for this long is named in `item list` and
/// `status` until it does (`recheck_reason`).
const RECHECK_WAITING_DAYS: i64 = 2;
/// Where the delivery events are kept (the `metadata` table): `{projectId: {kind: Delivery}}`.
const DELIVERIES_KEY: &str = "coordinatorDeliveries";

/// The events a wait can name instead of words (`item wait W7 --on release`), with the words the
/// user reads.
///
/// CDXC:Coordinators 2026-11-15 DECISION:
/// User, of the Work panel's Waiting group: "Why do I see a lot of stuff on the work items in
/// waiting and in the other statuses that I think should be marked as done? Why does this keep
/// happening?" Every one of the 22 Waiting items waited on words ("the next release", "next phone
/// over-the-air update", "the next rebuild of your Mac app"), which never clear, so 15 delivered
/// items sat there after 11.7.1 shipped. On the options page (board-stale-options.html) the user
/// picked A (waits on known events), B (a nudge for stale Waiting and Later items) and D (done
/// when landed). A wait on one of these events closes once the event happens after the wait was
/// set, and the item comes back to the coordinator as delivered, to check and finish: Ghostex
/// still never finishes an item itself (turns.rs). What gxserver can see: a rebuild is the
/// running gxserver starting from an executable built after the wait (`cargo xtask rebuild` and
/// an app update both restart it), or `item delivered rebuild`; a release and a phone update are
/// published by GitHub Actions, which this computer cannot see, so the release flow records them
/// (`ghostex coordinator item delivered release v11.7.1`, ghostex-release skill), per project, so
/// another project's release clears nothing here.
pub const WAIT_EVENTS: [(&str, &str); 3] = [
    ("release", "the next release"),
    ("ota", "the next phone update"),
    ("rebuild", "the next rebuild of Ghostex on this computer"),
];

/// The event a wait word names (`release`, `ota`, `rebuild` and a few spellings), or `None` for
/// words.
pub fn wait_event(word: &str) -> Option<&'static str> {
    match word.trim().to_ascii_lowercase().as_str() {
        "release" | "next-release" => Some("release"),
        "ota" | "phone-update" | "next-ota" => Some("ota"),
        "rebuild" | "app-rebuild" | "next-rebuild" => Some("rebuild"),
        _ => None,
    }
}

fn wait_event_words(kind: &str) -> &'static str {
    WAIT_EVENTS
        .iter()
        .find(|(event, _)| *event == kind)
        .map_or("", |(_, words)| words)
}

/// The latest time an event happened: when, and its name ("v11.7.1"), "" for none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Delivery {
    pub at: String,
    pub name: String,
}

type ProjectDeliveries = HashMap<String, Delivery>;

/// The delivery events, read once from the database and kept current by `record_delivery`.
static DELIVERIES: std::sync::RwLock<Option<HashMap<String, ProjectDeliveries>>> =
    std::sync::RwLock::new(None);

/// When the executable this gxserver runs was built, read once at its start.
static RUNNING_BUILD: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

fn read_deliveries(db: &Connection) -> HashMap<String, ProjectDeliveries> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        params![DELIVERIES_KEY],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|text| serde_json::from_str(&text).ok())
    .unwrap_or_default()
}

/// Reads the delivery events once, and notes this gxserver's build. Called where a database is at
/// hand: the panels' refresh (soon after start), `item` requests and `status`.
pub fn load_deliveries(db: &Connection) {
    note_running_build();
    if DELIVERIES.read().is_ok_and(|loaded| loaded.is_some()) {
        return;
    }
    let read = read_deliveries(db);
    if let Ok(mut loaded) = DELIVERIES.write() {
        loaded.get_or_insert(read);
    }
}

/// Notes when the running executable was built (its modification time), the first time it is
/// asked: a rebuild or an update replaces the file and restarts gxserver.
pub fn note_running_build() {
    RUNNING_BUILD.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        let modified = std::fs::metadata(exe).ok()?.modified().ok()?;
        Some(DateTime::<Utc>::from(modified).to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    });
}

fn project_deliveries(project_id: &str) -> ProjectDeliveries {
    DELIVERIES
        .read()
        .ok()
        .and_then(|loaded| loaded.as_ref()?.get(project_id).cloned())
        .unwrap_or_default()
}

/// The delivery events of a project as one text, for a cache that rebuilds when one happens
/// (session_work.rs).
pub(super) fn deliveries_stamp(project_id: &str) -> String {
    let sorted: std::collections::BTreeMap<String, Delivery> =
        project_deliveries(project_id).into_iter().collect();
    serde_json::to_string(&sorted).unwrap_or_default()
}

/// Records that an event happened now for the coordinator's project (`item delivered`).
fn record_delivery(
    db: &Connection,
    project_id: &str,
    kind: &str,
    name: &str,
    at: &str,
) -> Result<(), DomainStateError> {
    let mut all = read_deliveries(db);
    all.entry(project_id.to_string()).or_default().insert(
        kind.to_string(),
        Delivery {
            at: at.to_string(),
            name: name.to_string(),
        },
    );
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        params![DELIVERIES_KEY, to_json(&all), at],
    )
    .map_err(sql_error)?;
    if let Ok(mut loaded) = DELIVERIES.write() {
        *loaded = Some(all);
    }
    Ok(())
}

fn parse_time(at: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// A thread working on an item: its server (empty for this computer), project and session.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkThread {
    pub server_id: String,
    pub project_id: String,
    pub session_id: String,
}

/// Something the item needs from the user: a question with one-click answers, or a task.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkAsk {
    /// Its question tag (`Q2101`), "" for an ask moved from the board without one.
    pub tag: String,
    /// What it asks, as its title: one short plain question or instruction.
    pub text: String,
    /// The longer explanation under the title (why, what to check, where), "" for none; asks
    /// stored before it read as "".
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// User, of dock rows showing the first words of a whole paragraph: "please we need to tell
    /// the agent to write a easy to understand title, and then the description can describe
    /// more". `item ask W7 "<title>" --detail "<the rest>"`.
    pub detail: String,
    /// One-click answers, the recommended one ending in `*` ("Yes*,No"); "" for a task.
    pub answers: String,
    /// The work stops until the user does it: the item is Blocked while it is open.
    pub holds: bool,
    /// A file or link with the steps to follow.
    pub steps: String,
    /// The answer the user gave ("" for none; a task's Mark done records "Done").
    pub answered: String,
    pub asked_at: String,
    /// Answered, done or withdrawn.
    pub settled_at: Option<String>,
    pub withdrawn: bool,
    /// Snoozed by the user until this moment (ISO): out of the dock and the Needs you count until
    /// then, still listed in the Work panel (`snooze` below).
    pub snoozed_until: Option<String>,
    /// A command the user runs for it, which its dock row shows as a chip that copies it
    /// (`ghostex needs-you add --command`), "" for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// The thread whose `ghostex needs-you add` made this ask: the user's answer goes to that
    /// thread too (server/src/session_needs_you/thread.rs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_thread: Option<WorkThread>,
}

impl WorkAsk {
    pub fn is_open(&self) -> bool {
        self.settled_at.is_none()
    }
}

/// What the item waits on: another item (`W12`), an ask (`Q12`) or something outside, in words.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkWait {
    pub on: String,
    /// When it is expected (a date or time in the coordinator's words), "" for none.
    pub until: String,
    pub since: String,
}

/// A step of the item. A chore (a rebuild, a test run) is kept but never shown (Q2094).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkStep {
    pub text: String,
    pub done: bool,
    pub at: String,
    pub chore: bool,
}

/// One change to the item, for its history and the chat's cards.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkEvent {
    pub at: String,
    /// add, now, step, ask, answer, reply, wait, done, drop, restore, status, thread, edit, moved,
    /// settle.
    pub kind: String,
    pub text: String,
    /// The coordinator's status after the change.
    pub status: String,
}

/// One row of `coordinator_work_items`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkItem {
    pub number: i64,
    pub title: String,
    /// The coordinator's word: later, needs, blocked, moving, waiting or finished.
    pub status: String,
    /// What kind of Later: next, held, parked, backlog, or "".
    pub reason: String,
    /// Where it stands, in one line; replaced, never appended.
    pub now: String,
    pub link: String,
    pub release: String,
    pub untested: bool,
    /// The user's tag (one of the sidebar's session tags), "" for none.
    pub tag: String,
    /// Exists only to place or hide one thread (a board pin): it takes the thread's title, and
    /// putting the thread back on its own state deletes it.
    pub for_thread: bool,
    pub threads: Vec<WorkThread>,
    pub asks: Vec<WorkAsk>,
    pub waits: Vec<WorkWait>,
    pub steps: Vec<WorkStep>,
    pub history: Vec<WorkEvent>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub dropped_at: Option<String>,
}

impl WorkItem {
    pub fn reference(&self) -> String {
        format!("W{}", self.number)
    }

    pub fn has_thread(&self, project_id: &str, session_id: &str) -> bool {
        self.threads
            .iter()
            .any(|thread| thread.project_id == project_id && thread.session_id == session_id)
    }

    fn record(&mut self, kind: &str, text: impl Into<String>, at: &str) {
        self.history.push(WorkEvent {
            at: at.to_string(),
            kind: kind.to_string(),
            text: text.into(),
            status: if self.finished_at.is_some() {
                "finished".to_string()
            } else {
                self.status.clone()
            },
        });
        if self.history.len() > HISTORY_KEEP {
            let extra = self.history.len() - HISTORY_KEEP;
            self.history.drain(..extra);
        }
        self.updated_at = at.to_string();
    }

    /// The question tags its open asks and its own words name, for the chat's ledger.
    fn question_tags(&self, ask: Option<&WorkAsk>) -> Vec<String> {
        match ask {
            Some(ask) if !ask.tag.is_empty() => vec![ask.tag.to_ascii_uppercase()],
            Some(ask) => tag_words(&format!("{}\n\n{}", ask.text, self.now)),
            None => tag_words(&format!("{}\n\n{}", self.title, self.now)),
        }
    }
}

fn tag_words(text: &str) -> Vec<String> {
    ghostex_gx_chat_core::transcript::open_questions::tag_words(&text.replace('`', ""))
}

fn json_list<T: for<'de> Deserialize<'de>>(row: &Row<'_>, column: &str) -> Vec<T> {
    row.get::<_, String>(column)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn item_from_row(row: &Row<'_>) -> rusqlite::Result<WorkItem> {
    Ok(WorkItem {
        number: row.get("itemNumber")?,
        title: row.get("title")?,
        status: row.get("status")?,
        reason: row.get("reason")?,
        now: row.get("nowLine")?,
        link: row.get("link")?,
        release: row.get("release")?,
        untested: row.get::<_, i64>("untested")? != 0,
        tag: row.get("tag")?,
        for_thread: row.get::<_, i64>("forThread")? != 0,
        threads: json_list(row, "threadsJson"),
        asks: json_list(row, "asksJson"),
        waits: json_list(row, "waitsJson"),
        steps: json_list(row, "stepsJson"),
        history: json_list(row, "historyJson"),
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
        finished_at: row.get("finishedAt")?,
        dropped_at: row.get("droppedAt")?,
    })
}

/// Every item of one coordinator, oldest first.
pub fn list_work_items(
    db: &Connection,
    key: &SessionKey,
) -> Result<Vec<WorkItem>, DomainStateError> {
    let mut statement = db
        .prepare_cached(
            "SELECT * FROM coordinator_work_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 ORDER BY itemNumber",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([&key.0, &key.1], item_from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

/// Every coordinator's items, for the supervisor's tick. A database without the table (a test
/// fixture) reads as no items.
pub fn list_all_work_items(db: &Connection) -> HashMap<SessionKey, Vec<WorkItem>> {
    let mut items: HashMap<SessionKey, Vec<WorkItem>> = HashMap::new();
    let Ok(mut statement) =
        db.prepare_cached("SELECT * FROM coordinator_work_items ORDER BY itemNumber")
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
) -> Result<Option<WorkItem>, DomainStateError> {
    db.query_row(
        "SELECT * FROM coordinator_work_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber = ?3",
        params![key.0, key.1, number],
        item_from_row,
    )
    .optional()
    .map_err(sql_error)
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[]".to_string())
}

/// Whether a coordinator's turn that started at `since` put something new on the user: an ask it
/// opened (an item ask, Blocked while it `holds`), or an item it moved into Needs you or Blocked
/// that is still there. A finished or dropped item never counts.
///
/// CDXC:Notifications 2026-11-13 DECISION:
/// User: "we're getting way too many notifications from the coordinator when it finishes. I
/// don't think anyone cares about when the coordinator finishes. Please disable those. Just show
/// for new needs-you, blocked or approval: those need user attention." A coordinator's handover
/// ends with a ❓ Needs you section nearly every turn, repeating what already waits, so its text no
/// longer decides; its board does (`turn_raised_for_user` in presentation.rs).
pub fn raised_for_user_since(items: &[WorkItem], since: DateTime<Utc>) -> bool {
    let after =
        |at: &str| DateTime::parse_from_rfc3339(at).is_ok_and(|at| at.with_timezone(&Utc) >= since);
    items
        .iter()
        .filter(|item| item.finished_at.is_none() && item.dropped_at.is_none())
        .any(|item| {
            if item
                .asks
                .iter()
                .any(|ask| ask.is_open() && !ask.withdrawn && after(&ask.asked_at))
            {
                return true;
            }
            if !matches!(item.status.as_str(), "needs" | "blocked") {
                return false;
            }
            // When it last entered its current status (the event whose status changed to it).
            let mut previous: Option<&str> = None;
            let mut entered: Option<&str> = None;
            for event in &item.history {
                if event.status == item.status && previous != Some(item.status.as_str()) {
                    entered = Some(event.at.as_str());
                }
                previous = Some(event.status.as_str());
            }
            after(entered.unwrap_or(item.created_at.as_str()))
        })
}

pub(crate) fn write_work_item(
    db: &Connection,
    key: &SessionKey,
    item: &WorkItem,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO coordinator_work_items (
          coordinatorProjectId, coordinatorSessionId, itemNumber, title, status, reason, nowLine,
          link, release, untested, tag, forThread, threadsJson, asksJson, waitsJson, stepsJson,
          historyJson, createdAt, updatedAt, finishedAt, droppedAt
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
        ON CONFLICT (coordinatorProjectId, coordinatorSessionId, itemNumber) DO UPDATE SET
          title = excluded.title, status = excluded.status, reason = excluded.reason,
          nowLine = excluded.nowLine, link = excluded.link, release = excluded.release,
          untested = excluded.untested, tag = excluded.tag, forThread = excluded.forThread,
          threadsJson = excluded.threadsJson, asksJson = excluded.asksJson,
          waitsJson = excluded.waitsJson, stepsJson = excluded.stepsJson,
          historyJson = excluded.historyJson, updatedAt = excluded.updatedAt,
          finishedAt = excluded.finishedAt, droppedAt = excluded.droppedAt
        "#,
        params![
            key.0,
            key.1,
            item.number,
            item.title,
            item.status,
            item.reason,
            item.now,
            item.link,
            item.release,
            i64::from(item.untested),
            item.tag,
            i64::from(item.for_thread),
            to_json(&item.threads),
            to_json(&item.asks),
            to_json(&item.waits),
            to_json(&item.steps),
            to_json(&item.history),
            item.created_at,
            item.updated_at,
            item.finished_at,
            item.dropped_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(crate) fn delete_work_item(
    db: &Connection,
    key: &SessionKey,
    number: i64,
) -> Result<(), DomainStateError> {
    db.execute(
        "DELETE FROM coordinator_work_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND itemNumber = ?3",
        params![key.0, key.1, number],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The next W<number>: one past the highest ever used, the board's B numbers included, so a
/// number is never reused while its old row is still around to be confused with it.
pub(crate) fn next_item_number(db: &Connection, key: &SessionKey) -> Result<i64, DomainStateError> {
    let work: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(itemNumber), 0) FROM coordinator_work_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2",
            params![key.0, key.1],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    // The old board's table, which a database from before 0055 (a test fixture) lacks.
    let board: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(itemNumber), 0) FROM coordinator_board_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2",
            params![key.0, key.1],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(work.max(board) + 1)
}

fn cutoff(days: i64) -> String {
    (Utc::now() - chrono::Duration::days(days)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Deletes finished items past `PRUNE_FINISHED_DAYS` and dropped ones past `DROPPED_KEEP_DAYS`.
fn prune(db: &Connection, key: &SessionKey) -> Result<(), DomainStateError> {
    db.execute(
        "DELETE FROM coordinator_work_items WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 AND ((finishedAt IS NOT NULL AND finishedAt < ?3) OR (droppedAt IS NOT NULL AND droppedAt < ?4))",
        params![key.0, key.1, cutoff(PRUNE_FINISHED_DAYS), cutoff(DROPPED_KEEP_DAYS)],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The status a word names (`needs-you`, `in-progress`, `on-hold`…) and the Later kind it implies.
/// The board's words still work, so a coordinator that learned them keeps going.
pub fn parse_status(word: &str) -> Option<(&'static str, &'static str)> {
    let word = word.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    Some(match word.as_str() {
        "later" | "todo" | "to-do" => ("later", ""),
        "parked" | "park" => ("later", "parked"),
        "next" | "up-next" | "upnext" => ("later", "next"),
        "held" | "held-for-release" => ("later", "held"),
        "backlog" | "ideas" | "idea" => ("later", "backlog"),
        "needs" | "needs-you" | "you" => ("needs", ""),
        "blocked" | "block" => ("blocked", ""),
        "waiting" | "wait" | "hold" | "on-hold" | "onhold" | "paused" => ("waiting", ""),
        "moving" | "in-progress" | "working" | "now" => ("moving", ""),
        "finished" | "done" => ("finished", ""),
        _ => return None,
    })
}

/// The heading each status is shown under.
pub fn status_heading(status: &str) -> &'static str {
    match status {
        "needs" => "Needs you",
        "blocked" => "Blocked",
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User: "Please don't say Moving say In Progress. Moving isn't clear to me" (and on 2026-11-07, of
        // the board: "rename to in progress please"). Wherever the user reads it, the `moving` status is
        // "In progress"; the stored value stays `moving` and `--status` takes `moving` or `in-progress`.
        "moving" => "In progress",
        "idle" => "Not being worked on",
        "waiting" => "Waiting",
        "later" => "Later",
        "finished" => "Finished",
        "dropped" => "Dropped",
        _ => "Work",
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// What the status and owner are worked out from: the coordinator's threads as the panel reads
/// them, and its other items (for waits).
pub struct WorkContext<'a> {
    threads: HashMap<(String, String), &'a Value>,
    items: &'a [WorkItem],
    /// An In progress item with no thread on it that the coordinator last changed before this is
    /// not being worked on (turns.rs `idle_floor`); `None` keeps every such item In progress.
    idle_floor: Option<DateTime<Utc>>,
    /// The question tags the coordinator's chat asked (a page's, a form's, a one-click answer's),
    /// uppercased, with whether each is settled, so a wait on one clears when it is answered.
    chat_tags: HashMap<String, bool>,
    /// The events that happened for the coordinator's project (`item delivered`).
    deliveries: ProjectDeliveries,
}

impl<'a> WorkContext<'a> {
    pub fn new(threads: &'a [Value], items: &'a [WorkItem]) -> Self {
        Self {
            threads: threads
                .iter()
                .map(|row| ((text(row, "projectId"), text(row, "sessionId")), row))
                .collect(),
            items,
            idle_floor: None,
            chat_tags: HashMap::new(),
            deliveries: ProjectDeliveries::new(),
        }
    }

    /// The same, with what the coordinator's chat asked and its project's delivery events.
    pub fn with_coordinator(mut self, key: Option<&SessionKey>) -> Self {
        if let Some(key) = key {
            self.chat_tags =
                crate::session_chat_open_questions::asked_question_tags(&key.0, &key.1);
            self.deliveries = project_deliveries(&key.0);
        }
        self
    }

    /// When the event a wait names happened after the wait was set: its name and time.
    fn delivered(&self, kind: &str, since: &str) -> Option<Delivery> {
        let since = parse_time(since)?;
        let mut best: Option<Delivery> = self
            .deliveries
            .get(kind)
            .filter(|event| parse_time(&event.at).is_some_and(|at| at > since))
            .cloned();
        if kind == "rebuild" && best.is_none() {
            // The running gxserver was built after the wait: Ghostex here was rebuilt or updated.
            best = RUNNING_BUILD
                .get()
                .cloned()
                .flatten()
                .filter(|at| parse_time(at).is_some_and(|at| at > since))
                .map(|at| Delivery {
                    at,
                    name: format!(
                        "{}'s Ghostex",
                        crate::connected_computers::this_computer_name()
                    ),
                });
        }
        best
    }

    /// The same, telling items nobody is on from the ones the coordinator is on (turns.rs).
    pub fn with_idle_floor(mut self, floor: Option<DateTime<Utc>>) -> Self {
        self.idle_floor = floor;
        self
    }

    /// The coordinator's own In progress for `item`, or Not being worked on when it last changed
    /// the item before the start of its last turn.
    fn coordinator_on(&self, item: &WorkItem) -> (&'static str, Value) {
        let idle = self.idle_floor.is_some_and(|floor| {
            DateTime::parse_from_rfc3339(coordinator_changed_at(item))
                .is_ok_and(|at| at.with_timezone(&Utc) < floor)
        });
        if idle && item.asks.iter().any(WorkAsk::is_open) {
            // Nobody is on it but it asks the user something: the ball is theirs.
            ("needs", json!({ "kind": "user", "label": "You" }))
        } else if idle {
            ("idle", json!({ "kind": "", "label": "" }))
        } else {
            (
                "moving",
                json!({ "kind": "coordinator", "label": "Coordinator" }),
            )
        }
    }

    fn thread(&self, thread: &WorkThread) -> Option<&'a Value> {
        self.threads
            .get(&(thread.project_id.clone(), thread.session_id.clone()))
            .copied()
    }

    fn ask(&self, tag: &str) -> Option<(&'a WorkItem, &'a WorkAsk)> {
        self.items.iter().find_map(|item| {
            item.asks
                .iter()
                .find(|ask| !ask.tag.is_empty() && ask.tag.eq_ignore_ascii_case(tag))
                .map(|ask| (item, ask))
        })
    }

    fn item(&self, reference: &str) -> Option<&'a WorkItem> {
        let number = item_number_of(reference)?;
        self.items.iter().find(|item| item.number == number)
    }

    /// A wait as shown: what it is, its words, and whether it still holds.
    fn wait(&self, wait: &WorkWait) -> Value {
        let mut out = json!({ "on": wait.on, "kind": "outside", "label": wait.on, "open": true });
        if let Some(item) = self.item(&wait.on) {
            out["kind"] = json!("item");
            out["label"] = json!(item.title);
            out["open"] = json!(item.finished_at.is_none() && item.dropped_at.is_none());
        } else if let Some((_, ask)) = question_tag(&wait.on).and_then(|tag| self.ask(&tag)) {
            out["kind"] = json!("ask");
            out["label"] = json!(ask.text);
            out["open"] = json!(ask.is_open());
        } else if let Some(settled) =
            question_tag(&wait.on).and_then(|tag| self.chat_tags.get(&tag))
        {
            // A question the chat asked (a page, a form): it clears once the user answers it.
            out["kind"] = json!("ask");
            out["label"] = json!(format!("{} (asked in the chat)", wait.on));
            out["open"] = json!(!settled);
        } else if let Some(kind) = wait_event(&wait.on) {
            out["kind"] = json!("event");
            out["event"] = json!(kind);
            out["label"] = json!(wait_event_words(kind));
            if let Some(event) = self.delivered(kind, &wait.since) {
                out["open"] = json!(false);
                out["delivered"] = json!(event.name);
                out["deliveredAt"] = json!(event.at);
            }
        }
        if !wait.until.is_empty() {
            out["until"] = json!(wait.until);
        }
        out
    }
}

/// When the coordinator last changed the item itself: its newest history event that is not the
/// user's (an answer, a settled ask) or a move from the board, else its creation.
pub fn coordinator_changed_at(item: &WorkItem) -> &str {
    item.history
        .iter()
        .rev()
        .find(|event| !matches!(event.kind.as_str(), "answer" | "settle" | "moved"))
        .map_or(item.created_at.as_str(), |event| event.at.as_str())
}

/// `W12`, `w12`, `B12` (the board's name for the same item) or `12`.
pub fn item_number_of(reference: &str) -> Option<i64> {
    reference
        .trim()
        .trim_start_matches(['W', 'w', 'B', 'b'])
        .parse()
        .ok()
        .filter(|number: &i64| *number > 0)
}

/// `Q12` as a question tag, uppercased.
fn question_tag(word: &str) -> Option<String> {
    let word = word.trim();
    let digits = word.strip_prefix(['Q', 'q'])?;
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| format!("Q{digits}"))
}

/// Whether a thread row's wait genuinely requires the user in Needs you (approval, folder trust,
/// sign-in, or an exhausted account pool with no auto-resume). Usage-limit pauses that stay calm
/// or auto-recover, exited agents, and coordinator-handled questions return `false`
/// (CDXC:Coordinators 2026-11-22 DECISION, Q4476-Q4478).
/// A legacy or remote row with `state == "waiting"` and no `waitKind` / `waitNeedsUser` metadata
/// keeps its existing behavior so older peers do not lose visibility.
pub(super) fn thread_wait_needs_user(row: &Value) -> bool {
    if let Some(needs) = row.get("waitNeedsUser").and_then(Value::as_bool) {
        return needs;
    }
    if row["needsApproval"] == true {
        return true;
    }
    match row.get("waitKind").and_then(Value::as_str) {
        Some("approval" | "signIn") => true,
        Some("usageLimit" | "agentExited" | "question") => false,
        None => row["state"] == "waiting",
        Some(_) => false,
    }
}

/// Whether a thread row is paused on a usage limit or waiting on something the coordinator handles
/// (not promoted to Needs you, and kept in In progress instead of dropping to Not being worked on).
fn thread_has_paused_wait(row: &Value) -> bool {
    if thread_wait_needs_user(row) {
        return false;
    }
    row["waitKind"] == "usageLimit" || row["state"] == "waiting"
}

/// The status the user sees and who has the ball.
pub fn derive(item: &WorkItem, context: &WorkContext<'_>) -> (&'static str, Value) {
    let you = || json!({ "kind": "user", "label": "You" });
    let nobody = || json!({ "kind": "", "label": "" });
    if item.dropped_at.is_some() {
        return ("dropped", nobody());
    }
    let threads: Vec<&Value> = item
        .threads
        .iter()
        .filter_map(|thread| context.thread(thread))
        .collect();
    let thread_owner = |row: &Value| {
        json!({
            "kind": "thread",
            "label": text(row, "title"),
            "ref": text(row, "sessionId"),
        })
    };
    if item.finished_at.is_some() || item.status == "finished" {
        return ("finished", nobody());
    }
    // An item for one thread follows the thread: resolved is finished.
    if item.for_thread && !threads.is_empty() && threads.iter().all(|row| row["state"] == "done") {
        return ("finished", nobody());
    }
    // CDXC:Coordinators 2026-11-20 DECISION:
    // User, of W756 shown red Blocked while its thread was only in a long silent build ("why are
    // you showing ME this as blocked? I don't care about this, there's no action here"), then on
    // the Q4301 page, picking A: "let the coordinator fix the stuck threads, not me". Blocked is
    // only an open ask that holds the work up (`item ask --holds`). A thread quiet after Ghostex's
    // nudge stays In progress: healing it is the coordinator's job, which hears about it every 8
    // minutes and sees it as `quiet` in `item list` and `status` (`quiet_hint`). A stored
    // "blocked" with no holding ask (older items, the board's words) is In progress too.
    if item.asks.iter().any(|ask| ask.is_open() && ask.holds) {
        return ("blocked", you());
    }
    // CDXC:Coordinators 2026-11-22 DECISION (Q4476, Q4478):
    // Only promote a thread wait to Needs you when it truly requires the user (approval, folder
    // trust, sign-in, or an exhausted usage-limit pool requiring manual action). Calm /
    // auto-recovering usage limits and coordinator-handled pauses stay out of Needs you.
    if threads.iter().any(|row| thread_wait_needs_user(row)) {
        return ("needs", you());
    }
    if let Some(row) = threads.iter().find(|row| row["state"] == "working") {
        return ("moving", thread_owner(row));
    }
    if item.status == "waiting" {
        let waits: Vec<Value> = item.waits.iter().map(|wait| context.wait(wait)).collect();
        match waits.iter().find(|wait| wait["open"] == true) {
            Some(wait) => {
                let owner = match wait["kind"].as_str() {
                    Some("ask") => you(),
                    Some("item") => {
                        json!({ "kind": "item", "label": wait["label"], "ref": wait["on"] })
                    }
                    _ => json!({ "kind": "outside", "label": wait["label"] }),
                };
                return ("waiting", owner);
            }
            // Waiting with no wait named (moved from the board's On hold) stays Waiting; once
            // every wait it named is done, the coordinator has the ball again.
            None if waits.is_empty() => {
                return ("waiting", json!({ "kind": "outside", "label": "" }));
            }
            None => return context.coordinator_on(item),
        }
    }
    if matches!(item.status.as_str(), "moving" | "blocked") {
        // Keep usage-limit-paused and coordinator-paused threads in In progress with their thread
        // chip and calm limit pill, rather than promoting to Needs you or dropping to Idle.
        if let Some(row) = threads.iter().find(|row| thread_has_paused_wait(row)) {
            return ("moving", thread_owner(row));
        }
        // A thread that still owes work (a job running, a turn cut off) keeps the ball.
        if let Some(row) = threads
            .iter()
            .find(|row| !text(row, "unfinished").is_empty())
        {
            return ("moving", thread_owner(row));
        }
        return context.coordinator_on(item);
    }
    if item.asks.iter().any(WorkAsk::is_open) || item.status == "needs" {
        return ("needs", you());
    }
    ("later", nobody())
}

/// An ask's stored answers ("Yes*,No") as choices: `{label, recommended}`.
pub fn parse_answers(answers: &str) -> Vec<Value> {
    split_answers(answers)
        .into_iter()
        .filter(|answer| !answer.is_empty() && answer != "*")
        .map(|answer| match answer.strip_suffix('*') {
            Some(label) => json!({ "label": label.trim(), "recommended": true }),
            None => json!({ "label": answer, "recommended": false }),
        })
        .collect()
}

/// Stored answers split on their commas, a `\,` being a comma inside an answer, each trimmed.
///
/// CDXC:Coordinators 2026-11-10 WHY:
/// `item ask --answers "Too small to read, simplify it,Looks fine"` made three buttons of two
/// answers. An answer may hold a comma: the repeatable `--answer` flag takes it as is, and
/// `--answers` takes `\,`; both are stored with the comma escaped.
fn split_answers(answers: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = answers.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\\' if chars.peek() == Some(&',') => {
                current.push(',');
                chars.next();
            }
            ',' => out.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(character),
        }
    }
    out.push(current.trim().to_string());
    out
}

/// The answers of a `--answers "A,B"` (a `\,` is a comma inside an answer) with `--recommended
/// B`, as stored ("A,B*").
pub fn mark_answers(answers: &str, recommended: &str) -> String {
    mark_answer_list(&split_answers(answers), recommended)
}

/// A list of answers (`--answer` repeated, commas allowed) with the recommended one marked, as
/// stored: commas escaped, joined with commas.
pub fn mark_answer_list(answers: &[String], recommended: &str) -> String {
    answers
        .iter()
        .map(|answer| answer.trim())
        .filter(|answer| !answer.is_empty())
        .map(|answer| {
            let answer = answer.trim_end_matches('*');
            let escaped = answer.replace(',', "\\,");
            if !recommended.is_empty() && answer.eq_ignore_ascii_case(recommended.trim()) {
                format!("{escaped}*")
            } else {
                escaped
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn ask_value(ask: &WorkAsk) -> Value {
    let mut out = json!({
        "tag": ask.tag,
        "text": ask.text,
        "answers": parse_answers(&ask.answers),
        "holds": ask.holds,
        "open": ask.is_open(),
        "askedAt": ask.asked_at,
    });
    for (key, value) in [
        ("detail", &ask.detail),
        ("steps", &ask.steps),
        ("answered", &ask.answered),
        ("command", &ask.command),
    ] {
        if !value.is_empty() {
            out[key] = json!(value);
        }
    }
    if let Some(at) = &ask.settled_at {
        out["settledAt"] = json!(at);
    }
    // The thread whose to-do it is: its row's Jump to context opens that thread's chat
    // (gx-chat-core extras/work_cards.rs `context_of`).
    if let Some(thread) = &ask.from_thread {
        out["fromThread"] = json!(thread);
    }
    if ask.withdrawn {
        out["withdrawn"] = json!(true);
    }
    // A snooze still running; one that has ended reads as if there was none.
    if let Some((until, at)) = ask.snoozed_until.as_deref().and_then(|until| {
        let at = DateTime::parse_from_rfc3339(until)
            .ok()?
            .with_timezone(&Local);
        (at.with_timezone(&Utc) > Utc::now()).then_some((until, at))
    }) {
        out["snoozedUntil"] = json!(until);
        // "3:40 PM", or "Tue 9:40 AM" past today, as the Work panel says it.
        let label = if at.date_naive() == Local::now().date_naive() {
            at.format("%-I:%M %p").to_string()
        } else {
            at.format("%a %-I:%M %p").to_string()
        };
        out["snoozedUntilLabel"] = json!(label);
    }
    out
}

/// A thread attached to an item, as its card names it.
fn thread_value(thread: &WorkThread, row: Option<&Value>) -> Value {
    let mut out = json!({
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "title": row.map(|row| text(row, "title")).unwrap_or_default(),
        "state": row.map(|row| text(row, "state")).unwrap_or_default(),
    });
    if !thread.server_id.is_empty() {
        out["serverId"] = json!(thread.server_id);
    }
    let Some(row) = row else {
        return out;
    };
    for key in [
        "detail",
        "agent",
        "computer",
        "branch",
        "globalRef",
        "unfinished",
        "waitKind",
        "waitSummary",
        "waitResetsAt",
    ] {
        let value = text(row, key);
        if !value.is_empty() {
            out[key] = json!(value);
        }
    }
    for key in ["stuck", "needsApproval", "untested", "waitAutoRecover"] {
        if row[key] == true {
            out[key] = json!(true);
        }
    }
    if let Some(needs) = row.get("waitNeedsUser").and_then(Value::as_bool) {
        out["waitNeedsUser"] = json!(needs);
    }
    if let Some(pr) = row.get("pullRequest").filter(|pr| pr.is_object()) {
        out["pr"] = pr.clone();
    }
    if let Some(links) = row.get("links").filter(|links| links.is_array()) {
        out["links"] = links.clone();
    }
    out
}

/// One item as every client and the CLI read it.
pub fn item_value(item: &WorkItem, context: &WorkContext<'_>, history: usize) -> Value {
    let (status, owner) = derive(item, context);
    let shown_steps: Vec<&WorkStep> = item.steps.iter().filter(|step| !step.chore).collect();
    let title = if item.for_thread {
        item.threads
            .first()
            .and_then(|thread| context.thread(thread))
            .map(|row| text(row, "title"))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| item.title.clone())
    } else {
        item.title.clone()
    };
    let mut out = json!({
        "key": format!("work:{}", item.number),
        "kind": "item",
        "ref": item.reference(),
        "number": item.number,
        "title": title,
        "status": status,
        "owner": owner,
        "now": item.now,
        "at": item.updated_at,
        "createdAt": item.created_at,
        "steps": {
            "done": shown_steps.iter().filter(|step| step.done).count(),
            "total": shown_steps.len(),
            "rows": shown_steps.iter().map(|step| json!({ "text": step.text, "done": step.done, "at": step.at })).collect::<Vec<_>>(),
        },
        "asks": item.asks.iter().map(ask_value).collect::<Vec<_>>(),
        "waits": if status == "waiting" || item.status == "waiting" {
            item.waits.iter().map(|wait| context.wait(wait)).collect::<Vec<_>>()
        } else {
            Vec::new()
        },
        "threads": item.threads.iter().map(|thread| thread_value(thread, context.thread(thread))).collect::<Vec<_>>(),
        "history": item.history.iter().rev().take(history).rev().collect::<Vec<_>>(),
    });
    // Every wait it had is over and one was an event that has happened: delivered, for the
    // coordinator to check and finish (`idle_notice`).
    if item.status == "waiting" && status != "waiting" {
        let delivered: Vec<String> = item
            .waits
            .iter()
            .map(|wait| context.wait(wait))
            .filter_map(|wait| {
                let name = wait["delivered"].as_str()?;
                Some(format!("{}: {name}", text(&wait, "event")))
            })
            .collect();
        if !delivered.is_empty() {
            out["delivered"] = json!(delivered.join(", "));
        }
    }
    if let Some(reason) = recheck_reason(item, status, context, Utc::now()) {
        out["recheck"] = json!(reason);
    }
    if let Some(hint) = quiet_hint(&item.threads, status, context) {
        out["quiet"] = json!(hint);
    }
    // A Later kind says only what kind of Later it is.
    if status == "later" && !item.reason.is_empty() {
        out["reason"] = json!(item.reason);
    }
    for (key, value) in [
        ("release", &item.release),
        ("link", &item.link),
        ("tag", &item.tag),
    ] {
        if !value.is_empty() {
            out[key] = json!(value);
        }
    }
    if item.untested {
        out["untested"] = json!(true);
    }
    // The user said Done on it and the coordinator has not changed it since: its cards show Done
    // as given (gx-chat-core `work_panel`).
    if said_done(item) {
        out["saidDone"] = json!(true);
    }
    // Its link as the board showed it: a PR with its checks, an issue, a ticket or a file.
    if !item.link.is_empty() {
        super::board::note_item_github_link(&item.link);
        if let Some(pr) = super::board::item_pr(&item.link) {
            out["links"] = json!([{
                "kind": "pr",
                "label": format!("#{}", pr["number"]),
                "state": pr["state"],
                "checks": pr.get("checks").cloned().unwrap_or(Value::Null),
                "url": item.link,
            }]);
            out["pr"] = pr;
        } else {
            out["links"] = json!([super::board::item_link(&item.link)]);
        }
    }
    if item.for_thread {
        out["forThread"] = json!(true);
    }
    if let Some(at) = &item.finished_at {
        out["finishedAt"] = json!(at);
    }
    if let Some(at) = &item.dropped_at {
        out["droppedAt"] = json!(at);
    }
    out
}

/// When the coordinator last looked at the item: its latest change by any item command (an edit
/// with no change too), unless that change was the user's answer.
fn checked_at(item: &WorkItem) -> &str {
    match item.history.last() {
        Some(event)
            if matches!(event.kind.as_str(), "answer" | "settle" | "moved")
                && event.at == item.updated_at =>
        {
            coordinator_changed_at(item)
        }
        _ => item.updated_at.as_str(),
    }
}

/// An `--until` that is a date or time still ahead: the wait is expected later, not forgotten.
fn until_ahead(until: &str, now: DateTime<Utc>) -> bool {
    let until = until.trim();
    if let Some(at) = parse_time(until) {
        return at > now;
    }
    NaiveDate::parse_from_str(until, "%Y-%m-%d")
        .is_ok_and(|day| day >= now.with_timezone(&Local).date_naive())
}

/// What the coordinator is told, and the user is not, about an In progress item whose thread is
/// quiet after Ghostex's nudge (`stuck` on its row): "G9dxg Title (quiet)". The user sees In
/// progress; `item list` and `status` print this for the coordinator to heal it.
fn quiet_hint(threads: &[WorkThread], status: &str, context: &WorkContext<'_>) -> Option<String> {
    if status != "moving" {
        return None;
    }
    let quiet: Vec<String> = threads
        .iter()
        .filter_map(|thread| context.thread(thread))
        .filter(|row| row["stuck"] == true)
        .map(|row| format!("{} {}", text(row, "sessionId"), text(row, "title")))
        .collect();
    (!quiet.is_empty()).then(|| quiet.join("; "))
}

/// Why the coordinator should look at the item again, or `None`: Waiting for
/// `RECHECK_WAITING_DAYS` with no item command since (and no `--until` still ahead), or Later
/// with every thread done and no change since they finished. Any item command clears it.
///
/// CDXC:Coordinators 2026-11-15 DECISION:
/// User picked B on the stale-board options page: the end-of-turn check looked only at In progress
/// and Not being worked on, and saying what an item waits on was a way out of it, so Waiting and
/// Later items were never looked at again (W454 and W461 sat in Later a day after their threads
/// finished). `item list` and `status` now name these until the coordinator rechecks them.
fn recheck_reason(
    item: &WorkItem,
    status: &str,
    context: &WorkContext<'_>,
    now: DateTime<Utc>,
) -> Option<String> {
    if item.for_thread {
        return None;
    }
    let checked = parse_time(checked_at(item))?;
    match status {
        "waiting" => {
            let days = now.signed_duration_since(checked).num_days();
            if days < RECHECK_WAITING_DAYS
                || item.waits.iter().any(|wait| until_ahead(&wait.until, now))
            {
                return None;
            }
            Some(format!("Waiting {days}d with no recheck"))
        }
        "later" => {
            let rows: Vec<&Value> = item
                .threads
                .iter()
                .filter_map(|thread| context.thread(thread))
                .collect();
            if rows.is_empty() || !rows.iter().all(|row| row["state"] == "done") {
                return None;
            }
            let last_active = rows
                .iter()
                .filter_map(|row| parse_time(row["activeAt"].as_str().unwrap_or_default()))
                .max();
            let rechecked = match last_active {
                Some(at) => checked > at,
                // No time to compare: rechecked once the coordinator did more than add it and
                // attach its threads.
                None => item
                    .history
                    .iter()
                    .rev()
                    .find(|event| !matches!(event.kind.as_str(), "answer" | "settle" | "moved"))
                    .is_some_and(|event| !matches!(event.kind.as_str(), "add" | "thread")),
            };
            (!rechecked).then(|| "Later, and its threads are done".to_string())
        }
        _ => None,
    }
}

fn recent(at: &str, now: DateTime<Utc>, days: i64) -> bool {
    DateTime::parse_from_rfc3339(at).is_ok_and(|at| now.signed_duration_since(at).num_days() < days)
}

fn local_day(at: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&Local).date_naive())
}

/// Every thread on no item, as a row of its own so nothing working goes unseen: its status from
/// its state, as the board placed it (board.rs `thread_row`). A dropped item hides its threads
/// until they work or wait on the user again.
fn loose_threads(threads: &[Value], items: &[WorkItem], now: DateTime<Utc>) -> Vec<Value> {
    let today = now.with_timezone(&Local).date_naive();
    threads
        .iter()
        .filter_map(|row| {
            let (project, session) = (text(row, "projectId"), text(row, "sessionId"));
            let holder = items
                .iter()
                .find(|item| item.has_thread(&project, &session));
            if let Some(holder) = holder {
                let live = matches!(text(row, "state").as_str(), "working" | "waiting");
                let dropped = holder.dropped_at.as_deref();
                let moved_since = dropped.is_some_and(|at| text(row, "activeAt").as_str() > at);
                if dropped.is_none() || !(live || moved_since) {
                    return None;
                }
            }
            let board = super::board::thread_row(row, None, today, now)?;
            let status = match board["group"].as_str() {
                Some("needs") => "needs",
                Some("blocked") => "blocked",
                Some("moving") => "moving",
                Some("hold") => "waiting",
                Some("done") => "finished",
                _ => "later",
            };
            let mut out = json!({
                "key": format!("thread:{project}:{session}"),
                "kind": "thread",
                "ref": session,
                "title": text(row, "title"),
                "status": status,
                "owner": if matches!(status, "needs" | "blocked") {
                    json!({ "kind": "user", "label": "You" })
                } else {
                    json!({ "kind": "thread", "label": text(row, "title"), "ref": session })
                },
                "now": board["detail"],
                "at": board["at"],
                "threads": [thread_value(&WorkThread { server_id: text(row, "serverId"), project_id: project.clone(), session_id: session.clone() }, Some(row))],
            });
            for key in ["flag", "flagTone", "reason", "waiting"] {
                if let Some(value) = board.get(key) {
                    out[key] = value.clone();
                }
            }
            Some(out)
        })
        .collect()
}

/// The coordinator's work: every item (`all`) or the chat's copy (live items, and the finished and
/// dropped ones of the last few days with a short history), and every thread on no item.
pub fn work_value(
    threads: &[Value],
    items: &[WorkItem],
    all: bool,
    now: DateTime<Utc>,
    idle_floor: Option<DateTime<Utc>>,
    coordinator: Option<&SessionKey>,
) -> Value {
    let context = WorkContext::new(threads, items)
        .with_idle_floor(idle_floor)
        .with_coordinator(coordinator);
    let today = now.with_timezone(&Local).date_naive();
    let mut rows: Vec<Value> = items
        .iter()
        .filter(|item| {
            all || item
                .finished_at
                .as_deref()
                .or(item.dropped_at.as_deref())
                .is_none_or(|at| recent(at, now, PANEL_RECENT_DAYS))
        })
        .map(|item| {
            let mut row = item_value(
                item,
                &context,
                if all { HISTORY_KEEP } else { PANEL_HISTORY },
            );
            if row["status"] == "finished" {
                let at = item
                    .finished_at
                    .clone()
                    .unwrap_or_else(|| item.updated_at.clone());
                row["today"] = json!(local_day(&at) == Some(today));
            }
            row
        })
        .collect();
    rows.extend(loose_threads(threads, items, now));
    let mut counts = Map::new();
    for status in WORK_STATUSES {
        let count = rows
            .iter()
            .filter(|row| {
                row["status"] == status && (status != "finished" || row["today"] != false)
            })
            .count();
        counts.insert(status.to_string(), json!(count));
    }
    // The asks waiting on the user, each naming its item, for Needs you and Blocked.
    let asks: Vec<Value> = items
        .iter()
        // A finished item's open question or task is still the user's (`withdraw_open_asks`).
        .filter(|item| item.dropped_at.is_none())
        .flat_map(|item| {
            item.asks.iter().filter(|ask| ask.is_open()).map(|ask| {
                let mut out = ask_value(ask);
                out["item"] = json!(item.reference());
                out["itemTitle"] = json!(item.title);
                out
            })
        })
        .collect();
    json!({ "items": rows, "asks": asks, "counts": counts })
}

/// What `item list` and `status` tell the coordinator about its items: the delivered ones (what
/// they waited on has happened), the ones nobody is on (`idle`) and the ones to recheck (Waiting a
/// while, or Later with their threads done), or `None` when there are none. It only asks: Ghostex
/// never finishes an item itself.
pub fn idle_notice(work: &Value) -> Option<String> {
    let rows: Vec<&Value> = work["items"].as_array().into_iter().flatten().collect();
    let named = |item: &Value| format!("{} {}", text(item, "ref"), text(item, "title"));
    let first = |list: &[String]| list[0].split(' ').next().unwrap_or("W1").to_string();
    let mut parts = Vec::new();
    let delivered: Vec<String> = rows
        .iter()
        .filter(|item| !text(item, "delivered").is_empty())
        .map(|item| format!("{} ({})", named(item), text(item, "delivered")))
        .collect();
    if !delivered.is_empty() {
        parts.push(format!(
            "Delivered (what it waited on has happened): {}. Check it is in what shipped and finish it: `ghostex coordinator item done {} \"<what was done; where it shipped>\"`, or say what it waits on now (`item wait`).",
            delivered.join("; "),
            first(&delivered)
        ));
    }
    let idle: Vec<String> = rows
        .iter()
        .filter(|item| item["status"] == "idle" && text(item, "delivered").is_empty())
        .map(|item| named(item))
        .collect();
    if !idle.is_empty() {
        let first = first(&idle);
        parts.push(format!(
            "Not being worked on (In progress, but no thread is on it and you have not changed it since your last turn): {}. Finished? Mark it done: `ghostex coordinator item done {first} \"<what was done>\"`. Otherwise say what it waits on (`item wait {first} --on ...`), ask the user (`item ask`), or say where it stands now (`item now`).",
            idle.join("; ")
        ));
    }
    let recheck: Vec<String> = rows
        .iter()
        .filter(|item| !text(item, "recheck").is_empty())
        .map(|item| format!("{} ({})", named(item), text(item, "recheck")))
        .collect();
    if !recheck.is_empty() {
        let first = first(&recheck);
        parts.push(format!(
            "Recheck: {}. Has it happened or shipped? Finish it (`item done {first} \"<line>\"`), say what it waits on now (`item wait {first} --on ...`), ask the user (`item ask`) or park it (`item edit {first} --status parked`). Any item command on it counts as a recheck.",
            recheck.join("; ")
        ));
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// [`idle_notice`] for one coordinator, read from its items and its panel's thread rows.
pub fn idle_notice_for(db: &Connection, key: &SessionKey) -> Option<String> {
    load_deliveries(db);
    let items = list_work_items(db, key).ok()?;
    let threads = super::panel::cached_thread_rows(key);
    let now = Utc::now();
    let floor = super::turns::idle_floor_for(db, key, now);
    idle_notice(&work_value(
        &threads,
        &items,
        false,
        now,
        Some(floor),
        Some(key),
    ))
}

fn param_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn param_list(params: &Map<String, Value>, key: &str) -> Vec<String> {
    match params.get(key) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(value)) if !value.trim().is_empty() => vec![value.trim().to_string()],
        _ => Vec::new(),
    }
}

fn bad(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message.into())
}

/// The threads a request attaches: `threads` (a list of `{serverId, projectId, sessionId}`) or one
/// `threadProjectId`/`threadSessionId`.
fn param_threads(params: &Map<String, Value>) -> Vec<WorkThread> {
    let mut threads: Vec<WorkThread> = params
        .get("threads")
        .and_then(|value| serde_json::from_value::<Vec<WorkThread>>(value.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|thread| !thread.session_id.is_empty())
        .collect();
    if let (Some(project_id), Some(session_id)) = (
        param_text(params, "threadProjectId"),
        param_text(params, "threadSessionId"),
    ) {
        threads.push(WorkThread {
            server_id: param_text(params, "threadServerId").unwrap_or_default(),
            project_id,
            session_id,
        });
    }
    threads
}

fn attach_threads(item: &mut WorkItem, threads: Vec<WorkThread>, at: &str) {
    for thread in threads {
        if item.has_thread(&thread.project_id, &thread.session_id) {
            continue;
        }
        let words = format!("Thread {} attached", thread.session_id);
        item.threads.push(thread);
        item.record("thread", words, at);
    }
}

/// The highest question tag the coordinator's items use, so a new ask takes the next one.
fn highest_ask_tag(items: &[WorkItem]) -> u64 {
    items
        .iter()
        .flat_map(|item| item.asks.iter())
        .filter_map(|ask| ask.tag.strip_prefix('Q')?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

/// A dropped item's open asks end with it. A finished item keeps them: a question or a task the
/// user still has stays under Needs you until they answer it or mark it done, or the coordinator
/// withdraws it (`item ask --withdraw`).
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User, through the coordinator, of "item done" withdrawing a "Remove the test app from Paired
/// devices" task: item done must not silently withdraw an open ask that is a task for the user;
/// it stays under Needs you until he marks it done. Then the same for a question: "Post the
/// thank-you note on PR #537?" showed as Withdrawn after its item was done, and "a question that
/// still waits for the user must outlive its item just like a task does". Only dropping the work
/// withdraws its asks.
fn withdraw_open_asks(item: &mut WorkItem, at: &str) {
    for ask in item.asks.iter_mut().filter(|ask| ask.is_open()) {
        ask.settled_at = Some(at.to_string());
        ask.withdrawn = true;
    }
}

/// After an ask settles: an item that was only waiting on the user (Needs you by the
/// coordinator's word) is finished once nothing is asked any more, as an answered board card was.
fn finish_if_only_asking(item: &mut WorkItem, at: &str) {
    if item.status == "needs"
        && item.threads.is_empty()
        && !item.asks.is_empty()
        && item.asks.iter().all(|ask| !ask.is_open())
    {
        item.status = "finished".to_string();
        item.finished_at = Some(at.to_string());
        item.record("done", "Nothing more is asked", at);
    }
}

/// Whether the user said Done on the item (it had no open question) and the coordinator has not
/// changed it since: its latest change is that answer.
fn said_done(item: &WorkItem) -> bool {
    item.history.last().is_some_and(|event| {
        event.kind == "answer" && event.text == "Done" && event.at == item.updated_at
    })
}

/// A coordinator's idle floor (turns.rs). An ordinary session's items have none: its turns are
/// not followed, so its In progress items stay In progress, as its Work panel shows them
/// (session_work.rs builds them without one).
fn coordinator_idle_floor(
    db: &Connection,
    key: &SessionKey,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    super::records::read_coordinator(db, &key.0, &key.1)
        .ok()
        .flatten()
        .map(|_| super::turns::idle_floor_for(db, key, now))
}

/// What a request for an item answers: the item as clients and the CLI read it.
fn answer(item: &WorkItem, threads: &[Value], db: &Connection, key: &SessionKey) -> Value {
    let items = list_work_items(db, key).unwrap_or_default();
    let floor = coordinator_idle_floor(db, key, Utc::now());
    let context = WorkContext::new(threads, &items)
        .with_idle_floor(floor)
        .with_coordinator(Some(key));
    item_value(item, &context, PANEL_HISTORY)
}

/// `op: delivered`: records that `kind` (release, ota or rebuild) happened now for the
/// coordinator's project, named `name` ("v11.7.1"), and answers with this coordinator's items it
/// delivered (`item delivered`, run by the release flow).
fn delivered_request(
    db: &Connection,
    key: &SessionKey,
    threads: &[Value],
    params: &Map<String, Value>,
    now: &str,
) -> Result<(Value, bool), DomainStateError> {
    let word = param_text(params, "event").unwrap_or_default();
    let kind = wait_event(&word).ok_or_else(|| {
        bad(format!(
            "Name the event: release, ota or rebuild (not \"{word}\")."
        ))
    })?;
    let name = param_text(params, "name").unwrap_or_default();
    record_delivery(db, &key.0, kind, &name, now)?;
    let items = list_work_items(db, key)?;
    let floor = super::turns::idle_floor_for(db, key, Utc::now());
    let work = work_value(threads, &items, false, Utc::now(), Some(floor), Some(key));
    let delivered: Vec<Value> = work["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| text(item, "delivered").contains(&format!("{kind}:")))
        .map(|item| json!({ "ref": item["ref"], "title": item["title"], "now": item["now"] }))
        .collect();
    Ok((
        json!({
            "event": kind,
            "name": name,
            "at": now,
            "items": delivered,
            "notice": idle_notice(&work),
        }),
        true,
    ))
}

/// `/api/coordinatorWork`. `op` is `list` (default; `all` for finished and dropped too), `delivered`
/// (an event happened: `event`, `name`), `add`,
/// `now`, `step`, `ask`, `wait`, `done`, `drop`, `restore`, `edit` (title, link, release, untested,
/// status, reason, threads), `answer` (an ask's one-click answer, recorded once), `settle` (an ask
/// done or withdrawn) or `tag` (the user's tag). `ask_floor` is the highest question tag the
/// coordinator's chat used, so a new ask's tag follows it, and `chat_tags` the tags its chat asked
/// (uppercased) with whether each is settled: an ask's own `--tag` may name one still open there,
/// never a settled one. Returns the answer and whether the work changed.
pub fn handle_work_request(
    db: &Connection,
    key: &SessionKey,
    threads: &[Value],
    params: &Map<String, Value>,
    ask_floor: u64,
    chat_tags: &std::collections::HashMap<String, bool>,
) -> Result<(Value, bool), DomainStateError> {
    let op = param_text(params, "op").unwrap_or_else(|| "list".to_string());
    let now = now_iso();
    load_deliveries(db);
    if op == "delivered" {
        return delivered_request(db, key, threads, params, &now);
    }
    if op == "list" {
        let items = list_work_items(db, key)?;
        let all = params.get("all").and_then(Value::as_bool) == Some(true);
        let now = Utc::now();
        let floor = coordinator_idle_floor(db, key, now);
        let mut work = work_value(threads, &items, all, now, floor, Some(key));
        // The notice is worded for a coordinator; an ordinary session's agent is not told it.
        if floor.is_some() {
            if let Some(notice) = idle_notice(&work) {
                work["idleNotice"] = json!(notice);
            }
        }
        return Ok((work, false));
    }
    if op == "add" {
        let title = param_text(params, "title").ok_or_else(|| bad("An item needs a title."))?;
        let mut item = WorkItem {
            number: next_item_number(db, key)?,
            title,
            status: "later".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            ..WorkItem::default()
        };
        item.record("add", item.title.clone(), &now);
        if let Some(line) = param_text(params, "now") {
            if let Some(refusal) = super::user_handoff::now_refusal("W<n>", &line, true) {
                return Err(bad(refusal));
            }
            item.now = line;
            item.status = "moving".to_string();
        }
        apply_edit(&mut item, params, &now)?;
        attach_threads(&mut item, param_threads(params), &now);
        write_work_item(db, key, &item)?;
        prune(db, key)?;
        return Ok((answer(&item, threads, db, key), true));
    }
    let reference = param_text(params, "item").ok_or_else(|| bad("Name the item (W7)."))?;
    let number = item_number_of(&reference)
        .ok_or_else(|| bad(format!("{reference} is not an item: name one as W7.")))?;
    let mut item = read_item(db, key, number)?
        .ok_or_else(|| bad(format!("This session has no item W{number}.")))?;
    let name = item.reference();
    if item.dropped_at.is_some() && !matches!(op.as_str(), "restore" | "tag") {
        return Err(bad(format!(
            "{name} was dropped. ghostex work item restore {name} brings it back."
        )));
    }
    match op.as_str() {
        "now" => {
            let line = param_text(params, "text").ok_or_else(|| bad("Say where it stands."))?;
            if let Some(refusal) = super::user_handoff::now_refusal(&name, &line, false) {
                return Err(bad(refusal));
            }
            item.now = line.clone();
            item.status = "moving".to_string();
            item.finished_at = None;
            item.waits.clear();
            attach_threads(&mut item, param_threads(params), &now);
            item.record("now", line, &now);
        }
        "step" => {
            let words = param_text(params, "text").ok_or_else(|| bad("Name the step."))?;
            let done = params.get("done").and_then(Value::as_bool) == Some(true);
            let chore = params.get("chore").and_then(Value::as_bool) == Some(true);
            match item
                .steps
                .iter_mut()
                .find(|step| step.text.eq_ignore_ascii_case(&words))
            {
                Some(step) => {
                    step.done = done;
                    step.at = now.clone();
                }
                None => item.steps.push(WorkStep {
                    text: words.clone(),
                    done,
                    at: now.clone(),
                    chore,
                }),
            }
            // A chore is kept, never shown (Q2094 "hidden"): no history line for the cards.
            if chore {
                item.updated_at = now.clone();
            } else {
                item.record("step", if done { format!("Done: {words}") } else { words }, &now);
            }
        }
        "ask" => {
            let words = param_text(params, "text").ok_or_else(|| bad("Say what you ask."))?;
            let tag = match param_text(params, "tag") {
                Some(tag) => {
                    let tag = question_tag(&tag)
                        .ok_or_else(|| bad(format!("{tag} is not a question tag: use Q12.")))?;
                    // CDXC:Coordinators 2026-11-12 WHY:
                    // Issue #579: an ask reusing a tag the chat had already settled was settled at once by that tag's old answer ("has no open ask"), so the user never saw it. `--tag` still puts a question the chat asked and is waiting on onto the board; a settled tag, or one another item already asked, is refused with the next free one.
                    let items = list_work_items(db, key)?;
                    let used_on_item = items.iter().any(|other| {
                        other.asks.iter().any(|ask| {
                            ask.tag.eq_ignore_ascii_case(&tag)
                                && !(other.number == item.number && ask.is_open())
                        })
                    });
                    if used_on_item || chat_tags.get(&tag) == Some(&true) {
                        let next = highest_ask_tag(&items).max(ask_floor) + 1;
                        return Err(bad(format!(
                            "{tag} was already asked and settled in this conversation, and a tag means one question. Leave out --tag to take the next free one (Q{next})."
                        )));
                    }
                    tag
                }
                None => {
                    let items = list_work_items(db, key)?;
                    format!("Q{}", highest_ask_tag(&items).max(ask_floor) + 1)
                }
            };
            if item.asks.iter().any(|ask| ask.is_open() && ask.tag == tag) {
                return Err(bad(format!("{name} already asks {tag}.")));
            }
            item.asks.push(WorkAsk {
                tag: tag.clone(),
                text: words.clone(),
                detail: param_text(params, "detail").unwrap_or_default(),
                answers: param_text(params, "answers").unwrap_or_default(),
                holds: params.get("holds").and_then(Value::as_bool) == Some(true),
                steps: param_text(params, "steps").unwrap_or_default(),
                asked_at: now.clone(),
                command: param_text(params, "command").unwrap_or_default(),
                // Set only by gxserver itself (session_needs_you/thread.rs), never by the CLI's
                // own params: `fromThread` is the thread whose to-do this ask is.
                from_thread: params
                    .get("fromThread")
                    .and_then(|thread| serde_json::from_value(thread.clone()).ok()),
                ..WorkAsk::default()
            });
            item.finished_at = None;
            if item.status == "finished" {
                item.status = "needs".to_string();
            }
            item.record("ask", format!("{tag}: {words}"), &now);
        }
        "wait" => {
            let on = param_list(params, "on");
            if on.is_empty() {
                return Err(bad("Say what it waits on: --on W12, --on Q12, --on release, --on ota, --on rebuild or --on \"words\"."));
            }
            if let Some(refusal) = super::user_handoff::wait_refusal(
                &name,
                &on,
                param_text(params, "text").as_deref(),
            ) {
                return Err(bad(refusal));
            }
            let until = param_text(params, "until").unwrap_or_default();
            for wait in &on {
                if item_number_of(wait) == Some(item.number) {
                    return Err(bad(format!("{name} cannot wait on itself.")));
                }
            }
            item.waits = on
                .iter()
                .map(|on| WorkWait {
                    on: question_tag(on)
                        .or_else(|| item_number_of(on).filter(|_| on.trim().len() < 12).map(|number| format!("W{number}")))
                        .or_else(|| wait_event(on).map(str::to_string))
                        .unwrap_or_else(|| on.clone()),
                    until: until.clone(),
                    since: now.clone(),
                })
                .collect();
            item.status = "waiting".to_string();
            item.finished_at = None;
            if let Some(line) = param_text(params, "text") {
                item.now = line;
            }
            // Named as the user reads them: another item by its title, an ask by its words.
            let others = list_work_items(db, key)?;
            let words = item
                .waits
                .iter()
                .map(|wait| {
                    let named = item_number_of(&wait.on)
                        .filter(|_| wait.on.starts_with('W'))
                        .and_then(|number| others.iter().find(|other| other.number == number))
                        .map(|other| other.title.clone())
                        .or_else(|| {
                            others.iter().flat_map(|other| other.asks.iter()).find_map(|ask| {
                                (!ask.tag.is_empty() && ask.tag == wait.on).then(|| ask.text.clone())
                            })
                        })
                        .or_else(|| {
                            wait_event(&wait.on).map(|kind| wait_event_words(kind).to_string())
                        });
                    named.unwrap_or_else(|| wait.on.clone())
                })
                .collect::<Vec<_>>()
                .join(", ");
            item.record("wait", format!("Waits on {words}"), &now);
        }
        "done" => {
            if let Some(line) = param_text(params, "text") {
                item.now = line;
            }
            item.status = "finished".to_string();
            item.finished_at = Some(now.clone());
            item.waits.clear();
            // Its open asks stay the user's (`withdraw_open_asks`).
            let words = item.now.clone();
            item.record("done", words, &now);
        }
        "drop" => {
            item.dropped_at = Some(now.clone());
            withdraw_open_asks(&mut item, &now);
            item.record("drop", "", &now);
        }
        "restore" => {
            if item.dropped_at.is_none() {
                return Err(bad(format!("{name} was not dropped.")));
            }
            item.dropped_at = None;
            item.record("restore", "", &now);
        }
        "edit" => {
            apply_edit(&mut item, params, &now)?;
            attach_threads(&mut item, param_threads(params), &now);
            for detached in param_list(params, "detach") {
                item.threads
                    .retain(|thread| !thread.session_id.eq_ignore_ascii_case(&detached));
            }
            item.updated_at = now.clone();
        }
        // A card's one-click answer, recorded once: the client sends the coordinator its message
        // only after this succeeds, so a second click, another client or a reload cannot send it
        // again.
        "answer" => {
            let given = param_text(params, "answer").ok_or_else(|| bad("Say the answer."))?;
            let wanted = param_text(params, "ask").and_then(|tag| question_tag(&tag));
            // The Done on an item that waits on the user with no question (the client then tells
            // the coordinator "W7: Done"): kept as said until the coordinator next changes the
            // item (`said_done`), so a second click or another client cannot send it again.
            //
            // CDXC:Coordinators 2026-11-10 DECISION:
            // User, of a Needs you item with no question whose card had only Show in chat and
            // Mention: "why don't i see a button to say All good on this task???? please fix for
            // always". Every Needs you item with no open question offers Done.
            if wanted.is_none() && given == "Done" && !item.asks.iter().any(WorkAsk::is_open) {
                if said_done(&item) {
                    return Err(bad(format!("{name} was already answered: Done.")));
                }
                item.record("answer", "Done", &now);
                write_work_item(db, key, &item)?;
                prune(db, key)?;
                return Ok((answer(&item, threads, db, key), true));
            }
            // A task's Mark done on its card ("Done"): the ask is done, recorded once.
            let task = item.asks.iter().position(|ask| {
                wanted.as_ref().is_some_and(|tag| ask.tag == *tag)
                    && ask.answers.is_empty()
                    && given == "Done"
            });
            if let Some(index) = task {
                let ask = &mut item.asks[index];
                if !ask.is_open() {
                    return Err(bad(format!("{name} was already answered: {}.", ask.answered)));
                }
                ask.answered = given.clone();
                ask.settled_at = Some(now.clone());
                let words = format!("{} done", ask.tag);
                item.record("answer", words, &now);
                finish_if_only_asking(&mut item, &now);
                write_work_item(db, key, &item)?;
                prune(db, key)?;
                return Ok((answer(&item, threads, db, key), true));
            }
            let ask = item
                .asks
                .iter_mut()
                .filter(|ask| wanted.as_ref().is_none_or(|tag| ask.tag == *tag))
                .find(|ask| {
                    parse_answers(&ask.answers)
                        .iter()
                        .any(|choice| choice["label"] == given.as_str())
                })
                .ok_or_else(|| bad(format!("{name} offers no answer {given}.")))?;
            if !ask.answered.is_empty() || !ask.is_open() {
                return Err(bad(format!(
                    "{name} was already answered: {}.",
                    if ask.answered.is_empty() { "withdrawn" } else { ask.answered.as_str() }
                )));
            }
            ask.answered = given.clone();
            ask.settled_at = Some(now.clone());
            let tag = ask.tag.clone();
            item.record("answer", format!("{tag}: {given}").trim_start_matches(": ").to_string(), &now);
            finish_if_only_asking(&mut item, &now);
        }
        // The user snoozed an open ask for `hours` from its card (1, 6 or 24), or cancelled it:
        // cancelled, it is withdrawn for good and the client tells the coordinator so, as it sends
        // an answer (gx-chat-core `coordinator_board::answer`).
        //
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User: "for any ask i should be able to click 'Snooze' and snooze it for 1h 6h 24h or
        // cancel it". A snoozed ask leaves the dock and the Needs you count until its end and comes
        // back by itself (`snoozedUntil` is read against the clock); the Work panel keeps it under
        // a quiet "Snoozed until".
        // The user's own words for an open ask (the dock's Reply): recorded once as its answer,
        // whatever answers it offers; the client then tells the coordinator "W7 Q12: words".
        //
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User: "whenever u send me a question card (needs you) ALWAYS have a button i can click to
        // reply to the question with my own text", then "WE MUST HAVE REPLY BUTTON FOR NEEDS YOU
        // STUFF".
        "reply" => {
            let given = param_text(params, "answer").ok_or_else(|| bad("Say the reply."))?;
            let tag = param_text(params, "ask").and_then(|tag| question_tag(&tag));
            // The user's words on an item with no open question ("W7: words"): kept in its
            // history; the item stays where it is for the coordinator to act on.
            if tag.is_none() && !item.asks.iter().any(WorkAsk::is_open) {
                let words: String = given.chars().take(120).collect();
                item.record("reply", words, &now);
                write_work_item(db, key, &item)?;
                prune(db, key)?;
                return Ok((answer(&item, threads, db, key), true));
            }
            let ask = item
                .asks
                .iter_mut()
                .find(|ask| tag.as_ref().is_none_or(|tag| ask.tag == *tag))
                .ok_or_else(|| bad(format!("{name} has no ask{}.", tag.as_ref().map(|tag| format!(" {tag}")).unwrap_or_default())))?;
            if !ask.is_open() {
                return Err(bad(format!("{name} was already answered: {}.", if ask.answered.is_empty() { "withdrawn" } else { ask.answered.as_str() })));
            }
            ask.answered = given.chars().take(2_000).collect();
            ask.settled_at = Some(now.clone());
            let words = format!("{}: {}", ask.tag, ask.answered.chars().take(120).collect::<String>());
            item.record("answer", words, &now);
            finish_if_only_asking(&mut item, &now);
        }
        "snooze" | "cancel" | "unsnooze" => {
            let tag = param_text(params, "ask").and_then(|tag| question_tag(&tag));
            let reference = item.reference();
            let ask = item
                .asks
                .iter_mut()
                .filter(|ask| ask.is_open())
                .find(|ask| tag.as_ref().is_none_or(|tag| ask.tag == *tag))
                .ok_or_else(|| bad(format!("{name} has no open ask{}.", tag.as_ref().map(|tag| format!(" {tag}")).unwrap_or_default())))?;
            if op == "cancel" {
                ask.settled_at = Some(now.clone());
                ask.withdrawn = true;
                // Dismiss sends the coordinator nothing now: its next message says so
                // (session_chat_dismissed.rs, where the user's decision lives).
                let title: String = ask.text.split_whitespace().collect::<Vec<_>>().join(" ");
                let title: String = title.replace('"', "'").chars().take(80).collect();
                let name = format!("{reference} {}", ask.tag);
                let name = if title.is_empty() { name } else { format!("{name} \"{title}\"") };
                crate::session_chat_dismissed::record(db, &key.0, &key.1, name.trim());
                let words = format!("{} dismissed by the user", ask.tag);
                item.record("settle", words.trim().to_string(), &now);
                finish_if_only_asking(&mut item, &now);
            } else if op == "unsnooze" {
                // Unsnooze from the dock's Snoozed row: back under Needs you now.
                ask.snoozed_until = None;
            } else {
                let hours = params
                    .get("hours")
                    .and_then(Value::as_u64)
                    .filter(|hours| (1..=24 * 14).contains(hours))
                    .ok_or_else(|| bad("Say for how many hours (1 to 336)."))?;
                ask.snoozed_until = Some(
                    (Utc::now() + chrono::Duration::hours(hours as i64))
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                );
            }
        }
        // An ask the user did (a task's Mark done, `item ask W7 --done Q12`) or the coordinator
        // withdrew (`--withdraw`).
        "settle" => {
            let tag = param_text(params, "ask").and_then(|tag| question_tag(&tag));
            let withdraw = params.get("withdraw").and_then(Value::as_bool) == Some(true);
            let ask = item
                .asks
                .iter_mut()
                .filter(|ask| ask.is_open())
                .find(|ask| tag.as_ref().is_none_or(|tag| ask.tag == *tag))
                .ok_or_else(|| bad(format!("{name} has no open ask{}.", tag.as_ref().map(|tag| format!(" {tag}")).unwrap_or_default())))?;
            ask.settled_at = Some(now.clone());
            if withdraw {
                ask.withdrawn = true;
            } else if ask.answered.is_empty() {
                ask.answered = "Done".to_string();
            }
            let words = format!(
                "{} {}",
                ask.tag,
                if withdraw { "withdrawn" } else { "done" }
            );
            item.record("settle", words.trim().to_string(), &now);
            finish_if_only_asking(&mut item, &now);
        }
        "tag" => {
            let tag = param_text(params, "tag").unwrap_or_default();
            if !tag.is_empty()
                && ghostex_gx_core::tag_choice(&tag, None).is_none()
                && !crate::custom_session_tags::is_custom_session_tag_id(&tag)
            {
                return Err(bad(format!("Unknown tag {tag}.")));
            }
            // A tag is the user's label, not the work moving: the item keeps its place and age.
            item.tag = tag;
        }
        other => {
            return Err(bad(format!(
                "Unknown work request {other}. Use list, add, now, step, ask, wait, done, drop, restore, edit, answer, settle or tag."
            )))
        }
    }
    write_work_item(db, key, &item)?;
    prune(db, key)?;
    Ok((answer(&item, threads, db, key), true))
}

/// The fields `add` and `edit` set: title, link, release, untested, status and its Later kind.
fn apply_edit(
    item: &mut WorkItem,
    params: &Map<String, Value>,
    at: &str,
) -> Result<(), DomainStateError> {
    if let Some(word) = param_text(params, "status") {
        let (status, reason) = parse_status(&word).ok_or_else(|| {
            bad(format!(
                "Unknown status {word}. Use needs-you, blocked, moving, waiting, later (or next, held, parked, backlog) or finished."
            ))
        })?;
        // Blocked is only an open ask that holds the work up: no ask, no Blocked.
        if status == "blocked" && !item.asks.iter().any(|ask| ask.is_open() && ask.holds) {
            return Err(bad(format!(
                "{} is not Blocked: Blocked means the user must act before it can go on. Ask them with `item ask {} \"<question>\" --holds`; work that is only quiet or waiting stays In progress (a thread that stopped is yours to heal) or Waiting (`item wait`).",
                item.reference(),
                item.reference()
            )));
        }
        if status == "finished" {
            item.finished_at = Some(at.to_string());
        } else {
            item.finished_at = None;
        }
        if status != item.status || reason != item.reason {
            item.status = status.to_string();
            item.reason = reason.to_string();
            item.record("status", status_heading(status), at);
        }
        if status != "waiting" {
            item.waits.clear();
        }
    }
    if let Some(reason) = param_text(params, "reason") {
        item.reason = match parse_status(&reason) {
            Some(("later", reason)) if !reason.is_empty() => reason.to_string(),
            _ => return Err(bad("A Later kind is next, held, parked or backlog.")),
        };
    }
    if let Some(title) = param_text(params, "title") {
        if title != item.title && !item.history.is_empty() {
            item.record("edit", format!("Renamed: {title}"), at);
        }
        item.title = title;
        item.for_thread = false;
    }
    for (key, field) in [
        ("link", &mut item.link),
        ("release", &mut item.release),
        ("note", &mut item.now),
    ] {
        if let Some(value) = params.get(key).and_then(Value::as_str) {
            *field = value.trim().to_string();
        }
    }
    if let Some(untested) = params.get("untested").and_then(Value::as_bool) {
        item.untested = untested;
    }
    // A release says the item is held for it.
    if !item.release.is_empty() && item.status == "later" && item.reason.is_empty() {
        item.reason = "held".to_string();
    }
    Ok(())
}

/// Settles the coordinator's open asks whose questions its chat has settled, and finishes the
/// Needs you items (moved from the board) that name a question once it is, and says whether any
/// changed. `settled` says of a tag whether its question was answered or withdrawn (`Some(true)`),
/// still waits (`Some(false)`), or was never asked in the coordinator's chat (`None`).
///
/// CDXC:Coordinators 2026-11-05 DECISION (carried over from the board):
/// A Discord user: "I've replied to a few inline questions in the coordinator response ... and the
/// questions pill disappeared from the composer, but the board still shows them". An ask (or a
/// Needs you item) that names a question's tag settles the way the question leaves the ? pill:
/// answered in the form, a one-click link or a page, withdrawn or asked again, and also answered
/// in plain words ("Q12: yes"), which the ledger reads from the user's messages
/// (server/src/session_chat_open_questions.rs).
pub fn settle_answered_items(
    db: &Connection,
    key: &SessionKey,
    settled: impl Fn(&str) -> Option<bool>,
) -> Result<bool, DomainStateError> {
    let is_settled = |tags: Vec<String>| {
        let asked: Vec<bool> = tags.iter().filter_map(|tag| settled(tag)).collect();
        !asked.is_empty() && !asked.contains(&false)
    };
    let mut changed = false;
    for mut item in list_work_items(db, key)? {
        // A finished item's open task still settles when the user answers it.
        if item.dropped_at.is_some() {
            continue;
        }
        let now = now_iso();
        let mut touched = false;
        for index in 0..item.asks.len() {
            let ask = &item.asks[index];
            if !ask.is_open() || !is_settled(item.question_tags(Some(ask))) {
                continue;
            }
            let ask = &mut item.asks[index];
            ask.settled_at = Some(now.clone());
            if ask.answered.is_empty() {
                ask.answered = "Answered in the chat".to_string();
            }
            let words = format!("{} answered", ask.tag);
            item.record("settle", words.trim().to_string(), &now);
            touched = true;
        }
        if touched {
            finish_if_only_asking(&mut item, &now);
        } else if item.status == "needs"
            && item.asks.is_empty()
            && item.threads.is_empty()
            && is_settled(item.question_tags(None))
        {
            item.status = "finished".to_string();
            item.finished_at = Some(now.clone());
            item.record("done", "Its question was answered", &now);
            touched = true;
        }
        if touched {
            write_work_item(db, key, &item)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// The "W7 Q12: words" answers in a message the user sent: each line that starts so begins an
/// answer, which runs to the next such line or the end, and each answer pill naming a work item
/// ask (`[<title>](answering:W7.Q12) words`). Words may be empty (a picture alone).
pub fn chat_replies(text: &str) -> Vec<(i64, String, String)> {
    let mut replies: Vec<(i64, String, String)> = Vec::new();
    for line in text.lines() {
        let opener = line.trim_start().split_once(':').and_then(|(head, rest)| {
            let (item, tag) = head.trim().split_once(' ')?;
            let number = item.strip_prefix(['W', 'w'])?.parse::<i64>().ok()?;
            Some((number, question_tag(tag)?, rest.trim().to_string()))
        });
        match (opener, replies.last_mut()) {
            (Some(reply), _) => replies.push(reply),
            (None, Some((_, _, words))) => {
                words.push('\n');
                words.push_str(line);
            }
            (None, None) => {}
        }
    }
    for (_, _, words) in &mut replies {
        *words = words.trim().to_string();
    }
    // A Reply in chat box's pill, `[<title>](answering:W7.Q12) words`, answers the same way
    // (gx-chat-core `questions::answer_pills`, CDXC:Coordinators 2026-11-16 DECISION there).
    for (pill, words) in ghostex_gx_chat_core::questions::answer_pills::answers_in(text) {
        if !pill.settles {
            continue;
        }
        let Some(number) = pill.item() else {
            continue;
        };
        for tag in pill.questions() {
            replies.push((number, tag, words.clone()));
        }
    }
    replies
}

/// Settles the open asks the user answered in a message that reached the coordinator ("W7 Q12:
/// words"), as the dock's Reply records them (`reply` above), and says whether any changed. An
/// answer already recorded (the dock's Send answer records before its message goes) is left as it
/// is.
///
/// CDXC:Coordinators 2026-11-11 WHY:
/// User, of a question he had answered with the dock's Reply: "i answered this question with a
/// reply, why do i still see it as needs you!!!". Enter in the Reply box is Add to prompt, which
/// puts "W218 Q2498: words" in the chat box; that message reached the coordinator, who takes the
/// prefix as recorded, but nothing recorded it, so Q2498 stayed open and in Needs you. A message
/// with that prefix now settles its ask whichever way it was sent (Add to prompt, typed, the
/// phone, the web), with words, a picture alone, or both.
pub fn settle_chat_replies(
    db: &Connection,
    key: &SessionKey,
    text: &str,
    pictures: usize,
) -> Result<bool, DomainStateError> {
    let mut changed = false;
    for (number, tag, words) in chat_replies(text) {
        let Some(mut item) = read_item(db, key, number)? else {
            continue;
        };
        let Some(ask) = item
            .asks
            .iter_mut()
            .find(|ask| ask.tag == tag && ask.is_open())
        else {
            continue;
        };
        let now = now_iso();
        ask.answered = if !words.is_empty() {
            words.chars().take(2_000).collect()
        } else if pictures > 0 {
            "Sent a picture".to_string()
        } else {
            "Answered in the chat".to_string()
        };
        ask.settled_at = Some(now.clone());
        let line = format!(
            "{tag}: {}",
            ask.answered.chars().take(120).collect::<String>()
        );
        item.record("answer", line, &now);
        finish_if_only_asking(&mut item, &now);
        write_work_item(db, key, &item)?;
        changed = true;
    }
    if changed {
        prune(db, key)?;
    }
    Ok(changed)
}

/// Whether the coordinator has an open ask or Needs you item naming a tag, so its ledger is worth
/// asking about tags (`settle_answered_items`).
pub fn has_question_items(db: &Connection, key: &SessionKey) -> bool {
    list_work_items(db, key).is_ok_and(|items| {
        items.iter().any(|item| {
            item.finished_at.is_none()
                && item.dropped_at.is_none()
                && (item
                    .asks
                    .iter()
                    .any(|ask| ask.is_open() && !item.question_tags(Some(ask)).is_empty())
                    || (item.status == "needs"
                        && item.asks.is_empty()
                        && !item.question_tags(None).is_empty()))
        })
    })
}

/// Each item's `tag` turned into its look (`tagLook`: label, glyph, colour), as the board does
/// (board.rs `add_tag_looks`), and the tag list the Work panel's menu offers.
pub fn add_work_tag_looks(work: &mut Value, custom_tags: &Value) {
    let custom: Option<ghostex_gx_protocol::CustomSessionTagsState> =
        serde_json::from_value(custom_tags.clone()).ok();
    for row in work["items"].as_array_mut().into_iter().flatten() {
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
    work["tagOptions"] = json!(ghostex_gx_core::tag_choices(custom.as_ref()));
}

/// Every question tag the coordinator's work items ask, open or settled, uppercased: a form or
/// one-click question in its chat with only these tags is that ask, counted once as its card
/// (server/src/session_chat_open_questions.rs, gx-chat-core extras/work_cards.rs `card_tags`).
pub fn ask_tags(db: &Connection, key: &SessionKey) -> std::collections::BTreeSet<String> {
    list_work_items(db, key)
        .unwrap_or_default()
        .iter()
        .flat_map(|item| item.asks.iter())
        .map(|ask| ask.tag.to_ascii_uppercase())
        .filter(|tag| !tag.is_empty())
        .collect()
}
