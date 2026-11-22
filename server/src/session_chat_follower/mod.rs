//! Session Chat transcript follower, split by concern. Every submodule is glob re-exported
//! here, so `crate::session_chat_follower::*` paths are unchanged.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::DomainRepository;
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::{
    first_prompt_agent_name, normalize_agent_name, read_runtime_text, read_session_text,
    session_observer_key, AppState, SessionChatFollowerEntry,
};
use crate::session_chat::*;
use crate::session_chat_options::{
    forget_session_chat_options, SessionChatOptionDetector, SessionChatOptionEvidence,
};
use crate::session_chat_paths::resolve_session_chat_transcript_path;
use crate::session_chat_successor::{
    codex_rollout_session_id, find_claude_successor_transcript, find_codex_successor_transcript,
    first_codex_record_timestamp_ms, first_substantive_transcript_timestamp_ms,
    is_uuid_transcript_stem, last_codex_record_timestamp_ms,
    last_substantive_transcript_timestamp_ms, SessionChatSuccessorOutcome,
};
use crate::storage::open_gxserver_database;

mod drain;
mod frames;
mod registry;
mod run;
mod successor;

pub(crate) use drain::*;
pub use frames::*;
pub(crate) use registry::*;
pub use run::*;
pub(crate) use successor::*;
