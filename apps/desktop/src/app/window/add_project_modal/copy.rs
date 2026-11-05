//! The Add Project dialog's labels, provider readiness, source order, placeholders and
//! empty-state copy: the Rust twin of packages/core-ui/add-project-modal/add-project-modal-logic.ts (deleted 2026-10-01).
use super::model::{
    AddProjectMachineOption, AddProjectProviderAuthStatus, AddProjectSourceControlDiscovery,
    AddProjectSourceId,
};

pub(crate) const ADD_PROJECT_DEFAULT_BROWSE_PATH: &str = "~/";

/*
CDXC:AddProject 2026-08-19:
Home is the default starting point, but external volumes and system folders live outside it
(`/Volumes` on macOS, `/mnt` and `/media` on Linux), and the browser can only walk down from
wherever it starts. The root entry gives that whole half of the filesystem a starting point
instead of making the user type the path. On Windows machines the daemon answers `/` with its
drive list; entries carry the host's full drive paths so the client never invents them.
*/
pub(crate) const ADD_PROJECT_ROOT_BROWSE_PATH: &str = "/";

const PROVIDER_UNAVAILABLE_HINT: &str =
    "Provider status unavailable. Open Settings -> Source Control and rescan.";

pub(crate) fn source_label(source: AddProjectSourceId) -> &'static str {
    match source {
        AddProjectSourceId::AzureDevops => "Azure DevOps",
        AddProjectSourceId::Bitbucket => "Bitbucket",
        AddProjectSourceId::Github => "GitHub",
        AddProjectSourceId::Gitlab => "GitLab",
        AddProjectSourceId::Url => "Git URL",
    }
}

fn source_path_hint(source: AddProjectSourceId) -> &'static str {
    match source {
        AddProjectSourceId::AzureDevops => "project/repository",
        AddProjectSourceId::Bitbucket => "workspace/repository",
        AddProjectSourceId::Github => "owner/repo",
        AddProjectSourceId::Gitlab => "group/project",
        AddProjectSourceId::Url => "URL",
    }
}

pub(crate) fn source_row_title(source: AddProjectSourceId) -> String {
    match source {
        AddProjectSourceId::Url => "Git URL".to_string(),
        _ => format!("{} repository", source_label(source)),
    }
}

pub(crate) fn source_row_description(source: AddProjectSourceId) -> String {
    match source {
        AddProjectSourceId::Url => "Clone from a remote URL".to_string(),
        _ => format!(
            "Clone {} {}",
            source_label(source),
            source_path_hint(source)
        ),
    }
}

/// `AddProjectSourceReadiness`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AddProjectSourceReadiness {
    pub(crate) hint: Option<String>,
    pub(crate) ready: bool,
    /// The managed tool that installs this provider's missing CLI with one click.
    pub(crate) install_tool: Option<String>,
}

/// Readiness per source, indexed by [`AddProjectSourceId::index`].
pub(crate) type AddProjectReadiness = [AddProjectSourceReadiness; 5];

fn trimmed_non_empty(value: Option<&String>) -> Option<String> {
    value
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// `buildAddProjectSourceReadiness`: `url` is always ready, every provider defaults to
/// unavailable, and an `unknown` auth status still counts as ready.
pub(crate) fn build_source_readiness(
    discovery: Option<&AddProjectSourceControlDiscovery>,
) -> AddProjectReadiness {
    let unavailable = AddProjectSourceReadiness {
        hint: Some(PROVIDER_UNAVAILABLE_HINT.to_string()),
        ready: false,
        install_tool: None,
    };
    let mut readiness: AddProjectReadiness = [
        AddProjectSourceReadiness {
            hint: None,
            ready: true,
            install_tool: None,
        },
        unavailable.clone(),
        unavailable.clone(),
        unavailable.clone(),
        unavailable,
    ];
    let Some(discovery) = discovery else {
        return readiness;
    };
    for source in AddProjectSourceId::PROVIDERS {
        let Some(provider) = discovery
            .providers
            .iter()
            .find(|entry| entry.provider == Some(source))
        else {
            continue;
        };
        readiness[source.index()] = if !provider.available {
            AddProjectSourceReadiness {
                hint: Some(
                    trimmed_non_empty(provider.install_hint.as_ref())
                        .unwrap_or_else(|| PROVIDER_UNAVAILABLE_HINT.to_string()),
                ),
                ready: false,
                install_tool: provider.install_tool.clone(),
            }
        } else if let Some((AddProjectProviderAuthStatus::Unauthenticated, detail)) = &provider.auth
        {
            AddProjectSourceReadiness {
                hint: Some(trimmed_non_empty(detail.as_ref()).unwrap_or_else(|| {
                    format!(
                        "{} is not authenticated. Open Settings -> Source Control for setup guidance.",
                        source_label(source)
                    )
                })),
                ready: false,
                install_tool: None,
            }
        } else {
            AddProjectSourceReadiness {
                hint: None,
                ready: true,
                install_tool: None,
            }
        };
    }
    readiness
}

/// `orderedAddProjectSources`: Git URL first, then ready providers, each bucket by label.
pub(crate) fn ordered_sources(readiness: &AddProjectReadiness) -> Vec<AddProjectSourceId> {
    let mut providers = AddProjectSourceId::PROVIDERS.to_vec();
    providers.sort_by(|left, right| {
        let left_ready = readiness[left.index()].ready;
        let right_ready = readiness[right.index()].ready;
        right_ready.cmp(&left_ready).then_with(|| {
            source_label(*left)
                .to_lowercase()
                .cmp(&source_label(*right).to_lowercase())
        })
    });
    let mut sources = vec![AddProjectSourceId::Url];
    sources.extend(providers);
    sources
}

pub(crate) fn repository_placeholder(source: AddProjectSourceId) -> String {
    match source {
        AddProjectSourceId::Url => "Enter repository, URL, or clone command".to_string(),
        AddProjectSourceId::Github => "Enter GitHub repository, URL, or clone command".to_string(),
        _ => format!(
            "Enter {} repository ({})",
            source_label(source),
            source_path_hint(source)
        ),
    }
}

pub(crate) fn repository_action_label(source: AddProjectSourceId) -> &'static str {
    match source {
        AddProjectSourceId::Github | AddProjectSourceId::Gitlab => "Lookup",
        _ => "Continue",
    }
}

pub(crate) fn path_placeholder(is_submenu: bool) -> &'static str {
    if is_submenu {
        "Enter path (e.g. ~/projects/my-app)"
    } else {
        "Enter project path, GitHub repository, or clone URL"
    }
}

pub(crate) fn initial_browse_query(machine: Option<&AddProjectMachineOption>) -> String {
    machine
        .and_then(|machine| machine.add_project_base_directory.as_deref())
        .map(str::trim)
        .filter(|directory| !directory.is_empty())
        .unwrap_or(ADD_PROJECT_DEFAULT_BROWSE_PATH)
        .to_string()
}

/// CDXC:AddProject 2026-10-09 DECISION:
/// User: "when i have powershell selected don't default to ~/ you should default to showing all the drives instead please!" The clone destination step on a native Windows (PowerShell) machine opens at gxserver's drive list, like Local folder does; WSL and other hosts keep their home folder, and a configured base directory still wins.
/// SEE-ALSO: derive.rs (the Local folder drive-list decision), server/src/server/project_paths.rs (the drive list).
pub(crate) fn initial_clone_destination_query(machine: Option<&AddProjectMachineOption>) -> String {
    let has_base_directory = machine
        .and_then(|machine| machine.add_project_base_directory.as_deref())
        .is_some_and(|directory| !directory.trim().is_empty());
    if !has_base_directory && machine.is_some_and(|machine| machine.starts_at_drive_list) {
        return ADD_PROJECT_ROOT_BROWSE_PATH.to_string();
    }
    initial_browse_query(machine)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddProjectEmptyCloneStep {
    Repository,
    Destination,
}

pub(crate) struct AddProjectEmptyStateInput {
    pub(crate) clone_step: Option<AddProjectEmptyCloneStep>,
    pub(crate) clone_source: Option<AddProjectSourceId>,
    pub(crate) is_loading_machines: bool,
    pub(crate) has_machines: bool,
    pub(crate) relative_path_needs_active_project: bool,
    pub(crate) unsupported_windows_path: bool,
    pub(crate) will_create_project_path: bool,
}

/// `addProjectEmptyStateMessage`.
pub(crate) fn empty_state_message(input: &AddProjectEmptyStateInput) -> &'static str {
    if input.is_loading_machines {
        return "Loading machines...";
    }
    if !input.has_machines {
        return "No machine is available.";
    }
    if input.clone_step == Some(AddProjectEmptyCloneStep::Repository) {
        return match input.clone_source {
            Some(
                AddProjectSourceId::Url
                | AddProjectSourceId::Bitbucket
                | AddProjectSourceId::AzureDevops,
            ) => "Enter a repository, URL, or clone command and press Enter to continue.",
            Some(AddProjectSourceId::Github) => {
                "Enter owner/repo, a GitHub URL, or a clone command and press Enter to continue."
            }
            _ => "Enter a repository path and press Enter to look it up.",
        };
    }
    if input.clone_step == Some(AddProjectEmptyCloneStep::Destination) {
        return if input.will_create_project_path {
            "Press Enter to review this new clone destination."
        } else {
            "Choose a destination path and press Enter to review the clone."
        };
    }
    if input.unsupported_windows_path {
        return "Windows-style paths are only supported on Windows machines.";
    }
    if input.relative_path_needs_active_project {
        return "Relative paths require an active project.";
    }
    if input.will_create_project_path {
        return "Press Enter to create this folder and add it as a project.";
    }
    "No matching directories."
}

/*
CDXC:AddProject 2026-08-18:
The new-folder step reuses the dialog's single input, so the list area is where the pending
folder is spelled out in full. It names the parent the folder lands in, because the typed name
alone never shows that. `as_project`: the folder comes from New project folder and is added
as a project once it exists.
*/
pub(crate) fn new_folder_message(name: &str, parent_path: &str, as_project: bool) -> String {
    let name = name.trim();
    if name.is_empty() {
        return if as_project {
            format!("Name the new project folder to create in {parent_path}.")
        } else {
            format!("Name the new folder to create in {parent_path}.")
        };
    }
    if name.contains(['/', '\\']) {
        return "A folder name cannot contain a path separator.".to_string();
    }
    let path = format!("{}/{name}", parent_path.trim_end_matches(['/', '\\']));
    if as_project {
        format!("Press Enter to create {path} and add it as a project.")
    } else {
        format!("Press Enter to create {path}.")
    }
}

/// `matchesAddProjectFilter`: a case-insensitive substring match over a row's title and terms.
pub(crate) fn matches_filter(query: &str, title: &str, terms: &[&str]) -> bool {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }
    std::iter::once(title)
        .chain(terms.iter().copied())
        .any(|term| term.to_lowercase().contains(&normalized))
}
