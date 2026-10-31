//! A scripted terminal for the model picker's tests: plain captures and writes for a session name
//! opened here are answered on the test's own thread instead of going to zmx.

use std::cell::RefCell;
use std::collections::HashMap;

use super::constants::{AGENT_TUI_CLEAR_INPUT_LINE, BRACKETED_PASTE_END, BRACKETED_PASTE_START};

/// One terminal's input box, and every write sent to it.
#[derive(Clone, Debug, Default)]
pub(crate) struct TestTerminal {
    /// What the input box shows.
    pub(crate) input: String,
    /// What the input box shows after Enter: OMP's completion of the command, or a line typed in
    /// the terminal while the picker waited. No answer is ever printed.
    pub(crate) after_enter: String,
    /// Refuse every paste, leaving the input as it is.
    pub(crate) refuse_paste: bool,
    /// Every write, in order.
    pub(crate) writes: Vec<String>,
}

thread_local! {
    static TERMINALS: RefCell<HashMap<String, TestTerminal>> = RefCell::new(HashMap::new());
}

/// Answers captures and writes for `name` from `terminal`, on this thread only.
pub(crate) fn open(name: &str, terminal: TestTerminal) {
    TERMINALS.with(|terminals| {
        terminals.borrow_mut().insert(name.to_string(), terminal);
    });
}

/// The terminal opened as `name`, as it is now.
pub(crate) fn terminal(name: &str) -> TestTerminal {
    TERMINALS.with(|terminals| terminals.borrow()[name].clone())
}

/// The screen: the input in OMP's `pi` composer shape (a padded row between two rules, which Pi's
/// input reader reads too) over a status line. `None` when `name` is not a test terminal.
pub(super) fn capture(name: &str) -> Option<String> {
    TERMINALS.with(|terminals| {
        terminals.borrow().get(name).map(|terminal| {
            let rule = "\u{2500}".repeat(40);
            format!(
                " Ready when you are.\n\n{rule}\n {}\n{rule}\n \u{2b22} Opus 5.5 \u{b7} \u{25d2} high",
                terminal.input
            )
        })
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
        } else if payload == AGENT_TUI_CLEAR_INPUT_LINE {
            terminal.input.clear();
        }
        Some(Ok(()))
    })
}
