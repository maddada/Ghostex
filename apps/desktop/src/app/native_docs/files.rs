//! Loading the Docs tree through the file bridge and turning it into the rows the files list draws.

use std::collections::{HashMap, HashSet};

use gpui::{Context, Window};
use serde_json::{Value, json};

use super::state::{DocsEntry, DocsEntryKind, DocsLoadState, DocsProjectKey};
use crate::GhostexGpuiApp;

/// One drawn row of the Project Docs tree.
#[derive(Clone, Debug)]
pub(crate) struct DocsTreeRow {
    pub(crate) path: String,
    pub(crate) display_path: String,
    pub(crate) name: String,
    pub(crate) kind: DocsEntryKind,
    /// Indent level in the tree; 0 for search results, which show their folder instead.
    pub(crate) depth: usize,
    pub(crate) expanded: bool,
}

fn entry_from_json(value: &Value) -> Option<DocsEntry> {
    let path = value["path"].as_str()?.to_string();
    let name = value["name"].as_str().unwrap_or_default().to_string();
    let kind = match value["kind"].as_str()? {
        "directory" => DocsEntryKind::Directory,
        _ => DocsEntryKind::File,
    };
    Some(DocsEntry {
        display_path: value["displayPath"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| path.clone()),
        path,
        name,
        kind,
        depth: value["depth"].as_u64().unwrap_or(0) as usize,
        size: value["size"].as_u64(),
        modified_at: value["modifiedAt"].as_str().map(str::to_string),
    })
}

/// `orderManageEntriesForTree`: each directory followed by its children, siblings kept in the
/// bridge's order, and anything whose parent is missing appended at the end.
fn order_entries_for_tree(entries: Vec<DocsEntry>) -> Vec<DocsEntry> {
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let parent = if entry.depth == 0 {
            String::new()
        } else {
            parent_path(&entry.path).to_string()
        };
        children.entry(parent).or_default().push(index);
    }
    let mut ordered = Vec::with_capacity(entries.len());
    let mut visited = vec![false; entries.len()];
    let mut stack: Vec<usize> = children
        .get("")
        .map(|c| c.iter().rev().copied().collect())
        .unwrap_or_default();
    while let Some(index) = stack.pop() {
        if std::mem::replace(&mut visited[index], true) {
            continue;
        }
        ordered.push(index);
        if entries[index].kind == DocsEntryKind::Directory
            && let Some(kids) = children.get(&entries[index].path)
        {
            stack.extend(kids.iter().rev().copied());
        }
    }
    ordered.extend((0..entries.len()).filter(|index| !visited[*index]));
    let mut slots: Vec<Option<DocsEntry>> = entries.into_iter().map(Some).collect();
    ordered
        .into_iter()
        .filter_map(|index| slots[index].take())
        .collect()
}

/// The parent directory of a bridge path, or "" for a top-level entry.
pub(crate) fn parent_path(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

impl GhostexGpuiApp {
    /// A request carrying the project identity the bridge checks.
    pub(crate) fn native_docs_request(&self, action: &str, fields: Value) -> Value {
        let mut request = json!({
            "action": action,
            "requestId": uuid::Uuid::new_v4().to_string(),
            "projectId": self
                .native_docs
                .project
                .as_ref()
                .map(|project| project.project_id.clone())
                .unwrap_or_default(),
        });
        if let (Some(target), Some(fields)) = (request.as_object_mut(), fields.as_object()) {
            for (key, value) in fields {
                target.insert(key.clone(), value.clone());
            }
        }
        request
    }

    /// Called from the app's render: switches the view to the current project and starts the
    /// first listing. True when the project changed.
    pub(crate) fn native_docs_sync_project(
        &mut self,
        project: &DocsProjectKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.native_docs.focus.is_none() {
            self.native_docs.focus = Some(cx.focus_handle());
            super::storage::import_browser_state_once();
            self.native_docs.sidebar_pinned = super::storage::read_sidebar_pinned();
            self.native_docs.format_bar_collapsed = super::storage::read_format_bar_collapsed();
            self.native_docs.line_numbers = true;
            self.native_docs.git_changes = true;
            self.native_docs.constrain_width = true;
        }
        self.native_docs_ensure_search(window, cx);
        if self.native_docs.project.as_ref() == Some(project) {
            return false;
        }
        self.native_docs.reset_for_project(project.clone());
        let generation = self.native_docs.generation;
        cx.defer_in(window, move |this, _window, cx| {
            if this.native_docs.generation == generation {
                this.native_docs_restore_open_files(cx);
                this.native_docs_load_notes(cx);
                if let Some(path) = this.native_docs.pending_open.take() {
                    this.native_docs_open_external(path, cx);
                }
                this.native_docs_refresh(cx);
            }
        });
        true
    }

    /// Re-lists the tree. The rows already drawn stay until the answer replaces them.
    pub(crate) fn native_docs_refresh(&mut self, cx: &mut Context<Self>) {
        let generation = self.native_docs.generation;
        if self.native_docs.entries.is_empty() {
            self.native_docs.load_state = Some(DocsLoadState::Loading);
        }
        let request = self.native_docs_request("list", json!({}));
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            if let Some(error) = response["error"].as_str() {
                this.native_docs.load_state = Some(DocsLoadState::Error);
                this.native_docs.error = Some(error.to_string());
            } else {
                let entries: Vec<DocsEntry> = response["entries"]
                    .as_array()
                    .map(|entries| entries.iter().filter_map(entry_from_json).collect())
                    .unwrap_or_default();
                this.native_docs.entries = order_entries_for_tree(entries);
                if this.native_docs.expand_all {
                    let all: Vec<String> = this
                        .native_docs_expandable_folders()
                        .into_iter()
                        .map(str::to_string)
                        .collect();
                    this.native_docs.expanded.extend(all);
                }
                this.native_docs.load_state = Some(DocsLoadState::Ready);
                this.native_docs.error = None;
            }
            this.native_docs_notify(cx);
        });
    }

    /// The rows the Project Docs tree draws: the tree with collapsed folders' children hidden, or,
    /// while searching, every entry whose path contains the query plus the folders above it, all
    /// open (`filterManageEntriesForSearch`).
    pub(crate) fn native_docs_tree_rows(&self) -> Vec<DocsTreeRow> {
        let state = &self.native_docs;
        let query = state.search_query.trim().to_lowercase();
        let searching = !query.is_empty();
        let visible: Option<HashSet<&str>> = searching.then(|| {
            let paths: HashSet<&str> = state
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect();
            let mut visible = HashSet::new();
            for entry in &state.entries {
                if !entry.path.to_lowercase().contains(&query) {
                    continue;
                }
                visible.insert(entry.path.as_str());
                let mut parent = parent_path(&entry.path);
                while !parent.is_empty() {
                    if let Some(path) = paths.get(parent) {
                        visible.insert(path);
                    }
                    parent = parent_path(parent);
                }
            }
            visible
        });
        let mut rows = Vec::new();
        // The depth below which rows are hidden because an ancestor is collapsed.
        let mut hidden_below: Option<usize> = None;
        for entry in &state.entries {
            if let Some(visible) = visible.as_ref()
                && !visible.contains(entry.path.as_str())
            {
                continue;
            }
            if let Some(depth) = hidden_below {
                if entry.depth > depth {
                    continue;
                }
                hidden_below = None;
            }
            let expanded = entry.kind == DocsEntryKind::Directory
                && (searching || state.expanded.contains(&entry.path));
            if entry.kind == DocsEntryKind::Directory && !expanded {
                hidden_below = Some(entry.depth);
            }
            rows.push(DocsTreeRow {
                path: entry.path.clone(),
                display_path: entry.display_path.clone(),
                name: entry.name.clone(),
                kind: entry.kind,
                depth: entry.depth,
                expanded,
            });
        }
        rows
    }

    /// Collapse All when any expandable folder is open, else Expand All. Nothing remembers the
    /// expansion from before.
    pub(crate) fn native_docs_toggle_all_folders(
        &mut self,
        collapse: bool,
        cx: &mut Context<Self>,
    ) {
        if collapse {
            self.native_docs.expanded.clear();
            self.native_docs.expand_all = false;
        } else {
            let all: Vec<String> = self
                .native_docs_expandable_folders()
                .into_iter()
                .map(str::to_string)
                .collect();
            self.native_docs.expanded.extend(all);
            self.native_docs.expand_all = true;
        }
        self.native_docs_notify(cx);
    }

    /// Clears the query, opens the folders above the open file and scrolls its row into view.
    pub(crate) fn native_docs_reveal_open_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self.native_docs.active.clone() else {
            return;
        };
        self.native_docs_clear_search(window, cx);
        self.native_docs_reveal_in_tree(&active);
        self.native_docs.reveal_request = Some(active);
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.native_docs.search_query.clear();
        if let Some(search) = self.native_docs.search.clone() {
            search.update(cx, |search, cx| search.set_value("", window, cx));
        }
        self.native_docs_notify(cx);
    }

    /// Selects an open file (Open Files row).
    pub(crate) fn native_docs_select(&mut self, path: &str, cx: &mut Context<Self>) {
        self.native_docs.active = Some(path.to_string());
        self.native_docs_close_drawer(cx);
        self.native_docs_persist_open_files(cx);
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_toggle_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.native_docs.expanded.remove(path) {
            self.native_docs.expanded.insert(path.to_string());
        }
        self.native_docs_notify(cx);
    }

    /// Opens every folder above `path` so its row is drawn.
    pub(crate) fn native_docs_reveal_in_tree(&mut self, path: &str) {
        let mut parent = parent_path(path);
        while !parent.is_empty() {
            self.native_docs.expanded.insert(parent.to_string());
            parent = parent_path(parent);
        }
    }

    pub(crate) fn native_docs_notify(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
}
