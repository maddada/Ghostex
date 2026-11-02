//! `POST /api/cloudBoxes`: the action dispatch (`ghostex_gx_protocol::cloud_box::rpc` lists each
//! action's params and result).

use std::sync::Arc;

use ghostex_gx_protocol::cloud_box::{
    box_verbs, error_codes, BoxParams, BoxResult, BoxWorkStatus, CloudBox, CloudBoxMachine,
    CloudBoxState, CloudBoxesAction, CloudProviderConnectionState, CloudProviderId, ConnectParams,
    ConnectResult, CreateParams, DeleteParams, ListParams, ListResult, ProjectSettingsParams,
    ProjectSettingsResult, ProvidersResult, SizesParams, SizesResult,
};
use serde_json::{Map, Value};

use super::params::{blocking, provider_error, provider_unavailable, to_value, typed, wrong_state};
use super::{create, events, local_provider, project, providers, registry, spend};
use crate::domain::DomainStateError;
use crate::server::AppState;

pub(crate) async fn dispatch(
    state: Arc<AppState>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .map(CloudBoxesAction::from_wire)
        .ok_or_else(|| DomainStateError::bad_request("Name a Cloud Boxes action with `action`."))?;
    let home = state.paths.home_dir.clone();
    match action {
        CloudBoxesAction::Providers => {
            let include_local = providers::local_provider_listed(&state);
            let rows = blocking(move || providers::provider_rows(&home, include_local)).await?;
            to_value(&ProvidersResult { providers: rows })
        }
        CloudBoxesAction::Connect => {
            let request: ConnectParams = typed(params)?;
            if request.provider == CloudProviderId::Local {
                let row = blocking(move || providers::local_row(&home)).await?;
                return to_value(&ConnectResult {
                    provider: CloudProviderId::Local,
                    state: CloudProviderConnectionState::Connected,
                    connect_id: None,
                    auth_url: None,
                    user_code: None,
                    browser_opened: false,
                    expires_at: None,
                    provider_row: Some(row),
                    error: None,
                });
            }
            Err(not_built(&format!(
                "Connecting {}",
                request.provider.label()
            )))
        }
        CloudBoxesAction::ConnectStatus | CloudBoxesAction::Disconnect => {
            Err(not_built("Provider sign-in"))
        }
        CloudBoxesAction::Sizes => {
            let request: SizesParams = typed(params)?;
            let providers = match request.provider {
                Some(provider) => vec![provider],
                None => vec![
                    CloudProviderId::Instabox,
                    CloudProviderId::Boat,
                    CloudProviderId::Local,
                ],
            };
            let sizes = providers
                .iter()
                .flat_map(ghostex_gx_cloud::prices::sizes)
                .collect();
            to_value(&SizesResult { sizes })
        }
        CloudBoxesAction::List => {
            let request: ListParams = typed(params)?;
            let mut boxes = list_boxes(&state).await?;
            if let Some(project_id) = request.project_id.as_deref() {
                boxes.retain(|row| row.home_project_id.as_deref() == Some(project_id));
            }
            if request.include_deleted != Some(true) {
                boxes.retain(|row| row.state != CloudBoxState::Deleted);
            }
            let machines = boxes
                .iter()
                .filter(|row| row.state.is_reachable())
                .map(CloudBoxMachine::from_box)
                .collect();
            to_value(&ListResult { boxes, machines })
        }
        CloudBoxesAction::Get => {
            let request: BoxParams = typed(params)?;
            to_value(&BoxResult {
                cloud_box: box_row(&state, &request.box_id).await?,
            })
        }
        CloudBoxesAction::Create => {
            let request: CreateParams = typed(params)?;
            to_value(&create::create(state, request).await?)
        }
        CloudBoxesAction::Resume => {
            let request: BoxParams = typed(params)?;
            let row = box_row(&state, &request.box_id).await?;
            if row.state.is_reachable() || row.state.is_starting() {
                return to_value(&BoxResult { cloud_box: row });
            }
            if !(row.state.wakes_on_send() || row.state == CloudBoxState::Failed)
                || row.provider_box_id.is_none()
            {
                return Err(wrong_state(format!(
                    "{} can't be resumed while it is {}.",
                    row.name,
                    row.state.as_str()
                )));
            }
            create::resume_local(state.clone(), row.box_id.clone()).await?;
            let row = box_row(&state, &request.box_id).await?;
            events::broadcast_box(&state, &row);
            to_value(&BoxResult { cloud_box: row })
        }
        CloudBoxesAction::Stop => {
            let request: BoxParams = typed(params)?;
            let row = box_row(&state, &request.box_id).await?;
            if row.provider_box_id.is_none() {
                return Err(wrong_state(format!(
                    "{} has no machine to stop yet.",
                    row.name
                )));
            }
            let box_id = row.box_id.clone();
            blocking(move || local_provider::stop(&home, &box_id))
                .await?
                .map_err(provider_error)?;
            let row = box_row(&state, &request.box_id).await?;
            events::broadcast_box(&state, &row);
            to_value(&BoxResult { cloud_box: row })
        }
        CloudBoxesAction::Delete => {
            let request: DeleteParams = typed(params)?;
            to_value(&BoxResult {
                cloud_box: delete(&state, request).await?,
            })
        }
        CloudBoxesAction::ConnectInfo => {
            let request: BoxParams = typed(params)?;
            let box_id = request.box_id.clone();
            let container = blocking(move || local_provider::find_container(&home, &box_id))
                .await?
                .map_err(provider_unavailable)?
                .ok_or_else(|| {
                    DomainStateError::not_found(format!("No box {}.", request.box_id))
                })?;
            let row = registry::local_row(&container);
            if !row.state.is_reachable() {
                return Err(wrong_state(format!(
                    "{} is {}; send a message to wake it first.",
                    row.name,
                    row.state.as_str()
                )));
            }
            let home = state.paths.home_dir.clone();
            let mut info = blocking(move || local_provider::connect_info(&home, &container))
                .await?
                .map_err(provider_error)?;
            info.gxserver_version = row.gxserver_version;
            to_value(&info)
        }
        CloudBoxesAction::Snapshots
        | CloudBoxesAction::SaveSnapshot
        | CloudBoxesAction::SetDefaultSnapshot
        | CloudBoxesAction::DeleteSnapshot => Err(not_built("Snapshots")),
        CloudBoxesAction::Spend => {
            let month = spend::month_of(&crate::domain::params::now_iso());
            to_value(&spend::read_spend(&state, &month))
        }
        CloudBoxesAction::ProjectSettings => {
            let request: ProjectSettingsParams = typed(params)?;
            let home_project = project::read_home_project(&state, &request.project_id)?;
            let settings = match request.settings {
                Some(settings) => {
                    project::write_project_settings(&state, &home_project, &settings)?;
                    events::broadcast_scope(&state, &["snapshots"]);
                    settings
                }
                None => home_project.settings,
            };
            to_value(&ProjectSettingsResult {
                project_id: request.project_id,
                settings,
            })
        }
        CloudBoxesAction::BoxIdleStatus
        | CloudBoxesAction::BoxWorkStatus
        | CloudBoxesAction::BoxWriteBack => Err(super::box_mode::not_in_box_mode()),
        CloudBoxesAction::Other(other) => Err(DomainStateError::bad_request(format!(
            "\"{other}\" is not a Cloud Boxes action."
        ))),
    }
}

/// A part of Cloud Boxes that the provider layer's next commit brings.
fn not_built(what: &str) -> DomainStateError {
    DomainStateError {
        code: "unsupported",
        message: format!(
            "{what}: not built yet; this Ghostex has only the Local (Docker) provider."
        ),
    }
}

/// Every box: the provider lists (the registry) plus boxes still being created.
async fn list_boxes(state: &Arc<AppState>) -> Result<Vec<CloudBox>, DomainStateError> {
    let home = state.paths.home_dir.clone();
    let docker_ready = local_provider_known(state);
    let mut boxes: Vec<CloudBox> = if docker_ready {
        blocking(move || local_provider::list_containers(&home))
            .await?
            // No Docker means no local boxes, not an error a list has to show.
            .unwrap_or_default()
            .iter()
            .map(registry::local_row)
            .collect()
    } else {
        Vec::new()
    };
    let extra = create::in_flight_rows(&boxes);
    boxes.extend(extra);
    Ok(boxes)
}

/// Local boxes are listed when the provider is, and also when Docker is installed at all (a box
/// made while the switch was on must not vanish from the list when it is turned off).
fn local_provider_known(state: &AppState) -> bool {
    providers::local_provider_listed(state)
        || super::docker::resolve_docker(&state.paths.home_dir).is_some()
}

/// One box's current row.
pub(super) async fn box_row(
    state: &Arc<AppState>,
    box_id: &str,
) -> Result<CloudBox, DomainStateError> {
    if !ghostex_gx_cloud::ids::is_gx_id(box_id) {
        return Err(DomainStateError::bad_request(format!(
            "\"{box_id}\" is not a box id."
        )));
    }
    list_boxes(state)
        .await?
        .into_iter()
        .find(|row| row.box_id == box_id)
        .ok_or_else(|| DomainStateError::not_found(format!("No box {box_id}.")))
}

/// `delete`. D13: a box with uncommitted or unpushed work is kept unless `force`.
async fn delete(
    state: &Arc<AppState>,
    request: DeleteParams,
) -> Result<CloudBox, DomainStateError> {
    let mut row = box_row(state, &request.box_id).await?;
    if row.provider_box_id.is_none() {
        // Never got a machine: only the registry entry goes.
        create::forget_in_flight(&row.box_id);
        registry::forget(&row.box_id);
        row.state = CloudBoxState::Deleted;
        events::broadcast_box(state, &row);
        return Ok(row);
    }
    let force = request.force == Some(true);
    if !force && row.state.is_reachable() {
        let home = state.paths.home_dir.clone();
        let box_id = row.box_id.clone();
        let work = blocking(move || {
            local_provider::run_box_verb(&home, &box_id, box_verbs::WORK_STATUS, &[])
        })
        .await?
        .and_then(|value| {
            serde_json::from_value::<BoxWorkStatus>(value).map_err(|error| error.to_string())
        });
        match work {
            Ok(work) if work.has_unpushed_work => {
                return Err(DomainStateError {
                    code: error_codes::HAS_UNPUSHED_WORK,
                    message: format!("{} has unpushed work: push it, or delete anyway.", row.name),
                })
            }
            Ok(_) => {}
            Err(message) => {
                return Err(DomainStateError {
                    code: error_codes::HAS_UNPUSHED_WORK,
                    message: format!(
                    "Couldn't check {} for unpushed work ({message}). Delete anyway to remove it.",
                    row.name
                ),
                })
            }
        }
    }
    registry::set_transition(&row.box_id, Some(CloudBoxState::Deleting));
    row.state = CloudBoxState::Deleting;
    events::broadcast_box(state, &row);
    let home = state.paths.home_dir.clone();
    let box_id = row.box_id.clone();
    let result = blocking(move || local_provider::delete(&home, &box_id)).await?;
    registry::set_transition(&row.box_id, None);
    result.map_err(provider_error)?;
    registry::forget(&row.box_id);
    row.state = CloudBoxState::Deleted;
    row.progress = None;
    events::broadcast_box(state, &row);
    Ok(row)
}
