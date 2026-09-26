//! Native GPUI Add Worktree dialog, the desktop twin of the React
//! `WorktreeCreateModal` in packages/core-ui/worktree-create-modal.tsx.
//!
//! CDXC:Worktrees 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, and "make sure the new gpui modal is EXACTLY 1 to 1 matching the react one": the same layout, copy, colors, radii, spacing, fonts, states, keyboard behaviour and bridge messages in both appearances. Add Worktree keeps its 17px child-window inset, its `#161616` field skin, the raised mode strip, the searchable branch and worktree pickers, and the image-link insertion into the first prompt.
//! SEE-ALSO: packages/core-ui/worktree-create-modal.tsx and the `.worktree-create-*` rules in packages/core-ui/styles/modals.css and modals-light.css (the React twin), packages/components/ui/segmented-control.tsx and raised-tab-rail.css (the mode strip), packages/components/ui/select.tsx and searchable-dropdown.css (the pickers), apps/desktop/src/app/window/native_modal_kit.rs (shared chrome and controls), apps/desktop/src/app/create_worktree_modal_lifecycle.rs (open, bridge commands, results), apps/desktop/src/bin/native_modal_demo/create_worktree.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px, rgb,
};
use gpui_component::input::{InputEvent, InputState, Paste};
use gpui_component::{h_flex, v_flex};
use std::hash::{BuildHasher as _, Hasher as _};
use std::rc::Rc;

/// `APP_MODAL_HOST_WORKTREE_WINDOW_WIDTH`.
pub(crate) const CREATE_WORKTREE_MODAL_WIDTH: f32 = 640.0;
/// `APP_MODAL_HOST_WORKTREE_WINDOW_HEIGHT`, the first-frame height; the window then fits its own layout.
pub(crate) const CREATE_WORKTREE_MODAL_INITIAL_HEIGHT: f32 = 640.0;

/// The React dialog owns a 17px edge inset in the child window instead of the shared 24px.
const WINDOW_INSET: f32 = 17.0;
/// `.session-rename-field-group { gap: 16px }`.
const FIELD_GROUP_GAP: f32 = 16.0;
/// shadcn `Field`: `gap-3` between label, control and description.
const FIELD_GAP: f32 = 12.0;
/// `.worktree-create-modal-shadcn [data-slot='textarea'] { min-height: 150px }`.
const TEXTAREA_MIN_HEIGHT: f32 = 150.0;
/// The fixed-window cap `max-height: min(220px, 42vh)` minus the 12px paddings
/// and 1px borders is 194px of 20px lines; the editor stops growing at nine rows.
const TEXTAREA_MAX_ROWS: usize = 9;

const ICON_GIT_BRANCH: &str = "modals/create-worktree/git-branch.svg";
const ICON_FOLDER_OPEN: &str = "modals/create-worktree/folder-open.svg";
const ICON_PHOTO_PLUS: &str = "modals/create-worktree/photo-plus.svg";
const ICON_CHEVRON_DOWN: &str = "modals/create-worktree/chevron-down.svg";
const ICON_SELECTOR: &str = "modals/kit/selector.svg";

const TITLE: &str = "Add Worktree";
const MODE_LABEL: &str = "Mode";
const MODE_CREATE: &str = "Create New";
const MODE_OPEN_EXISTING: &str = "Open Existing";
const EXISTING_WORKTREE_LABEL: &str = "Existing worktree";
const EXISTING_LOADING_PLACEHOLDER: &str = "Loading worktrees";
const EXISTING_PLACEHOLDER: &str = "Select worktree";
const EXISTING_FILTER_PLACEHOLDER: &str = "Filter worktrees...";
const EXISTING_EMPTY_ROW: &str = "No worktrees match";
const EXISTING_EMPTY_MESSAGE: &str = "No worktrees match.";
const EXISTING_LOADING_DESCRIPTION: &str = "Loading existing worktrees.";
const EXISTING_NONE_DESCRIPTION: &str = "No existing worktrees found.";
const EXISTING_LOAD_FAILED: &str = "Could not load existing worktrees.";
const BASE_BRANCH_LABEL: &str = "Base branch";
const BRANCH_LOADING_PLACEHOLDER: &str = "Loading branches";
const BRANCH_PLACEHOLDER: &str = "Select branch";
const BRANCH_FILTER_PLACEHOLDER: &str = "Filter branches...";
const BRANCH_EMPTY_ROW: &str = "No branches match";
const BRANCH_EMPTY_MESSAGE: &str = "No branches match.";
const BRANCH_LOADING_DESCRIPTION: &str = "Loading branches.";
const BRANCH_NONE_DESCRIPTION: &str = "No branches found.";
const AGENT_LABEL: &str = "Agent";
const AGENT_PLACEHOLDER: &str = "Select agent";
const PROMPT_LABEL: &str = "First prompt";
const PROMPT_LABEL_OPTIONAL: &str = "First prompt (optional)";
const PROMPT_PLACEHOLDER: &str = "Describe the worktree task";
const PROMPT_PLACEHOLDER_OPTIONAL: &str = "Optional prompt after opening";
const PROMPT_DESCRIPTION: &str =
    "Paste images or pick files to insert image links into the prompt.";
const ADD_IMAGES: &str = "Add Images";
const SUBMIT_CREATE: &str = "New Worktree";
const SUBMIT_OPEN: &str = "Open Worktree";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreateWorktreeMode {
    Create,
    OpenExisting,
}

/// A sidebar HUD agent with a launch command (the React modal lists only those).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CreateWorktreeAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
}

/// One `branches[]` entry of `projectWorktreesResult`, already normalized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorktreeBaseBranchOption {
    pub(crate) name: String,
    pub(crate) current: bool,
    pub(crate) remote: bool,
}

/// One `worktrees[]` entry of `projectWorktreesResult`, already normalized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExistingWorktreeOption {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) branch: String,
    pub(crate) is_registered: bool,
    pub(crate) is_current_project: bool,
    pub(crate) worktree_key: Option<String>,
}

impl ExistingWorktreeOption {
    /// The select option value: the opaque gxserver key when present, else the path.
    pub(crate) fn option_value(&self) -> String {
        self.worktree_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.path.trim().to_string())
    }
}

/// `normalizeWorktreeBaseBranchOptions`: objects with a non-empty trimmed name, deduplicated by name.
pub(crate) fn normalize_worktree_base_branch_options(
    candidate: Option<&serde_json::Value>,
) -> Vec<WorktreeBaseBranchOption> {
    let Some(entries) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    entries
        .iter()
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?.trim();
            if name.is_empty() || !seen.insert(name.to_string()) {
                return None;
            }
            Some(WorktreeBaseBranchOption {
                name: name.to_string(),
                current: entry.get("current").and_then(serde_json::Value::as_bool) == Some(true),
                remote: entry.get("remote").and_then(serde_json::Value::as_bool) == Some(true),
            })
        })
        .collect()
}

/// `normalizeExistingWorktreeOptions`: objects with a non-empty trimmed path and name.
pub(crate) fn normalize_existing_worktree_options(
    candidate: Option<&serde_json::Value>,
) -> Vec<ExistingWorktreeOption> {
    let Some(entries) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let text = |key: &str| entry.get(key).and_then(serde_json::Value::as_str);
            let path = text("path")?.trim();
            let name = text("name")?.trim();
            if path.is_empty() || name.is_empty() {
                return None;
            }
            let flag =
                |key: &str| entry.get(key).and_then(serde_json::Value::as_bool) == Some(true);
            Some(ExistingWorktreeOption {
                name: name.to_string(),
                path: path.to_string(),
                branch: text("branch").map(str::trim).unwrap_or("").to_string(),
                is_registered: flag("isRegistered"),
                is_current_project: flag("isCurrentProject"),
                worktree_key: text("worktreeKey")
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(str::to_string),
            })
        })
        .collect()
}

/// `trimPromptEditorTrailingSpaces`: removes spaces and tabs at the end of
/// every line while keeping indentation, blank lines and line endings.
pub(crate) fn trim_prompt_editor_trailing_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut line_start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' || byte == b'\r' {
            out.push_str(text[line_start..index].trim_end_matches([' ', '\t']));
            out.push(byte as char);
            line_start = index + 1;
        }
    }
    out.push_str(text[line_start..].trim_end_matches([' ', '\t']));
    out
}

/// `worktrees-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`.
pub(crate) fn new_worktree_list_request_id() -> String {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(now);
    format!("worktrees-{}-{}", base36(now), base36(hasher.finish()))
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    while value > 0 {
        digits.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}

/// The submitted draft (`WorktreeCreateModalDraft`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CreateWorktreeDraft {
    Create {
        agent_id: String,
        base_branch: String,
        prompt: String,
    },
    /// A blank prompt opens the worktree only; a prompt carries the agent with it.
    OpenExisting {
        agent_id: Option<String>,
        prompt: Option<String>,
        existing_worktree_key: Option<String>,
        existing_worktree_path: String,
    },
}

/// What the dialog asks its host to do. `Create` and `Cancel` are sent after
/// the dialog removed its own window.
pub(crate) enum CreateWorktreeModalCommand {
    /// `requestProjectWorktrees`, sent once when the dialog opens.
    RequestWorktrees {
        request_id: String,
    },
    /// `pickWorktreeImages`: open the native file prompt.
    PickImages,
    /// `createProjectWorktree`.
    Create(CreateWorktreeDraft),
    Cancel,
}

pub(crate) type CreateWorktreeModalHost = Rc<dyn Fn(CreateWorktreeModalCommand, &mut App)>;

pub(crate) struct CreateWorktreeModalConfig {
    pub(crate) agents: Vec<CreateWorktreeAgent>,
    /// `settings.defaultPromptAgentId`: preselected when it has a command.
    pub(crate) default_agent_id: Option<String>,
    pub(crate) palette: ModalPalette,
}

/// The three pickers, also the preview binary's handle for opening one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreateWorktreePicker {
    Existing,
    Branch,
    Agent,
}

type OpenMenu = CreateWorktreePicker;

pub(crate) struct GpuiCreateWorktreeModalWindow {
    host: CreateWorktreeModalHost,
    palette: ModalPalette,
    agents: Vec<CreateWorktreeAgent>,
    mode: CreateWorktreeMode,
    selected_agent_id: String,
    base_branches: Vec<WorktreeBaseBranchOption>,
    selected_base_branch: String,
    existing_worktrees: Vec<ExistingWorktreeOption>,
    selected_existing_value: String,
    worktree_list_error: Option<String>,
    is_loading_worktrees: bool,
    image_count: usize,
    request_id: String,
    prompt: gpui::Entity<InputState>,
    existing_select: ModalSelect,
    existing_filter: gpui::Entity<InputState>,
    branch_select: ModalSelect,
    branch_filter: gpui::Entity<InputState>,
    agent_select: ModalSelect,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiCreateWorktreeModalWindow {
    pub(crate) fn new(
        config: CreateWorktreeModalConfig,
        host: CreateWorktreeModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected_agent_id =
            resolve_initial_agent_id(&config.agents, config.default_agent_id.as_deref());
        let prompt = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(1, TEXTAREA_MAX_ROWS)
                // Plain Enter submits (`handleKeyDown`), Shift+Enter is the editor's newline.
                .submit_on_enter(true)
                .placeholder(PROMPT_PLACEHOLDER)
        });
        let existing_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(EXISTING_FILTER_PLACEHOLDER));
        let branch_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(BRANCH_FILTER_PLACEHOLDER));
        let subscriptions = vec![
            cx.subscribe_in(
                &prompt,
                window,
                |this: &mut Self, _input, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { shift: false, .. } => this.submit(window, cx),
                    InputEvent::Change | InputEvent::Focus | InputEvent::Blur => cx.notify(),
                    InputEvent::PressEnter { .. } => {}
                },
            ),
            cx.subscribe_in(
                &existing_filter,
                window,
                |this: &mut Self, _input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.existing_select.highlight = Some(0);
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &branch_filter,
                window,
                |this: &mut Self, _input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.branch_select.highlight = Some(0);
                        cx.notify();
                    }
                },
            ),
        ];
        // `autoFocus` on the first-prompt textarea.
        prompt.update(cx, |prompt, cx| prompt.focus(window, cx));
        let request_id = new_worktree_list_request_id();
        (host)(
            CreateWorktreeModalCommand::RequestWorktrees {
                request_id: request_id.clone(),
            },
            cx,
        );
        Self {
            host,
            palette: config.palette,
            agents: config.agents,
            mode: CreateWorktreeMode::Create,
            selected_agent_id,
            base_branches: Vec::new(),
            selected_base_branch: String::new(),
            existing_worktrees: Vec::new(),
            selected_existing_value: String::new(),
            worktree_list_error: None,
            is_loading_worktrees: true,
            image_count: 0,
            request_id,
            prompt,
            existing_select: ModalSelect::new(),
            existing_filter,
            branch_select: ModalSelect::new(),
            branch_filter,
            agent_select: ModalSelect::new(),
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The answer to `RequestWorktrees`. Answers for another request id are ignored.
    pub(crate) fn receive_project_worktrees_result(
        &mut self,
        request_id: &str,
        ok: bool,
        error: Option<String>,
        branches: Vec<WorktreeBaseBranchOption>,
        worktrees: Vec<ExistingWorktreeOption>,
        cx: &mut Context<Self>,
    ) {
        if request_id != self.request_id {
            return;
        }
        self.is_loading_worktrees = false;
        if !ok {
            self.worktree_list_error =
                Some(error.unwrap_or_else(|| EXISTING_LOAD_FAILED.to_string()));
            self.base_branches = Vec::new();
            self.selected_base_branch = String::new();
            self.existing_worktrees = Vec::new();
            cx.notify();
            return;
        }
        if !branches
            .iter()
            .any(|branch| branch.name == self.selected_base_branch)
        {
            self.selected_base_branch = branches
                .iter()
                .find(|branch| branch.current)
                .or_else(|| branches.first())
                .map(|branch| branch.name.clone())
                .unwrap_or_default();
        }
        self.base_branches = branches;
        if !worktrees
            .iter()
            .any(|worktree| worktree.option_value() == self.selected_existing_value)
        {
            self.selected_existing_value = worktrees
                .iter()
                .find(|worktree| !worktree.is_registered)
                .or_else(|| worktrees.first())
                .map(ExistingWorktreeOption::option_value)
                .unwrap_or_default();
        }
        self.existing_worktrees = worktrees;
        cx.notify();
    }

    /// `worktreeImageFilesPicked`: inserts `[Image #n](path)` lines at the caret.
    pub(crate) fn receive_image_files_picked(
        &mut self,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        let links: Vec<String> = paths
            .iter()
            .enumerate()
            .map(|(index, path)| format!("[Image #{}]({path})", self.image_count + index + 1))
            .collect();
        self.image_count += paths.len();
        let prompt = self.prompt.read(cx).value().to_string();
        let prefix = if prompt.is_empty() || prompt.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        self.insert_prompt_text(format!("{prefix}{}\n", links.join("\n")), window, cx);
    }

    /// Preview hooks for the standalone demo binary: put the dialog into a
    /// mode, a prompt text, or an open picker with a typed filter.
    #[allow(dead_code)] // used by src/bin/native_modal_demo/create_worktree.rs only
    pub(crate) fn preview_set_mode(
        &mut self,
        mode: CreateWorktreeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_mode(mode, window, cx);
    }

    #[allow(dead_code)] // used by src/bin/native_modal_demo/create_worktree.rs only
    pub(crate) fn preview_set_prompt(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt.update(cx, |prompt, cx| {
            prompt.set_value(text.to_string(), window, cx);
            // A typed prompt leaves the caret at its end; `set_value` parks it at 0.
            let end = prompt.value().len();
            prompt.set_selected_range(end..end, cx);
            prompt.focus(window, cx);
        });
        cx.notify();
    }

    #[allow(dead_code)] // used by src/bin/native_modal_demo/create_worktree.rs only
    pub(crate) fn preview_open_picker(
        &mut self,
        picker: CreateWorktreePicker,
        filter: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_menus();
        self.toggle_menu(picker, window, cx);
        let filter_state = match picker {
            CreateWorktreePicker::Existing => Some(&self.existing_filter),
            CreateWorktreePicker::Branch => Some(&self.branch_filter),
            CreateWorktreePicker::Agent => None,
        };
        if let Some(filter_state) = filter_state {
            if !filter.is_empty() {
                filter_state.update(cx, |state, cx| {
                    state.set_value(filter.to_string(), window, cx)
                });
                match picker {
                    CreateWorktreePicker::Existing => self.existing_select.highlight = Some(0),
                    CreateWorktreePicker::Branch => self.branch_select.highlight = Some(0),
                    CreateWorktreePicker::Agent => {}
                }
            }
        }
        cx.notify();
    }

    fn insert_prompt_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt.update(cx, |prompt, cx| {
            prompt.replace(text, window, cx);
            prompt.focus(window, cx);
        });
        cx.notify();
    }

    fn command_agents(&self) -> &[CreateWorktreeAgent] {
        &self.agents
    }

    fn normalized_prompt(&self, cx: &App) -> String {
        trim_prompt_editor_trailing_spaces(&self.prompt.read(cx).value())
    }

    fn trimmed_prompt(&self, cx: &App) -> String {
        self.normalized_prompt(cx).trim().to_string()
    }

    fn can_create(&self, cx: &App) -> bool {
        let trimmed_prompt = self.trimmed_prompt(cx);
        match self.mode {
            CreateWorktreeMode::OpenExisting => {
                !self.selected_existing_value.is_empty()
                    && (trimmed_prompt.is_empty() || !self.selected_agent_id.is_empty())
            }
            CreateWorktreeMode::Create => {
                !trimmed_prompt.is_empty()
                    && !self.selected_agent_id.is_empty()
                    && !self.selected_base_branch.is_empty()
            }
        }
    }

    fn draft(&self, cx: &App) -> CreateWorktreeDraft {
        let prompt = self.trimmed_prompt(cx);
        match self.mode {
            CreateWorktreeMode::OpenExisting => {
                let selected = self
                    .existing_worktrees
                    .iter()
                    .find(|worktree| worktree.option_value() == self.selected_existing_value);
                let (agent_id, prompt) = if prompt.is_empty() {
                    (None, None)
                } else {
                    (Some(self.selected_agent_id.clone()), Some(prompt))
                };
                CreateWorktreeDraft::OpenExisting {
                    agent_id,
                    prompt,
                    existing_worktree_key: selected
                        .and_then(|worktree| worktree.worktree_key.clone()),
                    existing_worktree_path: selected
                        .map(|worktree| worktree.path.clone())
                        .unwrap_or_else(|| self.selected_existing_value.clone()),
                }
            }
            CreateWorktreeMode::Create => CreateWorktreeDraft::Create {
                agent_id: self.selected_agent_id.clone(),
                base_branch: self.selected_base_branch.clone(),
                prompt,
            },
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_create(cx) {
            return;
        }
        let draft = self.draft(cx);
        window.remove_window();
        (self.host)(CreateWorktreeModalCommand::Create(draft), cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(CreateWorktreeModalCommand::Cancel, cx);
    }

    fn pick_images(&mut self, cx: &mut Context<Self>) {
        (self.host)(CreateWorktreeModalCommand::PickImages, cx);
    }

    fn set_mode(&mut self, mode: CreateWorktreeMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.close_menus();
        self.prompt.update(cx, |prompt, cx| {
            prompt.set_placeholder(
                match mode {
                    CreateWorktreeMode::Create => PROMPT_PLACEHOLDER,
                    CreateWorktreeMode::OpenExisting => PROMPT_PLACEHOLDER_OPTIONAL,
                },
                window,
                cx,
            );
        });
        cx.notify();
    }

    fn open_menu(&self) -> Option<OpenMenu> {
        if self.existing_select.open {
            Some(OpenMenu::Existing)
        } else if self.branch_select.open {
            Some(OpenMenu::Branch)
        } else if self.agent_select.open {
            Some(OpenMenu::Agent)
        } else {
            None
        }
    }

    fn close_menus(&mut self) {
        self.existing_select.close();
        self.branch_select.close();
        self.agent_select.close();
    }

    /// Existing-worktree rows that survive the filter, with the option index each maps to
    /// (`None` for the disabled "No worktrees match" placeholder of an empty list).
    fn visible_existing_rows(
        &self,
        cx: &App,
    ) -> (Vec<ModalSearchableSelectRow>, Vec<Option<usize>>) {
        let query = self.existing_filter.read(cx).value().to_string();
        if self.existing_worktrees.is_empty() {
            return if modal_select_filter_matches(&query, EXISTING_EMPTY_ROW) {
                (
                    vec![ModalSearchableSelectRow {
                        label: EXISTING_EMPTY_ROW.to_string(),
                        disabled: true,
                    }],
                    vec![None],
                )
            } else {
                (Vec::new(), Vec::new())
            };
        }
        let mut rows = Vec::new();
        let mut indices = Vec::new();
        for (index, worktree) in self.existing_worktrees.iter().enumerate() {
            let label = existing_worktree_label(worktree);
            // `label` prop plus the rendered text, the way `collectSelectItems` indexes it.
            let search_text = format!(
                "{} {} {} {label}",
                worktree.name, worktree.branch, worktree.path
            );
            if modal_select_filter_matches(&query, &search_text) {
                rows.push(ModalSearchableSelectRow {
                    label,
                    disabled: false,
                });
                indices.push(Some(index));
            }
        }
        (rows, indices)
    }

    fn visible_branch_rows(&self, cx: &App) -> (Vec<ModalSearchableSelectRow>, Vec<Option<usize>>) {
        let query = self.branch_filter.read(cx).value().to_string();
        if self.base_branches.is_empty() {
            return if modal_select_filter_matches(&query, BRANCH_EMPTY_ROW) {
                (
                    vec![ModalSearchableSelectRow {
                        label: BRANCH_EMPTY_ROW.to_string(),
                        disabled: true,
                    }],
                    vec![None],
                )
            } else {
                (Vec::new(), Vec::new())
            };
        }
        let mut rows = Vec::new();
        let mut indices = Vec::new();
        for (index, branch) in self.base_branches.iter().enumerate() {
            let label = base_branch_label(branch);
            if modal_select_filter_matches(&query, &label) {
                rows.push(ModalSearchableSelectRow {
                    label,
                    disabled: false,
                });
                indices.push(Some(index));
            }
        }
        (rows, indices)
    }

    fn selected_existing_index(&self) -> Option<usize> {
        self.existing_worktrees
            .iter()
            .position(|worktree| worktree.option_value() == self.selected_existing_value)
    }

    fn selected_branch_index(&self) -> Option<usize> {
        self.base_branches
            .iter()
            .position(|branch| branch.name == self.selected_base_branch)
    }

    fn selected_agent_index(&self) -> Option<usize> {
        self.command_agents()
            .iter()
            .position(|agent| agent.agent_id == self.selected_agent_id)
    }

    fn toggle_menu(&mut self, menu: OpenMenu, window: &mut Window, cx: &mut Context<Self>) {
        let was_open = self.open_menu() == Some(menu);
        self.close_menus();
        if was_open {
            self.focus_handle.focus(window, cx);
            cx.notify();
            return;
        }
        match menu {
            OpenMenu::Existing => {
                // The popup opens with an empty query and focus on the filter.
                self.existing_filter.update(cx, |filter, cx| {
                    filter.set_value("", window, cx);
                    filter.focus(window, cx);
                });
                let (_, indices) = self.visible_existing_rows(cx);
                let selected = self.selected_existing_index();
                let highlight = indices
                    .iter()
                    .position(|index| *index == selected && selected.is_some());
                self.existing_select.toggle(highlight);
            }
            OpenMenu::Branch => {
                self.branch_filter.update(cx, |filter, cx| {
                    filter.set_value("", window, cx);
                    filter.focus(window, cx);
                });
                let (_, indices) = self.visible_branch_rows(cx);
                let selected = self.selected_branch_index();
                let highlight = indices
                    .iter()
                    .position(|index| *index == selected && selected.is_some());
                self.branch_select.toggle(highlight);
            }
            OpenMenu::Agent => {
                if self.command_agents().is_empty() {
                    return;
                }
                self.focus_handle.focus(window, cx);
                self.agent_select.toggle(self.selected_agent_index());
            }
        }
        cx.notify();
    }

    fn dismiss_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn choose_existing(
        &mut self,
        visible_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (_, indices) = self.visible_existing_rows(cx);
        if let Some(Some(index)) = indices.get(visible_index) {
            if let Some(worktree) = self.existing_worktrees.get(*index) {
                self.selected_existing_value = worktree.option_value();
            }
        } else {
            return;
        }
        self.dismiss_menus(window, cx);
    }

    fn choose_branch(&mut self, visible_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (_, indices) = self.visible_branch_rows(cx);
        if let Some(Some(index)) = indices.get(visible_index) {
            if let Some(branch) = self.base_branches.get(*index) {
                self.selected_base_branch = branch.name.clone();
            }
        } else {
            return;
        }
        self.dismiss_menus(window, cx);
    }

    fn choose_agent(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(agent) = self.command_agents().get(index) {
            self.selected_agent_id = agent.agent_id.clone();
        }
        self.dismiss_menus(window, cx);
    }

    fn clear_filter(&mut self, menu: OpenMenu, window: &mut Window, cx: &mut Context<Self>) {
        let filter = match menu {
            OpenMenu::Existing => &self.existing_filter,
            OpenMenu::Branch => &self.branch_filter,
            OpenMenu::Agent => return,
        };
        filter.update(cx, |filter, cx| {
            filter.set_value("", window, cx);
            filter.focus(window, cx);
        });
        match menu {
            OpenMenu::Existing => self.existing_select.highlight = Some(0),
            OpenMenu::Branch => self.branch_select.highlight = Some(0),
            OpenMenu::Agent => {}
        }
        cx.notify();
    }

    /// Plain-text paste into the prompt drops trailing spaces per line
    /// (`handlePaste`); a paste that changes nothing is left to the editor.
    fn on_paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if !self.prompt.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let trimmed = trim_prompt_editor_trailing_spaces(&text);
        if text.is_empty() || trimmed == text {
            return;
        }
        cx.stop_propagation();
        self.insert_prompt_text(trimmed, window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if matches!(key, "up" | "down" | "enter" | "escape") {
            if let Some(menu) = self.open_menu() {
                let count = match menu {
                    OpenMenu::Existing => self.visible_existing_rows(cx).0.len(),
                    OpenMenu::Branch => self.visible_branch_rows(cx).0.len(),
                    OpenMenu::Agent => self.command_agents().len(),
                };
                let select = match menu {
                    OpenMenu::Existing => &mut self.existing_select,
                    OpenMenu::Branch => &mut self.branch_select,
                    OpenMenu::Agent => &mut self.agent_select,
                };
                match select.handle_key(key, count) {
                    ModalSelectKey::Consumed => {
                        if !select.open {
                            self.focus_handle.focus(window, cx);
                        }
                        cx.notify();
                    }
                    ModalSelectKey::Choose(index) => match menu {
                        OpenMenu::Existing => self.choose_existing(index, window, cx),
                        OpenMenu::Branch => self.choose_branch(index, window, cx),
                        OpenMenu::Agent => self.choose_agent(index, window, cx),
                    },
                    ModalSelectKey::Ignored => {}
                }
                cx.stop_propagation();
                return;
            }
        }
        match key {
            "escape" => self.cancel(window, cx),
            "enter" => {
                // With the prompt focused, gpui runs the editor's Enter action
                // before this listener, so its `PressEnter` subscription submits
                // and the propagated key must not submit a second time.
                if event.keystroke.modifiers.shift
                    || event.is_held
                    || self.prompt.read(cx).focus_handle(cx).is_focused(window)
                {
                    return;
                }
                self.submit(window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// The trigger skin: the same field, but no focus edge while a popup is
    /// open, because Base UI moves keyboard focus into the popup and the React
    /// trigger keeps its resting hairline.
    fn trigger_skin(&self) -> ModalFieldSkin {
        let skin = self.field_skin();
        ModalFieldSkin {
            focus_border: skin.border,
            ..skin
        }
    }

    fn field_skin(&self) -> ModalFieldSkin {
        let p = self.palette;
        // `.worktree-create-modal-shadcn :is([data-slot='select-trigger'], [data-slot='textarea'])`
        // and its light override: #161616 / #f5f5f5 on rgba(255,255,255,.08) / rgba(0,0,0,.16),
        // focused on rgba(255,255,255,.28) / #525252.
        if p.light {
            ModalFieldSkin {
                background: rgb(0xf5f5f5),
                border: modal_rgba(0x000000, 0.16),
                focus_border: rgb(0x525252),
                text_size: 13.0,
            }
        } else {
            ModalFieldSkin {
                background: rgb(0x161616),
                border: modal_rgba(0xffffff, 0.08),
                focus_border: modal_rgba(0xffffff, 0.28),
                text_size: 13.0,
            }
        }
    }

    /// `[data-slot='field-label']`: 12px/500 muted, `leading-snug`.
    fn render_field_label(&self, text: &'static str) -> AnyElement {
        div()
            .text_size(px(12.0))
            .line_height(px(16.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(self.palette.muted))
            .child(text)
            .into_any_element()
    }

    /// `[data-slot='field-description']`: 12px/400 muted, `leading-normal`.
    fn render_field_description(&self, text: String) -> AnyElement {
        div()
            .text_size(px(12.0))
            .line_height(px(18.0))
            .text_color(hsla(self.palette.muted))
            .child(text)
            .into_any_element()
    }

    fn render_field(&self, children: Vec<AnyElement>) -> gpui::Div {
        v_flex().w_full().gap(px(FIELD_GAP)).children(children)
    }

    fn render_mode_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let items = [
            ModalSegmentedItem {
                icon: Some(ICON_GIT_BRANCH),
                label: MODE_CREATE,
            },
            ModalSegmentedItem {
                icon: Some(ICON_FOLDER_OPEN),
                label: MODE_OPEN_EXISTING,
            },
        ];
        let selected = match self.mode {
            CreateWorktreeMode::Create => 0,
            CreateWorktreeMode::OpenExisting => 1,
        };
        self.render_field(vec![
            self.render_field_label(MODE_LABEL),
            modal_segmented_control(
                &p,
                "create-worktree-mode",
                &items,
                selected,
                |this, index, window, cx| {
                    this.set_mode(
                        if index == 0 {
                            CreateWorktreeMode::Create
                        } else {
                            CreateWorktreeMode::OpenExisting
                        },
                        window,
                        cx,
                    );
                },
                cx,
            ),
        ])
        .into_any_element()
    }

    fn render_existing_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let value = self
            .existing_worktrees
            .iter()
            .find(|worktree| worktree.option_value() == self.selected_existing_value)
            .map(existing_worktree_label);
        let description = self.worktree_list_error.clone().or_else(|| {
            if self.is_loading_worktrees {
                Some(EXISTING_LOADING_DESCRIPTION.to_string())
            } else if self.existing_worktrees.is_empty() {
                Some(EXISTING_NONE_DESCRIPTION.to_string())
            } else {
                None
            }
        });
        let mut children = vec![
            self.render_field_label(EXISTING_WORKTREE_LABEL),
            modal_select_trigger_skinned(
                &p,
                &self.existing_select,
                "create-worktree-existing-select",
                value,
                if self.is_loading_worktrees {
                    EXISTING_LOADING_PLACEHOLDER
                } else {
                    EXISTING_PLACEHOLDER
                },
                self.trigger_skin(),
                ICON_CHEVRON_DOWN,
                false,
                |this, window, cx| this.toggle_menu(OpenMenu::Existing, window, cx),
                cx,
            ),
        ];
        if let Some(description) = description {
            children.push(self.render_field_description(description));
        }
        self.render_field(children)
            .on_children_prepainted(capture_child_bounds(
                self.existing_select.trigger_bounds.clone(),
                1,
            ))
            .into_any_element()
    }

    fn render_branch_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let value = self
            .base_branches
            .iter()
            .find(|branch| branch.name == self.selected_base_branch)
            .map(base_branch_label);
        let mut children = vec![
            self.render_field_label(BASE_BRANCH_LABEL),
            modal_select_trigger_skinned(
                &p,
                &self.branch_select,
                "create-worktree-branch-select",
                value,
                if self.is_loading_worktrees {
                    BRANCH_LOADING_PLACEHOLDER
                } else {
                    BRANCH_PLACEHOLDER
                },
                self.trigger_skin(),
                ICON_CHEVRON_DOWN,
                false,
                |this, window, cx| this.toggle_menu(OpenMenu::Branch, window, cx),
                cx,
            ),
        ];
        if self.selected_base_branch.is_empty() {
            children.push(
                self.render_field_description(
                    if self.is_loading_worktrees {
                        BRANCH_LOADING_DESCRIPTION
                    } else {
                        BRANCH_NONE_DESCRIPTION
                    }
                    .to_string(),
                ),
            );
        }
        self.render_field(children)
            .on_children_prepainted(capture_child_bounds(
                self.branch_select.trigger_bounds.clone(),
                1,
            ))
            .into_any_element()
    }

    fn render_agent_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let value = self
            .command_agents()
            .iter()
            .find(|agent| agent.agent_id == self.selected_agent_id)
            .map(|agent| agent.name.clone());
        self.render_field(vec![
            self.render_field_label(AGENT_LABEL),
            modal_select_trigger_skinned(
                &p,
                &self.agent_select,
                "create-worktree-agent-select",
                value,
                AGENT_PLACEHOLDER,
                self.trigger_skin(),
                ICON_SELECTOR,
                false,
                |this, window, cx| this.toggle_menu(OpenMenu::Agent, window, cx),
                cx,
            ),
        ])
        .on_children_prepainted(capture_child_bounds(
            self.agent_select.trigger_bounds.clone(),
            1,
        ))
        .into_any_element()
    }

    fn render_prompt_field(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        self.render_field(vec![
            self.render_field_label(match self.mode {
                CreateWorktreeMode::Create => PROMPT_LABEL,
                CreateWorktreeMode::OpenExisting => PROMPT_LABEL_OPTIONAL,
            }),
            modal_text_area_skinned(
                &p,
                &self.prompt,
                self.field_skin(),
                TEXTAREA_MIN_HEIGHT,
                false,
                window,
                cx,
            ),
            self.render_field_description(PROMPT_DESCRIPTION.to_string()),
        ])
        .into_any_element()
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let mut body = v_flex()
            .w_full()
            .gap(px(FIELD_GROUP_GAP))
            .child(self.render_mode_field(cx));
        body = match self.mode {
            CreateWorktreeMode::OpenExisting => body.child(self.render_existing_field(cx)),
            CreateWorktreeMode::Create => body.child(self.render_branch_field(cx)),
        };
        body.child(self.render_agent_field(cx))
            .child(self.render_prompt_field(window, cx))
            .into_any_element()
    }

    /// shadcn `Button` inside `.worktree-create-modal-shadcn`: 32px, 8px radius, 13px/400,
    /// 12px side padding (10px before a leading icon), 6px icon gap, half opacity when disabled.
    fn render_footer_button(
        &self,
        id: &'static str,
        label: &'static str,
        icon: Option<&'static str>,
        primary: bool,
        disabled: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        // `--secondary` / `--secondary-foreground`: oklch(27.4% .006 286) and oklch(98.5%)
        // in dark, #f1f1f1 / #262626 in light (modals-light.css).
        let (secondary, secondary_foreground) = if p.light {
            (rgb(0xf1f1f1), rgb(0x262626))
        } else {
            (rgb(0x27272a), rgb(0xfafafa))
        };
        let (background, text, hover) = if primary {
            (p.primary, p.primary_foreground, rgba_of(p.primary, 0.8))
        } else {
            // `hover:bg-[color-mix(in_oklch,var(--secondary),var(--foreground)_5%)]`.
            (
                secondary,
                secondary_foreground,
                css_mix(secondary, 0.95, p.foreground),
            )
        };
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h(px(MODAL_FOOTER_BUTTON_HEIGHT))
            .pl(px(if icon.is_some() { 10.0 } else { 12.0 }))
            .pr(px(12.0))
            .gap(px(6.0))
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .bg(hsla(background))
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(hsla(text))
            .whitespace_nowrap()
            .when(disabled, |this| this.opacity(0.5).cursor_default())
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_click(this, window, cx);
                    }))
            })
            .children(icon.map(|icon| modal_icon(icon, 16.0, text)))
            .child(label)
            .into_any_element()
    }

    /// `DialogFooter`: right-aligned, 8px apart.
    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let add_images = self.render_footer_button(
            "create-worktree-add-images",
            ADD_IMAGES,
            Some(ICON_PHOTO_PLUS),
            false,
            false,
            |this, _window, cx| this.pick_images(cx),
            cx,
        );
        let submit = self.render_footer_button(
            "create-worktree-submit",
            match self.mode {
                CreateWorktreeMode::Create => SUBMIT_CREATE,
                CreateWorktreeMode::OpenExisting => SUBMIT_OPEN,
            },
            None,
            true,
            !self.can_create(cx),
            |this, window, cx| this.submit(window, cx),
            cx,
        );
        h_flex()
            .w_full()
            .justify_end()
            .gap(px(8.0))
            .child(add_images)
            .child(submit)
            .into_any_element()
    }

    fn render_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        match self.open_menu()? {
            OpenMenu::Existing => {
                let (rows, _) = self.visible_existing_rows(cx);
                modal_searchable_select_menu(
                    &p,
                    &self.existing_select,
                    "create-worktree-existing-menu",
                    &self.existing_filter,
                    &rows,
                    EXISTING_EMPTY_MESSAGE,
                    |this, index, window, cx| this.choose_existing(index, window, cx),
                    |this, window, cx| this.clear_filter(OpenMenu::Existing, window, cx),
                    |this, window, cx| this.dismiss_menus(window, cx),
                    window,
                    cx,
                )
            }
            OpenMenu::Branch => {
                let (rows, _) = self.visible_branch_rows(cx);
                modal_searchable_select_menu(
                    &p,
                    &self.branch_select,
                    "create-worktree-branch-menu",
                    &self.branch_filter,
                    &rows,
                    BRANCH_EMPTY_MESSAGE,
                    |this, index, window, cx| this.choose_branch(index, window, cx),
                    |this, window, cx| this.clear_filter(OpenMenu::Branch, window, cx),
                    |this, window, cx| this.dismiss_menus(window, cx),
                    window,
                    cx,
                )
            }
            OpenMenu::Agent => self.render_agent_menu(cx),
        }
    }

    /// The plain shadcn `SelectContent` (`packages/components/ui/select.tsx`)
    /// as the React modal renders it: `bg-popover` (the modal surface), the
    /// tooltip border, 8px radius, `shadow-lg`, a 4px-padded group of 28px rows
    /// at 14px with 6px 8px padding and 6px radius, the selected row on 12%
    /// foreground and the hovered or keyboard-highlighted row on the accent.
    fn render_agent_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let select = &self.agent_select;
        if !select.open {
            return None;
        }
        let trigger = select.trigger_bounds.get()?;
        let highlight = select.highlight;
        let selected = self.selected_agent_index();
        let popup_border = if p.light {
            modal_rgba(0x000000, 0.14)
        } else {
            modal_rgba(0xffffff, 0.12)
        };
        let rows = self
            .command_agents()
            .iter()
            .enumerate()
            .map(|(index, agent)| {
                let is_selected = selected == Some(index);
                let highlighted = highlight == Some(index);
                h_flex()
                    .id(("create-worktree-agent-row", index))
                    .w_full()
                    .min_h(px(28.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .gap(px(8.0))
                    .items_center()
                    .rounded(px(6.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.foreground))
                    .cursor_default()
                    .when(is_selected, |this| {
                        this.bg(hsla(p.menu_selected_background()))
                    })
                    .when(!is_selected && highlighted, |this| this.bg(hsla(p.accent)))
                    .when(!is_selected, |this| {
                        this.hover(move |this| this.bg(hsla(p.accent)))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose_agent(index, window, cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(agent.name.clone()),
                    )
            })
            .collect::<Vec<_>>();
        // Base UI `alignItemWithTrigger`: the popup overlays the trigger so the
        // selected row (32px tall: 20px line plus 6px paddings) sits on it,
        // 1px border plus 4px group padding plus 1px text alignment above.
        let aligned_row = selected.unwrap_or(0) as f32;
        let position = gpui::point(
            trigger.origin.x,
            trigger.origin.y - px(6.0) - px(32.0 * aligned_row),
        );
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        v_flex()
                            .id("create-worktree-agent-menu")
                            .occlude()
                            .w(trigger.size.width)
                            .p(px(4.0))
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .border_1()
                            .border_color(hsla(popup_border))
                            .bg(hsla(p.surface))
                            .shadow_lg()
                            .on_mouse_down_out(cx.listener(
                                move |this, event: &gpui::MouseDownEvent, window, cx| {
                                    if trigger.contains(&event.position) {
                                        return;
                                    }
                                    this.dismiss_menus(window, cx);
                                },
                            ))
                            .children(rows),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

impl Render for GpuiCreateWorktreeModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let menu = self.render_menu(window, cx);
        // `[data-slot='dialog-title']`: 15px/500, line-height 1.35.
        let header = div()
            .w_full()
            .text_size(px(15.0))
            .line_height(px(20.25))
            .font_weight(FontWeight::MEDIUM)
            .child(TITLE)
            .into_any_element();
        let content = vec![header, self.render_body(window, cx)];
        let footer = self.render_footer(cx);
        modal_shell_inset(
            &p,
            "ghostex-gpui-create-worktree-modal",
            &self.focus_handle,
            &self.fit,
            WINDOW_INSET,
            Self::on_key_down,
            content,
            footer,
            menu,
            cx,
        )
        .capture_action(cx.listener(Self::on_paste))
    }
}

/// `resolveInitialWorktreeAgentId`: the default prompt agent when it has a command, else the first.
fn resolve_initial_agent_id(
    agents: &[CreateWorktreeAgent],
    default_agent_id: Option<&str>,
) -> String {
    agents
        .iter()
        .find(|agent| Some(agent.agent_id.as_str()) == default_agent_id)
        .or_else(|| agents.first())
        .map(|agent| agent.agent_id.clone())
        .unwrap_or_default()
}

/// `{worktree.name} {worktree.branch ? `(${worktree.branch})` : ''}`.
fn existing_worktree_label(worktree: &ExistingWorktreeOption) -> String {
    if worktree.branch.is_empty() {
        format!("{} ", worktree.name)
    } else {
        format!("{} ({})", worktree.name, worktree.branch)
    }
}

/// `{branch.name}{branch.current ? ' (current)' : branch.remote ? ' (remote)' : ''}`.
fn base_branch_label(branch: &WorktreeBaseBranchOption) -> String {
    if branch.current {
        format!("{} (current)", branch.name)
    } else if branch.remote {
        format!("{} (remote)", branch.name)
    } else {
        branch.name.clone()
    }
}
