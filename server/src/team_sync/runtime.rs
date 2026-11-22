//! The live subscription to each connected workspace's command queue.
//!
//! CDXC:TeamSync 2026-10-09 WHY:
//! A command must reach the requester's Ghostex the moment it is online, so each connected
//! workspace keeps one WebSocket subscription to `commands:listOpen` through the official Rust
//! `convex` client (it runs on gxserver's tokio runtime and reconnects and replays the query on
//! its own), instead of a polling loop that would add delay and Convex function calls while idle.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use convex::{ConvexClient, ConvexClientBuilder, FunctionResult, WebSocketState};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, Notify};
use tokio::task::{JoinHandle, JoinSet};

use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::AppState;

use super::commands::{handles_command_type, run_team_command, CommandOutcome};
use super::connections::{read_team_connections, TeamConnection};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(120);
const FIRST_RETRY: Duration = Duration::from_secs(5);
const MAX_RETRY: Duration = Duration::from_secs(300);

/// What `readTeamSyncStatus` shows about one workspace's subscription.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubscriptionStatus {
    /// `connecting`, `connected`, `reconnecting` or `error`.
    pub(crate) state: &'static str,
    pub(crate) last_error: Option<String>,
    pub(crate) connected_at: Option<String>,
    pub(crate) last_update_at: Option<String>,
    pub(crate) open_commands: usize,
    pub(crate) handled_commands: u64,
}

fn statuses() -> &'static Mutex<HashMap<String, SubscriptionStatus>> {
    static STATUSES: OnceLock<Mutex<HashMap<String, SubscriptionStatus>>> = OnceLock::new();
    STATUSES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn reload_signal() -> &'static Notify {
    static RELOAD: OnceLock<Notify> = OnceLock::new();
    RELOAD.get_or_init(Notify::new)
}

fn update_status(workspace_id: &str, update: impl FnOnce(&mut SubscriptionStatus)) {
    if let Ok(mut statuses) = statuses().lock() {
        update(statuses.entry(workspace_id.to_string()).or_default());
    }
}

pub(crate) fn subscription_status(workspace_id: &str) -> Option<SubscriptionStatus> {
    statuses().lock().ok()?.get(workspace_id).cloned()
}

/// Re-reads the connections and restarts every subscription (after a join, a deploy or a leave).
pub(crate) fn reload_team_sync() {
    reload_signal().notify_one();
}

/// Starts the subscriptions for every connected workspace. Aborting the handle stops them all.
pub(crate) fn spawn_team_sync_task(state: &Arc<AppState>) -> JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        loop {
            let paths = state.paths.clone();
            // While the Workspaces built-in extension is off no workspace subscribes; the stored
            // connections are kept, and turning it on reloads (`reload_team_sync`).
            let connections = tokio::task::spawn_blocking(move || {
                if crate::workspaces::read_workspaces_feature_enabled(&paths) {
                    read_team_connections(&paths)
                } else {
                    Vec::new()
                }
            })
            .await
            .unwrap_or_default();
            if let Ok(mut statuses) = statuses().lock() {
                statuses.clear();
            }
            // At start and after a join or connect: the member's Linear user and own key.
            super::linear_keys::spawn_member_linear_key_sync(&state, None);
            // Dropping the set at the next reload (or when this task is aborted) aborts them.
            let mut subscriptions = JoinSet::new();
            for connection in connections {
                update_status(&connection.workspace_id, |status| {
                    status.state = "connecting"
                });
                subscriptions.spawn(run_member_subscription(state.clone(), connection));
            }
            reload_signal().notified().await;
            subscriptions.abort_all();
        }
    })
}

async fn run_member_subscription(state: Arc<AppState>, connection: TeamConnection) {
    let mut retry = FIRST_RETRY;
    let mut last_logged_error: Option<String> = None;
    loop {
        let error = match subscribe_once(&state, &connection).await {
            Ok(()) => "The connection to the team's Convex project closed.".to_string(),
            Err(error) => error,
        };
        update_status(&connection.workspace_id, |status| {
            status.state = "error";
            status.last_error = Some(error.clone());
        });
        if last_logged_error.as_deref() != Some(error.as_str()) {
            log_team_sync_failure(&state, &connection.workspace_id, &error);
            last_logged_error = Some(error);
        }
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(MAX_RETRY);
    }
}

fn token_args(connection: &TeamConnection) -> BTreeMap<String, convex::Value> {
    BTreeMap::from([(
        "memberToken".to_string(),
        convex::Value::String(connection.member_token.clone()),
    )])
}

fn now_text() -> String {
    chrono::Utc::now().to_rfc3339()
}

async fn subscribe_once(state: &Arc<AppState>, connection: &TeamConnection) -> Result<(), String> {
    let workspace_id = connection.workspace_id.as_str();
    let (socket_tx, mut socket_rx) = mpsc::channel::<WebSocketState>(8);
    let mut client = ConvexClientBuilder::new(&connection.deployment_url)
        .with_on_state_change(socket_tx)
        .build()
        .await
        .map_err(|error| format!("Could not connect to the team's Convex project: {error}"))?;
    let mut subscription = client
        .subscribe("commands:listOpen", token_args(connection))
        .await
        .map_err(|error| format!("Could not subscribe to this member's commands: {error}"))?;
    update_status(workspace_id, |status| {
        status.state = "connected";
        status.last_error = None;
        status.connected_at = Some(now_text());
    });
    let in_flight: Arc<Mutex<HashSet<String>>> = Arc::default();
    let mut heartbeat = crate::ticks::steady_interval(HEARTBEAT_INTERVAL);
    loop {
        tokio::select! {
            update = subscription.next() => match update {
                None => return Ok(()),
                Some(FunctionResult::Value(value)) => {
                    let commands = match value.export() {
                        Value::Array(commands) => commands,
                        _ => Vec::new(),
                    };
                    update_status(workspace_id, |status| {
                        status.state = "connected";
                        status.last_error = None;
                        status.last_update_at = Some(now_text());
                        status.open_commands = commands.len();
                    });
                    for command in commands {
                        start_command(state, connection, &client, &in_flight, command);
                    }
                }
                Some(FunctionResult::ErrorMessage(message)) => {
                    update_status(workspace_id, |status| {
                        status.state = "error";
                        status.last_error = Some(message);
                    });
                }
                Some(FunctionResult::ConvexError(error)) => {
                    let message = match error.data {
                        convex::Value::String(text) => text,
                        _ => error.message,
                    };
                    update_status(workspace_id, |status| {
                        status.state = "error";
                        status.last_error = Some(message);
                    });
                }
            },
            Some(socket) = socket_rx.recv() => {
                update_status(workspace_id, |status| {
                    status.state = match socket {
                        WebSocketState::Connected => "connected",
                        WebSocketState::Connecting => "reconnecting",
                    };
                });
            }
            _ = heartbeat.tick() => {
                // Lets the team's Convex tell "this member's Ghostex is offline" from "busy".
                let _ = client.mutation("teams:heartbeat", token_args(connection)).await;
            }
        }
    }
}

/// Claims and runs one open command in the background, unless this Ghostex does not handle its
/// type, another of this member's Ghostex instances holds it, or it is already running here.
fn start_command(
    state: &Arc<AppState>,
    connection: &TeamConnection,
    client: &ConvexClient,
    in_flight: &Arc<Mutex<HashSet<String>>>,
    command: Value,
) {
    let Some(command_id) = command
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let command_type = command.get("type").and_then(Value::as_str).unwrap_or("");
    if !handles_command_type(command_type) {
        return;
    }
    let claimed_elsewhere = command.get("status").and_then(Value::as_str) == Some("claimed")
        && command.get("claimedBy").and_then(Value::as_str)
            != Some(state.metadata.server_id.as_str());
    if claimed_elsewhere {
        return;
    }
    let newly_started = in_flight
        .lock()
        .map(|mut running| running.insert(command_id.clone()))
        .unwrap_or(false);
    if !newly_started {
        return;
    }
    let state = state.clone();
    let connection = connection.clone();
    let mut client = client.clone();
    let in_flight = in_flight.clone();
    tokio::spawn(async move {
        let client_id = state.metadata.server_id.clone();
        let mut args = token_args(&connection);
        args.insert(
            "commandId".to_string(),
            convex::Value::String(command_id.clone()),
        );
        args.insert(
            "clientId".to_string(),
            convex::Value::String(client_id.clone()),
        );
        let claimed = matches!(
            client.mutation("commands:claim", args.clone()).await,
            Ok(FunctionResult::Value(value)) if value != convex::Value::Null
        );
        if claimed {
            let worker_state = state.clone();
            let worker_connection = connection.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                run_team_command(&worker_state, &worker_connection, &command)
            })
            .await
            .unwrap_or_else(|error| {
                CommandOutcome::Failed(format!("The command crashed: {error}"))
            });
            match outcome {
                CommandOutcome::Done(result) => {
                    args.insert("ok".to_string(), convex::Value::Boolean(true));
                    if let Ok(result) = convex::Value::try_from(result) {
                        args.insert("result".to_string(), result);
                    }
                }
                CommandOutcome::Failed(error) => {
                    args.insert("ok".to_string(), convex::Value::Boolean(false));
                    args.insert("error".to_string(), convex::Value::String(error));
                }
            }
            if let Err(error) = client.mutation("commands:complete", args).await {
                log_team_sync_failure(
                    &state,
                    &connection.workspace_id,
                    &format!("Could not report command {command_id} as finished: {error}"),
                );
            }
            update_status(&connection.workspace_id, |status| {
                status.handled_commands += 1;
            });
        }
        if let Ok(mut running) = in_flight.lock() {
            running.remove(&command_id);
        }
    });
}

fn log_team_sync_failure(state: &AppState, workspace_id: &str, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "teamSyncSubscriptionFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: Some(serde_json::json!({ "workspaceId": workspace_id })),
    });
}
