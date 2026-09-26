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
    /// Shared with the store's runtime facts holder rather than copied per install
    /// (gx_store/runtime_facts.rs).
    pub(crate) hud: std::sync::Arc<Value>,
    pub(crate) groups: Vec<NativeSidebarGroup>,
    pub(crate) selected_machine_id: String,
    pub(crate) machines: Vec<NativeSidebarMachine>,
    pub(crate) spaces: Vec<NativeSidebarSpace>,
    pub(crate) spaces_enabled: bool,
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
    pub(crate) expanded: bool,
    pub(crate) hidden_session_count: usize,
    pub(crate) show_list_toggle: bool,
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
    pub(crate) session_ids: Vec<String>,
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
