//! Keeps the registry in step with the providers' own lists: a box paused or stopped by its
//! provider (Instabox's idle pause, a Boat box that stopped itself) changes state here, a box
//! deleted at the provider becomes `deleted`, and a Ghostex box this gxserver never saw (made from
//! the phone, or after the registry was lost) is adopted from its metadata. Blocking.

use ghostex_gx_cloud::provider::ProviderMachine;
use ghostex_gx_protocol::cloud_box::{CloudBoxState, CloudProviderId};

use super::ops::Ops;
use super::store::{self, StoredBox};
use super::{events, registry};
use crate::server::AppState;

/// The providers that may have boxes: every connected one, and `local` when listed or when the
/// registry already holds local boxes.
pub(super) fn providers_to_check(state: &AppState) -> Vec<CloudProviderId> {
    let mut providers = Vec::new();
    for provider in [CloudProviderId::Instabox, CloudProviderId::Boat] {
        if super::connect::provider_key(&state.paths, &provider).is_ok() {
            providers.push(provider);
        }
    }
    let local_rows = store::list_boxes(&state.paths)
        .unwrap_or_default()
        .iter()
        .any(|stored| {
            stored.row.provider == CloudProviderId::Local
                && stored.row.state != CloudBoxState::Deleted
        });
    if super::providers::local_provider_listed(state) || local_rows {
        providers.push(CloudProviderId::Local);
    }
    providers
}

fn usd_per_hour(provider: &CloudProviderId, size: Option<&str>) -> Option<f64> {
    ghostex_gx_cloud::prices::size_or_default(provider, size).and_then(|size| size.usd_per_hour)
}

fn apply_machine(stored: &mut StoredBox, machine: &ProviderMachine) -> bool {
    let row = &mut stored.row;
    let before = (
        row.state.clone(),
        row.started_at.clone(),
        row.cost_month_usd,
        row.size.clone(),
    );
    if row.provider_box_id.is_none() {
        row.provider_box_id = Some(machine.provider_box_id.clone());
    }
    let busy = registry::is_starting(&row.box_id);
    if !busy && row.state != CloudBoxState::Deleting {
        match &machine.state {
            // Running at the provider but not booted by this gxserver yet stays as it was.
            CloudBoxState::Creating if row.state.is_reachable() => {}
            CloudBoxState::Other(_) => {}
            state => {
                if *state != row.state {
                    if *state == CloudBoxState::Stopped || *state == CloudBoxState::Paused {
                        row.stopped_at = machine
                            .stopped_at
                            .clone()
                            .or_else(|| Some(super::time::now_iso()));
                    }
                    row.state = state.clone();
                    row.state_detail = machine
                        .error
                        .clone()
                        .filter(|_| *state == CloudBoxState::Failed);
                }
            }
        }
    }
    if machine.state == CloudBoxState::Running {
        if let Some(started) = machine.started_at.clone() {
            row.started_at = Some(started);
        }
    }
    if let Some(size) = machine.size.clone() {
        row.size = Some(size);
    }
    if let Some(cost) = machine.cost_month_usd {
        row.cost_month_usd = Some(cost);
    }
    row.usd_per_hour = usd_per_hour(&row.provider, row.size.as_deref());
    before
        != (
            row.state.clone(),
            row.started_at.clone(),
            row.cost_month_usd,
            row.size.clone(),
        )
}

fn adopt(provider: &CloudProviderId, machine: &ProviderMachine) -> Option<StoredBox> {
    let meta = machine.metadata.clone()?;
    if !ghostex_gx_cloud::ids::is_gx_id(&meta.box_id) {
        return None;
    }
    let mut row = registry::new_row(
        &meta.box_id,
        provider.clone(),
        &meta.project_key,
        meta.home_project_id.clone(),
    );
    row.snapshot_id = meta.snapshot_id.clone();
    row.size = meta.size.clone();
    row.created_by = meta.created_by.clone();
    row.created_at = meta
        .created_at
        .clone()
        .or_else(|| machine.created_at.clone());
    row.state = CloudBoxState::Pending;
    let mut stored = StoredBox {
        row,
        idempotency_key: meta.idempotency_key.clone(),
        token_sealed: None,
    };
    apply_machine(&mut stored, machine);
    Some(stored)
}

/// Brings one provider's rows up to date and returns this month's dollars of deleted boxes there
/// (Boat reports them; the spend reader adds them).
pub(super) fn reconcile(state: &AppState, ops: &Ops) -> Result<f64, String> {
    let provider = ops.provider();
    let (machines, deleted_usd) = ops.list()?;
    let stored = store::list_boxes(&state.paths).map_err(|error| error.message)?;
    let mut changed = Vec::new();
    for machine in &machines {
        let Some(box_id) = machine.metadata.as_ref().map(|meta| meta.box_id.clone()) else {
            continue;
        };
        match stored.iter().find(|row| row.row.box_id == box_id) {
            Some(existing) => {
                let mut existing = existing.clone();
                if apply_machine(&mut existing, machine) {
                    changed.push(existing);
                }
            }
            None => changed.extend(adopt(&provider, machine)),
        }
    }
    for existing in stored.iter().filter(|row| row.row.provider == provider) {
        let row = &existing.row;
        let known = machines.iter().any(|machine| {
            Some(&machine.provider_box_id) == row.provider_box_id.as_ref()
                || machine.metadata.as_ref().map(|meta| &meta.box_id) == Some(&row.box_id)
        });
        let settled = !matches!(row.state, CloudBoxState::Deleted | CloudBoxState::Pending)
            && row.provider_box_id.is_some()
            && !registry::is_starting(&row.box_id);
        if !known && settled {
            let mut gone = existing.clone();
            gone.row.state = CloudBoxState::Deleted;
            gone.row.state_detail = Some("Deleted at the provider.".to_string());
            changed.push(gone);
        }
    }
    for stored in changed {
        if store::save_box(&state.paths, &stored).is_ok() {
            events::broadcast_box(state, &registry::with_overlay_applied(stored.row));
        }
    }
    Ok(deleted_usd)
}

/// Every provider that may have boxes; a refused key marks its provider expired.
pub(super) fn reconcile_all(state: &AppState) -> f64 {
    let mut deleted_usd = 0.0;
    for provider in providers_to_check(state) {
        let Ok(ops) = Ops::for_provider(&state.paths, &provider) else {
            continue;
        };
        match reconcile(state, &ops) {
            Ok(deleted) => deleted_usd += deleted,
            Err(message)
                if message.contains("401")
                    || message.to_lowercase().contains("unauthorized")
                    || message.contains("invalid_api_key") =>
            {
                super::connect::mark_expired(state, &provider, &message);
            }
            Err(_) => {}
        }
    }
    deleted_usd
}
