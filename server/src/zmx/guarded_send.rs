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
        Err(error) => {
            // No daemon to read from is no daemon to write to either; the check still decides
            // whether an unreadable screen may be written to, for the error it reports.
            return Ok(match check(None) {
                Err(reason) => ZmxCheckedSend::Refused(reason),
                Ok(()) => ZmxCheckedSend::NotTaken(error),
            });
        }
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
    let mut frame = zmx_ipc_header(ZMX_IPC_TAG_SEND_ACKED, payload.len() as u32).to_vec();
    frame.extend_from_slice(payload.as_bytes());
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
            return Ok(match body.first() {
                Some(&ZMX_SEND_ACK_QUEUED) => ZmxCheckedSend::Sent,
                status => ZmxCheckedSend::NotTaken(format!(
                    "the session refused the input (receipt {status:?})"
                )),
            });
        }
    }
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
        super::read_zmx_session_screen_capture_vt(zmx_name)
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
