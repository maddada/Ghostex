//! How a GBot chat on another computer reaches its home computer, where its record, folder and
//! memory are: that computer's `/api/gbots` sends the chat's own brief and memory calls home to
//! `/api/gbotsRemote`, and home answers them as the GBot's own chat once it finds that session in
//! `gbots_remote_chats`.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! Plan risk "memory writes from a remote chat must go through the home gxserver (never a file copy)". A connected computer's token names only the computer, so home can't tell which session is asking; the other computer's gxserver can (the session's own key), and it is trusted like every request from that computer. So the call is made by gxserver itself with the session's ids, never by the relay: `/api/gbotsRemote` is open to connected computers (scope.rs) but the relay refuses to forward it (relay.rs), so an agent there can't send it with another session's ids, and home answers only a session it started as that GBot's chat.
//! SEE-ALSO: server/src/gbots/computer/remote_chat.rs (the chat and its `gbotHome` mark), server/src/connected_computers/scope.rs, server/src/connected_computers/relay.rs.

use std::sync::{Arc, OnceLock, Weak};

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::remote_chat::HOME_KEY;
use super::store::{self, Place};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::protocol::{rpc_error, rpc_success};
use crate::security::Principal;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

pub(crate) const GBOTS_REMOTE_ENDPOINT: &str = "/api/gbotsRemote";

static STATE: OnceLock<Weak<AppState>> = OnceLock::new();

/// Keeps a handle on gxserver's state for the Cloud Box start, which needs to own one.
pub(crate) fn remember_state(state: &Arc<AppState>) {
    let _ = STATE.set(Arc::downgrade(state));
}

pub(crate) fn app_state() -> Option<Arc<AppState>> {
    STATE.get()?.upgrade()
}

/// The ops a GBot chat on another computer sends home: its brief and its memory.
fn goes_home(op: &str) -> bool {
    op == "brief" || super::super::memory::handles(op)
}

fn op_of(params: &Map<String, Value>) -> &str {
    params
        .get("op")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("list")
}

fn answer(
    endpoint_path: &str,
    request_id: &str,
    outcome: Result<Value, DomainStateError>,
) -> RoutedResponse {
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path.to_string()),
            StatusCode::OK,
            rpc_success(request_id.to_string(), result),
        ),
        Err(error) => {
            domain_error_response(endpoint_path.to_string(), request_id.to_string(), error)
        }
    }
}

/// On the computer a GBot chat runs on: sends that chat's brief and memory calls to its home
/// computer. `None` for everyone else (the call is answered here as usual).
pub(crate) async fn forward_home(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: &str,
    request_id: &str,
    params: &Map<String, Value>,
) -> Option<RoutedResponse> {
    let Principal::Session {
        project_id,
        session_id,
        ..
    } = caller
    else {
        return None;
    };
    if !goes_home(op_of(params)) {
        return None;
    }
    let (project_id, session_id) = (project_id.clone(), session_id.clone());
    let task_state = state.clone();
    let params = params.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&task_state.paths).ok()?;
        let repository = DomainRepository::new(&db, &task_state.metadata.server_id);
        let session = repository.get_session(&project_id, &session_id).ok()??;
        let home = session
            .pointer(&format!("/runtimeSettings/{HOME_KEY}"))?
            .clone();
        let home_id = home.get("serverId").and_then(Value::as_str)?.to_string();
        let home_name = home
            .get("computerName")
            .and_then(Value::as_str)
            .unwrap_or("your GBot's home computer")
            .to_string();
        Some(
            super::link::reach(&home_id, &home_name).and_then(|computer| {
                super::link::call(
                    &computer,
                    GBOTS_REMOTE_ENDPOINT,
                    json!({
                        "chat": { "projectId": project_id, "sessionId": session_id },
                        "request": Value::Object(params),
                    }),
                )
            }),
        )
    })
    .await
    .ok()??;
    Some(answer(endpoint_path, request_id, outcome))
}

/// `/api/gbotsRemote` on a GBot's home computer: a call from one of its chats on a connected
/// computer, sent by that computer's gxserver.
pub(crate) async fn handle_gbots_remote_http(
    state: Arc<AppState>,
    caller: Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let Principal::Peer { server_id, .. } = caller else {
        return answer(
            &endpoint_path,
            &request_id,
            Err(DomainStateError {
                code: "forbidden",
                message: "Only a GBot's chat on another computer uses this.".to_string(),
            }),
        );
    };
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if !super::super::gbots_on(&state.paths) {
        return routed_json(
            Some(endpoint_path),
            StatusCode::CONFLICT,
            rpc_error(
                "turnedOff",
                super::super::gbots_off_message(&state.paths),
                Some(request_id),
            ),
        );
    }
    let task_state = state.clone();
    let outcome = tokio::task::spawn_blocking(move || run_remote(&task_state, &server_id, &params))
        .await
        .unwrap_or_else(|error| {
            Err(DomainStateError {
                code: "internalError",
                message: format!("GBots could not answer: {error}"),
            })
        });
    answer(&endpoint_path, &request_id, outcome)
}

fn run_remote(
    state: &AppState,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let text = |value: Option<&Value>, key: &str| {
        value
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (
        text(params.get("chat"), "projectId"),
        text(params.get("chat"), "sessionId"),
    );
    let request = params
        .get("request")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let chat =
        store::find_remote_chat(&db, server_id, &project_id, &session_id)?.ok_or_else(|| {
            DomainStateError {
                code: "forbidden",
                message: "This chat isn't one of your GBots' chats on that computer.".to_string(),
            }
        })?;
    let bot = super::super::record::require_bot(&db, &chat.bot_id)?;
    let op = op_of(&request).to_string();
    if op == "brief" {
        let team = super::super::brief::Team::load(&db)?;
        return Ok(json!({
            "bot": super::super::record::bot_summary(&bot),
            "brief": super::super::brief::brief_text(&state.paths, &bot, &team),
        }));
    }
    if !goes_home(&op) {
        return Err(DomainStateError {
            code: "forbidden",
            message: "From another computer a GBot reads its brief and keeps its memory; the rest it asks for in its reply.".to_string(),
        });
    }
    super::super::memory::run(
        state,
        &db,
        &repository,
        super::super::memory::MemoryCaller {
            own_bot: Some(bot),
            may_manage: false,
            label: "user",
        },
        &op,
        &request,
    )
}

/// The brief's part about where the GBot works and whether it may use the computer's screen.
pub(crate) fn brief_section(paths: &GxserverPaths, bot_id: &str) -> String {
    let Ok(db) = open_gxserver_database(paths) else {
        return String::new();
    };
    let Ok(computer) = store::read(&db, bot_id) else {
        return String::new();
    };
    let mut out = String::new();
    let home = crate::connected_computers::this_computer_name();
    match &computer.place {
        Place::This => {}
        Place::Computer(_) => out.push_str(&format!(
            "## Where you work\n\nThe chats the user, Chief of Staff and other GBots open with you run on {name}, in {folder} there; your working files go in that folder. Your own folder (SOUL.md, MEMORY.md, your notes) stays on {home}: from {name}, `ghostex gbots brief` and `ghostex gbots memory …` reach it as usual, so use them and don't look for those files on {name}. Your routine runs stay on {home}.\n\n",
            name = computer.label(),
            folder = computer.folder.clone().unwrap_or_else(|| super::ops::default_folder("GBot")),
        )),
        Place::CloudBox(_) => out.push_str(&format!(
            "## Where you work\n\nThe chats the user, Chief of Staff and other GBots open with you run on the Cloud Box {name}, each in its own worktree of that project, and carry this brief with them. Your routine runs stay on {home}.\n\n",
            name = computer.label(),
        )),
    }
    if computer.computer_use {
        out.push_str("## Using the computer's screen and apps\n\nThe user lets you use the screen and apps of the computer you work on when the work needs it: use `$ghostex-spaceo` (your own virtual display, so you never take over the user's screen; one SpaceO session per chat, destroyed when you're done) or `$ghostex-computer-use` (cua-driver, in the background). Prefer a command, file or API when there is one, and never sign in, buy, send or delete anything there unless the user asked for exactly that.\n\n");
    }
    out
}
