//! Session Chat terminal notices, split by concern. Every submodule is glob re-exported
//! here, so `crate::session_chat_notice::*` paths are unchanged.

/*
CDXC:AgentScreenDetection 2026-08-19:
Chat is a transcript projection, so everything an agent TUI paints ONLY on the
screen is invisible in it: codex's expired-login banner, a workspace-trust
dialog, a usage-limit countdown, a stream error, the CLI having exited back to a
shell. A message typed into one of those screens is silently lost.

This module turns that screen state into one nullable wire field
(`terminalNotice`) carried exactly like `prompt`/`selectedOptions`. It is a PURE
classifier plus a tiny in-memory store: it never spawns a process and never
touches the filesystem. The screen text it classifies is the SAME `zmx history`
capture the model/effort detector already pays for (session_chat_options/), so
notices cost zero extra process spawns.

Matching is phrase-based rather than regex-based, for the same reason the option
grammar is hand-written: this crate deliberately carries no regex dependency
(it ships as a static musl binary to remote machines). The screen is folded to
single spaces and joined across lines first, so a wrapped TUI sentence matches
the same literal as an unwrapped one, and a `Gap` stands in for every variable
middle (`·` separators, version numbers, reasons).

Two match windows, per the researched catalog:
  - Banner-class states (an inline error/limit line, a crashed process) must
    appear in the LAST few non-blank lines — an identical line further up is
    scrollback from a problem that is already over.
  - Dialog-class states (trust, login, onboarding, update modal) own the visible
    screen, so they may appear anywhere in it. "The visible screen" is
    approximated by a wider tail window, NOT by the whole 256 KiB scrollback.

Wordings are versioned: remote machines run older agent builds, so every state
keeps a LIST of signatures ordered newest-first and the first match wins.

CDXC:Copy 2026-09-03:
User decision: Ghostex-owned user-facing copy in the desktop, web, and mobile apps uses no em dashes; use punctuation that preserves the sentence's natural reading instead.
*/

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::session_chat_options::{
    normalize_spaces, session_chat_option_agent, strip_ansi_sgr, SessionChatOptionAgent,
};

mod classify;
mod rules;
mod screen;
mod start_failure;
pub(crate) use start_failure::{account_emails_hidden, claude_slot_email};
mod types;
mod watchdog_store;

pub use classify::*;
pub use rules::*;
use screen::*;
pub use start_failure::*;
pub use types::*;
pub use watchdog_store::*;
