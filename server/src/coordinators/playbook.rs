//! The orchestrator playbook in effect: the `ghostex-orchestrator` skill's body as Ghostex ships
//! it, followed by the user's playbook overrides (one text for every coordinator). `ghostex
//! orchestrator guide` prints it, and `ghostex orchestrator status` says when it changed since the
//! orchestrator last read it.
//!
//! Until 2026-11-07 the user edited full copies of the playbook, for all projects or for one
//! project (whose copy also covers its worktrees). A copy saved then stays in effect until the user
//! moves to Ghostex's playbook (`retireOldCopies`), which saves each copy as a Markdown file first;
//! no new copy can be saved.
//!
//! CDXC:Coordinators 2026-11-07 DECISION:
//! User: "Too complex man. Not many people will adjust this. Let us actually make people override parts of the original playbook, not edit it ok? So they have a section for overriding any part of the playbook so this way updates still work". One overrides text for every coordinator replaces the full-copy editor (role.rs `coordinator_guide_for` puts it last, ranked above the playbook). Migration: a copy saved before keeps working until the user switches, so no coordinator changes how it works without the user knowing; the editors say a copy is still in use and switching saves it as a file, so nothing the user wrote is lost.
//!
//! CDXC:Coordinators 2026-10-19 DECISION:
//! User: "ok i want to have a button to click on to set the orchestrator prompt and i want to be able to set a different orchestrator prompt for a specific project too (the orchestrator prompt that gets overriden for a project should apply for all it's worktrees too please)". Q310 "Edit the playbook itself": the editor shows the full playbook, changeable for all projects or for one project and its worktrees, and Reset to Default goes back to the playbook Ghostex ships (for a project, to the all-projects one). Q311 "Yes, existing ones too": orchestrators that already exist take an edited playbook at their next guide read. Q312 "Replace them with the playbook editor": the default standing instructions built first are gone; the same buttons (Settings > Chat, Settings > Projects, the New Orchestrator dialog) edit the playbook, and each orchestrator's own Goal & Instructions stay as they are. Also asked: Reset to Default needs a confirming second click and is disabled while the text matches its default, saving text equal to the default stores no edited copy, and an on/off switch turns an edited copy off without losing it.
//! SEE-ALSO: server/src/coordinators/role.rs (`coordinator_guide`, the shipped playbook), server/src/ghostex_cli/coordinator/playbook.rs (the CLI verbs), apps/desktop/src/app/window/settings_modal/tabs/orchestrator_playbook.rs and apps/desktop/src/app/window/new_coordinator_modal.rs (the editors), skills/ghostex-orchestrator/SKILL.md (tells the orchestrator the guide wins over the skill file).

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::records::{now_iso, sql_error, SessionKey};
use super::role::{coordinator_guide_for, coordinator_playbook};
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;

/// `{ all?: Copy, projects: { <projectId>: Copy }, overrides?: { text, updatedAt } }`; a copy is
/// `{ text, enabled, base, updatedAt }` (saved before 2026-11-07, see the module note).
const PLAYBOOKS_METADATA_KEY: &str = "coordinatorPlaybooks";
/// `{ "<projectId>:<sessionId>": "<guide version>" }`: the guide each orchestrator read last.
const GUIDE_READS_METADATA_KEY: &str = "coordinatorGuideReads";
/// `{ "<projectId>:<sessionId>": "<guide version>" | "new" }`: the guide version each orchestrator
/// was last told to re-read under a message, or `new` for one whose first prompt sends it to the
/// guide.
const GUIDE_NOTES_METADATA_KEY: &str = "coordinatorGuideNotes";
/// The note record of an orchestrator created since the notes existed, before its first guide read.
const NEW_COORDINATOR_NOTE: &str = "new";
/// An edited playbook never grows past this (the shipped one is about 16 KB).
pub const COORDINATOR_PLAYBOOK_MAX_CHARS: usize = 100_000;
/// The user's playbook overrides never grow past this.
pub const COORDINATOR_PLAYBOOK_OVERRIDES_MAX_CHARS: usize = 20_000;
/// Where `retireOldCopies` saves the full copies it removes, under gxserver's root folder.
const OLD_PLAYBOOKS_DIR: &str = "coordinators/old-playbooks";

pub const COORDINATOR_PLAYBOOK_ENDPOINTS: &[&str] = &[
    "/api/readCoordinatorPlaybook",
    "/api/setCoordinatorPlaybook",
    "/api/readCoordinatorGuide",
];

/// One edited copy of the playbook.
#[derive(Clone, Debug, Default)]
struct EditedPlaybook {
    text: String,
    enabled: bool,
    /// Hash of the playbook this copy replaced when it was saved, to tell when that one changed.
    base: String,
    updated_at: String,
}

impl EditedPlaybook {
    fn from_value(value: &Value) -> Option<Self> {
        let text = value.get("text")?.as_str()?.to_string();
        Some(Self {
            text,
            enabled: value.get("enabled").and_then(Value::as_bool) != Some(false),
            base: value
                .get("base")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            updated_at: value
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "text": self.text,
            "enabled": self.enabled,
            "base": self.base,
            "updatedAt": self.updated_at,
        })
    }
}

/// Where the playbook in effect comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybookSource {
    Project,
    AllProjects,
    Shipped,
}

impl PlaybookSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::AllProjects => "allProjects",
            Self::Shipped => "shipped",
        }
    }
}

fn digest(text: &str) -> String {
    let hash = Sha256::digest(text.trim().as_bytes());
    hash.iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_metadata(db: &Connection, key: &str) -> Result<Value, DomainStateError> {
    Ok(db
        .query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(sql_error)?
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({})))
}

fn write_metadata(db: &Connection, key: &str, value: &Value) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![key, value.to_string(), now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The stored copies: the all-projects one and each project's.
struct Playbooks {
    all: Option<EditedPlaybook>,
    projects: Map<String, Value>,
    /// The user's overrides, `{ text, updatedAt }`.
    overrides: Option<Value>,
}

impl Playbooks {
    fn read(db: &Connection) -> Result<Self, DomainStateError> {
        let stored = read_metadata(db, PLAYBOOKS_METADATA_KEY)?;
        Ok(Self {
            all: stored.get("all").and_then(EditedPlaybook::from_value),
            projects: stored
                .get("projects")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            overrides: stored
                .get("overrides")
                .filter(|value| !text_of(value, "text").trim().is_empty())
                .cloned(),
        })
    }

    fn write(&self, db: &Connection) -> Result<(), DomainStateError> {
        let mut value = json!({ "projects": Value::Object(self.projects.clone()) });
        if let Some(all) = &self.all {
            value["all"] = all.to_value();
        }
        if let Some(overrides) = &self.overrides {
            value["overrides"] = overrides.clone();
        }
        write_metadata(db, PLAYBOOKS_METADATA_KEY, &value)
    }

    fn overrides_text(&self) -> String {
        self.overrides
            .as_ref()
            .map(|value| text_of(value, "text").trim().to_string())
            .unwrap_or_default()
    }

    /// Stores `text` as the overrides; blank text removes them.
    fn set_overrides(&mut self, text: &str) {
        let text = text.trim();
        self.overrides = (!text.is_empty())
            .then(|| json!({ "text": text, "updatedAt": Utc::now().to_rfc3339() }));
    }

    fn project(&self, project_id: &str) -> Option<EditedPlaybook> {
        self.projects
            .get(project_id)
            .and_then(EditedPlaybook::from_value)
    }

    /// What every project without its own copy uses, and what a project's Reset goes back to.
    fn all_projects_in_effect(&self) -> (String, PlaybookSource) {
        match &self.all {
            Some(all) if all.enabled => (all.text.clone(), PlaybookSource::AllProjects),
            _ => (shipped_playbook(), PlaybookSource::Shipped),
        }
    }
}

/// The playbook Ghostex ships: the skill's body.
/// The edited copies as settings sync carries them, `{text, enabled}`: the all-projects one and
/// each project's, by project id.
pub fn synced_playbooks(
    db: &Connection,
) -> Result<(Option<Value>, Vec<(String, Value)>), DomainStateError> {
    let playbooks = Playbooks::read(db)?;
    let carried = |copy: &EditedPlaybook| json!({ "text": copy.text, "enabled": copy.enabled });
    let projects = playbooks
        .projects
        .keys()
        .filter_map(|project_id| {
            Some((project_id.clone(), carried(&playbooks.project(project_id)?)))
        })
        .collect();
    Ok((playbooks.all.as_ref().map(carried), projects))
}

/// Writes copies settings sync brought from another computer: `all` when it changed (`Some(None)`
/// removes it) and each changed project's (`None` removes it). Each is based on the playbook it
/// replaces here, as an edit made here would be.
pub fn write_synced_playbooks(
    db: &Connection,
    all: Option<Option<&Value>>,
    projects: &[(String, Option<&Value>)],
) -> Result<(), DomainStateError> {
    let mut playbooks = Playbooks::read(db)?;
    let copy = |value: &Value, base: String| {
        Some(EditedPlaybook {
            text: value.get("text")?.as_str()?.to_string(),
            enabled: value.get("enabled").and_then(Value::as_bool) != Some(false),
            base,
            updated_at: now_iso(),
        })
    };
    if let Some(all) = all {
        playbooks.all = all.and_then(|value| copy(value, digest(&shipped_playbook())));
    }
    for (project_id, value) in projects {
        let base = digest(&playbooks.all_projects_in_effect().0);
        match value.and_then(|value| copy(value, base)) {
            Some(copy) => {
                playbooks
                    .projects
                    .insert(project_id.clone(), copy.to_value());
            }
            None => {
                playbooks.projects.remove(project_id);
            }
        }
    }
    playbooks.write(db)
}

/// The user's overrides as settings sync carries them, `{text}`, when there are any.
pub fn synced_playbook_overrides(db: &Connection) -> Result<Option<Value>, DomainStateError> {
    let text = Playbooks::read(db)?.overrides_text();
    Ok((!text.is_empty()).then(|| json!({ "text": text })))
}

/// Writes overrides settings sync brought from another computer (`None` removes them).
pub fn write_synced_playbook_overrides(
    db: &Connection,
    value: Option<&Value>,
) -> Result<(), DomainStateError> {
    let mut playbooks = Playbooks::read(db)?;
    let text = value
        .map(|value| text_of(value, "text"))
        .unwrap_or_default();
    if playbooks.overrides_text() == text.trim() {
        return Ok(());
    }
    playbooks.set_overrides(text);
    playbooks.write(db)
}

/// The user's playbook overrides (empty when there are none).
pub fn playbook_overrides(db: &Connection) -> Result<String, DomainStateError> {
    Ok(Playbooks::read(db)?.overrides_text())
}

pub fn shipped_playbook() -> String {
    coordinator_playbook().trim().to_string()
}

/// The project whose copy covers `project_id`: the project itself, or the parent checkout of a
/// worktree project (when that parent still exists).
fn playbook_owner(
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<Value, DomainStateError> {
    let project = repository.get_project(project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("No project matched \"{project_id}\"."))
    })?;
    let parent_id = project
        .pointer("/worktree/parentProjectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty());
    if let Some(parent_id) = parent_id {
        if let Some(parent) = repository.get_project(parent_id)? {
            return Ok(parent);
        }
    }
    Ok(project)
}

fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The playbook in effect for an orchestrator in `project_id` (none: outside any project): the
/// project's copy if on, else the all-projects copy if on, else the shipped one.
pub fn resolve_playbook(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: Option<&str>,
) -> Result<(String, PlaybookSource), DomainStateError> {
    let playbooks = Playbooks::read(db)?;
    if let Some(project_id) = project_id.filter(|id| !id.trim().is_empty()) {
        if let Ok(owner) = playbook_owner(repository, project_id) {
            if let Some(own) = playbooks.project(text_of(&owner, "projectId")) {
                if own.enabled {
                    return Ok((own.text, PlaybookSource::Project));
                }
            }
        }
    }
    Ok(playbooks.all_projects_in_effect())
}

/// One scope in the editor's view: its edited copy (if any), the text Reset goes back to, and
/// whether that default changed since the copy was saved.
fn scope_view(copy: Option<&EditedPlaybook>, default: &str) -> Value {
    match copy {
        Some(copy) => json!({
            "edited": true,
            "text": copy.text,
            "enabled": copy.enabled,
            "updatedAt": copy.updated_at,
            "defaultText": default,
            "defaultChanged": !copy.base.is_empty() && copy.base != digest(default),
        }),
        None => json!({
            "edited": false,
            "text": "",
            "enabled": true,
            "defaultText": default,
            "defaultChanged": false,
        }),
    }
}

fn playbook_view(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    let playbooks = Playbooks::read(db)?;
    let shipped = shipped_playbook();
    let (all_in_effect, _) = playbooks.all_projects_in_effect();
    let mut view = json!({
        "allProjects": scope_view(playbooks.all.as_ref(), &shipped),
    });
    if let Some(project_id) = project_id {
        let owner = playbook_owner(repository, project_id)?;
        let owner_id = text_of(&owner, "projectId");
        let mut project = scope_view(playbooks.project(owner_id).as_ref(), &all_in_effect);
        project["projectId"] = json!(owner_id);
        project["name"] = json!(text_of(&owner, "name"));
        project["requestedProjectId"] = json!(project_id);
        project["isWorktree"] = json!(owner_id != project_id);
        view["project"] = project;
    }
    let (_, source) = resolve_playbook(db, repository, project_id)?;
    view["source"] = json!(source.as_str());
    view["overrides"] = playbooks
        .overrides
        .clone()
        .unwrap_or_else(|| json!({ "text": "" }));
    view["oldCopies"] = Value::Array(old_copies(&playbooks, repository));
    Ok(view)
}

/// The full copies saved before overrides (see the module note): `{ scope: "allProjects" |
/// "project", projectId?, name?, enabled }`, the all-projects one first.
fn old_copies(playbooks: &Playbooks, repository: &DomainRepository<'_>) -> Vec<Value> {
    let mut copies = Vec::new();
    if let Some(all) = &playbooks.all {
        copies.push(json!({ "scope": "allProjects", "enabled": all.enabled }));
    }
    for project_id in playbooks.projects.keys() {
        let Some(copy) = playbooks.project(project_id) else {
            continue;
        };
        let name = repository
            .get_project(project_id)
            .ok()
            .flatten()
            .map(|project| text_of(&project, "name").to_string())
            .unwrap_or_default();
        copies.push(json!({
            "scope": "project",
            "projectId": project_id,
            "name": name,
            "enabled": copy.enabled,
        }));
    }
    copies
}

/// A file name made of `name`'s letters and digits, for an old copy's Markdown file.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if stem.is_empty() {
        "project".to_string()
    } else {
        stem
    }
}

/// Saves every full copy as a Markdown file in `dir` and removes them all, so every coordinator
/// follows Ghostex's playbook (and the overrides). Returns the files written.
fn retire_old_copies(
    db: &Connection,
    repository: &DomainRepository<'_>,
    dir: &std::path::Path,
) -> Result<Vec<String>, DomainStateError> {
    let mut playbooks = Playbooks::read(db)?;
    let mut copies: Vec<(String, String)> = Vec::new();
    if let Some(all) = &playbooks.all {
        copies.push(("all-projects".to_string(), all.text.clone()));
    }
    for project_id in playbooks.projects.keys() {
        if let Some(copy) = playbooks.project(project_id) {
            let name = repository
                .get_project(project_id)
                .ok()
                .flatten()
                .map(|project| text_of(&project, "name").to_string())
                .unwrap_or_else(|| project_id.clone());
            copies.push((format!("project-{}", file_stem(&name)), copy.text));
        }
    }
    if copies.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(dir).map_err(|error| {
        DomainStateError::bad_request(format!(
            "Could not make {} for the old playbooks: {error}",
            dir.display()
        ))
    })?;
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let mut saved = Vec::new();
    for (stem, text) in copies {
        let mut path = dir.join(format!("{stem}-{date}.md"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{stem}-{date}-{n}.md"));
            n += 1;
        }
        std::fs::write(&path, format!("{}\n", text.trim())).map_err(|error| {
            DomainStateError::bad_request(format!(
                "Could not save the old playbook to {}: {error}",
                path.display()
            ))
        })?;
        saved.push(path.display().to_string());
    }
    playbooks.all = None;
    playbooks.projects.clear();
    playbooks.write(db)?;
    Ok(saved)
}

fn param_text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Applies `/api/setCoordinatorPlaybook`: `text` saves a copy (text equal to the scope's default
/// stores none), `reset: true` removes the copy, `enabled` turns an existing copy on or off.
fn set_playbook(
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let mut playbooks = Playbooks::read(db)?;
    if let Some(overrides) = params.get("overrides") {
        let Some(text) = overrides.as_str() else {
            return Err(DomainStateError::bad_request("Pass the overrides as text."));
        };
        if text.trim().chars().count() > COORDINATOR_PLAYBOOK_OVERRIDES_MAX_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "The playbook overrides are limited to {COORDINATOR_PLAYBOOK_OVERRIDES_MAX_CHARS} characters."
            )));
        }
        playbooks.set_overrides(text);
        return playbooks.write(db);
    }
    let owner_id = match param_text(params, "projectId") {
        Some(project_id) => {
            Some(text_of(&playbook_owner(repository, project_id)?, "projectId").to_string())
        }
        None => None,
    };
    let default = match &owner_id {
        Some(_) => playbooks.all_projects_in_effect().0,
        None => shipped_playbook(),
    };
    let mut copy = match &owner_id {
        Some(owner_id) => playbooks.project(owner_id),
        None => playbooks.all.clone(),
    };
    let reset = params.get("reset").and_then(Value::as_bool) == Some(true);
    let text = params.get("text").and_then(Value::as_str);
    let enabled = params.get("enabled").and_then(Value::as_bool);
    if !reset && text.is_none() && enabled.is_none() {
        return Err(DomainStateError::bad_request(
            "Pass overrides, retireOldCopies: true, reset: true, or enabled: true or false.",
        ));
    }
    if !reset && text.is_some() {
        return Err(DomainStateError::bad_request(
            "Ghostex's playbook is no longer edited, so its updates keep arriving: write playbook overrides instead (`ghostex coordinator playbook --set-overrides`), which take precedence over the playbook.",
        ));
    }
    if reset {
        copy = None;
    } else if let Some(text) = text {
        let text = text.trim();
        if text.chars().count() > COORDINATOR_PLAYBOOK_MAX_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "The coordinator playbook is limited to {COORDINATOR_PLAYBOOK_MAX_CHARS} characters."
            )));
        }
        if text.is_empty() {
            return Err(DomainStateError::bad_request(
                "An empty playbook would leave coordinators without their role. Use Reset to Default to go back to the default playbook.",
            ));
        }
        copy = if text == default.trim() {
            None
        } else {
            Some(EditedPlaybook {
                text: text.to_string(),
                enabled: enabled.unwrap_or_else(|| copy.as_ref().is_none_or(|copy| copy.enabled)),
                base: digest(&default),
                updated_at: Utc::now().to_rfc3339(),
            })
        };
    } else if let Some(enabled) = enabled {
        let Some(existing) = copy.as_mut() else {
            return Err(DomainStateError::bad_request(
                "There is no edited playbook to turn on or off here; it already uses the default one.",
            ));
        };
        existing.enabled = enabled;
    }
    match owner_id {
        Some(owner_id) => match copy {
            Some(copy) => {
                playbooks.projects.insert(owner_id, copy.to_value());
            }
            None => {
                playbooks.projects.remove(&owner_id);
            }
        },
        None => playbooks.all = copy,
    }
    playbooks.write(db)
}

/// The guide (playbook in effect plus the reply format) for an orchestrator, and its version.
fn guide_for(
    db: &Connection,
    repository: &DomainRepository<'_>,
    paths: &GxserverPaths,
    project_id: Option<&str>,
) -> Result<(String, String, PlaybookSource), DomainStateError> {
    let (playbook, source) = resolve_playbook(db, repository, project_id)?;
    let overrides = playbook_overrides(db)?;
    let guide = coordinator_guide_for(&playbook, &overrides, paths);
    let version = digest(&guide);
    Ok((guide, version, source))
}

fn guide_read_key(key: &SessionKey) -> String {
    format!("{}:{}", key.0, key.1)
}

fn recorded<'a>(records: &'a Value, key: &SessionKey) -> Option<&'a str> {
    records.get(guide_read_key(key)).and_then(Value::as_str)
}

/// The line `ghostex orchestrator status` prints when the guide changed since this orchestrator
/// last read it, or when it never read one (an orchestrator older than the read record).
pub fn playbook_status_notice(
    db: &Connection,
    repository: &DomainRepository<'_>,
    key: &SessionKey,
) -> Result<Option<String>, DomainStateError> {
    let paths = crate::paths::get_gxserver_paths(None);
    let (_, version, _) = guide_for(db, repository, &paths, Some(&key.0))?;
    let reads = read_metadata(db, GUIDE_READS_METADATA_KEY)?;
    Ok(match recorded(&reads, key) {
        Some(read) if read == version => None,
        Some(_) => Some("Your playbook changed since you last read it (work items and their cards replace the board and your replies' status sections): run `ghostex coordinator guide` now and follow it from here on.".to_string()),
        None => Some("You have not read your playbook yet: run `ghostex coordinator guide` now and follow it from here on.".to_string()),
    })
}

/// Whether the next message to an orchestrator carries the note to re-read its guide: its last
/// read is not this version, it was not told about this version yet, and it is not a new
/// orchestrator still on its way to its first read (its first prompt sends it there).
fn guide_note_due(read: Option<&str>, noted: Option<&str>, version: &str) -> bool {
    if read == Some(version) || noted == Some(version) {
        return false;
    }
    !(read.is_none() && noted == Some(NEW_COORDINATOR_NOTE))
}

/// The note added under the next message to an orchestrator whose guide changed since it last
/// read it, and the guide version it is about; `None` when it is not due (see [`guide_note_due`]).
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User approved: "After an update changes the guide, every existing coordinator is told to re-read it at its next turn, with no action from the user." The status notice alone missed two cases: an orchestrator that never ran `ghostex coordinator guide` (it predates the read record) on the shipped playbook got none, and an old playbook may never send it to `status` again. So every message that reaches an orchestrator (the user's chat box, thread reports, `ghostex agents send`, queued rows) passes the chat send (session_chat_queue_runtime/send.rs), which adds this note under it, once per guide version: the note is recorded once the message is delivered, and reading the guide records the read, so neither repeats it. Idle orchestrators are not woken for it. A never-read orchestrator counts as changed on the shipped playbook too; one created since then is marked `new` at creation (`mark_new_coordinator`), because its first prompt already sends it to the guide.
pub fn playbook_update_note(
    db: &Connection,
    repository: &DomainRepository<'_>,
    key: &SessionKey,
) -> Result<Option<(String, String)>, DomainStateError> {
    if super::records::read_coordinator(db, &key.0, &key.1)?.is_none() {
        return Ok(None);
    }
    let paths = crate::paths::get_gxserver_paths(None);
    let (_, version, source) = guide_for(db, repository, &paths, Some(&key.0))?;
    let reads = read_metadata(db, GUIDE_READS_METADATA_KEY)?;
    let notes = read_metadata(db, GUIDE_NOTES_METADATA_KEY)?;
    if !guide_note_due(recorded(&reads, key), recorded(&notes, key), &version) {
        return Ok(None);
    }
    // Written in the chat core, which recognises the note under the turn the agent records
    // (packages/gx-chat-core/src/session/appended_notes.rs), so the user's message shows once.
    use ghostex_gx_chat_core::session::appended_notes::{
        PLAYBOOK_CHANGED_BY_UPDATE_NOTE, PLAYBOOK_CHANGED_NOTE,
    };
    // Ghostex's own playbook with no overrides can only have changed with an update.
    let note = match source {
        PlaybookSource::Shipped if playbook_overrides(db)?.is_empty() => {
            PLAYBOOK_CHANGED_BY_UPDATE_NOTE
        }
        _ => PLAYBOOK_CHANGED_NOTE,
    };
    Ok(Some((note.to_string(), version)))
}

/// Records that an orchestrator was told about guide `version`, so later messages go without it.
pub fn record_playbook_update_note(
    db: &Connection,
    key: &SessionKey,
    version: &str,
) -> Result<(), DomainStateError> {
    let mut notes = read_metadata(db, GUIDE_NOTES_METADATA_KEY)?;
    notes[guide_read_key(key)] = json!(version);
    write_metadata(db, GUIDE_NOTES_METADATA_KEY, &notes)
}

/// Marks a just-created orchestrator, whose first prompt sends it to the guide, so its messages
/// before that first read carry no note.
pub fn mark_new_coordinator(db: &Connection, key: &SessionKey) -> Result<(), DomainStateError> {
    let reads = read_metadata(db, GUIDE_READS_METADATA_KEY)?;
    let notes = read_metadata(db, GUIDE_NOTES_METADATA_KEY)?;
    if recorded(&reads, key).is_some() || recorded(&notes, key).is_some() {
        return Ok(());
    }
    record_playbook_update_note(db, key, NEW_COORDINATOR_NOTE)
}

/// `/api/readCoordinatorPlaybook` `{ projectId? }`, `/api/setCoordinatorPlaybook`
/// `{ overrides }` (blank removes them), `{ retireOldCopies: true }` (saves every old full copy
/// as a Markdown file, removes them all and answers their paths as `savedFiles`), or
/// `{ projectId?, reset? | enabled? }` for one old copy (a worktree's id means its project; no
/// `projectId`: the all-projects copy), and `/api/readCoordinatorGuide`
/// `{ projectId?, sessionId? }` (records the read for that orchestrator).
pub fn handle_coordinator_playbook_endpoint(
    endpoint_path: &str,
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = param_text(params, "projectId");
    match endpoint_path {
        "/api/readCoordinatorGuide" => {
            let paths = crate::paths::get_gxserver_paths(None);
            let (guide, version, source) = guide_for(db, repository, &paths, project_id)?;
            if let (Some(project_id), Some(session_id)) =
                (project_id, param_text(params, "sessionId"))
            {
                if super::records::read_coordinator(db, project_id, session_id)?.is_some() {
                    let mut reads = read_metadata(db, GUIDE_READS_METADATA_KEY)?;
                    reads[guide_read_key(&(project_id.to_string(), session_id.to_string()))] =
                        json!(version);
                    write_metadata(db, GUIDE_READS_METADATA_KEY, &reads)?;
                }
            }
            Ok(json!({ "guide": guide, "version": version, "source": source.as_str() }))
        }
        "/api/setCoordinatorPlaybook" => {
            if params.get("retireOldCopies").and_then(Value::as_bool) == Some(true) {
                let paths = crate::paths::get_gxserver_paths(None);
                let saved =
                    retire_old_copies(db, repository, &paths.root_dir.join(OLD_PLAYBOOKS_DIR))?;
                let mut view = playbook_view(db, repository, project_id)?;
                view["savedFiles"] = json!(saved);
                return Ok(view);
            }
            set_playbook(db, repository, params)?;
            playbook_view(db, repository, project_id)
        }
        _ => playbook_view(db, repository, project_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinators::insert_coordinator;
    use crate::paths::get_gxserver_paths;
    use crate::storage::{initialize_gxserver_storage, open_gxserver_database};

    fn params(entries: Value) -> Map<String, Value> {
        entries.as_object().cloned().expect("params object")
    }

    fn note(
        db: &Connection,
        repository: &DomainRepository<'_>,
        key: &SessionKey,
    ) -> Option<String> {
        playbook_update_note(db, repository, key)
            .expect("note")
            .map(|(note, _)| note)
    }

    fn read_guide(db: &Connection, repository: &DomainRepository<'_>, key: &SessionKey) {
        let mut request = Map::new();
        request.insert("projectId".into(), json!(key.0));
        request.insert("sessionId".into(), json!(key.1));
        handle_coordinator_playbook_endpoint("/api/readCoordinatorGuide", db, repository, &request)
            .expect("read guide");
    }

    #[test]
    fn the_note_is_due_for_an_unread_or_old_guide_once_per_version() {
        // Never read, older than the read record: due on the shipped playbook too.
        assert!(guide_note_due(None, None, "v2"));
        // Read an old version: due.
        assert!(guide_note_due(Some("v1"), None, "v2"));
        // Read the current version: not due.
        assert!(!guide_note_due(Some("v2"), None, "v2"));
        // Already told about this version: not due again.
        assert!(!guide_note_due(Some("v1"), Some("v2"), "v2"));
        assert!(!guide_note_due(None, Some("v2"), "v2"));
        // Told about an older version that changed again: due.
        assert!(guide_note_due(Some("v1"), Some("v1b"), "v2"));
        // A new orchestrator before its first read: not due; once it read, changes count.
        assert!(!guide_note_due(None, Some(NEW_COORDINATOR_NOTE), "v2"));
        assert!(guide_note_due(Some("v1"), Some(NEW_COORDINATOR_NOTE), "v2"));
    }

    #[test]
    fn orchestrators_are_told_to_reread_a_changed_guide_once() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = DomainRepository::new(&db, "S1");
        let project_id = repository
            .create_project(&params(
                json!({ "name": "Notes", "path": std::env::temp_dir() }),
            ))
            .expect("create project")["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        let old = (project_id.clone(), "OLD".to_string());
        let fresh = (project_id.clone(), "NEW".to_string());
        let stranger = (project_id.clone(), "NOTCOORD".to_string());
        insert_coordinator(&db, &old.0, &old.1, "", "").expect("old coordinator");
        insert_coordinator(&db, &fresh.0, &fresh.1, "", "").expect("new coordinator");
        mark_new_coordinator(&db, &fresh).expect("mark new");

        // An ordinary session never gets the note.
        assert_eq!(note(&db, &repository, &stranger), None);
        // Never read and on the shipped playbook: the old orchestrator is told, once.
        let (text, version) = playbook_update_note(&db, &repository, &old)
            .expect("note")
            .expect("due");
        assert!(text.starts_with("Ghostex was updated and your playbook changed"));
        record_playbook_update_note(&db, &old, &version).expect("record");
        assert_eq!(note(&db, &repository, &old), None);
        // The new orchestrator's first prompt sends it to the guide: no note before its first read.
        assert_eq!(note(&db, &repository, &fresh), None);

        // Both read the current guide: nothing due, and status is quiet.
        read_guide(&db, &repository, &old);
        read_guide(&db, &repository, &fresh);
        assert_eq!(note(&db, &repository, &old), None);
        assert_eq!(note(&db, &repository, &fresh), None);
        assert_eq!(
            playbook_status_notice(&db, &repository, &old).expect("status"),
            None
        );

        // New overrides change the guide: both are told (they read an old version).
        handle_coordinator_playbook_endpoint(
            "/api/setCoordinatorPlaybook",
            &db,
            &repository,
            &params(json!({ "overrides": "Never push without asking." })),
        )
        .expect("set overrides");
        let (text, version) = playbook_update_note(&db, &repository, &fresh)
            .expect("note")
            .expect("due");
        assert!(text.starts_with("Your playbook changed"));
        assert!(note(&db, &repository, &old).is_some());
        assert!(playbook_status_notice(&db, &repository, &fresh)
            .expect("status")
            .is_some());
        record_playbook_update_note(&db, &fresh, &version).expect("record");
        assert_eq!(note(&db, &repository, &fresh), None);
        // Reading the edited guide settles the old one without a note.
        read_guide(&db, &repository, &old);
        assert_eq!(note(&db, &repository, &old), None);
    }

    fn temp_db() -> (tempfile::TempDir, crate::storage::GxserverDatabase) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }

    #[test]
    fn overrides_follow_the_playbook_last_and_outrank_it() {
        let (temp, db) = temp_db();
        let repository = DomainRepository::new(&db, "S1");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let (plain, _, _) = guide_for(&db, &repository, &paths, None).expect("guide");
        assert!(!plain.contains(crate::coordinators::PLAYBOOK_OVERRIDES_HEADING));

        let view = handle_coordinator_playbook_endpoint(
            "/api/setCoordinatorPlaybook",
            &db,
            &repository,
            &params(
                json!({ "overrides": "  Never push without asking.\nIgnore the board rules.  " }),
            ),
        )
        .expect("set overrides");
        assert_eq!(
            view["overrides"]["text"],
            "Never push without asking.\nIgnore the board rules."
        );
        let (guide, _, source) = guide_for(&db, &repository, &paths, None).expect("guide");
        assert_eq!(source, PlaybookSource::Shipped);
        assert!(guide.starts_with(shipped_playbook().as_str()));
        let tail = &guide[guide
            .find(crate::coordinators::PLAYBOOK_OVERRIDES_HEADING)
            .expect("overrides heading")..];
        assert!(tail.contains(crate::coordinators::PLAYBOOK_OVERRIDES_PRECEDENCE));
        assert!(tail.trim_end().ends_with("Ignore the board rules."));

        // Blank overrides remove them.
        let view = handle_coordinator_playbook_endpoint(
            "/api/setCoordinatorPlaybook",
            &db,
            &repository,
            &params(json!({ "overrides": "   " })),
        )
        .expect("clear overrides");
        assert_eq!(view["overrides"]["text"], "");
        assert_eq!(playbook_overrides(&db).expect("overrides"), "");
    }

    #[test]
    fn no_new_full_copy_can_be_saved() {
        let (_temp, db) = temp_db();
        let repository = DomainRepository::new(&db, "S1");
        let error = handle_coordinator_playbook_endpoint(
            "/api/setCoordinatorPlaybook",
            &db,
            &repository,
            &params(json!({ "text": "My own playbook." })),
        )
        .expect_err("text refused");
        assert!(error.message.contains("--set-overrides"));
        assert!(Playbooks::read(&db).expect("read").all.is_none());
    }

    #[test]
    fn an_old_copy_stays_in_use_until_it_is_retired_to_a_file() {
        let (temp, db) = temp_db();
        let repository = DomainRepository::new(&db, "S1");
        let project_id = repository
            .create_project(&params(json!({ "name": "My App", "path": temp.path() })))
            .expect("create project")["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        // Copies saved before overrides (sync writes them the way the old editor did).
        let copy = json!({ "text": "Old edited playbook.", "enabled": true });
        let project_copy = json!({ "text": "Project playbook.", "enabled": true });
        write_synced_playbooks(
            &db,
            Some(Some(&copy)),
            &[(project_id.clone(), Some(&project_copy))],
        )
        .expect("old copies");
        handle_coordinator_playbook_endpoint(
            "/api/setCoordinatorPlaybook",
            &db,
            &repository,
            &params(json!({ "overrides": "Never push without asking." })),
        )
        .expect("set overrides");

        // Still in effect, with the overrides after it.
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let (guide, _, source) =
            guide_for(&db, &repository, &paths, Some(&project_id)).expect("guide");
        assert_eq!(source, PlaybookSource::Project);
        assert!(guide.starts_with("Project playbook."));
        assert!(guide.contains("Never push without asking."));
        let view = playbook_view(&db, &repository, Some(&project_id)).expect("view");
        let old = view["oldCopies"].as_array().expect("old copies");
        assert_eq!(old.len(), 2);
        assert_eq!(old[0]["scope"], "allProjects");
        assert_eq!(old[1]["name"], "My App");

        // Retiring saves both as files and moves everything to Ghostex's playbook.
        let dir = temp.path().join("old-playbooks");
        let saved = retire_old_copies(&db, &repository, &dir).expect("retire");
        assert_eq!(saved.len(), 2);
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let all_file = dir.join(format!("all-projects-{today}.md"));
        assert_eq!(
            std::fs::read_to_string(&all_file).expect("all file"),
            "Old edited playbook.\n"
        );
        assert!(dir.join(format!("project-my-app-{today}.md")).exists());
        let (guide, _, source) =
            guide_for(&db, &repository, &paths, Some(&project_id)).expect("guide");
        assert_eq!(source, PlaybookSource::Shipped);
        assert!(guide.starts_with(shipped_playbook().as_str()));
        assert!(guide.contains("Never push without asking."));
        let view = playbook_view(&db, &repository, None).expect("view");
        assert_eq!(view["oldCopies"], json!([]));
        // Nothing left to retire: no files.
        assert!(retire_old_copies(&db, &repository, &dir)
            .expect("again")
            .is_empty());
    }

    #[test]
    fn overrides_round_trip_through_settings_sync() {
        let (_temp, db) = temp_db();
        assert_eq!(synced_playbook_overrides(&db).expect("none"), None);
        write_synced_playbook_overrides(&db, Some(&json!({ "text": "Be brief." }))).expect("write");
        assert_eq!(
            synced_playbook_overrides(&db).expect("some"),
            Some(json!({ "text": "Be brief." }))
        );
        write_synced_playbook_overrides(&db, None).expect("remove");
        assert_eq!(playbook_overrides(&db).expect("overrides"), "");
    }
}
