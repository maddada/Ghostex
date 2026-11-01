//! Hook-free activity observers for Claude, Codex and OpenCode, run beside their hooks in shadow
//! mode: their events drive nothing yet and are only compared with the hooks'.

mod claude;
mod codex;
mod comparison;
mod hook;
mod opencode;
mod task;

pub(crate) use hook::observe_hook_event;
pub(crate) use opencode::observe_stream_event as observe_opencode_stream_event;
pub(crate) use task::{read_comparison, spawn_hookless_activity_task};
