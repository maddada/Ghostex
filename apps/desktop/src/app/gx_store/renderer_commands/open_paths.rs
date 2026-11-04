//! `ghostex open` and `ghostex edit` (the `openPaths` renderer command): where each path opens.
//!
//! A folder becomes (or focuses) a project, as the operating system's open-folder request does. A
//! file goes where its verb and Settings > File opening say, and a file that is in no project opens
//! in the project on screen instead of becoming a project of its own.
//!
//! CDXC:OsIntegration 2026-10-26 DECISION:
//! User: `ghostex open` follows every Settings > File opening row for its file's type ("HTML, Markdown and images", "Other files", and Videos and Audio): Files opens it in Files, VS Code in the VS Code view, an editor app or the system app there. This supersedes the 2026-10-25 rule that only HTML, Markdown and images followed the setting while every other file opened in VS Code. `ghostex edit` always opens in VS Code. A file outside every project opens in the project currently on screen (its Files or VS Code view) and creates no project; a file inside a project opens in that project.

use std::path::{Path, PathBuf};

use ghostex_gx_core::{OpenPathTarget, OpenPathsMode};
use gpui::Window;
use serde_json::{Map, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::{
    gpui_os_integration_git_root_for_path, gpui_os_integration_project_root_for_path,
};
use crate::app::model::{PendingSourceFileOpen, PendingSourceFileOpenOrigin, TitlebarMode};
use crate::app::session_chat_context_menu::GpuiResolvedSessionChatFile;
use crate::shared_settings::SharedFileOpenTarget;

/// A local project's folder: the path it was registered with (what Code and the sidebar know it
/// by) and that path with symlinks resolved, so a file named either way is found in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CliProjectFolder {
    pub(crate) project_id: String,
    pub(crate) path: PathBuf,
    pub(crate) root: PathBuf,
}

/// Where a file named on the command line opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliFileRoute {
    /// Settings > File opening decides: Files, an editor app or the system app.
    FileOpening,
    /// The VS Code view of the project that holds the file.
    ProjectCode,
    /// The VS Code view of the project on screen, because no project holds the file.
    ActiveProjectCode,
}

/// `opens_in_code` says File opening sends the file to the VS Code view, which takes the Code
/// routes below because they carry the line and column.
pub(crate) fn cli_file_route(
    mode: OpenPathsMode,
    opens_in_code: bool,
    in_project: bool,
) -> CliFileRoute {
    if mode == OpenPathsMode::Open && !opens_in_code {
        CliFileRoute::FileOpening
    } else if in_project {
        CliFileRoute::ProjectCode
    } else {
        CliFileRoute::ActiveProjectCode
    }
}

/// The project whose folder holds `file` (given as named, and with symlinks resolved); the deepest
/// one when projects nest. A file in a git repository belongs only to a project at or inside that
/// repository, so a project made of a parent folder (the home folder, `~/dev`) never claims a
/// repository that is not a project of its own.
///
/// CDXC:OsIntegration 2026-10-25 WHY:
/// The user keeps their home folder open as a project, so "the deepest project above the file" sent `ghostex edit ~/dev/_scratch/codex-smoke/README.md` (its own repository, no project) to the home project instead of the project on screen.
pub(crate) fn project_holding_path<'a>(
    file: &Path,
    file_resolved: &Path,
    repository: Option<(&Path, &Path)>,
    projects: &'a [CliProjectFolder],
) -> Option<&'a CliProjectFolder> {
    projects
        .iter()
        .filter(|project| {
            !project.path.as_os_str().is_empty()
                && (file.starts_with(&project.path) || file_resolved.starts_with(&project.root))
                && repository.is_none_or(|(repo, repo_resolved)| {
                    project.path.starts_with(repo) || project.root.starts_with(repo_resolved)
                })
        })
        .max_by_key(|project| project.root.components().count())
}

/// The first file named, with where it goes.
struct CliFileOpen {
    target: OpenPathTarget,
    file_path: PathBuf,
    route: CliFileRoute,
    /// The project that holds it, when one does.
    project: Option<CliProjectFolder>,
    /// The file's git root (or folder) is itself a project: the operating system's open-file
    /// request already lands it there.
    os_route: bool,
}

struct CliOpenPlan {
    /// Folders, missing paths and files whose project the OS open request reaches.
    os_paths: Vec<PathBuf>,
    file: Option<CliFileOpen>,
}

fn plan_cli_open(
    mode: OpenPathsMode,
    targets: Vec<OpenPathTarget>,
    projects: Vec<(String, PathBuf)>,
) -> CliOpenPlan {
    let projects: Vec<CliProjectFolder> = projects
        .into_iter()
        .map(|(project_id, path)| CliProjectFolder {
            root: std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone()),
            project_id,
            path,
        })
        .collect();
    let mut plan = CliOpenPlan {
        os_paths: Vec::new(),
        file: None,
    };
    for target in targets {
        let path = PathBuf::from(&target.path);
        if !path.is_file() {
            plan.os_paths.push(path);
            continue;
        }
        let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let repository = path
            .parent()
            .and_then(gpui_os_integration_git_root_for_path)
            .map(|repo| {
                let repo_resolved = std::fs::canonicalize(&repo).unwrap_or_else(|_| repo.clone());
                (repo, repo_resolved)
            });
        let project = project_holding_path(
            &path,
            &resolved,
            repository
                .as_ref()
                .map(|(repo, resolved)| (repo.as_path(), resolved.as_path())),
            &projects,
        )
        .cloned();
        let os_route = gpui_os_integration_project_root_for_path(&path).is_some_and(|root| {
            let root_resolved = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
            projects
                .iter()
                .any(|project| project.path == root || project.root == root_resolved)
        });
        if plan.file.is_some() {
            // Only the first file is shown; the others still bring their project forward, but a
            // file in no project never adds one.
            if os_route {
                plan.os_paths.push(path);
            }
            continue;
        }
        let opens_in_code = mode == OpenPathsMode::Open
            && crate::app::native_docs::entry::local_file_open_target(&path)
                == SharedFileOpenTarget::Code;
        let route = cli_file_route(mode, opens_in_code, project.is_some());
        if route == CliFileRoute::ProjectCode && os_route {
            plan.os_paths.push(path.clone());
        }
        plan.file = Some(CliFileOpen {
            target,
            file_path: if project.is_some() { path } else { resolved },
            route,
            project,
            os_route,
        });
    }
    plan
}

impl GhostexGpuiApp {
    /// `openPaths`: every target resolved off the main thread, then opened where it belongs.
    pub(super) fn gx_store_open_cli_paths(
        &mut self,
        mode: OpenPathsMode,
        targets: Vec<OpenPathTarget>,
        cx: &mut gpui::Context<Self>,
    ) {
        let projects = self.gx_store_local_project_folders();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let plan = background
                .spawn(async move { plan_cli_open(mode, targets, projects) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.perform_cli_open_plan(plan, window, cx);
            });
        })
        .detach();
    }

    fn gx_store_local_project_folders(&self) -> Vec<(String, PathBuf)> {
        let Some(machine) = self
            .gx_store
            .core
            .presentation()
            .loaded(&ghostex_gx_core::MachineId::Local)
        else {
            return Vec::new();
        };
        machine
            .projects()
            .iter()
            .filter_map(|project| {
                let path = project.path.as_deref()?.trim();
                (!path.is_empty()).then(|| (project.project_id.clone(), PathBuf::from(path)))
            })
            .collect()
    }

    fn perform_cli_open_plan(
        &mut self,
        plan: CliOpenPlan,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let CliOpenPlan { os_paths, file } = plan;
        let Some(file) = file else {
            if !os_paths.is_empty() {
                self.open_gpui_os_integration_paths(os_paths, window, cx);
            }
            return;
        };
        match file.route {
            CliFileRoute::FileOpening => {
                match &file.project {
                    // Quick Access's open: the project comes forward first when the file opens
                    // in Files, and an editor or the system app opens it as it is.
                    Some(project) => {
                        let relative = file
                            .file_path
                            .strip_prefix(&project.path)
                            .map(|relative| (project.path.clone(), relative.to_path_buf()))
                            .or_else(|_| {
                                std::fs::canonicalize(&file.file_path)
                                    .unwrap_or_else(|_| file.file_path.clone())
                                    .strip_prefix(&project.root)
                                    .map(|relative| (project.root.clone(), relative.to_path_buf()))
                            });
                        if let Ok((root, relative)) = relative {
                            let mut command = Map::new();
                            command.insert("projectId".into(), json!(project.project_id));
                            command.insert(
                                "path".into(),
                                json!(relative.to_string_lossy().replace('\\', "/")),
                            );
                            command.insert("rootPath".into(), json!(root.to_string_lossy()));
                            self.open_quick_access_project_file(&command, window, cx);
                        }
                    }
                    None => self.open_cli_file_in_active_project(&file, window, cx),
                }
                if !os_paths.is_empty() {
                    self.open_gpui_os_integration_paths(os_paths, window, cx);
                }
            }
            CliFileRoute::ProjectCode if file.os_route => {
                let Some(project_path) = gpui_os_integration_project_root_for_path(&file.file_path)
                else {
                    return;
                };
                self.pending_source_file_open =
                    Some(Self::cli_pending_source_open(&file, project_path));
                self.open_gpui_os_integration_paths(os_paths, window, cx);
            }
            CliFileRoute::ProjectCode => {
                if !os_paths.is_empty() {
                    self.open_gpui_os_integration_paths(os_paths, window, cx);
                }
                let Some(project) = file.project.clone() else {
                    return;
                };
                self.open_cli_file_in_project_code_view(&file, project, cx);
            }
            CliFileRoute::ActiveProjectCode => {
                if !os_paths.is_empty() {
                    self.open_gpui_os_integration_paths(os_paths, window, cx);
                }
                self.open_cli_file_in_active_project_code_view(&file, window, cx);
            }
        }
    }

    fn cli_pending_source_open(file: &CliFileOpen, project_path: PathBuf) -> PendingSourceFileOpen {
        PendingSourceFileOpen {
            column: file.target.column,
            file_path: file.file_path.clone(),
            line: file.target.line,
            origin: PendingSourceFileOpenOrigin::SessionChat,
            project_path,
            remote_target: None,
            remote_working_directory: None,
        }
    }

    /// A file of a project whose git root is not itself a project (a project made of a subfolder of
    /// a repository): that project's VS Code view, without registering the repository.
    fn open_cli_file_in_project_code_view(
        &mut self,
        file: &CliFileOpen,
        project: CliProjectFolder,
        cx: &mut gpui::Context<Self>,
    ) {
        if crate::app::helpers::gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source) {
            self.copy_path_for_disabled_project_workarea(
                &file.file_path.to_string_lossy(),
                "VS Code",
                cx,
            );
            return;
        }
        self.pending_source_file_open = Some(Self::cli_pending_source_open(file, project.path));
        // Lands now when the project is already on screen, else once the activation does
        // (`land_pending_source_file_open_on_source_mode`).
        if !self.gx_store_activate_project(&project.project_id, cx) {
            self.pending_source_file_open = None;
            self.report_session_chat_file_open_failure("That project could not be opened.", cx);
            return;
        }
        self.defer_in_main_window(cx, |this, window, cx| {
            this.land_pending_source_file_open_on_source_mode(window, cx);
        });
    }

    /// A file in no project, in the VS Code view of the project on screen.
    fn open_cli_file_in_active_project_code_view(
        &mut self,
        file: &CliFileOpen,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let path_text = file.file_path.to_string_lossy().to_string();
        if crate::app::helpers::gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
            || !self.titlebar_mode_available(TitlebarMode::Source)
        {
            self.copy_path_for_disabled_project_workarea(&path_text, "VS Code", cx);
            return;
        }
        if let Some(reason) = self.embedded_code_editor_unavailable_reason() {
            self.report_session_chat_file_open_failure(reason, cx);
            return;
        }
        let Some(project_path) = self.active_local_project_path() else {
            self.report_session_chat_file_open_failure(
                "No project is open to show this file in VS Code view.",
                cx,
            );
            return;
        };
        self.report_session_chat_file_opening("VS Code view", &file.file_path, cx);
        self.pending_source_file_open = Some(Self::cli_pending_source_open(file, project_path));
        self.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Source, cx);
        self.focus_project_editor_surface(TitlebarMode::Source, window, cx);
    }

    /// A file in no project, where File opening says, shown by the project on screen (Files lists it
    /// the way it lists a chat link to a file outside the project).
    fn open_cli_file_in_active_project(
        &mut self,
        file: &CliFileOpen,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let project_id = self.gpui_daemon_sessions_active_project_id();
        let (Some(project_id), Some(project_path)) = (project_id, self.active_local_project_path())
        else {
            self.report_session_chat_file_open_failure("No project is open to show this file.", cx);
            return;
        };
        let project_root =
            std::fs::canonicalize(&project_path).unwrap_or_else(|_| project_path.clone());
        self.open_resolved_project_file(
            GpuiResolvedSessionChatFile {
                file_path: file.file_path.clone(),
                is_directory: false,
                project_id,
                project_path,
                project_root,
            },
            file.target.line,
            file.target.column,
            None,
            None,
            window,
            cx,
        );
    }

    fn active_local_project_path(&self) -> Option<PathBuf> {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .filter(|snapshot| !snapshot.is_quick_projectless)
            .and_then(|snapshot| snapshot.in_memory_project_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, path: &str) -> CliProjectFolder {
        CliProjectFolder {
            project_id: id.to_string(),
            path: PathBuf::from(path),
            root: PathBuf::from(path),
        }
    }

    #[test]
    fn open_follows_file_opening_for_every_file_it_does_not_send_to_code() {
        use CliFileRoute::*;
        use OpenPathsMode::*;
        // Any file whose File opening row says Files, an editor app or the system app ("Other
        // files" for greet.py and config.json, "HTML, Markdown and images" for page.html).
        assert_eq!(cli_file_route(Open, false, true), FileOpening);
        assert_eq!(cli_file_route(Open, false, false), FileOpening);
        // Its row says VS Code: the project's VS Code view, or the one on screen.
        assert_eq!(cli_file_route(Open, true, true), ProjectCode);
        assert_eq!(cli_file_route(Open, true, false), ActiveProjectCode);
    }

    #[test]
    fn edit_always_opens_in_code_and_never_needs_a_project_of_its_own() {
        use CliFileRoute::*;
        use OpenPathsMode::*;
        assert_eq!(cli_file_route(Edit, false, true), ProjectCode);
        assert_eq!(cli_file_route(Edit, false, false), ActiveProjectCode);
        assert_eq!(cli_file_route(Edit, true, true), ProjectCode);
        assert_eq!(cli_file_route(Edit, true, false), ActiveProjectCode);
    }

    #[test]
    fn a_file_belongs_to_the_deepest_project_that_holds_it() {
        let projects = vec![
            folder("repo", "/Users/me/dev/repo"),
            folder("sub", "/Users/me/dev/repo/packages/sub"),
            folder("other", "/Users/me/dev/repo-other"),
        ];
        let holder = |file: &str| {
            project_holding_path(Path::new(file), Path::new(file), None, &projects)
                .map(|project| project.project_id.as_str())
        };
        assert_eq!(holder("/Users/me/dev/repo/README.md"), Some("repo"));
        assert_eq!(holder("/Users/me/dev/repo/packages/sub/a.rs"), Some("sub"));
        assert_eq!(holder("/Users/me/dev/repo-other/a.rs"), Some("other"));
        assert_eq!(holder("/Users/me/dev/_scratch/codex-smoke/README.md"), None);
    }

    #[test]
    fn a_parent_folder_project_does_not_claim_a_repository_below_it() {
        let projects = vec![
            folder("home", "/Users/me"),
            folder("repo", "/Users/me/dev/repo"),
            folder("sub", "/Users/me/dev/mono/packages/sub"),
        ];
        let holder = |file: &str, repo: &str| {
            let repo = Path::new(repo);
            project_holding_path(
                Path::new(file),
                Path::new(file),
                Some((repo, repo)),
                &projects,
            )
            .map(|project| project.project_id.as_str())
        };
        // A repository that is no project: nobody holds its files.
        assert_eq!(
            holder(
                "/Users/me/dev/_scratch/codex-smoke/README.md",
                "/Users/me/dev/_scratch/codex-smoke"
            ),
            None
        );
        // The repository is a project, or a project is a subfolder of it.
        assert_eq!(
            holder("/Users/me/dev/repo/README.md", "/Users/me/dev/repo"),
            Some("repo")
        );
        assert_eq!(
            holder("/Users/me/dev/mono/packages/sub/a.rs", "/Users/me/dev/mono"),
            Some("sub")
        );
        // Outside any repository the home project still holds a loose file.
        assert_eq!(
            project_holding_path(
                Path::new("/Users/me/notes.md"),
                Path::new("/Users/me/notes.md"),
                None,
                &projects
            )
            .map(|project| project.project_id.as_str()),
            Some("home")
        );
    }

    #[test]
    fn a_file_named_through_a_symlink_still_finds_its_project() {
        let projects = vec![CliProjectFolder {
            project_id: "linked".to_string(),
            path: PathBuf::from("/Users/me/link"),
            root: PathBuf::from("/Volumes/data/real"),
        }];
        let found = project_holding_path(
            Path::new("/Volumes/data/real/notes.md"),
            Path::new("/Volumes/data/real/notes.md"),
            None,
            &projects,
        );
        assert_eq!(found.map(|p| p.project_id.as_str()), Some("linked"));
    }
}
