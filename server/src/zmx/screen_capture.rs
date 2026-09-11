use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

#[cfg(not(unix))]
use crate::toolchain::require_bundled_zmx;

#[cfg(not(unix))]
use super::*;

/// Terminal text for one zmx session, read by name.
pub(crate) struct ZmxHistoryCapture {
    pub text: String,
    /// The capture lost its tail (the live screen), so screen-state readers
    /// must not draw conclusions from it. Unix retains the tail; the Windows
    /// subprocess path can still lose it at the stdout byte cap.
    pub truncated: bool,
}

/*
CDXC:AgentScreenDetection 2026-09-06 DECISION:
User: capture the actual active buffer, only the grid for alternate-screen TUIs and the grid plus bounded recent scrollback for scrolling CLIs, without changing polling or full-history APIs or restarting live sessions.
Capture asks zmx to select rows before formatting and sending them; trimming the History reply here still made the daemon serialize and transfer its entire scrollback.
An Info request follows Capture on the same connection as an ordering barrier: an Info reply without Capture means an old daemon ignored the optional tag, so only that daemon uses the existing History path.
SEE-ALSO: .dependencies/zmx/src/ipc.zig (Capture), .dependencies/zmx/src/util.zig (serializeTerminalRange).
*/

/// Recent physical rows in addition to the entire live grid. Codex dialogs
/// inspect 160 lines, composer detection 120 nonblank lines, notices 60 and
/// activity/options 15; 512 leaves space for blank rows and wrapped content.
const ZMX_SCREEN_CAPTURE_SCROLLBACK_ROWS: u32 = 512;
/// `ipc.Tag.Capture` and the compatibility ordering barrier `ipc.Tag.Info`.
#[cfg(unix)]
const ZMX_IPC_TAG_CAPTURE: u8 = 209;
#[cfg(unix)]
const ZMX_IPC_TAG_INFO: u8 = 6;
/// `ipc.Tag.History`.
#[cfg(unix)]
const ZMX_IPC_TAG_HISTORY: u8 = 8;
/// `@sizeOf(ipc.Header)`: a packed `struct { u8, u32 }` backs to `u40`, which
/// rounds up to 8 bytes. The top three bytes are padding on both ends.
#[cfg(unix)]
const ZMX_IPC_HEADER_BYTES: usize = 8;
/// `@sizeOf(ipc.Capture)`: like the header, a packed `struct { format: u8,
/// rows: u32 }` rounds up to 8 bytes, and the daemon rejects any other
/// payload length. Bytes 1..5 are the little-endian row count; the last
/// three bytes are padding the daemon ignores.
#[cfg(unix)]
const ZMX_IPC_CAPTURE_BYTES: usize = 8;
/// `util.HistoryFormat.plain`.
#[cfg(unix)]
const ZMX_IPC_HISTORY_FORMAT_PLAIN: u8 = 0;
/// Matches the 5s poll `zmx history` uses before it gives up on the daemon.
#[cfg(unix)]
const ZMX_SCREEN_CAPTURE_TIMEOUT: Duration = Duration::from_millis(5_000);
/// Maximum retained text on both bounded and legacy captures. Old daemons
/// still send full history; retain its tail without unbounded memory growth.
#[cfg(unix)]
const ZMX_SCREEN_CAPTURE_TAIL_BYTES: usize = 256 * 1024;

/// Directory the zmx daemon binds its session sockets in, resolved exactly as
/// `Cfg.socketDir` does in .dependencies/zmx/src/main.zig. Both ends agree: gxserver exports
/// this same environment into every daemon it launches, and the macOS launchd
/// supervisor already watches the resulting path as its liveness signal.
#[cfg(unix)]
fn zmx_socket_directory() -> PathBuf {
    if let Some(zmx_dir) = std::env::var_os("ZMX_DIR") {
        return PathBuf::from(zmx_dir);
    }
    if let Some(xdg_runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(xdg_runtime_dir).join("zmx");
    }
    let temporary_directory = std::env::var("TMPDIR")
        .unwrap_or_else(|_| "/tmp".to_string())
        .trim_end_matches('/')
        .to_string();
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("{temporary_directory}/zmx-{uid}"))
}

#[cfg(unix)]
pub(crate) fn zmx_session_socket_path(session_name: &str) -> PathBuf {
    zmx_socket_directory().join(session_name)
}

/// Whether a daemon still owns this session name, decided from the socket file
/// alone. `Some(false)` means the name is free; `None` means this platform
/// cannot see the daemon's socket namespace at all, so callers must treat the
/// daemon as unobservable rather than absent.
///
/// CDXC:ZmxWireGeneration 2026-08-23: this is the liveness signal for daemons that
/// cannot answer IPC. A probe would time out on a pre-wire-break daemon and on
/// a merely busy one alike, and the second must never be terminated.
#[cfg(unix)]
pub(crate) fn zmx_session_daemon_socket_present(session_name: &str) -> Option<bool> {
    use std::os::unix::fs::FileTypeExt;

    Some(
        fs::symlink_metadata(zmx_session_socket_path(session_name))
            .map(|metadata| metadata.file_type().is_socket())
            .unwrap_or(false),
    )
}

#[cfg(not(unix))]
pub(crate) fn zmx_session_daemon_socket_present(_session_name: &str) -> Option<bool> {
    None
}

/// Frees a session name whose daemon was signalled rather than asked to quit.
/// zmx unlinks the socket only on its own graceful shutdown, so without this
/// the name stays unusable and the restored session cannot claim it back.
#[cfg(unix)]
pub(crate) fn remove_zmx_session_socket(session_name: &str) {
    let _ = fs::remove_file(zmx_session_socket_path(session_name));
}

#[cfg(not(unix))]
pub(crate) fn remove_zmx_session_socket(_session_name: &str) {}

/// The session's ACTIVE screen, read straight off the daemon's IPC socket. zmx
/// answers `Capture` through Ghostty's `TerminalFormatter`, which serializes
/// only the screen currently in use: the primary screen together with the tail
/// of its scrollback, or — while a full-screen TUI such as Claude Code holds
/// the alternate screen — that grid alone, since Ghostty gives the alternate
/// screen no scrollback. Primary-screen history is never mixed into an
/// alternate-screen capture. Old daemons use History with a client-side tail cap.
#[cfg(unix)]
pub(crate) fn read_zmx_session_screen_capture(zmx_name: &str) -> Result<ZmxHistoryCapture, String> {
    read_zmx_session_screen_capture_format(zmx_name, ZMX_IPC_HISTORY_FORMAT_PLAIN)
}

#[cfg(unix)]
pub(crate) fn read_zmx_session_screen_capture_vt(
    zmx_name: &str,
) -> Result<ZmxHistoryCapture, String> {
    read_zmx_session_screen_capture_format(zmx_name, 1)
}

#[cfg(unix)]
fn read_zmx_session_screen_capture_format(
    zmx_name: &str,
    format: u8,
) -> Result<ZmxHistoryCapture, String> {
    let socket_path = zmx_session_socket_path(zmx_name);
    let mut stream = std::os::unix::net::UnixStream::connect(&socket_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::ConnectionRefused {
            // Same hygiene as `zmx history`: a refused connect means the daemon
            // is gone and only its socket file is left behind.
            let _ = fs::remove_file(&socket_path);
        }
        format!("zmx session screen capture could not reach the session: {error}")
    })?;
    stream
        .set_read_timeout(Some(ZMX_SCREEN_CAPTURE_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(ZMX_SCREEN_CAPTURE_TIMEOUT)))
        .map_err(|error| format!("zmx session screen capture could not arm timeouts: {error}"))?;

    // Send the capture and its ordering barrier in one write.
    let mut request = [0_u8; ZMX_IPC_HEADER_BYTES * 2 + ZMX_IPC_CAPTURE_BYTES];
    request[0] = ZMX_IPC_TAG_CAPTURE;
    request[1..5].copy_from_slice(&(ZMX_IPC_CAPTURE_BYTES as u32).to_le_bytes());
    request[ZMX_IPC_HEADER_BYTES] = format;
    request[ZMX_IPC_HEADER_BYTES + 1..ZMX_IPC_HEADER_BYTES + 5]
        .copy_from_slice(&ZMX_SCREEN_CAPTURE_SCROLLBACK_ROWS.to_le_bytes());
    request[ZMX_IPC_HEADER_BYTES + ZMX_IPC_CAPTURE_BYTES] = ZMX_IPC_TAG_INFO;
    stream
        .write_all(&request)
        .map_err(|error| format!("zmx session screen capture could not be requested: {error}"))?;

    read_zmx_screen_capture_reply(&mut stream, format)
}

/// Drains one capture reply, retaining only the last
/// `ZMX_SCREEN_CAPTURE_TAIL_BYTES` of payload so a huge scrollback costs
/// bounded memory here regardless of what the daemon serialized.
#[cfg(unix)]
fn read_zmx_screen_capture_reply(
    stream: &mut std::os::unix::net::UnixStream,
    format: u8,
) -> Result<ZmxHistoryCapture, String> {
    let mut header = [0_u8; ZMX_IPC_HEADER_BYTES];
    let mut chunk = vec![0_u8; 64 * 1024];
    let mut expected_tag = ZMX_IPC_TAG_CAPTURE;
    loop {
        stream.read_exact(&mut header).map_err(|error| {
            format!("zmx session screen capture reply header was unreadable: {error}")
        })?;
        let tag = header[0];
        let payload_len = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;

        let mut remaining = payload_len;
        let mut tail: Vec<u8> = Vec::new();
        let keep = tag == expected_tag;
        while remaining > 0 {
            let want = remaining.min(chunk.len());
            let read = stream.read(&mut chunk[..want]).map_err(|error| {
                format!("zmx session screen capture reply body was unreadable: {error}")
            })?;
            if read == 0 {
                return Err(
                    "zmx session screen capture reply ended before the payload did".to_string(),
                );
            }
            remaining -= read;
            if keep {
                tail.extend_from_slice(&chunk[..read]);
                if tail.len() > ZMX_SCREEN_CAPTURE_TAIL_BYTES {
                    let drop_to = tail.len() - ZMX_SCREEN_CAPTURE_TAIL_BYTES;
                    tail.drain(..drop_to);
                }
            }
        }
        if keep {
            return Ok(ZmxHistoryCapture {
                text: zmx_screen_capture_tail_text(tail, payload_len),
                truncated: false,
            });
        }
        if tag == ZMX_IPC_TAG_INFO && expected_tag == ZMX_IPC_TAG_CAPTURE {
            let mut request = [0_u8; ZMX_IPC_HEADER_BYTES + 1];
            request[0] = ZMX_IPC_TAG_HISTORY;
            request[1..5].copy_from_slice(&1_u32.to_le_bytes());
            request[ZMX_IPC_HEADER_BYTES] = format;
            stream.write_all(&request).map_err(|error| {
                format!("zmx legacy screen capture could not be requested: {error}")
            })?;
            expected_tag = ZMX_IPC_TAG_HISTORY;
        }
        // Any other tag on this connection is a message we did not ask for
        // (or one a newer daemon volunteers); skip it and keep reading.
    }
}

/// Decodes a retained tail into text, starting at the first clean line
/// boundary so a reader never sees half of a dropped line.
#[cfg(unix)]
fn zmx_screen_capture_tail_text(tail: Vec<u8>, payload_len: usize) -> String {
    let clipped = tail.len() < payload_len;
    let mut text = String::from_utf8_lossy(&tail).into_owned();
    if clipped {
        if let Some(first_newline) = text.find('\n') {
            text.drain(..=first_newline);
        }
    }
    text
}

/*
CDXC:AppShots 2026-08-22:
On Windows every zmx daemon lives inside WSL, so its session socket sits in the
WSL filesystem namespace and a Windows process has no AF_UNIX path that reaches
it.
Windows runs scoped `zmx history` through the existing WSL command wrapper, reverting to History only for an explicitly unsupported daemon.
That path keeps the head at the stdout byte cap, so an oversized capture still reports `truncated` and screen-state readers decline to classify it.
*/
#[cfg(not(unix))]
pub(crate) fn read_zmx_session_screen_capture(zmx_name: &str) -> Result<ZmxHistoryCapture, String> {
    read_zmx_session_screen_capture_format(zmx_name, false)
}

#[cfg(not(unix))]
pub(crate) fn read_zmx_session_screen_capture_vt(
    zmx_name: &str,
) -> Result<ZmxHistoryCapture, String> {
    read_zmx_session_screen_capture_format(zmx_name, true)
}

#[cfg(not(unix))]
fn read_zmx_session_screen_capture_format(
    zmx_name: &str,
    vt: bool,
) -> Result<ZmxHistoryCapture, String> {
    let zmx = require_bundled_zmx()?;
    let result = run_zmx_interaction_command(
        build_zmx_screen_capture_command(
            zmx_name,
            &zmx.executable_path,
            ZMX_SCREEN_CAPTURE_SCROLLBACK_ROWS,
            vt,
        ),
        ZmxCommandOptions {
            allow_stdout_truncation: true,
            stdout_limit_bytes: Some(GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES),
            ..ZmxCommandOptions::default()
        },
    )
    .map_err(|error| match error {
        ZmxEndpointError::DependencyUnavailable(message) => message,
        ZmxEndpointError::Domain(error) => error.message,
    })?;
    Ok(ZmxHistoryCapture {
        truncated: result.stdout_truncated,
        text: result.stdout,
    })
}

/*
CDXC:SessionChatTerminalActivity 2026-09-04 WHY:
Whether anyone is looking at this session's terminal right now. Every viewer
(the desktop terminal pane, the web and mobile terminals) is a zmx client that
announces its visible, chat, or parked state through ZMX_VISIBLE / ZMX_CHAT /
ZMX_HIDDEN sequences, and `zmx grid` reports that state per client, so "no visible client"
is the daemon's own answer, not a guess from the chat view's state. `None`
when the daemon could not be asked.
*/
pub(crate) fn zmx_session_has_visible_client(zmx_name: &str) -> Option<bool> {
    zmx_grid_has_visible_client(&read_zmx_grid(zmx_name)?)
}

pub(crate) fn zmx_session_has_only_chat_viewers(zmx_name: &str) -> Option<bool> {
    let (visible, chat) = zmx_grid_visibility(&read_zmx_grid(zmx_name)?)?;
    Some(chat && !visible)
}

fn read_zmx_grid(zmx_name: &str) -> Option<String> {
    let zmx = crate::toolchain::require_bundled_zmx().ok()?;
    let result = crate::zmx::run_zmx_interaction_command(
        crate::zmx::build_zmx_grid_command(zmx_name, &zmx.executable_path),
        crate::zmx::ZmxCommandOptions::default(),
    )
    .ok()?;
    if result.exit_code != 0 {
        return None;
    }
    Some(result.stdout)
}

/// Reads zmx's current per-client visibility contract.
pub(crate) fn zmx_grid_has_visible_client(grid_json: &str) -> Option<bool> {
    zmx_grid_visibility(grid_json).map(|(visible, _)| visible)
}

/// CDXC:Zmx 2026-09-06 WHY:
/// GridInfo reports `state`, not the retired `hidden` boolean. Treating a missing `hidden` as invisible allowed chat to dismiss terminal UI that another client was using. Unknown client states must prevent automatic dismissal.
/// SEE-ALSO: .dependencies/zmx/src/loop.zig handleGridInfo.
fn zmx_grid_visibility(grid_json: &str) -> Option<(bool, bool)> {
    let grid: serde_json::Value = serde_json::from_str(grid_json.trim()).ok()?;
    let clients = grid.get("clients")?.as_array()?;
    let mut visible = false;
    let mut chat = false;
    for client in clients {
        match client.get("state")?.as_str()? {
            "visible" => visible = true,
            "chat" => chat = true,
            "parked" => {}
            _ => return None,
        }
    }
    Some((visible, chat))
}
