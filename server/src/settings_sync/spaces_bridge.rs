//! This computer's Spaces and project groups, read for sync and written back from it.

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_core::settings_sync::SyncEntry;
use ghostex_gx_core::unified_sidebar::{
    merge_repo_identities, origin_keys_of, repo_identity, resolve_repo_identity, RepoIdentity,
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::sidebar_project_collections::{
    read_sidebar_project_collections, update_sidebar_project_collections,
};
use crate::sidebar_spaces::{read_sidebar_spaces, update_sidebar_spaces};

/// This computer's two documents and its shareable projects keyed by repository.
pub(super) struct LocalSpacesRead {
    pub(super) spaces: Value,
    pub(super) collections: Value,
    pub(super) repos: Vec<(String, String)>,
}

pub(super) fn read_local_spaces(
    db: &Connection,
    server_id: &str,
    entries: &BTreeMap<String, SyncEntry>,
) -> Result<LocalSpacesRead, DomainStateError> {
    let collections = read_sidebar_project_collections(db)?;
    let spaces = read_sidebar_spaces(db)?;
    let projects = DomainRepository::new(db, server_id).list_projects()?;
    Ok(LocalSpacesRead {
        repos: repo_keys(&projects, entries),
        spaces,
        collections,
    })
}

/// [`repo_keys`] for this computer's projects now.
pub(super) fn project_repo_keys(
    db: &Connection,
    server_id: &str,
    entries: &BTreeMap<String, SyncEntry>,
) -> Result<Vec<(String, String)>, DomainStateError> {
    let projects = DomainRepository::new(db, server_id).list_projects()?;
    Ok(repo_keys(&projects, entries))
}

/// Project id → repository key for every project filed like its copies on other computers (not a
/// worktree, not a bot), keyed the way the one list keys them and matched against the repository
/// keys the other computers already synced, so one repository has one key everywhere.
///
/// CDXC:SettingsSync 2026-11-06 WHY:
/// A computer sees only its own projects, so a project whose origin isn't probed yet (or that has none) would get its folder name as key while another computer filed the same repository under its origin. Matching against the keys in the entries applies the one list's rule (`merge_repo_identities`: same `owner/repo` whatever the host, a folder joins the one repository of its name) across computers.
fn repo_keys(projects: &[Value], entries: &BTreeMap<String, SyncEntry>) -> Vec<(String, String)> {
    let mut local: Vec<(String, RepoIdentity)> = Vec::new();
    for project in projects {
        let is_worktree = project
            .get("worktree")
            .and_then(|worktree| worktree.get("parentProjectId"))
            .and_then(Value::as_str)
            .is_some_and(|parent| !parent.is_empty());
        if is_worktree || crate::bot_projects::bot_profile(project).is_some() {
            continue;
        }
        let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        let path = project
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let origin = crate::project_git_remote::cached_project_git_remote(path)
            .and_then(|remote| remote.origin_url)
            .filter(|url| !url.is_empty());
        let title = project
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        local.push((
            project_id.to_string(),
            repo_identity(origin.as_deref(), path, None, title),
        ));
    }
    let synced: Vec<RepoIdentity> = entries
        .keys()
        .filter_map(|key| key.strip_prefix("spaces/repo/place/"))
        .map(|key| RepoIdentity {
            key: key.to_string(),
            label: key.rsplit('/').next().unwrap_or(key).to_string(),
        })
        .collect();
    let merged = merge_repo_identities(
        local
            .iter()
            .map(|(_, identity)| identity)
            .chain(synced.iter()),
    );
    let origins_here: BTreeSet<String> =
        origin_keys_of(local.iter().map(|(_, identity)| identity), &merged);
    local
        .into_iter()
        .map(|(project_id, identity)| {
            let key = resolve_repo_identity(&identity, &merged, &origins_here).key;
            (project_id, key)
        })
        .collect()
}

/// Writes the documents sync decided on, groups first (gxserver drops a Space's group it doesn't
/// hold yet), and tells every client, as an edit from a client would.
pub(super) fn write_local_spaces(
    state: &AppState,
    db: &Connection,
    spaces: &Value,
    collections: &Value,
) -> Result<(), DomainStateError> {
    let protocol = crate::constants::GXSERVER_PROTOCOL_VERSION;
    let _event_sequence = crate::server::lock_presentation_event_sequence(state)?;
    let previous_collections = read_sidebar_project_collections(db)?;
    if &previous_collections != collections {
        let written = update_sidebar_project_collections(db, &state_params(collections))?;
        let revision = crate::presentation::increment_presentation_revision(db)?;
        state.event_hub.broadcast(json!({
            "protocolVersion": protocol,
            "revision": revision,
            "serverId": state.metadata.server_id.clone(),
            "sidebarProjectCollections": written.clone(),
            "type": "sidebarProjectCollectionsChanged",
        }));
        crate::server::broadcast_pruned_sidebar_spaces(state, db, &previous_collections, &written)?;
    }
    if &read_sidebar_spaces(db)? != spaces {
        let written = update_sidebar_spaces(db, &state_params(spaces))?;
        let revision = crate::presentation::increment_presentation_revision(db)?;
        state.event_hub.broadcast(json!({
            "protocolVersion": protocol,
            "revision": revision,
            "serverId": state.metadata.server_id.clone(),
            "sidebarSpaces": written,
            "sidebarSpacesEnabled": crate::sidebar_spaces::read_sidebar_spaces_enabled(&state.paths),
            "type": "sidebarSpacesChanged",
        }));
    }
    Ok(())
}

fn state_params(state: &Value) -> Map<String, Value> {
    Map::from_iter([("state".to_string(), state.clone())])
}
