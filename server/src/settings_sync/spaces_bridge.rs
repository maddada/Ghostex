//! This computer's Spaces and project groups, read for sync and written back from it.

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_core::settings_sync::SyncEntry;
use ghostex_gx_core::unified_sidebar::{
    folder_in_repository, merge_repo_identities, origin_keys_of, project_repo_key, repo_identity,
    repository_of_project_key, resolve_repo_identity, RepoIdentity,
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::sidebar_project_collections::{
    read_sidebar_project_collections, update_sidebar_project_collections,
};
use crate::sidebar_spaces::{read_sidebar_spaces, update_sidebar_spaces};

/// This computer's two documents and its shareable projects keyed by repository and folder.
pub(super) struct LocalSpacesRead {
    pub(super) spaces: Value,
    pub(super) collections: Value,
    pub(super) repos: Vec<(String, String)>,
    /// Every shareable project's git origin and repository root have been probed, so `repos`
    /// holds their final keys.
    pub(super) remotes_probed: bool,
}

pub(super) fn read_local_spaces(
    db: &Connection,
    server_id: &str,
    entries: &BTreeMap<String, SyncEntry>,
) -> Result<LocalSpacesRead, DomainStateError> {
    let collections = read_sidebar_project_collections(db)?;
    let spaces = read_sidebar_spaces(db)?;
    let projects = DomainRepository::new(db, server_id).list_projects()?;
    let keys = repo_keys(&projects, entries, KeyBy::RepositoryAndFolder);
    Ok(LocalSpacesRead {
        repos: keys.repos,
        remotes_probed: keys.remotes_probed,
        spaces,
        collections,
    })
}

/// [`repo_keys`] for this computer's projects now: the key a coordinator playbook edited for one
/// project is synced under.
pub(super) fn project_repo_keys(
    db: &Connection,
    server_id: &str,
    entries: &BTreeMap<String, SyncEntry>,
    key_by: KeyBy,
) -> Result<RepoKeys, DomainStateError> {
    let projects = DomainRepository::new(db, server_id).list_projects()?;
    Ok(repo_keys(&projects, entries, key_by))
}

/// What a synced project key names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyBy {
    /// The repository alone (`github.com/o/monorepo`).
    Repository,
    /// The repository and the project's folder inside it (`github.com/o/monorepo#trading`).
    RepositoryAndFolder,
}

pub(super) struct RepoKeys {
    pub(super) repos: Vec<(String, String)>,
    /// Every shareable project's git origin and repository root have been probed.
    pub(super) remotes_probed: bool,
}

/// Project id → repository key for every project filed like its copies on other computers (not a
/// worktree, not a bot), keyed the way the one list keys them and matched against the repository
/// keys the other computers already synced, so one repository has one key everywhere.
///
/// CDXC:SettingsSync 2026-11-06 WHY:
/// A computer sees only its own projects, so a project whose origin isn't probed yet (or that has none) would get its folder name as key while another computer filed the same repository under its origin. Matching against the keys in the entries applies the one list's rule (`merge_repo_identities`: same `owner/repo` whatever the host, a folder joins the one repository of its name) across computers.
///
/// CDXC:SettingsSync 2026-11-08 WHY:
/// Issue #488: Spaces keyed by the repository alone gave every folder project of one monorepo one `repo/place` entry, so the first sync filed all of them in the Space of the first one (and took the rest out of theirs). Spaces now key a project as the one list does since #461 (`project_repo_key`): its repository plus its folder inside it, so `monorepo/trading` on one computer shares a place with `monorepo/trading` on another and with nothing else. A repository's root keeps the bare key it had.
fn repo_keys(projects: &[Value], entries: &BTreeMap<String, SyncEntry>, key_by: KeyBy) -> RepoKeys {
    let mut remotes_probed = true;
    let mut local: Vec<(String, RepoIdentity, Option<String>)> = Vec::new();
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
        // Only a project the sidebar publishes is probed (a parked one never is).
        remotes_probed &= !crate::presentation::should_include_presentation_project(project)
            || crate::project_git_remote::project_git_remote_probed(path);
        let remote = crate::project_git_remote::cached_project_git_remote(path);
        let origin = remote
            .as_ref()
            .and_then(|remote| remote.origin_url.clone())
            .filter(|url| !url.is_empty());
        // A folder with no origin is keyed by its own folder name already.
        let folder = remote
            .and_then(|remote| remote.repository_root_path)
            .filter(|_| origin.is_some() && key_by == KeyBy::RepositoryAndFolder)
            .and_then(|root| folder_in_repository(path, Some(&root)));
        let title = project
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        local.push((
            project_id.to_string(),
            repo_identity(origin.as_deref(), path, None, title),
            folder,
        ));
    }
    let synced: Vec<RepoIdentity> = entries
        .keys()
        .filter_map(|key| key.strip_prefix("spaces/repo/place/"))
        .map(repository_of_project_key)
        .map(|key| RepoIdentity {
            key: key.to_string(),
            label: key.rsplit('/').next().unwrap_or(key).to_string(),
        })
        .collect();
    let merged = merge_repo_identities(
        local
            .iter()
            .map(|(_, identity, _)| identity)
            .chain(synced.iter()),
    );
    let origins_here: BTreeSet<String> =
        origin_keys_of(local.iter().map(|(_, identity, _)| identity), &merged);
    let repos = local
        .into_iter()
        .map(|(project_id, identity, folder)| {
            let repository = resolve_repo_identity(&identity, &merged, &origins_here).key;
            (project_id, project_repo_key(&repository, folder.as_deref()))
        })
        .collect();
    RepoKeys {
        repos,
        remotes_probed,
    }
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
