//! The Add Project picker's labels, provider readiness, source order and placeholders: the Rust twin of packages/core-ui/add-project-modal/add-project-modal-logic.ts (deleted 2026-10-01).
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

/// `AddProjectSourceReadiness`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AddProjectSourceReadiness {
    pub(crate) hint: Option<String>,
    pub(crate) ready: bool,
    /// The managed tool that installs this provider's missing CLI with one click.
    pub(crate) install_tool: Option<String>,
    /// Run in terminal's id for a signed-out provider's sign-in (`glabAuthLogin`; GitHub signs in
    /// from the repository list instead), from `auth.runInTerminal`.
    pub(crate) run_in_terminal: Option<String>,
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
        run_in_terminal: None,
    };
    let mut readiness: AddProjectReadiness = [
        AddProjectSourceReadiness {
            hint: None,
            ready: true,
            install_tool: None,
            run_in_terminal: None,
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
                run_in_terminal: None,
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
                run_in_terminal: provider.sign_in_run_in_terminal.clone(),
            }
        } else {
            AddProjectSourceReadiness {
                hint: None,
                ready: true,
                install_tool: None,
                run_in_terminal: None,
            }
        };
    }
    readiness
}

/// `orderedAddProjectSources`: GitHub first, then Git URL, then the other providers with
/// ready ones first, each bucket by label.
///
/// CDXC:AddProject 2026-11-25 DECISION:
/// User: "pls make github the main option and first one here." GitHub leads the From row
/// whatever its readiness (an unready GitHub still shows its setup hint), Git URL follows,
/// and the remaining providers keep their ready-first, by-label order.
/// SEE-ALSO: window.rs (GitHub is the default source), apps/mobile/app src/addProject/sources.ts.
pub(crate) fn ordered_sources(readiness: &AddProjectReadiness) -> Vec<AddProjectSourceId> {
    let mut providers: Vec<AddProjectSourceId> = AddProjectSourceId::PROVIDERS
        .into_iter()
        .filter(|source| *source != AddProjectSourceId::Github)
        .collect();
    providers.sort_by(|left, right| {
        let left_ready = readiness[left.index()].ready;
        let right_ready = readiness[right.index()].ready;
        right_ready.cmp(&left_ready).then_with(|| {
            source_label(*left)
                .to_lowercase()
                .cmp(&source_label(*right).to_lowercase())
        })
    });
    let mut sources = vec![AddProjectSourceId::Github, AddProjectSourceId::Url];
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
