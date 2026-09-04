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
    /// The capture lost its TAIL — the live screen — so screen-state readers
    /// must not draw conclusions from it. The in-process screen capture keeps
    /// the tail by construction, so only the spawned `/api/readSessionText`
    /// path can still set this.
    pub truncated: bool,
}

/*
CDXC:AppShots 2026-08-22:
Screen-state readers (chat model/effort pills, terminal notices, the compaction
activity row, the send-delivery watchdog) talk the zmx IPC socket directly
instead of spawning `zsh -lc … zmx history`. Measured on this machine against
live daemons, p50 per capture:

    session scrollback   zsh -lc + zmx    direct socket
    871 B                       5.67 ms          0.10 ms
    193 KB                     13.74 ms          1.67 ms
    686 KB                     11.25 ms          6.20 ms

The spawn was the whole cost at small sizes: `command_shell()` runs `zsh -lc`,
so every capture paid for a LOGIN shell sourcing the user's profile, plus an
exec of the zmx binary, just to copy bytes off a socket the daemon was already
listening on. At large sizes the remaining time is the daemon's own
`serializeTerminal` (5.5 ms of the 6.2 ms above is time-to-first-byte), which
no client-side change can touch — cutting that needs a tail-scoped IPC tag in
zmx itself.

Two behavior notes, both improvements over the spawn:

  - `/api/readSessionText` capped stdout at 256 KiB and kept the HEAD, so any
    session with more than 256 KiB of scrollback reported `truncated` and every
    screen-state reader correctly refused to conclude anything — those sessions
    never showed model/effort pills at all. This path keeps the TAIL instead,
    which is the only part any reader looks at (the widest window is 60
    non-blank lines), so scrollback size stops deciding whether detection works.
  - the public `/api/readSessionText` endpoint, the CLI, and automations still
    go through the spawned path unchanged: they are whole-history consumers,
    not screen-state readers.

The wire format is frozen (see .dependencies/zmx/src/ipc.zig): 8-byte header of a packed
`struct { tag: u8, len: u32 }` — one tag byte, four little-endian length bytes,
three bytes of backing-integer padding — followed by `len` payload bytes.
*/

/// `ipc.Tag.History`.
#[cfg(unix)]
const ZMX_IPC_TAG_HISTORY: u8 = 8;
/// `@sizeOf(ipc.Header)`: a packed `struct { u8, u32 }` backs to `u40`, which
/// rounds up to 8 bytes. The top three bytes are padding on both ends.
#[cfg(unix)]
const ZMX_IPC_HEADER_BYTES: usize = 8;
/// `util.HistoryFormat.plain`.
#[cfg(unix)]
const ZMX_IPC_HISTORY_FORMAT_PLAIN: u8 = 0;
/// Matches the 5s poll `zmx history` uses before it gives up on the daemon.
#[cfg(unix)]
const ZMX_SCREEN_CAPTURE_TIMEOUT: Duration = Duration::from_millis(5_000);
/// Scrollback TAIL retained by a screen capture. The widest consumer window is
/// 60 non-blank lines, so this is orders of magnitude more than any reader
/// needs; it exists to bound memory against a 10 MB daemon scrollback.
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
/// answers `History` through Ghostty's `TerminalFormatter`, which serializes
/// only the screen currently in use: the primary screen together with the tail
/// of its scrollback, or — while a full-screen TUI such as Claude Code holds
/// the alternate screen — that grid alone, since Ghostty gives the alternate
/// screen no scrollback. Primary-screen history is never mixed into an
/// alternate-screen capture. See `CDXC:AppShots`.
#[cfg(unix)]
pub(crate) fn read_zmx_session_screen_capture(zmx_name: &str) -> Result<ZmxHistoryCapture, String> {
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

    let mut request = [0_u8; ZMX_IPC_HEADER_BYTES + 1];
    request[0] = ZMX_IPC_TAG_HISTORY;
    request[1..5].copy_from_slice(&1_u32.to_le_bytes());
    request[ZMX_IPC_HEADER_BYTES] = ZMX_IPC_HISTORY_FORMAT_PLAIN;
    stream
        .write_all(&request)
        .map_err(|error| format!("zmx session screen capture could not be requested: {error}"))?;

    read_zmx_screen_capture_reply(&mut stream)
}

/// Drains one `History` reply, retaining only the last
/// `ZMX_SCREEN_CAPTURE_TAIL_BYTES` of payload so a huge scrollback costs
/// bounded memory here regardless of what the daemon serialized.
#[cfg(unix)]
fn read_zmx_screen_capture_reply(
    stream: &mut std::os::unix::net::UnixStream,
) -> Result<ZmxHistoryCapture, String> {
    let mut header = [0_u8; ZMX_IPC_HEADER_BYTES];
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        stream.read_exact(&mut header).map_err(|error| {
            format!("zmx session screen capture reply header was unreadable: {error}")
        })?;
        let tag = header[0];
        let payload_len = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;

        let mut remaining = payload_len;
        let mut tail: Vec<u8> = Vec::new();
        let keep = tag == ZMX_IPC_TAG_HISTORY;
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
it. The direct read above is therefore Unix-only, and Windows keeps running
`zmx history` through the same WSL command wrapper every other zmx interaction
uses. That path caps stdout and keeps the HEAD, so a session with more
scrollback than the cap reports `truncated` and screen-state readers correctly
decline to conclude anything from it — the behaviour Windows already had.
*/
#[cfg(not(unix))]
pub(crate) fn read_zmx_session_screen_capture(zmx_name: &str) -> Result<ZmxHistoryCapture, String> {
    let zmx = require_bundled_zmx()?;
    let result = run_zmx_interaction_command(
        build_zmx_history_command(zmx_name, &zmx.executable_path),
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
announces itself hidden or visible through the ZMX_HIDDEN / ZMX_VISIBLE
sequences, and `zmx grid` reports that flag per client, so "no visible client"
is the daemon's own answer, not a guess from the chat view's state. `None`
when the daemon could not be asked.
*/
pub(crate) fn zmx_session_has_visible_client(zmx_name: &str) -> Option<bool> {
    let zmx = crate::toolchain::require_bundled_zmx().ok()?;
    let result = crate::zmx::run_zmx_interaction_command(
        crate::zmx::build_zmx_grid_command(zmx_name, &zmx.executable_path),
        crate::zmx::ZmxCommandOptions::default(),
    )
    .ok()?;
    if result.exit_code != 0 {
        return None;
    }
    zmx_grid_has_visible_client(&result.stdout)
}

/// Reads `zmx grid` JSON: any client that is not hidden is a visible one.
pub(crate) fn zmx_grid_has_visible_client(grid_json: &str) -> Option<bool> {
    let grid: serde_json::Value = serde_json::from_str(grid_json.trim()).ok()?;
    let clients = grid.get("clients")?.as_array()?;
    Some(clients.iter().any(|client| {
        client
            .get("hidden")
            .and_then(serde_json::Value::as_bool)
            .is_some_and(|hidden| !hidden)
    }))
}
