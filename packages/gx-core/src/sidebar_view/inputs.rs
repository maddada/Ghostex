//! Everything the sidebar view model reads besides the store: the sidebar's own UI state, the
//! settings it depends on, and the facts only the host knows (browser tabs, git diff stats, the
//! close-after-done and delayed-send timers it owns).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::tags::{TagCatalog, TagListItem};

/// The machine tab id of this computer's daemon.
pub const LOCAL_MACHINE_ID: &str = "local";

/// The connection state word of a machine the host can build a list for.
pub const MACHINE_STATE_CONNECTED: &str = "connected";

/// One machine tab, as the host knows it.
///
/// The machine tabs are the host's list, not the store's: which machines are saved, whether they
/// are enabled in the sidebar, what the user named them, and how their SSH tunnel is doing are all
/// facts of the remote transport. The store only holds their rows, and only for the ones the host
/// says it feeds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineTabInput {
    /// [`LOCAL_MACHINE_ID`], or the saved remote machine's settings id.
    pub machine_id: String,
    pub label: String,
    /// The host's connection state word (`connected`, `connecting`, `disconnected`, `sshFailed`,
    /// and the rest of the connect ladder). Always `connected` for this computer.
    pub state: String,
    /// The already-sanitized failure summary, when the host has one.
    pub message: Option<String>,
    /// The host subscribes to this machine's presentation and feeds it into the store, so a list
    /// can be built for it. A machine the host does not feed keeps whatever the caller drew.
    pub fed: bool,
}

impl MachineTabInput {
    pub fn is_local(&self) -> bool {
        self.machine_id == LOCAL_MACHINE_ID
    }

    pub fn is_connected(&self) -> bool {
        self.state == MACHINE_STATE_CONNECTED
    }
}

/// Which sessions a project's list shows first, and in which order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionSortMode {
    /// Pinned first, then by activity and recency.
    #[default]
    LastActivity,
    /// Pinned first, then the saved order.
    Manual,
}

/// The six section headings of a project's session list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SectionId {
    Browser,
    Pinned,
    Drafts,
    Sessions,
    Parked,
    Snoozed,
}

impl SectionId {
    /// Render order of the headings.
    pub const ORDER: [SectionId; 6] = [
        SectionId::Browser,
        SectionId::Pinned,
        SectionId::Drafts,
        SectionId::Sessions,
        SectionId::Parked,
        SectionId::Snoozed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SectionId::Browser => "browser",
            SectionId::Pinned => "pinned",
            SectionId::Drafts => "drafts",
            SectionId::Sessions => "sessions",
            SectionId::Parked => "parked",
            SectionId::Snoozed => "snoozed",
        }
    }
}

/// Which of a project's section headings are collapsed. Drafts, Parked, and Snoozed start
/// collapsed; only Pinned and Sessions are persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionCollapse {
    pub browser: bool,
    pub pinned: bool,
    pub drafts: bool,
    pub sessions: bool,
    pub parked: bool,
    pub snoozed: bool,
}

impl Default for SectionCollapse {
    fn default() -> Self {
        Self {
            browser: false,
            pinned: false,
            drafts: true,
            sessions: false,
            parked: true,
            snoozed: true,
        }
    }
}

impl SectionCollapse {
    pub fn get(&self, section: SectionId) -> bool {
        match section {
            SectionId::Browser => self.browser,
            SectionId::Pinned => self.pinned,
            SectionId::Drafts => self.drafts,
            SectionId::Sessions => self.sessions,
            SectionId::Parked => self.parked,
            SectionId::Snoozed => self.snoozed,
        }
    }

    pub fn set(&mut self, section: SectionId, collapsed: bool) {
        match section {
            SectionId::Browser => self.browser = collapsed,
            SectionId::Pinned => self.pinned = collapsed,
            SectionId::Drafts => self.drafts = collapsed,
            SectionId::Sessions => self.sessions = collapsed,
            SectionId::Parked => self.parked = collapsed,
            SectionId::Snoozed => self.snoozed = collapsed,
        }
    }
}

/// What the user collapsed and expanded. Group ids key the project rows; the storage id (a
/// project's own id, or the group id for a user-made group) keys everything below a project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarCollapseState {
    pub collapsed_groups: BTreeSet<String>,
    /// Keyed by `<section key>:<collection id>`.
    pub collapsed_collections: BTreeSet<String>,
    pub expanded_session_lists: BTreeSet<String>,
    pub expanded_hover_actions: BTreeSet<String>,
    pub section_collapse: BTreeMap<String, SectionCollapse>,
    /// Keyed by section key (`local`, `remote:<machine>`).
    pub selected_space_by_section: BTreeMap<String, String>,
    /// The sessions each Space of each section was last focused into, newest first, capped at
    /// [`MAX_RECENT_SPACE_SESSION_IDS`]. Keyed by section key, then by Space id.
    ///
    /// This is what a Space switch restores the focus to when `sidebarSpaceSwitchBehavior` is
    /// `restore`. It is written on every focus change, which is why it is a plain list and not
    /// something richer: a value the sidebar writes that often has to be cheap to compare.
    pub recent_sessions_by_space: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

/// `MAX_RECENT_SIDEBAR_SPACE_SESSION_IDS`.
pub const MAX_RECENT_SPACE_SESSION_IDS: usize = 20;

/// Projects and collections the user hid from the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarHiddenItems {
    pub group_ids: Vec<String>,
    /// `<section key>:<collection id>`.
    pub collection_keys: Vec<String>,
}

/// The sidebar's own state: what is collapsed, hidden, filtered, and selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarUiState {
    pub selected_machine_id: String,
    pub collapse: SidebarCollapseState,
    pub hidden_items: SidebarHiddenItems,
    pub show_hidden: bool,
    /// Tag filters the user ticked, already pruned to the enabled and visible ones.
    pub selected_tag_filters: Vec<String>,
    /// Sidebar row ids of a multi-selection.
    pub selected_session_ids: Vec<String>,
}

impl Default for SidebarUiState {
    fn default() -> Self {
        Self {
            selected_machine_id: LOCAL_MACHINE_ID.to_string(),
            collapse: SidebarCollapseState::default(),
            hidden_items: SidebarHiddenItems::default(),
            show_hidden: false,
            selected_tag_filters: Vec::new(),
            selected_session_ids: Vec::new(),
        }
    }
}

impl SidebarUiState {
    /// The key a section's Space selection and collection storage ids are built from.
    pub fn section_key(&self) -> String {
        if self.selected_machine_id == LOCAL_MACHINE_ID {
            LOCAL_MACHINE_ID.to_string()
        } else {
            format!("remote:{}", self.selected_machine_id)
        }
    }
}

/// The settings the rows and the list depend on, with the defaults and clamps of
/// `normalizeghostexSettings`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSettings {
    pub enable_session_parking: bool,
    pub project_session_list_collapsed_count: u32,
    pub sidebar_spaces_enabled: bool,
    /// The section follows the active session into its Space.
    pub sidebar_space_follow_active_session: bool,
    /// A session card draws the relative time of its last interaction.
    pub show_last_active_time: bool,
    pub debugging_mode: bool,
    /// The user's tag filter list, raw as it sits in settings; normalized where it is read.
    pub tag_list_items: Value,
    /// The sort mode the sidebar HUD publishes.
    pub sort_mode: SessionSortMode,
    /// The hover button strip, raw as it sits in settings; normalized where it is read.
    pub session_card_hover_buttons: Value,
    /// Park and Snooze offer the tag submenu.
    pub show_tag_menu_when_parking: bool,
    /// The hover strip's actions are repeated at the top of the row's context menu.
    pub show_session_card_hover_buttons_in_context_menu: bool,
    pub show_session_command_copy_actions: bool,
    pub show_session_details_copy_action: bool,
    pub show_beta_features: bool,
    pub hide_keep_awake_titlebar_control: bool,
    /// The project header offers New Browser Tab.
    pub browser_view_tab_hidden: bool,
    /// A project slot hotkey (cmd+ctrl+1..9 by default) expands the project it jumps to.
    pub expand_collapsed_projects_on_jump: bool,
    /// That jump also puts the project's session list back to the compact one.
    pub show_less_for_expanded_project_jumps: bool,
}

impl Default for SidebarSettings {
    fn default() -> Self {
        Self {
            enable_session_parking: true,
            project_session_list_collapsed_count: 13,
            sidebar_spaces_enabled: false,
            sidebar_space_follow_active_session: false,
            show_last_active_time: true,
            debugging_mode: false,
            tag_list_items: Value::Null,
            sort_mode: SessionSortMode::LastActivity,
            session_card_hover_buttons: Value::Null,
            show_tag_menu_when_parking: true,
            show_session_card_hover_buttons_in_context_menu: true,
            show_session_command_copy_actions: false,
            show_session_details_copy_action: false,
            show_beta_features: false,
            hide_keep_awake_titlebar_control: false,
            browser_view_tab_hidden: false,
            expand_collapsed_projects_on_jump: true,
            show_less_for_expanded_project_jumps: false,
        }
    }
}

impl SidebarSettings {
    /// Reads the saved settings object with the TypeScript defaults and clamps.
    pub fn from_settings_json(settings: &Value, sort_mode: SessionSortMode) -> Self {
        let defaults = Self::default();
        let boolean = |key: &str, fallback: bool| {
            settings
                .get(key)
                .and_then(Value::as_bool)
                .unwrap_or(fallback)
        };
        let count = settings
            .get("projectSessionListCollapsedCount")
            .and_then(Value::as_f64)
            .map_or(defaults.project_session_list_collapsed_count, |value| {
                // `Math.round` rounds a half up, towards positive infinity.
                let rounded = (value + 0.5).floor();
                rounded.clamp(1.0, 50.0) as u32
            });
        Self {
            enable_session_parking: boolean(
                "enableSessionParking",
                defaults.enable_session_parking,
            ),
            project_session_list_collapsed_count: count,
            sidebar_spaces_enabled: boolean(
                "sidebarSpacesEnabled",
                defaults.sidebar_spaces_enabled,
            ),
            sidebar_space_follow_active_session: boolean(
                "sidebarSpaceFollowActiveSession",
                defaults.sidebar_space_follow_active_session,
            ),
            show_last_active_time: !boolean("hideLastActiveTimeOnSessionCards", false),
            debugging_mode: boolean("debuggingMode", defaults.debugging_mode),
            tag_list_items: settings
                .get("sidebarSessionTagListItems")
                .cloned()
                .unwrap_or(Value::Null),
            sort_mode,
            session_card_hover_buttons: settings
                .get("sessionCardHoverButtons")
                .cloned()
                .unwrap_or(Value::Null),
            show_tag_menu_when_parking: boolean(
                "showTagMenuWhenParking",
                defaults.show_tag_menu_when_parking,
            ),
            show_session_card_hover_buttons_in_context_menu: boolean(
                "showSessionCardHoverButtonsInContextMenu",
                defaults.show_session_card_hover_buttons_in_context_menu,
            ),
            show_session_command_copy_actions: boolean(
                "showSessionCommandCopyActions",
                defaults.show_session_command_copy_actions,
            ),
            show_session_details_copy_action: boolean(
                "showSessionDetailsCopyAction",
                defaults.show_session_details_copy_action,
            ),
            show_beta_features: boolean("showBetaFeatures", defaults.show_beta_features),
            hide_keep_awake_titlebar_control: boolean(
                "hideKeepAwakeTitlebarControl",
                defaults.hide_keep_awake_titlebar_control,
            ),
            browser_view_tab_hidden: boolean(
                "browserViewTabHidden",
                defaults.browser_view_tab_hidden,
            ),
            expand_collapsed_projects_on_jump: boolean(
                "expandCollapsedProjectsOnJump",
                defaults.expand_collapsed_projects_on_jump,
            ),
            show_less_for_expanded_project_jumps: boolean(
                "showLessForExpandedProjectJumps",
                defaults.show_less_for_expanded_project_jumps,
            ),
        }
    }

    /// The tag filters the Sort & Filter menu offers.
    pub(crate) fn enabled_tag_filters(&self, catalog: &TagCatalog) -> Vec<String> {
        super::tags::enabled_visible_tag_filters(&self.tag_list_items, catalog)
    }

    /// The filters the Sort & Filter menu offers, for a host that has to prune a ticked one the
    /// settings or the daemon's catalog took away.
    pub fn offered_tag_filters(
        &self,
        catalog_state: Option<&ghostex_gx_protocol::CustomSessionTagsState>,
    ) -> Vec<String> {
        self.enabled_tag_filters(&TagCatalog::from_state(catalog_state))
    }

    /// The tag filter list itself, for a menu builder.
    pub fn tag_list(
        &self,
        catalog_state: Option<&ghostex_gx_protocol::CustomSessionTagsState>,
    ) -> Vec<TagListItem> {
        let catalog = TagCatalog::from_state(catalog_state);
        super::tags::normalize_tag_list_items(&self.tag_list_items, Some(&catalog))
    }
}

/// One browser tab the host shows as a sidebar row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowserTabInput {
    pub project_id: String,
    pub tab_id: String,
    pub title: String,
    pub favicon_url: Option<String>,
    pub is_active: bool,
    pub is_sleeping: bool,
    pub is_visible: bool,
}

/// The git numbers a project header draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectDiffStats {
    pub additions: i64,
    pub deletions: i64,
    pub files: i64,
    pub is_loading: bool,
    pub is_repo: bool,
}

/// A session's Close After Done timer, owned by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloseAfterDoneInput {
    pub armed: bool,
    pub deadline_at: Option<String>,
    pub remaining_label: Option<String>,
    pub remaining_ms: Option<i64>,
}

/// A session's Delayed Send, from the host's own timers. The daemon's own delayed send, when it
/// has one, wins over this.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DelayedSendInput {
    pub deadline_at: Option<String>,
    pub remaining_label: Option<String>,
    pub remaining_ms: Option<i64>,
    pub send_when_all_project_sessions_stop_active: bool,
    pub send_when_agent_stops_active: bool,
}

/// Whether the daemon is reachable, and since when, for the empty-state copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnavailableState {
    /// Host time the machine was first seen unavailable, while it still is.
    pub since_ms: Option<u64>,
    /// The machine was loaded with rows at least once since the app started.
    pub observed_available: bool,
}

/// Facts the host owns and the store does not hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarHostInputs {
    /// The app tabs this list DRAWS as rows, in the order the host publishes them. Nothing acts on
    /// this list: since app tabs moved to the view panel's tab strip the desktop host supplies none
    /// at all, and the project actions are sessions only (`sidebar_actions/bulk.rs`).
    pub browser_tabs: Vec<BrowserTabInput>,
    /// The sidebar's own copy of the project collections, as client storage holds it. Read only
    /// while the daemon has published none: the sidebar shows this copy until its first adoption.
    pub stored_project_collections: Option<Value>,
    /// The machine tabs, this computer first, in the order the sidebar draws them.
    pub machines: Vec<MachineTabInput>,
    /// Projects the daemon parked as Recent Projects, by their own id. The daemon keeps them out
    /// of the presentation, so this list only matters while a project is being parked or
    /// restored, but it is the authoritative one and the list hides them either way.
    ///
    /// This computer's only; a remote machine's parked projects are in
    /// [`Self::remote_recent_project_ids`], because a project id is unique per daemon and two
    /// machines can hand out the same one.
    pub recent_project_ids: BTreeSet<String>,
    /// The same list per REMOTE machine, keyed by machine id.
    pub remote_recent_project_ids: BTreeMap<String, BTreeSet<String>>,
    /// By project id.
    pub project_diff_stats: BTreeMap<String, ProjectDiffStats>,
    /// By sidebar session id.
    pub local_delayed_sends: BTreeMap<String, DelayedSendInput>,
    /// How many Recent Projects the host knows of, this machine's and every remote machine's.
    /// The empty state reads it to tell a first run from a list the user emptied.
    pub recent_project_count: usize,
    pub unavailable: UnavailableState,
}

impl SidebarHostInputs {
    /// The machine tab with this id.
    pub fn machine(&self, machine_id: &str) -> Option<&MachineTabInput> {
        self.machines
            .iter()
            .find(|machine| machine.machine_id == machine_id)
    }

    /// Whether the host feeds this machine's presentation into the store. This computer always is;
    /// a remote machine is only while its client runs, and an empty machine list is the moment
    /// before the host has published one.
    pub fn feeds(&self, machine_id: &str) -> bool {
        machine_id == LOCAL_MACHINE_ID
            || self.machine(machine_id).is_some_and(|machine| machine.fed)
    }

    /// The projects a machine has parked as Recent Projects.
    pub(crate) fn parked_project_ids(&self, machine: &crate::keys::MachineId) -> &BTreeSet<String> {
        static EMPTY: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
        match machine.remote_id() {
            None => &self.recent_project_ids,
            Some(machine_id) => self
                .remote_recent_project_ids
                .get(machine_id)
                .unwrap_or_else(|| EMPTY.get_or_init(BTreeSet::new)),
        }
    }
}

/// Everything besides the store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarInputs {
    pub ui: SidebarUiState,
    pub settings: SidebarSettings,
    pub host: SidebarHostInputs,
}
