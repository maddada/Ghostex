//! Quick Access's Files tab: answering its searches and opening the file the user picked.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::Window;
use serde_json::{Map, Value};

use crate::app::helpers::*;
use crate::app::session_chat_context_menu::GpuiResolvedSessionChatFile;
use crate::*;

/// How long a picked file waits for its project to become the active one.
const PROJECT_ACTIVATION_POLL: Duration = Duration::from_millis(100);
const PROJECT_ACTIVATION_POLLS: usize = 50;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_project_files_command(
        &mut self,
        command_type: &str,
        command: &Map<String, Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            "requestProjectFiles" => self.gx_store_answer_project_files(command, cx),
            "openProjectFile" => self.open_quick_access_project_file(command, window, cx),
            _ => {}
        }
    }

    /// Opens a file picked in Quick Access where Settings > File opening says. Files and Code show
    /// the active project, so a file of another project makes its project active first; an editor
    /// app or the system app opens the file as it is. `ghostex open` uses it for a project's file
    /// (gx_store/renderer_commands/open_paths.rs).
    pub(crate) fn open_quick_access_project_file(
        &mut self,
        command: &Map<String, Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            command
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(project_id), Some(path), Some(root)) =
            (text("projectId"), text("path"), text("rootPath"))
        else {
            return;
        };
        // A project-relative path, as gxserver listed it: never absolute, never climbing out.
        if path.contains('\0')
            || path.starts_with('/')
            || path.starts_with('\\')
            || path.split(['/', '\\']).any(|segment| segment == "..")
            || path.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        {
            return;
        }
        let remote = match (text("machineId"), text("remoteProjectId")) {
            (Some(machine_id), Some(remote_project_id)) => Some(GpuiRemoteAttachSessionKey {
                remote_machine_id: machine_id,
                project_id: remote_project_id,
                session_id: String::new(),
            }),
            _ => None,
        };
        if remote.is_none() {
            let root = PathBuf::from(&root);
            let file = root.join(&path);
            match crate::app::native_docs::entry::local_file_open_target(&file) {
                shared_settings::SharedFileOpenTarget::Files
                | shared_settings::SharedFileOpenTarget::Code => {}
                // Nothing on screen has to change project first.
                _ => {
                    self.open_quick_access_local_file(project_id, root, &path, window, cx);
                    return;
                }
            }
        }
        if self.active_project_is(&project_id) {
            self.open_quick_access_file_in_active_project(
                project_id, root, path, remote, window, cx,
            );
            return;
        }
        self.gx_store_flush_local_selection(cx);
        if !self.gx_store_activate_project(&project_id, cx) {
            self.report_session_chat_file_open_failure("That project could not be opened.", cx);
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..PROJECT_ACTIVATION_POLLS {
                cx.background_executor()
                    .timer(PROJECT_ACTIVATION_POLL)
                    .await;
                let opened = this.update_in(cx, |app, window, cx| {
                    if !app.active_project_is(&project_id) {
                        return false;
                    }
                    app.open_quick_access_file_in_active_project(
                        project_id.clone(),
                        root.clone(),
                        path.clone(),
                        remote.clone(),
                        window,
                        cx,
                    );
                    true
                });
                if !matches!(opened, Ok(false)) {
                    return;
                }
            }
        })
        .detach();
    }

    fn active_project_is(&self, project_id: &str) -> bool {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .is_some_and(|active| active.0 == project_id)
    }

    fn open_quick_access_file_in_active_project(
        &mut self,
        project_id: String,
        root: String,
        path: String,
        remote: Option<GpuiRemoteAttachSessionKey>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match remote {
            Some(key) => {
                let windows_root = root.contains('\\') && !root.contains('/');
                let separator = if windows_root { "\\" } else { "/" };
                let relative = if windows_root {
                    path.replace('/', "\\")
                } else {
                    path
                };
                let absolute = format!(
                    "{}{separator}{relative}",
                    root.trim_end_matches(['/', '\\'])
                );
                self.open_remote_session_chat_file_for_key(
                    key, &absolute, None, None, None, window, cx,
                );
            }
            None => self.open_quick_access_local_file(
                project_id,
                PathBuf::from(root),
                &path,
                window,
                cx,
            ),
        }
    }

    fn open_quick_access_local_file(
        &mut self,
        project_id: String,
        project_path: PathBuf,
        path: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(project_root) = std::fs::canonicalize(&project_path) else {
            self.report_session_chat_file_open_failure("That project's folder is unavailable.", cx);
            return;
        };
        let file_path = match std::fs::canonicalize(project_root.join(Path::new(path))) {
            Ok(file_path) if file_path.starts_with(&project_root) && file_path.is_file() => {
                file_path
            }
            _ => {
                self.report_session_chat_file_open_failure(
                    "That file is no longer in the project.",
                    cx,
                );
                return;
            }
        };
        self.open_resolved_project_file(
            GpuiResolvedSessionChatFile {
                file_path,
                is_directory: false,
                project_id,
                project_path,
                project_root,
            },
            None,
            None,
            None,
            None,
            window,
            cx,
        );
    }
}
