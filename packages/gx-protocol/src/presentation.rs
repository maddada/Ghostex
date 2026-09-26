//! Presentation snapshot, project, group, and session.
//!
//! The server builds these with `json!`, so field presence is part of the contract: several keys
//! are present-only (`isDraft`, `hasComposerDraft`, `attention`, `gitStatus`, `workingStartedAt`,
//! the delayed-send trio) and clear themselves by being absent from the next whole-object upsert.
//! A store must therefore replace a session, never merge into it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::de::{Rows, null_as_default};
use crate::side_state::{
    CustomSessionTagsState, SidebarProjectCollectionsState, SidebarSpacesState,
    WorkspaceSessionGroupsState,
};
use crate::tri::Tri;

/// Full presentation state of one daemon at `revision`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSnapshot {
    pub revision: i64,
    /// Projection time (ISO-8601, ms, Z). Activity and working timers were evaluated against it.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub generated_at: String,
    /// Read row by row: a row that does not fit is skipped and listed in `Rows::skipped`, so one
    /// damaged row cannot keep the machine from loading.
    pub projects: Rows<PresentationProject>,
    pub groups: Rows<PresentationGroup>,
    pub sessions: Rows<PresentationSession>,
    /// Absent on an older daemon: hide settle, snooze, Spaces, worktree sessions, and git rows for
    /// that machine instead of inventing them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<PresentationCapabilities>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub auto_settle_after_days: Tri<f64>,
    /// Shape not surveyed; kept loose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portless: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_groups: Option<WorkspaceSessionGroupsState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_project_collections: Option<SidebarProjectCollectionsState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_spaces: Option<SidebarSpacesState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_session_tags: Option<CustomSessionTagsState>,
}

impl PresentationSnapshot {
    /// How many wire rows were left out because they did not fit their type. A host logs this:
    /// it means a daemon row is damaged or the wire contract moved.
    pub fn skipped_row_count(&self) -> usize {
        self.projects.skipped.len() + self.groups.skipped.len() + self.sessions.skipped.len()
    }
}

/// Result of `POST /api/readPresentationSnapshot`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPresentationSnapshotResult {
    pub snapshot: PresentationSnapshot,
}

/// Machine-scoped capability flags. Every flag reads as `false` when missing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationCapabilities {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub session_settlement: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub session_snooze: bool,
    /// `false` on any daemon whose own sidebar is V1, including every headless remote daemon.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub session_git_status: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub spaces: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub worktree_sessions: bool,
}

/// One synthetic group per project today (`<projectId>:active`). User-made groups live in
/// [`WorkspaceSessionGroupsState`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationGroup {
    pub group_id: String,
    pub project_id: String,
    /// Display order.
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub session_ids: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub sort_key: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub title: String,
}

open_string_enum! {
    /// Whether the project folder is reachable.
    PathState {
        Available => "available",
        Missing => "missing",
        NotDirectory => "notDirectory",
        Unavailable => "unavailable",
    }
}

open_string_enum! {
    GitPrimaryAction {
        Commit => "commit",
        Push => "push",
        Pr => "pr",
        SyncRemote => "syncRemote",
        SyncMain => "syncMain",
        MultiRelease => "multiRelease",
        Release => "release",
    }
}

/// Sanitized git preferences published on the presentation project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationGitConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_commit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_commit_body: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_action: Option<GitPrimaryAction>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationProject {
    pub project_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_state: Option<PathState>,
    /// Copied from the domain row; `null` on a corrupt row reads as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub is_favorite: bool,
    #[serde(default, deserialize_with = "null_as_default")]
    pub is_pinned: bool,
    /// `<pinRank>:<lowercased name>:<projectId>`; the server orders projects by its byte order.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub sort_key: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub group_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_config: Option<PresentationGitConfig>,
    /// Absent = not probed yet, not a git folder, or an older daemon. `null` = probed, no
    /// `origin`. String = the origin URL.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub git_remote_origin_url: Tri<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_repository_root_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovered_icon_data_url: Option<String>,
    /// Copied verbatim from the domain row; kept loose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Value>,
}

impl PresentationProject {
    /// The project's default group id: `groupIds[0]`, else the synthetic `<projectId>:active`.
    pub fn default_group_id(&self) -> String {
        match self.group_ids.first() {
            Some(group_id) => group_id.clone(),
            None => default_group_id_for_project(&self.project_id),
        }
    }
}

/// The synthetic default group id the server assigns to a project.
pub fn default_group_id_for_project(project_id: &str) -> String {
    format!("{project_id}:active")
}

open_string_enum! {
    SessionKind {
        Terminal => "terminal",
        Agent => "agent",
    }
}

open_string_enum! {
    /// Where the session lives: a workspace tab or the command pane.
    SessionSurface {
        Workspace => "workspace",
        Commands => "commands",
    }
}

open_string_enum! {
    /// Effective lifecycle. Do not infer attachability from it; use `actions.attach`.
    LifecycleState {
        Running => "running",
        Sleeping => "sleeping",
        Stopped => "stopped",
        Unknown => "unknown",
    }
}

open_string_enum! {
    /// Liveness of the zmx provider, separate from lifecycle: a `running` row can have a `missing`
    /// provider.
    #[derive(Default)]
    ProviderSessionState {
        Exists => "exists",
        Missing => "missing",
        #[default]
        Unknown => "unknown",
        PersistenceDisabled => "persistence-disabled",
    }
}

open_string_enum! {
    SessionPersistenceProvider {
        Tmux => "tmux",
        Zmx => "zmx",
        Zellij => "zellij",
    }
}

open_string_enum! {
    SessionActivity {
        Idle => "idle",
        Working => "working",
        Attention => "attention",
    }
}

open_string_enum! {
    TitleSource {
        BrowserAuto => "browser-auto",
        Draft => "draft",
        Generated => "generated",
        Placeholder => "placeholder",
        TerminalAuto => "terminal-auto",
        User => "user",
    }
}

open_string_enum! {
    /// Explicit Sidebar V2 shelf pin.
    SettledOverride {
        Active => "active",
        Settled => "settled",
    }
}

open_string_enum! {
    PrState {
        Closed => "closed",
        Draft => "draft",
        Merged => "merged",
        Open => "open",
    }
}

open_string_enum! {
    TitleObservationStatus {
        Active => "active",
        Failed => "failed",
        Retrying => "retrying",
        Starting => "starting",
    }
}

/// What the daemon allows on a session right now. A missing object reads as nothing allowed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionActions {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub acknowledge_attention: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub attach: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub focus: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub kill: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub read_text: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub send_message: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub send_text: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub sleep: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub wake: bool,
}

/// Present only while `activity` is `attention`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionState {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub acknowledged: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entered_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
}

/// Health of the zmx title watcher.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleObservation {
    pub status: TitleObservationStatus,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub failure_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<String>,
}

/// Cached git probe for the session's effective cwd. The whole object is absent when the daemon
/// has nothing to publish; that is not an error state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionGitStatus {
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub additions: u64,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub deletions: u64,
    /// `null` for a detached HEAD or a cwd that is not a work tree.
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub updated_at: String,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub pr_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_state: Option<PrState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
}

/// One session row. Unique only together with `project_id`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSession {
    // Identity and placement.
    pub project_id: String,
    pub session_id: String,
    pub group_id: String,
    pub kind: SessionKind,
    pub surface: SessionSurface,
    /// zmx or wmx session name used for attach.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub zmx_name: String,
    /// Saved Action id owning a command-pane session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Server sort key; the server orders a project's sessions by its byte order.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub sort_key: String,
    /// Manual order. The server publishes an explicit `null` when the row has one; it reads the
    /// same as absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_order: Option<f64>,
    /// `surface == workspace` and running or sleeping.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub visible_in_sidebar_by_default: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub created_at: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub updated_at: String,

    // Lifecycle and activity.
    pub lifecycle_state: LifecycleState,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub provider_session_state: ProviderSessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_persistence_provider: Option<SessionPersistenceProvider>,
    pub activity: SessionActivity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<AttentionState>,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub pending_question_count: u64,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub actions: SessionActions,
    /// Falls back to `createdAt` on the server, so it cannot say whether the session was ever
    /// prompted; `has_ever_been_active` answers that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    /// Absent on daemons that predate the field; read that as not yet active.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub has_ever_been_active: bool,
    /// The recency clients sort by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaningful_activity_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_started_at: Option<String>,
    /// Present while a background shell or monitor the agent started is still running after its
    /// turn; the sidebar draws it as a grey dot, separate from `activity`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_work_detected_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_observation: Option<TitleObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_override: Option<SettledOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<String>,
    /// Copied raw from the row; `null` reads as `false`.
    #[serde(default, deserialize_with = "null_as_default")]
    pub is_pinned: bool,
    /// Copied raw from the row; `null` on older rows reads as `false`.
    #[serde(default, deserialize_with = "null_as_default")]
    pub is_parked: bool,
    /// `sessionTag == "favorite"` or the legacy boolean.
    #[serde(default, deserialize_with = "null_as_default")]
    pub is_favorite: bool,
    /// Built-in or custom tag id; omitted when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_tag: Option<String>,
    /// Present only while the session is a draft; never `false` on the wire.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub is_draft: bool,

    // Agent identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_slot: Option<String>,
    /// Accounts or agents this session can be resumed under; element shape kept loose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switchable_agents: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_session_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub fork_branch_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_family_session_ids: Option<Vec<String>>,

    // Title block.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_source: Option<TitleSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title_tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_resume_title: Option<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub is_primary_title_terminal_title: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub is_temporary_title: bool,
    /// Loading chrome only.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub is_generating_first_prompt_title: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,

    // Counters and chat side state.
    /// Absent at zero, and on a daemon that predates the queue.
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub queued_prompt_count: Option<u64>,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub queued_prompt_failed_count: Option<u64>,
    /// Present only when true.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub has_composer_draft: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_note: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub stashed_prompt_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_status: Option<SessionGitStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delayed_send_deadline_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delayed_send_remaining_label: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub delayed_send_remaining_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_when_all_project_sessions_stop_active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_when_agent_stops_active: Option<bool>,
    /// TS `DelayedSendAgentReference`; kept loose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_when_specific_agent_finishes: Option<Value>,
    /// Close After Done is armed; gxserver owns the timer. Present only when true.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub close_after_done: bool,
    /// When the armed session closes, while it shows Done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_after_done_deadline_at: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}
