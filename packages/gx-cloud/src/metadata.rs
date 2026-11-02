//! What Ghostex writes into a provider's metadata for every box it makes, so the provider's own
//! box list is the registry and any client (including the phone with the computer off) can
//! rebuild it: Instabox `metadata`, Boat name/labels, Docker labels for `local`.

use std::collections::BTreeMap;

use ghostex_gx_protocol::cloud_box::CloudBoxCreatedBy;

/// `ghostex=1` marks a box as Ghostex's; the provider list is filtered by it.
pub const MARKER_KEY: &str = "ghostex";
pub const MARKER_VALUE: &str = "1";
pub const BOX_ID_KEY: &str = "ghostex.box";
pub const PROJECT_KEY_KEY: &str = "ghostex.project";
pub const HOME_PROJECT_KEY: &str = "ghostex.homeProject";
pub const HOME_MACHINE_KEY: &str = "ghostex.homeMachine";
pub const SNAPSHOT_KEY: &str = "ghostex.snapshot";
pub const SIZE_KEY: &str = "ghostex.size";
pub const IDEMPOTENCY_KEY: &str = "ghostex.idempotency";
pub const CREATED_BY_KEY: &str = "ghostex.createdBy";
pub const CREATED_AT_KEY: &str = "ghostex.createdAt";

/// Ghostex's metadata of one box.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoxMetadata {
    pub box_id: String,
    pub project_key: String,
    pub home_project_id: Option<String>,
    pub home_machine_id: Option<String>,
    pub snapshot_id: Option<String>,
    pub size: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_by: Option<CloudBoxCreatedBy>,
    pub created_at: Option<String>,
}

impl BoxMetadata {
    /// The key/value pairs to write, marker first.
    pub fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = vec![
            (MARKER_KEY, MARKER_VALUE.to_string()),
            (BOX_ID_KEY, self.box_id.clone()),
            (PROJECT_KEY_KEY, self.project_key.clone()),
        ];
        let optional = [
            (HOME_PROJECT_KEY, self.home_project_id.clone()),
            (HOME_MACHINE_KEY, self.home_machine_id.clone()),
            (SNAPSHOT_KEY, self.snapshot_id.clone()),
            (SIZE_KEY, self.size.clone()),
            (IDEMPOTENCY_KEY, self.idempotency_key.clone()),
            (
                CREATED_BY_KEY,
                self.created_by.as_ref().map(|by| by.as_str().to_string()),
            ),
            (CREATED_AT_KEY, self.created_at.clone()),
        ];
        pairs.extend(
            optional.into_iter().filter_map(|(key, value)| {
                value.filter(|value| !value.is_empty()).map(|v| (key, v))
            }),
        );
        pairs
    }

    /// Reads a provider's metadata map; `None` when the box is not Ghostex's.
    pub fn from_map(map: &BTreeMap<String, String>) -> Option<Self> {
        if map.get(MARKER_KEY).map(String::as_str) != Some(MARKER_VALUE) {
            return None;
        }
        let text = |key: &str| map.get(key).filter(|value| !value.is_empty()).cloned();
        Some(Self {
            box_id: text(BOX_ID_KEY)?,
            project_key: text(PROJECT_KEY_KEY).unwrap_or_default(),
            home_project_id: text(HOME_PROJECT_KEY),
            home_machine_id: text(HOME_MACHINE_KEY),
            snapshot_id: text(SNAPSHOT_KEY),
            size: text(SIZE_KEY),
            idempotency_key: text(IDEMPOTENCY_KEY),
            created_by: text(CREATED_BY_KEY).map(|by| CloudBoxCreatedBy::from_wire(&by)),
            created_at: text(CREATED_AT_KEY),
        })
    }
}
