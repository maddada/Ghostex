//! gxserver HTTP server unit tests, grouped by what they cover. Shared fixtures
//! (`test_app_state`, git helpers, RPC helpers) live in `helpers.rs`.
mod account_login_thread;
mod attention_stint;
mod chat_linked_files;
mod collab_routes;
mod connected_computers_routes;
mod draft_worktree_routes;
mod easy_connect_addresses_routes;
mod event_streams;
mod failure_episodes;
mod headless_settings_routes;
pub(crate) mod helpers;
mod notification_failures;
mod omp_subagent_viewer;
mod omp_waits;
mod project_directories;
mod protocol_and_routes;
mod push;
mod push_delivery;
mod remote_thread_stall;
mod renderer_and_titles;
mod security_routes;
mod settings_sync_appearance;
mod socket_tokens;
mod voice_caller_token;
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
