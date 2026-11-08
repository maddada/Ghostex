//! The `gbots_changes` journal read back: `ghostex gbots changes`, Undo of a GBot added or
//! changed, the "GBot added" card's state, and the caps on how many GBots Chief of Staff adds.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q2103 A): Chief of Staff adds a GBot on its own and tells you in one "GBot added" card with Undo. Undo removes the bot and closes its chats (their transcripts stay in Previous Sessions) and is refused once the bot was changed after it was added, so Undo never throws away a later change; an undone change says so on its card from then on, on every computer and phone.
//! CDXC:GBots 2026-11-09 WHY:
//! A runaway Chief of Staff must not fill the sidebar with bots, so its adds are capped on the server: 4 in any 15 minutes (the plan's "4 per turn"; gxserver can't see turns) and 12 a day. More at once goes to the user as a proposal (the playbook's questions form), which the user accepts before anything is added.

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use super::record::{bot_summary, read_bot, Bot};
use crate::domain::DomainStateError;

/// GBots Chief of Staff may add in any 15 minutes, and in any day.
pub(crate) const CHIEF_ADDS_PER_BURST: i64 = 4;
pub(crate) const CHIEF_ADDS_PER_DAY: i64 = 12;

#[derive(Clone, Debug)]
pub(crate) struct Change {
    pub change_id: String,
    pub kind: String,
    pub subject_id: String,
    pub before: Option<Bot>,
    pub after: Option<Bot>,
    pub by: String,
    pub at: String,
    pub undone_at: Option<String>,
    /// `before` and `after` as saved, for changes that are not about a bot (a routine's).
    pub before_raw: Option<Value>,
    pub after_raw: Option<Value>,
}

fn is_routine(kind: &str) -> bool {
    kind.starts_with("routine")
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

const COLUMNS: &str = "changeId, kind, subjectId, beforeJson, afterJson, byWho, at, undoneAt";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Change> {
    let bot = |text: &Option<String>| {
        text.as_deref()
            .and_then(|text| serde_json::from_str::<Bot>(text).ok())
    };
    let raw = |text: &Option<String>| {
        text.as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
    };
    let (before, after): (Option<String>, Option<String>) = (row.get(3)?, row.get(4)?);
    Ok(Change {
        change_id: row.get(0)?,
        kind: row.get(1)?,
        subject_id: row.get(2)?,
        before: bot(&before),
        after: bot(&after),
        by: row.get(5)?,
        at: row.get(6)?,
        undone_at: row.get(7)?,
        before_raw: raw(&before),
        after_raw: raw(&after),
    })
}

pub(crate) fn read_change(
    db: &Connection,
    change_id: &str,
) -> Result<Option<Change>, DomainStateError> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_changes WHERE changeId = ?1"),
        [change_id.trim()],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// The newest changes first.
pub(crate) fn list_changes(db: &Connection, limit: usize) -> Result<Vec<Change>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM gbots_changes ORDER BY at DESC, rowid DESC LIMIT ?1"
        ))
        .map_err(sql)?;
    let rows = statement.query_map([limit as i64], from_row).map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

fn mark_undone(db: &Connection, change_id: &str) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_changes SET undoneAt = ?2 WHERE changeId = ?1",
        rusqlite::params![change_id, crate::coordinators::now_iso()],
    )
    .map_err(sql)?;
    Ok(())
}

/// Refuses another add by Chief of Staff past its caps.
pub(crate) fn check_chief_add_caps(db: &Connection) -> Result<(), DomainStateError> {
    let since = |minutes: i64| {
        (chrono::Utc::now() - chrono::Duration::minutes(minutes))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    let count = |from: String| -> Result<i64, DomainStateError> {
        db.query_row(
            "SELECT COUNT(*) FROM gbots_changes WHERE kind = 'botAdded' AND byWho = 'chief' \
             AND at >= ?1",
            [from],
            |row| row.get(0),
        )
        .map_err(sql)
    };
    if count(since(15))? >= CHIEF_ADDS_PER_BURST {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "You added {CHIEF_ADDS_PER_BURST} GBots in the last 15 minutes, the most at once. Propose the rest to the user as a questions form and add them once they accept."
            ),
        });
    }
    if count(since(24 * 60))? >= CHIEF_ADDS_PER_DAY {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "You added {CHIEF_ADDS_PER_DAY} GBots today, the most in a day. Hand the work to a GBot you already have, or ask the user to add more."
            ),
        });
    }
    Ok(())
}

/// The bot's persona as a change saved it, without the times that every save moves.
fn same_persona(a: &Bot, b: &Bot) -> bool {
    let strip = |bot: &Bot| {
        let mut bot = bot.clone();
        bot.updated_at.clear();
        bot.created_at.clear();
        bot
    };
    strip(a) == strip(b)
}

/// Why `change` can't be undone now; `None` when it can.
pub(crate) fn undo_problem(
    db: &Connection,
    change: &Change,
) -> Result<Option<String>, DomainStateError> {
    if change.undone_at.is_some() {
        return Ok(Some("That change was already undone.".to_string()));
    }
    if is_routine(&change.kind) {
        return super::routines::undo_problem(
            db,
            &change.kind,
            &change.subject_id,
            change.before_raw.as_ref(),
            change.after_raw.as_ref(),
        );
    }
    let current = read_bot(db, &change.subject_id)?;
    let name = change
        .after
        .as_ref()
        .or(change.before.as_ref())
        .map(|bot| bot.name.clone())
        .unwrap_or_else(|| "That GBot".to_string());
    Ok(match (change.kind.as_str(), current, change.after.as_ref()) {
        ("botAdded" | "botEdited", None, _) => Some(format!("{name} was removed since.")),
        ("botAdded" | "botEdited", Some(current), Some(after)) if !same_persona(&current, after) => Some(format!(
            "{name} was changed since, so Undo would lose that change. Change or remove it in its GBot Settings instead."
        )),
        ("botAdded" | "botEdited", Some(_), Some(_)) => None,
        ("botRemoved", ..) => Some(format!(
            "A removed GBot can't come back with Undo; its folder is in the Trash. Add {name} again instead."
        )),
        _ => Some("Only adding or changing a GBot can be undone.".to_string()),
    })
}

/// A change as `/api/gbots` answers it; `bot` is the bot as it is now, when it still exists.
pub(crate) fn change_summary(db: &Connection, change: &Change) -> Result<Value, DomainStateError> {
    if is_routine(&change.kind) {
        let shown = change.after_raw.as_ref().or(change.before_raw.as_ref());
        let problem = undo_problem(db, change)?;
        let routine = super::routines::current(db, &change.subject_id)?;
        let bot_id = shown
            .and_then(|routine| routine.get("botId"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Ok(json!({
            "changeId": change.change_id,
            "kind": change.kind,
            "routineId": change.subject_id,
            "botId": bot_id,
            "name": shown.and_then(|routine| routine.get("name")).cloned(),
            "by": change.by,
            "at": change.at,
            "undoneAt": change.undone_at,
            "undoable": problem.is_none(),
            "undoProblem": problem,
            "routine": routine,
            "bot": read_bot(db, bot_id)?.as_ref().map(bot_summary),
        }));
    }
    let current = read_bot(db, &change.subject_id)?;
    let shown = change.after.as_ref().or(change.before.as_ref());
    let problem = undo_problem(db, change)?;
    Ok(json!({
        "changeId": change.change_id,
        "kind": change.kind,
        "botId": change.subject_id,
        "name": shown.map(|bot| bot.name.clone()),
        "by": change.by,
        "at": change.at,
        "undoneAt": change.undone_at,
        "undoable": problem.is_none(),
        "undoProblem": problem,
        "bot": current.as_ref().map(bot_summary),
    }))
}

/// `op: undo`: removes an added bot or puts a changed one back. Answers the change, now undone.
/// `remove` and `restore` are the endpoint's own remove and edit.
pub(crate) fn undo(
    db: &Connection,
    change: &Change,
    remove: impl FnOnce(&str) -> Result<Value, DomainStateError>,
    restore: impl FnOnce(&Bot) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    if let Some(problem) = undo_problem(db, change)? {
        return Err(DomainStateError {
            code: if change.undone_at.is_some() {
                "alreadyUndone"
            } else {
                "conflict"
            },
            message: problem,
        });
    }
    if is_routine(&change.kind) {
        let done = super::routines::undo(
            db,
            &change.kind,
            &change.subject_id,
            change.before_raw.as_ref(),
        )?;
        mark_undone(db, &change.change_id)?;
        let change = read_change(db, &change.change_id)?.unwrap_or_else(|| change.clone());
        return Ok(json!({ "change": change_summary(db, &change)?, "done": done }));
    }
    let done = match (change.kind.as_str(), change.before.as_ref()) {
        ("botAdded", _) => remove(&change.subject_id)?,
        ("botEdited", Some(before)) => restore(before)?,
        _ => {
            return Err(DomainStateError::bad_request(
                "Only adding or changing a GBot can be undone.",
            ))
        }
    };
    mark_undone(db, &change.change_id)?;
    let change = read_change(db, &change.change_id)?.unwrap_or_else(|| change.clone());
    Ok(json!({ "change": change_summary(db, &change)?, "done": done }))
}
