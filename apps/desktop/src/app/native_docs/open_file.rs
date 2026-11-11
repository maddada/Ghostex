//! Open File: the hotkey and the files list button that bring up the Files search box, and
//! opening what was typed there (a file name, a project path, or the path of any file on this
//! computer) with Enter.

use std::path::{Path, PathBuf};

use gpui::{Context, Window};

use super::state::DocsEntryKind;
use crate::GhostexGpuiApp;
use crate::app::helpers::{
    authorize_manage_chat_file, gpui_remote_project_reference_from_project_id,
    manage_chat_file_is_address,
};
use crate::app::model::TitlebarMode;

/// A typed or pasted absolute path (`/…`, `~/…`, `file://…`, `C:\…`), quotes and `file://`
/// removed and `~` expanded; `None` for anything else.
pub(super) fn typed_absolute_path(text: &str) -> Option<PathBuf> {
    let text = text.trim().trim_matches(|c| c == '"' || c == '\'');
    let text = match text.strip_prefix("file://") {
        Some(url) => percent_encoding::percent_decode_str(url)
            .decode_utf8()
            .ok()?
            .into_owned(),
        None => text.to_string(),
    };
    if let Some(rest) = text.strip_prefix("~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        return Some(PathBuf::from(home).join(rest));
    }
    let path = PathBuf::from(&text);
    path.is_absolute().then_some(path)
}

impl GhostexGpuiApp {
    /// The Open File hotkey and button: Files on screen with its list showing and the search box
    /// focused, ready for a name or a pasted path.
    pub(crate) fn native_docs_open_file_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_mode != TitlebarMode::Manage {
            self.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
            self.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
            self.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
        }
        if self.native_docs.search.is_some() && self.native_docs.project.is_some() {
            self.native_docs_show_search(window, cx);
        } else {
            // Files has not drawn for this project yet; its next draw shows the search.
            self.native_docs.open_file_prompt = true;
        }
        cx.notify();
    }

    /// Enter in the files search box: a path opens that file; anything else opens the first match.
    pub(crate) fn native_docs_submit_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.native_docs.search_query.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Some(path) = typed_absolute_path(&text) {
            if self.native_docs_open_absolute_path(&path, cx) {
                self.native_docs_clear_search(window, cx);
            }
            return;
        }
        let relative = text
            .trim_start_matches("./")
            .trim_end_matches('/')
            .to_string();
        let project_file = self.native_docs.project.as_ref().and_then(|project| {
            let local =
                gpui_remote_project_reference_from_project_id(&project.project_id).is_none();
            let candidate = project.project_path.join(&relative);
            (local && relative.contains('/') && candidate.exists()).then(|| candidate.is_dir())
        });
        if let Some(is_directory) = project_file {
            self.native_docs_clear_search(window, cx);
            self.native_docs_show_project_path(&relative, is_directory, cx);
            return;
        }
        let first = (self.native_docs.search_results_query == self.native_docs_search_key())
            .then(|| self.native_docs.search_results.as_ref()?.first().cloned())
            .flatten();
        let Some(entry) = first else {
            return;
        };
        self.native_docs_clear_search(window, cx);
        self.native_docs_show_project_path(&entry.path, entry.kind == DocsEntryKind::Directory, cx);
    }

    /// Opens a file, or opens a folder in the tree, by its path in the project.
    fn native_docs_show_project_path(
        &mut self,
        path: &str,
        is_directory: bool,
        cx: &mut Context<Self>,
    ) {
        if is_directory {
            self.native_docs.expanded.insert(path.to_string());
            self.native_docs_list_folder(path, cx);
            self.native_docs_reveal_in_tree(path, cx);
            self.native_docs.reveal_request = Some(path.to_string());
            self.native_docs_notify(cx);
        } else {
            self.native_docs_open_external(path.to_string(), cx);
        }
    }

    /// Opens an absolute path: inside the project as a tree path, outside it (on this computer)
    /// through the same one-file grant chat links use. False, with a toast, when it can't open.
    ///
    /// CDXC:Docs 2026-09-27 DECISION:
    /// User: the Open File box accepts the path of any file on the computer, not only the project's, with suggestions from the project's files while typing. A file outside the project opens through its one-file grant (`authorize_manage_chat_file`), so Files still never lists folders outside the project.
    pub(crate) fn native_docs_open_absolute_path(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.native_docs.project.clone() else {
            return false;
        };
        let fail = |this: &mut Self, message: &str, cx: &mut Context<Self>| {
            this.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't open that path",
                message,
                cx,
            );
            false
        };
        if gpui_remote_project_reference_from_project_id(&project.project_id).is_some() {
            // The path names a file on the remote computer, which only the project's own folder
            // can reach.
            let root = project
                .project_path
                .to_string_lossy()
                .trim_end_matches('/')
                .to_string();
            let text = path.to_string_lossy();
            let Some(relative) = text.strip_prefix(&format!("{root}/")) else {
                return fail(
                    self,
                    "On a remote project, Files opens paths inside the project folder.",
                    cx,
                );
            };
            self.native_docs_show_project_path(relative, false, cx);
            return true;
        }
        let Ok(resolved) = std::fs::canonicalize(path) else {
            return fail(self, &format!("Nothing exists at {}.", path.display()), cx);
        };
        let root =
            std::fs::canonicalize(&project.project_path).unwrap_or(project.project_path.clone());
        if let Ok(relative) = resolved.strip_prefix(&root) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            if relative.is_empty() {
                return fail(self, "That is the project folder itself.", cx);
            }
            self.native_docs_show_project_path(&relative, resolved.is_dir(), cx);
            return true;
        }
        if resolved.is_dir() {
            return fail(self, "Files opens folders inside the project only.", cx);
        }
        match authorize_manage_chat_file(&project.project_id, &resolved) {
            Ok(routed) => {
                self.native_docs_open_external(routed, cx);
                true
            }
            Err(error) => fail(self, &error, cx),
        }
    }

    /// Whether `path` addresses a file opened from outside the project (it has no tree folders):
    /// absolute here, or a Windows drive or UNC path, which a remote Windows computer's chat link
    /// opens on any computer (`C:/dev/…` is not absolute on a Mac, and was opened as tree folders).
    pub(crate) fn native_docs_is_outside_file(path: &str) -> bool {
        manage_chat_file_is_address(path) || crate::app::helpers::gpui_is_windows_remote_path(path)
    }

    /// The path `path`'s images and page load from on the Docs resource origin: its own path for
    /// a project file, its granted folder's mount for an outside one.
    pub(crate) fn native_docs_resource_path(&self, path: &str) -> String {
        self.native_docs
            .project
            .as_ref()
            .filter(|_| Self::native_docs_is_outside_file(path))
            .and_then(|project| {
                crate::app::helpers::manage_chat_file_resource_address(&project.project_id, path)
            })
            .unwrap_or_else(|| path.to_string())
    }
}
