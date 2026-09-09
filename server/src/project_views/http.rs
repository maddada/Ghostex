use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
};
use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use serde_json::{json, Value};

pub(crate) async fn handle(
    state: &AppState,
    path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => Value::Object(params),
        Err(error) => {
            return routed_json(
                Some(path),
                StatusCode::BAD_REQUEST,
                rpc_error(error.code, error.message, Some(request_id)),
            )
        }
    };
    let port = state.config.listeners.local.port;
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        if super::resolve::text(&params["view"], "availability") == "spaces" {
            let db = crate::storage::open_gxserver_database(&paths)?;
            if !super::scope::matches(&db, &server_id, &params)? {
                let mut release = params;
                release["operation"] = json!("release");
                super::runtime::operation(release, port)?;
                return Ok(json!({"state":"unavailable", "available":false, "scopeMatches":false}));
            }
        }
        let mut status = super::runtime::operation(params, port)?;
        status["scopeMatches"] = json!(true);
        Ok(status)
    })
    .await;
    match result {
        Ok(Ok(status)) => routed_json(
            Some(path),
            StatusCode::OK,
            rpc_success(request_id, json!({"status":status})),
        ),
        error => routed_json(
            Some(path),
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                match error {
                    Ok(Err(error)) => format!("{error:#}"),
                    _ => "Project view operation failed.".into(),
                },
                Some(request_id),
            ),
        ),
    }
}
pub(crate) async fn serve(path: String) -> RoutedResponse {
    tokio::task::spawn_blocking(move || serve_sync(&path))
        .await
        .unwrap_or_else(|_| not_found())
}
fn not_found() -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: StatusCode::NOT_FOUND.into_response(),
    }
}
fn serve_sync(path: &str) -> RoutedResponse {
    let Some((token, relative)) = path
        .strip_prefix("/project-view-report/")
        .and_then(|s| s.split_once('/'))
    else {
        return not_found();
    };
    let Some(instance) = super::runtime::report_instance(token) else {
        return not_found();
    };
    let Some(root) = instance.report.lock().ok().and_then(|r| r.clone()) else {
        return not_found();
    };
    let Ok(relative) = crate::extensions::serve::decode_relative_path(relative) else {
        return not_found();
    };
    let Ok(file) = std::fs::canonicalize(root.join(relative)) else {
        return not_found();
    };
    if !file.starts_with(&root) || !file.is_file() {
        return not_found();
    }
    let Ok(bytes) = std::fs::read(&file) else {
        return not_found();
    };
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(crate::extensions::serve::extension_content_type(&file)),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}
