//! A write that goes to a session's terminal only when a fresh read of its screen allows it, with
//! the read and the write on one connection to the daemon so nothing the program draws in between
//! goes unseen for longer than the check itself takes.

#[cfg(unix)]
use std::io::{Read, Write};

#[cfg(unix)]
use super::ipc_frame::{parse_zmx_ipc_header, zmx_ipc_header, ZMX_IPC_HEADER_BYTES};
#[cfg(unix)]
use super::screen_capture::{
    read_zmx_screen_capture_reply, request_zmx_session_screen_capture,
    ZMX_IPC_HISTORY_FORMAT_PLAIN, ZMX_SCREEN_CAPTURE_SCROLLBACK_ROWS,
};

/// `ipc.Tag.SendAcked`: `Send` plus a receipt.
#[cfg(unix)]
const ZMX_IPC_TAG_SEND_ACKED: u8 = 205;
/// `ipc.Tag.SendAck`: the receipt, one `SendAckStatus` byte.
#[cfg(unix)]
const ZMX_IPC_TAG_SEND_ACK: u8 = 206;
/// `ipc.Tag.LabelGet` and its reply `LabelData`: the ordering barrier after `SendAcked`. Not
/// `Info`: the capture's own `Info` barrier is still unanswered on this connection when the input
/// goes out, and taking its reply for this one wrote every message twice (seen live 2026-11-12).
#[cfg(unix)]
const ZMX_IPC_TAG_LABEL_GET: u8 = 14;
#[cfg(unix)]
const ZMX_IPC_TAG_LABEL_DATA: u8 = 17;
/// `ipc.Tag.Unsupported`: a daemon's answer to a tag it does not know.
#[cfg(unix)]
const ZMX_IPC_TAG_UNSUPPORTED: u8 = 255;
/// `ipc.SendAckStatus.queued`.
#[cfg(unix)]
const ZMX_SEND_ACK_QUEUED: u8 = 0;
/// `util.HistoryFormat.vt`.
#[cfg(unix)]
const ZMX_IPC_HISTORY_FORMAT_VT: u8 = 1;

/// What `send_after_screen_check` did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ZmxCheckedSend {
    /// The check passed and the daemon took the bytes for the program.
    Sent,
    /// The check refused the screen it read, with its reason; nothing was written.
    Refused(String),
    /// The check passed and the terminal did not take the bytes.
    NotTaken(String),
    /// The session's daemon could not be reached at all: nothing was read or written.
    Unreachable(String),
}

/// Reads the session's screen (`vt` keeps its styles), hands it to `check` (`None` when it could not
/// be read whole), and writes `payload` only when `check` allows it.
///
/// CDXC:SessionChat 2026-11-12 WHY: GitHub #555: an approval that appeared after a send's last
/// screen read took the Return meant for the message. A write through `zmx send` starts a process
/// after that read, tens of milliseconds in which the screen can change unseen. Here the capture
/// and the write share one connection, and the daemon handles a client's messages in order, so the
/// write follows the read by no more than the check's own time.
/// SEE-ALSO: .dependencies/zmx/src/ipc.zig (`SendAcked`, `SendAckStatus`), server/src/session_chat_send/guarded_write.rs.
#[cfg(unix)]
pub(crate) fn send_after_screen_check(
    zmx_name: &str,
    vt: bool,
    payload: &str,
    check: impl FnOnce(Option<&str>) -> Result<(), String>,
) -> Result<ZmxCheckedSend, String> {
    let format = if vt {
        ZMX_IPC_HISTORY_FORMAT_VT
    } else {
        ZMX_IPC_HISTORY_FORMAT_PLAIN
    };
    let mut stream = match request_zmx_session_screen_capture(
        zmx_name,
        format,
        ZMX_SCREEN_CAPTURE_SCROLLBACK_ROWS,
    ) {
        Ok(stream) => stream,
        // No daemon to read from is no daemon to write to either.
        Err(error) => return Ok(ZmxCheckedSend::Unreachable(error)),
    };
    let capture = read_zmx_screen_capture_reply(&mut stream, format);
    let screen = capture
        .as_ref()
        .ok()
        .filter(|capture| !capture.truncated)
        .map(|capture| capture.text.as_str());
    if let Err(reason) = check(screen) {
        return Ok(ZmxCheckedSend::Refused(reason));
    }
    // No bytes: the caller only wanted the check.
    if payload.is_empty() {
        return Ok(ZmxCheckedSend::Sent);
    }
    if capture.is_err() {
        // A reply that broke off leaves the connection mid-frame: write through `zmx send`.
        return Ok(match super::session_chat_zmx_write(zmx_name, payload)? {
            0 => ZmxCheckedSend::Sent,
            code => ZmxCheckedSend::NotTaken(format!("zmx send exited with {code}")),
        });
    }
    // Long input goes in pieces the daemon's input buffer can take (input_chunks.rs), each with
    // its own receipt; a piece the full buffer dropped is offered again once the agent has read
    // what is queued.
    let chunks = super::input_chunks::zmx_input_chunks(payload);
    let mut sent = 0;
    let mut stalled_since = None;
    let mut index = 0;
    while index < chunks.len() {
        let chunk = chunks[index];
        let receipt =
            match write_input_chunk(&mut stream, chunk, sent == 0 && stalled_since.is_none()) {
                Ok(receipt) => receipt,
                Err(error) => return Err(close_cut_paste(zmx_name, payload, sent, error)),
            };
        match receipt {
            Some(ZMX_SEND_ACK_QUEUED) => {
                sent += chunk.len();
                index += 1;
                stalled_since = None;
            }
            Some(ZMX_SEND_ACK_BUFFER_FULL) => {
                let since = *stalled_since.get_or_insert_with(std::time::Instant::now);
                if since.elapsed() >= super::input_chunks::ZMX_INPUT_STALL_LIMIT {
                    return Ok(ZmxCheckedSend::NotTaken(close_cut_paste(
                        zmx_name,
                        payload,
                        sent,
                        "the session's input buffer stayed full".to_string(),
                    )));
                }
                std::thread::sleep(super::input_chunks::ZMX_INPUT_DRAIN_WAIT);
            }
            Some(status) => {
                return Ok(ZmxCheckedSend::NotTaken(close_cut_paste(
                    zmx_name,
                    payload,
                    sent,
                    format!("the session refused the input (receipt {status})"),
                )));
            }
            // A daemon too old for `SendAcked` queued nothing: `zmx send` probes it itself.
            None => {
                return Ok(match super::session_chat_zmx_write(zmx_name, payload)? {
                    0 => ZmxCheckedSend::Sent,
                    code => ZmxCheckedSend::NotTaken(format!("zmx send exited with {code}")),
                });
            }
        }
    }
    Ok(ZmxCheckedSend::Sent)
}

/// `ipc.SendAckStatus.dropped_pty_buffer_full`.
#[cfg(unix)]
const ZMX_SEND_ACK_BUFFER_FULL: u8 = 1;

/// Writes one piece of input as `SendAcked` and answers its receipt's status, or `None` when the
/// daemon is too old for `SendAcked`.
///
/// The first piece is followed by a barrier: a daemon too old for `SendAcked` drops it (answering
/// `Unsupported`, or nothing) and answers the barrier first, which means nothing was queued
/// (GitHub #555 review). Any other reply (the capture's `Info`, output broadcasts, a later
/// piece's barrier answer) is skipped.
#[cfg(unix)]
fn write_input_chunk(
    stream: &mut std::os::unix::net::UnixStream,
    chunk: &str,
    with_barrier: bool,
) -> Result<Option<u8>, String> {
    let mut frame = zmx_ipc_header(ZMX_IPC_TAG_SEND_ACKED, chunk.len() as u32).to_vec();
    frame.extend_from_slice(chunk.as_bytes());
    if with_barrier {
        frame.extend_from_slice(&zmx_ipc_header(ZMX_IPC_TAG_LABEL_GET, 0));
    }
    stream
        .write_all(&frame)
        .map_err(|error| format!("zmx session input could not be written: {error}"))?;
    let mut header = [0_u8; ZMX_IPC_HEADER_BYTES];
    loop {
        stream
            .read_exact(&mut header)
            .map_err(|error| format!("zmx session input receipt was unreadable: {error}"))?;
        let (tag, len) = parse_zmx_ipc_header(&header);
        let mut body = vec![0_u8; len];
        stream
            .read_exact(&mut body)
            .map_err(|error| format!("zmx session input receipt was unreadable: {error}"))?;
        if tag == ZMX_IPC_TAG_SEND_ACK {
            return Ok(Some(body.first().copied().unwrap_or(u8::MAX)));
        }
        if with_barrier && matches!(tag, ZMX_IPC_TAG_LABEL_DATA | ZMX_IPC_TAG_UNSUPPORTED) {
            return Ok(None);
        }
    }
}

/// Ends a bracketed paste that broke off after `sent` bytes, so the agent does not take the next
/// send's keys as pasted text, and answers `error`.
#[cfg(unix)]
fn close_cut_paste(zmx_name: &str, payload: &str, sent: usize, error: String) -> String {
    if let Some(end) = super::input_chunks::unfinished_paste_end(payload, sent) {
        let _ = super::session_chat_zmx_write(zmx_name, end);
    }
    error
}

/// Windows reaches its daemons through a command, so the read and the write are two commands.
#[cfg(not(unix))]
pub(crate) fn send_after_screen_check(
    zmx_name: &str,
    vt: bool,
    payload: &str,
    check: impl FnOnce(Option<&str>) -> Result<(), String>,
) -> Result<ZmxCheckedSend, String> {
    let capture = if vt {
        // A styled read is cut at the stdout cap long before a plain one; a cut read is read
        // again as plain text, which the checks also understand (GitHub #555 review).
        super::read_zmx_session_screen_capture_vt(zmx_name)
            .ok()
            .filter(|capture| !capture.truncated)
            .map_or_else(|| super::read_zmx_session_screen_capture(zmx_name), Ok)
    } else {
        super::read_zmx_session_screen_capture(zmx_name)
    };
    let screen = capture
        .as_ref()
        .ok()
        .filter(|capture| !capture.truncated)
        .map(|capture| capture.text.as_str());
    if let Err(reason) = check(screen) {
        return Ok(ZmxCheckedSend::Refused(reason));
    }
    if payload.is_empty() {
        return Ok(ZmxCheckedSend::Sent);
    }
    Ok(match super::session_chat_zmx_write(zmx_name, payload)? {
        0 => ZmxCheckedSend::Sent,
        code => ZmxCheckedSend::NotTaken(format!("zmx send exited with {code}")),
    })
}
