use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarSnapshot {
    pub(crate) scroll_scope: String,
    pub(crate) rename_request: Option<NativeSidebarRenameRequest>,
    pub(crate) reveal_request: Option<NativeSidebarRevealRequest>,
    pub(crate) empty_state: Value,
    /// The selected remote machine is not connected: the list says why above what it holds.
    #[serde(skip)]
    pub(crate) machine_notice: Option<ghostex_gx_core::MachineNotice>,
    /// The row after the list that brings the projects with no sessions back.
    #[serde(skip)]
    pub(crate) show_all_projects_row: Option<ghostex_gx_core::ShowAllProjectsRow>,
    /// Shared with the store's runtime facts holder rather than copied per install
    /// (gx_store/runtime_facts.rs).
    pub(crate) hud: std::sync::Arc<Value>,
    pub(crate) groups: Vec<NativeSidebarGroup>,
    pub(crate) selected_machine_id: String,
    pub(crate) machines: Vec<NativeSidebarMachine>,
    pub(crate) spaces: Vec<NativeSidebarSpace>,
    pub(crate) spaces_enabled: bool,
    /// The Bots extension is on, so the Hermes button is drawn.
    pub(crate) bots_enabled: bool,
    /// The list shows the Hermes bots rather than the projects.
    pub(crate) bots_mode: bool,
    /// The Bots list starts with the pinned Automations row.
    pub(crate) automations_row: bool,
    /// The runs every bot delivered today, drawn on the Automations row.
    pub(crate) automations_today: u64,
    pub(crate) collections: Vec<NativeSidebarCollection>,
    pub(crate) order: Vec<NativeSidebarOrderItem>,
    pub(crate) more_menu: Value,
    pub(crate) search_shortcut: Option<String>,
    pub(crate) commands_shortcut: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarGroup {
    pub(crate) title_tooltip: Option<String>,
    pub(crate) group_id: String,
    pub(crate) storage_id: String,
    pub(crate) summary: Value,
    pub(crate) collapsed: bool,
    pub(crate) hover_actions_expanded: bool,
    /// Behind an `Arc` because they are the heavy part of a group and never change on their own:
    /// the header buttons carry the agent launcher, whose rows each hold a logo data URL of up to
    /// eight kilobytes. A snapshot is cloned whole on every patch and whenever a clock wake moves
    /// one row's label, and those two must not copy a few hundred kilobytes of menu JSON.
    pub(crate) menu: Arc<Value>,
    pub(crate) header_actions: Arc<Vec<Value>>,
    pub(crate) sections: Vec<NativeSidebarSection>,
    pub(crate) title: String,
    pub(crate) is_active: bool,
    #[serde(default)]
    pub(crate) is_stale: bool,
    /// The project a user-made session group belongs to (gx-core `owner_project_group_id`); the
    /// list draws the group inside that project's block.
    #[serde(default)]
    pub(crate) owner_group_id: Option<String>,
    /// One of this project's session groups holds the focused session.
    #[serde(default)]
    pub(crate) holds_active_session_group: bool,
    pub(crate) project_context: Option<Value>,
    pub(crate) remote_machine_context: Option<Value>,
    #[serde(deserialize_with = "deserialize_sessions")]
    pub(crate) sessions: Vec<Arc<NativeSidebarSession>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarSection {
    pub(crate) id: String,
    pub(crate) collapsed: bool,
    pub(crate) count: usize,
    pub(crate) contains_active_session: bool,
    pub(crate) working_count: usize,
    pub(crate) attention_count: usize,
    #[serde(default)]
    pub(crate) background_work_count: usize,
    pub(crate) question_count: usize,
    /// Rows waiting for approval: the red dot (gx-core `SessionRow::needs_you`).
    #[serde(default)]
    pub(crate) approval_count: usize,
    pub(crate) session_ids: Vec<String>,
    /// The heading's right-click menu; only the Parked heading has one (Sleep All, Close All).
    #[serde(skip)]
    pub(crate) menu: Option<Arc<Value>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarSession {
    pub(crate) session_id: String,
    pub(crate) display_title: Option<String>,
    pub(crate) alias: String,
    pub(crate) activity: String,
    #[serde(default)]
    pub(crate) has_background_work: bool,
    #[serde(default)]
    pub(crate) model_selection_failed: bool,
    /// The session's program reported `error` (OSC 7501) and drives its status.
    #[serde(default)]
    pub(crate) program_status_failed: bool,
    /// Stuck on a permission prompt or approval card only the user can grant (the red dot).
    #[serde(default)]
    pub(crate) awaiting_approval: bool,
    pub(crate) agent_icon: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) session_kind: Option<String>,
    pub(crate) is_focused: bool,
    pub(crate) is_visible: bool,
    #[serde(default)]
    pub(crate) is_pinned: bool,
    #[serde(default)]
    pub(crate) is_draft: bool,
    pub(crate) last_interaction_at: Option<String>,
    pub(crate) lifecycle_state: Option<String>,
    pub(crate) session_note: Option<String>,
    pub(crate) favicon_data_url: Option<String>,
    #[serde(default)]
    pub(crate) has_composer_draft: bool,
    #[serde(default)]
    pub(crate) queued_prompt_count: u64,
    /// When gxserver gave up on the user's last rename; the title shakes for a fresh one
    /// (`ghostex_gx_core::rename_shake_key`).
    #[serde(default)]
    pub(crate) rename_failed_at: Option<String>,
    /// What a work-mode session is linked to, present only when it has links: the card's second
    /// line (native_sidebar/work_chips.rs).
    #[serde(skip)]
    pub(crate) work: Option<Arc<ghostex_gx_core::SessionWork>>,
    /// Every PR, GitHub issue and Linear ticket the session is linked to, present only when it has
    /// one: the line a hovered card opens (native_sidebar/link_chips.rs).
    #[serde(skip)]
    pub(crate) links: Option<Arc<ghostex_gx_core::SessionLinks>>,
    #[serde(flatten)]
    pub(crate) details: serde_json::Map<String, Value>,
}

impl NativeSidebarSession {
    pub(crate) fn title(&self) -> &str {
        self.display_title.as_deref().unwrap_or(&self.alias)
    }

    pub(crate) fn is_browser(&self) -> bool {
        self.kind.as_deref() == Some("browser") || self.session_kind.as_deref() == Some("browser")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarMachine {
    pub(crate) working_count: usize,
    pub(crate) attention_count: usize,
    #[serde(default)]
    pub(crate) background_work_count: usize,
    #[serde(default)]
    pub(crate) question_count: usize,
    /// Rows waiting for approval: the red dot (gx-core `SessionRow::needs_you`).
    #[serde(default)]
    pub(crate) approval_count: usize,
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) state: String,
    pub(crate) message: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarSpace {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon: String,
    pub(crate) color: String,
    pub(crate) selected: bool,
    pub(crate) contains_active_session: bool,
    pub(crate) working_count: usize,
    pub(crate) attention_count: usize,
    #[serde(default)]
    pub(crate) background_work_count: usize,
    #[serde(default)]
    pub(crate) question_count: usize,
    /// Rows waiting for approval: the red dot (gx-core `SessionRow::needs_you`).
    #[serde(default)]
    pub(crate) approval_count: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct NativeSidebarOrderItem {
    pub(crate) kind: String,
    pub(crate) id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarCollection {
    pub(crate) awake_count: u64,
    pub(crate) collection_id: String,
    pub(crate) title: String,
    pub(crate) color: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) collapsed: bool,
    pub(crate) contains_active_session: bool,
    pub(crate) working_count: usize,
    pub(crate) attention_count: usize,
    #[serde(default)]
    pub(crate) background_work_count: usize,
    #[serde(default)]
    pub(crate) question_count: usize,
    /// Rows waiting for approval: the red dot (gx-core `SessionRow::needs_you`).
    #[serde(default)]
    pub(crate) approval_count: usize,
    pub(crate) menu: Arc<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarRevealRequest {
    pub(crate) session_id: String,
    pub(crate) request_id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSidebarRenameRequest {
    pub(crate) collection_id: String,
    pub(crate) request_id: u64,
}

fn deserialize_sessions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Arc<NativeSidebarSession>>, D::Error> {
    Vec::<NativeSidebarSession>::deserialize(deserializer)
        .map(|sessions| sessions.into_iter().map(Arc::new).collect())
}
