//! The files list from the keyboard, like the former React Docs page: in the tree the arrows,
//! Home and End, Page Up and Page Down move
//! between rows, Enter or Space opens a file or folder, and the context-menu key or Shift+F10
//! opens the row menu; in Open Files, Enter or Space shows the file and Delete or Backspace
//! closes it.

use gpui::{Context, FocusHandle, KeyDownEvent, Window, point, px};

use super::state::DocsEntryKind;
use crate::GhostexGpuiApp;

/// The tree's row height, for Page Up and Page Down.
const ROW_HEIGHT: f32 = 34.0;

impl GhostexGpuiApp {
    pub(crate) fn native_docs_tree_focus_handle(&mut self, cx: &mut Context<Self>) -> FocusHandle {
        self.native_docs
            .tree_focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    pub(crate) fn native_docs_open_files_focus_handle(
        &mut self,
        cx: &mut Context<Self>,
    ) -> FocusHandle {
        self.native_docs
            .open_files_focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// Puts keyboard focus on the tree row for `path`.
    pub(crate) fn native_docs_focus_tree_row(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_docs.tree_focus = Some(path.to_string());
        let handle = self.native_docs_tree_focus_handle(cx);
        handle.focus(window, cx);
        self.native_docs_notify(cx);
    }

    /// Opens (a file) or toggles (a folder) the row at `index` of the drawn rows.
    fn native_docs_activate_tree_row(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self
            .native_docs_tree_rows()
            .into_iter()
            .find(|row| row.path == path)
        else {
            return;
        };
        if row.kind == DocsEntryKind::Directory {
            if row.folder.is_some() || !self.native_docs.search_query.trim().is_empty() {
                self.native_docs_clear_search(window, cx);
                self.native_docs.expanded.insert(row.path.clone());
                self.native_docs_list_folder(&row.path, cx);
                self.native_docs_reveal_in_tree(&row.path, cx);
                self.native_docs.reveal_request = Some(row.path.clone());
            } else {
                self.native_docs_toggle_folder(&row.path, cx);
            }
        } else {
            self.native_docs_open(&row.path, &row.display_path, cx);
        }
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_tree_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        let menu_key = key == "menu" || key == "contextmenu" || (key == "f10" && modifiers.shift);
        if !menu_key && (modifiers.alt || modifiers.platform || modifiers.control) {
            return;
        }
        let rows = self.native_docs_tree_rows();
        if rows.is_empty() {
            return;
        }
        let current = self
            .native_docs
            .tree_focus
            .as_ref()
            .and_then(|path| rows.iter().position(|row| &row.path == path));
        let page = (f32::from(self.native_docs.tree_scroll.bounds().size.height) / ROW_HEIGHT)
            .floor()
            .max(1.0) as usize;
        let last = rows.len() - 1;
        let target = match key {
            "down" => Some(current.map_or(0, |index| (index + 1).min(last))),
            "up" => Some(current.map_or(0, |index| index.saturating_sub(1))),
            "home" => Some(0),
            "end" => Some(last),
            "pagedown" => Some(current.map_or(0, |index| (index + page).min(last))),
            "pageup" => Some(current.map_or(0, |index| index.saturating_sub(page))),
            _ => None,
        };
        if let Some(target) = target {
            let path = rows[target].path.clone();
            self.native_docs.tree_focus = Some(path);
            let offset = self.native_docs.tree_status_rows;
            self.native_docs.tree_scroll.scroll_to_item(target + offset);
            self.native_docs_notify(cx);
            cx.stop_propagation();
            return;
        }
        let Some(index) = current else {
            return;
        };
        let path = rows[index].path.clone();
        let kind = rows[index].kind;
        match key {
            "enter" | "space" => {
                self.native_docs_activate_tree_row(&path, window, cx);
                cx.stop_propagation();
            }
            _ if menu_key => {
                // At the row's left + 28 and top + 22, like the Docs page.
                let offset = self.native_docs.tree_status_rows;
                let bounds = self.native_docs.tree_scroll.bounds_for_item(index + offset);
                let position = bounds.map_or_else(
                    || self.native_docs.tree_scroll.bounds().origin,
                    |bounds| {
                        point(
                            bounds.left() + px(28.0),
                            bounds.top() + bounds.size.height.min(px(22.0)),
                        )
                    },
                );
                self.show_native_docs_entry_menu(&path, kind, position, window, cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    pub(crate) fn native_docs_open_files_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.alt || modifiers.platform || modifiers.control {
            return;
        }
        let Some(path) = self.native_docs.active.clone() else {
            return;
        };
        match event.keystroke.key.as_str() {
            "enter" | "space" => {
                self.native_docs_select(&path, cx);
                // The tab row has nothing to type into: Enter hands the keys to the document.
                if self.docs_open_files_as_tabs && event.keystroke.key == "enter" {
                    self.focus_project_editor_surface(
                        crate::app::model::TitlebarMode::Manage,
                        window,
                        cx,
                    );
                }
                cx.stop_propagation();
            }
            "delete" | "backspace" => {
                self.native_docs_request_close(&path, window, cx);
                cx.stop_propagation();
            }
            "down" | "up" | "right" | "left" => {
                // Arrows move between the open files, the way a tab list steps; the tab row
                // steps sideways.
                let paths: Vec<String> = self
                    .native_docs
                    .documents
                    .iter()
                    .map(|document| document.path.to_string())
                    .collect();
                if let Some(index) = paths.iter().position(|candidate| *candidate == path) {
                    let next = if matches!(event.keystroke.key.as_str(), "down" | "right") {
                        (index + 1).min(paths.len() - 1)
                    } else {
                        index.saturating_sub(1)
                    };
                    self.native_docs_select(&paths[next], cx);
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }
}
