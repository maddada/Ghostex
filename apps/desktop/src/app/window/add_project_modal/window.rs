//! The Add Project dialog entity: its state, the effects that follow it (browse,
//! source-control discovery, pasted-input detection) and the host's answers. The values React
//! derived on every render are `derive.rs`, the steps' operations `steps.rs`, the keyboard model
//! `keyboard.rs`, and the drawing `render.rs` and `review.rs`.
use super::super::native_modal_kit::ModalPalette;
use super::copy::*;
use super::input::*;
use super::model::*;
use super::paths::*;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, ScrollHandle, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

/// `APP_MODAL_HOST_ADD_PROJECT_WINDOW_WIDTH` / `_HEIGHT`: the dialog fills this fixed frame.
pub(crate) const ADD_PROJECT_MODAL_WIDTH: f32 = 640.0;
pub(crate) const ADD_PROJECT_MODAL_HEIGHT: f32 = 460.0;

pub(super) const BROWSE_UP_VALUE: &str = "browse:up";
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
    /// The dialog removed its own window (Escape, or a project was added).
    Close,
}

pub(crate) type AddProjectModalHost = Rc<dyn Fn(AddProjectModalCommand, &mut App)>;

pub(crate) struct AddProjectModalConfig {
    pub(crate) machines: Vec<AddProjectMachineOption>,
    /// Preselects a machine and skips the machine step (the open message's `machineId`).
    pub(crate) initial_machine_id: Option<String>,
    /// Absolute cwd used to resolve `./` and `../`; the app never passes one, like the React host.
    pub(crate) active_project_cwd: Option<String>,
    /// This computer's platform in `navigator.platform` spelling, used before a machine is picked.
    pub(crate) client_platform: String,
    pub(crate) palette: ModalPalette,
    pub(crate) clone_job_poll_interval: Duration,
    pub(crate) slow_operation_notice: Duration,
    /// Opens straight on the clone flow's destination step for this repository, on
    /// `initial_machine_id`.
    ///
    /// CDXC:TranscriptExport 2026-10-25 DECISION:
    /// User (Q544): "show the already existing clone/open project flow that we have for remote machine and local machine but start with the url of the repo to clone already pre selected so the user can pick the folder to clone into or create a new folder." Handoff to Remote opens Add Project this way when the chosen computer has no project with the session's repository.
    pub(crate) initial_clone_url: Option<String>,
}

impl AddProjectModalConfig {
    pub(crate) const CLONE_JOB_POLL_INTERVAL: Duration = Duration::from_millis(900);
    pub(crate) const SLOW_OPERATION_NOTICE: Duration = Duration::from_millis(8000);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddProjectView {
    Machines,
    Sources {
        machine_id: String,
    },
    Browse {
        machine_id: String,
        initial_query: String,
    },
    Clone {
        machine_id: String,
    },
}

impl AddProjectView {
    pub(super) fn machine_id(&self) -> Option<&str> {
        match self {
            Self::Machines => None,
            Self::Sources { machine_id }
            | Self::Browse { machine_id, .. }
            | Self::Clone { machine_id } => Some(machine_id),
        }
    }

    fn kind(&self) -> u8 {
        match self {
            Self::Machines => 0,
            Self::Sources { .. } => 1,
            Self::Browse { .. } => 2,
            Self::Clone { .. } => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CloneStep {
    Repository,
    Destination,
    Review,
}

#[derive(Clone, Debug)]
pub(super) struct CloneFlow {
    pub(super) remote_url: String,
    pub(super) repository: Option<AddProjectRepositoryInfo>,
    pub(super) repository_input: String,
    pub(super) source: AddProjectSourceId,
    pub(super) step: CloneStep,
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

/// What a list row does when chosen (the React rows' `onSelect` closures).
#[derive(Clone, Debug)]
pub(super) enum RowAction {
    BrowseInputFolder {
        machine_id: String,
        path: String,
    },
    ChooseDetectedClone {
        machine_id: String,
        clone: DetectedCloneInput,
    },
    AddSuggested(String),
    BrowseUp,
    BrowseTo(String),
    ChooseMachine(String),
    StartLocalBrowse {
        machine_id: String,
        start: Option<String>,
    },
    StartClone {
        machine_id: String,
        source: AddProjectSourceId,
    },
}

#[derive(Clone, Debug)]
pub(super) struct AddProjectRow {
    pub(super) value: String,
    pub(super) icon: &'static str,
    pub(super) title: String,
    pub(super) description: Option<String>,
    pub(super) disabled: bool,
    /// A not-ready provider: its hint, shown in the `Setup Required` (or Install) tooltip, and the
    /// managed tool that installs its missing CLI, when one does.
    pub(super) setup_required: Option<(AddProjectSourceId, String, Option<String>)>,
    pub(super) action: RowAction,
}

pub(super) const ICON_ALERT: &str = "modals/add-project/alert-triangle.svg";
pub(super) const ICON_ARROW_LEFT: &str = "modals/add-project/arrow-left.svg";
pub(super) const ICON_CHECK: &str = "modals/add-project/check.svg";
pub(super) const ICON_CORNER_LEFT_UP: &str = "modals/add-project/corner-left-up.svg";
pub(super) const ICON_DEVICE_DESKTOP: &str = "modals/add-project/device-desktop.svg";
pub(super) const ICON_FOLDER: &str = "modals/add-project/folder.svg";
pub(super) const ICON_FOLDER_CHECK: &str = "modals/add-project/folder-check.svg";
pub(super) const ICON_FOLDER_PLUS: &str = "modals/add-project/folder-plus.svg";
pub(super) const ICON_FOLDER_ROOT: &str = "modals/add-project/folder-root.svg";
pub(super) const ICON_GIT_BRANCH: &str = "modals/add-project/git-branch.svg";
pub(super) const ICON_SEARCH: &str = "modals/add-project/search.svg";
pub(super) const ICON_SERVER: &str = "modals/add-project/server.svg";

pub(super) fn source_icon(source: AddProjectSourceId) -> &'static str {
    match source {
        AddProjectSourceId::AzureDevops => "modals/add-project/brand-azure.svg",
        AddProjectSourceId::Bitbucket => "modals/add-project/brand-bitbucket.svg",
        AddProjectSourceId::Github => "modals/add-project/brand-github.svg",
        AddProjectSourceId::Gitlab => "modals/add-project/brand-gitlab.svg",
        AddProjectSourceId::Url => "modals/add-project/link.svg",
    }
}

/// The values the React component derived on every render.
pub(super) struct Derived {
    pub(super) machine: Option<AddProjectMachineOption>,
    pub(super) machine_id: Option<String>,
    pub(super) platform: String,
    pub(super) can_pop_view: bool,
    pub(super) is_repository_step: bool,
    pub(super) is_clone_destination_step: bool,
    pub(super) is_browsing: bool,
    pub(super) is_new_folder_step: bool,
    pub(super) browse_directory_path: String,
    pub(super) is_windows_drive_root: bool,
    pub(super) unsupported_windows_path: bool,
    pub(super) relative_path_needs_active_project: bool,
    pub(super) detected_input: Option<DetectedProjectInput>,
    pub(super) detection_machine: Option<AddProjectMachineOption>,
    pub(super) ambiguous: Option<(String, DetectedCloneInput)>,
    pub(super) filtered: FilteredBrowseEntries,
    pub(super) has_highlighted_browse_item: bool,
    pub(super) path_inspection: Option<AddProjectPathInspection>,
    pub(super) suggested_project_path: Option<String>,
    pub(super) resolved_add_project_path: String,
    pub(super) can_submit_browse_path: bool,
    pub(super) will_create_project_path: bool,
    pub(super) new_folder_parent_path: String,
    pub(super) can_create_new_folder: bool,
    pub(super) submit_action_label: &'static str,
    pub(super) add_shortcut_label: String,
    pub(super) readiness: AddProjectReadiness,
    pub(super) rows: Vec<AddProjectRow>,
}

impl Derived {
    pub(super) fn selectable_rows(&self) -> impl Iterator<Item = &AddProjectRow> {
        self.rows.iter().filter(|row| !row.disabled)
    }
}

/// A request in flight and what its answer is for.
pub(super) enum Pending {
    Browse {
        seq: u64,
    },
    Discovery {
        machine_id: String,
        generation: u64,
    },
    Lookup {
        repository_input: String,
        parsed: Option<DetectedCloneInput>,
        source: AddProjectSourceId,
    },
    Preview {
        destination_path: String,
    },
    StartClone,
    ReadCloneJob {
        job_id: String,
    },
    CancelCloneJob,
    Add {
        after_clone: bool,
    },
    CreateFolder,
    /// `installSourceControlTool` / `readSourceControlTool` (tool_install.rs).
    ToolInstall {
        machine_id: String,
    },
}

/// Keyboard focus outside the path input: the chrome buttons Tab reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FocusSlot {
    PathInput,
    PathBack,
    SetupRequired(usize),
    ReviewBack,
    BranchInput,
    CloneMainOnly,
    ShallowClone,
    FooterBack,
    FooterClone,
}

pub(crate) struct GpuiAddProjectModalWindow {
    pub(super) host: AddProjectModalHost,
    pub(super) palette: ModalPalette,
    pub(super) machines: Vec<AddProjectMachineOption>,
    pub(super) active_project_cwd: Option<String>,
    pub(super) client_platform: String,
    pub(super) is_loading_machines: bool,
    pub(super) view_stack: Vec<AddProjectView>,
    pub(super) clone_flow: Option<CloneFlow>,
    pub(super) clone_options: CloneOptions,
    pub(super) clone_preview: Option<AddProjectClonePreview>,
    pub(super) clone_destination_path: String,
    pub(super) query: String,
    pub(super) highlight: Option<String>,
    pub(super) browse_generation: u64,
    pub(super) browse_result: Option<AddProjectBrowseResult>,
    pub(super) is_browse_pending: bool,
    pub(super) error: Option<String>,
    pub(super) busy: Option<Busy>,
    pub(super) is_slow: bool,
    pub(super) discovery: HashMap<String, Option<AddProjectSourceControlDiscovery>>,
    pub(super) pending_discovery_machine_id: Option<String>,
    /// The provider CLI being installed from its row (tool_install.rs).
    pub(super) tool_install: Option<super::tool_install::ToolInstall>,
    /// `None` means "not naming a folder"; the query keeps the listing's directory meanwhile.
    pub(super) new_folder_name: Option<String>,
    pub(super) clone_job_id: Option<String>,
    /// The running clone's latest git progress line (`Receiving objects: 45% …`).
    pub(super) clone_progress: Option<String>,
    pub(super) path_input: Entity<InputState>,
    pub(super) branch_input: Entity<InputState>,
    pub(super) list_scroll: ScrollHandle,
    pub(super) review_scroll: ScrollHandle,
    pub(super) focus_handle: FocusHandle,
    pub(super) chrome_focus: HashMap<FocusSlot, FocusHandle>,
    pub(super) clone_job_poll_interval: Duration,
    slow_operation_notice: Duration,
    next_request_id: u64,
    pending: HashMap<u64, Pending>,
    browse_seq: u64,
    browse_key: Option<String>,
    discovery_key: Option<String>,
    discovery_generation: u64,
    detection_key: Option<String>,
    busy_generation: u64,
    placeholder: String,
    branch_input_disabled: bool,
    _subscriptions: Vec<Subscription>,
}

fn build_initial_view_stack(
    machines: &[AddProjectMachineOption],
    initial_machine_id: Option<&str>,
) -> Vec<AddProjectView> {
    if machines.is_empty() {
        return Vec::new();
    }
    if let Some(preselected) =
        initial_machine_id.and_then(|id| machines.iter().find(|machine| machine.machine_id == id))
    {
        return vec![AddProjectView::Sources {
            machine_id: preselected.machine_id.clone(),
        }];
    }
    if machines.len() == 1 {
        return vec![AddProjectView::Sources {
            machine_id: machines[0].machine_id.clone(),
        }];
    }
    vec![AddProjectView::Machines]
}

impl GpuiAddProjectModalWindow {
    pub(crate) fn new(
        config: AddProjectModalConfig,
        host: AddProjectModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let muted = gpui::Hsla::from(config.palette.muted);
        let path_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_placeholder_color(Some(muted));
            input
        });
        let branch_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Default branch");
            input.set_placeholder_color(Some(muted));
            input
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &path_input,
                window,
                |this: &mut Self, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().to_string();
                        this.on_path_input_changed(value, window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &branch_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.clone_options.branch_name = input.read(cx).value().to_string();
                        this.error = None;
                        cx.notify();
                    }
                },
            ),
        ];
        let view_stack =
            build_initial_view_stack(&config.machines, config.initial_machine_id.as_deref());
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
            is_loading_machines: false,
            view_stack,
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
            clone_job_id: None,
            clone_progress: None,
            path_input,
            branch_input,
            list_scroll: ScrollHandle::new(),
            review_scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            chrome_focus: HashMap::new(),
            clone_job_poll_interval: config.clone_job_poll_interval,
            slow_operation_notice: config.slow_operation_notice,
            next_request_id: 0,
            pending: HashMap::new(),
            browse_seq: 0,
            browse_key: None,
            discovery_key: None,
            discovery_generation: 0,
            detection_key: None,
            busy_generation: 0,
            placeholder: String::new(),
            branch_input_disabled: false,
            _subscriptions: subscriptions,
        };
        if let (Some(url), Some(machine_id)) = (
            config.initial_clone_url.as_deref(),
            this.view_stack
                .last()
                .and_then(|view| view.machine_id())
                .map(str::to_string),
        ) && let Some(ParsedCloneInput::Clone(parsed)) =
            parse_add_project_clone_input(url, AddProjectSourceId::Url)
        {
            this.start_clone_flow(machine_id, AddProjectSourceId::Url);
            this.clone_options = CloneOptions::from_detected(&parsed);
            this.enter_clone_destination_step(
                parsed.remote_url.clone(),
                None,
                url.to_string(),
                parsed.source,
                Some(parsed.destination.clone()),
            );
        }
        // `autoFocus` on the path input.
        this.path_input
            .update(cx, |input, cx| input.focus(window, cx));
        this.settle(window, cx);
        this
    }

    // -- effects --------------------------------------------------------------------------------

    /// Runs the React effects whose dependencies changed, then brings the input in line with the
    /// state. Every entry point ends here.
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

    /// One pass of the discovery, browse and detection effects; true when the detection moved
    /// the dialog to another view, which re-runs them against it.
    fn run_effects(&mut self, cx: &mut Context<Self>) -> bool {
        let d = self.derive();
        let view_kind = self.current_view().map(AddProjectView::kind);
        let is_sources = view_kind == Some(1);

        // Source-control readiness is probed once per machine when its Sources step opens.
        let discovery_key = format!(
            "{is_sources}|{:?}|{}",
            d.machine_id,
            d.machine_id
                .as_deref()
                .is_some_and(|id| self.discovery.contains_key(id))
        );
        if self.discovery_key.as_deref() != Some(discovery_key.as_str()) {
            self.discovery_key = Some(discovery_key);
            self.discovery_generation += 1;
            if let Some(machine_id) = d
                .machine_id
                .clone()
                .filter(|id| is_sources && !self.discovery.contains_key(id))
            {
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
            }
        }

        // Inspect the full input as well as listing its parent: a leaf can be an existing
        // project or a file whose repository root should be offered.
        let browse_key = format!(
            "{:?}|{}|{}|{}|{}|{}|{}|{}|{:?}",
            d.machine_id,
            d.is_browsing,
            d.browse_directory_path,
            d.unsupported_windows_path,
            d.relative_path_needs_active_project,
            self.query,
            d.is_clone_destination_step,
            self.browse_generation,
            self.active_project_cwd,
        );
        if self.browse_key.as_deref() != Some(browse_key.as_str()) {
            self.browse_key = Some(browse_key);
            self.browse_seq += 1;
            if let Some(machine_id) = d.machine_id.clone().filter(|_| {
                d.is_browsing
                    && !d.browse_directory_path.is_empty()
                    && !d.unsupported_windows_path
                    && !d.relative_path_needs_active_project
            }) {
                self.browse_seq += 1;
                let seq = self.browse_seq;
                self.is_browse_pending = true;
                self.browse_result = None;
                let mut params = serde_json::Map::new();
                params.insert(
                    "partialPath".to_string(),
                    serde_json::json!(d.browse_directory_path),
                );
                if !d.is_clone_destination_step {
                    params.insert("inspectPath".to_string(), serde_json::json!(self.query));
                }
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
            }
        }

        // A pasted path or clone on the machine or source step jumps straight to its step.
        let detection_key = format!(
            "{:?}|{}|{:?}",
            view_kind,
            self.query,
            d.detection_machine
                .as_ref()
                .map(|machine| machine.machine_id.clone())
        );
        if self.detection_key.as_deref() == Some(detection_key.as_str()) {
            return false;
        }
        self.detection_key = Some(detection_key);
        if !matches!(view_kind, Some(0 | 1)) {
            return false;
        }
        let (Some(target), Some(input)) = (d.detection_machine, d.detected_input) else {
            return false;
        };
        match input {
            DetectedProjectInput::Error { message } => {
                self.error = Some(message);
                false
            }
            DetectedProjectInput::Ambiguous { .. } => false,
            DetectedProjectInput::Browse { query, .. } => {
                self.push_view(AddProjectView::Browse {
                    machine_id: target.machine_id,
                    initial_query: query,
                });
                true
            }
            DetectedProjectInput::Clone(clone) => {
                self.clone_options = CloneOptions::from_detected(&clone);
                self.clone_preview = None;
                self.clone_destination_path.clear();
                self.clone_flow = Some(CloneFlow {
                    remote_url: clone.remote_url.clone(),
                    repository: None,
                    repository_input: String::new(),
                    source: clone.source,
                    step: CloneStep::Repository,
                });
                self.push_view(AddProjectView::Clone {
                    machine_id: target.machine_id,
                });
                self.query = clone.query;
                true
            }
        }
    }

    /// The path input shows the new folder's name while one is being named, else the query.
    fn sync_path_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self
            .new_folder_name
            .clone()
            .unwrap_or_else(|| self.query.clone());
        let placeholder = if self.new_folder_name.is_some() {
            "New folder name".to_string()
        } else if let Some(flow) = self
            .clone_flow
            .as_ref()
            .filter(|flow| flow.step == CloneStep::Repository)
        {
            repository_placeholder(flow.source)
        } else {
            path_placeholder(self.view_stack.len() > 1).to_string()
        };
        let placeholder_changed = placeholder != self.placeholder;
        if placeholder_changed {
            self.placeholder = placeholder.clone();
        }
        self.path_input.update(cx, |input, cx| {
            if placeholder_changed {
                input.set_placeholder(placeholder, window, cx);
            }
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
                    // The clone's notice sits under the options; bring it into view.
                    if this.busy == Some(Busy::Clone) {
                        this.review_scroll.scroll_to_bottom();
                    }
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
                    Ok(browse) => self.browse_result = Some(browse),
                    Err(error) => {
                        self.browse_result = None;
                        self.error = Some(describe_add_project_error(
                            &error,
                            "Unable to browse that directory.",
                        ));
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
                repository_input,
                parsed,
                source,
            } => {
                self.set_busy(None, cx);
                match result.and_then(|value| read_add_project_repository(&value)) {
                    Ok(repository) => {
                        let remote_url = parsed
                            .as_ref()
                            .map(|parsed| parsed.remote_url.clone())
                            .unwrap_or_else(|| repository.url.clone());
                        self.enter_clone_destination_step(
                            remote_url,
                            Some(repository),
                            repository_input,
                            parsed
                                .as_ref()
                                .map(|parsed| parsed.source)
                                .unwrap_or(source),
                            parsed.map(|parsed| parsed.destination),
                        );
                    }
                    Err(error) => {
                        self.error = Some(describe_add_project_error(
                            &error,
                            "Repository lookup failed.",
                        ));
                    }
                }
            }
            Pending::Preview { destination_path } => {
                self.set_busy(None, cx);
                match result.and_then(|value| read_add_project_clone_preview(&value)) {
                    Ok(preview) => {
                        self.clone_destination_path = destination_path;
                        self.clone_preview = Some(preview);
                        if let Some(flow) = self.clone_flow.as_mut() {
                            flow.step = CloneStep::Review;
                        }
                        self.review_scroll = ScrollHandle::new();
                        let branch = self.clone_options.branch_name.clone();
                        self.branch_input.update(cx, |input, cx| {
                            input.set_value(branch.clone(), window, cx);
                            input.set_selected_range(branch.len()..branch.len(), cx);
                        });
                        self.focus_handle.focus(window, cx);
                    }
                    Err(error) => {
                        self.error = Some(describe_add_project_error(
                            &error,
                            "Unable to review the clone destination.",
                        ));
                    }
                }
            }
            Pending::StartClone => {
                match result.and_then(|value| read_add_project_clone_handle(&value)) {
                    Ok(job_id) => {
                        self.clone_job_id = Some(job_id.clone());
                        self.read_clone_job(job_id, cx);
                    }
                    Err(error) => self.finish_clone(
                        Some(describe_add_project_error(&error, "Clone failed.")),
                        cx,
                    ),
                }
            }
            Pending::ReadCloneJob { job_id } => {
                match result.and_then(|value| read_add_project_clone_job(&value)) {
                    Ok(job) => self.receive_clone_job(job_id, job, cx),
                    Err(error) => self.finish_clone(
                        Some(describe_add_project_error(&error, "Clone failed.")),
                        cx,
                    ),
                }
            }
            Pending::CancelCloneJob => {
                if let Err(error) = result {
                    self.error = Some(describe_add_project_error(
                        &error,
                        "Unable to cancel the clone.",
                    ));
                }
            }
            Pending::Add { after_clone } => {
                match result.and_then(|value| read_add_project_add_result(&value)) {
                    Ok(()) => {
                        // `onProjectAdded`, then `onClose`; the app activates the new project.
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
            Pending::CreateFolder => {
                self.set_busy(None, cx);
                match result.and_then(|value| read_add_project_created_directory(&value)) {
                    Ok(path) => {
                        /*
                        The new folder becomes the browse location, so the very next Enter adds or
                        clones into it. The query keeps whatever prefix the user typed (`~/dev/`),
                        because the created path is only ever a child of it.
                        */
                        self.new_folder_name = None;
                        self.highlight = None;
                        self.query = ensure_browse_directory_path(&path);
                        self.browse_result = None;
                        self.browse_generation += 1;
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

    fn read_clone_job(&mut self, job_id: String, cx: &mut Context<Self>) {
        let Some(machine_id) = self.derive().machine_id else {
            return;
        };
        self.request(
            "readCloneJob",
            &machine_id,
            serde_json::json!({ "jobId": job_id }),
            Pending::ReadCloneJob { job_id },
            cx,
        );
    }

    fn receive_clone_job(
        &mut self,
        job_id: String,
        job: AddProjectCloneJob,
        cx: &mut Context<Self>,
    ) {
        match job.state {
            AddProjectCloneJobState::Running => {
                if job.progress.is_some() {
                    // The progress line sits under the options; bring it into view once.
                    if self.clone_progress.is_none() {
                        self.review_scroll.scroll_to_bottom();
                    }
                    self.clone_progress = job.progress;
                    cx.notify();
                }
                let delay = self.clone_job_poll_interval;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.clone_job_id.as_deref() == Some(job_id.as_str()) {
                            this.read_clone_job(job_id, cx);
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
                self.register_project(cloned, false, true, cx);
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

impl super::super::native_modal_kit::ModalCornerClose for GpuiAddProjectModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
