//! Stop, delete and attach. Every function blocks; call from `spawn_blocking`.

use ghostex_gx_protocol::cloud_box::{
    box_verbs, error_codes, BoxWorkStatus, CloudBox, CloudBoxConnectInfo, CloudBoxSessionRow,
    CloudBoxState, CloudProviderId, CLOUD_BOX_TRANSPORT_TAILCAT,
};

use super::ops::{Ops, Target};
use super::{boot_steps, events, params, registry, store};
use crate::domain::DomainStateError;
use crate::server::AppState;

fn target(row: &CloudBox) -> Result<Target, DomainStateError> {
    Ok(Target {
        box_id: row.box_id.clone(),
        provider_box_id: row
            .provider_box_id
            .clone()
            .ok_or_else(|| params::wrong_state(format!("{} has no machine yet.", row.name)))?,
    })
}

/// Asks a running box whether it holds uncommitted or unpushed work (D13) and records it.
pub(super) fn check_work(state: &AppState, ops: &Ops, row: &CloudBox) -> Result<bool, String> {
    let value = boot_steps::run_box_verb(
        ops,
        &target(row).map_err(|e| e.message)?,
        box_verbs::WORK_STATUS,
        &[],
        120,
    )?;
    let work: BoxWorkStatus = serde_json::from_value(value).map_err(|error| error.to_string())?;
    let _ = registry::update(&state.paths, &row.box_id, |stored| {
        stored.row.has_unpushed_work = Some(work.has_unpushed_work);
    });
    Ok(work.has_unpushed_work)
}

/// `stop`: records whether the box has unpushed work and brings refreshed Codex logins home, then
/// stops it (programs end, the disk is
/// kept; Boat archives it).
pub(super) fn stop(state: &AppState, box_id: &str) -> Result<CloudBox, DomainStateError> {
    let row = registry::row(&state.paths, box_id)?;
    if matches!(row.state, CloudBoxState::Stopped | CloudBoxState::Deleted) {
        return Ok(row);
    }
    let ops = Ops::for_provider(&state.paths, &row.provider)?;
    if row.state.is_reachable() {
        let _ = check_work(state, &ops, &row);
        // A stop empties the box's private folder: Codex logins it refreshed come home first (D6).
        let _ = boot_steps::write_back_logins(state, &ops, &target(&row)?);
    }
    ops.stop(&target(&row)?).map_err(params::provider_error)?;
    let row = registry::update(&state.paths, box_id, |stored| {
        stored.row.state = CloudBoxState::Stopped;
        stored.row.stopped_at = Some(super::time::now_iso());
        stored.row.idle_stops_at = None;
    })?;
    events::broadcast_box(state, &row);
    Ok(row)
}

/// `delete`. D13: a box with uncommitted or unpushed work, or one whose work can't be checked,
/// is kept unless `force`.
///
/// CDXC:CloudBoxes 2026-11-02 DECISION: the user chose "keep and say" for a box due for deletion that still has unpushed work (D13): it is never deleted on schedule, and the chip and Settings say "Has unpushed work: push or delete anyway".
pub(super) fn delete(
    state: &AppState,
    box_id: &str,
    force: bool,
) -> Result<CloudBox, DomainStateError> {
    let row = registry::row(&state.paths, box_id)?;
    if row.state == CloudBoxState::Deleted {
        return Ok(row);
    }
    if row.provider_box_id.is_none() {
        // Never got a machine: only the registry row goes.
        registry::forget(box_id);
        let row = registry::update(&state.paths, box_id, |stored| {
            stored.row.state = CloudBoxState::Deleted;
        })?;
        events::broadcast_box(state, &row);
        return Ok(row);
    }
    let ops = Ops::for_provider(&state.paths, &row.provider)?;
    if !force {
        let unpushed = if row.state.is_reachable() {
            check_work(state, &ops, &row)
                .map(Some)
                .unwrap_or(row.has_unpushed_work)
        } else {
            row.has_unpushed_work
        };
        match unpushed {
            Some(false) => {}
            Some(true) => {
                return Err(DomainStateError {
                    code: error_codes::HAS_UNPUSHED_WORK,
                    message: format!("{} has unpushed work: push it, or delete anyway.", row.name),
                })
            }
            None => {
                return Err(DomainStateError {
                    code: error_codes::HAS_UNPUSHED_WORK,
                    message: format!(
                        "Couldn't check {} for unpushed work. Push its branch, or delete anyway.",
                        row.name
                    ),
                })
            }
        }
    }
    if row.state.is_reachable() {
        let _ = boot_steps::write_back_logins(state, &ops, &target(&row)?);
    }
    registry::set_transition(box_id, Some(CloudBoxState::Deleting));
    events::broadcast_box(state, &registry::row(&state.paths, box_id)?);
    let result = ops.delete(&target(&row)?);
    registry::set_transition(box_id, None);
    result.map_err(params::provider_error)?;
    registry::forget(box_id);
    let row = registry::update(&state.paths, box_id, |stored| {
        stored.row.state = CloudBoxState::Deleted;
        stored.row.state_detail = None;
        stored.row.delete_after = None;
        stored.token_sealed = None;
    })?;
    events::broadcast_box(state, &row);
    Ok(row)
}

/// `connectInfo`: how a client attaches to a running box's gxserver.
pub(super) fn connect_info(
    state: &AppState,
    box_id: &str,
) -> Result<CloudBoxConnectInfo, DomainStateError> {
    let stored = registry::stored(&state.paths, box_id)?;
    let row = registry::with_overlay_applied(stored.row.clone());
    if !row.state.is_reachable() {
        return Err(params::wrong_state(format!(
            "{} is {}; send a message to wake it first.",
            row.name,
            row.state.as_str()
        )));
    }
    if row.provider == CloudProviderId::Local {
        let container = super::local_provider::find_container(&state.paths.home_dir, box_id)
            .map_err(params::provider_unavailable)?
            .ok_or_else(|| DomainStateError::not_found(format!("No box {box_id}.")))?;
        let mut info = super::local_provider::connect_info(&state.paths.home_dir, &container)
            .map_err(params::provider_error)?;
        info.gxserver_version = row.gxserver_version;
        return Ok(info);
    }
    let token = stored
        .token_sealed
        .as_deref()
        .ok_or_else(|| {
            params::wrong_state(format!("{} hasn't finished starting Ghostex.", row.name))
        })
        .and_then(|sealed| store::unseal(&state.paths, sealed))?;
    let tailcat_address = row
        .tailcat_address
        .clone()
        .ok_or_else(|| params::wrong_state(format!("{} has no private address yet.", row.name)))?;
    Ok(CloudBoxConnectInfo {
        box_id: box_id.to_string(),
        transport: CLOUD_BOX_TRANSPORT_TAILCAT.to_string(),
        tailcat_address: Some(tailcat_address),
        ssh_port: Some(22),
        ssh_user: None,
        gxserver_port: u64::from(crate::constants::GXSERVER_LOCAL_API_PORT),
        base_url: None,
        gxserver_token: token,
        gxserver_version: row.gxserver_version,
    })
}

/// `reportBoxSessions`: a client attached to a box reports the box's sessions, so a stopped box
/// still lists them (Q1013) and the archive rule can see when they are all archived (Q1012).
pub(super) fn report_sessions(
    state: &AppState,
    box_id: &str,
    sessions: Vec<CloudBoxSessionRow>,
) -> Result<CloudBox, DomainStateError> {
    let row = registry::update(&state.paths, box_id, |stored| {
        stored.row.sessions = sessions;
        if stored.row.sessions.iter().any(|session| !session.archived) {
            stored.row.delete_after = None;
        }
        stored.row.last_active_at = stored
            .row
            .sessions
            .iter()
            .filter_map(|session| session.last_active_at.clone())
            .max()
            .or(stored.row.last_active_at.clone());
    })?;
    events::broadcast_box(state, &row);
    Ok(row)
}
