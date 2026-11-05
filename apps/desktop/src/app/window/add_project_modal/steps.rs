//! The Add Project dialog's operations: moving between steps, browsing, naming a new folder,
//! looking up and reviewing a repository, cloning, adding, and closing.
use super::copy::*;
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

    pub(super) fn push_view(&mut self, view: AddProjectView) {
        self.query = match &view {
            AddProjectView::Browse { initial_query, .. } => initial_query.clone(),
            _ => String::new(),
        };
        self.view_stack.push(view);
        self.highlight = None;
        self.browse_result = None;
        self.error = None;
        self.browse_generation += 1;
    }

    pub(super) fn pop_view(&mut self) {
        self.new_folder_name = None;
        self.add_new_folder_as_project = false;
        self.clone_flow = None;
        self.clone_options = CloneOptions::default();
        self.clone_preview = None;
        self.clone_destination_path.clear();
        if self.view_stack.len() > 1 {
            self.view_stack.pop();
        }
        self.highlight = None;
        self.query.clear();
        self.browse_result = None;
        self.error = None;
        self.browse_generation += 1;
    }

    pub(super) fn on_path_input_changed(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_folder_name.is_some() {
            if self.new_folder_name.as_deref() != Some(value.as_str()) {
                self.error = None;
                self.new_folder_name = Some(value);
                cx.notify();
            }
            return;
        }
        if value == self.query {
            return;
        }
        self.highlight = None;
        self.query = value;
        self.error = None;
        let pops = self.query.is_empty()
            && self.view_stack.len() > 1
            && matches!(
                self.current_view(),
                Some(AddProjectView::Browse { initial_query, .. }) if !initial_query.is_empty()
            );
        if pops {
            self.pop_view();
        }
        self.settle(window, cx);
    }

    pub(super) fn browse_to(&mut self, full_path: &str) {
        self.highlight = None;
        self.query = ensure_browse_directory_path(full_path);
        self.browse_generation += 1;
    }

    pub(super) fn browse_up(&mut self, d: &Derived) {
        let parent = if d.is_windows_drive_root {
            Some(ADD_PROJECT_ROOT_BROWSE_PATH.to_string())
        } else {
            get_browse_parent_path(&self.query)
        };
        let Some(parent) = parent else {
            return;
        };
        self.highlight = None;
        self.query = parent;
        self.browse_generation += 1;
    }

    pub(super) fn start_local_browse(&mut self, machine_id: String, start: Option<String>) {
        let target = self
            .machines
            .iter()
            .find(|machine| machine.machine_id == machine_id)
            .cloned();
        self.clone_flow = None;
        self.clone_options = CloneOptions::default();
        self.clone_preview = None;
        self.clone_destination_path.clear();
        let start = start.unwrap_or_else(|| initial_browse_query(target.as_ref()));
        self.push_view(AddProjectView::Browse {
            machine_id,
            initial_query: ensure_browse_directory_path(&start),
        });
    }

    pub(super) fn start_clone_flow(&mut self, machine_id: String, source: AddProjectSourceId) {
        self.clone_options = CloneOptions::default();
        self.clone_preview = None;
        self.clone_destination_path.clear();
        self.clone_flow = Some(CloneFlow {
            remote_url: String::new(),
            repository: None,
            repository_input: String::new(),
            source,
            step: CloneStep::Repository,
        });
        self.push_view(AddProjectView::Clone { machine_id });
    }

    pub(super) fn enter_clone_destination_step(
        &mut self,
        remote_url: String,
        repository: Option<AddProjectRepositoryInfo>,
        repository_input: String,
        source: AddProjectSourceId,
        destination: Option<String>,
    ) {
        self.clone_flow = Some(CloneFlow {
            remote_url,
            repository,
            repository_input,
            source,
            step: CloneStep::Destination,
        });
        self.highlight = None;
        self.browse_result = None;
        let d_machine = self.derive().machine;
        self.query = match destination.filter(|destination| !destination.is_empty()) {
            Some(destination) => self.resolve_input_destination(
                &destination,
                d_machine.as_ref(),
                d_machine
                    .as_ref()
                    .map(|machine| machine.machine_id.as_str()),
            ),
            None => {
                ensure_browse_directory_path(&initial_clone_destination_query(d_machine.as_ref()))
            }
        };
        self.browse_generation += 1;
    }

    pub(super) fn select_row(
        &mut self,
        action: RowAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            RowAction::BrowseInputFolder { machine_id, path } => {
                self.push_view(AddProjectView::Browse {
                    machine_id,
                    initial_query: path,
                });
            }
            RowAction::ChooseDetectedClone { machine_id, clone } => {
                self.start_clone_flow(machine_id, clone.source);
                self.clone_options = CloneOptions::from_detected(&clone);
                self.query = clone.query;
            }
            RowAction::AddSuggested(path) => self.submit_add_project(&path, cx),
            RowAction::BrowseUp => {
                let d = self.derive();
                self.browse_up(&d);
            }
            RowAction::BrowseTo(path) => self.browse_to(&path),
            RowAction::ChooseMachine(machine_id) => {
                self.clone_flow = None;
                self.push_view(AddProjectView::Sources { machine_id });
            }
            RowAction::StartLocalBrowse { machine_id, start } => {
                self.start_local_browse(machine_id, start)
            }
            RowAction::NewProjectFolder { machine_id, parent } => {
                self.start_local_browse(machine_id, Some(parent));
                self.new_folder_name = Some(String::new());
                self.add_new_folder_as_project = true;
            }
            RowAction::StartClone { machine_id, source } => {
                self.start_clone_flow(machine_id, source)
            }
        }
        self.focus_path_input(window, cx);
        self.settle(window, cx);
    }

    // -- steps ----------------------------------------------------------------------------------

    pub(super) fn submit_repository_step(&mut self, cx: &mut Context<Self>) {
        let repository_input = self.query.trim().to_string();
        let Some(flow) = self
            .clone_flow
            .clone()
            .filter(|flow| flow.step == CloneStep::Repository)
        else {
            return;
        };
        let Some(machine_id) = self.derive().machine_id else {
            return;
        };
        if repository_input.is_empty() || self.busy.is_some() {
            return;
        }
        let parsed = match parse_add_project_clone_input(&repository_input, flow.source) {
            Some(ParsedCloneInput::Error(message)) => {
                self.error = Some(message);
                return;
            }
            Some(ParsedCloneInput::Clone(parsed)) => Some(parsed),
            None => None,
        };
        if let Some(parsed) = &parsed {
            self.clone_options = CloneOptions::from_detected(parsed);
            if flow.source == AddProjectSourceId::Url
                || !matches!(
                    parsed.source,
                    AddProjectSourceId::Github | AddProjectSourceId::Gitlab
                )
            {
                self.enter_clone_destination_step(
                    parsed.remote_url.clone(),
                    None,
                    repository_input,
                    parsed.source,
                    Some(parsed.destination.clone()),
                );
                return;
            }
        }
        if parsed.is_none() && flow.source == AddProjectSourceId::Url {
            self.error = Some("Enter a Git repository URL or clone command.".to_string());
            return;
        }
        self.set_busy(Some(Busy::Lookup), cx);
        self.error = None;
        let provider = parsed
            .as_ref()
            .map(|parsed| parsed.source)
            .unwrap_or(flow.source);
        let repository = match &parsed {
            Some(parsed) if parsed.source == AddProjectSourceId::Gitlab => {
                gitlab_project_path(&parsed.remote_url)
            }
            Some(parsed) => parsed.remote_url.clone(),
            None => repository_input.clone(),
        };
        self.request(
            "lookupRepository",
            &machine_id,
            serde_json::json!({ "provider": provider.wire(), "repository": repository }),
            Pending::Lookup {
                repository_input,
                parsed,
                source: flow.source,
            },
            cx,
        );
    }

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

    pub(super) fn register_project(
        &mut self,
        path: String,
        create_if_missing: bool,
        after_clone: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(machine_id) = self.derive().machine_id else {
            return;
        };
        self.request(
            "add",
            &machine_id,
            serde_json::json!({ "createIfMissing": create_if_missing, "path": path }),
            Pending::Add { after_clone },
            cx,
        );
    }

    pub(super) fn submit_add_project(&mut self, raw: &str, cx: &mut Context<Self>) {
        let d = self.derive();
        if self.busy.is_some() || d.machine_id.is_none() {
            return;
        }
        let Some(path) = self.validate_project_path(raw, &d.platform) else {
            return;
        };
        self.set_busy(Some(Busy::Add), cx);
        self.error = None;
        self.register_project(path, d.will_create_project_path, false, cx);
    }

    pub(super) fn open_clone_review(&mut self, raw: &str, cx: &mut Context<Self>) {
        let d = self.derive();
        let Some(flow) = self
            .clone_flow
            .clone()
            .filter(|flow| flow.step == CloneStep::Destination)
        else {
            return;
        };
        let Some(machine_id) = d.machine_id.clone() else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        let Some(destination_path) = self.validate_project_path(raw, &d.platform) else {
            return;
        };
        self.set_busy(Some(Busy::Preview), cx);
        self.error = None;
        let options = self.clone_options.clone();
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
            Pending::Preview { destination_path },
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
        let Some(flow) = self
            .clone_flow
            .clone()
            .filter(|flow| flow.step == CloneStep::Review)
        else {
            return;
        };
        let Some(machine_id) = self.derive().machine_id else {
            return;
        };
        if !self.can_clone() {
            return;
        }
        self.set_busy(Some(Busy::Clone), cx);
        self.error = None;
        let options = self.clone_options.clone();
        self.request(
            "startClone",
            &machine_id,
            serde_json::json!({
                "branchName": options.branch_name,
                "cloneMainOnly": options.clone_main_only,
                "destinationPath": self.clone_destination_path,
                "remoteUrl": flow.remote_url,
                "shallowClone": options.shallow_clone,
            }),
            Pending::StartClone,
            cx,
        );
        self.settle(window, cx);
    }

    pub(super) fn start_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.derive().can_create_new_folder || self.busy.is_some() {
            return;
        }
        self.error = None;
        self.highlight = None;
        self.new_folder_name = Some(String::new());
        self.focus_path_input(window, cx);
        self.settle(window, cx);
    }

    pub(super) fn cancel_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_folder_name = None;
        self.add_new_folder_as_project = false;
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
        if self.busy.is_some() || name.is_empty() || d.new_folder_parent_path.is_empty() {
            return;
        }
        self.set_busy(Some(Busy::CreateFolder), cx);
        self.error = None;
        self.request(
            "createDirectory",
            &machine_id,
            serde_json::json!({ "name": name, "parentPath": d.new_folder_parent_path }),
            Pending::CreateFolder,
            cx,
        );
        self.settle(window, cx);
    }

    pub(super) fn submit_resolved_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.derive();
        if d.is_clone_destination_step {
            self.open_clone_review(&d.resolved_add_project_path, cx);
        } else {
            self.submit_add_project(&d.resolved_add_project_path, cx);
        }
        self.settle(window, cx);
    }

    pub(super) fn submit_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submit_repository_step(cx);
        self.settle(window, cx);
    }

    pub(super) fn return_to_clone_destination(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        let Some(flow) = self
            .clone_flow
            .as_mut()
            .filter(|flow| flow.step == CloneStep::Review)
        else {
            return;
        };
        flow.step = CloneStep::Destination;
        self.clone_preview = None;
        self.clone_destination_path.clear();
        self.error = None;
        self.highlight = None;
        self.browse_generation += 1;
        self.focus_path_input(window, cx);
        self.settle(window, cx);
    }

    pub(super) fn cancel_clone(&mut self, cx: &mut Context<Self>) {
        let Some(job_id) = self.clone_job_id.clone() else {
            return;
        };
        let Some(machine_id) = self.derive().machine_id else {
            return;
        };
        self.request(
            "cancelCloneJob",
            &machine_id,
            serde_json::json!({ "jobId": job_id }),
            Pending::CancelCloneJob,
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
        self.settle(window, cx);
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(AddProjectModalCommand::Close, cx);
    }

    pub(super) fn focus_path_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.path_input
            .update(cx, |input, cx| input.focus(window, cx));
    }

    /// Preview hook for the standalone demo binary: one scripted user step (`type:<text>`,
    /// `key:down|up|enter|mod-enter|backspace|tab`, `click:<row value>`, `new-folder`,
    /// `folder-name:<name>`, `branch:<name>`, `clone`).
    #[allow(dead_code)] // used by src/bin/native_modal_demo/add_project.rs only
    pub(crate) fn preview_step(&mut self, step: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = step
            .strip_prefix("type:")
            .or_else(|| step.strip_prefix("folder-name:"))
        {
            self.path_input.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx);
                input.set_selected_range(text.len()..text.len(), cx);
            });
            self.on_path_input_changed(text.to_string(), window, cx);
        } else if let Some(key) = step.strip_prefix("key:") {
            match key {
                "down" => self.move_highlight(1, cx),
                "up" => self.move_highlight(-1, cx),
                "enter" => self.press_enter(false, window, cx),
                "mod-enter" => self.press_enter(true, window, cx),
                "backspace" => {
                    self.press_backspace(window, cx);
                }
                "tab" => self.cycle_focus(1, window, cx),
                _ => {}
            }
        } else if let Some(value) = step.strip_prefix("click:") {
            let action = self
                .derive()
                .rows
                .into_iter()
                .find(|row| row.value == value && !row.disabled)
                .map(|row| row.action);
            if let Some(action) = action {
                self.select_row(action, window, cx);
            }
        } else if step == "new-folder" {
            self.start_new_folder(window, cx);
        } else if let Some(branch) = step.strip_prefix("branch:") {
            self.branch_input.update(cx, |input, cx| {
                input.set_value(branch.to_string(), window, cx);
            });
            self.clone_options.branch_name = branch.to_string();
            cx.notify();
        } else if step == "clone" {
            self.submit_clone(window, cx);
        }
    }

    /// Preview hook: a one-line summary of the dialog's state for the demo's log.
    #[allow(dead_code)] // used by src/bin/native_modal_demo/add_project.rs only
    pub(crate) fn preview_summary(&self) -> String {
        let d = self.derive();
        format!(
            "view={:?} step={:?} query={:?} folder={:?} highlight={:?} error={:?} busy={:?} slow={} rows={} submit={:?} shortcut={:?}",
            self.current_view(),
            self.clone_step(),
            self.query,
            self.new_folder_name,
            self.highlight,
            self.error,
            self.busy,
            self.is_slow,
            d.rows.len(),
            d.submit_action_label,
            d.add_shortcut_label,
        )
    }
}
