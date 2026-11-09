//! `teamPreview` and `teamImport`: a team file read back into GBots. The preview says what would
//! be added (names already taken get " 2"), what the file left out, what Ghostex ignored and what
//! keeps it from importing; the import then adds the departments, the GBots with their starter
//! notes and their routines, paused, and writes one `teamFileAdded` change so Undo removes the
//! whole import.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! A team file carries no agent or approval level, so an imported GBot gets what a new GBot gets: the agent picked for the import (your first Claude Code or Codex by default) and that agent's default approval level (session.rs `default_approval`: Codex GBots start at Full access, the others at Ask). Every imported routine arrives paused and only runs once you resume it.
//! CDXC:GBots 2026-11-09 WHY:
//! Importing is yours alone (your computer, phone or terminal, which ask for your approval first, like adding one GBot): a team file is text from outside, and the GBots it adds may be answered for and directed by Chief of Staff, so Chief of Staff and other agents may read a file's preview but not import it. Undo is refused once an imported GBot was changed since, so it never throws a later change away; remove that GBot first, or the others one by one.

use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::format::{read, FileBot, Kind, ReadFile, NOTES_TOPIC_MAX_BYTES};
use crate::coordinators::now_iso;
use crate::domain::{DomainRepository, DomainStateError};
use crate::gbots::changes::Change;
use crate::gbots::departments::{self, list_departments, read_department, Department};
use crate::gbots::folder::{bot_folder, MEMORY_FILE};
use crate::gbots::memory::{
    commit_memory, read_memory_file, redact, topic_path, within_budget, FileChange,
    MEMORY_MAX_BYTES, MEMORY_MAX_LINES,
};
use crate::gbots::record::{list_bots, new_id, read_bot, Bot, NAME_MAX_CHARS};
use crate::server::AppState;

pub(crate) const TEAM_CHANGE: &str = "teamFileAdded";

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

/// The file's text from `file` (its text, or the JSON itself).
pub(crate) fn file_text(params: &Map<String, Value>) -> Result<String, DomainStateError> {
    match params.get("file") {
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(text.clone()),
        Some(value @ Value::Object(_)) => Ok(value.to_string()),
        _ => Err(DomainStateError::bad_request(
            "Pass the team file's text as file.",
        )),
    }
}

/// The file read, or the sentence saying why it can't be.
pub(crate) fn read_params(params: &Map<String, Value>) -> Result<ReadFile, DomainStateError> {
    read(&file_text(params)?).map_err(DomainStateError::bad_request)
}

/// A digest of the file, so an approval is spent only on the file it was given for.
pub(crate) fn digest(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// `name`, or `name 2`, `name 3`, … whichever no one in `taken` (lowercase) has.
fn free_name(name: &str, taken: &[String]) -> String {
    if !taken.contains(&name.to_lowercase()) {
        return name.to_string();
    }
    (2..)
        .map(|n| {
            let suffix = format!(" {n}");
            let room = NAME_MAX_CHARS.saturating_sub(suffix.chars().count());
            format!(
                "{}{suffix}",
                name.chars().take(room).collect::<String>().trim_end()
            )
        })
        .find(|candidate| !taken.contains(&candidate.to_lowercase()))
        .unwrap_or_else(|| name.to_string())
}

/// What an import would add.
pub(crate) struct Plan {
    pub read: ReadFile,
    /// The name each bot and department gets here, by its key.
    pub bot_names: Vec<(String, String)>,
    pub department_names: Vec<(String, String)>,
    pub agent_id: Option<String>,
    pub approval: String,
    pub notes: Vec<String>,
}

impl Plan {
    fn bot_name(&self, key: &str) -> String {
        self.bot_names
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| key.to_string())
    }

    fn department_name(&self, key: &str) -> String {
        self.department_names
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| key.to_string())
    }
}

pub(crate) fn plan(
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Plan, DomainStateError> {
    let mut read = read_params(params)?;
    let mut notes = Vec::new();
    let existing = list_bots(db)?;
    let mut taken: Vec<String> = existing.iter().map(|bot| bot.name.to_lowercase()).collect();
    let mut bot_names = Vec::new();
    for bot in &read.file.bots {
        let name = free_name(&bot.name, &taken);
        if name != bot.name {
            notes.push(format!(
                "There is already a GBot called {}, so this one is added as {name}.",
                bot.name
            ));
        }
        taken.push(name.to_lowercase());
        bot_names.push((bot.key.clone(), name));
    }
    let mut taken_departments: Vec<String> = list_departments(db)?
        .iter()
        .map(|department| department.name.to_lowercase())
        .collect();
    let mut department_names = Vec::new();
    for department in &read.file.departments {
        let name = free_name(&department.name, &taken_departments);
        if name != department.name {
            notes.push(format!(
                "There is already a department called {}, so this one is added as {name}.",
                department.name
            ));
        }
        taken_departments.push(name.to_lowercase());
        department_names.push((department.key.clone(), name));
    }
    if existing.len() + read.file.bots.len() > crate::gbots::record::MAX_BOTS {
        read.problems.push(format!(
            "This computer can have {} GBots and has {}, so the {} in this file don't fit.",
            crate::gbots::record::MAX_BOTS,
            existing.len(),
            read.file.bots.len()
        ));
    }
    if taken_departments.len() > departments::MAX_DEPARTMENTS {
        read.problems.push(format!(
            "This computer can have {} departments, so the file's don't fit.",
            departments::MAX_DEPARTMENTS
        ));
    }
    for bot in &read.file.bots {
        let count = read
            .file
            .routines
            .iter()
            .filter(|routine| routine.bot == bot.key)
            .count();
        if count > crate::gbots::routines::MAX_ROUTINES_PER_BOT {
            read.problems.push(format!(
                "{} has {count} routines; a GBot keeps {} at most.",
                bot.name,
                crate::gbots::routines::MAX_ROUTINES_PER_BOT
            ));
        }
    }
    let rows = crate::gbots::session::bot_agent_rows(repository)?;
    let agent_id = match params.get("agentId").and_then(Value::as_str).map(str::trim) {
        Some(agent_id) if !agent_id.is_empty() => {
            crate::gbots::session::check_agent(&rows, agent_id)?;
            Some(agent_id.to_string())
        }
        _ => crate::coordinators::default_coordinator_launcher(&rows),
    };
    if agent_id.is_none() {
        read.problems.push(format!(
            "No agent a GBot can run on is set up here. Set up {} first.",
            crate::coordinators::coordinator_agent_families_text()
        ));
    }
    let approval = agent_id
        .as_deref()
        .map(|agent_id| crate::gbots::session::default_approval(&rows, agent_id))
        .unwrap_or("ask")
        .to_string();
    Ok(Plan {
        read,
        bot_names,
        department_names,
        agent_id,
        approval,
        notes,
    })
}

fn notes_summary(bot: &FileBot) -> Value {
    let notes = &bot.starter_notes;
    json!({
        "memoryLines": notes.memory.lines().filter(|line| !line.trim().is_empty()).count(),
        "topics": notes.topics.keys().collect::<Vec<_>>(),
    })
}

/// What the import would add, as the preview shows it.
pub(crate) fn preview_value(plan: &Plan) -> Value {
    let file = &plan.read.file;
    let bots: Vec<Value> = file
        .bots
        .iter()
        .map(|bot| {
            let name = plan.bot_name(&bot.key);
            let department = bot
                .department
                .as_deref()
                .map(|key| plan.department_name(key));
            let leads = bot.department.as_deref().is_some_and(|key| {
                file.departments.iter().any(|department| {
                    department.key == key && department.lead.as_deref() == Some(bot.key.as_str())
                })
            });
            json!({
                "key": bot.key,
                "name": name,
                "renamedFrom": (name != bot.name).then(|| bot.name.clone()),
                "title": bot.title,
                "description": bot.description,
                "icon": bot.icon,
                "color": bot.color,
                "colorHex": crate::gbots::record::color_hex(&bot.color),
                "department": department,
                "isLead": leads,
                "instructionsChars": bot.instructions.chars().count(),
                "starterNotes": notes_summary(bot),
                "routines": file.routines.iter().filter(|routine| routine.bot == bot.key).count(),
            })
        })
        .collect();
    let departments: Vec<Value> = file
        .departments
        .iter()
        .map(|department| {
            let name = plan.department_name(&department.key);
            json!({
                "key": department.key,
                "name": name,
                "renamedFrom": (name != department.name).then(|| department.name.clone()),
                "lead": department.lead.as_deref().map(|key| plan.bot_name(key)),
                "members": file.bots.iter()
                    .filter(|bot| bot.department.as_deref() == Some(department.key.as_str()))
                    .map(|bot| plan.bot_name(&bot.key))
                    .collect::<Vec<_>>(),
                "hasInstructions": !department.instructions.is_empty(),
            })
        })
        .collect();
    let routines: Vec<Value> = file
        .routines
        .iter()
        .map(|routine| {
            let command = routine
                .precheck
                .as_ref()
                .and_then(|check| check.get("kind"))
                .and_then(Value::as_str)
                == Some("command");
            json!({
                "bot": plan.bot_name(&routine.bot),
                "name": routine.name,
                "scheduleSentence": crate::automations::schedule_sentence(&routine.schedule),
                "precheckSentence": routine.precheck.as_ref().map(crate::automations::precheck_sentence),
                "deliver": routine.deliver,
                "runsCommand": command,
                "paused": true,
            })
        })
        .collect();
    let mut warnings = Vec::new();
    for routine in &file.routines {
        if let Some(command) = routine
            .precheck
            .as_ref()
            .filter(|check| check.get("kind").and_then(Value::as_str) == Some("command"))
            .and_then(|check| check.get("command"))
            .and_then(Value::as_str)
        {
            warnings.push(format!(
                "The routine {} runs the command `{command}` on this computer every few minutes once you resume it. Read it before you do.",
                routine.name
            ));
        }
    }
    let approval_sentence = match plan.approval.as_str() {
        "full" => "They run on Codex at Full access, as every new Codex GBot does; change that in GBot Settings.",
        _ => "They ask before they act (approval Ask), as every new GBot does; change that in GBot Settings.",
    };
    json!({
        "kind": file.kind.as_str(),
        "name": file.name,
        "summary": file.summary,
        "release": file.release,
        "exportedAt": file.exported_at,
        "version": file.version,
        "bots": bots,
        "departments": departments,
        "routines": routines,
        "leftOut": file.left_out,
        "ignored": plan.read.ignored,
        "problems": plan.read.problems,
        "notes": plan.notes,
        "warnings": warnings,
        "agentId": plan.agent_id,
        "approval": plan.approval,
        "approvalSentence": approval_sentence,
        "routinesArrivePaused": true,
        "canImport": plan.read.problems.is_empty() && file.kind == Kind::Team,
    })
}

/// The starter notes `bot` gets: added to its `MEMORY.md` within its budget, and its topics.
pub(crate) fn write_starter_notes(
    state: &AppState,
    db: &Connection,
    bot: &Bot,
    file_bot: &FileBot,
    by: &str,
    from: &str,
) -> Result<(), DomainStateError> {
    let notes = &file_bot.starter_notes;
    if notes.is_empty() {
        return Ok(());
    }
    let folder = bot_folder(&state.paths, &bot.bot_id);
    let mut changes = Vec::new();
    if !notes.memory.trim().is_empty() {
        let before = read_memory_file(&folder, MEMORY_FILE);
        let (clean, _) = redact(notes.memory.trim());
        let joined = format!(
            "{}\n{}\n",
            before.clone().unwrap_or_default().trim_end(),
            clean
        );
        let (after, _) = within_budget(&joined, MEMORY_MAX_LINES, MEMORY_MAX_BYTES);
        changes.push(FileChange::replaced(MEMORY_FILE, before, after));
    }
    for (stem, text) in &notes.topics {
        let (clean, _) = redact(text);
        let clean: String = if clean.len() > NOTES_TOPIC_MAX_BYTES {
            let mut cut = NOTES_TOPIC_MAX_BYTES;
            while !clean.is_char_boundary(cut) {
                cut -= 1;
            }
            clean[..cut].to_string()
        } else {
            clean
        };
        let path = topic_path(stem);
        let before = read_memory_file(&folder, &path);
        changes.push(FileChange::replaced(
            &path,
            before,
            format!("{}\n", clean.trim_end()),
        ));
    }
    let lock = crate::gbots::memory::bot_memory_lock(&bot.bot_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    commit_memory(
        &state.paths,
        db,
        bot,
        by,
        &format!("Starter notes from {from}"),
        changes,
        None,
    )?;
    Ok(())
}

/// What one import added, for its change row and for Undo.
#[derive(Default)]
struct Added {
    bots: Vec<String>,
    departments: Vec<Department>,
    routines: Vec<(String, String, String)>,
}

fn undo_added(
    db: &Connection,
    added: &Added,
    by: &str,
    remove: &mut dyn FnMut(&str) -> Result<Value, DomainStateError>,
) {
    for (routine_id, _, _) in &added.routines {
        let _ = crate::automations::delete_routine(db, routine_id);
    }
    for bot_id in &added.bots {
        let _ = remove(bot_id);
    }
    for department in &added.departments {
        let _ = departments::remove(db, by, &department.department_id);
    }
}

/// `teamImport`. `create` is the endpoint's own add of one GBot (params as `op: create` takes
/// them) and `remove` its removal, so an import adds each GBot exactly the way New GBot does.
pub(crate) fn import(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    by: &str,
    params: &Map<String, Value>,
    create: &mut dyn FnMut(&Map<String, Value>, &str) -> Result<Value, DomainStateError>,
    remove: &mut dyn FnMut(&str) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    let plan = plan(db, repository, params)?;
    let file = &plan.read.file;
    if file.kind == Kind::Preset {
        return Err(DomainStateError::bad_request(
            "That file is a preset: keep it for New GBot with `ghostex gbots preset import`, or add a GBot from it with `ghostex gbots create --preset`.",
        ));
    }
    if !plan.read.problems.is_empty() {
        return Err(DomainStateError::bad_request(format!(
            "That team file can't be imported: {}",
            plan.read.problems.join(" ")
        )));
    }
    let from = if file.name.is_empty() {
        "a team file".to_string()
    } else {
        format!("the team file {}", file.name)
    };
    // `createdBy` says where an imported GBot came from (never "chief": Chief of Staff answers
    // only for the GBots it added itself).
    let created_by: String = format!("teamFile:{}", file.name)
        .chars()
        .take(100)
        .collect();
    let mut added = Added::default();
    let outcome = (|| -> Result<Vec<String>, DomainStateError> {
        let mut skipped = Vec::new();
        let mut department_ids: Vec<(String, String)> = Vec::new();
        for department in &file.departments {
            let mut fields = Map::new();
            fields.insert("name".into(), json!(plan.department_name(&department.key)));
            fields.insert("icon".into(), json!(department.icon));
            fields.insert("instructions".into(), json!(department.instructions));
            let (created, _) = departments::create(db, by, &fields)?;
            department_ids.push((department.key.clone(), created.department_id.clone()));
            added.departments.push(created);
        }
        for file_bot in &file.bots {
            let mut fields = Map::new();
            fields.insert("name".into(), json!(plan.bot_name(&file_bot.key)));
            fields.insert("title".into(), json!(file_bot.title));
            fields.insert("description".into(), json!(file_bot.description));
            fields.insert("soul".into(), json!(file_bot.instructions));
            fields.insert("icon".into(), json!(file_bot.icon));
            fields.insert("color".into(), json!(file_bot.color));
            if let Some(agent_id) = &plan.agent_id {
                fields.insert("agentId".into(), json!(agent_id));
            }
            if let Some(key) = &file_bot.department {
                if let Some((_, id)) = department_ids.iter().find(|(known, _)| known == key) {
                    fields.insert("department".into(), json!(id));
                    let leads = file.departments.iter().any(|department| {
                        &department.key == key
                            && department.lead.as_deref() == Some(file_bot.key.as_str())
                    });
                    if leads {
                        fields.insert("lead".into(), json!(true));
                    }
                }
            }
            let answer = create(&fields, &created_by)?;
            let bot_id = answer["bot"]["botId"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            added.bots.push(bot_id.clone());
            let Some(bot) = read_bot(db, &bot_id)? else {
                continue;
            };
            if let Err(error) = write_starter_notes(state, db, &bot, file_bot, by, &from) {
                skipped.push(format!("{}'s starter notes: {}", bot.name, error.message));
            }
            for routine in file
                .routines
                .iter()
                .filter(|routine| routine.bot == file_bot.key)
            {
                let saved = crate::automations::save_routine(
                    db,
                    crate::automations::RoutineSave {
                        id: None,
                        project_id: &bot.project_id,
                        agent_id: &bot.agent_id,
                        bot_id: &bot.bot_id,
                        name: &routine.name,
                        prompt: &routine.prompt,
                        schedule: &routine.schedule,
                        precheck: routine.precheck.as_ref(),
                        deliver: &routine.deliver,
                        continuity: &routine.continuity,
                        enabled: false,
                        created_by: by,
                    },
                );
                match saved {
                    Ok(saved) => added.routines.push((
                        saved["id"].as_str().unwrap_or_default().to_string(),
                        routine.name.clone(),
                        bot.bot_id.clone(),
                    )),
                    Err(error) => skipped.push(format!(
                        "{}'s routine {}: {}",
                        bot.name, routine.name, error.message
                    )),
                }
            }
        }
        Ok(skipped)
    })();
    let skipped = match outcome {
        Ok(skipped) => skipped,
        Err(error) => {
            // Nothing half-imported stays behind.
            undo_added(db, &added, by, remove);
            return Err(DomainStateError {
                code: error.code,
                message: format!("Nothing was imported: {}", error.message),
            });
        }
    };
    let mut bots = Vec::new();
    for bot_id in &added.bots {
        if let Some(bot) = read_bot(db, bot_id)? {
            bots.push(bot);
        }
    }
    let departments_now: Vec<Department> = added
        .departments
        .iter()
        .filter_map(|department| {
            read_department(db, &department.department_id)
                .ok()
                .flatten()
        })
        .collect();
    let after = json!({
        "name": file.name,
        "kind": file.kind.as_str(),
        "bots": bots,
        "departments": departments_now.iter().map(|department| json!({
            "departmentId": department.department_id,
            "name": department.name,
        })).collect::<Vec<_>>(),
        "routines": added.routines.iter().map(|(id, name, bot_id)| json!({
            "id": id, "name": name, "botId": bot_id,
        })).collect::<Vec<_>>(),
    });
    let change_id = new_id("c_");
    db.execute(
        "INSERT INTO gbots_changes (changeId, kind, subjectId, beforeJson, afterJson, byWho, at) \
         VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6)",
        rusqlite::params![
            change_id,
            TEAM_CHANGE,
            new_id("t_"),
            after.to_string(),
            by,
            now_iso()
        ],
    )
    .map_err(sql)?;
    crate::gbots::presentation::refresh_presented_bots(db);
    crate::gbots::brief::refresh_all_instruction_files(&state.paths, db);
    let ids: Vec<String> = bots.iter().map(|bot| bot.bot_id.clone()).collect();
    crate::gbots::department_ops::redraw(state, db, repository, &ids);
    Ok(json!({
        "changeId": change_id,
        "name": file.name,
        "bots": bots.iter().map(crate::gbots::record::bot_summary).collect::<Vec<_>>(),
        "departments": after["departments"],
        "routines": after["routines"],
        "routinesPaused": true,
        "skipped": skipped,
        "notes": plan.notes,
        "approval": plan.approval,
    }))
}

/// The bots of an import as it saved them.
fn saved_bots(change: &Change) -> Vec<Bot> {
    change
        .after_raw
        .as_ref()
        .and_then(|after| after.get("bots"))
        .and_then(Value::as_array)
        .map(|bots| {
            bots.iter()
                .filter_map(|bot| serde_json::from_value::<Bot>(bot.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn same_persona(a: &Bot, b: &Bot) -> bool {
    let strip = |bot: &Bot| {
        let mut bot = bot.clone();
        bot.updated_at.clear();
        bot.created_at.clear();
        bot
    };
    strip(a) == strip(b)
}

/// Why an import can't be undone now; `None` when it can.
pub(crate) fn undo_problem(
    db: &Connection,
    change: &Change,
) -> Result<Option<String>, DomainStateError> {
    if change.undone_at.is_some() {
        return Ok(Some("That import was already undone.".to_string()));
    }
    let mut changed = Vec::new();
    let mut left = 0;
    for saved in saved_bots(change) {
        if let Some(current) = read_bot(db, &saved.bot_id)? {
            left += 1;
            if !same_persona(&current, &saved) {
                changed.push(current.name);
            }
        }
    }
    if !changed.is_empty() {
        return Ok(Some(format!(
            "{} changed since the import, so Undo would lose that change. Remove {} in GBot Settings first, or remove the GBots one by one.",
            changed.join(", "),
            if changed.len() == 1 { "it" } else { "them" }
        )));
    }
    if left == 0 {
        return Ok(Some(
            "Every GBot of that import was removed since.".to_string(),
        ));
    }
    Ok(None)
}

/// The import as `ghostex gbots changes` and the import dialog show it.
pub(crate) fn change_summary(db: &Connection, change: &Change) -> Result<Value, DomainStateError> {
    let after = change.after_raw.clone().unwrap_or(Value::Null);
    let problem = undo_problem(db, change)?;
    let bots: Vec<Value> = saved_bots(change)
        .iter()
        .map(|bot| json!({ "botId": bot.bot_id, "name": bot.name }))
        .collect();
    Ok(json!({
        "changeId": change.change_id,
        "kind": change.kind,
        "name": after.get("name").cloned().unwrap_or(Value::Null),
        "teamBots": bots,
        "departments": after.get("departments").cloned().unwrap_or(json!([])),
        "routines": after.get("routines").cloned().unwrap_or(json!([])),
        "by": change.by,
        "at": change.at,
        "undoneAt": change.undone_at,
        "undoable": problem.is_none(),
        "undoProblem": problem,
    }))
}

/// Undo of an import: its routines, its GBots (chats closed, folders to the Trash) and the
/// departments it added once they hold no other GBot.
pub(crate) fn undo(
    db: &Connection,
    change: &Change,
    by: &str,
    remove: &mut dyn FnMut(&str) -> Result<Value, DomainStateError>,
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
    let after = change.after_raw.clone().unwrap_or(Value::Null);
    let mut removed_routines = 0;
    for routine in after["routines"].as_array().cloned().unwrap_or_default() {
        if let Some(id) = routine["id"].as_str() {
            if crate::automations::routine_definition(db, id)?.is_some() {
                crate::automations::delete_routine(db, id)?;
                removed_routines += 1;
            }
        }
    }
    let mut removed_bots = Vec::new();
    for bot in saved_bots(change) {
        if read_bot(db, &bot.bot_id)?.is_some() {
            remove(&bot.bot_id)?;
            removed_bots.push(bot.name);
        }
    }
    let mut removed_departments = Vec::new();
    let mut kept_departments = Vec::new();
    let bots = list_bots(db)?;
    for department in after["departments"].as_array().cloned().unwrap_or_default() {
        let Some(id) = department["departmentId"].as_str() else {
            continue;
        };
        let Some(current) = read_department(db, id)? else {
            continue;
        };
        if bots
            .iter()
            .any(|bot| bot.department_id.as_deref() == Some(id))
        {
            kept_departments.push(current.name);
        } else {
            departments::remove(db, by, id)?;
            removed_departments.push(current.name);
        }
    }
    db.execute(
        "UPDATE gbots_changes SET undoneAt = ?2 WHERE changeId = ?1",
        rusqlite::params![change.change_id, now_iso()],
    )
    .map_err(sql)?;
    let change = crate::gbots::changes::read_change(db, &change.change_id)?
        .unwrap_or_else(|| change.clone());
    Ok(json!({
        "change": change_summary(db, &change)?,
        "done": {
            "removedBots": removed_bots,
            "removedRoutines": removed_routines,
            "removedDepartments": removed_departments,
            "keptDepartments": kept_departments,
        },
    }))
}
