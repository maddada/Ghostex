//! The Add Project picker entity: its state, the effects that follow it (the folder listing and
//! its columns, the sidebar's places, source-control discovery) and the host's answers. What is
//! derived on every render is `derive.rs`, the operations `steps.rs`, the keyboard `keyboard.rs`,
//! and the drawing `render.rs` (window, sidebar, columns, preview) and `sheets.rs` (Clone a repo,
//! New project).
use super::super::native_modal_kit::ModalPalette;
use super::copy::*;
use super::input::*;
use super::model::*;
use super::new_project::*;
use super::paths::*;
use gpui::{
    App, AppContext as _, Bounds, Context, Entity, FocusHandle, Pixels, ScrollHandle, Subscription,
    Window,
};
use gpui_component::input::{InputEvent, InputState};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

/// The picker's fixed frame: sidebar, three columns and the preview side by side.
pub(crate) const ADD_PROJECT_MODAL_WIDTH: f32 = 980.0;
pub(crate) const ADD_PROJECT_MODAL_HEIGHT: f32 = 640.0;

pub(super) const LOCAL_MACHINE_ID: &str = "local";

/// What the dialog asks its host to do.
pub(crate) enum AddProjectModalCommand {
    /// One gxserver round trip on `machine_id` (the `addProjectDialogRequest` operations); the
    /// answer comes back through [`GpuiAddProjectModalWindow::receive_response`].
    Request {
        request_id: u64,
        operation: &'static str,
        machine_id: String,
        params: serde_json::Value,
    },
    /// Clone & Add was pressed: the app runs the clone in the background, behind the header's
    /// jobs button (app/background_jobs/), and the dialog closes right after sending this.
    StartBackgroundClone {
        machine_id: String,
        /// The `startClone` parameters, as the dialog would have sent them.
        params: serde_json::Value,
        /// What is cloned (`owner/name`, else the URL).
        repository: String,
        /// The folder the clone goes into.
        destination: String,
    },
    /// The dialog removed its own window (Escape, or a project was added).
    Close,
}

pub(crate) type AddProjectModalHost = Rc<dyn Fn(AddProjectModalCommand, &mut App)>;

pub(crate) struct AddProjectModalConfig {
    pub(crate) machines: Vec<AddProjectMachineOption>,
    /// The computer the picker opens on (the open message's `machineId`); else this computer.
    pub(crate) initial_machine_id: Option<String>,
    /// Absolute cwd used to resolve `./` and `../`; the app never passes one, like the React host.
    pub(crate) active_project_cwd: Option<String>,
    /// This computer's platform in `navigator.platform` spelling, used before a machine is picked.
    pub(crate) client_platform: String,
    pub(crate) palette: ModalPalette,
    pub(crate) clone_job_poll_interval: Duration,
    pub(crate) slow_operation_notice: Duration,
    /// Clone & Add hands the clone to the app and closes ([`AddProjectModalCommand::StartBackgroundClone`]);
    /// off, the dialog stays open and follows the clone itself (a handoff waits on the new project).
    pub(crate) background_clone: bool,
    /// Opens with Clone a repo filled in with this repository, on `initial_machine_id`.
    ///
    /// CDXC:TranscriptExport 2026-10-25 DECISION:
    /// User (Q544): "show the already existing clone/open project flow that we have for remote machine and local machine but start with the url of the repo to clone already pre selected so the user can pick the folder to clone into or create a new folder." Handoff to Remote opens Add Project this way when the chosen computer has no project with the session's repository.
    pub(crate) initial_clone_url: Option<String>,
    /// What New project's form starts from (new_project.rs).
    pub(crate) new_project: AddProjectNewProjectConfig,
}

impl AddProjectModalConfig {
    pub(crate) const CLONE_JOB_POLL_INTERVAL: Duration = Duration::from_millis(900);
    pub(crate) const SLOW_OPERATION_NOTICE: Duration = Duration::from_millis(8000);
}

/// The panel over the preview: Clone a repo, New project, or one of the form's lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sheet {
    Clone,
    NewProject,
    NewProjectPick(NewProjectField),
}

/// The repository Clone a repo resolved (after a provider lookup when one was needed).
#[derive(Clone, Debug)]
pub(super) struct CloneFlow {
    pub(super) remote_url: String,
    pub(super) repository: Option<AddProjectRepositoryInfo>,
    /// The folder name the clone gets inside the open folder (or a path the command named).
    pub(super) destination: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CloneOptions {
    pub(super) branch_name: String,
    pub(super) clone_main_only: bool,
    pub(super) shallow_clone: bool,
}

impl CloneOptions {
    pub(super) fn from_detected(input: &DetectedCloneInput) -> Self {
        Self {
            branch_name: input.branch_name.clone(),
            clone_main_only: input.clone_main_only,
            shallow_clone: input.shallow_clone,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Busy {
    Add,
    Clone,
    CreateFolder,
    Lookup,
    Preview,
}

pub(super) const ICON_ALERT: &str = "modals/add-project/alert-triangle.svg";
pub(super) const ICON_ARROW_LEFT: &str = "modals/add-project/arrow-left.svg";
pub(super) const ICON_ARROW_RIGHT: &str = "titlebar/arrow-right.svg";
pub(super) const ICON_ARROW_UP: &str = "titlebar/arrow-up.svg";
pub(super) const ICON_CHECK: &str = "modals/add-project/check.svg";
pub(super) const ICON_CHEVRON_DOWN: &str = "titlebar/chevron-down.svg";
pub(super) const ICON_CHEVRON_RIGHT: &str = "titlebar/chevron-right.svg";
pub(super) const ICON_DEVICE_DESKTOP: &str = "modals/add-project/device-desktop.svg";
pub(super) const ICON_DISK: &str = "modals/add-project/folder-root.svg";
pub(super) const ICON_FOLDER: &str = "modals/add-project/folder.svg";
pub(super) const ICON_FOLDER_CHECK: &str = "modals/add-project/folder-check.svg";
pub(super) const ICON_FOLDER_PLUS: &str = "modals/add-project/folder-plus.svg";
pub(super) const ICON_GIT_BRANCH: &str = "modals/add-project/git-branch.svg";
pub(super) const ICON_HISTORY: &str = "titlebar/history.svg";
pub(super) const ICON_HOME: &str = "titlebar/home.svg";
pub(super) const ICON_PIN: &str = "titlebar/pin-outline.svg";
pub(super) const ICON_PLUS: &str = "titlebar/plus.svg";
pub(super) const ICON_SERVER: &str = "modals/add-project/server.svg";
pub(super) const ICON_SPARKLES: &str = "titlebar/sparkles.svg";
pub(super) const ICON_TERMINAL: &str = "titlebar/terminal-2.svg";
pub(super) const ICON_X: &str = "titlebar/x.svg";

pub(super) fn source_icon(source: AddProjectSourceId) -> &'static str {
    match source {
        AddProjectSourceId::AzureDevops => "modals/add-project/brand-azure.svg",
        AddProjectSourceId::Bitbucket => "modals/add-project/brand-bitbucket.svg",
        AddProjectSourceId::Github => "modals/add-project/brand-github.svg",
        AddProjectSourceId::Gitlab => "modals/add-project/brand-gitlab.svg",
        AddProjectSourceId::Url => "modals/add-project/link.svg",
    }
}

/// What is derived from the state on every render (`derive.rs`).
pub(super) struct Derived {
    pub(super) machine: Option<AddProjectMachineOption>,
    pub(super) machine_id: Option<String>,
    pub(super) platform: String,
    /// The typed line is a folder path (not a repository or a command).
    pub(super) is_browsing: bool,
    /// The folder whose contents the last column lists, as typed (`~/dev/`).
    pub(super) browse_directory_path: String,
    /// That folder as the machine resolved it (`/Users/me/dev`), once it answered.
    pub(super) selected_folder: Option<String>,
    pub(super) is_drive_list: bool,
    pub(super) unsupported_windows_path: bool,
    pub(super) relative_path_needs_active_project: bool,
    /// A repository URL or clone command typed in the path field.
    pub(super) clone_detected: Option<DetectedCloneInput>,
    /// `owner/repo`, which is also a relative folder.
    pub(super) ambiguous: Option<(String, DetectedCloneInput)>,
    pub(super) detected_error: Option<String>,
    pub(super) filtered: FilteredBrowseEntries,
    pub(super) path_inspection: Option<AddProjectPathInspection>,
    pub(super) suggested_project_path: Option<String>,
    pub(super) resolved_add_project_path: String,
    pub(super) can_submit_browse_path: bool,
    pub(super) will_create_project_path: bool,
    pub(super) new_folder_parent_path: String,
    pub(super) can_create_new_folder: bool,
    pub(super) submit_action_label: &'static str,
    pub(super) readiness: AddProjectReadiness,
}

/// A request in flight and what its answer is for.
pub(super) enum Pending {
    Browse {
        seq: u64,
    },
    /// An ancestor folder's listing for the columns.
    Column {
        key: String,
    },
    Places {
        machine_id: String,
        seq: u64,
        is_pin_change: bool,
    },
    Discovery {
        machine_id: String,
        generation: u64,
    },
    Lookup {
        machine_id: String,
        generation: u64,
        parsed: Option<DetectedCloneInput>,
    },
    Preview {
        machine_id: String,
        generation: u64,
        destination_path: String,
    },
    StartClone {
        machine_id: String,
        generation: u64,
    },
    ReadCloneJob {
        machine_id: String,
        generation: u64,
        job_id: String,
    },
    CancelCloneJob {
        machine_id: String,
        generation: u64,
    },
    Add {
        machine_id: String,
        after_clone: bool,
        generation: u64,
    },
    CreateFolder {
        machine_id: String,
        parent: String,
        generation: u64,
    },
    /// `installSourceControlTool` / `readSourceControlTool` (tool_install.rs).
    ToolInstall {
        machine_id: String,
    },
    /// What is already at the folder the New project form would make (new_project.rs).
    NewProjectInspect {
        key: String,
    },
}

/// A spot the picker has been, for Back and Forward.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HistoryEntry {
    pub(super) machine_id: String,
    pub(super) query: String,
    pub(super) column_root: Option<String>,
}

pub(crate) struct GpuiAddProjectModalWindow {
    pub(super) host: AddProjectModalHost,
    pub(super) palette: ModalPalette,
    pub(super) machines: Vec<AddProjectMachineOption>,
    pub(super) active_project_cwd: Option<String>,
    pub(super) client_platform: String,
    /// The computer being browsed.
    pub(super) machine_id: Option<String>,
    pub(super) sheet: Option<Sheet>,
    /// The sidebar's Suggested is open: the folder area shows the suggested repositories.
    pub(super) suggested_mode: bool,
    pub(super) machine_menu_open: bool,
    pub(super) history_back: Vec<HistoryEntry>,
    pub(super) history_forward: Vec<HistoryEntry>,
    /// The folder the columns start from (a sidebar place); `None` shows two levels up.
    pub(super) column_root: Option<String>,
    /// The columns start at the folder the picker opens on, once the machine resolved it.
    pub(super) root_at_start: bool,
    /// Folder listings by `machine\nfolder`, for the columns.
    pub(super) listings: HashMap<String, AddProjectBrowseResult>,
    pending_columns: HashSet<String>,
    /// Columns whose listing failed transiently; cleared when the user navigates.
    pub(super) failed_columns: HashSet<String>,
    /// The sidebar's places by machine (`None`: the machine could not say, an older gxserver).
    pub(super) places: HashMap<String, Option<AddProjectPlaces>>,
    pending_places: HashSet<String>,
    pub(super) places_seq: HashMap<String, u64>,
    /// The last folder the machine resolved, kept while the next listing loads.
    pub(super) last_selected_folder: Option<String>,
    pub(super) clone_source: AddProjectSourceId,
    pub(super) clone_repo_text: String,
    pub(super) clone_flow: Option<CloneFlow>,
    pub(super) clone_options: CloneOptions,
    pub(super) clone_preview: Option<AddProjectClonePreview>,
    pub(super) clone_destination_path: String,
    pub(super) query: String,
    /// The highlighted entry of the last column (its full path), moved with the arrow keys.
    pub(super) highlight: Option<String>,
    pub(super) browse_generation: u64,
    pub(super) browse_result: Option<AddProjectBrowseResult>,
    pub(super) is_browse_pending: bool,
    pub(super) error: Option<String>,
    pub(super) busy: Option<Busy>,
    pub(super) is_slow: bool,
    pub(super) discovery: HashMap<String, Option<AddProjectSourceControlDiscovery>>,
    pub(super) pending_discovery_machine_id: Option<String>,
    /// The provider CLI being installed from Clone a repo (tool_install.rs).
    pub(super) tool_install: Option<super::tool_install::ToolInstall>,
    /// The new folder being named in the last column; `None` when none is.
    pub(super) new_folder_name: Option<String>,
    /// The folder just made, marked in its column for a moment.
    pub(super) created_folder: Option<String>,
    /// What New project's form starts from, and the form while it is open.
    pub(super) new_project_config: AddProjectNewProjectConfig,
    pub(super) new_project: Option<NewProjectForm>,
    /// The folder on screen when New project opened, and whether the form now follows the picker.
    pub(super) new_project_opened_folder: Option<String>,
    pub(super) new_project_follows_browse: bool,
    /// The name and folder last asked about, so each is asked once.
    pub(super) new_project_inspect_key: Option<String>,
    pub(super) clone_job_id: Option<String>,
    /// The running clone's latest git progress line (`Receiving objects: 45% …`).
    pub(super) clone_progress: Option<String>,
    pub(super) path_input: Entity<InputState>,
    pub(super) branch_input: Entity<InputState>,
    pub(super) folder_name_input: Entity<InputState>,
    pub(super) repo_input: Entity<InputState>,
    pub(super) project_name_input: Entity<InputState>,
    pub(super) sidebar_scroll: ScrollHandle,
    pub(super) open_column_scroll: ScrollHandle,
    pub(super) sheet_scroll: ScrollHandle,
    pub(super) machine_button_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) focus_handle: FocusHandle,
    pub(super) clone_job_poll_interval: Duration,
    pub(super) background_clone: bool,
    slow_operation_notice: Duration,
    next_request_id: u64,
    pending: HashMap<u64, Pending>,
    browse_seq: u64,
    browse_key: Option<String>,
    discovery_key: Option<String>,
    discovery_generation: u64,
    busy_generation: u64,
    pub(super) clone_generation: u64,
    pub(super) machine_generation: u64,
    branch_input_disabled: bool,
    repo_placeholder: String,
    _subscriptions: Vec<Subscription>,
}

/// The listing cache's key for `folder` on `machine_id`.
pub(super) fn listing_key(machine_id: &str, folder: &str) -> String {
    format!("{machine_id}\n{}", trim_folder(folder))
}

/// A folder path without its trailing separator (a root keeps it).
pub(super) fn trim_folder(folder: &str) -> String {
    let trimmed = folder.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return folder.chars().take(1).collect();
    }
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        return format!("{trimmed}\\");
    }
    trimmed.to_string()
}

fn text_input(
    window: &mut Window,
    cx: &mut Context<GpuiAddProjectModalWindow>,
    placeholder: &str,
    muted: gpui::Hsla,
) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    cx.new(|cx| {
        let mut input = InputState::new(window, cx).placeholder(placeholder);
        input.set_placeholder_color(Some(muted));
        input
    })
}

impl GpuiAddProjectModalWindow {
    pub(crate) fn new(
        config: AddProjectModalConfig,
        host: AddProjectModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let muted = gpui::Hsla::from(config.palette.muted);
        let path_input = text_input(
            window,
            cx,
            "Type a folder path, or paste a repository to clone",
            muted,
        );
        let branch_input = text_input(window, cx, "Default branch", muted);
        let folder_name_input = text_input(window, cx, "Folder name", muted);
        let repo_input = text_input(window, cx, "Repository, URL, or clone command", muted);
        let project_name_input = text_input(window, cx, "Project name", muted);
        let on_change =
            |input: &Entity<InputState>,
             window: &mut Window,
             cx: &mut Context<Self>,
             apply: fn(&mut Self, String, &mut Window, &mut Context<Self>)| {
                cx.subscribe_in(
                    input,
                    window,
                    move |this: &mut Self, input, event: &InputEvent, window, cx| {
                        if matches!(event, InputEvent::Change) {
                            let value = input.read(cx).value().to_string();
                            apply(this, value, window, cx);
                        }
                    },
                )
            };
        let subscriptions = vec![
            on_change(&path_input, window, cx, Self::on_path_input_changed),
            on_change(&branch_input, window, cx, |this, value, _window, cx| {
                this.clone_options.branch_name = value;
                this.clone_preview = None;
                this.clone_generation += 1;
                this.error = None;
                cx.notify();
            }),
            on_change(
                &folder_name_input,
                window,
                cx,
                |this, value, _window, cx| {
                    if this.new_folder_name.is_some() {
                        this.new_folder_name = Some(value);
                        this.error = None;
                        cx.notify();
                    }
                },
            ),
            on_change(&repo_input, window, cx, Self::on_repo_input_changed),
            on_change(
                &project_name_input,
                window,
                cx,
                |this, value, window, cx| {
                    if this.set_new_project_name(value) {
                        this.settle(window, cx);
                    }
                },
            ),
        ];
        let machine_id = config
            .initial_machine_id
            .as_deref()
            .and_then(|id| {
                config
                    .machines
                    .iter()
                    .find(|machine| machine.machine_id == id)
            })
            .or_else(|| {
                config
                    .machines
                    .iter()
                    .find(|machine| machine.machine_id == LOCAL_MACHINE_ID)
            })
            .or(config.machines.first())
            .map(|machine| machine.machine_id.clone());
        let error = config
            .machines
            .is_empty()
            .then(|| "No machine is available.".to_string());
        let mut this = Self {
            host,
            palette: config.palette,
            machines: config.machines,
            active_project_cwd: config.active_project_cwd,
            client_platform: config.client_platform,
            machine_id,
            sheet: None,
            suggested_mode: false,
            machine_menu_open: false,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            column_root: None,
            root_at_start: true,
            listings: HashMap::new(),
            pending_columns: HashSet::new(),
            failed_columns: HashSet::new(),
            places: HashMap::new(),
            pending_places: HashSet::new(),
            places_seq: HashMap::new(),
            last_selected_folder: None,
            clone_source: AddProjectSourceId::Url,
            clone_repo_text: String::new(),
            clone_flow: None,
            clone_options: CloneOptions::default(),
            clone_preview: None,
            clone_destination_path: String::new(),
            query: String::new(),
            highlight: None,
            browse_generation: 0,
            browse_result: None,
            is_browse_pending: false,
            error,
            busy: None,
            is_slow: false,
            discovery: HashMap::new(),
            pending_discovery_machine_id: None,
            tool_install: None,
            new_folder_name: None,
            created_folder: None,
            new_project_config: config.new_project,
            new_project: None,
            new_project_opened_folder: None,
            new_project_follows_browse: false,
            new_project_inspect_key: None,
            clone_job_id: None,
            clone_progress: None,
            path_input,
            branch_input,
            folder_name_input,
            repo_input,
            project_name_input,
            sidebar_scroll: ScrollHandle::new(),
            open_column_scroll: ScrollHandle::new(),
            sheet_scroll: ScrollHandle::new(),
            machine_button_bounds: Rc::new(Cell::new(None)),
            focus_handle: cx.focus_handle(),
            clone_job_poll_interval: config.clone_job_poll_interval,
            background_clone: config.background_clone,
            slow_operation_notice: config.slow_operation_notice,
            next_request_id: 0,
            pending: HashMap::new(),
            browse_seq: 0,
            browse_key: None,
            discovery_key: None,
            discovery_generation: 0,
            busy_generation: 0,
            clone_generation: 0,
            machine_generation: 0,
            branch_input_disabled: false,
            repo_placeholder: String::new(),
            _subscriptions: subscriptions,
        };
        this.query = this.start_query();
        let clone_url = config
            .initial_clone_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty());
        match clone_url {
            Some(url) => {
                // The handoff's destination: the clone's folder is picked on the left.
                let machine = this.derive().machine;
                this.query = ensure_browse_directory_path(&initial_clone_destination_query(
                    machine.as_ref(),
                ));
                this.open_clone_sheet(Some(url.to_string()), window, cx);
            }
            None => this.focus_path_input(window, cx),
        }
        this.settle(window, cx);
        this
    }

    /// Where the picker opens on the current machine: its Default projects folder, else home.
    ///
    /// CDXC:AddProject 2026-09-28 DECISION: User: on a native Windows (PowerShell) machine, browsing starts at the list of drives (home first) instead of `~/`; a Default projects folder still wins (CDXC:AddProject 2026-10-09 in copy.rs).
    pub(super) fn start_query(&self) -> String {
        let machine = self.machine_id.as_deref().and_then(|id| {
            self.machines
                .iter()
                .find(|machine| machine.machine_id == id)
        });
        ensure_browse_directory_path(&initial_clone_destination_query(machine))
    }

    // -- effects --------------------------------------------------------------------------------

    /// Runs the effects whose inputs changed, then brings the path field in line with the state.
    /// Every entry point ends here.
    pub(super) fn settle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for _ in 0..4 {
            if !self.run_effects(cx) {
                break;
            }
        }
        self.sync_path_input(window, cx);
        let busy = self.busy.is_some();
        if busy != self.branch_input_disabled {
            self.branch_input_disabled = busy;
            self.branch_input
                .update(cx, |input, cx| input.set_disabled(busy, cx));
        }
        cx.notify();
    }

    /// One pass of the effects; true when a pasted machine path moved the picker, which re-runs
    /// them against the new state.
    fn run_effects(&mut self, cx: &mut Context<Self>) -> bool {
        let d = self.derive();
        let Some(machine_id) = d.machine_id.clone() else {
            return false;
        };

        // The sidebar's places, once per machine.
        if !self.places.contains_key(&machine_id) && !self.pending_places.contains(&machine_id) {
            self.pending_places.insert(machine_id.clone());
            let seq = {
                let entry = self.places_seq.entry(machine_id.clone()).or_insert(0);
                *entry += 1;
                *entry
            };
            self.request(
                "places",
                &machine_id,
                serde_json::json!({}),
                Pending::Places {
                    machine_id: machine_id.clone(),
                    seq,
                    is_pin_change: false,
                },
                cx,
            );
        }

        // Source-control readiness, once per machine, when Clone a repo opens.
        let cloning = self.sheet == Some(Sheet::Clone);
        let discovery_key = format!(
            "{cloning}|{machine_id}|{}",
            self.discovery.contains_key(&machine_id)
        );
        if self.discovery_key.as_deref() != Some(discovery_key.as_str()) {
            self.discovery_key = Some(discovery_key);
            self.discovery_generation += 1;
            if cloning && !self.discovery.contains_key(&machine_id) {
                self.pending_discovery_machine_id = Some(machine_id.clone());
                let generation = self.discovery_generation;
                self.request(
                    "discoverSourceControl",
                    &machine_id,
                    serde_json::json!({}),
                    Pending::Discovery {
                        machine_id: machine_id.clone(),
                        generation,
                    },
                    cx,
                );
            } else {
                self.pending_discovery_machine_id = None;
            }
        }

        // The open folder's listing, with what each folder is, and the typed path inspected.
        let browse_key = format!(
            "{machine_id}|{}|{}|{}|{}|{}|{}|{:?}",
            d.is_browsing,
            d.browse_directory_path,
            d.unsupported_windows_path,
            d.relative_path_needs_active_project,
            self.query,
            self.browse_generation,
            self.active_project_cwd,
        );
        if self.browse_key.as_deref() != Some(browse_key.as_str()) {
            self.browse_key = Some(browse_key);
            self.browse_seq += 1;
            if d.is_browsing
                && !d.browse_directory_path.is_empty()
                && !d.unsupported_windows_path
                && !d.relative_path_needs_active_project
            {
                let seq = self.browse_seq;
                self.is_browse_pending = true;
                // The listing stays on screen while the next one loads, unless it is another folder.
                if self
                    .browse_result
                    .as_ref()
                    .is_some_and(|result| !self.is_listing_of(result, &d.browse_directory_path))
                {
                    self.browse_result = None;
                }
                let mut params = serde_json::Map::new();
                params.insert(
                    "partialPath".to_string(),
                    serde_json::json!(d.browse_directory_path),
                );
                params.insert("inspectPath".to_string(), serde_json::json!(self.query));
                params.insert("details".to_string(), serde_json::json!(true));
                if let Some(cwd) = &self.active_project_cwd {
                    params.insert("cwd".to_string(), serde_json::json!(cwd));
                }
                self.request(
                    "browse",
                    &machine_id,
                    serde_json::Value::Object(params),
                    Pending::Browse { seq },
                    cx,
                );
            } else {
                self.is_browse_pending = false;
                self.browse_result = None;
            }
        }

        // The columns' ancestor folders.
        for folder in self.column_folders(&d) {
            let key = listing_key(&machine_id, &folder);
            if self.listings.contains_key(&key)
                || self.pending_columns.contains(&key)
                || self.failed_columns.contains(&key)
            {
                continue;
            }
            self.pending_columns.insert(key.clone());
            self.request(
                "browse",
                &machine_id,
                serde_json::json!({
                    "partialPath": ensure_browse_directory_path(&folder),
                    "details": true,
                }),
                Pending::Column { key },
                cx,
            );
        }

        // New project's folder follows the folder on screen once the user opens another one.
        if matches!(
            self.sheet,
            Some(Sheet::NewProject | Sheet::NewProjectPick(_))
        ) && let (Some(selected), false) = (d.selected_folder.clone(), d.is_drive_list)
        {
            if !self.new_project_follows_browse
                && self.new_project_opened_folder.as_deref() != Some(selected.as_str())
            {
                self.new_project_follows_browse = true;
            }
            if self.new_project_follows_browse
                && let Some(form) = self.new_project.as_mut()
                && form.parent != selected
            {
                form.parent = selected;
                form.existing = None;
            }
        }
        self.run_new_project_inspection(cx);

        // A pasted `machine:path` opens that machine; a pasted `cd …`, `file://` or escaped path
        // becomes the plain path.
        if !d.is_browsing {
            match classify_add_project_input(&self.query, &self.machines) {
                Some(DetectedProjectInput::Browse {
                    query,
                    machine_id: target,
                }) => {
                    if let Some(target) = target.filter(|target| target != &machine_id) {
                        if self.busy.is_some() {
                            return false;
                        }
                        self.machine_id = Some(target);
                        self.reset_for_machine();
                        self.column_root = None;
                    }
                    if query != self.query {
                        self.query = query;
                        self.highlight = None;
                        self.failed_columns.clear();
                        self.clone_generation += 1;
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// True when `result` lists the folder `directory` names (`~/dev/` against `/Users/me/dev`).
    fn is_listing_of(&self, result: &AddProjectBrowseResult, directory: &str) -> bool {
        let home = self.current_home();
        let expanded = expand_home(directory, home.as_deref());
        trim_folder(&expanded) == trim_folder(&result.parent_path)
    }

    /// The path field shows the query; it is only rewritten when the state moved it.
    fn sync_path_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The repository field's hint names what the chosen source takes.
        let placeholder = repository_placeholder(self.clone_source);
        if placeholder != self.repo_placeholder {
            self.repo_placeholder = placeholder.clone();
            self.repo_input.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx)
            });
        }
        let value = self.query.clone();
        self.path_input.update(cx, |input, cx| {
            if input.value().as_ref() != value.as_str() {
                input.set_value(value.clone(), window, cx);
                let end = value.len();
                input.set_selected_range(end..end, cx);
            }
        });
    }

    pub(super) fn request(
        &mut self,
        operation: &'static str,
        machine_id: &str,
        params: serde_json::Value,
        pending: Pending,
        cx: &mut Context<Self>,
    ) {
        self.next_request_id += 1;
        let request_id = self.next_request_id;
        self.pending.insert(request_id, pending);
        (self.host)(
            AddProjectModalCommand::Request {
                request_id,
                operation,
                machine_id: machine_id.to_string(),
                params,
            },
            cx,
        );
    }

    /// The slow-operation notice: a pending call that runs past the threshold says so instead of
    /// silently dying; the host owns the hard timeout.
    pub(super) fn set_busy(&mut self, busy: Option<Busy>, cx: &mut Context<Self>) {
        if self.busy == busy {
            return;
        }
        self.busy = busy;
        self.busy_generation += 1;
        if busy.is_none() {
            self.is_slow = false;
            return;
        }
        let generation = self.busy_generation;
        let delay = self.slow_operation_notice;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if this.busy_generation == generation && this.busy.is_some() {
                    this.is_slow = true;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // -- answers ----------------------------------------------------------------------------------

    /// The host's answer to a [`AddProjectModalCommand::Request`]. `Err` carries the daemon's own
    /// refusal text (or the host's transport error).
    pub(crate) fn receive_response(
        &mut self,
        request_id: u64,
        result: Result<serde_json::Value, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.pending.remove(&request_id) else {
            return;
        };
        match pending {
            Pending::Browse { seq } => {
                if seq != self.browse_seq {
                    return;
                }
                self.is_browse_pending = false;
                match result.and_then(|value| read_add_project_browse_result(&value)) {
                    Ok(browse) => {
                        if let Some(machine_id) = self.machine_id.clone() {
                            let mut listing = browse.clone();
                            listing.inspection = None;
                            self.listings
                                .insert(listing_key(&machine_id, &browse.parent_path), listing);
                        }
                        if !browse.is_drive_list || browse.parent_path != "/" {
                            self.last_selected_folder = Some(trim_folder(&browse.parent_path));
                        }
                        if self.root_at_start {
                            self.root_at_start = false;
                            self.column_root = Some(trim_folder(&browse.parent_path));
                        }
                        self.browse_result = Some(browse);
                    }
                    Err(error) => {
                        self.browse_result = None;
                        self.error = Some(describe_add_project_error(
                            &error,
                            "Unable to browse that directory.",
                        ));
                    }
                }
            }
            Pending::Column { key } => {
                self.pending_columns.remove(&key);
                match result.and_then(|value| read_add_project_browse_result(&value)) {
                    Ok(listing) => {
                        self.failed_columns.remove(&key);
                        self.listings.insert(key, listing);
                    }
                    Err(_) => {
                        self.failed_columns.insert(key);
                    }
                }
            }
            Pending::Places {
                machine_id,
                seq,
                is_pin_change,
            } => {
                if self.places_seq.get(&machine_id).copied() != Some(seq) {
                    return;
                }
                self.pending_places.remove(&machine_id);
                match result.and_then(|value| read_add_project_places(&value)) {
                    Ok(places) => {
                        self.places.insert(machine_id, Some(places));
                    }
                    Err(error) => {
                        if is_pin_change {
                            self.error = Some(describe_add_project_error(
                                &error,
                                "Unable to update pinned folders.",
                            ));
                        } else if !self.places.contains_key(&machine_id) {
                            self.places.insert(machine_id, None);
                        }
                    }
                }
            }
            Pending::Discovery {
                machine_id,
                generation,
            } => {
                if generation != self.discovery_generation {
                    return;
                }
                let discovery = result
                    .ok()
                    .and_then(|value| read_add_project_discovery(&value).ok());
                self.discovery.insert(machine_id, discovery);
                self.pending_discovery_machine_id = None;
            }
            Pending::Lookup {
                machine_id,
                generation,
                parsed,
            } => {
                if self.sheet != Some(Sheet::Clone)
                    || self.machine_id.as_deref() != Some(machine_id.as_str())
                    || generation != self.clone_generation
                {
                    return;
                }
                self.set_busy(None, cx);
                match result.and_then(|value| read_add_project_repository(&value)) {
                    Ok(repository) => {
                        let remote_url = parsed
                            .as_ref()
                            .map(|parsed| parsed.remote_url.clone())
                            .unwrap_or_else(|| repository.url.clone());
                        let destination = parsed
                            .as_ref()
                            .map(|parsed| parsed.destination.clone())
                            .filter(|destination| !destination.trim().is_empty())
                            .unwrap_or_else(|| repository_folder_name(&repository.name_with_owner));
                        self.clone_flow = Some(CloneFlow {
                            remote_url,
                            repository: Some(repository),
                            destination,
                        });
                        self.preview_clone(cx);
                    }
                    Err(error) => {
                        self.error = Some(describe_add_project_error(
                            &error,
                            "Repository lookup failed.",
                        ));
                    }
                }
            }
            Pending::Preview {
                machine_id,
                generation,
                destination_path,
            } => {
                if self.sheet != Some(Sheet::Clone)
                    || self.machine_id.as_deref() != Some(machine_id.as_str())
                    || generation != self.clone_generation
                {
                    return;
                }
                self.set_busy(None, cx);
                match result.and_then(|value| read_add_project_clone_preview(&value)) {
                    Ok(preview) => {
                        let blocked = preview.destination_blocked;
                        self.clone_destination_path = destination_path;
                        self.clone_preview = Some(preview);
                        // A free destination clones right away; a refused one says why.
                        if !blocked && self.can_clone() {
                            self.submit_clone(window, cx);
                            return;
                        }
                    }
                    Err(error) => {
                        self.error = Some(describe_add_project_error(
                            &error,
                            "Unable to review the clone destination.",
                        ));
                    }
                }
            }
            Pending::StartClone {
                machine_id,
                generation,
            } => {
                if self.machine_id.as_deref() != Some(machine_id.as_str())
                    || generation != self.clone_generation
                {
                    return;
                }
                match result.and_then(|value| read_add_project_clone_handle(&value)) {
                    Ok(job_id) => {
                        self.clone_job_id = Some(job_id.clone());
                        self.read_clone_job(machine_id, generation, job_id, cx);
                    }
                    Err(error) => self.finish_clone(
                        Some(describe_add_project_error(&error, "Clone failed.")),
                        cx,
                    ),
                }
            }
            Pending::ReadCloneJob {
                machine_id,
                generation,
                job_id,
            } => {
                if self.machine_id.as_deref() != Some(machine_id.as_str())
                    || generation != self.clone_generation
                {
                    return;
                }
                match result.and_then(|value| read_add_project_clone_job(&value)) {
                    Ok(job) => self.receive_clone_job(machine_id, generation, job_id, job, cx),
                    Err(error) => self.finish_clone(
                        Some(describe_add_project_error(&error, "Clone failed.")),
                        cx,
                    ),
                }
            }
            Pending::CancelCloneJob {
                machine_id,
                generation,
            } => {
                if self.machine_id.as_deref() != Some(machine_id.as_str())
                    || generation != self.clone_generation
                {
                    return;
                }
                if let Err(error) = result {
                    self.error = Some(describe_add_project_error(
                        &error,
                        "Unable to cancel the clone.",
                    ));
                }
            }
            Pending::Add {
                machine_id,
                after_clone,
                generation,
            } => {
                let expected = if after_clone {
                    self.clone_generation
                } else {
                    self.machine_generation
                };
                if self.machine_id.as_deref() != Some(machine_id.as_str()) || generation != expected
                {
                    return;
                }
                match result.and_then(|value| read_add_project_add_result(&value)) {
                    Ok(()) => {
                        // The app activates the new project.
                        self.close(window, cx);
                        return;
                    }
                    Err(error) => {
                        let fallback = if after_clone {
                            "Clone failed."
                        } else {
                            "Failed to add project."
                        };
                        self.error = Some(describe_add_project_error(&error, fallback));
                        if after_clone {
                            self.clone_job_id = None;
                        }
                        self.set_busy(None, cx);
                    }
                }
            }
            Pending::ToolInstall { machine_id } => {
                self.receive_tool_install(machine_id, result, cx);
            }
            Pending::NewProjectInspect { key } => {
                self.receive_new_project_inspection(key, result);
            }
            Pending::CreateFolder {
                machine_id,
                parent,
                generation,
            } => {
                if result.is_ok() {
                    self.listings.remove(&listing_key(&machine_id, &parent));
                }
                if self.machine_id.as_deref() != Some(machine_id.as_str())
                    || generation != self.machine_generation
                {
                    return;
                }
                self.set_busy(None, cx);
                match result.and_then(|value| read_add_project_created_directory(&value)) {
                    Ok(path) => {
                        /*
                        CDXC:AddProject 2026-11-21:
                        The new folder shows in its column and becomes the selected folder, so
                        Add project, New project and Clone a repo all act on it next. Its parent's
                        listing is read again so the column has it.
                        */
                        self.new_folder_name = None;
                        self.created_folder = Some(trim_folder(&path));
                        self.highlight = None;
                        self.query = ensure_browse_directory_path(&self.tildify(&path));
                        self.browse_result = None;
                        self.browse_generation += 1;
                        self.focus_path_input(window, cx);
                    }
                    Err(error) => {
                        self.error = Some(describe_add_project_error(
                            &error,
                            "Failed to create the folder.",
                        ));
                    }
                }
            }
        }
        self.settle(window, cx);
    }

    fn read_clone_job(
        &mut self,
        machine_id: String,
        generation: u64,
        job_id: String,
        cx: &mut Context<Self>,
    ) {
        self.request(
            "readCloneJob",
            &machine_id,
            serde_json::json!({ "jobId": job_id }),
            Pending::ReadCloneJob {
                machine_id: machine_id.clone(),
                generation,
                job_id,
            },
            cx,
        );
    }

    fn receive_clone_job(
        &mut self,
        machine_id: String,
        generation: u64,
        job_id: String,
        job: AddProjectCloneJob,
        cx: &mut Context<Self>,
    ) {
        match job.state {
            AddProjectCloneJobState::Running => {
                if job.progress.is_some() {
                    self.sheet_scroll.scroll_to_bottom();
                    self.clone_progress = job.progress;
                    cx.notify();
                }
                let delay = self.clone_job_poll_interval;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.clone_job_id.as_deref() == Some(job_id.as_str())
                            && this.machine_id.as_deref() == Some(machine_id.as_str())
                            && this.clone_generation == generation
                        {
                            this.read_clone_job(machine_id, generation, job_id, cx);
                        }
                    });
                })
                .detach();
            }
            AddProjectCloneJobState::Canceled => {
                self.finish_clone(Some("Clone canceled.".to_string()), cx);
            }
            AddProjectCloneJobState::Failed => {
                let message = [job.error.as_deref(), job.message.as_deref()]
                    .into_iter()
                    .flatten()
                    .map(str::trim)
                    .find(|text| !text.is_empty())
                    .unwrap_or("Repository clone failed.")
                    .to_string();
                self.finish_clone(Some(message), cx);
            }
            AddProjectCloneJobState::Completed => {
                let cloned = job
                    .project_path
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string();
                if cloned.is_empty() {
                    self.finish_clone(
                        Some("Clone finished without a project path.".to_string()),
                        cx,
                    );
                    return;
                }
                self.register_project_on(machine_id, cloned, false, true, generation, cx);
            }
        }
    }

    fn finish_clone(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        if error.is_some() {
            self.error = error;
        }
        self.clone_job_id = None;
        self.clone_progress = None;
        self.set_busy(None, cx);
        cx.notify();
    }
}

/// The folder a repository is cloned into by default: its name without the owner.
pub(super) fn repository_folder_name(name_with_owner: &str) -> String {
    name_with_owner
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(name_with_owner)
        .trim_end_matches(".git")
        .to_string()
}

/// `~` and `~/…` spelled out against `home` (the machine's home folder, once known).
pub(super) fn expand_home(path: &str, home: Option<&str>) -> String {
    let Some(home) = home.filter(|home| !home.is_empty()) else {
        return path.to_string();
    };
    let home = home.trim_end_matches(['/', '\\']);
    if path == "~" {
        return home.to_string();
    }
    match path.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => path.to_string(),
    }
}

impl super::super::native_modal_kit::ModalCornerClose for GpuiAddProjectModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
