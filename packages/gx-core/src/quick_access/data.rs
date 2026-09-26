//! What the Quick Access model reads: the sidebar's groups and sessions, the HUD, and the few host
//! facts the TypeScript controller read from the zustand store or client storage.

use std::collections::BTreeMap;

use ghostex_gx_protocol::CustomSessionTagsState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::text::HotkeyPlatform;

/// One session row, with the fields `SidebarSessionItem` carries that Quick Access reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessSession {
    /// The sidebar id: `combined-session:…`, `remote:…:session:…` or `gpui-browser:…`.
    pub session_id: String,
    pub last_interaction_at: Option<String>,
    pub alias: String,
    /// The daemon's own `displayTitle`, before the heading rules.
    pub display_title: Option<String>,
    pub primary_title: Option<String>,
    pub terminal_title: Option<String>,
    pub detail: Option<String>,
    pub session_number: Option<String>,
    pub session_tag: Option<String>,
    pub is_favorite: bool,
    pub favicon_data_url: Option<String>,
    pub agent_icon: Option<String>,
    pub session_kind: Option<String>,
    pub lifecycle_state: Option<String>,
    pub agent_session_id: Option<String>,
    pub session_routing_id: Option<String>,
}

/// One sidebar group, in the order the store holds them (`groupOrder`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessStoreGroup {
    pub group_id: String,
    pub title: String,
    /// `projectContext.editor.projectId`.
    pub editor_project_id: Option<String>,
    /// `remoteMachineContext.machineId`, for a group of a remote machine.
    pub remote_machine_id: Option<String>,
    /// `remoteMachineContext.projectId`; absent for a remote machine's Chats group.
    pub remote_project_id: Option<String>,
    /// `sessionIdsByGroup[groupId]` resolved through `sessionsById`, in store order.
    pub sessions: Vec<QuickAccessSession>,
}

/// `SidebarCommandRunFeedbackState`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessRunState {
    pub status: String,
    pub active_run_ids: Vec<String>,
}

/// One "Open In" target the user can see: every visible built-in and custom target except Finder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessOpenTarget {
    pub id: String,
    pub label: String,
    /// A custom target, which the Commands tab searches with different words.
    pub custom: bool,
}

/// Everything the model reads that the host owns, read at the moment the model asks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessData {
    /// The sidebar HUD as the store holds it (`settings`, `commands`, `activeProjectId`,
    /// `activeProjectSpaceRefs`).
    pub hud: Value,
    pub groups: Vec<QuickAccessStoreGroup>,
    pub command_run_states: BTreeMap<String, QuickAccessRunState>,
    /// This computer's custom session tags (`store.customSessionTags`).
    pub local_custom_tags: Option<CustomSessionTagsState>,
    /// Every remote machine's, in the store's order.
    pub remote_custom_tags: Vec<CustomSessionTagsState>,
    pub open_targets: Vec<QuickAccessOpenTarget>,
    pub platform: HotkeyPlatformWire,
}

/// [`HotkeyPlatform`] as the replay files spell it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyPlatformWire {
    #[default]
    Mac,
    Windows,
    Linux,
}

impl From<HotkeyPlatformWire> for HotkeyPlatform {
    fn from(value: HotkeyPlatformWire) -> Self {
        match value {
            HotkeyPlatformWire::Mac => HotkeyPlatform::Mac,
            HotkeyPlatformWire::Windows => HotkeyPlatform::Windows,
            HotkeyPlatformWire::Linux => HotkeyPlatform::Linux,
        }
    }
}

impl QuickAccessData {
    pub(crate) fn settings(&self) -> &Value {
        &self.hud["settings"]
    }

    pub(crate) fn platform(&self) -> HotkeyPlatform {
        self.platform.into()
    }

    /// `sessionsById[sessionId]`.
    pub(crate) fn session(&self, session_id: &str) -> Option<&QuickAccessSession> {
        self.groups
            .iter()
            .flat_map(|group| group.sessions.iter())
            .find(|session| session.session_id == session_id)
    }

    /// `hud.settings.petOverlayEnabled === true`.
    pub(crate) fn pet_overlay_enabled(&self) -> bool {
        self.settings()["petOverlayEnabled"] == Value::Bool(true)
    }
}

/// A local draft the Recovered view lists (`RecoveredSessionChatDraft`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessRecoveredDraft {
    pub session_key: String,
    pub recovery_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub text: String,
    pub updated_at: i64,
}

/// The sidebar's hidden projects and collections (`ghostex.sidebar.hidden-items.v1`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessHiddenItems {
    pub collection_keys: Vec<String>,
    pub group_ids: Vec<String>,
}

/// One local project collection (`readSidebarProjectCollections().collections`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickAccessCollection {
    pub collection_id: String,
    pub project_ids: Vec<String>,
}

/// The client storage the TypeScript controller read and wrote in place, synchronously. The host
/// implements it over the same keys; the model never touches storage itself.
pub trait QuickAccessStorage {
    fn hidden_items(&mut self) -> QuickAccessHiddenItems;
    fn project_collections(&mut self) -> Vec<QuickAccessCollection>;
    /// `listRecoveredSessionChatDrafts()`.
    fn recovered_drafts(&mut self) -> Vec<QuickAccessRecoveredDraft>;
    /// `listSentSessionChatMessages()`: stashed-prompt shaped records, newest first.
    fn sent_messages(&mut self) -> Vec<Value>;
    fn delete_sent_message(&mut self, prompt_id: &str);
    fn dismiss_draft_recovery(&mut self, recovery_id: &str);
    fn delete_stored_draft(&mut self, session_key: &str);
    /// `importDraftRecovery(message.recoveryDrafts ?? [])`.
    fn import_draft_recovery(&mut self, recovery_drafts: &Value);
    /// `reconcileSessionChatDraftsFromServer(message.drafts ?? [])`.
    fn reconcile_drafts_from_server(&mut self, drafts: &Value);
    /// `recordDeliveredSessionChatDrafts(message.deliveredDrafts ?? [])`.
    fn record_delivered_drafts(&mut self, delivered_drafts: &Value);
}
