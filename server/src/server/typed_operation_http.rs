use super::*;

/*
CDXC:GxserverRustPort 2026-06-16-00:49:
Phase 7 typed operation endpoints reuse the durable project registry before building allowlisted subprocesses. Keep returned command metadata redacted and persistent logs metadata-only, because argv, cwd, stdout, stderr, branch names, file paths, and setup commands can contain user-owned content.
*/
pub(crate) async fn handle_typed_operation_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let projects = {
        let db = match open_gxserver_database(&state.paths) {
            Ok(db) => db,
            Err(error) => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "internalError",
                        message: format!("SQLite gxserver state error: {error}"),
                    },
                );
            }
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        match repository.list_projects() {
            Ok(projects) => projects,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        }
    };
    match dispatch_typed_operation_endpoint(&endpoint_path, &params, projects).await {
        Ok(result) => {
            let action = result
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let exit_code = result.get("exitCode").and_then(Value::as_i64).unwrap_or(1) as i32;
            let has_error = result.get("error").is_some();
            let level = if typed_operation_log_level(&action, exit_code, has_error) == "info" {
                crate::logging::LogLevel::Info
            } else {
                crate::logging::LogLevel::Warn
            };
            let _ = state.logger.log_routine(
                DiagnosticLogScenario::TypedOperations,
                GxserverLogInput {
                    level,
                    event: "typedOperation".to_string(),
                    server_id: Some(state.metadata.server_id.clone()),
                    request_id: Some(request_id.clone()),
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: Some(typed_operation_log_details(&result)),
                },
            );
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => {
            if error.scope_rejection {
                /*
                CDXC:ProjectBoardRouting 2026-06-22-10:22:
                Project Board and typed-operation scope misses are support-relevant but private. Match TypeScript by logging only endpoint/action/error identity and presence booleans, never raw project paths, command text, URLs, or user-owned board content.
                */
                let _ = state.logger.log(GxserverLogInput {
                    level: LogLevel::Warn,
                    event: "typedOperation.scopeRejected".to_string(),
                    server_id: Some(state.metadata.server_id.clone()),
                    request_id: Some(request_id.clone()),
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: Some(typed_operation_scope_rejection_details(
                        &endpoint_path,
                        &params,
                        &error,
                    )),
                });
            }
            typed_operation_error_response(endpoint_path, request_id, error)
        }
    }
}

pub(crate) fn typed_operation_scope_rejection_details(
    endpoint_path: &str,
    params: &Map<String, Value>,
    error: &TypedOperationError,
) -> Value {
    let mut details = Map::new();
    if let Some(action) = params.get("action").and_then(Value::as_str) {
        details.insert("action".to_string(), json!(action));
    }
    details.insert(
        "endpoint".to_string(),
        json!(endpoint_path.trim_start_matches("/api/")),
    );
    details.insert("errorCode".to_string(), json!(error.code));
    details.insert("errorType".to_string(), json!("GxserverProjectPathError"));
    details.insert(
        "hasProjectId".to_string(),
        json!(params
            .get("projectId")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())),
    );
    details.insert(
        "hasProjectPath".to_string(),
        json!(params
            .get("projectPath")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())),
    );
    Value::Object(details)
}

pub(crate) fn typed_operation_error_response(
    endpoint_path: String,
    request_id: String,
    error: TypedOperationError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "dependencyUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "forbidden" => StatusCode::FORBIDDEN,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}
