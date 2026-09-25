//! What the sidebar draws: the same content the native renderer once read from the TypeScript
//! snapshot, minus menus, hover actions and header actions (`sidebar_menu/` builds those).

use std::sync::Arc;

use crate::keys::SessionKey;

use super::inputs::{CloseAfterDoneInput, ProjectDiffStats, SectionId};
use super::session_text::{last_interaction_label, next_label_deadline, timer_trailing_label};
use super::tags::TagPresentation;

/// The whole list for one machine tab.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarView {
    /// A first snapshot of THIS COMPUTER's daemon has been applied, or the host has seen it
    /// unavailable. It is the local machine's fact on every tab, exactly as the old projection's
    /// `state.hasReceivedSnapshot` is.
    pub ready: bool,
    /// The selected machine is one this view model can build: this computer, or a remote machine
    /// the host feeds into the store. A host that selects a machine tab this says `false` for must
    /// keep drawing whatever it had.
    pub supported: bool,
    pub selected_machine_id: String,
    /// `<machine>|<space or all>`: the scope a scroll position belongs to.
    pub scroll_scope: String,
    /// The selected machine's counts.
    pub machine: MachineSummary,
    /// Every machine tab, this computer first, with the counts its badge draws.
    pub machines: Vec<MachineTabView>,
    pub spaces_enabled: bool,
    pub spaces: Vec<SpaceView>,
    /// Every group of the machine that is drawn, in order.
    pub groups: Vec<GroupView>,
    pub collections: Vec<CollectionView>,
    /// The top-level row sequence: a project group or a collection of them.
    pub order: Vec<OrderItem>,
    pub empty_state: EmptyState,
}

impl SidebarView {
    pub fn group(&self, group_id: &str) -> Option<&GroupView> {
        self.groups
            .iter()
            .find(|group| group.core.group_id == group_id)
    }
}

/// The working and attention counts of a machine tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MachineSummary {
    pub working_count: usize,
    pub attention_count: usize,
    /// Rows drawn with the grey dot: idle, with a background shell or monitor still running.
    pub background_work_count: usize,
}

/// One machine tab: what the host said about it, plus the counts its badge draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineTabView {
    pub id: String,
    pub label: String,
    /// The host's connection state word; always `connected` for this computer.
    pub state: String,
    pub message: Option<String>,
    pub working_count: usize,
    pub attention_count: usize,
    pub background_work_count: usize,
}

/// The machine a drawn group belongs to; absent for this computer's groups.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteMachineView {
    pub machine_id: String,
    pub machine_name: String,
    /// The raw project id in that machine's daemon; absent for its Chats group.
    pub project_id: Option<String>,
}

/// A drawn group: a project, or a user-made session group inside one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupView {
    pub core: Arc<GroupCore>,
    /// The colour of the collection the group is in, when it is drawn inside one.
    pub collection_color: Option<String>,
    /// The collection the group belongs to, from the collections document rather than from the
    /// drawn list: a collection the user hid still owns its projects, and the Space rules are
    /// written against ownership, not against what is on screen.
    pub collection_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupCore {
    pub group_id: String,
    /// The id every per-project UI state is keyed by: the project id, or the group id for a
    /// user-made group.
    pub storage_id: String,
    pub title: String,
    /// The multi-line project header tooltip; absent for a user-made group.
    pub title_tooltip: Option<String>,
    pub is_active: bool,
    pub project_context: Option<ProjectContextView>,
    pub summary: GroupSummary,
    pub collapsed: bool,
    /// The session list shows every row rather than the compact first rows.
    pub expanded: bool,
    pub hidden_session_count: usize,
    pub show_list_toggle: bool,
    pub hover_actions_expanded: bool,
    pub sections: Vec<SectionView>,
    /// Every row of the group that passes the tag filter, in display order.
    pub sessions: Vec<SessionView>,
    /// The machine this group belongs to; absent for this computer's groups.
    pub remote_machine: Option<RemoteMachineView>,
    /// The machine's stream is down while its rows are still held, so the group draws faded and
    /// its terminal rows are not interactive. Never set for this computer's groups.
    pub is_stale: bool,
}

/// What a project row draws besides its sessions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectContextView {
    pub project_id: String,
    pub path: String,
    pub icon_data_url: Option<String>,
    pub discovered_icon_data_url: Option<String>,
    pub diff_stats: ProjectDiffStats,
    pub worktree: Option<WorktreeView>,
    /// The project's git origin, when the daemon has probed one. Absent and an explicit `null`
    /// read the same here, because the one reader (the project menu's Copy Remote URL) tests it
    /// for truthiness.
    pub git_remote_origin_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorktreeView {
    pub branch: String,
    pub name: String,
    pub parent_project_id: String,
    pub parent_project_name: String,
    pub parent_project_path: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroupSummary {
    pub working_count: usize,
    pub attention_count: usize,
    pub background_work_count: usize,
    pub awake_count: usize,
}

/// One heading of a project's session list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionView {
    pub id: SectionId,
    pub collapsed: bool,
    pub count: usize,
    pub contains_active_session: bool,
    pub working_count: usize,
    pub attention_count: usize,
    pub background_work_count: usize,
    pub question_count: usize,
    /// The rows this heading draws: its sessions, minus the ones the compact list leaves out.
    pub session_ids: Vec<String>,
}

/// A row in a group: the session's own values plus what this list says about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub row: Arc<SessionRow>,
    pub is_focused: bool,
    pub is_visible: bool,
    pub is_multi_selected: bool,
}

impl SessionRow {
    /// CDXC:SessionStatus 2026-09-24 DECISION:
    /// User: a section, project, collection, Space or machine header shows the grey dot when it has no working session but has a grey-dot session, so the headers count exactly the rows that draw the grey dot: idle rows with a background shell or monitor still running.
    pub fn shows_background_work(&self) -> bool {
        self.has_background_work && self.activity != "working" && self.activity != "attention"
    }
}

/// One session (or browser tab) as a sidebar row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionRow {
    /// `combined-session:<project>:<session>` for a session, `gpui-browser:<project>:<tab>` for a
    /// browser tab.
    pub sidebar_session_id: String,
    /// The store key; absent for a browser tab, which is host state and not a session.
    pub key: Option<SessionKey>,
    pub is_browser: bool,
    /// A browser tab the host reports as the focused one of its project.
    pub browser_is_active: bool,
    /// A browser tab the host reports as on screen.
    pub browser_is_visible: bool,
    /// The stored title, before the display rules.
    pub alias: String,
    /// The one line the row draws.
    pub display_title: String,
    /// The hover tooltip, already assembled.
    pub title_tooltip: String,
    pub activity: String,
    /// A background shell or monitor is still running after the agent's turn; drawn as a grey dot
    /// when the row is otherwise idle.
    pub has_background_work: bool,
    pub pending_question_count: u64,
    pub agent_icon: Option<String>,
    /// `terminal`, `browser`, or whatever else a newer daemon publishes.
    pub session_kind: Option<String>,
    /// The sidebar's own lifecycle vocabulary: `running`, `sleeping`, `error`, `done`.
    pub lifecycle_state: String,
    pub is_pinned: bool,
    pub is_parked: bool,
    pub is_draft: bool,
    pub is_favorite: bool,
    pub session_tag: Option<String>,
    pub effective_tag: Option<String>,
    pub tag_presentation: Option<TagPresentation>,
    pub last_interaction_at: Option<String>,
    pub session_note: Option<String>,
    pub favicon_data_url: Option<String>,
    pub has_composer_draft: bool,
    pub queued_prompt_count: Option<u64>,
    pub queued_prompt_failed_count: Option<u64>,
    pub delayed_send: Option<DelayedSendView>,
    pub close_after_done: Option<CloseAfterDoneInput>,
    pub is_generating_first_prompt_title: bool,
    /// Inputs of the time-based values, which the renderer formats against its own clock.
    pub timing: SessionTiming,
    /// The session facts only the row's menus and its Copy Details text read.
    pub menu_facts: SessionMenuFacts,
}

/// What a row's context menu, hover actions and Copy Details need beyond what it draws.
///
/// CDXC:ContextMenus 2026-09-20 WHY:
/// These are on the row rather than looked up per menu because a menu is built for a row the list
/// is already holding, and reaching back into the store for the session would make the menu
/// answer from a different moment than the row it belongs to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionMenuFacts {
    /// `agentName ?? agentId`, which is what the transcript-agent lookup reads.
    pub agent_name: Option<String>,
    pub agent_session_id: Option<String>,
    pub session_persistence_provider: Option<String>,
    pub session_persistence_name: Option<String>,
    /// `<project>:<session>`, the id a pane is routed by.
    pub session_routing_id: Option<String>,
    /// The daemon's own `displayTitle`, before the heading rules. Copy Details quotes this rather
    /// than the heading the row draws.
    pub raw_display_title: Option<String>,
    pub primary_title: Option<String>,
    pub terminal_title: Option<String>,
    /// The session's subtitle.
    pub detail: Option<String>,
    /// The saved first prompt, which Generate Title and View 1st Message need. The presentation
    /// stream does not carry it, so on this client it is always absent and both items are hidden,
    /// exactly as they are in the TypeScript projection.
    pub first_user_message: Option<String>,
    /// A remote row publishes whether its machine can do these; a local daemon row always can.
    pub can_schedule_delayed_send: bool,
    pub can_toggle_close_after_done: bool,
}

/// The daemon's or the host's Delayed Send, as the row shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DelayedSendView {
    pub deadline_at: Option<String>,
    pub remaining_label: Option<String>,
    pub remaining_ms: Option<i64>,
    pub send_when_all_project_sessions_stop_active: bool,
    pub send_when_agent_stops_active: bool,
    /// The daemon's `sendWhenSpecificAgentFinishes`, kept as the daemon sent it. Only the daemon's
    /// own Delayed Send carries one: the host's timers never do, so a row whose Delayed Send came
    /// from the host has none, which is what the TypeScript projection's fallback leaves too. The
    /// row draws nothing from it; the Delayed Send dialog is seeded with it.
    pub send_when_specific_agent_finishes: Option<serde_json::Value>,
}

/// The timestamps the row's labels, sections and order are derived from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionTiming {
    pub created_at: Option<String>,
    pub created_ms: Option<i64>,
    pub last_interaction_ms: Option<i64>,
    pub working_started_ms: Option<i64>,
    pub snoozed_until_ms: Option<i64>,
}

impl SessionRow {
    /// The compact countdown a row draws instead of its relative time, at the host's clock.
    pub fn timer_label(&self, now_ms: u64) -> Option<String> {
        timer_trailing_label(self, now_ms)
    }

    /// The relative time a row draws (`5m`), at the host's clock.
    pub fn last_interaction_label(&self, now_ms: u64) -> Option<String> {
        self.last_interaction_at
            .as_deref()
            .map(|at| last_interaction_label(at, now_ms))
    }

    /// The next host time at which the time this row draws reads differently, or `None` when it
    /// draws none or draws one that never moves. `show_relative_time` is the card setting: with it
    /// off, only a countdown is drawn. A host that draws these wakes then and no more often;
    /// nothing in the store reports it, because they are formatted against the host's clock.
    pub fn next_label_deadline(
        &self,
        now_ms: u64,
        show_relative_time: bool,
    ) -> Option<LabelDeadline> {
        next_label_deadline(self, now_ms, show_relative_time)
    }
}

/// The next moment a row's time reads differently, and which of the two times it is. The kind is
/// what tells a host whether a wake once a second is a countdown doing its job or a relative time
/// booked for a row that does not draw one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelDeadline {
    /// A Delayed Send or Close After Done counting down; it moves every second until it ends.
    Countdown(u64),
    /// The relative time of the last interaction; it moves by the second only in the first minute.
    Relative(u64),
}

impl LabelDeadline {
    pub fn at_ms(self) -> u64 {
        match self {
            LabelDeadline::Countdown(at) | LabelDeadline::Relative(at) => at,
        }
    }
}

/// A Space button.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpaceView {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub selected: bool,
    pub contains_active_session: bool,
    pub working_count: usize,
    pub attention_count: usize,
    pub background_work_count: usize,
}

/// A collection (a colored folder of projects).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CollectionView {
    pub collection_id: String,
    /// `<section key>:<collection id>`, the key its UI state is stored under.
    pub storage_id: String,
    pub title: String,
    pub color: String,
    pub group_ids: Vec<String>,
    pub collapsed: bool,
    pub contains_active_session: bool,
    pub working_count: usize,
    pub attention_count: usize,
    pub background_work_count: usize,
    pub awake_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderKind {
    Project,
    Collection,
}

/// One row of the top-level sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderItem {
    pub kind: OrderKind,
    pub id: String,
}

/// What the list says when it draws nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmptyState {
    pub loading: bool,
    pub error: bool,
    pub can_add_project: bool,
    pub copy: String,
}
