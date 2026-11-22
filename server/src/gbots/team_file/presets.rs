//! Presets: a GBot to start from in New GBot (name, role, description, look, instructions and
//! starter notes). A few are built in; the ones you import are kept in
//! `<app data>/gbots/presets.json`. A preset file is a team file with `"kind": "preset"`.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! New GBot fills its fields from a preset and you change them before you add it, so a preset never adds anything by itself; the GBot's name always comes from the form (or `--name`), so adding from a preset asks for your approval like any add (server/src/server/security_gates.rs). Importing a preset only keeps it for New GBot, and only you and Chief of Staff keep or remove presets.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use super::format::{read, FileBot, Kind, StarterNotes, TeamFile, FORMAT, VERSION};
use crate::coordinators::now_iso;
use crate::domain::DomainStateError;
use crate::gbots::folder::{gbots_dir, write_atomic};
use crate::gbots::memory::redact;
use crate::gbots::record::{color_hex, new_id};
use crate::paths::GxserverPaths;

/// Presets you may keep beside the built-in ones.
const MAX_SAVED: usize = 50;
const PRESETS_FILE: &str = "presets.json";

/// Built-in presets: id, name, role, icon, colour, description, instructions.
const BUILT_IN: [(&str, &str, &str, &str, &str, &str, &str); 5] = [
    (
        "researcher",
        "Researcher",
        "Finds and compares options",
        "microscope",
        "sky",
        "Looks things up, compares the options and brings back a short list with sources.",
        "You research what you are asked about and come back with a short answer first, then the detail.\n\n- Compare at least three options when there is a choice to make, with what each costs and its main trade-off.\n- Name your sources with links, and say how sure you are when they disagree.\n- Keep what you learn about the user's preferences in your memory so the next answer starts from it.",
    ),
    (
        "writer",
        "Writer",
        "Drafts and edits text",
        "pencil",
        "lavender",
        "Drafts emails, posts and documents in the user's voice, and edits what they wrote.",
        "You draft and edit text for the user.\n\n- Match the user's voice: short sentences, plain words, no filler.\n- Give one draft, then two or three short notes on what you would change if they want it different.\n- Never send or publish anything yourself; hand the draft back.\n- Keep the user's style preferences in your memory.",
    ),
    (
        "planner",
        "Planner",
        "Turns goals into next steps",
        "checklist",
        "mint",
        "Breaks a goal into steps, keeps the list current and says what to do next.",
        "You turn the user's goals into plans they can act on.\n\n- Break a goal into steps small enough to finish in a day, in the order they should happen.\n- Say what is next and what is blocked, and why.\n- Keep the plan and its progress in your memory, so every chat starts from where it stands.",
    ),
    (
        "reviewer",
        "Code reviewer",
        "Reviews changes for bugs",
        "code",
        "coral",
        "Reads a change, finds real bugs and risky spots, and says how to fix them.",
        "You review code changes.\n\n- Look for real bugs first: wrong logic, missed cases, data loss, security problems. Then readability.\n- For each finding give the file and line, what goes wrong and a fix.\n- Say plainly when a change looks fine; don't invent problems.\n- Don't change files unless you are asked to.",
    ),
    (
        "notes",
        "Note keeper",
        "Keeps notes and answers from them",
        "book",
        "gold",
        "Keeps what you tell it in topic notes and answers questions from them later.",
        "You keep the user's notes.\n\n- When the user tells you something worth keeping, add it to your memory or to a topic note with `ghostex gbots memory`.\n- When asked, answer from your notes first and say which note it came from.\n- Keep notes short and dated; merge duplicates instead of adding new ones.",
    ),
];

fn presets_path(paths: &GxserverPaths) -> PathBuf {
    gbots_dir(paths).join(PRESETS_FILE)
}

fn internal(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("The GBot presets could not be saved: {error}"),
    }
}

/// One preset: its id, whether it is built in, where it came from, and its GBot.
pub(crate) struct Preset {
    pub preset_id: String,
    pub built_in: bool,
    pub source: String,
    pub saved_at: Option<String>,
    pub bot: FileBot,
}

impl Preset {
    pub fn summary(&self, full: bool) -> Value {
        let bot = &self.bot;
        let mut summary = json!({
            "presetId": self.preset_id,
            "builtIn": self.built_in,
            "source": self.source,
            "savedAt": self.saved_at,
            "name": bot.name,
            "title": bot.title,
            "description": bot.description,
            "icon": bot.icon,
            "color": bot.color,
            "colorHex": color_hex(&bot.color),
            "starterNotes": {
                "memoryLines": bot.starter_notes.memory.lines().filter(|line| !line.trim().is_empty()).count(),
                "topics": bot.starter_notes.topics.keys().collect::<Vec<_>>(),
            },
        });
        if full {
            summary["instructions"] = json!(bot.instructions);
        } else {
            summary["instructionsChars"] = json!(bot.instructions.chars().count());
        }
        summary
    }
}

fn built_in() -> Vec<Preset> {
    BUILT_IN
        .iter()
        .map(|(id, name, title, icon, color, description, soul)| Preset {
            preset_id: (*id).to_string(),
            built_in: true,
            source: "Built in".to_string(),
            saved_at: None,
            bot: FileBot {
                key: (*id).to_string(),
                name: (*name).to_string(),
                title: (*title).to_string(),
                description: (*description).to_string(),
                icon: (*icon).to_string(),
                color: (*color).to_string(),
                department: None,
                instructions: (*soul).to_string(),
                starter_notes: StarterNotes::default(),
            },
        })
        .collect()
}

/// The saved presets as stored: `{ "presets": [{ presetId, savedAt, source, file }] }`.
fn read_saved_raw(paths: &GxserverPaths) -> Vec<Value> {
    std::fs::read_to_string(presets_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("presets").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

fn write_saved_raw(paths: &GxserverPaths, presets: &[Value]) -> Result<(), DomainStateError> {
    let text = serde_json::to_string_pretty(&json!({ "presets": presets })).map_err(internal)?;
    write_atomic(&presets_path(paths), &format!("{text}\n")).map_err(internal)
}

fn saved(paths: &GxserverPaths) -> Vec<Preset> {
    read_saved_raw(paths)
        .iter()
        .filter_map(|entry| {
            let file = entry.get("file")?;
            let read = read(&file.to_string()).ok()?;
            let bot = read.file.bots.into_iter().next()?;
            Some(Preset {
                preset_id: entry.get("presetId")?.as_str()?.to_string(),
                built_in: false,
                source: entry
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("Imported")
                    .to_string(),
                saved_at: entry
                    .get("savedAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                bot,
            })
        })
        .collect()
}

/// Every preset: the built-in ones, then yours, newest first.
pub(crate) fn all(paths: &GxserverPaths) -> Vec<Preset> {
    let mut presets = built_in();
    let mut mine = saved(paths);
    mine.reverse();
    presets.extend(mine);
    presets
}

/// The preset `reference` names: its id, or its name (case-blind).
pub(crate) fn find(paths: &GxserverPaths, reference: &str) -> Result<Preset, DomainStateError> {
    let wanted = reference.trim().to_lowercase();
    all(paths)
        .into_iter()
        .find(|preset| preset.preset_id == reference.trim() || preset.bot.name.to_lowercase() == wanted)
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "There is no GBot preset called {reference}. `ghostex gbots preset list` lists them."
            ))
        })
}

/// Keeps a preset file's GBot for New GBot. Answers the preset.
pub(crate) fn save(
    paths: &GxserverPaths,
    file: TeamFile,
    source: &str,
) -> Result<Preset, DomainStateError> {
    let mut raw = read_saved_raw(paths);
    if raw.len() >= MAX_SAVED {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "You keep {MAX_SAVED} GBot presets, the most there can be. Remove one first."
            ),
        });
    }
    let mut file = file;
    file.kind = Kind::Preset;
    file.departments.clear();
    file.routines.clear();
    file.bots.truncate(1);
    for bot in &mut file.bots {
        bot.department = None;
        bot.instructions = redact(&bot.instructions).0;
        bot.description = redact(&bot.description).0;
        bot.starter_notes.memory = redact(&bot.starter_notes.memory).0;
        for text in bot.starter_notes.topics.values_mut() {
            *text = redact(text).0;
        }
    }
    let preset_id = new_id("p_");
    let saved_at = now_iso();
    let value = serde_json::to_value(&file).map_err(internal)?;
    raw.push(json!({
        "presetId": preset_id,
        "savedAt": saved_at,
        "source": source,
        "file": value,
    }));
    write_saved_raw(paths, &raw)?;
    find(paths, &preset_id)
}

pub(crate) fn remove(paths: &GxserverPaths, reference: &str) -> Result<Preset, DomainStateError> {
    let preset = find(paths, reference)?;
    if preset.built_in {
        return Err(DomainStateError::bad_request(
            "A built-in preset can't be removed.",
        ));
    }
    let raw: Vec<Value> = read_saved_raw(paths)
        .into_iter()
        .filter(|entry| {
            entry.get("presetId").and_then(Value::as_str) != Some(preset.preset_id.as_str())
        })
        .collect();
    write_saved_raw(paths, &raw)?;
    Ok(preset)
}

/// A preset as a file of its own (`presetExport` of a kept or built-in preset).
pub(crate) fn as_file(preset: &Preset) -> TeamFile {
    TeamFile {
        format: FORMAT.to_string(),
        version: VERSION,
        kind: Kind::Preset,
        name: preset.bot.name.clone(),
        summary: preset.bot.description.clone(),
        release: String::new(),
        exported_at: now_iso(),
        departments: Vec::new(),
        bots: vec![preset.bot.clone()],
        routines: Vec::new(),
        left_out: Vec::new(),
    }
}

/// `create` with `preset`: the fields the request leaves out come from the preset (never its
/// name). Answers the params to create with and the preset's GBot, for its starter notes.
pub(crate) fn with_preset(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<(Map<String, Value>, Option<FileBot>), DomainStateError> {
    let Some(reference) = params
        .get("preset")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    else {
        return Ok((params.clone(), None));
    };
    let preset = find(paths, reference)?;
    let mut merged = params.clone();
    merged.remove("preset");
    let bot = &preset.bot;
    for (key, value) in [
        ("title", &bot.title),
        ("description", &bot.description),
        ("soul", &bot.instructions),
        ("icon", &bot.icon),
        ("color", &bot.color),
    ] {
        let given = merged
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty());
        if !given && !value.is_empty() {
            merged.insert(key.to_string(), json!(value));
        }
    }
    Ok((merged, Some(preset.bot)))
}

/// `presets`: every preset, without their full instructions.
pub(crate) fn list(paths: &GxserverPaths) -> Value {
    json!({
        "presets": all(paths).iter().map(|preset| preset.summary(false)).collect::<Vec<_>>(),
    })
}
