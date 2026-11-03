//! gxserver HTTP server unit tests, grouped by what they cover. Shared fixtures
//! (`test_app_state`, git helpers, RPC helpers) live in `helpers.rs`.
mod connected_computers_routes;
mod draft_worktree_routes;
mod helpers;
mod project_directories;
mod protocol_and_routes;
mod renderer_and_titles;
mod voice_remote_sessions;
#[cfg(unix)]
mod work_mode_pass;
mod work_source_routes;
#[cfg(unix)]
mod workspace_deletion_routes;
mod worktree_projects;
mod worktree_session_routes;
mod zmx_and_sidecars;

use helpers::*;

use super::*;
