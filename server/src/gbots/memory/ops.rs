//! The memory ops of `/api/gbots` (endpoint.rs hands them here):
//!
//! | op | params | who |
//! | --- | --- | --- |
//! | `memoryShow` | `bot?`, `topic?` | the bot's own chats, you, Chief of Staff |
//! | `memoryAdd` | `bot?`, `text`, `until?` (YYYY-MM-DD), `topic?`, `title?` | the same |
//! | `memoryForget` | `bot?`, `match` (words of the note) | the same |
//! | `memoryEdit` | `bot?`, `text`, `base?` (the `hash` it was read at), `topic?` | the same |
//! | `memoryUndo` | `bot?`, `change?` (the newest change by default) | the same |
//! | `memoryTopics`, `memoryTopic` (`topic`), `memoryLog` (`days?`, `date?`), `memoryJournal` | `bot?` | the same |
//! | `prefsShow` | | anyone here |
//! | `prefsSet` | `text` | you, Chief of Staff |
//!
//! A bot's chats name no bot: the memory is their own bot's. Another bot's memory is refused.

use std::path::Path;

use chrono::{Local, NaiveDate};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::budget::{make_room, within_budget, Gauge, MEMORY_MAX_BYTES, MEMORY_MAX_LINES};
use super::entries::{check_text, format_entry, parse, struck_through};
use super::journal::{self, FileChange};
use super::store::{bot_lock, commit, read_file};
use super::topics::{self, ARCHIVE_STEM};
use crate::domain::{DomainRepository, DomainStateError};
use crate::gbots::folder::{bot_folder, MEMORY_FILE};
use crate::gbots::record::{bot_summary, list_bots, require_bot, Bot};
use crate::server::AppState;

/// Who asks, as memory's rules see it.
pub(crate) struct MemoryCaller {
    /// The bot whose chat is asking, if a bot's chat is.
    pub own_bot: Option<Bot>,
    /// You or Chief of Staff.
    pub may_manage: bool,
    /// `user` or `chief` (for the journal), when `may_manage`.
    pub label: &'static str,
}

const ARCHIVE_FILE: &str = "memory/archive.md";
const ARCHIVE_HEAD: &str = "# Archive\n\nNotes moved out of MEMORY.md to keep it within what loads at the start of a chat, oldest first. Read this file when an older note matters.\n";
/// Changes the Memory tab and `memory show` list.
const JOURNAL_SHOWN: usize = 20;

pub(crate) fn handles(op: &str) -> bool {
    op.starts_with("memory") || op.starts_with("prefs")
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

fn conflict(message: String) -> DomainStateError {
    DomainStateError {
        code: "conflict",
        message,
    }
}

pub(crate) fn hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// The bot whose memory the caller means, and who the journal says made the change.
fn target(
    db: &Connection,
    caller: &MemoryCaller,
    params: &Map<String, Value>,
) -> Result<(Bot, &'static str), DomainStateError> {
    match (text(params, "bot"), caller.own_bot.as_ref()) {
        (None, Some(own)) => Ok((own.clone(), "self")),
        (Some(reference), Some(own)) => {
            let named = require_bot(db, reference)?;
            if named.bot_id == own.bot_id {
                Ok((named, "self"))
            } else if caller.may_manage {
                Ok((named, caller.label))
            } else {
                Err(forbidden("A GBot keeps only its own memory."))
            }
        }
        (Some(reference), None) if caller.may_manage => {
            Ok((require_bot(db, reference)?, caller.label))
        }
        (Some(_), None) => Err(forbidden(
            "Only a GBot's own chats, you and Chief of Staff read or change its memory.",
        )),
        (None, None) => Err(DomainStateError::bad_request(
            "This isn't a GBot's chat: name the GBot with --bot.",
        )),
    }
}

pub(crate) fn run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    caller: MemoryCaller,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    match op {
        "prefsShow" => return Ok(json!({ "preferences": super::prefs::show(&state.paths) })),
        "prefsSet" => {
            if !caller.may_manage {
                return Err(forbidden(
                    "Only you and Chief of Staff change the shared preferences.",
                ));
            }
            let body = params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let answer = super::prefs::set(&state.paths, body)?;
            crate::gbots::brief::refresh_all_instruction_files(&state.paths, db);
            super::notify::tell_every_bot(state, repository, &list_bots(db)?);
            return Ok(json!({ "preferences": answer }));
        }
        _ => {}
    }
    let (bot, by) = target(db, &caller, params)?;
    let folder = bot_folder(&state.paths, &bot.bot_id);
    let answer = match op {
        "memoryShow" => return show(state, db, &bot, params),
        "memoryTopics" => {
            return Ok(json!({
                "bot": bot_summary(&bot),
                "topics": topics::list(&folder).iter().map(topics::Topic::summary).collect::<Vec<_>>(),
            }))
        }
        "memoryTopic" => return topic(&bot, &folder, params),
        "memoryLog" => return log(&bot, &folder, params),
        "memoryJournal" => {
            let entries = journal::read(&state.paths, &bot.bot_id);
            return Ok(json!({
                "bot": bot_summary(&bot),
                "journal": journal::summaries(&folder, &entries, &bot.name, 100),
            }));
        }
        "memoryAdd" => add(state, db, &bot, &folder, by, params)?,
        "memoryForget" => forget(state, db, &bot, &folder, by, params)?,
        "memoryEdit" => edit(state, db, &bot, &folder, by, params)?,
        "memoryUndo" => undo(state, db, &bot, &folder, by, params)?,
        other => {
            return Err(DomainStateError::bad_request(format!(
                "Unknown GBots memory op {other}: use memoryShow, memoryAdd, memoryForget, memoryEdit, memoryUndo, memoryTopics, memoryTopic, memoryLog, memoryJournal, prefsShow or prefsSet."
            )))
        }
    };
    if by != "self" {
        super::notify::tell_open_chats(state, repository, &bot);
    }
    Ok(answer)
}

/// What loads into the bot and how full its memory is, for `memory show` and the Memory tab.
fn show(
    state: &AppState,
    db: &Connection,
    bot: &Bot,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let folder = bot_folder(&state.paths, &bot.bot_id);
    let memory = read_file(&folder, MEMORY_FILE).unwrap_or_default();
    let gauge = Gauge::of(&memory);
    let (loaded, cut) = within_budget(&memory, MEMORY_MAX_LINES, MEMORY_MAX_BYTES);
    let archive = read_file(&folder, ARCHIVE_FILE).unwrap_or_default();
    let topic_list = topics::list(&folder);
    let indexed = topics::index_lines(&folder).len();
    let preferences = super::prefs::show(&state.paths);
    let entries = journal::read(&state.paths, &bot.bot_id);
    let mut answer = json!({
        "bot": bot_summary(bot),
        "folder": folder,
        "memory": {
            "text": memory,
            "hash": hash(&memory),
            "path": folder.join(MEMORY_FILE),
            "lines": gauge.lines,
            "bytes": gauge.bytes,
            "maxLines": MEMORY_MAX_LINES,
            "maxBytes": MEMORY_MAX_BYTES,
            "share": gauge.share(),
            "level": gauge.level(),
            "loadedLines": if loaded.is_empty() { 0 } else { loaded.lines().count() },
            "cut": cut,
            "notes": parse(&memory).len(),
        },
        "archive": {
            "path": folder.join(ARCHIVE_FILE),
            "notes": parse(&archive).len(),
            "bytes": archive.len(),
        },
        "loads": {
            "instructionsLines": bot.soul.lines().count(),
            "preferencesLines": preferences["lines"],
            "topicsIndexed": indexed,
            "team": list_bots(db)?.len(),
        },
        "preferences": preferences,
        "topics": topic_list.iter().map(topics::Topic::summary).collect::<Vec<_>>(),
        "journal": journal::summaries(&folder, &entries, &bot.name, JOURNAL_SHOWN),
        "log": super::log::recent(&folder, 3),
        "today": today().format("%Y-%m-%d").to_string(),
    });
    if text(params, "topic").is_some() {
        answer["topic"] = topic(bot, &folder, params)?["topic"].take();
    }
    Ok(answer)
}

fn topic(bot: &Bot, folder: &Path, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let name = text(params, "topic")
        .ok_or_else(|| DomainStateError::bad_request("Name the topic: --topic <name>."))?;
    let stem = topics::stem_of(name).map_err(DomainStateError::bad_request)?;
    let relative = topics::relative_path(&stem);
    let body = read_file(folder, &relative).ok_or_else(|| {
        DomainStateError::not_found(format!(
            "{} has no topic {stem}. `ghostex gbots memory topics` lists them.",
            bot.name
        ))
    })?;
    Ok(json!({
        "bot": bot_summary(bot),
        "topic": { "name": stem, "path": folder.join(&relative), "text": body, "hash": hash(&body) },
    }))
}

fn log(bot: &Bot, folder: &Path, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    if let Some(date) = text(params, "date") {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| {
            DomainStateError::bad_request("Give the date as YYYY-MM-DD, e.g. --date 2026-11-08.")
        })?;
        return Ok(json!({ "bot": bot_summary(bot), "log": [super::log::day(folder, date)] }));
    }
    let days = params.get("days").and_then(Value::as_u64).unwrap_or(1) as usize;
    Ok(json!({ "bot": bot_summary(bot), "log": super::log::recent(folder, days) }))
}

/// The memory file a write goes to: `MEMORY.md`, or a topic.
struct Target {
    relative: String,
    /// The topic's name, for a topic.
    topic: Option<String>,
}

fn write_target(params: &Map<String, Value>) -> Result<Target, DomainStateError> {
    match text(params, "topic") {
        Some(name) => {
            let stem = topics::stem_of(name).map_err(DomainStateError::bad_request)?;
            Ok(Target {
                relative: topics::relative_path(&stem),
                topic: Some(stem),
            })
        }
        None => Ok(Target {
            relative: MEMORY_FILE.to_string(),
            topic: None,
        }),
    }
}

/// The file changes that write `after` to `MEMORY.md` within its budget: the notes it moves out
/// are appended to the archive. Refused when what can't move (lines without a date) is too much.
fn memory_changes(
    folder: &Path,
    before: Option<String>,
    after: String,
) -> Result<(Vec<FileChange>, usize), DomainStateError> {
    let mut changes = Vec::new();
    let mut moved_count = 0;
    let after = match make_room(&after, today()) {
        None => after,
        Some((kept, moved)) => {
            if !Gauge::of(&kept).fits() {
                return Err(DomainStateError::bad_request(format!(
                    "MEMORY.md would pass what loads ({MEMORY_MAX_LINES} lines, {} KB) even with every dated note moved to the archive: shorten the lines without a date, or move detail into a topic.",
                    MEMORY_MAX_BYTES / 1024
                )));
            }
            if !moved.is_empty() {
                moved_count = moved.len();
                let mut appended = archive_heading(folder, "Moved");
                for note in &moved {
                    appended.push_str(note);
                    appended.push('\n');
                }
                changes.push(FileChange::appended(ARCHIVE_FILE, appended));
            }
            kept
        }
    };
    changes.push(FileChange::replaced(MEMORY_FILE, before, after));
    Ok((changes, moved_count))
}

/// What goes before notes appended to the archive: its head when it is new, and today's
/// `## Moved <date>` (or `Forgotten`) heading unless the archive already ends under it.
fn archive_heading(folder: &Path, kind: &str) -> String {
    let archive = read_file(folder, ARCHIVE_FILE).unwrap_or_default();
    let heading = format!("## {kind} {}", today().format("%Y-%m-%d"));
    let mut out = String::new();
    if archive.trim().is_empty() {
        out.push_str(ARCHIVE_HEAD);
    }
    let last_heading = archive.lines().rev().find(|line| line.starts_with("## "));
    if last_heading != Some(heading.as_str()) {
        out.push_str(&format!("\n{heading}\n\n"));
    }
    out
}

fn redacted_note(count: usize) -> Option<String> {
    (count > 0).then(|| {
        format!(
            "{count} thing{} that looked like a key or password {} replaced with «redacted».",
            if count == 1 { "" } else { "s" },
            if count == 1 { "was" } else { "were" }
        )
    })
}

fn add(
    state: &AppState,
    db: &Connection,
    bot: &Bot,
    folder: &Path,
    by: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let body = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let note = check_text(body).map_err(DomainStateError::bad_request)?;
    let (note, redacted) = super::redact::redact(&note);
    let until = match text(params, "until") {
        Some(until) => Some(NaiveDate::parse_from_str(until, "%Y-%m-%d").map_err(|_| {
            DomainStateError::bad_request("Give --until as YYYY-MM-DD, e.g. --until 2026-12-01.")
        })?),
        None => None,
    };
    let target = write_target(params)?;
    let lock = bot_lock(&bot.bot_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = read_file(folder, &target.relative);
    let current = before.clone().unwrap_or_default();
    let collapsed = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    if parse(&current)
        .iter()
        .any(|entry| !entry.struck && collapsed(&entry.text) == collapsed(&note))
    {
        return Ok(json!({
            "bot": bot_summary(bot),
            "added": false,
            "message": format!("{} already has that note.", bot.name),
        }));
    }
    let mut after = match (&target.topic, before.as_deref()) {
        (Some(stem), None) => {
            if topics::list(folder).len() >= topics::TOPICS_MAX {
                return Err(DomainStateError::bad_request(format!(
                    "{} has {} topics, the most it can have: add to an existing one.",
                    bot.name,
                    topics::TOPICS_MAX
                )));
            }
            topics::new_topic_text(text(params, "title").unwrap_or(stem))
        }
        _ => current.clone(),
    };
    if !after.is_empty() && !after.ends_with('\n') {
        after.push('\n');
    }
    // The first note under a paragraph starts a list of its own.
    let last = after
        .trim_end_matches('\n')
        .lines()
        .next_back()
        .unwrap_or_default();
    if !last.trim().is_empty()
        && !last.starts_with("- ")
        && !last.starts_with("  ")
        && !last.starts_with('#')
    {
        after.push('\n');
    }
    after.push_str(&format_entry(today(), &note, until));
    after.push('\n');
    let (changes, moved) = if target.topic.is_some() {
        if after.len() > topics::TOPIC_MAX_BYTES {
            return Err(DomainStateError::bad_request(format!(
                "That topic is full ({} KB): start another topic or tidy this one.",
                topics::TOPIC_MAX_BYTES / 1024
            )));
        }
        (
            vec![FileChange::replaced(&target.relative, before, after)],
            0,
        )
    } else {
        memory_changes(folder, before, after)?
    };
    let what = match &target.topic {
        Some(stem) => format!("Added a note to {stem}"),
        None => "Added a note".to_string(),
    };
    let entry = commit(&state.paths, db, bot, by, &what, changes, None)?;
    Ok(json!({
        "bot": bot_summary(bot),
        "added": true,
        "change": entry.id,
        "file": target.relative,
        "movedToArchive": moved,
        "redacted": redacted,
        "redactedNote": redacted_note(redacted),
    }))
}

fn forget(
    state: &AppState,
    db: &Connection,
    bot: &Bot,
    folder: &Path,
    by: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let words = text(params, "match")
        .ok_or_else(|| DomainStateError::bad_request("Say which note: some of its words."))?
        .to_lowercase();
    let lock = bot_lock(&bot.bot_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = read_file(folder, MEMORY_FILE).unwrap_or_default();
    let lines: Vec<&str> = before.lines().collect();
    let found: Vec<_> = parse(&before)
        .into_iter()
        .filter(|entry| !entry.struck && entry.text.to_lowercase().contains(&words))
        .collect();
    let entry = match found.as_slice() {
        [] => {
            return Err(DomainStateError::not_found(format!(
                "No note in {}'s memory has those words. `ghostex gbots memory show` lists them.",
                bot.name
            )))
        }
        [entry] => entry.clone(),
        many => {
            return Err(DomainStateError::bad_request(format!(
                "{} notes have those words; use more of the one you mean:\n{}{}",
                many.len(),
                many.iter()
                    .take(5)
                    .map(|entry| format!("- {}", entry.text))
                    .collect::<Vec<_>>()
                    .join("\n"),
                if many.len() > 5 {
                    format!("\n…and {} more.", many.len() - 5)
                } else {
                    String::new()
                }
            )))
        }
    };
    let mut kept: Vec<&str> = lines[..entry.start].to_vec();
    kept.extend_from_slice(&lines[entry.end..]);
    let mut after = kept.join("\n");
    after.push('\n');
    let mut appended = archive_heading(folder, "Forgotten");
    appended.push_str(&struck_through(&lines[entry.start..entry.end]));
    appended.push('\n');
    let changes = vec![
        FileChange::appended(ARCHIVE_FILE, appended),
        FileChange::replaced(MEMORY_FILE, Some(before.clone()), after),
    ];
    let change = commit(&state.paths, db, bot, by, "Forgot a note", changes, None)?;
    Ok(json!({ "bot": bot_summary(bot), "forgot": entry.text, "change": change.id }))
}

fn edit(
    state: &AppState,
    db: &Connection,
    bot: &Bot,
    folder: &Path,
    by: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let body = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = write_target(params)?;
    if target.topic.as_deref() == Some(ARCHIVE_STEM) {
        return Err(forbidden("The archive is kept by Ghostex."));
    }
    let (body, redacted) = super::redact::redact(body.trim_end());
    let mut after = body;
    if !after.is_empty() {
        after.push('\n');
    }
    let lock = bot_lock(&bot.bot_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = read_file(folder, &target.relative);
    let current = before.clone().unwrap_or_default();
    if let Some(base) = text(params, "base") {
        if base != hash(&current) {
            let last = journal::read(&state.paths, &bot.bot_id).pop();
            let who = match last.as_ref() {
                Some(entry) if entry.by == "self" => format!("{} wrote to it", bot.name),
                Some(entry) if entry.by == "user" => {
                    "it was edited from another window".to_string()
                }
                Some(entry) if entry.by == "chief" => "Chief of Staff wrote to it".to_string(),
                _ => "it was written to".to_string(),
            };
            let file = target
                .relative
                .rsplit('/')
                .next()
                .unwrap_or(&target.relative)
                .to_string();
            return Err(conflict(format!(
                "{file} changed while you were editing it ({who}), so your text was not saved. Reload to see the change, then edit again."
            )));
        }
    }
    if after == current {
        return Ok(json!({ "bot": bot_summary(bot), "changed": false, "hash": hash(&current) }));
    }
    let (changes, moved) = if target.topic.is_some() {
        if after.len() > topics::TOPIC_MAX_BYTES {
            return Err(DomainStateError::bad_request(format!(
                "Keep a topic under {} KB: split it into two topics.",
                topics::TOPIC_MAX_BYTES / 1024
            )));
        }
        if before.is_none() && topics::list(folder).len() >= topics::TOPICS_MAX {
            return Err(DomainStateError::bad_request(format!(
                "{} has {} topics, the most it can have.",
                bot.name,
                topics::TOPICS_MAX
            )));
        }
        (
            vec![FileChange::replaced(&target.relative, before, after)],
            0,
        )
    } else {
        memory_changes(folder, before, after)?
    };
    let what = match &target.topic {
        Some(stem) => format!("Edited {stem}"),
        None => "Edited MEMORY.md".to_string(),
    };
    let entry = commit(&state.paths, db, bot, by, &what, changes, None)?;
    let now = read_file(folder, &target.relative).unwrap_or_default();
    Ok(json!({
        "bot": bot_summary(bot),
        "changed": true,
        "change": entry.id,
        "hash": hash(&now),
        "movedToArchive": moved,
        "redacted": redacted,
        "redactedNote": redacted_note(redacted),
    }))
}

fn undo(
    state: &AppState,
    db: &Connection,
    bot: &Bot,
    folder: &Path,
    by: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let lock = bot_lock(&bot.bot_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let entries = journal::read(&state.paths, &bot.bot_id);
    let next = journal::last_undoable(&entries).ok_or_else(|| {
        DomainStateError::not_found(format!(
            "There is no change to {}'s memory to undo.",
            bot.name
        ))
    })?;
    if let Some(wanted) = text(params, "change") {
        if wanted != next.id {
            let known = entries.iter().any(|entry| entry.id == wanted);
            return Err(DomainStateError::bad_request(if known {
                format!(
                    "Undo goes back one change at a time: undo {} first (`ghostex gbots memory journal` lists them).",
                    next.id
                )
            } else {
                format!("There is no memory change {wanted}. `ghostex gbots memory journal` lists them.")
            }));
        }
    }
    if let Err(file) = journal::still_as_left(folder, next) {
        return Err(conflict(format!(
            "{file} changed after that change, so undoing it would lose the newer text. Edit the file instead."
        )));
    }
    let what = format!("Undid “{}”", next.what);
    let entry = commit(
        &state.paths,
        db,
        bot,
        by,
        &what,
        journal::reversal(next),
        Some(next.id.clone()),
    )?;
    Ok(json!({
        "bot": bot_summary(bot),
        "undid": { "id": next.id, "what": next.what, "by": journal::by_label(&next.by, &bot.name) },
        "change": entry.id,
    }))
}

/// `ghostex coordinator remember` from a GBot's chat (`/api/updateCoordinator` with `remember`):
/// the note goes into the bot's memory instead of the chat's coordinator notes, so the habit lands
/// where every chat of the bot reads it. `None` for any other session or update.
pub(crate) fn coordinator_remember(
    state: &AppState,
    endpoint_path: &str,
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Option<Value>, DomainStateError> {
    if endpoint_path != "/api/updateCoordinator" {
        return Ok(None);
    }
    let (Some(note), Some(project_id)) = (text(params, "remember"), text(params, "projectId"))
    else {
        return Ok(None);
    };
    let Some(bot) = crate::gbots::record::bot_for_project(db, project_id)? else {
        return Ok(None);
    };
    let folder = bot_folder(&state.paths, &bot.bot_id);
    let mut add_params = Map::new();
    add_params.insert("text".into(), json!(note));
    let added = add(state, db, &bot, &folder, "self", &add_params)?;
    Ok(Some(
        json!({ "gbotsMemory": added, "coordinator": { "memory": [] } }),
    ))
}
