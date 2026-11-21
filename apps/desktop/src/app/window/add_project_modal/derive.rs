//! What the picker derives from its state on every render: the machine and platform, whether the
//! typed line is a folder or a repository, the open folder's listing and filter, the inspection of
//! the typed path, what Add project would add, and the columns' ancestor folders.
use super::copy::*;
use super::input::*;
use super::model::*;
use super::paths::*;
use super::window::*;

/// How many folders above the selected one get a column.
const ANCESTOR_COLUMNS: usize = 2;

impl GpuiAddProjectModalWindow {
    pub(super) fn current_machine(&self) -> Option<&AddProjectMachineOption> {
        self.machine_id.as_deref().and_then(|id| {
            self.machines
                .iter()
                .find(|machine| machine.machine_id == id)
        })
    }

    /// The current machine's places, once it answered.
    pub(super) fn current_places(&self) -> Option<&AddProjectPlaces> {
        self.machine_id
            .as_deref()
            .and_then(|id| self.places.get(id))
            .and_then(Option::as_ref)
    }

    pub(super) fn current_home(&self) -> Option<String> {
        self.current_places().map(|places| places.home.clone())
    }

    /// `path` with the machine's home folder written `~`, as a person types it.
    pub(super) fn tildify(&self, path: &str) -> String {
        let Some(home) = self.current_home() else {
            return path.to_string();
        };
        let home = home.trim_end_matches(['/', '\\']);
        if home.is_empty() || home.contains('\\') {
            return path.to_string();
        }
        if path == home {
            return "~".to_string();
        }
        match path.strip_prefix(home) {
            Some(rest) if rest.starts_with('/') => format!("~{rest}"),
            _ => path.to_string(),
        }
    }

    pub(super) fn derive(&self) -> Derived {
        let machine = self.current_machine().cloned();
        let machine_id = machine.as_ref().map(|machine| machine.machine_id.clone());
        // A connected host with an unknown platform is validated by that host, not by this OS.
        let platform = match &machine {
            Some(machine) => machine.platform.clone().unwrap_or_default(),
            None => self.client_platform.clone(),
        };
        let query = self.query.as_str();
        let browse_platform = if platform.is_empty() {
            "Win32"
        } else {
            &platform
        };
        let is_browsing = is_filesystem_browse_query(query, browse_platform);
        let (clone_detected, ambiguous, detected_error) = if is_browsing {
            (None, None, None)
        } else {
            match classify_add_project_input(query, &self.machines) {
                Some(DetectedProjectInput::Clone(clone)) => (Some(clone), None, None),
                Some(DetectedProjectInput::Ambiguous { query, clone }) => {
                    (None, Some((query, clone)), None)
                }
                Some(DetectedProjectInput::Error { message }) => (None, None, Some(message)),
                _ => (None, None, None),
            }
        };
        let browse_directory_path = if is_browsing {
            get_browse_directory_path(query)
        } else {
            String::new()
        };
        let browse_filter_query = if is_browsing && !has_trailing_path_separator(query) {
            get_browse_leaf_path_segment(query)
        } else {
            String::new()
        };
        let unsupported_windows_path =
            !platform.is_empty() && is_unsupported_windows_project_path(query.trim(), &platform);
        let relative_path_needs_active_project =
            is_explicit_relative_project_path(query.trim()) && self.active_project_cwd.is_none();
        let entries = self
            .browse_result
            .as_ref()
            .map(|result| result.entries.as_slice())
            .unwrap_or(&[]);
        let highlight_value = self.highlight.as_ref().map(|path| format!("browse:{path}"));
        let filtered =
            filter_browse_entries(entries, &browse_filter_query, highlight_value.as_deref());
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
        let is_drive_list = self
            .browse_result
            .as_ref()
            .is_some_and(|result| result.is_drive_list);
        let selected_folder = self
            .browse_result
            .as_ref()
            .filter(|result| !result.is_drive_list)
            .map(|result| trim_folder(&result.parent_path))
            .or_else(|| self.last_selected_folder.clone().filter(|_| is_browsing));
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
            && !query.trim().is_empty()
            && filtered.highlighted.is_none()
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
        let submit_action_label = if path_inspection
            .as_ref()
            .is_some_and(|inspection| inspection.project_id.is_some())
        {
            "Open project"
        } else if path_inspection
            .as_ref()
            .is_some_and(|inspection| inspection.git_root.is_some())
        {
            "Add repository root"
        } else if will_create_project_path {
            "Create & Add"
        } else {
            "Add project"
        };
        let readiness = build_source_readiness(
            machine_id
                .as_deref()
                .and_then(|id| self.discovery.get(id))
                .and_then(Option::as_ref),
        );
        Derived {
            machine,
            machine_id,
            platform,
            is_browsing,
            browse_directory_path,
            selected_folder,
            is_drive_list,
            unsupported_windows_path,
            relative_path_needs_active_project,
            clone_detected,
            ambiguous,
            detected_error,
            filtered,
            path_inspection,
            suggested_project_path,
            resolved_add_project_path,
            can_submit_browse_path,
            will_create_project_path,
            new_folder_parent_path,
            can_create_new_folder,
            submit_action_label,
            readiness,
        }
    }

    /// The folder above `folder` on this machine: `/` above a Windows drive (its drive list),
    /// nothing above a root.
    pub(super) fn parent_folder(&self, folder: &str, platform: &str) -> Option<String> {
        let trimmed = trim_folder(folder);
        if is_windows_drive_root(&ensure_browse_directory_path(&trimmed)) {
            return platform.starts_with("Win").then(|| "/".to_string());
        }
        get_browse_parent_path(&ensure_browse_directory_path(&trimmed))
            .map(|parent| trim_folder(&parent))
    }

    /// The ancestor folders that get a column, top first: up to two above the selected folder,
    /// never above the sidebar place the columns started from.
    pub(super) fn column_folders(&self, d: &Derived) -> Vec<String> {
        if self.suggested_mode {
            return Vec::new();
        }
        let Some(selected) = d.selected_folder.clone() else {
            return Vec::new();
        };
        let root = self.column_root.as_deref().map(trim_folder);
        if root.as_deref() == Some(selected.as_str()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut current = selected;
        while out.len() < ANCESTOR_COLUMNS {
            let Some(parent) = self.parent_folder(&current, &d.platform) else {
                break;
            };
            out.push(parent.clone());
            if root.as_deref() == Some(parent.as_str()) {
                break;
            }
            current = parent;
        }
        out.reverse();
        out
    }
}
