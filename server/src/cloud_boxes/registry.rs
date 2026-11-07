//! The box registry: rows in the `cloud_boxes` table (store.rs), kept in step with the providers'
//! own lists (reconcile.rs), plus what only this gxserver's memory holds: each box's boot progress
//! card and a stop or delete in flight. A row a client reads is the stored row with that overlay.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ghostex_gx_protocol::cloud_box::{
    CloudBox, CloudBoxBootStep, CloudBoxDraftRef, CloudBoxProgress, CloudBoxProgressStep,
    CloudBoxState, CloudBoxStepState,
};

use super::store::{self, StoredBox};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

#[derive(Clone, Default)]
struct Overlay {
    progress: Option<CloudBoxProgress>,
    /// `deleting` while that call runs.
    transition: Option<CloudBoxState>,
}

fn overlays() -> &'static Mutex<HashMap<String, Overlay>> {
    static OVERLAYS: OnceLock<Mutex<HashMap<String, Overlay>>> = OnceLock::new();
    OVERLAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_overlay<T>(box_id: &str, change: impl FnOnce(&mut Overlay) -> T) -> T {
    let mut map = overlays()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    change(map.entry(box_id.to_string()).or_default())
}

/// Starts a fresh progress card (`create` or `resume`) with every step waiting.
pub(super) fn start_progress(box_id: &str, kind: &str, draft: Option<CloudBoxDraftRef>) {
    let now = super::time::now_iso();
    with_overlay(box_id, |overlay| {
        overlay.progress = Some(CloudBoxProgress {
            draft,
            kind: kind.to_string(),
            steps: CloudBoxBootStep::ALL
                .iter()
                .cloned()
                .map(CloudBoxProgressStep::waiting)
                .collect(),
            started_at: Some(now),
            finished_at: None,
            error: None,
        });
    });
}

/// Moves one progress line to `state`, stamping its times; a failure ends the card.
pub(super) fn set_step(
    box_id: &str,
    step: CloudBoxBootStep,
    state: CloudBoxStepState,
    message: Option<String>,
) {
    let now = super::time::now_iso();
    with_overlay(box_id, |overlay| {
        let Some(progress) = overlay.progress.as_mut() else {
            return;
        };
        if let Some(line) = progress.steps.iter_mut().find(|line| line.step == step) {
            match state {
                CloudBoxStepState::Running => line.started_at = Some(now.clone()),
                CloudBoxStepState::Done
                | CloudBoxStepState::Failed
                | CloudBoxStepState::Skipped => {
                    line.started_at.get_or_insert_with(|| now.clone());
                    line.finished_at = Some(now.clone());
                }
                _ => {}
            }
            if state == CloudBoxStepState::Failed {
                progress.error = message.clone();
                progress.finished_at = Some(now.clone());
            }
            line.state = state;
            line.message = message;
        }
    });
}

pub(super) fn finish_progress(box_id: &str) {
    let now = super::time::now_iso();
    with_overlay(box_id, |overlay| {
        if let Some(progress) = overlay.progress.as_mut() {
            progress.finished_at.get_or_insert(now);
        }
    });
}

/// The box is starting or resuming right now (its card is open and has not failed).
pub(super) fn is_starting(box_id: &str) -> bool {
    with_overlay(box_id, |overlay| {
        overlay
            .progress
            .as_ref()
            .is_some_and(|progress| progress.finished_at.is_none())
    })
}

pub(super) fn set_transition(box_id: &str, transition: Option<CloudBoxState>) {
    with_overlay(box_id, |overlay| overlay.transition = transition);
}

pub(super) fn forget(box_id: &str) {
    overlays()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(box_id);
}

/// A stored row with the overlay applied.
pub(super) fn with_overlay_applied(mut row: CloudBox) -> CloudBox {
    let overlay = with_overlay(&row.box_id, |overlay| overlay.clone());
    if let Some(transition) = overlay.transition {
        row.state = transition;
    }
    if let Some(progress) = overlay.progress {
        if let Some(error) = progress.error.clone() {
            row.state = CloudBoxState::Failed;
            row.state_detail = Some(error);
        }
        row.progress = Some(progress);
    }
    row
}

/// Every box (deleted rows included), overlay applied. Blocking.
pub(super) fn rows(paths: &GxserverPaths) -> Result<Vec<CloudBox>, DomainStateError> {
    Ok(store::list_boxes(paths)?
        .into_iter()
        .map(|stored| with_overlay_applied(stored.row))
        .collect())
}

/// One box's stored row and secrets. Blocking.
pub(super) fn stored(paths: &GxserverPaths, box_id: &str) -> Result<StoredBox, DomainStateError> {
    if !ghostex_gx_cloud::ids::is_gx_id(box_id) {
        return Err(DomainStateError::bad_request(format!(
            "\"{box_id}\" is not a box id."
        )));
    }
    store::get_box(paths, box_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("No box {box_id}.")))
}

/// One box's row, overlay applied. Blocking.
pub(super) fn row(paths: &GxserverPaths, box_id: &str) -> Result<CloudBox, DomainStateError> {
    Ok(with_overlay_applied(stored(paths, box_id)?.row))
}

/// Reads, changes and writes a box row. Blocking.
pub(super) fn update(
    paths: &GxserverPaths,
    box_id: &str,
    change: impl FnOnce(&mut StoredBox),
) -> Result<CloudBox, DomainStateError> {
    let mut stored = stored(paths, box_id)?;
    change(&mut stored);
    store::save_box(paths, &stored)?;
    Ok(with_overlay_applied(stored.row))
}

/// A new row for a box Ghostex is about to make.
pub(super) fn new_row(
    box_id: &str,
    provider: ghostex_gx_protocol::cloud_box::CloudProviderId,
    project_key: &str,
    home_project_id: Option<String>,
) -> CloudBox {
    CloudBox {
        box_id: box_id.to_string(),
        provider,
        provider_box_id: None,
        name: ghostex_gx_cloud::ids::box_display_name(box_id),
        state: CloudBoxState::Creating,
        state_detail: None,
        project_key: project_key.to_string(),
        home_project_id,
        snapshot_id: None,
        size: None,
        tailcat_address: None,
        gxserver_version: None,
        private_dir: None,
        created_by: None,
        created_at: Some(super::time::now_iso()),
        started_at: None,
        last_active_at: None,
        stopped_at: None,
        delete_after: None,
        idle_stops_at: None,
        cost_total_usd: Some(0.0),
        cost_month_usd: Some(0.0),
        cost_estimated: false,
        usd_per_hour: None,
        has_unpushed_work: None,
        snapshot_update_reason: None,
        sessions: Vec::new(),
        progress: None,
    }
}
