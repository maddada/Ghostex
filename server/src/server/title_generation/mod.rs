//! Session title generation: agent-metadata title checks, the fork initial rename, the
//! first user input draft, the first-prompt auto title job, and manual title generation,
//! split by concern from the former `server/title_generation.rs`. Every submodule is glob
//! re-exported here, so callers keep using `title_generation::…` unchanged.
mod codex_fallback;
mod first_input_draft;
mod first_prompt_decision;
mod first_prompt_job;
mod fork_rename;
mod manual_title;
mod project_status_titles;
mod rename_confirmation;
mod title_command;
mod title_preview;

pub(crate) use codex_fallback::*;
pub(crate) use first_input_draft::*;
pub(crate) use first_prompt_decision::*;
pub(crate) use first_prompt_job::*;
pub(crate) use fork_rename::*;
pub(crate) use manual_title::*;
pub(crate) use project_status_titles::*;
pub(crate) use rename_confirmation::*;
pub(crate) use title_command::*;
pub(crate) use title_preview::*;

use super::*;
