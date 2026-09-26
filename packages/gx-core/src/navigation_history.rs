//! Titlebar Back/Forward: the trail stop the sidebar is on, and the client half of the trail
//! gxserver keeps (`server/src/navigation_history`).
//!
//! CDXC:Navigation 2026-09-25 WHY:
//! The runtime's `NavigationHistoryController` (packages/shared/navigation-history/, deleted with
//! this change) ran this in QuickJS and the titlebar only painted what it posted. The app runtime
//! port (family F6) moves it here: [`NavigationTrail`] is the controller's bookkeeping (the last recorded stop, the
//! pending visit, the settle window after a Back or Forward), [`sidebar_navigation_entry`] is
//! `createNavigationHistoryEntry` read off the store's own sidebar model, and the host performs
//! the three calls and the debounce (apps/desktop/src/navigation_history/controller.rs). The
//! wire shapes, the key and the normalization are that contract file's, unchanged, so a trail the
//! runtime wrote before this change walks the same after it.
//!
//! SEE-ALSO: server/src/navigation_history (the trail and its key),
//! apps/desktop/src/navigation_history/controller.rs (the host).

use serde_json::{Map, Value};

use crate::keys::{parse_workspace_subgroup_id, ProjectKey};
use crate::sidebar_view::SidebarViewModel;
use crate::FocusState;

pub const NAVIGATION_HISTORY_READ_ENDPOINT: &str = "/api/readNavigationHistory";
pub const NAVIGATION_HISTORY_VISIT_ENDPOINT: &str = "/api/recordNavigationVisit";
pub const NAVIGATION_HISTORY_NAVIGATE_ENDPOINT: &str = "/api/navigateHistory";
/// The desktop's trail, so a browser on the same daemon keeps a cursor of its own.
pub const NAVIGATION_HISTORY_SCOPE_GPUI: &str = "gpui-desktop";
/// How long a changed stop waits before it is sent, so a burst of focus changes is one visit.
pub const NAVIGATION_VISIT_DEBOUNCE_MS: u64 = 150;
/// How long a Back or Forward target stays authoritative while it activates.
pub const NAVIGATION_SETTLE_TIMEOUT_MS: u64 = 4_000;
/// Bounded, so a trail of stops that no longer exist cannot spin.
pub const MAX_NAVIGATION_ATTEMPTS: usize = 12;

/// Byte-identical to the daemon's `KEY_SEPARATOR`.
const KEY_SEPARATOR: char = '\u{1f}';

/// One trail stop, in the sidebar's vocabulary: the ids a focus command takes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigationHistoryEntry {
    pub project_id: String,
    pub session_id: Option<String>,
    pub group_id: Option<String>,
    pub project_label: Option<String>,
    pub session_label: Option<String>,
}

impl NavigationHistoryEntry {
    /// `navigationHistoryEntryKey`: the daemon's identity of a stop, which `forgetKeys` names.
    pub fn key(&self) -> String {
        format!(
            "{}{KEY_SEPARATOR}{}",
            self.project_id,
            self.session_id.as_deref().unwrap_or("")
        )
    }

    /// The wire object, with absent fields left out as the TypeScript spread left them out.
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("projectId".into(), Value::String(self.project_id.clone()));
        for (name, value) in [
            ("sessionId", &self.session_id),
            ("groupId", &self.group_id),
            ("projectLabel", &self.project_label),
            ("sessionLabel", &self.session_label),
        ] {
            if let Some(value) = value {
                object.insert(name.into(), Value::String(value.clone()));
            }
        }
        Value::Object(object)
    }

    /// `normalizeNavigationHistoryEntry`: an entry the daemon sent, or `None` without a project.
    pub fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let text = |name: &str, max: usize| normalize_text(object.get(name), max);
        Some(Self {
            project_id: text("projectId", 512)?,
            session_id: text("sessionId", 512),
            group_id: text("groupId", 512),
            project_label: text("projectLabel", 256),
            session_label: text("sessionLabel", 256),
        })
    }
}

/// `normalizeText`: trimmed, cut to `max` UTF-16 units as `slice` cuts, and absent when empty.
fn normalize_text(value: Option<&Value>, max: usize) -> Option<String> {
    let trimmed = value?.as_str()?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut units = 0;
    let mut end = trimmed.len();
    for (index, character) in trimmed.char_indices() {
        units += character.len_utf16();
        if units > max {
            end = index;
            break;
        }
    }
    Some(trimmed[..end].to_string())
}

/// What the titlebar draws: availability only, as the runtime posted it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NavigationHistoryButtons {
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

/// `normalizeNavigationHistoryState` reduced to the two flags the titlebar reads, from any of the
/// three answers (their `navigationHistory` block, or the answer itself).
pub fn navigation_history_buttons(response: &Value) -> NavigationHistoryButtons {
    let state = match response.get("navigationHistory") {
        Some(state) if !state.is_null() => state,
        _ => response,
    };
    NavigationHistoryButtons {
        can_go_back: state.get("canGoBack").and_then(Value::as_bool) == Some(true),
        can_go_forward: state.get("canGoForward").and_then(Value::as_bool) == Some(true),
    }
}

/// The stop a `navigateHistory` answer moved the cursor to.
pub fn navigation_history_target(response: &Value) -> Option<NavigationHistoryEntry> {
    NavigationHistoryEntry::from_json(response.get("target")?)
}

/// `createNavigationHistoryEntry`: the active group's project, and its focused session when one
/// is. The Chats collection and a projectless selection are no stop.
///
/// The active group is the one the model marks; when none is marked and a project is selected,
/// that project's group, as `withSelectedProjectActiveGroup` re-asserted the selection.
pub fn sidebar_navigation_entry(
    model: &SidebarViewModel,
    focus: &FocusState,
) -> Option<NavigationHistoryEntry> {
    let groups = model.built_groups();
    let selected_group_id = focus
        .active_project
        .as_ref()
        .map(ProjectKey::to_sidebar_group_id);
    let (group, _) = groups
        .iter()
        .find(|(group, _)| group.is_active)
        .or_else(|| {
            let selected = selected_group_id.as_deref()?;
            groups
                .iter()
                .find(|(group, _)| group.project_context.is_some() && group.group_id == selected)
        })?;
    let project = ProjectKey::parse_sidebar_group_id(&group.group_id)
        .filter(|_| group.project_context.is_some())
        .or_else(|| parse_workspace_subgroup_id(&group.group_id).map(|(project, _)| project))?;
    let focused = group.sessions.iter().find(|session| session.is_focused);
    let session_label = focused.and_then(|session| {
        let row = &session.row;
        [
            row.menu_facts.raw_display_title.as_deref(),
            row.menu_facts.primary_title.as_deref(),
            Some(row.alias.as_str()),
        ]
        .into_iter()
        .flatten()
        .next()
        .filter(|label| !label.is_empty())
        .map(str::to_string)
    });
    Some(NavigationHistoryEntry {
        project_id: project.to_workspace_project_id(),
        session_id: focused.map(|session| session.row.sidebar_session_id.clone()),
        group_id: Some(group.group_id.clone()),
        project_label: (!group.title.is_empty()).then(|| group.title.clone()),
        session_label,
    })
}

/// The settle window a landed Back or Forward opens.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Settle {
    key: String,
    project_id: String,
    deadline_ms: u64,
}

/// The controller's bookkeeping, without the timer or the calls.
#[derive(Clone, Debug, Default)]
pub struct NavigationTrail {
    last_visit: Option<NavigationHistoryEntry>,
    pending: Option<NavigationHistoryEntry>,
    pending_replaces_current: bool,
    settle: Option<Settle>,
}

impl NavigationTrail {
    /// `recordVisit`, called whenever the sidebar may have moved. Returns whether a visit is now
    /// pending and the debounce should be armed.
    pub fn record_visit(&mut self, entry: NavigationHistoryEntry, now_ms: u64) -> bool {
        let key = entry.key();
        if let Some(settle) = &self.settle {
            if key == settle.key {
                // The target became active: the daemon's cursor is already on it.
                self.settle = None;
                self.last_visit = Some(entry);
                return false;
            }
            if entry.project_id == settle.project_id {
                // Landing refined the target (Back went to a project, then a session in it took
                // focus): update the stop in place rather than truncating the forward branch.
                self.settle = None;
                self.last_visit = Some(entry.clone());
                self.pending = Some(entry);
                self.pending_replaces_current = true;
                return true;
            }
            if now_ms < settle.deadline_ms {
                return false;
            }
            self.settle = None;
        }
        if self.last_visit.as_ref() == Some(&entry) {
            return false;
        }
        self.last_visit = Some(entry.clone());
        self.pending = Some(entry);
        true
    }

    /// The visit to send now, and whether it replaces the current stop.
    pub fn take_pending(&mut self) -> Option<(NavigationHistoryEntry, bool)> {
        let entry = self.pending.take()?;
        let replaces = std::mem::take(&mut self.pending_replaces_current);
        Some((entry, replaces))
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// A visit failed: forget the memo so the next update sends the stop again.
    pub fn visit_failed(&mut self) {
        self.last_visit = None;
    }

    /// A Back or Forward target was activated: it is the stop now, and nothing older is sent.
    pub fn landed(&mut self, target: NavigationHistoryEntry, now_ms: u64) {
        self.settle = Some(Settle {
            key: target.key(),
            project_id: target.project_id.clone(),
            deadline_ms: now_ms + NAVIGATION_SETTLE_TIMEOUT_MS,
        });
        self.last_visit = Some(target);
        self.pending = None;
        self.pending_replaces_current = false;
    }
}
