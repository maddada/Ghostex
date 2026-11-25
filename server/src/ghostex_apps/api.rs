//! The authenticated Ghostex Apps routes. Every one is refused while Ghostex Apps is off.
//!
//! - `/api/publishGhostexApp` (publish.rs): agents publish through `ghostex app show`.
//! - `/api/listGhostexApps`, `/api/readGhostexApp {id}`: what is there, with each power's state.
//! - `/api/openGhostexApp {id}`: a launch token and the address to open the app with.
//! - `/api/setGhostexAppGrant {id, permission, decision}`: the user's decision on one power.
//! - `/api/deleteGhostexApp {id}`: removes the app, its versions, icon and data.
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! The brief: "never grant anything without the user's click". Opening an app with its powers, changing a grant and deleting an app are for the user's own Ghostex (the desktop app, the web build and the `ghostex` CLI in the user's own terminal on the token file, and paired phones: `Principal::is_owner_device`); an agent's session token, another computer, a Collab guest and an extension are refused, so an agent can build and publish an app but never allow it anything or call its bridge.
//!
//! CDXC:GhostexApps 2026-11-26 WHY:
//! Fable's security review (finding 3, proven live): an agent that sends the token file instead of its own session token passed as the user and allowed its own app's powers. These three routes now take the stricter rule of server/src/server/autopilot_http.rs `is_user`: the token file counts as the user only from a terminal that names no Ghostex session (the CLI inside a session sends the one it runs in). Every grant change, open and delete, and every refusal of one, goes to the security log. What stays open is the same-user limit Ghostex can't close: an agent that sends the token file without the session header, or edits app.json. Finding 9: list and read show an agent only its own project's apps.
//!
//! CDXC:GhostexApps 2026-11-26 WHY:
//! Fable's re-review of 970a63ba9: the security log sentence quoted the caller's own `id`, `permission` and `decision`, so a refused session wrote "Approved by you: allow" and a right-to-left override into its entry, and every refusal wrote a line without limit. The sentence now holds only fixed forms (a real app id, a known permission, one of the three decisions, or "unknown"), and each caller's refusals of one route are logged three times per 10 minutes, the next line saying how many were left out. List and read follow `is_user`: the token file naming a session sees no app.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    security::Principal,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::{
    feature::{apps_off_message, apps_on},
    manifest::{Power, ALL_POWERS},
    publish::{publish_app, PUBLISH_GHOSTEX_APP_ENDPOINT},
    serve::APP_ROUTE_PREFIX,
    store::{delete, list_records, owner_project, power_state, read_record, set_grant},
    tokens,
};

pub(crate) const LIST_GHOSTEX_APPS_ENDPOINT: &str = "/api/listGhostexApps";
pub(crate) const READ_GHOSTEX_APP_ENDPOINT: &str = "/api/readGhostexApp";
pub(crate) const OPEN_GHOSTEX_APP_ENDPOINT: &str = "/api/openGhostexApp";
pub(crate) const SET_GHOSTEX_APP_GRANT_ENDPOINT: &str = "/api/setGhostexAppGrant";
pub(crate) const DELETE_GHOSTEX_APP_ENDPOINT: &str = "/api/deleteGhostexApp";

pub(crate) fn is_ghostex_apps_endpoint(path: &str) -> bool {
    matches!(
        path,
        PUBLISH_GHOSTEX_APP_ENDPOINT
            | LIST_GHOSTEX_APPS_ENDPOINT
            | READ_GHOSTEX_APP_ENDPOINT
            | OPEN_GHOSTEX_APP_ENDPOINT
            | SET_GHOSTEX_APP_GRANT_ENDPOINT
            | DELETE_GHOSTEX_APP_ENDPOINT
    )
}

/// Whether `endpoint` is only for the user's own Ghostex.
fn users_only(endpoint: &str) -> bool {
    matches!(
        endpoint,
        OPEN_GHOSTEX_APP_ENDPOINT | SET_GHOSTEX_APP_GRANT_ENDPOINT | DELETE_GHOSTEX_APP_ENDPOINT
    )
}

/// Whether the caller is the user: the owner session, a paired phone, or the token file from a
/// terminal that names no Ghostex session (`token_file_names_session`).
fn is_user(caller: &Principal, token_file_names_session: bool) -> bool {
    caller.is_owner_device()
        && !(matches!(caller, Principal::LocalAgent) && token_file_names_session)
}

/// Which apps a caller may see: every app for the user, its own project's for a session, none for
/// anyone else.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sees {
    All,
    Project(String),
    Nothing,
}

/// Follows [`is_user`]: the token file naming a session is neither the user nor a session whose
/// token proved its project, so it sees nothing.
fn sees(caller: &Principal, token_file_names_session: bool) -> Sees {
    match caller {
        Principal::Session { project_id, .. } => Sees::Project(project_id.clone()),
        caller if is_user(caller, token_file_names_session) => Sees::All,
        _ => Sees::Nothing,
    }
}

fn visible(record: &Value, sees: &Sees) -> bool {
    match sees {
        Sees::All => true,
        Sees::Project(project) => owner_project(record).as_deref() == Some(project.as_str()),
        Sees::Nothing => false,
    }
}

fn audit_kind(endpoint: &str) -> &'static str {
    match endpoint {
        OPEN_GHOSTEX_APP_ENDPOINT => "ghostexApps.open",
        SET_GHOSTEX_APP_GRANT_ENDPOINT => "ghostexApps.grant",
        _ => "ghostexApps.delete",
    }
}

/// The security log's sentence for a call to one of the user's routes, and the app it names. Only
/// fixed forms go in: an id that is not an app id, a permission Ghostex does not know or a
/// decision that is not one of the three is named as such, never quoted, so a caller cannot write
/// its own words (or control and direction characters) into the log (re-review of 970a63ba9).
fn audit_summary(endpoint: &str, params: &Value) -> (String, String) {
    let params = params.get("params").unwrap_or(params);
    let field = |key: &str| params.get(key).and_then(Value::as_str).map(str::trim);
    let id = field("id")
        .filter(|id| super::store::is_app_id(id))
        .map(str::to_string);
    let app = id.as_deref().map_or_else(
        || "an app with a malformed id".to_string(),
        |id| format!("the Ghostex App {id}"),
    );
    let summary = match endpoint {
        OPEN_GHOSTEX_APP_ENDPOINT => format!("Open {app}"),
        SET_GHOSTEX_APP_GRANT_ENDPOINT => {
            let permission = field("permission").and_then(Power::from_name).map_or_else(
                || "an unknown permission".to_string(),
                |power| format!("the \"{}\" permission", power.name()),
            );
            let decision = field("decision")
                .filter(|decision| matches!(*decision, "allowed" | "denied" | "ask"))
                .unwrap_or("an unknown decision");
            format!("Set {permission} of {app} to {decision}")
        }
        _ => format!("Delete {app}"),
    };
    (summary, id.unwrap_or_default())
}

/// Refusals logged per caller and route in [`REFUSAL_WINDOW`]; the rest are counted and the next
/// line logged says how many were left out (re-review: 40 refused opens wrote 40 lines).
const REFUSALS_LOGGED: u32 = 3;
const REFUSAL_WINDOW: std::time::Duration = std::time::Duration::from_secs(10 * 60);

struct Refusals {
    since: std::time::Instant,
    logged: u32,
    left_out: u32,
}

static REFUSALS: std::sync::Mutex<Option<std::collections::HashMap<String, Refusals>>> =
    std::sync::Mutex::new(None);

/// Whether to log this refusal, and how many like it went unlogged before it.
fn log_refusal(key: String) -> Option<u32> {
    let mut guard = REFUSALS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let refusals = guard.get_or_insert_with(std::collections::HashMap::new);
    let now = std::time::Instant::now();
    refusals
        .retain(|_, entry| now.duration_since(entry.since) < REFUSAL_WINDOW || entry.left_out > 0);
    if refusals.len() > 1024 {
        refusals.clear();
    }
    let entry = refusals.entry(key).or_insert(Refusals {
        since: now,
        logged: 0,
        left_out: 0,
    });
    if now.duration_since(entry.since) >= REFUSAL_WINDOW {
        let left_out = entry.left_out;
        *entry = Refusals {
            since: now,
            logged: 1,
            left_out: 0,
        };
        return Some(left_out);
    }
    if entry.logged < REFUSALS_LOGGED {
        entry.logged += 1;
        return Some(0);
    }
    entry.left_out = entry.left_out.saturating_add(1);
    None
}

/// Who a refusal came from, to count its repeats.
fn caller_key(caller: &Principal) -> String {
    match caller {
        Principal::Session { session_id, .. } => format!("session:{session_id}"),
        Principal::Owner { device_id } | Principal::Device { device_id } => {
            format!("device:{device_id}")
        }
        Principal::LocalAgent => "token file".to_string(),
        Principal::Peer { server_id, .. } => format!("peer:{server_id}"),
        Principal::Guest { share_id } => format!("guest:{share_id}"),
        Principal::Extension { extension_id } => format!("extension:{extension_id}"),
        Principal::Anonymous => "anonymous".to_string(),
    }
}

pub(crate) async fn handle_ghostex_apps_http(
    state: Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: Value,
    caller: Principal,
) -> RoutedResponse {
    if !apps_on(&state.paths) {
        tokens::revoke_all();
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::bad_request(apps_off_message()),
        );
    }
    let users = users_only(&endpoint_path);
    let audit = users.then(|| audit_summary(&endpoint_path, &body));
    let record = |outcome: crate::audit::Outcome| {
        if let Some((summary, id)) = audit.clone() {
            crate::audit::record(
                crate::audit::Action::new(audit_kind(&endpoint_path), summary).target(id),
                crate::audit::Actor::current(),
                outcome,
                None,
            );
        }
    };
    let names_session = crate::audit::token_file_names_session();
    if users && !is_user(&caller, names_session) {
        let message = "Only you can do this, from Ghostex itself; an agent can't.";
        let key = format!("{}|{}", audit_kind(&endpoint_path), caller_key(&caller));
        if let Some(left_out) = log_refusal(key) {
            let message = match left_out {
                0 => message.to_string(),
                count => format!(
                    "{message} ({count} more like it from the same caller were not logged.)"
                ),
            };
            record(crate::audit::Outcome::Refused(message));
        }
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "forbidden",
                message: message.to_string(),
            },
        );
    }
    let params = match read_domain_rpc_params(&body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let state_dir = state.paths.root_dir.clone();
    let endpoint = endpoint_path.clone();
    let paths = state.paths.clone();
    let result = tokio::task::spawn_blocking(move || {
        let generation = super::feature::generation(&paths);
        run(
            &state_dir,
            &endpoint,
            &params,
            &caller,
            names_session,
            generation,
        )
    })
    .await
    .unwrap_or_else(|error| {
        Err(DomainStateError::corrupt_state(format!(
            "Ghostex Apps stopped unexpectedly: {error}"
        )))
    });
    record(crate::audit::Outcome::of(
        &result.as_ref().map_err(|error| &error.message),
    ));
    match result {
        Ok(value) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn run(
    state_dir: &std::path::Path,
    endpoint: &str,
    params: &Map<String, Value>,
    caller: &Principal,
    token_file_names_session: bool,
    generation: u64,
) -> Result<Value, DomainStateError> {
    if endpoint == PUBLISH_GHOSTEX_APP_ENDPOINT {
        return publish_app(state_dir, params, caller);
    }
    let sees = sees(caller, token_file_names_session);
    if endpoint == LIST_GHOSTEX_APPS_ENDPOINT {
        let apps: Vec<Value> = list_records(state_dir)
            .iter()
            .filter(|record| visible(record, &sees))
            .map(summary)
            .collect();
        return Ok(json!({"apps": apps}));
    }
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let record = read_record(state_dir, &id)
        .filter(|record| visible(record, &sees))
        .ok_or_else(|| DomainStateError::not_found("No such app."))?;
    match endpoint {
        READ_GHOSTEX_APP_ENDPOINT => Ok(summary(&record)),
        OPEN_GHOSTEX_APP_ENDPOINT => {
            let token = tokens::mint(&id, generation);
            Ok(json!({
                "id": id,
                "path": format!("{APP_ROUTE_PREFIX}{id}"),
                "token": token,
                "fragment": format!("gx-app-token={token}"),
            }))
        }
        SET_GHOSTEX_APP_GRANT_ENDPOINT => {
            let power = params
                .get("permission")
                .and_then(Value::as_str)
                .and_then(Power::from_name)
                .ok_or_else(|| DomainStateError::bad_request("Name a permission such as send."))?;
            let decision = params
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let record = set_grant(state_dir, &id, power, decision)
                .map_err(DomainStateError::bad_request)?;
            Ok(summary(&record))
        }
        DELETE_GHOSTEX_APP_ENDPOINT => {
            tokens::revoke_app(&id);
            let deleted = delete(state_dir, &id).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("Could not delete the app: {error}"),
            })?;
            Ok(json!({"id": id, "deleted": deleted}))
        }
        _ => Err(DomainStateError::not_found("Unknown Ghostex Apps call.")),
    }
}

/// What a client is told about an app: no HTML and no data, only its card, its versions and each
/// declared power's state.
fn summary(record: &Value) -> Value {
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let permissions: Map<String, Value> = ALL_POWERS
        .into_iter()
        .filter_map(|power| {
            let state = power_state(record, power);
            (state != "undeclared").then(|| (power.name().to_string(), json!(state)))
        })
        .collect();
    let pending: Vec<Value> = record
        .get("pendingAsks")
        .and_then(Value::as_object)
        .map(|asks| asks.keys().map(|name| json!(name)).collect())
        .unwrap_or_default();
    json!({
        "id": id,
        "name": record.get("name").cloned().unwrap_or(Value::Null),
        "version": record.get("version").cloned().unwrap_or(Value::Null),
        "createdAt": record.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": record.get("updatedAt").cloned().unwrap_or(Value::Null),
        "publisher": record.get("publisher").cloned().unwrap_or(Value::Null),
        "sourcePath": record.get("sourcePath").cloned().unwrap_or(Value::Null),
        "path": format!("{APP_ROUTE_PREFIX}{id}"),
        "iconPath": format!("{APP_ROUTE_PREFIX}{id}/icon"),
        "permissions": permissions,
        "pendingAsks": pending,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(project: &str) -> Principal {
        Principal::Session {
            project_id: project.to_string(),
            session_id: "S1".to_string(),
            role: crate::security::SessionRole::Agent,
        }
    }

    #[test]
    fn an_agent_with_the_token_file_is_not_the_user() {
        // Review finding 3: the token file with the session header the CLI sends inside a session.
        assert!(!is_user(&Principal::LocalAgent, true));
        assert!(is_user(&Principal::LocalAgent, false));
        assert!(!is_user(&session("P1"), false));
        assert!(is_user(
            &Principal::Owner {
                device_id: "d".to_string()
            },
            true
        ));
        assert!(!is_user(&Principal::Anonymous, false));
    }

    #[test]
    fn a_session_sees_only_its_own_projects_apps() {
        let mine = json!({"id": "a", "publisher": {"projectId": "P1", "sessionId": "S1"}});
        let theirs = json!({"id": "b", "publisher": {"projectId": "P2", "sessionId": "S2"}});
        let local = json!({"id": "c", "publisher": null});
        let agent = sees(&session("P1"), false);
        assert!(visible(&mine, &agent));
        assert!(!visible(&theirs, &agent));
        assert!(!visible(&local, &agent));
        let user = sees(&Principal::LocalAgent, false);
        assert!(visible(&theirs, &user) && visible(&local, &user));
        // The token file naming a session is not the user (re-review nit).
        let named = sees(&Principal::LocalAgent, true);
        assert!(!visible(&mine, &named) && !visible(&local, &named));
        let guest = sees(
            &Principal::Guest {
                share_id: "g".to_string(),
            },
            false,
        );
        assert!(!visible(&mine, &guest));
    }

    #[test]
    fn the_log_sentence_holds_only_fixed_forms() {
        let id = "0123456789abcdef0123456789abcdef";
        let (summary, target) = audit_summary(
            SET_GHOSTEX_APP_GRANT_ENDPOINT,
            &json!({"params": {"id": id, "permission": "send", "decision": "allowed"}}),
        );
        assert_eq!(
            summary,
            format!("Set the \"send\" permission of the Ghostex App {id} to allowed")
        );
        assert_eq!(target, id);
        // The review's case: a refused session's own words and a right-to-left override.
        let (summary, target) = audit_summary(
            SET_GHOSTEX_APP_GRANT_ENDPOINT,
            &json!({"id": "x\u{202E}. Approved by you: allow", "permission": "send\nApproved", "decision": "allow\u{202E}"}),
        );
        assert_eq!(
            summary,
            "Set an unknown permission of an app with a malformed id to an unknown decision"
        );
        assert_eq!(target, "");
        let (summary, _) = audit_summary(OPEN_GHOSTEX_APP_ENDPOINT, &json!({"id": 7}));
        assert_eq!(summary, "Open an app with a malformed id");
    }

    #[test]
    fn repeated_refusals_are_collapsed() {
        let key = || "ghostexApps.open|session:test-collapse".to_string();
        for _ in 0..REFUSALS_LOGGED {
            assert_eq!(log_refusal(key()), Some(0));
        }
        for _ in 0..37 {
            assert_eq!(log_refusal(key()), None);
        }
        assert_eq!(
            log_refusal("ghostexApps.open|session:other".to_string()),
            Some(0)
        );
    }
}
