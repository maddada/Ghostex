//! `/api/beadsView` and `/api/beadsGenerateTitle` (the Beads view; `crate::beads_view`).

use super::*;

fn registered_projects(state: &AppState) -> Result<Vec<Value>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    DomainRepository::new(&db, state.metadata.server_id.as_str()).list_projects()
}

pub(crate) async fn handle_beads_view_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let projects = match registered_projects(state) {
        Ok(projects) => projects,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = if endpoint_path == "/api/beadsGenerateTitle" {
        crate::beads_view::generate_bead_title(state, &params, projects)
            .await
            .map(|title| json!({ "title": title }))
    } else {
        crate::beads_view::beads_view_snapshot(&params, projects)
            .await
            .map(|snapshot| serde_json::to_value(snapshot).unwrap_or(Value::Null))
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => typed_operation_error_response(endpoint_path, request_id, error),
    }
}
