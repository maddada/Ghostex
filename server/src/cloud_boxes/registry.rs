//! The box registry. The provider's own list is the source of truth (every box carries Ghostex's
//! metadata); this keeps what the provider can't say: boot progress, the gxserver version a boot
//! reported, and a box that is being stopped or deleted. Rows are rebuilt from the provider list
//! plus this overlay on every read.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ghostex_gx_cloud::local::LocalContainer;
use ghostex_gx_protocol::cloud_box::{
    CloudBox, CloudBoxBootStep, CloudBoxProgress, CloudBoxProgressStep, CloudBoxState,
    CloudBoxStepState, CloudProviderId,
};

#[derive(Clone, Default)]
struct Overlay {
    progress: Option<CloudBoxProgress>,
    gxserver_version: Option<String>,
    /// `stopping` or `deleting` while that call runs.
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
pub(super) fn start_progress(
    box_id: &str,
    kind: &str,
    draft: Option<ghostex_gx_protocol::cloud_box::CloudBoxDraftRef>,
) {
    let now = crate::domain::params::now_iso();
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
    let now = crate::domain::params::now_iso();
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
    let now = crate::domain::params::now_iso();
    with_overlay(box_id, |overlay| {
        if let Some(progress) = overlay.progress.as_mut() {
            progress.finished_at.get_or_insert(now);
        }
    });
}

pub(super) fn set_booted(box_id: &str, gxserver_version: String) {
    with_overlay(box_id, |overlay| {
        overlay.gxserver_version = Some(gxserver_version)
    });
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

/// A `pending`/`creating` row for a box whose provider machine does not exist yet.
pub(super) fn creating_row(
    box_id: &str,
    provider: CloudProviderId,
    project_key: &str,
    home_project_id: Option<String>,
) -> CloudBox {
    let overlay = with_overlay(box_id, |overlay| overlay.clone());
    CloudBox {
        box_id: box_id.to_string(),
        provider,
        provider_box_id: None,
        name: ghostex_gx_cloud::ids::box_display_name(box_id),
        state: if overlay
            .progress
            .as_ref()
            .and_then(|p| p.error.as_ref())
            .is_some()
        {
            CloudBoxState::Failed
        } else {
            CloudBoxState::Creating
        },
        state_detail: overlay.progress.as_ref().and_then(|p| p.error.clone()),
        project_key: project_key.to_string(),
        home_project_id,
        snapshot_id: None,
        size: None,
        tailcat_address: None,
        gxserver_version: None,
        created_by: None,
        created_at: overlay.progress.as_ref().and_then(|p| p.started_at.clone()),
        started_at: None,
        last_active_at: None,
        stopped_at: None,
        delete_after: None,
        idle_stops_at: None,
        cost_total_usd: Some(0.0),
        cost_month_usd: Some(0.0),
        cost_estimated: false,
        usd_per_hour: Some(0.0),
        has_unpushed_work: None,
        snapshot_update_reason: None,
        sessions: Vec::new(),
        progress: overlay.progress,
    }
}

/// The registry row of a `local` container.
pub(super) fn local_row(container: &LocalContainer) -> CloudBox {
    let meta = &container.metadata;
    let overlay = with_overlay(&meta.box_id, |overlay| overlay.clone());
    let progress = overlay.progress.clone();
    let failed = progress.as_ref().and_then(|p| p.error.clone());
    let starting = progress.as_ref().is_some_and(|p| p.finished_at.is_none());
    let docker_state = container.box_state();
    let state = match (&overlay.transition, &failed, starting, &docker_state) {
        (Some(transition), _, _, _) => transition.clone(),
        (None, Some(_), _, _) => CloudBoxState::Failed,
        (None, None, true, CloudBoxState::Running) => CloudBoxState::Booting,
        (None, None, true, _) => CloudBoxState::Creating,
        // Running but never booted by this gxserver (it restarted): still reachable once its own
        // gxserver is up, which `connectInfo` checks.
        _ => docker_state.clone(),
    };
    let size = meta.size.clone();
    let usd_per_hour =
        ghostex_gx_cloud::prices::size_or_default(&CloudProviderId::Local, size.as_deref())
            .and_then(|size| size.usd_per_hour);
    CloudBox {
        box_id: meta.box_id.clone(),
        provider: CloudProviderId::Local,
        provider_box_id: Some(container.container_id.chars().take(12).collect()),
        name: ghostex_gx_cloud::ids::box_display_name(&meta.box_id),
        state: state.clone(),
        state_detail: failed,
        project_key: meta.project_key.clone(),
        home_project_id: meta.home_project_id.clone(),
        snapshot_id: meta.snapshot_id.clone(),
        size,
        tailcat_address: None,
        gxserver_version: overlay.gxserver_version,
        created_by: meta.created_by.clone(),
        created_at: meta
            .created_at
            .clone()
            .or_else(|| container.created.clone()),
        started_at: if docker_state == CloudBoxState::Running {
            container.started_at.clone()
        } else {
            None
        },
        last_active_at: None,
        stopped_at: if state == CloudBoxState::Stopped {
            container.finished_at.clone()
        } else {
            None
        },
        delete_after: None,
        idle_stops_at: None,
        cost_total_usd: Some(0.0),
        cost_month_usd: Some(0.0),
        cost_estimated: false,
        usd_per_hour,
        has_unpushed_work: None,
        snapshot_update_reason: None,
        sessions: Vec::new(),
        progress,
    }
}
