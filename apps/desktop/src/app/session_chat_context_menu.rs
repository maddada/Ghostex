use std::fs;
use std::path::{Path, PathBuf};

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

mod remote_files;
pub(crate) use remote_files::open_remote_source_file;

pub(crate) struct GpuiResolvedSessionChatFile {
    pub(crate) file_path: PathBuf,
    pub(crate) is_directory: bool,
    pub(crate) project_id: String,
    /// CDXC:SessionChat 2026-09-15 WHY:
    /// Code identifies its workspace by the registered project path, including symlinks; using the canonical root leaves chat file opens waiting forever for a different workspace.
    pub(crate) project_path: PathBuf,
    pub(crate) project_root: PathBuf,
}

pub(crate) enum GpuiSessionChatFileResolutionError {
    InvalidPath,
    ProjectUnavailable,
    NotFound,
    NotFile,
}

pub(crate) struct GpuiSessionChatFileResolutionContext {
    project_id: String,
    project_path: PathBuf,
    working_directory: Option<PathBuf>,
}

fn local_session_chat_path(path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    if path.to_string_lossy().starts_with('/') && !path.is_absolute() {
        return windows_terminal_backend::windows_path_for_wsl_path(path).ok();
    }
    Some(path.to_path_buf())
}

impl GhostexGpuiApp {
    pub(crate) fn session_chat_file_resolution_context(
        &mut self,
        session_id: TerminalSessionId,
    ) -> Result<GpuiSessionChatFileResolutionContext, GpuiSessionChatFileResolutionError> {
        let Some(session_key) = self.local_workspace_key_for_shell_session(session_id) else {
            return Err(GpuiSessionChatFileResolutionError::ProjectUnavailable);
        };
        let project_path = self
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
            .map(PathBuf::from);
        Ok(GpuiSessionChatFileResolutionContext {
            project_id: session_key.project_id,
            project_path,
            working_directory,
        })
    }
}

pub(crate) fn resolve_session_chat_file(
    context: GpuiSessionChatFileResolutionContext,
    path: &str,
) -> Result<GpuiResolvedSessionChatFile, GpuiSessionChatFileResolutionError> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS {
        return Err(GpuiSessionChatFileResolutionError::InvalidPath);
    }

    let local_project_path = local_session_chat_path(&context.project_path)
        .ok_or(GpuiSessionChatFileResolutionError::ProjectUnavailable)?;
    let project_root = fs::canonicalize(&local_project_path)
        .map_err(|_| GpuiSessionChatFileResolutionError::ProjectUnavailable)?;
    if !fs::metadata(&project_root).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(GpuiSessionChatFileResolutionError::ProjectUnavailable);
    }

    let explicit_candidate = if Path::new(trimmed).is_absolute() || trimmed.starts_with('/') {
        // A link that cannot be translated (a POSIX path in a native PowerShell session) is a
        // missing file, so it falls back to copying the path like before.
        Some(
            local_session_chat_path(Path::new(trimmed))
                .ok_or(GpuiSessionChatFileResolutionError::NotFound)?,
        )
    } else if trimmed == "~" {
        Some(home_dir())
    } else {
        trimmed
            .strip_prefix("~/")
            .or_else(|| trimmed.strip_prefix("~\\"))
            .map(|relative| home_dir().join(relative))
    };

    let mut candidates = Vec::with_capacity(2);
    if let Some(candidate) = explicit_candidate {
        candidates.push((candidate, false));
    } else {
        candidates.push((project_root.join(trimmed), true));
        let working_directory = context
            .working_directory
            .as_deref()
            .filter(|working_directory| !working_directory.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .filter(|working_directory| {
                working_directory.is_absolute()
                    || working_directory.to_string_lossy().starts_with('/')
            })
            .and_then(|working_directory| local_session_chat_path(&working_directory))
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
        let Ok(metadata) = fs::metadata(&file_path) else {
            continue;
        };
        if !metadata.is_file() && !metadata.is_dir() {
            found_non_file = true;
            continue;
        }
        return Ok(GpuiResolvedSessionChatFile {
            file_path,
            is_directory: metadata.is_dir(),
            project_id: context.project_id,
            project_path: context.project_path,
            project_root,
        });
    }

    Err(if found_non_file {
        GpuiSessionChatFileResolutionError::NotFile
    } else {
        GpuiSessionChatFileResolutionError::NotFound
    })
}

fn session_chat_file_resolution_error_message(
    error: GpuiSessionChatFileResolutionError,
) -> &'static str {
    match error {
        GpuiSessionChatFileResolutionError::InvalidPath => "That file path is invalid.",
        GpuiSessionChatFileResolutionError::ProjectUnavailable => {
            "That session's project folder is unavailable."
        }
        GpuiSessionChatFileResolutionError::NotFound => "That file could not be found.",
        GpuiSessionChatFileResolutionError::NotFile => "That path is not a file.",
    }
}

impl GhostexGpuiApp {
    pub(crate) fn locate_session_chat_file(
        &mut self,
        session_id: TerminalSessionId,
        path: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let trimmed = path.trim();
        if trimmed.is_empty() || trimmed.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS {
            self.report_session_chat_file_locate_failure(
                session_chat_file_resolution_error_message(
                    GpuiSessionChatFileResolutionError::InvalidPath,
                ),
                cx,
            );
            return;
        }
        let context = match self.session_chat_file_resolution_context(session_id) {
            Ok(context) => context,
            Err(error) => {
                self.report_session_chat_file_locate_failure(
                    session_chat_file_resolution_error_message(error),
                    cx,
                );
                return;
            }
        };
        let path = trimmed.to_string();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let resolution = background
                .spawn(async move { resolve_session_chat_file(context, &path) })
                .await;
            let result = match resolution {
                Ok(resolved) => {
                    let file_path = resolved.file_path;
                    background
                        .spawn(async move { gpui_reveal_path_in_finder(&file_path) })
                        .await
                }
                Err(error) => Err(session_chat_file_resolution_error_message(error).to_string()),
            };
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    this.report_session_chat_file_locate_failure(&message, cx);
                });
            }
        })
        .detach();
    }

    fn report_session_chat_file_locate_failure(
        &mut self,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: "gpui-session-chat-file-locate-failed".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("warning")),
                title: "Could not locate file".to_string(),
                description: Some(description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }
}
