//! `route_http`: the gxserver HTTP gate (CORS/OPTIONS, health, auth, method, body and protocol checks) and the dispatch that offers each endpoint to the per-area route modules in turn.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::IntoResponse,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::{
    auth::{bearer_token, hash_device_api_token, is_authorized_headers},
    connected_computers::{authorize_peer_token, PeerAuthorization},
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
        return crate::file_links::serve(
            state.paths.root_dir.clone(),
            method,
            path,
            query,
            parts.headers,
            body,
        )
        .await;
    }

    // Before the page itself: `/visual/<id>/scroll` is the page's own scroll position.
    if matches!(method, Method::GET | Method::POST) && crate::visual_pages::is_scroll_route(&path) {
        return crate::visual_pages::page_scroll(state.clone(), method, path, body).await;
    }

    // Before the page itself too: `GET /visual/<id>/draft` is the page's saved draft.
    if method == Method::GET && crate::visual_pages::is_draft_route(&path) {
        return crate::visual_pages::read_draft(state.clone(), path).await;
    }

    if method == Method::GET && path.starts_with(crate::visual_pages::VISUAL_PAGE_ROUTE_PREFIX) {
        let host = parts
            .headers
            .get(axum::http::header::HOST)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let query = parts.uri.query().map(str::to_string);
        return crate::visual_pages::serve(state.paths.root_dir.clone(), path, query, host).await;
    }

    if method == Method::POST && crate::visual_pages::is_answers_route(&path) {
        return crate::visual_pages::submit_answers(state.clone(), path, body).await;
    }

    if method == Method::POST && crate::visual_pages::is_draft_route(&path) {
        return crate::visual_pages::save_draft(state.clone(), path, body).await;
    }

    if method == Method::POST && crate::visual_pages::is_image_route(&path) {
        return crate::visual_pages::save_page_image(state.clone(), path, body).await;
    }

    if method == Method::POST && crate::visual_pages::is_notice_route(&path) {
        return crate::visual_pages::dismiss_page_notice(state.clone(), path, body).await;
    }

    // Composio's sign-in comes back in the user's browser, which has no gxserver token; the
    // handler trusts only the OAuth `state` a workspace's own sign-in issued.
    if method == Method::GET
        && path == crate::connectors::OAUTH_REDIRECT_PATH
        && crate::connectors::connectors_enabled(&state.paths)
    {
        let query = parts.uri.query().map(str::to_string);
        return crate::connectors::serve_oauth_redirect(state.clone(), query).await;
    }

    if path.starts_with(crate::connected_computers::RELAY_ROUTE_PREFIX) {
        if !is_authorized_headers(&parts.headers, &state.auth_token) {
            return routed_json(
                Some(crate::connected_computers::RELAY_ROUTE_PREFIX.to_string()),
                StatusCode::UNAUTHORIZED,
                rpc_error(
                    "unauthorized",
                    "Only this computer's own agents can reach its connected computers.",
                    Some(request_id),
                ),
            );
        }
        return crate::connected_computers::relay(method, &path, parts.headers, body, request_id)
            .await;
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
    /*
    CDXC:RemoteMachines 2026-11-03 WHY:
    A connected computer's agents reach this gxserver with the token this computer gave that computer (`issueComputerGrant`), never this computer's own token, so turning off its Agent access or disconnecting it cuts them off at once. That token opens only `PEER_ALLOWED_ENDPOINTS` (connected_computers/scope.rs), not the whole `RemoteAllowed` surface a remote desktop client uses.
    */
    let peer = (endpoint.requires_auth
        && token_extension_id.is_none()
        && !is_authorized_headers(&parts.headers, &state.auth_token))
    .then(|| bearer_token(&parts.headers).and_then(authorize_peer_token))
    .flatten();
    match &peer {
        Some(PeerAuthorization::AccessOff { name }) => {
            let this = crate::connected_computers::this_computer_name();
            return crate::connected_computers::peer_refusal(
                "agentAccessOff",
                format!("{this} has Agent access for this computer turned off. Turn on Agent access for {name} in {this}'s Settings > Remote > Remote machines."),
                request_id,
            );
        }
        Some(PeerAuthorization::Allowed { .. })
            if !crate::connected_computers::is_peer_allowed(&endpoint.path) =>
        {
            return crate::connected_computers::peer_refusal(
                "forbidden",
                format!(
                    "Agents on another computer can't use {} on {}.",
                    endpoint.path,
                    crate::connected_computers::this_computer_name()
                ),
                request_id,
            );
        }
        _ => {}
    }
    // Which paired phone's own token (if any) authorized this request is settled once, here,
    // before the body is read: a phone removed while its body arrives must not pass the gate as
    // itself and then reach the push registration as the main token (CDXC:Notifications
    // 2026-11-07 DECISION in push/registration.rs).
    let caller_token_hash = bearer_token(&parts.headers)
        .filter(|token| state.device_api_tokens.accepts(token))
        .map(hash_device_api_token);
    // A Ghostex Collab share token (server/src/collab/shares.rs): only its own chat, only
    // `GUEST_ENDPOINTS`, checked again against the request's ids once the body is read.
    let share = (endpoint.requires_auth
        && token_extension_id.is_none()
        && peer.is_none()
        && !is_authorized_headers(&parts.headers, &state.auth_token)
        && caller_token_hash.is_none())
    .then(|| bearer_token(&parts.headers).and_then(crate::collab::shares::authorize_token))
    .flatten();
    if share.is_some() && !crate::collab::shares::GUEST_ENDPOINTS.contains(&endpoint.path.as_str())
    {
        return super::super::collab_http::guest_refusal(endpoint.path, request_id);
    }
    if endpoint.requires_auth
        && !is_authorized_headers(&parts.headers, &state.auth_token)
        && caller_token_hash.is_none()
        && token_extension_id.is_none()
        && peer.is_none()
        && share.is_none()
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
            Err(ReadBodyError::TooLarge { size_bytes }) => {
                let mut response = routed_json(
                    Some(endpoint.path),
                    StatusCode::PAYLOAD_TOO_LARGE,
                    rpc_error(
                        "payloadTooLarge",
                        match size_bytes {
                            Some(size) => format!("Request body is {size} bytes; the limit is {body_limit_bytes} bytes."),
                            None => format!("Request body exceeds the {body_limit_bytes} byte limit."),
                        },
                        Some(request_id),
                    ),
                );
                response.response.extensions_mut().insert(BodySizeLimit {
                    size_bytes,
                    limit_bytes: body_limit_bytes,
                });
                return response;
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

    if let Some(share) = share {
        if let Some(response) = super::super::collab_http::route_guest(
            &state,
            share,
            &parts.headers,
            &endpoint.path,
            request_id.clone(),
            &body_json,
        )
        .await
        {
            return response;
        }
    }

    // An agent another computer starts here gets its hook first (agent_hooks/peer_start.rs).
    if let Some(PeerAuthorization::Allowed { name, .. }) = &peer {
        crate::agent_hooks::install_for_peer_agent_start(&state, &endpoint.path, &body_json, name)
            .await;
    }
    // The exchange needs to know which computer's token asked, which no area handler sees.
    if endpoint.path == crate::settings_sync::EXCHANGE_ENDPOINT {
        let peer_server_id = match &peer {
            Some(PeerAuthorization::Allowed { server_id, .. }) => Some(server_id.clone()),
            _ => None,
        };
        return crate::settings_sync::handle_settings_sync_exchange(
            state,
            peer_server_id,
            body_json,
            request_id,
        )
        .await;
    }
    // Push registration needs to know which paired phone's own token (if any) authorized the
    // request, settled at the auth gate above.
    if endpoint.path == crate::push::REGISTER_PUSH_TOKEN_ENDPOINT {
        return crate::push::handle_register_push_token_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            caller_token_hash,
        )
        .await;
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
    let request = match route_session_links_http(request).await {
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
    let request = match route_work_source_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_work_cloud_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_claude_cloud_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_gbots_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_connectors_http(request).await {
        Ok(response) => return response,
        Err(request) => request,
    };
    let request = match route_secrets_http(request).await {
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
