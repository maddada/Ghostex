//! The picker's operations: going to a folder (sidebar, columns, path field, Back, Forward, Up),
//! switching computers, adding, making a folder, Clone a repo (lookup, destination check, clone),
//! pinning a folder, and closing.
use super::input::*;
use super::model::*;
use super::paths::*;
use super::window::*;
use gpui::{Context, Window};

/// The GitLab lookup names the project path, not the clone URL:
/// `/^(?:https?:\/\/|ssh:\/\/(?:[^@/]+@)?|[^@]+@)[^/:]+[/:]/u` and a trailing `.git` are removed.
fn gitlab_project_path(remote_url: &str) -> String {
    static PREFIX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let prefix = PREFIX.get_or_init(|| {
        regex::Regex::new(r"^(?:https?://|ssh://(?:[^@/]+@)?|[^@]+@)[^/:]+[/:]")
            .expect("gitlab prefix regex")
    });
    let stripped = prefix.replace(remote_url, "");
    stripped
        .strip_suffix(".git")
        .unwrap_or(&stripped)
        .to_string()
}

impl GpuiAddProjectModalWindow {
    // -- navigation -----------------------------------------------------------------------------

    fn history_entry(&self) -> Option<HistoryEntry> {
        Some(HistoryEntry {
            machine_id: self.machine_id.clone()?,
            query: self.query.clone(),
            column_root: self.column_root.clone(),
        })
    }

    /// Opens `folder` (any spelling: `~/dev`, `/Users/me/dev`, `C:\dev`), remembering where the
    /// picker was for Back. `column_root` starts the columns there (a sidebar place).
    pub(super) fn go_to_folder(&mut self, folder: &str, column_root: Option<String>) {
        if self.busy.is_some() {
            return;
        }
        let query = ensure_browse_directory_path(&self.tildify(&trim_folder(folder)));
        if query == self.query && !self.suggested_mode {
            return;
        }
        if let Some(entry) = self.history_entry().filter(|_| !self.suggested_mode) {
            self.history_back.push(entry);
        }
        self.history_forward.clear();
        self.suggested_mode = false;
        self.root_at_start = false;
        self.column_root = column_root.map(|root| trim_folder(&root));
        self.set_query(query);
    }

    /// A click in a column: the folder opens beside it and the columns keep their start.
    pub(super) fn open_column_entry(&mut self, full_path: &str) {
        let root = self.column_root.clone();
        self.go_to_folder(full_path, root);
    }

    fn set_query(&mut self, query: String) {
        self.query = query;
        self.highlight = None;
        self.new_folder_name = None;
        self.error = None;
        self.clone_preview = None;
        self.machine_menu_open = false;
        self.failed_columns.clear();
        self.clone_generation += 1;
        self.browse_generation += 1;
    }

    pub(super) fn go_back(&mut self) {
        self.step_history(true);
    }

    pub(super) fn go_forward(&mut self) {
        self.step_history(false);
    }

    fn step_history(&mut self, back: bool) {
        if self.busy.is_some() {
            return;
        }
        let entry = if back {
            self.history_back.pop()
        } else {
            self.history_forward.pop()
        };
        let Some(entry) = entry else {
            return;
        };
        if let Some(current) = self.history_entry() {
            if back {
                self.history_forward.push(current);
            } else {
                self.history_back.push(current);
            }
        }
        if self.machine_id.as_deref() != Some(entry.machine_id.as_str()) {
            self.machine_id = Some(entry.machine_id);
            self.reset_for_machine();
        }
        self.suggested_mode = false;
        self.column_root = entry.column_root;
        self.set_query(entry.query);
    }

    /// Up: the folder above the selected one.
    pub(super) fn go_up(&mut self, d: &Derived) {
        if self.busy.is_some() {
            return;
        }
        let current = d
            .selected_folder
            .clone()
            .unwrap_or_else(|| trim_folder(&d.browse_directory_path));
        let Some(parent) = self.parent_folder(&current, &d.platform) else {
            return;
        };
        let root = self
            .column_root
            .clone()
            .filter(|root| folder_contains(root, &parent));
        let highlight = current.clone();
        self.go_to_folder(&parent, root);
        self.highlight = Some(highlight);
    }

    pub(super) fn on_path_input_changed(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() || value == self.query {
            return;
        }
        self.highlight = None;
        self.query = value;
        self.error = None;
        self.clone_preview = None;
        self.suggested_mode = false;
        self.failed_columns.clear();
        self.clone_generation += 1;
        // A path typed outside the columns' start shows its own two levels.
        if let Some(root) = self.column_root.clone() {
            let home = self.current_home();
            let typed = expand_home(&get_browse_directory_path(&self.query), home.as_deref());
            if !folder_contains(&root, &typed) {
                self.column_root = None;
            }
        }
        self.settle(window, cx);
    }

    /// Another computer: its places and its starting folder.
    pub(super) fn choose_machine(&mut self, machine_id: String) {
        self.machine_menu_open = false;
        if self.busy.is_some() {
            return;
        }
        if self.machine_id.as_deref() == Some(machine_id.as_str()) {
            return;
        }
        if let Some(entry) = self.history_entry() {
            self.history_back.push(entry);
        }
        self.history_forward.clear();
        self.machine_id = Some(machine_id.clone());
        self.reset_for_machine();
        self.suggested_mode = false;
        self.column_root = None;
        self.root_at_start = true;
        let start = self.start_query();
        self.set_query(start);
        if self.new_project.is_some() {
            self.open_new_project_form(machine_id);
        }
    }

    /// What belongs to the previous machine is dropped when another one is browsed.
    pub(super) fn reset_for_machine(&mut self) {
        self.browse_result = None;
        self.last_selected_folder = None;
        self.clone_preview = None;
        self.clone_flow = None;
        self.tool_install = None;
        self.new_folder_name = None;
        self.pending_discovery_machine_id = None;
        self.failed_columns.clear();
        self.machine_generation += 1;
        self.clone_generation += 1;
        if matches!(self.sheet, Some(Sheet::NewProjectPick(_))) {
            self.sheet = Some(Sheet::NewProject);
        }
    }

    pub(super) fn open_suggested(&mut self) {
        self.suggested_mode = true;
        self.new_folder_name = None;
        self.highlight = None;
    }

    // -- adding ---------------------------------------------------------------------------------

    pub(super) fn validate_project_path(&mut self, raw: &str, platform: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !platform.is_empty() && is_unsupported_windows_project_path(trimmed, platform) {
            self.error =
                Some("Windows-style paths are only supported on Windows machines.".to_string());
            return None;
        }
        if is_explicit_relative_project_path(trimmed) && self.active_project_cwd.is_none() {
            self.error = Some("Relative paths require an active project.".to_string());
            return None;
        }
        let resolved =
            resolve_project_path_for_dispatch(trimmed, self.active_project_cwd.as_deref());
        (!resolved.is_empty()).then_some(resolved)
    }

    pub(super) fn register_project_on(
        &mut self,
        machine_id: String,
        path: String,
        create_if_missing: bool,
        after_clone: bool,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        self.request(
            "add",
            &machine_id,
            serde_json::json!({ "createIfMissing": create_if_missing, "path": path }),
            Pending::Add {
                machine_id: machine_id.clone(),
                after_clone,
                generation,
            },
            cx,
        );
    }

    pub(super) fn register_project(
        &mut self,
        path: String,
        create_if_missing: bool,
        after_clone: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(machine_id) = self.machine_id.clone() else {
            return;
        };
        let generation = if after_clone {
            self.clone_generation
        } else {
            self.machine_generation
        };
        self.register_project_on(
            machine_id,
            path,
            create_if_missing,
            after_clone,
            generation,
            cx,
        );
    }

    /// Adds `raw` as a project; `create_if_missing` makes the folder first (Create & Add).
    pub(super) fn submit_add_path(
        &mut self,
        raw: &str,
        create_if_missing: bool,
        cx: &mut Context<Self>,
    ) {
        let d = self.derive();
        if self.busy.is_some() || d.machine_id.is_none() {
            return;
        }
        let Some(path) = self.validate_project_path(raw, &d.platform) else {
            return;
        };
        self.set_busy(Some(Busy::Add), cx);
        self.error = None;
        self.register_project(path, create_if_missing, false, cx);
    }

    /// The footer's Add project (or Open project, Create & Add, Add repository root).
    pub(super) fn submit_resolved_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.suggested_mode || self.sheet.is_some() {
            return;
        }
        let d = self.derive();
        if !d.can_submit_browse_path {
            return;
        }
        self.submit_add_path(&d.resolved_add_project_path, d.will_create_project_path, cx);
        self.settle(window, cx);
    }

    // -- new folder -----------------------------------------------------------------------------

    pub(super) fn start_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.derive().can_create_new_folder || self.busy.is_some() {
            return;
        }
        self.error = None;
        self.highlight = None;
        self.suggested_mode = false;
        self.new_folder_name = Some(String::new());
        self.folder_name_input.update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
            input.focus(window, cx);
        });
        self.settle(window, cx);
    }

    pub(super) fn cancel_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_folder_name = None;
        self.error = None;
        self.focus_path_input(window, cx);
        self.settle(window, cx);
    }

    pub(super) fn submit_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .new_folder_name
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();
        let d = self.derive();
        let Some(machine_id) = d.machine_id.clone() else {
            return;
        };
        if self.busy.is_some() || d.new_folder_parent_path.is_empty() {
            return;
        }
        if name.is_empty() {
            self.cancel_new_folder(window, cx);
            return;
        }
        if name.contains(['/', '\\']) {
            self.error = Some("A folder name cannot contain a path separator.".to_string());
            self.settle(window, cx);
            return;
        }
        self.set_busy(Some(Busy::CreateFolder), cx);
        self.error = None;
        let parent = d.new_folder_parent_path.clone();
        let generation = self.machine_generation;
        self.request(
            "createDirectory",
            &machine_id,
            serde_json::json!({ "name": name, "parentPath": parent }),
            Pending::CreateFolder {
                machine_id: machine_id.clone(),
                parent,
                generation,
            },
            cx,
        );
        self.settle(window, cx);
    }

    // -- clone a repo ---------------------------------------------------------------------------

    /// Opens Clone a repo, filled in with `prefill` (a pasted repository) when given.
    pub(super) fn open_clone_sheet(
        &mut self,
        prefill: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        self.sheet = Some(Sheet::Clone);
        // GitHub's repository list opens with the panel, unless a repository came with it.
        self.repo_list_open = prefill.is_none();
        self.new_folder_name = None;
        self.clone_preview = None;
        self.clone_flow = None;
        self.clone_generation += 1;
        self.error = None;
        if let Some(text) = prefill {
            if let Some(ParsedCloneInput::Clone(parsed)) =
                parse_add_project_clone_input(&text, AddProjectSourceId::Github)
            {
                self.clone_options = CloneOptions::from_detected(&parsed);
                let branch = self.clone_options.branch_name.clone();
                self.branch_input.update(cx, |input, cx| {
                    input.set_value(branch, window, cx);
                });
                self.clone_source = parsed.source;
            }
            self.clone_repo_text = text.clone();
            self.repo_input.update(cx, |input, cx| {
                input.set_value(text, window, cx);
            });
        }
        self.repo_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.settle(window, cx);
    }

    pub(super) fn close_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.sheet {
            Some(Sheet::NewProjectPick(_)) => self.sheet = Some(Sheet::NewProject),
            Some(_) => {
                if self.busy == Some(Busy::Clone) {
                    return;
                }
                if matches!(self.busy, Some(Busy::Lookup | Busy::Preview)) {
                    self.set_busy(None, cx);
                }
                self.sheet = None;
                self.new_project = None;
                self.clone_preview = None;
                self.clone_flow = None;
                self.clone_generation += 1;
                self.error = None;
                self.focus_path_input(window, cx);
            }
            None => {}
        }
        self.settle(window, cx);
    }

    pub(super) fn choose_clone_source(&mut self, source: AddProjectSourceId) {
        if self.busy.is_some() {
            return;
        }
        self.clone_source = source;
        self.clone_preview = None;
        self.clone_flow = None;
        self.clone_generation += 1;
        self.error = None;
    }

    pub(super) fn on_repo_input_changed(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() || value == self.clone_repo_text {
            return;
        }
        // A pasted URL or command picks its source; a bare `owner/repo` keeps the chosen one.
        if let Some(ParsedCloneInput::Clone(parsed)) =
            parse_add_project_clone_input(&value, self.clone_source)
        {
            self.clone_source = parsed.source;
            if !parsed.branch_name.is_empty() {
                self.clone_options.branch_name = parsed.branch_name.clone();
                let branch = self.clone_options.branch_name.clone();
                self.branch_input.update(cx, |input, cx| {
                    input.set_value(branch, window, cx);
                });
            }
            self.clone_options.clone_main_only |= parsed.clone_main_only;
            self.clone_options.shallow_clone |= parsed.shallow_clone;
        }
        self.clone_repo_text = value;
        self.repo_list_open = true;
        self.repo_highlight = None;
        self.clone_preview = None;
        self.clone_flow = None;
        self.clone_generation += 1;
        self.error = None;
        cx.notify();
    }

    /// The folder Clone & Add clones into: the folder on screen plus the repository's name.
    pub(super) fn clone_destination(&self, d: &Derived) -> Option<String> {
        let destination = self
            .clone_flow
            .as_ref()
            .map(|flow| flow.destination.clone())
            .or_else(|| {
                match parse_add_project_clone_input(&self.clone_repo_text, self.clone_source) {
                    Some(ParsedCloneInput::Clone(parsed)) => Some(parsed.destination),
                    _ => Some(repository_folder_name(self.clone_repo_text.trim()))
                        .filter(|name| !name.is_empty()),
                }
            })?;
        if let Some(explicit) = normalize_pasted_project_path(&destination)
            && !is_explicit_relative_project_path(&explicit)
            && (explicit.starts_with('/') || explicit.starts_with('~') || explicit.contains(':'))
        {
            return Some(explicit);
        }
        let folder = d.selected_folder.clone().filter(|_| !d.is_drive_list)?;
        Some(format!(
            "{}{}",
            ensure_browse_directory_path(&folder),
            destination.strip_prefix("./").unwrap_or(&destination)
        ))
    }

    /// Clone & Add: a provider lookup when the repository needs one, then the destination check,
    /// then the clone (`preview_clone` and its answer chain the steps).
    pub(super) fn submit_clone_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.machine_id.is_none() {
            return;
        }
        let d = self.derive();
        let source = self.clone_source;
        if !d.readiness[source.index()].ready {
            return;
        }
        let input = self.clone_repo_text.trim().to_string();
        if input.is_empty() {
            self.error = Some("Enter a repository, URL, or clone command.".to_string());
            self.settle(window, cx);
            return;
        }
        if self.clone_flow.is_some() {
            self.preview_clone(cx);
            self.settle(window, cx);
            return;
        }
        let parsed = match parse_add_project_clone_input(&input, source) {
            Some(ParsedCloneInput::Error(message)) => {
                self.error = Some(message);
                self.settle(window, cx);
                return;
            }
            Some(ParsedCloneInput::Clone(parsed)) => Some(parsed),
            None => None,
        };
        if let Some(parsed) = &parsed {
            if !parsed.branch_name.is_empty() {
                self.clone_options.branch_name = parsed.branch_name.clone();
                let branch = self.clone_options.branch_name.clone();
                self.branch_input.update(cx, |input, cx| {
                    input.set_value(branch, window, cx);
                });
            }
            self.clone_options.clone_main_only |= parsed.clone_main_only;
            self.clone_options.shallow_clone |= parsed.shallow_clone;
            if source == AddProjectSourceId::Url
                || !matches!(
                    parsed.source,
                    AddProjectSourceId::Github | AddProjectSourceId::Gitlab
                )
            {
                self.clone_flow = Some(CloneFlow {
                    remote_url: parsed.remote_url.clone(),
                    repository: None,
                    destination: parsed.destination.clone(),
                });
                self.preview_clone(cx);
                self.settle(window, cx);
                return;
            }
        }
        if parsed.is_none() && source == AddProjectSourceId::Url {
            self.error = Some("Enter a Git repository URL or clone command.".to_string());
            self.settle(window, cx);
            return;
        }
        let Some(machine_id) = self.machine_id.clone() else {
            return;
        };
        self.set_busy(Some(Busy::Lookup), cx);
        self.error = None;
        let provider = parsed
            .as_ref()
            .map(|parsed| parsed.source)
            .unwrap_or(source);
        let repository = match &parsed {
            Some(parsed) if parsed.source == AddProjectSourceId::Gitlab => {
                gitlab_project_path(&parsed.remote_url)
            }
            Some(parsed) => parsed.remote_url.clone(),
            None => input.clone(),
        };
        let generation = self.clone_generation;
        self.request(
            "lookupRepository",
            &machine_id,
            serde_json::json!({ "provider": provider.wire(), "repository": repository }),
            Pending::Lookup {
                machine_id: machine_id.clone(),
                generation,
                parsed,
            },
            cx,
        );
        self.settle(window, cx);
    }

    /// Asks the machine whether the destination is free; a free one clones right away.
    pub(super) fn preview_clone(&mut self, cx: &mut Context<Self>) {
        let d = self.derive();
        let Some(flow) = self.clone_flow.clone() else {
            return;
        };
        let Some(machine_id) = d.machine_id.clone() else {
            return;
        };
        let Some(destination) = self.clone_destination(&d) else {
            self.error = Some("Pick the folder to clone into on the left.".to_string());
            return;
        };
        let Some(destination_path) = self.validate_project_path(&destination, &d.platform) else {
            return;
        };
        if !is_repository_clone_branch_name_input_valid(&self.clone_options.branch_name) {
            self.error = Some("Enter a valid Git branch name.".to_string());
            return;
        }
        self.set_busy(Some(Busy::Preview), cx);
        self.error = None;
        let options = self.clone_options.clone();
        let generation = self.clone_generation;
        self.request(
            "previewClone",
            &machine_id,
            serde_json::json!({
                "branchName": options.branch_name,
                "cloneMainOnly": options.clone_main_only,
                "destinationPath": destination_path,
                "remoteUrl": flow.remote_url,
                "shallowClone": options.shallow_clone,
            }),
            Pending::Preview {
                machine_id: machine_id.clone(),
                generation,
                destination_path,
            },
            cx,
        );
    }

    pub(super) fn can_clone(&self) -> bool {
        is_repository_clone_branch_name_input_valid(&self.clone_options.branch_name)
            && self
                .clone_preview
                .as_ref()
                .is_some_and(|preview| !preview.destination_blocked)
            && self.busy.is_none()
    }

    pub(super) fn submit_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(flow) = self.clone_flow.clone() else {
            return;
        };
        let Some(machine_id) = self.machine_id.clone() else {
            return;
        };
        if !self.can_clone() {
            return;
        }
        self.error = None;
        let options = self.clone_options.clone();
        let params = serde_json::json!({
            "branchName": options.branch_name,
            "cloneMainOnly": options.clone_main_only,
            "destinationPath": self.clone_destination_path,
            "remoteUrl": flow.remote_url,
            "shallowClone": options.shallow_clone,
        });
        // CDXC:BackgroundJobs 2026-11-09 DECISION: User: the clone's progress must not hold the dialog open; Clone closes it and the clone shows in the header's jobs button (window/jobs_popover_view.rs).
        if self.background_clone {
            let repository = flow
                .repository
                .as_ref()
                .map(|repository| repository.name_with_owner.clone())
                .unwrap_or_else(|| flow.remote_url.clone());
            let destination = self
                .clone_preview
                .as_ref()
                .map(|preview| preview.destination_path.clone())
                .unwrap_or_else(|| self.clone_destination_path.clone());
            (self.host)(
                AddProjectModalCommand::StartBackgroundClone {
                    machine_id,
                    params,
                    repository,
                    destination,
                },
                cx,
            );
            self.close(window, cx);
            return;
        }
        self.set_busy(Some(Busy::Clone), cx);
        // The progress line sits under the options.
        self.sheet_scroll.scroll_to_bottom();
        let generation = self.clone_generation;
        self.request(
            "startClone",
            &machine_id,
            params,
            Pending::StartClone {
                machine_id: machine_id.clone(),
                generation,
            },
            cx,
        );
        self.settle(window, cx);
    }

    pub(super) fn cancel_clone(&mut self, cx: &mut Context<Self>) {
        let Some(job_id) = self.clone_job_id.clone() else {
            return;
        };
        let Some(machine_id) = self.machine_id.clone() else {
            return;
        };
        let generation = self.clone_generation;
        self.request(
            "cancelCloneJob",
            &machine_id,
            serde_json::json!({ "jobId": job_id }),
            Pending::CancelCloneJob {
                machine_id: machine_id.clone(),
                generation,
            },
            cx,
        );
    }

    pub(super) fn set_clone_option(
        &mut self,
        main_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        if main_only {
            self.clone_options.clone_main_only = !self.clone_options.clone_main_only;
        } else {
            self.clone_options.shallow_clone = !self.clone_options.shallow_clone;
        }
        self.clone_preview = None;
        self.clone_generation += 1;
        self.settle(window, cx);
    }

    // -- places ---------------------------------------------------------------------------------

    /// Pins (or unpins) a folder in the sidebar of the machine being browsed.
    pub(super) fn set_pinned(&mut self, path: &str, pinned: bool, cx: &mut Context<Self>) {
        let Some(machine_id) = self.machine_id.clone() else {
            return;
        };
        let seq = {
            let entry = self.places_seq.entry(machine_id.clone()).or_insert(0);
            *entry += 1;
            *entry
        };
        self.request(
            "places",
            &machine_id,
            serde_json::json!({
                "pin": pinned.then_some(path),
                "unpin": (!pinned).then_some(path),
            }),
            Pending::Places {
                machine_id: machine_id.clone(),
                seq,
                is_pin_change: true,
            },
            cx,
        );
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(AddProjectModalCommand::Close, cx);
    }

    pub(super) fn focus_path_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.path_input
            .update(cx, |input, cx| input.focus(window, cx));
    }

    /// Preview hook for the standalone demo binary: one scripted step. `type:<text>` (the path
    /// field), `key:down|up|enter|mod-enter|escape|tab`, `place:<path>` (a sidebar place),
    /// `open:<path>` (a column entry), `machine:<id>`, `suggested`, `back`, `up`, `new-folder`,
    /// `folder-name:<name>`, `new-project`, `project-name:<name>`, `clone`, `repo:<text>`,
    /// `source:<wire>`, `branch:<name>`, `clone-submit`, `create`, `add`, `pin`.
    #[allow(dead_code)] // used by src/bin/native_modal_demo/add_project.rs only
    pub(crate) fn preview_step(&mut self, step: &str, window: &mut Window, cx: &mut Context<Self>) {
        let set = |input: &gpui::Entity<gpui_component::input::InputState>,
                   text: &str,
                   window: &mut Window,
                   cx: &mut Context<Self>| {
            input.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx);
                input.set_selected_range(text.len()..text.len(), cx);
            });
        };
        if let Some(text) = step.strip_prefix("type:") {
            set(&self.path_input.clone(), text, window, cx);
            self.on_path_input_changed(text.to_string(), window, cx);
        } else if let Some(key) = step.strip_prefix("key:") {
            match key {
                "down" => self.move_highlight(1, cx),
                "up" => self.move_highlight(-1, cx),
                "enter" => self.press_enter(false, window, cx),
                "mod-enter" => self.press_enter(true, window, cx),
                "escape" => self.press_escape(window, cx),
                "tab" => self.complete_path(window, cx),
                _ => {}
            }
        } else if let Some(path) = step.strip_prefix("place:") {
            self.go_to_folder(path, Some(path.to_string()));
            self.settle(window, cx);
        } else if let Some(path) = step.strip_prefix("open:") {
            self.open_column_entry(path);
            self.settle(window, cx);
        } else if let Some(machine_id) = step.strip_prefix("machine:") {
            self.choose_machine(machine_id.to_string());
            self.settle(window, cx);
        } else if step == "machine-menu" {
            self.machine_menu_open = true;
            cx.notify();
        } else if let Some(field) = step.strip_prefix("pick:") {
            let field = match field {
                "space" => Some(super::new_project::NewProjectField::Space),
                "group" => Some(super::new_project::NewProjectField::Group),
                "start" => Some(super::new_project::NewProjectField::Start),
                _ => None,
            };
            if let Some(field) = field {
                self.open_new_project_pick(field);
                self.settle(window, cx);
            }
        } else if step == "suggested" {
            self.open_suggested();
            self.settle(window, cx);
        } else if step == "back" {
            self.go_back();
            self.settle(window, cx);
        } else if step == "up" {
            let d = self.derive();
            self.go_up(&d);
            self.settle(window, cx);
        } else if step == "new-folder" {
            self.start_new_folder(window, cx);
        } else if let Some(name) = step.strip_prefix("folder-name:") {
            set(&self.folder_name_input.clone(), name, window, cx);
            self.new_folder_name = Some(name.to_string());
            cx.notify();
        } else if step == "new-project" {
            self.open_new_project_sheet(window, cx);
        } else if let Some(name) = step.strip_prefix("project-name:") {
            set(&self.project_name_input.clone(), name, window, cx);
            if self.set_new_project_name(name.to_string()) {
                self.settle(window, cx);
            }
        } else if step == "clone" {
            self.open_clone_sheet(None, window, cx);
        } else if let Some(text) = step.strip_prefix("repo:") {
            set(&self.repo_input.clone(), text, window, cx);
            self.on_repo_input_changed(text.to_string(), window, cx);
        } else if let Some(source) = step.strip_prefix("source:") {
            if let Some(source) = AddProjectSourceId::from_wire(source) {
                self.choose_clone_source(source);
                self.settle(window, cx);
            }
        } else if let Some(branch) = step.strip_prefix("branch:") {
            set(&self.branch_input.clone(), branch, window, cx);
            self.clone_options.branch_name = branch.to_string();
            cx.notify();
        } else if step == "github-sign-in" {
            self.start_github_sign_in(cx);
        } else if let Some(key) = step.strip_prefix("repo-key:") {
            match key {
                "down" => {
                    self.move_repo_highlight(1, cx);
                }
                "up" => {
                    self.move_repo_highlight(-1, cx);
                }
                "enter" => {
                    if !self.press_repo_enter(window, cx) {
                        self.submit_clone_sheet(window, cx);
                    }
                }
                _ => {}
            }
        } else if step == "clone-submit" {
            self.submit_clone_sheet(window, cx);
        } else if step == "create" {
            self.submit_new_project(window, cx);
        } else if step == "add" {
            self.submit_resolved_path(window, cx);
        } else if step == "pin" {
            if let Some(folder) = self.derive().selected_folder {
                self.set_pinned(&folder, true, cx);
            }
        }
    }

    /// Preview hook: a one-line summary of the dialog's state for the demo's log.
    #[allow(dead_code)] // used by src/bin/native_modal_demo/add_project.rs only
    pub(crate) fn preview_summary(&self) -> String {
        let d = self.derive();
        format!(
            "machine={:?} query={:?} selected={:?} columns={:?} sheet={:?} folder={:?} highlight={:?} error={:?} busy={:?} submit={:?}",
            self.machine_id,
            self.query,
            d.selected_folder,
            self.column_folders(&d),
            self.sheet,
            self.new_folder_name,
            self.highlight,
            self.error,
            self.busy,
            d.submit_action_label,
        )
    }
}

/// True when `child` is `root` or inside it (`~/dev` holds `~/dev/a`, not `~/devtools`).
pub(super) fn folder_contains(root: &str, child: &str) -> bool {
    let root = trim_folder(root);
    let child = trim_folder(child);
    if child == root {
        return true;
    }
    let root = if root.ends_with(['/', '\\']) {
        root
    } else {
        format!("{root}/")
    };
    child
        .replace('\\', "/")
        .starts_with(&root.replace('\\', "/"))
}
