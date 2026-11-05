pub const GXSERVER_PRODUCT: &str = "gxserver";
pub const GXSERVER_PROTOCOL_VERSION: u64 = 1;
pub const GXSERVER_PROTOCOL_HEADER: &str = "x-gxserver-protocol-version";
pub const GXSERVER_LOCAL_API_HOST: &str = "127.0.0.1";
pub const GXSERVER_LOCAL_API_PORT: u16 = 58744;
pub const GXSERVER_DEV_LOCAL_API_PORT_ENV: &str = "GHOSTEX_GXSERVER_DEV_PORT";
pub const GXSERVER_REMOTE_API_HOST: &str = "0.0.0.0";
pub const GXSERVER_REMOTE_API_PORT: u16 = 58745;
pub const GXSERVER_JSON_BODY_LIMIT_BYTES: usize = 1024 * 1024;
/*
CDXC:Clipboard 2026-11-01 DECISION:
User chose a 32 MiB raw image limit after measuring agent image support. saveSessionChatImage carries base64 in JSON, so its 44 MiB body limit includes the 4/3 expansion and envelope headroom; the general RPC limit stays small.
*/
pub const GXSERVER_IMAGE_BODY_LIMIT_BYTES: usize = 44 * 1024 * 1024;
/*
Chat attachments cap at 32 MiB on disk (saveSessionChatAttachment); the same
4/3 base64 inflation plus envelope headroom lands at 44 MiB.
*/
pub const GXSERVER_ATTACHMENT_BODY_LIMIT_BYTES: usize = 44 * 1024 * 1024;
/*
CDXC:Feedback 2026-10-18 WHY:
sendFeedback and startFeedbackAgent carry up to five 5 MiB images as base64
(about 35 MB), the same 36 MiB request cap the feedback relay itself enforces;
`FeedbackImage::from_params` checks the count and size of each image. The Send
Bug / Feedback dialog's Start Agent posted to startFeedbackAgent under the
general 1 MiB cap, so screenshots over about 750 KB in total failed with 413
(GitHub issue #253). Supersedes the 2026-10-04 note that named only sendFeedback.
*/
pub const GXSERVER_FEEDBACK_BODY_LIMIT_BYTES: usize = 36 * 1024 * 1024;
/// The app version (`package.json`), not the placeholder crate version; see `server/build.rs`.
pub const GXSERVER_VERSION: &str = env!("GHOSTEX_BUILD_MARKETING_VERSION");

pub const GXSERVER_CAPABILITIES: &[&str] = &[
    "health",
    "events",
    "localFullApi",
    "remoteLimitedApi",
    "strictProtocolVersion",
    /*
    CDXC:PlatformSupport 2026-08-03:
    Windows shells can outlive or temporarily bundle an older WSL gxserver.
    Advertise the atomic create/start/attach operation explicitly so clients
    select it before sending a request instead of inferring endpoint support
    from a 404 response.
    */
    "atomicWorkspaceTerminalCreate",
    /*
    CDXC:StateSync 2026-07-29-00:00:
    Sidebar V2 hides its settle/snooze affordances against daemons that cannot
    persist the lifecycle. The presentation snapshot carries the machine-scoped
    `capabilities` object clients actually read; these strings keep
    /api/health/server's capability inventory in step for diagnostics.
    */
    "sessionSettlement",
    "sessionSnooze",
    /*
    CDXC:Git 2026-07-29-00:00:
    Sidebar V2's card row hides its branch/PR affordances against daemons that
    cannot resolve per-session git state. Same split as the lifecycle flags: the
    presentation snapshot's `capabilities` object is what clients read, this
    string keeps /api/health/server's inventory in step for diagnostics.
    */
    "sessionGitStatus",
    /*
    CDXC:Worktrees 2026-07-29-00:00:
    Sidebar V2 hides "New worktree session…" against daemons that cannot create
    a worktree session atomically. Same split as the flags above: the
    presentation snapshot's `capabilities` object is what clients read, this
    string keeps /api/health/server's inventory in step for diagnostics.
    */
    "worktreeSessions",
    /*
    CDXC:RemoteMachines 2026-08-20:
    A remote machine runs the gxserver package its client installed, so it can
    be older than the client that talks to it. `promptEditor: "code-server"` is
    rejected outright by daemons built before that mode existed, which turned a
    New Terminal click on a remote project into a bare 400. Advertise the mode
    explicitly, exactly like `atomicWorkspaceTerminalCreate`, so clients choose
    the prompt-editor selector this daemon accepts before sending the create.
    */
    "codeServerPromptEditor",
    "browserTcpWebSocketV1",
    /*
    CDXC:SessionNotes 2026-08-24:
    Session notes are keyed by the agent session id and published on the
    presentation snapshot, so a client talking to an older remote daemon must
    be able to tell "this daemon has no notes table" apart from "this session
    has no note" before it offers the Session Note affordances.
    */
    "sessionAgentNotes",
    /*
    CDXC:Coordinators 2026-11-02 WHY:
    A handed-off orchestrator keeps its staged handoff draft for its Chat box
    instead of having it typed into its terminal while `/ghostex-orchestrator`
    starts (title_generation/first_input_draft.rs). An older remote daemon types
    it into the terminal on top of the skill line, so a client handing an
    orchestrator off to another computer checks this before it stages a draft
    there, and sends the handoff with the orchestrator's first message instead.
    SEE-ALSO: apps/desktop/src/app/gx_store/git/export_transcript.rs (`git_start_handoff_session`).
    */
    "coordinatorHandoffDraft",
    /*
    CDXC:Spaces 2026-08-27:
    Spaces are the daemon-owned saved sidebar filter. Same split as the flags
    above: the presentation snapshot's `capabilities` object is what clients
    read before offering the Space row and the Spaces context submenu; this
    string keeps /api/health/server's inventory in step for diagnostics.
    */
    "spaces",
];

pub const GXSERVER_MIGRATION_IDS: &[&str] = &[
    "0001_foundation",
    "0002_domain_state",
    "0003_session_sidebar_order",
    "0004_previous_session_history_quality",
    "0005_session_tags",
    "0006_expand_session_tags",
    "0007_expand_session_tags_in_progress_and_type",
    "0008_remove_retired_session_type_tags",
    "0009_remove_legacy_zmux_chat_projects",
    "0010_portless_persistence_model",
    "0011_t3_session_kind",
    "0012_recent_projects",
    "0013_app_user_data",
    "0014_automations",
    "0015_project_visibility",
    "0016_session_settle_snooze_lifecycle",
    "0017_stashed_prompts",
    "0018_global_sidebar_commands",
    "0019_remove_unsupported_session_kinds",
    "0020_delayed_sends",
    "0021_session_chat_queue",
    "0022_stashed_prompt_tags",
    "0023_session_parking",
    "0024_stashed_prompt_tag",
    "0025_session_agent_notes",
    "0026_stashed_prompt_agent_session",
    "0027_tailcat_state",
    "0028_remote_pairing",
    "0029_session_chat_model_selections",
    "0030_session_chat_draft_versions",
    "0031_session_chat_selection_options",
    "0032_session_chat_delivered_drafts",
    "0033_session_chat_draft_recovery",
    "0034_session_chat_startup_sends",
    "0035_notification_feed",
    "0036_custom_session_tags",
    "0037_session_fork_revision",
    "0038_delayed_send_watched_session",
    "0039_session_chat_selection_scope",
    "0040_prune_consumed_draft_recovery",
    "0041_coordinators",
    "0042_coordinator_pending_messages",
    "0043_session_chat_send_requests",
    "0044_session_chat_queued_images",
    "0045_agent_message_deliveries",
    "0046_session_lifecycle_index",
    "0047_session_presentation_kept_index",
    "0048_session_chat_resources",
    "0049_cloud_boxes",
    "0050_coordinator_close_threads",
    "0051_coordinator_remote_threads",
    "0052_coordinator_remote_thread_message",
    "0053_coordinator_remote_thread_activity",
    "0054_work_ticket_titles",
    "0055_coordinator_board",
    "0056_coordinator_board_removed",
    "0057_coordinator_board_answers",
    "0058_coordinator_board_answered",
    "0059_paired_device_api_token",
];
