//! The workspaces document: the list of workspaces (name, color, letter, kind, Claude account) in
//! the metadata table, normalized on every read and write like the Spaces document.

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

const SIDEBAR_WORKSPACES_METADATA_KEY: &str = "sidebarWorkspaces";
const MAX_WORKSPACES: usize = 64;
const MAX_ID_CHARS: usize = 256;
const MAX_NAME_CHARS: usize = 128;
const MAX_MACHINE_ASSIGNMENTS: usize = 256;

/// The workspace every project and Space without a `workspaceId` belongs to. It always exists
/// and cannot be deleted.
pub const DEFAULT_WORKSPACE_ID: &str = "personal";
const DEFAULT_WORKSPACE_NAME: &str = "Personal";

/// Same rotation as Spaces and project collections, so a workspace tile sits in the same family
/// of colors as the Space tiles beside it.
const WORKSPACE_COLORS: [&str; 13] = [
    "#596fd1", "#3aa675", "#d6873f", "#d75b72", "#3f8fc7", "#b36ad4", "#8c9b45", "#c95353",
    "#c4a23d", "#2f9b95", "#7c6df2", "#4f5663", "#808080",
];

/// What a workspace is for. Work turns work mode on by default for its projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceKind {
    Work,
    Personal,
}

impl WorkspaceKind {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Personal => "personal",
        }
    }

    fn from_wire(value: Option<&Value>) -> Option<Self> {
        match value?.as_str()?.trim() {
            "work" => Some(Self::Work),
            "personal" => Some(Self::Personal),
            _ => None,
        }
    }
}

pub fn read_sidebar_workspaces(db: &Connection) -> Result<Value, DomainStateError> {
    let stored = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [SIDEBAR_WORKSPACES_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    let stored = stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    Ok(normalize_sidebar_workspaces_state(&stored))
}

pub(crate) fn write_sidebar_workspaces(
    db: &Connection,
    state: &Value,
) -> Result<Value, DomainStateError> {
    let normalized = normalize_sidebar_workspaces_state(state);
    let serialized = serde_json::to_string(&normalized).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Workspaces serialization error: {error}"),
    })?;
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![SIDEBAR_WORKSPACES_METADATA_KEY, serialized, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(normalized)
}

/// The kind of a workspace in a document; an unknown id reads as the default workspace's kind.
pub fn workspace_kind(state: &Value, workspace_id: &str) -> WorkspaceKind {
    let workspaces = state.get("workspaces");
    let kind_of = |id: &str| {
        workspaces
            .and_then(|workspaces| workspaces.get(id))
            .and_then(|workspace| WorkspaceKind::from_wire(workspace.get("kind")))
    };
    kind_of(workspace_id)
        .or_else(|| kind_of(DEFAULT_WORKSPACE_ID))
        .unwrap_or(WorkspaceKind::Personal)
}

pub fn workspace_exists(state: &Value, workspace_id: &str) -> bool {
    state
        .get("workspaces")
        .and_then(|workspaces| workspaces.get(workspace_id))
        .is_some()
}

/// The Claude account a workspace's agents use, when one is chosen.
pub fn workspace_claude_account_id(state: &Value, workspace_id: &str) -> Option<String> {
    state
        .get("workspaces")?
        .get(workspace_id)?
        .get("claudeAccountId")?
        .as_str()
        .map(str::to_string)
}

/// What a request is told when no workspace has the id or name it gave. It repeats nothing the
/// request sent: every domain error's message goes to the request log, and a stale name can
/// spell a credential the workspace stored.
pub const NO_SUCH_WORKSPACE: &str = "No workspace matched that id or name.";

/// The workspace `reference` names: a workspace id, else a workspace name (any case), so the
/// `ghostex workspace` verbs can take either.
pub fn find_workspace_id(state: &Value, reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    if workspace_exists(state, reference) {
        return Some(reference.to_string());
    }
    state
        .get("workspaces")?
        .as_object()?
        .iter()
        .find(|(_, workspace)| {
            workspace
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.trim().eq_ignore_ascii_case(reference))
        })
        .map(|(id, _)| id.clone())
}

/// Puts a remote machine's tab in a workspace of this computer.
///
/// CDXC:Workspaces 2026-10-09 WHY:
/// A remote machine's projects carry that machine's own workspace ids, which mean nothing here,
/// so the whole machine tab is placed instead: it shows in the windows of the workspace it was
/// assigned to on this computer (Personal until moved), and its sessions keep their own machine's
/// workspaces. The assignment is this computer's daemon's, beside its workspaces, so every client
/// of this computer agrees and deleting the workspace sends the tab back to Personal.
pub(crate) fn assign_machine_workspace_in(
    state: &Value,
    machine_id: &str,
    workspace_id: &str,
) -> Result<Value, DomainStateError> {
    let machine_id = machine_id.trim();
    if machine_id.is_empty() || machine_id.chars().count() > MAX_ID_CHARS {
        return Err(DomainStateError::bad_request("Pass machineId."));
    }
    if !workspace_exists(state, workspace_id) {
        return Err(DomainStateError::bad_request("No such workspace."));
    }
    let mut next = state.clone();
    let Some(document) = next.as_object_mut() else {
        return Err(DomainStateError::bad_request("No such workspace."));
    };
    let machines = document
        .entry("machineWorkspaces")
        .or_insert_with(|| json!({}));
    if !machines.is_object() {
        *machines = json!({});
    }
    let machines = machines.as_object_mut().expect("set to an object above");
    if workspace_id == DEFAULT_WORKSPACE_ID {
        machines.remove(machine_id);
    } else {
        machines.insert(machine_id.to_string(), json!(workspace_id));
    }
    Ok(normalize_sidebar_workspaces_state(&next))
}

/// Adds a workspace built from `params` (`name`, optional `color`, `letter`, `kind`) and returns
/// its id with the new document.
pub(crate) fn create_workspace_in(
    state: &Value,
    params: &Map<String, Value>,
) -> Result<(String, Value), DomainStateError> {
    let name = bounded_text(params.get("name"), MAX_NAME_CHARS)
        .ok_or_else(|| DomainStateError::bad_request("A workspace needs a name."))?;
    refuse_other_trackers(params)?;
    let mut next = state.clone();
    let count = next
        .get("workspaces")
        .and_then(Value::as_object)
        .map_or(0, Map::len);
    if count >= MAX_WORKSPACES {
        return Err(DomainStateError::bad_request("Too many workspaces."));
    }
    let workspace_id = format!("ws-{}", uuid::Uuid::new_v4().simple());
    let mut workspace = Map::new();
    workspace.insert("workspaceId".into(), json!(workspace_id));
    workspace.insert("name".into(), json!(name));
    for key in ["color", "letter", "kind", "claudeAccountId", "tracker"] {
        if let Some(value) = params.get(key) {
            workspace.insert(key.into(), value.clone());
        }
    }
    if !workspace.contains_key("kind") {
        workspace.insert("kind".into(), json!(WorkspaceKind::Work.as_wire()));
    }
    if !workspace.contains_key("color") {
        workspace.insert(
            "color".into(),
            json!(WORKSPACE_COLORS[count % WORKSPACE_COLORS.len()]),
        );
    }
    if let Some(workspaces) = next.get_mut("workspaces").and_then(Value::as_object_mut) {
        workspaces.insert(workspace_id.clone(), Value::Object(workspace));
    }
    if let Some(order) = next.get_mut("order").and_then(Value::as_array_mut) {
        order.push(json!(workspace_id));
    }
    Ok((workspace_id, normalize_sidebar_workspaces_state(&next)))
}

/// Applies `params` (`name`, `color`, `letter`, `kind`, `claudeAccountId`, `tracker`; `null`
/// clears the account or the tracker) to one workspace.
pub(crate) fn update_workspace_in(
    state: &Value,
    workspace_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut next = state.clone();
    let workspace = next
        .get_mut("workspaces")
        .and_then(|workspaces| workspaces.get_mut(workspace_id))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| DomainStateError::bad_request("No such workspace."))?;
    // A blank name would be dropped by the normalizer, which then names the workspace after its
    // id; refuse it the way create does.
    if params.contains_key("name") && bounded_text(params.get("name"), MAX_NAME_CHARS).is_none() {
        return Err(DomainStateError::bad_request("A workspace needs a name."));
    }
    refuse_other_trackers(params)?;
    for key in [
        "name",
        "color",
        "letter",
        "kind",
        "claudeAccountId",
        "tracker",
    ] {
        match params.get(key) {
            Some(Value::Null) => {
                workspace.remove(key);
            }
            Some(value) => {
                workspace.insert(key.into(), value.clone());
            }
            None => {}
        }
    }
    Ok(normalize_sidebar_workspaces_state(&next))
}

pub(crate) fn delete_workspace_in(
    state: &Value,
    workspace_id: &str,
) -> Result<Value, DomainStateError> {
    if workspace_id == DEFAULT_WORKSPACE_ID {
        return Err(DomainStateError::bad_request(
            "The Personal workspace cannot be deleted.",
        ));
    }
    if !workspace_exists(state, workspace_id) {
        return Err(DomainStateError::bad_request("No such workspace."));
    }
    let mut next = state.clone();
    if let Some(workspaces) = next.get_mut("workspaces").and_then(Value::as_object_mut) {
        workspaces.remove(workspace_id);
    }
    Ok(normalize_sidebar_workspaces_state(&next))
}

/// Bounds every field, drops what does not fit, and always keeps the default workspace first in
/// a fresh document.
pub fn normalize_sidebar_workspaces_state(state: &Value) -> Value {
    let entries = state.get("workspaces").and_then(Value::as_object);
    let mut ordered_ids: Vec<String> = Vec::new();
    let push_id = |id: &str, ordered_ids: &mut Vec<String>| {
        let id = id.trim();
        if !id.is_empty()
            && id.chars().count() <= MAX_ID_CHARS
            && entries.is_some_and(|entries| entries.get(id).is_some_and(Value::is_object))
            && !ordered_ids.iter().any(|existing| existing == id)
        {
            ordered_ids.push(id.to_string());
        }
    };
    for id in state
        .get("order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        push_id(id, &mut ordered_ids);
    }
    for id in entries.into_iter().flat_map(|entries| entries.keys()) {
        push_id(id, &mut ordered_ids);
    }
    if !ordered_ids.iter().any(|id| id == DEFAULT_WORKSPACE_ID) {
        ordered_ids.insert(0, DEFAULT_WORKSPACE_ID.to_string());
    }
    ordered_ids.truncate(MAX_WORKSPACES);

    let mut workspaces = Map::new();
    for (index, workspace_id) in ordered_ids.iter().enumerate() {
        let source = entries
            .and_then(|entries| entries.get(workspace_id))
            .cloned()
            .unwrap_or(Value::Null);
        let is_default = workspace_id == DEFAULT_WORKSPACE_ID;
        let name = bounded_text(source.get("name"), MAX_NAME_CHARS).unwrap_or_else(|| {
            if is_default {
                DEFAULT_WORKSPACE_NAME.to_string()
            } else {
                workspace_id.clone()
            }
        });
        let kind = WorkspaceKind::from_wire(source.get("kind")).unwrap_or(if is_default {
            WorkspaceKind::Personal
        } else {
            WorkspaceKind::Work
        });
        let color = source
            .get("color")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|color| is_valid_color(color))
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| {
                if is_default {
                    "#3f8fc7".to_string()
                } else {
                    WORKSPACE_COLORS[index % WORKSPACE_COLORS.len()].to_string()
                }
            });
        let letter = source
            .get("letter")
            .and_then(Value::as_str)
            .and_then(|letter| letter.trim().chars().find(|c| !c.is_whitespace()))
            .or_else(|| name.chars().find(|c| c.is_alphanumeric()))
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_else(|| "W".to_string());
        let mut workspace = Map::new();
        workspace.insert("workspaceId".into(), json!(workspace_id));
        workspace.insert("name".into(), json!(name));
        workspace.insert("color".into(), json!(color));
        workspace.insert("letter".into(), json!(letter));
        workspace.insert("kind".into(), json!(kind.as_wire()));
        if let Some(account_id) = bounded_text(source.get("claudeAccountId"), MAX_ID_CHARS) {
            workspace.insert("claudeAccountId".into(), json!(account_id));
        }
        // The primary tracker this computer picked (crate::work_mode::tracker); absent = never
        // picked, which resolves to Linear when a Linear key applies and GitHub otherwise.
        if let Some(tracker) = source
            .get("tracker")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|tracker| matches!(*tracker, "linear" | "github"))
        {
            workspace.insert("tracker".into(), json!(tracker));
        }
        workspaces.insert(workspace_id.clone(), Value::Object(workspace));
    }
    // A machine's assignment to the default workspace, or to one that is gone, is no assignment.
    let mut machine_workspaces = Map::new();
    for (machine_id, workspace_id) in state
        .get("machineWorkspaces")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let machine_id = machine_id.trim();
        let Some(workspace_id) = workspace_id.as_str().map(str::trim) else {
            continue;
        };
        if machine_id.is_empty()
            || machine_id.chars().count() > MAX_ID_CHARS
            || workspace_id == DEFAULT_WORKSPACE_ID
            || !workspaces.contains_key(workspace_id)
        {
            continue;
        }
        if machine_workspaces.len() >= MAX_MACHINE_ASSIGNMENTS {
            break;
        }
        machine_workspaces.insert(machine_id.to_string(), json!(workspace_id));
    }
    json!({
        "defaultWorkspaceId": DEFAULT_WORKSPACE_ID,
        "machineWorkspaces": machine_workspaces,
        "order": ordered_ids,
        "workspaces": workspaces,
    })
}

/// The generic workspace writes, which remote clients may call, set only a built-in tracker. A
/// workspace's ticket command is set and chosen on its own computer (the decision on the
/// work-source endpoints in `protocol.rs`), so a command id here is refused instead of dropped.
fn refuse_other_trackers(params: &Map<String, Value>) -> Result<(), DomainStateError> {
    match params.get("tracker") {
        None | Some(Value::Null) => Ok(()),
        Some(tracker) if matches!(tracker.as_str().map(str::trim), Some("linear" | "github")) => {
            Ok(())
        }
        Some(_) => Err(DomainStateError::bad_request(
            "tracker must be \"linear\" or \"github\"; a ticket command is set and picked on its own computer.",
        )),
    }
}

fn is_valid_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn bounded_text(value: Option<&Value>, max_chars: usize) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty() || text.chars().count() > max_chars {
        return None;
    }
    Some(text.to_string())
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite workspaces error: {error}"),
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
