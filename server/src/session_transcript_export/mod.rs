//! Session transcript export, split by concern. Every submodule is glob re-exported here,
//! so `crate::session_transcript_export::*` paths are unchanged.

/*
CDXC:TranscriptExport 2026-08-20:
"Export transcript" turns a running agent session's conversation into one
markdown file so a NEW agent conversation can be seeded with it (plan
`plans/015-export-transcript.md`).

The parsers here read the provider's RAW transcript JSONL rather than the
normalized `SessionChatMessage` stream that powers chat view. That is
deliberate: chat view drops every record it cannot render (token counts, turn
context, git snapshots, developer prompts, session events), and it flattens a
tool call and its output into role-tagged blocks that no longer say WHICH tool
ran. Export classifies every record into the full section taxonomy below, so a
caller-supplied selection can later enable any of them without a second parser.

Only transcript RESOLUTION is shared with chat view
(`resolve_session_chat_transcript_path` + Claude successor adoption), because
that logic is load-bearing and already correct for all four agents.
*/

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::Local;
use serde_json::{Map, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::resume_lookup::{parse_json_line, read_lines_lossy};
use crate::server::{
    domain_error_response, read_runtime_text, read_session_text, routed_json, AppState,
    RoutedResponse,
};
use crate::session_chat::{
    find_claude_successor_transcript, last_substantive_transcript_timestamp_ms,
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    SessionChatSuccessorOutcome, SessionChatTranscriptAgent,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::storage::open_gxserver_database;
use axum::http::StatusCode;
use serde_json::json;

mod carry_files;
mod classify;
mod export;
mod model;
mod output_file;
mod parse_claude_cursor;
mod parse_codex_grok;
mod parse_hermes_pi;
mod render;
mod sections;

pub use carry_files::*;
use classify::*;
pub use export::*;
use model::*;
pub(crate) use output_file::*;
use parse_claude_cursor::*;
pub(crate) use parse_codex_grok::*;
use parse_hermes_pi::*;
use render::*;
pub use sections::*;
