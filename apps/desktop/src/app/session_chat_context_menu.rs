use std::fs;
use std::path::{Path, PathBuf};

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

pub(crate) struct GpuiResolvedSessionChatFile {
    pub(crate) file_path: PathBuf,
    pub(crate) project_id: String,
    pub(crate) project_root: PathBuf,
}

pub(crate) enum GpuiSessionChatFileResolutionError {
    InvalidPath,
    ProjectUnavailable,
    NotFound,
    NotFile,
}

impl GhostexGpuiApp {
    pub(crate) fn resolve_session_chat_file(
        &mut self,
        session_id: TerminalSessionId,
        path: &str,
    ) -> Result<GpuiResolvedSessionChatFile, GpuiSessionChatFileResolutionError> {
        let trimmed = path.trim();
        if trimmed.is_empty() || trimmed.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS {
            return Err(GpuiSessionChatFileResolutionError::InvalidPath);
        }

        let Some(session_key) = self.local_workspace_key_for_shell_session(session_id) else {
            return Err(GpuiSessionChatFileResolutionError::ProjectUnavailable);
        };
        let project_root = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .filter(|snapshot| {
                snapshot
                    .active_project_id
                    .as_ref()
                    .is_some_and(|project_id| project_id.0 == session_key.project_id)
            })
            .and_then(|snapshot| snapshot.in_memory_project_path.clone())
            .or_else(|| {
                self.extension_projects
                    .get(&session_key.project_id)
                    .and_then(|project| project.path.as_deref())
                    .map(PathBuf::from)
            })
            .ok_or(GpuiSessionChatFileResolutionError::ProjectUnavailable)?;
        let project_root = fs::canonicalize(project_root)
            .map_err(|_| GpuiSessionChatFileResolutionError::ProjectUnavailable)?;
        if !fs::metadata(&project_root).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(GpuiSessionChatFileResolutionError::ProjectUnavailable);
        }

        let explicit_candidate = if Path::new(trimmed).is_absolute() {
            Some(PathBuf::from(trimmed))
        } else if trimmed == "~" {
            Some(home_dir())
        } else if let Some(relative) = trimmed
            .strip_prefix("~/")
            .or_else(|| trimmed.strip_prefix("~\\"))
        {
            Some(home_dir().join(relative))
        } else {
            None
        };

        let mut candidates = Vec::with_capacity(2);
        if let Some(candidate) = explicit_candidate {
            candidates.push((candidate, false));
        } else {
            candidates.push((project_root.join(trimmed), true));
            let working_directory = self
                .agents_terminal_runtime_sessions
                .runtime_session_id_for_shell_session(session_id)
                .and_then(|runtime_session_id| {
                    self.agents_terminal_runtime_osc_states
                        .get(&runtime_session_id)
                })
                .and_then(|state| state.pwd.as_deref())
                .map(str::trim)
                .filter(|working_directory| !working_directory.is_empty())
                .map(PathBuf::from)
                .filter(|working_directory| working_directory.is_absolute())
                .and_then(|working_directory| fs::canonicalize(working_directory).ok())
                .filter(|working_directory| working_directory.starts_with(&project_root));
            if let Some(working_directory) = working_directory {
                let cwd_candidate = working_directory.join(trimmed);
                if candidates
                    .first()
                    .is_none_or(|(candidate, _)| candidate != &cwd_candidate)
                {
                    candidates.push((cwd_candidate, true));
                }
            }
        }

        let mut found_non_file = false;
        for (candidate, must_stay_in_project) in candidates {
            let Ok(file_path) = fs::canonicalize(candidate) else {
                continue;
            };
            if must_stay_in_project && !file_path.starts_with(&project_root) {
                continue;
            }
            if !fs::metadata(&file_path).is_ok_and(|metadata| metadata.is_file()) {
                found_non_file = true;
                continue;
            }
            return Ok(GpuiResolvedSessionChatFile {
                file_path,
                project_id: session_key.project_id,
                project_root,
            });
        }

        Err(if found_non_file {
            GpuiSessionChatFileResolutionError::NotFile
        } else {
            GpuiSessionChatFileResolutionError::NotFound
        })
    }

    pub(crate) fn locate_session_chat_file(
        &mut self,
        session_id: TerminalSessionId,
        path: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let file_path = match self.resolve_session_chat_file(session_id, path) {
            Ok(resolved) => resolved.file_path,
            Err(error) => {
                let description = match error {
                    GpuiSessionChatFileResolutionError::InvalidPath => "That file path is invalid.",
                    GpuiSessionChatFileResolutionError::ProjectUnavailable => {
                        "That session's project folder is unavailable."
                    }
                    GpuiSessionChatFileResolutionError::NotFound => "That file could not be found.",
                    GpuiSessionChatFileResolutionError::NotFile => "That path is not a file.",
                };
                self.report_session_chat_file_locate_failure(description, cx);
                return;
            }
        };
        if let Err(message) = gpui_reveal_path_in_finder(&file_path) {
            self.report_session_chat_file_locate_failure(&message, cx);
        }
    }

    fn report_session_chat_file_locate_failure(
        &mut self,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-session-chat-file-locate-failed".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("warning")),
                title: "Could not locate file".to_string(),
                description: Some(description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }
}
