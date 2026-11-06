//! Add Project's form for a brand-new project: a name, the folder it makes, the Space and project
//! group it joins and what it starts. The name is typed in the dialog's one input; the four
//! choices are rows under it, each opening a short list (or, for the folder, a folder browse).
//!
//! CDXC:AddProject 2026-11-05 DECISION:
//! User (Q1306, "keep it called Add Project"): Add Project offers a small form for a brand-new project instead of a folder name alone: the name, the folder it makes, then Space, Group and Start filled in from where you are, each changeable before it is made. The doors stay the same (Q1307).
//! User (Q1310): with no Default projects folder the folder starts in the folder that holds the selected project's folder, else ~/dev; any folder can be picked, and the first project made saves it as the Default projects folder.
//! User (Q1308): Start shows the agent that starts and switches to another agent, Terminal or Nothing; the last pick is remembered.
//! SEE-ALSO: apps/desktop/src/app/add_project_modal_lifecycle.rs (what the form starts from), apps/desktop/src/app/gx_store/added_project.rs (where the made project goes and what it starts), packages/gx-core/src/sidebar_drag/added_project.rs (`plan_new_project_placement`).
use super::copy::*;
use super::model::*;
use super::paths::*;
use super::window::*;
use gpui::{Context, Window};
use std::collections::HashMap;

/// Where a project is made when no folder is set or suggested (user, Q1310).
pub(crate) const NEW_PROJECT_FALLBACK_PARENT: &str = "~/dev";

/// The icons of the form's rows.
pub(super) const ICON_SPACE: &str = "titlebar/stack.svg";
pub(super) const ICON_GROUP: &str = "titlebar/folders.svg";
pub(super) const ICON_START_AGENT: &str = "titlebar/sparkles.svg";
pub(super) const ICON_START_TERMINAL: &str = "titlebar/terminal-2.svg";
pub(super) const ICON_START_NOTHING: &str = "titlebar/player-play.svg";

/// What a new project starts once it is added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AddProjectStart {
    Agent(String),
    Terminal,
    Nothing,
}

impl AddProjectStart {
    /// `agent:<id>`, `terminal` or `none`: the add request's `newProject.start` and the stored
    /// last pick.
    pub(crate) fn wire(&self) -> String {
        match self {
            Self::Agent(agent_id) => format!("agent:{agent_id}"),
            Self::Terminal => "terminal".to_string(),
            Self::Nothing => "none".to_string(),
        }
    }

    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value.trim() {
            "terminal" => Some(Self::Terminal),
            "none" => Some(Self::Nothing),
            other => other
                .strip_prefix("agent:")
                .map(str::trim)
                .filter(|agent_id| !agent_id.is_empty() && agent_id.len() <= 128)
                .map(|agent_id| Self::Agent(agent_id.to_string())),
        }
    }
}

/// A Space or project group the form offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AddProjectPlaceChoice {
    pub(crate) id: String,
    pub(crate) label: String,
    /// For a project group: the Spaces that list it, which a project in it joins.
    pub(crate) space_ids: Vec<String>,
}

/// What the form starts from on one machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AddProjectPlacementOptions {
    /// `None` when Spaces are off there: the form has no Space row.
    pub(crate) spaces: Option<Vec<AddProjectPlaceChoice>>,
    /// Empty when the machine has no project group, or when the app cannot file one there.
    pub(crate) groups: Vec<AddProjectPlaceChoice>,
    /// The Space open in the sidebar; `None` for Other.
    pub(crate) space_id: Option<String>,
    /// The selected project's group.
    pub(crate) group_id: Option<String>,
    /// The folder that holds the selected project's folder on that machine.
    pub(crate) selected_project_parent: Option<String>,
}

/// An agent Start can launch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AddProjectStartAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
}

/// Everything the form starts from, built by the app when Add Project opens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AddProjectNewProjectConfig {
    /// By machine id; a machine without an entry gets no Space or group row.
    pub(crate) placements: HashMap<String, AddProjectPlacementOptions>,
    pub(crate) agents: Vec<AddProjectStartAgent>,
    /// The remembered pick, else the default agent.
    pub(crate) start: Option<AddProjectStart>,
}

/// A form row that opens a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NewProjectField {
    Space,
    Group,
    Start,
}

/// What is already at the folder the form would make, as the machine last answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NewProjectExisting {
    Nothing,
    Folder,
    Project,
}

/// The form's state while it is open.
#[derive(Clone, Debug)]
pub(super) struct NewProjectForm {
    pub(super) machine_id: String,
    pub(super) name: String,
    /// The folder the project folder is made in, as typed or picked (`~/dev`, `/Users/me/dev`).
    pub(super) parent: String,
    /// No Default projects folder is set on this computer, so the first create saves `parent`.
    pub(super) save_parent_as_default: bool,
    pub(super) space_id: Option<String>,
    pub(super) group_id: Option<String>,
    pub(super) start: AddProjectStart,
    pub(super) existing: Option<NewProjectExisting>,
}

impl NewProjectForm {
    /// The folder the project gets, `<parent>/<name>`.
    pub(super) fn folder_path(&self) -> String {
        format!(
            "{}{}",
            ensure_browse_directory_path(&self.parent),
            self.name.trim()
        )
    }

    /// Why Create is refused, or `None` when it can go.
    pub(super) fn name_problem(&self) -> Option<&'static str> {
        let name = self.name.trim();
        if name.is_empty() {
            return Some("Type a name for the new project.");
        }
        if name.contains(['/', '\\']) {
            return Some("A project name cannot contain a path separator.");
        }
        if name == "." || name == ".." {
            return Some("Choose another name.");
        }
        None
    }
}

impl GpuiAddProjectModalWindow {
    pub(super) fn new_project_placement(&self, machine_id: &str) -> AddProjectPlacementOptions {
        self.new_project_config
            .placements
            .get(machine_id)
            .cloned()
            .unwrap_or_default()
    }

    /// The folder a new project is made in on `machine_id`, and whether it is a suggestion that the
    /// first create saves as the Default projects folder.
    pub(super) fn new_project_parent(&self, machine_id: &str) -> (String, bool) {
        let machine = self
            .machines
            .iter()
            .find(|machine| machine.machine_id == machine_id);
        if let Some(folder) = machine
            .and_then(|machine| machine.add_project_base_directory.as_deref())
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
        {
            return (normalize_project_path_for_dispatch(folder), false);
        }
        let suggested = self
            .new_project_placement(machine_id)
            .selected_project_parent
            .filter(|parent| !parent.trim().is_empty())
            .unwrap_or_else(|| NEW_PROJECT_FALLBACK_PARENT.to_string());
        // The setting is this computer's; a remote machine's suggestion is never saved.
        (suggested, machine_id == LOCAL_MACHINE_ID)
    }

    /// New project folder: opens the form on `machine_id`, filled in from where the user is.
    pub(super) fn open_new_project_form(&mut self, machine_id: String) {
        let placement = self.new_project_placement(&machine_id);
        let (parent, save_parent_as_default) = self.new_project_parent(&machine_id);
        let start = self
            .new_project_config
            .start
            .clone()
            .filter(|start| match start {
                AddProjectStart::Agent(agent_id) => self
                    .new_project_config
                    .agents
                    .iter()
                    .any(|agent| &agent.agent_id == agent_id),
                _ => true,
            })
            .or_else(|| {
                self.new_project_config
                    .agents
                    .first()
                    .map(|agent| AddProjectStart::Agent(agent.agent_id.clone()))
            })
            .unwrap_or(AddProjectStart::Terminal);
        let group_id = placement
            .group_id
            .filter(|group_id| placement.groups.iter().any(|group| &group.id == group_id));
        self.new_project = Some(NewProjectForm {
            machine_id: machine_id.clone(),
            name: String::new(),
            parent,
            save_parent_as_default,
            space_id: placement.space_id.filter(|space_id| {
                placement
                    .spaces
                    .as_ref()
                    .is_some_and(|spaces| spaces.iter().any(|space| &space.id == space_id))
            }),
            group_id,
            start,
            existing: None,
        });
        self.new_project_inspect_key = None;
        self.push_view(AddProjectView::NewProject { machine_id });
    }

    pub(super) fn is_new_project_view(&self) -> bool {
        matches!(self.current_view(), Some(AddProjectView::NewProject { .. }))
    }

    pub(super) fn new_project_pick_field(&self) -> Option<NewProjectField> {
        match self.current_view() {
            Some(AddProjectView::NewProjectPick { field, .. }) => Some(*field),
            _ => None,
        }
    }

    /// The typed name, while the form is the open view.
    pub(super) fn set_new_project_name(&mut self, value: String) -> bool {
        let Some(form) = self.new_project.as_mut() else {
            return false;
        };
        if form.name == value {
            return false;
        }
        form.name = value;
        form.existing = None;
        self.error = None;
        true
    }

    fn space_label(&self, placement: &AddProjectPlacementOptions, space_id: &str) -> String {
        placement
            .spaces
            .iter()
            .flatten()
            .find(|space| space.id == space_id)
            .map(|space| space.label.clone())
            .unwrap_or_else(|| "None".to_string())
    }

    fn start_label(&self, start: &AddProjectStart) -> String {
        match start {
            AddProjectStart::Agent(agent_id) => self
                .new_project_config
                .agents
                .iter()
                .find(|agent| &agent.agent_id == agent_id)
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| agent_id.clone()),
            AddProjectStart::Terminal => "Terminal".to_string(),
            AddProjectStart::Nothing => "Nothing".to_string(),
        }
    }

    /// The form's rows: Folder, Space (when Spaces are on), Group (when there is one) and Start.
    pub(super) fn build_new_project_rows(&self) -> Vec<AddProjectRow> {
        let Some(form) = self.new_project.as_ref() else {
            return Vec::new();
        };
        let placement = self.new_project_placement(&form.machine_id);
        let mut rows = Vec::new();
        let folder = if form.name.trim().is_empty() {
            format!("{}…", ensure_browse_directory_path(&form.parent))
        } else {
            form.folder_path()
        };
        let note = match form.existing {
            Some(NewProjectExisting::Project) => " · already a project; Create opens it",
            Some(NewProjectExisting::Folder) => " · already exists; Create adds it as it is",
            _ if form.save_parent_as_default => " · becomes your projects folder",
            _ => "",
        };
        rows.push(AddProjectRow {
            value: "new-project:folder".to_string(),
            icon: ICON_FOLDER,
            title: "Folder".to_string(),
            description: Some(format!("{folder}{note}")),
            disabled: false,
            setup_required: None,
            action: RowAction::NewProjectChangeFolder,
        });
        let group = form
            .group_id
            .as_deref()
            .and_then(|group_id| placement.groups.iter().find(|group| group.id == group_id));
        if placement.spaces.is_some() {
            let description = match (group, form.space_id.as_deref()) {
                (Some(group), _) if !group.space_ids.is_empty() => format!(
                    "{} (from the group)",
                    group
                        .space_ids
                        .iter()
                        .map(|space_id| self.space_label(&placement, space_id))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                (Some(_), _) => "None (from the group)".to_string(),
                (None, Some(space_id)) => self.space_label(&placement, space_id),
                (None, None) => "None".to_string(),
            };
            rows.push(AddProjectRow {
                value: "new-project:space".to_string(),
                icon: ICON_SPACE,
                title: "Space".to_string(),
                description: Some(description),
                disabled: false,
                setup_required: None,
                action: RowAction::NewProjectPick(NewProjectField::Space),
            });
        }
        if !placement.groups.is_empty() {
            rows.push(AddProjectRow {
                value: "new-project:group".to_string(),
                icon: ICON_GROUP,
                title: "Group".to_string(),
                description: Some(
                    group
                        .map(|group| group.label.clone())
                        .unwrap_or_else(|| "None".to_string()),
                ),
                disabled: false,
                setup_required: None,
                action: RowAction::NewProjectPick(NewProjectField::Group),
            });
        }
        rows.push(AddProjectRow {
            value: "new-project:start".to_string(),
            icon: start_icon(&form.start),
            title: "Start".to_string(),
            description: Some(self.start_label(&form.start)),
            disabled: false,
            setup_required: None,
            action: RowAction::NewProjectPick(NewProjectField::Start),
        });
        rows
    }

    /// One field's choices, filtered by the input, the current one ticked.
    pub(super) fn build_new_project_pick_rows(&self, field: NewProjectField) -> Vec<AddProjectRow> {
        let Some(form) = self.new_project.as_ref() else {
            return Vec::new();
        };
        let placement = self.new_project_placement(&form.machine_id);
        let choice = |value: Option<String>, label: String, icon: &'static str, current: bool| {
            AddProjectRow {
                value: format!(
                    "new-project-pick:{}",
                    value.clone().unwrap_or_else(|| "none".to_string())
                ),
                icon: if current { ICON_CHECK } else { icon },
                title: label,
                description: None,
                disabled: false,
                setup_required: None,
                action: RowAction::NewProjectChoose { field, value },
            }
        };
        let rows: Vec<AddProjectRow> = match field {
            NewProjectField::Space => std::iter::once(choice(
                None,
                "None".to_string(),
                ICON_SPACE,
                form.space_id.is_none() && form.group_id.is_none(),
            ))
            .chain(placement.spaces.iter().flatten().map(|space| {
                choice(
                    Some(space.id.clone()),
                    space.label.clone(),
                    ICON_SPACE,
                    form.group_id.is_none() && form.space_id.as_deref() == Some(&space.id),
                )
            }))
            .collect(),
            NewProjectField::Group => std::iter::once(choice(
                None,
                "None".to_string(),
                ICON_GROUP,
                form.group_id.is_none(),
            ))
            .chain(placement.groups.iter().map(|group| {
                choice(
                    Some(group.id.clone()),
                    group.label.clone(),
                    ICON_GROUP,
                    form.group_id.as_deref() == Some(&group.id),
                )
            }))
            .collect(),
            NewProjectField::Start => self
                .new_project_config
                .agents
                .iter()
                .map(|agent| AddProjectStart::Agent(agent.agent_id.clone()))
                .chain([AddProjectStart::Terminal, AddProjectStart::Nothing])
                .map(|start| {
                    choice(
                        Some(start.wire()),
                        self.start_label(&start),
                        start_icon(&start),
                        form.start == start,
                    )
                })
                .collect(),
        };
        rows.into_iter()
            .filter(|row| matches_filter(&self.query, &row.title, &[]))
            .collect()
    }

    pub(super) fn open_new_project_pick(&mut self, field: NewProjectField) {
        let Some(machine_id) = self
            .new_project
            .as_ref()
            .map(|form| form.machine_id.clone())
        else {
            return;
        };
        self.push_view(AddProjectView::NewProjectPick { machine_id, field });
        // The current choice is highlighted, so Enter right away keeps it.
        self.highlight = self
            .build_new_project_pick_rows(field)
            .into_iter()
            .find(|row| row.icon == ICON_CHECK)
            .map(|row| row.value);
    }

    pub(super) fn choose_new_project_value(
        &mut self,
        field: NewProjectField,
        value: Option<String>,
    ) {
        let Some(machine_id) = self
            .new_project
            .as_ref()
            .map(|form| form.machine_id.clone())
        else {
            return;
        };
        let placement = self.new_project_placement(&machine_id);
        if let Some(form) = self.new_project.as_mut() {
            match field {
                NewProjectField::Space => {
                    // A project in a group is in the group's Spaces; picking a Space the group is
                    // not in takes the project out of the group.
                    let keeps_group = form.group_id.as_deref().is_some_and(|group_id| {
                        placement.groups.iter().any(|group| {
                            group.id == group_id
                                && value
                                    .as_ref()
                                    .is_some_and(|space_id| group.space_ids.contains(space_id))
                        })
                    });
                    if !keeps_group {
                        form.group_id = None;
                    }
                    form.space_id = value;
                }
                NewProjectField::Group => {
                    if let Some(group) = value
                        .as_deref()
                        .and_then(|id| placement.groups.iter().find(|group| group.id == id))
                    {
                        form.space_id = group.space_ids.first().cloned();
                    }
                    form.group_id = value;
                }
                NewProjectField::Start => {
                    if let Some(start) = value.as_deref().and_then(AddProjectStart::from_wire) {
                        form.start = start;
                    }
                }
            }
        }
        // Back on the form with nothing highlighted, so the next Enter makes the project.
        self.pop_view();
    }

    /// The Folder row: browse from the current folder; the folder on screen is taken with
    /// Use Folder.
    pub(super) fn start_new_project_folder_pick(&mut self) {
        let Some(form) = self.new_project.as_ref() else {
            return;
        };
        let machine_id = form.machine_id.clone();
        let start = ensure_browse_directory_path(&form.parent);
        self.push_view(AddProjectView::Browse {
            machine_id,
            initial_query: start,
        });
        self.picking_new_project_parent = true;
    }

    /// Use Folder while picking: the directory on screen becomes the parent, back on the form.
    pub(super) fn take_new_project_folder(&mut self, d: &Derived) {
        let picked = if has_trailing_path_separator(&self.query) {
            self.browse_result
                .as_ref()
                .map(|result| result.parent_path.clone())
                .unwrap_or_else(|| self.query.trim().to_string())
        } else {
            d.filtered
                .exact
                .as_ref()
                .map(|entry| entry.full_path.clone())
                .unwrap_or_else(|| self.query.trim().to_string())
        };
        let picked = normalize_project_path_for_dispatch(&picked);
        if picked.is_empty() {
            return;
        }
        if let Some(form) = self.new_project.as_mut() {
            form.parent = picked;
            form.existing = None;
        }
        self.pop_view();
    }

    /// The machine's answer about the folder the form would make.
    pub(super) fn receive_new_project_inspection(
        &mut self,
        key: String,
        result: Result<serde_json::Value, String>,
    ) {
        if self.new_project_inspect_key.as_deref() != Some(key.as_str()) {
            return;
        }
        let existing = match result.and_then(|value| read_add_project_browse_result(&value)) {
            Ok(browse) => match browse.inspection {
                Some(inspection) if inspection.project_id.is_some() => NewProjectExisting::Project,
                Some(inspection)
                    if inspection.kind == Some(AddProjectInspectionKind::Directory) =>
                {
                    NewProjectExisting::Folder
                }
                _ => NewProjectExisting::Nothing,
            },
            // A parent that does not exist yet is made with the project.
            Err(_) => NewProjectExisting::Nothing,
        };
        if let Some(form) = self.new_project.as_mut() {
            form.existing = Some(existing);
        }
    }

    /// Asks the machine what is at the folder the form would make, once per name and folder.
    pub(super) fn run_new_project_inspection(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self
            .new_project
            .as_ref()
            .filter(|form| form.name_problem().is_none())
        else {
            return;
        };
        if !self.is_new_project_view() {
            return;
        }
        let machine_id = form.machine_id.clone();
        let path = form.folder_path();
        let key = format!("{machine_id}|{path}");
        if self.new_project_inspect_key.as_deref() == Some(key.as_str()) {
            return;
        }
        self.new_project_inspect_key = Some(key.clone());
        self.request(
            "browse",
            &machine_id,
            serde_json::json!({
                "partialPath": ensure_browse_directory_path(&form.parent),
                "inspectPath": path,
            }),
            Pending::NewProjectInspect { key },
            cx,
        );
    }

    /// Create: the folder is made (with its parent when that is missing) and added; what the form
    /// picked rides along for the app to apply once the project is there.
    pub(super) fn submit_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.new_project.clone() else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        if let Some(problem) = form.name_problem() {
            self.error = Some(problem.to_string());
            self.settle(window, cx);
            return;
        }
        let placement = self.new_project_placement(&form.machine_id);
        // A name that is already a project opens that project where it is.
        let existing_project = form.existing == Some(NewProjectExisting::Project);
        let group_id = form.group_id.clone().filter(|_| !existing_project);
        let space_id = match group_id {
            Some(_) => None,
            None => form
                .space_id
                .clone()
                .filter(|_| placement.spaces.is_some() && !existing_project),
        };
        self.error = None;
        self.set_busy(Some(Busy::Add), cx);
        self.request(
            "add",
            &form.machine_id,
            serde_json::json!({
                "createIfMissing": true,
                "path": form.folder_path(),
                "newProject": {
                    "collectionId": group_id,
                    "saveDefaultFolder": form.save_parent_as_default.then_some(form.parent.clone()),
                    "spaceId": space_id,
                    "start": form.start.wire(),
                },
            }),
            Pending::Add { after_clone: false },
            cx,
        );
        self.settle(window, cx);
    }

    /// The Sources row that opens the form, offered on every machine.
    pub(super) fn new_project_source_row(&self, machine_id: &str) -> AddProjectRow {
        let (parent, _) = self.new_project_parent(machine_id);
        AddProjectRow {
            value: "source:new-project-folder".to_string(),
            icon: ICON_FOLDER_PLUS,
            title: "New project folder".to_string(),
            description: Some(format!("Make a new project in {parent}")),
            disabled: false,
            setup_required: None,
            action: RowAction::NewProjectFolder {
                machine_id: machine_id.to_string(),
            },
        }
    }
}

pub(super) fn start_icon(start: &AddProjectStart) -> &'static str {
    match start {
        AddProjectStart::Agent(_) => ICON_START_AGENT,
        AddProjectStart::Terminal => ICON_START_TERMINAL,
        AddProjectStart::Nothing => ICON_START_NOTHING,
    }
}

/// The list's label over the form or one of its lists.
pub(super) fn new_project_group_label(field: Option<NewProjectField>) -> &'static str {
    match field {
        None => "New project",
        Some(NewProjectField::Space) => "Space",
        Some(NewProjectField::Group) => "Project group",
        Some(NewProjectField::Start) => "Start",
    }
}

/// The input's placeholder over the form or one of its lists.
pub(super) fn new_project_placeholder(field: Option<NewProjectField>) -> &'static str {
    match field {
        None => "Project name",
        Some(NewProjectField::Space) => "Filter Spaces",
        Some(NewProjectField::Group) => "Filter project groups",
        Some(NewProjectField::Start) => "Filter",
    }
}
