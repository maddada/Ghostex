//! The scoped extension API, `/api/ext/v1`: what a sidebar extension (or a side panel view) uses
//! to fill its panel and to read Ghostex, limited to what the user granted.
//!
//! | Route | Who | Does |
//! | --- | --- | --- |
//! | `GET /api/ext/v1/panel?surface=&session=` | the extension, and you and your agents (read) | The panel document and its revision |
//! | `PUT /api/ext/v1/panel` `{document, baseRevision?}` | the extension | Replaces the panel |
//! | `PATCH /api/ext/v1/panel` `{blocks?, removeBlocks?, title?, tabs?, baseRevision?}` | the extension | Upserts blocks by id, removes blocks |
//! | `GET /api/ext/v1/projects` | the extension, with `projects.read` | This computer's projects |
//! | `GET /api/ext/v1/sessions?project=` | the extension, with `sessions.read` | Its sessions and what they are doing |
//! | `GET /api/ext/v1/events` | the extension, and you and your agents (its panel events only) | Server-sent events: `panelChanged`, `checklistEdited`, `panelAction`, and `projectsChanged` / `sessionsChanged` with the matching read permission |
//! | `POST /api/ext/v1/panelAction` `{extension, surface?, session?, blockId, itemId?, edit? \| action?, data?, baseRevision?}` | your Ghostex apps and phones | A tap: an editable-checklist edit gxserver applies, or an `extension` action passed to the extension |
//!
//! The extension is the one whose server holds the bearer token (`GHOSTEX_API_TOKEN`, given to
//! its command server at start); every other caller names it with `?extension=<id>`. Answered
//! before gxserver's token gate (route_http/dispatch.rs), which keeps an extension token to its
//! three old endpoints. Refusals: 400, 401, 403 (a permission the user didn't grant, or a caller
//! that may only read), 404, 409 (a stale `baseRevision`, or the extension is off), 413.
//!
//! CDXC:Extensions 2026-11-14 DECISION:
//! Q25 (user picked a scoped extension API in gxserver over the CLI or a page-owned store): gxserver owns each panel and pushes it to every client in the sidebar data they already receive; the extension changes it through this API, and the CLI (`ghostex extensions panel …`) and, later, the web block's bridge are thin wrappers over the same routes.
//!
//! CDXC:Extensions 2026-11-14 WHY:
//! Q28 (who may change an installed panel's content from the CLI) was left to the build; the safest choice was taken: only the extension's own process writes its panel, through its own token, so `ghostex extensions panel set|patch` works only inside that process (where `GHOSTEX_API_TOKEN` is set). Agents and scripts may read a panel and follow its events. Your own devices (the desktop app, the web build, paired phones, the CLI with the token file) also send taps, and gxserver applies an editable checklist's edits itself; an agent's session token cannot, so an agent can't tick items for you. Revisit if the user wants agents to update panels (the design's "agents only after you allow it" switch).

use std::{
    collections::BTreeSet,
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

use super::{
    ChecklistEdit, ExtensionError, ExtensionPermission, PanelSurface, PanelWriter, SidebarExtension,
};
use crate::server::{routed_json, AppState, RoutedResponse};

/// Every route of the scoped extension API starts with this.
pub(crate) const EXT_API_ROUTE_PREFIX: &str = "/api/ext/v1/";
/// A panel document is at most 64 KiB; its envelope fits well within this.
const BODY_MAX_BYTES: usize = 256 * 1024;
const KEEPALIVE: Duration = Duration::from_secs(15);
/// Projects and sessions change many times a second while agents work; an extension hears them
/// at most this often, with every id that changed meanwhile.
const CHANGE_COALESCE: Duration = Duration::from_secs(1);

/// Who is calling the extension API.
#[derive(Clone, Debug)]
enum Caller {
    /// An extension's own server, through its token.
    Extension(String),
    /// You: the desktop app, the web build, a paired phone, the CLI with the token file.
    Owner,
    /// An agent, through its session token: may read, and send its own extension messages about
    /// its own session (`<projectId>:<sessionId>`, from the token, never from the request).
    Agent { session: String },
}

fn answer(status: StatusCode, body: Value) -> RoutedResponse {
    routed_json(Some(EXT_API_ROUTE_PREFIX.to_string()), status, body)
}

fn refuse(status: StatusCode, error: &str, message: impl Into<String>) -> RoutedResponse {
    answer(
        status,
        json!({ "ok": false, "error": error, "message": message.into() }),
    )
}

fn refuse_error(error: ExtensionError) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "notFound" => StatusCode::NOT_FOUND,
        "conflict" => StatusCode::CONFLICT,
        "forbidden" => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refuse(status, error.code, error.message)
}

fn forbidden(message: impl Into<String>) -> ExtensionError {
    ExtensionError {
        code: "forbidden",
        message: message.into(),
    }
}

fn resolve_caller(state: &AppState, headers: &HeaderMap) -> Option<Caller> {
    if let Some(id) = state.extension_registry.extension_for_token(headers) {
        return Some(Caller::Extension(id));
    }
    // A web block's page is the extension's own code: it calls with the extension's scope and
    // permissions (`/api/extensionPageToken`).
    if let Some(id) = state.extension_registry.extension_for_page_token(headers) {
        return Some(Caller::Extension(id));
    }
    if crate::auth::is_authorized_headers(headers, &state.auth_token) {
        return Some(Caller::Owner);
    }
    let token = crate::auth::bearer_token(headers)?;
    if state.device_api_tokens.entry_for(token).is_some() {
        return Some(Caller::Owner);
    }
    match state.security.session_tokens.resolve(token)? {
        crate::security::Principal::Owner { .. } => Some(Caller::Owner),
        crate::security::Principal::Session {
            project_id,
            session_id,
            ..
        } => Some(Caller::Agent {
            session: format!("{project_id}:{session_id}"),
        }),
        _ => None,
    }
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    url::form_urlencoded::parse(query?.as_bytes())
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The extension a call is about: the token's own, or the one named by `?extension=`.
fn target_extension(caller: &Caller, named: Option<String>) -> Result<String, ExtensionError> {
    match (caller, named) {
        (Caller::Extension(own), Some(named)) if *own != named => Err(forbidden(
            "An extension's token reaches only its own panel.",
        )),
        (Caller::Extension(own), _) => Ok(own.clone()),
        (_, Some(named)) => Ok(named),
        (_, None) => Err(ExtensionError::bad_request(
            "Name the extension: ?extension=<id> (or \"extension\" in the body).",
        )),
    }
}

fn require_permission(
    extension: &SidebarExtension,
    permission: ExtensionPermission,
) -> Result<(), ExtensionError> {
    if extension.state.granted_permissions.contains(&permission) {
        Ok(())
    } else if extension.manifest.permissions.contains(&permission) {
        Err(forbidden(format!(
            "The user hasn't granted {} to {}. They can grant it in Settings > Extensions.",
            permission.as_str(),
            extension.manifest.name
        )))
    } else {
        Err(forbidden(format!(
            "{} doesn't declare the {} permission in its manifest.",
            extension.manifest.name,
            permission.as_str()
        )))
    }
}

pub(crate) async fn serve_ext_api(
    state: Arc<AppState>,
    method: Method,
    path: String,
    query: Option<String>,
    headers: HeaderMap,
    body: Body,
) -> RoutedResponse {
    let Some(caller) = resolve_caller(&state, &headers) else {
        return refuse(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Send the extension's GHOSTEX_API_TOKEN (or a Ghostex token) as `Authorization: Bearer …`.",
        );
    };
    let route = path.strip_prefix(EXT_API_ROUTE_PREFIX).unwrap_or_default();
    match (&method, route) {
        (&Method::GET, "events") => return events(state, caller, query).await,
        (&Method::GET, "panel" | "projects" | "sessions")
        | (&Method::PUT | &Method::PATCH, "panel")
        | (&Method::POST, "panelAction" | "agentMessage" | "sessionMessage") => {}
        (
            _,
            "panel" | "projects" | "sessions" | "events" | "panelAction" | "agentMessage"
            | "sessionMessage",
        ) => {
            return refuse(
                StatusCode::METHOD_NOT_ALLOWED,
                "methodNotAllowed",
                format!("{} {path} is not an extension API route.", method.as_str()),
            )
        }
        _ => {
            return refuse(
                StatusCode::NOT_FOUND,
                "notFound",
                format!("{path} is not an extension API route."),
            )
        }
    }
    let body = if matches!(method, Method::GET) {
        Value::Null
    } else {
        match axum::body::to_bytes(body, BODY_MAX_BYTES).await {
            Ok(bytes) if bytes.is_empty() => json!({}),
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) if value.is_object() => value,
                _ => {
                    return refuse(
                        StatusCode::BAD_REQUEST,
                        "badRequest",
                        "The body must be a JSON object.",
                    )
                }
            },
            Err(_) => {
                return refuse(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "payloadTooLarge",
                    format!("The body is over {BODY_MAX_BYTES} bytes."),
                )
            }
        }
    };
    let route = route.to_string();
    let blocking_state = state.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        handle(
            &blocking_state,
            &caller,
            &method,
            &route,
            query.as_deref(),
            &body,
        )
    })
    .await
    .unwrap_or_else(|error| {
        Err(ExtensionError::internal(format!(
            "Extension API task failed: {error}"
        )))
    });
    match outcome {
        Ok((result, panels_changed)) => {
            if panels_changed {
                publish_extension_panels(&state);
            }
            answer(StatusCode::OK, result)
        }
        Err(error) => refuse_error(error),
    }
}

/// The answer, and whether a panel changed (so clients hear of it).
fn handle(
    state: &AppState,
    caller: &Caller,
    method: &Method,
    route: &str,
    query: Option<&str>,
    body: &Value,
) -> Result<(Value, bool), ExtensionError> {
    let registry = &state.extension_registry;
    let body_text = |key: &str| {
        body.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let named = query_param(query, "extension").or_else(|| body_text("extension"));
    let id = target_extension(caller, named)?;
    let extension = registry.sidebar_extension(&id)?;
    let surface = || {
        PanelSurface::parse(
            query_param(query, "surface")
                .or_else(|| body_text("surface"))
                .as_deref(),
            query_param(query, "session")
                .or_else(|| body_text("session"))
                .as_deref(),
        )
    };
    let base_revision = body.get("baseRevision").and_then(Value::as_u64);
    let panels = registry.panels();
    match (method, route) {
        (&Method::GET, "panel") => {
            let surface = surface()?;
            let record = panels.read(&extension.manifest, &extension.payload_dir, &surface)?;
            Ok((with_ok(record.to_json(&id, &surface)), false))
        }
        (&Method::PUT, "panel") | (&Method::PATCH, "panel") => {
            if !matches!(caller, Caller::Extension(_)) {
                return Err(forbidden(format!(
                    "Only {id}'s own process changes its panel (with its GHOSTEX_API_TOKEN). Agents and scripts can read it with `ghostex extensions panel get {id}`."
                )));
            }
            require_enabled(&extension)?;
            let surface = surface()?;
            let record = if *method == Method::PUT {
                let document = body.get("document").ok_or_else(|| {
                    ExtensionError::bad_request("PUT /panel needs {\"document\": {…}}.")
                })?;
                panels.write(
                    &extension.manifest,
                    &extension.payload_dir,
                    &surface,
                    document,
                    base_revision,
                    &extension.state.granted_permissions,
                    PanelWriter::Extension,
                )?
            } else {
                panels.patch(
                    &extension.manifest,
                    &extension.payload_dir,
                    &surface,
                    &body.as_object().cloned().unwrap_or_default(),
                    base_revision,
                    &extension.state.granted_permissions,
                )?
            };
            Ok((with_ok(record.to_json(&id, &surface)), true))
        }
        (&Method::GET, "projects") => {
            only_the_extension(caller, &id)?;
            require_enabled(&extension)?;
            require_permission(&extension, ExtensionPermission::ProjectsRead)?;
            let snapshot = presentation_snapshot(state)?;
            Ok((
                json!({ "ok": true, "projects": projects(&snapshot) }),
                false,
            ))
        }
        (&Method::GET, "sessions") => {
            only_the_extension(caller, &id)?;
            require_enabled(&extension)?;
            require_permission(&extension, ExtensionPermission::SessionsRead)?;
            let snapshot = presentation_snapshot(state)?;
            let project = query_param(query, "project");
            let forks = forked_from(state);
            Ok((
                json!({ "ok": true, "sessions": sessions(&snapshot, project.as_deref(), &forks) }),
                false,
            ))
        }
        (&Method::POST, "agentMessage") => {
            let Caller::Agent { session } = caller else {
                return Err(forbidden(
                    "Only an agent sends this, about its own session: run `ghostex extensions message` inside the agent's session.",
                ));
            };
            require_enabled(&extension)?;
            require_permission(&extension, ExtensionPermission::AgentsMessage)?;
            let name = body_text("name")
                .filter(|name| {
                    name.len() <= 64
                        && name.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                        })
                })
                .ok_or_else(|| {
                    ExtensionError::bad_request(
                        "Name the message: 1-64 letters, digits, '.', '_' or '-'.",
                    )
                })?;
            let data = body.get("data").cloned().unwrap_or(Value::Null);
            if data.to_string().len() > AGENT_MESSAGE_DATA_MAX_BYTES {
                return Err(ExtensionError::bad_request(format!(
                    "The message's data is over {AGENT_MESSAGE_DATA_MAX_BYTES} bytes."
                )));
            }
            let event =
                json!({ "type": "agentMessage", "session": session, "name": name, "data": data });
            let answer = registry.send_agent_message(&id, &event)?;
            Ok((json!({ "ok": true, "answer": answer }), false))
        }
        (&Method::POST, "sessionMessage") => {
            if !matches!(caller, Caller::Extension(_)) {
                return Err(forbidden("Only the extension's own process sends this."));
            }
            require_enabled(&extension)?;
            require_permission(&extension, ExtensionPermission::SessionsMessage)?;
            let session = body_text("session").ok_or_else(|| {
                ExtensionError::bad_request("Name the session: <projectId>:<sessionId>.")
            })?;
            let text = body_text("text")
                .filter(|text| text.chars().count() <= SESSION_MESSAGE_MAX_CHARS)
                .ok_or_else(|| {
                    ExtensionError::bad_request(format!(
                        "The message is 1 to {SESSION_MESSAGE_MAX_CHARS} characters."
                    ))
                })?;
            let (project_id, session_id) = session.split_once(':').ok_or_else(|| {
                ExtensionError::bad_request("session must be <projectId>:<sessionId>.")
            })?;
            if !registry.take_session_panel_tap(&id, &session) {
                return Err(forbidden(
                    "A thread gets a message only right after you tap a button in its panel.",
                ));
            }
            send_session_message(state, project_id, session_id, &text)?;
            Ok((json!({ "ok": true, "sent": true }), false))
        }
        (&Method::POST, "panelAction") => {
            if !matches!(caller, Caller::Owner) {
                return Err(forbidden(
                    "Panel taps come from your Ghostex apps and phones; an extension or an agent can't send them.",
                ));
            }
            require_enabled(&extension)?;
            panel_action(state, &extension, &surface()?, body, base_revision)
        }
        _ => Err(ExtensionError::not_found("Unknown extension API route.")),
    }
}

fn with_ok(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("ok".into(), json!(true));
    }
    value
}

fn only_the_extension(caller: &Caller, id: &str) -> Result<(), ExtensionError> {
    if matches!(caller, Caller::Extension(_)) {
        Ok(())
    } else {
        Err(forbidden(format!(
            "/projects and /sessions answer {id}'s own token; use `ghostex sessions` instead."
        )))
    }
}

fn require_enabled(extension: &SidebarExtension) -> Result<(), ExtensionError> {
    if extension.state.enabled {
        Ok(())
    } else {
        Err(ExtensionError {
            code: "conflict",
            message: format!("{} is turned off.", extension.manifest.name),
        })
    }
}

fn panel_action(
    state: &AppState,
    extension: &SidebarExtension,
    surface: &PanelSurface,
    body: &Value,
    base_revision: Option<u64>,
) -> Result<(Value, bool), ExtensionError> {
    let id = &extension.manifest.name;
    let block_id = body
        .get("blockId")
        .and_then(Value::as_str)
        .ok_or_else(|| ExtensionError::bad_request("A panel action needs blockId."))?;
    let panels = state.extension_registry.panels();
    if let Some(edit) = body.get("edit") {
        let edit = ChecklistEdit::parse(edit)?;
        let record = panels.edit_checklist(
            &extension.manifest,
            &extension.payload_dir,
            surface,
            block_id,
            &edit,
            base_revision,
        )?;
        let mut event = Map::new();
        event.insert("type".into(), json!("checklistEdited"));
        surface.describe(&mut event);
        event.insert("blockId".into(), json!(block_id));
        event.insert("edit".into(), edit.to_json());
        event.insert("revision".into(), json!(record.revision));
        panels.emit(id, Value::Object(event));
        return Ok((with_ok(record.to_json(id, surface)), true));
    }
    let name = body
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| ExtensionError::bad_request("A panel action needs edit or action."))?;
    // Only an action the panel offers on that block can be sent back.
    let record = panels.read(&extension.manifest, &extension.payload_dir, surface)?;
    let block = record
        .document
        .blocks
        .iter()
        .find(|block| block.id == block_id)
        .ok_or_else(|| {
            ExtensionError::not_found(format!("The panel has no block {block_id:?}."))
        })?;
    let block_json = serde_json::to_value(block).unwrap_or(Value::Null);
    if !offers_extension_action(&block_json, name) {
        return Err(ExtensionError::bad_request(format!(
            "Block {block_id:?} offers no extension action {name:?}."
        )));
    }
    if let PanelSurface::SessionSidePanel {
        project_id,
        session_id,
    } = surface
    {
        state
            .extension_registry
            .note_session_panel_tap(id, &format!("{project_id}:{session_id}"));
    }
    let mut event = Map::new();
    event.insert("type".into(), json!("panelAction"));
    surface.describe(&mut event);
    event.insert("blockId".into(), json!(block_id));
    if let Some(item_id) = body.get("itemId").and_then(Value::as_str) {
        event.insert("itemId".into(), json!(item_id));
    }
    event.insert("name".into(), json!(name));
    if let Some(data) = body.get("data").filter(|data| !data.is_null()) {
        event.insert("data".into(), data.clone());
    }
    let event = Value::Object(event);
    panels.emit(id, event.clone());
    state.extension_registry.post_panel_action(id, event);
    Ok((json!({ "ok": true, "delivered": true }), false))
}

/// The RPC door for clients' taps (the desktop, the web build and phones call gxserver through
/// `/api/...` RPCs): the same as `POST /api/ext/v1/panelAction`, for your own devices only.
pub(crate) const PANEL_ACTION_RPC_ENDPOINT: &str = "/api/extensionPanelAction";

pub(crate) async fn handle_panel_action_rpc(
    state: Arc<AppState>,
    caller: &crate::security::Principal,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let error_response = |error: ExtensionError| {
        let status = match error.code {
            "badRequest" => StatusCode::BAD_REQUEST,
            "notFound" => StatusCode::NOT_FOUND,
            "conflict" => StatusCode::CONFLICT,
            "forbidden" => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        routed_json(
            Some(endpoint_path.clone()),
            status,
            crate::protocol::rpc_error(error.code, error.message, Some(request_id.clone())),
        )
    };
    if !caller.is_owner_device() {
        return error_response(forbidden(
            "Panel taps come from your Ghostex apps and phones; an extension or an agent can't send them.",
        ));
    }
    let params = match crate::domain::read_domain_rpc_params(&body) {
        Ok(params) => Value::Object(params),
        Err(error) => return error_response(ExtensionError::bad_request(error.message)),
    };
    let blocking_state = state.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        handle(
            &blocking_state,
            &Caller::Owner,
            &Method::POST,
            "panelAction",
            None,
            &params,
        )
    })
    .await
    .unwrap_or_else(|error| {
        Err(ExtensionError::internal(format!(
            "Extension API task failed: {error}"
        )))
    });
    match outcome {
        Ok((result, panels_changed)) => {
            if panels_changed {
                publish_extension_panels(&state);
            }
            routed_json(
                Some(endpoint_path.clone()),
                StatusCode::OK,
                crate::protocol::rpc_success(request_id.clone(), result),
            )
        }
        Err(error) => error_response(error),
    }
}

/// The RPC that gives the desktop the token a sidebar extension's web pages call the extension
/// API with. Only your own apps get it; the page itself never sees another extension's.
pub(crate) const PAGE_TOKEN_RPC_ENDPOINT: &str = "/api/extensionPageToken";

/// CDXC:Extensions 2026-11-14 WHY:
/// Phase 4's brief: a `web` block's page talks through the extension bridge with "the same /api/ext/v1 calls with the same permissions" as the extension's server. The page is served from gxserver's own origin, so the bridge (cef/sidebar_bridge_manifest.rs) calls those routes itself with this token, which carries the extension's scope (`Caller::Extension`) and nothing else: it opens no other `/api` door, unlike the server's token.
pub(crate) async fn handle_page_token_rpc(
    state: Arc<AppState>,
    caller: &crate::security::Principal,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let refuse_rpc = |status: StatusCode, error: ExtensionError| {
        routed_json(
            Some(endpoint_path.clone()),
            status,
            crate::protocol::rpc_error(error.code, error.message, Some(request_id.clone())),
        )
    };
    if !caller.is_owner_device() {
        return refuse_rpc(
            StatusCode::FORBIDDEN,
            forbidden("Only your Ghostex apps open an extension's web pages."),
        );
    }
    let id = crate::domain::read_domain_rpc_params(&body)
        .ok()
        .and_then(|params| {
            params
                .get("extension")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let Some(id) = id else {
        return refuse_rpc(
            StatusCode::BAD_REQUEST,
            ExtensionError::bad_request("Name the extension."),
        );
    };
    let registry = state.extension_registry.clone();
    let outcome = tokio::task::spawn_blocking(move || registry.sidebar_page_token(&id))
        .await
        .unwrap_or_else(|error| {
            Err(ExtensionError::internal(format!(
                "Extension API task failed: {error}"
            )))
        });
    match outcome {
        Ok(token) => routed_json(
            Some(endpoint_path.clone()),
            StatusCode::OK,
            crate::protocol::rpc_success(
                request_id.clone(),
                json!({ "token": token, "api": EXT_API_ROUTE_PREFIX }),
            ),
        ),
        Err(error) => {
            let status = match error.code {
                "notFound" => StatusCode::NOT_FOUND,
                "conflict" => StatusCode::CONFLICT,
                "badRequest" => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            refuse_rpc(status, error)
        }
    }
}

/// Whether `{"extension": name}` (or `{"extension": {"name": name, …}}`) appears in the block.
fn offers_extension_action(value: &Value, name: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.get("extension").is_some_and(|target| {
                target.as_str() == Some(name)
                    || target.get("name").and_then(Value::as_str) == Some(name)
            }) || object
                .values()
                .any(|child| offers_extension_action(child, name))
        }
        Value::Array(items) => items
            .iter()
            .any(|child| offers_extension_action(child, name)),
        _ => false,
    }
}

fn presentation_snapshot(state: &AppState) -> Result<Value, ExtensionError> {
    let internal = |error: String| ExtensionError::internal(error);
    let db = crate::storage::open_gxserver_database(&state.paths)
        .map_err(|error| internal(format!("Could not open the Ghostex database: {error}")))?;
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let sessions = repository
        .list_presentation_sessions()
        .map_err(|error| internal(format!("Could not list sessions: {error}")))?;
    crate::server::presentation_delta::read_presentation_snapshot_in_sequence(
        state,
        &db,
        &state.metadata.server_id,
        sessions,
    )
    .map_err(|error| internal(format!("Could not read the sidebar: {}", error.message)))
}

fn text(value: &Value, key: &str) -> Value {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|text| json!(text))
        .unwrap_or(Value::Null)
}

/// The stable, small shape extensions get, not the whole presentation row.
fn projects(snapshot: &Value) -> Vec<Value> {
    snapshot
        .get("projects")
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .map(|project| {
                    json!({
                        "projectId": text(project, "projectId"),
                        "title": text(project, "title"),
                        "path": text(project, "path"),
                        "isPinned": project.get("isPinned").and_then(Value::as_bool).unwrap_or(false),
                        "isFavorite": project.get("isFavorite").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A message an agent sends its extension carries at most this much data.
const AGENT_MESSAGE_DATA_MAX_BYTES: usize = 32 * 1024;
/// The longest message an extension sends a thread after a tap.
const SESSION_MESSAGE_MAX_CHARS: usize = 500;

/// Sends `text` to the thread as if typed in its chat box (the path the memory guard's Resume
/// takes), from the blocking task the extension API runs on.
fn send_session_message(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<(), ExtensionError> {
    let body = json!({ "params": {
        "projectId": project_id,
        "sessionId": session_id,
        "text": text,
        "sendRequestId": uuid::Uuid::new_v4().to_string(),
    }});
    let routed = tokio::runtime::Handle::current().block_on(
        crate::session_chat_send::handle_send_session_chat_message_http(
            state,
            "/api/sendSessionChatMessage".to_string(),
            uuid::Uuid::new_v4().to_string(),
            &body,
        ),
    );
    if routed.response.status().is_success() {
        Ok(())
    } else {
        Err(ExtensionError {
            code: "conflict",
            message: "The thread couldn't take the message right now.".to_string(),
        })
    }
}

/// `<projectId>:<sessionId>` of each forked session, by its own ref: the session it was forked from.
fn forked_from(state: &AppState) -> std::collections::HashMap<String, String> {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return Default::default();
    };
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    repository
        .map_session_fork_rows(|row| {
            let project_id = row.get("projectId")?.as_str()?.to_string();
            let session_id = row.get("sessionId")?.as_str()?;
            let parent = ["runtimeSettings", "launchSettings"]
                .iter()
                .find_map(|key| {
                    row.get(*key)?
                        .get("forkedFromSessionId")?
                        .as_str()
                        .map(str::to_string)
                })?;
            Some((
                format!("{project_id}:{session_id}"),
                format!("{project_id}:{parent}"),
            ))
        })
        .map(|rows| rows.into_iter().flatten().collect())
        .unwrap_or_default()
}

fn sessions(
    snapshot: &Value,
    project: Option<&str>,
    forks: &std::collections::HashMap<String, String>,
) -> Vec<Value> {
    snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .map(|sessions| {
            sessions
                .iter()
                .filter(|session| {
                    project.is_none_or(|project| {
                        session.get("projectId").and_then(Value::as_str) == Some(project)
                    })
                })
                .map(|session| {
                    let project_id = session.get("projectId").and_then(Value::as_str).unwrap_or_default();
                    let session_id = session.get("sessionId").and_then(Value::as_str).unwrap_or_default();
                    let title = ["primaryTitle", "displayTitle", "title"]
                        .iter()
                        .find_map(|key| {
                            session
                                .get(*key)
                                .and_then(Value::as_str)
                                .filter(|title| !title.trim().is_empty())
                        })
                        .unwrap_or_default();
                    let reference = format!("{project_id}:{session_id}");
                    json!({
                        "forkedFrom": forks.get(&reference),
                        "session": reference,
                        "projectId": project_id,
                        "sessionId": session_id,
                        "title": title,
                        "agent": text(session, "agentId"),
                        "activity": text(session, "activity"),
                        "lifecycleState": text(session, "lifecycleState"),
                        "awaitingAnswer": session.get("awaitingAnswer").and_then(Value::as_bool).unwrap_or(false),
                        "lastActiveAt": text(session, "lastActiveAt"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Tells every client the whole `extensionPanels` map after a panel or a sidebar extension
/// changed. Inside the presentation sequencer, so it is ordered with the snapshots clients get.
pub(crate) fn publish_extension_panels(state: &AppState) {
    let Ok(_sequence) = crate::server::presentation_delta::lock_presentation_event_sequence(state)
    else {
        return;
    };
    state.event_hub.broadcast(json!({
        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": ghostex_gx_protocol::extension_panel::EXTENSION_PANELS_CHANGED_EVENT_TYPE,
        "extensionPanels": state.extension_registry.extension_panels(),
    }));
}

/// Adds `extensionPanels` to a presentation snapshot while any sidebar extension is on (absent
/// otherwise, which every client reads as "none").
pub(crate) fn insert_extension_panels_presentation_payload(snapshot: &mut Value, state: &AppState) {
    let panels = state.extension_registry.extension_panels();
    if panels.as_object().is_some_and(|panels| !panels.is_empty()) {
        if let Some(snapshot) = snapshot.as_object_mut() {
            snapshot.insert("extensionPanels".to_string(), panels);
        }
    }
}

/// `GET /events`: server-sent events until the caller hangs up.
async fn events(state: Arc<AppState>, caller: Caller, query: Option<String>) -> RoutedResponse {
    let id = match target_extension(&caller, query_param(query.as_deref(), "extension")) {
        Ok(id) => id,
        Err(error) => return refuse_error(error),
    };
    let registry = state.extension_registry.clone();
    let lookup_id = id.clone();
    let extension =
        match tokio::task::spawn_blocking(move || registry.sidebar_extension(&lookup_id)).await {
            Ok(Ok(extension)) => extension,
            Ok(Err(error)) => return refuse_error(error),
            Err(error) => {
                return refuse_error(ExtensionError::internal(format!(
                    "Extension API task failed: {error}"
                )))
            }
        };
    let is_extension = matches!(caller, Caller::Extension(_));
    let stream = EventStream {
        state: state.clone(),
        id: id.clone(),
        is_extension,
        grants: extension.state.granted_permissions.clone(),
        grants_read_at: Instant::now(),
        panels: state.extension_registry.panels().subscribe(),
        hub: is_extension.then(|| state.event_hub.subscribe()),
        projects: BTreeSet::new(),
        sessions: BTreeSet::new(),
        pending_since: None,
        greeted: false,
    };
    let body = Body::from_stream(futures_util::stream::unfold(
        stream,
        |mut stream| async move {
            let chunk = stream.next_chunk().await?;
            Some((Ok::<_, Infallible>(chunk), stream))
        },
    ));
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    RoutedResponse {
        endpoint_path: Some(EXT_API_ROUTE_PREFIX.to_string()),
        response,
    }
}

struct EventStream {
    state: Arc<AppState>,
    id: String,
    is_extension: bool,
    grants: Vec<ExtensionPermission>,
    grants_read_at: Instant,
    panels: broadcast::Receiver<super::PanelEvent>,
    hub: Option<broadcast::Receiver<crate::events::EventPayload>>,
    projects: BTreeSet<String>,
    sessions: BTreeSet<String>,
    pending_since: Option<Instant>,
    greeted: bool,
}

fn sse(event: &Value) -> String {
    let name = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    format!("event: {name}\ndata: {event}\n\n")
}

impl EventStream {
    /// The next text to send; `None` ends the stream.
    async fn next_chunk(&mut self) -> Option<String> {
        if !self.greeted {
            self.greeted = true;
            return Some(sse(&json!({ "type": "ready", "extension": self.id })));
        }
        loop {
            let flush_in = self
                .pending_since
                .map(|since| CHANGE_COALESCE.saturating_sub(since.elapsed()))
                .unwrap_or(KEEPALIVE);
            let hub = async {
                match self.hub.as_mut() {
                    Some(hub) => hub.recv().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                panel = self.panels.recv() => match panel {
                    Ok(event) if event.extension_id == self.id => return Some(sse(&event.payload)),
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        return Some(sse(&json!({ "type": "lagged", "missed": missed })));
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
                frame = hub => match frame {
                    Ok(frame) => self.note_change(&frame),
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Lost track of what changed: say everything may have.
                        self.projects.insert("*".into());
                        self.sessions.insert("*".into());
                        self.pending_since.get_or_insert_with(Instant::now);
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
                () = tokio::time::sleep(flush_in) => {
                    if self.pending_since.is_some() {
                        if let Some(chunk) = self.flush().await {
                            return Some(chunk);
                        }
                    } else {
                        return Some(": keepalive\n\n".to_string());
                    }
                }
            }
        }
    }

    fn note_change(&mut self, frame: &Value) {
        if !self.is_extension {
            return;
        }
        if frame.get("type").and_then(Value::as_str) != Some("presentationDelta") {
            return;
        }
        let Some(delta) = frame.get("delta") else {
            return;
        };
        let delta_type = delta
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let read = |value: &Value, key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match delta_type {
            "projectAdded" | "projectUpdated" => {
                if let Some(project) = delta.get("project") {
                    self.projects.insert(read(project, "projectId"));
                }
            }
            "projectRemoved" => {
                self.projects.insert(read(delta, "projectId"));
            }
            "sessionPresentationChanged" => {
                if let Some(session) = delta.get("session") {
                    self.sessions.insert(format!(
                        "{}:{}",
                        read(session, "projectId"),
                        read(session, "sessionId")
                    ));
                }
            }
            "sessionRemoved" => {
                self.sessions.insert(format!(
                    "{}:{}",
                    read(delta, "projectId"),
                    read(delta, "sessionId")
                ));
            }
            _ => return,
        }
        self.pending_since.get_or_insert_with(Instant::now);
    }

    /// The coalesced `projectsChanged` / `sessionsChanged`, for the read permissions the user
    /// granted (read again at most every few seconds, so a revoked grant stops them).
    async fn flush(&mut self) -> Option<String> {
        self.pending_since = None;
        let projects = std::mem::take(&mut self.projects);
        let sessions = std::mem::take(&mut self.sessions);
        if self.grants_read_at.elapsed() > Duration::from_secs(5) {
            let registry = self.state.extension_registry.clone();
            let id = self.id.clone();
            if let Ok(Ok(extension)) =
                tokio::task::spawn_blocking(move || registry.sidebar_extension(&id)).await
            {
                self.grants = extension.state.granted_permissions;
            }
            self.grants_read_at = Instant::now();
        }
        let mut chunk = String::new();
        if !projects.is_empty() && self.grants.contains(&ExtensionPermission::ProjectsRead) {
            chunk.push_str(&sse(&json!({
                "type": "projectsChanged",
                "projectIds": projects.into_iter().filter(|id| id != "*").collect::<Vec<_>>(),
            })));
        }
        if !sessions.is_empty() && self.grants.contains(&ExtensionPermission::SessionsRead) {
            chunk.push_str(&sse(&json!({
                "type": "sessionsChanged",
                "sessions": sessions.into_iter().filter(|id| id != "*").collect::<Vec<_>>(),
            })));
        }
        (!chunk.is_empty()).then_some(chunk)
    }
}
