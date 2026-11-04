//! Project snapshots (Q1002: each project has its snapshots, one Default ★). Save as snapshot
//! scrubs the box first (`ghostex box prepare-snapshot`), saves, waits until the provider says it
//! is ready and puts the box back (`finish-snapshot`). Update snapshot replaces the Default and
//! keeps the old one as "Previous" for 7 days (D9).

use std::sync::Arc;
use std::time::{Duration, Instant};

use ghostex_gx_protocol::cloud_box::{
    box_verbs, CloudProviderId, CloudSnapshot, CloudSnapshotStatus, SaveSnapshotParams,
};
use serde_json::Value;

use super::ops::{Ops, Target};
use super::{boot_steps, events, params, project, registry, store};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::server::AppState;

/// How long a "Previous" snapshot is kept after Update snapshot replaced it (D9).
const PREVIOUS_KEEP_SECONDS: i64 = 7 * 24 * 3600;
/// The longest a provider may take to finish saving a snapshot.
const SAVE_WAIT: Duration = Duration::from_secs(1800);
/// A Default older than this suggests Update snapshot on the box chip (D9).
pub(super) const STALE_DEFAULT_SECONDS: u64 = 7 * 24 * 3600;

fn status(word: &str) -> CloudSnapshotStatus {
    CloudSnapshotStatus::from_wire(word)
}

/// The snapshot a create starts from: the one asked for (id or this project's name), else the
/// project's Default (only a `ready` one). `None` starts a blank box.
pub(super) fn resolve_for_create(
    paths: &GxserverPaths,
    project_key: &str,
    requested: Option<&str>,
    provider: Option<&CloudProviderId>,
    project_default: Option<&str>,
) -> Result<Option<CloudSnapshot>, DomainStateError> {
    let all = store::list_snapshots(paths)?;
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        let found = all
            .iter()
            .find(|snapshot| snapshot.snapshot_id == requested)
            .or_else(|| {
                all.iter().find(|snapshot| {
                    snapshot.project_key == project_key
                        && snapshot.name.eq_ignore_ascii_case(requested)
                        && provider.is_none_or(|provider| snapshot.provider == *provider)
                })
            })
            .cloned()
            .ok_or_else(|| {
                DomainStateError::not_found(format!(
                    "This project has no snapshot \"{requested}\"."
                ))
            })?;
        if found.status != CloudSnapshotStatus::Ready {
            return Err(params::wrong_state(format!(
                "Snapshot {} is {}; wait until it is ready.",
                found.name,
                found.status.as_str()
            )));
        }
        return Ok(Some(found));
    }
    Ok(all
        .into_iter()
        .filter(|snapshot| {
            snapshot.project_key == project_key && snapshot.status == CloudSnapshotStatus::Ready
        })
        .filter(|snapshot| provider.is_none_or(|provider| snapshot.provider == *provider))
        .find(|snapshot| {
            snapshot.is_default || Some(snapshot.snapshot_id.as_str()) == project_default
        }))
}

/// `snapshots`: a project's snapshots (all with no project), Default first. Blocking.
pub(super) fn list(
    state: &AppState,
    project_id: Option<&str>,
    provider: Option<&CloudProviderId>,
) -> Result<Vec<CloudSnapshot>, DomainStateError> {
    let project_key = match project_id {
        Some(project_id) => Some(project::read_home_project(state, project_id)?.project_key),
        None => None,
    };
    let mut snapshots: Vec<CloudSnapshot> = store::list_snapshots(&state.paths)?
        .into_iter()
        .filter(|snapshot| {
            project_key
                .as_ref()
                .is_none_or(|key| snapshot.project_key == *key)
        })
        .filter(|snapshot| provider.is_none_or(|provider| snapshot.provider == *provider))
        .collect();
    snapshots.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then(b.created_at.cmp(&a.created_at))
    });
    Ok(snapshots)
}

fn home_projects_with_key(state: &AppState, project_key: &str) -> Vec<String> {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    let ids: Vec<String> = repository
        .list_projects()
        .unwrap_or_default()
        .iter()
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    drop(db);
    ids.into_iter()
        .filter(|id| {
            project::read_home_project(state, id).is_ok_and(|home| home.project_key == project_key)
        })
        .collect()
}

/// Makes `snapshot` its project's only Default and points the projects' settings at it.
fn make_default(state: &AppState, snapshot: &mut CloudSnapshot) -> Result<(), DomainStateError> {
    for mut other in store::list_snapshots(&state.paths)? {
        if other.project_key == snapshot.project_key
            && other.is_default
            && other.snapshot_id != snapshot.snapshot_id
        {
            other.is_default = false;
            store::save_snapshot(&state.paths, &other)?;
        }
    }
    snapshot.is_default = true;
    snapshot.expires_at = None;
    snapshot.previous_of = None;
    for project_id in home_projects_with_key(state, &snapshot.project_key) {
        let home = project::read_home_project(state, &project_id)?;
        let mut settings = home.settings.clone();
        settings.snapshot_id = Some(snapshot.snapshot_id.clone());
        settings.provider = Some(snapshot.provider.clone());
        if settings.size.is_none() {
            settings.size = snapshot.size.clone();
        }
        project::write_project_settings(state, &home, &settings)?;
    }
    Ok(())
}

/// `saveSnapshot`: starts the save and returns the snapshot `pending`.
pub(super) async fn save(
    state: Arc<AppState>,
    request: SaveSnapshotParams,
) -> Result<CloudSnapshot, DomainStateError> {
    let row = registry::row(&state.paths, &request.box_id)?;
    if !row.state.is_reachable() {
        return Err(params::wrong_state(format!(
            "{} is {}; a snapshot is saved from a running box.",
            row.name,
            row.state.as_str()
        )));
    }
    let ops = Ops::for_provider(&state.paths, &row.provider)?;
    let mut entropy = [0u8; 16];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut entropy);
    let snapshot_id = ghostex_gx_cloud::ids::gx_id_from_entropy(&entropy);
    let existing = store::list_snapshots(&state.paths)?;
    let first_for_project = !existing
        .iter()
        .any(|snapshot| snapshot.project_key == row.project_key);
    let replace = request.replace_default == Some(true);
    let name = request
        .name
        .clone()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            if first_for_project || replace {
                "Base".to_string()
            } else {
                format!("Snapshot {}", &super::time::now_iso()[..10])
            }
        });
    let snapshot = CloudSnapshot {
        snapshot_id: snapshot_id.clone(),
        provider: row.provider.clone(),
        provider_snapshot_id: None,
        project_key: row.project_key.clone(),
        name,
        is_default: false,
        status: CloudSnapshotStatus::Pending,
        source_box_id: Some(row.box_id.clone()),
        base_branch: None,
        commit_sha: None,
        size_bytes: None,
        size: row.size.clone(),
        created_at: Some(super::time::now_iso()),
        setup_script_path: None,
        previous_of: None,
        expires_at: None,
        shared_with_org: false,
        error: None,
    };
    store::save_snapshot(&state.paths, &snapshot)?;
    events::broadcast_scope(&state, &["snapshots"]);
    let make_default_after = first_for_project || replace || request.make_default == Some(true);
    let target = Target {
        box_id: row.box_id.clone(),
        provider_box_id: row.provider_box_id.clone().unwrap_or_default(),
    };
    let task_state = state.clone();
    let task_snapshot = snapshot.clone();
    tokio::spawn(async move {
        let worker_state = task_state.clone();
        let _ = params::blocking(move || {
            run_save(
                &worker_state,
                ops,
                target,
                task_snapshot,
                make_default_after,
                replace,
            )
        })
        .await;
        events::broadcast_scope(&task_state, &["snapshots", "boxes"]);
    });
    Ok(snapshot)
}

fn run_save(
    state: &AppState,
    ops: Ops,
    target: Target,
    mut snapshot: CloudSnapshot,
    make_default_after: bool,
    replace: bool,
) {
    let created_at = snapshot.created_at.clone().unwrap_or_default();
    let outcome = (|| -> Result<(), String> {
        boot_steps::run_box_verb(&ops, &target, box_verbs::PREPARE_SNAPSHOT, &[], 300)?;
        let saved =
            ops.create_snapshot(&target, &snapshot.snapshot_id, &snapshot.name, &created_at);
        let result = saved.and_then(|mut provider_snapshot| {
            snapshot.provider_snapshot_id = Some(provider_snapshot.provider_snapshot_id.clone());
            let _ = store::save_snapshot(&state.paths, &snapshot);
            let started = Instant::now();
            while provider_snapshot.status == "pending" {
                if started.elapsed() > SAVE_WAIT {
                    return Err(
                        "The provider is still saving the snapshot after 30 minutes.".to_string(),
                    );
                }
                std::thread::sleep(Duration::from_secs(5));
                provider_snapshot = ops
                    .get_snapshot(&provider_snapshot.provider_snapshot_id)?
                    .ok_or("The provider lost the snapshot while saving it.")?;
            }
            snapshot.size_bytes = provider_snapshot.size_bytes;
            match provider_snapshot.status.as_str() {
                "ready" => Ok(()),
                _ => Err(provider_snapshot
                    .error
                    .unwrap_or_else(|| "The snapshot failed.".to_string())),
            }
        });
        // The box gets its identity and logins back whatever happened to the save.
        let finished =
            boot_steps::run_box_verb(&ops, &target, box_verbs::FINISH_SNAPSHOT, &[], 300);
        result?;
        finished.map(|_| ())
    })();
    match outcome {
        Ok(()) => {
            snapshot.status = status("ready");
            if make_default_after {
                if replace {
                    keep_previous(state, &snapshot);
                }
                let _ = make_default(state, &mut snapshot);
            }
        }
        Err(message) => {
            snapshot.status = status("failed");
            snapshot.error = Some(message);
        }
    }
    let _ = store::save_snapshot(&state.paths, &snapshot);
}

/// D9: the Default being replaced stays as "Previous" for 7 days.
fn keep_previous(state: &AppState, replacement: &CloudSnapshot) {
    let Ok(all) = store::list_snapshots(&state.paths) else {
        return;
    };
    for mut old in all {
        if old.project_key == replacement.project_key
            && old.is_default
            && old.snapshot_id != replacement.snapshot_id
        {
            old.is_default = false;
            old.name = "Previous".to_string();
            old.previous_of = Some(replacement.snapshot_id.clone());
            old.expires_at = Some(super::time::iso_in(PREVIOUS_KEEP_SECONDS));
            let _ = store::save_snapshot(&state.paths, &old);
        }
    }
}

/// `setDefaultSnapshot`. Blocking.
pub(super) fn set_default(
    state: &AppState,
    snapshot_id: &str,
) -> Result<CloudSnapshot, DomainStateError> {
    let mut snapshot = store::get_snapshot(&state.paths, snapshot_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("No snapshot {snapshot_id}.")))?;
    if snapshot.status != CloudSnapshotStatus::Ready {
        return Err(params::wrong_state(format!(
            "Snapshot {} isn't ready yet.",
            snapshot.name
        )));
    }
    make_default(state, &mut snapshot)?;
    store::save_snapshot(&state.paths, &snapshot)?;
    events::broadcast_scope(state, &["snapshots"]);
    Ok(snapshot)
}

/// `deleteSnapshot`: deletes it at the provider, then forgets it. Blocking.
pub(super) fn delete(
    state: &AppState,
    snapshot_id: &str,
) -> Result<CloudSnapshot, DomainStateError> {
    let snapshot = store::get_snapshot(&state.paths, snapshot_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("No snapshot {snapshot_id}.")))?;
    if let Some(provider_snapshot_id) = snapshot.provider_snapshot_id.as_deref() {
        let ops = Ops::for_provider(&state.paths, &snapshot.provider)?;
        ops.delete_snapshot(provider_snapshot_id)
            .map_err(params::provider_error)?;
    }
    store::delete_snapshot(&state.paths, snapshot_id)?;
    if snapshot.is_default {
        for project_id in home_projects_with_key(state, &snapshot.project_key) {
            if let Ok(home) = project::read_home_project(state, &project_id) {
                if home.settings.snapshot_id.as_deref() == Some(snapshot_id) {
                    let mut settings = home.settings.clone();
                    settings.snapshot_id = None;
                    let _ = project::write_project_settings(state, &home, &settings);
                }
            }
        }
    }
    events::broadcast_scope(state, &["snapshots"]);
    Ok(snapshot)
}

/// Deletes "Previous" snapshots whose 7 days are over (the scheduler). Blocking.
pub(super) fn expire_previous(state: &AppState) {
    let Ok(all) = store::list_snapshots(&state.paths) else {
        return;
    };
    for snapshot in all {
        if snapshot
            .expires_at
            .as_deref()
            .is_some_and(super::time::is_past)
            && !snapshot.is_default
        {
            let _ = delete(state, &snapshot.snapshot_id);
        }
    }
}

/// Why Update snapshot is suggested for a box started from `snapshot_id` (D9's age rule; the
/// lockfile rule comes from the box itself).
pub(super) fn update_reason(all: &[CloudSnapshot], snapshot_id: Option<&str>) -> Option<String> {
    let snapshot = all
        .iter()
        .find(|snapshot| Some(snapshot.snapshot_id.as_str()) == snapshot_id)?;
    let created = snapshot.created_at.as_deref()?;
    (snapshot.is_default && super::time::seconds_since(created) > STALE_DEFAULT_SECONDS)
        .then(|| "The Default snapshot is older than 7 days.".to_string())
}
