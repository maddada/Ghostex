//! A ticket command as its workspace's tracker: picked only on this computer (`/api/setWorkTracker`
//! is local only), only while the workspace pins it and has no team. A clear or a Switch moves the
//! pick with the command. Each runs under the store's writer lock, so a pick can't land on a
//! command another change has just cleared or switched from.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::workspaces::{read_sidebar_workspaces, update_workspace_in, write_sidebar_workspaces};

use super::*;

/// Picks the workspace's ticket command, `source_id`, as its tracker.
pub(crate) fn pick_command_tracker(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    source_id: &str,
) -> Result<(), DomainStateError> {
    let _writer = writer()?;
    if crate::team_sync::workspace_has_team(paths, workspace_id) {
        return Err(DomainStateError::bad_request(TEAM_REFUSAL));
    }
    let pinned = read_work_sources(db)?
        .workspaces
        .get(workspace_id)
        .is_some_and(|command| command.source_id == source_id);
    if !pinned {
        return Err(DomainStateError::bad_request(format!(
            "\"{source_id}\" isn't this workspace's ticket command; set it first with `ghostex issues source set <path>`."
        )));
    }
    write_tracker(db, workspace_id, json!(source_id))
}

/// After a clear or a Switch, under the writer lock: a tracker that named the old id follows the
/// command to its new id, or with none goes back to the rule for a workspace that never picked one.
pub(super) fn follow_command_tracker(
    db: &Connection,
    workspace_id: &str,
    old_id: &str,
    new_id: Option<&str>,
) -> Result<(), DomainStateError> {
    let workspaces = read_sidebar_workspaces(db)?;
    let named = workspaces
        .get("workspaces")
        .and_then(|workspaces| workspaces.get(workspace_id))
        .and_then(|workspace| workspace.get("tracker"))
        .and_then(Value::as_str);
    if named != Some(old_id) {
        return Ok(());
    }
    write_tracker(db, workspace_id, new_id.map_or(Value::Null, |id| json!(id)))
}

fn write_tracker(
    db: &Connection,
    workspace_id: &str,
    tracker: Value,
) -> Result<(), DomainStateError> {
    let current = read_sidebar_workspaces(db)?;
    let mut patch = Map::new();
    patch.insert("tracker".to_string(), tracker);
    write_sidebar_workspaces(db, &update_workspace_in(&current, workspace_id, &patch)?)?;
    Ok(())
}
