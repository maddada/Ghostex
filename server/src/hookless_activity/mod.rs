//! Hook-free activity observers for Claude, Codex and OpenCode, run beside their hooks and compared
//! with them. Claude's can also drive Claude sessions' state in place of its hooks (drive.rs).

mod claude;
mod codex;
mod comparison;
mod drive;
mod hook;
mod opencode;
mod task;

pub(crate) use drive::claude_hook_superseded;
pub(crate) use hook::observe_hook_event;
pub(crate) use opencode::observe_stream_event as observe_opencode_stream_event;
pub(crate) use task::{read_comparison, spawn_hookless_activity_task};
