// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote gxserver presentation
// WebSocket client: handshake, frame read/write, and stream/event payload
// handling. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures::channel::mpsc;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_gxserver_rpc_result(
    target: &GpuiRemoteGxserverRequestTarget,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let (status_code, body) =
        gpui_remote_gxserver_post_typed_operation(target, path, params, timeout)?;
    if !(200..300).contains(&status_code) {
        return Err("Remote gxserver request failed.".to_string());
    }
    parse_gpui_gxserver_rpc_result(&body)
}

/*
CDXC:AddProject 2026-07-30:
The add-project dialog is the one gxserver surface that shows the daemon's own
rejection text ("Workspace root is not a directory: …", "Destination path
already exists and is not empty.", a `gh` lookup failure). Generic errors made
the old remote add-project flow unusable: the user could not tell a bad path
from a dead tunnel. Only the daemon's structured `message` is forwarded, and
only for a rejected request — transport failures stay as fixed local copy so no
tunnel host, port, token, or raw response body can reach the renderer.
*/
pub(crate) fn gpui_remote_gxserver_presentation_stream_loop(
    target: GpuiRemoteGxserverRequestTarget,
    cancel: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<GpuiRemoteGxserverPresentationStreamMessage>,
    client_id: String,
    last_revision: Option<u64>,
) {
    let mut last_revision = last_revision;
    for attempt in 0..GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_ATTEMPTS {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        let _ = gpui_remote_gxserver_presentation_stream_once(
            &target,
            cancel.as_ref(),
            &tx,
            client_id.as_str(),
            &mut last_revision,
        );
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        if attempt + 1 < GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_ATTEMPTS {
            thread::sleep(GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_RECONNECT_DELAY);
        }
    }
    if !cancel.load(Ordering::SeqCst) {
        let _ = tx.unbounded_send(GpuiRemoteGxserverPresentationStreamMessage::Failed);
    }
}

pub(crate) fn gpui_remote_gxserver_presentation_stream_once(
    target: &GpuiRemoteGxserverRequestTarget,
    cancel: &AtomicBool,
    tx: &mpsc::UnboundedSender<GpuiRemoteGxserverPresentationStreamMessage>,
    client_id: &str,
    last_revision: &mut Option<u64>,
) -> Result<(), String> {
    /*
    CDXC:GPUIRemotePresentationStreaming 2026-06-24-19:54:
    The remote presentation stream uses gxserver's existing WebSocket protocol through the authenticated localhost tunnel. The only renderer-bound products of this helper are presentation snapshots/deltas and the bounded workspace-order metadata already consumed by SidebarApp; eventStreamReady, server ids, auth details, raw response bodies, and transport failures stay native-only.
    */
    let mut stream = gpui_remote_gxserver_open_presentation_websocket(target, cancel)?;
    stream
        .set_read_timeout(Some(GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_READ_TIMEOUT))
        .map_err(|_| "Could not configure remote gxserver event read timeout.".to_string())?;
    let subscribe = gpui_remote_gxserver_presentation_subscribe_message(client_id, *last_revision);
    gpui_websocket_write_text_frame(&mut stream, subscribe.as_bytes())?;
    let mut liveness = GpuiRemoteGxserverPresentationLiveness::new();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Ok(());
        }
        let Some(frame) = gpui_websocket_read_frame(&mut stream, target, &mut liveness, cancel)?
        else {
            return Ok(());
        };
        match frame {
            GpuiWebSocketFrame::Text(text) => {
                let Ok(event) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
                    continue;
                };
                let Some((payload, revision)) =
                    gpui_remote_gxserver_presentation_event_payload(&event)
                else {
                    continue;
                };
                if let Some(revision) = revision {
                    *last_revision = Some(revision);
                }
                tx.unbounded_send(GpuiRemoteGxserverPresentationStreamMessage::Event(payload))
                    .map_err(|_| "Remote presentation receiver closed.".to_string())?;
            }
            GpuiWebSocketFrame::Ping(payload) => {
                gpui_websocket_write_control_frame(&mut stream, 0xA, &payload)?;
            }
            GpuiWebSocketFrame::Close => {
                return Err("Remote gxserver event stream closed.".to_string());
            }
            GpuiWebSocketFrame::Pong | GpuiWebSocketFrame::Other => {}
        }
    }
}

pub(crate) fn gpui_remote_gxserver_open_presentation_websocket(
    target: &GpuiRemoteGxserverRequestTarget,
    cancel: &AtomicBool,
) -> Result<TcpStream, String> {
    let address = format!("127.0.0.1:{}", target.local_port);
    let mut stream = TcpStream::connect(&address)
        .map_err(|_| "Remote gxserver event stream is not reachable.".to_string())?;
    stream
        .set_read_timeout(Some(GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_READ_TIMEOUT))
        .map_err(|_| "Could not configure remote gxserver event read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(
            GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_HANDSHAKE_TIMEOUT,
        ))
        .map_err(|_| "Could not configure remote gxserver event write timeout.".to_string())?;
    let request = format!(
        "GET /api/events?protocolVersion={GPUI_GXSERVER_PROTOCOL_VERSION}&authToken={} HTTP/1.1\r\nHost: {address}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
        target.token.as_str(),
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not send remote gxserver event handshake.".to_string())?;
    let headers = gpui_read_websocket_handshake_headers(&mut stream, cancel)?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "Remote gxserver returned an invalid event stream status.".to_string())?;
    if status != 101 {
        return Err("Remote gxserver did not accept the event stream.".to_string());
    }
    Ok(stream)
}

pub(crate) fn gpui_read_websocket_handshake_headers(
    stream: &mut TcpStream,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let deadline = Instant::now() + GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_HANDSHAKE_TIMEOUT;
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while Instant::now() < deadline {
        if cancel.load(Ordering::SeqCst) {
            return Err("Remote gxserver event stream was cancelled.".to_string());
        }
        match stream.read(&mut byte) {
            Ok(0) => return Err("Remote gxserver closed the event handshake.".to_string()),
            Ok(_) => {
                bytes.push(byte[0]);
                if bytes.ends_with(b"\r\n\r\n") {
                    return String::from_utf8(bytes)
                        .map_err(|_| "Remote gxserver event handshake was invalid.".to_string());
                }
                if bytes.len() > 16 * 1024 {
                    return Err("Remote gxserver event handshake was too large.".to_string());
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return Err("Could not read remote gxserver event handshake.".to_string()),
        }
    }
    Err("Remote gxserver event handshake timed out.".to_string())
}

pub(crate) fn gpui_remote_gxserver_presentation_subscribe_message(
    client_id: &str,
    last_revision: Option<u64>,
) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert("clientId".to_string(), serde_json::json!(client_id));
    if let Some(last_revision) = last_revision {
        payload.insert("lastRevision".to_string(), serde_json::json!(last_revision));
    }
    payload.insert(
        "type".to_string(),
        serde_json::json!("subscribePresentation"),
    );
    serde_json::Value::Object(payload).to_string()
}

pub(crate) enum GpuiWebSocketFrame {
    Close,
    Other,
    Ping(Vec<u8>),
    Pong,
    Text(String),
}

pub(crate) struct GpuiRemoteGxserverPresentationLiveness {
    pub(crate) next_health_check: Instant,
}

impl GpuiRemoteGxserverPresentationLiveness {
    pub(crate) fn new() -> Self {
        Self {
            next_health_check: Instant::now() + GPUI_REMOTE_GXSERVER_PRESENTATION_HEALTH_INTERVAL,
        }
    }

    pub(crate) fn confirm_after_read_timeout(
        &mut self,
        target: &GpuiRemoteGxserverRequestTarget,
    ) -> Result<(), String> {
        if Instant::now() < self.next_health_check {
            return Ok(());
        }
        self.next_health_check = Instant::now() + GPUI_REMOTE_GXSERVER_PRESENTATION_HEALTH_INTERVAL;
        if gpui_remote_authenticated_health(target.local_port, target.token.as_str()).is_some() {
            Ok(())
        } else {
            Err("Remote gxserver event stream failed its liveness check.".to_string())
        }
    }
}

pub(crate) fn gpui_websocket_read_frame(
    stream: &mut TcpStream,
    target: &GpuiRemoteGxserverRequestTarget,
    liveness: &mut GpuiRemoteGxserverPresentationLiveness,
    cancel: &AtomicBool,
) -> Result<Option<GpuiWebSocketFrame>, String> {
    let mut header = [0u8; 2];
    if !gpui_websocket_read_exact(stream, &mut header, target, liveness, cancel)? {
        return Ok(None);
    }
    let fin = header[0] & 0x80 != 0;
    let opcode = header[0] & 0x0F;
    if !fin {
        return Err("Remote gxserver sent a fragmented event frame.".to_string());
    }
    let masked = header[1] & 0x80 != 0;
    let mut payload_len = u64::from(header[1] & 0x7F);
    if payload_len == 126 {
        let mut extended = [0u8; 2];
        if !gpui_websocket_read_exact(stream, &mut extended, target, liveness, cancel)? {
            return Ok(None);
        }
        payload_len = u64::from(u16::from_be_bytes(extended));
    } else if payload_len == 127 {
        let mut extended = [0u8; 8];
        if !gpui_websocket_read_exact(stream, &mut extended, target, liveness, cancel)? {
            return Ok(None);
        }
        payload_len = u64::from_be_bytes(extended);
    }
    if payload_len as usize > GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_FRAME_MAX_BYTES {
        return Err("Remote gxserver event frame was too large.".to_string());
    }
    let mut mask = [0u8; 4];
    if masked && !gpui_websocket_read_exact(stream, &mut mask, target, liveness, cancel)? {
        return Ok(None);
    }
    let mut payload = vec![0u8; payload_len as usize];
    if !payload.is_empty()
        && !gpui_websocket_read_exact(stream, &mut payload, target, liveness, cancel)?
    {
        return Ok(None);
    }
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % mask.len()];
        }
    }
    match opcode {
        0x1 => String::from_utf8(payload)
            .map(GpuiWebSocketFrame::Text)
            .map(Some)
            .map_err(|_| "Remote gxserver event frame was not UTF-8.".to_string()),
        0x8 => Ok(Some(GpuiWebSocketFrame::Close)),
        0x9 => Ok(Some(GpuiWebSocketFrame::Ping(payload))),
        0xA => Ok(Some(GpuiWebSocketFrame::Pong)),
        _ => Ok(Some(GpuiWebSocketFrame::Other)),
    }
}

pub(crate) fn gpui_websocket_read_exact(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    target: &GpuiRemoteGxserverRequestTarget,
    liveness: &mut GpuiRemoteGxserverPresentationLiveness,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    let mut offset = 0;
    while offset < buffer.len() {
        if cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        match stream.read(&mut buffer[offset..]) {
            Ok(0) => return Err("Remote gxserver closed the event stream.".to_string()),
            Ok(read_count) => {
                offset += read_count;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                liveness.confirm_after_read_timeout(target)?;
                continue;
            }
            Err(_) => return Err("Could not read remote gxserver event stream.".to_string()),
        }
    }
    Ok(true)
}

pub(crate) fn gpui_websocket_write_text_frame(
    stream: &mut TcpStream,
    payload: &[u8],
) -> Result<(), String> {
    gpui_websocket_write_frame(stream, 0x1, payload)
}

pub(crate) fn gpui_websocket_write_control_frame(
    stream: &mut TcpStream,
    opcode: u8,
    payload: &[u8],
) -> Result<(), String> {
    if payload.len() > 125 {
        return Err("Remote gxserver control frame was too large.".to_string());
    }
    gpui_websocket_write_frame(stream, opcode, payload)
}

pub(crate) fn gpui_websocket_write_frame(
    stream: &mut TcpStream,
    opcode: u8,
    payload: &[u8],
) -> Result<(), String> {
    let mask = gpui_websocket_mask();
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | (opcode & 0x0F));
    if payload.len() <= 125 {
        frame.push(0x80 | payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(&mask);
    for (index, byte) in payload.iter().enumerate() {
        frame.push(*byte ^ mask[index % mask.len()]);
    }
    stream
        .write_all(&frame)
        .map_err(|_| "Could not write remote gxserver event frame.".to_string())
}

pub(crate) fn gpui_websocket_mask() -> [u8; 4] {
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32)
        .wrapping_add(std::process::id())
        .to_be_bytes()
}

pub(crate) fn gpui_remote_gxserver_presentation_event_payload(
    event: &serde_json::Value,
) -> Option<(serde_json::Value, Option<u64>)> {
    let object = event.as_object()?;
    match object.get("type").and_then(serde_json::Value::as_str)? {
        "presentationSnapshot" => {
            let snapshot = object.get("snapshot")?;
            if !gpui_remote_gxserver_presentation_snapshot_allowed(snapshot) {
                return None;
            }
            let revision = gpui_json_revision(snapshot).or_else(|| gpui_json_revision(event));
            Some((
                serde_json::json!({
                    "snapshot": (*snapshot).clone(),
                    "type": "presentationSnapshot",
                }),
                revision,
            ))
        }
        "presentationDelta" => {
            let delta = object.get("delta")?;
            if !gpui_remote_gxserver_presentation_delta_allowed(delta) {
                return None;
            }
            let revision = gpui_json_revision(event)?;
            Some((
                serde_json::json!({
                    "delta": (*delta).clone(),
                    "revision": revision,
                    "type": "presentationDelta",
                }),
                Some(revision),
            ))
        }
        "sidebarProjectCollectionsChanged" => {
            let collections = object.get("sidebarProjectCollections")?;
            if !gpui_remote_sidebar_project_collections_allowed(collections) {
                return None;
            }
            let revision = gpui_json_revision(event)?;
            Some((
                serde_json::json!({
                    "revision": revision,
                    "sidebarProjectCollections": (*collections).clone(),
                    "type": "sidebarProjectCollectionsChanged",
                }),
                Some(revision),
            ))
        }
        "workspaceGroupsChanged" => {
            let groups = object.get("groups")?;
            if !gpui_remote_workspace_groups_allowed(groups) {
                return None;
            }
            let revision = gpui_json_revision(event)?;
            Some((
                serde_json::json!({
                    "groups": (*groups).clone(),
                    "revision": revision,
                    "type": "workspaceGroupsChanged",
                }),
                Some(revision),
            ))
        }
        _ => None,
    }
}

fn gpui_remote_sidebar_project_collections_allowed(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("collections")
        .and_then(serde_json::Value::as_object)
        .is_some()
        && object
            .get("order")
            .and_then(serde_json::Value::as_array)
            .is_some()
        && object
            .get("nextCollectionNumber")
            .and_then(serde_json::Value::as_u64)
            .is_some()
}

fn gpui_remote_workspace_groups_allowed(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("projectOrder")
        .and_then(serde_json::Value::as_array)
        .is_some()
        && object
            .get("projects")
            .and_then(serde_json::Value::as_object)
            .is_some()
}

pub(crate) fn gpui_remote_gxserver_presentation_snapshot_allowed(
    value: &serde_json::Value,
) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .is_some()
        && object
            .get("projects")
            .and_then(serde_json::Value::as_array)
            .is_some()
        && object
            .get("sessions")
            .and_then(serde_json::Value::as_array)
            .is_some()
        && gpui_json_revision(value).is_some()
}

pub(crate) fn gpui_remote_gxserver_presentation_delta_allowed(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(serde_json::Value::as_str)
        .is_some()
}

pub(crate) fn gpui_json_revision(value: &serde_json::Value) -> Option<u64> {
    value.get("revision").and_then(|revision| {
        revision.as_u64().or_else(|| {
            revision
                .as_i64()
                .and_then(|value| u64::try_from(value).ok())
        })
    })
}
