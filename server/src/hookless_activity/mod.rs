//! Hook-free activity watchers for Claude, Codex and OpenCode. They drive those agents' sessions
//! in place of their hooks by default (drive.rs, agent_drive.rs, agent_hooks/hook_free.rs), fall
//! back to a hook where they cannot see a running agent (hook_free_fallback.rs), and can be
//! compared with the hooks side by side (task.rs, comparison.rs).

mod agent_drive;
mod attach;
mod claude;
mod claude_question_screen;
mod claude_turns;
mod codex;
mod comparison;
mod drive;
mod hook;
mod hook_free_fallback;
mod opencode;
mod plan_mode;
mod process_starts;
mod surfaces;
mod task;

pub(crate) use comparison::{note_session_closed, note_session_slept};
pub(crate) use drive::{hook_superseded, OBSERVER_MARK};
pub(crate) use hook::observe_hook_event;
pub(crate) use hook_free_fallback::{
    answer_codex_update, answer_fallback_notice, codex_update_notice, fallback_notice,
    ACTION_RESTART_FOR_HOOK, ACTION_UPDATE_CODEX,
};
pub(crate) use opencode::{
    observe_stream_event as observe_opencode_stream_event,
    observe_terminal_title as observe_opencode_terminal_title, OBSERVED_PROMPT,
};
#[cfg(test)]
pub(crate) use process_starts::ClaudeStartSource;
pub(crate) use process_starts::{
    hook_line_start as claude_hook_line_start, read_claude_process_starts, ClaudeProcessStart,
    ProcessStartCursor,
};
pub(crate) use task::{read_comparison, spawn_hookless_activity_task};
