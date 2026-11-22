//! `/api/searchProjectFiles`: Quick Access's Files tab asks this machine for the files of some of
//! its projects that match what the user typed. The client names projects, never folders; each
//! project's folder comes from this machine's own project records.
//!
//! SEE-ALSO: packages/project-docs/src/file_search.rs (the listing and the ranking),
//! packages/gx-core/src/quick_access/files.rs (the tab that asks).

use std::path::PathBuf;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::session_git_status::run_git_probe_command;
use crate::storage::open_gxserver_database;

/// Most projects one request may search.
const MAX_PROJECTS: usize = 64;
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

pub(crate) async fn handle_search_project_files_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(256)
        .collect::<String>();
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .map(|limit| (limit as usize).clamp(1, MAX_LIMIT))
        .unwrap_or(DEFAULT_LIMIT);
    let project_ids: Vec<String> = params
        .get("projectIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .take(MAX_PROJECTS)
        .map(str::to_string)
        .collect();
    let resolved = (|| {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let mut roots: Vec<(String, Option<PathBuf>)> = Vec::new();
        for project_id in &project_ids {
            let root = repository
                .get_project(project_id)?
                .and_then(|project| {
                    project
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|path| !path.is_empty())
                        .map(PathBuf::from)
                })
                .filter(|path| path.is_absolute() && path.is_dir());
            roots.push((project_id.clone(), root));
        }
        Ok::<_, DomainStateError>(roots)
    })();
    let roots = match resolved {
        Ok(roots) => roots,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let searched = tokio::task::spawn_blocking(move || {
        roots
            .into_iter()
            .map(|(project_id, root)| {
                let Some(root) = root else {
                    return json!({ "projectId": project_id, "files": [], "total": 0, "unavailable": true });
                };
                let outcome = ghostex_docs::file_search::search(
                    &root,
                    &query,
                    limit,
                    run_git_probe_command,
                );
                json!({
                    "projectId": project_id,
                    "rootPath": root.to_string_lossy(),
                    "files": outcome.matches.into_iter().map(|found| found.path).collect::<Vec<_>>(),
                    "total": outcome.total,
                    "incomplete": outcome.incomplete,
                })
            })
            .collect::<Vec<_>>()
    })
    .await;
    let Ok(projects) = searched else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: "Project files could not be searched.".to_string(),
            },
        );
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "projects": projects })),
    )
}
