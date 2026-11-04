//! Loading the Docs tree through the file bridge and turning it into the rows the files list draws.

use std::collections::{HashMap, HashSet};

use gpui::{Context, Window};
use serde_json::{Value, json};

use super::state::{DocsEntry, DocsEntryKind, DocsLoadState, DocsProjectKey};
use crate::GhostexGpuiApp;
use crate::app::model::TitlebarMode;

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
    /// A search result's folder, drawn after its name.
    pub(crate) folder: Option<String>,
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

/// The folder an entry was listed under ("" for the top level), as `order_entries_for_tree`
/// groups it.
fn tree_parent(entry: &DocsEntry) -> &str {
    if entry.depth == 0 {
        ""
    } else {
        parent_path(&entry.path)
    }
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
            self.native_docs.word_wrap = super::storage::read_word_wrap();
            self.native_docs.line_numbers = true;
            self.native_docs.git_changes = true;
            // Docs starts with the full content width, as the former React Docs page did.
            self.native_docs.constrain_width = false;
        }
        self.native_docs_ensure_search(window, cx);
        if self.native_docs.project.as_ref() == Some(project) {
            return false;
        }
        let parked = self
            .native_docs
            .parked
            .remove(&project.project_id)
            .filter(|parked| parked.project.as_ref() == Some(project));
        let restored = parked.is_some();
        let mut outgoing = match parked {
            Some(parked) => self.native_docs.restore_parked(parked),
            None => self.native_docs.reset_for_project(project.clone()),
        };
        if let Some(left) = outgoing
            .project
            .clone()
            .filter(|left| self.native_docs_parks_project(&left.project_id))
        {
            outgoing.strip_for_parking();
            self.native_docs.parked.insert(left.project_id, outgoing);
        }
        let released = self
            .native_docs
            .parked
            .keys()
            .filter(|project_id| !self.native_docs_parks_project(project_id))
            .cloned()
            .collect::<Vec<_>>();
        for project_id in released {
            self.native_docs.parked.remove(&project_id);
        }
        let generation = self.native_docs.generation;
        cx.defer_in(window, move |this, _window, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            if restored {
                this.native_docs_resume_parked(cx);
            } else {
                this.native_docs_restore_open_files(cx);
                this.native_docs_load_notes(cx);
            }
            if let Some(path) = this.native_docs.pending_open.take() {
                this.native_docs_open_external(path, cx);
            }
            if let Some(page) = this.native_docs.pending_page.take() {
                this.native_docs_open_page(page, cx);
            }
            this.native_docs_refresh(cx);
        });
        true
    }

    /// Whether `project_id`'s Docs state is kept while another project is on screen: Docs is the
    /// side panel view it was left on, awake (`GpuiProjectViewState::active_view_awake`), or, with
    /// Keep views loaded on, one of its open views while Docs is awake.
    fn native_docs_parks_project(&self, project_id: &str) -> bool {
        let docs_awake = self
            .project_editor_shell
            .is_mode_awake(TitlebarMode::Manage);
        let keep_views_loaded = self.project_editor_auto_sleep_policy.keep_views_loaded;
        if self.agents_workspace_project_id.as_deref() == Some(project_id) {
            return docs_awake
                && (self.active_mode == TitlebarMode::Manage
                    || keep_views_loaded && self.open_views.contains(&TitlebarMode::Manage));
        }
        self.project_view_states_by_project
            .get(project_id)
            .is_some_and(|state| {
                state.active_view_awake && state.active_mode == TitlebarMode::Manage
                    || keep_views_loaded
                        && docs_awake
                        && state.open_views.contains(&TitlebarMode::Manage)
            })
    }

    /// Answers that arrived while the state was parked were dropped by the generation check, so
    /// what they would have finished is asked again. The files list is listed again by the caller;
    /// the rows already drawn stay until the answer replaces them.
    fn native_docs_resume_parked(&mut self, cx: &mut Context<Self>) {
        let mut reread = Vec::new();
        for document in &mut self.native_docs.documents {
            document.saving = false;
            if document.load == super::state::DocsDocumentLoad::Loading {
                reread.push(document.path.clone());
            }
        }
        for path in reread {
            self.native_docs_read(&path, cx);
        }
        if !self.native_docs.notes_loaded {
            self.native_docs_load_notes(cx);
        }
    }

    /// Re-lists the project's top level and every open folder. The rows already drawn stay until
    /// the answers replace them; a folder whose listing did not change is left as it is.
    pub(crate) fn native_docs_refresh(&mut self, cx: &mut Context<Self>) {
        if self.native_docs.entries.is_empty() {
            self.native_docs.load_state = Some(DocsLoadState::Loading);
        }
        let open_folders: Vec<String> = self
            .native_docs
            .expanded
            .iter()
            .filter(|folder| self.native_docs.loaded_folders.contains(*folder))
            .cloned()
            .collect();
        self.native_docs_list_folder("", cx);
        for folder in open_folders {
            self.native_docs_list_folder(&folder, cx);
        }
    }

    /// Lists one folder's children ("" for the top level) and puts them in the tree.
    ///
    /// CDXC:Docs 2026-09-27 DECISION:
    /// User: the Files view lists the whole project, so it loads one folder at a time when it is opened ("to keep the file list more performant") instead of walking the project up front.
    pub(crate) fn native_docs_list_folder(&mut self, folder: &str, cx: &mut Context<Self>) {
        let generation = self.native_docs.generation;
        let revision = self
            .native_docs
            .folder_revisions
            .get(folder)
            .cloned()
            .unwrap_or_default();
        let request = self.native_docs_request(
            "list",
            json!({ "path": folder, "directoryOnly": true, "revision": revision }),
        );
        let folder = folder.to_string();
        // The row shows "…" while its listing is on the way (`Folder not loaded yet`).
        if !folder.is_empty() {
            self.native_docs.folders_loading.insert(folder.clone());
        }
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            this.native_docs.folders_loading.remove(&folder);
            if let Some(error) = response["error"].as_str() {
                if folder.is_empty() {
                    this.native_docs.load_state = Some(DocsLoadState::Error);
                    this.native_docs.error = Some(error.to_string());
                } else {
                    // The row shows "!" with the error (`Folder could not load`). Listing its
                    // parent again drops the row when the folder went away (renamed, deleted).
                    this.native_docs
                        .folder_errors
                        .insert(folder.clone(), error.to_string());
                    this.native_docs.loaded_folders.remove(&folder);
                    let parent = parent_path(&folder).to_string();
                    this.native_docs_list_folder(&parent, cx);
                }
                this.native_docs_notify(cx);
                return;
            }
            this.native_docs.folder_errors.remove(&folder);
            if let Some(revision) = response["revision"].as_str() {
                this.native_docs
                    .folder_revisions
                    .insert(folder.clone(), revision.to_string());
            }
            this.native_docs.loaded_folders.insert(folder.clone());
            if folder.is_empty() {
                this.native_docs.load_state = Some(DocsLoadState::Ready);
                this.native_docs.error = None;
            }
            if response["unchanged"].as_bool() == Some(true) {
                this.native_docs_notify(cx);
                return;
            }
            let children: Vec<DocsEntry> = response["entries"]
                .as_array()
                .map(|entries| entries.iter().filter_map(entry_from_json).collect())
                .unwrap_or_default();
            this.native_docs_apply_folder_listing(&folder, children);
            this.native_docs_notify(cx);
        });
    }

    /// Replaces `folder`'s direct children with `children`. A child folder that is gone takes
    /// everything listed under it along; one that is still there keeps its listed children.
    fn native_docs_apply_folder_listing(&mut self, folder: &str, children: Vec<DocsEntry>) {
        let kept: HashSet<&str> = children.iter().map(|child| child.path.as_str()).collect();
        let gone: Vec<String> = self
            .native_docs
            .entries
            .iter()
            .filter(|entry| {
                entry.kind == DocsEntryKind::Directory
                    && tree_parent(entry) == folder
                    && !kept.contains(entry.path.as_str())
            })
            .map(|entry| entry.path.clone())
            .collect();
        for path in &gone {
            self.native_docs_forget_folder(path);
        }
        let mut entries: Vec<DocsEntry> = std::mem::take(&mut self.native_docs.entries)
            .into_iter()
            .filter(|entry| tree_parent(entry) != folder)
            .collect();
        entries.extend(children);
        self.native_docs.entries = order_entries_for_tree(entries);
    }

    /// Drops a folder that no longer exists: its listed contents, and its open and loaded marks.
    fn native_docs_forget_folder(&mut self, folder: &str) {
        let prefix = format!("{folder}/");
        let state = &mut self.native_docs;
        state
            .entries
            .retain(|entry| entry.path != folder && !entry.path.starts_with(&prefix));
        let under = |path: &String| path == folder || path.starts_with(&prefix);
        state.expanded.retain(|path| !under(path));
        state.loaded_folders.retain(|path| !under(path));
        state.folder_revisions.retain(|path, _| !under(path));
        state.folder_errors.retain(|path, _| !under(path));
        state.folders_loading.retain(|path| !under(path));
    }

    /// Runs the project-wide search for the search box's text a moment after typing stops.
    /// With types picked in the type filter (`type_filter.rs`) it runs with no text too, listing
    /// every file of those types.
    pub(crate) fn native_docs_schedule_search(&mut self, cx: &mut Context<Self>) {
        let query = self.native_docs.search_query.trim().to_string();
        if query.is_empty() && !self.native_docs_type_filter_active() {
            self.native_docs.search_task = None;
            self.native_docs.search_results = None;
            self.native_docs.search_results_query.clear();
            return;
        }
        let key = self.native_docs_search_key();
        let extensions = self.native_docs_type_filter_extensions();
        let generation = self.native_docs.generation;
        self.native_docs.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.native_docs.generation != generation || this.native_docs_search_key() != key
                {
                    return;
                }
                let request = this.native_docs_request(
                    "search",
                    json!({ "query": query, "extensions": extensions }),
                );
                this.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
                    if this.native_docs.generation != generation
                        || this.native_docs_search_key() != key
                    {
                        return;
                    }
                    this.native_docs.search_results = Some(
                        response["entries"]
                            .as_array()
                            .map(|entries| entries.iter().filter_map(entry_from_json).collect())
                            .unwrap_or_default(),
                    );
                    this.native_docs.search_truncated =
                        response["truncated"].as_bool().unwrap_or(false);
                    this.native_docs.search_incomplete =
                        response["incomplete"].as_bool().unwrap_or(false);
                    this.native_docs.search_results_query = key;
                    this.native_docs_notify(cx);
                });
            });
        }));
    }

    /// The rows the files list draws: the tree with collapsed folders' children hidden, or, while
    /// searching, the project-wide search's matches, each with its folder.
    pub(crate) fn native_docs_tree_rows(&self) -> Vec<DocsTreeRow> {
        let state = &self.native_docs;
        if state.search_query.trim().is_empty() && self.native_docs_type_filter_active() {
            return super::type_filter::filtered_tree_rows(
                state.search_results.as_deref().unwrap_or_default(),
                &state.type_filter_collapsed,
            );
        }
        if !state.search_query.trim().is_empty() {
            return state
                .search_results
                .iter()
                .flatten()
                .map(|entry| DocsTreeRow {
                    path: entry.path.clone(),
                    display_path: entry.display_path.clone(),
                    name: entry.name.clone(),
                    kind: entry.kind,
                    depth: 0,
                    expanded: false,
                    folder: Some(parent_path(&entry.display_path).to_string())
                        .filter(|folder| !folder.is_empty()),
                })
                .collect();
        }
        let mut rows = Vec::new();
        // The depth below which rows are hidden because an ancestor is collapsed.
        let mut hidden_below: Option<usize> = None;
        for entry in &state.entries {
            if let Some(depth) = hidden_below {
                if entry.depth > depth {
                    continue;
                }
                hidden_below = None;
            }
            let expanded =
                entry.kind == DocsEntryKind::Directory && state.expanded.contains(&entry.path);
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
                folder: None,
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
        self.native_docs_reveal_in_tree(&active, cx);
        self.native_docs.reveal_request = Some(active);
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.native_docs.search_query.clear();
        self.native_docs_schedule_search(cx);
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

    /// Opens or closes a folder; opening one lists it (again), so it shows what is there now.
    pub(crate) fn native_docs_toggle_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        // The type filter's tree folds its own folders, leaving the real tree as it was.
        if self.native_docs.search_query.trim().is_empty() && self.native_docs_type_filter_active()
        {
            if !self.native_docs.type_filter_collapsed.remove(path) {
                self.native_docs
                    .type_filter_collapsed
                    .insert(path.to_string());
            }
            self.native_docs_notify(cx);
            return;
        }
        if !self.native_docs.expanded.remove(path) {
            self.native_docs.expanded.insert(path.to_string());
            self.native_docs_list_folder(path, cx);
        }
        self.native_docs_notify(cx);
    }

    /// Opens every folder above `path` so its row is drawn, listing the ones not listed yet.
    pub(crate) fn native_docs_reveal_in_tree(&mut self, path: &str, cx: &mut Context<Self>) {
        if Self::native_docs_is_outside_file(path) {
            return;
        }
        let mut parent = parent_path(path);
        let mut unlisted = Vec::new();
        while !parent.is_empty() {
            self.native_docs.expanded.insert(parent.to_string());
            if !self.native_docs.loaded_folders.contains(parent) {
                unlisted.push(parent.to_string());
            }
            parent = parent_path(parent);
        }
        // Outermost first, so each answer lands under a folder that is already in the tree.
        for folder in unlisted.into_iter().rev() {
            self.native_docs_list_folder(&folder, cx);
        }
    }

    pub(crate) fn native_docs_notify(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
}
