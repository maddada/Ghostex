//! The six computed status entries of the Session Tags list (Needs you, Question, Error, Finished
//! (not yet read), Working, Idle), and how a stored list keeps them through a write by an app that
//! does not know them.
//!
//! An entry is stored as `{ "id": "status-working", "type": "status", "enabled": true, "visible":
//! true }`, with no `tag`: it is worked out from each session's state, never stored on a session,
//! never a tag filter and never offered in Tag As. The order of `StatusGroup::ALL` is the default
//! order of the entries and the urgency order a coordinator's tree is placed by.
//!
//! SEE-ALSO: packages/settings-catalog/src/data/session_tags.rs (the stored default list),
//! packages/shared/session-tags.ts, apps/mobile/app/src/contract/sessionTagList.ts (the phone's
//! port), unified_sidebar/status_groups.rs (which group a session stands in),
//! server/src/settings_sync/sidebar_tags.rs (where sync keeps them).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One computed status entry, and the group it heads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatusGroup {
    NeedsYou,
    Question,
    Error,
    Finished,
    Working,
    Idle,
}

impl StatusGroup {
    /// Default order, most urgent first.
    pub const ALL: [Self; 6] = [
        Self::NeedsYou,
        Self::Question,
        Self::Error,
        Self::Finished,
        Self::Working,
        Self::Idle,
    ];

    /// The stored id.
    pub fn id(self) -> &'static str {
        match self {
            Self::NeedsYou => "status-needs-you",
            Self::Question => "status-question",
            Self::Error => "status-error",
            Self::Finished => "status-finished",
            Self::Working => "status-working",
            Self::Idle => "status-idle",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.id() == id)
    }

    /// The group's name, on its header and its Session Tags row.
    pub fn label(self) -> &'static str {
        match self {
            Self::NeedsYou => "Needs you",
            Self::Question => "Question",
            Self::Error => "Error",
            Self::Finished => "Finished (not yet read)",
            Self::Working => "Working",
            Self::Idle => "Idle",
        }
    }

    /// Its place in the default order: 0 is the most urgent.
    pub fn rank(self) -> usize {
        self as usize
    }

    /// The symbol's asset under the shared icon set, drawn on the group's header and its Session
    /// Tags row.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::NeedsYou => "titlebar/alert-triangle.svg",
            Self::Question => "titlebar/help-circle.svg",
            Self::Error => "titlebar/alert-circle.svg",
            Self::Finished => "titlebar/circle-check.svg",
            Self::Working => "titlebar/circle-dashed.svg",
            // The approved idle ring (radius 9, stroke 2), not the smaller shared `circle.svg`.
            Self::Idle => "titlebar/state-idle.svg",
        }
    }

    /// The state colour as `(dark appearance, light appearance)` RGB.
    ///
    /// CDXC:Sidebar 2026-11-14 SEE-ALSO: the approved state table (#EF4444, #F472B6, #EF4444, #95D7F6, #C68A06 and the neutral #B4B8BF in the dark appearance). The approved design has no light table; the light colours are the phone's (apps/mobile/app/src/components/sessions/unified/groupHeaders.tsx), darker shades of the same hues: on a light sidebar from #E8E8E8 to white the symbol keeps 4.1:1 to 6.7:1 against its 8% band and #262626 text at least 10.9:1. Needs you and Error share one red and differ by silhouette and word.
    pub fn colors(self) -> (u32, u32) {
        match self {
            Self::NeedsYou | Self::Error => (0xef4444, 0xc81e1e),
            Self::Question => (0xf472b6, 0xbe185d),
            Self::Finished => (0x95d7f6, 0x0369a1),
            Self::Working => (0xc68a06, 0x8a5a00),
            Self::Idle => (0xb4b8bf, 0x4b5563),
        }
    }

    /// The stored form of the entry as a first run writes it: enabled and visible.
    fn default_entry(self) -> Value {
        json!({ "enabled": true, "id": self.id(), "type": "status", "visible": true })
    }
}

/// The status entry a stored list item is, if it is one.
pub(crate) fn status_of_item(item: &Value) -> Option<StatusGroup> {
    item.get("id")
        .and_then(Value::as_str)
        .and_then(StatusGroup::from_id)
}

/// The list a winning write `incoming` should be, given the list this computer held before it
/// (`previous`), or `None` when `incoming` already holds every status entry or `previous` held
/// none.
///
/// Every status entry `incoming` lacks is put back where `previous` had it, with its flags, in
/// ascending order of its old place (clamped to the end); the ones `previous` never held go on top
/// in their default order, as a list is read. The entries `incoming` carries keep their order and
/// flags.
///
/// CDXC:Sidebar 2026-11-14 WHY:
/// A Ghostex that predates the status entries drops them when it normalizes the list and writes back what is left (its desktop's Session Tags editor, the phone's tag switches from 11.5.0 on), so after one tag switch on an older app the positions the user gave Working or Error would be gone everywhere. A computer that knows them puts them back from the list it held, and only when it held them: a list that never had them gets them at their default places on read, which is the same order. No second order key: what the user arranged lives in the one list.
pub fn keep_status_entries(previous: &Value, incoming: &Value) -> Option<Value> {
    let previous = previous.as_array()?;
    let incoming = incoming.as_array()?;
    let mut held: Vec<(usize, StatusGroup, &Value)> = Vec::new();
    for (index, item) in previous.iter().enumerate() {
        if let Some(group) = status_of_item(item) {
            if !held.iter().any(|(_, known, _)| *known == group) {
                held.push((index, group, item));
            }
        }
    }
    if held.is_empty() {
        return None;
    }
    let present: Vec<StatusGroup> = incoming.iter().filter_map(status_of_item).collect();
    let mut restored = incoming.clone();
    let mut changed = false;
    for (index, group, item) in &held {
        if present.contains(group) {
            continue;
        }
        restored.insert((*index).min(restored.len()), (*item).clone());
        changed = true;
    }
    let missing: Vec<Value> = StatusGroup::ALL
        .into_iter()
        .filter(|group| {
            !present.contains(group) && !held.iter().any(|(_, known, _)| known == group)
        })
        .map(StatusGroup::default_entry)
        .collect();
    if !missing.is_empty() {
        restored.splice(0..0, missing);
        changed = true;
    }
    changed.then_some(Value::Array(restored))
}

/// A stored list with every status entry it lacks put on top in their default order, as the list
/// is read (tags.rs `normalize_tag_list_items`), or `None` when it has them all or is no list. The
/// entries it holds keep their places and flags.
///
/// Turning grouping on writes this, so the places the status entries are read at are the stored
/// ones before anything depends on them (`keep_status_entries`).
pub fn with_status_entries(list: &Value) -> Option<Value> {
    let items = list.as_array()?;
    let present: Vec<StatusGroup> = items.iter().filter_map(status_of_item).collect();
    let missing: Vec<Value> = StatusGroup::ALL
        .into_iter()
        .filter(|group| !present.contains(group))
        .map(StatusGroup::default_entry)
        .collect();
    if missing.is_empty() {
        return None;
    }
    let mut out = missing;
    out.extend(items.iter().cloned());
    Some(Value::Array(out))
}

/// The list with its status entries left out, which is what an app without them reads of it.
pub fn without_status_entries(list: &Value) -> Value {
    match list.as_array() {
        Some(items) => Value::Array(
            items
                .iter()
                .filter(|item| status_of_item(item).is_none())
                .cloned()
                .collect(),
        ),
        None => list.clone(),
    }
}
