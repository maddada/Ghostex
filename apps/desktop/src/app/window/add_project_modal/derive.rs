//! The values the React dialog derived on every render: the current machine and platform, the
//! step flags, the browse listing's filter and inspection, the submit label and shortcut, the
//! provider readiness and the list rows.
use super::copy::*;
use super::input::*;
use super::model::*;
use super::paths::*;
use super::window::*;

impl GpuiAddProjectModalWindow {
    pub(super) fn current_view(&self) -> Option<&AddProjectView> {
        self.view_stack.last()
    }

    pub(super) fn clone_step(&self) -> Option<CloneStep> {
        self.clone_flow.as_ref().map(|flow| flow.step)
    }

    pub(super) fn derive(&self) -> Derived {
        let current = self.current_view();
        let machine_id = current
            .and_then(AddProjectView::machine_id)
            .map(str::to_string);
        let machine = machine_id.as_deref().and_then(|id| {
            self.machines
                .iter()
                .find(|machine| machine.machine_id == id)
                .cloned()
        });
        // A connected host with an unknown platform is validated by that host, not by this OS.
        let platform = match &machine {
            Some(machine) => machine.platform.clone().unwrap_or_default(),
            None => self.client_platform.clone(),
        };
        let can_pop_view = self.view_stack.len() > 1;
        let step = self.clone_step();
        let is_repository_step = step == Some(CloneStep::Repository);
        let is_clone_destination_step = step == Some(CloneStep::Destination);
        let is_clone_review_step = step == Some(CloneStep::Review);
        let query = self.query.as_str();
        let browse_platform = if platform.is_empty() {
            "Win32"
        } else {
            &platform
        };
        let is_browsing = !is_repository_step
            && !is_clone_review_step
            && is_filesystem_browse_query(query, browse_platform);
        let is_new_folder_step = self.new_folder_name.is_some();
        let browse_directory_path = if is_browsing {
            get_browse_directory_path(query)
        } else {
            String::new()
        };
        let is_windows_drive_root = is_windows_drive_root(&browse_directory_path);
        let browse_filter_query = if is_browsing && !has_trailing_path_separator(query) {
            get_browse_leaf_path_segment(query)
        } else {
            String::new()
        };
        let unsupported_windows_path =
            !platform.is_empty() && is_unsupported_windows_project_path(query.trim(), &platform);
        let relative_path_needs_active_project =
            is_explicit_relative_project_path(query.trim()) && self.active_project_cwd.is_none();
        let detected_input = classify_add_project_input(query, &self.machines);
        let detection_machine = match &detected_input {
            Some(DetectedProjectInput::Browse {
                machine_id: Some(id),
                ..
            }) => self
                .machines
                .iter()
                .find(|machine| &machine.machine_id == id)
                .cloned(),
            _ => machine.clone().or_else(|| {
                self.machines
                    .iter()
                    .find(|machine| machine.machine_id == LOCAL_MACHINE_ID)
                    .cloned()
            }),
        };
        let menu_view = matches!(
            current,
            Some(AddProjectView::Machines | AddProjectView::Sources { .. })
        );
        let ambiguous = match (&detected_input, menu_view) {
            (Some(DetectedProjectInput::Ambiguous { query, clone }), true) => {
                Some((query.clone(), clone.clone()))
            }
            _ => None,
        };
        let entries = self
            .browse_result
            .as_ref()
            .map(|result| result.entries.as_slice())
            .unwrap_or(&[]);
        let filtered =
            filter_browse_entries(entries, &browse_filter_query, self.highlight.as_deref());
        let has_highlighted_browse_item =
            filtered.highlighted.is_some() || self.highlight.as_deref() == Some(BROWSE_UP_VALUE);
        let path_inspection = if self.is_browse_pending {
            None
        } else {
            self.browse_result
                .as_ref()
                .and_then(|result| result.inspection.clone())
        };
        let suggested_project_path = path_inspection.as_ref().and_then(|inspection| {
            inspection
                .project_path
                .clone()
                .or_else(|| inspection.git_root.clone())
        });
        let resolved_add_project_path = suggested_project_path.clone().unwrap_or_else(|| {
            if has_trailing_path_separator(query) {
                self.browse_result
                    .as_ref()
                    .map(|result| result.parent_path.clone())
                    .unwrap_or_else(|| query.trim().to_string())
            } else {
                filtered
                    .exact
                    .as_ref()
                    .map(|entry| entry.full_path.clone())
                    .unwrap_or_else(|| query.trim().to_string())
            }
        });
        let is_drive_list = self
            .browse_result
            .as_ref()
            .is_some_and(|result| result.is_drive_list);
        let inspection_kind = path_inspection
            .as_ref()
            .and_then(|inspection| inspection.kind);
        let can_submit_browse_path = is_browsing
            && !is_drive_list
            && !self.is_browse_pending
            && !relative_path_needs_active_project
            && !unsupported_windows_path
            && !(inspection_kind == Some(AddProjectInspectionKind::File)
                && suggested_project_path.is_none());
        let will_create_project_path = can_submit_browse_path
            && !self.is_browse_pending
            && !query.trim().is_empty()
            && !has_highlighted_browse_item
            && suggested_project_path.is_none()
            && inspection_kind != Some(AddProjectInspectionKind::Directory)
            && if has_trailing_path_separator(query) {
                self.browse_result.is_none()
            } else {
                filtered.exact.is_none()
            };
        /*
        CDXC:AddProject 2026-08-18:
        The folder is created inside the directory whose entries are on screen, which is the
        server-resolved parent of the current query. A typed leaf filter narrows that listing but
        never changes which directory it belongs to, so the affordance stays available while the
        user is filtering.
        */
        let new_folder_parent_path = self
            .browse_result
            .as_ref()
            .map(|result| result.parent_path.clone())
            .unwrap_or_default();
        let can_create_new_folder = is_browsing
            && !is_drive_list
            && machine_id.is_some()
            && !self.is_browse_pending
            && !new_folder_parent_path.is_empty()
            && !unsupported_windows_path
            && !relative_path_needs_active_project;
        let submit_action_label = if is_clone_destination_step {
            "Continue"
        } else if path_inspection
            .as_ref()
            .is_some_and(|inspection| inspection.project_id.is_some())
        {
            "Open existing project"
        } else if path_inspection
            .as_ref()
            .is_some_and(|inspection| inspection.git_root.is_some())
        {
            "Add repository root"
        } else if will_create_project_path {
            "Create & Add"
        } else {
            "Add"
        };
        let add_shortcut_label = if has_highlighted_browse_item {
            crate::hotkey_label::terminal_overlay_hotkey_chord_label("cmd+enter")
        } else {
            "Enter".to_string()
        };
        let readiness = build_source_readiness(
            machine_id
                .as_deref()
                .and_then(|id| self.discovery.get(id))
                .and_then(Option::as_ref),
        );
        let mut derived = Derived {
            machine,
            machine_id,
            platform,
            can_pop_view,
            is_repository_step,
            is_clone_destination_step,
            is_browsing,
            is_new_folder_step,
            browse_directory_path,
            is_windows_drive_root,
            unsupported_windows_path,
            relative_path_needs_active_project,
            detected_input,
            detection_machine,
            ambiguous,
            filtered,
            has_highlighted_browse_item,
            path_inspection,
            suggested_project_path,
            resolved_add_project_path,
            can_submit_browse_path,
            will_create_project_path,
            new_folder_parent_path,
            can_create_new_folder,
            submit_action_label,
            add_shortcut_label,
            readiness,
            rows: Vec::new(),
        };
        derived.rows = self.build_rows(&derived);
        derived
    }

    pub(super) fn resolve_input_destination(
        &self,
        path: &str,
        target: Option<&AddProjectMachineOption>,
        current_machine_id: Option<&str>,
    ) -> String {
        if let Some(explicit) = normalize_pasted_project_path(path)
            && !is_explicit_relative_project_path(&explicit)
        {
            return explicit;
        }
        let base = match (target, &self.active_project_cwd) {
            (Some(target), Some(cwd)) if Some(target.machine_id.as_str()) == current_machine_id => {
                cwd.clone()
            }
            _ => initial_browse_query(target),
        };
        format!(
            "{}{}",
            ensure_browse_directory_path(&base),
            path.strip_prefix("./").unwrap_or(path)
        )
    }

    fn build_rows(&self, d: &Derived) -> Vec<AddProjectRow> {
        if d.is_repository_step || d.is_new_folder_step {
            return Vec::new();
        }
        if let (Some((query, clone)), Some(target)) = (&d.ambiguous, &d.detection_machine) {
            let path = self.resolve_input_destination(query, Some(target), d.machine_id.as_deref());
            return vec![
                AddProjectRow {
                    value: "input:folder".to_string(),
                    icon: ICON_FOLDER,
                    title: "Local folder".to_string(),
                    description: Some(path.clone()),
                    disabled: false,
                    setup_required: None,
                    action: RowAction::BrowseInputFolder {
                        machine_id: target.machine_id.clone(),
                        path,
                    },
                },
                AddProjectRow {
                    value: "input:github".to_string(),
                    icon: source_icon(AddProjectSourceId::Github),
                    title: "GitHub repository".to_string(),
                    description: Some(query.clone()),
                    disabled: false,
                    setup_required: None,
                    action: RowAction::ChooseDetectedClone {
                        machine_id: target.machine_id.clone(),
                        clone: clone.clone(),
                    },
                },
            ];
        }
        if d.is_browsing {
            if d.unsupported_windows_path || d.relative_path_needs_active_project {
                return Vec::new();
            }
            let mut rows = Vec::new();
            if let Some(suggested) = d
                .suggested_project_path
                .as_ref()
                .filter(|_| !d.is_clone_destination_step)
            {
                let is_project = d
                    .path_inspection
                    .as_ref()
                    .is_some_and(|inspection| inspection.project_id.is_some());
                rows.push(AddProjectRow {
                    value: "browse:project".to_string(),
                    icon: ICON_FOLDER_CHECK,
                    title: if is_project {
                        "Open existing project"
                    } else {
                        "Add repository root"
                    }
                    .to_string(),
                    description: Some(suggested.clone()),
                    disabled: false,
                    setup_required: None,
                    action: RowAction::AddSuggested(suggested.clone()),
                });
            }
            if d.is_windows_drive_root || can_navigate_up(&d.browse_directory_path) {
                rows.push(AddProjectRow {
                    value: BROWSE_UP_VALUE.to_string(),
                    icon: ICON_CORNER_LEFT_UP,
                    title: "..".to_string(),
                    description: None,
                    disabled: false,
                    setup_required: None,
                    action: RowAction::BrowseUp,
                });
            }
            for entry in &d.filtered.filtered {
                rows.push(AddProjectRow {
                    value: format!("browse:{}", entry.full_path),
                    icon: ICON_FOLDER,
                    title: entry.name.clone(),
                    description: None,
                    disabled: false,
                    setup_required: None,
                    action: RowAction::BrowseTo(entry.full_path.clone()),
                });
            }
            return rows;
        }
        match self.current_view() {
            Some(AddProjectView::Machines) => self
                .machines
                .iter()
                .filter(|machine| {
                    matches_filter(
                        &self.query,
                        &machine.label,
                        &[
                            machine.description.as_deref().unwrap_or(""),
                            &machine.machine_id,
                        ],
                    )
                })
                .map(|machine| AddProjectRow {
                    value: format!("machine:{}", machine.machine_id),
                    icon: if machine.machine_id == LOCAL_MACHINE_ID {
                        ICON_DEVICE_DESKTOP
                    } else {
                        ICON_SERVER
                    },
                    title: machine.label.clone(),
                    description: machine.description.clone(),
                    disabled: false,
                    setup_required: None,
                    action: RowAction::ChooseMachine(machine.machine_id.clone()),
                })
                .collect(),
            Some(AddProjectView::Sources { machine_id }) => self.build_source_rows(machine_id, d),
            _ => Vec::new(),
        }
    }

    fn build_source_rows(&self, machine_id: &str, d: &Derived) -> Vec<AddProjectRow> {
        let mut rows = Vec::new();
        /*
        CDXC:AddProject 2026-09-28 DECISION:
        User: on a native Windows (PowerShell) machine, Local folder should show the drives by default instead of `~/`, merged with External drives and other folders into one row. gxserver lists the home folder first in that drive list.
        SEE-ALSO: server/src/server/project_paths.rs (drive list), apps/desktop/src/app/remote_conn/clone_job_and_preview.rs (startsAtDriveList).
        */
        let base_directory = d
            .machine
            .as_ref()
            .and_then(|machine| machine.add_project_base_directory.as_deref())
            .map(str::trim)
            .filter(|directory| !directory.is_empty());
        // A Default projects folder (Settings > Projects) wins over the drive list.
        let starts_at_drive_list = base_directory.is_none()
            && d.machine
                .as_ref()
                .is_some_and(|machine| machine.starts_at_drive_list);
        if let Some(parent) = base_directory
            && matches_filter(
                &self.query,
                "New project folder",
                &["new", "create", "subfolder", "folder", "project"],
            )
        {
            rows.push(AddProjectRow {
                value: "source:new-project-folder".to_string(),
                icon: ICON_FOLDER_PLUS,
                title: "New project folder".to_string(),
                description: Some(format!("Make a subfolder in {parent} and add it")),
                disabled: false,
                setup_required: None,
                action: RowAction::NewProjectFolder {
                    machine_id: machine_id.to_string(),
                    parent: parent.to_string(),
                },
            });
        }
        let mut local_terms = vec!["browse", "directory", "disk"];
        if starts_at_drive_list {
            local_terms.extend(["drive", "external", "usb", "home"]);
        }
        if matches_filter(&self.query, "Local folder", &local_terms) {
            rows.push(AddProjectRow {
                value: "source:local".to_string(),
                icon: ICON_FOLDER,
                title: "Local folder".to_string(),
                description: Some(
                    if starts_at_drive_list {
                        "Browse your drives and home folder"
                    } else {
                        "Browse a folder on disk"
                    }
                    .to_string(),
                ),
                disabled: false,
                setup_required: None,
                action: RowAction::StartLocalBrowse {
                    machine_id: machine_id.to_string(),
                    start: starts_at_drive_list.then(|| ADD_PROJECT_ROOT_BROWSE_PATH.to_string()),
                },
            });
        }
        if !starts_at_drive_list
            && matches_filter(
                &self.query,
                "External drives and other folders",
                &["root", "volumes", "external", "drive", "disk", "usb", "/"],
            )
        {
            rows.push(AddProjectRow {
                value: "source:root".to_string(),
                icon: ICON_FOLDER_ROOT,
                title: "External drives and other folders".to_string(),
                description: Some("Browse from the root of the filesystem".to_string()),
                disabled: false,
                setup_required: None,
                action: RowAction::StartLocalBrowse {
                    machine_id: machine_id.to_string(),
                    start: Some(ADD_PROJECT_ROOT_BROWSE_PATH.to_string()),
                },
            });
        }
        for source in ordered_sources(&d.readiness) {
            let title = source_row_title(source);
            if !matches_filter(
                &self.query,
                &title,
                &[source.wire(), "clone", "repository", "git"],
            ) {
                continue;
            }
            let readiness = &d.readiness[source.index()];
            let install_status = self
                .tool_install
                .as_ref()
                .filter(|install| install.source == source && install.machine_id == machine_id)
                .and_then(|install| install.status_line());
            let description = if let Some(status) = install_status {
                status
            } else if readiness.ready {
                source_row_description(source)
            } else {
                readiness
                    .hint
                    .clone()
                    .unwrap_or_else(|| source_row_description(source))
            };
            rows.push(AddProjectRow {
                value: format!("source:{}", source.wire()),
                icon: source_icon(source),
                title,
                description: Some(description),
                disabled: !readiness.ready,
                setup_required: (!readiness.ready).then(|| {
                    (
                        source,
                        readiness.hint.clone().unwrap_or_default(),
                        readiness.install_tool.clone(),
                    )
                }),
                action: RowAction::StartClone {
                    machine_id: machine_id.to_string(),
                    source,
                },
            });
        }
        rows
    }
}
