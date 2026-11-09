//! `teamExport`: GBots written out as a team file (format.rs), or one GBot as a preset. Every text
//! field goes through the memory redaction (memory/redact.rs) first, and what it took out is
//! counted in the file's `leftOut` and in the answer.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::format::{
    slug, unique_key, FileBot, FileDepartment, FileRoutine, Kind, StarterNotes, TeamFile, FORMAT,
    MAX_FILE_BYTES, NOTES_MEMORY_MAX_BYTES, NOTES_TOPICS_MAX, NOTES_TOPIC_MAX_BYTES, VERSION,
};
use crate::coordinators::now_iso;
use crate::domain::DomainStateError;
use crate::gbots::departments::{list_departments, require_department, Department};
use crate::gbots::folder::{bot_folder, MEMORY_FILE};
use crate::gbots::memory::{read_memory_file, redact, topic_list};
use crate::gbots::record::{list_bots, require_bot, Bot};
use crate::paths::GxserverPaths;

/// What is redacted, counted by where it was.
#[derive(Default)]
struct Tally {
    lines: Vec<String>,
    total: usize,
}

impl Tally {
    fn scrub(&mut self, text: &str, whose: &str, what: &str) -> String {
        let (clean, url_count) = scrub_url_queries(text);
        let (clean, count) = redact(&clean);
        let count = count + url_count;
        if count > 0 {
            self.total += count;
            self.lines.push(format!(
                "{count} secret{} redacted in {whose}'s {what}",
                if count == 1 { "" } else { "s" }
            ));
        }
        clean
    }

    /// Every string inside `value` (a routine's pre-check).
    fn scrub_value(&mut self, value: &Value, whose: &str, what: &str) -> Value {
        match value {
            Value::String(text) => Value::String(self.scrub(text, whose, what)),
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|item| self.scrub_value(item, whose, what))
                    .collect(),
            ),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, item)| (key.clone(), self.scrub_value(item, whose, what)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// Query values of a URL that are secrets by their name (`?key=…`, `&token=…`) or by their shape.
const SECRET_QUERY_NAMES: [&str; 14] = [
    "key",
    "apikey",
    "api_key",
    "api-key",
    "token",
    "access_token",
    "auth",
    "auth_token",
    "secret",
    "client_secret",
    "password",
    "pass",
    "sig",
    "signature",
];

/// Redacts the secret values in every URL's query in `text` (memory's redaction reads
/// `key=sk-…` as one word and keeps it). Answers the text and how many it redacted.
///
/// CDXC:GBots 2026-11-09 WHY:
/// A routine's url check is the likeliest place for a key in a team file (`https://…?key=…`), and it travels to another Ghostex, so its query is cleaned by name and by shape before the memory redaction runs on the whole text.
fn scrub_url_queries(text: &str) -> (String, usize) {
    let mut count = 0;
    let words: Vec<String> = text
        .split(' ')
        .map(|word| {
            let Some((base, query)) = word.split_once('?') else {
                return word.to_string();
            };
            if !(base.starts_with("http://") || base.starts_with("https://")) {
                return word.to_string();
            }
            let pairs: Vec<String> = query
                .split('&')
                .map(|pair| match pair.split_once('=') {
                    Some((name, value)) if !value.is_empty() => {
                        let secret_name =
                            SECRET_QUERY_NAMES.contains(&name.to_ascii_lowercase().as_str());
                        let (clean, shaped) = redact(value);
                        if secret_name {
                            count += 1;
                            format!("{name}=«redacted {} chars»", value.chars().count())
                        } else {
                            count += shaped;
                            format!("{name}={clean}")
                        }
                    }
                    _ => pair.to_string(),
                })
                .collect();
            format!("{base}?{}", pairs.join("&"))
        })
        .collect();
    (words.join(" "), count)
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The bots the export holds and the file's kind and default name, from `preset`, `department`,
/// `bots` (a list of names or ids), or every GBot.
fn scope(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<(Kind, Vec<Bot>, String), DomainStateError> {
    if let Some(reference) = text(params, "preset") {
        let bot = require_bot(db, reference)?;
        let name = bot.name.clone();
        return Ok((Kind::Preset, vec![bot], name));
    }
    let all = list_bots(db)?;
    if let Some(reference) = text(params, "department") {
        let department = require_department(db, reference)?;
        let bots: Vec<Bot> = all
            .into_iter()
            .filter(|bot| bot.department_id.as_deref() == Some(department.department_id.as_str()))
            .collect();
        if bots.is_empty() {
            return Err(DomainStateError::bad_request(format!(
                "{} has no GBots to export.",
                department.name
            )));
        }
        return Ok((Kind::Team, bots, department.name));
    }
    if let Some(list) = params.get("bots").and_then(Value::as_array) {
        let mut bots = Vec::new();
        for reference in list.iter().filter_map(Value::as_str) {
            let bot = require_bot(db, reference)?;
            if !bots.iter().any(|known: &Bot| known.bot_id == bot.bot_id) {
                bots.push(bot);
            }
        }
        if !bots.is_empty() {
            return Ok((Kind::Team, bots, "GBots team".to_string()));
        }
    }
    if all.is_empty() {
        return Err(DomainStateError::bad_request(
            "There are no GBots to export yet.",
        ));
    }
    Ok((Kind::Team, all, "GBots team".to_string()))
}

/// The bot's `MEMORY.md` without the two lines every new GBot's starts with.
fn memory_notes(paths: &GxserverPaths, bot: &Bot) -> String {
    let folder = bot_folder(paths, &bot.bot_id);
    let text = read_memory_file(&folder, MEMORY_FILE).unwrap_or_default();
    let header = format!("# {}'s memory", bot.name);
    let intro = format!("What {} keeps between chats:", bot.name);
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| line.trim() != header && !line.trim().starts_with(&intro))
        .collect();
    let notes = kept.join("\n").trim().to_string();
    if notes.len() > NOTES_MEMORY_MAX_BYTES {
        crate::gbots::memory::within_budget(
            &notes,
            crate::gbots::memory::MEMORY_MAX_LINES,
            NOTES_MEMORY_MAX_BYTES,
        )
        .0
    } else {
        notes
    }
}

fn starter_notes(
    paths: &GxserverPaths,
    bot: &Bot,
    tally: &mut Tally,
    cut: &mut Vec<String>,
) -> StarterNotes {
    let folder = bot_folder(paths, &bot.bot_id);
    let memory = tally.scrub(&memory_notes(paths, bot), &bot.name, "memory");
    let mut topics = std::collections::BTreeMap::new();
    for topic in topic_list(&folder) {
        if topics.len() >= NOTES_TOPICS_MAX {
            cut.push(format!(
                "{}'s topics past the first {NOTES_TOPICS_MAX}",
                bot.name
            ));
            break;
        }
        if topic.bytes > NOTES_TOPIC_MAX_BYTES {
            cut.push(format!(
                "{}'s topic {} (larger than 64 KB)",
                bot.name, topic.stem
            ));
            continue;
        }
        let body = std::fs::read_to_string(&topic.path).unwrap_or_default();
        let what = format!("topic {}", topic.stem);
        topics.insert(topic.stem.clone(), tally.scrub(&body, &bot.name, &what));
    }
    StarterNotes { memory, topics }
}

fn routines(
    db: &Connection,
    bot: &Bot,
    key: &str,
    tally: &mut Tally,
) -> Result<Vec<FileRoutine>, DomainStateError> {
    let mut routines = Vec::new();
    for view in crate::automations::bot_routines(db, &bot.bot_id)? {
        let Some(id) = view.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(definition) = crate::automations::routine_definition(db, id)? else {
            continue;
        };
        let field = |name: &str| {
            definition
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let name = field("name");
        let what = format!("routine {name}");
        routines.push(FileRoutine {
            bot: key.to_string(),
            prompt: tally.scrub(&field("prompt"), &bot.name, &what),
            name,
            schedule: definition["schedule"].clone(),
            precheck: Some(&definition["precheck"])
                .filter(|value| value.is_object())
                .map(|value| tally.scrub_value(value, &bot.name, &what)),
            deliver: field("deliver"),
            continuity: Some(field("continuity"))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "new".to_string()),
        });
    }
    Ok(routines)
}

/// The file `params` asks for: `preset`, `department` or `bots` (else every GBot), `notes`
/// (starter notes), `name`, `summary`, `release`.
pub(crate) fn build(
    paths: &GxserverPaths,
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<(TeamFile, usize), DomainStateError> {
    let (kind, bots, default_name) = scope(db, params)?;
    let with_notes = params.get("notes").and_then(Value::as_bool) == Some(true);
    let all_departments = list_departments(db)?;
    let mut tally = Tally::default();
    let mut cut = Vec::new();
    // Each bot's key, by its id.
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for bot in &bots {
        let key = unique_key(&slug(&bot.name), &taken);
        taken.push(key.clone());
        keys.push((bot.bot_id.clone(), key));
    }
    let key_of = |bot_id: &str| {
        keys.iter()
            .find(|(id, _)| id == bot_id)
            .map(|(_, key)| key.clone())
    };
    // The departments the bots are in (every department for a whole-team export).
    let whole_team =
        kind == Kind::Team && text(params, "department").is_none() && params.get("bots").is_none();
    let included: Vec<&Department> = all_departments
        .iter()
        .filter(|department| {
            kind == Kind::Team
                && (whole_team
                    || bots.iter().any(|bot| {
                        bot.department_id.as_deref() == Some(department.department_id.as_str())
                    }))
        })
        .collect();
    let mut department_keys: Vec<(String, String)> = Vec::new();
    let mut taken_departments: Vec<String> = Vec::new();
    let departments: Vec<FileDepartment> = included
        .iter()
        .map(|department| {
            let key = unique_key(&slug(&department.name), &taken_departments);
            taken_departments.push(key.clone());
            department_keys.push((department.department_id.clone(), key.clone()));
            FileDepartment {
                key,
                name: department.name.clone(),
                icon: department.icon.clone(),
                lead: department.lead_bot_id.as_deref().and_then(key_of),
                instructions: tally.scrub(
                    &department.instructions,
                    &department.name,
                    "shared instructions",
                ),
            }
        })
        .collect();
    let mut file_bots = Vec::new();
    let mut file_routines = Vec::new();
    for bot in &bots {
        let key = key_of(&bot.bot_id).unwrap_or_default();
        let department = bot.department_id.as_deref().and_then(|id| {
            department_keys
                .iter()
                .find(|(department_id, _)| department_id == id)
                .map(|(_, key)| key.clone())
        });
        file_bots.push(FileBot {
            key: key.clone(),
            name: bot.name.clone(),
            title: tally.scrub(&bot.title, &bot.name, "title"),
            description: tally.scrub(&bot.description, &bot.name, "description"),
            icon: bot.icon.clone(),
            color: bot.color.clone(),
            department,
            instructions: tally.scrub(&bot.soul, &bot.name, "instructions"),
            starter_notes: if with_notes {
                starter_notes(paths, bot, &mut tally, &mut cut)
            } else {
                StarterNotes::default()
            },
        });
        if kind == Kind::Team {
            file_routines.extend(routines(db, bot, &key, &mut tally)?);
        }
    }
    let mut left_out = tally.lines;
    left_out.extend(cut);
    let file = TeamFile {
        format: FORMAT.to_string(),
        version: VERSION,
        kind,
        name: text(params, "name")
            .map(str::to_string)
            .unwrap_or(default_name)
            .chars()
            .take(super::format::TEAM_NAME_MAX_CHARS)
            .collect(),
        summary: text(params, "summary")
            .unwrap_or_default()
            .chars()
            .take(super::format::SUMMARY_MAX_CHARS)
            .collect(),
        release: text(params, "release")
            .unwrap_or_default()
            .chars()
            .take(40)
            .collect(),
        exported_at: now_iso(),
        departments,
        bots: file_bots,
        routines: file_routines,
        left_out,
    };
    Ok((file, tally.total))
}

/// `teamExport`: the file, the name to save it under, and what was left out of it.
pub(crate) fn export(
    paths: &GxserverPaths,
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (file, redacted) = build(paths, db, params)?;
    let value = serde_json::to_value(&file).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("The team file could not be written: {error}"),
    })?;
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    if text.len() > MAX_FILE_BYTES {
        return Err(DomainStateError::bad_request(
            "That team is larger than 1 MB as a file. Export it without starter notes, or one department at a time.",
        ));
    }
    let routines_paused = !file.routines.is_empty();
    Ok(json!({
        "file": value,
        "text": format!("{text}\n"),
        "fileName": format!("{}{}", slug(&file.name), file.kind.extension()),
        "kind": file.kind.as_str(),
        "bots": file.bots.len(),
        "departments": file.departments.len(),
        "routines": file.routines.len(),
        "redacted": redacted,
        "leftOut": file.left_out,
        "neverIncluded": "Chat history, the agent and model each GBot runs on, approval levels, computers, keys and sign-ins are never in a team file.",
        "routinesArrivePaused": routines_paused,
    }))
}
