use axum::{body::Body, http::Response};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;

use crate::{
    automations::AutomationRuntime,
    config::GxserverConfig,
    delayed_sends::DelayedSendRuntime,
    events::GxserverEventHub,
    extensions::ExtensionRegistry,
    logging::GxserverLogger,
    paths::GxserverPaths,
    protocol::{MigrationStatus, RuntimeMetadata},
    remote_access::RemotePairingRuntime,
    repository_clone::RepositoryCloneJobManager,
    session_chat_options::SessionChatOptionCacheEntry,
    session_git_status, session_lifecycle,
    tailcat::TailcatRuntime,
    worktree_sessions,
};

pub struct GxserverForegroundOptions {
    pub build_identity: Option<String>,
    pub home_dir: Option<PathBuf>,
    pub version: String,
}

pub struct GxserverForegroundResult {
    pub reused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExistingGxserverState {
    Reusable,
    Running,
    Stopped,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) accounts: Arc<crate::accounts::runtime::AccountRuntime>,
    pub(crate) auth_token: String,
    /// Paired phones' own API tokens (CDXC:RemotePairing 2026-10-25 in `auth.rs`).
    pub(crate) device_api_tokens: crate::auth::DeviceApiTokens,
    /// The desktop's DevTools host and the sessions through it (`cef_devtools.rs`).
    pub(crate) cef_devtools: super::cef_devtools::CefDevtoolsHub,
    pub(crate) automation_runtime: AutomationRuntime,
    pub(crate) delayed_send_runtime: DelayedSendRuntime,
    /// Serializes `/api/startBoardWork` so concurrent calls for one bead
    /// cannot both observe "no usable link" and create two worker sessions.
    pub(crate) board_start_work_gate: Arc<Mutex<()>>,
    pub(crate) build_identity: String,
    pub(crate) config: GxserverConfig,
    pub(crate) event_hub: GxserverEventHub,
    pub(crate) extension_registry: ExtensionRegistry,
    pub(crate) logger: Arc<GxserverLogger>,
    pub(crate) metadata: RuntimeMetadata,
    pub(crate) migration: MigrationStatus,
    pub(crate) paths: GxserverPaths,
    pub(crate) presentation_event_sequence: Arc<Mutex<()>>,
    pub(crate) remote_pairing_runtime: RemotePairingRuntime,
    pub(crate) repository_clone_jobs: RepositoryCloneJobManager,
    pub(crate) session_chat_followers: Arc<Mutex<HashMap<String, SessionChatFollowerEntry>>>,
    /// Per-session model/effort detection cache (observer key → last detect).
    /// Detection spawns `zmx history`, so every trigger reads through this.
    pub(crate) session_chat_option_cache: Arc<Mutex<HashMap<String, SessionChatOptionCacheEntry>>>,
    pub(crate) shutdown_tx: broadcast::Sender<()>,
    pub(crate) stale_activity_timers: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub(crate) tailcat_runtime: TailcatRuntime,
    pub(crate) version: String,
    pub(crate) zmx_title_observers: Arc<Mutex<HashMap<String, ZmxTitleObserverTask>>>,
}

pub(crate) struct RoutedResponse {
    pub(crate) endpoint_path: Option<String>,
    pub(crate) response: Response<Body>,
}

pub(crate) struct ZmxTitleObserverTask {
    pub(crate) handle: tokio::task::JoinHandle<()>,
    pub(crate) zmx_name: String,
}

/*
CDXC:SessionChat 2026-07-31:
Session Chat transcript followers mirror the zmx title-observer lifecycle
(sync from presentation deltas, boot sync, shutdown stop-all) but are ALSO
refcounted by live /api/events subscribers: the tail-follow task runs only
while at least one client subscribes AND the session is running. The entry
outlives the task so epoch/seq (stream) and the resnapshot signal survive
respawns, and so a sleeping session's subscribers pick the stream back up on
wake without resubscribing.
*/
pub(crate) struct SessionChatFollowerEntry {
    pub(crate) subscribers: usize,
    pub(crate) fingerprint: String,
    pub(crate) limit: usize,
    pub(crate) task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) stream: Arc<crate::session_chat::SessionChatStream>,
    pub(crate) resnapshot: Arc<tokio::sync::Notify>,
    /*
    CDXC:AgentScreenDetection 2026-08-24:
    Progress the running task publishes for itself, because `task.is_finished()`
    cannot tell a healthy follower apart from one wedged in an inline await.
    Replaced on every respawn (see `sync_session_chat_follower_for_session`).
    */
    pub(crate) heartbeat: Arc<crate::session_chat::SessionChatFollowerHeartbeat>,
}

pub(super) const GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS: u64 = 3_000;
/*
CDXC:SessionChat 2026-08-26:
These two were flat `sleep(4s)` calls: the provider had just launched a CLI, and
four seconds was the guess for when its composer would exist. Both numbers were
wrong in both directions — an idle claude is ready in well under a second, while
a cold agent loading skills and MCP servers is still painting at five — so the
fast case paid a pointless wait and the slow case typed into a blank screen.

They are now the UNKNOWN HOLD of a real composer wait: an agent with a measured
signature is released the moment its input box appears, and an agent without one
keeps exactly the four-second behaviour it had rather than being released early
on evidence that does not exist. The ceiling is separate and higher, because the
case worth waiting out is precisely the slow cold start.
*/
pub(super) const GXSERVER_FORK_INITIAL_RENAME_READY_DELAY_MS: u64 = 4_000;
pub(super) const GXSERVER_FIRST_USER_INPUT_DRAFT_READY_DELAY_MS: u64 = 4_000;
/// Ceiling for every provider-startup composer wait.
pub(crate) const GXSERVER_PROVIDER_COMPOSER_WAIT_TIMEOUT_MS: u64 = 10_000;
pub(super) const GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS: u64 = 300;
pub(super) const GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH: usize = 250;
pub(super) const GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH: usize = 39;
/*
CDXC:SessionTitles 2026-07-29:
Empty-title Generate Name summarizes the last few transcript user prompts
instead of one pasted blob. Recent messages carry the naming signal, so the
budget is per-message with a wider overall cap than the single-prompt source.
*/
pub(super) const GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_COUNT: usize = 5;
pub(super) const GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_MAX_LENGTH: usize = 400;
pub(super) const GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MAX_LENGTH: usize = 2200;
pub(super) const GXSERVER_FIRST_PROMPT_TITLE_GENERATION_TIMEOUT_MS: u64 = 30_000;
pub(super) const GXSERVER_COMMIT_MESSAGE_GENERATION_TIMEOUT_MS: u64 = 120_000;
pub(super) const GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES: u64 = 1024 * 1024;

// Six actions were retired on 2026-09-25 (why: packages/gx-core/src/renderer_commands/verbs.rs).
pub(super) const RENDERER_COMMAND_ACTIONS: &[&str] = &[
    "clickButton",
    "focusGroup",
    "focusSession",
    "fullReloadSession",
    "moveProject",
    "openBrowser",
    "openBrowserPane",
    "openPaths",
    "openSettings",
    "readFocusedSession",
    "readResourcesSnapshot",
    "restartSession",
    /*
    CDXC:AgentSkills 2026-06-17-17:02:
    Generated-title `ghostex rename-command` now enters gxserver as a renderer command so macOS can submit Claude Code `/rename <title>` with a real native Enter event instead of zmx carriage-return text. Keep Rust's action allow-list in lockstep with the TypeScript daemon before full cutover.
    */
    "renameCommand",
    "runCommand",
    "switchProject",
    "toggleCloseAfterDone",
    "toggleSidebarCollapsed",
    "updateSettingsPatch",
];
pub(super) const AGENT_METADATA_TITLE_SYNC_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const SESSION_CHAT_FOLLOWER_SYNC_INTERVAL: Duration = Duration::from_secs(10);
pub(super) const SESSION_LIFECYCLE_SWEEP_INTERVAL: Duration =
    Duration::from_secs(session_lifecycle::SESSION_LIFECYCLE_SWEEP_INTERVAL_SECONDS);
pub(super) const SESSION_GIT_STATUS_REFRESH_INTERVAL: Duration =
    Duration::from_secs(session_git_status::SESSION_GIT_STATUS_REFRESH_INTERVAL_SECONDS);
pub(super) const WORKTREE_BRANCH_RENAME_SWEEP_INTERVAL: Duration =
    Duration::from_secs(worktree_sessions::WORKTREE_BRANCH_RENAME_SWEEP_INTERVAL_SECONDS);
