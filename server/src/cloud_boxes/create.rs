//! Create, reuse and resume. The request returns at once with the box `creating`; the steps run in
//! the background, each one a line on the progress card (Q1009) and a `cloudBoxesChanged` push,
//! and the row in the registry follows them so a restart of this gxserver keeps the outcome.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use ghostex_gx_cloud::metadata::BoxMetadata;
use ghostex_gx_protocol::cloud_box::{
    error_codes, BoxBootTicket, CloudBox, CloudBoxBootStep as Step, CloudBoxCreatedBy,
    CloudBoxDraftRef, CloudBoxSessionRow, CloudBoxState, CloudBoxStepState as StepState,
    CloudProviderConnectionState, CloudProviderId, CloudSnapshot, CreateParams, CreateResult,
    ProjectCloudBoxSettings,
};
use rand::Rng;

use super::ops::{Ops, Target};
use super::store::{self, StoredBox};
use super::{boot_steps, events, params, project, registry, snapshots, spend};
use crate::domain::DomainStateError;
use crate::server::AppState;

/// One create at a time decides "is there already a box for this idempotency key", so two
/// retried Sends never make two boxes.
fn create_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn new_id() -> String {
    let mut entropy = [0u8; 16];
    rand::rng().fill_bytes(&mut entropy);
    ghostex_gx_cloud::ids::gx_id_from_entropy(&entropy)
}

/// What the session steps need from the home project and the draft.
#[derive(Clone, Default)]
struct SessionContext {
    draft: Option<CloudBoxDraftRef>,
    repo_url: Option<String>,
    project_name: String,
    project_path: Option<String>,
    settings: ProjectCloudBoxSettings,
    snapshot_id: Option<String>,
    /// A second session on a reused box works in its own worktree.
    worktree: bool,
}

/// The provider a create uses: the one asked for, the snapshot's, the project's, the first
/// connected one, else `local` when it is listed.
fn pick_provider(
    state: &AppState,
    requested: Option<&CloudProviderId>,
    snapshot: Option<&CloudSnapshot>,
    settings: &ProjectCloudBoxSettings,
) -> Result<CloudProviderId, DomainStateError> {
    if let (Some(requested), Some(snapshot)) = (requested, snapshot) {
        if *requested != snapshot.provider {
            return Err(DomainStateError::bad_request(format!(
                "Snapshot {} is on {}, not {}.",
                snapshot.name,
                snapshot.provider.label(),
                requested.label()
            )));
        }
    }
    if let Some(provider) = requested
        .cloned()
        .or_else(|| snapshot.map(|snapshot| snapshot.provider.clone()))
        .or_else(|| settings.provider.clone())
    {
        return Ok(provider);
    }
    for provider in [CloudProviderId::Instabox, CloudProviderId::Boat] {
        if super::connect::provider_key(&state.paths, &provider).is_ok() {
            return Ok(provider);
        }
    }
    if super::providers::local_provider_listed(state) {
        return Ok(CloudProviderId::Local);
    }
    Err(params::not_connected(
        "Connect Instabox or Boat in Settings › Cloud Boxes to start a Cloud Box.",
    ))
}

/// D10: free plans and trials still work; at the plan's box limit the create is refused with a
/// reason and a running box of the project to reuse instead.
fn check_plan_limit(
    state: &AppState,
    provider: &CloudProviderId,
    home_project_id: &str,
) -> Result<(), DomainStateError> {
    let Some(limit) = store::read_provider(&state.paths, provider)?
        .and_then(|stored| stored.row.limits)
        .and_then(|limits| limits.max_boxes)
    else {
        return Ok(());
    };
    let boxes = store::list_boxes(&state.paths)?;
    let live: Vec<&CloudBox> = boxes
        .iter()
        .map(|stored| &stored.row)
        .filter(|row| row.provider == *provider && !matches!(row.state, CloudBoxState::Deleted))
        .collect();
    if (live.len() as u64) < limit {
        return Ok(());
    }
    let reuse = live
        .iter()
        .find(|row| row.home_project_id.as_deref() == Some(home_project_id))
        .or_else(|| live.first());
    Err(DomainStateError {
        code: error_codes::PLAN_LIMIT,
        message: match reuse {
            Some(row) => format!(
                "Your {} plan allows {limit} box{} at a time. Use {} for this chat instead, or delete it first.",
                provider.label(),
                if limit == 1 { "" } else { "es" },
                row.name
            ),
            None => format!("Your {} plan allows {limit} boxes at a time.", provider.label()),
        },
    })
}

/// `create`.
pub(super) async fn create(
    state: Arc<AppState>,
    request: CreateParams,
) -> Result<CreateResult, DomainStateError> {
    if let Some(reuse_box_id) = request.reuse_box_id.clone() {
        return reuse(state, request, reuse_box_id).await;
    }
    let home_project = project::read_home_project(&state, &request.project_id)?;
    let snapshot = if request.blank == Some(true) {
        None
    } else {
        snapshots::resolve_for_create(
            &state.paths,
            &home_project.project_key,
            request.snapshot_id.as_deref(),
            request
                .provider
                .as_ref()
                .or(home_project.settings.provider.as_ref()),
            home_project.settings.snapshot_id.as_deref(),
        )?
    };
    let provider = pick_provider(
        &state,
        request.provider.as_ref(),
        snapshot.as_ref(),
        &home_project.settings,
    )?;
    if provider == CloudProviderId::Local && !super::providers::local_provider_listed(&state) {
        return Err(params::provider_unavailable(
            "The Local (Docker) provider is for development: turn on Show debug UI controls to use it.",
        ));
    }
    let ops = Ops::for_provider(&state.paths, &provider)?;
    spend::check_cap(&state)?;
    check_plan_limit(&state, &provider, &home_project.project_id)?;

    let _gate = create_gate().lock().await;
    let idempotency_key = request
        .idempotency_key
        .clone()
        .filter(|key| !key.is_empty());
    if let Some(key) = idempotency_key.as_deref() {
        if let Some(existing) = store::find_box_by_idempotency_key(&state.paths, key)? {
            return Ok(CreateResult {
                cloud_box: registry::with_overlay_applied(existing.row),
                reused: true,
            });
        }
    }

    let package = if provider == CloudProviderId::Local {
        let home = state.paths.home_dir.clone();
        Some(
            params::blocking(move || -> Result<PathBuf, String> {
                let docker_arch = super::local_provider::docker_arch(&home)?;
                let arch =
                    ghostex_gx_cloud::local::package_arch(&docker_arch).ok_or_else(|| {
                        format!("Docker runs {docker_arch}, which has no Linux gxserver package.")
                    })?;
                super::local_provider::package_dir(arch)
            })
            .await?
            .map_err(params::provider_unavailable)?,
        )
    } else {
        None
    };
    let size = ghostex_gx_cloud::prices::size_or_default(
        &provider,
        request
            .size
            .as_deref()
            .or(snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.size.as_deref()))
            .or(home_project.settings.size.as_deref()),
    )
    .ok_or_else(|| DomainStateError::bad_request("That provider offers no box sizes."))?;

    let box_id = new_id();
    let meta = BoxMetadata {
        box_id: box_id.clone(),
        project_key: home_project.project_key.clone(),
        home_project_id: Some(home_project.project_id.clone()),
        home_machine_id: Some(state.metadata.server_id.clone()),
        snapshot_id: snapshot
            .as_ref()
            .map(|snapshot| snapshot.snapshot_id.clone()),
        size: Some(size.id.clone()),
        idempotency_key: idempotency_key.clone(),
        created_by: Some(CloudBoxCreatedBy::Computer),
        created_at: Some(super::time::now_iso()),
    };
    let mut row = registry::new_row(
        &box_id,
        provider.clone(),
        &meta.project_key,
        meta.home_project_id.clone(),
    );
    row.snapshot_id = meta.snapshot_id.clone();
    row.size = Some(size.id.clone());
    row.usd_per_hour = size.usd_per_hour;
    row.created_by = meta.created_by.clone();
    row.created_at = meta.created_at.clone();
    store::save_box(
        &state.paths,
        &StoredBox {
            row,
            idempotency_key: idempotency_key.clone(),
            token_sealed: None,
        },
    )?;
    registry::start_progress(&box_id, "create", request.draft.clone());
    registry::set_step(&box_id, Step::CheckCap, StepState::Done, None);
    let first = registry::row(&state.paths, &box_id)?;
    events::broadcast_box(&state, &first);

    let base_image = std::env::var(ghostex_gx_cloud::local::BASE_IMAGE_ENV)
        .ok()
        .filter(|image| !image.trim().is_empty())
        .unwrap_or_else(|| ghostex_gx_cloud::local::BASE_IMAGE.to_string());
    let context = SessionContext {
        draft: request.draft.clone(),
        repo_url: home_project.origin_url(),
        project_name: home_project.name(),
        project_path: home_project.path.clone(),
        settings: home_project.settings.clone(),
        snapshot_id: meta.snapshot_id.clone(),
        worktree: false,
    };
    let provider_snapshot = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.provider_snapshot_id.clone());
    let ticket = boot_steps::ticket(&state, &box_id, &provider, meta.home_project_id.clone());
    let idle_minutes = ticket.idle_stop_minutes;
    let provider_key = idempotency_key.unwrap_or_else(|| format!("ghostex-{box_id}-create"));
    let task_state = state.clone();
    tokio::spawn(async move {
        let target_box = meta.box_id.clone();
        let created = step(&task_state, &target_box, Step::Create, {
            let ops = ops.clone();
            move || {
                ops.create(
                    &meta,
                    provider_snapshot.as_deref(),
                    &size.id,
                    size.vcpus,
                    size.mem_mib,
                    idle_minutes,
                    &provider_key,
                    package.as_deref(),
                    &base_image,
                )
            }
        })
        .await;
        let Some(provider_box_id) = created else {
            return;
        };
        let _ = registry::update(&task_state.paths, &target_box, |stored| {
            stored.row.provider_box_id = Some(provider_box_id.clone());
            stored.row.state = CloudBoxState::Booting;
        });
        let target = Target {
            box_id: target_box,
            provider_box_id,
        };
        finish_boot(&task_state, ops, target, ticket, context).await;
    });
    Ok(CreateResult {
        cloud_box: first,
        reused: false,
    })
}

/// `create` with `reuseBoxId`: the draft's session starts on an existing box, in its own worktree.
async fn reuse(
    state: Arc<AppState>,
    request: CreateParams,
    box_id: String,
) -> Result<CreateResult, DomainStateError> {
    let row = registry::row(&state.paths, &box_id)?;
    if matches!(row.state, CloudBoxState::Deleted | CloudBoxState::Deleting)
        || row.provider_box_id.is_none()
    {
        return Err(params::wrong_state(format!(
            "{} can't take another session.",
            row.name
        )));
    }
    let home_project = project::read_home_project(&state, &request.project_id)?;
    let ops = Ops::for_provider(&state.paths, &row.provider)?;
    let target = Target {
        box_id: box_id.clone(),
        provider_box_id: row.provider_box_id.clone().unwrap_or_default(),
    };
    let context = SessionContext {
        draft: request.draft.clone(),
        repo_url: home_project.origin_url(),
        project_name: home_project.name(),
        project_path: home_project.path.clone(),
        settings: home_project.settings.clone(),
        snapshot_id: row.snapshot_id.clone(),
        worktree: true,
    };
    let ticket = boot_steps::ticket(&state, &box_id, &row.provider, row.home_project_id.clone());
    let wake = row.state.wakes_on_send() || row.state == CloudBoxState::Failed;
    registry::start_progress(
        &box_id,
        if wake { "resume" } else { "create" },
        request.draft.clone(),
    );
    registry::set_step(&box_id, Step::CheckCap, StepState::Skipped, None);
    let task_state = state.clone();
    let current = row.state.clone();
    tokio::spawn(async move {
        if wake {
            let resume_ops = ops.clone();
            let resume_target = target.clone();
            if step(&task_state, &box_id, Step::Create, move || {
                resume_ops.resume(&resume_target, &current)
            })
            .await
            .is_none()
            {
                return;
            }
            finish_boot(&task_state, ops, target, ticket, context).await;
        } else {
            registry::set_step(&box_id, Step::Create, StepState::Skipped, None);
            registry::set_step(&box_id, Step::Boot, StepState::Skipped, None);
            session_steps(&task_state, &ops, &target, &context).await;
        }
    });
    let row = registry::row(&state.paths, &request.reuse_box_id.unwrap_or_default())?;
    events::broadcast_box(&state, &row);
    Ok(CreateResult {
        cloud_box: row,
        reused: true,
    })
}

async fn push(state: &Arc<AppState>, box_id: &str) {
    if let Ok(row) = registry::row(&state.paths, box_id) {
        events::broadcast_box(state, &row);
    }
}

/// Runs one blocking step as a progress line; a failure also marks the stored row `failed`.
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
            let _ = registry::update(&state.paths, box_id, |stored| {
                stored.row.state = CloudBoxState::Failed;
                stored.row.state_detail = Some(message.clone());
            });
            registry::set_step(box_id, step, StepState::Failed, Some(message));
            push(state, box_id).await;
            None
        }
    }
}

/// Boot, then the session steps. Used by create and resume.
async fn finish_boot(
    state: &Arc<AppState>,
    ops: Ops,
    target: Target,
    ticket: BoxBootTicket,
    context: SessionContext,
) {
    let box_id = target.box_id.clone();
    let boot_ops = ops.clone();
    let boot_target = target.clone();
    let Some(result) = step(state, &box_id, Step::Boot, move || {
        boot_steps::install_package(&boot_ops, &boot_target)?;
        boot_steps::boot(&boot_ops, &boot_target, &ticket)
    })
    .await
    else {
        return;
    };
    // A `local` box's token is read from the box itself when a client attaches; the others keep
    // it sealed, since their gxserver is reachable only through tailcat with this token.
    let token_sealed = if ops.provider() == CloudProviderId::Local {
        None
    } else {
        let seal_state = state.clone();
        let token = result.gxserver_token.clone();
        params::blocking(move || store::seal(&seal_state.paths, &token).ok())
            .await
            .ok()
            .flatten()
    };
    let _ = registry::update(&state.paths, &box_id, |stored| {
        stored.row.state = CloudBoxState::Running;
        stored.row.state_detail = None;
        stored.row.started_at = Some(super::time::now_iso());
        stored.row.stopped_at = None;
        stored.row.tailcat_address = result.tailcat_address.clone();
        stored.row.gxserver_version = Some(result.gxserver_version.clone());
        if token_sealed.is_some() {
            stored.token_sealed = token_sealed;
        }
    });
    session_steps(state, &ops, &target, &context).await;
}

/// Secrets, then prepare-session for a draft, then the card closes (clients attach themselves).
async fn session_steps(
    state: &Arc<AppState>,
    ops: &Ops,
    target: &Target,
    context: &SessionContext,
) {
    let box_id = target.box_id.clone();
    let secrets =
        boot_steps::secrets_file(state, context.project_path.as_deref(), &context.settings);
    let secret_ops = ops.clone();
    let secret_target = target.clone();
    let sent = step(state, &box_id, Step::Secrets, move || {
        boot_steps::send_secrets(&secret_ops, &secret_target, &secrets)
    })
    .await;
    match sent {
        None => return,
        Some(false) => registry::set_step(&box_id, Step::Secrets, StepState::Skipped, None),
        Some(true) => {}
    }
    if let Some(draft) = context.draft.clone() {
        let prepare_state = state.clone();
        let prepare_ops = ops.clone();
        let prepare_target = target.clone();
        let context = context.clone();
        let provider = ops.provider();
        let prepared = step(state, &box_id, Step::PrepareSession, move || {
            let request = boot_steps::prepare_request(
                &prepare_state,
                &prepare_target.box_id,
                &draft,
                context.repo_url.clone(),
                &context.project_name,
                context.snapshot_id.clone(),
                context.worktree,
                provider,
            )?;
            let result = boot_steps::prepare_session(&prepare_ops, &prepare_target, &request)?;
            Ok((request, result))
        })
        .await;
        let Some((request, result)) = prepared else {
            return;
        };
        let _ = registry::update(&state.paths, &box_id, |stored| {
            stored
                .row
                .sessions
                .retain(|row| row.session_id != result.session_id);
            stored.row.sessions.push(CloudBoxSessionRow {
                project_id: result.project_id.clone(),
                session_id: result.session_id.clone(),
                title: request.title.clone().unwrap_or_default(),
                agent_id: Some(request.agent_id.clone()),
                branch: result.branch.clone(),
                agent_session_id: result.agent_session_id.clone(),
                last_active_at: Some(super::time::now_iso()),
                home_session_id: request.home_session_id.clone(),
                archived: false,
            });
            stored.row.last_active_at = Some(super::time::now_iso());
        });
    } else {
        registry::set_step(&box_id, Step::PrepareSession, StepState::Skipped, None);
    }
    registry::set_step(&box_id, Step::Connect, StepState::Skipped, None);
    registry::finish_progress(&box_id);
    push(state, &box_id).await;
}

/// `resume`: wakes a paused or stopped box (Q1013: the send path calls it, never an open), boots
/// box mode again and returns at once with the box `creating`.
pub(super) async fn resume(
    state: Arc<AppState>,
    box_id: String,
) -> Result<CloudBox, DomainStateError> {
    let row = registry::row(&state.paths, &box_id)?;
    if row.state.is_reachable() || registry::is_starting(&box_id) {
        return Ok(row);
    }
    if !(row.state.wakes_on_send() || row.state == CloudBoxState::Failed)
        || row.provider_box_id.is_none()
    {
        return Err(params::wrong_state(format!(
            "{} can't be resumed while it is {}.",
            row.name,
            row.state.as_str()
        )));
    }
    let provider_row = store::read_provider(&state.paths, &row.provider)?;
    if row.provider != CloudProviderId::Local
        && provider_row.map(|stored| stored.row.state)
            != Some(CloudProviderConnectionState::Connected)
    {
        return Err(params::not_connected(format!(
            "{} isn't connected. Connect it again in Settings › Cloud Boxes to wake {}.",
            row.provider.label(),
            row.name
        )));
    }
    let ops = Ops::for_provider(&state.paths, &row.provider)?;
    let target = Target {
        box_id: box_id.clone(),
        provider_box_id: row.provider_box_id.clone().unwrap_or_default(),
    };
    let ticket = boot_steps::ticket(&state, &box_id, &row.provider, row.home_project_id.clone());
    registry::start_progress(&box_id, "resume", None);
    registry::set_step(&box_id, Step::CheckCap, StepState::Skipped, None);
    let current = row.state.clone();
    let task_state = state.clone();
    // A stopped box lost its private folder (tmpfs), so the logins and ticked secrets go again.
    let home_project = row
        .home_project_id
        .as_deref()
        .and_then(|project_id| project::read_home_project(&state, project_id).ok());
    let context = SessionContext {
        snapshot_id: row.snapshot_id.clone(),
        project_path: home_project.as_ref().and_then(|home| home.path.clone()),
        settings: home_project.map(|home| home.settings).unwrap_or_default(),
        ..Default::default()
    };
    let _ = registry::update(&state.paths, &box_id, |stored| {
        stored.row.state = CloudBoxState::Creating;
        stored.row.state_detail = None;
    });
    tokio::spawn(async move {
        let resume_ops = ops.clone();
        let resume_target = target.clone();
        if step(&task_state, &box_id, Step::Create, move || {
            resume_ops.resume(&resume_target, &current)
        })
        .await
        .is_some()
        {
            let _ = registry::update(&task_state.paths, &box_id, |stored| {
                stored.row.state = CloudBoxState::Booting;
            });
            finish_boot(&task_state, ops, target, ticket, context).await;
        }
    });
    let row = registry::row(&state.paths, &row.box_id)?;
    events::broadcast_box(&state, &row);
    Ok(row)
}
