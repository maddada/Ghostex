//! Collab's calls on gxserver, all thin:
//!
//! - `/api/collab` (the owner's: Settings › Collab, the Share popover, the phone): forwarded to
//!   the component (server/collab/src/owner.rs lists the ops). Without the component, `read`
//!   answers `{installed: false}` and everything else `collabNotInstalled`. When the component
//!   answers that a change needs the owner's approval, the gate runs here
//!   (`require_owner_presence`, GitHub #495) with the call's exact params and the component's
//!   words, and once the owner approves gxserver repeats the call with the grant itself, as the
//!   caller that asked; a caller repeating it with `approvalId` gets that answer back.
//! - `/api/collabTyping` (the owner typing in a shared chat): passed to the component.
//! - `/api/collabHost`: the component's own calls, with the host token it got at start; nobody
//!   else may call it.
//! - `/api/extensionComponents`: install (with the owner's signed approval), update and
//!   uninstall components that open a way in (Collab).
//! - A guest's replayed reads of their one chat ([`route_guest`]).

use std::sync::Arc;
use std::time::Duration;

use axum::body::to_bytes;
use axum::http::{HeaderMap, StatusCode};
use ghostex_gx_protocol::approval::{SensitiveAction, APPROVAL_ID_PARAM};
use ghostex_gx_protocol::collab_component::{
    GrantInfo, HostCall, OwnerCall, OwnerReply, PresenceNeed, ShareScope, SidecarEvent,
    COLLAB_COMPONENT, COLLAB_HOST_ENDPOINT, COLLAB_VIEWER_HEADER,
};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::domain::read_domain_rpc_params;
use crate::domain::DomainRepository;
use crate::protocol::{rpc_error, rpc_success};
use crate::security::{ActionRequest, PresenceGrant, PresenceOutcome, Principal};
use crate::server::{routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

use super::{install, registry, sidecar};

pub(crate) const COLLAB_ENDPOINT: &str = "/api/collab";
pub(crate) const COLLAB_TYPING_ENDPOINT: &str = "/api/collabTyping";
/// `{projectId, sessionId, pageId}` or `{projectId, sessionId, path}`: a Ghostex page the shared
/// chat published, or an HTML file it links to, as `{html, title, pageId, answerable}` for the
/// guest's page viewer (visual_pages/guest.rs). Guests only.
pub(crate) const GUEST_PAGE_ENDPOINT: &str = "/api/collabGuestPage";
/// A guest's answers to a page: answered by the component, never here.
pub(crate) const GUEST_PAGE_ANSWERS_ENDPOINT: &str = "/api/collabGuestPageAnswers";
pub(crate) const EXTENSION_COMPONENTS_ENDPOINT: &str = "/api/extensionComponents";

pub(crate) const COLLAB_ENDPOINTS: &[&str] = &[
    COLLAB_ENDPOINT,
    COLLAB_TYPING_ENDPOINT,
    GUEST_PAGE_ENDPOINT,
    GUEST_PAGE_ANSWERS_ENDPOINT,
    EXTENSION_COMPONENTS_ENDPOINT,
];

fn ok(endpoint_path: String, request_id: String, result: Value) -> RoutedResponse {
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

fn fail(
    endpoint_path: String,
    request_id: String,
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> RoutedResponse {
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(code, message.into(), Some(request_id)),
    )
}

pub(crate) fn guest_refusal(endpoint_path: String, request_id: String) -> RoutedResponse {
    fail(
        endpoint_path,
        request_id,
        StatusCode::FORBIDDEN,
        "forbidden",
        "A guest can only follow and write in the chat that was shared.",
    )
}

fn not_installed(endpoint_path: String, request_id: String) -> RoutedResponse {
    fail(
        endpoint_path,
        request_id,
        StatusCode::CONFLICT,
        "collabNotInstalled",
        "Collab isn't installed. Install it in Settings > Extensions.",
    )
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) async fn handle_collab_http(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = read_domain_rpc_params(body).unwrap_or_default();
    match endpoint_path.as_str() {
        EXTENSION_COMPONENTS_ENDPOINT => {
            extension_components(state, caller, endpoint_path, request_id, &params).await
        }
        // Only a guest's page reads pages through the tunnel (`route_guest`), and the component
        // answers their answers; the owner opens pages at their own address.
        GUEST_PAGE_ENDPOINT | GUEST_PAGE_ANSWERS_ENDPOINT => fail(
            endpoint_path,
            request_id,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "Only a Ghostex Collab guest's page calls this.",
        ),
        COLLAB_TYPING_ENDPOINT => {
            let (Some(project_id), Some(session_id)) =
                (text(&params, "projectId"), text(&params, "sessionId"))
            else {
                return fail(
                    endpoint_path,
                    request_id,
                    StatusCode::BAD_REQUEST,
                    "badRequest",
                    "projectId and sessionId are required.",
                );
            };
            if registry::for_chat(&project_id, &session_id).is_some() {
                let typing = params
                    .get("typing")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                sidecar::send_event_later(SidecarEvent::OwnerTyping {
                    project_id,
                    session_id,
                    typing,
                });
            }
            ok(endpoint_path, request_id, json!({}))
        }
        _ => owner_call(state, caller, endpoint_path, request_id, params).await,
    }
}

/// What a client reads while Collab isn't running.
fn not_running_read() -> Value {
    let item = install::item();
    json!({
        "installed": item["installed"],
        "running": false,
        "component": item,
        "shareHidden": super::share_hidden(),
    })
}

async fn owner_call(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    params: Map<String, Value>,
) -> RoutedResponse {
    let op = text(&params, "op").unwrap_or_else(|| "read".to_string());
    // "Don't show Share" (and showing it again): gxserver's own, with or without the component.
    if op == "hideShare" {
        let hidden = params
            .get("hidden")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        return match super::set_share_hidden(&state.paths, hidden) {
            Ok(()) => {
                super::republish_followed(state);
                ok(endpoint_path, request_id, json!({ "shareHidden": hidden }))
            }
            Err(error) => fail(
                endpoint_path,
                request_id,
                StatusCode::INTERNAL_SERVER_ERROR,
                "internalError",
                format!("Couldn't save that: {error}"),
            ),
        };
    }
    if !sidecar::is_running() {
        return if op == "read" {
            ok(endpoint_path, request_id, not_running_read())
        } else {
            not_installed(endpoint_path, request_id)
        };
    }
    // A repeated call for an approval gxserver already acted on gets that answer back.
    if let Some(approval_id) = text(&params, APPROVAL_ID_PARAM) {
        if let Some(answer) = approvals::claim_retry(&approval_id, caller).await {
            return owner_answer(state, caller, endpoint_path, request_id, &params, answer);
        }
    }
    let answer = forward_owner(state, caller, &params).await;
    owner_answer(state, caller, endpoint_path, request_id, &params, answer)
}

/// Why an owner call did not go through.
#[derive(Clone, Debug)]
pub(crate) enum OwnerError {
    Refused(String, String),
    Presence(PresenceOutcome),
}

type OwnerResult = Result<Value, OwnerError>;

/// The gate for what the component asked: the grant, or the outcome to answer with. The card's
/// words are the component's (built from the chat's title and the stored share, never from the
/// caller); `params` is exactly what the call acts on, `approvalId` included when repeated.
fn gate(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
    need: &PresenceNeed,
) -> Result<PresenceGrant, OwnerError> {
    let mut bound = params.clone();
    bound.remove(APPROVAL_ID_PARAM);
    let request = ActionRequest::new(need.action.clone(), need.summary.clone())
        .target(need.target.clone())
        .detail(need.detail.clone())
        .params(Value::Object(bound));
    let body = json!({ "params": params });
    match crate::server::presence_outcome(state, caller, request, &body) {
        PresenceOutcome::Granted(grant) => Ok(grant),
        other => Err(OwnerError::Presence(other)),
    }
}

/// After an approved change ran: the security log's entry and its notification.
pub(crate) fn performed(state: &AppState, caller: &Principal, grant: &PresenceGrant, ok: bool) {
    // Your own share's link again or its mode (Q2103) asked nobody: logged as allowed.
    crate::audit::record(
        crate::audit::Action::from_approval(&grant.approval),
        crate::audit::Actor::from(caller),
        match (ok, grant.by_policy) {
            (false, _) => {
                crate::audit::Outcome::Failed("The change did not go through.".to_string())
            }
            (true, true) => crate::audit::Outcome::Allowed,
            (true, false) => crate::audit::Outcome::Approved,
        },
        (!grant.by_policy).then(|| crate::audit::ApprovalRef::from(&grant.approval)),
    );
    state.security.record_performed(
        grant.approval.action.clone(),
        grant.approval.target.clone(),
        caller,
        ok,
    );
}

async fn call_component(call: OwnerCall) -> Result<OwnerReply, OwnerError> {
    tokio::task::spawn_blocking(move || sidecar::call_owner(&call))
        .await
        .unwrap_or_else(|_| Err("The call to Collab stopped.".to_string()))
        .map_err(|message| OwnerError::Refused("collabUnavailable".to_string(), message))
}

/// One owner call through the component, with the gate between its two halves.
pub(crate) async fn forward_owner(
    state: &Arc<AppState>,
    caller: &Principal,
    params: &Map<String, Value>,
) -> OwnerResult {
    let mut forwarded = params.clone();
    forwarded.remove(APPROVAL_ID_PARAM);
    let mut call = OwnerCall {
        params: forwarded,
        grant: None,
    };
    let mut grant: Option<PresenceGrant> = None;
    // At most one approval per call: a component asking twice is refused.
    for _ in 0..2 {
        let reply = call_component(call.clone()).await?;
        if let Some(granted) = &grant {
            for done in &reply.performed {
                if done.approval_id == granted.approval.id {
                    performed(state, caller, granted, done.ok);
                }
            }
        }
        if let Some(need) = reply.needs_presence {
            if grant.is_some() {
                break;
            }
            let granted = gate(state, caller, params, &need)?;
            call.grant = Some(GrantInfo {
                approval_id: granted.approval.id.clone(),
                action: need.action,
                target: need.target,
            });
            grant = Some(granted);
            continue;
        }
        if let Some(refused) = reply.refused {
            return Err(OwnerError::Refused(refused.code, refused.message));
        }
        let mut result = reply.result.unwrap_or(Value::Null);
        if let Some(object) = result.as_object_mut() {
            if object.contains_key("config") {
                // This caller's own Collab approval requests of the last minutes and where they
                // stand, so a client can follow one without its id.
                object.insert(
                    "approvals".to_string(),
                    Value::Array(approvals::asked_by(state, caller)),
                );
                object.insert("shareHidden".to_string(), json!(super::share_hidden()));
                object.insert("component".to_string(), install::item());
            }
        }
        return Ok(result);
    }
    Err(OwnerError::Refused(
        "internalError".to_string(),
        "Collab asked for a second approval in one call.".to_string(),
    ))
}

fn owner_answer(
    _state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
    answer: OwnerResult,
) -> RoutedResponse {
    match answer {
        Ok(result) => ok(endpoint_path, request_id, result),
        Err(OwnerError::Presence(outcome)) => {
            if let PresenceOutcome::Pending(request) = &outcome {
                approvals::await_approval(&request.id, caller, params);
            }
            match crate::server::presence_outcome_response(outcome, &endpoint_path, &request_id) {
                Err(response) => response,
                Ok(_) => fail(
                    endpoint_path,
                    request_id,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internalError",
                    "The approval was not used.",
                ),
            }
        }
        Err(OwnerError::Refused(code, message)) => fail(
            endpoint_path,
            request_id,
            match code.as_str() {
                "badRequest" => StatusCode::BAD_REQUEST,
                "notFound" => StatusCode::NOT_FOUND,
                "guestPageUnverified" | "collabUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
                "sendFailed" => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            &code,
            message,
        ),
    }
}

/// Approvals Collab asked for, finished by gxserver once the owner signs them.
///
/// CDXC:Collab 2026-11-08 WHY:
/// The gate's contract has the caller repeat its call with `approvalId`. The chat's held-message card (desktop, web and phone, one chat core), the phone's sheet and an agent's CLI call would each need that wait-and-repeat loop; instead gxserver repeats the call itself, as that same caller, the moment the owner's signed approval arrives, so approving on the Mac or the phone is what makes the change. A caller that does repeat its call gets the same answer back (`claim_retry`), once the change ran, and never a second request.
pub(crate) mod approvals {
    use super::*;

    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock, PoisonError};

    /// How long a finished answer waits for its caller to repeat the call.
    const KEEP_ANSWER_MS: u64 = 10 * 60_000;
    /// How long a request stays in `asked_by`: past the 10 minutes a pending one lives.
    const KEEP_ASKED_MS: u64 = 15 * 60_000;

    /// A call gxserver repeats once approved: Collab's owner call, or an install.
    #[derive(Clone, Debug)]
    pub(crate) enum Waiting {
        Owner(Map<String, Value>),
        Install(Map<String, Value>),
    }

    #[derive(Default)]
    struct Book {
        waiting: HashMap<String, (Principal, Waiting)>,
        running: HashMap<String, Principal>,
        done: HashMap<String, (Principal, OwnerResult, u64)>,
        asked: HashMap<String, (Principal, u64)>,
    }

    fn book() -> &'static Mutex<Book> {
        static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();
        BOOK.get_or_init(Mutex::default)
    }

    fn with_book<T>(change: impl FnOnce(&mut Book) -> T) -> T {
        let mut book = book().lock().unwrap_or_else(PoisonError::into_inner);
        let now = crate::security::now_ms();
        book.done
            .retain(|_, (_, _, at)| now.saturating_sub(*at) < KEEP_ANSWER_MS);
        book.asked
            .retain(|_, (_, at)| now.saturating_sub(*at) < KEEP_ASKED_MS);
        change(&mut book)
    }

    fn same_caller(asked: &Principal, caller: &Principal) -> bool {
        asked == caller || caller.is_owner_device()
    }

    pub(crate) fn await_approval(
        approval_id: &str,
        caller: &Principal,
        params: &Map<String, Value>,
    ) {
        await_call(approval_id, caller, Waiting::Owner(params.clone()));
    }

    pub(crate) fn await_call(approval_id: &str, caller: &Principal, waiting: Waiting) {
        let strip = |mut params: Map<String, Value>| {
            params.remove(APPROVAL_ID_PARAM);
            params
        };
        let waiting = match waiting {
            Waiting::Owner(params) => Waiting::Owner(strip(params)),
            Waiting::Install(params) => Waiting::Install(strip(params)),
        };
        // Kept with the request too, so approving it after a gxserver restart still runs it.
        if let Some(security) = SECURITY.get() {
            let (kind, params) = match &waiting {
                Waiting::Owner(params) => ("owner", params),
                Waiting::Install(params) => ("install", params),
            };
            security.approvals.attach(
                approval_id,
                COLLAB_RESUME,
                json!({ "caller": caller, "kind": kind, "params": params }),
            );
        }
        with_book(|book| {
            book.waiting
                .insert(approval_id.to_string(), (caller.clone(), waiting));
            book.asked
                .entry(approval_id.to_string())
                .or_insert_with(|| (caller.clone(), crate::security::now_ms()));
        });
    }

    /// The kind of data a waiting Collab call keeps with its approval request.
    const COLLAB_RESUME: &str = "collab";

    /// The security state, for [`await_call`] (set by [`start`] when gxserver starts).
    static SECURITY: OnceLock<crate::security::SecurityState> = OnceLock::new();

    /// At gxserver start: the calls still waiting on an approval when it stopped wait again; one
    /// approved meanwhile runs now.
    fn restore_waiting(state: &Arc<AppState>) {
        let now = crate::security::now_ms();
        for (request, saved) in state.security.approvals.resumable(COLLAB_RESUME, now) {
            let Ok(caller) = serde_json::from_value::<Principal>(saved["caller"].clone()) else {
                continue;
            };
            let Some(params) = saved["params"].as_object().cloned() else {
                continue;
            };
            let waiting = match saved["kind"].as_str() {
                Some("install") => Waiting::Install(params),
                _ => Waiting::Owner(params),
            };
            with_book(|book| {
                book.waiting
                    .insert(request.id.clone(), (caller.clone(), waiting));
                book.asked
                    .insert(request.id.clone(), (caller, request.created_at_ms));
            });
            if request.status == ghostex_gx_protocol::approval::ApprovalStatus::Approved {
                let state = state.clone();
                tokio::spawn(async move {
                    finish(&state, &request.id).await;
                });
            }
        }
    }

    /// `caller`'s Collab approval requests of the last minutes, oldest first.
    pub(crate) fn asked_by(state: &AppState, caller: &Principal) -> Vec<Value> {
        let ids: Vec<String> = with_book(|book| {
            book.asked
                .iter()
                .filter(|(_, (asked, _))| same_caller(asked, caller))
                .map(|(id, _)| id.clone())
                .collect()
        });
        let now = crate::security::now_ms();
        let mut requests: Vec<_> = ids
            .iter()
            .filter_map(|id| state.security.approvals.read(id, caller, now))
            .collect();
        requests.sort_by_key(|request| request.created_at_ms);
        requests
            .into_iter()
            .map(|request| {
                json!({
                    "id": request.id,
                    "action": request.action,
                    "target": request.target,
                    "summary": request.summary,
                    "status": request.status,
                })
            })
            .collect()
    }

    /// A caller repeating its call with `approvalId`: the answer gxserver already got, waiting a
    /// moment if it is running it now. `None` lets the call go through the gate as usual.
    pub(crate) async fn claim_retry(approval_id: &str, caller: &Principal) -> Option<OwnerResult> {
        for _ in 0..100 {
            let found = with_book(|book| {
                if let Some((asked, answer, _)) = book.done.get(approval_id) {
                    return Some(same_caller(asked, caller).then(|| answer.clone()));
                }
                if book.running.contains_key(approval_id) {
                    return None;
                }
                book.waiting.remove(approval_id);
                Some(None)
            });
            match found {
                Some(answer) => return answer,
                None => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
        None
    }

    /// Runs one approved call as the caller that asked; `false` when it wasn't awaited here.
    pub(crate) async fn finish(state: &Arc<AppState>, approval_id: &str) -> bool {
        let Some((caller, waiting)) = with_book(|book| {
            let entry = book.waiting.remove(approval_id)?;
            book.running
                .insert(approval_id.to_string(), entry.0.clone());
            Some(entry)
        }) else {
            return false;
        };
        let answer = match waiting {
            Waiting::Owner(mut params) => {
                params.insert(APPROVAL_ID_PARAM.to_string(), json!(approval_id));
                Box::pin(forward_owner(state, &caller, &params)).await
            }
            Waiting::Install(mut params) => {
                params.insert(APPROVAL_ID_PARAM.to_string(), json!(approval_id));
                install_component(state, &caller, &params)
            }
        };
        if let Err(error) = &answer {
            eprintln!("[collab] the approved change {approval_id} did not go through: {error:?}");
        }
        with_book(|book| {
            book.running.remove(approval_id);
            book.done.insert(
                approval_id.to_string(),
                (caller, answer, crate::security::now_ms()),
            );
        });
        true
    }

    fn forget(approval_id: &str) {
        with_book(|book| {
            book.waiting.remove(approval_id);
        });
    }

    /// Follows the approval stream for the life of gxserver.
    pub(crate) fn start(state: Arc<AppState>) {
        let _ = SECURITY.set(state.security.clone());
        let mut events = state.security.subscribe();
        restore_waiting(&state);
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(crate::security::SecurityEvent::ApprovalApproved { request, .. }) => {
                        let state = state.clone();
                        tokio::spawn(async move {
                            finish(&state, &request.id).await;
                        });
                    }
                    Ok(
                        crate::security::SecurityEvent::ApprovalDenied { request, .. }
                        | crate::security::SecurityEvent::ApprovalExpired { request }
                        | crate::security::SecurityEvent::ApprovalWithdrawn { request, .. },
                    ) => forget(&request.id),
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}

// ---- Extension components ------------------------------------------------------------------

/// `/api/extensionComponents`: `op: "list"` (default), `install` / `update` / `uninstall` with
/// `name`. Only `collab` today; the install action is shared by any component that opens a way
/// into this computer.
async fn extension_components(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
) -> RoutedResponse {
    let op = text(params, "op").unwrap_or_else(|| "list".to_string());
    if op == "list" {
        return ok(
            endpoint_path,
            request_id,
            json!({ "components": [install::item()] }),
        );
    }
    let name = text(params, "name").unwrap_or_default();
    if name != COLLAB_COMPONENT {
        return fail(
            endpoint_path,
            request_id,
            StatusCode::NOT_FOUND,
            "notFound",
            format!("There is no extension component called {name}."),
        );
    }
    let answer = match op.as_str() {
        "install" => {
            if let Some(approval_id) = text(params, APPROVAL_ID_PARAM) {
                if let Some(answer) = approvals::claim_retry(&approval_id, caller).await {
                    return component_answer(caller, endpoint_path, request_id, params, answer);
                }
            }
            install_component(state, caller, params)
        }
        "update" => install::start_update(state.clone())
            .map(|()| json!({ "component": install::item() }))
            .map_err(|message| OwnerError::Refused("updateFailed".to_string(), message)),
        "uninstall" => {
            let state = state.clone();
            tokio::task::spawn_blocking(move || install::uninstall(&state))
                .await
                .unwrap_or_else(|_| Err("Uninstalling Collab stopped.".to_string()))
                .map(|()| json!({ "component": install::item() }))
                .map_err(|message| OwnerError::Refused("uninstallFailed".to_string(), message))
        }
        other => Err(OwnerError::Refused(
            "badRequest".to_string(),
            format!("Unknown op {other}."),
        )),
    };
    component_answer(caller, endpoint_path, request_id, params, answer)
}

fn component_answer(
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
    answer: OwnerResult,
) -> RoutedResponse {
    match answer {
        Ok(result) => ok(endpoint_path, request_id, result),
        Err(OwnerError::Presence(outcome)) => {
            if let PresenceOutcome::Pending(request) = &outcome {
                approvals::await_call(
                    &request.id,
                    caller,
                    approvals::Waiting::Install(params.clone()),
                );
            }
            match crate::server::presence_outcome_response(outcome, &endpoint_path, &request_id) {
                Err(response) => response,
                Ok(_) => fail(
                    endpoint_path,
                    request_id,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internalError",
                    "The approval was not used.",
                ),
            }
        }
        Err(OwnerError::Refused(code, message)) => fail(
            endpoint_path,
            request_id,
            match code.as_str() {
                "badRequest" => StatusCode::BAD_REQUEST,
                "notInstallable" => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            &code,
            message,
        ),
    }
}

/// Installing Collab: the owner's signed approval, then the download on a thread of its own.
fn install_component(
    state: &Arc<AppState>,
    caller: &Principal,
    params: &Map<String, Value>,
) -> OwnerResult {
    if install::installed().is_some() {
        return Ok(json!({ "component": install::item() }));
    }
    if !install::installable() {
        return Err(OwnerError::Refused(
            "notInstallable".to_string(),
            "This build of Ghostex has no Collab component to install.".to_string(),
        ));
    }
    let need = PresenceNeed {
        action: SensitiveAction::InstallExtensionComponent,
        target: COLLAB_COMPONENT.to_string(),
        summary: "Install Ghostex Collab".to_string(),
        detail: "Collab lets you share a chat through a link. Anyone with a link and its code can follow that chat, and write in it if you let them. Ghostex downloads the Collab component to turn it on.".to_string(),
    };
    let mut bound = Map::new();
    bound.insert("op".to_string(), json!("install"));
    bound.insert("name".to_string(), json!(COLLAB_COMPONENT));
    if let Some(approval_id) = params.get(APPROVAL_ID_PARAM) {
        bound.insert(APPROVAL_ID_PARAM.to_string(), approval_id.clone());
    }
    let grant = gate(state, caller, &bound, &need)?;
    let (done_state, done_caller) = (state.clone(), caller.clone());
    install::start_install(
        state.clone(),
        Box::new(move |ok| performed(&done_state, &done_caller, &grant, ok)),
    )
    .map_err(|message| OwnerError::Refused("notInstallable".to_string(), message))?;
    Ok(json!({ "component": install::item() }))
}

// ---- The component's calls -----------------------------------------------------------------

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// `/api/collabHost`: only the running component, with the host token it got at start.
pub(crate) async fn handle_host_http(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let path = COLLAB_HOST_ENDPOINT.to_string();
    if !bearer(headers).is_some_and(sidecar::host_token_matches) {
        return fail(
            path,
            request_id,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Only the Collab component calls this.",
        );
    }
    let call: HostCall = match body.get("params").cloned().map(serde_json::from_value) {
        Some(Ok(call)) => call,
        _ => {
            return fail(
                path,
                request_id,
                StatusCode::BAD_REQUEST,
                "badRequest",
                "Unknown Collab host call.",
            )
        }
    };
    match host_call(state, call).await {
        Ok(result) => ok(path, request_id, result),
        Err(message) => fail(path, request_id, StatusCode::CONFLICT, "refused", message),
    }
}

fn shared(project_id: &str, session_id: &str) -> Result<ShareScope, String> {
    registry::for_chat(project_id, session_id).ok_or_else(|| "That chat isn't shared.".to_string())
}

async fn read_routed(routed: RoutedResponse) -> (StatusCode, Value) {
    let status = routed.response.status();
    let bytes = to_bytes(routed.response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap_or_default();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub(crate) async fn host_call(state: &Arc<AppState>, call: HostCall) -> Result<Value, String> {
    match call {
        HostCall::SetShares { shares } => {
            registry::set(shares);
            Ok(json!({}))
        }
        HostCall::SetChatExtras {
            project_id,
            session_id,
            extras,
        } => {
            // The component may only describe chats it shares, or say one stopped being shared.
            if extras.get("shared") == Some(&Value::Bool(true)) {
                shared(&project_id, &session_id)?;
            }
            super::set_chat_extras(&project_id, &session_id, extras);
            crate::server::republish_session_chat(state, &project_id, &session_id);
            Ok(json!({}))
        }
        HostCall::ChatInfo {
            project_id,
            session_id,
        } => Ok(chat_info(state, &project_id, &session_id)),
        HostCall::Deliver {
            project_id,
            session_id,
            text,
            image_paths,
        } => {
            shared(&project_id, &session_id)?;
            let mut body = json!({
                "params": {
                    "projectId": project_id,
                    "sessionId": session_id,
                    "text": text,
                    "sendRequestId": Uuid::new_v4().to_string(),
                }
            });
            if !image_paths.is_empty() {
                body["params"]["imagePaths"] = json!(image_paths);
            }
            let routed = crate::session_chat_send::handle_send_session_chat_message_http(
                state,
                "/api/sendSessionChatMessage".to_string(),
                Uuid::new_v4().to_string(),
                &body,
            )
            .await;
            let (status, answer) = read_routed(routed).await;
            if status.is_success() {
                Ok(json!({}))
            } else {
                Err(answer["message"]
                    .as_str()
                    .unwrap_or("The message didn't reach the agent.")
                    .to_string())
            }
        }
        HostCall::SaveGuestImage {
            project_id,
            session_id,
            mut params,
        } => {
            shared(&project_id, &session_id)?;
            params.insert("projectId".to_string(), json!(project_id));
            params.insert("sessionId".to_string(), json!(session_id));
            let routed = crate::session_chat_files::handle_save_session_chat_image_http(
                state,
                "/api/saveSessionChatImage".to_string(),
                Uuid::new_v4().to_string(),
                &json!({ "params": params }),
            );
            let (status, answer) = read_routed(routed).await;
            if status.is_success() {
                Ok(answer["result"].clone())
            } else {
                Err(answer["message"]
                    .as_str()
                    .unwrap_or("The picture couldn't be saved.")
                    .to_string())
            }
        }
        HostCall::GuestPageAnswersMessage {
            project_id,
            session_id,
            page_id,
            answers,
        } => {
            shared(&project_id, &session_id)?;
            let root = state.paths.root_dir.clone();
            tokio::task::spawn_blocking(move || {
                crate::visual_pages::guest_page_answers_message(
                    &root,
                    &page_id,
                    &project_id,
                    &session_id,
                    &answers,
                )
            })
            .await
            .map_err(|_| "The page could not be read.".to_string())?
            .map(|message| json!({ "message": message }))
            .map_err(str::to_string)
        }
        HostCall::NotifyGuestJoined {
            project_id,
            session_id,
            guest,
        } => {
            shared(&project_id, &session_id)?;
            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                notify_guest_joined(&state, &project_id, &session_id, &guest)
            })
            .await
            .ok();
            Ok(json!({}))
        }
    }
}

fn chat_info(state: &AppState, project_id: &str, session_id: &str) -> Value {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return json!({ "exists": null });
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(session) = repository.get_session(project_id, session_id) else {
        return json!({ "exists": null });
    };
    let Some(session) = session else {
        return json!({ "exists": false });
    };
    let title = ["title", "displayTitle", "name"].iter().find_map(|key| {
        session
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    });
    let project_name = repository
        .get_project(project_id)
        .ok()
        .flatten()
        .and_then(|project| project["name"].as_str().map(str::to_string))
        .unwrap_or_default();
    json!({ "exists": true, "title": title, "projectName": project_name })
}

/// A guest joined a shared chat: a row in the owner's notifications (the Mac's and the phone's).
///
/// CDXC:Collab 2026-11-08 DECISION:
/// User (#495): "Notify when a guest joins". The row is filed under the shared chat, so it opens it, but never replaces that chat's unread rows (an approval waiting there must stay).
fn notify_guest_joined(state: &AppState, project_id: &str, session_id: &str, guest: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let info = chat_info(state, project_id, session_id);
    let title = info["title"].as_str().unwrap_or("A chat").to_string();
    let row = crate::notification_feed::store::NewNotificationFeedRow {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        kind: crate::notification_feed::store::NOTIFICATION_FEED_KIND_CUSTOM,
        title: format!("{guest} joined \u{201c}{title}\u{201d}"),
        subtitle: info["projectName"].as_str().unwrap_or_default().to_string(),
        body: "A guest is following this shared chat. Open Share to see who is watching or remove them."
            .to_string(),
        agent_name: crate::notification_feed::store::notification_agent_icon(&session),
        attention_event_id: None,
    };
    if crate::notification_feed::store::insert_notification_feed_row_beside(&db, row).is_ok() {
        crate::notification_feed::api::broadcast_notification_feed_changed(state);
    }
}

// ---- Guest reads ---------------------------------------------------------------------------

/// What a share token gets. `None` lets the request through to its normal route (a read of the
/// shared chat); `Some` is the whole answer.
pub(crate) async fn route_guest(
    state: &Arc<AppState>,
    scope: ShareScope,
    headers: &HeaderMap,
    endpoint_path: &str,
    request_id: String,
    body: &Value,
) -> Option<RoutedResponse> {
    let path = endpoint_path.to_string();
    // The share as it is now: ended, or the guest removed, since the token was looked at.
    let Some(scope) = registry::current(&scope.share_id) else {
        return Some(fail(
            path,
            request_id,
            StatusCode::FORBIDDEN,
            "collabEnded",
            "This link has ended.",
        ));
    };
    let params = read_domain_rpc_params(body).unwrap_or_default();
    if text(&params, "projectId").as_deref() != Some(scope.project_id.as_str())
        || text(&params, "sessionId").as_deref() != Some(scope.session_id.as_str())
    {
        return Some(guest_refusal(path, request_id));
    }
    let viewer = headers
        .get(COLLAB_VIEWER_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|id| scope.viewer_ids.iter().any(|viewer| viewer == id));
    if viewer.is_none() {
        return Some(guest_refusal(path, request_id));
    }
    match endpoint_path {
        "/api/readSessionChat" | "/api/readSessionChatResource" => None,
        "/api/readSessionChatImage" => {
            let wanted = text(&params, "path").unwrap_or_default();
            let shown = scope.images.iter().any(|image| image == &wanted)
                || crate::session_files_links::session_shows_path(
                    state,
                    &scope.project_id,
                    &scope.session_id,
                    &wanted,
                )
                .await
                || crate::session_files_links::session_read_image(
                    state,
                    &scope.project_id,
                    &scope.session_id,
                    &wanted,
                )
                .await;
            if shown {
                None
            } else {
                Some(guest_refusal(path, request_id))
            }
        }
        GUEST_PAGE_ENDPOINT => Some(guest_page(state, &scope, &params, path, request_id).await),
        _ => Some(guest_refusal(path, request_id)),
    }
}

async fn guest_page(
    state: &Arc<AppState>,
    scope: &ShareScope,
    params: &Map<String, Value>,
    path: String,
    request_id: String,
) -> RoutedResponse {
    let state_dir = state.paths.root_dir.clone();
    let (project_id, session_id) = (scope.project_id.clone(), scope.session_id.clone());
    let page = if let Some(page_id) = text(params, "pageId") {
        tokio::task::spawn_blocking(move || {
            Ok(crate::visual_pages::guest_page(
                &state_dir,
                &page_id,
                &project_id,
                &session_id,
            ))
        })
        .await
    } else if let Some(file) = text(params, "path") {
        if !crate::session_files_links::session_shows_path(
            state,
            &scope.project_id,
            &scope.session_id,
            &file,
        )
        .await
        {
            return guest_refusal(path, request_id);
        }
        tokio::task::spawn_blocking(move || {
            crate::visual_pages::guest_file_page(
                &state_dir,
                std::path::Path::new(&file),
                &project_id,
                &session_id,
            )
        })
        .await
    } else {
        return fail(
            path,
            request_id,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "pageId or path is required.",
        );
    };
    match page {
        Ok(Ok(Some(page))) => ok(
            path,
            request_id,
            json!({
                "html": page.html,
                "title": page.title,
                "pageId": page.page_id,
                "answerable": page.answerable,
            }),
        ),
        Ok(Ok(None)) => fail(
            path,
            request_id,
            StatusCode::NOT_FOUND,
            "notFound",
            "This page can't be opened here.",
        ),
        Ok(Err(message)) => fail(
            path,
            request_id,
            StatusCode::UNPROCESSABLE_ENTITY,
            "pageUnavailable",
            message,
        ),
        Err(_) => fail(
            path,
            request_id,
            StatusCode::INTERNAL_SERVER_ERROR,
            "pageUnavailable",
            "The page could not be read.",
        ),
    }
}
