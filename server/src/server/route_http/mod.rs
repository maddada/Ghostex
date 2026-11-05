//! The gxserver HTTP router: the gate and dispatch (`dispatch.rs`) plus one sibling module per route area, each with its own `route_*_http` match.

use super::*;

mod agents;
mod chat;
mod claude_cloud;
mod connectors;
mod control;
mod dispatch;
mod git;
mod projects;
mod prompts;
mod secrets;
mod session_links;
mod sessions;
mod sidebar;
mod team_slack;
mod team_sync;
mod work_cloud;
mod work_items;
mod work_links;
mod work_mode;
mod work_source;
mod work_tickets;
mod work_tracker;
mod workspaces;

use agents::route_agents_http;
use chat::route_chat_http;
use claude_cloud::route_claude_cloud_http;
use connectors::route_connectors_http;
use control::route_control_http;
pub(in crate::server) use dispatch::route_http;
use dispatch::RouteHttpRequest;
use git::route_git_http;
use projects::route_projects_http;
use prompts::route_prompts_http;
use secrets::route_secrets_http;
use session_links::route_session_links_http;
use sessions::route_sessions_http;
use sidebar::route_sidebar_http;
use team_slack::route_team_slack_http;
use team_sync::route_team_sync_http;
use work_cloud::route_work_cloud_http;
use work_items::route_work_items_http;
use work_links::route_work_links_http;
use work_mode::route_work_mode_http;
use work_source::route_work_source_http;
#[cfg(test)]
pub(crate) use work_tickets::create_command_ticket;
use work_tickets::route_work_tickets_http;
use work_tracker::route_work_tracker_http;
use workspaces::route_workspaces_http;
