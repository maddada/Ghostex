//! `ghostex cloud attach <box-id> <project-id> <session-id>`: this terminal becomes the session's
//! terminal on a Cloud Box.
//!
//! CDXC:CloudBoxes 2026-11-02 WHY:
//! The desktop opens a remote machine's session by running `ssh … zmx attach` in a terminal pane,
//! and a box has no saved SSH machine. Its gxserver serves the session's terminal on
//! `/api/terminal` (the web build's path), so this verb is the pane's process instead: it asks home
//! gxserver how to reach the box (`connectInfo`, so no token is ever on a command line), opens a
//! tailcat forward when the box is reached that way, puts this terminal in raw mode and pumps bytes
//! both ways. The desktop's visibility sequences (`ZMX_VISIBLE` and friends) are bytes like any
//! other and reach the box's `zmx attach` client unchanged; a window resize becomes the socket's
//! `resize` control.
//!
//! SEE-ALSO: server/src/terminal_ws.rs (the socket), apps/desktop/src/app/gx_store/cloud_boxes.rs
//! (`cloud_box_attach_command`, which the desktop's remote open runs).

use std::io::Write as _;

use futures_util::{SinkExt as _, StreamExt as _};
use ghostex_gx_protocol::cloud_box::{
    CloudBoxConnectInfo, CLOUD_BOXES_ROUTE, CLOUD_BOX_TRANSPORT_DIRECT, CLOUD_BOX_TRANSPORT_TAILCAT,
};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use crate::ghostex_cli::{
    args::Flags,
    rpc::{call_gxserver_rpc, CliError, CliResult, GXSERVER_LOCAL_API_HOST},
    tailcat_tunnel::{start_tailcat_tunnel, TailcatForwardPlan},
};

/// Restores the terminal's mode when the attach ends, however it ends.
struct RawMode(bool);

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.0 {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

pub(crate) fn attach(
    box_id: &str,
    project_id: &str,
    session_id: &str,
    flags: &Flags,
) -> CliResult<()> {
    let info = call_gxserver_rpc(
        CLOUD_BOXES_ROUTE,
        &json!({ "action": "connectInfo", "boxId": box_id }),
        flags,
    )?;
    let info: CloudBoxConnectInfo = serde_json::from_value(info)
        .map_err(|error| CliError::Other(format!("Unexpected connectInfo answer: {error}")))?;
    let mut tunnel = None;
    let base_url = match info.transport.as_str() {
        CLOUD_BOX_TRANSPORT_DIRECT => info
            .base_url
            .clone()
            .ok_or_else(|| CliError::Other("The box has no address yet.".to_string()))?,
        CLOUD_BOX_TRANSPORT_TAILCAT => {
            let address = info.tailcat_address.clone().ok_or_else(|| {
                CliError::Other("The box has no tailcat address yet.".to_string())
            })?;
            let binary = crate::tailcat::binary::resolve_client_tailcat_binary().ok_or_else(|| {
                CliError::Other(format!(
                    "Easy Connect (tailcat {} or newer) is not installed on this computer. Install it in Settings › Remote › Easy Connect.",
                    ghostex_paths::tailcat::TAILCAT_MIN_VERSION
                ))
            })?;
            let local_port = std::net::TcpListener::bind((GXSERVER_LOCAL_API_HOST, 0))
                .and_then(|listener| listener.local_addr())
                .map(|address| address.port())?;
            let plan = TailcatForwardPlan {
                address_blob: address,
                base_url: format!("http://{GXSERVER_LOCAL_API_HOST}:{local_port}"),
                local_port,
                remote_port: u16::try_from(info.gxserver_port).map_err(|_| {
                    CliError::Other("The box's gxserver port is invalid.".to_string())
                })?,
                client_key: None,
            };
            tunnel = Some(start_tailcat_tunnel(&binary, &plan)?);
            plan.base_url
        }
        other => return Err(CliError::Other(format!("Unknown box transport {other}."))),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(pump(
        &base_url,
        &info.gxserver_token,
        project_id,
        session_id,
    ));
    if let Some(tunnel) = tunnel {
        tunnel.stop();
    }
    result
}

async fn pump(base_url: &str, token: &str, project_id: &str, session_id: &str) -> CliResult<()> {
    ghostex_paths::loopback_owner::check_url(base_url)
        .map_err(|error| CliError::Other(error.to_string()))?;
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let encode = |value: &str| -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    };
    let url = format!(
        "{}/api/terminal?authToken={}&protocolVersion=1&projectId={}&sessionId={}&cols={cols}&rows={rows}",
        base_url.trim_end_matches('/').replacen("http", "ws", 1),
        encode(token),
        encode(project_id),
        encode(session_id),
    );
    let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| CliError::Connection(format!("Could not reach the box: {error}")))?;
    let (mut sender, mut receiver) = socket.split();
    let _raw = RawMode(crossterm::terminal::enable_raw_mode().is_ok());

    // stdin is read on a thread of its own: a blocking read is the only portable way to see every
    // byte as it is typed.
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        use std::io::Read as _;
        let mut stdin = std::io::stdin();
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    if input_tx.blocking_send(buffer[..read].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    #[cfg(unix)]
    let mut resized =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())?;
    let mut stdout = std::io::stdout();
    loop {
        #[cfg(unix)]
        let window_change = resized.recv();
        #[cfg(not(unix))]
        let window_change = std::future::pending::<Option<()>>();
        tokio::select! {
            input = input_rx.recv() => match input {
                Some(bytes) => {
                    if sender.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            _ = window_change => {
                if let Ok((cols, rows)) = crossterm::terminal::size() {
                    let control = json!({ "type": "resize", "cols": cols, "rows": rows });
                    if sender.send(Message::Text(control.to_string().into())).await.is_err() {
                        break;
                    }
                }
            }
            message = receiver.next() => match message {
                Some(Ok(Message::Binary(bytes))) => {
                    if stdout.write_all(&bytes).and_then(|()| stdout.flush()).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    let control: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                    match control["type"].as_str() {
                        Some("exit") => break,
                        Some("error") => {
                            return Err(CliError::Other(
                                control["message"].as_str().unwrap_or("The box refused the terminal.").to_string(),
                            ));
                        }
                        _ => {}
                    }
                }
                Some(Ok(Message::Ping(bytes))) => {
                    let _ = sender.send(Message::Pong(bytes)).await;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    let _ = sender.send(Message::Close(None)).await;
    Ok(())
}
