//! Claude cloud sessions: sidebar sessions whose agent runs on claude.ai's computers (Claude Code
//! on the web), started through the cloud runner (crate::cloud_runner). Split by concern; see
//! each file.

mod account_pick;
mod accounts;
mod bring_back;
mod chat;
mod copy;
mod create;
mod endpoint;
mod github;
mod page_status;
mod poller;
mod record;
mod remote;
mod start;
mod work;

pub(crate) use account_pick::{cloud_session_accounts, select_cloud_account};
pub(crate) use chat::send_chat_text;
pub(crate) use endpoint::claude_cloud_action;
pub(crate) use poller::{notification_text, start_claude_cloud_poller, wake_claude_cloud_poller};
pub(crate) use work::{add_started_cloud_session, StartedCloudSession};

pub(crate) use create::{
    cloud_create_params, pick_claude_account, prepare_cloud_runtime, strip_client_record,
};

pub(crate) use record::{
    close_keeps_session_asleep, is_cloud_session_id, presentation_cloud_value,
};

/// `runtimeSettings` key of a local Claude session's create that teleports a cloud session on
/// its first launch (agents/launch_plan.rs; bring_back.rs sets it).
pub(crate) const TELEPORT_RUNTIME_KEY: &str = "teleportCloudSession";

/// The command a Claude cloud session's terminal runs: Ghostex's cloud client
/// (`ghostex claude-cloud attach`), by the absolute path every zmx session exports.
pub(crate) const CLAUDE_CLOUD_COMMAND: &str =
    "\"${GHOSTEX_CLI_EXECUTABLE:-ghostex}\" claude-cloud attach";

/// The built-in agent id of a Claude cloud session (`CLAUDE_CLOUD_AGENT_ID` in gx-core).
pub(crate) const CLAUDE_CLOUD_AGENT_ID: &str = "claude-cloud";

/// The `runLocation` that asks for a Claude cloud session.
pub(crate) const CLAUDE_CLOUD_RUN_LOCATION: &str = "claude-cloud";
