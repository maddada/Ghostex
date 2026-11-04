//! The orchestrator playbook in effect: the `ghostex-orchestrator` skill's body as Ghostex ships
//! it, unless the user edited it for all projects or for one project (whose copy also covers its
//! worktrees). `ghostex orchestrator guide` prints the one in effect for the orchestrator's
//! project, and `ghostex orchestrator status` says when it changed since the orchestrator last
//! read it.
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

/// `{ all?: Copy, projects: { <projectId>: Copy } }`; a copy is `{ text, enabled, base, updatedAt }`.
const PLAYBOOKS_METADATA_KEY: &str = "coordinatorPlaybooks";
/// `{ "<projectId>:<sessionId>": "<guide version>" }`: the guide each orchestrator read last.
const GUIDE_READS_METADATA_KEY: &str = "coordinatorGuideReads";
/// An edited playbook never grows past this (the shipped one is about 16 KB).
pub const COORDINATOR_PLAYBOOK_MAX_CHARS: usize = 100_000;

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
        })
    }

    fn write(&self, db: &Connection) -> Result<(), DomainStateError> {
        let mut value = json!({ "projects": Value::Object(self.projects.clone()) });
        if let Some(all) = &self.all {
            value["all"] = all.to_value();
        }
        write_metadata(db, PLAYBOOKS_METADATA_KEY, &value)
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
    Ok(view)
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
            "Pass text, reset: true, or enabled: true or false.",
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
    let guide = coordinator_guide_for(&playbook, paths);
    let version = digest(&guide);
    Ok((guide, version, source))
}

fn guide_read_key(key: &SessionKey) -> String {
    format!("{}:{}", key.0, key.1)
}

/// The line `ghostex orchestrator status` prints when the guide changed since this orchestrator
/// last read it (or, never having read one since this existed, when its playbook was edited).
pub fn playbook_status_notice(
    db: &Connection,
    repository: &DomainRepository<'_>,
    key: &SessionKey,
) -> Result<Option<String>, DomainStateError> {
    let paths = crate::paths::get_gxserver_paths(None);
    let (_, version, source) = guide_for(db, repository, &paths, Some(&key.0))?;
    let reads = read_metadata(db, GUIDE_READS_METADATA_KEY)?;
    let changed = match reads.get(guide_read_key(key)).and_then(Value::as_str) {
        Some(read) => read != version,
        None => source != PlaybookSource::Shipped,
    };
    Ok(changed.then(|| {
        "Your playbook changed since you last read it: run `ghostex coordinator guide` now and follow it from here on.".to_string()
    }))
}

/// `/api/readCoordinatorPlaybook` `{ projectId? }`, `/api/setCoordinatorPlaybook`
/// `{ projectId?, text? | reset? | enabled? }` (a worktree's id means its project; no `projectId`:
/// the all-projects copy) and `/api/readCoordinatorGuide` `{ projectId?, sessionId? }` (records
/// the read for that orchestrator).
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
            set_playbook(db, repository, params)?;
            playbook_view(db, repository, project_id)
        }
        _ => playbook_view(db, repository, project_id),
    }
}
