//! The client storage Quick Access reads and writes in place: the sidebar's hidden projects and
//! project collections for the Projects tab, and the chat's drafts and sent history for the Saved
//! Prompts tab's Recovered and Sent views.
//!
//! SEE-ALSO: packages/gx-core/src/quick_access/data.rs (`QuickAccessStorage`),
//! apps/desktop/src/app/gx_chat/saved_prompt_records.rs (the chat records).

use ghostex_gx_core::{
    HIDDEN_ITEMS_STORAGE_KEY, PROJECT_COLLECTIONS_STORAGE_KEY, QuickAccessCollection,
    QuickAccessHiddenItems, QuickAccessRecoveredDraft, QuickAccessStorage,
    hidden_items_from_storage,
};
use serde_json::Value;

use crate::app::gx_chat::saved_prompt_records as chat;
use crate::app::gx_store::read_preference_value;

/// The desktop's client storage, at one instant.
pub(crate) struct DesktopQuickAccessStorage {
    pub(crate) now_ms: i64,
}

impl QuickAccessStorage for DesktopQuickAccessStorage {
    /// `readSidebarHiddenItems`.
    fn hidden_items(&mut self) -> QuickAccessHiddenItems {
        let raw = read_preference_value(HIDDEN_ITEMS_STORAGE_KEY)
            .ok()
            .flatten();
        let items = hidden_items_from_storage(raw.as_deref());
        QuickAccessHiddenItems {
            collection_keys: items.collection_keys,
            group_ids: items.group_ids,
        }
    }

    /// `readSidebarProjectCollections().collections`, sanitized the way the TypeScript sanitized
    /// it: a collection needs an id and a title, and a project belongs to the first collection that
    /// names it.
    fn project_collections(&mut self) -> Vec<QuickAccessCollection> {
        let Some(raw) = read_preference_value(PROJECT_COLLECTIONS_STORAGE_KEY)
            .ok()
            .flatten()
        else {
            return Vec::new();
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
            return Vec::new();
        };
        let mut seen_collections: Vec<String> = Vec::new();
        let mut seen_projects: Vec<String> = Vec::new();
        let mut collections = Vec::new();
        for candidate in parsed["collections"].as_array().into_iter().flatten() {
            let trimmed = |value: &Value, max: usize| -> String {
                value
                    .as_str()
                    .map(|text| text.trim().chars().take(max).collect())
                    .unwrap_or_default()
            };
            let collection_id = trimmed(&candidate["collectionId"], 120);
            let title = trimmed(&candidate["title"], 80);
            if collection_id.is_empty()
                || title.is_empty()
                || seen_collections.contains(&collection_id)
            {
                continue;
            }
            seen_collections.push(collection_id.clone());
            let mut project_ids = Vec::new();
            for project in candidate["projectIds"].as_array().into_iter().flatten() {
                let Some(project) = project.as_str() else {
                    continue;
                };
                let project: String = project.trim().chars().take(300).collect();
                if project.is_empty() || seen_projects.contains(&project) {
                    continue;
                }
                seen_projects.push(project.clone());
                project_ids.push(project);
            }
            if project_ids.is_empty() {
                continue;
            }
            collections.push(QuickAccessCollection {
                collection_id,
                project_ids,
            });
        }
        collections
    }

    fn recovered_drafts(&mut self) -> Vec<QuickAccessRecoveredDraft> {
        chat::list_recovered_drafts(self.now_ms)
            .into_iter()
            .map(|draft| QuickAccessRecoveredDraft {
                session_key: draft.session_key,
                recovery_id: None,
                project_id: draft.project_id,
                session_id: draft.session_id,
                text: draft.text,
                updated_at: draft.updated_at,
            })
            .collect()
    }

    fn sent_messages(&mut self) -> Vec<Value> {
        chat::list_sent(self.now_ms)
    }

    fn delete_sent_message(&mut self, prompt_id: &str) {
        chat::delete_sent(prompt_id, self.now_ms);
    }

    fn dismiss_draft_recovery(&mut self, recovery_id: &str) {
        chat::dismiss_recovery(recovery_id, self.now_ms);
    }

    fn delete_stored_draft(&mut self, session_key: &str) {
        chat::delete_stored_draft(session_key, self.now_ms);
    }

    fn import_draft_recovery(&mut self, recovery_drafts: &Value) {
        chat::import_recovery(recovery_drafts, self.now_ms);
    }

    fn reconcile_drafts_from_server(&mut self, drafts: &Value) {
        chat::reconcile_drafts(drafts, self.now_ms);
    }

    fn record_delivered_drafts(&mut self, delivered_drafts: &Value) {
        chat::record_delivered(delivered_drafts, self.now_ms);
    }
}
