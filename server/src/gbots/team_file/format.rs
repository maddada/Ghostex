//! The team file and preset format: one JSON file, `<name>.gbots-team.json` (a team) or
//! `<name>.gbots-preset.json` (one GBot), read and written with serde_json.
//!
//! ```json
//! { "format": "ghostex-gbots-team", "version": 1, "kind": "team",
//!   "name": "Travel desk", "summary": "…", "release": "1.0.0", "exportedAt": "…",
//!   "departments": [{ "key": "research", "name": "Research", "icon": "microscope",
//!                     "lead": "researcher", "instructions": "…" }],
//!   "bots": [{ "key": "researcher", "name": "Researcher", "title": "…", "description": "…",
//!              "icon": "microscope", "color": "sky", "department": "research",
//!              "instructions": "…",
//!              "starterNotes": { "memory": "…", "topics": { "airlines": "…" } } }],
//!   "routines": [{ "bot": "researcher", "name": "Fare watch", "schedule": { … },
//!                  "prompt": "…", "precheck": { … }, "deliver": "chief", "continuity": "new" }],
//!   "leftOut": ["2 secrets redacted in Researcher's instructions"] }
//! ```
//!
//! `key`s tie bots to departments and routines inside the file only. `schedule` and `precheck`
//! are the routines' own JSON (server/src/automations/). A preset is the same with
//! `"kind": "preset"`: exactly one bot, no departments or routines.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User: "no need to import, don't add yaml to gxserver, do our own thing as you see is best, make it json". So the format is Ghostex's own JSON, nothing is imported from any other bot tool, and gxserver gains no YAML library. A file never carries secrets, chat history, memory beyond starter notes, the agent or model a GBot runs on, its approval level, its computer, keys or sign-ins: only what describes the team.
//! CDXC:GBots 2026-11-09 WHY:
//! Unknown fields are ignored and listed in the preview, and a newer `version` still reads, so a file from a newer Ghostex imports on an older one with what it understands. Rooms are not in Ghostex yet: a file's `rooms` are listed as left out rather than refused.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::gbots::departments::{
    INSTRUCTIONS_MAX_CHARS as DEPARTMENT_INSTRUCTIONS_MAX_CHARS, MAX_DEPARTMENTS,
    NAME_MAX_CHARS as DEPARTMENT_NAME_MAX_CHARS,
};
use crate::gbots::record::{
    COLORS, DESCRIPTION_MAX_CHARS, MAX_BOTS, NAME_MAX_CHARS, SOUL_MAX_CHARS, TITLE_MAX_CHARS,
};
use crate::gbots::routines::{NAME_MAX_CHARS as ROUTINE_NAME_MAX_CHARS, PROMPT_MAX_CHARS};

pub(crate) const FORMAT: &str = "ghostex-gbots-team";
pub(crate) const VERSION: u64 = 1;
/// A file's size limit, and how many of each thing it may hold.
pub(crate) const MAX_FILE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_FILE_BOTS: usize = MAX_BOTS;
pub(crate) const MAX_FILE_DEPARTMENTS: usize = MAX_DEPARTMENTS;
pub(crate) const MAX_FILE_ROUTINES: usize = 100;
/// A team's name and summary.
pub(crate) const TEAM_NAME_MAX_CHARS: usize = 80;
pub(crate) const SUMMARY_MAX_CHARS: usize = 1_000;
/// Starter notes: `MEMORY.md` within its budget, and topic notes.
pub(crate) const NOTES_MEMORY_MAX_BYTES: usize = crate::gbots::memory::MEMORY_MAX_BYTES;
pub(crate) const NOTES_TOPICS_MAX: usize = 40;
pub(crate) const NOTES_TOPIC_MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Kind {
    Team,
    Preset,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::Preset => "preset",
        }
    }

    /// The file name's ending.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Team => ".gbots-team.json",
            Self::Preset => ".gbots-preset.json",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StarterNotes {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub memory: String,
    /// Topic name (its file stem) to its text.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub topics: BTreeMap<String, String>,
}

impl StarterNotes {
    pub fn is_empty(&self) -> bool {
        self.memory.trim().is_empty() && self.topics.is_empty()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileDepartment {
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub instructions: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileBot {
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub icon: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    #[serde(skip_serializing_if = "StarterNotes::is_empty")]
    pub starter_notes: StarterNotes,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileRoutine {
    pub bot: String,
    pub name: String,
    pub schedule: Value,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precheck: Option<Value>,
    pub deliver: String,
    pub continuity: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TeamFile {
    pub format: String,
    pub version: u64,
    pub kind: Kind,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub release: String,
    pub exported_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub departments: Vec<FileDepartment>,
    pub bots: Vec<FileBot>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routines: Vec<FileRoutine>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left_out: Vec<String>,
}

/// A file as read: what it holds, plus what Ghostex did not understand (`ignored`) and what keeps
/// it from importing (`problems`).
pub(crate) struct ReadFile {
    pub file: TeamFile,
    pub ignored: Vec<String>,
    pub problems: Vec<String>,
}

/// A key as the file uses it: lowercase letters, digits and dashes.
pub(crate) fn slug(text: &str) -> String {
    let mut slug = String::new();
    for character in text.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "gbot".to_string()
    } else {
        slug.chars().take(40).collect()
    }
}

/// `base`, or `base-2`, `base-3`, … whichever `taken` does not hold yet.
pub(crate) fn unique_key(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|key| key == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|key| !taken.iter().any(|taken| taken == key))
        .unwrap_or_else(|| base.to_string())
}

/// Reads one object's fields: `known` are taken, the rest are listed in `ignored` under `path`.
struct Fields<'a> {
    object: &'a Map<String, Value>,
    path: String,
}

impl<'a> Fields<'a> {
    fn new(
        object: &'a Map<String, Value>,
        path: String,
        known: &[&str],
        ignored: &mut Vec<String>,
    ) -> Self {
        for key in object.keys() {
            if !known.contains(&key.as_str()) {
                ignored.push(if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                });
            }
        }
        Self { object, path }
    }

    fn text(&self, key: &str) -> String {
        self.object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    }

    /// A text field within `max` characters; a longer one is a problem.
    fn limited(&self, key: &str, max: usize, problems: &mut Vec<String>) -> String {
        let value = self.text(key);
        if value.chars().count() > max {
            problems.push(format!(
                "{}{key} is longer than {max} characters.",
                self.prefix()
            ));
        }
        value
    }

    fn prefix(&self) -> String {
        if self.path.is_empty() {
            String::new()
        } else {
            format!("{}: ", self.path)
        }
    }
}

fn objects<'a>(
    root: &'a Map<String, Value>,
    key: &str,
    problems: &mut Vec<String>,
) -> Vec<&'a Map<String, Value>> {
    match root.get(key) {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let object = item.as_object();
                if object.is_none() {
                    problems.push(format!("{key}[{index}] is not an object."));
                }
                object
            })
            .collect(),
        Some(_) => {
            problems.push(format!("{key} is not a list."));
            Vec::new()
        }
    }
}

fn one_line(value: &str) -> bool {
    !value.contains(['\n', '\r'])
}

fn plain_icon(icon: &str) -> bool {
    icon.len() <= 40
        && icon
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn read_notes(
    fields: &Fields<'_>,
    ignored: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> StarterNotes {
    let Some(notes) = fields.object.get("starterNotes").and_then(Value::as_object) else {
        return StarterNotes::default();
    };
    let notes = Fields::new(
        notes,
        format!("{}.starterNotes", fields.path),
        &["memory", "topics"],
        ignored,
    );
    let memory = notes.text("memory");
    if memory.len() > NOTES_MEMORY_MAX_BYTES {
        problems.push(format!(
            "{}memory is larger than {} KB.",
            notes.prefix(),
            NOTES_MEMORY_MAX_BYTES / 1024
        ));
    }
    let mut topics = BTreeMap::new();
    if let Some(map) = notes.object.get("topics").and_then(Value::as_object) {
        for (name, text) in map {
            let Some(text) = text.as_str() else { continue };
            match crate::gbots::memory::topic_stem(name) {
                Ok(stem) => {
                    if text.len() > NOTES_TOPIC_MAX_BYTES {
                        problems.push(format!(
                            "{}topic {name} is larger than {} KB.",
                            notes.prefix(),
                            NOTES_TOPIC_MAX_BYTES / 1024
                        ));
                    }
                    topics.insert(stem, text.to_string());
                }
                Err(problem) => problems.push(format!("{}topic {name}: {problem}", notes.prefix())),
            }
        }
        if topics.len() > NOTES_TOPICS_MAX {
            problems.push(format!(
                "{}has more than {NOTES_TOPICS_MAX} topics.",
                notes.prefix()
            ));
        }
    }
    StarterNotes { memory, topics }
}

/// Reads a file's text. `Err` when it is not a GBots file at all; problems with what it holds
/// come back in [`ReadFile::problems`].
pub(crate) fn read(text: &str) -> Result<ReadFile, String> {
    if text.len() > MAX_FILE_BYTES {
        return Err(format!(
            "That file is larger than {} MB, the most a team file may be.",
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("That file is not JSON ({error})."))?;
    let root = value
        .as_object()
        .ok_or_else(|| "That file is not a GBots team file.".to_string())?;
    if root.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(format!(
            "That file is not a GBots team file: it needs \"format\": \"{FORMAT}\"."
        ));
    }
    let mut ignored = Vec::new();
    let mut problems = Vec::new();
    let top = Fields::new(
        root,
        String::new(),
        &[
            "format",
            "version",
            "kind",
            "name",
            "summary",
            "release",
            "exportedAt",
            "departments",
            "bots",
            "routines",
            "rooms",
            "leftOut",
        ],
        &mut ignored,
    );
    let version = root
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or(VERSION);
    let kind = match top.text("kind").as_str() {
        "" | "team" => Kind::Team,
        "preset" => Kind::Preset,
        other => {
            return Err(format!(
                "That file's kind is {other}; Ghostex reads team and preset files."
            ))
        }
    };
    let mut left_out: Vec<String> = root
        .get("leftOut")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|item| item.chars().take(200).collect())
                .take(50)
                .collect()
        })
        .unwrap_or_default();
    if let Some(rooms) = root
        .get("rooms")
        .and_then(Value::as_array)
        .filter(|rooms| !rooms.is_empty())
    {
        let count = rooms.len();
        left_out.push(format!(
            "{count} room{} (Ghostex has no GBot rooms yet)",
            if count == 1 { "" } else { "s" }
        ));
    }

    let mut departments = Vec::new();
    for (index, object) in objects(root, "departments", &mut problems)
        .into_iter()
        .enumerate()
    {
        let fields = Fields::new(
            object,
            format!("departments[{index}]"),
            &["key", "name", "icon", "lead", "instructions"],
            &mut ignored,
        );
        let name = fields.limited("name", DEPARTMENT_NAME_MAX_CHARS, &mut problems);
        if name.is_empty() || !one_line(&name) {
            problems.push(format!("{}needs a one-line name.", fields.prefix()));
        }
        let icon = fields.text("icon");
        if !plain_icon(&icon) {
            problems.push(format!(
                "{}icon {icon} is not a Ghostex icon name.",
                fields.prefix()
            ));
        }
        let key = Some(fields.text("key"))
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| slug(&name));
        departments.push(FileDepartment {
            key,
            name,
            icon,
            lead: Some(fields.text("lead")).filter(|lead| !lead.is_empty()),
            instructions: fields.limited(
                "instructions",
                DEPARTMENT_INSTRUCTIONS_MAX_CHARS,
                &mut problems,
            ),
        });
    }

    let mut bots = Vec::new();
    for (index, object) in objects(root, "bots", &mut problems).into_iter().enumerate() {
        let fields = Fields::new(
            object,
            format!("bots[{index}]"),
            &[
                "key",
                "name",
                "title",
                "description",
                "icon",
                "color",
                "department",
                "instructions",
                "starterNotes",
            ],
            &mut ignored,
        );
        let name = fields.limited("name", NAME_MAX_CHARS, &mut problems);
        if name.is_empty() || !one_line(&name) {
            problems.push(format!("{}needs a one-line name.", fields.prefix()));
        }
        let title = fields.limited("title", TITLE_MAX_CHARS, &mut problems);
        if !one_line(&title) {
            problems.push(format!("{}title is more than one line.", fields.prefix()));
        }
        let icon = Some(fields.text("icon"))
            .filter(|icon| !icon.is_empty())
            .unwrap_or_else(|| crate::gbots::record::DEFAULT_ICON.to_string());
        if !plain_icon(&icon) {
            problems.push(format!(
                "{}icon {icon} is not a Ghostex icon name.",
                fields.prefix()
            ));
        }
        let color = Some(fields.text("color").to_lowercase())
            .filter(|color| !color.is_empty())
            .unwrap_or_else(|| COLORS[0].0.to_string());
        if !COLORS.iter().any(|(id, _)| *id == color) {
            problems.push(format!(
                "{}colour {color} is not one of {}.",
                fields.prefix(),
                COLORS.map(|(id, _)| id).join(", ")
            ));
        }
        let key = Some(fields.text("key"))
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| slug(&name));
        bots.push(FileBot {
            key,
            name,
            title,
            description: fields.limited("description", DESCRIPTION_MAX_CHARS, &mut problems),
            icon,
            color,
            department: Some(fields.text("department")).filter(|department| !department.is_empty()),
            instructions: fields.limited("instructions", SOUL_MAX_CHARS, &mut problems),
            starter_notes: read_notes(&fields, &mut ignored, &mut problems),
        });
    }

    let mut routines = Vec::new();
    for (index, object) in objects(root, "routines", &mut problems)
        .into_iter()
        .enumerate()
    {
        let fields = Fields::new(
            object,
            format!("routines[{index}]"),
            &[
                "bot",
                "name",
                "schedule",
                "prompt",
                "precheck",
                "deliver",
                "continuity",
            ],
            &mut ignored,
        );
        let name = fields.limited("name", ROUTINE_NAME_MAX_CHARS, &mut problems);
        let prompt = fields.limited("prompt", PROMPT_MAX_CHARS, &mut problems);
        if name.is_empty() || prompt.is_empty() {
            problems.push(format!("{}needs a name and a prompt.", fields.prefix()));
        }
        let schedule = object.get("schedule").cloned().unwrap_or(Value::Null);
        if !schedule.is_object() {
            problems.push(format!("{}needs a schedule.", fields.prefix()));
        }
        let deliver = Some(fields.text("deliver"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "chat".into());
        if !matches!(deliver.as_str(), "chat" | "chief" | "brief") {
            problems.push(format!(
                "{}deliver is chat, chief or brief.",
                fields.prefix()
            ));
        }
        let continuity = Some(fields.text("continuity"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "new".into());
        if !matches!(continuity.as_str(), "new" | "standing") {
            problems.push(format!("{}continuity is new or standing.", fields.prefix()));
        }
        routines.push(FileRoutine {
            bot: fields.text("bot"),
            name,
            schedule,
            prompt,
            precheck: object
                .get("precheck")
                .filter(|value| value.is_object())
                .cloned(),
            deliver,
            continuity,
        });
    }

    let file = TeamFile {
        format: FORMAT.to_string(),
        version,
        kind,
        name: top.limited("name", TEAM_NAME_MAX_CHARS, &mut problems),
        summary: top.limited("summary", SUMMARY_MAX_CHARS, &mut problems),
        release: top.text("release").chars().take(40).collect(),
        exported_at: top.text("exportedAt").chars().take(40).collect(),
        departments,
        bots,
        routines,
        left_out,
    };
    check_links(&file, &mut problems);
    Ok(ReadFile {
        file,
        ignored,
        problems,
    })
}

/// The counts, the keys and what refers to what.
fn check_links(file: &TeamFile, problems: &mut Vec<String>) {
    if file.bots.is_empty() {
        problems.push("The file holds no GBots.".to_string());
    }
    if file.kind == Kind::Preset
        && (file.bots.len() != 1 || !file.departments.is_empty() || !file.routines.is_empty())
    {
        problems
            .push("A preset holds exactly one GBot, with no departments or routines.".to_string());
    }
    if file.bots.len() > MAX_FILE_BOTS {
        problems.push(format!("The file holds more than {MAX_FILE_BOTS} GBots."));
    }
    if file.departments.len() > MAX_FILE_DEPARTMENTS {
        problems.push(format!(
            "The file holds more than {MAX_FILE_DEPARTMENTS} departments."
        ));
    }
    if file.routines.len() > MAX_FILE_ROUTINES {
        problems.push(format!(
            "The file holds more than {MAX_FILE_ROUTINES} routines."
        ));
    }
    let mut bot_keys: Vec<String> = Vec::new();
    for bot in &file.bots {
        if bot_keys.contains(&bot.key) {
            problems.push(format!("Two GBots use the key {}.", bot.key));
        }
        bot_keys.push(bot.key.clone());
        if let Some(department) = &bot.department {
            if !file
                .departments
                .iter()
                .any(|known| &known.key == department)
            {
                problems.push(format!(
                    "{} is in the department {department}, which the file does not hold.",
                    bot.name
                ));
            }
        }
    }
    let mut department_keys: Vec<String> = Vec::new();
    for department in &file.departments {
        if department_keys.contains(&department.key) {
            problems.push(format!("Two departments use the key {}.", department.key));
        }
        department_keys.push(department.key.clone());
        if let Some(lead) = &department.lead {
            let member = file
                .bots
                .iter()
                .any(|bot| &bot.key == lead && bot.department.as_ref() == Some(&department.key));
            if !member {
                problems.push(format!(
                    "{}'s lead {lead} is not one of its GBots.",
                    department.name
                ));
            }
        }
    }
    for routine in &file.routines {
        if !bot_keys.contains(&routine.bot) {
            problems.push(format!(
                "The routine {} belongs to {}, which the file does not hold.",
                routine.name, routine.bot
            ));
        }
    }
}
