//! The Files view's Search sidebar (Cmd+Shift+F, Ctrl+Shift+F on Windows and Linux): VS Code's
//! Search view in place of the files list. This file holds its state, the search itself and the
//! keyboard; `project_search_view.rs` draws it.
//!
//! CDXC:Docs 2026-10-25 DECISION:
//! User: "make cmd + shift + f work just like the VS Code one pls (same sidebar look and behavior)", search only ("no replace at all"). The Search view takes the files list's place in the Files sidebar and switches back to the tree. It searches as you type (debounced, a new keystroke cancels the running search), Cmd+Shift+F with text selected in the editor seeds the query, Enter or F4 goes to the next result and Shift+F4 to the previous one, a single click opens the match with it highlighted while focus stays in the results (VS Code's preview) and a double click moves focus to the editor, Escape returns focus to the editor, and the query and options stay while the panel is closed.
//!
//! SEE-ALSO: packages/project-docs/src/content_search.rs (the engine and which files it skips).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ghostex_docs::content_search::{self, ContentSearchOptions, ContentSearchSummary, FileMatches};
use gpui::{
    AppContext as _, Context, Entity, FocusHandle, Focusable as _, KeyDownEvent, ScrollStrategy,
    Subscription, Task, UniformListScrollHandle, Window, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::GhostexGpuiApp;

/// VS Code's `search.searchOnTypeDebouncePeriod` is 300ms; a little quicker reads as instant.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
/// How often found files move from the search thread into the list while it runs.
const STREAM_INTERVAL: Duration = Duration::from_millis(60);
/// Frames a code editor's selection is applied for, so it scrolls once the editor has laid out.
const REVEAL_FRAMES: u8 = 4;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SearchRowKey {
    Folder(String),
    File(String),
    Match(String, usize),
}

/// One drawn row of the results.
#[derive(Clone, Debug)]
pub(crate) enum SearchRow {
    /// Tree view only: a folder, with single-folder chains folded into one row like VS Code.
    Folder {
        path: String,
        label: String,
        depth: usize,
        count: usize,
        expanded: bool,
    },
    File {
        path: String,
        depth: usize,
        count: usize,
        expanded: bool,
    },
    Match {
        path: String,
        index: usize,
        depth: usize,
    },
}

impl SearchRow {
    pub(crate) fn key(&self) -> SearchRowKey {
        match self {
            Self::Folder { path, .. } => SearchRowKey::Folder(path.clone()),
            Self::File { path, .. } => SearchRowKey::File(path.clone()),
            Self::Match { path, index, .. } => SearchRowKey::Match(path.clone(), *index),
        }
    }
}

/// A match being shown in its file: applied once the file's editor exists.
pub(crate) struct SearchReveal {
    path: String,
    line: usize,
    start: usize,
    end: usize,
    frames_left: u8,
    focus_editor: bool,
}

pub(crate) struct DocsProjectSearch {
    pub(crate) query: Entity<InputState>,
    pub(crate) include: Entity<InputState>,
    pub(crate) exclude: Entity<InputState>,
    pub(crate) match_case: bool,
    pub(crate) whole_word: bool,
    pub(crate) is_regex: bool,
    /// "Use exclude settings and ignore files", on by default.
    pub(crate) use_ignore_files: bool,
    /// "Toggle Search Details": the include and exclude fields are showing.
    pub(crate) details_open: bool,
    pub(crate) view_as_tree: bool,
    /// Files with matches, sorted by path.
    pub(crate) results: Vec<FileMatches>,
    pub(crate) rows: Vec<SearchRow>,
    rows_stale: bool,
    /// Folded files and folders.
    pub(crate) collapsed: HashSet<SearchRowKey>,
    /// What the results are for, and in which project folder.
    pub(crate) searched: Option<(ContentSearchOptions, PathBuf)>,
    pub(crate) running: bool,
    pub(crate) capped: bool,
    pub(crate) error: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
    task: Option<Task<()>>,
    generation: u64,
    pub(crate) selected: Option<SearchRowKey>,
    pub(crate) results_focus: FocusHandle,
    pub(crate) scroll: UniformListScrollHandle,
    reveal: Option<SearchReveal>,
    /// The Markdown document whose find highlight marks the shown match.
    marked_markdown: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl DocsProjectSearch {
    pub(crate) fn match_count(&self) -> usize {
        self.results.iter().map(|file| file.matches.len()).sum()
    }

    pub(crate) fn file(&self, path: &str) -> Option<&FileMatches> {
        self.results
            .binary_search_by(|file| file.path.as_str().cmp(path))
            .ok()
            .map(|index| &self.results[index])
    }

    fn merge(&mut self, files: Vec<FileMatches>) {
        for file in files {
            match self
                .results
                .binary_search_by(|existing| existing.path.cmp(&file.path))
            {
                Ok(index) => self.results[index] = file,
                Err(index) => self.results.insert(index, file),
            }
        }
        self.rows_stale = true;
    }

    pub(crate) fn rebuild_rows(&mut self) {
        if !self.rows_stale {
            return;
        }
        self.rows_stale = false;
        self.rows = build_rows(&self.results, self.view_as_tree, &self.collapsed);
        if self
            .selected
            .as_ref()
            .is_some_and(|selected| !self.rows.iter().any(|row| &row.key() == selected))
        {
            self.selected = None;
        }
    }

    pub(crate) fn mark_rows_stale(&mut self) {
        self.rows_stale = true;
    }

    /// Whether any file or folder in the results is open (the header shows Collapse All then).
    pub(crate) fn any_expanded(&self) -> bool {
        self.results.iter().any(|file| {
            !self
                .collapsed
                .contains(&SearchRowKey::File(file.path.clone()))
        })
    }
}

#[derive(Default)]
struct DirectoryNode {
    folders: BTreeMap<String, DirectoryNode>,
    files: Vec<usize>,
    count: usize,
}

fn push_file_rows(
    rows: &mut Vec<SearchRow>,
    file: &FileMatches,
    depth: usize,
    collapsed: &HashSet<SearchRowKey>,
) {
    let expanded = !collapsed.contains(&SearchRowKey::File(file.path.clone()));
    rows.push(SearchRow::File {
        path: file.path.clone(),
        depth,
        count: file.matches.len(),
        expanded,
    });
    if expanded {
        rows.extend((0..file.matches.len()).map(|index| SearchRow::Match {
            path: file.path.clone(),
            index,
            depth: depth + 1,
        }));
    }
}

fn push_folder_rows(
    rows: &mut Vec<SearchRow>,
    node: &DirectoryNode,
    prefix: &str,
    depth: usize,
    results: &[FileMatches],
    collapsed: &HashSet<SearchRowKey>,
) {
    for (name, child) in &node.folders {
        let mut label = name.clone();
        let mut path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let mut child = child;
        while child.files.is_empty() && child.folders.len() == 1 {
            let (next_name, next) = child.folders.iter().next().expect("one folder");
            label = format!("{label}/{next_name}");
            path = format!("{path}/{next_name}");
            child = next;
        }
        let expanded = !collapsed.contains(&SearchRowKey::Folder(path.clone()));
        rows.push(SearchRow::Folder {
            path: path.clone(),
            label,
            depth,
            count: child.count,
            expanded,
        });
        if expanded {
            push_folder_rows(rows, child, &path, depth + 1, results, collapsed);
        }
    }
    for index in &node.files {
        push_file_rows(rows, &results[*index], depth, collapsed);
    }
}

/// The rows for the results: files with their matches (list), or under their folders (tree).
fn build_rows(
    results: &[FileMatches],
    view_as_tree: bool,
    collapsed: &HashSet<SearchRowKey>,
) -> Vec<SearchRow> {
    let mut rows = Vec::new();
    if !view_as_tree {
        for file in results {
            push_file_rows(&mut rows, file, 0, collapsed);
        }
        return rows;
    }
    let mut root = DirectoryNode::default();
    for (index, file) in results.iter().enumerate() {
        let mut node = &mut root;
        node.count += file.matches.len();
        let mut parts = file.path.split('/').collect::<Vec<_>>();
        parts.pop();
        for part in parts {
            node = node.folders.entry(part.to_string()).or_default();
            node.count += file.matches.len();
        }
        node.files.push(index);
    }
    push_folder_rows(&mut rows, &root, "", 0, results, collapsed);
    rows
}

/// The byte range of `start..end` on 0-based `line` of `text`, clamped to the line.
fn line_range(text: &str, line: usize, start: usize, end: usize) -> std::ops::Range<usize> {
    let mut offset = 0;
    for (index, piece) in text.split_inclusive('\n').enumerate() {
        if index == line {
            let length = piece.trim_end_matches(['\n', '\r']).len();
            return offset + start.min(length)..offset + end.min(length);
        }
        offset += piece.len();
    }
    text.len()..text.len()
}

/// Whether a configured hotkey opens the Search view while Files has focus: Search in Files
/// itself, or the app-wide hotkey bound to the same chord (Find Prompts by default), which GPUI may
/// try first.
pub(crate) fn claims_hotkey(action_id: &str) -> bool {
    claims_hotkey_with(action_id, crate::app::hotkeys::gpui_configured_hotkey_key)
}

/// `claims_hotkey` with the configured chord of each action looked up through `key_of`.
fn claims_hotkey_with(action_id: &str, key_of: impl Fn(&str) -> Option<String>) -> bool {
    const SEARCH: &str = "searchProjectFiles";
    if action_id == SEARCH {
        return true;
    }
    let Some(chord) = key_of(SEARCH) else {
        return false;
    };
    key_of(action_id).is_some_and(|other| {
        crate::app::hotkeys::gpui_keystroke_from_shared_hotkey(&other)
            == crate::app::hotkeys::gpui_keystroke_from_shared_hotkey(&chord)
    })
}

impl GhostexGpuiApp {
    /// The Files roots' answer to a configured hotkey: Search in Files claims its chord (see
    /// `claims_hotkey`) and anything else goes on to the next listener.
    pub(crate) fn native_docs_claim_hotkey(
        &mut self,
        action: &crate::app::hotkeys::RunConfiguredGhostexHotkey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if claims_hotkey(&action.action_id) {
            self.native_docs_show_project_search(window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn native_docs_has_search_results(&self) -> bool {
        self.native_docs
            .project_search
            .as_ref()
            .is_some_and(|search| !search.results.is_empty())
    }

    pub(crate) fn native_docs_ensure_project_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.native_docs.project_search.is_some() {
            return;
        }
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let include =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. *.ts, src/**/include"));
        let exclude =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. *.ts, src/**/exclude"));
        let field_events =
            |this: &mut Self, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>| {
                match event {
                    InputEvent::Change => this.native_docs_schedule_project_search(false, cx),
                    // A search still waiting out the typing pause runs at once; otherwise Enter
                    // steps through the results like F4.
                    InputEvent::PressEnter { shift, .. } => {
                        if this
                            .native_docs
                            .project_search
                            .as_ref()
                            .is_some_and(|search| search.running && search.results.is_empty())
                        {
                            this.native_docs_schedule_project_search(true, cx);
                        } else {
                            this.native_docs_search_step(if *shift { -1 } else { 1 }, window, cx);
                        }
                    }
                    _ => {}
                }
            };
        let subscriptions = [&query, &include, &exclude]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(
                    input,
                    window,
                    move |this: &mut Self, _, event, window, cx| {
                        field_events(this, event, window, cx)
                    },
                )
            })
            .collect();
        self.native_docs.project_search = Some(DocsProjectSearch {
            query,
            include,
            exclude,
            match_case: false,
            whole_word: false,
            is_regex: false,
            use_ignore_files: true,
            details_open: false,
            view_as_tree: false,
            results: Vec::new(),
            rows: Vec::new(),
            rows_stale: false,
            collapsed: HashSet::new(),
            searched: None,
            running: false,
            capped: false,
            error: None,
            cancel: None,
            task: None,
            generation: 0,
            selected: None,
            results_focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            reveal: None,
            marked_markdown: None,
            _subscriptions: subscriptions,
        });
    }

    /// Cmd+Shift+F in Files: shows the Search view with the caret in its query, seeded with the
    /// editor's selection when one line of text is selected.
    pub(crate) fn native_docs_show_project_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_docs_ensure_project_search(window, cx);
        let seed = self.native_docs_editor_selection(cx);
        self.native_docs.search_panel = true;
        let layout = self.native_docs_sidebar_layout();
        if !layout.visible() || layout.narrow {
            self.native_docs_show_sidebar(cx);
        }
        let Some(search) = self.native_docs.project_search.as_ref() else {
            return;
        };
        let query = search.query.clone();
        if let Some(seed) = seed {
            let seed = if search.is_regex {
                regex::escape(&seed)
            } else {
                seed
            };
            query.update(cx, |input, cx| input.set_value(seed, window, cx));
            self.native_docs_schedule_project_search(true, cx);
        } else {
            self.native_docs_refresh_stale_project_search(cx);
        }
        self.native_docs_focus_list_input(&query, window, cx);
        self.native_docs_notify(cx);
    }

    /// Back to the files tree; the query, options and results stay for next time.
    pub(crate) fn native_docs_show_files_tree(&mut self, cx: &mut Context<Self>) {
        self.native_docs.search_panel = false;
        self.native_docs_notify(cx);
    }

    /// One line of selected text in the open editor, the query Cmd+Shift+F starts from.
    fn native_docs_editor_selection(&self, cx: &Context<Self>) -> Option<String> {
        let document = self.native_docs.active_document()?;
        let selected = if let Some(editor) = &document.editor {
            editor.read(cx).selected_text().to_string()
        } else if let Some(live) = &document.live {
            let live = live.read(cx);
            live.text().get(live.selected_range())?.to_string()
        } else {
            return None;
        };
        (!selected.trim().is_empty() && !selected.contains('\n')).then_some(selected)
    }

    fn native_docs_project_search_options(
        &self,
        cx: &Context<Self>,
    ) -> Option<ContentSearchOptions> {
        let search = self.native_docs.project_search.as_ref()?;
        Some(ContentSearchOptions {
            query: search.query.read(cx).value().to_string(),
            is_regex: search.is_regex,
            match_case: search.match_case,
            whole_word: search.whole_word,
            include: search.include.read(cx).value().to_string(),
            exclude: search.exclude.read(cx).value().to_string(),
            use_ignore_files: search.use_ignore_files,
            max_matches: content_search::DEFAULT_MAX_MATCHES,
        })
    }

    /// Runs the search again when the results are for another project or other options (the
    /// panel came back after a project switch).
    pub(crate) fn native_docs_refresh_stale_project_search(&mut self, cx: &mut Context<Self>) {
        let (Some(options), Some(project)) = (
            self.native_docs_project_search_options(cx),
            self.native_docs.project.as_ref(),
        ) else {
            return;
        };
        if options.query.is_empty() {
            return;
        }
        let current = Some((options, project.project_path.clone()));
        let Some(search) = self.native_docs.project_search.as_ref() else {
            return;
        };
        if !search.running && search.searched != current {
            self.native_docs_schedule_project_search(true, cx);
        }
    }

    /// Starts the search for the fields' current text: after a short pause while typing, at once
    /// for a toggle. A search still running for older text is cancelled.
    pub(crate) fn native_docs_schedule_project_search(
        &mut self,
        immediate: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(options) = self.native_docs_project_search_options(cx) else {
            return;
        };
        let Some(project) = self.native_docs.project.clone() else {
            return;
        };
        let remote =
            crate::app::helpers::gpui_remote_project_reference_from_project_id(&project.project_id)
                .is_some();
        let Some(search) = self.native_docs.project_search.as_mut() else {
            return;
        };
        if let Some(cancel) = search.cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        search.generation += 1;
        let generation = search.generation;
        if options.query.is_empty() {
            search.task = None;
            search.running = false;
            search.results.clear();
            search.searched = None;
            search.capped = false;
            search.error = None;
            search.mark_rows_stale();
            self.native_docs_notify(cx);
            return;
        }
        search.running = true;
        search.error = None;
        search.task = Some(cx.spawn(async move |this, cx| {
            if !immediate {
                cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            }
            let _ = this.update(cx, |this, cx| {
                let current = this
                    .native_docs
                    .project_search
                    .as_ref()
                    .is_some_and(|search| search.generation == generation);
                if !current {
                    return;
                }
                if remote {
                    this.native_docs_run_remote_project_search(
                        options,
                        project.project_path,
                        generation,
                        cx,
                    );
                } else {
                    this.native_docs_run_local_project_search(
                        options,
                        project.project_path,
                        generation,
                        cx,
                    );
                }
            });
        }));
        self.native_docs_notify(cx);
    }

    fn native_docs_begin_project_results(
        &mut self,
        options: &ContentSearchOptions,
        root: &std::path::Path,
        generation: u64,
    ) -> bool {
        let Some(search) = self
            .native_docs
            .project_search
            .as_mut()
            .filter(|search| search.generation == generation)
        else {
            return false;
        };
        search.results.clear();
        search.collapsed.clear();
        search.selected = None;
        search.capped = false;
        search.searched = Some((options.clone(), root.to_path_buf()));
        search.mark_rows_stale();
        search.scroll.scroll_to_item(0, ScrollStrategy::Top);
        true
    }

    /// A local project: the search runs on its own thread and found files stream into the list.
    fn native_docs_run_local_project_search(
        &mut self,
        options: ContentSearchOptions,
        root: PathBuf,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.native_docs_begin_project_results(&options, &root, generation) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let found: Arc<Mutex<Vec<FileMatches>>> = Arc::default();
        let finished: Arc<Mutex<Option<Result<ContentSearchSummary, String>>>> = Arc::default();
        let spawned = std::thread::Builder::new()
            .name("ghostex-files-search".into())
            .spawn({
                let (cancel, found, finished) = (cancel.clone(), found.clone(), finished.clone());
                move || {
                    let outcome =
                        content_search::search_content(&root, &options, &cancel, |file| {
                            found
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .push(file);
                        });
                    *finished
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(outcome);
                }
            });
        let Some(search) = self.native_docs.project_search.as_mut() else {
            return;
        };
        if spawned.is_err() {
            search.running = false;
            search.error = Some("The search could not start.".to_string());
            return;
        }
        search.cancel = Some(cancel);
        search.task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(STREAM_INTERVAL).await;
                // Read `finished` first: every file was handed over before it was set.
                let outcome = finished
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take();
                let batch = std::mem::take(
                    &mut *found
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()),
                );
                let keep_going = this
                    .update(cx, |this, cx| {
                        let Some(search) = this
                            .native_docs
                            .project_search
                            .as_mut()
                            .filter(|search| search.generation == generation)
                        else {
                            return false;
                        };
                        if !batch.is_empty() {
                            search.merge(batch);
                        }
                        if let Some(outcome) = outcome {
                            search.running = false;
                            search.cancel = None;
                            match outcome {
                                Ok(summary) => search.capped = summary.capped,
                                Err(error) => search.error = Some(error),
                            }
                            this.native_docs_notify(cx);
                            return false;
                        }
                        this.native_docs_notify(cx);
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }

    /// A remote project: its gxserver searches and answers with every match at once.
    fn native_docs_run_remote_project_search(
        &mut self,
        options: ContentSearchOptions,
        root: PathBuf,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.native_docs_begin_project_results(&options, &root, generation) {
            return;
        }
        let request =
            self.native_docs_request("searchContent", content_search::options_to_json(&options));
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            let Some(search) = this
                .native_docs
                .project_search
                .as_mut()
                .filter(|search| search.generation == generation)
            else {
                return;
            };
            search.running = false;
            if let Some(error) = response["error"].as_str() {
                search.error = Some(error.to_string());
            } else {
                let files = response["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(content_search::file_matches_from_json)
                    .collect();
                search.merge(files);
                search.capped = response["capped"].as_bool().unwrap_or(false);
            }
            this.native_docs_notify(cx);
        });
    }

    pub(crate) fn native_docs_toggle_search_option(
        &mut self,
        toggle: impl FnOnce(&mut DocsProjectSearch),
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.native_docs.project_search.as_mut() {
            toggle(search);
        }
        self.native_docs_schedule_project_search(true, cx);
    }

    /// Clear Search Results: empties the query and the list.
    pub(crate) fn native_docs_clear_project_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_docs_unmark_markdown(cx);
        let Some(search) = self.native_docs.project_search.as_ref() else {
            return;
        };
        let query = search.query.clone();
        query.update(cx, |input, cx| input.set_value("", window, cx));
        self.native_docs_schedule_project_search(true, cx);
        self.native_docs_focus_list_input(&query, window, cx);
    }

    pub(crate) fn native_docs_toggle_search_collapsed(
        &mut self,
        key: SearchRowKey,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.native_docs.project_search.as_mut() {
            if !search.collapsed.remove(&key) {
                search.collapsed.insert(key);
            }
            search.mark_rows_stale();
        }
        self.native_docs_notify(cx);
    }

    /// Collapse All when anything is open, otherwise Expand All.
    pub(crate) fn native_docs_toggle_search_collapse_all(&mut self, cx: &mut Context<Self>) {
        if let Some(search) = self.native_docs.project_search.as_mut() {
            if search.any_expanded() {
                let files = search
                    .results
                    .iter()
                    .map(|file| SearchRowKey::File(file.path.clone()));
                let folders = search.rows.iter().filter_map(|row| match row {
                    SearchRow::Folder { path, .. } => Some(SearchRowKey::Folder(path.clone())),
                    _ => None,
                });
                let keys: Vec<_> = files.chain(folders).collect();
                search.collapsed.extend(keys);
            } else {
                search.collapsed.clear();
            }
            search.mark_rows_stale();
        }
        self.native_docs_notify(cx);
    }

    /// The × on a hovered row: drops the match (or the file and its matches) from the list.
    pub(crate) fn native_docs_dismiss_search_row(
        &mut self,
        key: SearchRowKey,
        cx: &mut Context<Self>,
    ) {
        let Some(search) = self.native_docs.project_search.as_mut() else {
            return;
        };
        search.rebuild_rows();
        let position = search.rows.iter().position(|row| row.key() == key);
        match &key {
            SearchRowKey::File(path) => search.results.retain(|file| &file.path != path),
            SearchRowKey::Folder(path) => {
                let prefix = format!("{path}/");
                search
                    .results
                    .retain(|file| !file.path.starts_with(&prefix));
            }
            SearchRowKey::Match(path, index) => {
                if let Ok(at) = search.results.binary_search_by(|file| file.path.cmp(path)) {
                    let file = &mut search.results[at];
                    if *index < file.matches.len() {
                        file.matches.remove(*index);
                    }
                    if file.matches.is_empty() {
                        search.results.remove(at);
                    }
                }
            }
        }
        search.mark_rows_stale();
        search.rebuild_rows();
        if search.selected.is_none() || search.selected.as_ref() == Some(&key) {
            search.selected = position
                .and_then(|position| search.rows.get(position).or_else(|| search.rows.last()))
                .map(SearchRow::key);
        }
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_select_search_row(
        &mut self,
        key: SearchRowKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(search) = self.native_docs.project_search.as_mut() else {
            return;
        };
        search.selected = Some(key);
        search.results_focus.focus(window, cx);
        self.native_docs_notify(cx);
    }

    /// Opens a match's file with the match selected: focus stays where it is (a single click, F4)
    /// or moves to the editor (a double click).
    pub(crate) fn native_docs_open_search_match(
        &mut self,
        path: &str,
        index: usize,
        focus_editor: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(found) = self
            .native_docs
            .project_search
            .as_ref()
            .and_then(|search| search.file(path))
            .and_then(|file| file.matches.get(index))
            .cloned()
        else {
            return;
        };
        if self.native_docs.active.as_deref() != Some(path) {
            self.native_docs_unmark_markdown(cx);
        }
        self.native_docs_open(path, path, cx);
        if let Some(search) = self.native_docs.project_search.as_mut() {
            search.selected = Some(SearchRowKey::Match(path.to_string(), index));
            search.reveal = Some(SearchReveal {
                path: path.to_string(),
                line: found.line,
                start: found.start,
                end: found.end,
                frames_left: REVEAL_FRAMES,
                focus_editor,
            });
        }
        self.native_docs_notify(cx);
    }

    /// F4 / Shift+F4, and Enter in the fields: the next or previous match after the selected row,
    /// wrapping around, opened in its file.
    pub(crate) fn native_docs_search_step(
        &mut self,
        delta: isize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(search) = self.native_docs.project_search.as_mut() else {
            return;
        };
        if search.results.is_empty() {
            return;
        }
        // Every match in list order, whatever is folded.
        let all: Vec<(String, usize)> =
            build_rows(&search.results, search.view_as_tree, &HashSet::new())
                .into_iter()
                .filter_map(|row| match row {
                    SearchRow::Match { path, index, .. } => Some((path, index)),
                    _ => None,
                })
                .collect();
        if all.is_empty() {
            return;
        }
        let current = match &search.selected {
            Some(SearchRowKey::Match(path, index)) => all
                .iter()
                .position(|(p, i)| p == path && i == index)
                .map(|at| at as isize),
            Some(SearchRowKey::File(path)) => all
                .iter()
                .position(|(p, _)| p == path)
                .map(|at| at as isize - 1),
            _ => None,
        };
        let len = all.len() as isize;
        let next = match current {
            Some(at) => (at + delta).rem_euclid(len),
            None if delta < 0 => len - 1,
            None => 0,
        } as usize;
        let (path, index) = all[next].clone();
        // Unfold what hides it.
        search.collapsed.remove(&SearchRowKey::File(path.clone()));
        let mut parent = path.as_str();
        while let Some((folder, _)) = parent.rsplit_once('/') {
            search
                .collapsed
                .remove(&SearchRowKey::Folder(folder.to_string()));
            parent = folder;
        }
        search.mark_rows_stale();
        search.rebuild_rows();
        let key = SearchRowKey::Match(path.clone(), index);
        if let Some(row) = search.rows.iter().position(|row| row.key() == key) {
            search.scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
        self.native_docs_open_search_match(&path, index, false, cx);
    }

    /// Escape in the Search view: back to the open file's editor.
    pub(crate) fn native_docs_search_focus_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let document = self.native_docs.active_document();
        if let Some(editor) = document.and_then(|document| document.editor.clone()) {
            editor.update(cx, |state, cx| state.focus(window, cx));
        } else if let Some(live) = document.and_then(|document| document.live.clone()) {
            live.update(cx, |editor, cx| editor.focus(window, cx));
        } else if let Some(focus) = self.native_docs.focus.clone() {
            focus.focus(window, cx);
        }
    }

    fn native_docs_unmark_markdown(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .native_docs
            .project_search
            .as_mut()
            .and_then(|search| search.marked_markdown.take())
        else {
            return;
        };
        let find_open = self.native_docs_find_visible();
        if let Some(live) = self
            .native_docs
            .document(&path)
            .and_then(|document| document.live.clone())
            && !find_open
        {
            live.update(cx, |editor, cx| editor.set_search(Vec::new(), None, cx));
        }
    }

    /// Selects the match a result click asked for once its file's editor exists. Runs from the
    /// Files view's render.
    pub(crate) fn native_docs_apply_search_reveal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(search) = self.native_docs.project_search.as_ref() else {
            return;
        };
        let Some(reveal) = search.reveal.as_ref() else {
            return;
        };
        let (path, line, start, end, focus_editor) = (
            reveal.path.clone(),
            reveal.line,
            reveal.start,
            reveal.end,
            reveal.focus_editor,
        );
        let query_focus = search.query.read(cx).focus_handle(cx);
        let selection_focus = if query_focus.is_focused(window) {
            query_focus
        } else {
            search.results_focus.clone()
        };
        let Some(document) = self
            .native_docs
            .document(&path)
            .filter(|_| self.native_docs.active.as_deref() == Some(path.as_str()))
        else {
            self.native_docs_end_reveal();
            return;
        };
        if let Some(editor) = document.editor.clone() {
            editor.update(cx, |state, cx| {
                let range = line_range(&state.value(), line, start, end);
                state.set_selection_focus((!focus_editor).then_some(selection_focus), cx);
                state.set_selected_range(range, cx);
                if focus_editor {
                    state.focus(window, cx);
                }
            });
            let done = self
                .native_docs
                .project_search
                .as_mut()
                .and_then(|search| search.reveal.as_mut())
                .is_none_or(|reveal| {
                    reveal.frames_left = reveal.frames_left.saturating_sub(1);
                    reveal.frames_left == 0
                });
            if done {
                self.native_docs_end_reveal();
            } else {
                window.request_animation_frame();
            }
        } else if let Some(live) = document.live.clone() {
            let scroll = document.scroll.clone();
            let range = line_range(live.read(cx).text(), line, start, end);
            // The document's own find panel keeps its matches while it is open.
            let find_open = self.native_docs_find_visible();
            live.update(cx, |editor, cx| {
                editor.select_range(range.clone(), cx);
                if !find_open {
                    editor.set_search(vec![range.clone()], Some(0), cx);
                }
                if focus_editor {
                    editor.focus(window, cx);
                }
            });
            // Scroll so the match sits a third of the way down, once the editor has laid out.
            match live.read(cx).offset_screen_top(range.start) {
                Some(top) => {
                    let viewport = scroll.bounds();
                    let mut offset = scroll.offset();
                    let target = viewport.top() + viewport.size.height / 3.0;
                    if top < viewport.top() + px(24.0) || top > viewport.bottom() - px(96.0) {
                        offset.y = (offset.y - (top - target)).min(px(0.0));
                        scroll.set_offset(offset);
                    }
                    if let Some(search) = self.native_docs.project_search.as_mut() {
                        search.marked_markdown = Some(path);
                    }
                    let done = self
                        .native_docs
                        .project_search
                        .as_mut()
                        .and_then(|search| search.reveal.as_mut())
                        .is_none_or(|reveal| {
                            reveal.frames_left = reveal.frames_left.saturating_sub(1);
                            reveal.frames_left == 0
                        });
                    if done {
                        self.native_docs_end_reveal();
                    } else {
                        window.request_animation_frame();
                    }
                }
                None => window.request_animation_frame(),
            }
        } else if !matches!(document.load, super::state::DocsDocumentLoad::Loading) {
            // Not a text file, or it failed to load: nothing to select.
            self.native_docs_end_reveal();
        }
    }

    fn native_docs_end_reveal(&mut self) {
        if let Some(search) = self.native_docs.project_search.as_mut() {
            search.reveal = None;
        }
    }

    /// The results list from the keyboard: the arrows move and fold, Enter or Space opens a match
    /// (or folds a file), Delete dismisses, F4 and Shift+F4 step through the matches.
    pub(crate) fn native_docs_project_search_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        let Some(search) = self.native_docs.project_search.as_mut() else {
            return;
        };
        let results_focused = search.results_focus.is_focused(window);
        let query_focused = search.query.read(cx).focus_handle(cx).is_focused(window);
        let key = event.keystroke.key.as_str();
        match key {
            "f4" => {
                self.native_docs_search_step(if modifiers.shift { -1 } else { 1 }, window, cx);
                cx.stop_propagation();
                return;
            }
            "escape" => {
                self.native_docs_search_focus_editor(window, cx);
                cx.stop_propagation();
                return;
            }
            "down" if query_focused => {
                search.rebuild_rows();
                if search.selected.is_none() {
                    search.selected = search.rows.first().map(SearchRow::key);
                }
                search.results_focus.focus(window, cx);
                cx.stop_propagation();
                self.native_docs_notify(cx);
                return;
            }
            _ => {}
        }
        if !results_focused || modifiers.shift {
            return;
        }
        search.rebuild_rows();
        let position = search
            .selected
            .as_ref()
            .and_then(|selected| search.rows.iter().position(|row| &row.key() == selected));
        let count = search.rows.len();
        if count == 0 {
            return;
        }
        let select = |search: &mut DocsProjectSearch, index: usize| {
            search.selected = search.rows.get(index).map(SearchRow::key);
            search.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        };
        match key {
            "down" => select(search, position.map_or(0, |at| (at + 1).min(count - 1))),
            "up" => {
                if position == Some(0) {
                    let query = search.query.clone();
                    query.update(cx, |input, cx| input.focus(window, cx));
                } else {
                    select(search, position.map_or(0, |at| at.saturating_sub(1)));
                }
            }
            "home" => select(search, 0),
            "end" => select(search, count - 1),
            "left" | "right" | "enter" | "space" => {
                let Some(row) = position.and_then(|at| search.rows.get(at)).cloned() else {
                    return;
                };
                match (key, row) {
                    (
                        "left",
                        SearchRow::Folder {
                            expanded: true,
                            path,
                            ..
                        },
                    ) => self.native_docs_toggle_search_collapsed(SearchRowKey::Folder(path), cx),
                    (
                        "left",
                        SearchRow::File {
                            expanded: true,
                            path,
                            ..
                        },
                    ) => self.native_docs_toggle_search_collapsed(SearchRowKey::File(path), cx),
                    ("left", SearchRow::Match { path, .. }) => {
                        search.selected = Some(SearchRowKey::File(path));
                    }
                    ("left", SearchRow::File { path, .. } | SearchRow::Folder { path, .. }) => {
                        if let Some((parent, _)) = path.rsplit_once('/')
                            && search.view_as_tree
                        {
                            let parent = search.rows.iter().position(|row| {
                                matches!(row, SearchRow::Folder { path: folder, .. }
                                    if parent == folder || parent.starts_with(&format!("{folder}/")))
                            });
                            if let Some(parent) = parent {
                                select(search, parent);
                            }
                        }
                    }
                    (
                        "right",
                        SearchRow::Folder {
                            expanded: false,
                            path,
                            ..
                        },
                    ) => self.native_docs_toggle_search_collapsed(SearchRowKey::Folder(path), cx),
                    (
                        "right",
                        SearchRow::File {
                            expanded: false,
                            path,
                            ..
                        },
                    ) => self.native_docs_toggle_search_collapsed(SearchRowKey::File(path), cx),
                    ("enter" | "space", SearchRow::Folder { path, .. }) => {
                        self.native_docs_toggle_search_collapsed(SearchRowKey::Folder(path), cx)
                    }
                    ("enter" | "space", SearchRow::File { path, .. }) => {
                        self.native_docs_toggle_search_collapsed(SearchRowKey::File(path), cx)
                    }
                    ("enter" | "space", SearchRow::Match { path, index, .. }) => {
                        self.native_docs_open_search_match(&path, index, false, cx)
                    }
                    _ => {}
                }
            }
            "delete" | "backspace" => {
                if let Some(selected) = search.selected.clone() {
                    self.native_docs_dismiss_search_row(selected, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        self.native_docs_notify(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::claims_hotkey_with;

    fn keys<'a>(map: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |id| {
            map.iter()
                .find(|(action, _)| *action == id)
                .map(|(_, key)| key.to_string())
        }
    }

    #[test]
    fn files_claims_the_chord_it_shares_with_find_prompts() {
        let defaults = [
            ("searchProjectFiles", "cmd+shift+f"),
            ("openFindPrompts", "cmd+shift+f"),
            ("focusNextSession", "cmd+alt+down"),
        ];
        assert!(claims_hotkey_with("searchProjectFiles", keys(&defaults)));
        assert!(claims_hotkey_with("openFindPrompts", keys(&defaults)));
        assert!(!claims_hotkey_with("focusNextSession", keys(&defaults)));
    }

    #[test]
    fn find_prompts_goes_on_once_search_in_files_has_another_chord() {
        let moved = [
            ("searchProjectFiles", "cmd+alt+f"),
            ("openFindPrompts", "cmd+shift+f"),
        ];
        assert!(claims_hotkey_with("searchProjectFiles", keys(&moved)));
        assert!(!claims_hotkey_with("openFindPrompts", keys(&moved)));
        let cleared = [("openFindPrompts", "cmd+shift+f")];
        assert!(!claims_hotkey_with("openFindPrompts", keys(&cleared)));
    }
}
