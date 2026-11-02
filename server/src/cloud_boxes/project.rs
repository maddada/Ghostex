//! Home projects as Cloud Boxes sees them: the project key and the project's Cloud Box settings.

use ghostex_gx_cloud::project_key::project_key_from_origin;
use ghostex_gx_protocol::cloud_box::{ProjectCloudBoxSettings, CLOUD_BOX_RUNTIME_SETTINGS_KEY};
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

pub(super) struct HomeProject {
    pub(super) project_id: String,
    pub(super) project_key: String,
    pub(super) settings: ProjectCloudBoxSettings,
    value: Value,
}

fn open(state: &AppState) -> Result<crate::storage::GxserverDatabase, DomainStateError> {
    open_gxserver_database(&state.paths)
        .map_err(|error| super::params::internal(format!("SQLite gxserver state error: {error}")))
}

/// The project key: the normalized origin URL when the project is a repository with one, else
/// its path (`packages/gx-cloud`, `project_key.rs`).
fn project_key(path: Option<&str>, project_id: &str) -> String {
    path.and_then(|path| {
        crate::project_git_remote::cached_project_git_remote(path)
            .and_then(|remote| remote.origin_url)
            .and_then(|origin| project_key_from_origin(&origin))
            .or_else(|| Some(path.to_string()))
    })
    .unwrap_or_else(|| format!("project:{project_id}"))
}

pub(super) fn read_home_project(
    state: &AppState,
    project_id: &str,
) -> Result<HomeProject, DomainStateError> {
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let value = repository.get_project(project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string);
    let settings = value
        .pointer(&format!(
            "/runtimeSettings/{CLOUD_BOX_RUNTIME_SETTINGS_KEY}"
        ))
        .cloned()
        .and_then(|stored| serde_json::from_value(stored).ok())
        .unwrap_or_default();
    Ok(HomeProject {
        project_key: project_key(path.as_deref(), project_id),
        project_id: project_id.to_string(),
        settings,
        value,
    })
}

/// Replaces the project's `runtimeSettings.cloudBox`, keeping every other runtime setting.
pub(super) fn write_project_settings(
    state: &AppState,
    project: &HomeProject,
    settings: &ProjectCloudBoxSettings,
) -> Result<(), DomainStateError> {
    let mut runtime_settings: Map<String, Value> = project
        .value
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert(
        CLOUD_BOX_RUNTIME_SETTINGS_KEY.to_string(),
        super::params::to_value(settings)?,
    );
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project.project_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_project(&update)?;
    Ok(())
}
