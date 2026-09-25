//! The session's terminal screen, read only when the user asks for it, ported from
//! `packages/shared/session-chat-controller/native-terminal-tail.ts`.
//!
//! Both readers are the ones React had (the Terminal View hover and the `composerNotReady` refusal
//! card), with their two rules kept: `unknown` is not "not ready", and a failed hover read keeps
//! the last verdict.

use serde_json::Value;

use crate::document::{TerminalTail, TerminalTailNotice};
use crate::extras::terminal_tail_format::format_terminal_tail_preview;
use crate::state::TerminalTailState;

/// `retire`: the refusal is gone, so the expanded excerpt under it goes with it.
pub fn retire(tail: &mut TerminalTailState) {
    if !tail.notice_open && tail.notice_tail.is_none() && tail.notice_error.is_none() {
        return;
    }
    tail.notice_open = false;
    tail.notice_tail = None;
    tail.notice_error = None;
    tail.notice_loading = false;
}

/// `project`: the `terminalTail` document value.
pub fn project(tail: &TerminalTailState) -> TerminalTail {
    /*
    Only a MEASURED verdict tints the button: `unknown`, an uncaptured screen and the time before
    the first hover all stay neutral, because the daemon fails open on `unknown` and a red button
    there would accuse a session that sends fine.
    */
    let readiness = tail
        .tail
        .as_ref()
        .filter(|read| captured(read))
        .and_then(|read| read.get("composerState").and_then(Value::as_str))
        .filter(|state| *state != "unknown")
        .map(str::to_string);
    TerminalTail {
        preview: match tail.tail.as_ref().filter(|read| captured(read)) {
            Some(read) => format_terminal_tail_preview(&lines(read)),
            None => String::new(),
        },
        reason: match readiness.as_deref() {
            Some("notReady") => tail
                .tail
                .as_ref()
                .and_then(|read| read.get("reason"))
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        },
        notice: TerminalTailNotice {
            open: tail.notice_open,
            loading: tail.notice_loading,
            error: tail.notice_error.clone(),
            excerpt: excerpt_of(tail.notice_tail.as_ref()),
            empty: if tail.notice_open && !tail.notice_loading && tail.notice_error.is_none() {
                empty_copy(tail.notice_tail.as_ref())
            } else {
                None
            },
        },
        readiness,
    }
}

/// `excerptOf`: the captured rows verbatim, or "".
fn excerpt_of(tail: Option<&Value>) -> String {
    match tail.filter(|read| captured(read)) {
        Some(read) => {
            let lines = lines(read);
            if lines.is_empty() {
                String::new()
            } else {
                lines.join("\n")
            }
        }
        None => String::new(),
    }
}

/// `emptyCopy`: why the sheet has nothing to show, or `null` when it does.
fn empty_copy(tail: Option<&Value>) -> Option<String> {
    if !excerpt_of(tail).is_empty() {
        return None;
    }
    Some(
        if tail.is_some_and(|read| !captured(read)) {
            "Ghostex could not read this session\u{2019}s terminal screen."
        } else {
            "The terminal screen is empty."
        }
        .to_string(),
    )
}

/// `tail.captured === true`.
fn captured(tail: &Value) -> bool {
    tail.get("captured") == Some(&Value::Bool(true))
}

/// `tail.lines`, as the strings the formatter takes.
fn lines(tail: &Value) -> Vec<String> {
    tail.get("lines")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}
