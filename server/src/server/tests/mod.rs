//! gxserver HTTP server unit tests, grouped by what they cover. Shared fixtures
//! (`test_app_state`, git helpers, RPC helpers) live in `helpers.rs`.
mod account_login_thread;
mod attention_stint;
mod connected_computers_routes;
mod draft_worktree_routes;
mod easy_connect_addresses_routes;
mod helpers;
mod omp_subagent_viewer;
mod project_directories;
mod protocol_and_routes;
mod renderer_and_titles;
mod voice_remote_sessions;
#[cfg(unix)]
mod work_mode_pass;
#[cfg(unix)]
mod work_source_routes;
#[cfg(unix)]
mod workspace_deletion_routes;
mod worktree_projects;
mod worktree_session_routes;
mod zmx_and_sidecars;

use helpers::*;

use super::*;
