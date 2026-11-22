//! Home projects as Cloud Boxes sees them: the project key and the project's Cloud Box settings.

use ghostex_gx_cloud::project_key::project_key_from_origin;
use ghostex_gx_protocol::cloud_box::{CLOUD_BOX_RUNTIME_SETTINGS_KEY, ProjectCloudBoxSettings};
use serde_json::{Map, Value, json};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

pub(super) struct HomeProject {
    pub(super) project_id: String,
    pub(super) project_key: String,
    pub(super) settings: ProjectCloudBoxSettings,
    /// The project folder on this computer.
    pub(super) path: Option<String>,
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
        path,
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

impl HomeProject {
    /// The repository's origin URL, which a blank box clones, without any credentials in it.
    pub(super) fn origin_url(&self) -> Option<String> {
        self.path.as_deref().and_then(|path| {
            crate::project_git_remote::cached_project_git_remote(path)
                .and_then(|remote| remote.origin_url)
                .map(|origin| without_credentials(&origin))
        })
    }

    /// The base branch the project's new-chat bar remembers, if any.
    pub(super) fn remembered_base_branch(&self) -> Option<String> {
        self.value
            .pointer("/runtimeSettings/newChatWorktree/baseBranch")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    /// The project's name, for its folder on a box.
    pub(super) fn name(&self) -> String {
        self.value
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.project_id.clone())
    }
}

/// CDXC:CloudBoxes 2026-11-21 WHY: an origin can carry a token (`https://user:ghp_…@github.com/o/r`),
/// and the clone request sent it to the box as-is, into the `git clone` arguments, the checkout's
/// `.git/config` and so every snapshot of it. The request now gets the URL without credentials: all
/// of an http(s) URL's user part (a token can stand there alone), and an ssh:// URL's password,
/// keeping its user (`git@`). A box reaches a private repository through its own GitHub login.
pub(super) fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        // scp-like `git@github.com:o/r` has no password.
        return url.to_string();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_string();
    };
    let user = if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        ""
    } else {
        userinfo.split(':').next().unwrap_or_default()
    };
    let user = if user.is_empty() {
        String::new()
    } else {
        format!("{user}@")
    };
    format!("{scheme}://{user}{host}{tail}")
}
