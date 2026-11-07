//! A scripted terminal for the send path's tests: captures (plain and VT) and writes for a
//! session name opened here are answered on the test's own thread instead of going to zmx.

use std::cell::RefCell;
use std::collections::HashMap;

use super::constants::{AGENT_TUI_CLEAR_INPUT_LINE, BRACKETED_PASTE_END, BRACKETED_PASTE_START};

/// One terminal's screen, input box, and every write sent to it.
#[derive(Clone, Debug, Default)]
pub(crate) struct TestTerminal {
    /// `None` draws the input box; `Some` returns the fixed screen, with `Some(None)` unreadable.
    pub(crate) screen: Option<Option<String>>,
    /// How often the screen was read.
    pub(crate) captures: usize,
    /// What the input box shows.
    pub(crate) input: String,
    /// What the input box shows after Enter: OMP's completion of the command, or a line typed in
    /// the terminal while the picker waited. No answer is ever printed.
    pub(crate) after_enter: String,
    /// Refuse every paste, leaving the input as it is.
    pub(crate) refuse_paste: bool,
    /// The agent's transcript, and what each Enter appends to it, in order: OMP's records of the
    /// change it applied (an empty entry appends nothing).
    pub(crate) transcript: Option<(std::path::PathBuf, Vec<String>)>,
    /// Every write, in order.
    pub(crate) writes: Vec<String>,
    /// The screen the first write leaves behind: an agent's dialog closing on its answer.
    pub(crate) screen_after_write: Option<String>,
}

thread_local! {
    static TERMINALS: RefCell<HashMap<String, TestTerminal>> = RefCell::new(HashMap::new());
}

/// Answers captures and writes for `name` from `terminal`, on this thread only.
/// CDXC:SessionChat 2026-11-03 WHY: a test of a card key's bytes alone missed a key written as the wrong key and a key written to a screen that no longer showed the card. Captures and writes are answered at the send worker's zmx boundary so tests drive the answer function, the send queue and its worker. Per thread for the reason `assume_gh_cli_available_for_test` is: the worker runs on the test's thread under `#[tokio::test]`.
pub(crate) fn open(name: &str, terminal: TestTerminal) {
    TERMINALS.with(|terminals| {
        terminals.borrow_mut().insert(name.to_string(), terminal);
    });
}

/// The terminal opened as `name`, as it is now.
pub(crate) fn terminal(name: &str) -> TestTerminal {
    TERMINALS.with(|terminals| terminals.borrow()[name].clone())
}

/// The fixed screen or the input in OMP's `pi` composer shape (a padded row between two rules,
/// which Pi's input reader reads too) over a status line. The outer `None` means `name` is not a
/// test terminal; `Some(None)` means its screen could not be read.
pub(super) fn capture(name: &str) -> Option<Option<String>> {
    TERMINALS.with(|terminals| {
        let mut terminals = terminals.borrow_mut();
        let terminal = terminals.get_mut(name)?;
        terminal.captures += 1;
        if let Some(screen) = &terminal.screen {
            return Some(screen.clone());
        }
        let rule = "\u{2500}".repeat(40);
        Some(Some(format!(
            " Ready when you are.\n\n{rule}\n {}\n{rule}\n \u{2b22} Opus 5.5 \u{b7} \u{25d2} high",
            terminal.input
        )))
    })
}

/// Applies `payload` to the terminal opened as `name`: a bracketed paste fills the input, Enter
/// replaces it with `after_enter`, and the clear-line key empties it. `None` when `name` is not a
/// test terminal.
pub(super) fn write(name: &str, payload: &str) -> Option<Result<(), String>> {
    TERMINALS.with(|terminals| {
        let mut terminals = terminals.borrow_mut();
        let terminal = terminals.get_mut(name)?;
        terminal.writes.push(payload.to_string());
        if let Some(screen) = terminal.screen_after_write.take() {
            terminal.screen = Some(Some(screen));
        }
        if let Some(text) = payload
            .strip_prefix(BRACKETED_PASTE_START)
            .and_then(|rest| rest.strip_suffix(BRACKETED_PASTE_END))
        {
            if terminal.refuse_paste {
                return Some(Err("The test terminal refused the paste.".to_string()));
            }
            terminal.input = text.to_string();
        } else if payload == "\r" {
            terminal.input = terminal.after_enter.clone();
            if let Some((path, records)) = terminal.transcript.as_mut() {
                if !records.is_empty() {
                    let record = records.remove(0);
                    if !record.is_empty() {
                        use std::io::Write;
                        let mut file = std::fs::OpenOptions::new()
                            .append(true)
                            .open(path)
                            .expect("open the test transcript");
                        writeln!(file, "{record}").expect("append to the test transcript");
                    }
                }
            }
        } else if payload == AGENT_TUI_CLEAR_INPUT_LINE {
            terminal.input.clear();
        }
        Some(Ok(()))
    })
}
