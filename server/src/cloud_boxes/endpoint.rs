//! `POST /api/cloudBoxes`: the action dispatch (`ghostex_gx_protocol::cloud_box::rpc` lists each
//! action's params and result).

use std::sync::Arc;

use ghostex_gx_protocol::cloud_box::{
    BoxParams, BoxResult, CloudBox, CloudBoxMachine, CloudBoxState, CloudBoxesAction,
    CloudProviderConnectionState, CloudProviderId, ConnectParams, ConnectResult,
    ConnectStatusParams, CreateParams, DeleteParams, ListParams, ListResult, ProjectSettingsParams,
    ProjectSettingsResult, ProviderParams, ProvidersResult, ReportBoxSessionsParams,
    SaveSnapshotParams, SetAgentLoginParams, SizesParams, SizesResult, SnapshotParams,
    SnapshotResult, SnapshotsParams, SnapshotsResult, SpendParams,
};
use serde_json::{Map, Value};

use super::params::{blocking, to_value, typed};
use super::{
    agent_logins, connect, create, events, lifecycle, project, providers, reconcile, registry,
    secret_files, snapshots, spend,
};
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
    match action {
        CloudBoxesAction::Providers => {
            let task_state = state.clone();
            let rows = blocking(move || providers::provider_rows(&task_state)).await?;
            to_value(&ProvidersResult { providers: rows })
        }
        CloudBoxesAction::Connect => {
            let request: ConnectParams = typed(params)?;
            if request.provider == CloudProviderId::Local {
                let home = state.paths.home_dir.clone();
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
            to_value(&connect::start(state, request).await?)
        }
        CloudBoxesAction::ConnectStatus => {
            let request: ConnectStatusParams = typed(params)?;
            to_value(&connect::status(&request.connect_id)?)
        }
        CloudBoxesAction::Disconnect => {
            let request: ProviderParams = typed(params)?;
            let task_state = state.clone();
            let rows = blocking(move || -> Result<_, DomainStateError> {
                connect::disconnect(&task_state, &request.provider)?;
                Ok(providers::provider_rows(&task_state))
            })
            .await??;
            to_value(&ProvidersResult { providers: rows })
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
            let task_state = state.clone();
            let boxes = blocking(move || -> Result<Vec<CloudBox>, DomainStateError> {
                if request.refresh == Some(true) {
                    reconcile::reconcile_all(&task_state);
                }
                let all_snapshots = super::store::list_snapshots(&task_state.paths)?;
                let mut boxes = registry::rows(&task_state.paths)?;
                if let Some(project_id) = request.project_id.as_deref() {
                    boxes.retain(|row| row.home_project_id.as_deref() == Some(project_id));
                }
                if request.include_deleted != Some(true) {
                    boxes.retain(|row| row.state != CloudBoxState::Deleted);
                }
                for row in boxes.iter_mut() {
                    if row.snapshot_update_reason.is_none() && row.state.is_reachable() {
                        row.snapshot_update_reason =
                            snapshots::update_reason(&all_snapshots, row.snapshot_id.as_deref());
                    }
                }
                Ok(boxes)
            })
            .await??;
            let machines = boxes
                .iter()
                .filter(|row| row.state.is_reachable())
                .map(CloudBoxMachine::from_box)
                .collect();
            to_value(&ListResult { boxes, machines })
        }
        CloudBoxesAction::Get => {
            let request: BoxParams = typed(params)?;
            let task_state = state.clone();
            let row = blocking(move || registry::row(&task_state.paths, &request.box_id)).await??;
            to_value(&BoxResult { cloud_box: row })
        }
        CloudBoxesAction::Create => {
            let request: CreateParams = typed(params)?;
            to_value(&create::create(state, request).await?)
        }
        CloudBoxesAction::Resume => {
            let request: BoxParams = typed(params)?;
            to_value(&BoxResult {
                cloud_box: create::resume(state, request.box_id).await?,
            })
        }
        CloudBoxesAction::Stop => {
            let request: BoxParams = typed(params)?;
            let task_state = state.clone();
            let row = blocking(move || lifecycle::stop(&task_state, &request.box_id)).await??;
            to_value(&BoxResult { cloud_box: row })
        }
        CloudBoxesAction::Delete => {
            let request: DeleteParams = typed(params)?;
            let task_state = state.clone();
            let row = blocking(move || {
                lifecycle::delete(&task_state, &request.box_id, request.force == Some(true))
            })
            .await??;
            to_value(&BoxResult { cloud_box: row })
        }
        CloudBoxesAction::ConnectInfo => {
            let request: BoxParams = typed(params)?;
            let task_state = state.clone();
            to_value(
                &blocking(move || lifecycle::connect_info(&task_state, &request.box_id)).await??,
            )
        }
        CloudBoxesAction::ReportBoxSessions => {
            let request: ReportBoxSessionsParams = typed(params)?;
            let task_state = state.clone();
            let row = blocking(move || {
                lifecycle::report_sessions(&task_state, &request.box_id, request.sessions)
            })
            .await??;
            to_value(&BoxResult { cloud_box: row })
        }
        CloudBoxesAction::Snapshots => {
            let request: SnapshotsParams = typed(params)?;
            let task_state = state.clone();
            let snapshots = blocking(move || {
                snapshots::list(
                    &task_state,
                    request.project_id.as_deref(),
                    request.provider.as_ref(),
                )
            })
            .await??;
            to_value(&SnapshotsResult { snapshots })
        }
        CloudBoxesAction::SaveSnapshot => {
            let request: SaveSnapshotParams = typed(params)?;
            to_value(&SnapshotResult {
                snapshot: snapshots::save(state, request).await?,
            })
        }
        CloudBoxesAction::SetDefaultSnapshot => {
            let request: SnapshotParams = typed(params)?;
            let task_state = state.clone();
            let snapshot =
                blocking(move || snapshots::set_default(&task_state, &request.snapshot_id))
                    .await??;
            to_value(&SnapshotResult { snapshot })
        }
        CloudBoxesAction::DeleteSnapshot => {
            let request: SnapshotParams = typed(params)?;
            let task_state = state.clone();
            let snapshot =
                blocking(move || snapshots::delete(&task_state, &request.snapshot_id)).await??;
            to_value(&SnapshotResult { snapshot })
        }
        CloudBoxesAction::Spend => {
            let request: SpendParams = typed(params)?;
            let task_state = state.clone();
            let spend = blocking(move || {
                if request.refresh == Some(true) {
                    spend::refresh(&task_state);
                }
                spend::read_spend(&task_state)
            })
            .await?;
            to_value(&spend)
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
            let env_files = match home_project.path {
                Some(path) => {
                    blocking(move || secret_files::env_files(std::path::Path::new(&path))).await?
                }
                None => Vec::new(),
            };
            to_value(&ProjectSettingsResult {
                project_id: request.project_id,
                settings,
                env_files,
            })
        }
        CloudBoxesAction::AgentLogins => {
            let state = state.clone();
            to_value(&blocking(move || agent_logins::list(&state)).await??)
        }
        CloudBoxesAction::SetAgentLogin => {
            let request: SetAgentLoginParams = typed(params)?;
            let state = state.clone();
            to_value(
                &blocking(move || agent_logins::set(state, &request.account_id, request.allowed))
                    .await??,
            )
        }
        CloudBoxesAction::BoxIdleStatus
        | CloudBoxesAction::BoxWorkStatus
        | CloudBoxesAction::BoxWriteBack => {
            let request: ghostex_gx_protocol::cloud_box::BoxWriteBackParams = typed(params)?;
            super::box_mode::answer_box_action(action, request.ack).await
        }
        CloudBoxesAction::PullBranch => {
            let request: ghostex_gx_protocol::cloud_box::PullBranchParams = typed(params)?;
            let row = {
                let task_state = state.clone();
                let box_id = request.box_id.clone();
                blocking(move || registry::row(&task_state.paths, &box_id)).await??
            };
            to_value(
                &crate::server::worktree_ops::pull_box_branch::pull_box_branch(
                    &state,
                    &row,
                    request.session_id.as_deref(),
                )
                .await?,
            )
        }
        CloudBoxesAction::Other(other) => Err(DomainStateError::bad_request(format!(
            "\"{other}\" is not a Cloud Boxes action."
        ))),
    }
}
