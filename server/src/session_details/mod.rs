//! The Session details panel's data: where one agent session runs and what it has loaded, read on
//! the machine that runs the session so the desktop, the web build and the phone show the same
//! thing. Every chat agent gets the sections its own records can fill (`agents.rs`); Claude's are
//! the fullest.
//!
//! - `gate.rs`: the built-in extension switch and the Claude task-list environment variable.
//! - `http.rs`: `/api/readSessionDetails` and `/api/stopSessionBackgroundTask`.
//! - `scan.rs`: what the session's Claude transcript says it loaded (MCP servers and their tools,
//!   memory files, skills) and which background commands it ran, read incrementally.
//! - `probe.rs`: Claude's own context breakdown, from a silent read-only copy of the session.
//! - `skills.rs`: where each listed skill's SKILL.md is (home, project or plugin), or that it is built in.
//! - `memory.rs`: the Environment section's RAM row, the session's process tree.
//! - `environment.rs`, `changes.rs`, `connectors.rs`, `background.rs`: the panel's other sections.
//! - `agents.rs`: the read for every other agent; `picker_skills.rs` (their Skills), `codex.rs` and
//!   `opencode.rs` (those agents' own sections).
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User: "i want this sidebar to be inside the chat view", like Claude Desktop's right panel, for Claude sessions first and Codex later. From the mockups (Q7) they picked "Live total + Claude's own breakdown from a silent read-only copy": the live total keeps coming from the statusline hook, and the breakdown is Claude's `get_context_usage` answer from `claude -p --resume <id> --fork-session --no-session-persistence`, never `/context` typed into the session ("i dont wanna read that all the time"). Q9 sections: Environment, Context, Connectors, Skills, Memory files, Changes, Background tasks (and Tasks, Q14). Q18: it ships as a beta built-in extension, which the user then asked to have on by default ("please make the Session detail panel extension enabled by default"; packages/settings-catalog/src/data/official_extensions.rs).
//! SEE-ALSO: packages/gx-chat-core/src/extras/session_details/ (the rows both chats draw), apps/desktop/src/app/native_chat/session_details/ (the panel), packages/settings-catalog/src/data/official_extensions.rs (the switch).

mod agents;
mod background;
mod changes;
mod codex;
mod connectors;
mod environment;
mod gate;
mod http;
mod memory;
mod opencode;
mod picker_skills;
mod probe;
mod scan;
mod skills;

pub(crate) use gate::{claude_task_tools_environment, session_details_enabled};
pub(crate) use http::{handle_read_session_details_http, handle_stop_session_background_task_http};
