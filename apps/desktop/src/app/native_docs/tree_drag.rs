//! Moving files and folders by dragging them in the files list: onto a folder moves the item into
//! it, onto a file moves it beside that file (a top-level file's row and the empty space below the
//! rows mean the `docs` folder), like the former React Docs page did.

use gpui::{Context, IntoElement, Render, SharedString, Window};
use serde_json::json;

use super::files::parent_path;
use super::state::DocsEntryKind;
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;

/// The row being dragged, drawn under the pointer as a small pill with its icon and name.
#[derive(Clone)]
pub(crate) struct DocsTreeDrag {
    pub(crate) path: String,
    pub(crate) kind: DocsEntryKind,
    pub(crate) name: SharedString,
    pub(crate) icon: &'static str,
}

impl Render for DocsTreeDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crate::app::helpers::drag_ghost(
            self.name.clone(),
            Some(
                titlebar_svg_icon(
                    self.icon,
                    15.0,
                    crate::app::helpers::drag_ghost_text_color(),
                )
                .into_any_element(),
            ),
            None,
            1.0,
        )
    }
}

/// Where a drop on the empty space, or on a top-level file, puts the item (`MANAGE_DOCS_ROOT_PATH`).
pub(crate) const DOCS_ROOT: &str = "docs";

/// The folder a drop on `target` puts the item in: the folder itself, or a file's folder (the
/// `docs` folder for a top-level file). `dropDirectoryPathForManageEntry`.
pub(crate) fn drop_folder(target_path: &str, target_kind: DocsEntryKind) -> String {
    match target_kind {
        DocsEntryKind::Directory => target_path.to_string(),
        DocsEntryKind::File => match parent_path(target_path) {
            "" => DOCS_ROOT.to_string(),
            parent => parent.to_string(),
        },
    }
}

/// `path` moved into `folder`.
fn moved_path(path: &str, folder: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// Whether `drag` can move into `folder` (`canMoveManageEntryToDirectory`): not onto itself or its
/// own folder, a folder not into its own subtree, and no name already taken there. `entries` are
/// the listed paths and kinds.
pub(crate) fn can_move(
    entries: &[(String, DocsEntryKind)],
    drag: &DocsTreeDrag,
    folder: &str,
) -> bool {
    if drag.path == folder || parent_path(&drag.path) == folder {
        return false;
    }
    if drag.kind == DocsEntryKind::Directory && folder.starts_with(&format!("{}/", drag.path)) {
        return false;
    }
    if folder != DOCS_ROOT
        && !entries
            .iter()
            .any(|(path, kind)| path == folder && *kind == DocsEntryKind::Directory)
    {
        return false;
    }
    let next = moved_path(&drag.path, folder).to_lowercase();
    !entries
        .iter()
        .any(|(path, _)| *path != drag.path && path.to_lowercase() == next)
}

impl GhostexGpuiApp {
    /// The listed paths and kinds, for `can_move` while a row is being dragged.
    pub(crate) fn native_docs_drag_entries(&self) -> Vec<(String, DocsEntryKind)> {
        self.native_docs
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.kind))
            .collect()
    }

    /// Moves the dragged item into `folder`: the file bridge's `move`, then open files, drafts,
    /// notes and folders follow it.
    pub(crate) fn native_docs_drop_into(
        &mut self,
        drag: &DocsTreeDrag,
        folder: &str,
        cx: &mut Context<Self>,
    ) {
        if !can_move(&self.native_docs_drag_entries(), drag, folder) {
            return;
        }
        let prefix = format!("{}/", drag.path);
        let busy = self.native_docs.documents.iter().any(|document| {
            (document.dirty || document.saving)
                && (document.path == drag.path || document.path.starts_with(&prefix))
        });
        if busy {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't move",
                "Save the current file before moving it.",
                cx,
            );
            return;
        }
        let from = drag.path.clone();
        let to = moved_path(&from, folder);
        let folder = folder.to_string();
        let request = self.native_docs_request("move", json!({ "path": from, "newPath": to }));
        let generation = self.native_docs.generation;
        self.native_docs_begin_operation("move", &from);
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            this.native_docs_end_operation();
            if this.native_docs.generation != generation {
                return;
            }
            if response.get("error").is_some() {
                let error = response["error"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                    .unwrap_or("Could not move item.")
                    .to_string();
                this.dispatch_gpui_workspace_action_toast("error", "Couldn't move", &error, cx);
                return;
            }
            this.native_docs_remap_paths(&from, &to, cx);
            if !folder.is_empty() {
                this.native_docs.expanded.insert(folder.clone());
            }
            this.native_docs_refresh(cx);
        });
    }
}
