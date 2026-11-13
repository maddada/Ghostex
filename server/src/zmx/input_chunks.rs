//! Long terminal input goes to a session's daemon in pieces its input buffer can take.

use std::time::Duration;

/// The most input one write hands the daemon.
///
/// CDXC:SessionChat 2026-11-13 WHY: a 350 KB crash report pasted into a coordinator's chat failed
/// twice with "The agent's terminal didn't take your message": the zmx daemon queues a session's
/// input in a buffer of at most 256 KiB (`PTY_WRITE_BUF_MAX`) and drops any write that would pass
/// it whole, answering `dropped_pty_buffer_full`, so one write of the whole paste could never land,
/// however quickly the agent read. Input is written in pieces a quarter of that size, and a piece
/// the buffer cannot take yet is written again once the agent has read what is queued; wmx (native
/// Windows) refuses one `send` over 1 MiB, which these pieces stay under too.
/// SEE-ALSO: .dependencies/zmx/src/loop.zig (`PTY_WRITE_BUF_MAX`, `queuePtyInputChecked`),
/// .dependencies/wmx/src/cli.rs (`send`), server/src/zmx/guarded_send.rs.
pub(crate) const ZMX_INPUT_CHUNK_BYTES: usize = 64 * 1024;

/// How long a piece waits before it is offered again to a full input buffer.
pub(crate) const ZMX_INPUT_DRAIN_WAIT: Duration = Duration::from_millis(25);

/// How long the input buffer may stay full before the write gives up: the program in the session
/// stopped reading its input.
pub(crate) const ZMX_INPUT_STALL_LIMIT: Duration = Duration::from_secs(30);

/// What zmx prints when the daemon dropped a `zmx send` because its input buffer was full
/// (`sendToPty` in .dependencies/zmx/src/main.zig).
pub(crate) const ZMX_SEND_BUFFER_FULL_MESSAGE: &str =
    "the program in the session stopped reading its input";

const BRACKETED_PASTE_START: &str = "\u{1b}[200~";
const BRACKETED_PASTE_END: &str = "\u{1b}[201~";

/// `payload` cut into pieces of at most `ZMX_INPUT_CHUNK_BYTES`, each ending after a line break
/// when the piece has one, never inside a character or an escape sequence.
pub(crate) fn zmx_input_chunks(payload: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut rest = payload;
    while rest.len() > ZMX_INPUT_CHUNK_BYTES {
        let cut = chunk_end(rest);
        chunks.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    if !rest.is_empty() || chunks.is_empty() {
        chunks.push(rest);
    }
    chunks
}

fn chunk_end(rest: &str) -> usize {
    let mut limit = ZMX_INPUT_CHUNK_BYTES;
    while !rest.is_char_boundary(limit) {
        limit -= 1;
    }
    let window = &rest.as_bytes()[..limit];
    if let Some(newline) = window.iter().rposition(|&byte| byte == b'\n') {
        if newline >= limit / 2 {
            return newline + 1;
        }
    }
    // An escape sequence is short: one that starts in the last few bytes goes to the next piece.
    let tail = limit - 16;
    window[tail..]
        .iter()
        .rposition(|&byte| byte == 0x1b)
        .map_or(limit, |at| tail + at)
}

/// The bytes that end a bracketed paste `payload` opened when only its first `sent` bytes reached
/// the terminal, so a paste cut short does not leave the agent reading every later key as pasted
/// text.
pub(crate) fn unfinished_paste_end(payload: &str, sent: usize) -> Option<&'static str> {
    (sent > 0
        && sent < payload.len()
        && payload.starts_with(BRACKETED_PASTE_START)
        && payload.ends_with(BRACKETED_PASTE_END))
    .then_some(BRACKETED_PASTE_END)
}
