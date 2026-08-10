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
CDXC:SessionChatImagePaste 2026-08-01:
saveSessionChatImage carries a pasted image as base64 inside the JSON body.
The terminal paste path caps images at 12 MiB on disk; base64 inflates that
by 4/3, so 20 MiB leaves headroom for the JSON envelope without opening the
general RPC surface to oversized bodies.
*/
pub const GXSERVER_IMAGE_BODY_LIMIT_BYTES: usize = 20 * 1024 * 1024;
/*
Chat attachments cap at 32 MiB on disk (saveSessionChatAttachment); the same
4/3 base64 inflation plus envelope headroom lands at 44 MiB.
*/
pub const GXSERVER_ATTACHMENT_BODY_LIMIT_BYTES: usize = 44 * 1024 * 1024;
pub const GXSERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const GXSERVER_CAPABILITIES: &[&str] = &[
    "health",
    "events",
    "localFullApi",
    "remoteLimitedApi",
    "strictProtocolVersion",
    /*
    CDXC:GPUIWindowsTerminalStartup 2026-08-03:
    Windows shells can outlive or temporarily bundle an older WSL gxserver.
    Advertise the atomic create/start/attach operation explicitly so clients
    select it before sending a request instead of inferring endpoint support
    from a 404 response.
    */
    "atomicWorkspaceTerminalCreate",
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
    Sidebar V2 hides its settle/snooze affordances against daemons that cannot
    persist the lifecycle. The presentation snapshot carries the machine-scoped
    `capabilities` object clients actually read; these strings keep
    /api/health/server's capability inventory in step for diagnostics.
    */
    "sessionSettlement",
    "sessionSnooze",
    /*
    CDXC:SidebarV2GitStatus 2026-07-29-00:00:
    Sidebar V2's card row hides its branch/PR affordances against daemons that
    cannot resolve per-session git state. Same split as the lifecycle flags: the
    presentation snapshot's `capabilities` object is what clients read, this
    string keeps /api/health/server's inventory in step for diagnostics.
    */
    "sessionGitStatus",
    /*
    CDXC:SidebarV2Worktrees 2026-07-29-00:00:
    Sidebar V2 hides "New worktree session…" against daemons that cannot create
    a worktree session atomically. Same split as the flags above: the
    presentation snapshot's `capabilities` object is what clients read, this
    string keeps /api/health/server's inventory in step for diagnostics.
    */
    "worktreeSessions",
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
];
