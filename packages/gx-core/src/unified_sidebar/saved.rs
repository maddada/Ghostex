//! What the Unified sidebar keeps across a restart.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::build::UnifiedFilter;

/// The client-storage key, in the same table as the Project sidebar's folds and hide list. The web
/// build keeps it in the page's own storage under the same name.
pub const UNIFIED_SAVED_STATE_KEY: &str = "ghostex.sidebar.unified.v1";

/// How many projects the project list keeps under Recent.
pub const RECENT_REPOS_MAX: usize = 5;

/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q855, "remember both"): the header's filter, which shelves are open and which project groups are folded all survive a restart, because a filter you set for your day should still be there after an update. It lives with the Project sidebar's folds, on this client, not as a setting: two computers looking at the same machines can each keep their own view. The session text search is deliberately not kept: an old search silently hiding sessions after a restart reads as lost work. Shelves are kept by name, and inside a project as `<repo key>|<shelf>`, so a list with grouping off and on each have their own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UnifiedSavedState {
    pub repos: BTreeSet<String>,
    pub machines: BTreeSet<String>,
    pub spaces: BTreeSet<String>,
    /// Shelves toggled away from how they start: Drafts starts open and the rest folded.
    pub toggled_shelves: BTreeSet<String>,
    /// Folded project groups, by repository.
    pub folded_groups: BTreeSet<String>,
    /// The chevron of a card's hover strip is open, on every card.
    pub strip_expanded: bool,
    /// The machine a new session last used per repository (the Run on row and the tray's machine
    /// chip start there).
    pub last_machines: BTreeMap<String, String>,
    /// The repositories new sessions last started in, newest first: the project list's Recent.
    pub recent_repos: Vec<String>,
}

impl UnifiedSavedState {
    /// `None` reads as nothing saved; an unreadable or old value too, since none of this is worth
    /// refusing to start over.
    pub fn parse(raw: Option<&str>) -> Self {
        raw.and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// The filter picks as the builder reads them. The text search starts empty.
    pub fn filter(&self) -> UnifiedFilter {
        UnifiedFilter {
            repos: self.repos.clone(),
            machines: self.machines.clone(),
            spaces: self.spaces.clone(),
            query: String::new(),
        }
    }

    /// A session started in `repo_key` on `machine_id`: that machine is where the repository
    /// starts next time, and the repository moves to the top of Recent.
    pub fn remember_new_session(&mut self, repo_key: &str, machine_id: &str) {
        self.last_machines
            .insert(repo_key.to_string(), machine_id.to_string());
        self.recent_repos.retain(|repo| repo != repo_key);
        self.recent_repos.insert(0, repo_key.to_string());
        self.recent_repos.truncate(RECENT_REPOS_MAX);
    }

    pub fn set_filter(&mut self, filter: &UnifiedFilter) {
        self.repos.clone_from(&filter.repos);
        self.machines.clone_from(&filter.machines);
        self.spaces.clone_from(&filter.spaces);
    }
}
