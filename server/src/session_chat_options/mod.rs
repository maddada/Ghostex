//! Session Chat model/effort/mode detection, split by concern. Every submodule is glob
//! re-exported here, so `crate::session_chat_options::*` paths are unchanged.

/*
CDXC:AgentScreenDetection 2026-08-01:
Reads the CURRENT model / reasoning effort from agent-owned structured
transcript metadata and the session's terminal scrollback, plus Claude's
current permission mode from its footer, so the composer's option pills show
evidence instead of a catalog guess.

The agent TUIs render their state into a statusline (Claude Code) or a footer
(Codex). `zmx history` already returns that text (the live screen is part of
the history output), so detection is one bounded process spawn — no new
protocol, no agent cooperation.

Matching is SEGMENT-EXACT and case-sensitive: each scanned line is split on the
statusline delimiters (`|` for Claude's custom statusline, `·` for Codex's
footer), every segment is trimmed, and a segment only counts when the WHOLE
segment matches the grammar. Prose can therefore never false-match (an
assistant sentence mentioning "high" is one long segment), and the Codex
session title is excluded by the grammar itself.

CDXC:AgentScreenDetection 2026-09-03 WHY:
Codex's footer is a user-ordered list (`tui.status_line`), so NO segment
position carries meaning. An earlier "the first segment is the title" skip
threw away `gpt-5.6-sol high` on every config that lists
`model-with-reasoning` first, leaving the pills empty until the first turn's
transcript record. The whole-segment grammar is the only guard; a title would
have to be exactly `<model id> <effort>` to be mistaken for one.

Terminal evidence wins per option because it can reflect an idle `/model`
change before the next response. The latest Claude assistant / Codex
turn-context record fills any missing value. Nothing matched ⇒ `None` ⇒ the
field is omitted from results/frames. There is deliberately no guessing.
*/

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};

use serde_json::{json, Map, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::DomainRepository;
use crate::events::GxserverEventHub;
use crate::paths::GxserverPaths;
use crate::server::{read_runtime_text, session_observer_key, AppState, SessionChatFollowerEntry};
use crate::session_chat_follower::{is_session_chat_followable_session, session_chat_hook_working};
use crate::storage::open_gxserver_database;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

mod agent_matchers;
mod codex_permissions_memory;
mod detection;
mod detector;
mod launch_selection;
mod redetect;
mod screen_state_cache;
mod screen_text;
mod selection_sources;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use agent_matchers::*;
pub(crate) use codex_permissions_memory::*;
pub use detection::*;
pub(crate) use detector::*;
pub(crate) use launch_selection::*;
pub(crate) use redetect::*;
pub(crate) use screen_state_cache::*;
pub(crate) use screen_text::*;
pub(crate) use selection_sources::*;
pub use types::*;
