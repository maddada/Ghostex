//! Trust project folders automatically: gxserver marks the folder a session runs in as trusted in
//! its agent's own records before starting the agent, so no session stops at a folder-trust prompt.

mod codex_ghostex_rule;
pub(crate) mod files;
mod launch;
mod stores;

pub(crate) use codex_ghostex_rule::allow_ghostex_in_codex_before_launch;
pub(crate) use launch::{
    added_project_folder_trusted, carry_claude_folder_trust_for_switch,
    carry_folder_trust_for_move, folder_trusted_by, trust_session_folder_before_launch,
};
