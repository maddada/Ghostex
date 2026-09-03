use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
};

use axum::{
    extract::ws::{Message, WebSocket},
    http::Uri,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{
    native_pty_system, Child, ChildKiller, CommandBuilder, ExitStatus, MasterPty, PtySize,
};
use serde_json::{json, Map, Value};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    agents::read_agent_settings,
    auth::is_expected_gxserver_auth_token,
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::DomainRepository,
    paths::GxserverPaths,
    storage::open_gxserver_database,
    zmx::{dispatch_zmx_lifecycle_endpoint, ZmxEndpointError, ZmxServerContext},
};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 30;

#[derive(Clone)]
pub(crate) struct TerminalWsState {
    pub auth_token: String,
    pub base_url: String,
    pub paths: GxserverPaths,
    pub server_id: String,
}

struct AttachMetadata {
    attach_command: String,
    cwd: String,
    zmx_name: String,
}

struct SpawnedTerminal {
    child: Box<dyn Child + Send + Sync>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

pub(crate) async fn handle_terminal_socket(
    mut socket: WebSocket,
    uri: Uri,
    state: TerminalWsState,
) {
    let query = terminal_query(&uri);
    if query
        .get("authToken")
        .map(String::as_str)
        .map(|token| is_expected_gxserver_auth_token(token, &state.auth_token))
        != Some(true)
    {
        send_error_and_close(
            &mut socket,
            "unauthorized",
            "gxserver auth token is required for this endpoint.",
        )
        .await;
        return;
    }
    let protocol_version = query.get("protocolVersion");
    if protocol_version.and_then(|value| value.parse::<u64>().ok())
        != Some(GXSERVER_PROTOCOL_VERSION)
    {
        let actual = protocol_version.map(String::as_str).unwrap_or("undefined");
        send_error_and_close(
            &mut socket,
            "protocolMismatch",
            &format!(
                "gxserver protocol mismatch. Expected protocol {GXSERVER_PROTOCOL_VERSION}, got {actual}. Update Ghostex and gxserver so their protocol versions match."
            ),
        )
        .await;
        return;
    }
    let (Some(project_id), Some(session_id)) = (
        query.get("projectId").cloned(),
        query.get("sessionId").cloned(),
    ) else {
        send_error_and_close(
            &mut socket,
            "notFound",
            "The requested gxserver terminal session does not exist.",
        )
        .await;
        return;
    };
    let cols = query_dimension(&query, "cols", DEFAULT_COLS);
    let rows = query_dimension(&query, "rows", DEFAULT_ROWS);
    let metadata_state = state.clone();
    let metadata = tokio::task::spawn_blocking(move || {
        resolve_attach_metadata(&metadata_state, project_id, session_id)
    })
    .await;
    let metadata = match metadata {
        Ok(Ok(metadata)) => metadata,
        Ok(Err((code, message))) => {
            send_error_and_close(&mut socket, code, &message).await;
            return;
        }
        Err(error) => {
            send_error_and_close(
                &mut socket,
                "providerNotRunning",
                &format!("Unable to resolve terminal attach metadata: {error}"),
            )
            .await;
            return;
        }
    };
    let zmx_name = metadata.zmx_name.clone();
    let spawned = tokio::task::spawn_blocking(move || spawn_terminal(metadata, cols, rows)).await;
    let spawned = match spawned {
        Ok(Ok(spawned)) => spawned,
        Ok(Err(message)) => {
            send_error_and_close(&mut socket, "providerNotRunning", &message).await;
            return;
        }
        Err(error) => {
            send_error_and_close(
                &mut socket,
                "providerNotRunning",
                &format!("Unable to spawn terminal attach process: {error}"),
            )
            .await;
            return;
        }
    };
    run_terminal_socket(socket, spawned, zmx_name, cols, rows).await;
}

fn resolve_attach_metadata(
    state: &TerminalWsState,
    project_id: String,
    session_id: String,
) -> Result<AttachMetadata, (&'static str, String)> {
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        (
            "providerNotRunning",
            format!("Unable to open gxserver state: {error}"),
        )
    })?;
    let repository = DomainRepository::new(&db, &state.server_id);
    let agent_settings = read_agent_settings(&db).map_err(|error| {
        (
            "providerNotRunning",
            format!("Unable to read gxserver agent settings: {error}"),
        )
    })?;
    let mut params = Map::new();
    params.insert("projectId".to_string(), Value::String(project_id));
    params.insert("sessionId".to_string(), Value::String(session_id));
    let context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: state.base_url.clone(),
    };
    let output = dispatch_zmx_lifecycle_endpoint(
        &repository,
        "/api/attachSessionMetadata",
        &params,
        &context,
        &agent_settings,
    )
    .map_err(|error| match error {
        ZmxEndpointError::Domain(error) if error.code == "notFound" => ("notFound", error.message),
        ZmxEndpointError::Domain(error) => ("providerNotRunning", error.message),
        ZmxEndpointError::DependencyUnavailable(message) => ("providerNotRunning", message),
    })?;
    let attach = output.result.get("attach").ok_or_else(|| {
        (
            "providerNotRunning",
            "Attach metadata is missing.".to_string(),
        )
    })?;
    if attach
        .get("providerState")
        .and_then(|value| value.get("lifecycleState"))
        .and_then(Value::as_str)
        != Some("exists")
    {
        return Err((
            "providerNotRunning",
            "The requested session provider is not running. Wake it before attaching.".to_string(),
        ));
    }
    Ok(AttachMetadata {
        attach_command: required_attach_text(attach, "attachCommand")?,
        cwd: required_attach_text(attach, "cwd")?,
        zmx_name: required_attach_text(attach, "zmxName")?,
    })
}

fn required_attach_text(
    attach: &Value,
    key: &'static str,
) -> Result<String, (&'static str, String)> {
    attach
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            (
                "providerNotRunning",
                format!("Terminal attach metadata is missing {key}."),
            )
        })
}

fn spawn_terminal(
    metadata: AttachMetadata,
    cols: u16,
    rows: u16,
) -> Result<SpawnedTerminal, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Unable to open terminal PTY: {error}"))?;
    let shell = env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string());
    let mut command = CommandBuilder::new(shell);
    command.args(["-lc", metadata.attach_command.as_str()]);
    command.cwd(metadata.cwd);
    command.env("TERM", "xterm-256color");
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Unable to open terminal PTY reader: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Unable to open terminal PTY writer: {error}"))?;
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("Unable to spawn terminal attach process: {error}"))?;
    drop(pair.slave);
    let killer = child.clone_killer();
    Ok(SpawnedTerminal {
        child,
        killer,
        master: pair.master,
        reader,
        writer,
    })
}

async fn run_terminal_socket(
    socket: WebSocket,
    spawned: SpawnedTerminal,
    zmx_name: String,
    cols: u16,
    rows: u16,
) {
    let SpawnedTerminal {
        child,
        mut killer,
        master,
        reader,
        writer,
    } = spawned;
    let (mut socket_sender, mut socket_receiver) = socket.split();
    if send_json(
        &mut socket_sender,
        json!({"type": "ready", "zmxName": zmx_name, "cols": cols, "rows": rows}),
    )
    .await
    .is_err()
    {
        cleanup_terminal(killer, child, None, None).await;
        return;
    }
    let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(64);
    let reader_task = tokio::task::spawn_blocking(move || read_pty(reader, output_tx));
    let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>(64);
    let writer_task = tokio::task::spawn_blocking(move || write_pty(writer, input_rx));
    let (exit_tx, mut exit_rx) = mpsc::channel(1);
    let child_task = tokio::task::spawn_blocking(move || {
        let mut child = child;
        let status = child.wait();
        let _ = exit_tx.blocking_send(status);
        child
    });
    let mut child_exited = false;
    loop {
        tokio::select! {
            message = socket_receiver.next() => {
                match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        if input_tx.send(bytes.to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Some(bytes) = handle_control_message(&master, text.as_str()) {
                            if input_tx.send(bytes).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(bytes))) => {
                        if socket_sender.send(Message::Pong(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                }
            }
            output = output_rx.recv() => {
                match output {
                    Some(bytes) => {
                        if socket_sender.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            status = exit_rx.recv() => {
                child_exited = true;
                let code = status
                    .and_then(Result::ok)
                    .as_ref()
                    .and_then(exit_code);
                let _ = send_json(&mut socket_sender, json!({"type": "exit", "code": code})).await;
                let _ = socket_sender.send(Message::Close(None)).await;
                break;
            }
        }
    }
    drop(input_tx);
    drop(output_rx);
    if !child_exited {
        let _ = tokio::task::spawn_blocking(move || killer.kill()).await;
    }
    let child = child_task.await.ok();
    cleanup_terminal_after_reap(child, reader_task, writer_task).await;
}

fn read_pty(mut reader: Box<dyn Read + Send>, output_tx: mpsc::Sender<Vec<u8>>) {
    let mut buffer = vec![0; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if output_tx.blocking_send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
        }
    }
}

fn write_pty(mut writer: Box<dyn Write + Send>, mut input_rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(bytes) = input_rx.blocking_recv() {
        if writer
            .write_all(&bytes)
            .and_then(|()| writer.flush())
            .is_err()
        {
            break;
        }
    }
}

/*
CDXC:Zmx 2026-09-03:
Web clients keep a session's xterm mounted (socket open) while it is parked
behind another tab or under the chat view, so a plain detach never happens
and the hidden client would keep clamping the shared zmx grid to its own
size. `{"type":"visibility","hidden":bool,"cols":..,"rows":..}` resizes this
pty exactly like `resize` and then feeds the zmx attach client the in-band
`ZMX_HIDDEN` / `ZMX_VISIBLE` sequence at that same grid, which zmx consumes
(never forwarding it to the shell) to hand the grid to whichever client is
actually displaying, or rest wide when none is. The sequence goes through the
pty writer channel so it is ordered with the client's own input bytes.
Returns the bytes to write, if any; the socket loop owns the channel.
*/
fn handle_control_message(master: &Box<dyn MasterPty + Send>, text: &str) -> Option<Vec<u8>> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return None;
    };
    let hidden = match value.get("type").and_then(Value::as_str)? {
        "resize" => None,
        "visibility" => Some(value.get("hidden").and_then(Value::as_bool)?),
        _ => return None,
    };
    let cols = json_dimension(&value, "cols")?;
    let rows = json_dimension(&value, "rows")?;
    let _ = master.resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    });
    hidden.map(|hidden| zmx_visibility_sequence(hidden, rows, cols))
}

fn zmx_visibility_sequence(hidden: bool, rows: u16, cols: u16) -> Vec<u8> {
    let key = if hidden { "ZMX_HIDDEN" } else { "ZMX_VISIBLE" };
    format!("\x1b]1337;{key}={rows},{cols}\x07").into_bytes()
}

fn json_dimension(value: &Value, key: &str) -> Option<u16> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn exit_code(status: &ExitStatus) -> Option<u32> {
    status.signal().is_none().then(|| status.exit_code())
}

async fn cleanup_terminal(
    mut killer: Box<dyn ChildKiller + Send + Sync>,
    mut child: Box<dyn Child + Send + Sync>,
    reader_task: Option<JoinHandle<()>>,
    writer_task: Option<JoinHandle<()>>,
) {
    let _ = tokio::task::spawn_blocking(move || {
        let _ = killer.kill();
        let _ = child.wait();
    })
    .await;
    if let Some(task) = reader_task {
        let _ = task.await;
    }
    if let Some(task) = writer_task {
        let _ = task.await;
    }
}

async fn cleanup_terminal_after_reap(
    _child: Option<Box<dyn Child + Send + Sync>>,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
) {
    let _ = reader_task.await;
    let _ = writer_task.await;
}

async fn send_error_and_close(socket: &mut WebSocket, code: &'static str, message: &str) {
    let _ = socket
        .send(Message::Text(
            json!({"type": "error", "code": code, "message": message})
                .to_string()
                .into(),
        ))
        .await;
    let _ = socket.send(Message::Close(None)).await;
}

async fn send_json(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    value: Value,
) -> Result<(), axum::Error> {
    sender.send(Message::Text(value.to_string().into())).await
}

fn terminal_query(uri: &Uri) -> HashMap<String, String> {
    uri.query()
        .map(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn query_dimension(query: &HashMap<String, String>, key: &str, default: u16) -> u16 {
    query
        .get(key)
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
