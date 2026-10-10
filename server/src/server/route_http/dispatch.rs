//! `route_http`: the gxserver HTTP gate (CORS/OPTIONS, health, auth, method, body and protocol checks) and the dispatch that offers each endpoint to the per-area route modules in turn.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::IntoResponse,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::{
    auth::is_authorized_headers,
    extensions::serve_extension_static,
    protocol::{
        endpoint_for, is_remote_endpoint_allowed, protocol_mismatch_error, rpc_error,
        EndpointDescriptor, ListenerKind, MinimalHealthResponse, Transport,
    },
};

use super::*;

/// What every per-area `route_*_http` dispatcher needs. An area that does not own the
/// endpoint hands it back unchanged, so `route_http` can offer it to the next area.
pub(super) struct RouteHttpRequest {
    pub(super) state: Arc<AppState>,
    pub(super) endpoint: EndpointDescriptor,
    pub(super) request_id: String,
    pub(super) body_json: Value,
    pub(super) token_extension_id: Option<String>,
}

pub(in crate::server) async fn route_http(
    state: Arc<AppState>,
    request: Request<Body>,
    request_id: String,
) -> RoutedResponse {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let method = parts.method.clone();

    if method == Method::GET && path.starts_with("/ext/") {
        return serve_extension_static(state.extension_registry.clone(), path).await;
    }

    if method == Method::GET && path.starts_with("/project-view-report/") {
        return crate::project_views::serve(path).await;
    }

    if matches!(method, Method::GET | Method::PUT)
        && path.starts_with(crate::file_links::FILE_LINK_ROUTE_PREFIX)
    {
        let query = parts.uri.query().map(str::to_string);
        return crate::file_links::serve(method, path, query, parts.headers, body).await;
    }

    if method == Method::GET && path.starts_with(crate::visual_pages::VISUAL_PAGE_ROUTE_PREFIX) {
        return crate::visual_pages::serve(state.paths.root_dir.clone(), path).await;
    }

    let endpoint = endpoint_for(&path);

    /*
    CDXC:ServerApi 2026-06-22-04:10:
    Rust routing must preserve TypeScript's protocol gate order: CORS/OPTIONS is answered before minimal health, auth, method, body, and protocol checks. Unknown or WebSocket-only OPTIONS requests therefore return the HTTP-endpoint 404 envelope instead of the generic endpoint lookup message.
    */
    if method == Method::OPTIONS {
        let Some(endpoint) = endpoint else {
            return routed_json(
                None,
                StatusCode::NOT_FOUND,
                rpc_error(
                    "notFound",
                    format!("{path} is not a gxserver HTTP endpoint."),
                    Some(request_id),
                ),
            );
        };
        if endpoint.transport != Transport::Http {
            return routed_json(
                Some(endpoint.path),
                StatusCode::NOT_FOUND,
                rpc_error(
                    "notFound",
                    format!("{path} is not a gxserver HTTP endpoint."),
                    Some(request_id),
                ),
            );
        }
        if !is_remote_endpoint_allowed(ListenerKind::Local, endpoint.permission) {
            return routed_json(
                Some(endpoint.path.clone()),
                StatusCode::FORBIDDEN,
                rpc_error(
                    "forbidden",
                    format!(
                        "{} is not available on the remote gxserver listener.",
                        endpoint.path
                    ),
                    Some(request_id),
                ),
            );
        }
        return RoutedResponse {
            endpoint_path: Some(endpoint.path),
            response: StatusCode::NO_CONTENT.into_response(),
        };
    }

    if method == Method::GET && path == "/api/health" {
        return routed_json(
            Some("/api/health".to_string()),
            StatusCode::OK,
            MinimalHealthResponse::new(&state.version),
        );
    }

    let Some(endpoint) = endpoint else {
        return routed_json(
            None,
            StatusCode::NOT_FOUND,
            rpc_error(
                "notFound",
                format!("No gxserver endpoint for {} {}.", method.as_str(), path),
                Some(request_id),
            ),
        );
    };

    if endpoint.transport != Transport::Http {
        return routed_json(
            None,
            StatusCode::NOT_FOUND,
            rpc_error(
                "notFound",
                format!("No gxserver endpoint for {} {}.", method.as_str(), path),
                Some(request_id),
            ),
        );
    }

    if endpoint.path != "/api/health/server" && method != Method::POST {
        return routed_json(
            Some(endpoint.path.clone()),
            StatusCode::METHOD_NOT_ALLOWED,
            rpc_error(
                "methodNotAllowed",
                format!("{} requires POST.", endpoint.path),
                Some(request_id),
            ),
        );
    }
    if endpoint.path == "/api/health/server" && method != Method::GET {
        return routed_json(
            Some(endpoint.path.clone()),
            StatusCode::METHOD_NOT_ALLOWED,
            rpc_error(
                "methodNotAllowed",
                format!("{} requires GET.", endpoint.path),
                Some(request_id),
            ),
        );
    }

    let token_extension_id = state
        .extension_registry
        .authorize_api_token(&parts.headers, &endpoint.path);
    if endpoint.requires_auth
        && !is_authorized_headers(&parts.headers, &state.auth_token)
        && token_extension_id.is_none()
    {
        return routed_json(
            Some(endpoint.path),
            StatusCode::UNAUTHORIZED,
            rpc_error(
                "unauthorized",
                "gxserver auth token is required for this endpoint.",
                Some(request_id),
            ),
        );
    }

    let body_json = if method == Method::POST {
        let body_limit_bytes = json_body_limit_bytes(&endpoint.path);
        match read_json_body(&parts.headers, body, body_limit_bytes).await {
            Ok(value) => value,
            Err(ReadBodyError::TooLarge) => {
                return routed_json(
                    Some(endpoint.path),
                    StatusCode::PAYLOAD_TOO_LARGE,
                    rpc_error(
                        "badRequest",
                        format!(
                            "Request body exceeds the gxserver JSON RPC limit of {body_limit_bytes} bytes."
                        ),
                        Some(request_id),
                    ),
                );
            }
            Err(ReadBodyError::InvalidJson) => {
                return routed_json(
                    Some(endpoint.path),
                    StatusCode::BAD_REQUEST,
                    rpc_error(
                        "badRequest",
                        "Request body must be valid JSON.",
                        Some(request_id),
                    ),
                );
            }
        }
    } else {
        json!({})
    };

    if endpoint.requires_protocol_version && token_extension_id.is_none() {
        let protocol_version = read_protocol_version(&parts.headers, &parts.uri, Some(&body_json));
        if !is_expected_protocol_version(protocol_version.as_ref()) {
            return routed_json(
                Some(endpoint.path),
                StatusCode::UPGRADE_REQUIRED,
                protocol_mismatch_error(protocol_version, Some(request_id)),
            );
        }
    }

    if !is_remote_endpoint_allowed(ListenerKind::Local, endpoint.permission) {
        return routed_json(
            Some(endpoint.path.clone()),
            StatusCode::FORBIDDEN,
            rpc_error(
                "forbidden",
                format!(
                    "{} is not available on the remote gxserver listener.",
                    endpoint.path
                ),
                Some(request_id),
            ),
        );
    }

    let request = RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    };
    // Every area matches distinct literal endpoint paths, so the order the areas are tried in
    // never changes which arm answers.
    let request = match route_projects_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_sessions_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_sidebar_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_prompts_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_agents_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_chat_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_git_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_control_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_work_mode_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_team_sync_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_team_slack_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_work_links_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_workspaces_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_work_tickets_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_work_items_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_work_tracker_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_work_cloud_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let RouteHttpRequest {
        endpoint,
        request_id,
        ..
    } = request;
    routed_json(
        Some(endpoint.path.clone()),
        StatusCode::NOT_IMPLEMENTED,
        rpc_error(
            "notImplemented",
            format!(
                "{} is defined but not implemented in this milestone.",
                endpoint.path
            ),
            Some(request_id),
        ),
    )
}
