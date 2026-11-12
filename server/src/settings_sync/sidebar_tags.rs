//! The custom Session Tags as Sidebar organization keys (the `spaces` category): one key per tag
//! and one for their order, so two computers' tags add up instead of one catalog replacing the
//! other.
//!
//! Keys: `spaces/sessionTag/<tagId>` (the tag's name, colour and icon; deleted with the tag) and
//! `spaces/sessionTagOrder` (the ids in order). The Session Tags list in Settings, which places
//! these tags among the built-in ones, syncs as the `sidebarSessionTagListItems` setting.
//!
//! CDXC:SettingsSync 2026-11-12 WHY:
//! A tag id is random (`custom-<token>`), so the tags two computers made apart never share a key and both survive their first sync; a whole-catalog value would have let the older catalog's tags vanish, and with them the tag on every session that carried one. A tag deleted on one computer is deleted on the others, as the Session Tags editor does on one computer. Supersedes "a remote daemon's tags stay that daemon's own" (CDXC:Sessions 2026-09-11 in custom_session_tags.rs) while Sidebar organization sync is on.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::custom_session_tags::{read_custom_session_tags, write_custom_session_tags_and_publish};
use crate::domain::{DomainRepository, DomainStateError};

use super::engine::Engine;

const TAG_KEY_PREFIX: &str = "spaces/sessionTag/";
const TAG_ORDER_KEY: &str = "spaces/sessionTagOrder";

/// A Session Tags key (rather than a Space, a group, a project's place or a setting).
pub(super) fn is_session_tag_key(key: &str) -> bool {
    key == TAG_ORDER_KEY || key.starts_with(TAG_KEY_PREFIX)
}

impl Engine<'_> {
    /// This computer's custom tags, by synced key.
    pub(super) fn session_tag_values(&self) -> Result<BTreeMap<String, Value>, DomainStateError> {
        let catalog = read_custom_session_tags(self.db)?;
        let mut values = BTreeMap::new();
        if let Some(tags) = catalog.get("tags").and_then(Value::as_object) {
            for (id, tag) in tags {
                values.insert(format!("{TAG_KEY_PREFIX}{id}"), tag.clone());
            }
        }
        if let Some(order) = catalog
            .get("order")
            .filter(|order| order.as_array().is_some_and(|order| !order.is_empty()))
        {
            values.insert(TAG_ORDER_KEY.to_string(), order.clone());
        }
        Ok(values)
    }

    /// Makes this computer's tags say what `wanted` (key and value, `None` deletes) says.
    pub(super) fn write_session_tags(
        &self,
        wanted: &[(String, Option<Value>, String)],
    ) -> Result<(), DomainStateError> {
        let wanted: Vec<&(String, Option<Value>, String)> = wanted
            .iter()
            .filter(|(key, _, _)| is_session_tag_key(key))
            .collect();
        if wanted.is_empty() {
            return Ok(());
        }
        let catalog = read_custom_session_tags(self.db)?;
        let mut tags: Map<String, Value> = catalog
            .get("tags")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut order: Vec<String> = catalog
            .get("order")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        for (key, value, _) in wanted {
            if key == TAG_ORDER_KEY {
                if let Some(synced) = value.as_ref().and_then(Value::as_array) {
                    order = synced
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect();
                }
            } else if let Some(id) = key.strip_prefix(TAG_KEY_PREFIX) {
                match value {
                    Some(tag) => {
                        tags.insert(id.to_string(), tag.clone());
                    }
                    None => {
                        tags.remove(id);
                    }
                }
            }
        }
        // The synced order, then any tag it doesn't name yet (one made elsewhere a moment ago).
        let mut seen = std::collections::BTreeSet::new();
        order.retain(|id| tags.contains_key(id) && seen.insert(id.clone()));
        for id in tags.keys() {
            if !order.contains(id) {
                order.push(id.clone());
            }
        }
        let params =
            Map::from_iter([("state".to_string(), json!({ "order": order, "tags": tags }))]);
        let repository = DomainRepository::new(self.db, &self.server_id);
        write_custom_session_tags_and_publish(self.state, self.db, &repository, &params)?;
        Ok(())
    }
}
