//! Create and resume: the request returns at once with the box `creating`; the steps run in the
//! background, each one a line on the progress card and a `cloudBoxesChanged` push.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use ghostex_gx_cloud::metadata::BoxMetadata;
use ghostex_gx_protocol::cloud_box::{
    BoxBootTicket, CloudBox, CloudBoxBootStep as Step, CloudBoxCreatedBy,
    CloudBoxStepState as StepState, CloudProviderId, CreateParams, CreateResult,
    BOX_BOOT_TICKET_VERSION, DEFAULT_IDLE_STOP_MINUTES,
};
use rand::Rng;

use super::{events, local_provider, params, project, registry, spend};
use crate::domain::DomainStateError;
use crate::server::AppState;

/// A box asked for whose provider machine does not exist yet (or never came to be).
#[derive(Clone)]
struct InFlight {
    provider: CloudProviderId,
    project_key: String,
    home_project_id: Option<String>,
    idempotency_key: Option<String>,
}

fn in_flight() -> &'static Mutex<HashMap<String, InFlight>> {
    static MAP: OnceLock<Mutex<HashMap<String, InFlight>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One create at a time decides "is there already a box for this idempotency key", so two
/// retried Sends never make two boxes.
fn create_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Rows of boxes still being created (no provider machine yet), or whose create failed early.
pub(super) fn in_flight_rows(existing: &[CloudBox]) -> Vec<CloudBox> {
    let map = in_flight()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.iter()
        .filter(|(box_id, _)| !existing.iter().any(|row| &row.box_id == *box_id))
        .map(|(box_id, entry)| {
            registry::creating_row(
                box_id,
                entry.provider.clone(),
                &entry.project_key,
                entry.home_project_id.clone(),
            )
        })
        .collect()
}

/// Drops an in-flight entry (the machine exists now, or the failed row was deleted).
pub(super) fn forget_in_flight(box_id: &str) -> bool {
    in_flight()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(box_id)
        .is_some()
}

fn new_box_id() -> String {
    let mut entropy = [0u8; 16];
    rand::rng().fill_bytes(&mut entropy);
    ghostex_gx_cloud::ids::gx_id_from_entropy(&entropy)
}

fn idle_stop_minutes(state: &AppState) -> u64 {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| {
            settings
                .get(ghostex_gx_protocol::cloud_box::setting_keys::IDLE_STOP_MINUTES)
                .and_then(serde_json::Value::as_u64)
        })
        .filter(|minutes| *minutes > 0)
        .unwrap_or(DEFAULT_IDLE_STOP_MINUTES)
}

/// `create`.
pub(super) async fn create(
    state: Arc<AppState>,
    request: CreateParams,
) -> Result<CreateResult, DomainStateError> {
    let home_project = project::read_home_project(&state, &request.project_id)?;
    let provider = request
        .provider
        .clone()
        .or_else(|| home_project.settings.provider.clone())
        .unwrap_or(CloudProviderId::Local);
    if provider != CloudProviderId::Local {
        return Err(params::not_connected(format!(
            "{} isn't connected yet. Connect it in Settings › Cloud Boxes.",
            provider.label()
        )));
    }
    if !super::providers::local_provider_listed(&state) {
        return Err(params::provider_unavailable(
            "The Local (Docker) provider is for development: turn on Show debug UI controls to use it.",
        ));
    }
    spend::check_cap(&state)?;

    let _gate = create_gate().lock().await;
    let home = state.paths.home_dir.clone();
    if let Some(key) = request
        .idempotency_key
        .clone()
        .filter(|key| !key.is_empty())
    {
        let existing = in_flight()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .find(|(_, entry)| entry.idempotency_key.as_deref() == Some(key.as_str()))
            .map(|(box_id, _)| box_id.clone());
        let found = match existing {
            Some(box_id) => Some(super::endpoint::box_row(&state, &box_id).await?),
            None => {
                let key = key.clone();
                let home = home.clone();
                params::blocking(move || local_provider::list_containers(&home))
                    .await?
                    .map_err(params::provider_unavailable)?
                    .iter()
                    .find(|container| {
                        container.metadata.idempotency_key.as_deref() == Some(key.as_str())
                    })
                    .map(registry::local_row)
            }
        };
        if let Some(cloud_box) = found {
            return Ok(CreateResult {
                cloud_box,
                reused: true,
            });
        }
    }

    let home_for_check = home.clone();
    let package = params::blocking(move || -> Result<(String, PathBuf), String> {
        let docker_arch = local_provider::docker_arch(&home_for_check)?;
        let arch = ghostex_gx_cloud::local::package_arch(&docker_arch).ok_or_else(|| {
            format!("Docker runs {docker_arch}, which has no Linux gxserver package.")
        })?;
        Ok((docker_arch, local_provider::package_dir(arch)?))
    })
    .await?
    .map_err(params::provider_unavailable)?
    .1;

    let snapshot_id = request
        .snapshot_id
        .clone()
        .filter(|_| request.blank != Some(true))
        .or_else(|| {
            home_project
                .settings
                .snapshot_id
                .clone()
                .filter(|_| request.blank != Some(true))
        });
    let size = ghostex_gx_cloud::prices::size_or_default(
        &provider,
        request
            .size
            .as_deref()
            .or(home_project.settings.size.as_deref()),
    )
    .ok_or_else(|| DomainStateError::bad_request("That provider offers no box sizes."))?;
    let box_id = new_box_id();
    let meta = BoxMetadata {
        box_id: box_id.clone(),
        project_key: home_project.project_key.clone(),
        home_project_id: Some(home_project.project_id.clone()),
        home_machine_id: Some(state.metadata.server_id.clone()),
        snapshot_id: snapshot_id.clone(),
        size: Some(size.id.clone()),
        idempotency_key: request
            .idempotency_key
            .clone()
            .filter(|key| !key.is_empty()),
        created_by: Some(CloudBoxCreatedBy::Computer),
        created_at: Some(crate::domain::params::now_iso()),
    };
    in_flight()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            box_id.clone(),
            InFlight {
                provider: provider.clone(),
                project_key: meta.project_key.clone(),
                home_project_id: meta.home_project_id.clone(),
                idempotency_key: meta.idempotency_key.clone(),
            },
        );
    registry::start_progress(&box_id, "create", request.draft.clone());
    registry::set_step(&box_id, Step::CheckCap, StepState::Done, None);
    registry::set_step(&box_id, Step::Create, StepState::Running, None);
    let row = super::endpoint::box_row(&state, &box_id).await?;
    events::broadcast_box(&state, &row);

    let image = snapshot_id
        .as_deref()
        .map(ghostex_gx_cloud::local::snapshot_image)
        .unwrap_or_else(|| {
            std::env::var(ghostex_gx_cloud::local::BASE_IMAGE_ENV)
                .ok()
                .filter(|image| !image.trim().is_empty())
                .unwrap_or_else(|| ghostex_gx_cloud::local::BASE_IMAGE.to_string())
        });
    let ticket = BoxBootTicket {
        version: BOX_BOOT_TICKET_VERSION,
        box_id: box_id.clone(),
        provider: provider.clone(),
        home_machine_id: meta.home_machine_id.clone(),
        home_project_id: meta.home_project_id.clone(),
        allow_device_keys: Vec::new(),
        idle_stop_minutes: idle_stop_minutes(&state),
        self_stop: false,
        tailcat: false,
    };
    let task_state = state.clone();
    tokio::spawn(async move {
        run_local_create(
            task_state,
            meta,
            image,
            size.vcpus,
            size.mem_mib,
            package,
            ticket,
        )
        .await;
    });
    Ok(CreateResult {
        cloud_box: row,
        reused: false,
    })
}

async fn step<T: Send + 'static>(
    state: &Arc<AppState>,
    box_id: &str,
    step: Step,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Option<T> {
    registry::set_step(box_id, step.clone(), StepState::Running, None);
    push(state, box_id).await;
    let result = params::blocking(work)
        .await
        .unwrap_or_else(|error| Err(error.message));
    match result {
        Ok(value) => {
            registry::set_step(box_id, step, StepState::Done, None);
            push(state, box_id).await;
            Some(value)
        }
        Err(message) => {
            registry::set_step(box_id, step, StepState::Failed, Some(message));
            push(state, box_id).await;
            None
        }
    }
}

async fn push(state: &Arc<AppState>, box_id: &str) {
    if let Ok(row) = super::endpoint::box_row(state, box_id).await {
        events::broadcast_box(state, &row);
    }
}

/// The remaining steps once the box has its Ghostex identity: boot, then the steps W2 and W3 fill
/// (secrets, prepare-session, connect), skipped for now when there is nothing to do.
async fn finish_boot(state: &Arc<AppState>, box_id: &str, ticket: BoxBootTicket) -> bool {
    let home = state.paths.home_dir.clone();
    let Some(result) = step(state, box_id, Step::Boot, move || {
        local_provider::boot(&home, &ticket)
    })
    .await
    else {
        return false;
    };
    registry::set_booted(box_id, result.gxserver_version);
    for skipped in [Step::Secrets, Step::PrepareSession, Step::Connect] {
        registry::set_step(box_id, skipped, StepState::Skipped, None);
    }
    registry::finish_progress(box_id);
    push(state, box_id).await;
    true
}

async fn run_local_create(
    state: Arc<AppState>,
    meta: BoxMetadata,
    image: String,
    vcpus: u64,
    mem_mib: u64,
    package: PathBuf,
    ticket: BoxBootTicket,
) {
    let box_id = meta.box_id.clone();
    let home = state.paths.home_dir.clone();
    let created = step(&state, &box_id, Step::Create, move || {
        local_provider::create_container(&home, &meta, &image, vcpus, mem_mib)?;
        local_provider::install_package(&home, &meta.box_id, &package)
    })
    .await;
    if created.is_none() {
        return;
    }
    forget_in_flight(&box_id);
    finish_boot(&state, &box_id, ticket).await;
}

/// `resume` of a `local` box: `docker start` (its programs ended, as on Boat), then boot again.
pub(super) async fn resume_local(
    state: Arc<AppState>,
    box_id: String,
) -> Result<(), DomainStateError> {
    let ticket = BoxBootTicket {
        version: BOX_BOOT_TICKET_VERSION,
        box_id: box_id.clone(),
        provider: CloudProviderId::Local,
        home_machine_id: Some(state.metadata.server_id.clone()),
        home_project_id: None,
        allow_device_keys: Vec::new(),
        idle_stop_minutes: idle_stop_minutes(&state),
        self_stop: false,
        tailcat: false,
    };
    registry::start_progress(&box_id, "resume", None);
    registry::set_step(&box_id, Step::CheckCap, StepState::Skipped, None);
    let task_state = state.clone();
    tokio::spawn(async move {
        let home = task_state.paths.home_dir.clone();
        let target = box_id.clone();
        if step(&task_state, &box_id, Step::Create, move || {
            local_provider::start(&home, &target)
        })
        .await
        .is_some()
        {
            finish_boot(&task_state, &box_id, ticket).await;
        }
    });
    Ok(())
}
