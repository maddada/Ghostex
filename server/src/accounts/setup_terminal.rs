//! The terminal an account login runs in: Settings reads its screen as plain text, and the login gets the replies to the terminal queries it waits on. The Slack cloud runner's `claude --cloud` start (server/src/cloud_runner.rs) runs in one too.

use std::io::Write;

pub(crate) const ROWS: u16 = 30;
pub(crate) const COLS: u16 = 160;

/// CDXC:AgentProviders 2026-10-03 WHY:
/// On Windows the login runs in a ConPTY (portable-pty opens it with PSEUDOCONSOLE_INHERIT_CURSOR), which sends `ESC[6n` and starts nothing until the host replies with the cursor position; a raw byte log never replied, so Settings showed `[6n` and the login hung. ConPTY also redraws with cursor moves instead of newlines, so Settings shows the emulated screen, never the raw stream.
pub(crate) struct LoginTerminal {
    parser: vt100::Parser<Queries>,
}

#[derive(Default)]
struct Queries {
    cursor_reports: Vec<(u16, u16)>,
}

impl vt100::Callbacks for Queries {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        // The position is read when the query arrives, before any later output in the same chunk moves the cursor.
        if c == 'n' && i1.is_none() && matches!(params, [&[6]]) {
            self.cursor_reports.push(screen.cursor_position());
        }
    }
}

impl LoginTerminal {
    pub(crate) fn new() -> Self {
        Self {
            parser: vt100::Parser::new_with_callbacks(ROWS, COLS, 0, Queries::default()),
        }
    }

    /// Feeds login output and returns the bytes the terminal owes the login in reply.
    pub(crate) fn process(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.process(bytes);
        let mut replies = Vec::new();
        for (row, col) in self.parser.callbacks_mut().cursor_reports.drain(..) {
            let _ = write!(replies, "\x1b[{};{}R", row + 1, col + 1);
        }
        replies
    }

    /// The screen as plain text, with wrapped rows joined so long sign-in URLs stay whole.
    pub(crate) fn text(&self) -> String {
        self.parser.screen().contents()
    }
}
