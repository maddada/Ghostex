//! Presentation snapshot, project, group, and session.
//!
//! The server builds these with `json!`, so field presence is part of the contract: several keys
//! are present-only (`isDraft`, `hasComposerDraft`, `attention`, `gitStatus`, `workingStartedAt`,
//! the delayed-send trio) and clear themselves by being absent from the next whole-object upsert.
//! A store must therefore replace a session, never merge into it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::de::{null_as_default, Rows};
use crate::side_state::{
    CustomSessionTagsState, SidebarProjectCollectionsState, SidebarSpacesState,
    SidebarWorkspacesState, WorkspaceSessionGroupsState,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_groups: Option<WorkspaceSessionGroupsState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_project_collections: Option<SidebarProjectCollectionsState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_spaces: Option<SidebarSpacesState>,
    /// Whether this daemon's own machine has Spaces turned on (`sidebarSpacesEnabled`). Absent on
    /// an older daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_spaces_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_session_tags: Option<CustomSessionTagsState>,
    /// Absent on a daemon without workspaces (`capabilities.workspaces`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_workspaces: Option<SidebarWorkspacesState>,
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
    /// The daemon has workspaces: `sidebarWorkspaces`, `workspaceId` on projects and Spaces, and
    /// the workspace routes. Without it a client shows the machine unfiltered.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub workspaces: bool,
    /// The daemon looks up `attention.notificationId`. Without it a client has no exact link from
    /// an attention to its Notifications row; with it, a missing id means there is no row to link
    /// right now (not written yet, or already dismissed, cleared or pruned).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub attention_notification_id: bool,
    /// The daemon publishes `awaitingAnswer` on its sessions, so a missing one means no question
    /// card; without the flag a client cannot tell.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub awaiting_answer: bool,
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
    /// The project at the computer's home folder, whose header is Main Hub's row
    /// (server/src/chief/home.rs). Absent for every other project and from an older daemon.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_home: bool,
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
    /// The Hermes profile this project is the bot of; absent for every other project and from an
    /// older daemon (server/src/bot_projects.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_profile: Option<String>,
    /// Whether that bot's Hermes gateway runs; absent for every other project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_gateway_running: Option<bool>,
    /// How many runs that bot's cron jobs delivered since local midnight; absent for every other
    /// project and from an older daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_runs_today: Option<u64>,
    /// Which bots this project is one of: `gbots` for a GBots bot (server/src/gbots/), absent for a
    /// Hermes bot, every other project and an older daemon. A GBots bot's `bot_profile` is its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_kind: Option<String>,
    /// A GBots bot's status: `working`, `idle`, or `offline` while GBots is not running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_activity: Option<String>,
    /// Issues Workflow (formerly Work mode) is on for this project: a Linear key applies to it, `gh`
    /// is signed in, or its workspace has a ticket command (server/src/work_mode/project.rs).
    /// Present only when true.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub work_mode: bool,
    /// Whether Autopilot covers this project and its worktrees (server/src/autopilot/mod.rs): sent
    /// only while Autopilot is on and goes by project, for the menu's Advanced > Autopilot row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autopilot: Option<bool>,
    /// Issues Workflow is on and a Linear key applies to this project, so it shows its Linear parts
    /// (Create Linear Ticket, Link to Linear). Present only when true.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub work_linear: bool,
    /// Create Linear Ticket is offered: a Linear key applies and the workspace picked Linear as its
    /// tracker or the key is the project's or its workspace's own, not only the shared key. Present
    /// only when true; absent from an older daemon (the item stays hidden).
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub work_linear_create: bool,
    /// Issues Workflow is on but `gh` is not signed in, so its GitHub parts (pull requests, GitHub
    /// issues and projects) are hidden. Present only when true; absent from an older daemon, whose
    /// work mode had no such rule (read as signed in).
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub work_github_off: bool,
    /// `linear` or `github`: the primary tracker of the project's workspace (server/src/work_mode/
    /// tracker.rs), which decides what Create ticket makes and what Link to offers. Present only
    /// with work mode on; absent from an older daemon (read as Linear), and when the tracker is
    /// the workspace's ticket command (`work_source.is_tracker`), so it never names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_tracker: Option<String>,
    /// The workspace this project belongs to; absent = the default workspace (and on an older
    /// daemon). A worktree project follows its parent checkout's workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// The project shows in every workspace (the Ghostex config folder's project, which holds the
    /// Help chats). Present only when true.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub every_workspace: bool,
    /// The ticket command of the project's workspace (server/src/work_mode/command/), a source of
    /// tickets on its sessions' cards. Absent without one, in a team workspace (where it is
    /// stopped), and from an older daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_source: Option<PresentationWorkSource>,
}

/// A workspace's ticket command as a project row shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWorkSource {
    /// The id its `info` pinned, which its tickets' keys and link kinds start with (`fixture`).
    pub id: String,
    /// Its name for the workspace (`info.name`). Its icon is not on the row (CDXC:WorkMode
    /// 2026-11-03 in server/src/work_mode/project.rs).
    pub name: String,
    /// Whether it is the workspace's primary tracker (picked with `ghostex issues tracker <id>`).
    /// Then the row has no `work_tracker` and no `work_linear`, which old clients would read as
    /// Linear's or GitHub's (server/src/presentation/session_projection.rs).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub is_tracker: bool,
    /// Whether it creates tickets now: its `info` says it does and its program can run there
    /// (`/api/createWorkTicket`). Absent from an older daemon.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub can_create: bool,
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

open_string_enum! {
    /// Whether an attention is a finished turn or waits on the user's answer.
    AttentionKind {
        Finished => "finished",
        NeedsInput => "needsInput",
    }
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
    /// `finished` only for a finished turn while nothing waits on the user (no unanswered question,
    /// also on a sleeping or stopped session, no approval, stored chat prompt or connector card);
    /// `needsInput` otherwise. Decided from the live session, so it can differ from the
    /// Notifications row's kind. Absent on an older daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AttentionKind>,
    /// When the working stint this attention ended started. Present when the session entered the
    /// attention from working, or a connector card shows a session that is still working; absent
    /// when the attention came from idle (a bell, Stop or connector card after an acknowledgement,
    /// including rows acknowledged by an older daemon) or the start is not known. Exception: an
    /// attention an older daemon stored before the upgrade keeps the start it copied, which can be
    /// an earlier, acknowledged stint's start. That holds on running, sleeping and stopped rows,
    /// and for a connector card shown over such an attention, until the next acknowledgement or
    /// working stint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_started_at: Option<String>,
    /// The Notifications row written for this attention event, while it exists
    /// (`PresentationCapabilities::attention_notification_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_id: Option<String>,
    /// A finished turn whose handover has a 🛑 Blocked or ❓ Needs you section, so it waits on the
    /// user although `kind` says `finished`. Present only when true; absent on an older daemon.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub handover_needs_you: bool,
    /// gxserver's verdict on whether this attention plays a sound and shows a system notification
    /// or phone push (gx-core `alert_rule::should_alert`, with Alert me for at All): false for a
    /// coordinator's thread finishing, a coordinator finishing with nothing for the user, or an
    /// approval Autopilot answers. Clients apply their own Alert me for on top. Absent on an older
    /// daemon, where a client decides from the row itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alerts: Option<bool>,
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

/// The agentbox sandbox a session's agent runs in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationAgentbox {
    /// `docker`, `hetzner`, `vercel`, `daytona`, `e2b`, `digitalocean`, or `docker:<ssh host>`.
    pub provider: String,
    /// The agentbox box name every `agentbox … <box>` command takes.
    pub box_name: String,
    /// Short label for the provider, e.g. "Docker" or "Hetzner".
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub provider_label: String,
    /// A draft whose chat Run on row picked this box: nothing runs in a box yet, and its first
    /// message creates it. Until then the draft keeps Chat View.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub pending: bool,
}

/// A Claude cloud session: the agent runs on claude.ai's computers, and the session's row opens
/// its claude.ai page (server/src/claude_cloud/).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationClaudeCloud {
    /// `draft` (no task yet), `starting`, `running`, `failed` or `moved` (brought to this computer).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: String,
    /// `https://claude.ai/code/session_…`, once the session exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The branch the session pushes to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The latest status line, e.g. "Pushed 3 commits to claude/x" or "Pull request #12 opened".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    /// Why the start failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The Saved Claude account the session runs on; its claude.ai page opens in that account's
    /// Browser profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    /// The email the account's claude.ai page must be signed in as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    /// "<account> hit its 5h limit · resumes 4:10 pm" while the account is out of usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_notice: Option<String>,
}

/// One OSC 7501 record of a session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramStatusRecord {
    /// `a/b/c` path; empty for the root record.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub id: String,
    /// `idle`, `working`, `done`, `blocked` or `error`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: String,
    /// `permission`, `question` or `auth` on a blocked record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 0-100 on a working or blocked record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    /// The program, inherited from the nearest ancestor record when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// One line from the program; shown on hover, never as markup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub updated_at_ms: i64,
}

/// What the session's own program reported over OSC 7501 (server/src/program_status/).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramStatus {
    /// The records drive `activity` under the Session status setting.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub governs: bool,
    /// The record state that decides the row: `idle`, `working`, `blocked`, `done` or `error`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// The deciding record's message (a worker's title in front), sanitised for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub records: Vec<ProgramStatusRecord>,
}

open_string_enum! {
    /// The combined result of a pull request's checks.
    WorkChecksState {
        Passing => "passing",
        Failing => "failing",
        Pending => "pending",
    }
}

/// What a session in a work-mode project is linked to (server/src/work_mode/). Only sessions of
/// projects with work mode on carry it, so its presence is what makes a sidebar card two lines.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSessionWork {
    /// The checkout's branch; absent on the default branch or a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PresentationWorkPullRequest>,
    /// The tickets of `linear_issues` and `github_issues` (from the branch, or set by hand) as one
    /// list in the session links' shape (`key` `linear:GHX-142` / `issue:owner/repo#12`,
    /// `state_group`). It is not the Links panel's list: removals made there are not applied and
    /// stored links are not added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<PresentationSessionLink>,
    /// CDXC:WorkMode 2026-10-27 WHY:
    /// The phone app (ghostex-mobile) still reads `linearIssues` and `githubIssues` for a session's work chip, its Link to rows and its Copy rows, so they stay next to the source-generic `tickets` until the phone reads `tickets`; then they go.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linear_issues: Vec<PresentationWorkLinearIssue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub github_issues: Vec<PresentationWorkGithubIssue>,
    /// A Linear project is a release the team works on, never a repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linear_project: Option<PresentationWorkLinearProject>,
    /// A GitHub Project (Projects v2), published instead of `linear_project` when the workspace's
    /// primary tracker is GitHub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_project: Option<PresentationWorkGithubProject>,
    /// Some link was set by hand (`ghostex link-session`, the Link to menu), so "Back to
    /// automatic" has something to undo.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub hand_set: bool,
    /// The linked PR is merged and the user has not answered the Clean up / Keep offer for it yet
    /// (server/src/work_mode/cleanup.rs).
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub offer_cleanup: bool,
}

impl PresentationSessionWork {
    /// Whether anything is linked, which is what earns the card its second line.
    pub fn has_links(&self) -> bool {
        self.pull_request.is_some()
            || !self.linear_issues.is_empty()
            || !self.github_issues.is_empty()
            || self.linear_project.is_some()
            || self.github_project.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWorkPullRequest {
    #[serde(deserialize_with = "crate::de::lenient_u64")]
    pub number: u64,
    pub state: PrState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Absent until the checks were read, and for a PR without checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<WorkChecksState>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWorkLinearIssue {
    /// `SPX-1245`.
    pub identifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Linear's workflow state type: `triage`, `backlog`, `unstarted`, `started`, `completed` or
    /// `canceled`, or `notFound` when Linear has no such ticket. Absent until Linear answered (no
    /// API key, or not fetched yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_type: Option<String>,
    /// The team's own name for the state, e.g. "In Review", or "Not found".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWorkGithubIssue {
    #[serde(deserialize_with = "crate::de::lenient_u64")]
    pub number: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `open` or `closed`; absent until `gh` answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWorkLinearProject {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A GitHub Project (Projects v2): `owner/number` names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWorkGithubProject {
    /// The user or organization login that owns it.
    pub owner: String,
    #[serde(deserialize_with = "crate::de::lenient_u64")]
    pub number: u64,
    /// Absent until `gh` answered (it needs the `read:project` scope).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The item's Status field on the project board, e.g. "In progress".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// `PresentationSession.links`: every PR, GitHub issue and Linear ticket a session is linked to,
/// open work first, and the PR and issue URLs its chat only mentioned (server/src/session_links/).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSessionLinks {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub items: Vec<PresentationSessionLink>,
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub suggested: Vec<PresentationSuggestedLink>,
    /// Links a person removed, newest first, which the lists offer to restore.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub removed: Vec<PresentationSuggestedLink>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSessionLink {
    /// `pr:owner/repo#318`, `issue:owner/repo#305` or `linear:GHX-142`: what removing it sends.
    pub key: String,
    /// `pullRequest`, `issue` or `linear`.
    pub kind: String,
    /// `#318`, `repo#12` (another repo) or `GHX-142`.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A PR's `open`, `draft`, `merged` or `closed`; an issue's `open` or `closed`; a Linear
    /// ticket's state type, or `notFound` when Linear has no such ticket. Absent until it was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// A Linear ticket's state as its team names it ("In Review"), or "Not found".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_name: Option<String>,
    /// A ticket's Work page group (`backlog`, `todo`, `progress`, `review`, `done`, `canceled`, or
    /// `closed` for a closed GitHub issue), which sorts and colours it whatever its source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_group: Option<String>,
    /// A PR's `passing`, `failing` or `pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<String>,
    /// A PR's `reviewRequired`, `approved` or `changesRequested`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Where it came from: `branch`, `agentOpened`, `agentReported`, `closes`, `mentioned` or
    /// `user`; `source_label` says it in words ("Closed by #318").
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub source: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub source_label: String,
    /// Merged, closed, done or canceled, or a Linear ticket Linear has no record of (`notFound`).
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub finished: bool,
    /// A person pinned it: it leads every list and always shows in the sidebar.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub pinned: bool,
    /// Never read yet, and a read is on its way: no title or state to show.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub loading: bool,
    /// A PR's branch (`headRefName`), for its hover card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSuggestedLink {
    pub key: String,
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// As on `PresentationSessionLink`; absent until it was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_name: Option<String>,
    /// The second line: "Mentioned in the chat" for a suggestion, where a removed link came from
    /// (or "Removed").
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub source_label: String,
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub finished: bool,
    /// As on `PresentationSessionLink`.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub loading: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
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
    /// The session is stuck on a permission prompt or approval card only the user can grant;
    /// clients draw the red approval dot. Absent (false) otherwise and on daemons that predate it.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub awaiting_approval: bool,
    /// Since when a working session has shown no output, present only while that has lasted long
    /// enough to count as stuck (gxserver's coordinators/stall.rs); clients draw the red
    /// not-moving dot and play the not-moving sound when it appears.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuck_since: Option<String>,
    /// Who is healing a stuck session, so its red dot does not alert yet: `autopilot` while
    /// Autopilot covers it and has not left it for the user. Absent otherwise and on older daemons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuck_handled_by: Option<String>,
    /// Since when the session is blocked on something Ghostex could not get past itself: every
    /// account out of usage, or an API, login or overload error its recovery could not retry past.
    /// Clients draw the red dot with `blocked_reason` and alert when it appears. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_since: Option<String>,
    /// Why the session is blocked, in a few plain words; present with `blocked_since`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    /// A running session holds a question card (an ask-user tool call) only the user can answer,
    /// also while its agent reads as working. Absent (false) otherwise and on daemons without
    /// [`PresentationCapabilities::awaiting_answer`]. Independent of `pending_question_count`.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub awaiting_answer: bool,
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
    /// Why the model change the user picked could not be applied; the messages behind it are held.
    /// The sidebar draws it as a red dot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_selection_failure: Option<String>,
    /// A chat message that could not be sent, or that waits on a closed session: the same red dot,
    /// and its tooltip (server/src/session_chat_send_failure.rs).
    #[serde(default)]
    pub send_failure: Option<String>,
    /// The session's OSC 7501 program status; absent when it has no records to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_status: Option<ProgramStatus>,
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
    /// Present only when the agent runs inside an agentbox sandbox instead of on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agentbox: Option<PresentationAgentbox>,
    /// Present only for a Claude cloud session: the agent runs on claude.ai's computers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_cloud: Option<PresentationClaudeCloud>,
    /// Present only for a Cloud Box session (on the box's gxserver) or a draft whose tray picked
    /// Cloud Box (on home gxserver): the ☁ badge and its state dot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_box: Option<crate::cloud_box::PresentationCloudBox>,
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
    /// This computer's Chief (server/src/chief/): drawn as its own row under the Spaces bar, never
    /// in its project's list (`listed_in_sidebar`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_chief: bool,
    /// `coordinator` for a coordinator session, `thread` for a session a coordinator started;
    /// absent otherwise. A coordinator that is also another coordinator's thread says
    /// `coordinator` and still carries the thread fields below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_session_id: Option<String>,
    /// `waiting`, `working`, `finished`, `sleeping`, `closed` or `done`; an unknown value reads as
    /// `finished`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_thread_state: Option<String>,
    /// A coordinator's threads that run on other computers this one is connected to; the client
    /// nests those computers' rows under the coordinator (server/src/coordinators/remote_threads.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_remote_threads: Option<Vec<PresentationRemoteThread>>,
    /// The main session this session is a side chat of (server/src/side_chats/); a side chat is
    /// left out of every session list and shown from its main chat's Side chats pill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_chat_parent_session_id: Option<String>,

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
    /// The name of the user's last rename that the agent never confirmed within 8 seconds: the
    /// title shakes when it appears and the next Rename opens with it. Absent once a rename is
    /// confirmed or another one is requested, and on a daemon that predates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_failed_title: Option<String>,
    /// When gxserver gave up on `rename_failed_title`; a new value is a new failure to shake for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_failed_at: Option<String>,
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
    /// Present only for a session of a project with work mode on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<PresentationSessionWork>,
    /// Every session's PR, issue and Linear links; absent when it has none and nothing to suggest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<PresentationSessionLinks>,
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

/// Whether the session waits on an unanswered question, without changing the async question count.
///
/// CDXC:SessionStatus 2026-11-03 WHY:
/// An open question card counts as a question on the desktop for every agent, just as on the phone: the question dot, status, ordering and inactivity sleep guards use it too. This supersedes leaving the desktop dot and automatic sleep unchanged when `awaitingAnswer` was added.
pub fn session_has_question(pending_question_count: u64, awaiting_answer: bool) -> bool {
    pending_question_count > 0 || awaiting_answer
}

impl PresentationSession {
    pub fn has_question(&self) -> bool {
        session_has_question(self.pending_question_count, self.awaiting_answer)
    }

    /// Whether the session is a row of the session lists: a workspace session the daemon shows by
    /// default, and not a side chat (those are reached from their main chat's Side chats pill).
    pub fn listed_in_sidebar(&self) -> bool {
        self.visible_in_sidebar_by_default
            && self.surface != SessionSurface::Commands
            && self.side_chat_parent_session_id.is_none()
            && !self.is_chief
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One of a coordinator's threads on another computer: that computer's server id, the thread's ids
/// there, and its state as the coordinator's computer sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationRemoteThread {
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_state: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::delta::PresentationDelta;

    const ENTERED_AT: &str = "2026-10-31T09:00:30.000Z";
    const STARTED_AT: &str = "2026-10-31T09:00:00.000Z";

    fn session_with_attention(attention: Value) -> Value {
        json!({
            "projectId": "P1",
            "sessionId": "G1",
            "groupId": "P1:active",
            "kind": "agent",
            "surface": "workspace",
            "lifecycleState": "running",
            "activity": "attention",
            "attention": attention,
        })
    }

    fn full_attention() -> Value {
        json!({
            "acknowledged": false,
            "enteredAt": ENTERED_AT,
            "eventId": "attn_1",
            "kind": "finished",
            "workingStartedAt": STARTED_AT,
            "notificationId": "row-1",
        })
    }

    #[test]
    fn attention_extensions_accept_older_wire_shapes() {
        let older_attention =
            json!({ "acknowledged": false, "enteredAt": ENTERED_AT, "eventId": "attn_1" });
        let older: PresentationSession =
            serde_json::from_value(session_with_attention(older_attention.clone()))
                .expect("an older daemon's attention parses");
        let attention = older.attention.expect("attention");
        assert_eq!(attention.kind, None);
        assert_eq!(attention.working_started_at, None);
        assert_eq!(attention.notification_id, None);
        assert_eq!(
            serde_json::to_value(&attention).expect("serialize"),
            older_attention,
            "absent additions stay absent, never null"
        );

        let capabilities: PresentationCapabilities =
            serde_json::from_value(json!({ "sessionSettlement": true, "sessionSnooze": true }))
                .expect("an older daemon's capabilities parse");
        assert!(!capabilities.attention_notification_id);
        let capabilities: PresentationCapabilities =
            serde_json::from_value(json!({ "attentionNotificationId": true }))
                .expect("capabilities parse");
        assert!(capabilities.attention_notification_id);

        let newer: PresentationSession =
            serde_json::from_value(session_with_attention(full_attention()))
                .expect("a newer daemon's attention parses");
        let attention = newer.attention.expect("attention");
        assert_eq!(attention.kind, Some(AttentionKind::Finished));
        assert_eq!(attention.working_started_at.as_deref(), Some(STARTED_AT));
        assert_eq!(attention.notification_id.as_deref(), Some("row-1"));
        assert_eq!(
            serde_json::to_value(&attention).expect("serialize"),
            full_attention()
        );
    }

    #[test]
    fn handover_needs_you_is_read_and_left_out_when_false() {
        let mut attention = full_attention();
        attention["handoverNeedsYou"] = json!(true);
        let session: PresentationSession =
            serde_json::from_value(session_with_attention(attention.clone()))
                .expect("attention parses");
        let parsed = session.attention.expect("attention");
        assert!(parsed.handover_needs_you);
        assert_eq!(serde_json::to_value(&parsed).expect("serialize"), attention);
        let older: PresentationSession =
            serde_json::from_value(session_with_attention(full_attention()))
                .expect("an older daemon's attention parses");
        let older = older.attention.expect("attention");
        assert!(!older.handover_needs_you);
        assert_eq!(
            serde_json::to_value(&older).expect("serialize"),
            full_attention(),
            "false is left out"
        );
    }

    #[test]
    fn unknown_attention_kind_does_not_drop_session() {
        let snapshot: PresentationSnapshot = serde_json::from_value(json!({
            "revision": 3,
            "projects": [],
            "groups": [],
            "sessions": [session_with_attention(json!({ "acknowledged": false, "kind": "failed" }))],
        }))
        .expect("snapshot parses");
        assert_eq!(snapshot.skipped_row_count(), 0);
        let attention = snapshot.sessions[0].attention.as_ref().expect("attention");
        assert_eq!(
            attention.kind,
            Some(AttentionKind::Other("failed".to_string()))
        );
        assert_eq!(
            serde_json::to_value(attention).expect("serialize")["kind"],
            json!("failed"),
            "an unknown kind round-trips verbatim"
        );
    }

    #[test]
    fn absent_attention_fields_clear_on_replacement() {
        let changed = |attention: Value| -> PresentationSession {
            match PresentationDelta::from_value(json!({
                "type": "sessionPresentationChanged",
                "session": session_with_attention(attention),
            }))
            .expect("delta parses")
            {
                PresentationDelta::SessionPresentationChanged { session } => *session,
                other => panic!("unexpected delta {other:?}"),
            }
        };
        let first = changed(full_attention());
        assert_eq!(
            first
                .attention
                .as_ref()
                .and_then(|attention| attention.notification_id.as_deref()),
            Some("row-1")
        );
        // The row was dismissed and a question came up: the next whole session carries no id and
        // another kind, and nothing of the earlier attention survives in the decoded session.
        let second = changed(json!({
            "acknowledged": false,
            "enteredAt": ENTERED_AT,
            "eventId": "attn_1",
            "kind": "needsInput",
        }));
        let attention = second.attention.as_ref().expect("attention");
        assert_eq!(attention.notification_id, None);
        assert_eq!(attention.working_started_at, None);
        assert_eq!(attention.kind, Some(AttentionKind::NeedsInput));
    }
}
