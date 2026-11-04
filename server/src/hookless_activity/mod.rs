//! Hook-free activity observers for Claude, Codex and OpenCode, run beside their hooks and compared
//! with them. Claude's can also drive Claude sessions' state in place of its hooks (drive.rs).

mod attach;
mod claude;
mod codex;
mod comparison;
mod drive;
mod hook;
mod opencode;
mod plan_mode;
mod process_starts;
mod surfaces;
mod task;

pub(crate) use drive::claude_hook_superseded;
pub(crate) use hook::observe_hook_event;
pub(crate) use opencode::observe_stream_event as observe_opencode_stream_event;
#[cfg(test)]
pub(crate) use process_starts::ClaudeStartSource;
pub(crate) use process_starts::{
    hook_line_start as claude_hook_line_start, read_claude_process_starts, ClaudeProcessStart,
    ProcessStartCursor,
};
pub(crate) use task::{read_comparison, spawn_hookless_activity_task};
