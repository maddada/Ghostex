//! The workspaces document: the list of workspaces (name, color, letter, icon, kind, Claude account) in
//! the metadata table, normalized on every read and write like the Spaces document.

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

const SIDEBAR_WORKSPACES_METADATA_KEY: &str = "sidebarWorkspaces";
const MAX_WORKSPACES: usize = 64;
const MAX_ID_CHARS: usize = 256;
const MAX_NAME_CHARS: usize = 128;
const MAX_MACHINE_ASSIGNMENTS: usize = 256;
const MAX_REMOTE_PROJECT_ASSIGNMENTS: usize = 2048;
const MAX_WORKSPACE_CLAUDE_ACCOUNTS: usize = 32;
/// How many Spaces one workspace may hide from its row with Hide Here.
const MAX_HIDDEN_SPACE_IDS: usize = 256;

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

/// The Claude account(s) a workspace's agents use, in the user's chosen order. Reads
/// `claudeAccountIds` when present, falling back to legacy single `claudeAccountId`.
pub fn workspace_claude_account_ids(state: &Value, workspace_id: &str) -> Vec<String> {
    let Some(workspace) = state.get("workspaces").and_then(|ws| ws.get(workspace_id)) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    if let Some(list) = workspace.get("claudeAccountIds").and_then(Value::as_array) {
        for item in list {
            if let Some(id) = bounded_text(Some(item), MAX_ID_CHARS) {
                if !ids.iter().any(|existing| existing == &id) {
                    ids.push(id);
                }
            }
        }
    }
    if ids.is_empty() {
        if let Some(id) = bounded_text(workspace.get("claudeAccountId"), MAX_ID_CHARS) {
            ids.push(id);
        }
    }
    ids.truncate(MAX_WORKSPACE_CLAUDE_ACCOUNTS);
    ids
}

/// The first Claude account a workspace's agents use, when one or more are chosen.
pub fn workspace_claude_account_id(state: &Value, workspace_id: &str) -> Option<String> {
    workspace_claude_account_ids(state, workspace_id)
        .into_iter()
        .next()
}

/// The display name of a workspace in a document, falling back to its id.
pub fn workspace_name(state: &Value, workspace_id: &str) -> String {
    state
        .get("workspaces")
        .and_then(|workspaces| workspaces.get(workspace_id))
        .and_then(|workspace| workspace.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| workspace_id.to_string())
}

/// The remote computer id where a workspace's instructions, Claude accounts and claude.ai
/// connectors live, when the workspace is tied to another computer rather than this one.
pub fn workspace_home_machine_id(state: &Value, workspace_id: &str) -> Option<String> {
    bounded_text(
        state
            .get("workspaces")?
            .get(workspace_id)?
            .get("homeMachineId"),
        MAX_ID_CHARS,
    )
}

/// Whether `workspace_id` lives on this computer (its instructions file, Claude accounts and
/// connector status are owned by this gxserver rather than delegated to another computer).
pub fn workspace_is_local(state: &Value, workspace_id: &str) -> bool {
    workspace_home_machine_id(state, workspace_id).is_none()
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

/// Puts one project of a remote machine in a workspace of this computer, apart from the rest of
/// its machine's tab (`assign_machine_workspace_in`). Kept even for the default workspace, which
/// is where the project goes when its machine sits in another one.
///
/// CDXC:Workspaces 2026-11-19 DECISION:
/// A Discord user (relayed by the user): "I have existing Project on other computer or server, I wanted to move it to the work workspace, but there is no option to move Projects to workspace". The user asked for Move to workspace on a remote computer's projects too, listing every workspace, both ways. A remote project's own `workspaceId` is its machine's and means nothing here, so the choice is this computer's, by machine and project (`remoteProjectWorkspaces`); a project with none shows where its machine's tab does. Its machine's tab shows in every workspace that holds one of its projects.
pub(crate) fn assign_remote_project_workspace_in(
    state: &Value,
    machine_id: &str,
    project_id: &str,
    workspace_id: &str,
) -> Result<Value, DomainStateError> {
    let machine_id = machine_id.trim();
    let project_id = project_id.trim();
    if machine_id.is_empty() || machine_id.chars().count() > MAX_ID_CHARS {
        return Err(DomainStateError::bad_request("Pass machineId."));
    }
    if project_id.is_empty() || project_id.chars().count() > MAX_ID_CHARS {
        return Err(DomainStateError::bad_request("Pass projectId."));
    }
    if !workspace_exists(state, workspace_id) {
        return Err(DomainStateError::bad_request("No such workspace."));
    }
    let mut next = state.clone();
    let Some(document) = next.as_object_mut() else {
        return Err(DomainStateError::bad_request("No such workspace."));
    };
    let machines = document
        .entry("remoteProjectWorkspaces")
        .or_insert_with(|| json!({}));
    if !machines.is_object() {
        *machines = json!({});
    }
    let machines = machines.as_object_mut().expect("set to an object above");
    let projects = machines
        .entry(machine_id.to_string())
        .or_insert_with(|| json!({}));
    if !projects.is_object() {
        *projects = json!({});
    }
    projects
        .as_object_mut()
        .expect("set to an object above")
        .insert(project_id.to_string(), json!(workspace_id));
    Ok(normalize_sidebar_workspaces_state(&next))
}

/// Adds a workspace built from `params` (`name`, optional `color`, `letter`, `icon`, `kind`) and returns
/// its id with the new document.
pub(crate) fn create_workspace_in(
    state: &Value,
    params: &Map<String, Value>,
) -> Result<(String, Value), DomainStateError> {
    let name = bounded_text(params.get("name"), MAX_NAME_CHARS)
        .ok_or_else(|| DomainStateError::bad_request("A workspace needs a name."))?;
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
    for key in [
        "color",
        "letter",
        "icon",
        "kind",
        "claudeAccountId",
        "claudeAccountIds",
        "homeMachineId",
        "homeMachineName",
        "tracker",
    ] {
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

/// Applies `params` (`name`, `color`, `letter`, `icon`, `kind`, `claudeAccountId`,
/// `claudeAccountIds`, `tracker`, `showSpacesFrom`, `hiddenSpaceIds`; `null` clears the icon, the
/// account(s), the tracker or a list) to one workspace.
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
    let prev_home_machine_id = workspace
        .get("homeMachineId")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_string);
    for key in [
        "name",
        "color",
        "letter",
        "icon",
        "kind",
        "homeMachineId",
        "homeMachineName",
        "tracker",
        "showSpacesFrom",
        "hiddenSpaceIds",
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
    // Clearing `homeMachineId` returns the workspace to this computer and drops any cached remote
    // computer name; switching `homeMachineId` to another computer without supplying a new
    // `homeMachineName` also drops the previous computer's cached name.
    if params
        .get("homeMachineId")
        .is_some_and(|v| v.is_null() || v.as_str().is_some_and(|s| s.trim().is_empty()))
    {
        workspace.remove("homeMachineId");
        workspace.remove("homeMachineName");
    } else if params.contains_key("homeMachineId") && !params.contains_key("homeMachineName") {
        let next_home_machine_id = workspace
            .get("homeMachineId")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_string);
        if next_home_machine_id != prev_home_machine_id {
            workspace.remove("homeMachineName");
        }
    }
    // `claudeAccountIds` (new multi-account clients) and `claudeAccountId` (legacy single-account
    // clients) stay in sync so older phones and desktops always see a consistent choice.
    if let Some(account_ids) = params.get("claudeAccountIds") {
        workspace.remove("claudeAccountId");
        match account_ids {
            Value::Null => {
                workspace.remove("claudeAccountIds");
            }
            value => {
                workspace.insert("claudeAccountIds".into(), value.clone());
            }
        }
    } else if let Some(account_id) = params.get("claudeAccountId") {
        workspace.remove("claudeAccountIds");
        match account_id {
            Value::Null => {
                workspace.remove("claudeAccountId");
            }
            value => {
                workspace.insert("claudeAccountId".into(), value.clone());
            }
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

/// A list of ids as text, trimmed, bounded, each once, at most `max` of them; anything else is
/// dropped.
fn bounded_id_list(value: Option<&Value>, max: usize) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        if let Some(id) = bounded_text(Some(item), MAX_ID_CHARS) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.len() >= max {
            break;
        }
    }
    ids
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
        // The icon picked in Settings (gx-protocol `WORKSPACE_ICONS`); anything else is dropped,
        // so the sidebar draws the kind's default (`SidebarWorkspace::icon_id`).
        if let Some(icon) = source
            .get("icon")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|icon| ghostex_gx_protocol::WORKSPACE_ICONS.contains(icon))
        {
            workspace.insert("icon".into(), json!(icon));
        }
        workspace.insert("kind".into(), json!(kind.as_wire()));
        let mut account_ids: Vec<String> = Vec::new();
        if let Some(list) = source.get("claudeAccountIds").and_then(Value::as_array) {
            for item in list {
                if let Some(id) = bounded_text(Some(item), MAX_ID_CHARS) {
                    if !account_ids.iter().any(|existing| existing == &id) {
                        account_ids.push(id);
                    }
                }
            }
        }
        if account_ids.is_empty() {
            if let Some(account_id) = bounded_text(source.get("claudeAccountId"), MAX_ID_CHARS) {
                account_ids.push(account_id);
            }
        }
        account_ids.truncate(MAX_WORKSPACE_CLAUDE_ACCOUNTS);
        if let Some(first_account_id) = account_ids.first() {
            workspace.insert("claudeAccountId".into(), json!(first_account_id));
            workspace.insert("claudeAccountIds".into(), json!(account_ids));
        }
        if let Some(home_machine_id) = bounded_text(source.get("homeMachineId"), MAX_ID_CHARS) {
            workspace.insert("homeMachineId".into(), json!(home_machine_id));
            if let Some(home_machine_name) =
                bounded_text(source.get("homeMachineName"), MAX_NAME_CHARS)
            {
                workspace.insert("homeMachineName".into(), json!(home_machine_name));
            }
        }
        // The primary tracker this computer picked (crate::work_mode::tracker): Linear, GitHub, or
        // a ticket command's id; absent = never picked, which resolves to Linear when a Linear key
        // applies and GitHub otherwise. A command id is kept as text even when its command breaks
        // or is gone; the tracker resolves it only while the workspace pins that command.
        if let Some(tracker) = source
            .get("tracker")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|tracker| {
                matches!(*tracker, "linear" | "github")
                    || crate::work_mode::may_be_command_source_id(tracker)
            })
        {
            workspace.insert("tracker".into(), json!(tracker));
        }
        // The other workspaces whose Spaces this one shows too (CDXC:Workspaces 2026-11-26 in
        // gx-protocol side_state.rs): only workspaces that exist, never itself, each once.
        let show_spaces_from = bounded_id_list(source.get("showSpacesFrom"), MAX_WORKSPACES)
            .into_iter()
            .filter(|id| id != workspace_id && ordered_ids.contains(id))
            .collect::<Vec<_>>();
        if !show_spaces_from.is_empty() {
            workspace.insert("showSpacesFrom".into(), json!(show_spaces_from));
        }
        // Their Spaces hidden from this workspace's row only (Hide Here).
        let hidden_space_ids = bounded_id_list(source.get("hiddenSpaceIds"), MAX_HIDDEN_SPACE_IDS);
        if !hidden_space_ids.is_empty() {
            workspace.insert("hiddenSpaceIds".into(), json!(hidden_space_ids));
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
    // A remote project's assignment to a workspace that is gone is no assignment.
    let mut remote_project_workspaces = Map::new();
    let mut remote_project_count = 0;
    for (machine_id, projects) in state
        .get("remoteProjectWorkspaces")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let machine_id = machine_id.trim();
        if machine_id.is_empty() || machine_id.chars().count() > MAX_ID_CHARS {
            continue;
        }
        let mut kept = Map::new();
        for (project_id, workspace_id) in projects.as_object().into_iter().flatten() {
            let project_id = project_id.trim();
            let Some(workspace_id) = workspace_id.as_str().map(str::trim) else {
                continue;
            };
            if project_id.is_empty()
                || project_id.chars().count() > MAX_ID_CHARS
                || !workspaces.contains_key(workspace_id)
                || remote_project_count >= MAX_REMOTE_PROJECT_ASSIGNMENTS
            {
                continue;
            }
            remote_project_count += 1;
            kept.insert(project_id.to_string(), json!(workspace_id));
        }
        if !kept.is_empty() {
            remote_project_workspaces.insert(machine_id.to_string(), Value::Object(kept));
        }
    }
    json!({
        "defaultWorkspaceId": DEFAULT_WORKSPACE_ID,
        "machineWorkspaces": machine_workspaces,
        "remoteProjectWorkspaces": remote_project_workspaces,
        "order": ordered_ids,
        "workspaces": workspaces,
    })
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
